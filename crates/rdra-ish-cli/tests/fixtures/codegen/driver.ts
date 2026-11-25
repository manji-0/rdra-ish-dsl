// Driver of tests/codegen_behaviour.rs: runs the TypeScript that `codegen` prints for order.rdra.
import * as d from "./domain";

function assert(ok: boolean, what: string): void {
  if (!ok) throw new Error(what);
}

/** The value of a Result the driver expects to hold one. */
function value<T, E>(result: d.Result<T, E>, what: string): T {
  if (!result.ok) throw new Error(`${what}: ${JSON.stringify(result.error)}`);
  return result.value;
}

// The boundary: a row from outside is parsed, every column by its type.
const raw = { id: 1, status: "pending", paid_at: null, method: "card", qty: 1, note: "n", customer_id: 7 };
const pending = value(d.parseOrderRow(raw), "a pending row parses");
const badDate = d.parseOrderRow({ ...raw, paid_at: "yesterday" });
assert(!badDate.ok && badDate.error.column === "paid_at", "a DateTime of another shape is refused");
const badMethod = d.parseOrderRow({ ...raw, method: "cheque" });
assert(!badMethod.ok && badMethod.error.column === "method", "a value outside the set is refused");
assert(!d.parseOrderRow("row").ok, "a non-object is refused");
assert(!d.OrderId.parse(1.5).ok && d.OrderId.parse(3).ok, "an Int id is an integer");
assert(d.Method.parse("cash").ok && !d.Method.parse("x").ok, "Method.parse");
assert(d.Money.parse("1200.50").ok && !d.Money.parse("12,00").ok, "Money.parse");

// Rows and states.
const order = value(d.Order.fromRow(pending), "a pending row fits pending");
assert(order.kind === "Pending", "a pending row is in the state Pending");
assert(JSON.stringify(d.Order.toRow(order)) === JSON.stringify(pending), "round trip");
const t = value(d.DateTime.parse("2026-01-01T00:00:00Z"), "a DateTime parses");
const bad = d.Order.fromRow({ ...pending, paid_at: t });
assert(!bad.ok && bad.error.column === "paid_at", "pending with a payment time is invalid");
const badPaid = d.Order.fromRow({ ...pending, status: "paid" });
assert(!badPaid.ok && badPaid.error.column === "paid_at", "paid without a payment time is invalid");

// Dispatch.
const paid = value(d.runPay(order, t), "Pay runs from pending");
assert(paid.order.kind === "Paid" && paid.order.paid_at === t && paid.order.qty === 2, "Pay writes");
assert(paid.ev_paid.kind === "EvPaid", "Pay raises EvPaid");
const again = d.runPay(paid.order, t);
assert(!again.ok && again.error.kind === "NotEnabled", "Pay does not run from paid");
const cancelled = value(d.runCancel(paid.order), "Cancel runs from paid");
assert(cancelled.order.kind === "Cancelled", "Cancel cancels");
const rejected = d.runCancel(order);
assert(
  !rejected.ok && rejected.error.kind === "Rejected" && rejected.error.condition === "Big",
  "Big rejects qty 1",
);

// Rule checks.
assert(d.orderRuleViolations(paid.order).length === 0, "a paid order keeps its rules");
const big = d.orderRuleViolations({ ...paid.order, qty: 5 });
assert(big.length === 1 && big[0] === "Forbidden_Order_0", "qty 5 breaks Forbidden_Order_0");
console.log("ok");
