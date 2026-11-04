//! A temporal property checked on its cone (`TransitionSystem::property_cone`), and what the
//! cone's TLC runs decide. `tests/engine_equivalence.rs` includes this file by path beside
//! `tlc_output.rs`, so it stays free of the other CLI modules.
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
//! the verdict reads from it; the `WfNext` run is on the unreduced cone, where `WF_vars(Next)`
//! keeps its meaning.

use rdra_ish_core::lifecycle::{Caps, LtlKind, TransitionSystem};
use rdra_ish_emit::tla::{self, PrintMode, TlaBundle};

use crate::tlc_output::Outcome;

pub struct PropertyCone {
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
    /// The `Plain` run found a counterexample the cone cannot vouch for; run `WfNext`.
    NeedsWfNext,
    /// The cone cannot decide: the whole model must run.
    Undecided(Why),
    ToolError,
}

/// Why a cone leaves a property undecided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// A cone counterexample needs `WF_vars(Next)` of the whole model to rule out.
    WfNext,
    /// The counterexample may create instances past the bound.
    FreesSlots,
}

impl Why {
    pub fn as_str(self) -> &'static str {
        match self {
            Why::WfNext => "needs the whole model's WF_vars(Next)",
            Why::FreesSlots => "its counterexample may create instances past the bound",
        }
    }
}

impl PropertyCone {
    pub fn new(ts: &TransitionSystem, i: usize) -> Self {
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
        let reduced = ts.reduced_cone(&cone, [ts.property_seed(i)], Caps::VERIFY);
        let reduced_index = reduced
            .properties()
            .iter()
            .position(|p| p.name == *name)
            .expect("the reduced cone keeps the property it is for");
        PropertyCone {
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

    /// The verdict after the `Plain` run and, when it ran, the `WfNext` run.
    pub fn verdict(&self, plain: Outcome, wf_next: Option<Outcome>) -> Verdict {
        verdict(self.kind, self.frees_slots, self.whole, plain, wf_next)
    }
}

fn verdict(
    kind: LtlKind,
    frees_slots: bool,
    whole: bool,
    plain: Outcome,
    wf_next: Option<Outcome>,
) -> Verdict {
    let counterexample = match frees_slots {
        true => Verdict::Undecided(Why::FreesSlots),
        false => Verdict::Counterexample,
    };
    match (plain, wf_next) {
        (Outcome::Ok, _) => Verdict::Ok,
        (Outcome::ToolError, _) | (_, Some(Outcome::ToolError)) => Verdict::ToolError,
        (Outcome::Counterexample, _) if kind == LtlKind::Always => counterexample,
        (Outcome::Counterexample, None) => Verdict::NeedsWfNext,
        (Outcome::Counterexample, Some(Outcome::Counterexample)) => counterexample,
        (Outcome::Counterexample, Some(Outcome::Ok)) if whole => Verdict::Ok,
        (Outcome::Counterexample, Some(Outcome::Ok)) => Verdict::Undecided(Why::WfNext),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_ok_decides_and_a_counterexample_asks_for_the_wf_run() {
        let v = |plain, wf| verdict(LtlKind::LeadsTo, false, false, plain, wf);
        assert_eq!(v(Outcome::Ok, None), Verdict::Ok);
        assert_eq!(v(Outcome::Counterexample, None), Verdict::NeedsWfNext);
        assert_eq!(
            v(Outcome::Counterexample, Some(Outcome::Counterexample)),
            Verdict::Counterexample
        );
        assert_eq!(
            v(Outcome::Counterexample, Some(Outcome::Ok)),
            Verdict::Undecided(Why::WfNext)
        );
        assert_eq!(v(Outcome::ToolError, None), Verdict::ToolError);
        assert_eq!(
            v(Outcome::Counterexample, Some(Outcome::ToolError)),
            Verdict::ToolError
        );
    }

    #[test]
    fn the_wf_run_decides_when_the_cone_is_the_whole_model() {
        assert_eq!(
            verdict(
                LtlKind::Eventually,
                false,
                true,
                Outcome::Counterexample,
                Some(Outcome::Ok)
            ),
            Verdict::Ok
        );
    }

    #[test]
    fn always_needs_one_run_and_freed_slots_keep_only_an_ok() {
        assert_eq!(
            verdict(LtlKind::Always, false, false, Outcome::Counterexample, None),
            Verdict::Counterexample
        );
        assert_eq!(
            verdict(LtlKind::Always, true, false, Outcome::Ok, None),
            Verdict::Ok
        );
        assert_eq!(
            verdict(LtlKind::Always, true, false, Outcome::Counterexample, None),
            Verdict::Undecided(Why::FreesSlots)
        );
        assert_eq!(
            verdict(
                LtlKind::LeadsTo,
                true,
                false,
                Outcome::Counterexample,
                Some(Outcome::Counterexample)
            ),
            Verdict::Undecided(Why::FreesSlots)
        );
    }
}
