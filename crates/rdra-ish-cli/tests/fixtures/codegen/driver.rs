// Driver of tests/codegen_behaviour.rs: runs the Rust that `codegen` prints for order.rdra,
// from outside the module, through its public shapes only.
#[path = "domain.rs"]
#[allow(dead_code)] // the driver uses part of the module
mod domain;
use domain::*;

fn at(text: &str) -> DateTime {
    text.parse().expect("a date and time")
}

fn row(status: OrderStatus, paid_at: Option<&str>, qty: i64) -> OrderRow {
    OrderRow {
        id: OrderId::from(1),
        status,
        paid_at: paid_at.map(at),
        method: Method::Card,
        qty,
        note: "n".to_string(),
        customer_id: CustomerId::from(7),
    }
}

fn main() {
    // The boundary: a scalar is made only from text of its shape.
    assert!("t".parse::<DateTime>().is_err());
    assert!("2026-01-01".parse::<Date>().is_ok());
    assert!("12.50".parse::<Money>().is_ok());
    assert!("12.".parse::<Money>().is_err());
    let err = "x".parse::<Method>().unwrap_err();
    assert_eq!(err.to_string(), "\"x\" is no value of Method");
    assert_eq!(OrderId::from(1).get(), 1);
    assert_eq!("7".parse::<OrderId>(), Ok(OrderId::from(7)));

    // A row fits its state and comes back unchanged.
    let pending = row(OrderStatus::Pending, None, 1);
    let order = Order::try_from(pending.clone()).expect("a pending row with no payment time");
    assert_eq!(order.kind(), OrderKind::Pending);
    assert_eq!(OrderRow::from(order.clone()), pending);
    // A row that breaks its state's refinement is refused, naming the column.
    let t = "2026-01-01T09:00:00Z";
    let bad = Order::from_row(row(OrderStatus::Pending, Some(t), 1)).unwrap_err();
    assert_eq!(bad.column, "paid_at");
    let bad = Order::from_row(row(OrderStatus::Paid, None, 1)).unwrap_err();
    assert_eq!(bad.column, "paid_at");
    let _: &dyn std::error::Error = &bad;

    // Dispatch: Pay runs from pending, writes the payment time and qty 2, raises EvPaid.
    let PayOutcome::Ran(paid) = run_pay(order, at(t)).expect("runs");
    assert_eq!(paid.order.paid_at(), &at(t));
    assert_eq!(*paid.order.qty(), 2);
    assert_eq!(paid.ev_paid, EvPaid);
    // Pay does not run from paid.
    let again = run_pay(Order::Paid(paid.order.clone()), at("2026-01-02T09:00:00+09:00"));
    assert!(matches!(again, Err(PayError::NotEnabled(_))));
    // Cancel runs from paid when the condition Big holds (qty >= 2).
    let cancelled = run_cancel(Order::Paid(paid.order.clone())).expect("Big holds");
    assert!(matches!(cancelled, CancelOutcome::FromPaid(_)));
    // Cancel is rejected by Big on a pending order with qty 1.
    let rejected = run_cancel(Order::from_row(row(OrderStatus::Pending, None, 1)).unwrap());
    assert!(matches!(
        rejected,
        Err(CancelError::Rejected(Rejected { condition: "Big", .. }))
    ));
    let why = rejected.unwrap_err();
    assert!(std::error::Error::source(&why).is_some());

    // Rule checks: qty 5 breaks `forbidden(Order, qty > 4)`.
    // A state's fields are private: a row with qty 5 is the way to such a state.
    assert!(paid.order.rule_violations().is_empty());
    let big = Order::from_row(OrderRow {
        qty: 5,
        ..Order::Paid(paid.order.clone()).into_row()
    })
    .expect("a paid row");
    assert_eq!(big.rule_violations(), vec!["Forbidden_Order_0"]);
    println!("ok");
}
