# API and System Boundaries

A system's entities derive from its APIs: `contains(System, Api)` places an API, the API's
CRUD says which entities it operates, and `invokes(UseCase, Api)` ties a business action to
it. `coordinates(UseCase, Entity, Entity)` explains a `relate` that crosses two systems.
`owns(System, Entity)` records intended ownership before the API CRUD exists, and `check`
compares it with the derived one.

## Commands

```sh
rdra-ish check src/
rdra-ish list src/ --kind api --format table
rdra-ish csv src/ --kind api-matrix
rdra-ish export src/ --kind openapi --out out/openapi.json
rdra-ish diagram src/ --kind rdra --format mermaid --buc <BucId>
```

## Signals

| Signal | Action |
|---|---|
| An API no use case invokes | Add `invokes`, or remove or defer the API |
| An invoked API with no CRUD | Add its CRUD, or reconsider it |
| An API in no system | `contains(System, Api)` once ownership matters |
| An API with method and path but no DTO | `request`, `response`, `error_response` |
| One API touches unrelated consistency groups | Split it by atomic boundary |
| `OwnedEntityWithoutApiOperation` | Add the API operation, or accept it as future ownership |
| `ApiOperatesEntityOutsideOwner` | Move the API, or revise `owns` |
| `EntityInMultipleSystems`, `EntityOwnedByMultipleSystems` | APIs of several systems operate, or several `owns` claim, one entity: split ownership, or model the coordination |
| `CrossSystemEntityRelation` | `coordinates(UseCase, A, B)`, or revisit ownership |
| `CoordinationMissingApi` | The coordinating use case invokes an API on each side |
| `CoordinationNotCrossSystem` | Remove or retarget `coordinates` |

```rdra
coordinates(ChangeCustomer, Order, CustomerAccount)
invokes(ChangeCustomer, OrderApi)
invokes(ChangeCustomer, CustomerApi)
```

Each invoked API operates its side's entity; a use case that invokes one side leaves the
consistency incomplete. Name the diagnostic, the entity pair, and the coordinating use case.
