//! Vision rule 6: a use case whose writes span k transaction groups is k actions, and the
//! checker explores the states in between.

use super::*;
use crate::lifecycle::{ActionOrigin, GuardWhy};

/// Pay updates Order and Payment through one API each, so it spans two groups. `{extra}`
/// completes the model.
fn split_pay(extra: &str) -> String {
    format!(
        r#"
entity Order "注文" {{ id: Int @pk  status: Enum(pending, paid) @default(pending)  rush: Bool }}
entity Payment "決済" {{ id: Int @pk  status: Enum(pending, captured) @default(pending) }}
entity Desk "窓口" {{ id: Int @pk  open: Bool }}
relate(Payment, Order, N:1)
usecase Place "注文確定"
usecase Pay "決済確定"
usecase Staff "開局"
api CapturePaymentApi "Capture"
api PayOrderApi "Pay order"
creates(Place, Order)
creates(Place, Payment)
creates(Staff, Desk)
invokes(Pay, CapturePaymentApi)
updates(CapturePaymentApi, Payment)
invokes(Pay, PayOrderApi)
updates(PayOrderApi, Order)
transitions(Order.status, Pay, pending -> paid)
transitions(Payment.status, Pay, pending -> captured)
{extra}
"#
    )
}

fn built(src: &str) -> TransitionSystem {
    TransitionSystem::build(&model_from(src), Bounds::STATES, &Scope::ALL).ts
}

fn action_names(ts: &TransitionSystem) -> Vec<String> {
    ts.actions().iter().map(|a| a.name.clone()).collect()
}

/// The entity ids an action binds, by name.
fn bound(ts: &TransitionSystem, name: &str) -> Vec<String> {
    let act = ts.actions().iter().find(|a| a.name == name).unwrap();
    act.params
        .iter()
        .map(|p| ts.entity(p.entity).id.clone())
        .collect()
}

#[test]
fn a_use_case_spanning_two_groups_is_one_action_per_group_in_tx_order() {
    let model = model_from(&split_pay(""));
    let ts = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).ts;
    assert_eq!(action_names(&ts), vec!["Pay_1", "Pay_2", "Place", "Staff"]);
    assert_eq!(bound(&ts, "Pay_1"), vec!["Payment"]);
    assert_eq!(bound(&ts, "Pay_2"), vec!["Order"]);
    // Place creates both through one FK-connected group: one atomic action.
    assert_eq!(bound(&ts, "Place"), vec!["Order", "Payment"]);
    let (pay, _) = model.use_cases.iter().find(|(_, u)| u.id == "Pay").unwrap();
    for name in ["Pay_1", "Pay_2"] {
        let act = ts.actions().iter().find(|a| a.name == name).unwrap();
        assert!(matches!(act.origin, ActionOrigin::UseCase { uc, .. } if uc == pay));
    }
}

#[test]
fn provenance_names_the_use_case_not_the_group_step() {
    let d = derive_src(&split_pay(""));
    let via: Vec<String> = d
        .entity("Order")
        .patterns
        .iter()
        .flat_map(|p| p.provenance.via.iter().map(|(_, uc)| uc.clone()))
        .collect();
    assert!(via.contains(&"Pay".to_string()), "{via:?}");
    assert!(via.iter().all(|uc| !uc.starts_with("Pay_")), "{via:?}");
}

#[test]
fn after_assert_is_checked_after_each_group_step() {
    let src = split_pay("after(Pay).assert(Order.status == paid, Payment.status == captured)");
    let ts = built(&src);
    let [a] = ts.after_asserts() else {
        panic!("one assert")
    };
    let steps: Vec<String> = a
        .steps
        .iter()
        .map(|s| ts.actions()[s.act.index()].name.clone())
        .collect();
    assert_eq!(steps, vec!["Pay_1", "Pay_2"]);
    // Each step binds its own group's entity; the other group's is universally bound.
    let binders: Vec<Vec<String>> = a
        .steps
        .iter()
        .map(|s| s.binders.iter().map(|e| ts.entity(*e).id.clone()).collect())
        .collect();
    assert_eq!(binders, vec![vec!["Order"], vec!["Payment"]]);
    // After Pay_1 alone the Order is still pending: the assert sees the in-between state.
    let d = derive_src(&src);
    assert!(
        d.diags.iter().any(|x| matches!(
            &x.diag,
            StateDiag::AfterAssertViolated { anchor, .. } if anchor == "Pay"
        )),
        "{:?}",
        d.diags
    );
}

#[test]
fn a_condition_guards_the_group_whose_entity_it_reads() {
    let conditions = |body: &str| {
        split_pay(&format!(
            "condition Ready \"可\"\nholds(Ready, {body})\napplies_to(Ready, Pay)\n\
             usecase Rush \"急ぎ\"\nsets(Rush, Order, rush == true)\nsets(Staff, Desk, open == true)"
        ))
    };
    let guarded = |ts: &TransitionSystem| -> Vec<String> {
        ts.actions()
            .iter()
            .filter(|a| {
                a.guards
                    .iter()
                    .any(|g| matches!(g.why, GuardWhy::Condition { .. }))
            })
            .map(|a| a.name.clone())
            .collect()
    };
    assert_eq!(
        guarded(&built(&conditions("Order.rush == true"))),
        vec!["Pay_2"]
    );
    // A body on an entity no group writes is a precondition of the first step.
    assert_eq!(
        guarded(&built(&conditions("Desk.open == true"))),
        vec!["Pay_1"]
    );
}
