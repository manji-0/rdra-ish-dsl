# Output, rendering, and exports

<!-- derived-from ../../../docs/cli-reference.md#export -->

## Diagram output

- `diagram` prints Mermaid text to stdout. `--out <path>` writes `<path>.mmd`.
- `diagram --all --title --out docs/diagrams` writes the review set: the whole-model
  `context`, `requirement`, `business`, `information`, `condition`, `er`, `screen`,
  `event`, and `state` index, and per BUC `usecase`, `robustness`, `flow`, `sequence`, and
  per entity `state`. Empty diagrams are left out.
- Every flowchart writes each description as a `%% desc <id>: <text>` line.
- To get an image, render the `.mmd` with `mmdc` or hand it to a tool that redraws it
  (`reading-diagrams.md` says what a redrawing keeps).

## Exports

| Kind | Output | Needs |
|---|---|---|
| `openapi` | OpenAPI 3.0 JSON | API `method` / `path` and DTO `request` / `response` / `error_response` |
| `asyncapi` | AsyncAPI 3.1 JSON | events, `raises` / `triggers`, the external systems that send or receive them |
| `dbml` | DBML | entities, variations, `relate` keys; each table's `Note` lists its rules by name |
| `sql` | SQL DDL (`--dialect postgres` or `sqlite`) | entities and keys; entity-local rules become named `CHECK`s; `reads` shapes add suggested indexes and `SELECT`s as comments |
| `json-schema` | JSON Schema | DTOs and entities; a mapped DTO field carries `x-rdra-ish-maps-to` |
| `mermaid-er` | Mermaid ER | the text `diagram --kind er` draws |
| `tla` | TLA+ `.tla` and `.cfg` | the lifecycle, for reading or a TLC run by hand |

```sh
rdra-ish export src/ --kind openapi --out out/openapi.json
rdra-ish export src/ --kind sql --dialect postgres --out out/schema.sql
```

Without `--out`, `export` writes `out.<kind file>` in the current directory
(`out.schema.dbml`). Use an export, not diagram labels, for downstream contract review.
For typed domain code, use `rdra-ish codegen`, which is no `export` kind.

## When an output refuses to write

- Every output except `diagram --kind state` reads declared facts, explores nothing, and
  writes even when the lifecycle has errors. It says nothing about those errors; run
  `check`.
- `diagram --kind state` refuses while `check` reports a lifecycle error other than a
  deadlock.
- `export --kind tla` writes anyway and prints a note with the error count.
- `diagram --kind state`, `export --kind tla`, `codegen`, and `next` explore at `check`'s
  default bounds, within `memory` under `[check]` in `rdra.toml` (4GiB unless set). They
  take no `--memory` flag. A cone past the limit is `MemoryCapReached`: `diagram --kind
  state` and `codegen` refuse; the `tla` export writes and counts it in its note. Raise
  `memory` under `[check]`, or narrow the model.
