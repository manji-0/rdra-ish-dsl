use super::jobs::{ConeSystem, Job, JobKind};
use super::tlc::covering;
use super::{run_line, whole_caps};
use crate::state_findings::{render_violation, Finding, TraceSource};
use crate::tlc_output::Outcome;
use rdra_ish_core::lifecycle::{
    Caps, ComponentRun, Diag, NotChecked, PropertyVerdict, Scope, Selection, Severity, StateDiag,
    Target, TransitionSystem,
};
use rdra_ish_core::{in_parallel, ResolvedProgram, SemanticModel};
use std::collections::BTreeSet;
use std::time::Duration;

/// What BFS found at the verify bounds.
pub(super) struct Bfs {
    /// Errors of the entity cones.
    pub(super) errors: usize,
    /// Errors of the `can_reach` cones: violations, and cones the state cap stopped.
    pub(super) reach_errors: usize,
    /// Findings an entity cone left undecided at the state cap.
    pub(super) capped: usize,
}

/// BFS on each selected entity cone at the verify bounds: the `NoWitness` errors, the
/// `Deadlock`s (errors, or warnings at the bound), and the `NeverRuns` warnings, rendered;
/// then BFS on each `can_reach` cone, unreduced; then the `not checked:` block and one
/// summary line each. Every run is held to `caps`, the state cap and the memory limit.
pub(super) fn state_models(
    program: &ResolvedProgram,
    model: &SemanticModel,
    ts: &TransitionSystem,
    scope: &Scope,
    selection: &Selection,
    caps: Caps,
) -> Bfs {
    let pick = |want: fn(&Target) -> bool| -> Vec<Target> {
        selection.targets.iter().copied().filter(want).collect()
    };
    let entities = pick(|t| matches!(t, Target::Entity(_)));
    // A scoped run checks no property, `can_reach` included.
    let reach = match scope.is_whole() {
        true => ts.reach_targets(),
        false => Vec::new(),
    };
    let mut run = ts.explore_targets_reduced(&entities, caps);
    // Its own call, unreduced: a cone shared with an entity target would share its run.
    let reach_run = (!reach.is_empty()).then(|| ts.explore_targets(&reach, caps));
    if !scope.bucs.is_empty() {
        run.defer_progress();
    }
    // A finding `--buc` leaves open is the scope's choice, not a bound's.
    let capped = run.undecided().iter().filter(|u| u.cone.is_some()).count();
    let not_checked = NotChecked {
        ts,
        model,
        scope,
        selection,
        undecided: run.undecided(),
        temporal: ts.properties().len() + ts.reach_properties().len(),
    };
    if !not_checked.is_empty() {
        println!("{not_checked}");
    }
    let report = |verdicts: &[Diag]| {
        let mut errors = 0;
        for d in verdicts {
            if let Some(text) = render_violation(program, model, &d.diag, TraceSource::Bfs) {
                let finding = Finding::new(&d.diag, &[], text);
                eprintln!("{finding}");
                errors += usize::from(finding.severity == Severity::Error);
            }
        }
        errors
    };
    let errors = report(run.verdicts());
    let reach_errors = reach_run.as_ref().map_or(0, |r| report(r.verdicts()));
    let t = run.tally();
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    println!(
        "state models (BFS on {}): {}, {} witnessed, {}, {}",
        plural(t.entity_cones, "entity cone", "entity cones"),
        plural(t.state_models, "state model", "state models"),
        plural(t.witnessed, "state", "states"),
        plural(t.deadlocks, "deadlock", "deadlocks"),
        plural(
            t.never_run,
            "use case that never runs",
            "use cases that never run"
        ),
    );
    if let Some(r) = &reach_run {
        let cones = r.cones.len();
        println!(
            "can_reach (BFS on {cones} cone{}): {} checked, {} violated",
            if cones == 1 { "" } else { "s" },
            reach.len(),
            r.verdicts()
                .iter()
                .filter(|d| matches!(d.diag, StateDiag::GoalUnreachable { .. }))
                .count()
        );
    }
    Bfs {
        errors,
        reach_errors,
        capped,
    }
}

/// What BFS decided of the `stable`, `unchanged`, and `precedes` properties.
#[derive(Default)]
pub(super) struct Explored {
    /// Properties decided this way.
    pub(super) checked: usize,
    pub(super) violated: usize,
    /// Properties whose cone BFS stopped at the state cap or the memory limit before a violation.
    pub(super) capped: usize,
}

/// Each property `LtlKind::is_explored` names, decided by BFS on its cone: a line per property,
/// and each violation rendered with the shortest trace to the step or state that breaks it.
/// A cone that frees slots is decided again on the whole model, under the budget and the
/// whole-model cap.
pub(super) fn explored_properties(
    program: &ResolvedProgram,
    model: &SemanticModel,
    ts: &TransitionSystem,
    caps: Caps,
    budget: Duration,
) -> Explored {
    let mut out = Explored::default();
    for (i, p) in ts.properties().iter().enumerate() {
        if !p.formula.kind().is_explored() {
            continue;
        }
        out.checked += 1;
        let verdict = ts.decide_property(i, caps, whole_caps(caps, budget));
        // BFS decides this property, so no other run looks for its premises.
        if let (PropertyVerdict::Holds, Some(d)) = (&verdict, ts.vacuous_property(i, caps)) {
            if let Some(text) = render_violation(program, model, &d.diag, TraceSource::Bfs) {
                eprintln!("{}", Finding::new(&d.diag, &[], text));
            }
        }
        let said = match &verdict {
            PropertyVerdict::Holds => "ok",
            PropertyVerdict::Violated(_) => "violated",
            PropertyVerdict::Undecided => {
                "not decided: BFS stopped at the state cap or the memory limit"
            }
        };
        println!("property {} (BFS on its cone): {said}", p.name);
        match verdict {
            PropertyVerdict::Holds => {}
            PropertyVerdict::Undecided => out.capped += 1,
            PropertyVerdict::Violated(diag) => {
                let text = render_violation(program, model, &diag, TraceSource::Bfs)
                    .expect("a property violation renders");
                eprintln!("{}", Finding::new(&diag, &[], text));
                out.violated += 1;
            }
        }
    }
    out
}

/// A safety cone decided by BFS: the explorer on the module's own system, the reduced cone
/// TLC would check, at verify's bounds and caps.
pub(super) enum BfsCone {
    Ran {
        states: usize,
        /// The violated invariants and after-asserts, each with BFS's shortest trace.
        found: Vec<Diag>,
        /// False when the run stopped at the state cap or the memory limit.
        complete: bool,
    },
    /// The cone at this index of the jobs covers this one (`covering`) and passed.
    Covered(usize),
}

impl BfsCone {
    /// The verdict of `run`, a run of `cone`'s system.
    pub(super) fn of(ts: &TransitionSystem, cone: &ConeSystem, run: &ComponentRun) -> BfsCone {
        // The module checks every invariant and after-assert its system holds, as TLC does.
        let found = run
            .diags
            .iter()
            .filter(|d| {
                matches!(
                    d.diag,
                    StateDiag::ConstraintViolated { .. } | StateDiag::AfterAssertViolated { .. }
                )
            })
            .cloned()
            .map(|mut d| {
                if let Some(trace) = d.diag.trace_mut() {
                    ts.annotate(&cone.cone, trace);
                }
                d
            })
            .collect();
        BfsCone::Ran {
            states: run.states.len(),
            found,
            complete: run.complete,
        }
    }

    pub(super) fn passed(&self) -> bool {
        self.verdict() == Some(Outcome::Ok)
    }

    /// The verdict, `None` when the run stopped before a violation.
    pub(super) fn verdict(&self) -> Option<Outcome> {
        match self {
            BfsCone::Ran {
                found, complete, ..
            } => match (found.is_empty(), complete) {
                (false, _) => Some(Outcome::Counterexample),
                (true, true) => Some(Outcome::Ok),
                (true, false) => None,
            },
            BfsCone::Covered(_) => Some(Outcome::Ok),
        }
    }

    /// The run line, in the form of `Ran::line`.
    pub(super) fn line(&self, module: &str, checks: &[String], jobs: &[Job]) -> String {
        let verdict = self.verdict().map_or("not decided", Outcome::as_str);
        match self {
            BfsCone::Ran {
                states, complete, ..
            } => {
                let notes = match complete {
                    true => Vec::new(),
                    false => vec!["stopped at the state cap or the memory limit".to_string()],
                };
                run_line(module, checks, verdict, Some(*states as u64), &notes)
            }
            BfsCone::Covered(by) => run_line(
                module,
                checks,
                verdict,
                None,
                &[format!("checked in {}", jobs[*by].module)],
            ),
        }
    }
}

/// The run met the memory limit, maybe only because the runs beside it held the rest.
pub(super) fn crowded(run: &ComponentRun) -> bool {
    run.diags
        .iter()
        .any(|d| matches!(d.diag, StateDiag::MemoryCapReached { .. }))
}

/// Each safety cone of `jobs` decided by BFS, `None` for a property's job, and the vacuous
/// checks their runs show. As `run_cones` does for TLC, a cone another cone covers waits for
/// it: it is `ok` when that one passes and runs itself when it does not. Its vacuous checks
/// are read off the cone that covers it. A run that met the memory limit beside others runs
/// again alone, so that a crowded pool does not leave it undecided.
pub(super) fn bfs_cones(
    ts: &TransitionSystem,
    jobs: &[Job],
    caps: Caps,
) -> (Vec<Option<BfsCone>>, Vec<Diag>) {
    let covers = covering(jobs);
    let cone = |at: usize| match &jobs[at].kind {
        JobKind::Cone(cone) => Some(cone.as_ref()),
        JobKind::PropertyCone(_) | JobKind::Whole(_) => None,
    };
    // The verdict of the cone at `at`, and the vacuous checks of it and of the cones it
    // covers, read off its run; the run is dropped here.
    let decide = |at: usize| {
        let me = cone(at).expect("a safety cone");
        let run = me.system.clone().run(caps);
        let vacuous: Vec<Diag> = std::iter::once(at)
            .chain((0..jobs.len()).filter(|a| covers[*a] == Some(at)))
            .filter_map(cone)
            .flat_map(|c| c.targets.iter())
            .filter_map(|t| ts.vacuity(*t, &run))
            .collect();
        (BfsCone::of(ts, me, &run), vacuous, crowded(&run))
    };
    let first: Vec<usize> = (0..jobs.len())
        .filter(|a| cone(*a).is_some() && covers[*a].is_none())
        .collect();
    let ran = in_parallel(first.clone(), rdra_ish_core::jobs(), decide);
    let mut out: Vec<Option<BfsCone>> = jobs.iter().map(|_| None).collect();
    let mut found = Vec::new();
    for (&at, (verdict, vacuous, crowded)) in first.iter().zip(ran) {
        let (verdict, vacuous, _) = match crowded {
            true => decide(at),
            false => (verdict, vacuous, crowded),
        };
        found.extend(vacuous);
        out[at] = Some(verdict);
    }
    for at in 0..jobs.len() {
        if let (Some(by), Some(me)) = (covers[at], cone(at)) {
            out[at] = Some(match out[by].as_ref().is_some_and(BfsCone::passed) {
                true => BfsCone::Covered(by),
                false => BfsCone::of(ts, me, &me.system.clone().run(caps)),
            });
        }
    }
    (out, found)
}

/// What BFS decided of the safety cones.
#[derive(Default)]
pub(super) struct Safety {
    /// Distinct invariants and after-asserts violated.
    pub(super) violated: usize,
    /// Cones the state cap or the memory limit stopped before a violation.
    pub(super) capped: usize,
}

/// Prints each safety cone's run line and renders each violation with its BFS trace.
pub(super) fn report_bfs_cones(
    program: &ResolvedProgram,
    model: &SemanticModel,
    jobs: &[Job],
    cones: &[Option<BfsCone>],
) -> Safety {
    let mut out = Safety::default();
    let mut reported = BTreeSet::new();
    for (job, cone) in jobs.iter().zip(cones) {
        let Some(cone) = cone else {
            continue;
        };
        println!("{}", cone.line(&job.module, &job.checks, jobs));
        let BfsCone::Ran {
            found, complete, ..
        } = cone
        else {
            continue;
        };
        // A check several cones hold is reported once, from the first cone that breaks it, so
        // the count does not depend on how the cones overlap.
        for d in found.iter().filter(|d| reported.insert(d.diag.name(&[]))) {
            let text = render_violation(program, model, &d.diag, TraceSource::Bfs)
                .expect("a violation renders");
            eprintln!("{}", Finding::new(&d.diag, &[], text));
        }
        out.capped += usize::from(found.is_empty() && !complete);
    }
    out.violated = reported.len();
    out
}
