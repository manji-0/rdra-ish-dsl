//! The state rules the safety pass refuted, for `--prove` to leave out (#421).

use super::bfs::BfsCone;
use super::jobs::{Job, JobKind};
use rdra_ish_core::lifecycle::{StateDiag, TransitionSystem};
use std::collections::BTreeMap;

/// Each invariant of `ts` (by index into `ts.invariants()`) that a safety cone of `jobs`
/// violates, with the instances per entity its cone ran at, the fewest when several cones
/// violate it. A cone's invariant is the whole system's of the same name and site, and BFS
/// reaches only states the whole system reaches, so its violation is a real one.
pub(super) fn refuted(
    ts: &TransitionSystem,
    jobs: &[Job],
    safety: &[Option<BfsCone>],
) -> BTreeMap<usize, usize> {
    let mut out = BTreeMap::new();
    for (job, cone) in jobs.iter().zip(safety) {
        let (JobKind::Cone(system), Some(BfsCone::Ran { found, .. })) = (&job.kind, cone) else {
            continue;
        };
        let at = system.system.most_slots();
        for d in found {
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
