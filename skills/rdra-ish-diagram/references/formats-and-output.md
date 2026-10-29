# Formats and Output

Use this reference when choosing Mermaid, PlantUML text, rendered image output,
filtered/diff views, or machine-readable exports.

## Format Guide

| Format | Output | Requirement |
|---|---|---|
| `mermaid` | `.mmd` text | None |
| `puml` | `.puml` text | None |
| `svg` | `.svg` file | `plantuml.jar` plus Java in `PATH`; set `PLANTUML_JAR=` |
| `png` | `.png` file | Same as `svg` |
| `openapi` | OpenAPI 3.0 JSON | API method/path and DTO request/response/error links |
| `asyncapi` | AsyncAPI 3.1 JSON | events, `raises`/`triggers`, and the external systems that publish or receive them |
| `dbml` | DBML schema text | logical entities, variations, and `relate` keys |
| `json-schema` | JSON Schema | DTOs and entities under `$defs` |
| `tla` | TLA+ `.tla` + `.cfg` | the lifecycle transition system, for reading or a TLC run by hand |

Default to `mermaid` unless the user asks for PlantUML or a rendered image.

Outputs that print the lifecycle (`diagram` of every kind but `er`, `csv` of every kind
but `entity`, `export --kind typescript-states`) refuse to write while `check` reports
a lifecycle error. The data outputs (`diagram --kind er`, `csv --kind entity`, and the
`dbml`, `json-schema`, `openapi`, `asyncapi`, and ER exports) still write and print one
note.

## Commands

```sh
# Mermaid text
rdra-ish diagram src/ --kind rdra --format mermaid --out docs/rdra

# PlantUML text
rdra-ish diagram src/ --kind rdra --format puml --out docs/rdra

# Rendered SVG
PLANTUML_JAR=/path/to/plantuml.jar rdra-ish diagram src/ --kind rdra --format svg --out docs/rdra

# Rendered PNG
PLANTUML_JAR=/path/to/plantuml.jar rdra-ish diagram src/ --kind rdra --format png --out docs/rdra

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

- If no extension is supplied, `rdra-ish` appends the default extension for the chosen
  format.
- SVG and PNG rendering fail when `PLANTUML_JAR` is unset or points to a missing jar.
- Prefer filtered diagrams (`--buc` or `--usecase`) before producing rendered assets.
- Use `--show-description` when descriptions should appear as notes/tooltips.
- Use `--view-preset`, `--node-kind`, and `--edge-kind` before accepting a too-large
  diagram.
- Use export output for downstream contract review rather than hand-copying diagram
  labels.
