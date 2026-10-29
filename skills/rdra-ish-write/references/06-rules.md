# Step 6: Rules

Use this context when lifecycle paths are modeled and the next task is to express
invalid, conditional, mandatory, or mutually exclusive state facts as executable checks.

## Goal

Add business rules that can be type-checked as model predicates. Move in three passes:
local guardrails, local obligations, then advanced comparison or cross-entity rules.
Every rule is validated against the reachable states of the lifecycle transition system.
Cross-entity rules range over every combination of the participating entities' instances.
Use `.along(EntityA, EntityB, ...)` only for rules that should quantify over instances
linked by a declared `relate` path; each adjacent pair needs a `relate`, else the rule is
`UnbindableScope`.

## Ask For

- Which reachable state combinations are invalid?
- Which facts must not co-occur?
- Which value must be present when another state or comparison holds?
- Which value must be present in every reachable state?
- Does the rule mention one entity or multiple entities?
- For cross-entity rules, is the intended scope global or linked through a declared
  relation path?
- Are the rules hard invariants or review warnings for future refinement?
- Does an apparent violation mean the rule is wrong, or a missing `sets`/transition
  effect exists upstream?

## Procedure

1. Add `forbidden(Entity, ...)` for invalid combinations inside one entity.
2. Add `exclusive(Entity, ...)` for mutually exclusive state facts.
3. Add `invariant(Entity).when(...).then(...)` for single-entity required
   co-occurrences.
4. Add `required(Entity, ...)` only for facts that truly apply to every reachable state.
5. Use comparison propositions when rules depend on expressions such as
   `stock < selling`, and add matching `sets(..., expr, true/false)` effects.
6. Add `forbidden(...)` or `invariant(...)` when a rule mentions columns
   from more than one entity; qualify columns as `Entity.column`.
7. Add `.along(...)` when the intended semantics is relation-scoped rather than global
   cross-product; keep it off rules that truly forbid global co-existence.
8. If a per-entity rule fails unexpectedly, inspect lifecycle inputs before weakening it:
   missing `sets`, missing transitions, or missing create/default paths are common.
9. Add `when(...).none/has(...)` for to-many quantifiers (not `forbidden_when`).
10. Add `after(...).assert(...)` for a postcondition (`check` evaluates it after each
    step of the use case) and `property` for a temporal rule (only `verify` evaluates
    it; see skill `rdra-ish-verify`).
11. Keep implementation policy notes outside the DSL unless they can be expressed as
    state, effect, forbidden, invariant, required, exclusive, quantifier, property,
    or cross-entity predicates.

## Minimal Pattern

The entities below exist only to make the rules readable; in a real model the
lifecycle of Step 5 moves them.

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
entity Stock "Stock" {
  id: Int @pk
  stock: Int
  selling: Int
  reorder: Bool @default(false)
}
relate(Payment, Order, N:1)
usecase SellOut "Sell Out"

// 1. Local guardrails
forbidden(Order, status == cancelled, paid_at == present)
exclusive(Order, cancelled_at == present, paid_at == present)

// 2. Local obligations
invariant(Order)
  .when(status == submitted)
  .then(submitted_at == present)
required(Order, total >= 0)

// 3. Comparisons, cross-entity rules, quantifiers, postconditions, properties
sets(SellOut, Stock, stock < selling, true)
sets(SellOut, Stock, reorder == true)
invariant(Stock)
  .when(stock < selling)
  .then(reorder == true)

forbidden(Order, Payment,
  Order.status == cancelled,
  Payment.amount > Order.total)

invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status == paid)
  .then(Payment.status == captured)

when(Order, status == cancelled).none(Payment.status == captured)

after(SellOut).assert(Stock.reorder == true)

property PaidLeadsToCaptured
  leads_to(Order.status == paid, Payment.status == captured)
```

## Validation

```sh
rdra-ish check src/
rdra-ish check src/ --entity Order   # only Order's view and the rules that read it
rdra-ish lint src/ --format table
rdra-ish states src/ --format json --entity Order
# two instances per entity and temporal properties (needs tlc on PATH)
rdra-ish verify src/ --backend tlc
```

A broken rule prints once, under its name, with a shortest counterexample in model
terms: one line per use case step with the instances it binds and its `path:line:col`,
and under each step the values it changed and the `transitions` arm or `sets` that
wrote them. Read the trace before touching the rule: the fix is often a missing effect
upstream. In this sample of skill `rdra-ish-verify`, DeliverOrder lacks
`sets(DeliverOrder, Order, delivered_at == present)`, and the warning above the error
says so:

```text
$ rdra-ish check ../rdra-ish-verify/samples/fail/order.rdra
bounds: 1 instance per entity, Int 0..5; 4096 states per cone; entities a check does not read are unbounded
warning[UnwrittenConstrainedVar_Order_delivered_at]: state derivation for entity 'Order': delivered_at is constrained but no use case writes it
error[Inv_Order_0]: invariant Inv_Order_0 is violated: invariant violated: when status=delivered then delivered_at=present is broken by delivered_at=null, status=delivered
  rule: ../rdra-ish-verify/samples/fail/order.rdra:21:1
  counterexample (2 steps):
    1. PlaceOrder [Order#1]                              ../rdra-ish-verify/samples/fail/order.rdra:10:1
         + Order#1 (status=pending, delivered_at=null)
    2. DeliverOrder [Order#1]                            ../rdra-ish-verify/samples/fail/order.rdra:11:1
         Order#1.status: pending -> delivered            ../rdra-ish-verify/samples/fail/order.rdra:18:1
```

## Achievement Conditions

- Rules are expressed as model predicates, not prose-only comments.
- `check` reports no `error[...]` for a rule, or each one is listed as an open design
  item.
- Cross-entity rules use `Entity.column`; an `UnbindableScope` error means a `.along(...)`
  path that no `relate` binds, and `AmbiguousOwnerLink` means two keys join the pair,
  so name one with `.by(role)`.
- Temporal `property` rules, and rules that need two instances, are checked with
  `verify` (skill `rdra-ish-verify`); `check` runs one instance per entity and does not
  evaluate properties.
- Required lifecycle effects have corresponding `sets` or transitions.
- `NoWitness`, `Deadlock`, `NeverRuns`, `UnwrittenConstrainedVar`, and `NoCreationPath`
  findings are reviewed instead of ignored.
- A comparison reads the Int values of its columns. Move them with `sets(UC, E, col == N)`,
  or drive the comparison with `sets(..., expr, true/false)`. A use case that drives it one
  way while its own Int `sets` make it the other is `ContradictoryDrivenComparison`, an
  error.
- The final model can explain BUC scope, actor authority, API/system boundaries,
  entity structure, lifecycle, and rules as a single refinement chain.

## Completion

When this step passes review, summarize remaining warnings from `check`,
`actor-permission-audit`, `states`, and (when used) `verify`, with the `bounds:` line
they hold under, rather than claiming the model is universally complete.
