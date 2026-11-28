//! Run-time certificates of a counterexample found on a cone (claims A2 and A16 of
//! docs/theory/claims.md). Every build checks them, and only on a counterexample or on the
//! exact cone a recheck runs, so no explored state pays for them:
//!
//! - `free_trigger_faults`: the free triggers `annotate` lists (`Trace::free_triggers`) are
//!   those read off each step's action directly: the raised marks its guards read
//!   (`pred_reads`) that the cone leaves out. It reads nothing of `mark_reads`, which the cone
//!   and the list are built from. On a mismatch the trace lists both, so it is a candidate
//!   only and its check is decided again (`recheck_on`).
//! - `certified_exact`: the exact cone reads every mark and, with `slots`, counts every slot.
//!   One that does not is replaced by the whole system, which is exact.
//! - `unvouched_fault`: the counterexample of the exact cone is one it vouches for
//!   (`Trace::vouched`). One that is not is a tool error for its check (`Recheck::Fault`).
//! - `is_unreduced_initial`: a lifted start is an initial state of the unreduced cone
//!   (`lift`); one that is not is `ReplayError::NotInitial`.
//!
//! A fault is a tool bug: a debug or `shadow` build panics with a message that starts
//! `shadow:`, unless a test planted it (`dropped`).

use std::collections::BTreeSet;

use super::cone::{Cone, Target};
use super::diag::{Diag, StateDiag};
use super::explore::symmetry::dropped;
use super::explore::LassoFault;
use super::expr::{State, StateSlice};
use super::trace::{ReplayError, Trace};
use super::types::{Birth, Start, Value, VarKind};
use super::TransitionSystem;
use crate::lifecycle::narrow::id8;

impl TransitionSystem {
    /// The `(use case, event)` of each step whose action has a guard reading a raised mark
    /// outside `cone`, from the action's guards alone; `Err` names a step whose action this
    /// system does not have.
    fn free_triggers_of_steps(
        &self,
        cone: &Cone,
        trace: &Trace,
    ) -> Result<BTreeSet<(String, String)>, String> {
        let mut out = BTreeSet::new();
        for step in &trace.steps {
            let action = (self.actions.iter())
                .find(|a| a.name == step.action)
                .ok_or_else(|| format!("step {} runs no action of the system", step.action))?;
            let mut reads = Vec::new();
            for g in action.guards.iter().filter(|g| g.copy_target().is_none()) {
                self.pred_reads(&g.pred, &mut reads);
            }
            for var in reads.into_iter().filter(|v| !cone.vars.contains(v)) {
                if let VarKind::Raised { event, .. } = &self.var(var).kind {
                    let usecase = (action.name)
                        .strip_suffix(&format!("_by_{event}"))
                        .unwrap_or(&action.name);
                    out.insert((usecase.to_string(), event.clone()));
                }
            }
        }
        Ok(out)
    }

    /// Holds `trace.free_triggers`, as `annotate` set it on `cone`, to the free triggers read
    /// off the steps' actions (`free_triggers_of_steps`). On a mismatch, adds what the list
    /// lacks, so the trace is vouched for only if both find no free trigger, and returns the
    /// fault. A step whose action the system lacks adds `(action, "")`, which no event
    /// matches: the trace is never vouched for.
    pub(super) fn free_trigger_faults(&self, cone: &Cone, trace: &mut Trace) -> Option<String> {
        let listed: BTreeSet<(String, String)> = trace.free_triggers.iter().cloned().collect();
        let (derived, why) = match self.free_triggers_of_steps(cone, trace) {
            Ok(derived) if derived == listed => return None,
            Ok(derived) => (derived, String::new()),
            Err(why) => {
                let unknown = (trace.steps.iter())
                    .filter(|s| !self.actions.iter().any(|a| a.name == s.action))
                    .map(|s| (s.action.clone(), String::new()));
                (listed.iter().cloned().chain(unknown).collect(), why)
            }
        };
        let fault = format!(
            "the free triggers listed {:?} are not those the steps' guards read {:?}{}",
            listed,
            derived,
            if why.is_empty() {
                why
            } else {
                format!(" ({why})")
            }
        );
        for pair in derived {
            if !trace.free_triggers.contains(&pair) {
                trace.free_triggers.push(pair);
            }
        }
        Some(fault)
    }

    /// `free_trigger_faults`, reported: a debug or `shadow` build stops unless a test planted
    /// the fault (`dropped("free_triggers")`); every build warns.
    pub(super) fn certify_free_triggers(&self, cone: &Cone, trace: &mut Trace) {
        if let Some(fault) = self.free_trigger_faults(cone, trace) {
            crate::shadow_assert!(dropped("free_triggers"), "shadow: annotate: {fault}");
            eprintln!(
                "warning: a counterexample's free triggers failed their check ({fault}); deciding its check again (a tool bug)"
            );
        }
    }

    /// `cone`, the exact cone of `target` as `exact_target_cone` built it, if it reads every
    /// mark (`cone_leaves_marks`) and with `slots` counts every slot (`cone_frees_slots`);
    /// otherwise the whole system, which does both. A test plants a cone that fails
    /// (`dropped("exact_target_cone")`): the target's own cone stands for it.
    pub(super) fn certified_exact(&self, target: Target, slots: bool, cone: Cone) -> Cone {
        if dropped("exact_target_cone_unchecked") {
            return self.planted_exact(target);
        }
        let cone = match dropped("exact_target_cone") {
            true => self.planted_exact(target),
            false => cone,
        };
        let leaves = self.cone_leaves_marks(&cone);
        let frees = slots && self.cone_frees_slots(&cone);
        if !leaves && !frees {
            return cone;
        }
        let fault = match (leaves, frees) {
            (true, true) => "leaves a raised mark out and frees slots",
            (true, false) => "leaves a raised mark out",
            _ => "frees slots",
        };
        crate::shadow_assert!(
            dropped("exact_target_cone"),
            "shadow: exact_target_cone: the exact cone of {target:?} {fault}"
        );
        eprintln!(
            "warning: the exact cone of {target:?} {fault}; deciding it on the whole system (a tool bug)"
        );
        Cone {
            vars: self.var_ids().collect(),
            keep: self.action_ids().collect(),
        }
    }

    /// The planted exact cone of `target`: its own cone, with no writer kept.
    fn planted_exact(&self, target: Target) -> Cone {
        let (seed, keep) = self.target_seed(target);
        Cone {
            vars: self.cone(&seed, &keep),
            keep,
        }
    }

    /// The tool error of a counterexample `d` of `target` that its exact cone does not vouch
    /// for with `slots`: a debug or `shadow` build stops unless a test planted an exact cone
    /// that is not (`dropped("exact_target_cone_unchecked")`).
    pub(super) fn unvouched_fault(&self, target: Target, d: &Diag, slots: bool) -> Diag {
        let trace = d.diag.trace().expect("only a trace is vouched for");
        let why = format!(
            "free triggers {:?}{}",
            trace.free_triggers,
            match slots {
                true => format!(", past the bound {:?}", trace.over_bound),
                false => String::new(),
            }
        );
        crate::shadow_assert!(
            dropped("exact_target_cone_unchecked"),
            "shadow: recheck_on: the exact cone of {target:?} does not vouch for its counterexample ({why})"
        );
        Diag {
            entities: d.entities.clone(),
            diag: StateDiag::TraceFault {
                name: d.diag.name(&[]),
                fault: LassoFault::Unvouched(why),
            },
        }
    }

    /// Whether `s` is an initial state of this system without its reduction: each var at its
    /// base, except that each slot of a pre-existing entity is present, an identity column
    /// holds its slot's value, and a column or owner that starts anywhere holds some value of
    /// its domain. A test, not an enumeration: it holds no state but `s`.
    pub(crate) fn is_unreduced_initial(&self, s: &StateSlice) -> bool {
        let mut want = self.base_state();
        if want.0.len() != s.0.len() {
            return false;
        }
        for decl in &self.entities {
            if decl.birth != Birth::PreExisting {
                continue;
            }
            for slot in 0..usize::from(decl.slots.get()) {
                want.set(self, decl.exists, slot, Value::PRESENT);
                for &var in decl.columns.iter().chain(&decl.owners) {
                    let d = self.var(var);
                    match d.start {
                        Start::Base => {}
                        Start::Identity => want.set(self, var, slot, d.domain.of_slot(id8(slot))),
                        Start::Any => {
                            let v = s.get(self, var, slot);
                            if usize::from(v.0) >= d.domain.size(self) {
                                return false;
                            }
                            want.set(self, var, slot, v);
                        }
                    }
                }
            }
        }
        want.0[..] == s.0[..]
    }
}

/// Holds `start`, the start `lift` built on reduced system `ts`, to the unreduced cone's
/// initial states (`is_unreduced_initial`): one that is not is `ReplayError::NotInitial`, and a
/// debug or `shadow` build stops unless a test planted it (`dropped("lifted_start")`: the first
/// entity's first slot flips between present and absent, which no initial state allows).
pub(crate) fn lifted_start(ts: &TransitionSystem, start: &mut State) -> Result<(), ReplayError> {
    if dropped("lifted_start") {
        if let Some(e) = ts.entities.first() {
            let flipped = match start.get(ts, e.exists, 0) {
                Value::PRESENT => Value::ABSENT,
                _ => Value::PRESENT,
            };
            start.set(ts, e.exists, 0, flipped);
        }
    }
    if ts.is_unreduced_initial(start) {
        return Ok(());
    }
    crate::shadow_assert!(
        dropped("lifted_start"),
        "shadow: lift: the lifted start is no initial state of the unreduced cone"
    );
    Err(ReplayError::NotInitial)
}
