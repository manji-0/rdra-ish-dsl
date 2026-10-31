//! Workspace-level analysis for editors and LSP.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::project::{build_project, Project};
use crate::{
    api_diagnostics, event_diagnostics, infer_usecase_transactions, permission_diagnostics,
    system_diagnostics, tx_diagnostics, Diagnostic, ResolvedProgram, SemanticModel,
};

/// Result of analyzing a multi-file workspace.
#[derive(Debug)]
pub struct WorkspaceAnalysis {
    pub program: ResolvedProgram,
    pub model: SemanticModel,
    pub diagnostics: Vec<Diagnostic>,
}

/// Resolve, build, and collect semantic + consistency diagnostics.
pub fn analyze_workspace(
    entry_paths: &[PathBuf],
    include_paths: &[PathBuf],
    overlays: &HashMap<PathBuf, String>,
) -> WorkspaceAnalysis {
    let overlays_opt = if overlays.is_empty() {
        None
    } else {
        Some(overlays)
    };
    analyze_project(build_project(
        entry_paths.to_vec(),
        include_paths.to_vec(),
        overlays_opt,
        None,
        Vec::new(),
    ))
}

/// Add the consistency and lint-review layers to a loaded project.
pub fn analyze_project(project: Project) -> WorkspaceAnalysis {
    let Project {
        program,
        model,
        mut diags,
        ..
    } = project;
    diags.extend(consistency_diagnostics(&model));
    diags.extend(crate::lint::lint_review_diagnostics(&model));

    WorkspaceAnalysis {
        program,
        model,
        diagnostics: diags,
    }
}

/// Secondary model-wide warnings with declaration-site locations.
pub fn consistency_diagnostics(model: &SemanticModel) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    diags.extend(permission_diagnostics(model));
    diags.extend(api_diagnostics(model));
    diags.extend(system_diagnostics(model));
    let txs = infer_usecase_transactions(model);
    diags.extend(tx_diagnostics(model, &txs));
    diags.extend(event_diagnostics(model));
    diags
}
