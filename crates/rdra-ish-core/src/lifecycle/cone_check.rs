//! The run-time certificate of a cone's choice (residual risk 1 of docs/theory/README.md,
//! claims A1 and A2): every cone a verdict reads is checked against what Theorems A1 and A2
//! of docs/theory/cones.md need of it, not against minimality. The check reads the cone's
//! vars and kept actions and each action's reads and writes (`plain_reads`, `mark_reads`,
//! `copy_reads`, `action_writes`, `pred_reads`), and nothing of `cone`'s worklist or
//! `keep_writers`' rounds.
//!
//! Two checks, at two places:
//!
//! - `cone_faults`, where a cone is chosen (`target_cone`, `property_cone`,
//!   `exact_target_cone`): the seed is in the cone, and the cone is closed (Definition 1,
//!   (C1)–(C4)); an exact cone also leaves no mark out and, when it counts slots, frees
//!   none (Definition 2). A fault replaces the cone by the whole system's vars with the same
//!   kept actions, which is closed and exact, so the run decides on the whole system.
//! - `restriction_faults`, in every `restrict_to`: the restricted system keeps exactly the
//!   actions that write the cone or are kept, each with exactly its effects on the cone, and
//!   drops only what Definition 2 lists: a copy guard whose target is outside the cone, a
//!   mark's guards and `Triggered` param when the mark is outside it (a left-out latch, which
//!   `cone_leaves_marks` then reports), and a `New` param with its `Create` when its entity's
//!   existence is outside it (a freed slot, which `cone_frees_slots` then reports). Every
//!   other guard and param stays, so every kept guard reads only the cone's vars (closure
//!   makes `restrict_to`'s own test `reads ⊆ vars` keep it). A cone has no stub entity. A
//!   fault is a tool error: the run stops, and no verdict prints.
//!
//! What dropping each listed item does: a dropped guard or param only adds steps, so the
//! restricted system still reaches a superset of the projection (Lemma 2, Theorem A2). A
//! freed slot drops the `Create` and the lowest-absent-slot rule of its `New` param, so the
//! entity never runs out of slots in the cone: a superset again, and a counterexample that
//! creates more rows than the bound is marked (`over_bound`, Proposition A28). No guard of
//! the cone's actions reads the freed entity's row but a copy into it, which goes with its
//! target: a plain guard that read it would read a var of that entity, which (C1) puts in
//! the cone and (C4) brings the existence with.

use std::collections::BTreeSet;
use std::fmt;

use super::cone::{Cone, VarSet};
use super::explore::symmetry::dropped;
use super::restrict::Remap;
use super::types::{ActId, Action, Effect, GuardWhy, ParamRole};
use super::TransitionSystem;
use crate::lifecycle::narrow::id8;

/// The condition a cone or its restriction fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ConeClause {
    /// A seed var is outside the cone.
    Seed,
    /// (C1): an action of the cone reads a var outside it (`plain_reads`).
    Reads,
    /// (C2): a mark the cone reads exactly is outside it, or what it brings is.
    Mark,
    /// (C3): a copy guard's target is in the cone and what it reads is not.
    Copy,
    /// (C4): a var's existence is outside the cone.
    Existence,
    /// An exact cone leaves a mark out.
    LeftOutMark,
    /// An exact cone that counts slots frees one.
    FreedSlot,
    /// The restriction drops an action that writes the cone or is kept, or keeps another.
    Writer,
    /// The restriction drops an effect on the cone, or keeps one off it.
    Effect,
    /// The restriction drops a guard Definition 2 does not list, or keeps one it lists.
    Guard,
    /// The restriction drops a param Definition 2 does not list, or keeps one it lists.
    Param,
    /// The restriction holds a var or an entity the cone does not (a stub).
    Stub,
}

/// One fault of a cone: its clause and what breaks it.
#[derive(Debug)]
pub(crate) struct ConeFault {
    pub(crate) clause: ConeClause,
    what: String,
}

impl fmt::Display for ConeFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let clause = match self.clause {
            ConeClause::Seed => "a seed var outside the cone",
            ConeClause::Reads => "closure (C1)",
            ConeClause::Mark => "closure (C2)",
            ConeClause::Copy => "closure (C3)",
            ConeClause::Existence => "closure (C4)",
            ConeClause::LeftOutMark => "an exact cone leaves a mark out",
            ConeClause::FreedSlot => "an exact cone frees a slot",
            ConeClause::Writer => "the restriction's actions",
            ConeClause::Effect => "the restriction's effects",
            ConeClause::Guard => "the restriction's guards",
            ConeClause::Param => "the restriction's params",
            ConeClause::Stub => "the restriction's vars",
        };
        write!(f, "{clause}: {}", self.what)
    }
}

/// What a cone's use needs beyond closure: no left-out mark (an entity view, a `can_reach`,
/// an exact cone), and no freed slot (a `can_reach`, an exact cone that counts slots).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Exactness {
    pub(crate) marks: bool,
    pub(crate) slots: bool,
}

impl Exactness {
    pub(crate) const NONE: Exactness = Exactness {
        marks: false,
        slots: false,
    };
}

impl TransitionSystem {
    /// `A_W`: per action, whether it is kept or writes a var of `vars`.
    fn cone_members(&self, vars: &VarSet, keep: &BTreeSet<ActId>) -> Vec<bool> {
        (self.actions.iter().enumerate())
            .map(|(i, a)| {
                keep.contains(&ActId::at(i))
                    || self.action_writes(a).iter().any(|v| vars.contains(v))
            })
            .collect()
    }

    /// Every way `vars` with `keep` fails Definition 1 of docs/theory/cones.md; empty when it
    /// is closed. Not whether it is the least closed set: any closed set gives Lemma 2.
    pub(crate) fn closure_faults(&self, vars: &VarSet, keep: &BTreeSet<ActId>) -> Vec<ConeFault> {
        let mut out = Vec::new();
        let mut fault = |clause, what: String| out.push(ConeFault { clause, what });
        let outside = |set: &VarSet| set.iter().copied().find(|v| !vars.contains(v));
        let member = self.cone_members(vars, keep);
        for (i, a) in self.actions.iter().enumerate() {
            if !member[i] {
                continue;
            }
            if let Some(v) = outside(&self.plain_reads(a)) {
                let var = self.var_name(v);
                fault(ConeClause::Reads, format!("{} reads {var}", a.name));
            }
            for (mark, reads) in self.mark_reads(a) {
                let missing = match vars.contains(&mark) {
                    false => Some(mark),
                    true => outside(&reads),
                };
                // Read exactly, the mark brings what it reads (C2); read inexactly, it may
                // stay out with them.
                let exact = || {
                    keep.contains(&ActId::at(i))
                        || self.is_counted(mark)
                        || (self.actions.iter().enumerate())
                            .filter(|(_, b)| self.action_writes(b).contains(&mark))
                            .all(|(j, _)| member[j])
                };
                if let Some(v) = missing.filter(|_| exact()) {
                    let (mark, var) = (self.var_name(mark), self.var_name(v));
                    fault(
                        ConeClause::Mark,
                        format!("{} reads the mark {mark} exactly, without {var}", a.name),
                    );
                }
            }
        }
        for a in &self.actions {
            for (target, reads) in self.copy_reads(a) {
                if !vars.contains(&target) {
                    continue;
                }
                if let Some(v) = outside(&reads) {
                    let (target, var) = (self.var_name(target), self.var_name(v));
                    fault(
                        ConeClause::Copy,
                        format!("{}'s copy into {target} reads {var}", a.name),
                    );
                }
            }
        }
        for &v in vars {
            if let Some(e) = self.existences(v).into_iter().find(|e| !vars.contains(e)) {
                let (var, e) = (self.var_name(v), self.var_name(e));
                fault(ConeClause::Existence, format!("{var} without {e}"));
            }
        }
        out
    }

    /// Every fault of `cone` as the cone of `seed` that a use needing `need` reads: the
    /// seed is in it, it is closed, and it has none of the deviations `need` excludes.
    pub(crate) fn cone_faults(
        &self,
        seed: &VarSet,
        cone: &Cone,
        need: Exactness,
    ) -> Vec<ConeFault> {
        let vars = &cone.vars;
        let mut out = Vec::new();
        if let Some(v) = seed.iter().find(|v| !vars.contains(v)) {
            out.push(ConeFault {
                clause: ConeClause::Seed,
                what: self.var_name(*v),
            });
        }
        out.extend(self.closure_faults(vars, &cone.keep));
        let member = self.cone_members(vars, &cone.keep);
        for (a, _) in (self.actions.iter().zip(member)).filter(|(_, m)| *m) {
            if need.marks {
                if let Some((mark, _)) =
                    (self.mark_reads(a).into_iter()).find(|(m, _)| !vars.contains(m))
                {
                    out.push(ConeFault {
                        clause: ConeClause::LeftOutMark,
                        what: format!("{} reads {}", a.name, self.var_name(mark)),
                    });
                }
            }
            if need.slots {
                let freed = (a.params.iter())
                    .filter(|p| p.role == ParamRole::New)
                    .find(|p| !vars.contains(&self.entity(p.entity).exists));
                if let Some(p) = freed {
                    out.push(ConeFault {
                        clause: ConeClause::FreedSlot,
                        what: format!("{} creates {}", a.name, self.entity(p.entity).id),
                    });
                }
            }
        }
        out
    }

    /// `cone` when it passes `cone_faults`; otherwise, with a warning, the cone of every var
    /// with the same kept actions, which is closed and exact. A debug or `shadow` build stops
    /// at a fault unless a test planted it (`dropped("cone_choice")`).
    pub(crate) fn certified(&self, seed: &VarSet, cone: Cone, need: Exactness) -> Cone {
        let Some(fault) = self.cone_faults(seed, &cone, need).into_iter().next() else {
            return cone;
        };
        crate::shadow_assert!(dropped("cone_choice"), "shadow: cone: {fault}");
        eprintln!(
            "warning: a cone failed its check ({fault}); deciding on the whole system instead (a tool bug)"
        );
        Cone {
            vars: self.var_ids().collect(),
            keep: cone.keep,
        }
    }

    /// Every way `out`, the restriction of this system to `vars` with `keep` (`map` its
    /// renumbering), keeps or drops other than the module comment lists.
    pub(crate) fn restriction_faults(
        &self,
        vars: &VarSet,
        keep: &BTreeSet<ActId>,
        out: &TransitionSystem,
        map: &Remap,
    ) -> Vec<ConeFault> {
        let mut faults = Vec::new();
        let mut fault = |clause, what: String| faults.push(ConeFault { clause, what });
        let entities = (self.entities.iter())
            .filter(|e| vars.contains(&e.exists))
            .count();
        if out.vars.len() != vars.len() || out.entities.len() != entities {
            fault(
                ConeClause::Stub,
                format!(
                    "{} vars and {} entities for {} and {entities}",
                    out.vars.len(),
                    out.entities.len(),
                    vars.len()
                ),
            );
        }
        let member = self.cone_members(vars, keep);
        for (i, a) in self.actions.iter().enumerate() {
            let Some(j) = map.act[i] else {
                if member[i] {
                    fault(ConeClause::Writer, format!("{} is dropped", a.name));
                }
                continue;
            };
            if !member[i] {
                fault(ConeClause::Writer, format!("{} is kept", a.name));
                continue;
            }
            let b = &out.actions[j.index()];
            if b.source != a.source {
                fault(
                    ConeClause::Writer,
                    format!("{} is kept as {}", a.name, b.name),
                );
            }
            let on_cone = |e: &Effect| self.effect_writes(a, e).iter().any(|v| vars.contains(v));
            if b.effects.len() != a.effects.iter().filter(|e| on_cone(e)).count() {
                fault(
                    ConeClause::Effect,
                    format!("{} keeps {} effects", a.name, b.effects.len()),
                );
            }
            let drops = self.listed_drops(a, vars);
            let index = map.params[i].as_deref();
            let kept_param = |p: u8| index.is_none_or(|ix| ix[usize::from(p)].is_some());
            for (p, param) in a.params.iter().enumerate() {
                if kept_param(id8(p)) == drops.params.contains(&id8(p)) {
                    let how = if kept_param(id8(p)) { "keeps" } else { "drops" };
                    fault(
                        ConeClause::Param,
                        format!("{} {how} param {p} ({:?})", a.name, param.role),
                    );
                }
            }
            let kept = (a.guards.iter().enumerate())
                .filter(|(g, _)| !drops.guards.contains(g))
                .count();
            if b.guards.len() != kept {
                fault(
                    ConeClause::Guard,
                    format!(
                        "{} keeps {} of {} guards, {kept} expected",
                        a.name,
                        b.guards.len(),
                        a.guards.len()
                    ),
                );
            }
        }
        faults
    }

    /// The guards (by index) and params of `a` that Definition 2 lets a restriction to
    /// `vars` drop.
    fn listed_drops(&self, a: &Action, vars: &VarSet) -> Drops {
        let left_out: Vec<(u8, _)> = (self.triggered_params(a).into_iter())
            .filter(|(_, mark)| !vars.contains(mark))
            .collect();
        let mut params: Vec<u8> = left_out.iter().map(|(p, _)| *p).collect();
        params.extend(
            (a.params.iter().enumerate())
                .filter(|(_, p)| {
                    p.role == ParamRole::New && !vars.contains(&self.entity(p.entity).exists)
                })
                .map(|(p, _)| id8(p)),
        );
        let guards = (a.guards.iter().enumerate())
            .filter(|(_, g)| {
                if let Some(target) = g.copy_target() {
                    return !vars.contains(&target);
                }
                let raised_out = matches!(g.why, GuardWhy::Raised { .. }) && {
                    let mut reads = VarSet::new();
                    self.pred_reads(&g.pred, &mut reads);
                    !reads.is_subset(vars)
                };
                let names = super::cone::named_params(&g.pred);
                raised_out || left_out.iter().any(|(p, _)| names.contains(p))
            })
            .map(|(g, _)| g)
            .collect();
        Drops { guards, params }
    }

    /// `restriction_faults` for `restrict_to`: a fault stops the run as a tool error.
    pub(crate) fn certify_restriction(
        &self,
        vars: &VarSet,
        keep: &BTreeSet<ActId>,
        out: &TransitionSystem,
        map: &Remap,
    ) {
        if let Some(fault) = self.restriction_faults(vars, keep, out, map).first() {
            crate::shadow_assert!(false, "shadow: restrict_to: {fault}");
            panic!("tool error: restrict_to: {fault} (a tool bug, not a finding about the model)");
        }
    }
}

/// What Definition 2 lets a restriction drop of one action.
struct Drops {
    guards: Vec<usize>,
    params: Vec<u8>,
}
