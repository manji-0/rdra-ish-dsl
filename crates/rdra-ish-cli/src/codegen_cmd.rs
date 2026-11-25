//! `rdra-ish codegen`: the model's domain types as code, on stdout. Only the TypeScript types
//! are covered by docs/compatibility.md; the rest is experimental.

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
) -> Result<()> {
    let (program, model, manifest) = load_clean(inputs)?;
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
    let code = generated.code;
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

/// The first line of a finding's message: a counterexample follows it.
fn one_line(message: &str) -> String {
    message
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}
