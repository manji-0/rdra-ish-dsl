# State Pattern Analysis

Use this reference when reviewing lifecycle completeness, reachable state patterns,
terminal states, forbidden states, invariants, or state-pattern truncation.

## Concept

`rdra-ish states` derives reachable combinations of state-axis values per entity.
State axes come from Enum, Bool, nullable columns, state-machine transitions, and
explicit `sets` effects. The output is not just a diagram; it is executable feedback
about which combinations the modeled use cases and events can actually reach.

Use state analysis after entity structure is stable enough to know which fields can
change. Do not invent lifecycle detail during early BUC skeleton work.

## Commands

```sh
rdra-ish check src/
rdra-ish states src/
rdra-ish states src/ --buc <BucId>
rdra-ish states src/ --entity <EntityId>
rdra-ish states src/ --format csv
rdra-ish states src/ --format json --entity <EntityId>
rdra-ish diagram src/ --kind state --format mermaid --buc <BucId>
rdra-ish diagram src/ --kind event-flow --format mermaid
```

## Reading Table Output

Example shape:

```text
Entity: Order (注文)
  axes: status[pending|paid|shipped], delivered_at[null|present]

  STATUS   DELIVERED_AT  INITIAL  TERMINAL  VIA
  pending  null          yes      no        BucOrder/PlaceOrder
  paid     null          no       no        BucPayment/Capture
```

- `INITIAL`: reachable from a creation path.
- `TERMINAL`: no modeled use case/event transition exits that combination.
- `VIA`: which BUC/use case/event reached the pattern.
- Missing combinations may be good or bad. Treat them as design questions, not
  automatic defects.

## Common Findings

| Signal | Interpretation | Next action |
|---|---|---|
| Enum/Bool/nullable column never appears in axes | No modeled lifecycle effect | Add `sets`, `transitions`, or accept that it is not stateful |
| Expected combination never appears | Missing creation/effect path | Add or fix `creates`, `sets`, `raises`, or `transitions` |
| Unexpected terminal state | Missing exit use case/event | Add transition/effect or mark terminal as intentional |
| Too many combinations | Effects are too broad | Make `sets` more precise or add rules |
| `truncated: true` in JSON | State space exceeded cap | Raise `--max-patterns`, narrow with `--buc`, or inspect one entity |
| Forbidden reachable state | Rule violation | Fix lifecycle/effects or update the rule |
| Invariant violation | Required condition not always satisfied | Add missing `sets`/transition or narrow the invariant |
| `present` lacks type suffix where type matters | Nullable effect is vague | Use a PostgreSQL-type value such as `"timestamptz"` when useful |

## How To Fix

- Use `transitions(Event, FromState, ToState)` when a real lifecycle state machine
  exists.
- Use `sets(UseCase|Event, Entity, "column", "value")` for Enum values without a
  state machine, Bool flags, nullable columns, and derived lifecycle effects.
- Use `raises(UseCase, Event)` before expecting an event transition to occur.
- Use `triggers(Event, Buc)` for downstream BUC handoff, then refine to
  `triggers(Event, UseCase)` when the entry use case is known.

## Reporting Tips

Report state findings as patterns and paths, not just columns. Include the entity,
axis values, whether the pattern is initial/terminal, and the BUC/use case/event path
that reached it.
