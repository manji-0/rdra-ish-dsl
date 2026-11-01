# Lifecycle and Event Views

Use this reference when reviewing state machines, event causality, event-triggered
BUCs, reachable state patterns, or entity constraints.

## Commands

```sh
# State diagram — whole model, an index: one note per state model with its counts
rdra-ish diagram src/ --kind state --format mermaid

# State diagram — one entity's state models whole (every lifecycle column); repeatable
rdra-ish diagram src/ --kind state --entity <EntityId> --format mermaid

# State diagram — scoped to a BUC
rdra-ish diagram src/ --kind state --buc <BucId> --format mermaid

# State-transition table — one row per arrow, with guards and actors
rdra-ish list src/ --kind transition --format markdown

# Event diagram — event causality across BUCs, use cases, and external systems
rdra-ish diagram src/ --kind event --format mermaid

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
  state models. One that a `sets` writes is a state axis of `check` all the same and is
  named on `%% set-only:` instead, not on `omitted:`.
- Each state model (a lifecycle column) is one composite state,
  `state "Order.status" as statemodel__Order_status { ... }`.
- States are the Enum variants of that column, named `state__{Entity}_{variant}` (the
  same id as in `--kind rdra`), in the Enum's order.
- Arrows are the `transitions` arms, labelled with the label of the use case that drives
  each one and `[...]` with the conditions that guard it: the use case's `applies_to`
  conditions, then the arm's own `.when(Cond)`. The line before each arrow names
  the use case by id (`%% transition usecase__Capture`).
- `[*] -->` start edges come from the states a creating use case sets, or the start of a
  pre-existing entity. `--> [*]` marks an end state: one with no transition to another
  state.
- `--buc <BucId>` draws each state model one of the BUC's use cases moves, whole.
- The diagram draws the transitions, not reachability: run `check` to learn whether
  each state is reached (`Witness_*`) and left (`Deadlock_*`).

## Reading the State Diagram

- Without a scope the diagram is an index: each state model is a composite state holding
  one note, `<n> 状態 / <m> 遷移 (--entity <Entity>)`. Run that `--entity` for the states.
- `--entity <Entity>` and `--buc <BucId>` draw each state model whole: the states, a
  transition per use case (`%% transition usecase__<id>` names it), `[*] -->` for the
  start state and `--> [*]` for an end state.
- A `class` line colours a state by the lifecycle check: `stateunreachable` (a value no
  reachable state holds, drawn with no arrow), `statedeadlock` (an instance cannot leave
  it, although it has a way out; the finding is `Deadlock_<Entity>_<column>_<value>`),
  `stateend` (no transition out). The `%% legend:` line says so. A deadlock still draws
  the diagram; `check` reports it as an error.
- `list --kind transition` is the table behind the arrows: one row per transition with
  `entity, column, from, to, usecase, guards, actors, site`. `from` and `to` read
  `label (value)`, `usecase` reads `label (id)`, `guards` are the condition labels the
  arrow shows in `[...]`, and `actors` perform the use case (an event- or timer-started use
  case lists only its explicit `performs`). `site` is the `transitions` line, `file:line`.
  Several values in a cell are joined with `|`.

## Reading the Event Diagram

- The header comes first (`%% rdra-ish diagram=event scope=... model=...`,
  `%% kinds:`, `%% omitted:`). Node ids are `<kind>__<DSL id>` (`event__EvPaymentAuthorized`,
  `usecase__Capture`, `extsystem__PaymentGW`, `buc__...`, `timer__...`), the same ids as in
  `--kind rdra`, and each node carries `:::<kind>`.
- Every edge is unlabelled: its two end kinds name the relation.
- Use-case-to-event and external-system-to-event edges come from `raises`.
- Event-to-use-case, event-to-BUC, and event-to-external-system edges come from
  `triggers`.
- A timer draws its `triggers` edges to use cases and BUCs.
- The diagram draws no states: `%% omitted: state transitions` says a drawn use case
  drives a transition, which `--kind state` shows.
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
