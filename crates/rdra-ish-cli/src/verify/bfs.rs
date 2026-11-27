use super::jobs::{ConeSystem, Job, JobKind};
use super::native::Plain;
use super::report::run_line;
use super::shared::Shared;
use super::WholeRun;
use crate::state_findings::{render_violation, Finding, TraceSource};
use crate::tlc_output::Outcome;
use rdra_ish_core::lifecycle::{
    Caps, Checks, ComponentRun, Diag, EntId, NotChecked, OrbitRun, PropertyVerdict, QuotientCache,
    Recheck, RunEnd, Scope, Selection, Severity, StateDiag, StoredRun, Target, TargetExploration,
    TransitionSystem, Undecided,
};
use rdra_ish_core::{in_parallel, ResolvedProgram, SemanticModel};
use std::collections::BTreeSet;

/// What BFS found at the verify bounds.
pub(super) struct Bfs {
    /// Errors of the entity cones.
    pub(super) errors: usize,
    /// Errors of the `can_reach` cones: their violations.
    pub(super) reach_errors: usize,
    /// Findings an entity cone, and `can_reach` properties their cone, left undecided at the
    /// state cap or the memory limit.
    pub(super) capped: usize,
    /// Under `--with-tlc`, what TLC checks again of the state models (`StateModels`).
    pub(super) models: StateModels,
}

/// The entity cones whose state models TLC checks under `--with-tlc`, and the deadlocks and
/// missing witnesses BFS found on any cone, errors and warnings alike.
#[derive(Default)]
pub(super) struct StateModels {
    pub(super) cones: Vec<StateModelCone>,
    /// Too large for TLC: each module's name and BFS's state count.
    pub(super) skipped: Vec<(String, usize)>,
    pub(super) found: BTreeSet<String>,
}

/// An entity cone BFS explored to the end, as TLC checks its state models: the system BFS
/// explored (reduced as `verify` explores it), so TLC reaches the states BFS reached.
pub(super) struct StateModelCone {
    pub(super) system: TransitionSystem,
    /// The entities of the cone with a state model, as ids of `system`.
    pub(super) entities: Vec<EntId>,
    /// `StateModel_<E>`, `E` the first of them, as `export --kind tla` names it.
    pub(super) first: String,
    pub(super) states: usize,
}

/// The largest entity cone, in BFS's states, whose state models TLC checks under
/// `--with-tlc`. Each rerun explores the cone again, and a witnesses run above 1,000 states
/// finds one witness per run.
pub(super) const STATE_MODEL_TLC_STATES: usize = 100_000;

/// BFS on each selected entity cone at the verify bounds: the `NoWitness` errors, the
/// `Deadlock`s (errors, or warnings at the bound), and the `NeverRuns` warnings, rendered;
/// then BFS on each `can_reach` cone, unreduced; then the `not checked:` block and one
/// summary line each. Every run is held to `caps`, the state cap and the memory limit.
/// An entity cone whose system is one of `every_check` evaluates every check, and
/// `on_cone` sees each entity cone's run while the pass holds it (#209).
#[allow(clippy::too_many_arguments)]
pub(super) fn state_models(
    program: &ResolvedProgram,
    model: &SemanticModel,
    ts: &TransitionSystem,
    scope: &Scope,
    selection: &Selection,
    caps: Caps,
    with_tlc: bool,
    quotients: QuotientCache,
    every_check: &[&TransitionSystem],
    mut on_cone: impl FnMut(&ComponentRun),
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
    let mut run = ts.explore_targets_reduced_in(&entities, caps, &quotients, every_check);
    for cone in &run.cones {
        on_cone(&cone.run);
    }
    // The entity pass is the last that reduces a cone: what the cache holds goes back to the
    // pool before the runs that follow, which see it as they would without the cache.
    drop(quotients);
    // Its own call, unreduced: a cone shared with an entity target would share its run.
    rdra_ish_core::perf::pass("reach_pass");
    let reach_run = (!reach.is_empty()).then(|| ts.explore_targets(&reach, caps));
    if !scope.bucs.is_empty() {
        run.defer_progress();
    }
    // A `can_reach` whose cone stopped is listed beside the entity findings it left open.
    let reach_undecided = reach_run.as_ref().map_or(&[][..], |r| r.undecided());
    let undecided: Vec<Undecided> = run
        .undecided()
        .iter()
        .chain(reach_undecided)
        .cloned()
        .collect();
    // A finding `--buc` leaves open is the scope's choice, not a bound's.
    let capped = undecided.iter().filter(|u| u.cone.is_some()).count();
    let not_checked = NotChecked {
        ts,
        model,
        scope,
        selection,
        undecided: &undecided,
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
    let models = match with_tlc && scope.is_whole() {
        true => state_model_cones(ts, &run),
        false => StateModels::default(),
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
        let open = match reach_undecided.len() {
            0 => String::new(),
            n => format!(", {n} not decided"),
        };
        println!(
            "can_reach (BFS on {cones} cone{}): {} checked, {} violated{open}",
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
        models,
    }
}

/// The entity cones of `run` with a state model that BFS explored to the end, the ones up to
/// `STATE_MODEL_TLC_STATES` states for TLC and the rest named as skipped, and the deadlocks
/// and missing witnesses BFS found. A cone the state cap or the memory limit stopped decides
/// nothing to compare.
fn state_model_cones(ts: &TransitionSystem, run: &TargetExploration) -> StateModels {
    let mut out = StateModels::default();
    let mut groups: Vec<(usize, Vec<EntId>)> = Vec::new();
    for (target, at) in &run.targets {
        let Target::Entity(e) = target else {
            continue;
        };
        match groups.iter_mut().find(|(g, _)| g == at) {
            Some((_, shared)) => shared.push(*e),
            None => groups.push((*at, vec![*e])),
        }
    }
    for (at, shared) in groups {
        let cone = &run.cones[at].run;
        // A stopped cone decides nothing to compare, and the entity pass released it.
        if !cone.complete {
            continue;
        }
        let modelled: Vec<EntId> = shared
            .into_iter()
            .filter(|e| !ts.state_model_states(*e).is_empty())
            .collect();
        let Some(first) = modelled.first() else {
            continue;
        };
        let first = format!("StateModel_{}", ts.entity(*first).id);
        let states = cone.states.len();
        if states > STATE_MODEL_TLC_STATES {
            out.skipped.push((first, states));
            continue;
        }
        let system = cone.system.clone();
        let entities = modelled
            .iter()
            .map(|e| {
                let key = ts.entity(*e).key;
                system
                    .entity_ids()
                    .find(|x| system.entity(*x).key == key)
                    .expect("an entity is in its own cone")
            })
            .collect();
        out.cones.push(StateModelCone {
            system,
            entities,
            first,
            states,
        });
    }
    out.found = run
        .verdicts()
        .iter()
        .map(|d| d.diag.name(&[]))
        .filter(|n| n.starts_with("Deadlock_") || n.starts_with("Witness_"))
        .collect();
    out
}

/// What BFS decided of the `stable`, `unchanged`, and `precedes` properties.
#[derive(Default)]
pub(super) struct Explored {
    /// Properties decided this way.
    pub(super) checked: usize,
    pub(super) violated: usize,
    /// Properties whose cone BFS stopped at the state cap or the memory limit before a violation.
    pub(super) capped: usize,
    /// Each property's index and its verdict, `None` when not decided, for `--with-tlc` to
    /// compare.
    pub(super) verdicts: Vec<(usize, Option<Outcome>)>,
}

impl Explored {
    /// The verdict of property `i`, when it is one of these and BFS decided it.
    pub(super) fn verdict(&self, i: usize) -> Option<Outcome> {
        self.verdicts
            .iter()
            .find(|(at, _)| *at == i)
            .and_then(|(_, v)| *v)
    }
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
    budget: WholeRun,
) -> Explored {
    let mut out = Explored::default();
    for (i, p) in ts.properties().iter().enumerate() {
        if !p.formula.kind().is_explored() {
            continue;
        }
        out.checked += 1;
        let verdict = ts.decide_property(i, caps, budget.caps(caps));
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
                "not decided: BFS stopped at the state cap (--max-states), the memory limit, or the time budget (--temporal-budget)"
            }
        };
        println!("property {} (BFS on its cone): {said}", p.name);
        out.verdicts.push((
            i,
            match &verdict {
                PropertyVerdict::Holds => Some(Outcome::Ok),
                PropertyVerdict::Violated(_) => Some(Outcome::Counterexample),
                PropertyVerdict::Undecided => None,
            },
        ));
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
        /// The states stored: under symmetry, one per class.
        states: usize,
        /// The violated invariants and after-asserts of the cone's module, each with BFS's
        /// shortest trace: what TLC's run of the module is compared with.
        found: Vec<Diag>,
        /// Violations of a reduced cone that do not lift to the cone (`StateDiag::LiftFault`):
        /// a tool bug, which makes the verdict a tool error (P52).
        faults: Vec<Diag>,
        /// Each violation of `found` the cone cannot vouch for (`Trace::vouched`), by name,
        /// decided again on its exact cone (`recheck_cones`); empty until then.
        rechecks: Vec<(String, Recheck)>,
        /// How the run ended: a run stops once every check has failed.
        end: RunEnd,
        /// The run stopped at the memory limit.
        memory: bool,
        /// The run was up to instance symmetry (`--symmetry`).
        orbits: Option<Box<Orbits>>,
        /// The largest store a plain run asked `Caps::memory` to cover
        /// (`ComponentRun::peak`); 0 for a run up to symmetry.
        peak: u64,
    },
    /// The cone at this index of the jobs covers this one (`covering`) and passed.
    Covered(usize),
    /// BFS stopped at the memory limit after `states` states and before a violation, and
    /// `--prove` proved every check of the cone for any number of rows (`decide_by_proof`).
    Proved { states: usize },
}

/// What a run up to instance symmetry adds to its cone's line.
pub(super) struct Orbits {
    /// The states of the plain run the classes stand for: an estimate.
    pub(super) estimate: u128,
    /// Classes stored without their canonical form (`BRANCH_CAP`).
    pub(super) cut: u64,
}

/// `n` to two significant figures, as `62M`.
pub(super) fn about(n: u128) -> String {
    const UNITS: [&str; 7] = ["", "K", "M", "G", "T", "P", "E"];
    if n < 1000 {
        return n.to_string();
    }
    let digits = n.to_string().len();
    // Round to two significant figures.
    let scale = 10u128.pow(digits as u32 - 2);
    let rounded = (n + scale / 2) / scale * scale;
    let digits = rounded.to_string().len();
    let unit = ((digits - 1) / 3).min(UNITS.len() - 1);
    let base = 10u128.pow(3 * unit as u32);
    let whole = rounded / base;
    match whole < 10 {
        true => {
            format!("{}.{}{}", whole, (rounded % base) * 10 / base, UNITS[unit]).replace(".0", "")
        }
        false => format!("{whole}{}", UNITS[unit]),
    }
}

impl BfsCone {
    /// The verdict of `run`, a run of `cone`'s system up to instance symmetry.
    pub(super) fn of_orbits(ts: &TransitionSystem, cone: &ConeSystem, run: &OrbitRun) -> BfsCone {
        let mut out = Self::of_parts(ts, cone, run.checks(), run.diags(), run.end());
        if let BfsCone::Ran { states, orbits, .. } = &mut out {
            *states = run.orbits();
            *orbits = Some(Box::new(Orbits {
                estimate: run.estimate(),
                cut: run.cut(),
            }));
        }
        out
    }

    /// The states of the cone's module, when a plain run explored all of them: not a run
    /// stopped once every check failed, at a cap or the memory limit, nor one up to symmetry,
    /// which counts orbits. The TLC scheduler sizes the cone's run by it (#384).
    pub(super) fn counted(&self) -> Option<u64> {
        match self {
            BfsCone::Ran {
                states,
                end: RunEnd::Complete,
                memory: false,
                orbits: None,
                ..
            } => Some(*states as u64),
            _ => None,
        }
    }

    /// The verdict of `run`, a run of `cone`'s system.
    pub(super) fn of(ts: &TransitionSystem, cone: &ConeSystem, run: &ComponentRun) -> BfsCone {
        let mut out = Self::of_parts(ts, cone, run.checks(), &run.diags, run.end());
        if let BfsCone::Ran { states, peak, .. } = &mut out {
            *states = run.states.len();
            *peak = run.peak;
        }
        out
    }

    fn of_parts(
        ts: &TransitionSystem,
        cone: &ConeSystem,
        checks: &Checks,
        diags: &[Diag],
        end: RunEnd,
    ) -> BfsCone {
        // The module checks every invariant and after-assert its system holds, as TLC does.
        rdra_ish_core::shadow_assert!(
            checks == &Checks::All,
            "a safety cone evaluates every check"
        );
        let found = diags
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
        let faults = (diags.iter())
            .filter(|d| matches!(d.diag, StateDiag::LiftFault { .. }))
            .cloned()
            .collect();
        BfsCone::Ran {
            states: 0,
            found,
            faults,
            rechecks: Vec::new(),
            end,
            memory: met_memory_limit(diags),
            orbits: None,
            peak: 0,
        }
    }

    /// The cone, decided by `--prove` when BFS stopped at the memory limit before a violation
    /// and every check of it is an invariant in `proved` (indices into `ts.invariants()`).
    /// The proof's encoding allows every step the explorer takes, at any number of rows and
    /// any Int value, so a rule it proves holds at verify's bounds too. An after-assert,
    /// which the proof does not state, leaves the cone undecided.
    fn decided_by_proof(
        self,
        ts: &TransitionSystem,
        cone: &ConeSystem,
        proved: &BTreeSet<usize>,
    ) -> BfsCone {
        let BfsCone::Ran {
            states,
            found,
            faults,
            end: RunEnd::Capped,
            memory: true,
            ..
        } = &self
        else {
            return self;
        };
        // A violation that did not lift (P52) stays a tool error, whatever the proof says.
        if !faults.is_empty() {
            return self;
        }
        // A cone's invariant is the whole system's of the same name; should two share a name,
        // both must be proved.
        let is_proved = |name: &str| {
            let mut same = (0..ts.invariants().len())
                .filter(|i| ts.invariants()[*i].name == name)
                .peekable();
            same.peek().is_some() && same.all(|i| proved.contains(&i))
        };
        let all_proved = cone.system.after_asserts().is_empty()
            && cone
                .system
                .invariants()
                .iter()
                .all(|inv| is_proved(&inv.name));
        match found.is_empty() && all_proved {
            true => BfsCone::Proved { states: *states },
            false => self,
        }
    }

    pub(super) fn passed(&self) -> bool {
        self.verdict() == Some(Outcome::Ok)
    }

    /// The verdict of the cone's module, `None` when the run stopped before a violation: what
    /// TLC's run of the same module is compared with. A cone that leaves a raised mark out
    /// reaches more than the whole system, so its counterexample may be none of the whole
    /// system's; `verdict` is what the run reports.
    pub(super) fn module_verdict(&self) -> Option<Outcome> {
        match self {
            BfsCone::Ran { faults, .. } if !faults.is_empty() => Some(Outcome::ToolError),
            BfsCone::Ran { found, end, .. } => match (found.is_empty(), end) {
                (false, _) => Some(Outcome::Counterexample),
                (true, RunEnd::Complete) => Some(Outcome::Ok),
                (true, RunEnd::Capped | RunEnd::ChecksFailed) => None,
            },
            BfsCone::Covered(_) | BfsCone::Proved { .. } => Some(Outcome::Ok),
        }
    }

    /// The verdict on the whole system, `None` when it is not decided: a counterexample when
    /// some violation is one of the whole system's (`violations`), `ok` when the cone found
    /// none or its exact cone showed each one it found spurious, and `None` when the run
    /// stopped first or a violation is neither vouched for nor rechecked to hold.
    pub(super) fn verdict(&self) -> Option<Outcome> {
        // A violation that did not lift to the cone (P52) is a tool error first.
        if let BfsCone::Ran { faults, .. } = self {
            if !faults.is_empty() {
                return Some(Outcome::ToolError);
            }
        }
        let BfsCone::Ran {
            found,
            rechecks,
            end,
            ..
        } = self
        else {
            return self.module_verdict();
        };
        if self.violations().next().is_some() {
            return Some(Outcome::Counterexample);
        }
        if found.is_empty() {
            return self.module_verdict();
        }
        // Every violation it found was rechecked; the run stopped once every check failed,
        // or ran to the end, so each check of the cone is decided.
        let held = |d: &Diag| {
            let name = d.diag.name(&[]);
            (rechecks.iter()).any(|(n, r)| *n == name && matches!(r, Recheck::Holds))
        };
        match (found.iter().all(held), end) {
            (true, RunEnd::Complete | RunEnd::ChecksFailed) => Some(Outcome::Ok),
            _ => None,
        }
    }

    /// The violations of the whole system the cone found: each one the cone vouches for
    /// (`Trace::vouched`, slots aside: a trace past the bound is a run at a larger bound),
    /// and the counterexample of each exact cone that rechecked one it does not.
    pub(super) fn violations(&self) -> impl Iterator<Item = &Diag> {
        let (found, rechecks): (&[Diag], &[(String, Recheck)]) = match self {
            BfsCone::Ran {
                found, rechecks, ..
            } => (found, rechecks),
            BfsCone::Covered(_) | BfsCone::Proved { .. } => (&[], &[]),
        };
        let vouched = found.iter().filter(|d| vouched(d));
        let rechecked = rechecks.iter().filter_map(|(_, r)| match r {
            Recheck::Violated(d) => Some(&**d),
            Recheck::Holds | Recheck::Undecided(_) => None,
        });
        vouched.chain(rechecked)
    }

    /// The run line, in the form of `Ran::line`.
    pub(super) fn line(&self, module: &str, checks: &[String], jobs: &[Job]) -> String {
        let verdict = self.verdict().map_or("not decided", Outcome::as_str);
        match self {
            BfsCone::Ran {
                states,
                end,
                orbits,
                rechecks,
                ..
            } => {
                let mut notes = Vec::new();
                let mut count = Some(*states as u64);
                if let Some(o) = orbits {
                    count = None;
                    notes.push(format!("about {} states", about(o.estimate)));
                }
                if *end == RunEnd::Capped {
                    notes.push(
                        "stopped at the state cap (--max-states) or the memory limit".to_string(),
                    );
                }
                let mut line = run_line(module, checks, verdict, count, &notes);
                for (name, recheck) in rechecks {
                    let on_exact = match recheck {
                        Recheck::Violated(_) => "counterexample",
                        Recheck::Holds => "ok",
                        Recheck::Undecided(_) => {
                            "not decided (stopped at the state cap or the memory limit)"
                        }
                    };
                    line.push_str(&format!(
                        "\n  note: {name}'s counterexample runs a use case without its event having been raised, which this cone does not rule out; on its exact cone: {on_exact}"
                    ));
                }
                // The note under the line: what the canonicalizer cut. What the group left
                // out goes to `RDRA_ISH_SYMMETRY_STATS` (`symmetry_stats`).
                if let Some(o) = orbits.as_ref().filter(|o| o.cut > 0) {
                    line.push_str(&format!(
                        "\n  note: {} states stored without their canonical form (branch cap); the count is an upper bound",
                        o.cut
                    ));
                }
                line
            }
            BfsCone::Covered(by) => run_line(
                module,
                checks,
                verdict,
                None,
                &[format!("checked in {}", jobs[*by].module)],
            ),
            BfsCone::Proved { states } => run_line(
                module,
                checks,
                verdict,
                Some(*states as u64),
                &[
                    "BFS stopped at the memory limit".to_string(),
                    "decided by proof: --prove proved every check for any number of rows"
                        .to_string(),
                ],
            ),
        }
    }
}

/// Each safety cone of `cones` that BFS left undecided at the memory limit, decided by the
/// rules `--prove` proved (`BfsCone::decided_by_proof`).
pub(super) fn decide_by_proof(
    ts: &TransitionSystem,
    jobs: &[Job],
    cones: Vec<Option<BfsCone>>,
    proved: &BTreeSet<usize>,
) -> Vec<Option<BfsCone>> {
    jobs.iter()
        .zip(cones)
        .map(|(job, cone)| match (&job.kind, cone) {
            (JobKind::Cone(system), Some(cone)) => Some(cone.decided_by_proof(ts, system, proved)),
            (_, cone) => cone,
        })
        .collect()
}

/// The cone vouches for the violation as one of the whole system (`Trace::vouched`), slots
/// aside: its trace runs no use case without its event.
fn vouched(d: &Diag) -> bool {
    d.diag.trace().is_none_or(|t| t.vouched(false))
}

/// Each violation a safety cone found and cannot vouch for, decided again on its check's exact
/// cone under `caps` (`TransitionSystem::recheck`), once per check, before anything reads the
/// verdicts: the proof leaves out only the rules a real violation refutes. A cone that leaves
/// a raised mark out runs the handler without the event, so its counterexample may be a run
/// the event order forbids at every bound.
pub(super) fn recheck_cones(
    ts: &TransitionSystem,
    jobs: &[Job],
    cones: &mut [Option<BfsCone>],
    caps: Caps,
) {
    let mut done: Vec<(String, Recheck)> = Vec::new();
    for (job, cone) in jobs.iter().zip(cones) {
        let (
            JobKind::Cone(_),
            Some(BfsCone::Ran {
                found, rechecks, ..
            }),
        ) = (&job.kind, cone)
        else {
            continue;
        };
        for d in found.iter().filter(|d| !vouched(d)) {
            let name = d.diag.name(&[]);
            let recheck = match done.iter().find(|(n, _)| *n == name) {
                Some((_, r)) => r.clone(),
                None => {
                    let target = ts
                        .violated_check(&d.diag)
                        .expect("a cone's check is the whole system's");
                    let r = ts.recheck(target, false, caps);
                    done.push((name.clone(), r.clone()));
                    r
                }
            };
            rechecks.push((name, recheck));
        }
    }
}

/// The run met the memory limit, maybe only because the runs beside it held the rest.
pub(super) fn crowded(run: &ComponentRun) -> bool {
    met_memory_limit(&run.diags)
}

/// Some diagnostic says the run met the memory limit.
fn met_memory_limit(diags: &[Diag]) -> bool {
    diags
        .iter()
        .any(|d| matches!(d.diag, StateDiag::MemoryCapReached { .. }))
}

/// The safety cone of `jobs[at]`, decided on `run`, a run of its system: its verdict, and the
/// vacuous checks of it and of the cones `covers` says it covers.
pub(super) fn safety_on(
    ts: &TransitionSystem,
    jobs: &[Job],
    covers: &[Option<usize>],
    at: usize,
    run: &ComponentRun,
) -> (BfsCone, Vec<Diag>) {
    let me = safety_cone(jobs, at).expect("a safety cone");
    (
        BfsCone::of(ts, me, run),
        vacuous_on(ts, jobs, covers, at, run),
    )
}

/// The vacuous checks of the safety cone of `jobs[at]` and of the cones `covers` says it
/// covers, read off `run`, a run of its system with or without symmetry.
fn vacuous_on(
    ts: &TransitionSystem,
    jobs: &[Job],
    covers: &[Option<usize>],
    at: usize,
    run: &impl StoredRun,
) -> Vec<Diag> {
    std::iter::once(at)
        .chain((0..jobs.len()).filter(|a| covers[*a] == Some(at)))
        .filter_map(|a| safety_cone(jobs, a))
        .flat_map(|c| c.targets.iter())
        .filter_map(|t| ts.vacuity(*t, run))
        .collect()
}

/// Under `RDRA_ISH_SYMMETRY_STATS`, one line on stderr per run up to instance symmetry: the
/// orbits it stored, the states they stand for, and the classes its group left out, which
/// the cone line does not print (docs/design/instance-symmetry.md, "Always by default", 3 and
/// 4).
pub(super) fn symmetry_stats(module: &str, run: &OrbitRun) {
    if std::env::var_os("RDRA_ISH_SYMMETRY_STATS").is_none() {
        return;
    }
    let left: Vec<String> = (run.left_out().iter())
        .map(|l| format!("{} ({})", l.entity, l.rule.text()))
        .collect();
    eprintln!(
        "symmetry: {module}: {} orbits, about {} states, {} cut; leaves out: {}",
        run.orbits(),
        run.estimate(),
        run.cut(),
        match left.is_empty() {
            true => "nothing".to_string(),
            false => left.join(", "),
        }
    );
}

fn safety_cone(jobs: &[Job], at: usize) -> Option<&ConeSystem> {
    match &jobs[at].kind {
        JobKind::Cone(cone) => Some(cone.as_ref()),
        JobKind::PropertyCone(_) | JobKind::Explored(_) | JobKind::Whole(_) => None,
    }
}

/// Each safety cone of `jobs` decided by BFS, `None` for a property's job, and the vacuous
/// checks their runs show. As `run_cones` does for TLC, a cone another cone covers waits for
/// it: it is `ok` when that one passes and runs itself when it does not. Its vacuous checks
/// are read off the cone that covers it. A run that met the memory limit beside others runs
/// again alone, so that a crowded pool does not leave it undecided.
///
/// A cone `shared` holds was decided on the entity pass's run of the same system (#209). A
/// cone whose system is a property's cone runs to the end, so that its run also gives the
/// property's plain answer, which goes into `shared`; when that run stops at a cap the cone
/// runs again as it would alone.
///
/// Under `--symmetry always` a cone that no plain run decides runs up to instance symmetry,
/// a reduced one up to `G_T` (#212 phase 3), alone (`Shared::up_to_symmetry`), and its run
/// gives no property its plain answer.
pub(super) fn bfs_cones(
    ts: &TransitionSystem,
    jobs: &[Job],
    caps: Caps,
    covers: &[Option<usize>],
    shared: &mut Shared,
) -> (Vec<Option<BfsCone>>, Vec<Diag>) {
    let cone = |at: usize| safety_cone(jobs, at);
    let up_to_symmetry: Vec<bool> = (0..jobs.len())
        .map(|at| shared.up_to_symmetry(jobs, at))
        .collect();
    let up_to_symmetry = |at: usize| up_to_symmetry[at];
    // Per safety cone, the properties not yet decided whose reduced cone is its system. A
    // cone that runs up to symmetry has none, and a property that does is no partner.
    let partners: Vec<Vec<usize>> = (0..jobs.len())
        .map(|at| match cone(at) {
            Some(me) if !up_to_symmetry(at) => (0..jobs.len())
                .filter(|k| shared.plain[*k].is_none() && !up_to_symmetry(*k))
                .filter(|k| matches!(&jobs[*k].kind, JobKind::PropertyCone(p) if p.cone.reduced == me.system))
                .collect(),
            Some(_) | None => Vec::new(),
        })
        .collect();
    let property = |k: usize| match &jobs[k].kind {
        JobKind::PropertyCone(p) => p.as_ref(),
        _ => unreachable!("a property's job"),
    };
    // The cone at `at` up to instance symmetry, stopped once every check has failed.
    let orbits = |at: usize| {
        let me = cone(at).expect("a safety cone");
        let run = me.system.clone().run_orbits(caps.until_all_violated());
        symmetry_stats(&jobs[at].module, &run);
        let vacuous = vacuous_on(ts, jobs, covers, at, &run);
        let memory = met_memory_limit(run.diags());
        (
            BfsCone::of_orbits(ts, me, &run),
            vacuous,
            memory,
            Vec::new(),
        )
    };
    // A safety cone decides its checks alone, so it stops once every one has failed: a check
    // it holds is then violated, so neither vacuous nor covering another.
    let alone = |at: usize| {
        if up_to_symmetry(at) {
            return orbits(at);
        }
        let run = cone(at)
            .expect("a safety cone")
            .system
            .clone()
            .run(caps.until_all_violated());
        let (verdict, vacuous) = safety_on(ts, jobs, covers, at, &run);
        (verdict, vacuous, crowded(&run), Vec::new())
    };
    // The verdict of the cone at `at`, the vacuous checks of it and of the cones it covers,
    // and the plain answers of the properties on its system, read off its run; the run is
    // dropped here.
    let decide = |at: usize| -> (BfsCone, Vec<Diag>, bool, Vec<(usize, Plain)>) {
        if partners[at].is_empty() {
            return alone(at);
        }
        let run = cone(at).expect("a safety cone").system.clone().run(caps);
        if !run.complete {
            drop(run);
            return alone(at);
        }
        let plains = (partners[at].iter())
            .map(|&k| (k, Plain::of(ts, property(k), &run)))
            .collect();
        let (verdict, vacuous) = safety_on(ts, jobs, covers, at, &run);
        (verdict, vacuous, false, plains)
    };
    let mut out: Vec<Option<BfsCone>> = jobs.iter().map(|_| None).collect();
    let mut found = Vec::new();
    let first: Vec<usize> = (0..jobs.len())
        .filter(|a| cone(*a).is_some() && covers[*a].is_none())
        .collect();
    let mut ran = Vec::new();
    for &at in &first {
        if let Some((verdict, vacuous)) = shared.safety[at].take() {
            ran.push((at, verdict, vacuous));
        }
    }
    let open: Vec<usize> = (first.iter().copied())
        .filter(|at| !ran.iter().any(|(a, _, _)| a == at))
        .collect();
    let decided = in_parallel(open.clone(), rdra_ish_core::jobs(), decide);
    for (&at, (verdict, vacuous, crowded, plains)) in open.iter().zip(decided) {
        let (verdict, vacuous, _, plains) = match crowded {
            true => {
                drop((verdict, vacuous, plains));
                decide(at)
            }
            false => (verdict, vacuous, crowded, plains),
        };
        for (k, plain) in plains {
            shared.plain[k] = Some(plain);
        }
        ran.push((at, verdict, vacuous));
    }
    // In job order, as the cones were before some came decided.
    ran.sort_by_key(|(at, _, _)| *at);
    for (at, verdict, vacuous) in ran {
        found.extend(vacuous);
        out[at] = Some(verdict);
    }
    for at in 0..jobs.len() {
        if let (Some(by), Some(_)) = (covers[at], cone(at)) {
            out[at] = Some(match out[by].as_ref().is_some_and(BfsCone::passed) {
                true => BfsCone::Covered(by),
                false => match shared.safety[at].take() {
                    Some((verdict, _)) => verdict,
                    None => alone(at).0,
                },
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
    /// Violations of reduced cones that do not lift to their cone (P52): tool errors.
    pub(super) faults: usize,
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
        if let BfsCone::Ran { faults, .. } = cone {
            // As a liveness lasso that fails its own check (`report.rs`): one `LassoFault`
            // line, counted with those.
            for d in faults {
                eprintln!(
                    "error[LassoFault]: {} [{}]: {}",
                    job.module,
                    job.checks.join(", "),
                    d.diag
                );
            }
            out.faults += faults.len();
        }
        // A check several cones hold is reported once, from the first cone that breaks it, so
        // the count does not depend on how the cones overlap.
        for d in cone
            .violations()
            .filter(|d| reported.insert(d.diag.name(&[])))
        {
            let text = render_violation(program, model, &d.diag, TraceSource::Bfs)
                .expect("a violation renders");
            eprintln!("{}", Finding::new(&d.diag, &[], text));
        }
        out.capped += usize::from(cone.verdict().is_none());
    }
    out.violated = reported.len();
    out
}

#[cfg(test)]
mod tests {
    use super::state_model_cones;
    use rdra_ish_core::lifecycle::{Bounds, Caps, QuotientCache, Scope, Target, TransitionSystem};
    use rdra_ish_core::load_project;
    use rdra_ish_core::memory::{MemoryPool, MemorySize};
    use std::path::Path;

    /// A cone whose reduced run found a violation that does not lift (`LiftFault`) is a tool
    /// error, never `ok`, though `found` holds no violation and the run is complete (P52); it
    /// covers no other cone.
    #[test]
    fn a_violation_that_does_not_lift_makes_the_cone_a_tool_error() {
        use super::BfsCone;
        use crate::tlc_output::Outcome;
        use rdra_ish_core::lifecycle::{Diag, LassoFault, ModelRef, RunEnd, Site, StateDiag};

        let fault = Diag {
            entities: Vec::new(),
            diag: StateDiag::LiftFault {
                name: "Forbidden_Order_0".into(),
                site: Site::bare(ModelRef::EntityInvariant(0)),
                fault: LassoFault::Satisfies,
            },
        };
        let cone = |faults: Vec<Diag>| BfsCone::Ran {
            states: 7,
            found: Vec::new(),
            faults,
            rechecks: Vec::new(),
            end: RunEnd::Complete,
            memory: false,
            orbits: None,
            peak: 0,
        };
        assert_eq!(cone(Vec::new()).verdict(), Some(Outcome::Ok));
        let faulty = cone(vec![fault]);
        assert_eq!(faulty.verdict(), Some(Outcome::ToolError));
        assert!(!faulty.passed());
    }

    /// Under a memory limit the entity pass releases each entity cone that stops, and the TLC
    /// side reads none of them: a reader of a released cone's graph panics (#284).
    #[test]
    fn tlc_reads_no_stopped_entity_cone() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let model = load_project(&[root.join("samples/patent-ops")], None)
            .expect("load patent-ops")
            .model;
        let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
        let entities: Vec<Target> = (ts.targets().into_iter())
            .filter(|t| matches!(t, Target::Entity(_)))
            .collect();
        let limit = MemorySize::from_bytes(1 << 20).expect("a size");
        let caps = Caps::VERIFY.charged_to(MemoryPool::for_command(limit, "--memory"));
        let run = ts.explore_targets_reduced_in(&entities, caps, &QuotientCache::new(), &[]);
        let stopped = run.cones.iter().filter(|c| !c.run.complete).count();
        assert!(stopped > 0, "some entity cone stops at 1MiB");
        let complete = run.cones.iter().filter(|c| c.run.complete).count();
        let models = state_model_cones(&ts, &run);
        assert!(
            models.cones.len() + models.skipped.len() <= complete,
            "TLC takes complete cones alone"
        );
    }
}
