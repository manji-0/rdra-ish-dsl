//! What each output reads of the lifecycle, and so what a lifecycle error does to it. An
//! output that prints a reachability result refuses while the lifecycle has errors. The
//! TLA+ export, which TLC checks for what the explorer finds, writes with one note. An
//! output of declared facts writes without exploring the lifecycle at all: nothing it
//! prints depends on what the explorer finds, so a search there would cost the run and say
//! nothing about the output.
//!
//! A `list` or `csv` kind is classified here by an exhaustive match, so a new kind does not
//! compile until it says what it reads. A `diagram` or `export` kind is not classified: it
//! names its emitter as an [`Output`], and the trait the emitter implements is what it
//! reads. An [`Emitter`] gets the model and the view, so it reads no lifecycle. A
//! [`StatesEmitter`] also gets [`Explored`], which only [`Output::gate`] hands out, so it is
//! never reached before its gate. An emitter that starts reading the explored states must
//! change its trait, and with it the gate. `states` is not gated: its report is the
//! lifecycle, findings included, and it exits 1 on an error after printing it.

use std::sync::OnceLock;

use rdra_ish_core::lifecycle::{derive_states_built, Bounds, Caps, Scope};
use rdra_ish_core::{ResolvedProgram, SemanticModel};
use rdra_ish_emit::{EmitError, Emitter, Explored, OnDeadlock, StatesEmitter, View};

use crate::cli::{CsvKind, ListKind};
use crate::state_findings::{
    findings_of, note_state_errors, reject_state_errors, reject_state_errors_except_deadlock,
    Finding, TLA_STATE_MODELS,
};

/// One exploration of the lifecycle: what it found, and the states it reached.
pub(crate) type Exploration = OnceLock<(Vec<Finding>, Explored)>;

/// The lifecycle at the `check` defaults, explored on the first gate that reads it and at
/// most once, so a command that gates several outputs runs one search, and one that gates
/// only outputs of declared facts runs none.
pub(crate) struct Lifecycle<'a> {
    program: &'a ResolvedProgram,
    model: &'a SemanticModel,
    /// `memory::generator_caps`: what `check` explores at by default.
    caps: Caps,
    explored: Held<'a>,
}

/// The exploration a [`Lifecycle`] runs for itself, or the one a
/// [`crate::session::Session`] keeps for every output it draws.
enum Held<'a> {
    Own(Exploration),
    Shared(&'a Exploration),
}

impl<'a> Lifecycle<'a> {
    pub(crate) fn new(program: &'a ResolvedProgram, model: &'a SemanticModel, caps: Caps) -> Self {
        Lifecycle {
            program,
            model,
            caps,
            explored: Held::Own(OnceLock::new()),
        }
    }

    /// A lifecycle whose exploration is `explored`: the first lifecycle sharing it to be
    /// gated runs it, and the rest read it.
    pub(crate) fn sharing(
        program: &'a ResolvedProgram,
        model: &'a SemanticModel,
        caps: Caps,
        explored: &'a Exploration,
    ) -> Self {
        Lifecycle {
            program,
            model,
            caps,
            explored: Held::Shared(explored),
        }
    }

    fn explored(&self) -> &(Vec<Finding>, Explored) {
        let once = match &self.explored {
            Held::Own(once) => once,
            Held::Shared(once) => once,
        };
        once.get_or_init(|| {
            let (report, ts) =
                derive_states_built(self.model, &Scope::ALL, Bounds::STATES, self.caps);
            let findings = findings_of(self.program, self.model, &report, &ts).findings;
            (findings, Explored::from_report(&report))
        })
    }
}

/// What an output reads of the lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reads {
    /// Declared facts and the data layer only. Its gate explores nothing.
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
/// the errors for the TLA+ export. An output of declared facts passes without exploring.
pub(crate) fn gate(lifecycle: &Lifecycle, reads: Reads) -> anyhow::Result<()> {
    match reads {
        Reads::Declared => {}
        Reads::System => note_state_errors(&lifecycle.explored().0, TLA_STATE_MODELS),
        Reads::States => reject_state_errors(&lifecycle.explored().0)?,
        Reads::StatesDrawingDeadlocks => {
            reject_state_errors_except_deadlock(&lifecycle.explored().0)?
        }
    }
    Ok(())
}

/// An output's emitter, by what the emitter reads.
pub(crate) enum Output {
    Declared(Box<dyn Emitter>),
    States(Box<dyn StatesEmitter>),
}

/// An [`Output`] that has passed its gate, with the explored states its emitter reads.
pub(crate) enum Gated<'a> {
    Declared(Box<dyn Emitter>),
    States(Box<dyn StatesEmitter>, &'a Explored),
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

    /// Applies the gate of what the emitter reads, then hands it the states it reads.
    pub(crate) fn gate<'a>(self, lifecycle: &'a Lifecycle) -> anyhow::Result<Gated<'a>> {
        gate(lifecycle, self.reads())?;
        Ok(match self {
            Output::Declared(emitter) => Gated::Declared(emitter),
            Output::States(emitter) => Gated::States(emitter, &lifecycle.explored().1),
        })
    }
}

impl Gated<'_> {
    pub(crate) fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        match self {
            Gated::Declared(emitter) => emitter.emit(model, view),
            Gated::States(emitter, explored) => emitter.emit(model, view, explored),
        }
    }
}

#[cfg(test)]
mod tests {
    use rdra_ish_core::SemanticModel;
    use rdra_ish_emit::mermaid::{ErMermaidEmitter, StateMermaidEmitter};
    use rdra_ish_emit::{EmitError, Explored, OnDeadlock, StatesEmitter, View};

    use super::{Output, Reads};

    /// A states emitter that refuses on a deadlock. No shipped output does since the
    /// TypeScript state unions were removed; the gate keeps the case.
    struct Refusing;

    impl StatesEmitter for Refusing {
        fn on_deadlock(&self) -> OnDeadlock {
            OnDeadlock::Refuse
        }

        fn emit(&self, _: &SemanticModel, _: &View, _: &Explored) -> Result<String, EmitError> {
            Ok(String::new())
        }
    }

    #[test]
    fn what_an_output_reads_follows_the_trait_its_emitter_implements() {
        assert_eq!(Output::declared(ErMermaidEmitter).reads(), Reads::Declared);
        assert_eq!(Output::states(Refusing).reads(), Reads::States);
        assert_eq!(
            Output::states(StateMermaidEmitter).reads(),
            Reads::StatesDrawingDeadlocks
        );
    }
}
