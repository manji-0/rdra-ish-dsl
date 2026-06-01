# Step 6: Rules

Use this context when lifecycle paths are modeled and the next task is to express
invalid or required state combinations as executable checks.

<!-- derived-from ../../../docs/language-reference.md#entity-state-constraints -->
<!-- derived-from ../../../docs/language-reference.md#cross-entity-constraints -->

## Goal

Add business rules that can be type-checked as model predicates. Per-entity rules are
validated against derived state patterns; cross-entity rules are checked by combining
the reached patterns of the participating entities when their conditions reference
state axes.

## Ask For

- Which reachable state combinations are invalid?
- Which value must be present when another state or comparison holds?
- Does the rule mention one entity or multiple entities?
- Are the rules hard invariants or review warnings for future refinement?
- Does an apparent violation mean the rule is wrong, or a missing `sets`/transition
  effect exists upstream?

## Procedure

1. Add `forbidden(Entity, ...)` for invalid combinations inside one entity.
2. Add `invariant(Entity).when(...).then(...)` for single-entity required
   co-occurrences.
3. Add `cross_forbidden(...)` or `cross_invariant(...)` when a rule mentions columns
   from more than one entity; qualify columns as `Entity.column`.
4. Use comparison propositions when rules depend on expressions such as
   `stock < selling`.
5. If a per-entity rule fails unexpectedly, inspect lifecycle inputs before weakening it:
   missing `sets`, missing transitions, or missing create/default paths are common.
6. Keep implementation policy notes outside the DSL unless they can be expressed as
   state, effect, forbidden, invariant, or cross-entity predicates.

## Minimal Pattern

```rdra
forbidden(Order, (status, cancelled), (paid_at, present))

invariant(Order)
  .when(status, submitted)
  .then(submitted_at, present)

sets(ReserveStock, Inventory, stock < selling, true)
forbidden(Inventory, stock < selling)

cross_forbidden(Order, Payment,
  (Order.status, cancelled),
  Payment.amount > Order.total)

cross_invariant(Order, Payment)
  .when(Order.status, paid)
  .then(Payment.status, captured)
```

## Validation

```sh
rdra-ish check src/
rdra-ish states src/ --entity Order
rdra-ish states src/ --format json --entity Order
```

## Achievement Conditions

- Rules are expressed as model predicates, not prose-only comments.
- Per-entity violations from `states` are either fixed or intentionally listed as
  open design items.
- Cross-entity rules use `Entity.column`; if `states` reports them as not evaluated,
  explain which condition is outside the abstract state space.
- Required lifecycle effects have corresponding `sets` or transitions.
- Terminal, unreachable, and no-create warnings are reviewed instead of ignored.
- The final model can explain BUC scope, actor authority, API/system boundaries,
  entity structure, lifecycle, and rules as a single refinement chain.

## Completion

When this step passes review, summarize remaining unresolved warnings from `check`,
`actor-permission-audit`, and `states` rather than claiming the model is universally
complete.
