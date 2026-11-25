//! A temporal property checked on its cone (`TransitionSystem::property_cone`), and what the
//! cone's TLC runs decide.
//!
//! The cone keeps what the property reads and each action of its `.fair` use cases whole, so
//! weak fairness on those actions restricted to the cone is weak fairness on them in the whole
//! model, and every other action outside the cone stutters for it. The whole model's
//! `WF_vars(Next)` does not restrict to the cone: a step outside the cone satisfies it. So an
//! `eventually` or `leads_to` property takes up to two runs on the cone:
//!
//! - `Plain` has no `WF_vars(Next)`, only the property's own `Fair_P`. Every fair behavior of
//!   the whole model projects to a behavior it admits, so its `ok` is the whole model's.
//! - `WfNext` adds `WF_vars(Next)` on the cone. Its counterexample lifts to the whole model:
//!   a loop keeps stepping, and a stutter has every cone action disabled, which the whole
//!   model's actions outside the cone cannot change. Its `ok` decides only when the cone
//!   holds every var an action writes, so that every step of the whole model changes the
//!   cone; otherwise the property needs the whole model's `WF_vars(Next)`.
//!
//! An `always` property needs the `Plain` run only. A cone that is not exact (`is_exact`: it
//! frees an entity's slots, or runs a handler without its event) reaches a superset, so only
//! its `ok` counts, and `PropertyCone::settle` decides the rest on wider cones.
//!
//! The `Plain` run is on the cone reduced by its independent sub-lifecycle
//! (`TransitionSystem::reduced`), which is stuttering equivalent to the cone with respect to
//! what the property and its fair actions read, and `Spec` admits stuttering in that run, so
//! its `ok` is the cone's. A counterexample there is real, but the reduced cone's lasso does
//! not lift step by step, so the `Full` run repeats the `Plain` run on the unreduced cone and
//! the verdict reads from it. The `WfNext` run reads the reduced cone first, with every class
//! that can loop inside itself taken as a place to stop, which keeps its `ok` sound; when that
//! does not decide, it runs on the unreduced cone, where `WF_vars(Next)` keeps its meaning
//! (docs/formal-verification.md, "WF_vars(Next) on a reduced cone").

use std::sync::atomic::{AtomicUsize, Ordering};

use rdra_ish_core::lifecycle::{
    Caps, ComponentRun, Cone, Fairness, LassoFault, Loop, LtlKind, PropertyVerdict, QuotientCache,
    StateDiag, TransitionSystem, VarSet,
};
use rdra_ish_emit::tla::{self, PrintMode, TlaBundle};

use crate::tlc_output::Outcome;

pub struct PropertyCone {
    /// The property's index in the whole system.
    pub at: usize,
    /// The cone's vars in the whole system.
    pub vars: VarSet,
    /// The cone as a system; `index` is the property's index in it.
    pub system: TransitionSystem,
    pub index: usize,
    /// The cone reduced for the `Plain` run, and the property's index in it; `system` and
    /// `index` again when nothing reduces.
    pub reduced: TransitionSystem,
    pub reduced_index: usize,
    pub kind: LtlKind,
    /// The cone is not exact (`is_exact`): an action of it creates an entity the cone does
    /// not count, or runs without the event whose mark the cone leaves out, so a
    /// counterexample may take a step the whole model does not.
    pub inexact: bool,
    /// The cone holds every var an action writes, so every step of the whole model changes
    /// the cone and `WF_vars(Next)` on the cone is the whole model's.
    pub whole: bool,
}

/// One TLC run on the cone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConeRun {
    /// The property alone, on the reduced cone.
    Plain,
    /// The property alone, on the unreduced cone, after a `Plain` counterexample.
    Full,
    WfNext,
}

/// What the cone's runs so far decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    Counterexample,
    /// The cone cannot decide: the whole model must run.
    Undecided(Why),
    ToolError,
}

/// What the `Plain` run alone decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterPlain {
    Decided(Verdict),
    /// The `Plain` run found a counterexample the cone cannot vouch for; run `WfNext`.
    NeedsWfNext,
}

/// Why a cone leaves a property undecided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// A cone counterexample needs `WF_vars(Next)` of the whole model to rule out.
    WfNext,
    /// The counterexample may take a step the whole model does not: create instances past
    /// the bound, or run a handler without its event.
    Inexact,
    /// A run of a widened cone (`PropertyCone::settle`) stopped at a cap or the budget.
    Stopped,
}

impl Why {
    pub fn as_str(self) -> &'static str {
        match self {
            Why::WfNext => "needs the whole model's WF_vars(Next)",
            Why::Inexact => {
                "its counterexample may create instances past the bound or run a handler without its event"
            }
            Why::Stopped => {
                "the widened cone stopped at the state cap, the memory limit, or the time budget"
            }
        }
    }
}

/// The step that decided a property its cone left undecided (`PropertyCone::settle`;
/// docs/design/native-temporal-verification.md, "Modes and the cone").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The cone made exact (`TransitionSystem::exact_cone`) decided it by its own runs.
    Exact,
    /// Step 4: the exact cone's `WF_vars(Next)` run said `ok`, and every var an action writes
    /// outside it is monotone (`cyclic_outside` is empty), so nothing outside runs forever.
    Monotone,
    /// Step 5: a var set outside the exact cone loops on its own (`independent_loop`), so the
    /// `Plain` counterexample's stop lasts forever in the whole model.
    Independent,
    /// Step 6: the exact cone widened over every var outside it that can change forever
    /// (`loop_cone`) decided it with `WF_vars(Next)`; nothing outside that cone runs forever.
    Loops,
}

impl Step {
    const ALL: [Step; 4] = [Step::Exact, Step::Monotone, Step::Independent, Step::Loops];
}

/// Where a run of a widened cone stopped before it decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stopped {
    /// At the state cap of this many states.
    StateCap(usize),
    /// At the memory limit.
    Memory,
    /// At the time budget (`--temporal-budget`).
    Budget,
}

/// How often each step decided a property in this process, for measuring how often each one
/// fires (`fired`).
static FIRED: [AtomicUsize; Step::ALL.len()] = [const { AtomicUsize::new(0) }; Step::ALL.len()];

/// How often `step` decided a property in this process (`PropertyCone::settle`).
pub fn fired(step: Step) -> usize {
    FIRED[step as usize].load(Ordering::Relaxed)
}

/// What `PropertyCone::settle` decided of a property its cone left undecided.
#[derive(Debug)]
pub struct Settled {
    /// `Ok` or `Counterexample` when a step decided; `Undecided(WfNext)` when the property is
    /// still the whole model's; `Undecided(Stopped)` when a widened run stopped; `ToolError`
    /// with `fault`.
    pub verdict: Verdict,
    /// The step that decided, with `Ok` or `Counterexample`.
    pub step: Option<Step>,
    /// The counterexample, on the cone the step read.
    pub found: Option<Box<StateDiag>>,
    pub fault: Option<LassoFault>,
    /// What the run line adds: which cone decided, and why.
    pub notes: Vec<String>,
    /// A widened run met the memory limit, maybe only because the runs beside it held the
    /// rest.
    pub crowded: bool,
    /// With `Undecided(Stopped)`: where the run stopped, and how many states it had.
    pub stop: Option<(Stopped, usize)>,
}

impl Settled {
    fn open(notes: Vec<String>) -> Settled {
        Settled {
            verdict: Verdict::Undecided(Why::WfNext),
            step: None,
            found: None,
            fault: None,
            notes,
            crowded: false,
            stop: None,
        }
    }

    fn decided(mut self, step: Step, found: Option<Box<StateDiag>>) -> Settled {
        self.verdict = match found {
            Some(_) => Verdict::Counterexample,
            None => Verdict::Ok,
        };
        self.step = Some(step);
        self.found = found;
        FIRED[step as usize].fetch_add(1, Ordering::Relaxed);
        self
    }

    /// `run` stopped before it decided.
    fn stopped(mut self, run: &ComponentRun) -> Settled {
        self.verdict = Verdict::Undecided(Why::Stopped);
        let at = run.diags.iter().find_map(|d| match &d.diag {
            StateDiag::StateCapReached { cap, .. } => Some(Stopped::StateCap(*cap)),
            StateDiag::MemoryCapReached { .. } => Some(Stopped::Memory),
            StateDiag::TimeBudgetReached { .. } => Some(Stopped::Budget),
            _ => None,
        });
        self.stop = Some((at.unwrap_or(Stopped::Memory), run.states.len()));
        self
    }

    fn faulty(mut self, fault: LassoFault) -> Settled {
        self.verdict = Verdict::ToolError;
        self.fault = Some(fault);
        self
    }
}

/// `properties()[i]` of `run`'s system on its graph: an `eventually` or `leads_to` by its fair
/// components under `fairness`, any other form by its states or steps.
fn decide(run: &ComponentRun, i: usize, fairness: Fairness) -> Result<PropertyVerdict, LassoFault> {
    match run.system.properties()[i].formula.kind().is_liveness() {
        true => run.decide_liveness(i, fairness),
        false => Ok(run
            .decide_property(i)
            .expect("a form BFS decides on its states or steps")),
    }
}

/// The run met the memory limit.
fn met_memory_limit(run: &ComponentRun) -> bool {
    run.diags
        .iter()
        .any(|d| matches!(d.diag, StateDiag::MemoryCapReached { .. }))
}

/// `cone` of `ts` as a system, and the index of `ts.properties()[at]` in it.
fn cone_system(ts: &TransitionSystem, cone: &Cone, at: usize) -> (TransitionSystem, usize) {
    let system = ts.restrict_to(&cone.vars, &cone.keep);
    let name = &ts.properties()[at].name;
    let index = system
        .properties()
        .iter()
        .position(|p| p.name == *name)
        .expect("a widened cone holds what the property reads");
    (system, index)
}

impl PropertyCone {
    pub fn new(ts: &TransitionSystem, i: usize) -> Self {
        Self::under(ts, i, Caps::VERIFY, &QuotientCache::new())
    }

    /// `new`, with the run that reduces the cone under `caps` and the quotient shared
    /// through `quotients`. Under `RDRA_ISH_WHOLE_CONE` the cone is the whole model, so each
    /// property is decided there: the oracle of `scripts/reduction-diff.py run --whole`.
    pub fn under(ts: &TransitionSystem, i: usize, caps: Caps, quotients: &QuotientCache) -> Self {
        let cone = match std::env::var_os("RDRA_ISH_WHOLE_CONE") {
            Some(_) => {
                let (_, keep) = ts.property_seed(i);
                ts.exact_cone(&ts.var_ids().collect(), &keep)
            }
            None => ts.property_cone(i),
        };
        let system = ts.restrict_to(&cone.vars, &cone.keep);
        let name = &ts.properties()[i].name;
        let index = system
            .properties()
            .iter()
            .position(|p| p.name == *name)
            .expect("the cone holds what the property reads");
        let inexact = !ts.is_exact(&cone);
        let whole = ts.cone_keeps_every_write(&cone.vars);
        // A create writes the created entity's existence and a raise its mark, so a cone that
        // holds every write is exact, and the `WfNext` run's `ok` needs no caveat.
        debug_assert!(!whole || !inexact);
        let reduced = ts.reduced_cone_in(&cone, [ts.property_seed(i)], caps, quotients);
        let reduced_index = reduced
            .properties()
            .iter()
            .position(|p| p.name == *name)
            .expect("the reduced cone keeps the property it is for");
        PropertyCone {
            at: i,
            vars: cone.vars,
            inexact,
            whole,
            kind: ts.properties()[i].formula.kind(),
            system,
            index,
            reduced,
            reduced_index,
        }
    }

    /// The `Plain` run is on a reduced cone, so a counterexample there asks for `Full`.
    pub fn is_reduced(&self) -> bool {
        self.reduced.reduction().is_some()
    }

    pub fn bundle(&self, run: ConeRun, module: &str) -> TlaBundle {
        match run {
            ConeRun::Plain => tla::print(
                &self.reduced,
                PrintMode::ConeProperty(self.reduced_index),
                module,
            ),
            ConeRun::Full => tla::print(&self.system, PrintMode::ConeProperty(self.index), module),
            ConeRun::WfNext => tla::print(&self.system, PrintMode::Property(self.index), module),
        }
    }

    /// The verdict after the `Plain` run alone, or that the `WfNext` run must answer.
    pub fn after_plain(&self, plain: Outcome) -> AfterPlain {
        after_plain(self.kind, self.inexact, plain)
    }

    /// The verdict after the `Plain` run asked for the `WfNext` run and it ran.
    pub fn after_wf(&self, plain: Outcome, wf_next: Outcome) -> Verdict {
        after_wf(self.inexact, self.whole, plain, wf_next)
    }

    /// The property, which the runs on this cone left undecided, decided on wider cones of
    /// `ts` under `caps` (docs/design/native-temporal-verification.md, "Modes and the cone").
    ///
    /// The cone is first made exact (`exact_cone`): no step of it creates a row the whole
    /// model has no slot for. Its reachable states are then the projection of the whole
    /// model's, so its `Plain` run's `ok` and any counterexample of an `always`, or of the run
    /// with `WF_vars(Next)`, are the whole model's. A `WF_vars(Next)` `ok` there is final when
    /// the exact cone holds every var an action writes. What is left is a `Plain`
    /// counterexample whose every stop has a step of the cone enabled, and an `ok` with
    /// `WF_vars(Next)`, both on the exact cone, which the steps after it decide (`beyond`).
    pub fn settle(&self, ts: &TransitionSystem, caps: Caps) -> Settled {
        let (seed, keep) = ts.property_seed(self.at);
        let exact = ts.exact_cone(&seed, &keep);
        debug_assert!(exact.vars.is_superset(&self.vars), "widening only adds");
        debug_assert!(ts.is_exact(&exact));
        // The cone was exact already: its own runs said what the exact cone's would.
        if exact.vars == self.vars {
            return self.beyond(ts, &exact, caps, None, Settled::open(Vec::new()));
        }
        let (system, i) = cone_system(ts, &exact, self.at);
        let run = system.run(caps);
        let mut out = Settled::open(vec![format!(
            "on the cone made exact ({} more vars, {} states)",
            exact.vars.len() - self.vars.len(),
            run.states.len()
        )]);
        out.crowded = met_memory_limit(&run);
        let liveness = self.kind.is_liveness();
        match decide(&run, i, Fairness::Plain) {
            Err(fault) => return out.faulty(fault),
            Ok(PropertyVerdict::Undecided) => return out.stopped(&run),
            Ok(PropertyVerdict::Holds) => return out.decided(Step::Exact, None),
            Ok(PropertyVerdict::Violated(d)) if !liveness => {
                return out.decided(Step::Exact, Some(d))
            }
            Ok(PropertyVerdict::Violated(_)) => {}
        }
        match decide(&run, i, Fairness::WfNext) {
            Err(fault) => out.faulty(fault),
            Ok(PropertyVerdict::Undecided) => out.stopped(&run),
            Ok(PropertyVerdict::Violated(d)) => out.decided(Step::Exact, Some(d)),
            Ok(PropertyVerdict::Holds) if ts.cone_keeps_every_write(&exact.vars) => {
                out.decided(Step::Exact, None)
            }
            Ok(PropertyVerdict::Holds) => self.beyond(ts, &exact, caps, Some((run, i)), out),
        }
    }

    /// Steps 4 to 6 on `exact`, an exact cone where the `Plain` run found a counterexample
    /// and the run with `WF_vars(Next)` found none: a behavior of the whole model that
    /// violates the property must, from some point on, change only vars outside the cone.
    ///
    /// Step 4: when every var an action writes outside the cone is monotone, each changes
    /// finitely often, so no behavior changes only outside vars forever. Every fair behavior
    /// of the whole model then keeps moving the cone or ends with no step enabled, and both
    /// project to behaviors the run with `WF_vars(Next)` on the exact cone covers: its `ok`
    /// is the whole model's.
    ///
    /// Step 5: when a var set outside the cone loops on its own (`independent_loop`), the
    /// whole model can reach the `Plain` run's stop and then run that loop forever. The cone
    /// stays at the stop, where no fair instance is enabled, and the loop meets
    /// `WF_vars(Next)`: the `Plain` counterexample stands. `run` is the exact cone's run when
    /// `settle` made one; otherwise the exact cone is the property's cone, explored here for
    /// the lasso.
    ///
    /// Step 6: otherwise the exact cone is widened over every var outside it that can change
    /// forever (`loop_cone`), and the run with `WF_vars(Next)` there decides. The widened cone
    /// may be the whole model; it runs under the same caps and budget.
    fn beyond(
        &self,
        ts: &TransitionSystem,
        exact: &Cone,
        caps: Caps,
        mut run: Option<(ComponentRun, usize)>,
        mut out: Settled,
    ) -> Settled {
        if ts.cyclic_outside(&exact.vars).is_empty() {
            out.notes
                .push("nothing outside the cone runs forever".to_string());
            return out.decided(Step::Monotone, None);
        }
        if let Some(entity) = ts.independent_loop(exact, caps) {
            let (run, i) = match run.take() {
                Some(run) => run,
                None => {
                    let (system, i) = cone_system(ts, exact, self.at);
                    (system.run(caps), i)
                }
            };
            out.crowded |= met_memory_limit(&run);
            // The `Plain` counterexample is a stop: a fair cycle would have taken steps
            // forever, and the run with `WF_vars(Next)` would have found it.
            match decide(&run, i, Fairness::Plain) {
                Ok(PropertyVerdict::Violated(d))
                    if matches!(
                        *d,
                        StateDiag::PropertyViolated {
                            lasso: Some(Loop::Stutter),
                            ..
                        }
                    ) =>
                {
                    out.notes
                        .push(format!("{entity} runs forever outside the cone"));
                    return out.decided(Step::Independent, Some(d));
                }
                Err(fault) => return out.faulty(fault),
                Ok(PropertyVerdict::Undecided) => return out.stopped(&run),
                Ok(_) => {}
            }
        }
        // Step 6: on the cone widened over what can change forever outside it, everything
        // outside is monotone, so step 4's argument makes its run with `WF_vars(Next)` exact:
        // its `ok` and its counterexample are the whole model's.
        // The exact cone's run is not read again: its memory goes back before the wider run.
        drop(run);
        let wide = ts.loop_cone(exact);
        let (system, i) = cone_system(ts, &wide, self.at);
        let run = system.run(caps);
        out.crowded |= met_memory_limit(&run);
        out.notes.push(format!(
            "on the cone widened by what changes outside it ({} more vars, {} states)",
            wide.vars.len() - exact.vars.len(),
            run.states.len()
        ));
        match decide(&run, i, Fairness::WfNext) {
            Err(fault) => out.faulty(fault),
            Ok(PropertyVerdict::Undecided) => out.stopped(&run),
            Ok(PropertyVerdict::Holds) => out.decided(Step::Loops, None),
            Ok(PropertyVerdict::Violated(d)) => out.decided(Step::Loops, Some(d)),
        }
    }
}

fn counterexample(inexact: bool) -> Verdict {
    match inexact {
        true => Verdict::Undecided(Why::Inexact),
        false => Verdict::Counterexample,
    }
}

fn after_plain(kind: LtlKind, inexact: bool, plain: Outcome) -> AfterPlain {
    match plain {
        Outcome::Ok => AfterPlain::Decided(Verdict::Ok),
        Outcome::ToolError => AfterPlain::Decided(Verdict::ToolError),
        Outcome::Counterexample if !kind.is_liveness() => {
            AfterPlain::Decided(counterexample(inexact))
        }
        Outcome::Counterexample => AfterPlain::NeedsWfNext,
    }
}

/// Only reached for a `plain` counterexample on a property that is not `Always`.
fn after_wf(inexact: bool, whole: bool, plain: Outcome, wf_next: Outcome) -> Verdict {
    match (plain, wf_next) {
        (Outcome::Ok, _) => Verdict::Ok,
        (Outcome::ToolError, _) | (_, Outcome::ToolError) => Verdict::ToolError,
        (Outcome::Counterexample, Outcome::Counterexample) => counterexample(inexact),
        (Outcome::Counterexample, Outcome::Ok) if whole => Verdict::Ok,
        (Outcome::Counterexample, Outcome::Ok) => Verdict::Undecided(Why::WfNext),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use AfterPlain::{Decided, NeedsWfNext};

    #[test]
    fn a_plain_ok_decides_and_a_counterexample_asks_for_the_wf_run() {
        let plain = |o| after_plain(LtlKind::LeadsTo, false, o);
        let wf = |o| after_wf(false, false, Outcome::Counterexample, o);
        assert_eq!(plain(Outcome::Ok), Decided(Verdict::Ok));
        assert_eq!(plain(Outcome::Counterexample), NeedsWfNext);
        assert_eq!(wf(Outcome::Counterexample), Verdict::Counterexample);
        assert_eq!(wf(Outcome::Ok), Verdict::Undecided(Why::WfNext));
        assert_eq!(plain(Outcome::ToolError), Decided(Verdict::ToolError));
        assert_eq!(wf(Outcome::ToolError), Verdict::ToolError);
    }

    #[test]
    fn the_wf_run_decides_when_the_cone_is_the_whole_model() {
        assert_eq!(
            after_wf(false, true, Outcome::Counterexample, Outcome::Ok),
            Verdict::Ok
        );
    }

    #[test]
    fn always_needs_one_run_and_freed_slots_keep_only_an_ok() {
        assert_eq!(
            after_plain(LtlKind::Always, false, Outcome::Counterexample),
            Decided(Verdict::Counterexample)
        );
        assert_eq!(
            after_plain(LtlKind::Always, true, Outcome::Ok),
            Decided(Verdict::Ok)
        );
        assert_eq!(
            after_plain(LtlKind::Always, true, Outcome::Counterexample),
            Decided(Verdict::Undecided(Why::Inexact))
        );
        assert_eq!(
            after_wf(
                true,
                false,
                Outcome::Counterexample,
                Outcome::Counterexample
            ),
            Verdict::Undecided(Why::Inexact)
        );
    }
}
