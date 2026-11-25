//! A counterexample of the `Plain` run on a reduced cone, lifted to the unreduced cone without
//! exploring it (docs/design/native-temporal-verification.md, "Lifting a reduced lasso",
//! phase 5).
//!
//! The prefix and the bridge lift with `TransitionSystem::lift`, as a trace does. A loop
//! lifts to a path that returns to the same class and maybe to another concrete state of the
//! sub-lifecycle, so it is lifted copy after copy until a concrete state repeats
//! (`lift_cycle`). The lifted lasso certifies itself on the unreduced cone: it replays
//! through `moves`, fails the property, and, for an `eventually` or a `leads_to`, its loop is
//! fair under `Fair_P` alone (`unfair_loop` with `Fairness::Plain`). One that does not is
//! `Spurious`, never printed: the caller explores the unreduced cone as before.

use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::expr::State;
use crate::lifecycle::lasso::{Fairness, Loop};
use crate::lifecycle::reduce::Step;
use crate::lifecycle::types::{ActId, Ltl};
use crate::lifecycle::TransitionSystem;

use super::liveness::StopRule;
use super::{ComponentRun, Edge, Spurious};

/// The counterexample the `Plain` run of a reduced cone found, as steps of the reduced graph:
/// what the lift reads once the run is dropped.
#[derive(Clone, Debug)]
pub struct ReducedLasso {
    start: State,
    /// From `start`: for an `always`, the path to the state that breaks it; for an
    /// `eventually` or a `leads_to`, the path to the state it stops at or its loop starts at.
    prefix: Vec<Step>,
    /// The loop of a liveness counterexample that cycles: from the last state of `prefix`
    /// back to it.
    cycle: Option<Vec<Step>>,
    /// An `eventually` or a `leads_to`, whose loop must be fair.
    liveness: bool,
}

/// A `ReducedLasso` lifted to the unreduced cone and certified there: `actions[k]` takes
/// `states[k]` to `states[k + 1]`, and with `Loop::Back` the last action takes the last
/// state back. `diag` is what the cone's own run would print for it.
#[derive(Clone, Debug)]
pub struct LiftedLasso {
    pub states: Vec<State>,
    pub actions: Vec<ActId>,
    pub tail: Loop,
    pub diag: Box<StateDiag>,
}

impl ComponentRun {
    /// The counterexample of `properties()[i]` on this run of a reduced cone, as the `Plain`
    /// run finds it (`decide_property` for an `always`, `decide_liveness` under
    /// `Fairness::Plain` otherwise), kept as reduced steps; `None` when this graph shows none
    /// or the property is of another form.
    pub fn reduced_lasso(&self, i: usize) -> Option<ReducedLasso> {
        let p = &self.system.properties()[i];
        let (first, path, cycle, liveness) = match &p.formula {
            Ltl::Always(x) => {
                let (first, path) = self.unsafe_state(p, x)?;
                (first, path, None, false)
            }
            Ltl::Eventually(_) | Ltl::LeadsTo(..) => {
                let (fair, avoid, start) =
                    self.liveness_search(i, StopRule::Fair(Fairness::Plain))?;
                let (first, mut walk, tail) = self.walk(&fair, &avoid, start);
                let cycle = match tail {
                    Loop::Stutter => None,
                    Loop::Back { to } => Some(walk.split_off(to - 1)),
                };
                (first, walk, cycle, true)
            }
            _ => return None,
        };
        let step = |e: &Edge| -> Step {
            let (act, params) = self.step(e);
            (
                act,
                params.into(),
                self.states.at(e.to as usize).into_state(),
            )
        };
        Some(ReducedLasso {
            start: self.states.at(first as usize).into_state(),
            prefix: path.iter().map(step).collect(),
            cycle: cycle.map(|c| c.iter().map(step).collect()),
            liveness,
        })
    }
}

impl ReducedLasso {
    /// The steps of the reduced counterexample, its loop included.
    pub fn steps(&self) -> usize {
        self.prefix.len() + self.cycle.as_ref().map_or(0, Vec::len)
    }

    /// This counterexample of `reduced`, lifted to `cone`, the unreduced cone with the
    /// property at `cone_i`, and certified there. Explores the sub-lifecycle the reduction
    /// came from (`lift`), never `cone`.
    pub fn lift(
        &self,
        reduced: &TransitionSystem,
        cone: &TransitionSystem,
        cone_i: usize,
    ) -> Result<LiftedLasso, Spurious> {
        let (mut states, mut actions, tail) = match &self.cycle {
            Some(cycle) => reduced.lift_cycle(&self.start, &self.prefix, cycle)?,
            None => {
                let (first, steps) = reduced.lifted(&self.start, &self.prefix);
                let actions = steps.iter().map(|s| s.0).collect();
                let states = std::iter::once(first)
                    .chain(steps.into_iter().map(|s| s.2))
                    .collect();
                (states, actions, Loop::Stutter)
            }
        };
        if !self.liveness {
            // A class move lifts to several steps: the path ends at the first state that
            // breaks the `always`, as the cone's own run would end it.
            let p = &cone.properties()[cone_i];
            let (_, at) = cone
                .property_failure(p, &states, states.len() - 1)
                .ok_or(Spurious::Fails)?;
            states.truncate(at + 1);
            actions.truncate(at);
        }
        let replay = cone
            .replay_lasso(states.clone(), &actions, tail)
            .map_err(|_| Spurious::Fails)?;
        let mut diag = cone
            .explain_property(cone_i, &replay)
            .ok_or(Spurious::Fails)?;
        match self.liveness {
            true => {
                if cone.unfair_loop(cone_i, &replay, Fairness::Plain).is_some() {
                    return Err(Spurious::Fails);
                }
            }
            // A finite prefix breaks an `always`, and every prefix extends to a fair
            // behavior: it reads no fairness, and prints no ending, as `decide_property`.
            false => {
                if let StateDiag::PropertyViolated { lasso, .. } = &mut diag {
                    *lasso = None;
                }
            }
        }
        Ok(LiftedLasso {
            states,
            actions,
            tail,
            diag: Box::new(diag),
        })
    }
}
