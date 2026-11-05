//! Scans a set of states for the guard checks that hold in none of them.

use crate::lifecycle::expr::{Env, State};
use crate::lifecycle::types::{ActId, Action, ParamRole, Value};
use crate::lifecycle::TransitionSystem;

use super::Scan;

impl TransitionSystem {
    /// Which of `act`'s checks in `moves` hold in no state of `states`. With `pin = (p, k)`,
    /// only the bindings that put param `p` on slot `k` count.
    pub(crate) fn scan_guards<'a>(
        &self,
        act: ActId,
        states: impl IntoIterator<Item = &'a State>,
        pin: Option<(usize, u8)>,
    ) -> Scan {
        let action = &self.actions[act.index()];
        let mut bound = vec![false; action.params.len()];
        let mut held = vec![false; action.guards.len()];
        let mut any_binding = false;
        let mut crowded = vec![false; action.params.len()];
        let creates = action.params.iter().any(|p| p.role == ParamRole::New);
        for s in states {
            for (p, param) in action.params.iter().enumerate() {
                let exists = self.entity(param.entity).exists;
                let want = match param.role {
                    ParamRole::New => Value::ABSENT,
                    ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Triggered => {
                        Value::PRESENT
                    }
                    // Any slot binds it; its run guard says whether a run reached it.
                    ParamRole::Earlier { .. } => {
                        bound[p] = true;
                        continue;
                    }
                };
                let mut slots = match pin {
                    Some((q, k)) if q == p => usize::from(k)..usize::from(k) + 1,
                    _ => 0..self.bounds.slots(),
                };
                bound[p] |= slots.any(|slot| s.get(self, exists, slot) == want);
            }
            let pinned = |params: &Vec<u8>| pin.is_none_or(|(p, k)| params[p] == k);
            for params in self.bindings(action, s).into_iter().filter(pinned) {
                any_binding = true;
                for (g, guard) in action.guards.iter().enumerate() {
                    if held[g] {
                        continue;
                    }
                    held[g] = if guard.reads_successor() {
                        self.successors(action, &params, s).iter().any(|next| {
                            let mut env = Env {
                                next: Some(next),
                                ..Env::new(&params)
                            };
                            self.eval(&guard.pred, s, &mut env)
                        })
                    } else {
                        self.eval(&guard.pred, s, &mut Env::new(&params))
                    };
                }
            }
            if creates {
                for p in self.crowded_in(action, s, pin) {
                    crowded[p] = true;
                }
            }
            // A creation keeps scanning: a later state may show the bound as what blocks it.
            if !creates && any_binding && held.iter().all(|b| *b) {
                return Scan::EachHeld;
            }
        }
        let unbound: Vec<usize> = (0..bound.len()).filter(|p| !bound[*p]).collect();
        if !unbound.is_empty() {
            return Scan::Unbound(unbound);
        }
        if !any_binding {
            return Scan::Apart;
        }
        let crowded: Vec<usize> = (0..crowded.len()).filter(|p| crowded[*p]).collect();
        if !crowded.is_empty() {
            // The crowded states read no post-guard, so one that never held under a real
            // binding still blocks; the pre-guards held there and are not the blocker.
            let post_never: Vec<usize> = (0..held.len())
                .filter(|g| !held[*g] && action.guards[*g].reads_successor())
                .collect();
            return if post_never.is_empty() {
                Scan::Crowded(crowded)
            } else {
                Scan::NeverHeld(post_never)
            };
        }
        let never: Vec<usize> = (0..held.len()).filter(|g| !held[*g]).collect();
        if never.is_empty() {
            Scan::EachHeld
        } else {
            Scan::NeverHeld(never)
        }
    }

    /// The `New` params of `action` that find no free slot in `s` while every other param
    /// finds one and every pre-guard holds under some binding that gives them any slot; empty
    /// when `s` has no such binding. Post-guards are not read: a creation on a taken slot has
    /// no successor to read them on.
    fn crowded_in(&self, action: &Action, s: &State, pin: Option<(usize, u8)>) -> Vec<usize> {
        let free = |p: &crate::lifecycle::types::Param| {
            let exists = self.entity(p.entity).exists;
            (0..self.bounds.slots()).any(|slot| s.get(self, exists, slot) == Value::ABSENT)
        };
        let missing: Vec<usize> = (0..action.params.len())
            .filter(|&p| action.params[p].role == ParamRole::New && !free(&action.params[p]))
            .collect();
        if missing.is_empty() {
            return missing;
        }
        let pinned = |params: &Vec<u8>| pin.is_none_or(|(p, k)| params[p] == k);
        let holds = self
            .bindings_for(action, s, true)
            .into_iter()
            .filter(pinned)
            .any(|params| {
                let mut env = Env::new(&params);
                action
                    .guards
                    .iter()
                    .filter(|g| !g.reads_successor())
                    .all(|g| self.eval(&g.pred, s, &mut env))
            });
        if holds {
            missing
        } else {
            Vec::new()
        }
    }
}
