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

# Lifecycle findings, then formal verification (temporal / two-instance rules)
rdra-ish check src/
rdra-ish verify src/ --backend tlc -o out/
```

## Reading State Diagrams

- The header comes first (`%% rdra-ish diagram=state scope=... model=...`, `%% kinds:`);
  `%% omitted:` names the Enum columns that have no `transitions` and so are not
  state models.
- Each state model (a lifecycle column) is one composite state,
  `state "Order.status" as statemodel__Order_status { ... }`.
- States are the Enum variants of that column, named `state__{Entity}_{variant}` (the
  same id as in `--kind rdra`), in the Enum's order.
- Arrows are the `transitions` arms, labelled with the id of the use case that drives
  each one (`state__Order_pending --> state__Order_paid : usecase__Capture`).
- `[*] -->` start edges come from the states a creating use case sets, or the start of a
  pre-existing entity. `--> [*]` marks an end state: one with no transition to another
  state.
- `--buc <BucId>` draws each state model one of the BUC's use cases moves, whole.
- The diagram draws the transitions, not reachability: run `check` to learn whether
  each state is reached (`Witness_*`) and left (`Deadlock_*`).

## Reading Event Flow

- Use-case-to-event and external-system-to-event edges come from `raises`.
- Event-to-use-case and event-to-BUC edges come from `triggers`; an event delivered to
  an external system draws a `delivers` edge.
- A timer draws its `triggers` edges to use cases and BUCs.
- State-to-state edges, labelled with the event, are the
  `transitions(Entity.col, UseCase, from -> to)` of the use cases the event triggers.
- AsyncAPI export is useful when event payload/contract review is needed alongside
  event causality.
- Warnings such as an event never being raised or consumed are part of the review
  signal; inspect stderr before trusting the diagram as complete.

## When To Prefer `check` and `states`

- Run `check` before diagram polish when reviewing entity constraints: it prints each
  broken rule once with a counterexample of use case steps, and each state no use case
  reaches or leaves.
- Treat multi-entity `forbidden` / `invariant` findings as checks over every
  combination of the participating entities' instances; `.along(...)` restricts them to
  instances linked by a `relate` path, and `UnbindableScope` means no `relate` binds the path.
- Use `states --entity <EntityId>` when a whole-model state table is too broad.
- Use `states --format json` for the exact reachable combinations and a witness path
  per pattern.

## When To Prefer TLA+

Prefer skill **`rdra-ish-verify`** for the full workflow. Short rule: run `verify` when
rules involve a temporal `property`, or quantifiers, `.along`, and 1:1 keys that need
two instances; `check` and `states` run one instance per entity and do not evaluate
`property`. Check one formal-verification sample at a time; the bundled samples reuse
ids and collide when checked as one directory.
