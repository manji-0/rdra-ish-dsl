# Changelog

## Unreleased

### Breaking

- Conditions use comparison expressions only: `col == val`, `stock < selling`
  (no tuples or flat `col, val` pairs).
- Transitions require an enum column: `transitions(Entity.col, Event, from -> to)`.
  Global `state` labels are no longer used as transition endpoints.
- Cross-entity rules use multi-entity `forbidden(...)` / `invariant(...)` (with
  optional `.along(...)`); `cross_*` and `forbidden_when` surface forms are removed.
  Quantifiers are `when(...).none/has(...)`.
- `relate(..., N:1)` cardinality is unquoted. Temporal connectives prefer
  `and` / `or` / `not` (`/\` `\/` `~` remain as aliases).
- PostgreSQL type names are rejected in `sets`; use `present` for nullable non-null.

### Added

- `rdra-ish export --kind tla` and `rdra-ish verify --backend tlc` for TLA+/TLC.
  Export writes both `.tla` and `.cfg`. Coverage includes Int / Money / Decimal /
  `now` arithmetic axes, multi-entity `forbidden`/`invariant` (with `.along` /
  `*_owner`), `when(...).none/has`, `after(...).assert` equality and comparison
  postconditions, and multi-instance temporal quantifiers
  (`[](\A…)`, `\A i: <>…`, `\A i: (p ~> q)`).
- `property` label string is optional (`property StockOk always(...)`).
- Skill `rdra-ish-verify` for TLA+/TLC formal verification workflow (when to use
  TLA vs `states`, DSL surface, export/verify, bundled `samples/*.rdra`).
  Install via `npx skills` / `gh skill`; see `skills/README.md`.
- All skills declare `license: MIT` for Agent Skills / `gh skill publish` validation.

### Fixed

- Fail-closed validation: empty `Enum()`, lexer junk, unknown predicates / wrong
  arity, duplicate columns, missing input paths, and generator commands that
  previously succeeded on invalid models.
- Import scopes honor `as` aliases, selective imports, and namespaced refs;
  same id may exist in different modules when referenced via aliases.
  Closed scope is per-file (siblings without `import` stay open-world);
  unknown modules, duplicate module paths, and conflicting `import` All
  bindings are errors; module-scoped resolution no longer falls back to
  another module's unique match.
- Formal-verify CI exports `PATH` in the TLC install step and requires a
  `TLC verification failed` fingerprint for expected-fail samples.
- `diagram --kind diff` fail-closes on semantic errors in `--diff-base`.
- TLA+: unique action names for multi-edge events; unresolved/dropped properties
  and contradictory `after.assert` fail export/verify; `now` Safety is not baked
  into `Assign`/`TickNow`; sets-only entities emit actions.
- `states` exits non-zero when output contains `[error]`.
- Composite `@pk(a,b)` recorded on the entity; duplicate API method+path rejected.
- FV samples: `now_coupon` is an expected TLC counterexample (Safety is checked,
  not enforced by Next).

### Changed

- Formal-verification docs and skills use multi-entity `forbidden`/`invariant` surface
  names (diagnostic ids may still say `Cross*`). Int/`now` are documented as a separate
  TLC layer from BFS comparison propositions. Docs/skills point to `export --kind tla`
  for quantifiers, temporal `property`, and relation-scoped `.along`. Sibling skills
  route deep FV work to `rdra-ish-verify`. FV sample `.rdra` files are canonical under
  `skills/rdra-ish-verify/samples/` with repo path symlinks for tests. Samples table
  records TLC pass/fail intent; order snapshot tracks the skill sample.
- CI runs TLC (`formal-verify` job) against pass/fail FV samples.

## v0.1.7 - 2026-06-23

### Added

- Added typed semantic-model representations for state transitions, concept-to-entity
  mappings, and parsed predicates (`StateTransition`, `EntityLifecycle`,
  `ConceptualRef` / `ConceptMapping`, `TypedPredicate`).
- Added `EntityStateVariant` derivation for reachable lifecycle states as
  discriminated unions.
- Added TypeScript state-union export via `rdra-ish states --format typescript`
  and `rdra-ish export --kind typescript-states`.

### Changed

- Updated GitHub Actions workflows to Node.js 24.
- Added manual `workflow_dispatch` trigger for PyPI publishing.

## v0.1.6 - 2026-06-16

<!-- derived-from ./docs/language-reference.md#relationship-predicates -->
<!-- derived-from ./docs/cli-reference.md#export -->
<!-- derived-from ./docs/state-derivation.md#diagnostics -->

### Added

- Added first-class business-flow modeling with `flow`, `step`, `precedes`,
  `branches`, `excepts`, `repeats`, and `covers`.
- Added richer requirement, ADR, NFR, quality, constraint, conceptual-model,
  screen-field, DTO, and API contract vocabulary.
- Added data-modeling annotations for indexes, composite constraints, checks,
  FK optionality/actions, soft delete, history, tenant scope, and derived columns.
- Added OpenAPI, AsyncAPI, DBML, JSON Schema, Mermaid ER, and PlantUML ER exports.
- Added diagram filtering, view presets, diff diagrams, and description rendering.
- Added `rdra-ish lint` and `rdra-ish fmt` for model readiness and formatting.

### Changed

- System ownership can now be declared with `owns(System, Entity)` and compared
  against API-derived ownership diagnostics.
- Relation-scoped `.along(...)` cross constraints now evaluate representative
  operation-linked cases instead of always falling back to global cross-product
  behavior.
- Skill definitions under `skills/` now guide modeling with the expanded DSL
  vocabulary and release/export review paths.

### Breaking Changes

<!-- derived-from ./docs/language-reference.md#file-structure-and-comments -->

- Line comments in `.rdra` files use `//`.
- Legacy `#` line comments are not accepted by the parser. Existing models that used
  `#` comments must migrate those lines to `//`.

### Migration: `#` Comments to `//`

Use a mechanical line-comment replacement for `.rdra` sources:

```sh
find path/to/model -name '*.rdra' -exec perl -pi -e 's/^(\s*)#/$1\/\//' {} +
```

Review the result if your model uses `#` inside string literals or external snippets.
Block comments remain available as `/* ... */`.
