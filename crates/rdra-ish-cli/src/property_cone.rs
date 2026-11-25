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
//! An `always` property needs the `Plain` run only. A cone that frees an entity's slots
//! (`cone_frees_slots`) reaches a superset, so only its `ok` counts.
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
    Caps, ComponentRun, Cone, Fairness, LassoFault, LtlKind, PropertyVerdict, QuotientCache,
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
    /// An action of the cone creates an entity the cone does not count, so a counterexample
    /// may create more instances than the bound allows.
    pub frees_slots: bool,
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
    /// The counterexample may create instances past the bound.
    FreesSlots,
    /// A run of a widened cone (`PropertyCone::settle`) stopped at a cap or the budget.
    Stopped,
}

impl Why {
    pub fn as_str(self) -> &'static str {
        match self {
            Why::WfNext => "needs the whole model's WF_vars(Next)",
            Why::FreesSlots => "its counterexample may create instances past the bound",
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
}

impl Step {
    const ALL: [Step; 1] = [Step::Exact];
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

    fn stopped(mut self) -> Settled {
        self.verdict = Verdict::Undecided(Why::Stopped);
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
    /// through `quotients`.
    pub fn under(ts: &TransitionSystem, i: usize, caps: Caps, quotients: &QuotientCache) -> Self {
        let cone = ts.property_cone(i);
        let system = ts.restrict_to(&cone.vars, &cone.keep);
        let name = &ts.properties()[i].name;
        let index = system
            .properties()
            .iter()
            .position(|p| p.name == *name)
            .expect("the cone holds what the property reads");
        let frees_slots = ts.cone_frees_slots(&cone);
        let whole = ts.cone_keeps_every_write(&cone.vars);
        // A create writes the created entity's existence, so a cone that holds every write
        // counts every slot, and the `WfNext` run's `ok` needs no slot caveat.
        debug_assert!(!whole || !frees_slots);
        let reduced = ts.reduced_cone_in(&cone, [ts.property_seed(i)], caps, quotients);
        let reduced_index = reduced
            .properties()
            .iter()
            .position(|p| p.name == *name)
            .expect("the reduced cone keeps the property it is for");
        PropertyCone {
            at: i,
            vars: cone.vars,
            frees_slots,
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
        after_plain(self.kind, self.frees_slots, plain)
    }

    /// The verdict after the `Plain` run asked for the `WfNext` run and it ran.
    pub fn after_wf(&self, plain: Outcome, wf_next: Outcome) -> Verdict {
        after_wf(self.frees_slots, self.whole, plain, wf_next)
    }

    /// The property, which the runs on this cone left undecided, decided on wider cones of
    /// `ts` under `caps` (docs/design/native-temporal-verification.md, "Modes and the cone").
    ///
    /// The cone is first made exact (`exact_cone`): no step of it creates a row the whole
    /// model has no slot for. Its reachable states are then the projection of the whole
    /// model's, so its `Plain` run's `ok` and any counterexample of an `always`, or of the run
    /// with `WF_vars(Next)`, are the whole model's. A `WF_vars(Next)` `ok` there is final when
    /// the exact cone holds every var an action writes; otherwise the property stays the whole
    /// model's.
    pub fn settle(&self, ts: &TransitionSystem, caps: Caps) -> Settled {
        let (seed, keep) = ts.property_seed(self.at);
        let exact = ts.exact_cone(&seed, &keep);
        debug_assert!(exact.vars.is_superset(&self.vars), "widening only adds");
        if exact.vars == self.vars {
            return Settled::open(Vec::new());
        }
        let (system, i) = cone_system(ts, &exact, self.at);
        let run = system.run(caps);
        let mut out = Settled::open(vec![format!(
            "decided on the cone made exact ({} more vars, {} states)",
            exact.vars.len() - self.vars.len(),
            run.states.len()
        )]);
        out.crowded = met_memory_limit(&run);
        let liveness = self.kind.is_liveness();
        match decide(&run, i, Fairness::Plain) {
            Err(fault) => return out.faulty(fault),
            Ok(PropertyVerdict::Undecided) => return out.stopped(),
            Ok(PropertyVerdict::Holds) => return out.decided(Step::Exact, None),
            Ok(PropertyVerdict::Violated(d)) if !liveness => {
                return out.decided(Step::Exact, Some(d))
            }
            Ok(PropertyVerdict::Violated(_)) => {}
        }
        match decide(&run, i, Fairness::WfNext) {
            Err(fault) => out.faulty(fault),
            Ok(PropertyVerdict::Undecided) => out.stopped(),
            Ok(PropertyVerdict::Violated(d)) => out.decided(Step::Exact, Some(d)),
            Ok(PropertyVerdict::Holds) if ts.cone_keeps_every_write(&exact.vars) => {
                out.decided(Step::Exact, None)
            }
            Ok(PropertyVerdict::Holds) => Settled::open(Vec::new()),
        }
    }
}

fn counterexample(frees_slots: bool) -> Verdict {
    match frees_slots {
        true => Verdict::Undecided(Why::FreesSlots),
        false => Verdict::Counterexample,
    }
}

fn after_plain(kind: LtlKind, frees_slots: bool, plain: Outcome) -> AfterPlain {
    match plain {
        Outcome::Ok => AfterPlain::Decided(Verdict::Ok),
        Outcome::ToolError => AfterPlain::Decided(Verdict::ToolError),
        Outcome::Counterexample if !kind.is_liveness() => {
            AfterPlain::Decided(counterexample(frees_slots))
        }
        Outcome::Counterexample => AfterPlain::NeedsWfNext,
    }
}

/// Only reached for a `plain` counterexample on a property that is not `Always`.
fn after_wf(frees_slots: bool, whole: bool, plain: Outcome, wf_next: Outcome) -> Verdict {
    match (plain, wf_next) {
        (Outcome::Ok, _) => Verdict::Ok,
        (Outcome::ToolError, _) | (_, Outcome::ToolError) => Verdict::ToolError,
        (Outcome::Counterexample, Outcome::Counterexample) => counterexample(frees_slots),
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
            Decided(Verdict::Undecided(Why::FreesSlots))
        );
        assert_eq!(
            after_wf(
                true,
                false,
                Outcome::Counterexample,
                Outcome::Counterexample
            ),
            Verdict::Undecided(Why::FreesSlots)
        );
    }
}
