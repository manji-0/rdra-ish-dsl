//! Rules that hold vacuously: `StateDiag::VacuousRule`.

use std::collections::BTreeMap;

use super::model_from;
use crate::lifecycle::{Bounds, Caps, Scope, StateDiag, TransitionSystem};

/// The premises no reachable state meets, per rule that holds vacuously.
fn vacuous(src: &str, bounds: Bounds, caps: Caps) -> BTreeMap<String, Vec<String>> {
    let ts = TransitionSystem::build(&model_from(src), bounds, &Scope::ALL).ts;
    ts.explore_targets(&ts.targets(), caps)
        .verdicts()
        .iter()
        .filter_map(|d| match &d.diag {
            StateDiag::VacuousRule { name, unmet, .. } => Some((name.clone(), unmet.clone())),
            _ => None,
        })
        .collect()
}

const ORDERS: &str = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid, void) @default(open)
  flagged: Bool @default(false)
}
usecase Place "注文"
usecase Pay "支払"
usecase Flag "印"
creates(Place, Order)
transitions(Pay, Order.status, open -> paid)
sets(Flag, Order.flagged == true)
"#;

/// No order is ever `void`, so a rule about void orders says nothing: the guards of a
/// `.when` never hold, and a `forbidden` pair names a value no row takes.
#[test]
fn a_rule_about_a_state_no_row_reaches_holds_vacuously() {
    let src = format!(
        "{ORDERS}invariant(Order).when(status == void).then(flagged == true)\n\
         forbidden(Order, status == void, flagged == true)\n"
    );
    let found = vacuous(&src, Bounds::STATES, Caps::STATES);
    let void = vec!["status=void".to_string()];
    assert_eq!(
        found,
        BTreeMap::from([
            ("Forbidden_Order_0".to_string(), void.clone()),
            ("Inv_Order_0".to_string(), void),
        ])
    );
}

/// What a `forbidden` forbids is unreachable when it holds: that is its claim, not a
/// premise. A pair whose values each some row takes, never together, is not vacuous, and
/// neither is a rule of one condition, nor a rule whose guards hold somewhere.
#[test]
fn a_rule_whose_premises_some_state_meets_is_not_vacuous() {
    // Flag needs an open order and Pay an unflagged one, so no paid order is flagged.
    let src = format!(
        "{ORDERS}condition Open \"未払\"\nholds(Open, Order.status == open)\napplies_to(Open, Flag)\n\
         condition Plain \"印なし\"\nholds(Plain, Order.flagged == false)\napplies_to(Plain, Pay)\n\
         forbidden(Order, status == paid, flagged == true)\n\
         forbidden(Order, status == void)\n\
         invariant(Order).when(status == paid).then(flagged == false)\n"
    );
    assert_eq!(vacuous(&src, Bounds::STATES, Caps::STATES), BTreeMap::new());
}

/// A rule over an entity no use case can create ranges over no row at all; that alone is
/// reported, not each of its conditions.
#[test]
fn a_rule_over_rows_that_never_exist_holds_vacuously() {
    let src = format!(
        "{ORDERS}entity Ghost \"幽霊\" {{ id: Int @pk  seen: Bool @default(false)  loud: Bool @default(false) }}\n\
         usecase Summon \"召喚\"\ncreates(Summon, Ghost)\nsets(Summon, Ghost.seen == true)\n\
         condition Never \"起きない\"\nholds(Never, Order.status == void)\napplies_to(Never, Summon)\n\
         forbidden(Ghost, seen == true, loud == true)\n"
    );
    let found = vacuous(&src, Bounds::STATES, Caps::STATES);
    assert_eq!(
        found.get("Forbidden_Ghost_0"),
        Some(&vec![String::new()]),
        "{found:?}"
    );
}

/// A violated rule is not vacuous, and a run a cap stops has not seen every state, so it
/// reports no vacuity.
#[test]
fn a_violated_rule_and_a_capped_run_report_no_vacuity() {
    let violated = format!("{ORDERS}forbidden(Order, status == paid, flagged == true)\n");
    assert_eq!(
        vacuous(&violated, Bounds::STATES, Caps::STATES),
        BTreeMap::new()
    );
    let capped = format!("{ORDERS}forbidden(Order, status == void, flagged == true)\n");
    assert_eq!(
        vacuous(&capped, Bounds::VERIFY, Caps::new(256, 3)),
        BTreeMap::new()
    );
    assert_eq!(
        vacuous(&capped, Bounds::VERIFY, Caps::VERIFY).len(),
        1,
        "the same rule is vacuous once the run completes"
    );
}

/// A rule over two entities meets a condition on one of them with a row of that entity
/// alone. No cancelled order keeps a payment here, since Cancel deletes it, but cancelled
/// orders exist and captured payments exist: the rule claims they never meet.
#[test]
fn a_condition_on_one_entity_needs_no_row_of_the_other() {
    let src = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, cancelled) @default(pending) }
entity Payment "決済" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
usecase Place "注文確定"
usecase Pay "決済確定"
usecase Cancel "キャンセル"
creates(Place, Order)
creates(Place, Payment)
updates(Pay, Order)
updates(Pay, Payment)
updates(Cancel, Order)
deletes(Cancel, Payment)
transitions(Pay, Order.status, pending -> paid)
transitions(Cancel, Order.status, pending -> cancelled)
transitions(Pay, Payment.status, pending -> captured)
relate(Payment, Order, N:1)
forbidden(Order, Payment, Order.status == cancelled, Payment.status == captured).along(Order, Payment)
"#;
    assert_eq!(vacuous(src, Bounds::STATES, Caps::STATES), BTreeMap::new());
}
