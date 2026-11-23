//! Initial states, bindings, successors, and the moves from a state.

use std::collections::HashSet;

use crate::lifecycle::expr::{Env, State, StateSlice};
use crate::lifecycle::types::{ActId, Action, Effect, EntId, ParamRole, Rhs, Start, Value, VarId};
use crate::lifecycle::TransitionSystem;

use super::{Move, Moves};

impl TransitionSystem {
    pub(crate) fn pattern_key(&self, state: &StateSlice, e: EntId, slot: usize) -> Box<[u8]> {
        self.entity(e)
            .columns
            .iter()
            .map(|v| state.get(self, *v, slot).0)
            .collect()
    }

    /// `pattern_key` written into `key`, which keeps its allocation.
    pub(crate) fn pattern_key_into(
        &self,
        state: &StateSlice,
        e: EntId,
        slot: usize,
        key: &mut Vec<u8>,
    ) {
        key.clear();
        key.extend(
            self.entity(e)
                .columns
                .iter()
                .map(|v| state.get(self, *v, slot).0),
        );
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
                    r.canonical_initial(self, &s)
                        .expect("an initial S state has a class")
                })
                .filter(|s| seen.insert(s.clone()))
                .collect();
        }
        out
    }

    /// Slot choices for each param: the lowest absent slot for `New`, every present slot
    /// otherwise.
    pub(super) fn bindings(&self, action: &Action, state: &StateSlice) -> Vec<Vec<u8>> {
        self.bindings_for(action, state, false)
    }

    /// `bindings`, except that with `crowd` a `New` param with no free slot takes every slot,
    /// as if the bound left one free.
    pub(super) fn bindings_for(
        &self,
        action: &Action,
        state: &StateSlice,
        crowd: bool,
    ) -> Vec<Vec<u8>> {
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

    /// The `bindings` of `action` in `state` whose pre-guards hold, in `bindings` order.
    #[cfg(test)]
    pub(super) fn enabled_bindings(&self, action: &Action, state: &StateSlice) -> Vec<Box<[u8]>> {
        let mut out = Vec::new();
        self.each_enabled(action, state, |params| out.push(params.into()));
        out
    }

    /// Calls `found` with each of the `bindings` of `action` in `state` whose pre-guards hold,
    /// in `bindings` order. It walks the choices in buffers on the stack, so a state where
    /// the action cannot run costs no allocation.
    fn each_enabled(&self, action: &Action, state: &StateSlice, mut found: impl FnMut(&[u8])) {
        const MAX: usize = 16;
        let slots = self.bounds.slots();
        let count = action.params.len();
        if slots > MAX || count > MAX {
            for params in self.bindings(action, state) {
                if self.pre_guards_hold(action, state, &params) {
                    found(&params);
                }
            }
            return;
        }
        // Per param: how many candidate slots it has, and the slots, lowest first.
        let mut choices = [(0usize, [0u8; MAX]); MAX];
        for (p, (n, list)) in action.params.iter().zip(&mut choices) {
            let exists = self.entity(p.entity).exists;
            match p.role {
                ParamRole::New => {
                    if let Some(s) =
                        (0..slots).find(|&s| state.get(self, exists, s) == Value::ABSENT)
                    {
                        list[0] = s as u8;
                        *n = 1;
                    }
                }
                ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Triggered => {
                    for s in 0..slots {
                        if state.get(self, exists, s) == Value::PRESENT {
                            list[*n] = s as u8;
                            *n += 1;
                        }
                    }
                }
                ParamRole::Earlier { .. } => {
                    for (s, cell) in list.iter_mut().enumerate().take(slots) {
                        *cell = s as u8;
                    }
                    *n = slots;
                }
            }
            if *n == 0 {
                return;
            }
        }
        let choices = &choices[..count];
        let mut at = [0usize; MAX];
        let mut params = [0u8; MAX];
        for (param, (_, list)) in params.iter_mut().zip(choices) {
            *param = list[0];
        }
        loop {
            if self.pre_guards_hold(action, state, &params[..count]) {
                found(&params[..count]);
            }
            // The last param varies fastest, as `bindings` lists them.
            let mut i = count;
            loop {
                if i == 0 {
                    return;
                }
                i -= 1;
                at[i] += 1;
                if at[i] < choices[i].0 {
                    params[i] = choices[i].1[at[i]];
                    break;
                }
                at[i] = 0;
                params[i] = choices[i].1[0];
            }
        }
    }

    pub(super) fn pre_guards_hold(
        &self,
        action: &Action,
        state: &StateSlice,
        params: &[u8],
    ) -> bool {
        let mut env = Env::new(params);
        action
            .guards
            .iter()
            .filter(|g| !g.reads_successor())
            .all(|g| self.eval(&g.pred, state, &mut env))
    }

    pub(super) fn successors(&self, action: &Action, params: &[u8], s: &StateSlice) -> Vec<State> {
        let mut out = Vec::new();
        let mut scratch = s.to_owned();
        self.each_successor(action, params, s, &mut scratch, |next| {
            out.push(next.clone())
        });
        out
    }

    /// Calls `found` with each successor of `s` under `action` bound to `params`, before the
    /// post-guards: one, or one per value of each choice the effects leave open. `found` may
    /// change the successor it is given. The one successor of a step without a choice is
    /// built in `scratch`, a state of the same system.
    fn each_successor(
        &self,
        action: &Action,
        params: &[u8],
        s: &StateSlice,
        scratch: &mut State,
        mut found: impl FnMut(&mut State),
    ) {
        let slot = |p: u8| usize::from(params[usize::from(p)]);
        scratch.0.copy_from_slice(&s.0);
        let next = scratch;
        let mut deleted: Vec<u8> = Vec::new();
        let mut choices: Vec<(VarId, u8, Vec<Value>)> = Vec::new();
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
                    let mut tos = arms.iter().filter(|arm| arm.from == from).map(|arm| arm.to);
                    match (tos.next(), tos.next()) {
                        (None, _) => {}
                        (Some(to), None) => next.set(self, *var, slot(*param), to),
                        // A choice forks after every other effect, so each arm carries them.
                        (Some(first), Some(second)) => {
                            let tos = [first, second].into_iter().chain(tos).collect();
                            choices.push((*var, *param, tos));
                        }
                    }
                }
                Effect::Choose {
                    var, param, values, ..
                } => choices.push((*var, *param, values.iter().map(|c| c.value).collect())),
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
        if choices.is_empty() {
            return found(next);
        }
        let mut out = vec![next.clone()];
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
        out.iter_mut().for_each(found);
    }

    /// Every move from `s`, in BFS order: actions by index, then `bindings`, then the
    /// successors of each binding whose pre-guards hold, keeping those whose post-guards hold.
    /// With a reduction, the S actions are left out and the class moves come last.
    pub(crate) fn moves(&self, s: &StateSlice) -> impl Iterator<Item = Move> {
        let mut out = Moves::default();
        self.moves_into(s, &mut s.to_owned(), &mut out);
        out.into_moves().into_iter()
    }

    /// `moves`, appended to `out`. `scratch` is any state of this system; it is overwritten.
    pub(crate) fn moves_into(&self, s: &StateSlice, scratch: &mut State, out: &mut Moves) {
        self.moves_of_into(s, true, scratch, out);
    }

    /// Calls `found` with each move from `s` of an action `skip` does not name, without the
    /// reduction: its action, its params, and its successor, which `found` may change.
    pub(crate) fn each_raw_move(
        &self,
        s: &StateSlice,
        skip: impl Fn(ActId) -> bool,
        scratch: &mut State,
        mut found: impl FnMut(ActId, &[u8], &mut State),
    ) {
        for (ai, action) in self.actions.iter().enumerate() {
            let act = ActId::at(ai);
            if skip(act) {
                continue;
            }
            self.each_enabled(action, s, |params| {
                self.each_successor(action, params, s, scratch, |next| {
                    let holds = {
                        let mut env = Env {
                            next: Some(next),
                            ..Env::new(params)
                        };
                        action
                            .guards
                            .iter()
                            .filter(|g| g.reads_successor())
                            .all(|g| self.eval(&g.pred, s, &mut env))
                    };
                    if holds {
                        found(act, params, next);
                    }
                });
            });
        }
    }
}
