//! A temporal counterexample, from another engine or from the liveness search: a lasso
//! replayed through `moves`, the property it violates, evaluated on the infinite behavior the
//! lasso stands for with the semantics the TLA printer gives it, and the fairness of its loop.

use crate::lifecycle::narrow::id8;
use std::collections::BTreeSet;

use super::diag::StateDiag;
use super::expr::{Env, State, StateSlice};
use super::trace::{Replay, ReplayError};
use super::types::{ActId, EntId, Ltl, Pred, Property};
use super::TransitionSystem;

/// The fairness a liveness run assumes beside the property's own `Fair_P`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fairness {
    /// `Fair_P` alone: a behavior may stop at a state where no fair instance is enabled.
    Plain,
    /// `Fair_P` and `WF_vars(Next)`: a behavior stops only at a state with no step to
    /// another state.
    WfNext,
}

/// Why a lasso's loop is not a behavior the fairness admits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unfair {
    /// The fair instance of `act` on `slots` (`Property::fair_instance`) is enabled at every
    /// state of the loop, and the loop never takes its step.
    Starved { act: ActId, slots: Box<[u8]> },
    /// Under `WF_vars(Next)`, the behavior stops at a state with a step to another state.
    Stops,
}

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
            lasso: (!p.formula.kind().is_explored()).then(|| replay.tail()),
            site: p.site.clone(),
            trace,
        })
    }

    /// Why the loop of `replay` is not fair for `properties()[i]` under `fairness`; `None`
    /// when it is. Weak fairness of an instance holds on the loop when the loop takes its
    /// step or passes a state where it is disabled. A step of an instance from `s` to `t` is
    /// a move of it with `t` as its successor; it is enabled at `s` when some move of it
    /// leaves `s`. A move that changes no var is neither. Reads `moves` at the lasso's states
    /// and nothing the search that built the lasso computed.
    pub fn unfair_loop(
        &self,
        i: usize,
        replay: &LassoReplay,
        fairness: Fairness,
    ) -> Option<Unfair> {
        let p = &self.properties[i];
        let states = &replay.prefix.states;
        let cycle = &states[replay.entry()..];
        let last = states.last().expect("replay has a state");
        // The loop's steps, the one back to its first state included; a stutter has none.
        let steps: Vec<(&State, &State)> = match replay.back {
            None => Vec::new(),
            Some(_) => cycle
                .windows(2)
                .map(|w| (&w[0], &w[1]))
                .chain(std::iter::once((last, &cycle[0])))
                .filter(|(a, b)| a != b)
                .collect(),
        };
        let fair_moves =
            |s: &StateSlice, to: Option<&StateSlice>| -> BTreeSet<(ActId, Box<[u8]>)> {
                self.moves(s)
                    .filter(|m| m.next != *s && to.is_none_or(|t| m.next == *t))
                    .filter_map(|m| {
                        let slots = p.fair_instance(&self.actions[m.act.index()], &m.params)?;
                        Some((m.act, slots))
                    })
                    .collect()
            };
        let mut starved = fair_moves(&cycle[0], None);
        for s in &cycle[1..] {
            let enabled = fair_moves(s, None);
            starved.retain(|a| enabled.contains(a));
        }
        for (a, b) in &steps {
            for taken in fair_moves(a, Some(b)) {
                starved.remove(&taken);
            }
        }
        if let Some((act, slots)) = starved.into_iter().next() {
            return Some(Unfair::Starved { act, slots });
        }
        let stops = steps.is_empty() && self.moves(last).any(|m| m.next != *last);
        (fairness == Fairness::WfNext && stops).then_some(Unfair::Stops)
    }

    /// The first binding of `p`'s binders (every slot, present or not, in index order, as
    /// the printed `\A b \in Ids` ranges) that fails on `states` followed by `states[entry..]`
    /// forever, with the state that shows it.
    pub(crate) fn property_failure(
        &self,
        p: &Property,
        states: &[State],
        entry: usize,
    ) -> Option<(Vec<u8>, usize)> {
        self.binder_slots(&p.binders)
            .find_map(|b| Some((b.clone(), self.fails_at(p, &b, states, entry)?)))
    }

    /// Every binding of `binders`, each over every slot of its entity, the last binder
    /// fastest.
    pub(crate) fn binder_slots(&self, binders: &[EntId]) -> impl Iterator<Item = Vec<u8>> {
        let radix: Vec<usize> = binders.iter().map(|&e| self.slots(e)).collect();
        (0..radix.iter().product::<usize>()).map(move |mut k| {
            let mut b = vec![0u8; radix.len()];
            for (slot, n) in b.iter_mut().zip(&radix).rev() {
                *slot = id8(k % n);
                k /= n;
            }
            b
        })
    }

    /// `pred` with the property's binders at `binding`.
    pub(crate) fn holds_bound(&self, pred: &Pred, s: &StateSlice, binding: &[u8]) -> bool {
        let mut env = Env::new(&[]);
        env.bound = binding.to_vec();
        self.eval(pred, s, &mut env)
    }

    /// The trigger of every form: the bound slots are present and `.along` links them.
    pub(crate) fn triggers(&self, p: &Property, binding: &[u8], s: &StateSlice) -> bool {
        self.present(p, binding, s) && self.holds_bound(&p.link, s, binding)
    }

    fn present(&self, p: &Property, binding: &[u8], s: &StateSlice) -> bool {
        p.binders
            .iter()
            .zip(binding)
            .all(|(e, slot)| s.is_present(self, *e, usize::from(*slot)))
    }

    /// The step from `a` to `b` breaks `stable` or `unchanged` for `binding`: the trigger
    /// and `p` (`when`) hold before it, the bound slots are present after it, and `p` fails
    /// (a column moves). The link is read before the step only, so a step that moves a key
    /// keeps the obligation the trigger took on. False for every other form.
    pub(crate) fn step_breaks(
        &self,
        p: &Property,
        binding: &[u8],
        a: &StateSlice,
        b: &StateSlice,
    ) -> bool {
        let breaks = match &p.formula {
            Ltl::Stable(x) => self.holds_bound(x, a, binding) && !self.holds_bound(x, b, binding),
            Ltl::Unchanged { when, columns } => {
                self.holds_bound(when, a, binding)
                    && columns
                        .iter()
                        .any(|c| !self.term_unchanged(c, a, b, binding))
            }
            _ => return false,
        };
        breaks && self.triggers(p, binding, a) && self.present(p, binding, b)
    }

    /// The first state where `p` fails for `binding`, mirroring the printed formula:
    /// `always(x)` is `[](present => x)`, `eventually(x)` is `[](present => <>x)`, and
    /// `leads_to(x, y)` is `(present /\ x) ~> y`. Only the trigger reads `present` (a
    /// `.present()` goal carries it in its own predicate, from the build), and the
    /// `.along` link joins it. `stable` and `unchanged` fail at the state a step enters, the
    /// step back to `entry` included; `precedes` fails where `q` holds before `p` has, as its
    /// latch reads it.
    fn fails_at(
        &self,
        p: &Property,
        binding: &[u8],
        states: &[State],
        entry: usize,
    ) -> Option<usize> {
        let holds = |pred: &Pred, s: &StateSlice| self.holds_bound(pred, s, binding);
        let trigger = |s: &StateSlice| self.triggers(p, binding, s);
        // From position i the behavior visits the rest of the finite part, then the loop.
        let never_from =
            |i: usize, goal: &Pred| !states[i.min(entry)..].iter().any(|s| holds(goal, s));
        let at = |fails: &dyn Fn(usize) -> bool| (0..states.len()).find(|&i| fails(i));
        match &p.formula {
            Ltl::Always(x) => at(&|i| trigger(&states[i]) && !holds(x, &states[i])),
            Ltl::Eventually(x) => at(&|i| trigger(&states[i]) && never_from(i, x)),
            Ltl::LeadsTo(x, y) => {
                at(&|i| trigger(&states[i]) && holds(x, &states[i]) && never_from(i, y))
            }
            Ltl::Stable(_) | Ltl::Unchanged { .. } => {
                let last = states.len() - 1;
                (0..last)
                    .map(|i| (i, i + 1))
                    .chain(std::iter::once((last, entry)))
                    .find(|&(i, j)| self.step_breaks(p, binding, &states[i], &states[j]))
                    .map(|(i, _)| i + 1)
            }
            Ltl::Precedes(x, y) => {
                let mut seen = false;
                states.iter().position(|s| {
                    let on = trigger(s);
                    seen |= on && holds(x, s);
                    on && holds(y, s) && !seen
                })
            }
        }
    }
}
