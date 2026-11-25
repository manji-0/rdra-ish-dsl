//! `check --code[=fresh|build|run]` (docs/design/code-check.md): does the service code
//! `[code.<lang>]` in rdra.toml names have what the model requires?
//!
//! rdra-ish parses none of the user's code. `fresh` generates every configured file again
//! and compares (by hash for the traces); `build` runs the language's compiler on the probe
//! and maps its errors back to model locations; `run` runs the conformance test the probe
//! holds. A language plugs in through [`Backend`]; the rest is shared.

mod ledger;
pub(crate) mod map;
mod rust;
mod ts;

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use rdra_ish_core::project::{CodeTarget, Manifest};
use rdra_ish_emit::codegen::ir::{ContractDecl, PortMethod};
use rdra_ish_emit::codegen::probe::{ContractIndex, Probe};

use crate::cli::CodeLevel;
use crate::codegen_cmd::{
    check_code, check_traces, configured_request, generate, shown, stamp, Generated, Loaded,
    Request,
};
use map::{map_messages, CompilerMessage, Finding, Mapping};

/// One finding of the code check, printed as `error[Name]: message` with the model location,
/// the code location, and the fix on the lines under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodeDiagnostic {
    /// `CodeMissingUseCase_ShipOrder`: a kind and the model name it is about.
    pub name: String,
    pub message: String,
    /// `path:line:col` and what is declared there.
    pub model: Option<String>,
    /// The compiler's or the scanner's location, and its message cut to one line.
    pub code: Option<String>,
    pub fix: Option<String>,
    /// A warning does not fail the check.
    pub warning: bool,
}

impl fmt::Display for CodeDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = if self.warning { "warning" } else { "error" };
        write!(f, "{level}[{}]: {}", self.name, self.message)?;
        for (key, value) in [
            ("model:", &self.model),
            ("code: ", &self.code),
            ("fix:  ", &self.fix),
        ] {
            if let Some(v) = value {
                write!(f, "\n  {key} {v}")?;
            }
        }
        Ok(())
    }
}

/// What a language's backend is given for one `[code.<lang>]` table. The fields the fresh-only
/// backends do not read yet are for the build backends.
#[allow(dead_code)]
pub(crate) struct Target<'a> {
    /// `rust`, `go`, or `ts`.
    pub name: &'static str,
    pub target: &'a CodeTarget,
    pub manifest: &'a Manifest,
    pub request: Request,
    pub generated: Generated,
}

impl Target<'_> {
    /// A path of the table, resolved against the directory of rdra.toml.
    pub fn path(&self, path: &Path) -> PathBuf {
        self.manifest.dir().join(path)
    }

    #[allow(dead_code)]
    /// The directory the build runs in and the ledger scans: `root`, else the nearest
    /// directory above `domain` holding a project file of any language.
    pub fn root(&self) -> PathBuf {
        if let Some(root) = &self.target.root {
            return self.path(root);
        }
        let domain = self.path(&self.target.domain);
        let start = domain.parent().map(Path::to_path_buf).unwrap_or_default();
        start
            .ancestors()
            .find(|d| {
                ["Cargo.toml", "go.mod", "package.json", "tsconfig.json"]
                    .iter()
                    .any(|f| d.join(f).is_file())
            })
            .map_or(start.clone(), Path::to_path_buf)
    }
}

/// A language's part of `check --code`: the probe it prints, the compiler it runs, and how
/// it reads the compiler's messages. The rest (freshness, running the command, mapping the
/// messages to use cases, the diagnostics) is shared. The default commands run only for
/// `--code=build` / `--code=run`, and so does a command `rdra.toml` gives in their place
/// (decision 7).
pub(crate) trait Backend {
    /// The probe: its path (relative to rdra.toml) and text with the line map; `None` when
    /// the language has none yet or the table names no `probe`.
    fn probe(&self, _target: &Target<'_>) -> Option<(PathBuf, Probe)> {
        None
    }

    /// The generated files beyond the domain code and the traces that `fresh` compares and
    /// `codegen --code` writes: `(path relative to rdra.toml, stamped text)`. The probe by
    /// default.
    fn extra_files(&self, target: &Target<'_>) -> Vec<(PathBuf, String)> {
        self.probe(target)
            .map(|(path, probe)| (path, stamp(&probe.text, &target.generated.model_hash)))
            .into_iter()
            .collect()
    }

    /// The command `build` runs in [`Target::root`] when rdra.toml gives none.
    fn build_command(&self, _target: &Target<'_>) -> Option<Vec<String>> {
        None
    }

    /// The command `run` runs in [`Target::root`] when rdra.toml gives none.
    fn run_command(&self, _target: &Target<'_>) -> Option<Vec<String>> {
        None
    }

    /// The compiler's errors in its output.
    fn parse_messages(&self, _stdout: &str, _stderr: &str) -> Vec<CompilerMessage> {
        Vec::new()
    }

    /// The `code:` line of a failed run: by default the output's last non-empty line; a
    /// language gives the harness's report of the first difference instead.
    fn run_failure(&self, stdout: &str, stderr: &str) -> Option<String> {
        format!("{stdout}\n{stderr}")
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
    }

    /// The members a message says are missing from a contract, and how many more it only
    /// counts ("and 3 more"): a message missing them all is the contract not implemented.
    fn missing_members(&self, _message: &CompilerMessage) -> Option<(Vec<String>, usize)> {
        None
    }

    /// What a message says about the contract it belongs to.
    fn classify(&self, _message: &CompilerMessage) -> Finding {
        Finding::Other
    }

    /// A port method's generated names in this language (Rust `prepare_pay`, `commit_pay`).
    fn method_names(&self, _method: &PortMethod) -> Vec<String> {
        Vec::new()
    }

    /// An entry point's generated names (#335), by its PascalCase name (Rust
    /// `run_create_charge`).
    fn entry_names(&self, _name: &str) -> Vec<String> {
        Vec::new()
    }

    /// The `fix:` line of a contract not implemented, or implemented in part.
    fn implement_hint(&self, contract: &ContractDecl, service: &str, methods: &[&str]) -> String {
        format!(
            "implement {} for {service} ({})",
            contract.name,
            methods.join(", ")
        )
    }
}

/// `build`: the build command (rdra.toml's, else the backend's) in the root, its errors
/// mapped to use cases; `None` when there is no command.
fn build(backend: &dyn Backend, t: &Target<'_>) -> Option<Result<Vec<CodeDiagnostic>>> {
    let command = t
        .target
        .build
        .clone()
        .or_else(|| backend.build_command(t))?;
    Some(compile(backend, t, &command))
}

/// `run`: the run command (rdra.toml's, else the backend's) in the root; a failure is one
/// diagnostic with the harness's report ([`Backend::run_failure`]), else the output's last
/// line. `None` when there is no command.
fn run_step(backend: &dyn Backend, t: &Target<'_>) -> Option<Result<Vec<CodeDiagnostic>>> {
    let command = t.target.run.clone().or_else(|| backend.run_command(t))?;
    Some((|| {
        let (ok, stdout, stderr) = execute(t, &command)?;
        if ok {
            return Ok(Vec::new());
        }
        // The whole output first: the diagnostic quotes one line of it.
        eprint!("{stdout}");
        eprint!("{stderr}");
        Ok(vec![run_diagnostic(backend, &command, &stdout, &stderr)])
    })())
}

/// The diagnostic of a failed run: the command, and the `code:` line its backend reads.
fn run_diagnostic(
    backend: &dyn Backend,
    command: &[String],
    stdout: &str,
    stderr: &str,
) -> CodeDiagnostic {
    CodeDiagnostic {
        name: "CodeConformance".to_string(),
        message: format!(
            "the service does not conform to the model's traces (`{}`)",
            command.join(" ")
        ),
        model: None,
        code: backend.run_failure(stdout, stderr),
        fix: None,
        warning: false,
    }
}

/// Runs `command` in the target's root: whether it succeeded, its stdout and stderr.
fn execute(t: &Target<'_>, command: &[String]) -> Result<(bool, String, String)> {
    let (program, args) = command
        .split_first()
        .context("an empty command in [code]")?;
    let root = t.root();
    let output = Command::new(program)
        .args(args)
        .current_dir(&root)
        .output()
        .with_context(|| format!("failed to run `{}` in {}", command.join(" "), shown(&root)))?;
    Ok((
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

/// Runs the build command and maps its errors; a failure with no error the backend reads is
/// one `CodeBuildFailed`.
fn compile(
    backend: &dyn Backend,
    t: &Target<'_>,
    command: &[String],
) -> Result<Vec<CodeDiagnostic>> {
    let (ok, stdout, stderr) = execute(t, command)?;
    let messages = backend.parse_messages(&stdout, &stderr);
    if ok && messages.is_empty() {
        return Ok(Vec::new());
    }
    if !ok && messages.is_empty() {
        eprint!("{stderr}");
        return Ok(vec![CodeDiagnostic {
            name: "CodeBuildFailed".to_string(),
            message: format!(
                "`{}` failed with no error rdra-ish reads",
                command.join(" ")
            ),
            model: None,
            code: None,
            fix: None,
            warning: false,
        }]);
    }
    let probe = backend.probe(t);
    let port = t.generated.domain.as_ref().and_then(|d| d.port.as_ref());
    let empty = Probe::default();
    let (probe_file, probe) = match &probe {
        Some((path, probe)) => (path.as_path(), probe),
        None => (Path::new(""), &empty),
    };
    let index = port.map_or_else(ContractIndex::default, |p| {
        ContractIndex::new(p, |m| backend.method_names(m))
            .with_entries(p, |e| backend.entry_names(&e.name))
    });
    // The sites are relative to rdra.toml, as the generated text has them; the diagnostics
    // show them as the other paths of the check are shown.
    let contracts: Vec<ContractDecl> = port
        .map(|p| p.contracts.clone())
        .unwrap_or_default()
        .into_iter()
        .map(|mut c| {
            c.site = c.site.map(|s| rebase_site(t, &s));
            c
        })
        .collect();
    let mapping = Mapping {
        probe_file,
        probe,
        index: &index,
        contracts: &contracts,
        service: t.target.service.as_deref().unwrap_or("the service"),
    };
    eprint!("{stderr}");
    Ok(map_messages(&messages, &mapping, backend))
}

/// Whether two paths name one file, compared as written and as resolved.
fn same_path(a: &Path, b: &Path) -> bool {
    a == b || matches!((dunce::canonicalize(a), dunce::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

/// A site `path:line:col` relative to rdra.toml, as the check shows paths.
fn rebase_site(t: &Target<'_>, site: &str) -> String {
    let mut parts = site.rsplitn(3, ':');
    let (Some(col), Some(line), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
        return site.to_string();
    };
    format!("{}:{line}:{col}", shown(&t.path(Path::new(path))))
}

/// The generated files beyond the code and the traces of one `[code.<lang>]` table, for
/// `codegen --code` to write.
pub(crate) fn extra_files(
    name: &'static str,
    target: &CodeTarget,
    manifest: &Manifest,
    request: Request,
    generated: Generated,
) -> Vec<(PathBuf, String)> {
    let t = Target {
        name,
        target,
        manifest,
        request,
        generated,
    };
    backend(name).extra_files(&t)
}

/// A level past `fresh`: [`build`] or [`run_step`].
type Step = fn(&dyn Backend, &Target<'_>) -> Option<Result<Vec<CodeDiagnostic>>>;

/// A language with no build or run check yet: `fresh` only.
struct FreshOnly;
impl Backend for FreshOnly {}

fn backend(name: &str) -> Box<dyn Backend> {
    match name {
        "rust" => Box::new(rust::Rust),
        "ts" => Box::new(ts::TypeScript),
        _ => Box::new(FreshOnly),
    }
}

/// Checks the code `manifest`'s `[code]` names at `level`; prints the findings on stderr and
/// a summary line on stdout. Whether it passed.
pub(crate) fn run(inputs: &[PathBuf], manifest: &Manifest, level: CodeLevel) -> Result<bool> {
    let loaded = Loaded::load(inputs)?;
    let code = manifest.code.as_ref().expect("the caller checked [code]");
    let mut failed = 0usize;
    let mut findings: Vec<CodeDiagnostic> = Vec::new();
    // The ledger's inputs: the model's obligations (sites as the check shows them), the
    // roots to scan for markers, and the generated files, which are not scanned.
    let mut obligations: Option<Vec<rdra_ish_emit::codegen::ir::Obligation>> = None;
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut generated_files: Vec<PathBuf> = Vec::new();
    for (name, target) in code.targets() {
        let request = configured_request(manifest, name, target, inputs);
        let generated = generate(&loaded, &request)?;
        let t = Target {
            name,
            target,
            manifest,
            request,
            generated,
        };
        let backend = backend(name);
        if obligations.is_none() {
            obligations = t.generated.domain.as_ref().map(|d| {
                d.obligations
                    .iter()
                    .map(|o| rdra_ish_emit::codegen::ir::Obligation {
                        site: o.site.as_ref().map(|s| rebase_site(&t, s)),
                        ..o.clone()
                    })
                    .collect()
            });
        }
        let root = t.root();
        if !roots.contains(&root) {
            roots.push(root);
        }
        generated_files.push(t.path(&target.domain));
        generated_files.extend(target.traces.as_ref().map(|p| t.path(p)));
        generated_files.extend(backend.extra_files(&t).into_iter().map(|(p, _)| t.path(&p)));
        let mut fresh = true;
        fresh &= compare(&t.path(&target.domain), |p| {
            check_code(p, &t.generated.code, &t.generated.model_hash)
        })?;
        if let (Some(path), Some((text, _))) = (&target.traces, &t.generated.traces) {
            fresh &= compare(&t.path(path), |p| {
                check_traces(p, text, &t.generated.model_hash)
            })?;
        }
        for (path, text) in backend.extra_files(&t) {
            fresh &= compare(&t.path(&path), |p| {
                check_code(p, &text, &t.generated.model_hash)
            })?;
        }
        if !fresh {
            failed += 1;
            eprintln!(
                "  generate them again: rdra-ish codegen {} --code",
                inputs
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            continue;
        }
        let steps: [(CodeLevel, &str, Step); 2] = [
            (CodeLevel::Build, "build", build),
            (CodeLevel::Run, "run", run_step),
        ];
        for (at, what, step) in steps {
            if level < at {
                break;
            }
            match step(backend.as_ref(), &t) {
                None => {
                    eprintln!("note: --code={what} is not supported for [code.{name}] yet; checked up to the level before it");
                    break;
                }
                Some(result) => {
                    let found = result?;
                    let errors = found.iter().filter(|d| !d.warning).count();
                    findings.extend(found);
                    if errors > 0 {
                        failed += errors;
                        break;
                    }
                }
            }
        }
    }
    let mut markers = Vec::new();
    for root in &roots {
        for file in ledger::files_under(root) {
            if generated_files.iter().any(|g| same_path(g, &file)) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            if !text.contains(ledger::MARKER) {
                continue;
            }
            for m in ledger::scan(&file.display().to_string(), &text) {
                markers.push((root.clone(), m));
            }
        }
    }
    let shown_path = |p: &Path| shown(p);
    let site = |s: &str| s.to_string();
    let obligations = obligations.unwrap_or_default();
    let book = ledger::Ledger {
        obligations: &obligations,
        markers: &markers,
        table: &code.obligations,
        manifest_dir: manifest.dir(),
        shown: &shown_path,
        site: &site,
    };
    if book.active() {
        let found = ledger::check(&book);
        failed += found.iter().filter(|d| !d.warning).count();
        findings.extend(found);
    } else if !obligations.is_empty() {
        eprintln!(
            "note: no obligation has an owner yet: mark one with `// rdra: obligation <name> [file]` or list it under [code.obligations] to keep the ledger"
        );
    }
    for d in &findings {
        eprintln!("{d}");
    }
    let level_name = match level {
        CodeLevel::Fresh => "fresh",
        CodeLevel::Build => "build",
        CodeLevel::Run => "run",
    };
    if failed > 0 {
        println!("FAIL: code: {failed} error(s) at --code={level_name}");
        return Ok(false);
    }
    println!("OK: code: what the model requires, at --code={level_name}");
    Ok(true)
}

/// Runs `check` on `path`, or reports that the file is missing.
fn compare(path: &Path, check: impl FnOnce(&Path) -> Result<bool>) -> Result<bool> {
    if !path.is_file() {
        eprintln!(
            "error: {} does not exist: `codegen --code` writes it",
            shown(path)
        );
        return Ok(false);
    }
    check(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A backend of a made-up language whose harness reports `DIFF: <why>`.
    struct Toy;
    impl Backend for Toy {
        fn run_failure(&self, stdout: &str, _stderr: &str) -> Option<String> {
            stdout
                .lines()
                .find_map(|l| l.strip_prefix("DIFF: "))
                .map(str::to_string)
        }
    }

    #[test]
    fn a_failed_run_quotes_the_line_its_backend_reads() {
        let command = ["toy".to_string(), "test".to_string()];
        let d = run_diagnostic(&Toy, &command, "start\nDIFF: Order 1 differs\nFAIL\n", "");
        assert_eq!(d.name, "CodeConformance");
        assert_eq!(d.code.as_deref(), Some("Order 1 differs"));
        assert!(d.message.contains("`toy test`"), "{}", d.message);
        // The default: the output's last non-empty line.
        let d = run_diagnostic(&FreshOnly, &command, "a\n", "b\nFAIL\n\n");
        assert_eq!(d.code.as_deref(), Some("FAIL"));
    }
}
