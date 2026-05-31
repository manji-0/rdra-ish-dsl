# Step 6: Rules

<!-- derived-from ../../../docs/incremental-modeling.md#stage-6-business-rules -->

Use this context when lifecycle paths are modeled and the next task is to express
invalid or required state combinations as executable checks.

## Goal

Add business rules that can be validated against derived state patterns, while keeping
the rules tied to already-modeled lifecycle effects.

## Ask For

- Which reachable state combinations are invalid?
- Which value must be present when another state or comparison holds?
- Are the rules hard invariants or review warnings for future refinement?
- Does an apparent violation mean the rule is wrong, or a missing `sets`/transition
  effect exists upstream?

## Procedure

1. Add `forbidden(Entity, ...)` for invalid combinations that must not be reachable.
2. Add `invariant(Entity, condition).requires(...)` for required co-occurrences.
3. Use comparison propositions when rules depend on expressions such as
   `stock < selling`.
4. If a rule fails unexpectedly, inspect lifecycle inputs before weakening the rule:
   missing `sets`, missing transitions, or missing create/default paths are common.
5. Keep implementation policy notes outside the DSL unless they can be expressed as
   state, effect, forbidden, or invariant predicates.

## Minimal Pattern

```rdra
forbidden(Order, status.cancelled, paid_at present)

invariant(Order, status.submitted)
  .requires(submitted_at present)

sets(ReserveStock, Inventory, stock < selling, true)
forbidden(Inventory, stock < selling)
```

## Validation

```sh
rdra-ish check src/
rdra-ish states src/ --entity Order
rdra-ish states src/ --format json --entity Order
```

## Achievement Conditions

- Rules are expressed as model predicates, not prose-only comments.
- Violations from `states` are either fixed or intentionally listed as open design
  items.
- Required lifecycle effects have corresponding `sets` or transitions.
- Terminal, unreachable, and no-create warnings are reviewed instead of ignored.
- The final model can explain BUC scope, actor authority, API/system boundaries,
  entity structure, lifecycle, and rules as a single refinement chain.

## Completion

When this step passes review, summarize remaining unresolved warnings from `check`,
`actor-permission-audit`, and `states` rather than claiming the model is universally
complete.
