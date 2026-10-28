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

Diagnostics are written to stderr as `error: ...` or `warning: ...`.

- `check` and `lint` exit non-zero when semantic errors (or lint errors) are present.
- Every generator subcommand refuses to emit when the model has parse or resolve errors.
  Whether Error-severity state diagnostics also stop it depends on whether the output
  prints the lifecycle. An output that prints it (`states`, `list`, `diagram` of every kind
  except `er`, `csv` of every kind except `entity`, `export --kind typescript-states`)
  is **fail-closed** and refuses. An output that reads only the data layer (`diagram
  --kind er`, `csv --kind entity`, and `export` of `dbml`, `json-schema`, `openapi`,
  `asyncapi`, `mermaid-er`, and `plantuml-er`) is written, and the command prints one
  stderr note that gives the error count and points to `rdra-ish check`.
  `export --kind tla` and `verify` gate on parse errors plus the Error
  diagnostics of the lifecycle transition system they print (built at `--instance-count`
  / `--int-range-max`). The code decides this per kind in `ExportKind::reads_lifecycle()`
  and `DiagramKind::reads_lifecycle()`.
- `fmt` only requires a parseable source; it does not run semantic analysis.

---

## `check`

Parse, type-check, and run whole-model consistency checks; produce no artifact output.

```
rdra-ish check <INPUTS...> [--strict]
```

Prints each diagnostic to stderr. If any error (non-warning) is present, exits with
status `1` before running secondary consistency checks. Otherwise it reports unresolved
model warnings and state-pattern findings. State diagnostics share severity with
`states`: Error-severity findings print as `error:` and cause exit `1`; only a clean
run prints `OK: no errors`. When the model has temporal `property` declarations,
`check` prints a trailing `note:` that those were not model-checked (use
`verify --backend tlc`).

A violated constraint or `after` assertion prints once, with a shortest counterexample:

```
error: invariant CrossForbidden_Order_0 is violated: multi-entity forbidden state is reachable across [Order, Payment]: Order.status=cancelled AND Payment.status=captured witnessed by Order(status=cancelled); Payment(status=captured)
  rule: cross_order_payment_split.rdra:41:1
  counterexample (3 steps):
    1. Place [Order#1, Payment#1]                            cross_order_payment_split.rdra:14:1
         + Order#1 (status=pending)
         + Payment#1 (status=pending, owner Order=Order#1)
    2. Pay_1, transaction 1 of Pay [Payment#1]               cross_order_payment_split.rdra:15:1
         Payment#1.status: pending -> captured               cross_order_payment_split.rdra:36:1
    3. Cancel [Order#1]                                      cross_order_payment_split.rdra:16:1
         Order#1.status: pending -> cancelled                cross_order_payment_split.rdra:35:1
```

A step names the use case (`Pay_1, transaction 1 of Pay` for a split one), the instances
it binds, and the use case's declaration. Under it, `+ E#k` is a creation with the values
the check reads, `- E#k` a deletion, `now: a -> b` a clock tick, and `E#k.col: a -> b` a
change with the `transitions` arm or `sets` that wrote it. `start` lists the instances
present before the first step. The trace shows only what the rule's cone reads. When the
cone does not count an entity's slots and the trace creates more of them than the bound,
a `note:` says that the whole model may not reach that state at this bound.

| Option | Description |
|---|---|
| `--strict` | Promote permission / access consistency gaps (`ActorPermissionMissing`, no-actor paths, etc.) from warnings to errors for exit. |

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
- state diagnostics (same severity as `states`), including missing creation paths,
  unreachable enum variants, a reachable state that breaks a `forbidden`, `invariant`,
  `required`, `exclusive`, multi-entity, or quantifier rule, a failed `after` assertion,
  a condition body that no use case can bind, and pattern truncation. See
  [Diagnostics](./state-derivation.md#diagnostics).

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

- `coverage-score` is a lightweight 0-100 readiness signal. Empty models score `0`.
  Finding penalties are scaled by model size (`×25/elements`); missing refinement
  stages subtract 8 each (floor 0). Use together with `stage-readiness`.
- `stage-readiness` reports which refinement stages are present: `scope`,
  `business-flow`, `data`, `interaction`, `system-boundary`, and `rules`.
- `flow-cycle` warns when `precedes` / `branches` edges form a cycle among steps.
- `enum-attribute` warns on an inline `Enum` column that no `transitions` and no `sets`
  moves. Its value is chosen by an actor, so the hint is to type the column by a
  [`variation`](./language-reference.md#variations).
- `rdra-*` rules are the RDRA definition sheet's 不整合 columns that lint enforces: an actor,
  external system, entity, condition, variation, or timer that no BUC reaches
  (`rdra-actor-no-buc`, `rdra-extsystem-no-buc`, `rdra-entity-no-buc`,
  `rdra-condition-no-buc`, `rdra-variation-no-buc`, `rdra-timer-no-buc`), and a use case
  named by `transitions` that no BUC contains (`rdra-transition-uc-no-buc`). They replace
  `orphan-node` for those kinds. The "undefined" columns are `check` errors. See
  [RDRA lint rules](./language-reference.md#rdra-lint-rules).
- `condition-no-rdra-anchor` warns on a [condition](./language-reference.md#conditions) whose
  `holds` body names neither a variation-typed column nor a lifecycle column.

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
| `--kind` | `rdra` \| `boundaryless-graph` \| `er` \| `state` \| `sequence` \| `event-flow` \| `diff` \| `business-area` \| `technical-area` | `rdra` | The diagram kind. `rdra` = RDRA layered graph mapped onto the original four-layer structure; `boundaryless-graph` = dense relationship graph without RDRA layer boundaries; `er` = entity-relationship diagram; `state` = state machine; `sequence` = write-focused sequence diagram with FK-inferred transaction boundaries; `event-flow` = event causality graph showing UC or ExtSystem->Event->UC/BUC/ExtSystem and Event->State chains, plus Timer->UC/BUC edges; `diff` = graph diff between `--diff-base` and current inputs; `business-area` = Actor -> input field -> UseCase; `technical-area` = System boxes containing only APIs and Entities. |
| `--format` | `puml` \| `svg` \| `png` \| `mermaid` | `puml` | Output format. `puml` writes PlantUML text (`.puml`); `mermaid` writes Mermaid text (`.mmd`); `svg` / `png` render via plantuml.jar. |
| `--buc <id>` | string (repeatable) | — (whole model) | Filter to one or more BUCs by id. With multiple ids, the **union** of reachable nodes across the named BUCs is shown. Applies to all diagram kinds. For `sequence` and `business-area`, only use cases directly contained in the selected BUCs are shown; event-triggered use cases in other BUCs are left to `event-flow`. |
| `--usecase <id>` | string (repeatable) | — (whole model) | Filter `sequence` or `business-area` diagrams to one or more use cases by id. Cannot be combined with `--buc`. |
| `--diff-base <path>` | path (repeatable) | — | Baseline model inputs for `--kind diff`. The normal `<INPUTS...>` are treated as the target model. |
| `--show-description` | flag | off | Render element `description` metadata as diagram annotations where supported. Mermaid emits linked annotation nodes; PlantUML emits `note right of ...` notes. Currently applies to `rdra` and `boundaryless-graph` diagram emitters. |
| `--node-kind <kind>` | string (repeatable) | — (all nodes) | Filter graph diagrams to specific node kinds. Applies to `rdra`, `boundaryless-graph`, and `diff`. Examples: `actor`, `buc`, `flow`, `step`, `usecase`, `api`, `entity`, `field`. Alias: `--kind-filter`. |
| `--edge-kind <kind>` | string (repeatable) | — (all edges) | Filter graph diagrams to specific relation kinds. Applies to `rdra`, `boundaryless-graph`, and `diff`. Examples: `contains`, `invokes`, `reads`, `writes`, `creates`, `maps-field`, `owns`, `relate`. Alias: `--edge-filter`. |
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
  system value (`actor`, `extsystem`, `requirement`, `adr`, `nfr`), system external
  environment (`quality`, `constraint`, `business`, `buc`, `flow`, `step`, `timer`,
  `medium`, `permission`), system boundary (`usecase`, `screen`, `field`, `event`), and
  system (`system`, `api`, `dto`, `entity`, `state`).
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

`scripts/sample-artifacts.tsv` lists every committed `out/` file of `samples/ec-site`,
`samples/clinic-ops`, `samples/personal-info`,
`samples/incremental-order/step-6-business-rules`, and `samples/api-contract`. Each row
holds the output path and the `rdra-ish` command that writes it. CI runs
`scripts/check-sample-artifacts.sh` after the Rust snapshot tests. The script builds the
binary once, runs every row, and fails if a tracked file changes. A generator change
therefore shows up as a diff of Mermaid diagrams, PlantUML diagrams, CSV review tables,
state-pattern output, and exports such as OpenAPI, AsyncAPI, DBML, and JSON Schema. After
an intended change, run the script and commit the regenerated files.

To compare the data layer of every sample across two builds, run
`scripts/data-diff.sh LABEL` with each build. The script writes the DBML, JSON Schema,
and entity CSV of every `samples.txt` root, with each exit code and log, under
`target/data-diff/LABEL/`. It uses `target/release/rdra-ish` (override with `RDRA_BIN`).
Compare two runs with `diff -r target/data-diff/A target/data-diff/B`.

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
rdra-ish export <INPUTS...> [--kind <KIND>] [-o <OUT>] [--instance-count <N>] [--int-range-max <N>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `openapi` \| `asyncapi` \| `dbml` \| `json-schema` \| `typescript-states` \| `mermaid-er` \| `plantuml-er` \| `tla` | `openapi` | Export kind. `openapi` emits an OpenAPI 3.0 JSON document from API `method`/`path` metadata and DTO `request` / `response` / `error_response` relations. `asyncapi` emits an AsyncAPI 3.1 event catalog from `event`, `raises`, `triggers`, `transitions`, and `outbox`. `dbml` emits a DBML schema from logical `entity` declarations, indexes, unique constraints, generated FK columns, and `relate` options. `json-schema` emits JSON Schema Draft 2020-12 definitions for DTO and Entity structures. `mermaid-er` and `plantuml-er` emit textual ER review artifacts from the same logical data model projection used by `diagram --kind er`. `tla` emits a TLA+ module plus TLC config from entity lifecycles and state constraints (see [formal-verification.md](./formal-verification.md)). |
| `-o`, `--out` | path | `out` | Output file path. If no extension is given, `openapi.json`, `asyncapi.json`, `schema.dbml`, `json-schema.json`, `er.mmd`, `er.puml`, or `RdraSpec.tla` (with sibling `.cfg`) is used. For `--kind tla`, a path ending in `.tla` uses that file stem as the TLA+ module name (sibling `.cfg`). |
| `--instance-count` | unsigned integer | `2` | TLA+ only: multi-instance `InstanceCount` (`1..=255`). Value `1` is accepted with a warning (singleton ids cannot distinguish multi-entity instances). |
| `--int-range-max` | unsigned integer | `5` | TLA+ only: upper bound for `IntRange == 0..N` (`1..=255`; `0` is rejected). |

For OpenAPI export, only APIs that declare both `method` and `path` become
`paths` operations. Path segments `{name}` become required `parameters` (`in: path`,
`schema: string`). Known `auth` values `bearer` / `api_key` map to standard security
schemes; unknown schemes fall back to an `apiKey` header `X-Auth` with
`x-rdra-ish-auth-scheme` holding the raw value. `requires_permission(Api, Permission)`
is projected as operation extension `x-rdra-ish-requires-permission` (not OpenAPI
`security`, so it is not mistaken for a runtime scheme). DTOs are emitted under
`components.schemas`.

For AsyncAPI export, each RDRA `event` becomes a channel and message. `raises`
and `outbox` produce `send` operations; `triggers` produce `receive` operations,
which list the `transitions` of the triggered use cases. An external system that
`raises` an event is listed as the channel's `x-publisher`, and one that the event
`triggers` (is delivered to) as its `x-subscriber`; both keys appear only when non-empty.
RDRA does not yet model broker protocol, server topology,
or event payload DTOs directly, so those details are left unspecified and RDRA
facts are preserved with `x-rdra-ish-*` extensions.

For DBML export, each `entity` becomes a table with its Enum types (one per inline
Enum column, plus one per `variation` shared by every column typed by it), indexes, and
foreign-key references. Every `relate` key prints as a `Ref`: a `1:1` key as `-` on a
`[unique]` column, a composite key as `Ref: Stock.(bin_site, bin_shelf) >
Bin.(site, shelf)`. DBML does not have native equivalents for every review annotation, so `@check`,
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

## `verify`

Export a TLA+ specification and run TLC formal verification. See
[formal-verification.md](./formal-verification.md).

```
rdra-ish verify <INPUTS...> [--backend tlc] [-o <OUT>] [--instance-count <N>] [--int-range-max <N>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--backend` | `tlc` | `tlc` | Verification backend. Requires `tlc` or `tlc2` on `PATH`. |
| `-o`, `--out` | path | temp dir, deleted when every check passes and kept when one fails | Directory for the specs: one `Cone_<check>.tla` / `.cfg` pair per cone, named after the first invariant or after-assert that reads it, a whole-model `RdraSpec.tla` / `.cfg` when the model declares temporal properties, and `cones.txt` listing each module and the checks it answers. A path ending in `.tla` writes into its directory and names the whole-model module after its stem. |
| `--instance-count` | unsigned integer | `2` | Multi-instance `InstanceCount` (`1..=255`; `1` warns). |
| `--int-range-max` | unsigned integer | `5` | Upper bound for `IntRange == 0..N` (`1..=255`; `0` rejected). |

A `holds` body applied to a use case is a guard in the spec, so TLC checks it. `verify`
prints no notice for conditions.

Warnings and errors of the lifecycle transition system are printed on stderr, and an
error stops `verify` before TLC runs. Each invariant and `after(..).assert` is checked on
its own cone of influence, a restriction of the system whose reachable states are exactly
the whole model's projected onto it, with the cones run in parallel. Temporal properties
need fairness on the whole system, so they are checked on the whole model, and only when
the model declares any. Stdout names the bounds, the cone count, and each run's result. Stdout or
stderr carries one `tlc_result: ok|counterexample|tool_error` line. Exits non-zero when
TLC is missing or reports a violation.

A failed run ends with `specs: <dir>`, the directory that holds the specs. Without `-o`
it adds `(kept because a check failed)`. Each cone's TLC runs with `-continue`, so every
violated check of a cone prints its own counterexample.

| Environment variable | Effect |
|---|---|
| `RDRA_TLC_HEAP` | Heap of every TLC JVM, such as `4g`. Without it a cone gets `1g` and the temporal run `2g`. |
| `JAVA_TOOL_OPTIONS` | Appended after the options `verify` sets (`-Xmx`, `-Djava.io.tmpdir`), so a `-Xmx` here wins. |

A run that runs out of memory is a `tool_error` whose message names both variables.

---

## `list`

List model elements in a human-readable form.

```
rdra-ish list <INPUTS...> [--kind <KIND>] [--format <FORMAT>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `actor` \| `entity` \| `requirement` \| `adr` \| `adr-impact` \| `nfr` \| `quality` \| `constraint` \| `buc` \| `flow` \| `step` \| `usecase` \| `field` \| `system` \| `api` \| `dto` \| `variation` \| `condition` \| `permission-callables` \| `actor-permission-audit` \| `business-inputs` | `actor` | The element kind to list. `actor` / `quality` / `buc` / `flow` / `step` / `system` / `variation` list id+label; `usecase` lists UC preconditions, guards, postconditions, alternatives, errors, and description; `requirement` lists requirement metadata; `adr` lists decision metadata plus impacted targets from `decides`; `adr-impact` lists one row per ADR-target pair; `nfr` and `constraint` list non-functional metadata; `field` lists screen-field metadata plus Entity.column mappings; `api` lists API contract metadata; `dto` lists DTO fields; `condition` lists the `holds` body, the variations and state models it combines (derived from the body, or from `combines` when there is none), and the targets of `applies_to`; `entity` lists each column with type, PK/unique/index/FK flags, FK optionality/actions, check constraints, soft-delete/history/tenant markers, and derived expressions; `permission-callables` lists each permission with callable use case/API ids and API-level `UseCase->Api` paths derived from `requires_permission`; `actor-permission-audit` lists inferred actor-side assignment gaps; `business-inputs` lists inferred actor-entered fields. |
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
| `--format` | `table` \| `csv` \| `json` \| `typescript` | `table` | Output format. `typescript` (alias `type-script`) emits entity state unions. |
| `--buc <id>` | string (repeatable) | — (whole model) | Restrict the reachable scope to the union of the named BUCs. |
| `--max-patterns` | unsigned integer | `256` | Cap on each entity's distinct patterns within one target cone (an entity view, invariant, or after-assert is explored on the variables it reads). The cone also stops after 16 joint states per allowed pattern (4096 at the default). Only entities the target reads are bounded to one instance: a use case that also creates an entity outside the cone (an audit row, say) never runs out of slots for it. When a cap is hit, the result is marked `truncated` and a fail-closed `PatternCapReached` or `StateCapReached` **error** is recorded (exit non-zero for all formats). |
| `--entity <id>` | string | — (all entities) | Restrict **stdout** to a single entity id. Filtering is applied to whichever output format is selected. Exit severity still considers the whole model; when other entities have Error-severity diagnostics, a stderr note reports the count. |

Output is written to stdout. Exit is based on structured severity (all formats), not
string-scanning `[error]`. stderr starts with a `bounds:` line: one instance per entity,
the Int range, the caps per target cone, and that entities a check does not read are unbounded. Diagnostics that name no entity are printed
on stderr too.

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
- [formal-verification.md](./formal-verification.md) — `export --kind tla` / `verify --backend tlc`.
