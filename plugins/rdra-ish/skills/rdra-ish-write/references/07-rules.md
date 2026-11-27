# Step 7: Rules

<!-- derived-from ../../../docs/incremental-modeling.md#stage-7a-local-guardrails -->

State the facts that must never, always, or together hold, as rules `check` tests against
every reachable state. Go in three passes: local guardrails (7A), local obligations (7B),
then comparisons and cross-entity rules (7C).

## Ask

<!-- stage-spec: asks local-guardrails -->
- Which combinations must never be reachable?
- Which facts must be mutually exclusive?
- Are these facts already represented as Enum, Bool, or nullable state axes?
<!-- /stage-spec -->
<!-- stage-spec: asks local-obligations -->
- Which values must co-occur once a condition is true?
- Which facts must be true in every reachable state?
- Is the rule too broad, or should it be an `invariant` with a guard instead of `required`?
<!-- /stage-spec -->
<!-- stage-spec: asks advanced-rules -->
- Which comparisons matter, and which use cases move the numeric or date values they read?
- Which rules really mention more than one entity?
- Is a cross-entity rule a global cross-product rule, or only intended along a declared relation path?
<!-- /stage-spec -->

## Do

1. 7A: `forbidden(Entity, ...)` for an invalid combination; `exclusive(Entity, ...)` for
   facts that never hold together.
2. 7B: `invariant(Entity).when(...).then(...)` for facts that hold together;
   `required(Entity, ...)` only for a fact true in every reachable state. A rule may name a
   `condition` with a `holds` body as its guard, `invariant(E).when(Cond).then(...)`, so the
   guard and the rule stay one statement.
3. 7C: compare values (`stock < selling`, `Payment.amount > Order.total`). On an Enum,
   variation, or Bool column a bare right-hand side is a value (`status == paid`); compare
   two columns with a qualified one (`status != Order.previous`). Write
   `status in (paid, shipped)` or `not in (...)`, not a chain of `!=`. A null operand
   satisfies no comparison but `!=`, and two nulls are not equal; admit null with
   `col == null or ...`.
4. 7C: a rule over several entities qualifies every column as `Entity.column`. Scope it with
   `.along(A, B, ..)` (each adjacent pair joined by a `relate`, else `UnbindableScope`) when
   it means linked rows, or `.global()` when it means every combination; with neither,
   `lint` `multi-rule-unscoped` warns.
5. Count related rows with `invariant(E).when(...).none(Related)`, `.has(...)`,
   `.at_most(n, ..)`, `.at_least(n, ..)`, or `.exactly(n, ..)`. Write `.none(Related)` for
   "no related row", not a list of values. A count of `n` breaks only with `n + 1` rows:
   verify it with `--instances Loan=4` (skill `rdra-ish-verify`).
6. Write `after(UseCase).assert(...)` for a postcondition (read after the use case's last
   step) and `property` for a temporal rule (only `verify` checks it).
7. End every rule with `.label("...")`: the rule as the business says it. `list --kind rule`
   and each violation print the label; `lint` `rule-unlabeled` lists the rules without one.
8. When a rule fails, read the counterexample before weakening the rule: a missing `sets`,
   arm, or create path upstream is the usual cause.

```rdra
entity Order "Order" {
  id: Int @pk
  status: Enum(draft, submitted, paid, cancelled) @default(draft)
  total: Money
  submitted_at: DateTime @null
  paid_at: DateTime @null
  cancelled_at: DateTime @null
}

entity Payment "Payment" {
  id: Int @pk
  status: Enum(pending, captured) @default(pending)
  amount: Money
}

relate(Payment, Order, N:1)

// 7A
forbidden(Order, status not in (paid, cancelled), paid_at == present).label("Only a paid or cancelled order has a payment date")
exclusive(Order, cancelled_at == present, paid_at == present).label("An order is not both paid and cancelled")

// 7B
invariant(Order).when(status == submitted).then(submitted_at == present).label("A submitted order records when it was submitted")
required(Order, total >= 0).label("No order total is negative")

// 7C
invariant(Order, Payment).along(Order, Payment).when(Order.status == paid).then(Payment.status == captured).label("A paid order's payment is captured")
invariant(Order).when(status == cancelled).none(Payment.status == captured).label("A cancelled order has no captured payment")
```

## Validate

<!-- stage-spec: review local-guardrails -->
```sh
rdra-ish check src/
rdra-ish states src/ --entity Order
```
<!-- /stage-spec -->

Also `rdra-ish list src/ --kind rule --format markdown` (the rule table a PRD takes its
acceptance criteria from) and `rdra-ish verify src/` for rules that need two instances and
for properties.

A broken rule prints once, under its name, with the shortest counterexample: each use case
step with the rows it binds, and the values it changed. In this sample of skill
`rdra-ish-verify`, DeliverOrder never sets `delivered_at`, and the warning above the error
says so:

```text
$ rdra-ish check ../rdra-ish-verify/samples/fail/order.rdra
bounds: 1 instance per entity, Int 0..5; no state cap per cone, memory 4GiB; entities a check does not read are unbounded
warning[UnwrittenConstrainedVar_Order_delivered_at]: state derivation for entity 'Order': delivered_at is constrained but no use case writes it
error[Inv_Order_0]: invariant Inv_Order_0 is violated: invariant violated: when status=delivered then delivered_at=present is broken by delivered_at=null, status=delivered
  rule: ../rdra-ish-verify/samples/fail/order.rdra:21:1
  counterexample (2 steps):
    1. PlaceOrder [Order#1]                              ../rdra-ish-verify/samples/fail/order.rdra:10:1
         + Order#1 (status=pending, delivered_at=null)
    2. DeliverOrder [Order#1]                            ../rdra-ish-verify/samples/fail/order.rdra:11:1
         Order#1.status: pending -> delivered            ../rdra-ish-verify/samples/fail/order.rdra:18:1
```

## Done when

- Rules are predicates, each with a label, not comments.
- `check` reports no rule error, or each one is an open design item.
- `lint` reports no `multi-rule-unscoped`.
- Properties and two-instance rules are checked with `verify`.
- `export --kind sql` writes each entity-local rule as a `CHECK`; it lists multi-entity,
  quantifier, minimum-card, condition-guarded, and `now` rules as application obligations.

Report the warnings `check`, `actor-permission-audit`, `states`, and `verify` leave, with
the `bounds:` line they hold under. Do not call the model complete.
