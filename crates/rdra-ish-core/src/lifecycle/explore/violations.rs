//! The diagnostics for an invariant or an after-assert that a state or step violates.

use crate::lifecycle::cone::VarSet;
use crate::lifecycle::diag::{Diag, StateDiag};
use crate::lifecycle::expr::State;
use crate::lifecycle::trace::Trace;
use crate::lifecycle::types::{ActId, EntId, InvKind};
use crate::lifecycle::view::describe_slot;
use crate::lifecycle::TransitionSystem;

impl TransitionSystem {
    /// The violation of `invariants()[i]` in `s`, with an empty trace; `None` when it holds.
    /// BFS and the replay of a TLC trace both build the diagnostic here.
    pub(crate) fn invariant_violation(&self, i: usize, s: &State) -> Option<Diag> {
        let inv = &self.invariants[i];
        let binding = self.find_violation(&inv.binders, &inv.body, s, &[])?;
        let local = matches!(
            inv.kind,
            InvKind::Forbidden
                | InvKind::Invariant
                | InvKind::Required
                | InvKind::Exclusive
                | InvKind::Check
        );
        let pattern_desc = if local {
            describe_slot(self, s, inv.binders[0], usize::from(binding[0]), false)
        } else {
            inv.binders
                .iter()
                .zip(&binding)
                .map(|(e, slot)| describe_slot(self, s, *e, usize::from(*slot), true))
                .collect::<Vec<_>>()
                .join("; ")
        };
        Some(Diag {
            entities: inv.binders.iter().map(|e| self.entity(*e).key).collect(),
            diag: StateDiag::ConstraintViolated {
                name: inv.name.clone(),
                kind: inv.kind,
                text: inv.text.clone(),
                label: inv.label.clone(),
                pattern_desc,
                site: inv.site.clone(),
                trace: Trace::default(),
            },
        })
    }

    /// The violation of `after_asserts()[i]` by the step `act` with `params` into `s2`, with
    /// an empty trace; `None` when the step does not trigger it or it holds.
    pub(crate) fn after_violation(
        &self,
        i: usize,
        act: ActId,
        params: &[u8],
        s2: &State,
    ) -> Option<Diag> {
        let a = &self.after[i];
        let action = &self.actions[act.index()];
        a.steps
            .iter()
            .filter(|step| step.act == act)
            .find_map(|step| {
                let binding = self.find_violation(&step.binders, &step.post, s2, params)?;
                let mut reads = VarSet::new();
                self.pred_reads(&step.post, &mut reads);
                let read: Vec<EntId> = reads.iter().filter_map(|v| self.var_entity(*v)).collect();
                let mut parts: Vec<String> = Vec::new();
                let mut entities = Vec::new();
                for (p, param) in action.params.iter().enumerate() {
                    if !param.role.is_written() && !read.contains(&param.entity) {
                        continue;
                    }
                    parts.push(describe_slot(
                        self,
                        s2,
                        param.entity,
                        usize::from(params[p]),
                        true,
                    ));
                    entities.push(self.entity(param.entity).key);
                }
                for (e, slot) in step.binders.iter().zip(&binding) {
                    parts.push(describe_slot(self, s2, *e, usize::from(*slot), true));
                    entities.push(self.entity(*e).key);
                }
                Some(Diag {
                    entities,
                    diag: StateDiag::AfterAssertViolated {
                        name: a.name.clone(),
                        anchor: a.anchor.clone(),
                        requireds: a.requireds.clone(),
                        label: a.label.clone(),
                        actual: parts.join("; "),
                        site: a.site.clone(),
                        trace: Trace::default(),
                    },
                })
            })
    }
}
