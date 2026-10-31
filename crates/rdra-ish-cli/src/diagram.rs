//! `diagram` CLI command.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rdra_ish_core::SemanticModel;
use rdra_ish_emit::{
    mermaid::{
        BusinessMermaidEmitter, ConditionMermaidEmitter, ContextMermaidEmitter, ErMermaidEmitter,
        EventMermaidEmitter, FlowMermaidEmitter, InformationMermaidEmitter, RdraMermaidEmitter,
        RequirementMermaidEmitter, RobustnessMermaidEmitter, StateMermaidEmitter,
        UsecaseMermaidEmitter,
    },
    unknown_diagram_filter_kinds, Emitter, Filter, Scope, View,
};
use std::fs;

use crate::cli::DiagramKind;
use crate::list_output::{note_state_errors, reject_state_errors};
use crate::load::reject_model_errors;
use rdra_ish_core::{load_project, Project};

pub struct DiagramRequest {
    pub inputs: Vec<PathBuf>,
    pub kind: DiagramKind,
    pub buc: Vec<String>,
    pub usecase: Vec<String>,
    pub node_kind: Vec<String>,
    pub edge_kind: Vec<String>,
    pub out: PathBuf,
}

pub fn run_diagram(request: DiagramRequest) -> Result<()> {
    let DiagramRequest {
        inputs,
        kind,
        buc,
        usecase,
        node_kind,
        edge_kind,
        out,
    } = request;
    let inputs = inputs.as_slice();
    let Project {
        program,
        model,
        diags,
        ..
    } = load_project(inputs, None)?;
    reject_model_errors(&program, &diags)?;
    if kind.reads_lifecycle() {
        reject_state_errors(&program, &model)?;
    } else {
        note_state_errors(&program, &model);
    }

    for buc_id in &buc {
        if !model.bucs.values().any(|b| b.id == *buc_id) {
            anyhow::bail!("unknown buc `{buc_id}`");
        }
    }

    if !usecase.is_empty() {
        if !matches!(kind, DiagramKind::Usecase | DiagramKind::Robustness) {
            anyhow::bail!("--usecase applies only to --kind usecase or robustness");
        }
        if !buc.is_empty() {
            anyhow::bail!("--usecase and --buc cannot be combined; pass one of them");
        }
    }
    for usecase_id in &usecase {
        if !model.use_cases.values().any(|u| u.id == *usecase_id) {
            anyhow::bail!("unknown usecase `{usecase_id}`");
        }
    }

    if (!node_kind.is_empty() || !edge_kind.is_empty()) && !matches!(kind, DiagramKind::Rdra) {
        anyhow::bail!("--node-kind and --edge-kind are currently supported only for --kind rdra");
    }
    if !node_kind.is_empty() || !edge_kind.is_empty() {
        let (unknown_nodes, unknown_edges) = unknown_diagram_filter_kinds(&node_kind, &edge_kind);
        if !unknown_nodes.is_empty() || !unknown_edges.is_empty() {
            let mut parts = Vec::new();
            if !unknown_nodes.is_empty() {
                parts.push(format!("unknown --node-kind: {}", unknown_nodes.join(", ")));
            }
            if !unknown_edges.is_empty() {
                parts.push(format!("unknown --edge-kind: {}", unknown_edges.join(", ")));
            }
            anyhow::bail!("{}", parts.join("; "));
        }
    }

    let scope = if !usecase.is_empty() {
        Scope::UseCases(usecase)
    } else if !buc.is_empty() {
        Scope::Bucs(buc)
    } else {
        Scope::Whole
    };
    let model_path = inputs
        .iter()
        .map(|input| input.display().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let view = View {
        scope,
        filter: match kind {
            DiagramKind::Er => Filter::Er,
            DiagramKind::Rdra
            | DiagramKind::Context
            | DiagramKind::Requirement
            | DiagramKind::Business
            | DiagramKind::Usecase
            | DiagramKind::Robustness
            | DiagramKind::Information
            | DiagramKind::Condition
            | DiagramKind::Flow
            | DiagramKind::State
            | DiagramKind::Event => Filter::None,
        },
        model_path: Some(model_path),
        node_kinds: Vec::new(),
        edge_kinds: Vec::new(),
    }
    .with_graph_filters(node_kind, edge_kind);

    if matches!(kind, DiagramKind::Event) {
        for diag in rdra_ish_core::event_diagnostics(&model) {
            eprintln!("warning: {}", diag.error);
        }
    }

    let diagram_text = emit_diagram(&model, &kind, &view)?;
    let out_path = out.with_extension("mmd");
    fs::write(&out_path, &diagram_text)
        .with_context(|| format!("failed to write {}", out_path.display()))?;
    println!("wrote {}", out_path.display());

    Ok(())
}

fn emit_diagram(model: &SemanticModel, kind: &DiagramKind, view: &View) -> Result<String> {
    match kind {
        DiagramKind::Rdra => Ok(RdraMermaidEmitter.emit(model, view)?),
        DiagramKind::Context => Ok(ContextMermaidEmitter.emit(model, view)?),
        DiagramKind::Requirement => Ok(RequirementMermaidEmitter.emit(model, view)?),
        DiagramKind::Business => Ok(BusinessMermaidEmitter.emit(model, view)?),
        DiagramKind::Usecase => Ok(UsecaseMermaidEmitter.emit(model, view)?),
        DiagramKind::Robustness => Ok(RobustnessMermaidEmitter.emit(model, view)?),
        DiagramKind::Information => Ok(InformationMermaidEmitter.emit(model, view)?),
        DiagramKind::Condition => Ok(ConditionMermaidEmitter.emit(model, view)?),
        DiagramKind::Flow => Ok(FlowMermaidEmitter.emit(model, view)?),
        DiagramKind::Er => Ok(ErMermaidEmitter.emit(model, view)?),
        DiagramKind::State => Ok(StateMermaidEmitter.emit(model, view)?),
        DiagramKind::Event => Ok(EventMermaidEmitter.emit(model, view)?),
    }
}
