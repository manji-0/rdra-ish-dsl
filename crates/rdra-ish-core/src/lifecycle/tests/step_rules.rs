//! `stable` and `unchanged`: step properties the explorer checks on every edge.

use super::*;

const ORDER: &str = r#"
entity Order "Order" {
  id: Int @pk
  status: Enum(pending, approved, cancelled) @default(pending)
  amount: Int @default(1)
}
usecase Place "Place"
usecase Approve "Approve"
usecase Cancel "Cancel"
usecase Reopen "Reopen"
usecase Bump "Bump"
creates(Place, Order)
transitions(Approve, Order.status, pending -> approved)
transitions(Cancel, Order.status, pending -> cancelled)
transitions(Reopen, Order.status, cancelled -> pending)
sets(Bump, Order.amount == 2)
property CancelledStays stable(Order.status == cancelled)
property ApprovedStays stable(Order.status == approved)
property AmountFixed unchanged(Order.amount).when(Order.status == approved)
"#;

/// The names of the step properties the run reports violated.
fn violated(d: &Derived) -> Vec<String> {
    let mut out: Vec<String> = d
        .diags
        .iter()
        .map(|x| &x.diag)
        .chain(d.views.iter().flat_map(|v| v.diagnostics.iter()))
        .filter_map(|x| match x {
            StateDiag::StepRuleViolated { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

#[test]
fn a_step_that_leaves_a_stable_state_or_changes_a_fixed_column_is_reported() {
    let d = derive_src(ORDER);
    assert_eq!(violated(&d), ["AmountFixed", "CancelledStays"]);
}

#[test]
fn a_guarded_step_keeps_a_stable_state_and_a_fixed_column() {
    let src = ORDER
        .replace("transitions(Reopen, Order.status, cancelled -> pending)\n", "")
        .replace(
            "sets(Bump, Order.amount == 2)",
            "condition Open \"Open\"\nholds(Open, Order.status == pending)\nsets(Bump, Order.amount == 2).when(Open)",
        );
    let d = derive_src(&src);
    assert!(violated(&d).is_empty(), "{:?}", violated(&d));
}

/// The trace ends in the breaking step, from a state where the trigger holds.
#[test]
fn the_trace_ends_in_the_step_that_breaks_the_rule() {
    let d = derive_src(ORDER);
    let trace = d
        .diags
        .iter()
        .map(|x| &x.diag)
        .chain(d.views.iter().flat_map(|v| v.diagnostics.iter()))
        .find_map(|x| match x {
            StateDiag::StepRuleViolated { name, trace, .. } if name == "CancelledStays" => {
                Some(trace.clone())
            }
            _ => None,
        })
        .expect("CancelledStays is violated");
    let actions: Vec<&str> = trace.steps.iter().map(|s| s.action.as_str()).collect();
    assert_eq!(actions, ["Place", "Cancel", "Reopen"]);
}

/// A deleted row is no longer bound to its rule: deleting a cancelled order is no step out of
/// the state.
#[test]
fn deleting_a_row_does_not_break_its_stable_state() {
    let src = ORDER
        .replace(
            "transitions(Reopen, Order.status, cancelled -> pending)\n",
            "",
        )
        .replace(
            "usecase Bump \"Bump\"",
            "usecase Bump \"Bump\"\nusecase Drop \"Drop\"\ndeletes(Drop, Order)",
        )
        .replace("sets(Bump, Order.amount == 2)\n", "");
    let d = derive_src(&src);
    assert!(violated(&d).is_empty(), "{:?}", violated(&d));
}
