# CLI Reference

The `rdra-ish` command-line tool parses `.rdra` sources, type-checks them, runs
model-consistency checks, and produces diagrams, CSV, listings, and state-pattern
derivations.

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
  anchor assertion diagnostics, and pattern truncation.

| Argument | Type | Description |
|---|---|---|
| `<INPUTS...>` | paths (required) | Files and/or directories to load. |

---

## `diagram`

Generate a diagram in PlantUML or Mermaid.

```
rdra-ish diagram <INPUTS...> [--kind <KIND>] [--format <FORMAT>] [--buc <ID>]... [--usecase <ID>]... [-o <OUT>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `rdra` \| `boundaryless-graph` \| `er` \| `state` \| `sequence` \| `event-flow` \| `business-area` \| `technical-area` | `rdra` | The diagram kind. `rdra` = RDRA layered graph mapped onto the original four-layer structure; `boundaryless-graph` = dense relationship graph without RDRA layer boundaries; `er` = entity-relationship diagram; `state` = state machine; `sequence` = write-focused sequence diagram with FK-inferred transaction boundaries; `event-flow` = event causality graph showing UC->Event->UC/BUC and Event->State chains; `business-area` = Actor -> input field -> UseCase; `technical-area` = System boxes containing only APIs and Entities. |
| `--format` | `puml` \| `svg` \| `png` \| `mermaid` | `puml` | Output format. `puml` writes PlantUML text (`.puml`); `mermaid` writes Mermaid text (`.mmd`); `svg` / `png` render via plantuml.jar. |
| `--buc <id>` | string (repeatable) | — (whole model) | Filter to one or more BUCs by id. With multiple ids, the **union** of reachable nodes across the named BUCs is shown. Applies to all diagram kinds. For `sequence` and `business-area`, only use cases directly contained in the selected BUCs are shown; event-triggered use cases in other BUCs are left to `event-flow`. |
| `--usecase <id>` | string (repeatable) | — (whole model) | Filter `sequence` or `business-area` diagrams to one or more use cases by id. Cannot be combined with `--buc`. |
| `-o`, `--out` | path | `out` | Output file path. The extension is added automatically based on `--format` (`.puml`, `.mmd`, `.svg`, `.png`). |

Notes:

- Diagram object labels include kind prefixes (`👤 actor`, `📦 BUC`, `✅ usecase`,
  `🖥️ screen`, `🔌 api`, `🗄️ entity`, `⚡ event`, `🔄 state`) while keeping DSL ids
  unchanged for edges and references.
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
- For `--kind sequence`, participant lifelines are grouped into RDRA-style layer boxes:
  system value (`actor`), system boundary (`screen`, `api`), and system (`system`,
  `entity`). The use case itself remains the sequence section title.
- For `--kind rdra`, model objects are placed into four RDRA-style layers:
  system value (`actor`, `requirement`), system external environment (`business`, `buc`,
  `usagescene`, `extsystem`, `condition`, `variation`), system boundary (`usecase`,
  `screen`, `event`), and system (`system`, `api`, `entity`, `state`).
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

## `csv`

Generate CSV output.

```
rdra-ish csv <INPUTS...> [--kind <KIND>] [-o <OUT>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `actor` \| `entity` \| `matrix` \| `api` \| `api-matrix` \| `screen-constraints` \| `permission-callables` \| `actor-permission-audit` \| `business-inputs` | `entity` | CSV kind. `actor` = actor list; `entity` = entity/column list; `matrix` = use-case × entity CRUD matrix; `api` = API list; `api-matrix` = API × entity CRUD matrix; `screen-constraints` = screen × UC/API permission/medium paths; `permission-callables` = permission × callable UC/API list derived from `requires_permission`, including `UseCase->Api` paths for API-level requirements; `actor-permission-audit` = actor × permission assignment audit inferred from UC/API requirements; `business-inputs` = Actor x inferred input field x UseCase rows derived from C/U/W paths. |
| `-o`, `--out` | path | `out` | Output file path. If no extension is given, a default is appended (`actor.csv` / `entity.csv` / `matrix.csv` / etc.). |

The command writes the CSV to the output path and prints `wrote <path>`.

---

## `list`

List model elements in a human-readable form.

```
rdra-ish list <INPUTS...> [--kind <KIND>] [--format <FORMAT>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `actor` \| `entity` \| `buc` \| `usecase` \| `system` \| `api` \| `permission-callables` \| `actor-permission-audit` \| `business-inputs` | `actor` | The element kind to list. `actor` / `buc` / `usecase` / `system` / `api` list id+label; `entity` lists each column with its type and PK/FK flags; `permission-callables` lists each permission with callable use case/API ids and API-level `UseCase->Api` paths derived from `requires_permission`; `actor-permission-audit` lists inferred actor-side assignment gaps; `business-inputs` lists inferred actor-entered fields. |
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
