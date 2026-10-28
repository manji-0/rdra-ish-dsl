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
10. Add `property` / `after(...).assert(...)` when temporal or postcondition checks
    matter; validate with skill `rdra-ish-verify` / `export --kind tla`
    (`states` does not evaluate `property`).
11. Keep implementation policy notes outside the DSL unless they can be expressed as
    state, effect, forbidden, invariant, required, exclusive, quantifier, property,
    or cross-entity predicates.

## Minimal Pattern

```rdra
forbidden(Order, status == cancelled, paid_at == present)

exclusive(Document, approved == true, rejected == true)

invariant(Order)
  .when(status == submitted)
  .then(submitted_at == present)

required(Account, active == true)

sets(ReserveStock, Inventory, stock < selling, true)
forbidden(Inventory, stock < selling)

forbidden(Order, Payment,
  Order.status == cancelled,
  Payment.amount > Order.total)

invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status == paid)
  .then(Payment.status == captured)

when(Cert, status == revoked).none(Assign.status == active)

property PaidLeadsToShipped
  leads_to(Order.status == paid, Order.status == shipped)

after(DeliverOrder).assert(Order.status == delivered)
```

## Validation

```sh
rdra-ish check src/
rdra-ish lint src/ --format table
rdra-ish states src/ --entity Order
rdra-ish states src/ --format json --entity Order
# temporal property / two-instance rules: use TLA+ (`states` does not evaluate `property`)
rdra-ish export src/ --kind tla -o out/
# optional if tlc is on PATH:
# rdra-ish verify src/ --backend tlc -o out/
```

## Achievement Conditions

- Rules are expressed as model predicates, not prose-only comments.
- Per-entity violations from `states` are either fixed or intentionally listed as
  open design items.
- Cross-entity rules use `Entity.column`; if `states` reports `UnbindableScope`, declare
  the `relate` that binds the `.along(...)` path.
- Temporal `property` rules, and rules that need two instances, are checked with
  `export --kind tla` / `verify --backend tlc`, not by `states`.
- Required lifecycle effects have corresponding `sets` or transitions.
- Terminal, unreachable, and no-create warnings are reviewed instead of ignored.
- A comparison reads the Int values of its columns. Move them with `sets(UC, E, col == N)`,
  or drive the comparison with `sets(..., expr, true/false)`. A use case that drives it one
  way while its own Int `sets` make it the other is `ContradictoryDrivenComparison`, an
  **`[error]`** under `states` (fail-closed).
- The final model can explain BUC scope, actor authority, API/system boundaries,
  entity structure, lifecycle, and rules as a single refinement chain.

## Completion

When this step passes review, summarize remaining unresolved warnings from `check`,
`actor-permission-audit`, `states`, and (when used) TLC rather than claiming the
model is universally complete.
