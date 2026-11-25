//! `rdra-ish codegen`: the model's domain types as code, on stdout. Only the TypeScript types
//! are covered by docs/compatibility.md; the rest is experimental.
//!
//! The second line of the output is a fingerprint: a hash of the model's source text and one
//! of the output without that line. `--check <FILE>` generates again and compares, and the
//! fingerprint tells an edited file from a model that changed. A traces file carries one too,
//! after its format line, and is compared by hash only: it can be hundreds of megabytes.
//!
//! `--code` generates every file `[code.<lang>]` in `rdra.toml` names, with paths and sites
//! relative to the directory of `rdra.toml`, so `check --code` (crate::code_check) generates
//! the same text wherever it runs.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use rdra_ish_core::fingerprint::{self, Fingerprint};
use rdra_ish_core::lifecycle::{Bounds, Scope, Severity};
use rdra_ish_core::project::{CodeTarget, Manifest};
use rdra_ish_core::{format_location, ResolvedProgram, SemanticModel};
use rdra_ish_emit::codegen::conformance::{Traces, DEFAULT_STATES};
use rdra_ish_emit::codegen::ir::DomainModel;
use rdra_ish_emit::codegen::{generate_all, generate_json, CheckStatus, Lang, Options};

use crate::cli::CodegenLang;
use crate::load::load_clean;
use crate::memory::generator_caps;
use crate::state_findings::state_findings;

/// What to generate, once the flags or `[code.<lang>]` are read.
#[derive(Clone, Debug)]
pub(crate) struct Request {
    pub lang: Lang,
    pub json: bool,
    pub package: String,
    pub allow_failing: bool,
    /// The states the BFS behind the traces explores at most, when the harness is asked for.
    pub conformance: Option<usize>,
    /// The inputs as the header names them.
    pub model_path: String,
    /// Sites print relative to this directory; the working directory when `None`.
    pub base: Option<PathBuf>,
}

/// The generated text: the code (stamped with its fingerprint, or JSON) and the traces
/// (stamped too).
pub(crate) struct Generated {
    pub code: String,
    pub traces: Option<(String, Traces)>,
    pub model_hash: String,
    /// The domain model the code was printed from (its contracts); none for JSON.
    pub domain: Option<DomainModel>,
}

/// The model loaded for generating, with the hash of its source text.
pub(crate) struct Loaded {
    pub program: ResolvedProgram,
    pub model: SemanticModel,
    pub manifest: Option<Manifest>,
}

impl Loaded {
    pub fn load(inputs: &[PathBuf]) -> Result<Self> {
        let (program, model, manifest) = load_clean(inputs)?;
        Ok(Self {
            program,
            model,
            manifest,
        })
    }

    pub fn model_hash(&self) -> String {
        fingerprint::model_hash(self.program.sources.iter().map(|(_, t, _)| t.as_str()))
    }
}

pub(crate) struct CodegenArgs<'a> {
    pub inputs: &'a [PathBuf],
    pub lang: Option<CodegenLang>,
    pub json: bool,
    pub out: Option<&'a Path>,
    pub package: &'a str,
    pub allow_failing: bool,
    pub conformance: Option<(&'a Path, usize)>,
    pub check_file: Option<&'a Path>,
    pub code: bool,
}

pub(crate) fn lang_of(lang: CodegenLang) -> Lang {
    match lang {
        CodegenLang::Ts => Lang::TypeScript,
        CodegenLang::Rust => Lang::Rust,
        CodegenLang::Go => Lang::Go,
    }
}

pub fn run_codegen(args: CodegenArgs<'_>) -> Result<()> {
    let loaded = Loaded::load(args.inputs)?;
    if args.code {
        return write_configured(&loaded, args.inputs);
    }
    if args.json && (args.conformance.is_some() || args.check_file.is_some()) {
        bail!("--format json takes neither --conformance nor --check");
    }
    let lang = match (args.lang, args.json) {
        (Some(lang), _) => lang_of(lang),
        (None, true) => Lang::TypeScript,
        (None, false) => bail!("--lang is required, except with --code or --format json"),
    };
    let request = Request {
        lang,
        json: args.json,
        package: args.package.to_string(),
        allow_failing: args.allow_failing,
        conformance: args.conformance.map(|(_, states)| states),
        model_path: args
            .inputs
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(" "),
        base: None,
    };
    let generated = generate(&loaded, &request)?;
    if let Some(path) = args.check_file {
        let mut same = check_code(path, &generated.code, &generated.model_hash)?;
        if let (Some((traces, _)), Some((tpath, _))) = (&generated.traces, args.conformance) {
            same &= check_traces(tpath, traces, &generated.model_hash)?;
        }
        if !same {
            std::process::exit(1);
        }
        return Ok(());
    }
    if let (Some((path, _)), Some((text, stats))) = (args.conformance, &generated.traces) {
        write_traces(path, text, stats)?;
    }
    match args.out {
        Some(path) => write_file(path, &generated.code)?,
        None => println!("{}", generated.code),
    }
    Ok(())
}

/// The generated code, with `check` run first: it refuses a model `check` rejects unless
/// `allow_failing`.
pub(crate) fn generate(loaded: &Loaded, request: &Request) -> Result<Generated> {
    let Loaded {
        program,
        model,
        manifest,
    } = loaded;
    let model_hash = loaded.model_hash();
    // The state types rest on the rules `check` verifies, so the generator runs it first, at
    // the caps `check` runs at by default.
    let caps = generator_caps(manifest.as_ref());
    let found = state_findings(program, model, &Scope::ALL, Bounds::STATES, caps);
    let errors: Vec<String> = found
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| format!("{}: {}", f.name, one_line(&f.message)))
        .collect();
    let check = if errors.is_empty() {
        let line = found.bounds.to_string();
        CheckStatus::Passed {
            bounds: line.strip_prefix("bounds: ").unwrap_or(&line).to_string(),
            caps,
        }
    } else if request.allow_failing {
        eprintln!(
            "note: `check` reports {} error(s); the state types narrow no column, and the obligations list the errors",
            errors.len()
        );
        CheckStatus::Failed { errors }
    } else {
        for finding in found
            .findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
        {
            eprintln!("{finding}");
        }
        bail!(
            "`check` reports {} error(s); refusing to generate code for a model it rejects (--allow-failing generates it anyway)",
            errors.len()
        );
    };
    let options = Options {
        lang: request.lang,
        package: request.package.clone(),
        model_path: request.model_path.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        check,
        conformance: request.conformance,
    };
    let bases = bases(request.base.as_deref());
    let site = |loc: &_| Some(relative(format_location(program, loc)?, &bases));
    if request.json {
        return Ok(Generated {
            code: generate_json(model, &options, &site)?,
            traces: None,
            model_hash,
            domain: None,
        });
    }
    let generated = generate_all(model, &options, &site)?;
    Ok(Generated {
        code: stamp(&generated.code, &model_hash),
        traces: generated
            .traces
            .map(|t| (stamp_traces(&t.text, &model_hash), t)),
        model_hash,
        domain: Some(generated.domain),
    })
}

/// The prefixes a site drops: `dir` as given and as resolved, each with a trailing `/`.
fn bases(dir: Option<&Path>) -> Vec<String> {
    let dir = match dir {
        Some(d) => Some(d.to_path_buf()),
        None => std::env::current_dir().ok(),
    };
    let Some(dir) = dir else {
        return Vec::new();
    };
    let mut out = vec![format!("{}/", dir.display())];
    if let Ok(canon) = dunce::canonicalize(&dir) {
        out.push(format!("{}/", canon.display()));
    }
    out
}

fn relative(text: String, bases: &[String]) -> String {
    bases
        .iter()
        .find_map(|b| text.strip_prefix(b.as_str()))
        .map_or(text.clone(), str::to_string)
}

/// `path` relative to `base` when it is under it, for a header or a message.
pub(crate) fn label(path: &Path, base: &Path) -> String {
    let absolute = |p: &Path| dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (p, b) = (absolute(path), absolute(base));
    match p.strip_prefix(&b) {
        Ok(rest) if rest.as_os_str().is_empty() => ".".to_string(),
        Ok(rest) => rest.display().to_string(),
        Err(_) => path.display().to_string(),
    }
}

/// `path` as the user can find it from the working directory: relative, with `..` where it
/// is not under it.
pub(crate) fn shown(path: &Path) -> String {
    let Ok(cwd) = std::env::current_dir() else {
        return path.display().to_string();
    };
    let resolve = |p: &Path| -> PathBuf {
        if let Ok(c) = dunce::canonicalize(p) {
            return c;
        }
        match (p.parent(), p.file_name()) {
            (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
                dunce::canonicalize(parent).map_or(p.to_path_buf(), |c| c.join(name))
            }
            _ => p.to_path_buf(),
        }
    };
    let (to, from) = (resolve(&cwd.join(path)), resolve(&cwd));
    let (a, b): (Vec<_>, Vec<_>) = (to.components().collect(), from.components().collect());
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    if common == 0 {
        return to.display().to_string();
    }
    let mut out = PathBuf::new();
    for _ in common..b.len() {
        out.push("..");
    }
    for c in &a[common..] {
        out.push(c.as_os_str());
    }
    if out.as_os_str().is_empty() {
        ".".to_string()
    } else {
        out.display().to_string()
    }
}

/// The request `[code.<name>]` makes: paths and sites relative to the directory of
/// `rdra.toml`, and the inputs as seen from there, so the text does not depend on where the
/// command runs.
pub(crate) fn configured_request(
    manifest: &Manifest,
    name: &str,
    target: &CodeTarget,
    inputs: &[PathBuf],
) -> Request {
    let lang = match name {
        "rust" => Lang::Rust,
        "go" => Lang::Go,
        _ => Lang::TypeScript,
    };
    Request {
        lang,
        json: false,
        package: target
            .package
            .clone()
            .unwrap_or_else(|| "domain".to_string()),
        allow_failing: false,
        conformance: target
            .traces
            .as_ref()
            .map(|_| target.conformance_states.unwrap_or(DEFAULT_STATES)),
        model_path: inputs
            .iter()
            .map(|p| label(p, manifest.dir()))
            .collect::<Vec<_>>()
            .join(" "),
        base: Some(manifest.dir().to_path_buf()),
    }
}

/// `codegen --code`: every file the `[code]` tables name.
fn write_configured(loaded: &Loaded, inputs: &[PathBuf]) -> Result<()> {
    let manifest = code_manifest(loaded.manifest.as_ref())?;
    let code = manifest.code.as_ref().expect("checked");
    for (name, target) in code.targets() {
        let request = configured_request(manifest, name, target, inputs);
        let generated = generate(loaded, &request)?;
        let dir = manifest.dir();
        write_file(&dir.join(&target.domain), &generated.code)?;
        if let (Some(path), Some((text, stats))) = (&target.traces, &generated.traces) {
            write_traces(&dir.join(path), text, stats)?;
        }
        for (path, text) in
            crate::code_check::extra_files(name, target, manifest, request, generated)
        {
            write_file(&dir.join(path), &text)?;
        }
    }
    Ok(())
}

/// The manifest, when it has a `[code]` table with a language in it.
pub(crate) fn code_manifest(manifest: Option<&Manifest>) -> Result<&Manifest> {
    let Some(manifest) = manifest else {
        bail!("--code needs an rdra.toml with a [code.rust], [code.go] or [code.ts] table; this model has no rdra.toml");
    };
    match &manifest.code {
        Some(code) if !code.targets().is_empty() => Ok(manifest),
        _ => bail!(
            "--code needs a [code.rust], [code.go] or [code.ts] table in {}",
            shown(&manifest.path)
        ),
    }
}

fn write_file(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, format!("{text}\n"))
        .with_context(|| format!("failed to write {}", path.display()))?;
    eprintln!("wrote {}", shown(path));
    Ok(())
}

fn write_traces(path: &Path, text: &str, traces: &Traces) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))?;
    eprintln!(
        "wrote {}: {} of {} moves in {} traces, {} refused calls, {} interleaved commits{}",
        shown(path),
        traces.covered,
        traces.moves,
        traces.traces,
        traces.refusals,
        traces.interleavings,
        if traces.complete {
            ""
        } else {
            "; BFS stopped at --conformance-states"
        }
    );
    Ok(())
}

/// The fingerprint line's prefix in code, after the comment marker every language shares.
const CODE_FINGERPRINT: &str = "// rdra-ish codegen fingerprint:";

/// `text` without its fingerprint line, with `\n` line ends and no trailing newline.
fn body(text: &str) -> String {
    text.replace("\r\n", "\n")
        .lines()
        .filter(|l| !l.starts_with(CODE_FINGERPRINT))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

/// `code` with the fingerprint as its second line, in the header comment.
pub(crate) fn stamp(code: &str, model_hash: &str) -> String {
    let line = format!(
        "{CODE_FINGERPRINT} model={model_hash} code={}",
        fingerprint::hash(body(code).as_bytes())
    );
    insert_second(code, &line)
}

/// Traces with the fingerprint after the format line, as a `#` comment the harness skips.
/// Its `code=` hashes the lines, so a check can stream a file of any size.
fn stamp_traces(text: &str, model_hash: &str) -> String {
    let line = format!(
        "# {} model={model_hash} code={}",
        fingerprint::FINGERPRINT,
        fingerprint::line_hash(text)
    );
    insert_second(text, &line)
}

fn insert_second(text: &str, line: &str) -> String {
    match text.split_once('\n') {
        Some((first, rest)) => format!("{first}\n{line}\n{rest}"),
        None => format!("{text}\n{line}"),
    }
}

/// Why a generated file is not what the model generates now.
pub(crate) fn cause_lines(
    name: &str,
    recorded: Option<&Fingerprint>,
    file_hash: &str,
    model_hash: &str,
) -> Vec<String> {
    match recorded {
        None => vec![format!(
            "  it has no fingerprint line (`{CODE_FINGERPRINT} ...` or `# {} ...`): it was not generated by this command, or the line was removed",
            fingerprint::FINGERPRINT
        )],
        Some(f) => {
            let mut out = Vec::new();
            let edited = f.code != file_hash;
            let model_changed = f.model != model_hash;
            if edited {
                out.push("  file edited: it no longer matches its fingerprint, so it was changed after it was generated".to_string());
            }
            if model_changed {
                out.push(format!("  model changed: its source text is not the text {name} was generated from"));
            }
            if !edited && !model_changed {
                out.push("  generator changed: neither the file nor the model did, so the output of `codegen` did (another rdra-ish version, or other --lang, --package, --allow-failing, --conformance-states, or input paths)".to_string());
            }
            out
        }
    }
}

/// Whether the file at `path` is `code` (the fingerprint line aside); prints `ok:` or a summary
/// of the difference on stderr.
pub(crate) fn check_code(path: &Path, code: &str, model_hash: &str) -> Result<bool> {
    let name = shown(path);
    let text = fs::read_to_string(path).with_context(|| format!("failed to read {name}"))?;
    let (file, generated) = (body(&text), body(code));
    let recorded = fingerprint::find(&text);
    if file == generated {
        if recorded.as_ref().is_some_and(|f| f.model != model_hash) {
            eprintln!("note: the model's text changed since {name} was generated; the code it generates did not");
        }
        eprintln!("ok: {name} is what the model generates");
        return Ok(true);
    }
    eprintln!("error: {name} is not what `codegen` generates from the model");
    for line in cause_lines(
        &name,
        recorded.as_ref(),
        &fingerprint::hash(file.as_bytes()),
        model_hash,
    ) {
        eprintln!("{line}");
    }
    let (ours, theirs): (Vec<&str>, Vec<&str>) =
        (file.lines().collect(), generated.lines().collect());
    let only = |a: &[&str], b: &[&str]| {
        let mut left: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for l in b {
            *left.entry(l).or_default() += 1;
        }
        a.iter()
            .filter(|l| match left.get_mut(**l) {
                Some(n) if *n > 0 => {
                    *n -= 1;
                    false
                }
                _ => true,
            })
            .count()
    };
    eprintln!(
        "  {} line(s) only in {name}, {} only in the generated code",
        only(&ours, &theirs),
        only(&theirs, &ours)
    );
    if let Some(at) = (0..ours.len().max(theirs.len())).find(|i| ours.get(*i) != theirs.get(*i)) {
        eprintln!(
            "  first difference at line {} of the code without the fingerprint:",
            at + 1
        );
        eprintln!(
            "    {name}: {}",
            ours.get(at).copied().unwrap_or("(end of file)")
        );
        eprintln!(
            "    generated: {}",
            theirs.get(at).copied().unwrap_or("(end of file)")
        );
    }
    Ok(false)
}

/// Whether the traces file at `path` is `traces`, compared by the hash of its lines only: the
/// file is streamed, never held or diffed.
pub(crate) fn check_traces(path: &Path, traces: &str, model_hash: &str) -> Result<bool> {
    let name = shown(path);
    let (recorded, file_hash) =
        fingerprint::scan_file(path).with_context(|| format!("failed to read {name}"))?;
    if file_hash == fingerprint::line_hash(traces) {
        if recorded.as_ref().is_some_and(|f| f.model != model_hash) {
            eprintln!("note: the model's text changed since {name} was generated; the traces it generates did not");
        }
        eprintln!("ok: {name} holds the traces the model generates");
        return Ok(true);
    }
    eprintln!("error: {name} is not the traces `codegen` generates from the model");
    for line in cause_lines(&name, recorded.as_ref(), &file_hash, model_hash) {
        eprintln!("{line}");
    }
    Ok(false)
}

/// The first line of a finding's message: a counterexample follows it.
fn one_line(message: &str) -> String {
    message
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}
