//! The state rules the safety pass refuted, for `--prove` to leave out (#421).

use super::bfs::BfsCone;
use super::jobs::{Job, JobKind};
use crate::state_findings::Instances;
use rdra_ish_core::lifecycle::{Diag, ParamRole, StateDiag, TransitionSystem};
use std::collections::BTreeMap;

/// Each invariant of `ts` (by index into `ts.invariants()`) that a safety cone of `jobs`
/// violates, with the instances per entity its counterexample takes (`instances`), the one
/// whose largest count is least when several cones violate it. A cone's invariant is the whole system's of the same name and site, and a
/// violation counts only when it is one of the whole system's (`BfsCone::violations`): a
/// cone that leaves a raised mark out reaches states the whole system does not, so its
/// violation counts only once the check's exact cone found it again (`recheck_cones`).
pub(super) fn refuted(
    ts: &TransitionSystem,
    jobs: &[Job],
    safety: &[Option<BfsCone>],
) -> BTreeMap<usize, Instances> {
    let mut out = BTreeMap::new();
    for (job, cone) in jobs.iter().zip(safety) {
        let (JobKind::Cone(system), Some(cone)) = (&job.kind, cone) else {
            continue;
        };
        let at = Instances::of(&system.system);
        for d in cone.violations() {
            let at = instances(ts, &at, d);
            let StateDiag::ConstraintViolated { name, site, .. } = &d.diag else {
                continue;
            };
            let same = |i: &usize| {
                let inv = &ts.invariants()[*i];
                inv.name == *name && inv.site == *site
            };
            for i in (0..ts.invariants().len()).filter(same) {
                let n = out.entry(i).or_insert_with(|| at.clone());
                if at.most() < n.most() {
                    *n = at.clone();
                }
            }
        }
    }
    out
}

/// The instances `d`'s counterexample takes: `at`, the slots of each entity its cone counts,
/// and for an entity the cone does not count (`Trace::over_bound`) the rows the trace creates,
/// each a creation of the whole system's actions (claim A25 of docs/theory/claims.md).
fn instances(ts: &TransitionSystem, at: &Instances, d: &Diag) -> Instances {
    let mut out = at.clone();
    let Some(trace) = d.diag.trace() else {
        return out;
    };
    let created = |entity: &str| -> usize {
        (trace.steps.iter())
            .filter_map(|s| ts.actions().iter().find(|a| a.name == s.action))
            .flat_map(|a| a.params.iter())
            .filter(|p| p.role == ParamRole::New && ts.entity(p.entity).id == entity)
            .count()
    };
    for e in &trace.over_bound {
        out.at_least(e, created(e));
    }
    out
}
