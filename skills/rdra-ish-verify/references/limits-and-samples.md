# Limits and samples

<!-- derived-from ../../../docs/formal-verification.md -->

## What the bounds approximate

- **Bounds.** `InstanceCount` (default 2) and `IntRange` (default `0..5`). An `ok` says
  nothing outside them. A deleted slot is never reused.
- **Int values.** Int, Money, Decimal, Date, and DateTime columns that something reads are
  variables over `IntRange`. A literal or an integer `@default` outside it is
  `IntLiteralOutOfBounds`, an error in `check` too: raise `--int-range-max` or lower the
  value.
- **`now`.** A global clock that ticks through `IntRange`. Nothing pins a column compared to
  `now`, so a missing deadline shows as a violation (`samples/now_coupon.rdra`).
- **Atomic use case.** A use case is one atomic step per transaction group it writes. One
  that writes through k APIs is k steps in `invokes` order, and other use cases can run
  between them (`samples/cross_order_payment_split.rdra`).
- **Event order.** `raises(UC, Ev)` then `triggers(Ev, UC2)`: `UC2` runs only on a row the
  raise marked. The mark is never cleared, so after the first raise `UC2` runs as often as
  its guards allow. Write `.counted(n)` on the trigger for once per raise. A timer, or an
  event an external system raises, orders nothing.
- **Keys.** A 1:1 key (`Unique_<Child>_<role>`) and a `@unique` set (`Unique_<E>_<cols>`)
  break only with two rows. A set with a `String` column, or on a `@soft_delete` entity, is
  not checked. `1..*:1` is `MinCard_*`, `0..n:1` is `MaxCard_*`, and a delete under a key
  with no `.on_delete` is guarded by `DanglingKey_*`. `cascade`, `set_null`, and
  `set_default` deletes are not run yet: the children stay, unchecked.
- **Null.** A `@null` column adds `null` to its values. See
  `references/dsl-surface.md` ("Null values").
- **Fairness.** `WF_vars(Next)` is fairness on the whole step relation, so another
  instance's loop satisfies it. `.fair(UC, ...)` adds weak fairness per instance; there is
  no strong fairness.
- **Cones.** Each check runs on its cone of influence. A temporal property runs on the
  whole model only when its cone does not decide it, within `--temporal-budget` (300 s).
  A large model's whole-model run can end `not decided`.
- **Deadlocks under TLC.** TLC's own `CHECK_DEADLOCK` stays off. The `StateModel_<E>` specs
  of `export --kind tla` find witnesses with TLC alone, but a TLC pass on their `Deadlock_`
  invariants is no proof; `check` or `verify` decides deadlocks.
- **Engines.** `verify --with-tlc` runs TLC beside the native checker on every check and
  fails on a disagreement.

## Bundled samples

Each file under `samples/` is standalone: check one at a time. In the `rdra-ish-dsl`
repository, `samples/formal-verification/` and `samples/formal-verification-fail/` are
symlinks to them. A sample that does not pass states its expected result in `// expect`
lines at the top (`// expect verify: fail Forbidden_Order_0`): the exit status and the
exact set of error names. The repository's `scripts/verify-samples.sh` checks every one.

| Skill path | `check` | `verify` | Focus |
|---|---|---|---|
| `samples/order.rdra` | ok | ok | Lifecycle + `after.assert` + `leads_to` / `eventually` |
| `samples/int_stock.rdra` | ok | ok | Int arithmetic invariant / property |
| `samples/now_coupon.rdra` | fail `Forbidden_Coupon_0` | fail, on the same names | `now` and `Tick`; no action pins `expired_at` |
| `samples/cross_order_payment.rdra` | ok | ok | Multi-instance + `.along` (Pay is one atomic action; Cancel deletes the pending Payment) |
| `samples/cross_order_payment_split.rdra` | fail `MultiForbidden_Order_0` | fail, on the same names | Pay through two APIs is two actions in `invokes` order, the capture first, so Cancel runs between them |
| `samples/split_run_order.rdra` | ok | ok | OpenSettlement through two APIs is one run: the settlement is created once, under the claim the first step moved, and `.fair(OpenSettlement, ..)` finishes a started run |
| `samples/quantifier_none.rdra` | fail `Quantifier_Cert_0` | fail, on the same names | `invariant(...).when(...).none(...)` (Revoke while an Assign stays active) |
| `samples/quantifier_guard.rdra` | ok | ok | `holds(..).none(..)` guard on Revoke; the `.none(...)` rule holds |
| `samples/untriggered_sets.rdra` | fail `Forbidden_Order_0` | fail, on the same names | `sets` from a use case that drives no transition, next to `transitions` |
| `samples/after_assert_unmet.rdra` | fail `AfterAssert_DeliverOrder_0` | fail, on the same names | `after(UC).assert` whose postcondition the use case does not set |
| `samples/temporal_fair.rdra` | ok | fail `GrantEndsUnfair` | The same `leads_to` with and without `.fair(Register, Dismiss)`: another application amending forever breaks the unfair one only |
| `samples/order_stability.rdra` | ok | fail `CancelledStaysCancelled`, `AmountKeptOnceDecided` and `ShippedAfterInvoiced` | `stable`, `unchanged(..).when(..)` and `precedes`, each decided by BFS on its cone |
| `samples/temporal_along.rdra` | ok | fail `CapturedPaidAnyOrder` | The same `always` over a Payment and an Order with and without `.along(Payment, Order)`: only the unscoped one fails |
| `samples/temporal_unmet.rdra` | ok | fail `NeverShipped` and `PendingLeadsToDelivered` | An `always` and a `leads_to` that both fail; the `leads_to` lasso loops back |
| `samples/unwitnessed_state.rdra` | fail `Witness_Order_status_refunded Deadlock_Order_status_shipped` | fail, on the same names | A `transitions` endpoint no use case reaches, and a shipped Order whose only way out is Refund; Refund warns `Runs_Refund` |
| `samples/deadlock.rdra` | fail `Deadlock_Order_status_paid` | fail, on the same name | A way out that never opens: Ship's condition holds only while `pending`; Ship warns `Runs_Ship` |
| `samples/can_reach_unmet.rdra` | ok | fail `OrderCanFinish` | No deadlock, but an Order moves between `pending` and `on_hold` forever; `can_reach` fails with the states it is confined to |
| `samples/can_reach_met.rdra` | ok | ok | The same model with Restock: every reachable state can still finish |
| `samples/event_order.rdra` | ok | ok | `triggers(EvOrderExecuted, IssueCert)` runs IssueCert only after the event, so its `after` assertion holds |
| `samples/partial_scope.rdra` | fail `DerivedColumnRead_Audit_score` | fails before any cone; `--entity Order` passes | Audit's `forbidden` reads an arithmetic `@derived` column; `--entity Order` defers it under `not checked:` |
| `samples/one_to_one_shared.rdra` | ok | fail `Unique_Shipment_Order` | Two use cases link two Shipments to one Order; one instance cannot show it |
| `samples/none_any_child.rdra` | ok | ok | `.none(Invoice)` on Issue says "no invoice yet" and reads the link only; `Unique_Invoice_Order` holds |
| `samples/none_any_child_bypassed.rdra` | ok | fail `Unique_Invoice_Order` | Reissue creates a second Invoice under a closed Order without the guard |
| `samples/min_card_unmet.rdra` | fail `MinCard_OrderLine_Order` | fail, on the same names | `1..*:1`; Place creates an Order without a line |
| `samples/min_card_met.rdra` | ok | ok | Place creates the Order and its first line in one transaction; `check` warns that AddLine never runs at one instance, `verify` runs it |
| `samples/check_unmet.rdra` | fail `Check_CartLine_qty_0` | fail, on the same names | `@check(qty >= 1)`; ClearQty sets `qty` to 0 |
| `samples/check_met.rdra` | ok | ok | The same check kept by every use case; a `forbidden` reads a Bool `@derived` column |
| `samples/fail/order.rdra` | fail `Inv_Order_0` | fail, on the same names | DeliverOrder never sets `delivered_at` |

`docs/formal-verification.md` in the repository holds the full mapping tables.
