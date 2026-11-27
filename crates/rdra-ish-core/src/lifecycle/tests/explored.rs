//! `stable`, `unchanged`, and `precedes`, decided by BFS on each property's cone (#166): a
//! verdict, and for a violation the instance, the state that shows it, and the shortest trace
//! to it.

use super::*;
use crate::lifecycle::{LtlKind, PropertyVerdict};

const ORDER: &str = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(draft, approved, cancelled, shipped) @default(draft)
  amount: Int @default(0)
  paid: Bool @default(false)
}
usecase Place "注文"
usecase Approve "承認"
usecase Cancel "取消"
usecase Reopen "再開"
usecase Amend "金額変更"
usecase Refund "返金"
usecase Pay "支払"
usecase Ship "出荷"
creates(Place, Order)
transitions(Approve, Order.status, draft -> approved)
transitions(Cancel, Order.status, draft -> cancelled)
transitions(Reopen, Order.status, cancelled -> draft)
sets(Amend, Order.amount == 2)
sets(Refund, Order.amount == 0)
sets(Pay, Order.paid == true)
transitions(Ship, Order.status, approved -> shipped)
condition Draft "下書き"
holds(Draft, Order.status == draft)
applies_to(Draft, Amend)
condition Cancelled "取消済"
holds(Cancelled, Order.status == cancelled)
applies_to(Cancelled, Refund)
condition Paid "支払済"
holds(Paid, Order.paid == true)
applies_to(Paid, Ship)
property ShippedStays stable(Order.status == shipped)
property CancelledStays stable(Order.status == cancelled)
property AmountKeptApproved unchanged(Order.amount).when(Order.status == approved)
property AmountKeptDecided unchanged(Order.amount).when(Order.status != draft)
property AmountNeverChanges unchanged(Order.amount)
property ShippedAfterApproved precedes(Order.status == approved, Order.status == shipped)
property ShippedAfterPaid precedes(Order.paid == true, Order.status == shipped)
property ShippedAfterAmended precedes(Order.amount == 2, Order.status == shipped)
property ShippedWhenShipped precedes(Order.status == shipped, Order.status == shipped)
"#;

/// The verdict of `properties()[i]`: `None` when it holds, else its kind, instance, witness,
/// and the actions of its trace.
type Violation = (LtlKind, Vec<String>, usize, Vec<String>);

fn decide(ts: &TransitionSystem, name: &str) -> Option<Violation> {
    let i = ts
        .properties()
        .iter()
        .position(|p| p.name == name)
        .unwrap_or_else(|| panic!("no property {name}"));
    match ts.decide_property(i, Caps::VERIFY, Caps::VERIFY) {
        PropertyVerdict::Holds => None,
        PropertyVerdict::Undecided => panic!("{name}: undecided"),
        PropertyVerdict::Fault(fault) => panic!("{name}: a tool fault: {fault:?}"),
        PropertyVerdict::Violated(diag) => match *diag {
            StateDiag::PropertyViolated {
                kind,
                instance,
                witness,
                lasso,
                trace,
                ..
            } => {
                assert_eq!(lasso, None, "{name}: a finite trace has no loop");
                Some((
                    kind,
                    instance.iter().map(ToString::to_string).collect(),
                    witness,
                    trace.steps.iter().map(|s| s.action.clone()).collect(),
                ))
            }
            other => panic!("{name}: {other:?}"),
        },
    }
}

fn strings(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|x| x.to_string()).collect()
}

fn order() -> TransitionSystem {
    TransitionSystem::build(&model_from(ORDER), Bounds::VERIFY, &Scope::ALL).ts
}

#[test]
fn stable_fails_at_the_first_step_that_leaves_it() {
    let ts = order();
    assert_eq!(decide(&ts, "ShippedStays"), None);
    assert_eq!(
        decide(&ts, "CancelledStays"),
        Some((
            LtlKind::Stable,
            strings(&["Order#1"]),
            3,
            strings(&["Place", "Cancel", "Reopen"])
        ))
    );
}

/// `.when` bounds the steps `unchanged` reads; without it every step of a present instance
/// keeps the columns.
#[test]
fn unchanged_fails_at_a_step_that_moves_a_column_under_when() {
    let ts = order();
    assert_eq!(decide(&ts, "AmountKeptApproved"), None);
    assert_eq!(
        decide(&ts, "AmountKeptDecided"),
        Some((
            LtlKind::Unchanged,
            strings(&["Order#1"]),
            4,
            strings(&["Place", "Amend", "Cancel", "Refund"])
        ))
    );
    assert_eq!(
        decide(&ts, "AmountNeverChanges"),
        Some((
            LtlKind::Unchanged,
            strings(&["Order#1"]),
            2,
            strings(&["Place", "Amend"])
        ))
    );
}

/// The latch remembers `p` after it stops holding: a shipped order is no longer approved, and
/// `ShippedAfterApproved` holds. A state where `p` and `q` hold together sets the latch before
/// the property reads it.
#[test]
fn precedes_reads_the_latch_of_every_earlier_state() {
    let ts = order();
    assert_eq!(decide(&ts, "ShippedAfterApproved"), None);
    assert_eq!(decide(&ts, "ShippedAfterPaid"), None);
    assert_eq!(decide(&ts, "ShippedWhenShipped"), None);
    let (kind, instance, witness, steps) =
        decide(&ts, "ShippedAfterAmended").expect("Ship never reads the amount");
    assert_eq!(
        (kind, instance, witness),
        (LtlKind::Precedes, strings(&["Order#1"]), 4)
    );
    assert_eq!(steps.len(), 4, "{steps:?}");
    assert_eq!(steps[0], "Place");
    assert_eq!(steps[3], "Ship");
    assert!(!steps.contains(&"Amend".to_string()), "{steps:?}");
}

const PAYMENT: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(open, shipped) @default(open) }
entity Payment "支払" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
relate(Payment, Order, 1:1)
usecase Capture "確定"
transitions(Capture, Payment.status, pending -> captured)
transitions(Capture, Order.status, open -> shipped)
property ShippedWithItsCapture
  precedes(Payment.status == captured, Order.status == shipped).along(Payment, Order)
property ShippedWithAnyCapture
  precedes(Payment.status == captured, Order.status == shipped).global()
"#;

/// Capture captures a payment and ships the one order it belongs to in one step. Along the
/// key the order ships with its own payment captured; over every pair a shipped order beside
/// another order's pending payment breaks it.
#[test]
fn precedes_along_reads_only_linked_rows() {
    let ts = TransitionSystem::build(&model_from(PAYMENT), Bounds::VERIFY, &Scope::ALL).ts;
    assert_eq!(decide(&ts, "ShippedWithItsCapture"), None);
    let (kind, instance, witness, steps) =
        decide(&ts, "ShippedWithAnyCapture").expect("an unlinked payment stays pending");
    assert_eq!((kind, witness, steps.len()), (LtlKind::Precedes, 1, 1));
    assert_eq!(instance.len(), 2, "{instance:?}");
}

const RECEIPTS: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(a, b, c, d) @default(a) }
entity Receipt "領収書" { id: Int @pk }
relate(Receipt, Order, N:1)
usecase Place "注文"
usecase Bump "進行"
creates(Place, Order)
transitions(Bump, Order.status, a -> b)
transitions(Bump, Order.status, b -> c)
transitions(Bump, Order.status, c -> d)
creates(Bump, Receipt)
property NeverD stable(Order.status != d)
"#;

/// Each Bump creates a receipt the property does not read, so the cone counts no receipt
/// slots and bumps an order to `d`; the whole model runs out of receipts after two bumps. The
/// cone's violation is decided again on the whole model, which holds.
#[test]
fn a_cone_that_frees_slots_decides_a_violation_on_the_whole_model() {
    let ts = TransitionSystem::build(&model_from(RECEIPTS), Bounds::VERIFY, &Scope::ALL).ts;
    let cone = ts.property_cone(0);
    assert!(ts.cone_frees_slots(&cone), "the cone counts no receipt");
    let on_cone = ts
        .restrict_to(&cone.vars, &cone.keep)
        .run(Caps::VERIFY)
        .decide_property(0);
    assert!(
        matches!(on_cone, Some(PropertyVerdict::Violated(_))),
        "{on_cone:?}"
    );
    assert_eq!(decide(&ts, "NeverD"), None);
}

const RELINK: &str = r#"
entity Order "注文" { id: Int @pk }
entity Payment "支払" { id: Int @pk  status: Enum(pending, captured, voided) @default(pending) }
relate(Payment, Order, N:1)
usecase Capture "確定"
usecase Move "付け替え"
transitions(Capture, Payment.status, pending -> captured)
relinks(Move, Payment, Order)
transitions(Move, Payment.status, captured -> voided)
property CapturedStays stable(Payment.status == captured).along(Payment, Order)
"#;

/// The link is read before the step only: Move takes a captured payment to another order and
/// voids it in one step, which breaks the obligation the payment took on under its first
/// order.
#[test]
fn a_step_that_moves_the_key_keeps_the_obligation() {
    let ts = TransitionSystem::build(&model_from(RELINK), Bounds::VERIFY, &Scope::ALL).ts;
    let (kind, instance, witness, steps) =
        decide(&ts, "CapturedStays").expect("Move voids a captured payment");
    assert_eq!(
        (kind, witness, steps),
        (LtlKind::Stable, 2, strings(&["Capture", "Move"]))
    );
    assert_eq!(instance.len(), 2, "{instance:?}");
}

const DELETE: &str = r#"
entity Payment "支払" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
usecase Open "起票"
usecase Capture "確定"
usecase Drop "削除"
creates(Open, Payment)
transitions(Capture, Payment.status, pending -> captured)
deletes(Drop, Payment)
property CapturedStays stable(Payment.status == captured)
property PendingStays stable(Payment.status == pending)
"#;

/// A deleted row resets to its base values, but a step that deletes it leaves no present
/// row to break `stable`.
#[test]
fn a_step_that_deletes_the_row_breaks_nothing() {
    let ts = TransitionSystem::build(&model_from(DELETE), Bounds::VERIFY, &Scope::ALL).ts;
    assert_eq!(decide(&ts, "CapturedStays"), None);
    assert_eq!(
        decide(&ts, "PendingStays"),
        Some((
            LtlKind::Stable,
            strings(&["Payment#1"]),
            2,
            strings(&["Open", "Capture"])
        ))
    );
}
