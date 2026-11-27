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
use rdra_ish_core::fingerprint;
use rdra_ish_core::lifecycle::{Bounds, Scope, Severity};
use rdra_ish_core::project::{CodeTarget, Manifest};
use rdra_ish_core::{format_location, ResolvedProgram, SemanticModel};
use rdra_ish_emit::codegen::conformance::{Traces, DEFAULT_STATES};
use rdra_ish_emit::codegen::ir::DomainModel;
use rdra_ish_emit::codegen::{generate_all, generate_json, CheckStatus, Lang, Options};

use crate::cli::CodegenLang;
use crate::generated_file::{check_code, check_traces, stamp, stamp_traces};
use crate::load::load_clean;
use crate::memory::generator_caps;
use crate::paths::{label, shown};
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
    /// TypeScript: seal the state types (`--seal`, `[code.ts] seal`).
    pub seal: bool,
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
        fingerprint::program_hash(&self.program)
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
    pub seal: bool,
    pub check_file: Option<&'a Path>,
    pub code: bool,
    pub probe: bool,
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
    if args.code || args.probe {
        return write_configured(&loaded, args.inputs, args.probe);
    }
    if args.json && (args.conformance.is_some() || args.check_file.is_some()) {
        bail!("--format json takes neither --conformance nor --check");
    }
    let lang = match (args.lang, args.json) {
        (Some(lang), _) => lang_of(lang),
        (None, true) => Lang::TypeScript,
        (None, false) => bail!("--lang is required, except with --code or --format json"),
    };
    if args.seal && lang != Lang::TypeScript {
        bail!("--seal is for --lang ts only");
    }
    let request = Request {
        lang,
        json: args.json,
        package: args.package.to_string(),
        allow_failing: args.allow_failing,
        conformance: args.conformance.map(|(_, states)| states),
        seal: args.seal,
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
        write_traces(path, text, stats, unchecked(&generated))?;
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
        .map(|f| format!("{}: {}", f.name, first_line(&f.message)))
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
        seal: request.seal,
    };
    let bases = bases(request.base.as_deref());
    let site = |loc: &_| Some(without_base(format_location(program, loc)?, &bases));
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
    let sep = std::path::MAIN_SEPARATOR;
    let mut out = vec![format!("{}{sep}", dir.display())];
    if let Ok(canon) = dunce::canonicalize(&dir) {
        out.push(format!("{}{sep}", canon.display()));
    }
    out
}

fn without_base(text: String, bases: &[String]) -> String {
    let rest = bases
        .iter()
        .find_map(|b| text.strip_prefix(b.as_str()))
        .map_or(text.clone(), str::to_string);
    // A site is data in the generated code and the JSON contract, so it reads the same on
    // every OS: Windows' separator is written as `/`.
    if std::path::MAIN_SEPARATOR == '/' {
        rest
    } else {
        rest.replace(std::path::MAIN_SEPARATOR, "/")
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
        seal: lang == Lang::TypeScript && target.seal.unwrap_or(false),
        model_path: inputs
            .iter()
            .map(|p| label(p, manifest.dir()))
            .collect::<Vec<_>>()
            .join(" "),
        base: Some(manifest.dir().to_path_buf()),
    }
}

/// `codegen --code`: every file the `[code]` tables name.
/// Writes the files `[code]` names; only the probes with `probe_only`.
fn write_configured(loaded: &Loaded, inputs: &[PathBuf], probe_only: bool) -> Result<()> {
    let manifest = code_manifest(loaded.manifest.as_ref())?;
    let code = manifest.code.as_ref().expect("checked");
    for (name, target) in code.targets() {
        let request = configured_request(manifest, name, target, inputs);
        let generated = generate(loaded, &request)?;
        let dir = manifest.dir();
        if !probe_only {
            write_file(&dir.join(&target.domain), &generated.code)?;
            if let (Some(path), Some((text, stats))) = (&target.traces, &generated.traces) {
                write_traces(&dir.join(path), text, stats, unchecked(&generated))?;
            }
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
        Some(code) if !code.targets().is_empty() => {
            for (name, target) in code.targets() {
                if name != "ts" && (target.seal.is_some() || target.tsconfig.is_some()) {
                    bail!(
                        "[code.{name}] in {}: `seal` and `tsconfig` are for [code.ts] only",
                        shown(&manifest.path)
                    );
                }
            }
            Ok(manifest)
        }
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

/// What the harness of `generated` does not compare (`PortDecl::unchecked`).
fn unchecked(generated: &Generated) -> &[String] {
    generated
        .domain
        .as_ref()
        .and_then(|d| d.port.as_ref())
        .map_or(&[], |p| &p.unchecked[..])
}

/// Writes the traces, then their summary and coverage statement on stderr, with a warning
/// when their BFS stopped at its cap.
fn write_traces(path: &Path, text: &str, traces: &Traces, unchecked: &[String]) -> Result<()> {
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
    let subject = shown(path);
    for line in traces.coverage(&subject, unchecked) {
        eprintln!("{line}");
    }
    if let Some(warning) = traces.incomplete(&subject) {
        eprintln!("warning: {warning}");
    }
    Ok(())
}

/// The first line of a finding's message: a counterexample follows it.
fn first_line(message: &str) -> String {
    message
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}
