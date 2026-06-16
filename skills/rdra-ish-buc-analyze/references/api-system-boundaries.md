# API and System Boundary Analysis

Use this reference when reviewing whether APIs, systems, entity ownership, and
cross-system coordination are modeled coherently.

## Concept

In RDRA-ish, system entity sets are derived from APIs by default:

- `contains(System, Api)` assigns an API to a system boundary.
- CRUD predicates on the API show which entities that system boundary operates.
- `invokes(UseCase, Api)` connects a business action to that boundary.
- `coordinates(UseCase, Entity, Entity)` explains intentional cross-system consistency
  when a `relate` edge crosses derived system entity sets.
- `owns(System, Entity)` can record intended future ownership before complete API CRUD
  exists; diagnostics compare explicit ownership with derived ownership.

This makes the API layer the explicit boundary between business actions and data
ownership.

## Commands

```sh
rdra-ish check src/
rdra-ish list src/ --kind system --format table
rdra-ish list src/ --kind api --format table
rdra-ish csv src/ --kind api-matrix
rdra-ish export src/ --kind openapi --out out/openapi.json
rdra-ish diagram src/ --kind sequence --format mermaid --buc <BucId>
rdra-ish diagram src/ --kind er --format mermaid --buc <BucId>
```

## What To Look For

| Signal | Interpretation | Next action |
|---|---|---|
| API declared but never invoked | Boundary is unused or future-facing | Add `invokes(UseCase, Api)` or remove/defer the API |
| API invoked but has no entity CRUD | Boundary does not operate modeled data | Add CRUD to API or reconsider the API |
| Stable API has no `contains(System, Api)` | System ownership is unknown | Add owning system when boundary matters |
| `OwnedEntityWithoutApiOperation` warning | Explicit owner has no API CRUD yet | Add intended API operation or accept as future ownership |
| `ApiOperatesEntityOutsideOwner` warning | API CRUD conflicts with explicit ownership | Move API/system ownership or revise `owns` |
| API has method/path but no DTO links | Contract is incomplete | Add `request`, `response`, or `error_response` DTOs |
| One API touches unrelated consistency groups | API may be too broad | Split by atomic consistency boundary |
| API matrix disagrees with sequence expectation | UC/API/entity path is wrong | Fix `invokes` or API CRUD |
| `CrossSystemEntityRelation` warning | `relate` crosses derived system entity sets | Add coordination or revisit ownership |
| `CoordinationMissingApi` warning | Coordinating UC does not invoke API on one side | Add the missing `invokes` path |
| `CoordinationNotCrossSystem` warning | `coordinates` is declared for a non-cross-system pair | Remove or retarget `coordinates` |
| `EntityInMultipleSystems` warning | Entity is operated through multiple systems | Split ownership or model coordination explicitly |

## Cross-System How-To

When two related entities belong to different derived systems and one use case keeps
them consistent:

```rdra
coordinates(<UC>, <EntityA>, <EntityB>)
invokes(<UC>, <ApiForEntityA>)
invokes(<UC>, <ApiForEntityB>)
```

Each invoked API should operate the corresponding entity. If the use case only invokes
one side, the model says the consistency responsibility is incomplete.

## Reporting Tips

- Name the diagnostic code exactly when `check` reports one.
- Include the entity pair and the coordinating use case.
- Distinguish derived API ownership from explicit future ownership; `owns` should be
  intentional and diagnostics-reviewed, not a replacement for API CRUD.
