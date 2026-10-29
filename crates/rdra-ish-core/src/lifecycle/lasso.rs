//! A temporal counterexample from another engine: a lasso replayed through `moves`, and the
//! property it violates, evaluated on the infinite behavior the lasso stands for with the
//! semantics the TLA printer gives it.

use super::diag::StateDiag;
use super::expr::{Env, State};
use super::trace::{Replay, ReplayError};
use super::types::{ActId, Ltl, Pred, Property};
use super::TransitionSystem;

/// How a lasso goes on after the last state of its finite part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loop {
    /// The last state repeats forever.
    Stutter,
    /// One more step takes the last state back to state `to` (from 1, as TLC numbers its
    /// states), and the behavior repeats from there.
    Back { to: usize },
}

/// A lasso matched step by step against `moves`, its back edge included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LassoReplay {
    prefix: Replay,
    back: Option<BackEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BackEdge {
    to: usize,
    act: ActId,
    params: Box<[u8]>,
}

impl LassoReplay {
    /// The position the behavior returns to after the last state: the last state itself for
    /// a stutter.
    fn entry(&self) -> usize {
        match &self.back {
            Some(b) => b.to - 1,
            None => self.prefix.states.len() - 1,
        }
    }

    fn tail(&self) -> Loop {
        match &self.back {
            Some(b) => Loop::Back { to: b.to },
            None => Loop::Stutter,
        }
    }
}

impl TransitionSystem {
    /// `replay` on the finite part, then the back edge: with `Loop::Back`, `actions` holds one
    /// more action than there are steps, the one that takes the last state back to state
    /// `to`. The edge takes the first matching move, as `replay` does.
    pub fn replay_lasso(
        &self,
        states: Vec<State>,
        actions: &[ActId],
        tail: Loop,
    ) -> Result<LassoReplay, ReplayError> {
        let (prefix, back) = match tail {
            Loop::Stutter => (actions, None),
            Loop::Back { to } => {
                let (&act, rest) = actions.split_last().ok_or(ReplayError::Length)?;
                (rest, Some((to, act)))
            }
        };
        let prefix = self.replay(states, prefix)?;
        let back = match back {
            None => None,
            Some((to, act)) => {
                let last = prefix.states.last().expect("replay has a state");
                let params = to
                    .checked_sub(1)
                    .and_then(|k| prefix.states.get(k))
                    .and_then(|target| {
                        self.moves(last)
                            .find(|m| m.act == act && m.next == *target)
                            .map(|m| m.params)
                    })
                    .ok_or(ReplayError::NoBackMove { to, action: act })?;
                Some(BackEdge { to, act, params })
            }
        };
        Ok(LassoReplay { prefix, back })
    }

    /// The violation of `properties()[i]` on the replayed lasso, its back edge rendered as the
    /// trace's last step; `None` when the lasso satisfies the property.
    pub fn explain_property(&self, i: usize, replay: &LassoReplay) -> Option<StateDiag> {
        let p = &self.properties[i];
        let states = &replay.prefix.states;
        let (instance, witness) = self.property_failure(p, states, replay.entry())?;
        let mut trace = self.replay_trace(&replay.prefix);
        if let Some(b) = &replay.back {
            let last = states.last().expect("replay has a state");
            trace
                .steps
                .push(self.trace_step(b.act, &b.params, last, &states[b.to - 1]));
        }
        Some(StateDiag::PropertyViolated {
            name: p.name.clone(),
            kind: p.formula.kind(),
            instance: p
                .binders
                .iter()
                .zip(&instance)
                .map(|(e, slot)| self.inst(*e, usize::from(*slot)))
                .collect(),
            witness,
            lasso: replay.tail(),
            site: p.site.clone(),
            trace,
        })
    }

    /// The first binding of `p`'s binders (every slot, present or not, in index order, as
    /// the printed `\A b \in Ids` ranges) that fails on `states` followed by `states[entry..]`
    /// forever, with the state that shows it.
    fn property_failure(
        &self,
        p: &Property,
        states: &[State],
        entry: usize,
    ) -> Option<(Vec<u8>, usize)> {
        let slots = self.bounds.slots();
        let mut binding = vec![0u8; p.binders.len()];
        loop {
            if let Some(at) = self.fails_at(p, &binding, states, entry) {
                return Some((binding, at));
            }
            let mut k = binding.len();
            loop {
                k = k.checked_sub(1)?;
                binding[k] += 1;
                if usize::from(binding[k]) < slots {
                    break;
                }
                binding[k] = 0;
            }
        }
    }

    /// The first state where `p` fails for `binding`, mirroring the printed formula:
    /// `always(x)` is `[](present => x)`, `eventually(x)` is `[](present => <>x)`, and
    /// `leads_to(x, y)` is `(present /\ x) ~> y`. Only the trigger reads `present`.
    fn fails_at(
        &self,
        p: &Property,
        binding: &[u8],
        states: &[State],
        entry: usize,
    ) -> Option<usize> {
        let holds = |pred: &Pred, s: &State| {
            let mut env = Env::new(&[]);
            env.bound = binding.to_vec();
            self.eval(pred, s, &mut env)
        };
        let present = |s: &State| {
            p.binders
                .iter()
                .zip(binding)
                .all(|(e, slot)| s.is_present(self, *e, usize::from(*slot)))
        };
        // From position i the behavior visits the rest of the finite part, then the loop.
        let never_from =
            |i: usize, goal: &Pred| !states[i.min(entry)..].iter().any(|s| holds(goal, s));
        states.iter().enumerate().find_map(|(i, s)| {
            let fails = present(s)
                && match &p.formula {
                    Ltl::Always(x) => !holds(x, s),
                    Ltl::Eventually(x) => never_from(i, x),
                    Ltl::LeadsTo(x, y) => holds(x, s) && never_from(i, y),
                };
            fails.then_some(i)
        })
    }
}
