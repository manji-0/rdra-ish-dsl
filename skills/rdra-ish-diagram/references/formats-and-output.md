# Formats and Output

Use this reference when choosing diagram output, filtered views, or
machine-readable exports.

## Format Guide

| Format | Output | Requirement |
|---|---|---|
| `mermaid` | `.mmd` text | None |
| `openapi` | OpenAPI 3.0 JSON | API method/path and DTO request/response/error links |
| `asyncapi` | AsyncAPI 3.1 JSON | events, `raises`/`triggers`, and the external systems that publish or receive them |
| `dbml` | DBML schema text | logical entities, variations, and `relate` keys |
| `json-schema` | JSON Schema | DTOs and entities under `$defs` |
| `tla` | TLA+ `.tla` + `.cfg` | the lifecycle transition system, for reading or a TLC run by hand |

`diagram` writes Mermaid text only, and `mermaid` is the default `--format`. The text
is meant to be redrawn: render the `.mmd` with Mermaid CLI (`mmdc`), or hand it to a tool
that draws SVG from it. `references/reading-diagrams.md` says what a redrawing must keep.

Only the outputs that print reachable states (`diagram --kind state` and
`export --kind typescript-states`) refuse to write while `check` reports a lifecycle
error. The outputs of declared facts and the data layer (`diagram` of every other kind,
`csv` and `list` of every kind, and the `dbml`, `json-schema`, `openapi`, `asyncapi`, ER,
and `tla` exports) still write and print one note. TLC on the `tla` export finds no
deadlock and no state without a witness, so run `verify` for those.

## Commands

```sh
# Mermaid text
rdra-ish diagram src/ --kind rdra --format mermaid --out docs/rdra

# Focused graph views
rdra-ish diagram src/ --kind rdra --format mermaid --node-kind api --node-kind dto --edge-kind request

# Contract exports
rdra-ish export src/ --kind openapi --out out/openapi.json
rdra-ish export src/ --kind asyncapi --out out/asyncapi.json
rdra-ish export src/ --kind dbml --out out/schema.dbml
rdra-ish export src/ --kind json-schema --out out/json-schema.json
rdra-ish export src/ --kind tla -o out/

# Golden/sample artifact regeneration
bash scripts/check-sample-artifacts.sh
```

## Notes

- `diagram` prints to stdout without `--out`; with it, `.mmd` is appended to the path.
- Prefer scoped diagrams (`--buc`) before rendering images.
- Every flowchart kind writes each description as a `%% desc <id>: <text>` comment
  line; there is no flag for it.
- Use `--buc`, and on `rdra` `--node-kind` and `--edge-kind`, before accepting a
  too-large diagram.
- Use export output for downstream contract review rather than hand-copying diagram
  labels.
