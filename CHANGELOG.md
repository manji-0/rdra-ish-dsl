# Changelog

## Unreleased

### Breaking

- `rdra-ish states` treats undriven comparison propositions and unevaluated
  multi-entity / quantifier / temporal-comparison constraints as `[error]`
  (non-zero exit on table/json). Previously these were `[warn]` and could leave
  a green exit while rules were inert or TLA-only.
- `PatternCapReached` (pattern truncation via `--max-patterns`) is `[error]` /
  fail-closed: constraints checked on a truncated set are not sound.
- `rdra-ish check` uses the same state-diagnostic severity as `states` (reachable
  forbidden / NotEvaluated / undriven comparisons / truncation → `error:` and
  exit 1). Previously those were printed as `warning:` before `OK: no errors`.
- `states --format csv` / `type-script` exit non-zero on Error-severity state
  diagnostics (exit no longer depends on scanning `[error]` in rendered text).
- `diagram` / `csv` / `list` / non-TLA `export` refuse to emit when state
  diagnostics are Error severity (aligned with fail-closed docs). Pass
  `--allow-tla-only` on those generators to demote undriven / NotEvaluated gaps.
  TLA export gates on `[TLA_FATAL:…]` (including out-of-range IntLits and frozen
  proposition axes).
- `ConflictingEffects` (same-origin dual assigns, including Int `sets`) is
  `[error]` / fail-closed (was warning with last-wins).
- Junction composite unique uses the N:M endpoint FK pair only (extra FKs on the
  junction are excluded). Sharing one `.via` across multiple N:M pairs is an error
  (`SharedJunctionVia`).
- TLA `.cfg` emits separate `INVARIANT TypeOK` and `INVARIANT Safety` (Safety no
  longer folds TypeOK).
- `actors_for_usecase` unions BUC-level and direct `performs` (direct no longer
  hides BUC performers in permission analysis).
- Removed the kinds no review output consumed. A model that still uses one now
  fails to parse with a message naming the keyword and the replacement:
  - `usagescene`: use `timing` for Business-BUC context.
  - `location` and the `.where(...)` clause of `belongs`: no output consumed
    them. Delete the declaration and the clause; keep `timing` / `.when(...)`
    and `medium` / `.by(...)`.
  - `state`: states are the variants of the lifecycle Enum column. Delete the
    declaration; the analyzer still derives `{Entity}_{variant}` state nodes from
    `transitions` for the state diagram.
  - `condition`, `variation` (reserved kinds that only warned before): model
    rules with use case conditions (`precondition`, `guard`, ...) or entity
    state constraints.
  - `concept`, `domain_object`, `aggregate`, `value_object` (alias
    `valueobject`): conceptual-model kinds were removed; model persistent
    structure with `entity` (and `dto` for payloads).
  - `maps_to` predicate: removed with the conceptual-model kinds.
  - `contains(Aggregate, DomainObject | ValueObject | Concept)` and the
    Concept / DomainObject / Aggregate / ValueObject targets of `decides` are
    gone; `decides` keeps its other targets.
  - `rdra-ish list --kind` no longer accepts `concept`, `domain-object`,
    `aggregate`, or `value-object`; `diagram --node-kind` / `--edge-kind` no
    longer accept the matching node kinds or `maps-to`. The `domain-object` and
    `value-object` lint orphan-node kinds and the `ReservedKind` diagnostic are
    gone, as are the `rdra-ish-core` types `Concept`, `DomainObject`,
    `Aggregate`, `ValueObject`, `UsageScene`, `Condition`, `Variation`,
    `ConceptualRef`, and `ConceptMapping`.

### Added

- `export --kind tla` / `verify --backend tlc` accept `--instance-count` (default
  2) and `--int-range-max` (default 5). An approximation banner is always emitted
  when multi-instance or Int axes are used. `--int-range-max 0` is rejected;
  `--instance-count 1` warns that singleton ids cannot distinguish instances.
- State diagnostics for undriven / NotEvaluated cases include evaluator hints
  (states vs TLA).
- `relate(A, B, N:M).via(Junction)` — many-to-many with an explicit junction
  entity; auto-generates both FK columns on the junction and a composite unique
  on the FK pair when missing. Bare `N:M` is an error. Also synthesizes `N:1`
  edges `Junction→A` / `Junction→B` so TLA owner links, ER, and `.along(A,
  Junction, B)` see the path. Self N:M and `.via` equal to an endpoint are
  errors; unresolved `.via` does not also emit bare-`N:M` `NMRelation`.
- OpenAPI export: `{path}` parameters, unknown `auth` as `apiKey` +
  `x-rdra-ish-auth-scheme`, and `x-rdra-ish-requires-permission` from
  `requires_permission(Api, Permission)`.
- Lint `flow-cycle` for `precedes`/`branches` cycles; empty models score
  `coverage-score` 0 with missing-stage penalties (−8 each).
- `states --format typescript` accepts alias `type-script`; `--entity` prints a
  stderr note when other entities still have Error-severity diagnostics.
- `condition` / `variation` kinds emit `ReservedKind` warnings (still accepted).
- TLA owner links keep every `(child, parent)` pair (junction with two parents
  uses `{Child}_{Parent}_owner`); owner-only entities still get `*_Ids`.
- Multi-hop `.along` adds binders for intermediate entities and warns instead of
  silently dropping the link filter.
- `check` / `states --allow-tla-only` demotes undriven comparisons and
  NotEvaluated diagnostics to warnings for exit (PatternCap / forbidden stay
  fail-closed).
- Import scopes keep same-id bindings of different kinds (e.g. `event Capture` +
  `usecase Capture`); `kind::Id` resolves across modules. CI runs `check` on
  sample models including `samples/ec-site`.
- `DrivenComparisonContradictsEffects`: `sets(..., cmp, bool)` that disagree with
  same-origin Int `sets` are Error (closes check-green / TLC-red dual-drive gap).
- TLA export rejects `sets` Int literals and comparison RHS IntLits outside
  `0..int_range_max` with `[TLA_FATAL:int_literal_out_of_range]`.
- Date / DateTime column-vs-column comparisons export as Int arithmetic (not
  frozen BOOLEAN props); never-assigned prop axes are
  `[TLA_FATAL:frozen_proposition_axis]`.
- `--allow-tla-only` on `diagram` / `csv` / `list` / non-TLA `export`.
- Sequence emitters (PlantUML / Mermaid) use `actors_for_usecase` (BUC ∪ direct
  performers) so a direct `performs` no longer hides BUC-level actors.
- `Assign_*` actions are labeled as synthetic nondet; `verify` notes when a
  counterexample mentions `Assign_`.
- `check --strict` promotes permission/access gaps to errors; `check` always
  notes outstanding temporal properties / TLA-only obligations.
- OpenAPI emits `oneOf` (+ `x-rdra-ish-ambiguous-status`) for multiple
  `response` / `error_response` DTOs.

### Changed

- Language reference separates reserved kinds and unsupported/limited topics;
  UC `precondition`/`postcondition`/`guard` are documented as review metadata
  only (use `after.assert` for machine-checked posts).
- Bare `relate(..., N:M)` without `.via` is an error (was a warning).
- `coverage-score` no longer rates empty models as 100; stage-readiness gaps
  reduce the score; finding penalties are size-normalized (`×25/elements`).
- `lint` no longer reports `orphan-node` or `naming-id` on derived state nodes:
  a transition now counts as a connection for the states it joins, and state ids
  (`{Entity}_{variant}`) are tool-generated, so the naming rule skips them.
## v0.2.0 - 2026-07-20

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
- Parse errors print human-readable “unexpected …; expected …” messages (no
  chumsky Debug dumps); `N:M` / temporal hints match the current surface;
  `value_object` is accepted alongside `valueobject`.
- Language-ref multi-entity `invariant` examples are valid; `N:M` is documented
  as unsupported (use an intermediate entity); Stage 5 no longer requires
  standalone `state` declarations; `condition`/`variation` marked reserved.
- User-facing multi-entity state diagnostics say “multi-entity” (ids may still
  be `Cross*`); OpenAPI lint warns when APIs lack both method and path;
  README learning path leads with incremental modeling.
- TLA `.cfg` sets `CHECK_DEADLOCK FALSE` so terminal lifecycle states are not
  reported as TLC deadlocks; CI TLC smoke-check accepts `-help` non-zero exit.
  Multi-instance exports always `EXTENDS Integers` for `1..InstanceCount`.
- Import scopes honor `as` aliases, selective imports, and namespaced refs;
  same id may exist in different modules when referenced via aliases.
  Closed scope is per-file (siblings without `import` stay open-world);
  unknown modules, duplicate module paths, and conflicting `import` All
  bindings are errors; module-scoped resolution no longer falls back to
  another module's unique match.
- Formal-verify CI exports `PATH` in the TLC install step and requires
  TLC-native fingerprints (`tlc_result: ok` / `tlc_result: counterexample`).
- Duplicate `import … as` aliases are errors; Init prefers the first Enum
  variant when no `states`/`@default` seed exists.
- FV docs/skills align with unconstrained Assign/TickNow and `now_coupon`
  as an expected TLC fail; cli-reference documents fail-closed generators;
  language-reference documents composite `@pk` FK limits.
- `after.assert` exports as independent `[][action => post]_vars` properties
  (no primed injection into SpecActions); stuttering-only Next is fatal;
  TLA obligation failures use `[TLA_FATAL:…]` tags only (legacy substring
  matching removed); multi-post asserts fold to `Act => (p1 /\ p2)`.
- Init `"pending"` last-resort is `[TLA_FATAL:init_pending_fallback]` unless
  `pending` is a declared Enum variant; `diagram --node-kind`/`--edge-kind`
  reject unknown kinds; `scripts/install-tlc.sh` verifies tla2tools.jar SHA256.
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
