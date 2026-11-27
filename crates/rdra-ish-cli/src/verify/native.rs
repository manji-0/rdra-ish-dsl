use super::bfs::{about, crowded, BfsCone};
use super::jobs::{Job, JobKind, PropertyJob, RunOn};
use super::report::run_line;
use super::shared::Shared;
use super::tlc::{undecided_hint, Decision, Stop};
use super::WholeRun;
use crate::property_cone::{AfterPlain, ConeRun, Verdict};
use crate::tlc_output::Outcome;
use rdra_ish_core::in_parallel;
use rdra_ish_core::lifecycle::{
    Caps, ComponentRun, Diag, Fairness, LassoFault, OrbitRun, PropertyVerdict, ReducedVerdict,
    RunEnd, StateDiag, TransitionSystem,
};
use rdra_ish_emit::tla::TlaNames;

/// What the native checker decides of one `always`, `eventually`, or `leads_to`, on its cone
/// or on the whole model.
pub(super) struct Native {
    pub(super) decision: Decision,
    /// The states of the run the line reports: the plain cone's, or the whole model's.
    pub(super) states: Option<u64>,
    pub(super) notes: Vec<String>,
    /// Each cone run, by the system it decides, and what it said, for `--with-tlc` to
    /// compare: `None` for a run that stopped at a cap or the budget before it decided, which
    /// no TLC verdict contradicts.
    pub(super) runs: Vec<(RunOn, Option<Outcome>)>,
    /// The counterexample of a `counterexample` verdict.
    pub(super) found: Option<Box<StateDiag>>,
    /// A counterexample that failed its own check, or a reduced cone that disagrees with its
    /// cone: a tool bug, never a finding about the model.
    pub(super) fault: Option<String>,
    /// A cone run stopped at the state cap or the memory limit: a finding the cone leaves
    /// undecided, which no whole-model run takes up.
    pub(super) cone_capped: bool,
    /// A run met the memory limit, maybe only because the runs beside it held the rest.
    pub(super) crowded: bool,
    /// The states of the reduced cone, when its plain run explored all of them: the size of
    /// the TLC job's plain module, for the TLC scheduler (#384).
    pub(super) counted: Option<u64>,
}

impl Native {
    pub(super) fn line(&self, module: &str, checks: &[String]) -> String {
        run_line(
            module,
            checks,
            self.decision.as_str(),
            self.states,
            &self.notes,
        )
    }

    /// The cone leaves the property to the whole model.
    pub(super) fn goes_whole(&self) -> bool {
        matches!(self.decision, Decision::Undecided { .. }) && !self.cone_capped
    }
}

/// One run's answer on a graph.
enum Answer {
    Verdict(Outcome, Option<Box<StateDiag>>),
    /// The run stopped at a cap or the budget before it found a counterexample.
    Stopped,
    Fault(LassoFault),
}

impl Answer {
    /// `properties()[i]` of `run`'s system on its graph: an `always` by its states, an
    /// `eventually` or `leads_to` by its fair components under `fairness`.
    pub(super) fn of(run: &ComponentRun, i: usize, fairness: Fairness) -> Answer {
        let verdict = match run.system.properties()[i].formula.kind().is_liveness() {
            true => run.decide_liveness(i, fairness),
            false => Ok(run.decide_property(i).expect("an always")),
        };
        match verdict {
            Ok(PropertyVerdict::Holds) => Answer::Verdict(Outcome::Ok, None),
            Ok(PropertyVerdict::Violated(d)) => Answer::Verdict(Outcome::Counterexample, Some(d)),
            Ok(PropertyVerdict::Undecided) => Answer::Stopped,
            Err(fault) => Answer::Fault(fault),
        }
    }

    /// `properties()[i]`, an `eventually` or `leads_to`, of `run`'s system on its stored
    /// graph under `fairness` (`OrbitRun::decide_liveness`).
    fn of_orbits(run: &OrbitRun, i: usize, fairness: Fairness) -> Answer {
        match run.decide_liveness(i, fairness) {
            Ok(PropertyVerdict::Holds) => Answer::Verdict(Outcome::Ok, None),
            Ok(PropertyVerdict::Violated(d)) => Answer::Verdict(Outcome::Counterexample, Some(d)),
            Ok(PropertyVerdict::Undecided) => Answer::Stopped,
            Err(fault) => Answer::Fault(fault),
        }
    }

    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Answer::Verdict(outcome, _) => outcome.as_str(),
            Answer::Stopped => "not decided",
            Answer::Fault(_) => "tool_error",
        }
    }

    /// What the run decided, `None` when it stopped first.
    pub(super) fn outcome(&self) -> Option<Outcome> {
        match self {
            Answer::Verdict(outcome, _) => Some(*outcome),
            Answer::Stopped => None,
            Answer::Fault(_) => Some(Outcome::ToolError),
        }
    }
}

/// Explores `system` under `caps`, and marks `crowded` when the run met the memory limit.
fn explore(system: &TransitionSystem, caps: Caps, crowded_run: &mut bool) -> ComponentRun {
    let run = system.clone().run(caps);
    *crowded_run |= crowded(&run);
    run
}

/// What the `Plain` run of a property's reduced cone says: read off a run of
/// `p.cone.reduced`, the property's own or another pass's run of the same system (#209).
pub(super) struct Plain {
    vacuous: Option<Diag>,
    states: u64,
    answer: Answer,
    /// The run with `WF_vars(Next)` on the reduced graph, when the plain one asks for it.
    reduced_wf: Option<ReducedVerdict>,
    crowded: bool,
    /// The run explored every state of the reduced cone.
    complete: bool,
}

impl Plain {
    /// What `run`, a complete or stopped run of `p.cone.reduced` under the verify caps,
    /// says of `p`.
    pub(super) fn of(ts: &TransitionSystem, p: &PropertyJob, run: &ComponentRun) -> Plain {
        let cone = &p.cone;
        rdra_ish_core::shadow_assert!(
            run.system == cone.reduced,
            "a property's plain answer is read off a run of its reduced cone"
        );
        let answer = Answer::of(run, cone.reduced_index, Fairness::Plain);
        // A liveness counterexample on the reduced cone asks for the run with `WF_vars(Next)`.
        // It runs first on the reduced graph, with every class that can loop taken as a place
        // to stop: its `ok` is the unreduced cone's, and a counterexample that lifts and
        // certifies on the unreduced cone is real. Anything else falls back to the unreduced
        // cone (`native_property_after`).
        let reduced_wf = match (&answer, &p.wf) {
            (Answer::Verdict(Outcome::Counterexample, _), Some(_))
                if cone.is_reduced()
                    && cone.after_plain(Outcome::Counterexample) == AfterPlain::NeedsWfNext
                    && std::env::var_os("RDRA_ISH_NO_WF_REDUCED").is_none() =>
            {
                Some(run.decide_liveness_reduced(cone.reduced_index, &cone.system, cone.index))
            }
            _ => None,
        };
        if let Some(verdict) = &reduced_wf {
            wf_reduced_stats(ts, p, &cone.reduced, verdict);
        }
        Plain {
            vacuous: ts.property_vacuity(p.index, run),
            states: run.states.len() as u64,
            answer,
            reduced_wf,
            crowded: crowded(run),
            complete: run.end() == RunEnd::Complete,
        }
    }

    /// The states of the reduced cone, when the run explored all of them.
    pub(super) fn counted(&self) -> Option<u64> {
        self.complete.then_some(self.states)
    }
}

/// `p` on its cone by the runs `property_cone` defines, each on a graph BFS explores: `Plain`
/// on the reduced cone, then, when `after_plain` asks for `WfNext`, `WfNext` on the same
/// reduced graph with the classes that can loop marked. When that does not decide, `Full` on
/// the unreduced cone after the counterexample, and `WfNext` on the same unreduced graph.
/// Also returns the vacuity the plain run shows.
///
/// With `orbits`, an `always` on a cone the sub-lifecycle reduction leaves whole runs up to
/// instance symmetry instead (`always_up_to_symmetry`).
fn native_property(
    ts: &TransitionSystem,
    p: &PropertyJob,
    caps: Caps,
    orbits: bool,
) -> (Native, Option<Diag>) {
    if orbits {
        return match p.cone.kind.is_liveness() {
            true => liveness_up_to_symmetry(ts, p, caps),
            false => always_up_to_symmetry(ts, p, caps),
        };
    }
    let plain = Plain::of(ts, p, &p.cone.reduced.clone().run(caps));
    native_property_after(p, caps, plain)
}

/// `native_property` from what its `Plain` run said, once that run is dropped.
fn native_property_after(p: &PropertyJob, caps: Caps, plain: Plain) -> (Native, Option<Diag>) {
    let cone = &p.cone;
    let mut out = Native {
        decision: Decision::Decided(Outcome::Ok),
        states: Some(plain.states),
        notes: Vec::new(),
        runs: Vec::new(),
        found: None,
        fault: None,
        cone_capped: false,
        crowded: false,
        counted: plain.counted(),
    };
    let mut crowded_run = plain.crowded;
    let vacuous = plain.vacuous;
    let mut last = plain.answer;
    out.runs.push((p.run_on(ConeRun::Plain), last.outcome()));
    let reduced_wf = plain.reduced_wf;
    let reduced_wf = match reduced_wf {
        Some(ReducedVerdict::Holds) => Some((Outcome::Ok, None)),
        Some(ReducedVerdict::Violated(d)) => Some((Outcome::Counterexample, Some(d))),
        Some(ReducedVerdict::Undecided | ReducedVerdict::Spurious(_)) | None => None,
    };
    if let Some((second, found)) = reduced_wf {
        out.crowded = crowded_run;
        out.runs.push((p.reduced_wf(), Some(second)));
        out.notes.push(format!(
            "WF_vars(Next) on the reduced cone (native only): {}",
            second.as_str()
        ));
        out.found = found;
        return (
            out.decided(cone.after_wf(Outcome::Counterexample, second)),
            vacuous,
        );
    }
    let mut unreduced: Option<ComponentRun> = None;
    // A counterexample on the reduced cone is real, but its lasso is read on the unreduced
    // cone, whose run gives the verdict from here on.
    if let (Answer::Verdict(Outcome::Counterexample, _), Some(full)) = (&last, &p.full) {
        let run = explore(&cone.system, caps, &mut crowded_run);
        let again = Answer::of(&run, cone.index, Fairness::Plain);
        unreduced = Some(run);
        out.runs.push((p.run_on(ConeRun::Full), again.outcome()));
        out.notes
            .push(format!("{}: {}", full.module, again.as_str()));
        last = match again {
            Answer::Verdict(Outcome::Ok, _) => {
                out.fault =
                    Some("the reduced cone finds a counterexample the cone does not".to_string());
                Answer::Verdict(Outcome::ToolError, None)
            }
            other => other,
        };
    }
    out.crowded = crowded_run;
    let (plain, found) = match last {
        Answer::Verdict(outcome, found) => (outcome, found),
        Answer::Stopped => return (out.capped(), vacuous),
        Answer::Fault(fault) => return (out.faulty(&fault), vacuous),
    };
    out.found = found;
    let verdict = match cone.after_plain(plain) {
        AfterPlain::Decided(verdict) => verdict,
        AfterPlain::NeedsWfNext => {
            let wf =
                p.wf.as_ref()
                    .expect("an eventually or leads_to has a WF module");
            let run = match unreduced {
                Some(run) => run,
                None => explore(&cone.system, caps, &mut crowded_run),
            };
            out.crowded = crowded_run;
            let second = Answer::of(&run, cone.index, Fairness::WfNext);
            out.runs.push((p.run_on(ConeRun::WfNext), second.outcome()));
            out.notes
                .push(format!("{}: {}", wf.module, second.as_str()));
            match second {
                Answer::Verdict(outcome, found) => {
                    out.found = found.or(out.found.take());
                    cone.after_wf(plain, outcome)
                }
                Answer::Stopped => return (out.capped(), vacuous),
                Answer::Fault(fault) => return (out.faulty(&fault), vacuous),
            }
        }
    };
    (out.decided(verdict), vacuous)
}

/// `p`, an `always`, on its cone explored up to instance symmetry: the first stored class
/// that breaks it, its counterexample lifted to concrete states.
fn always_up_to_symmetry(
    ts: &TransitionSystem,
    p: &PropertyJob,
    caps: Caps,
) -> (Native, Option<Diag>) {
    let cone = &p.cone;
    let mut out = Native {
        decision: Decision::Decided(Outcome::Ok),
        states: None,
        notes: Vec::new(),
        runs: Vec::new(),
        found: None,
        fault: None,
        cone_capped: false,
        crowded: false,
        counted: None,
    };
    let run = cone.reduced.clone().run_orbits(caps);
    out.crowded = run
        .diags()
        .iter()
        .any(|d| matches!(d.diag, StateDiag::MemoryCapReached { .. }));
    let vacuous = ts.property_vacuity(p.index, &run);
    super::bfs::symmetry_stats(&ts.properties()[p.index].name, &run);
    out.notes
        .push(format!("about {} states", about(run.estimate())));
    let answer = match run.decide_always(cone.reduced_index) {
        Some(PropertyVerdict::Holds) => Answer::Verdict(Outcome::Ok, None),
        Some(PropertyVerdict::Violated(d)) => Answer::Verdict(Outcome::Counterexample, Some(d)),
        Some(PropertyVerdict::Undecided) => Answer::Stopped,
        None => unreachable!("an always"),
    };
    out.runs.push((p.run_on(ConeRun::Plain), answer.outcome()));
    match answer {
        Answer::Verdict(outcome, found) => {
            out.found = found;
            match cone.after_plain(outcome) {
                AfterPlain::Decided(verdict) => (out.decided(verdict), vacuous),
                AfterPlain::NeedsWfNext => unreachable!("an always needs no WF_vars(Next)"),
            }
        }
        Answer::Stopped => (out.capped(), vacuous),
        Answer::Fault(fault) => (out.faulty(&fault), vacuous),
    }
}

/// `p`, an `eventually` or `leads_to`, on its cone explored up to instance symmetry with
/// each edge's permutation kept (#212 phase 4), decided per binding on (stored state, binding)
/// pairs: the `Plain` run, and on a cone the sub-lifecycle reduction leaves whole the `WfNext`
/// run on the same graph when the `Plain` one finds a counterexample, as `native_property`
/// runs them. On a reduced cone, a `Plain` counterexample asks for the `WF_vars(Next)` run on
/// the reduced graph or for the unreduced cone (`native_property_after`), which this phase
/// does not run up to symmetry: the property is decided plainly instead, and that run must
/// find a counterexample too.
fn liveness_up_to_symmetry(
    ts: &TransitionSystem,
    p: &PropertyJob,
    caps: Caps,
) -> (Native, Option<Diag>) {
    let cone = &p.cone;
    let mut out = Native {
        decision: Decision::Decided(Outcome::Ok),
        states: None,
        notes: Vec::new(),
        runs: Vec::new(),
        found: None,
        fault: None,
        cone_capped: false,
        crowded: false,
        counted: None,
    };
    let run = cone.reduced.clone().run_annotated_orbits(caps);
    out.crowded = run
        .diags()
        .iter()
        .any(|d| matches!(d.diag, StateDiag::MemoryCapReached { .. }));
    let vacuous = ts.property_vacuity(p.index, &run);
    super::bfs::symmetry_stats(&ts.properties()[p.index].name, &run);
    out.notes
        .push(format!("about {} states", about(run.estimate())));
    let first = Answer::of_orbits(&run, cone.reduced_index, Fairness::Plain);
    out.runs.push((p.run_on(ConeRun::Plain), first.outcome()));
    let found = match first {
        Answer::Verdict(Outcome::Counterexample, found) => found,
        Answer::Verdict(outcome, _) => {
            return match cone.after_plain(outcome) {
                AfterPlain::Decided(verdict) => (out.decided(verdict), vacuous),
                AfterPlain::NeedsWfNext => unreachable!("only a counterexample asks for WfNext"),
            };
        }
        Answer::Stopped => return (out.capped(), vacuous),
        Answer::Fault(fault) => return (out.faulty(&fault), vacuous),
    };
    if cone.is_reduced() {
        drop(run);
        let (plain, vacuous) = native_property(ts, p, caps, false);
        if plain.runs.first().and_then(|(_, o)| *o) == Some(Outcome::Ok) {
            let mut plain = plain;
            plain.fault = Some(
                "the run up to symmetry finds a counterexample the plain run does not".to_string(),
            );
            plain.decision = Decision::Decided(Outcome::ToolError);
            plain.found = None;
            return (plain, vacuous);
        }
        return (plain, vacuous);
    }
    let wf =
        p.wf.as_ref()
            .expect("an eventually or leads_to has a WF module");
    let second = Answer::of_orbits(&run, cone.reduced_index, Fairness::WfNext);
    out.runs.push((p.run_on(ConeRun::WfNext), second.outcome()));
    out.notes
        .push(format!("{}: {}", wf.module, second.as_str()));
    match second {
        Answer::Verdict(outcome, again) => {
            out.found = again.or(found);
            let verdict = cone.after_wf(Outcome::Counterexample, outcome);
            (out.decided(verdict), vacuous)
        }
        Answer::Stopped => (out.capped(), vacuous),
        Answer::Fault(fault) => (out.faulty(&fault), vacuous),
    }
}

/// Under `RDRA_ISH_WF_REDUCED_STATS`, one line on stderr per run with `WF_vars(Next)` on a
/// reduced cone: what it decided, and the divergence marks of the reduction, for measuring
/// the trial of docs/formal-verification.md, "WF_vars(Next) on a reduced cone".
fn wf_reduced_stats(
    ts: &TransitionSystem,
    p: &PropertyJob,
    reduced: &TransitionSystem,
    verdict: &ReducedVerdict,
) {
    if std::env::var_os("RDRA_ISH_WF_REDUCED_STATS").is_none() {
        return;
    }
    let outcome = match verdict {
        ReducedVerdict::Holds => "ok".to_string(),
        ReducedVerdict::Undecided => "undecided".to_string(),
        ReducedVerdict::Violated(_) => "counterexample".to_string(),
        ReducedVerdict::Spurious(why) => format!("spurious-{why:?}"),
    };
    let d = reduced
        .reduction()
        .map(|r| r.divergence)
        .unwrap_or_default();
    let sensitive = d.sensitive.map_or("-".to_string(), |n| n.to_string());
    eprintln!(
        "wf-reduced: {} {outcome} classes={} looping={} mixed={} sensitive={sensitive}",
        ts.properties()[p.index].name,
        d.classes,
        d.looping,
        d.mixed
    );
}

impl Native {
    /// The decision `verdict` states; a counterexample's lasso is kept only for one.
    fn decided(mut self, verdict: Verdict) -> Native {
        self.decision = match verdict {
            Verdict::Ok => Decision::Decided(Outcome::Ok),
            Verdict::Counterexample => Decision::Decided(Outcome::Counterexample),
            Verdict::ToolError => Decision::Decided(Outcome::ToolError),
            Verdict::Undecided(why) => {
                self.notes.push(why.as_str().to_string());
                Decision::Undecided {
                    why: why.as_str().to_string(),
                    stop: None,
                }
            }
        };
        if self.decision.verdict() != Some(Outcome::Counterexample) {
            self.found = None;
        }
        self
    }

    /// A cone run stopped at the state cap or the memory limit before it decided.
    pub(super) fn capped(mut self) -> Native {
        let why = "stopped at the state cap (--max-states) or the memory limit".to_string();
        self.notes.push(why.clone());
        self.decision = Decision::Undecided { why, stop: None };
        self.cone_capped = true;
        self.found = None;
        self
    }

    /// A counterexample that failed its own check.
    fn faulty(mut self, fault: &LassoFault) -> Native {
        self.fault = Some(format!(
            "the counterexample fails its own check ({fault:?})"
        ));
        self.decision = Decision::Decided(Outcome::ToolError);
        self.found = None;
        self
    }
}

/// `native_property` for every property job, side by side, `None` for a safety cone. A
/// property whose plain answer `shared` holds, read off an earlier pass's run of its reduced
/// cone (#209), goes on from there. A property whose runs met the memory limit beside others
/// is decided again alone. Also returns the vacuous properties. Under `--symmetry always` an
/// `always` on a cone the reduction leaves whole runs up to instance symmetry when `shared`
/// holds no plain answer for it (`Shared::up_to_symmetry`).
pub(super) fn native_properties(
    ts: &TransitionSystem,
    jobs: &[Job],
    caps: Caps,
    shared: &mut Shared,
) -> (Vec<Option<Native>>, Vec<Diag>) {
    let orbits: Vec<bool> = (0..jobs.len())
        .map(|k| shared.up_to_symmetry(jobs, k))
        .collect();
    let at: Vec<(usize, Option<Plain>)> = (0..jobs.len())
        .filter(|&k| matches!(jobs[k].kind, JobKind::PropertyCone(_)))
        .map(|k| (k, shared.plain[k].take()))
        .collect();
    let job = |k: usize| match &jobs[k].kind {
        JobKind::PropertyCone(p) => p.as_ref(),
        JobKind::Cone(_) | JobKind::Explored(_) | JobKind::Whole(_) => {
            unreachable!("a property's job")
        }
    };
    let decide = |(k, plain): (usize, Option<Plain>)| match plain {
        Some(plain) => native_property_after(job(k), caps, plain),
        None => native_property(ts, job(k), caps, orbits[k]),
    };
    let ks: Vec<usize> = at.iter().map(|(k, _)| *k).collect();
    let ran = in_parallel(at, rdra_ish_core::jobs(), decide);
    let mut out: Vec<Option<Native>> = jobs.iter().map(|_| None).collect();
    let mut vacuous = Vec::new();
    for (k, (native, vacuity)) in ks.into_iter().zip(ran) {
        let (native, vacuity) = match native.crowded {
            true => {
                drop((native, vacuity));
                decide((k, None))
            }
            false => (native, vacuity),
        };
        vacuous.extend(vacuity);
        out[k] = Some(native);
    }
    (out, vacuous)
}

/// The properties `open` names, which their cones leave undecided, on the whole model: one
/// exploration under the budget and the whole-model state cap, and each property decided on
/// its graph with `WF_vars(Next)`. A run that stopped decides a property only by a
/// counterexample, which is real on the states it reached, as TLC's verdict printed before a
/// kill is.
pub(super) fn native_wholes(
    ts: &TransitionSystem,
    open: &[usize],
    caps: Caps,
    budget: WholeRun,
) -> Vec<Native> {
    if open.is_empty() {
        return Vec::new();
    }
    let run = ts.clone().run(budget.caps(caps));
    let states = run.states.len() as u64;
    let stop = run.diags.iter().find_map(|d| match &d.diag {
        StateDiag::StateCapReached { cap, .. } => Some((
            Stop::StateCap,
            format!("stopped at the state cap of {cap} states"),
        )),
        StateDiag::MemoryCapReached { limit, .. } => Some((
            Stop::Memory,
            format!("stopped at the memory limit of {limit}"),
        )),
        StateDiag::TimeBudgetReached { .. } => Some((
            Stop::Budget,
            format!(
                "stopped at the time budget of {} s",
                budget.budget.as_secs()
            ),
        )),
        _ => None,
    });
    open.iter()
        .map(|&i| {
            let mut native = Native {
                decision: Decision::Decided(Outcome::Ok),
                states: (states > 0).then_some(states),
                notes: Vec::new(),
                runs: Vec::new(),
                found: None,
                fault: None,
                cone_capped: false,
                crowded: false,
                counted: None,
            };
            match Answer::of(&run, i, Fairness::WfNext) {
                Answer::Verdict(outcome, found) => {
                    native.decision = Decision::Decided(outcome);
                    native.found = found;
                }
                Answer::Stopped => {
                    let (stop, why) = stop.clone().expect("a run that stopped says why");
                    native.notes.push(why.clone());
                    native.decision = Decision::Undecided {
                        why,
                        stop: Some(stop),
                    };
                }
                Answer::Fault(fault) => native = native.faulty(&fault),
            }
            native
        })
        .collect()
}

/// What the native checker decided: per job, a safety cone's verdict or a property's on its
/// cone, and per property the cone left open, its whole-model verdict.
pub(super) struct Natives {
    pub(super) safety: Vec<Option<BfsCone>>,
    pub(super) properties: Vec<Option<Native>>,
    pub(super) wholes: Vec<(usize, Native)>,
}

impl Natives {
    /// The whole-model verdict of property `i`, when its cone left it open.
    pub(super) fn whole(&self, i: usize) -> Option<&Native> {
        self.wholes.iter().find(|(at, _)| *at == i).map(|(_, n)| n)
    }

    /// The properties the whole-model run leaves undecided, one line each.
    pub(super) fn undecided_lines(&self, names: &TlaNames) -> Vec<String> {
        self.wholes
            .iter()
            .filter_map(|(i, native)| {
                let Decision::Undecided {
                    why,
                    stop: Some(stop),
                } = &native.decision
                else {
                    return None;
                };
                let states = native
                    .states
                    .map_or(String::new(), |n| format!(" after {n} states"));
                Some(format!(
                    "  {}: the whole-model run {why}{states}; {}",
                    names.property(*i),
                    undecided_hint(*stop)
                ))
            })
            .collect()
    }
}
