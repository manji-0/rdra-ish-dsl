//! `diagram` CLI command.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rdra_ish_core::SemanticModel;
use rdra_ish_emit::{
    mermaid::{ErMermaidEmitter, EventFlowMermaidEmitter, RdraMermaidEmitter, StateMermaidEmitter},
    unknown_diagram_filter_kinds, Emitter, Filter, Scope, View,
};
use std::fs;

use crate::cli::DiagramKind;
use crate::list_output::{note_state_errors, reject_state_errors};
use crate::load::{load_model, reject_model_errors};

pub struct DiagramRequest {
    pub inputs: Vec<PathBuf>,
    pub kind: DiagramKind,
    pub buc: Vec<String>,
    pub node_kind: Vec<String>,
    pub edge_kind: Vec<String>,
    pub out: PathBuf,
}

pub fn run_diagram(request: DiagramRequest) -> Result<()> {
    let DiagramRequest {
        inputs,
        kind,
        buc,
        node_kind,
        edge_kind,
        out,
    } = request;
    let inputs = inputs.as_slice();
    let (program, model, diags) = load_model(inputs)?;
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

    let scope = if buc.is_empty() {
        Scope::Whole
    } else {
        Scope::Bucs(buc)
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
            DiagramKind::Rdra | DiagramKind::State | DiagramKind::EventFlow => Filter::None,
        },
        model_path: Some(model_path),
        node_kinds: Vec::new(),
        edge_kinds: Vec::new(),
    }
    .with_graph_filters(node_kind, edge_kind);

    if matches!(kind, DiagramKind::EventFlow) {
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
        DiagramKind::Er => Ok(ErMermaidEmitter.emit(model, view)?),
        DiagramKind::State => Ok(StateMermaidEmitter.emit(model, view)?),
        DiagramKind::EventFlow => Ok(EventFlowMermaidEmitter.emit(model, view)?),
    }
}
