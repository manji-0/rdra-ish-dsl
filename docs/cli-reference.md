# CLI Reference

The `rdra-ish` command-line tool parses `.rdra` sources, type-checks them, and produces
diagrams, CSV, listings, and state-pattern derivations.

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

Parse and type-check only; produce no output.

```
rdra-ish check <INPUTS...>
```

Prints each diagnostic to stderr. If any error (non-warning) is present, exits with
status `1`. Otherwise prints `OK: no errors` and exits `0`.

| Argument | Type | Description |
|---|---|---|
| `<INPUTS...>` | paths (required) | Files and/or directories to load. |

---

## `diagram`

Generate a diagram in PlantUML or Mermaid.

```
rdra-ish diagram <INPUTS...> [--kind <KIND>] [--format <FORMAT>] [--buc <ID>]... [-o <OUT>]
```

| Option | Type | Default | Description |
|---|---|---|---|
| `<INPUTS...>` | paths (required) | — | Files and/or directories to load. |
| `--kind` | `rdra` \| `er` \| `state` \| `sequence` \| `event-flow` | `rdra` | The diagram kind. `rdra` = full RDRA relationship graph; `er` = entity-relationship diagram; `state` = state machine; `sequence` = write-focused sequence diagram with FK-inferred transaction boundaries; `event-flow` = event causality graph showing UC→Event→UC and Event→State chains. |
| `--format` | `puml` \| `svg` \| `png` \| `mermaid` | `puml` | Output format. `puml` writes PlantUML text (`.puml`); `mermaid` writes Mermaid text (`.mmd`); `svg` / `png` render via plantuml.jar. |
| `--buc <id>` | string (repeatable) | — (whole model) | Filter to one or more BUCs by id. With multiple ids, the **union** of reachable nodes across the named BUCs is shown. Applies to all diagram kinds. |
| `-o`, `--out` | path | `out` | Output file path. The extension is added automatically based on `--format` (`.puml`, `.mmd`, `.svg`, `.png`). |

Notes:

- For `--kind sequence`, the tool additionally runs FK-based transaction-boundary
  inference and emits a `warning:` to stderr for any FK-isolated write within a use case
  that also has an FK-connected write group. API diagnostics (`ApiNeverInvoked`,
  `ApiInvokedButNoEntity`) are also run and reported as warnings.
- For `--kind event-flow`, the tool runs event-integrity diagnostics and emits `warning:`
  lines for events that are never raised, raised but consume nothing, or trigger a use
  case belonging to no BUC. Mermaid node IDs are prefixed (`ev__`, `uc__`, `st__`) to
  avoid collisions when a use case and an event share the same DSL identifier.
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
| `--kind` | `actor` \| `entity` \| `matrix` \| `api` \| `api-matrix` | `entity` | CSV kind. `actor` = actor list; `entity` = entity/column list; `matrix` = use-case × entity CRUD matrix; `api` = API list; `api-matrix` = API × entity CRUD matrix. |
| `-o`, `--out` | path | `out` | Output file path. If no extension is given, a default is appended (`actor.csv` / `entity.csv` / `matrix.csv`). |

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
| `--kind` | `actor` \| `entity` \| `buc` \| `usecase` \| `api` | `actor` | The element kind to list. `actor` / `buc` / `usecase` / `api` list id+label; `entity` lists each column with its type and PK/FK flags. |
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
