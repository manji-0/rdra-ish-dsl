//! `rdra-ish codegen` (experimental): the model's domain types as code, on stdout.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rdra_ish_core::lifecycle::{Bounds, Caps, Scope, Severity};
use rdra_ish_core::{format_location, load_project, Project};
use rdra_ish_emit::codegen::{generate, CheckStatus, Lang, Options};

use crate::cli::CodegenLang;
use crate::list_output::state_findings;
use crate::load::reject_model_errors;

pub fn run_codegen(
    inputs: &[PathBuf],
    lang: CodegenLang,
    out: Option<&Path>,
    package: &str,
    allow_failing: bool,
) -> Result<()> {
    let Project {
        program,
        model,
        diags,
        ..
    } = load_project(inputs, None)?;
    reject_model_errors(&program, &diags)?;
    // The state types rest on the rules `check` verifies, so the generator runs it first.
    let found = state_findings(&program, &model, &Scope::ALL, Bounds::STATES, Caps::STATES);
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
    let code = generate(&model, &options, &site)?;
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
