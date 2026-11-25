//! Compiler messages back to model locations (docs/design/code-check.md, "Diagnostics"). A
//! message on a probe line belongs to that line's use case (the probe's line map); a message
//! elsewhere belongs to the use case of the generated method it names (the contract index),
//! which is unique per action. What a message says (a contract not implemented, a method
//! missing, a signature) is the language's to tell ([`super::Backend::classify`]); the routing
//! and the diagnostics are shared.

use std::path::Path;

use rdra_ish_emit::codegen::ir::ContractDecl;
use rdra_ish_emit::codegen::probe::{ContractIndex, Probe};

use super::{Backend, CodeDiagnostic};

/// One compiler error, as a backend parses it from the compiler's output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompilerMessage {
    /// As the compiler prints it: relative to the directory it ran in, or absolute.
    pub file: String,
    /// 1-based.
    pub line: usize,
    /// 1-based.
    pub column: usize,
    /// `E0277`, `TS2739`; none for a compiler with no codes (Go).
    pub code: Option<String>,
    pub text: String,
}

impl CompilerMessage {
    /// `file:line:col  CODE: text`, cut to the text's first line: the `code:` line.
    fn shown(&self) -> String {
        let code = self
            .code
            .as_ref()
            .map(|c| format!("{c}: "))
            .unwrap_or_default();
        let text = self.text.lines().next().unwrap_or_default();
        format!("{}:{}:{}  {code}{text}", self.file, self.line, self.column)
    }
}

/// What a compiler message says about a use case's contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // a language backend constructs them; Rust's comes with P2
pub(crate) enum Finding {
    /// The service does not implement the contract (Rust E0277 on a probe line).
    NotImplemented,
    /// The service implements the contract without some of its methods (E0046).
    Incomplete,
    /// A method's argument or return types are not the contract's (E0053).
    Signature,
    /// Another error, on a probe line or naming a generated method.
    Other,
}

/// What the mapping reads: the probe and its line map, the contracts and their methods in the
/// language's spelling, and the service the probe names.
pub(crate) struct Mapping<'a> {
    /// The probe's path, relative to rdra.toml or to the build directory: compared by suffix.
    pub probe_file: &'a Path,
    pub probe: &'a Probe,
    pub index: &'a ContractIndex,
    pub contracts: &'a [ContractDecl],
    /// The service type `[code.<lang>] service` names.
    pub service: &'a str,
}

/// Whether `text` holds `word` as a whole word (a contract's name in a message about a bound).
fn names(text: &str, word: &str) -> bool {
    let w = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(word).any(|(at, _)| {
        !text[..at].chars().next_back().is_some_and(w)
            && !text[at + word.len()..].chars().next().is_some_and(w)
    })
}

/// Whether two paths name the same file, one maybe relative to a directory above the other.
fn same_file(a: &Path, b: &Path) -> bool {
    a.ends_with(b) || b.ends_with(a)
}

/// The diagnostics of `messages`: one per use case and finding, and one `CodeUnmapped`
/// counting the messages that belong to no use case.
pub(crate) fn map_messages(
    messages: &[CompilerMessage],
    m: &Mapping<'_>,
    backend: &dyn Backend,
) -> Vec<CodeDiagnostic> {
    let mut out: Vec<CodeDiagnostic> = Vec::new();
    let mut unmapped: Vec<&CompilerMessage> = Vec::new();
    let on_probe = |msg: &CompilerMessage| {
        same_file(Path::new(&msg.file), m.probe_file)
            .then(|| m.probe.at(msg.line))
            .flatten()
            .and_then(|l| m.contracts.iter().position(|c| c.name == l.contract))
    };
    // The messages on probe lines first: one names the use case where the probe states it,
    // and the same finding elsewhere (an impl that needs the contract) adds nothing.
    let mut ordered: Vec<&CompilerMessage> = messages.iter().collect();
    ordered.sort_by_key(|msg| on_probe(msg).is_none());
    for msg in ordered {
        let contract = on_probe(msg)
            .or_else(|| m.index.find(&msg.text).map(|(_, c, _)| c))
            .or_else(|| m.contracts.iter().position(|c| names(&msg.text, &c.name)));
        let Some(c) = contract else {
            unmapped.push(msg);
            continue;
        };
        let d = diagnostic(&m.contracts[c], c, msg, backend.classify(msg), m, backend);
        if !out.iter().any(|x| x.name == d.name) {
            out.push(d);
        }
    }
    if let Some(first) = unmapped.first() {
        out.push(CodeDiagnostic {
            name: "CodeUnmapped".to_string(),
            message: format!(
                "{} compiler error(s) belong to no use case: see the compiler's output above",
                unmapped.len()
            ),
            model: None,
            code: Some(first.shown()),
            fix: None,
            warning: false,
        });
    }
    out
}

fn diagnostic(
    contract: &ContractDecl,
    c: usize,
    msg: &CompilerMessage,
    finding: Finding,
    m: &Mapping<'_>,
    backend: &dyn Backend,
) -> CodeDiagnostic {
    let uc = &contract.usecase;
    let hint = || backend.implement_hint(contract, m.service, &m.index.methods_of(c));
    let (kind, message, fix) = match finding {
        Finding::NotImplemented => (
            "CodeMissingUseCase",
            format!("use case {uc} has no implementation in {}", m.service),
            Some(hint()),
        ),
        Finding::Incomplete => (
            "CodeIncomplete",
            format!("use case {uc} is implemented in part in {}", m.service),
            Some(hint()),
        ),
        Finding::Signature => (
            "CodeSignature",
            format!("use case {uc} takes other inputs than the model's"),
            Some(format!(
                "take and return the types {} declares in the generated code",
                contract.name
            )),
        ),
        Finding::Other => (
            "CodeError",
            format!("the implementation of use case {uc} does not compile"),
            None,
        ),
    };
    let model = match &contract.site {
        Some(site) => format!("{site}  usecase {uc} \"{}\"", contract.label),
        None => format!("usecase {uc} \"{}\"", contract.label),
    };
    CodeDiagnostic {
        name: format!("{kind}_{uc}"),
        message,
        model: Some(model),
        code: Some(msg.shown()),
        fix,
        warning: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_emit::codegen::ir::{PortDecl, PortMethod};
    use rdra_ish_emit::codegen::probe::ProbeWriter;

    /// A backend of a made-up language: codes `X1` (not implemented) and `X2` (signature).
    struct Toy;
    impl Backend for Toy {
        fn classify(&self, m: &CompilerMessage) -> Finding {
            match m.code.as_deref() {
                Some("X1") => Finding::NotImplemented,
                Some("X2") => Finding::Signature,
                _ => Finding::Other,
            }
        }
    }

    fn method(action: &str) -> PortMethod {
        PortMethod {
            action: action.to_string(),
            name: action.to_string(),
            usecase: action.to_string(),
            rows: Vec::new(),
            marks: Vec::new(),
            event: None,
            inputs: Vec::new(),
            choices: Vec::new(),
            write: String::new(),
        }
    }

    fn contract(usecase: &str, m: usize) -> ContractDecl {
        ContractDecl {
            name: format!("{usecase}UseCase"),
            usecase: usecase.to_string(),
            label: format!("{usecase} label"),
            site: Some(format!("m.rdra:{}:1", m + 1)),
            error: format!("{usecase}Error"),
            methods: vec![m],
            entries: Vec::new(),
        }
    }

    fn msg(file: &str, line: usize, code: &str, text: &str) -> CompilerMessage {
        CompilerMessage {
            file: file.to_string(),
            line,
            column: 7,
            code: Some(code.to_string()),
            text: text.to_string(),
        }
    }

    #[test]
    fn messages_go_back_to_use_cases_by_probe_line_and_by_method() {
        let port = PortDecl {
            methods: vec![method("Ship"), method("Cancel")],
            contracts: vec![contract("Ship", 0), contract("Cancel", 1)],
            ..PortDecl::default()
        };
        let mut w = ProbeWriter::new("// probe");
        w.usecase("is::<ShipUseCase>();", &port.contracts[0]);
        w.usecase("is::<CancelUseCase>();", &port.contracts[1]);
        let probe = w.finish();
        let index = ContractIndex::new(&port, |m| vec![format!("prep_{}", m.action)]);
        let m = Mapping {
            probe_file: Path::new("svc/tests/probe.x"),
            probe: &probe,
            index: &index,
            contracts: &port.contracts,
            service: "Svc",
        };
        let found = map_messages(
            &[
                // Line 3 is Ship's (line 2 is the fingerprint).
                msg("tests/probe.x", 3, "X1", "Svc lacks ShipUseCase\nmore"),
                msg("tests/probe.x", 3, "X1", "the same again"),
                msg(
                    "src/svc.x",
                    40,
                    "X2",
                    "method `prep_Cancel` has another type",
                ),
                msg("src/svc.x", 41, "X9", "something else"),
                msg("src/other.x", 2, "X9", "and more"),
            ],
            &m,
            &Toy,
        );
        let names: Vec<&str> = found.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "CodeMissingUseCase_Ship",
                "CodeSignature_Cancel",
                "CodeUnmapped"
            ]
        );
        assert_eq!(
            found[0].to_string(),
            "error[CodeMissingUseCase_Ship]: use case Ship has no implementation in Svc\n  \
             model: m.rdra:1:1  usecase Ship \"Ship label\"\n  \
             code:  tests/probe.x:3:7  X1: Svc lacks ShipUseCase\n  \
             fix:   implement ShipUseCase for Svc (prep_Ship)"
        );
        assert!(found[2].message.starts_with("2 compiler error(s)"));
    }
}
