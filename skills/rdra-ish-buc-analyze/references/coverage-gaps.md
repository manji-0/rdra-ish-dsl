# Coverage Gaps

Ask whether the business story is connected enough to review: who does what, in which
business, against which data, and which fields each actor supplies. Severity depends on
the stage ([stage-readiness.md](stage-readiness.md)).

## Commands

```sh
rdra-ish check src/
rdra-ish lint src/ --format table
rdra-ish list src/ --kind usecase --format table
rdra-ish list src/ --kind requirement --format table
rdra-ish csv src/ --kind matrix
rdra-ish csv src/ --kind business-inputs
```

## Signals

| Signal | Cause | Action |
|---|---|---|
| A use case with no performer, event, or timer | No actor assigned | `performs(Actor, UseCase)`; `performs(Actor, Buc)` only when the actor runs every use case |
| `lint` `rdra-actor-no-buc`, `rdra-extsystem-no-buc`, `rdra-entity-no-buc`, `rdra-condition-no-buc`, `rdra-variation-no-buc`, `rdra-timer-no-buc` | Declared, but no BUC reaches it | Connect it (`performs`, an event, CRUD, `applies_to`, a column type, `triggers`) or delete it |
| `lint` `rdra-transition-uc-no-buc` | A transitioning use case is in no BUC | `contains(Buc, UseCase)` |
| A BUC with no `belongs`, a use case with no `contains` | Orphan | Attach it, or remove it |
| A step with no `covers` | The flow is not tied to behavior | Link it to a use case, API, event, or timer |
| A field outside a screen, or actor-entered and unmapped | Missing UI link | `contains(Screen, Field)`, `maps_field`, or say it is external |
| `requirement-no-goal`, `goal-unmeasured`, an unscoped NFR | Broken traceability | `achieves`, a goal's `metric` and `target`, `applies_to` / `qualifies` / `constrains` |
| A blank CRUD row | The use case touches no data | Ask which entity it touches |
| An overloaded CRUD row | Several actions in one use case | Split it, or explain the transaction |
| An entity in no CRUD column | Premature, or a use case is missing | Add the touchpoint, or remove the entity |
| No `business-inputs` row once columns exist | No actor path, no write, or every column derived | Check `performs`, CRUD, defaults, generated keys, `sets` |
| Too many `business-inputs` columns for one use case | CRUD too coarse | Split the use case or API, or derive values with defaults and `sets` |
| A BUC-local predicate in `shared/`, a `module` that does not match its path, a redeclared symbol | Layout drift | Move it, fix `module`, or import ([layout](../../rdra-ish-write/references/layout.md)) |

## Next questions

- No performer: who performs this capability?
- Empty CRUD: which object does the use case create, read, update, or delete?
- Surprising inputs: which fields does the actor enter, and which come from defaults, keys,
  APIs, events, or `sets`?
- Unscoped NFR or requirement: which BUC, use case, API, system, or entity does it bind?
- Premature detail: defer it instead of inventing a deeper model.
