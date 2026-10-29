//! An action that labels no edge of any entity cone that keeps it never runs: a warning that
//! names only what blocks it, once per action.

use super::cone::{load, SAMPLES};
use super::*;
use crate::lifecycle::{Blocked, GuardWhy, ModelRef, ParamRole, Severity, Target};

/// The `NeverRuns` of a run, by action name.
fn never_runs(diags: &[Diag]) -> Vec<(&str, &Blocked)> {
    diags
        .iter()
        .filter_map(|d| match &d.diag {
            StateDiag::NeverRuns { action, why, .. } => Some((action.as_str(), why)),
            _ => None,
        })
        .collect()
}

/// On every sample at `Bounds::STATES`, the only actions that never run are the three the
/// samples explain: `Ship` in `deadlock` and `Refund` (their guards never hold together),
/// and `AddLine`, whose `Order` exists only with its one `OrderLine` slot taken at one
/// instance per entity.
#[test]
fn only_the_explained_sample_actions_never_run() {
    let mut found = Vec::new();
    for sample in SAMPLES {
        let report =
            super::super::derive_states(&load(sample), &Scope::ALL, Caps::DEFAULT_PATTERNS);
        for (action, why) in never_runs(&report.diags) {
            found.push((*sample, action.to_string(), why.clone()));
        }
    }
    assert_eq!(
        found,
        [
            (
                "samples/formal-verification/deadlock.rdra",
                "Ship".to_string(),
                Blocked::Together
            ),
            (
                "samples/formal-verification/min_card_met.rdra",
                "AddLine".to_string(),
                Blocked::Apart(vec![
                    ("OrderLine".to_string(), ParamRole::New),
                    ("Order".to_string(), ParamRole::Parent),
                ])
            ),
            (
                "samples/formal-verification/unwitnessed_state.rdra",
                "Refund".to_string(),
                Blocked::Together
            ),
        ]
    );
}

/// Refund's from-state and its window each hold somewhere, never together: one warning, with
/// its stable name, on the one entity it writes.
#[test]
fn refund_never_runs_once_with_its_name() {
    let d = derive(&sample_model(
        "samples/formal-verification/unwitnessed_state.rdra",
    ));
    let runs: Vec<&Diag> = d
        .diags
        .iter()
        .filter(|x| matches!(x.diag, StateDiag::NeverRuns { .. }))
        .collect();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].diag.name(&[]), "Runs_Refund");
    assert_eq!(runs[0].diag.severity(), Severity::Warning);
    assert_eq!(
        runs[0].diag.to_string(),
        "Refund can never run: each of its guards holds in some reachable state, but never all of them in one"
    );
    assert!(d.has("Order", |x| matches!(x, StateDiag::NeverRuns { .. })));
}

/// Pay spans two transaction groups. Only the Order group is blocked, so only `Pay_2` warns.
#[test]
fn each_transaction_group_runs_or_not_on_its_own() {
    let src = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid) @default(pending) }
entity Payment "決済" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
relate(Payment, Order, N:1)
usecase Place "注文確定"
usecase Pay "決済確定"
api CapturePaymentApi "Capture"
api PayOrderApi "Pay order"
creates(Place, Order)
creates(Place, Payment)
invokes(Pay, CapturePaymentApi)
updates(CapturePaymentApi, Payment)
invokes(Pay, PayOrderApi)
updates(PayOrderApi, Order)
transitions(Order.status, Pay, pending -> paid)
transitions(Payment.status, Pay, pending -> captured)
condition Settled "精算済み"
holds(Settled, Order.status == paid)
applies_to(Settled, Pay)
"#;
    let d = derive_src(src);
    let names: Vec<&str> = never_runs(&d.diags).into_iter().map(|(a, _)| a).collect();
    assert_eq!(names, ["Pay_2"]);
}

/// Close writes Order and Payment in one transaction, so both entity cones keep it and both
/// decide it. It is one warning, on both entities' views.
#[test]
fn an_action_two_cones_decide_warns_once() {
    let src = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, closed) @default(pending) }
entity Payment "決済" { id: Int @pk  status: Enum(pending, done) @default(pending) }
relate(Payment, Order, N:1)
usecase Place "注文確定"
usecase Close "締め"
creates(Place, Order)
creates(Place, Payment)
updates(Close, Order)
updates(Close, Payment)
transitions(Order.status, Close, pending -> closed)
transitions(Payment.status, Close, pending -> done)
condition Settled "精算済み"
holds(Settled, Payment.status == done)
applies_to(Settled, Close)
"#;
    let d = derive_src(src);
    let runs: Vec<&Diag> = d
        .diags
        .iter()
        .filter(|x| matches!(x.diag, StateDiag::NeverRuns { .. }))
        .collect();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].diag.name(&[]), "Runs_Close");
    assert_eq!(runs[0].entities.len(), 2, "{:?}", runs[0].entities);
    for entity in ["Order", "Payment"] {
        assert!(
            d.has(entity, |x| matches!(x, StateDiag::NeverRuns { .. })),
            "{entity}"
        );
    }
}

/// An action whose only moves leave the state as it is still runs: a self-loop is an edge.
#[test]
fn a_self_loop_counts_as_running() {
    let src = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid) @default(pending) }
usecase Place "注文確定"
usecase Pay "支払い"
usecase Touch "参照"
creates(Place, Order)
updates(Pay, Order)
updates(Touch, Order)
transitions(Order.status, Pay, pending -> paid)
"#;
    let model = model_from(src);
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    let ts = &built.ts;
    let run = ts.explore_targets(&ts.targets(), Caps::per_cone(Caps::DEFAULT_PATTERNS));
    let order = run.cone_of(Target::Entity(
        ts.local_entity(model.entities.keys().next().unwrap()),
    ));
    let touch = order
        .run
        .system
        .actions()
        .iter()
        .position(|a| a.name == "Touch")
        .unwrap();
    let edges: Vec<(u32, u32)> = order
        .run
        .graph()
        .edges()
        .filter(|(_, _, act)| act.index() == touch)
        .map(|(from, to, _)| (from, to))
        .collect();
    assert!(
        !edges.is_empty() && edges.iter().all(|(from, to)| from == to),
        "{edges:?}"
    );
    assert!(
        never_runs(run.verdicts()).is_empty(),
        "{:?}",
        run.verdicts()
    );
}

/// The why lists only the guards that hold in no reachable state: Refund's from-state
/// `shipped` is reachable, the audit condition never holds.
#[test]
fn the_why_lists_only_guards_that_never_hold() {
    let src = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, shipped, refunded) @default(pending) }
usecase Place "注文確定"
usecase Ship "発送"
usecase Refund "返金"
creates(Place, Order)
updates(Ship, Order)
updates(Refund, Order)
transitions(Order.status, Ship, pending -> shipped)
transitions(Order.status, Refund, shipped -> refunded)
condition Audited "監査済み"
holds(Audited, Order.status == refunded)
applies_to(Audited, Refund)
"#;
    let model = model_from(src);
    let d = derive(&model);
    let runs = never_runs(&d.diags);
    assert_eq!(runs.len(), 1, "{runs:?}");
    let (action, Blocked::Guards(guards)) = runs[0] else {
        panic!("{runs:?}")
    };
    assert_eq!(action, "Refund");
    let (audited, _) = model
        .conditions
        .iter()
        .find(|(_, c)| c.id == "Audited")
        .unwrap();
    assert_eq!(guards.len(), 1, "{guards:?}");
    assert_eq!(guards[0].0, GuardWhy::Condition { cond: audited });
    assert_eq!(guards[0].1.fact, ModelRef::Condition(audited));
    assert!(guards[0].1.span.is_some(), "the guard has a site");
}

/// RetryPayment's guards hold once Fail leaves the Order awaiting_retry, and by then Fail's
/// Payment holds the one slot. The why is that slot, not the from-state guard, which fails
/// only in the states where a slot is still free.
#[test]
fn a_creation_the_bound_crowds_out_names_the_slot_not_a_guard() {
    let model = model_from(super::deadlock::RETRY);
    let d = derive(&model);
    let runs = never_runs(&d.diags);
    assert_eq!(
        runs,
        [(
            "RetryPayment",
            &Blocked::Crowded(vec!["Payment".to_string()])
        )]
    );
    let diag = d
        .diags
        .iter()
        .find(|x| matches!(x.diag, StateDiag::NeverRuns { .. }))
        .unwrap();
    assert_eq!(diag.diag.severity(), Severity::Warning);
    assert_eq!(
        diag.diag.to_string(),
        "RetryPayment can never run: whenever its guards hold, the instance bound leaves no free slot for an instance it creates"
    );
}

/// The same model with a condition on the created Payment that never holds: the bound is not
/// the blocker, the condition is, and it is the one guard listed.
#[test]
fn a_crowded_creation_still_names_a_guard_on_its_successor_that_never_holds() {
    let src = super::deadlock::RETRY.replace(
        "holds(Unsettled, Payment.status == failed)",
        "holds(Unsettled, Payment.status == captured)",
    );
    let model = model_from(&src);
    let d = derive(&model);
    let runs = never_runs(&d.diags);
    let [("RetryPayment", Blocked::Guards(guards))] = runs.as_slice() else {
        panic!("{runs:?}")
    };
    let (unsettled, _) = model
        .conditions
        .iter()
        .find(|(_, c)| c.id == "Unsettled")
        .unwrap();
    let whys: Vec<GuardWhy> = guards.iter().map(|(why, _)| *why).collect();
    assert_eq!(whys, [GuardWhy::Condition { cond: unsettled }]);
}
