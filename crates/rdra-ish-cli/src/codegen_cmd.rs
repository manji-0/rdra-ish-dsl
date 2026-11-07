//! `rdra-ish codegen` (experimental): the model's domain types as code, on stdout.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rdra_ish_core::{format_location, load_project, Project};
use rdra_ish_emit::codegen::{generate, Lang, Options};

use crate::cli::CodegenLang;
use crate::lifecycle_gate::{gate, Reads};
use crate::load::reject_model_errors;

pub fn run_codegen(
    inputs: &[PathBuf],
    lang: CodegenLang,
    out: Option<&Path>,
    package: &str,
) -> Result<()> {
    let Project {
        program,
        model,
        diags,
        ..
    } = load_project(inputs, None)?;
    reject_model_errors(&program, &diags)?;
    // This phase prints the data layer, which reads no reachability result.
    gate(&program, &model, Reads::Declared)?;
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
