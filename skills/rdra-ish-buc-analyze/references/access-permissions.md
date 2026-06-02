# Access and Permission Analysis

Use this reference when reviewing permissions, operation media, screen paths, or actor
grant coverage.

## Concept

RDRA-ish does not attach access rules directly to screens. Access is derived from the
path a user action takes:

- `displays(UseCase, Screen)` says which screen participates in the use case.
- `invokes(UseCase, Api)` says which API boundary the use case calls.
- `requires_permission(UseCase|Api, Permission)` says which authority is required.
- `requires_medium(UseCase|Api, Medium)` says which operation medium is required.
- `has_permission(Actor, Permission)` says which permissions the actor has.

The three access CSVs answer different questions. Use them together.

## Commands

```sh
rdra-ish check src/
rdra-ish csv src/ --kind screen-constraints
rdra-ish csv src/ --kind permission-callables
rdra-ish csv src/ --kind actor-permission-audit
rdra-ish list src/ --kind permission-callables --format table
rdra-ish list src/ --kind actor-permission-audit --format table
```

## How To Read Each View

`screen-constraints` answers: "which permission/media constraints reach each screen
through the UC/API path?" Each row is a derived Screen x UseCase x Api? path. If a
screen appears with no constraints after the model is at interaction-boundary stage,
verify whether the action is truly unconstrained.

`permission-callables` answers: "what does this permission enable?" The
`usecase_api_paths` column shows which `UseCase->Api` invocations carry API-level
requirements. Unexpected UC/API rows usually mean `requires_permission` is attached too
broadly or to the wrong boundary.

`actor-permission-audit` answers: "do actor grants match performed operations?" Status
meanings:

- `ok`: actor has a permission required by a modeled performed UC/API path.
- `missing`: actor performs a UC/API path requiring a permission they do not have.
- `excess`: actor has a permission no modeled performed path currently requires.

## Common Findings

| Signal | Interpretation | Next action |
|---|---|---|
| `missing` audit row | Required operation lacks actor grant | Add `has_permission` or reconsider who performs the UC |
| `excess` audit row | Actor grant is unused by modeled paths | Remove it, add missing operation requirements, or mark as out-of-model grant |
| Permission enables too many callables | Requirement is too broad | Move `requires_permission` from UC to API or split permission |
| API requires permission but no UC invokes it | Boundary is disconnected | Add `invokes` or remove the API |
| UC/API declares `requires_medium` but screen path is not reviewed | Medium constraint may be invisible to UI review | Inspect `screen-constraints` |
| Contextual `belongs(...).when/.where/.by` uses untyped text | Context reference may not type-check as intended | Use declared `timing`, `location`, or `medium` where appropriate |

## How To Fix

Prefer precise authority:

- Put a requirement on the use case when the whole interaction needs that permission.
- Put a requirement on the API when the backend operation specifically needs it.
- Keep actor grants in shared actor/access vocabulary when reused across BUCs.
- Avoid screen-only access predicates; derive screen constraints from the UC/API path.

## Reporting Tips

Always report which view produced the finding. "Actor lacks permission" is less useful
than "`actor-permission-audit` reports `missing` for Actor X / Permission Y via
UseCase Z".
