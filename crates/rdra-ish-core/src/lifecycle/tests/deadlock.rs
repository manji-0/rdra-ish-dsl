//! A present instance at a state with a way out, from which no path moves it, is a deadlock:
//! an error, or a warning when only the instance bound blocks the way out.

use super::cone::{load, SAMPLES};
use super::witness::final_values;
use super::*;
use crate::lifecycle::{Blocked, GuardWhy, ModelRef, ParamRole, Severity, Target};

const DEADLOCK: &str = "samples/formal-verification/deadlock.rdra";

fn deadlocks(diags: &[Diag]) -> Vec<&StateDiag> {
    diags
        .iter()
        .map(|d| &d.diag)
        .filter(|d| matches!(d, StateDiag::Deadlock { .. }))
        .collect()
}

/// On every sample at `Bounds::STATES`, the deadlocks are the ones the samples explain.
#[test]
fn only_the_explained_samples_deadlock() {
    let mut found = Vec::new();
    for sample in SAMPLES {
        let report =
            super::super::derive_states(&load(sample), &Scope::ALL, Caps::DEFAULT_PATTERNS);
        for d in deadlocks(&report.diags) {
            found.push((*sample, d.name(&[]), d.severity()));
        }
    }
    assert_eq!(
        found,
        [
            (
                DEADLOCK,
                "Deadlock_Order_status_paid".to_string(),
                Severity::Error
            ),
            (
                "samples/formal-verification/unwitnessed_state.rdra",
                "Deadlock_Order_status_shipped".to_string(),
                Severity::Error
            ),
        ]
    );
}

/// Each deadlock's trace ends in a state where the stuck instance holds the stuck state.
#[test]
fn every_deadlock_trace_ends_at_its_state() {
    let mut checked = 0;
    for sample in SAMPLES {
        let report =
            super::super::derive_states(&load(sample), &Scope::ALL, Caps::DEFAULT_PATTERNS);
        for d in deadlocks(&report.diags) {
            let StateDiag::Deadlock {
                column,
                value,
                at,
                trace,
                ..
            } = d
            else {
                unreachable!()
            };
            let held = final_values(trace);
            assert_eq!(
                held[&(at.entity.clone(), at.slot)].get(column),
                Some(value),
                "{sample}: {d}"
            );
            checked += 1;
        }
    }
    assert!(checked > 0);
}

/// The sample's paid Order: one error with its name, a shortest path, and the one way out
/// with the guard that never holds while the Order is stuck.
#[test]
fn a_paid_order_that_ship_never_leaves_is_one_error() {
    let model = sample_model(DEADLOCK);
    let d = derive(&model);
    let found = deadlocks(&d.diags);
    assert_eq!(found.len(), 1, "{found:?}");
    let diag = found[0];
    assert_eq!(diag.severity(), Severity::Error);
    assert_eq!(
        diag.to_string(),
        "deadlock: Order#1 stays at status = paid forever, and paid is not an end state"
    );
    let StateDiag::Deadlock { exits, trace, .. } = diag else {
        unreachable!()
    };
    let steps: Vec<&str> = trace.steps.iter().map(|s| s.action.as_str()).collect();
    assert_eq!(steps, ["Place", "Pay"]);
    assert_eq!(exits.len(), 1, "{exits:?}");
    assert_eq!(exits[0].action, "Ship");
    assert_eq!(exits[0].to.as_deref(), Some("shipped"));
    assert_eq!(exits[0].site.fact, ModelRef::Transition(1));
    let (packed, _) = model
        .conditions
        .iter()
        .find(|(_, c)| c.id == "Packed")
        .unwrap();
    let Blocked::Guards(guards) = &exits[0].why else {
        panic!("{:?}", exits[0].why)
    };
    let whys: Vec<GuardWhy> = guards.iter().map(|(why, _)| *why).collect();
    assert_eq!(whys, [GuardWhy::Condition { cond: packed }]);
    assert!(d.has("Order", |x| matches!(x, StateDiag::Deadlock { .. })));
}

/// RetryPayment leaves awaiting_retry, and it needs a new Payment. At one instance per
/// entity the failed Payment holds the only slot: a warning, since the model cannot fix the
/// bound. At two instances two failed Orders take both slots, and that is a warning too.
pub(super) const RETRY: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, awaiting_retry, paid) @default(pending) }
entity Payment "決済" { id: Int @pk  status: Enum(failed, captured) @default(failed) }
relate(Payment, Order, N:1)
usecase Place "注文確定"
usecase Pay "支払い"
usecase Fail "決済失敗"
usecase RetryPayment "再決済"
creates(Place, Order)
updates(Pay, Order)
updates(Fail, Order)
creates(Fail, Payment)
updates(RetryPayment, Order)
creates(RetryPayment, Payment)
transitions(Order.status, Pay, pending -> paid)
transitions(Order.status, Fail, pending -> awaiting_retry)
transitions(Order.status, RetryPayment, awaiting_retry -> paid)
condition Unsettled "未決済"
holds(Unsettled, Payment.status == failed)
applies_to(Unsettled, RetryPayment)
"#;

#[test]
fn a_way_out_blocked_only_by_the_instance_bound_is_a_warning() {
    let model = model_from(RETRY);
    let report = super::super::derive_states(&model, &Scope::ALL, Caps::DEFAULT_PATTERNS);
    let found = deadlocks(&report.diags);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].severity(), Severity::Warning);
    assert_eq!(
        found[0].to_string(),
        "deadlock at the bound: Order#1 stays at status = awaiting_retry forever; RetryPayment needs a free Payment slot within the instance bound"
    );
    let StateDiag::Deadlock { exits, .. } = found[0] else {
        unreachable!()
    };
    assert_eq!(
        exits[0].why,
        Blocked::NoInstance(vec![("Payment".to_string(), ParamRole::New)])
    );
    assert!(!report.has_errors(), "{:?}", report.diags);

    let built = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL);
    let run = built.ts.explore_targets(&built.ts.targets(), Caps::VERIFY);
    let found = deadlocks(run.verdicts());
    assert!(
        !found.is_empty() && found.iter().all(|d| d.severity() == Severity::Warning),
        "{found:?}"
    );
}

/// Toggle flips the Order's light forever, so every reachable state has a move that changes
/// it: no state is stuck as a whole. The Order still never leaves paid, and that is the
/// deadlock.
#[test]
fn a_loop_that_always_runs_does_not_hide_a_stuck_instance() {
    let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, shipped) @default(pending)
  light: Enum(off, on) @default(off)
}
usecase Place "注文確定"
usecase Pay "支払い"
usecase Ship "発送"
usecase Toggle "点灯切替"
creates(Place, Order)
updates(Pay, Order)
updates(Ship, Order)
updates(Toggle, Order)
transitions(Order.status, Pay, pending -> paid)
transitions(Order.status, Ship, paid -> shipped)
transitions(Order.light, Toggle, off -> on)
transitions(Order.light, Toggle, on -> off)
condition Packed "梱包済み"
holds(Packed, Order.status == pending)
applies_to(Packed, Ship)
"#;
    let model = model_from(src);
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    let ts = &built.ts;
    let run = ts.explore_targets(&ts.targets(), Caps::per_cone(Caps::DEFAULT_PATTERNS));
    let order = ts.local_entity(model.entities.keys().next().unwrap());
    let g = run.cone_of(Target::Entity(order)).run.graph();
    let mut moves = vec![false; g.states.len()];
    for (from, to, _) in g.edges() {
        moves[from as usize] |= from != to;
    }
    assert!(moves.iter().all(|m| *m), "every reachable state has a move");
    let names: Vec<String> = deadlocks(run.verdicts())
        .iter()
        .map(|d| d.name(&[]))
        .collect();
    assert_eq!(names, ["Deadlock_Order_status_paid"]);
}
