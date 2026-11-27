//! The state rules the safety pass refuted, for `--prove` to leave out (#421).

use super::bfs::BfsCone;
use super::jobs::{Job, JobKind};
use rdra_ish_core::lifecycle::{Diag, ParamRole, StateDiag, TransitionSystem};
use std::collections::BTreeMap;

/// Each invariant of `ts` (by index into `ts.invariants()`) that a safety cone of `jobs`
/// violates, with the instances per entity its counterexample takes (`instances`), the fewest
/// when several cones violate it. A cone's invariant is the whole system's of the same name and site, and a
/// violation counts only when it is one of the whole system's (`BfsCone::violations`): a
/// cone that leaves a raised mark out reaches states the whole system does not, so its
/// violation counts only once the check's exact cone found it again (`recheck_cones`).
pub(super) fn refuted(
    ts: &TransitionSystem,
    jobs: &[Job],
    safety: &[Option<BfsCone>],
) -> BTreeMap<usize, usize> {
    let mut out = BTreeMap::new();
    for (job, cone) in jobs.iter().zip(safety) {
        let (JobKind::Cone(system), Some(cone)) = (&job.kind, cone) else {
            continue;
        };
        let at = system.system.most_slots();
        for d in cone.violations() {
            let at = instances(ts, at, d);
            let StateDiag::ConstraintViolated { name, site, .. } = &d.diag else {
                continue;
            };
            let same = |i: &usize| {
                let inv = &ts.invariants()[*i];
                inv.name == *name && inv.site == *site
            };
            for i in (0..ts.invariants().len()).filter(same) {
                let n = out.entry(i).or_insert(at);
                *n = (*n).min(at);
            }
        }
    }
    out
}

/// The instances `d`'s counterexample takes: `at`, the most slots of an entity its cone
/// counts, or more when it creates more rows of an entity the cone does not count
/// (`Trace::over_bound`), each a creation of the whole system's actions (P11).
fn instances(ts: &TransitionSystem, at: usize, d: &Diag) -> usize {
    let Some(trace) = d.diag.trace() else {
        return at;
    };
    let created = |entity: &str| -> usize {
        (trace.steps.iter())
            .filter_map(|s| ts.actions().iter().find(|a| a.name == s.action))
            .flat_map(|a| a.params.iter())
            .filter(|p| p.role == ParamRole::New && ts.entity(p.entity).id == entity)
            .count()
    };
    (trace.over_bound.iter())
        .map(|e| created(e))
        .fold(at, usize::max)
}
