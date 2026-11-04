//! What each output reads of the lifecycle, and so what a lifecycle error does to it. An
//! output that prints a reachability result refuses while the lifecycle has errors; one
//! that prints no such result writes, with one note.
//!
//! A `list` or `csv` kind is classified here by an exhaustive match, so a new kind does not
//! compile until it says what it reads. A `diagram` or `export` kind is not classified: it
//! names its emitter as an [`Output`], and the trait the emitter implements is what it
//! reads. An [`Emitter`] gets the model and the view, so it reads no lifecycle. A
//! [`StatesEmitter`] also gets [`Explored`], which only [`Output::gate`] builds, so it is
//! never reached before its gate. An emitter that starts reading the explored states must
//! change its trait, and with it the gate. `states` is not gated: its report is the
//! lifecycle, findings included, and it exits 1 on an error after printing it.

use rdra_ish_core::lifecycle::{derive_states_built, Bounds, Caps, Scope};
use rdra_ish_core::{ResolvedProgram, SemanticModel};
use rdra_ish_emit::{EmitError, Emitter, Explored, OnDeadlock, StatesEmitter, View};

use crate::cli::{CsvKind, ListKind};
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

/// An output's emitter, by what the emitter reads.
pub(crate) enum Output {
    Declared(Box<dyn Emitter>),
    States(Box<dyn StatesEmitter>),
}

/// An [`Output`] that has passed its gate, with the explored states its emitter reads.
pub(crate) enum Gated {
    Declared(Box<dyn Emitter>),
    States(Box<dyn StatesEmitter>, Explored),
}

impl Output {
    pub(crate) fn declared(emitter: impl Emitter + 'static) -> Self {
        Output::Declared(Box::new(emitter))
    }

    pub(crate) fn states(emitter: impl StatesEmitter + 'static) -> Self {
        Output::States(Box::new(emitter))
    }

    /// What the emitter reads, from the trait it implements.
    pub(crate) fn reads(&self) -> Reads {
        match self {
            Output::Declared(_) => Reads::Declared,
            Output::States(emitter) => match emitter.on_deadlock() {
                OnDeadlock::Refuse => Reads::States,
                OnDeadlock::Draw => Reads::StatesDrawingDeadlocks,
            },
        }
    }

    /// Applies the gate of what the emitter reads, then explores the states it reads.
    pub(crate) fn gate(
        self,
        program: &ResolvedProgram,
        model: &SemanticModel,
    ) -> anyhow::Result<Gated> {
        gate(program, model, self.reads())?;
        Ok(match self {
            Output::Declared(emitter) => Gated::Declared(emitter),
            Output::States(emitter) => {
                let (report, _) =
                    derive_states_built(model, &Scope::ALL, Bounds::STATES, Caps::STATES);
                Gated::States(emitter, Explored::from_report(&report))
            }
        })
    }
}

impl Gated {
    pub(crate) fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        match self {
            Gated::Declared(emitter) => emitter.emit(model, view),
            Gated::States(emitter, explored) => emitter.emit(model, view, explored),
        }
    }
}

#[cfg(test)]
mod tests {
    use rdra_ish_emit::mermaid::{ErMermaidEmitter, StateMermaidEmitter};
    use rdra_ish_emit::typescript::TypeScriptStateUnionEmitter;

    use super::{Output, Reads};

    #[test]
    fn what_an_output_reads_follows_the_trait_its_emitter_implements() {
        assert_eq!(Output::declared(ErMermaidEmitter).reads(), Reads::Declared);
        assert_eq!(
            Output::states(TypeScriptStateUnionEmitter::default()).reads(),
            Reads::States
        );
        assert_eq!(
            Output::states(StateMermaidEmitter).reads(),
            Reads::StatesDrawingDeadlocks
        );
    }
}
