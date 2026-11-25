//! `rdra-ish codegen`: the model's domain types as code, on stdout. Only the TypeScript types
//! are covered by docs/compatibility.md; the rest is experimental.
//!
//! The second line of the output is a fingerprint: a hash of the model's source text and one
//! of the output without that line. `--check <FILE>` generates again and compares, and the
//! fingerprint tells an edited file from a model that changed.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rdra_ish_core::format_location;
use rdra_ish_core::lifecycle::{Bounds, Scope, Severity};
use rdra_ish_emit::codegen::{generate_all, CheckStatus, Lang, Options};

use crate::cli::CodegenLang;
use crate::load::load_clean;
use crate::memory::generator_caps;
use crate::state_findings::state_findings;

pub fn run_codegen(
    inputs: &[PathBuf],
    lang: CodegenLang,
    out: Option<&Path>,
    package: &str,
    allow_failing: bool,
    conformance: Option<(&Path, usize)>,
    check_file: Option<&Path>,
) -> Result<()> {
    let (program, model, manifest) = load_clean(inputs)?;
    let model_hash = hash(
        program
            .sources
            .iter()
            .map(|(_, text, _)| text.replace("\r\n", "\n"))
            .collect::<Vec<_>>()
            .join("\u{0}")
            .as_bytes(),
    );
    // The state types rest on the rules `check` verifies, so the generator runs it first, at
    // the caps `check` runs at by default.
    let caps = generator_caps(manifest.as_ref());
    let found = state_findings(&program, &model, &Scope::ALL, Bounds::STATES, caps);
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
    } else if allow_failing {
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
        anyhow::bail!(
            "`check` reports {} error(s); refusing to generate code for a model it rejects (--allow-failing generates it anyway)",
            errors.len()
        );
    };
    let options = Options {
        lang: match lang {
            CodegenLang::Ts => Lang::TypeScript,
            CodegenLang::Rust => Lang::Rust,
            CodegenLang::Go => Lang::Go,
        },
        package: package.to_string(),
        model_path: inputs
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(" "),
        version: env!("CARGO_PKG_VERSION").to_string(),
        check,
        conformance: conformance.map(|(_, states)| states),
    };
    // Sites relative to the working directory, as the user named the inputs.
    let cwd = std::env::current_dir().ok();
    let site = |loc: &_| {
        let text = format_location(&program, loc)?;
        Some(
            match cwd
                .as_ref()
                .and_then(|d| text.strip_prefix(&format!("{}/", d.display())))
            {
                Some(relative) => relative.to_string(),
                None => text,
            },
        )
    };
    let generated = generate_all(&model, &options, &site)?;
    let code = stamp(&generated.code, &model_hash);
    if let Some(path) = check_file {
        return check_against(path, &code, &model_hash);
    }
    if let (Some((path, _)), Some(traces)) = (conformance, &generated.traces) {
        fs::write(path, &traces.text)
            .with_context(|| format!("failed to write {}", path.display()))?;
        eprintln!(
            "wrote {}: {} of {} moves in {} traces, {} refused calls, {} interleaved commits{}",
            path.display(),
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
    }
    match out {
        Some(path) => {
            fs::write(path, format!("{code}\n"))
                .with_context(|| format!("failed to write {}", path.display()))?;
            eprintln!("wrote {}", path.display());
        }
        None => println!("{code}"),
    }
    Ok(())
}

/// The fingerprint line's prefix, after the comment marker every language shares.
const FINGERPRINT: &str = "// rdra-ish codegen fingerprint:";

/// FNV-1a, 64 bits, as 16 hex digits: it tells a changed text, not a forged one.
fn hash(bytes: &[u8]) -> String {
    let h = bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{h:016x}")
}

/// `text` without its fingerprint line, with `\n` line ends and no trailing newline.
fn body(text: &str) -> String {
    text.replace("\r\n", "\n")
        .lines()
        .filter(|l| !l.starts_with(FINGERPRINT))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

/// `code` with the fingerprint as its second line, in the header comment.
fn stamp(code: &str, model_hash: &str) -> String {
    let line = format!(
        "{FINGERPRINT} model={model_hash} code={}",
        hash(body(code).as_bytes())
    );
    match code.split_once('\n') {
        Some((first, rest)) => format!("{first}\n{line}\n{rest}"),
        None => format!("{code}\n{line}"),
    }
}

/// The `(model, code)` hashes a fingerprint line records.
fn fingerprint(text: &str) -> Option<(String, String)> {
    let line = text.lines().find(|l| l.starts_with(FINGERPRINT))?;
    let mut model = None;
    let mut code = None;
    for word in line[FINGERPRINT.len()..].split_whitespace() {
        if let Some(v) = word.strip_prefix("model=") {
            model = Some(v.to_string());
        } else if let Some(v) = word.strip_prefix("code=") {
            code = Some(v.to_string());
        }
    }
    Some((model?, code?))
}

/// Exits 1 with a summary when `path` is not `code` (the fingerprint line aside).
fn check_against(path: &Path, code: &str, model_hash: &str) -> Result<()> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let (file, generated) = (body(&text), body(code));
    let recorded = fingerprint(&text);
    if file == generated {
        if recorded.as_ref().is_some_and(|(m, _)| m != model_hash) {
            eprintln!(
                "note: the model's text changed since {} was generated; the code it generates did not",
                path.display()
            );
        }
        eprintln!("ok: {} is what the model generates", path.display());
        return Ok(());
    }
    let name = path.display();
    eprintln!("error: {name} is not what `codegen` generates from the model");
    match &recorded {
        None => eprintln!(
            "  it has no fingerprint line (`{FINGERPRINT} ...`): it was not generated by this command, or the line was removed"
        ),
        Some((model, code)) => {
            let edited = *code != hash(file.as_bytes());
            let model_changed = model != model_hash;
            if edited {
                eprintln!("  file edited: it no longer matches its fingerprint, so it was changed after it was generated");
            }
            if model_changed {
                eprintln!("  model changed: its source text is not the text {name} was generated from");
            }
            if !edited && !model_changed {
                eprintln!("  generator changed: neither the file nor the model did, so the output of `codegen` did (another rdra-ish version, or other --lang, --package, --allow-failing, or input paths)");
            }
        }
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
    std::process::exit(1);
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
