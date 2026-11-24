//! One exploration per equal cone (#209, #296 item 1.2). The entity, safety and property
//! passes of `verify` each reduce their cones on their own, and different targets can come out
//! as one system: patent-ops' `view OfficeAction` and `Cone_Forbidden_OfficeAction_3`, or
//! insurance-claim-ops' `Cone_Forbidden_Claim_4` and `Cone_ClaimEventuallyDecided`. The pass
//! that runs such a system first runs it with what every later pass reads (every check, no
//! early stop), decides the later passes' jobs on that run while it holds it, and the later
//! passes take those decisions instead of exploring the system again.
//!
//! Two jobs share a run only when their systems are `==` (`TransitionSystem`'s every field:
//! vars, actions, checks, properties, bounds and reduction), and only a complete run is
//! shared, so a job decided on a shared run is decided as on a run of its own. What changes is
//! the state count a failing safety cone prints: the whole shared run's, where a run of its own
//! would have stopped once its checks had all failed (#213).

use super::bfs::{safety_on, BfsCone};
use super::jobs::{Job, JobKind};
use super::native::Plain;
use rdra_ish_core::lifecycle::{Checks, ComponentRun, Diag, TransitionSystem};

/// What an earlier pass decided of a later pass's jobs on a run they share, per job index.
pub(super) struct Shared {
    /// A safety cone's verdict, and the vacuous checks of it and of the cones it covers.
    pub(super) safety: Vec<Option<(BfsCone, Vec<Diag>)>>,
    /// What a property's plain run says.
    pub(super) plain: Vec<Option<Plain>>,
}

impl Shared {
    pub(super) fn new(jobs: &[Job]) -> Self {
        Shared {
            safety: jobs.iter().map(|_| None).collect(),
            plain: jobs.iter().map(|_| None).collect(),
        }
    }

    /// The systems of the safety and property cones among `jobs`: an entity cone of one of
    /// them is run with every check, so that its run can decide them.
    pub(super) fn wanted(jobs: &[Job]) -> Vec<&TransitionSystem> {
        jobs.iter()
            .filter_map(|job| match &job.kind {
                JobKind::Cone(cone) => Some(&cone.system),
                JobKind::PropertyCone(p) => Some(&p.cone.reduced),
                JobKind::Explored(_) | JobKind::Whole(_) => None,
            })
            .collect()
    }

    /// Decides on `run` each job of `jobs` on `run`'s system that is not decided yet: a safety
    /// cone with the vacuous checks of the cones `covers` says it covers, and a property's
    /// plain run. A run that is not complete, or that did not evaluate every check, decides
    /// nothing here.
    pub(super) fn decide_on(
        &mut self,
        ts: &TransitionSystem,
        jobs: &[Job],
        covers: &[Option<usize>],
        run: &ComponentRun,
    ) {
        if !run.complete || run.checks() != &Checks::All {
            return;
        }
        for (k, job) in jobs.iter().enumerate() {
            match &job.kind {
                JobKind::Cone(cone) if self.safety[k].is_none() && cone.system == run.system => {
                    self.safety[k] = Some(safety_on(ts, jobs, covers, k, run));
                }
                JobKind::PropertyCone(p)
                    if self.plain[k].is_none() && p.cone.reduced == run.system =>
                {
                    self.plain[k] = Some(Plain::of(ts, p, run));
                }
                _ => {}
            }
        }
    }
}
