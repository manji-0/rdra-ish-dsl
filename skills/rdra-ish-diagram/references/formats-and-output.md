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

`diagram` writes Mermaid text only, and `mermaid` is the default `--format`. The
`puml`, `svg`, and `png` formats were removed. For an image, render the `.mmd` with
Mermaid CLI (`mmdc`) or hand the text to a tool that draws it.

Outputs that print the lifecycle (`diagram` of every kind but `er`, `csv` of every kind
but `entity`, `export --kind typescript-states`) refuse to write while `check` reports
a lifecycle error. The data outputs (`diagram --kind er`, `csv --kind entity`, and the
`dbml`, `json-schema`, `openapi`, `asyncapi`, and ER exports) still write and print one
note.

## Commands

```sh
# Mermaid text
rdra-ish diagram src/ --kind rdra --format mermaid --out docs/rdra

# Focused graph views
rdra-ish diagram src/ --kind rdra --format mermaid --view-preset api
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

- `diagram` appends `.mmd` to `--out`.
- Prefer filtered diagrams (`--buc`) before rendering images.
- Use `--show-description` when descriptions should appear as notes/tooltips.
- Use `--view-preset`, `--node-kind`, and `--edge-kind` before accepting a too-large
  diagram.
- Use export output for downstream contract review rather than hand-copying diagram
  labels.
