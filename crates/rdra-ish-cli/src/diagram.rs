//! `diagram` CLI command.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rdra_ish_core::SemanticModel;
use rdra_ish_emit::{
    diff::{DiffMermaidEmitter, DiffPlantUmlEmitter},
    mermaid::{
        BusinessAreaMermaidEmitter, ErMermaidEmitter, EventFlowMermaidEmitter,
        ObjectGraphMermaidEmitter, RdraMermaidEmitter, SequenceMermaidEmitter, StateMermaidEmitter,
        TechnicalAreaMermaidEmitter,
    },
    plantuml::{
        BusinessAreaPlantUmlEmitter, ErPlantUmlEmitter, EventFlowPlantUmlEmitter,
        ObjectGraphPlantUmlEmitter, RdraPlantUmlEmitter, SequenceDiagramEmitter,
        StateDiagramEmitter, TechnicalAreaPlantUmlEmitter,
    },
    unknown_diagram_filter_kinds, Emitter, Filter, Scope, View,
};
use std::fs;

use crate::cli::{DiagramKind, DiagramViewPreset, OutputFormat};
use crate::load::{diagram_preset_filters, load_model, reject_model_errors};

pub struct DiagramRequest {
    pub inputs: Vec<PathBuf>,
    pub kind: DiagramKind,
    pub format: OutputFormat,
    pub buc: Vec<String>,
    pub usecase: Vec<String>,
    pub diff_base: Vec<PathBuf>,
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
        format,
        buc,
        usecase,
        diff_base,
        show_description,
        node_kind,
        edge_kind,
        view_preset,
        out,
    } = request;
    let inputs = inputs.as_slice();
    let (program, model, diags) = load_model(inputs)?;
    reject_model_errors(&program, &diags)?;

    for buc_id in &buc {
        if !model.bucs.values().any(|b| b.id == *buc_id) {
            anyhow::bail!("unknown buc `{buc_id}`");
        }
    }
    for uc_id in &usecase {
        if !model.use_cases.values().any(|u| u.id == *uc_id) {
            anyhow::bail!("unknown usecase `{uc_id}`");
        }
    }

    if !usecase.is_empty() && !matches!(kind, DiagramKind::Sequence | DiagramKind::BusinessArea) {
        anyhow::bail!(
            "--usecase is currently supported only for --kind sequence and --kind business-area"
        );
    }
    if !usecase.is_empty() && !buc.is_empty() {
        anyhow::bail!("--buc and --usecase cannot be combined");
    }
    if (!node_kind.is_empty() || !edge_kind.is_empty() || view_preset.is_some())
        && !matches!(
            kind,
            DiagramKind::Rdra | DiagramKind::BoundarylessGraph | DiagramKind::Diff
        )
    {
        anyhow::bail!(
            "--node-kind, --edge-kind, and --view-preset are currently supported only for --kind rdra, --kind boundaryless-graph, or --kind diff"
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
    if matches!(kind, DiagramKind::Diff) && diff_base.is_empty() {
        anyhow::bail!("--kind diff requires at least one --diff-base path");
    }
    if !matches!(kind, DiagramKind::Diff) && !diff_base.is_empty() {
        anyhow::bail!("--diff-base is supported only with --kind diff");
    }

    let scope = if !usecase.is_empty() {
        Scope::UseCases(usecase)
    } else if buc.is_empty() {
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
        DiagramKind::Rdra
        | DiagramKind::BoundarylessGraph
        | DiagramKind::State
        | DiagramKind::Sequence
        | DiagramKind::EventFlow
        | DiagramKind::Diff
        | DiagramKind::BusinessArea
        | DiagramKind::TechnicalArea => View {
            scope,
            filter: Filter::None,
            show_descriptions: show_description,
            node_kinds: Vec::new(),
            edge_kinds: Vec::new(),
        }
        .with_graph_filters(node_kinds, edge_kinds),
    };

    if matches!(kind, DiagramKind::Sequence) {
        let txs = rdra_ish_core::infer_usecase_transactions(&model);
        for diag in rdra_ish_core::tx_diagnostics(&model, &txs) {
            eprintln!("warning: {}", diag.error);
        }
        for diag in rdra_ish_core::api_diagnostics(&model) {
            eprintln!("warning: {}", diag.error);
        }
        for diag in rdra_ish_core::system_diagnostics(&model) {
            eprintln!("warning: {}", diag.error);
        }
    }

    if matches!(kind, DiagramKind::EventFlow) {
        for diag in rdra_ish_core::event_diagnostics(&model) {
            eprintln!("warning: {}", diag.error);
        }
    }

    let diagram_text = emit_diagram(&model, &kind, &format, &view, &diff_base)?;

    match format {
        OutputFormat::Puml => {
            let out_path = out.with_extension("puml");
            fs::write(&out_path, &diagram_text)
                .with_context(|| format!("failed to write {}", out_path.display()))?;
            println!("wrote {}", out_path.display());
        }
        OutputFormat::Mermaid => {
            let out_path = out.with_extension("mmd");
            fs::write(&out_path, &diagram_text)
                .with_context(|| format!("failed to write {}", out_path.display()))?;
            println!("wrote {}", out_path.display());
        }
        OutputFormat::Svg => {
            use rdra_ish_render::{render_to_file, PlantumlCliRenderer, RenderFormat};
            let renderer =
                PlantumlCliRenderer::discover().context("failed to find plantuml.jar")?;
            let out_path = out.with_extension("svg");
            render_to_file(&renderer, &diagram_text, &out_path, RenderFormat::Svg)
                .context("plantuml rendering failed")?;
            println!("wrote {}", out_path.display());
        }
        OutputFormat::Png => {
            use rdra_ish_render::{render_to_file, PlantumlCliRenderer, RenderFormat};
            let renderer =
                PlantumlCliRenderer::discover().context("failed to find plantuml.jar")?;
            let out_path = out.with_extension("png");
            render_to_file(&renderer, &diagram_text, &out_path, RenderFormat::Png)
                .context("plantuml rendering failed")?;
            println!("wrote {}", out_path.display());
        }
    }

    Ok(())
}

fn emit_diagram(
    model: &SemanticModel,
    kind: &DiagramKind,
    format: &OutputFormat,
    view: &View,
    diff_base: &[PathBuf],
) -> Result<String> {
    match format {
        OutputFormat::Mermaid => match kind {
            DiagramKind::Rdra => Ok(ObjectGraphMermaidEmitter.emit(model, view)?),
            DiagramKind::BoundarylessGraph => Ok(RdraMermaidEmitter.emit(model, view)?),
            DiagramKind::Er => Ok(ErMermaidEmitter.emit(model, view)?),
            DiagramKind::State => Ok(StateMermaidEmitter.emit(model, view)?),
            DiagramKind::Sequence => Ok(SequenceMermaidEmitter.emit(model, view)?),
            DiagramKind::EventFlow => Ok(EventFlowMermaidEmitter.emit(model, view)?),
            DiagramKind::Diff => {
                let (program, base_model, diags) = load_model(diff_base)?;
                reject_model_errors(&program, &diags)?;
                Ok(DiffMermaidEmitter { base: &base_model }.emit_diff(model, view)?)
            }
            DiagramKind::BusinessArea => Ok(BusinessAreaMermaidEmitter.emit(model, view)?),
            DiagramKind::TechnicalArea => Ok(TechnicalAreaMermaidEmitter.emit(model, view)?),
        },
        _ => match kind {
            DiagramKind::Rdra => Ok(ObjectGraphPlantUmlEmitter.emit(model, view)?),
            DiagramKind::BoundarylessGraph => Ok(RdraPlantUmlEmitter.emit(model, view)?),
            DiagramKind::Er => Ok(ErPlantUmlEmitter.emit(model, view)?),
            DiagramKind::State => Ok(StateDiagramEmitter.emit(model, view)?),
            DiagramKind::Sequence => Ok(SequenceDiagramEmitter.emit(model, view)?),
            DiagramKind::EventFlow => Ok(EventFlowPlantUmlEmitter.emit(model, view)?),
            DiagramKind::Diff => {
                let (program, base_model, diags) = load_model(diff_base)?;
                reject_model_errors(&program, &diags)?;
                Ok(DiffPlantUmlEmitter { base: &base_model }.emit_diff(model, view)?)
            }
            DiagramKind::BusinessArea => Ok(BusinessAreaPlantUmlEmitter.emit(model, view)?),
            DiagramKind::TechnicalArea => Ok(TechnicalAreaPlantUmlEmitter.emit(model, view)?),
        },
    }
}
