// Driver of tests/codegen_behaviour.rs: runs the Rust that `codegen` prints for order.rdra.
#[path = "domain.rs"]
mod domain;
use domain::*;

fn row(status: OrderStatus, paid_at: Option<&str>, qty: i64) -> OrderRow {
    OrderRow {
        id: OrderId(1),
        status,
        paid_at: paid_at.map(|t| DateTime(t.to_string())),
        method: Method::Card,
        qty,
        note: "n".to_string(),
        customer_id: CustomerId(7),
    }
}

fn main() {
    // A row fits its state and comes back unchanged.
    let pending = row(OrderStatus::Pending, None, 1);
    let order = Order::from_row(pending.clone()).expect("a pending row with no payment time");
    assert!(matches!(order, Order::Pending(_)));
    assert_eq!(order.clone().into_row(), pending);
    // A row that breaks its state's refinement is refused, naming the column.
    let bad = Order::from_row(row(OrderStatus::Pending, Some("t"), 1)).unwrap_err();
    assert_eq!(bad.column, "paid_at");
    let bad = Order::from_row(row(OrderStatus::Paid, None, 1)).unwrap_err();
    assert_eq!(bad.column, "paid_at");

    // Dispatch: Pay runs from pending, writes the payment time and qty 2, raises EvPaid.
    let PayOutcome::Ran(paid) = run_pay(order, DateTime("t".to_string())).expect("runs");
    assert_eq!(paid.order.paid_at, DateTime("t".to_string()));
    assert_eq!(paid.order.qty, 2);
    assert_eq!(paid.ev_paid, EvPaid);
    // Pay does not run from paid.
    let again = run_pay(Order::Paid(paid.order.clone()), DateTime("u".to_string()));
    assert!(matches!(again, Err(Refused::NotEnabled(_))));
    // Cancel runs from paid when qty <= ... the condition Big holds (qty >= 2).
    let cancelled = run_cancel(Order::Paid(paid.order.clone())).expect("Big holds");
    assert!(matches!(cancelled, CancelOutcome::FromPaid(_)));
    // Cancel is rejected by Big on a pending order with qty 1.
    let rejected = run_cancel(Order::from_row(row(OrderStatus::Pending, None, 1)).unwrap());
    assert!(matches!(rejected, Err(Refused::Rejected(Rejected { condition: "Big", .. }))));

    // Rule checks: qty 5 breaks `forbidden(Order, qty > 4)`.
    let mut big = paid.order.clone();
    assert!(big.rule_violations().is_empty());
    big.qty = 5;
    assert_eq!(Order::Paid(big).rule_violations(), vec!["Forbidden_Order_0"]);
    println!("ok");
}
