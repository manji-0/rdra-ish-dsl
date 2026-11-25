# Access and Permissions

Access is derived from the path an action takes, never attached to a screen:
`displays(UseCase, Screen)` and `invokes(UseCase, Api)` give the path,
`requires_permission` and `requires_medium` on the use case or API give what it needs, and
`has_permission(Actor, Permission)` gives what the actor holds. Each actor of a use case
must hold its permissions ([who performs a use case](../../rdra-ish-write/references/layout.md#who-performs-a-use-case)).

## Commands

```sh
rdra-ish check src/ --strict   # a permission gap is an error
rdra-ish csv src/ --kind screen-constraints
rdra-ish csv src/ --kind permission-callables
rdra-ish csv src/ --kind actor-permission-audit
```

`list --kind permission-callables` and `list --kind actor-permission-audit` print the same
views as tables.

## The three views

- `screen-constraints`: which permission and medium constraints reach each screen, one row
  per screen, use case, and API path. A screen with no constraint after the access stage
  needs a look.
- `permission-callables`: what each permission enables; `usecase_api_paths` lists the
  `UseCase->Api` calls with API-level requirements.
- `actor-permission-audit`: whether grants match what each actor performs. `ok`: a
  required permission is held. `missing`: the actor performs a path whose permission it
  lacks. `excess`: no performed path needs the grant.

`check` prints a missing grant as `warning: actor 'Clerk' is missing permission
'OrderWrite' required by usecase 'CancelOrder'`, and as an error under `--strict`.

## Signals

| Signal | Action |
|---|---|
| `missing` | `has_permission`, or change who performs the use case |
| `missing` for an actor who performs only the BUC | `performs(Actor, Buc)` made it a performer of every use case: move it to the use cases it runs, or grant the permission |
| `excess` | Remove the grant, add the missing requirement, or say the grant is outside the model |
| A permission enables too much | Move `requires_permission` from the use case to the API, or split the permission |
| A required API no use case invokes | Add `invokes`, or remove the API |
| `belongs(...).by("text")` | Use a declared `medium` |

Put a requirement on the use case when the whole interaction needs it, and on the API when
only the backend operation does. Cite the view: "`actor-permission-audit`: `missing` for
Clerk / OrderWrite via CancelOrder".
