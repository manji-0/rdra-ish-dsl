---
name: rdra-buc-analyze
description: Analyze a BUC or the whole model for coverage gaps, state patterns, consistency, and readiness for the next refinement stage
---

## Analyze a BUC

Run CLI commands to surface coverage gaps, state patterns, and consistency issues in a BUC or the whole model.

When the user is refining a model incrementally, identify the current abstraction
stage first and report the next information needed from the user.

### Quick analysis commands

```sh
# 1. Syntax and type check
rdra-ish check src/

# 2. List all BUCs
rdra-ish list src/ --kind buc --format table

# 3. List use cases
rdra-ish list src/ --kind usecase --format table

# 4. List actors
rdra-ish list src/ --kind actor --format table

# 5. Derive state patterns for all entities
rdra-ish states src/

# 6. Derive state patterns scoped to one BUC
rdra-ish states src/ --buc <BucId>

# 7. Derive state patterns for one entity
rdra-ish states src/ --entity <EntityId>
```

### Interpreting `rdra-ish states` output

The output shows every reachable combination of column values for each entity:

```
Entity: Order (注文)
  axes: status[pending|paid|shipped], delivered_at[null|present]

  STATUS   DELIVERED_AT  INITIAL  TERMINAL  VIA
  pending  null          yes      no        BucOrder/PlaceOrder
  paid     null          no       no        BucPayment/Capture
```

- **`INITIAL`** — the combination is reachable from a `creates` predicate
- **`TERMINAL`** — no use case transitions out of this combination
- **`VIA`** — which BUC/use case reaches this state
- A combination that never appears is unreachable — treat as a design gap if expected

### What to look for

#### Refinement readiness

| Current signal | Likely stage | Ask next |
|----------------|--------------|----------|
| BUCs exist but actors/use cases are sparse | Scope or BUC skeleton | actors and user-visible actions |
| Use cases exist but CRUD matrix is empty | BUC skeleton | entities touched by each use case |
| CRUD exists but sequence output has only `System` lane | Data touchpoints | screens and API boundaries |
| Entities have only `id` columns | Data touchpoints | fields, keys, and relationships |
| Entities have Enum/Bool/nullable columns but no state output changes | Entity structure | events, transitions, and `sets` effects |
| `states` shows unreachable variants or unexpected terminals | Lifecycle | missing use cases, events, transitions, or effects |
| Reachable states look stable | Lifecycle complete | forbidden states and invariants |

#### Coverage gaps

| Signal | Likely cause |
|--------|--------------|
| BUC has no `performs` | Actor assignment is missing |
| BUC has no `belongs` | Business domain assignment is missing |
| Use case has no CRUD predicate | UC is declared but not connected to data |
| Use case has no `displays` | No screen assigned — flag for intentional review |
| Entity column never appears in `states` axes | No `sets` or `transitions` predicate covers it |
| `TERMINAL` state is unexpected | Missing use case or transition to exit that state |
| Unreachable state combination | `sets` or `transitions` predicate may be missing or wrong |

#### State pattern anomalies

- **Too many patterns** — consider whether all combinations make business sense; prune impossible ones with `sets` precision
- **`truncated: true`** in JSON output — raise `--max-patterns` or narrow with `--buc`
- **`present` but no type suffix** — add a PostgreSQL-type `sets` value (e.g. `"timestamptz"`) for nullable datetime/json columns

#### Relationship integrity

- Run `rdra-ish list src/ --kind entity --format json` and check for FK columns that should be covered by a `relate` predicate
- Cross-reference CRUD predicates: a use case that `creates` a child entity without `creates`-ing the parent (or an explicit FK `sets`) may indicate a missing transaction boundary

### Reporting findings

```
## Analysis: <BUC or "whole model">

### Current refinement stage
- Stage: <scope | BUC skeleton | data touchpoints | interaction boundary | entity structure | lifecycle | business rules>
- Evidence: <commands or model signals>
- Next user information needed: <focused questions>

### State pattern summary
- Entity <Id>: <n> reachable patterns, <n> terminal, initial via <UC>
- ...

### Coverage gaps
- [high] BUC `<Id>`: no performs
- [medium] usecase `<Id>`: no displays predicate
- [low] entity `<Id>` column `<col>`: not tracked by any sets or transitions
- ...

### Anomalies
- Entity `<Id>`: state `(<col>=X, <col>=Y)` appears terminal but no use case exits it
- ...

### Recommendations
- Add `sets(<UC>, <Entity>, "<col>", "<val>")` for ...
- Add `transitions(event::<E>, <From>, <To>)` for ...
```
