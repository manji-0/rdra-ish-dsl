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
//!
//! Under `--symmetry always` the entity pass runs and shares as under `never`, and a job that
//! may run up to instance symmetry (#212) does so only when no plain run decided it: no
//! entity cone's complete run, and, for a safety cone, no property's plain run of the same
//! system (`up_to_symmetry`). An `eventually` or `leads_to` whose reduced cone is a safety
//! cone's system does not run up to symmetry, so that one plain run decides both (`new`).
//! Which runs are complete is what they are alone, since the entity
//! pass runs a crowded cone again alone, so the choice does not depend on `RDRA_JOBS`. A run
//! up to symmetry decides no other job.
//!
//! The test-only `RDRA_ISH_FORCE_ORBITS` runs every job that may run up to symmetry so,
//! sharing nothing, as `RDRA_ISH_NO_REDUCE` turns the reduction off: the samples, whose cones
//! the entity pass mostly decides, then reach the run up to symmetry, its twins and
//! engine-agreement rule 8.

use super::bfs::{safety_on, BfsCone};
use super::jobs::{Job, JobKind};
use super::native::Plain;
use rdra_ish_core::lifecycle::{Checks, ComponentRun, Diag, Symmetry, TransitionSystem};

/// What an earlier pass decided of a later pass's jobs on a run they share, per job index.
pub(super) struct Shared {
    /// A safety cone's verdict, and the vacuous checks of it and of the cones it covers.
    pub(super) safety: Vec<Option<(BfsCone, Vec<Diag>)>>,
    /// What a property's plain run says.
    pub(super) plain: Vec<Option<Plain>>,
    /// The jobs that may run up to instance symmetry (`--symmetry always`).
    symmetric: Vec<bool>,
    /// `RDRA_ISH_FORCE_ORBITS`: each of them does, and shares nothing.
    forced: bool,
}

impl Shared {
    pub(super) fn new(jobs: &[Job], symmetry: Symmetry) -> Self {
        let forced = std::env::var_os("RDRA_ISH_FORCE_ORBITS").is_some();
        // An `eventually` or `leads_to` whose reduced cone is a safety cone's system stays
        // plain, so that one plain run decides both, as before phase 4 (#212): the two would
        // otherwise run up to symmetry apart. Which ones is read off the jobs before any run.
        let shares_safety = |p: &super::jobs::PropertyJob| {
            jobs.iter()
                .any(|j| matches!(&j.kind, JobKind::Cone(c) if c.system == p.cone.reduced))
        };
        let symmetric: Vec<bool> = (jobs.iter())
            .map(|job| {
                symmetry == Symmetry::Always
                    && job.symmetric()
                    && match &job.kind {
                        JobKind::PropertyCone(p) if p.cone.kind.is_liveness() && !forced => {
                            !shares_safety(p)
                        }
                        _ => true,
                    }
            })
            .collect();
        Shared {
            safety: jobs.iter().map(|_| None).collect(),
            plain: jobs.iter().map(|_| None).collect(),
            symmetric,
            forced,
        }
    }

    /// Whether `jobs[k]` runs up to symmetry whatever the other passes decide.
    fn forced(&self, k: usize) -> bool {
        self.forced && self.symmetric[k]
    }

    /// Whether `jobs[k]` runs up to instance symmetry, read once the entity pass is done: it
    /// may (`--symmetry always`), and no plain run decides it. A safety cone is decided by the
    /// entity pass's run of its system, or runs plainly to give a property not yet decided on
    /// that system its plain answer (`bfs_cones`); a property is decided by an earlier pass.
    /// Its own run then decides no other job.
    pub(super) fn up_to_symmetry(&self, jobs: &[Job], k: usize) -> bool {
        if !self.symmetric[k] || self.forced(k) {
            return self.forced(k);
        }
        match &jobs[k].kind {
            JobKind::Cone(cone) => {
                self.safety[k].is_none()
                    && !(jobs.iter().enumerate()).any(|(j, job)| match &job.kind {
                        JobKind::PropertyCone(p) => {
                            !self.symmetric[j]
                                && self.plain[j].is_none()
                                && p.cone.reduced == cone.system
                        }
                        _ => false,
                    })
            }
            JobKind::PropertyCone(_) => self.plain[k].is_none(),
            JobKind::Explored(_) | JobKind::Whole(_) => false,
        }
    }

    /// The systems of the safety and property cones among `jobs`: an entity cone of one of
    /// them is run with every check, so that its run can decide them.
    pub(super) fn wanted<'j>(&self, jobs: &'j [Job]) -> Vec<&'j TransitionSystem> {
        (jobs.iter().enumerate())
            .filter(|(k, _)| !self.forced(*k))
            .filter_map(|(_, job)| match &job.kind {
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
            if self.forced(k) {
                continue;
            }
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
