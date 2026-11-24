//! The reference twin of `each_planned` (#253, #275): the enabled bindings as their
//! definition states them, every binding `bindings` lists whose pre-state guards hold, each
//! guard read by `eval`. A build with debug assertions or the `shadow` feature holds every
//! `each_enabled` call to it. `bindings` and `pre_guards_hold` also serve the paths that
//! have no fast form: `each_planned` past 16 slots or params, and `scan`.

use crate::lifecycle::expr::{Env, StateSlice};
use crate::lifecycle::types::{Action, ParamRole, Value};
use crate::lifecycle::TransitionSystem;

impl TransitionSystem {
    /// Slot choices for each param: the lowest absent slot for `New`, every present slot
    /// otherwise.
    pub(in crate::lifecycle::explore) fn bindings(
        &self,
        action: &Action,
        state: &StateSlice,
    ) -> Vec<Vec<u8>> {
        self.bindings_for(action, state, false)
    }

    /// `bindings`, except that with `crowd` a `New` param with no free slot takes every slot,
    /// as if the bound left one free.
    pub(in crate::lifecycle::explore) fn bindings_for(
        &self,
        action: &Action,
        state: &StateSlice,
        crowd: bool,
    ) -> Vec<Vec<u8>> {
        let mut out: Vec<Vec<u8>> = vec![Vec::new()];
        for p in &action.params {
            let exists = self.entity(p.entity).exists;
            let n = self.slots(p.entity);
            let slots: Vec<u8> = match p.role {
                ParamRole::New => {
                    match (0..n).find(|&s| state.get(self, exists, s) == Value::ABSENT) {
                        Some(s) => vec![s as u8],
                        None if crowd => (0..n as u8).collect(),
                        None => Vec::new(),
                    }
                }
                ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Triggered => (0..n)
                    .filter(|&s| state.get(self, exists, s) == Value::PRESENT)
                    .map(|s| s as u8)
                    .collect(),
                // Matched by its run mark, a guard, whether or not the slot is present.
                ParamRole::Earlier { .. } => (0..n as u8).collect(),
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

    /// Whether every pre-state guard of `action` holds under `params`, each read by `eval`.
    pub(in crate::lifecycle::explore) fn pre_guards_hold(
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
}

/// The bindings of `action` in `state` whose pre-guards hold, in `bindings` order.
#[cfg(any(debug_assertions, feature = "shadow"))]
pub(super) fn enabled_bindings(
    ts: &TransitionSystem,
    action: &Action,
    state: &StateSlice,
) -> Vec<Box<[u8]>> {
    ts.bindings(action, state)
        .into_iter()
        .filter(|params| ts.pre_guards_hold(action, state, params))
        .map(Vec::into_boxed_slice)
        .collect()
}
