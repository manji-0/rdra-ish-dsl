//! The Rust backend of `check --code`: the probe under the service crate's `tests/`, `cargo
//! check --test <probe>` for `build` and `cargo test --test <probe>` for `run`, and rustc's
//! JSON messages read by their error codes.

use std::path::{Path, PathBuf};

use rdra_ish_emit::codegen::ir::{ContractDecl, PortMethod};
use rdra_ish_emit::codegen::probe::Probe;
use rdra_ish_emit::codegen::{probe_for, Lang, ProbeBinding};

use super::map::{CompilerMessage, Finding};
use super::{Backend, Target};

pub(crate) struct Rust;

/// The probe's test target: its file name without `.rs`.
fn test_name(probe: &Path) -> Option<String> {
    Some(probe.file_stem()?.to_str()?.to_string())
}

impl Backend for Rust {
    fn probe(&self, t: &Target<'_>) -> Option<(PathBuf, Probe)> {
        let path = t.target.probe.clone()?;
        let domain = t.generated.domain.as_ref()?;
        let traces = t.target.traces.as_ref().and_then(|traces| {
            let root = t.root();
            t.path(traces)
                .strip_prefix(&root)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
        });
        let binding = ProbeBinding {
            service: t.target.service.clone()?,
            module: t.target.module.clone()?,
            conformance: t.target.new.clone().zip(traces),
        };
        Some((path, probe_for(Lang::Rust, domain, &binding)?))
    }

    fn build_command(&self, t: &Target<'_>) -> Option<Vec<String>> {
        let name = test_name(t.target.probe.as_ref()?)?;
        Some(
            ["cargo", "check", "--test", &name, "--message-format=json"]
                .map(str::to_string)
                .to_vec(),
        )
    }

    fn run_command(&self, t: &Target<'_>) -> Option<Vec<String>> {
        let name = test_name(t.target.probe.as_ref()?)?;
        t.target.new.as_ref()?;
        Some(
            ["cargo", "test", "--test", &name, "--", "rdra_conformance"]
                .map(str::to_string)
                .to_vec(),
        )
    }

    fn parse_messages(&self, stdout: &str, _stderr: &str) -> Vec<CompilerMessage> {
        stdout.lines().filter_map(compiler_message).collect()
    }

    /// The probe's test panics with `rdra-ish conformance: <report>`; the report runs to the
    /// line before the test harness's `note:`.
    fn run_failure(&self, stdout: &str, stderr: &str) -> Option<String> {
        let all = format!("{stdout}\n{stderr}");
        let (_, rest) = all.split_once("rdra-ish conformance: ")?;
        let report: Vec<&str> = rest
            .lines()
            .take_while(|l| !l.starts_with("note:") && !l.trim().is_empty())
            .collect();
        Some(report.join("\n"))
    }

    fn classify(&self, m: &CompilerMessage) -> Finding {
        match m.code.as_deref() {
            Some("E0277") => Finding::NotImplemented,
            Some("E0046") => Finding::Incomplete,
            Some("E0053" | "E0050" | "E0185" | "E0186" | "E0195") => Finding::Signature,
            _ => Finding::Other,
        }
    }

    fn method_names(&self, m: &PortMethod) -> Vec<String> {
        let snake = rdra_ish_emit::codegen::snake(&m.name);
        vec![format!("prepare_{snake}"), format!("commit_{snake}")]
    }

    fn entry_names(&self, name: &str) -> Vec<String> {
        vec![format!("run_{}", rdra_ish_emit::codegen::snake(name))]
    }

    fn implement_hint(&self, contract: &ContractDecl, service: &str, methods: &[&str]) -> String {
        format!(
            "impl {} for {service} ({})",
            contract.name,
            methods.join(", ")
        )
    }
}

/// An error of cargo's `--message-format=json` line, at its primary span.
fn compiler_message(line: &str) -> Option<CompilerMessage> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("reason")?.as_str()? != "compiler-message" {
        return None;
    }
    let msg = v.get("message")?;
    if msg.get("level")?.as_str()? != "error" {
        return None;
    }
    let spans = msg.get("spans")?.as_array()?;
    let span = spans
        .iter()
        .find(|s| s.get("is_primary").and_then(|p| p.as_bool()) == Some(true))
        .or_else(|| spans.first())?;
    Some(CompilerMessage {
        file: span.get("file_name")?.as_str()?.to_string(),
        line: span.get("line_start")?.as_u64()? as usize,
        column: span.get("column_start")?.as_u64()? as usize,
        code: msg
            .get("code")
            .and_then(|c| c.get("code"))
            .and_then(|c| c.as_str())
            .map(str::to_string),
        text: msg.get("message")?.as_str()?.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_json_errors_are_read_at_their_primary_span() {
        let line = r#"{"reason":"compiler-message","message":{"level":"error","code":{"code":"E0277"},"message":"the trait bound `Svc: ShipUseCase` is not satisfied","spans":[{"file_name":"src/lib.rs","line_start":1,"column_start":1,"is_primary":false},{"file_name":"tests/rdra_contract.rs","line_start":9,"column_start":59,"is_primary":true}]}}"#;
        let m = compiler_message(line).unwrap();
        assert_eq!(
            (m.file.as_str(), m.line, m.column, m.code.as_deref()),
            ("tests/rdra_contract.rs", 9, 59, Some("E0277"))
        );
        assert_eq!(Rust.classify(&m), Finding::NotImplemented);
        let warning = line.replace("\"level\":\"error\"", "\"level\":\"warning\"");
        assert_eq!(compiler_message(&warning), None);
        assert_eq!(
            compiler_message(r#"{"reason":"build-finished","success":false}"#),
            None
        );
    }
}
