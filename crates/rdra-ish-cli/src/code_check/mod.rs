//! `check --code[=fresh|build|run]` (docs/design/code-check.md): does the service code
//! `[code.<lang>]` in rdra.toml names have what the model requires?
//!
//! rdra-ish parses none of the user's code. `fresh` generates every configured file again
//! and compares (by hash for the traces); `build` runs the language's compiler on the probe
//! and maps its errors back to model locations; `run` runs the conformance test the probe
//! holds. A language plugs in through [`Backend`]; the rest is shared.

mod go;
mod ledger;
pub(crate) mod map;
mod rust;
mod ts;

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use rdra_ish_core::project::{CodeTarget, Manifest};
use rdra_ish_emit::codegen::adapter::{Ran, REPORT};
use rdra_ish_emit::codegen::conformance::{mutants, Mutant, MutantKind, TRACES_ENV};
use rdra_ish_emit::codegen::ir::{ContractDecl, PortMethod};
use rdra_ish_emit::codegen::probe::{ContractIndex, Probe};

use crate::cli::CodeLevel;
use crate::codegen_cmd::{configured_request, generate, Generated, Loaded, Request};
use crate::generated_file::{check_code, check_traces, stamp, table_settings};
use crate::paths::shown;
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
                // A value of several lines keeps them under its first.
                let v = v.replace('\n', "\n         ");
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
        let domain = self.path(self.target.code_file());
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

    /// The build's output as a person reads it, printed before the findings that map it: by
    /// default all of stdout and stderr (tsc writes its errors to stdout, `go` to stderr).
    fn compiler_output(&self, stdout: &str, stderr: &str) -> String {
        format!("{stdout}{stderr}")
    }

    /// The `code:` line of a failed run: by default the output's last non-empty line; a
    /// language gives the harness's report of the first difference instead.
    fn run_failure(&self, stdout: &str, stderr: &str) -> Option<String> {
        last_line(&format!("{stdout}\n{stderr}"))
    }

    /// The members a message says are missing from a contract, and how many more it only
    /// counts ("and 3 more"): a message missing them all is the contract not implemented.
    fn missing_members(&self, _message: &CompilerMessage) -> Option<(Vec<String>, usize)> {
        None
    }

    /// One finding for a build that fails as a whole, for a cause no use case has (a project
    /// setting): it stands in for the per-use-case findings its messages would give.
    fn build_cause(&self, _messages: &[CompilerMessage]) -> Option<CodeDiagnostic> {
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

/// The probe of `t` in `lang`: the model adapter's when the table names `adapter`, else the
/// contract probe of the domain code.
fn probe_of(
    t: &Target<'_>,
    lang: rdra_ish_emit::codegen::Lang,
    domain: &rdra_ish_emit::codegen::ir::DomainModel,
    binding: &rdra_ish_emit::codegen::ProbeBinding,
) -> Option<Probe> {
    if t.target.adapter.is_some() {
        rdra_ish_emit::codegen::adapter::probe(lang, domain, binding)
    } else {
        rdra_ish_emit::codegen::probe_for(lang, domain, binding)
    }
}

/// `build`: the build command (rdra.toml's, else the backend's) in the root, its errors
/// mapped to use cases; `None` when there is no command.
fn build(backend: &dyn Backend, t: &Target<'_>) -> Option<Result<Stepped>> {
    let command = t
        .target
        .build
        .clone()
        .or_else(|| backend.build_command(t))?;
    Some(compile(backend, t, &command).map(|found| (found, Vec::new())))
}

/// What a step found, and the lines it adds to the coverage statement.
type Stepped = (Vec<CodeDiagnostic>, Vec<String>);

/// The line of the coverage statement that says what a passing model adapter ran (#439), from
/// the line its conformance test prints; empty for the generated port, whose test prints none.
fn ran_line(t: &Target<'_>, stdout: &str, stderr: &str) -> Vec<String> {
    if t.target.adapter.is_none() {
        return Vec::new();
    }
    let Some(ran) = Ran::read(&format!("{stdout}\n{stderr}")) else {
        return vec![format!(
            "  ran: not reported: the run command printed no `{}` line",
            REPORT.trim_end()
        )];
    };
    let mut line = format!(
        "  ran: {} traces, {} calls, {} refused calls, {} interleaved commits",
        ran.traces, ran.calls, ran.refusals, ran.interleavings
    );
    if ran.split > 0 {
        line.push_str(&format!(
            "; {} calls made again by prepare and commit",
            ran.split
        ));
    }
    if ran.skipped > 0 {
        line.push_str(&format!(
            "; {} interleavings run as plain calls: the adapter has no prepare and commit, so conditional writes are not checked",
            ran.skipped
        ));
    }
    vec![line]
}

/// `run`: the run command (rdra.toml's, else the backend's) in the root; a failure is one
/// diagnostic with the harness's report ([`Backend::run_failure`]), else the output's last
/// line. `None` when there is no command.
fn run_step(backend: &dyn Backend, t: &Target<'_>) -> Option<Result<Stepped>> {
    let command = t.target.run.clone().or_else(|| backend.run_command(t))?;
    Some((|| {
        let (ok, stdout, stderr) = execute(t, &command, None)?;
        if ok {
            return Ok((Vec::new(), ran_line(t, &stdout, &stderr)));
        }
        // The whole output first: the diagnostic quotes one line of it.
        eprint!("{stdout}");
        eprint!("{stderr}");
        Ok((
            vec![run_diagnostic(backend, &command, &stdout, &stderr)],
            Vec::new(),
        ))
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

/// Runs `command` in the target's root: whether it succeeded, its stdout and stderr. With
/// `traces`, the probe's conformance test reads them in place of its traces file
/// ([`TRACES_ENV`]); without, the variable is cleared, so a value in the environment never
/// stands in for the real traces.
fn execute(
    t: &Target<'_>,
    command: &[String],
    traces: Option<&Path>,
) -> Result<(bool, String, String)> {
    let (program, args) = command
        .split_first()
        .context("an empty command in [code]")?;
    let root = t.root();
    let mut cmd = Command::new(program);
    match traces {
        Some(path) => cmd.env(TRACES_ENV, path),
        None => cmd.env_remove(TRACES_ENV),
    };
    let output = cmd
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
    let (ok, stdout, stderr) = execute(t, command, None)?;
    let messages = backend.parse_messages(&stdout, &stderr);
    if ok && messages.is_empty() {
        return Ok(Vec::new());
    }
    // The whole output first: the findings quote it.
    eprint!("{}", backend.compiler_output(&stdout, &stderr));
    if !ok && messages.is_empty() {
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
    let functions = match (t.generated.domain.as_ref(), port, lang_of(t.name)) {
        (Some(domain), Some(p), Some(lang)) => ContractIndex::of_functions(domain, p, |name| {
            rdra_ish_emit::codegen::function_name(lang, name)
        }),
        _ => ContractIndex::default(),
    };
    // The compiler's paths are relative to the root it ran in; the check shows paths relative
    // to the working directory.
    let root = t.root();
    let shown_file = |file: &str| shown(&root.join(file));
    let source_line = |msg: &CompilerMessage| {
        let text = std::fs::read_to_string(root.join(&msg.file)).ok()?;
        text.lines()
            .nth(msg.line.checked_sub(1)?)
            .map(str::to_string)
    };
    let table = format!("[code.{}]", t.name);
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
        shown: &shown_file,
        functions: &functions,
        table: &table,
        source_line: &source_line,
    };
    Ok(map_messages(&messages, &mapping, backend))
}

/// The language of the `[code.<name>]` table.
fn lang_of(name: &str) -> Option<rdra_ish_emit::codegen::Lang> {
    use rdra_ish_emit::codegen::Lang;
    match name {
        "rust" => Some(Lang::Rust),
        "go" => Some(Lang::Go),
        "ts" => Some(Lang::TypeScript),
        _ => None,
    }
}

/// The last non-empty line of `out`, trimmed.
fn last_line(out: &str) -> Option<String> {
    out.lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
}

/// The harness's report in the output of a failed run: the text after
/// `rdra-ish conformance: ` (the last one, when a run prints several), with the `model only:`
/// and `store only:` lines under it, one per row that differs.
pub(crate) fn harness_report(out: &str) -> Option<String> {
    let (_, rest) = out.rsplit_once("rdra-ish conformance: ")?;
    let mut lines = rest.lines();
    let mut report = lines.next()?.trim_end().to_string();
    for line in lines {
        let line = line.trim();
        if line.starts_with("model only: ") || line.starts_with("store only: ") {
            report.push_str("\n  ");
            report.push_str(line);
        } else {
            break;
        }
    }
    Some(report)
}

/// A panic in the output of a failed run, when the harness reported none: Rust's
/// `panicked at <file>:<line>:<col>:` with the message under it, Go's `panic: <message>` with
/// the function that panicked and its line, or a JavaScript error Node threw. `None` when
/// nothing panicked.
pub(crate) fn panic_report(out: &str) -> Option<String> {
    let lines: Vec<&str> = out.lines().collect();
    // Rust: "thread '..' panicked at src/x.rs:3:5:" then the message.
    if let Some(i) = lines.iter().position(|l| l.contains(" panicked at ")) {
        let (_, at) = lines[i].split_once(" panicked at ")?;
        let message = lines.get(i + 1).map_or("", |l| l.trim());
        return Some(format!(
            "the service panicked at {}: {message}",
            at.trim_end_matches(':')
        ));
    }
    // Go: "panic: <message>", and under "panic(" the frame that panicked: its function, then
    // its file and line.
    if let Some(i) = lines.iter().position(|l| l.starts_with("panic: ")) {
        let message = lines[i].trim_start_matches("panic: ").trim();
        let frame = lines
            .iter()
            .skip(i)
            .position(|l| l.starts_with("panic("))
            .and_then(|p| {
                let func = lines.get(i + p + 2)?.trim();
                // Its arguments are addresses.
                let func = func.rsplit_once('(').map_or(func, |(f, _)| f);
                let file = lines.get(i + p + 3)?.trim();
                let file = file.split(" +0x").next().unwrap_or(file);
                Some(format!(" in {func} at {file}"))
            })
            .unwrap_or_default();
        return Some(format!("the service panicked{frame}: {message}"));
    }
    // Node: an uncaught error is a line "<Name>Error: <message>" (or "Error [CODE]: ..")
    // followed by its stack, whose first frame is the place it was thrown.
    let i = lines.iter().position(|l| {
        let head = l.split([':', ' ']).next().unwrap_or("");
        head.ends_with("Error")
            && head.chars().all(|c| c.is_ascii_alphanumeric())
            && l.contains(": ")
    })?;
    let at = lines
        .get(i + 1)
        .map(|l| l.trim())
        .filter(|l| l.starts_with("at "))
        .map(|l| format!(" {l}"))
        .unwrap_or_default();
    Some(format!("the service threw{at}: {}", lines[i].trim()))
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
type Step = fn(&dyn Backend, &Target<'_>) -> Option<Result<Stepped>>;

/// A language with no build or run check yet: `fresh` only.
struct FreshOnly;
impl Backend for FreshOnly {}

fn backend(name: &str) -> Box<dyn Backend> {
    match name {
        "rust" => Box::new(rust::Rust),
        "ts" => Box::new(ts::TypeScript),
        "go" => Box::new(go::GoBackend::default()),
        _ => Box::new(FreshOnly),
    }
}

/// The coverage statement of the traces `t` ran, and a finding when they are not complete: a
/// warning, or an error with `deny_incomplete`.
fn coverage(t: &Target<'_>, deny_incomplete: bool) -> (Vec<String>, Option<CodeDiagnostic>) {
    let Some((_, traces)) = &t.generated.traces else {
        return (Vec::new(), None);
    };
    let subject = format!("[code.{}]", t.name);
    let unchecked = t
        .generated
        .domain
        .as_ref()
        .and_then(|d| d.port.as_ref())
        .map(|p| p.unchecked.clone())
        .unwrap_or_default();
    let finding = traces.incomplete(&subject).map(|message| CodeDiagnostic {
        name: "CodeCoverageIncomplete".to_string(),
        message,
        model: None,
        code: None,
        fix: Some(format!(
            "{}{}",
            if traces.complete {
                "the statement's `untried refusals` and `never run` lines name what no trace checks: check it by other means".to_string()
            } else {
                format!(
                    "raise `conformance_states` in [code.{}] of rdra.toml and run `codegen --code` again",
                    t.name
                )
            },
            if deny_incomplete {
                ""
            } else {
                "; --deny-incomplete fails the check on it"
            }
        )),
        warning: !deny_incomplete,
    });
    (traces.coverage(&subject, &unchecked), finding)
}

/// How many mutant traces of each kind the self-check of `--code=run` runs, and the seed
/// that picks them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SelfCheck {
    pub per_kind: usize,
    pub seed: u64,
}

/// The self-check of a run that passed (#439): the run command again on mutant traces, each
/// wrong at its last row, which a run of the harness on them fails at that row. The line of
/// the coverage statement, and a `CodeMutantSurvived` per mutant that did not fail there.
fn self_check(
    backend: &dyn Backend,
    t: &Target<'_>,
    how: SelfCheck,
) -> Result<(Vec<String>, Vec<CodeDiagnostic>)> {
    let (Some((text, traces)), Some(command)) = (
        &t.generated.traces,
        t.target.run.clone().or_else(|| backend.run_command(t)),
    ) else {
        return Ok((Vec::new(), Vec::new()));
    };
    let set = mutants(
        text,
        &traces.one_row,
        &traces.absent_id,
        &traces.guards,
        how.per_kind,
        how.seed,
    );
    let dir = std::env::temp_dir().join(format!(
        "rdra-ish-mutants-{}-{}",
        std::process::id(),
        t.name
    ));
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let mut survivors = Vec::new();
    let result = (|| -> Result<()> {
        for (i, m) in set.mutants.iter().enumerate() {
            let path = dir.join(format!("{i}-{}.txt", m.kind.name()));
            std::fs::write(&path, &m.text)
                .with_context(|| format!("failed to write {}", path.display()))?;
            let (ok, stdout, stderr) = execute(t, &command, Some(&path))?;
            let at = format!("traces line {}:", m.line);
            if !ok && format!("{stdout}\n{stderr}").contains(&at) {
                continue;
            }
            let what = if ok {
                "the run passed".to_string()
            } else {
                format!(
                    "the run failed elsewhere: {}",
                    backend.run_failure(&stdout, &stderr).unwrap_or_default()
                )
            };
            survivors.push((m, what));
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result?;
    // A run that fails no mutant of the harness's own kinds runs no harness on them, so an
    // absent row passing says nothing of the service then.
    let pipeline = set
        .mutants
        .iter()
        .filter(|m| m.kind != MutantKind::AbsentRow)
        .count();
    let harness_runs = survivors
        .iter()
        .filter(|(m, _)| m.kind != MutantKind::AbsentRow)
        .count()
        < pipeline;
    let total = set.mutants.len();
    let survivors: Vec<CodeDiagnostic> = survivors
        .into_iter()
        .map(|(m, what)| survived(m, &command, what, harness_runs))
        .collect();
    let mut line = format!(
        "  self-check: {} of {total} mutant traces failed at their changed row (seed {})",
        total - survivors.len(),
        how.seed
    );
    let guards: Vec<&str> = set
        .mutants
        .iter()
        .filter_map(|m| m.guard.as_deref())
        .collect();
    if !guards.is_empty() {
        line.push_str(&format!("; guard-removed mutants: {}", guards.join(", ")));
    }
    for (kind, why) in &set.none {
        line.push_str(&format!("; no {}: {why}", kind.name()));
    }
    Ok((vec![line], survivors))
}

/// The error of a mutant the run did not fail at its changed row.
fn survived(m: &Mutant, command: &[String], what: String, harness_runs: bool) -> CodeDiagnostic {
    let fix = match m.kind {
        MutantKind::AbsentRow if harness_runs => "the service must look the row up by the id it is given and refuse a call on a row that does not exist".to_string(),
        _ => format!(
            "`{}` must run the probe's conformance test on the traces {TRACES_ENV} names, and fail when it fails",
            command.join(" ")
        ),
    };
    CodeDiagnostic {
        name: "CodeMutantSurvived".to_string(),
        message: format!(
            "the run did not fail the {} mutant of traces line {} at its changed row: `{}` became `{}`, which the model never does",
            m.guard.as_ref().map_or_else(
                || m.kind.name().to_string(),
                |g| format!("{} ({g})", m.kind.name())
            ),
            m.was.0,
            m.was.1,
            m.now
        ),
        model: None,
        code: Some(what),
        fix: Some(fix),
        warning: false,
    }
}

/// Checks the code `manifest`'s `[code]` names at `level`; prints the findings on stderr and
/// a summary line on stdout, after the coverage statement of each conformance run. With
/// `deny_incomplete`, traces whose BFS stopped at its cap fail the run. Whether it passed.
pub(crate) fn run(
    inputs: &[PathBuf],
    manifest: &Manifest,
    level: CodeLevel,
    deny_incomplete: bool,
    mutation: SelfCheck,
) -> Result<bool> {
    let loaded = Loaded::load(inputs)?;
    let code = manifest.code.as_ref().expect("the caller checked [code]");
    let mut failed = 0usize;
    let mut findings: Vec<CodeDiagnostic> = Vec::new();
    let mut statements: Vec<String> = Vec::new();
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
        let extra = backend.extra_files(&t);
        generated_files.push(t.path(target.code_file()));
        generated_files.extend(target.traces.as_ref().map(|p| t.path(p)));
        generated_files.extend(extra.iter().map(|(p, _)| t.path(p)));
        let stale = fresh(&t, &extra)?;
        if stale > 0 {
            failed += stale;
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
                    let (mut found, ran) = result?;
                    if at == CodeLevel::Run {
                        let passed = found.iter().all(|d| d.warning);
                        let (lines, finding) = coverage(&t, deny_incomplete);
                        statements.extend(lines);
                        statements.extend(ran);
                        found.extend(finding);
                        if passed && mutation.per_kind > 0 {
                            let (lines, survivors) = self_check(backend.as_ref(), &t, mutation)?;
                            statements.extend(lines);
                            found.extend(survivors);
                        }
                    }
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
    for line in &statements {
        println!("{line}");
    }
    if failed > 0 {
        println!("FAIL: code: {failed} error(s) at --code={level_name}");
        return Ok(false);
    }
    println!("OK: code: what the model requires, at --code={level_name}");
    Ok(true)
}

/// `fresh`: how many of the domain code, the traces and the `extra` files of `t` are not
/// what the model generates (one error each); prints each difference on stderr.
fn fresh(t: &Target<'_>, extra: &[(PathBuf, String)]) -> Result<usize> {
    let hash = &t.generated.model_hash;
    let settings = table_settings(t.name);
    let mut fresh = vec![compare(&t.path(t.target.code_file()), |p| {
        check_code(p, &t.generated.code, hash, &settings)
    })?];
    if let (Some(path), Some((text, _))) = (&t.target.traces, &t.generated.traces) {
        fresh.push(compare(&t.path(path), |p| {
            check_traces(p, text, hash, &settings)
        })?);
    }
    for (path, text) in extra {
        fresh.push(compare(&t.path(path), |p| {
            check_code(p, text, hash, &settings)
        })?);
    }
    Ok(fresh.iter().filter(|f| !**f).count())
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

    #[test]
    fn a_report_keeps_the_rows_that_differ() {
        // go/logs/fault_dropped_row.txt of a dogfooding run.
        let out = "--- FAIL: TestRdraConformance (1.30s)\n    rdra_contract_test.go:473: rdra-ish conformance: traces line 28086: after `SendIntakeForms 1 1 1 1 1 2026-01-01T00:00:00Z 1 email 2026-01-01T00:00:00Z`, the store differs from the model:\n          model only: PatientMessage 1 status=sent sent_at=+ patientaccount_id=1\n          store only: PatientMessage 1 status=queued sent_at=- patientaccount_id=1\nFAIL\nFAIL\texample.com/clinic/clinic\t1.548s\nFAIL\n";
        let d = run_diagnostic(&go::GoBackend::default(), &["go".to_string()], out, "");
        assert_eq!(
            d.to_string(),
            "error[CodeConformance]: the service does not conform to the model's traces (`go`)\n  \
             code:  traces line 28086: after `SendIntakeForms 1 1 1 1 1 2026-01-01T00:00:00Z 1 email 2026-01-01T00:00:00Z`, the store differs from the model:\n           \
             model only: PatientMessage 1 status=sent sent_at=+ patientaccount_id=1\n           \
             store only: PatientMessage 1 status=queued sent_at=- patientaccount_id=1"
        );
    }

    #[test]
    fn a_panic_of_the_service_is_the_code_line() {
        // go/logs/run2.txt of a dogfooding run, the paths cut.
        let go_out = "--- FAIL: TestRdraConformance (1.23s)\npanic: runtime error: invalid memory address or nil pointer dereference [recovered, repanicked]\n[signal SIGSEGV: segmentation violation code=0x2 addr=0x10 pc=0x104c60e1c]\n\ngoroutine 8 [running]:\ntesting.tRunner.func1.2({0x10504f0c0, 0x1050ff560})\n\t/go/src/testing/testing.go:1974 +0x1a0\ntesting.tRunner.func1()\n\t/go/src/testing/testing.go:1977 +0x318\npanic({0x10504f0c0?, 0x1050ff560?})\n\t/go/src/runtime/panic.go:860 +0x12c\nexample.com/clinic/clinic.(*Service).commit(0x182280d1e0b8, {0x104c8ce96, 0xf}, {0x105086400?, 0x182280fc0ca8?})\n\t/svc/clinic/service.go:172 +0x43c\nexample.com/clinic/clinic.(*Service).CommitCheckInPatient1(0x182280d1e0b8, {0x182280fbb780, 0x0, 0x0})\n\t/svc/clinic/usecases_glue.go:1138 +0x54\nFAIL\texample.com/clinic/clinic\t1.513s\nFAIL\n";
        assert_eq!(
            go::GoBackend::default().run_failure(go_out, "").as_deref(),
            Some("the service panicked in example.com/clinic/clinic.(*Service).commit at /svc/clinic/service.go:172: runtime error: invalid memory address or nil pointer dereference [recovered, repanicked]")
        );
        // rustc 1.99's panic, as libtest prints it.
        let rust_out = "running 1 test\n\nthread 'rdra_conformance' (398749682) panicked at src/service.rs:1:47:\nindex out of bounds: the len is 0 but the index is 3\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\ntest rdra_conformance ... FAILED\n";
        assert_eq!(
            rust::Rust.run_failure(rust_out, "").as_deref(),
            Some("the service panicked at src/service.rs:1:47: index out of bounds: the len is 0 but the index is 3")
        );
        // Node 24's uncaught error.
        let node_out = "file:///svc/p.ts:1\nclass Svc { prepare() { return m.get(1) .a; } }\n                                         ^\n\nTypeError: Cannot read properties of undefined (reading 'a')\n    at Svc.prepare (file:///svc/p.ts:1:91)\n    at Object.<anonymous> (file:///svc/p.ts:2:11)\n\nNode.js v24.21.0\n";
        assert_eq!(
            ts::TypeScript.run_failure("", node_out).as_deref(),
            Some("the service threw at Svc.prepare (file:///svc/p.ts:1:91): TypeError: Cannot read properties of undefined (reading 'a')")
        );
    }
}
