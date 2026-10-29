//! `diagram` CLI command.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rdra_ish_core::SemanticModel;
use rdra_ish_emit::{
    mermaid::{ErMermaidEmitter, EventFlowMermaidEmitter, RdraMermaidEmitter, StateMermaidEmitter},
    unknown_diagram_filter_kinds, Emitter, Filter, Scope, View,
};
use std::fs;

use crate::cli::{DiagramKind, DiagramViewPreset};
use crate::list_output::{note_state_errors, reject_state_errors};
use crate::load::{diagram_preset_filters, load_model, reject_model_errors};

pub struct DiagramRequest {
    pub inputs: Vec<PathBuf>,
    pub kind: DiagramKind,
    pub buc: Vec<String>,
    pub show_description: bool,
    pub node_kind: Vec<String>,
    pub edge_kind: Vec<String>,
    pub view_preset: Option<DiagramViewPreset>,
    pub out: PathBuf,
}

pub fn run_diagram(request: DiagramRequest) -> Result<()> {
    let DiagramRequest {
        inputs,
        kind,
        buc,
        show_description,
        node_kind,
        edge_kind,
        view_preset,
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

    if (!node_kind.is_empty() || !edge_kind.is_empty() || view_preset.is_some())
        && !matches!(kind, DiagramKind::Rdra)
    {
        anyhow::bail!(
            "--node-kind, --edge-kind, and --view-preset are currently supported only for --kind rdra"
        );
    }
    // Validate user-supplied kinds only (presets are authored from the known set).
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
    let (preset_node_kinds, preset_edge_kinds) = diagram_preset_filters(&view_preset);
    let node_kinds = if node_kind.is_empty() {
        preset_node_kinds
    } else {
        node_kind
    };
    let edge_kinds = if edge_kind.is_empty() {
        preset_edge_kinds
    } else {
        edge_kind
    };

    let view = match &kind {
        DiagramKind::Er => View {
            scope,
            filter: Filter::Er,
            show_descriptions: show_description,
            node_kinds: Vec::new(),
            edge_kinds: Vec::new(),
        }
        .with_graph_filters(node_kinds, edge_kinds),
        DiagramKind::Rdra | DiagramKind::State | DiagramKind::EventFlow => View {
            scope,
            filter: Filter::None,
            show_descriptions: show_description,
            node_kinds: Vec::new(),
            edge_kinds: Vec::new(),
        }
        .with_graph_filters(node_kinds, edge_kinds),
    };

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
