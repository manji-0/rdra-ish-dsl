use super::bfs::{crowded, BfsCone};
use super::jobs::{Job, JobKind, PropertyJob};
use super::report::run_line;
use super::tlc::{undecided_hint, Decision, Stop};
use super::WholeRun;
use crate::property_cone::{AfterPlain, ConeRun, Verdict};
use crate::tlc_output::Outcome;
use rdra_ish_core::in_parallel;
use rdra_ish_core::lifecycle::{
    Caps, ComponentRun, Diag, Fairness, LassoFault, PropertyVerdict, ReducedVerdict, StateDiag,
    TransitionSystem,
};
use rdra_ish_emit::tla::TlaNames;

/// What the native checker decides of one `always`, `eventually`, or `leads_to`, on its cone
/// or on the whole model.
pub(super) struct Native {
    pub(super) decision: Decision,
    /// The states of the run the line reports: the plain cone's, or the whole model's.
    pub(super) states: Option<u64>,
    pub(super) notes: Vec<String>,
    /// Each cone run and what it said, for `--with-tlc` to compare.
    pub(super) runs: Vec<(ConeRun, Outcome)>,
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

    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Answer::Verdict(outcome, _) => outcome.as_str(),
            Answer::Stopped => "not decided",
            Answer::Fault(_) => "tool_error",
        }
    }

    pub(super) fn outcome(&self) -> Outcome {
        match self {
            Answer::Verdict(outcome, _) => *outcome,
            Answer::Stopped => Outcome::Ok,
            Answer::Fault(_) => Outcome::ToolError,
        }
    }
}

/// Explores `system` under `caps`, and marks `crowded` when the run met the memory limit.
fn explore(system: &TransitionSystem, caps: Caps, crowded_run: &mut bool) -> ComponentRun {
    let run = system.clone().run(caps);
    *crowded_run |= crowded(&run);
    run
}

/// `p` on its cone by the runs `property_cone` defines, each on a graph BFS explores: `Plain`
/// on the reduced cone, then, when `after_plain` asks for `WfNext`, `WfNext` on the same
/// reduced graph with the classes that can loop marked. When that does not decide, `Full` on
/// the unreduced cone after the counterexample, and `WfNext` on the same unreduced graph.
/// Also returns the vacuity the plain run shows.
fn native_property(ts: &TransitionSystem, p: &PropertyJob, caps: Caps) -> (Native, Option<Diag>) {
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
    };
    let mut crowded_run = false;
    let plain_run = explore(&cone.reduced, caps, &mut crowded_run);
    let vacuous = ts.property_vacuity(p.index, &plain_run);
    out.states = Some(plain_run.states.len() as u64);
    let mut last = Answer::of(&plain_run, cone.reduced_index, Fairness::Plain);
    out.runs.push((ConeRun::Plain, last.outcome()));
    // A liveness counterexample on the reduced cone asks for the run with `WF_vars(Next)`.
    // It runs first on the reduced graph, with every class that can loop taken as a place
    // to stop: its `ok` is the unreduced cone's, and a counterexample that lifts and
    // certifies on the unreduced cone is real. Anything else falls back to the unreduced
    // cone below.
    let reduced_wf = match (&last, &p.wf) {
        (Answer::Verdict(Outcome::Counterexample, _), Some(_))
            if cone.is_reduced()
                && cone.after_plain(Outcome::Counterexample) == AfterPlain::NeedsWfNext
                && std::env::var_os("RDRA_ISH_NO_WF_REDUCED").is_none() =>
        {
            Some(plain_run.decide_liveness_reduced(cone.reduced_index, &cone.system, cone.index))
        }
        _ => None,
    };
    if let Some(verdict) = &reduced_wf {
        wf_reduced_stats(ts, p, &cone.reduced, verdict);
    }
    drop(plain_run);
    let reduced_wf = match reduced_wf {
        Some(ReducedVerdict::Holds) => Some((Outcome::Ok, None)),
        Some(ReducedVerdict::Violated(d)) => Some((Outcome::Counterexample, Some(d))),
        Some(ReducedVerdict::Undecided | ReducedVerdict::Spurious(_)) | None => None,
    };
    if let Some((second, found)) = reduced_wf {
        let wf = p.wf.as_ref().expect("checked above");
        out.crowded = crowded_run;
        out.runs.push((ConeRun::WfNext, second));
        out.notes.push(format!(
            "{}: {} on the reduced cone",
            wf.module,
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
        out.runs.push((ConeRun::Full, again.outcome()));
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
            out.runs.push((ConeRun::WfNext, second.outcome()));
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
        let why = "stopped at the state or pattern cap (--max-states, --max-patterns) or the memory limit".to_string();
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
/// property whose runs met the memory limit beside others is decided again alone. Also
/// returns the vacuous properties.
pub(super) fn native_properties(
    ts: &TransitionSystem,
    jobs: &[Job],
    caps: Caps,
) -> (Vec<Option<Native>>, Vec<Diag>) {
    let at: Vec<usize> = (0..jobs.len())
        .filter(|&k| matches!(jobs[k].kind, JobKind::PropertyCone(_)))
        .collect();
    let decide = |k: usize| match &jobs[k].kind {
        JobKind::PropertyCone(p) => native_property(ts, p, caps),
        JobKind::Cone(_) | JobKind::Whole(_) => unreachable!("a property's job"),
    };
    let ran = in_parallel(at.clone(), rdra_ish_core::jobs(), decide);
    let mut out: Vec<Option<Native>> = jobs.iter().map(|_| None).collect();
    let mut vacuous = Vec::new();
    for (k, (native, vacuity)) in at.into_iter().zip(ran) {
        let (native, vacuity) = match native.crowded {
            true => decide(k),
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
