//! `diagram` CLI command.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rdra_ish_core::lifecycle::{derive_states_built, Caps, Scope as LifecycleScope};
use rdra_ish_core::SemanticModel;
use rdra_ish_emit::{
    mermaid::{
        BusinessMermaidEmitter, ConditionMermaidEmitter, ContextMermaidEmitter, ErMermaidEmitter,
        EventMermaidEmitter, FlowMermaidEmitter, InformationMermaidEmitter, RdraMermaidEmitter,
        RequirementMermaidEmitter, RobustnessMermaidEmitter, ScreenMermaidEmitter,
        SequenceMermaidEmitter, StateMermaidEmitter, UsecaseMermaidEmitter,
    },
    unknown_diagram_filter_kinds, Emitter, Filter, Scope, StateMarks, StateView, View,
};
use std::fs;

use crate::cli::DiagramKind;
use crate::list_output::{note_state_errors, reject_state_errors_except_deadlock};
use crate::load::reject_model_errors;
use rdra_ish_core::{load_project, Project};

pub struct DiagramRequest {
    pub inputs: Vec<PathBuf>,
    pub kind: DiagramKind,
    pub buc: Vec<String>,
    pub usecase: Vec<String>,
    pub entity: Vec<String>,
    pub node_kind: Vec<String>,
    pub edge_kind: Vec<String>,
    pub out: Option<PathBuf>,
}

pub fn run_diagram(request: DiagramRequest) -> Result<()> {
    let DiagramRequest {
        inputs,
        kind,
        buc,
        usecase,
        entity,
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
    let mut state = StateView::default();
    if kind.reads_lifecycle() {
        // A deadlock is coloured in the diagram, not a reason to refuse it.
        reject_state_errors_except_deadlock(&program, &model)?;
        let (report, _) = derive_states_built(&model, &LifecycleScope::ALL, Caps::STATES);
        state.marks = StateMarks::from_report(&report);
    } else {
        note_state_errors(&program, &model);
    }

    for buc_id in &buc {
        if !model.bucs.values().any(|b| b.id == *buc_id) {
            anyhow::bail!("unknown buc `{buc_id}`");
        }
    }

    if matches!(kind, DiagramKind::Sequence) {
        if !buc.is_empty() {
            anyhow::bail!("--kind sequence draws one use case; pass --usecase, not --buc");
        }
        if usecase.len() != 1 {
            anyhow::bail!("--kind sequence needs exactly one --usecase <id>");
        }
    }

    if !usecase.is_empty() {
        if !matches!(
            kind,
            DiagramKind::Usecase | DiagramKind::Robustness | DiagramKind::Sequence
        ) {
            anyhow::bail!("--usecase applies only to --kind usecase, robustness, or sequence");
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

    if !entity.is_empty() {
        if !matches!(kind, DiagramKind::State) {
            anyhow::bail!("--entity applies only to --kind state");
        }
        if !buc.is_empty() {
            anyhow::bail!("--entity and --buc cannot be combined; pass one of them");
        }
    }
    for entity_id in &entity {
        let Some((key, _)) = model.entities.iter().find(|(_, e)| e.id == *entity_id) else {
            anyhow::bail!("unknown entity `{entity_id}`");
        };
        let modeled = model.entities[key]
            .columns
            .iter()
            .any(|c| model.is_lifecycle_column(key, &c.name));
        if !modeled {
            anyhow::bail!("entity `{entity_id}` has no state model");
        }
    }
    state.entities = entity;

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
        state,
        scope,
        filter: match kind {
            DiagramKind::Er => Filter::Er,
            DiagramKind::Rdra
            | DiagramKind::Context
            | DiagramKind::Requirement
            | DiagramKind::Business
            | DiagramKind::Usecase
            | DiagramKind::Robustness
            | DiagramKind::Sequence
            | DiagramKind::Information
            | DiagramKind::Condition
            | DiagramKind::Flow
            | DiagramKind::Screen
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
    match out {
        Some(out) => {
            let out_path = out.with_extension("mmd");
            fs::write(&out_path, &diagram_text)
                .with_context(|| format!("failed to write {}", out_path.display()))?;
            println!("wrote {}", out_path.display());
        }
        None => print!("{diagram_text}"),
    }

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
        DiagramKind::Sequence => Ok(SequenceMermaidEmitter.emit(model, view)?),
        DiagramKind::Information => Ok(InformationMermaidEmitter.emit(model, view)?),
        DiagramKind::Condition => Ok(ConditionMermaidEmitter.emit(model, view)?),
        DiagramKind::Flow => Ok(FlowMermaidEmitter.emit(model, view)?),
        DiagramKind::Screen => Ok(ScreenMermaidEmitter.emit(model, view)?),
        DiagramKind::Er => Ok(ErMermaidEmitter.emit(model, view)?),
        DiagramKind::State => Ok(StateMermaidEmitter.emit(model, view)?),
        DiagramKind::Event => Ok(EventMermaidEmitter.emit(model, view)?),
    }
}
