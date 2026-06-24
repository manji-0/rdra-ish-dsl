//! Workspace-level analysis for editors and LSP.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::{
    api_diagnostics, build_merged_model, event_diagnostics, infer_usecase_transactions,
    permission_diagnostics, resolve_overlaid, system_diagnostics, tx_diagnostics, Diagnostic,
    ResolvedProgram, SemanticModel,
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
    let (program, resolve_diags) = resolve_overlaid(entry_paths, include_paths, overlays_opt);
    let (model, model_diags) = build_merged_model(&program, include_paths);

    let mut diagnostics = resolve_diags;
    diagnostics.extend(model_diags);
    diagnostics.extend(consistency_diagnostics(&model));
    diagnostics.extend(crate::lint::lint_review_diagnostics(&model));

    WorkspaceAnalysis {
        program,
        model,
        diagnostics,
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
