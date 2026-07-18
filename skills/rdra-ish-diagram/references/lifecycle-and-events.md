# Lifecycle and Event Views

Use this reference when reviewing state machines, event causality, event-triggered
BUCs, reachable state patterns, or entity constraints.

## Commands

```sh
# State diagram — whole model
rdra-ish diagram src/ --kind state --format mermaid

# State diagram — scoped to a BUC
rdra-ish diagram src/ --kind state --buc <BucId> --format mermaid

# Event-flow diagram — event causality across BUCs/use cases/states
rdra-ish diagram src/ --kind event-flow --format mermaid

# Event contract export
rdra-ish export src/ --kind asyncapi --out out/asyncapi.json

# Reachable state patterns
rdra-ish states src/
rdra-ish states src/ --entity <EntityId>
rdra-ish states src/ --buc <BucId>
rdra-ish states src/ --format json --entity <EntityId>

# Formal verification (Int / now / temporal / multi-instance)
rdra-ish export src/ --kind tla -o out/
rdra-ish verify src/ --backend tlc -o out/
```

## Reading State Diagrams

- Nodes are `state` declarations.
- Arrows are labelled with event display names.
- `[*]` initial state is derived from creation paths.

## Reading Event Flow

- Use-case-to-event edges come from `raises`.
- Event-to-use-case or event-to-BUC edges come from `triggers`.
- Event-to-state edges come from `transitions(Entity.col, Event, from -> to)`.
- AsyncAPI export is useful when event payload/contract review is needed alongside
  event causality.
- Warnings such as an event never being raised or consumed are part of the review
  signal; inspect stderr before trusting the diagram as complete.

## When To Prefer `states`

- Use `states` before diagram polish when reviewing entity constraints.
- Treat multi-entity `forbidden` / `invariant` diagnostics as cross-product checks over
  participating entities; `CrossConstraintNotEvaluated` means the rule references
  values that are not present in abstract state patterns, exceeds the cross-product
  cap, or uses relation-scoped `.along(...)` linked-instance semantics.
- Use `states --entity <EntityId>` when a whole-model state table is too broad.
- Use `states --format json` when checking truncation or exact reachable combinations.

## When To Prefer TLA+

- Use `export --kind tla` when rules involve Int / Money / Decimal, `now`, temporal
  `property`, `after.assert`, quantifiers, or multi-instance `.along` that BFS does
  not evaluate. Export writes both `.tla` and `.cfg`.
- Prefer one FV sample/module at a time; standalone FV samples can collide on ids if
  checked as a whole directory.
