//! Initial states, bindings, successors, and the moves from a state.

use std::borrow::Cow;
use std::collections::HashSet;

use crate::lifecycle::expr::{Env, State};
use crate::lifecycle::types::{ActId, Action, Effect, EntId, ParamRole, Rhs, Start, Value, VarId};
use crate::lifecycle::TransitionSystem;

use super::Move;

impl TransitionSystem {
    pub(crate) fn pattern_key(&self, state: &State, e: EntId, slot: usize) -> Box<[u8]> {
        self.entity(e)
            .columns
            .iter()
            .map(|v| state.get(self, *v, slot).0)
            .collect()
    }

    pub(crate) fn initial_states(&self) -> Vec<State> {
        let mut base = self.base_state();
        let mut free: Vec<(VarId, usize, usize)> = Vec::new();
        for decl in &self.entities {
            if decl.birth == crate::lifecycle::types::Birth::PreExisting {
                for slot in 0..self.bounds.slots() {
                    base.set(self, decl.exists, slot, Value::PRESENT);
                    for &var in decl.columns.iter().chain(&decl.owners) {
                        let d = self.var(var);
                        match d.start {
                            Start::Base => {}
                            Start::Any => free.push((var, slot, d.domain.size(self.bounds))),
                            Start::Identity => {
                                base.set(self, var, slot, d.domain.of_slot(slot as u8));
                            }
                        }
                    }
                }
            }
        }
        let mut out = vec![base];
        for (var, slot, size) in free {
            out = out
                .into_iter()
                .flat_map(|s| {
                    (0..size).map(move |v| {
                        let mut s = s.clone();
                        s.set(self, var, slot, Value(v as u8));
                        s
                    })
                })
                .collect();
        }
        // A reduced system starts each S part at its class's representative.
        if let Some(r) = self.reduction() {
            let mut seen = HashSet::new();
            out = out
                .into_iter()
                .map(|s| {
                    r.canonical_initial(&s)
                        .expect("an initial S state has a class")
                })
                .filter(|s| seen.insert(s.clone()))
                .collect();
        }
        out
    }

    /// Slot choices for each param: the lowest absent slot for `New`, every present slot
    /// otherwise.
    pub(super) fn bindings(&self, action: &Action, state: &State) -> Vec<Vec<u8>> {
        self.bindings_for(action, state, false)
    }

    /// `bindings`, except that with `crowd` a `New` param with no free slot takes every slot,
    /// as if the bound left one free.
    pub(super) fn bindings_for(&self, action: &Action, state: &State, crowd: bool) -> Vec<Vec<u8>> {
        let mut out: Vec<Vec<u8>> = vec![Vec::new()];
        for p in &action.params {
            let exists = self.entity(p.entity).exists;
            let slots: Vec<u8> = match p.role {
                ParamRole::New => match (0..self.bounds.slots())
                    .find(|&s| state.get(self, exists, s) == Value::ABSENT)
                {
                    Some(s) => vec![s as u8],
                    None if crowd => (0..self.bounds.slots() as u8).collect(),
                    None => Vec::new(),
                },
                ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Triggered => {
                    (0..self.bounds.slots())
                        .filter(|&s| state.get(self, exists, s) == Value::PRESENT)
                        .map(|s| s as u8)
                        .collect()
                }
                // Matched by its run mark, a guard, whether or not the slot is present.
                ParamRole::Earlier { .. } => (0..self.bounds.slots() as u8).collect(),
            };
            out = out
                .into_iter()
                .flat_map(|prefix| {
                    slots.iter().map(move |s| {
                        let mut b = prefix.clone();
                        b.push(*s);
                        b
                    })
                })
                .collect();
            if out.is_empty() {
                break;
            }
        }
        out
    }

    pub(super) fn successors(&self, action: &Action, params: &[u8], s: &State) -> Vec<State> {
        let slot = |p: u8| usize::from(params[usize::from(p)]);
        let mut next = s.clone();
        let mut deleted: Vec<u8> = Vec::new();
        let mut choices: Vec<(VarId, u8, Cow<'_, [Value]>)> = Vec::new();
        for eff in &action.effects {
            match eff {
                Effect::Create { param } => {
                    let e = action.params[usize::from(*param)].entity;
                    next.set(self, self.entity(e).exists, slot(*param), Value::PRESENT);
                }
                Effect::Delete { param } => deleted.push(*param),
                Effect::Assign {
                    var, param, rhs, ..
                } => {
                    let v = match rhs {
                        Rhs::Lit(v) => *v,
                        Rhs::Param(q) => self.var(*var).domain.of_slot(params[usize::from(*q)]),
                    };
                    next.set(self, *var, slot(*param), v);
                }
                Effect::Transition {
                    var, param, arms, ..
                } => {
                    let from = s.get(self, *var, slot(*param));
                    let tos: Vec<Value> = arms
                        .iter()
                        .filter(|(f, _)| *f == from)
                        .map(|(_, t)| *t)
                        .collect();
                    match tos[..] {
                        [] => {}
                        [to] => next.set(self, *var, slot(*param), to),
                        // A choice forks after every other effect, so each arm carries them.
                        _ => choices.push((*var, *param, Cow::Owned(tos))),
                    }
                }
                Effect::Choose {
                    var, param, values, ..
                } => choices.push((*var, *param, Cow::Borrowed(values))),
                Effect::Tick => {
                    let now = self.now.expect("tick without a clock");
                    let v = s.get(self, now, 0);
                    next.set(self, now, 0, Value(v.0 + 1));
                }
            }
        }
        for &p in &deleted {
            let e = action.params[usize::from(p)].entity;
            let decl = self.entity(e);
            for &var in decl.columns.iter().chain(decl.owners.iter()) {
                next.set(self, var, slot(p), self.var(var).base);
            }
            next.set(self, decl.exists, slot(p), Value::DELETED);
        }
        choices.retain(|(_, p, _)| !deleted.contains(p));
        let mut out = vec![next];
        for (var, p, values) in choices {
            let sl = slot(p);
            out = out
                .into_iter()
                .flat_map(|st| {
                    values.iter().map(move |v| {
                        let mut st = st.clone();
                        st.set(self, var, sl, *v);
                        st
                    })
                })
                .collect();
        }
        out
    }

    /// Every move from `s`, in BFS order: actions by index, then `bindings`, then the
    /// successors of each binding whose pre-guards hold, keeping those whose post-guards hold.
    /// With a reduction, the S actions are left out and the class moves come last.
    pub(crate) fn moves<'s>(&'s self, s: &'s State) -> impl Iterator<Item = Move> + 's {
        self.moves_of(s, true)
    }

    /// `moves` without the reduction.
    pub(crate) fn raw_moves<'s>(&'s self, s: &'s State) -> impl Iterator<Item = Move> + 's {
        self.actions
            .iter()
            .enumerate()
            .flat_map(move |(ai, action)| {
                self.bindings(action, s)
                    .into_iter()
                    .filter(move |params| {
                        let mut env = Env::new(params);
                        action
                            .guards
                            .iter()
                            .filter(|g| !g.reads_successor())
                            .all(|g| self.eval(&g.pred, s, &mut env))
                    })
                    .flat_map(move |params| {
                        let params: Box<[u8]> = params.into();
                        self.successors(action, &params, s)
                            .into_iter()
                            .filter(|next| {
                                let mut env = Env {
                                    next: Some(next),
                                    ..Env::new(&params)
                                };
                                action
                                    .guards
                                    .iter()
                                    .filter(|g| g.reads_successor())
                                    .all(|g| self.eval(&g.pred, s, &mut env))
                            })
                            .map(|next| Move {
                                act: ActId::at(ai),
                                params: params.clone(),
                                next,
                            })
                            .collect::<Vec<_>>()
                    })
            })
    }
}
