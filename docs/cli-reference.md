# CLI Reference

The `rdra-ish` command-line tool parses `.rdra` sources, type-checks them, runs
model-consistency checks, and produces diagrams, CSV, listings, exports, and
state-pattern derivations.

```
rdra-ish <SUBCOMMAND> <INPUTS...> [OPTIONS]
```

`<INPUTS...>` is one or more files and/or directories. Directories are searched
recursively for `.rdra` files. At least one input is required. When loading, the tool
merges all reachable `.rdra` files into a single semantic model and resolves imports
against include paths derived from the input layout (it walks up from each input so that,
e.g., `shared/actors.rdra` resolves from a root containing both `shared/` and `buc/`).

Diagnostics are written to stderr as `error: ...` or `warning: ...`. Most subcommands
print diagnostics and still produce output; `check` exits non-zero on any error.

---

## `check`

Parse, type-check, and run whole-model consistency checks; produce no artifact output.

```
rdra-ish check <INPUTS...>
```

Prints each diagnostic to stderr. If any error (non-warning) is present, exits with
status `1` before running secondary consistency checks. Otherwise it reports unresolved
model warnings and prints `OK: no errors`.

<!-- derived-from ./state-derivation.md#diagnostics -->
`check` includes the same broad review signals exposed elsewhere:

- actor permission coverage for `requires_permission` on use cases and invoked APIs,
  including missing and excess actor-side assignments inferred from performer paths;
- API invocation and API entity-operation gaps;
- system boundary derivation and cross-system entity coordination gaps;
- FK/API transaction-boundary warnings;
- event-flow gaps, including events that are never raised, raised events that trigger
  no transition/use case/BUC, or triggered use cases with no containing BUC. Mark
  intentional external publications with `outbox(Event)` to suppress only the
  raised-but-unconsumed warning;
- state-pattern warnings, including missing creation paths, unreachable enum variants,
  forbidden reachable states, invariant / required / exclusive violations, temporal
  anchor assertion diagnostics, to-many quantifier evaluation gaps, and pattern
  truncation.

| Argument | Type | Description |
|---|---|---|
| `<INPUTS...>` | paths (required) | Files and/or directories to load. |

---

## `lint`

Audit model coverage and review readiness.

```
rdra-ish lint <INPUTS...> [--format <FORMAT>]
```

`lint` is review-oriented rather than a replacement for `check`. It includes
parse/model diagnostics and whole-model consistency warnings, then adds modeling
coverage findings such as orphan nodes, untraced requirements, unscoped NFRs or
constraints, empty BUCs/flows, unmapped screen fields, unused APIs/DTOs, incomplete
API method/path contracts, and naming convention warnings.

It also emits two `info` rows:

- `coverage-score` is a lightweight 0-100 readiness signal derived from the current
  finding set.
- `stage-readiness` reports which refinement stages are present: `scope`,
  `business-flow`, `data`, `interaction`, `system-boundary`, and `rules`.

Lint warnings do not fail the command. Existing parse/model errors are emitted as
`semantic-error` rows and make the command exit with status `1`.

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--format` | `table` \| `json` \| `csv` | `table` | Output format. |

---

## `fmt`

Format RDRA DSL source files.

```
rdra-ish fmt <INPUTS...> [--write | --check]
```

The formatter parses each `.rdra` file and prints a canonical source layout:
top-level items are separated by one blank line, instance metadata is indented by
two spaces, entity/DTO columns are indented by two spaces, annotations use compact
spacing, and predicate calls keep their chain form on one line.

This is an AST-based formatter. It preserves model structure, identifiers, labels,
metadata values, columns, predicates, chains, tuples, and comparison expressions,
but it does not preserve comments or original blank-line grouping. Prefer
`--check` before `--write` when adopting it on an existing hand-commented file.

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to format. Directories are searched recursively for `.rdra` files. |
| `--write` | flag | off | Rewrite files in place. |
| `--check` | flag | off | Exit with status `1` and print `needs formatting:` rows when any file differs from the canonical format. |

Without `--write` or `--check`, formatted source is written to stdout. With multiple
input files, each output block is prefixed by a `// <path>` marker.

---

## `diagram`

Generate a diagram in PlantUML or Mermaid.

```
rdra-ish diagram <INPUTS...> [--kind <KIND>] [--format <FORMAT>] [--buc <ID>]... [--usecase <ID>]... [--diff-base <PATH>]... [--show-description] [--node-kind <KIND>]... [--edge-kind <KIND>]... [--view-preset <PRESET>] [-o <OUT>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `rdra` \| `boundaryless-graph` \| `er` \| `state` \| `sequence` \| `event-flow` \| `diff` \| `business-area` \| `technical-area` | `rdra` | The diagram kind. `rdra` = RDRA layered graph mapped onto the original four-layer structure; `boundaryless-graph` = dense relationship graph without RDRA layer boundaries; `er` = entity-relationship diagram; `state` = state machine; `sequence` = write-focused sequence diagram with FK-inferred transaction boundaries; `event-flow` = event causality graph showing UC->Event->UC/BUC and Event->State chains; `diff` = graph diff between `--diff-base` and current inputs; `business-area` = Actor -> input field -> UseCase; `technical-area` = System boxes containing only APIs and Entities. |
| `--format` | `puml` \| `svg` \| `png` \| `mermaid` | `puml` | Output format. `puml` writes PlantUML text (`.puml`); `mermaid` writes Mermaid text (`.mmd`); `svg` / `png` render via plantuml.jar. |
| `--buc <id>` | string (repeatable) | — (whole model) | Filter to one or more BUCs by id. With multiple ids, the **union** of reachable nodes across the named BUCs is shown. Applies to all diagram kinds. For `sequence` and `business-area`, only use cases directly contained in the selected BUCs are shown; event-triggered use cases in other BUCs are left to `event-flow`. |
| `--usecase <id>` | string (repeatable) | — (whole model) | Filter `sequence` or `business-area` diagrams to one or more use cases by id. Cannot be combined with `--buc`. |
| `--diff-base <path>` | path (repeatable) | — | Baseline model inputs for `--kind diff`. The normal `<INPUTS...>` are treated as the target model. |
| `--show-description` | flag | off | Render element `description` metadata as diagram annotations where supported. Mermaid emits linked annotation nodes; PlantUML emits `note right of ...` notes. Currently applies to `rdra` and `boundaryless-graph` diagram emitters. |
| `--node-kind <kind>` | string (repeatable) | — (all nodes) | Filter graph diagrams to specific node kinds. Applies to `rdra`, `boundaryless-graph`, and `diff`. Examples: `actor`, `buc`, `flow`, `step`, `usecase`, `api`, `entity`, `domain-object`, `value-object`, `field`. Alias: `--kind-filter`. |
| `--edge-kind <kind>` | string (repeatable) | — (all edges) | Filter graph diagrams to specific relation kinds. Applies to `rdra`, `boundaryless-graph`, and `diff`. Examples: `contains`, `invokes`, `reads`, `writes`, `creates`, `maps-field`, `maps-to`, `owns`, `relate`. Alias: `--edge-filter`. |
| `--view-preset <preset>` | `business` \| `system` \| `data` \| `api` \| `ui` | — | Apply a graph filter preset. Explicit `--node-kind` or `--edge-kind` values override the preset for that dimension. |
| `-o`, `--out` | path | `out` | Output file path. The extension is added automatically based on `--format` (`.puml`, `.mmd`, `.svg`, `.png`). |

Notes:

- Diagram object labels include kind prefixes (`👤 actor`, `📦 BUC`, `✅ usecase`,
  `🖥️ screen`, `🔌 api`, `🗄️ entity`, `⚡ event`, `🔄 state`) while keeping DSL ids
  unchanged for edges and references.
- Descriptions stay out of normal node labels. Use `--show-description` when the review
  needs explanatory notes without changing the model's primary names.
- Graph filters are intentionally review-oriented. They hide nodes and any edges whose
  endpoints are hidden; they do not change the model or diagnostics. Use `--view-preset`
  for common slices, then override with explicit `--node-kind` or `--edge-kind` when a
  review needs a sharper lens.
- `boundaryless-graph` keeps its existing business/data focus and omits API nodes even
  when graph filters are present. Use `--kind rdra --view-preset api` when API-contract
  nodes should be part of the slice.
- For `--kind sequence`, the tool additionally runs FK-based transaction-boundary
  inference and emits a `warning:` to stderr for any FK-isolated write within a use case
  that also has an FK-connected write group. API diagnostics (`ApiNeverInvoked`,
  `ApiInvokedButNoEntity`) are also run and reported as warnings.
  FK connectivity is evaluated across the model's full `relate` graph, so sibling
  writes that share an unwritten parent entity are treated as one inferred transaction
  group rather than separate isolated writes.
- For `--kind event-flow`, the tool runs event-integrity diagnostics and emits `warning:`
  lines for events that are never raised, raised but consume nothing, or trigger a use
  case belonging to no BUC. `outbox(Event)` suppresses the raised-but-unconsumed warning
  for events intentionally published outside the local model. Event targets may be either
  BUCs or use cases. Mermaid node IDs are prefixed (`ev__`, `uc__`, `buc__`, `st__`) to
  avoid collisions when model elements share the same DSL identifier.
- For `--kind diff`, the command compares the graph induced by `--diff-base` against the
  current `<INPUTS...>`. Added nodes/edges are marked with `+`, removed nodes/edges with
  `-`, and label-only node changes with a changed style. The diagram includes only changed
  nodes plus unchanged context nodes needed by changed edges.
- For `--kind sequence`, participant lifelines are grouped into RDRA-style layer boxes:
  system value (`actor`), system boundary (`screen`, `api`), and system (`system`,
  `entity`). The use case itself remains the sequence section title.
- For `--kind rdra`, model objects are placed into four RDRA-style layers:
  system value (`actor`, `requirement`, `adr`), system external environment (`business`, `buc`,
  `flow`, `step`, `usagescene`, `extsystem`, `condition`, `variation`), system boundary
  (`usecase`, `screen`, `field`, `event`), and system (`system`, `api`, `entity`, `state`).
- For `--kind business-area`, the diagram shows only business actors, inferred input
  fields, and use cases. The input nodes are derived from `business-inputs`.
- For `--kind technical-area`, each system is rendered as a container holding only its
  APIs and the entities those APIs operate through CRUD predicates.
- For `--kind boundaryless-graph`, the same relationships are rendered as one flat graph
  for dense link inspection. API nodes are omitted there so the graph stays focused on
  business and data relationships.
- `--format svg` and `--format png` require plantuml.jar to be discoverable (see
  [Environment Variables](#environment-variables)). If it cannot be found, the command
  fails with an error.

---

## Sample Artifact Snapshots

CI runs `scripts/check-sample-artifacts.sh` after the Rust snapshot tests. The script
regenerates representative artifacts from
`samples/incremental-order/step-6-business-rules` and `samples/api-contract`, then
fails if the tracked files under those samples' `out/` directories change. This makes
DSL or emitter changes visible as a normal PR diff for Mermaid diagrams, PlantUML
diagrams, CSV review tables, state-pattern output, and representative exports such as
OpenAPI, AsyncAPI, DBML, and JSON Schema.

---

## `csv`

Generate CSV output.

```
rdra-ish csv <INPUTS...> [--kind <KIND>] [-o <OUT>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `actor` \| `entity` \| `matrix` \| `api` \| `api-matrix` \| `screen-constraints` \| `permission-callables` \| `actor-permission-audit` \| `business-inputs` | `entity` | CSV kind. `actor` = actor list; `entity` = entity/column list; `matrix` = use-case × entity CRUD matrix; `api` = API contract list including method/path/idempotency/mode/auth; `api-matrix` = API × entity CRUD matrix; `screen-constraints` = screen × UC/API permission/medium paths; `permission-callables` = permission × callable UC/API list derived from `requires_permission`, including `UseCase->Api` paths for API-level requirements; `actor-permission-audit` = actor × permission assignment audit inferred from UC/API requirements; `business-inputs` = Actor x inferred input field x UseCase rows derived from C/U/W paths. |
| `-o`, `--out` | path | `out` | Output file path. If no extension is given, a default is appended (`actor.csv` / `entity.csv` / `matrix.csv` / etc.). |

The command writes the CSV to the output path and prints `wrote <path>`.

---

## `export`

Export machine-readable or review artifacts from the model.

```
rdra-ish export <INPUTS...> [--kind <KIND>] [-o <OUT>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `openapi` \| `asyncapi` \| `dbml` \| `json-schema` \| `mermaid-er` \| `plantuml-er` | `openapi` | Export kind. `openapi` emits an OpenAPI 3.0 JSON document from API `method`/`path` metadata and DTO `request` / `response` / `error_response` relations. `asyncapi` emits an AsyncAPI 3.1 event catalog from `event`, `raises`, `triggers`, `transitions`, and `outbox`. `dbml` emits a DBML schema from logical `entity` declarations, indexes, unique constraints, generated FK columns, and `relate` options. `json-schema` emits JSON Schema Draft 2020-12 definitions for DTO and Entity structures. `mermaid-er` and `plantuml-er` emit textual ER review artifacts from the same logical data model projection used by `diagram --kind er`. |
| `-o`, `--out` | path | `out` | Output file path. If no extension is given, `openapi.json`, `asyncapi.json`, `schema.dbml`, `json-schema.json`, `er.mmd`, or `er.puml` is appended. |

For OpenAPI export, only APIs that declare both `method` and `path` become
`paths` operations. DTOs are emitted under `components.schemas`.

For AsyncAPI export, each RDRA `event` becomes a channel and message. `raises`
and `outbox` produce `send` operations; `triggers` and `transitions` produce
`receive` operations. RDRA does not yet model broker protocol, server topology,
or event payload DTOs directly, so those details are left unspecified and RDRA
facts are preserved with `x-rdra-ish-*` extensions.

For DBML export, conceptual model elements are not emitted directly; they become
part of the schema only through explicit `maps_to(..., Entity)` decisions. DBML
does not have native equivalents for every review annotation, so `@check`,
`@soft_delete`, `@history`, `@tenant`, and `@derived` are preserved as column notes.

For JSON Schema export, DTOs and Entities are emitted under `$defs` as
`Dto.<id>` and `Entity.<id>` so cross-kind ids cannot collide. Standard structural
shape is represented with JSON Schema keywords; RDRA-specific metadata such as PK,
FK, indexes, checks, soft delete, history, tenant scope, and derived expressions is
preserved with `x-rdra-ish-*` extensions.

For Mermaid ER and PlantUML ER export, the output is intentionally the same text-level
review artifact as `diagram --kind er --format mermaid` or
`diagram --kind er --format puml`. Use `diagram` when choosing a diagram view during
interactive review; use `export` when a downstream tool or CI job expects a named
artifact kind.

The command writes the artifact to the output path and prints `wrote <path>`.

---

## `list`

List model elements in a human-readable form.

```
rdra-ish list <INPUTS...> [--kind <KIND>] [--format <FORMAT>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `actor` \| `entity` \| `requirement` \| `adr` \| `adr-impact` \| `nfr` \| `quality` \| `constraint` \| `concept` \| `domain-object` \| `aggregate` \| `value-object` \| `buc` \| `flow` \| `step` \| `usecase` \| `field` \| `system` \| `api` \| `dto` \| `permission-callables` \| `actor-permission-audit` \| `business-inputs` | `actor` | The element kind to list. `actor` / `quality` / `concept` / `domain-object` / `aggregate` / `value-object` / `buc` / `flow` / `step` / `system` list id+label; `usecase` lists UC preconditions, guards, postconditions, alternatives, errors, and description; `requirement` lists requirement metadata; `adr` lists decision metadata plus impacted targets from `decides`; `adr-impact` lists one row per ADR-target pair; `nfr` and `constraint` list non-functional metadata; `field` lists screen-field metadata plus Entity.column mappings; `api` lists API contract metadata; `dto` lists DTO fields; `entity` lists each column with type, PK/unique/index/FK flags, FK optionality/actions, check constraints, soft-delete/history/tenant markers, and derived expressions; `permission-callables` lists each permission with callable use case/API ids and API-level `UseCase->Api` paths derived from `requires_permission`; `actor-permission-audit` lists inferred actor-side assignment gaps; `business-inputs` lists inferred actor-entered fields. |
| `--format` | `table` \| `json` \| `csv` | `table` | Output format. |

Output is written to stdout.

For `--format table`, an empty result is explicit rather than silent, for example
`No APIs found.`. CSV still prints only the header row and JSON prints `[]`.

---

## `states`

Derive the reachable state patterns per entity, aggregated across BUCs. See
[state-derivation.md](./state-derivation.md) for the algorithm.

```
rdra-ish states <INPUTS...> [--format <FORMAT>] [--buc <ID>]... [--max-patterns <N>] [--entity <ID>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--format` | `table` \| `csv` \| `json` | `table` | Output format. |
| `--buc <id>` | string (repeatable) | — (whole model) | Restrict the reachable scope to the union of the named BUCs. |
| `--max-patterns` | unsigned integer | `256` | Per-entity cap on the number of patterns before truncation. When exceeded, the entity result is marked `truncated` and a `PatternCapReached` diagnostic is recorded. |
| `--entity <id>` | string | — (all entities) | Restrict output to a single entity id. Filtering is applied to whichever output format is selected. |

Output is written to stdout.

---

## Environment Variables

| Variable | Description |
|---|---|
| `PLANTUML_JAR` | Path to `plantuml.jar`. Required when rendering `--format svg` or `--format png` with the `diagram` subcommand. The renderer discovers the jar from this variable; if it is unset or the jar is not found, SVG/PNG rendering fails with `failed to find plantuml.jar`. |

---

## See Also

- [language-reference.md](./language-reference.md) — the DSL syntax.
- [incremental-modeling.md](./incremental-modeling.md) — staged abstract-to-concrete modeling flow.
- [state-derivation.md](./state-derivation.md) — how `states` computes its output.
