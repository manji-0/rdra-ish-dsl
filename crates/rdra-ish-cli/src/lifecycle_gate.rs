//! What each output reads of the lifecycle, and so what a lifecycle error does to it. An
//! output that prints a reachability result refuses while the lifecycle has errors; one
//! that prints no such result writes, with one note. Every `--kind` of `list`, `csv`,
//! `diagram`, and `export` is classified here by an exhaustive match, so a new kind does
//! not compile until it says what it reads. `states` is not gated: its report is the
//! lifecycle, findings included, and it exits 1 on an error after printing it.

use rdra_ish_core::{ResolvedProgram, SemanticModel};

use crate::cli::{CsvKind, DiagramKind, ExportKind, ListKind};
use crate::list_output::{
    note_state_errors, reject_state_errors, reject_state_errors_except_deadlock,
    READS_NO_LIFECYCLE, TLC_MISSES_STATE_MODELS,
};

/// What an output reads of the lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reads {
    /// Declared facts and the data layer only.
    Declared,
    /// The transition system, printed for TLC to explore. Its build errors stop the output
    /// where it is built (`checked_system`); the explorer's findings are not in it.
    System,
    /// The explored states, such as each state model's start states and reachable states.
    States,
    /// The explored states, drawing each deadlock instead of refusing on it.
    StatesDrawingDeadlocks,
}

impl ListKind {
    pub(crate) fn reads(&self) -> Reads {
        match self {
            ListKind::Actor
            | ListKind::Entity
            | ListKind::Requirement
            | ListKind::Adr
            | ListKind::AdrImpact
            | ListKind::Goal
            | ListKind::Nfr
            | ListKind::Quality
            | ListKind::Constraint
            | ListKind::Buc
            | ListKind::Flow
            | ListKind::Step
            | ListKind::Usecase
            | ListKind::Field
            | ListKind::System
            | ListKind::Api
            | ListKind::Dto
            | ListKind::Variation
            | ListKind::Screen
            | ListKind::Event
            | ListKind::Timer
            | ListKind::Condition
            | ListKind::Rule
            | ListKind::PermissionCallables
            | ListKind::ActorPermissionAudit
            | ListKind::BusinessInputs
            | ListKind::BucRoles
            | ListKind::Transition
            | ListKind::Surface => Reads::Declared,
        }
    }
}

impl CsvKind {
    pub(crate) fn reads(&self) -> Reads {
        match self {
            CsvKind::Actor
            | CsvKind::Entity
            | CsvKind::Matrix
            | CsvKind::Api
            | CsvKind::ApiMatrix
            | CsvKind::ScreenConstraints
            | CsvKind::PermissionCallables
            | CsvKind::ActorPermissionAudit
            | CsvKind::BusinessInputs => Reads::Declared,
        }
    }
}

impl DiagramKind {
    pub(crate) fn reads(&self) -> Reads {
        match self {
            // Each state model's start states and the marks of its reachable states.
            DiagramKind::State => Reads::StatesDrawingDeadlocks,
            DiagramKind::Rdra
            | DiagramKind::Context
            | DiagramKind::Requirement
            | DiagramKind::Business
            | DiagramKind::Usecase
            | DiagramKind::Robustness
            | DiagramKind::Sequence
            | DiagramKind::Flow
            | DiagramKind::Screen
            | DiagramKind::Information
            | DiagramKind::Condition
            | DiagramKind::Er
            | DiagramKind::Event => Reads::Declared,
        }
    }
}

impl ExportKind {
    pub(crate) fn reads(&self) -> Reads {
        match self {
            // One union member per reachable state.
            ExportKind::TypeScriptStates => Reads::States,
            ExportKind::Tla => Reads::System,
            ExportKind::Openapi
            | ExportKind::Asyncapi
            | ExportKind::Dbml
            | ExportKind::JsonSchema
            | ExportKind::MermaidEr => Reads::Declared,
        }
    }
}

/// Refuses an output that prints explored states while the lifecycle has errors, and notes
/// the errors for an output that does not.
pub(crate) fn gate(
    program: &ResolvedProgram,
    model: &SemanticModel,
    reads: Reads,
) -> anyhow::Result<()> {
    match reads {
        Reads::Declared => note_state_errors(program, model, READS_NO_LIFECYCLE),
        Reads::System => note_state_errors(program, model, TLC_MISSES_STATE_MODELS),
        Reads::States => reject_state_errors(program, model)?,
        Reads::StatesDrawingDeadlocks => reject_state_errors_except_deadlock(program, model)?,
    }
    Ok(())
}
