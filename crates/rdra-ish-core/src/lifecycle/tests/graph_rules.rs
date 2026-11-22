//! `precedes` and `can_reach`: properties decided on the graph BFS explores for their cone.

use super::*;

const ORDER: &str = r#"
entity Order "Order" {
  id: Int @pk
  status: Enum(pending, on_hold, review, paid, shipped, cancelled) @default(pending)
}
usecase Place "Place"
usecase Hold "Hold"
usecase Review "Review"
usecase Back "Back"
usecase Pay "Pay"
usecase Ship "Ship"
usecase ShipHeld "Ship held"
usecase Cancel "Cancel"
creates(Place, Order)
transitions(Hold, Order.status, pending -> on_hold)
transitions(Review, Order.status, on_hold -> review)
transitions(Back, Order.status, review -> on_hold)
transitions(Pay, Order.status, pending -> paid)
transitions(Ship, Order.status, paid -> shipped)
transitions(ShipHeld, Order.status, on_hold -> shipped)
transitions(Cancel, Order.status, pending -> cancelled)
property PaidBeforeShipped precedes(Order.status == paid, Order.status == shipped)
property CanFinish can_reach(Order.status in (shipped, cancelled))
"#;

/// Each graph-rule violation the run reports: its name and the actions of its trace.
fn violations(d: &Derived) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = d
        .diags
        .iter()
        .map(|x| &x.diag)
        .chain(d.views.iter().flat_map(|v| v.diagnostics.iter()))
        .filter_map(|x| match x {
            StateDiag::PrecedenceViolated { name, trace, .. }
            | StateDiag::ReachabilityViolated { name, trace, .. } => Some((
                name.clone(),
                trace.steps.iter().map(|s| s.action.clone()).collect(),
            )),
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn names(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|x| x.to_string()).collect()
}

/// A path that ships a held order never pays it: the shortest such path is the trace.
#[test]
fn precedes_fails_on_a_path_that_skips_its_first_condition() {
    let d = derive_src(ORDER);
    assert!(violations(&d).contains(&(
        "PaidBeforeShipped".to_string(),
        names(&["Place", "Hold", "ShipHeld"])
    )));
}

#[test]
fn precedes_holds_when_every_path_meets_its_first_condition() {
    let src = ORDER.replace(
        "transitions(ShipHeld, Order.status, on_hold -> shipped)\n",
        "",
    );
    let d = derive_src(&src);
    assert!(
        !violations(&d).iter().any(|(n, _)| n == "PaidBeforeShipped"),
        "{:?}",
        violations(&d)
    );
}

/// Without `ShipHeld`, a held order cycles between on_hold and review and never finishes.
#[test]
fn can_reach_fails_from_a_cycle_with_no_way_out() {
    let src = ORDER.replace(
        "transitions(ShipHeld, Order.status, on_hold -> shipped)\n",
        "",
    );
    let d = derive_src(&src);
    assert_eq!(
        violations(&d),
        [("CanFinish".to_string(), names(&["Place", "Hold"]))]
    );
    let confined = d
        .diags
        .iter()
        .find_map(|x| match &x.diag {
            StateDiag::ReachabilityViolated { confined, .. } => Some(*confined),
            _ => None,
        })
        .expect("CanFinish is violated");
    assert_eq!(confined, 2, "on_hold and review");
}

#[test]
fn can_reach_holds_when_every_state_has_a_way_to_the_goal() {
    let d = derive_src(ORDER);
    assert!(
        !violations(&d).iter().any(|(n, _)| n == "CanFinish"),
        "{:?}",
        violations(&d)
    );
}

/// A terminal state that is not the goal fails `can_reach`.
#[test]
fn can_reach_fails_at_a_dead_end() {
    let src = ORDER.replace(
        "property CanFinish can_reach(Order.status in (shipped, cancelled))",
        "property CanFinish can_reach(Order.status == shipped)",
    );
    let d = derive_src(&src);
    assert!(violations(&d).contains(&("CanFinish".to_string(), names(&["Place", "Cancel"]))));
}

const PAYMENTS: &str = r#"
entity Order "Order" { id: Int @pk  status: Enum(open, paid, shipped) @default(open) }
entity Payment "Payment" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
relate(Payment, Order, 1:1)
usecase Place "Place"
usecase Capture "Capture"
usecase Ship "Ship"
creates(Place, Order)
creates(Place, Payment)
transitions(Capture, Order.status, open -> paid)
transitions(Capture, Payment.status, pending -> captured)
transitions(Ship, Order.status, paid -> shipped)
property Linked precedes(Payment.status == captured, Order.status == shipped).along(Payment, Order)
property Global precedes(Payment.status == captured, Order.status == shipped).global()
"#;

/// `.along(..)` binds only linked rows: another order's pending payment does not count.
#[test]
fn an_along_scope_reads_only_linked_rows() {
    let d = derive_scoped(&model_from(PAYMENTS), Bounds::VERIFY, &Scope::ALL);
    let failed: Vec<String> = violations(&d).into_iter().map(|(n, _)| n).collect();
    assert_eq!(failed, ["Global"]);
}
