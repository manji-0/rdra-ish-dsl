//! `col in (..)` is a disjunction of `col == v`, `col not in (..)` a conjunction of `col != v`.

use super::*;
use crate::lifecycle::{InvKind, Target};

const ORDER: &str = r#"
entity Order "注文" {
  id: Int @pk
  was_paid: Bool @default(false)
  status: Enum(pending, paid, cancelled) @default(pending)
  qty: Int @default(0)
}
usecase Place "注文"
usecase Pay "支払"
usecase Cancel "取消"
usecase Bump "数量"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
sets(Pay, Order, was_paid == true)
transitions(Cancel, Order.status, pending -> cancelled)
transitions(Cancel, Order.status, paid -> cancelled)
sets(Bump, Order, qty == 2)
"#;

fn violated(src: &str, kind: InvKind) -> bool {
    let d = derive_src(&format!("{ORDER}\n{src}"));
    assert!(
        !d.has("Order", |x| matches!(
            x,
            StateDiag::UnsupportedComparison { .. }
        )),
        "{src}"
    );
    d.has(
        "Order",
        |x| matches!(x, StateDiag::ConstraintViolated { kind: k, .. } if *k == kind),
    )
}

#[test]
fn in_holds_when_one_value_does() {
    assert!(violated(
        "forbidden(Order, status in (paid, cancelled), was_paid == true)",
        InvKind::Forbidden
    ));
    assert!(!violated(
        "forbidden(Order, status in (pending), was_paid == true)",
        InvKind::Forbidden
    ));
    assert!(violated(
        "forbidden(Order, qty in (1, 2))",
        InvKind::Forbidden
    ));
    assert!(!violated(
        "forbidden(Order, qty in (1, 3))",
        InvKind::Forbidden
    ));
}

#[test]
fn not_in_holds_when_no_value_does() {
    assert!(!violated(
        "forbidden(Order, status not in (pending, paid, cancelled))",
        InvKind::Forbidden
    ));
    assert!(violated(
        "forbidden(Order, status not in (pending, paid))",
        InvKind::Forbidden
    ));
    assert!(!violated(
        "invariant(Order).when(was_paid == true).then(status in (paid, cancelled))",
        InvKind::Invariant
    ));
    assert!(violated(
        "invariant(Order).when(was_paid == true).then(status not in (cancelled))",
        InvKind::Invariant
    ));
}

#[test]
fn a_not_in_guard_keeps_a_use_case_out_of_the_listed_states() {
    let rule = "forbidden(Order, status == cancelled, was_paid == true)";
    assert!(violated(rule, InvKind::Forbidden));
    let guard =
        "condition Unpaid \"未払い\"\nholds(Unpaid, Order.status not in (paid))\napplies_to(Unpaid, Cancel)";
    assert!(!violated(&format!("{guard}\n{rule}"), InvKind::Forbidden));
    let guard =
        "condition Open \"未確定\"\nholds(Open, Order.status in (pending))\napplies_to(Open, Cancel)";
    assert!(!violated(&format!("{guard}\n{rule}"), InvKind::Forbidden));
}

/// A set of every value of the column is true: `.none(Invoice.status in (<every value>))` says
/// "no Invoice", so the cone that decides the key's uniqueness holds the link and not the
/// status, and a guard of `not in (<every value>)` never holds.
#[test]
fn a_set_of_every_value_reads_no_column() {
    let model = sample_model("samples/formal-verification/none_any_child.rdra");
    let built = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL);
    let ts = &built.ts;
    let invoice = ts.entities().iter().find(|e| e.id == "Invoice").unwrap();
    let unique = ts
        .invariants()
        .iter()
        .position(|i| i.name == "Unique_Invoice_Order")
        .unwrap();
    let cone = ts.target_cone(Target::Invariant(unique));
    let names: Vec<String> = cone.vars.iter().map(|v| ts.var_name(*v)).collect();
    assert!(
        names.iter().any(|n| n == "Invoice._owner_Order"),
        "{names:?}"
    );
    assert!(
        !cone.vars.contains(&invoice.columns[0]),
        "Invoice.status stays out of {names:?}"
    );

    let d = derive_src(&format!(
        "{ORDER}\ncondition Never \"どの状態でもない\"\nholds(Never, Order.status not in (pending, paid, cancelled))\napplies_to(Never, Bump)"
    ));
    assert!(
        d.diags
            .iter()
            .any(|x| matches!(&x.diag, StateDiag::NeverRuns { action, .. } if action == "Bump")),
        "{:?}",
        d.diags
    );
}
