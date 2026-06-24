//! Model loading and diagram preset helpers for the CLI.

use anyhow::Result;
use rdra_ish_core::{
    build_merged_model, format_diagnostic_message, resolve, Diagnostic, ResolvedProgram,
    SemanticModel,
};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::cli::DiagramViewPreset;

pub(crate) fn collect_rdra_files(inputs: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = vec![];
    for input in inputs {
        if input.is_file() {
            files.push(input.clone());
        } else if input.is_dir() {
            for entry in walkdir::WalkDir::new(input).into_iter().flatten() {
                if entry.path().extension().is_some_and(|e| e == "rdra") {
                    files.push(entry.path().to_owned());
                }
            }
        }
    }
    files
}

/// Compute the set of include paths by going up one level from any directories
/// that contain the entry files.  This ensures `shared/actors.rdra` resolves
/// from a root that contains both `shared/` and `buc/`.
fn root_include_paths(files: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut paths = vec![];
    for f in files {
        // Canonicalize so relative paths work.
        let canon = fs::canonicalize(f).unwrap_or_else(|_| f.clone());
        // For a file like /root/buc/buc_purchase.rdra we want /root as include path.
        // Walk up from the file's parent until we can't go further.
        if let Some(parent) = canon.parent() {
            // Try parent and grandparent.
            for ancestor in [parent, parent.parent().unwrap_or(parent)] {
                let ap = ancestor.to_path_buf();
                if seen.insert(ap.clone()) {
                    paths.push(ap);
                }
            }
        }
    }
    paths
}

pub(crate) fn eprint_diagnostic(program: &ResolvedProgram, diag: &Diagnostic) {
    let message = format_diagnostic_message(
        Some(program),
        diag.is_warning,
        diag.location.as_ref(),
        &diag.error.to_string(),
    );
    eprintln!("{message}");
}

pub(crate) fn load_model(
    inputs: &[PathBuf],
) -> Result<(ResolvedProgram, SemanticModel, Vec<Diagnostic>)> {
    let entry_files = collect_rdra_files(inputs);
    if entry_files.is_empty() {
        anyhow::bail!("no .rdra files found in the given inputs");
    }

    let include_paths = root_include_paths(&entry_files);

    let (program, resolve_diags) = resolve(&entry_files, &include_paths);

    let (model, model_diags) = build_merged_model(&program, &include_paths);

    let mut all_diags = resolve_diags;
    all_diags.extend(model_diags);

    Ok((program, model, all_diags))
}

pub(crate) fn diagram_preset_filters(
    preset: &Option<DiagramViewPreset>,
) -> (Vec<String>, Vec<String>) {
    let Some(preset) = preset else {
        return (Vec::new(), Vec::new());
    };
    let (nodes, edges): (&[&str], &[&str]) = match preset {
        DiagramViewPreset::Business => (
            &[
                "actor",
                "requirement",
                "nfr",
                "business",
                "buc",
                "flow",
                "step",
                "usecase",
                "event",
            ],
            &[
                "performs",
                "motivates",
                "belongs",
                "contains",
                "precedes",
                "branches",
                "excepts",
                "repeats",
                "covers",
                "raises",
                "triggers",
                "compensates",
            ],
        ),
        DiagramViewPreset::System => (
            &[
                "usecase", "screen", "field", "event", "system", "api", "dto", "entity",
            ],
            &[
                "contains",
                "invokes",
                "request",
                "response",
                "error-response",
                "reads",
                "writes",
                "creates",
                "updates",
                "deletes",
                "displays",
                "shows",
                "maps-field",
                "raises",
                "triggers",
                "owns",
            ],
        ),
        DiagramViewPreset::Data => (
            &[
                "concept",
                "domain-object",
                "aggregate",
                "value-object",
                "entity",
                "state",
            ],
            &["contains", "maps-to", "relate", "transitions", "owns"],
        ),
        DiagramViewPreset::Api => (
            &[
                "usecase",
                "system",
                "api",
                "dto",
                "entity",
                "permission",
                "medium",
            ],
            &[
                "contains",
                "invokes",
                "request",
                "response",
                "error-response",
                "reads",
                "writes",
                "creates",
                "updates",
                "deletes",
                "requires-permission",
                "requires-medium",
                "owns",
            ],
        ),
        DiagramViewPreset::Ui => (
            &["actor", "buc", "usecase", "screen", "field", "entity"],
            &["performs", "contains", "displays", "shows", "maps-field"],
        ),
    };
    (
        nodes.iter().map(|value| (*value).to_string()).collect(),
        edges.iter().map(|value| (*value).to_string()).collect(),
    )
}
