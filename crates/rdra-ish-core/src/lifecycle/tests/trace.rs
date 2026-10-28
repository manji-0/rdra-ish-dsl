//! Counterexample traces: one per violated rule, a shortest one, in model terms.

use super::*;
use crate::lifecycle::{Change, Trace};

fn traces(d: &Derived) -> Vec<(&StateDiag, &Trace)> {
    d.diags
        .iter()
        .filter_map(|x| Some((&x.diag, x.diag.trace()?)))
        .collect()
}

fn actions(t: &Trace) -> Vec<&str> {
    t.steps.iter().map(|s| s.action.as_str()).collect()
}

const ORDER: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, shipped, cancelled) @default(pending)  flagged: Bool @default(false) }
usecase Place "注文"
usecase Pay "支払"
usecase Ship "出荷"
usecase Cancel "取消"
creates(Place, Order)
transitions(Order.status, Pay, pending -> paid)
transitions(Order.status, Ship, paid -> shipped)
transitions(Order.status, Cancel, pending -> cancelled)
transitions(Order.status, Cancel, paid -> cancelled)
"#;

#[test]
fn a_violated_rule_reports_its_shortest_trace_once() {
    // Three patterns violate it (paid with either flag, shipped); only the first reached is
    // reported.
    let d = derive_src(&format!(
        "{ORDER}\nusecase Flag \"印\"\nupdates(Flag, Order)\nsets(Flag, Order, flagged == true)\nforbidden(Order, status == paid)"
    ));
    let found = traces(&d);
    assert_eq!(found.len(), 1, "{found:?}");
    let (_, trace) = found[0];
    assert_eq!(actions(trace), ["Place", "Pay"]);
    assert!(trace.over_bound.is_empty());
    let place = &trace.steps[0];
    assert_eq!(
        place
            .bound
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["Order#1"]
    );
    assert!(matches!(
        &place.changes[..],
        [Change::Create { at, values }] if at.to_string() == "Order#1"
            && values == &[("status".to_string(), "pending".to_string())]
    ));
    assert!(matches!(
        &trace.steps[1].changes[..],
        [Change::Set { field, from, to, cause: Some(_), .. }]
            if field == "status" && from == "pending" && to == "paid"
    ));
}

#[test]
fn an_after_assert_trace_ends_in_the_failing_step() {
    // Abort reaches `cancelled` first; the assert fails on Cancel's later step into the same
    // state, so the trace ends in that step, not on the BFS path to its target.
    let d = derive_src(
        r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, cancelled) @default(pending) }
usecase Place "注文"
usecase Pay "支払"
usecase Abort "中止"
usecase Cancel "取消"
creates(Place, Order)
updates(Cancel, Order)
transitions(Order.status, Pay, pending -> paid)
transitions(Order.status, Abort, pending -> cancelled)
transitions(Order.status, Cancel, paid -> cancelled)
after(Cancel).assert(Order.status == paid)
"#,
    );
    let found = traces(&d);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(matches!(found[0].0, StateDiag::AfterAssertViolated { .. }));
    assert_eq!(actions(found[0].1), ["Place", "Pay", "Cancel"]);
}

#[test]
fn a_trace_that_takes_more_slots_than_a_freed_cone_counts_is_marked() {
    // The forbidden's cone holds Order only, so it does not count Payment slots: Retry takes
    // a second Payment that one instance per entity does not have.
    let d = derive_src(
        r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, retried) @default(pending) }
entity Payment "決済" { id: Int @pk }
usecase Place "注文"
usecase Retry "再試行"
creates(Place, Order)
creates(Place, Payment)
creates(Retry, Payment)
updates(Retry, Order)
transitions(Order.status, Retry, pending -> retried)
relate(Payment, Order, N:1)
forbidden(Order, status == retried)
"#,
    );
    let found = traces(&d);
    assert_eq!(found.len(), 1, "{found:?}");
    let (_, trace) = found[0];
    assert_eq!(actions(trace), ["Place", "Retry"]);
    assert_eq!(trace.over_bound, ["Payment"]);
}
