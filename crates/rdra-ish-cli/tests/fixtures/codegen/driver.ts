// Driver of tests/codegen_behaviour.rs: runs the TypeScript that `codegen` prints for order.rdra.
import * as d from "./domain";

function assert(ok: boolean, what: string): void {
  if (!ok) throw new Error(what);
}

const pending: d.OrderRow = {
  id: 1 as d.OrderId, status: "pending", paid_at: null, method: "card", qty: 1, note: "n",
  customer_id: 7 as d.CustomerId,
};
const order = d.orderFromRow(pending);
assert("status" in order && order.status === "pending", "a pending row fits pending");
if (!("status" in order)) throw new Error("unreachable");
assert(JSON.stringify(d.orderToRow(order)) === JSON.stringify(pending), "round trip");
const bad = d.orderFromRow({ ...pending, paid_at: "t" as d.DateTime });
assert("invalid" in bad && bad.column === "paid_at", "pending with a payment time is invalid");
const badPaid = d.orderFromRow({ ...pending, status: "paid" });
assert("invalid" in badPaid && badPaid.column === "paid_at", "paid without a payment time is invalid");

const paid = d.runPay(order, "t" as d.DateTime);
if (!("order" in paid)) throw new Error("Pay runs from pending");
assert(paid.order.status === "paid" && paid.order.paid_at === "t" && paid.order.qty === 2, "Pay writes");
assert(paid.ev_paid.event === "EvPaid", "Pay raises EvPaid");
assert("notEnabled" in d.runPay(paid.order, "u" as d.DateTime), "Pay does not run from paid");
const cancelled = d.runCancel(paid.order);
assert("order" in cancelled && cancelled.order.status === "cancelled", "Cancel runs from paid");
const rejected = d.runCancel(order);
assert("rejected" in rejected && rejected.rejected === "Big", "Big rejects qty 1");

assert(d.orderRuleViolations(paid.order).length === 0, "a paid order keeps its rules");
const big = d.orderRuleViolations({ ...paid.order, qty: 5 });
assert(big.length === 1 && big[0] === "Forbidden_Order_0", "qty 5 breaks Forbidden_Order_0");
console.log("ok");
