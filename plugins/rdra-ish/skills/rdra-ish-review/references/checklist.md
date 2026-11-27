# Review Checklist

Judge each item at the model's stage: a gap the next stage fills is a question, not a
defect. The analysis references of skill `rdra-ish-buc-analyze` explain each diagnostic;
this list says what to look for.

## Layout and imports

- Each file starts with a `module` that mirrors its path; BUC-specific predicates stay in
  `buc/` ([layout](../../rdra-ish-write/references/layout.md)).
- Each symbol used is imported; no import is unused; nothing is declared twice.

## Actors, BUCs, and traceability

- Each use case has a performer, directly or through its BUC, or an event or timer
  starts it. Each `performs(Actor, Buc)` is meant for every use case of the BUC.
- Each BUC `belongs` to a business. Each `extsystem` raises or receives an event; each
  `timer` triggers a use case or BUC.
- Requirements `motivates` a BUC and `achieves` a measured goal (`lint`
  `requirement-no-goal`, `goal-unmeasured`). NFRs, qualities, and constraints are scoped
  with `applies_to`, `qualifies`, or `constrains`. An ADR `decides` only a real decision.
- The `lint` `rdra-*` rules name an actor, external system, entity, condition, variation,
  or timer no BUC reaches.

See [coverage-gaps.md](../../rdra-ish-buc-analyze/references/coverage-gaps.md).

## Use cases, flows, screens

- Each use case is in a BUC, displays a screen or is started by an event or timer, and
  touches data directly or through an API.
- Ordered, branching, exceptional, or repeated behavior uses `flow` and `step`, each step
  `covers` behavior.
- Each `field` is in a screen; an actor-entered one maps to a column with `maps_field` or
  is external on purpose. `precondition`, `guard`, and `postcondition` are metadata; a
  checked postcondition is `after(UseCase).assert(...)`.

## Access, APIs, systems

- Permissions and media are on the use case or API, granted with `has_permission`, never
  on a screen. Read the three access CSVs with
  [access-permissions.md](../../rdra-ish-buc-analyze/references/access-permissions.md).
- Each API is invoked, belongs to one system, operates entities, and has DTOs when a
  contract is in scope. Each cross-system `relate` has a coordinating use case. See
  [api-system-boundaries.md](../../rdra-ish-buc-analyze/references/api-system-boundaries.md).

## Entities

- Each `relate` has the intended direction and cardinality; no hand-written key collides
  with a generated one (`FkConflict`); two relates of a pair carry distinct `.role(...)`s.
- Annotations state real constraints; a value an actor chooses is a `variation`.
- Each list or search read declares its shape, and `query-index-missing` is resolved or
  accepted.

## Lifecycle and rules

- Each state change names its use case; each changing Enum, `Bool`, `@null`, or Int column
  has a `transitions` arm or a `sets`.
- `check` reports no `Witness_*` or `Deadlock_*`; each `Runs_*`, `Vacuous_*`,
  `UnjoinedWrite_*`, and rule error is understood. See
  [state-patterns.md](../../rdra-ish-buc-analyze/references/state-patterns.md).
- `diagram --kind event` prints no `EventNeverRaised`, `EventNeverConsumed`, or
  `TriggeredUseCaseUnreachable` on stderr.
- Each rule has a `.label`; each multi-entity rule has `.along(...)` or `.global()`. A rule
  that needs two rows, and each `property`, is checked with `verify` (skill
  `rdra-ish-verify`).
- A deadlock where staying is right is declared with `@end(value)`, not a dummy arm.

## Generated artifacts

When the change claims a contract, run the export: `openapi` for APIs and DTOs,
`asyncapi` for events, `dbml`, `sql`, or `json-schema` for data. When the model has a
`[code.<lang>]` table, run `check --code` (skill `rdra-ish-implement`).
