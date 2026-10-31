//! States read a comparison the way the analysis resolved it: a value, or a column.

use super::*;
use crate::lifecycle::InvKind;

const ORDER: &str = r#"
entity Order "注文" {
  id: Int @pk
  paid: Bool
  status: Enum(pending, paid, cancelled) @default(pending)
  previous: Enum(pending, paid, cancelled) @default(pending)
}
usecase Place "注文"
usecase Pay "支払"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
"#;

fn violated(d: &Derived, kind: InvKind) -> bool {
    d.has(
        "Order",
        |x| matches!(x, StateDiag::ConstraintViolated { kind: k, .. } if *k == kind),
    )
}

fn unsupported(d: &Derived) -> bool {
    d.has("Order", |x| {
        matches!(x, StateDiag::UnsupportedComparison { .. })
    })
}

#[test]
fn a_value_named_like_a_column_is_checked_as_the_value() {
    // `Pay` reaches status = paid with paid = false.
    let d = derive_src(&format!(
        "{ORDER}\nforbidden(Order, status == paid, paid == false)"
    ));
    assert!(!unsupported(&d));
    assert!(violated(&d, InvKind::Forbidden));
}

#[test]
fn not_equal_to_a_value_is_checked() {
    let d = derive_src(&format!("{ORDER}\nforbidden(Order, status != pending)"));
    assert!(!unsupported(&d));
    assert!(violated(&d, InvKind::Forbidden));
    let d = derive_src(&format!(
        "{ORDER}\nforbidden(Order, status != cancelled, status != paid, status != pending)"
    ));
    assert!(!violated(&d, InvKind::Forbidden));
}

#[test]
fn a_qualified_rhs_compares_two_enum_columns() {
    // `previous` stays pending, so `Pay` makes the two columns differ.
    let d = derive_src(&format!(
        "{ORDER}\nforbidden(Order, status != Order.previous)"
    ));
    assert!(!unsupported(&d));
    assert!(violated(&d, InvKind::Forbidden));
}

#[test]
fn a_check_on_an_enum_value_is_checked() {
    let d = derive_src(
        r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid) @default(pending)
  paid: Bool @default(false) @check(status == pending or paid == true)
}
usecase Place "注文"
usecase Pay "支払"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
"#,
    );
    assert!(!unsupported(&d));
    assert!(d.has("Order", |x| matches!(
        x,
        StateDiag::ConstraintViolated { .. }
    )));
}
