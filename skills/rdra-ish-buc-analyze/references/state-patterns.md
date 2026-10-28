# State Pattern Analysis

Use this reference when reviewing lifecycle completeness, reachable state patterns,
terminal states, entity constraints, cross-entity rules, or state-pattern truncation.

## Concept

`rdra-ish states` derives reachable combinations of state-axis values per entity from one
transition system that `check` and `verify` share. State axes come from inline Enum, Bool,
nullable columns, state-machine transitions, explicit `sets` effects, and Int columns that a
`sets` or a rule names. A column typed by a `variation` is an attribute the actor chooses;
it is an axis only when something names it, and its unreached values draw no warning. The
output is not just a diagram; it is executable feedback about which combinations the
modeled use cases can actually reach.
It evaluates every `forbidden`, `invariant`, `required`, and `exclusive` rule, every
multi-entity rule, every `when(...).none/has(...)` rule, and every `after(...).assert`
against the reachable states. Rules with `.along(EntityA, EntityB, ...)` count only the
instances that a `relate` links. A use case is one atomic action per transaction group,
and an entity that no use case creates starts present.

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
- `TERMINAL`: a use case deletes the instance in that combination, or no enabled use case
  moves it to another combination.
- `VIA`: which BUC/use case reached the pattern.
- Missing combinations may be good or bad. Treat them as design questions, not
  automatic defects.

## Common Findings

| Signal | Interpretation | Next action |
|---|---|---|
| Enum/Bool/nullable column never appears in axes | No modeled lifecycle effect | Add `sets`, `transitions`, or accept that it is not stateful |
| Enum variants reported unreachable that an actor simply chooses (payment method, channel) | The column is an attribute, not a lifecycle | Type it by a `variation`; `lint` `enum-attribute` finds these |
| Expected combination never appears | Missing creation/effect path | Add or fix `creates`, `sets`, `raises`, or `transitions` |
| Unexpected terminal state | Missing exit use case/event | Add transition/effect or mark terminal as intentional |
| Too many combinations | Effects are too broad | Make `sets` more precise or add rules |
| `truncated: true` in JSON | State space exceeded cap | Raise `--max-patterns`, narrow with `--buc`, or inspect one entity |
| Forbidden reachable state | Rule violation | Fix lifecycle/effects or update the rule |
| Invariant violation | Required condition not always satisfied | Add missing `sets`/transition or narrow the invariant |
| Required violation | Always-required fact is absent in a reachable pattern | Add missing `sets`/transition or relax the required fact |
| Exclusive violation | Mutually exclusive facts co-occur in a reachable pattern | Add missing transition/effect guards or revise the exclusivity rule |
| Cross-entity violation | Rule violation across entity patterns | Fix lifecycle/effects or update the cross rule |
| `UnbindableScope` | `.along(...)` names entities that no `relate` binds | `[error]`. Declare the `relate`, or drop `.along` for a rule over every pair |
| `UnbindableCondition` | A condition body is applied to a use case that writes nothing | `[error]`. Apply it to a use case that writes, or drop the body |
| `ContradictoryDrivenComparison` | A use case drives a comparison one way while its own Int `sets` make it the other | `[error]`. Fix the `sets` or drop the driving `sets` |
| `UnwrittenConstrainedVar` | A rule reads a column that no use case writes | Warning. The rule checks only the start value; add the `sets` or drop the rule |
| `NoCreationPath` | No use case creates the entity | Info. It starts present; add `creates` if that is wrong |
| `present` lacks type suffix where type matters | Nullable effect is vague | Prefer `present` (PG type names in `sets` are rejected) |

## Formal verification (TLA+)

For the dedicated workflow (when to use TLA, DSL surface, export/verify,
samples), load skill **`rdra-ish-verify`**.

`states` and `check` read the same system as TLC at one instance per entity. Use TLA+
when the model has:

- `property` (`always`, `eventually`, `leads_to`), which `states` does not evaluate
- rules that need two instances: `when(...).none/has(...)` over several rows, or
  `.along` between entities with more than one linked row

```sh
rdra-ish export src/ --kind tla -o out/
rdra-ish verify src/ --backend tlc -o out/   # requires tlc on PATH
```

Export writes both `RdraSpec.tla` and `RdraSpec.cfg`. See `docs/formal-verification.md`.

## How To Fix

- Use `transitions(Entity.col, UseCase, from -> to)` when a real lifecycle state
  machine exists; name the use case that drives each edge.
- Use `sets(UseCase, Entity, column == value)` for Enum values without a
  state machine, Bool flags, nullable columns, and derived lifecycle effects.
- Use `triggers(Event, Buc)` for downstream BUC handoff, then refine to
  `triggers(Event, UseCase)` when the entry use case is known.

## Reporting Tips

Report state findings as patterns and paths, not just columns. Include the entity,
axis values, whether the pattern is initial/terminal, and the BUC/use case/event path
that reached it. For temporal findings, cite TLC property names or the `INVARIANT` names
from the exported `.cfg`.
