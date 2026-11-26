//! A time budget (`Caps::until`) on the runs `check` and `states` make: a cone that passes it
//! leaves what it reads undecided, as a cone stopped at a cap does. The language server runs
//! `check` in the background under such a budget (#214).

use std::time::{Duration, Instant};

use super::cone::load;
use super::*;
use crate::lifecycle::{derive_states_checked, CheckOutcome, UndecidedWhy};

const UNMET: &str = "samples/formal-verification/can_reach_unmet.rdra";

/// A budget already spent stops every cone at its first state: each state of a state model,
/// each use case, and each `can_reach` is undecided, and nothing is reported as decided.
#[test]
fn a_spent_budget_leaves_every_finding_undecided() {
    let model = load(UNMET);
    let caps = Caps::for_instances(1).until(Instant::now());
    let report = super::super::derive_states(&model, &Scope::ALL, caps);
    assert!(!report.undecided.is_empty());
    assert!(
        report
            .undecided
            .iter()
            .all(|u| u.why == UndecidedWhy::TimeBudget),
        "{:?}",
        report.undecided
    );
    assert!(
        !report.diags.iter().any(|d| matches!(
            d.diag,
            StateDiag::NeverRuns { .. } | StateDiag::NoWitness { .. } | StateDiag::Deadlock { .. }
        )),
        "{:?}",
        report.diags
    );

    let ts = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).ts;
    let run = ts.explore_targets(&ts.reach_targets(), caps);
    assert!(run.verdicts().is_empty(), "{:?}", run.verdicts());
    assert!(run
        .undecided()
        .iter()
        .all(|u| u.why == UndecidedWhy::TimeBudget));
}

/// Each invariant gets one outcome: `ok` on a complete cone, `violated` with a
/// counterexample, and `not decided` on a cone the budget stopped.
#[test]
fn each_rule_has_an_outcome() {
    let model = model_from(
        r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(draft, paid) @default(draft)
  paid_at: Enum(none, set) @default(none)
}
usecase Pay "支払う"
transitions(Pay, Order.status, draft -> paid)
invariant(Order).when(status == paid).then(paid_at == set).label("支払済には支払日時")
invariant(Order).when(status == draft).then(paid_at == none).label("下書きに支払日時はない")
"#,
    );
    let outcomes = |caps: Caps| -> Vec<CheckOutcome> {
        derive_states_checked(&model, &Scope::ALL, Bounds::STATES, caps)
            .checks
            .iter()
            .map(|c| c.outcome)
            .collect()
    };
    let roomy = Caps::for_instances(1).until(Instant::now() + Duration::from_secs(3600));
    assert_eq!(
        outcomes(roomy),
        [CheckOutcome::Violated, CheckOutcome::Holds]
    );
    let spent = Caps::for_instances(1).until(Instant::now());
    assert_eq!(
        outcomes(spent),
        [CheckOutcome::Undecided, CheckOutcome::Undecided]
    );
}
