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
| `openapi` | OpenAPI JSON/YAML | API method/path and DTO request/response/error links |
| `asyncapi` | AsyncAPI JSON/YAML | events and event causality |
| `dbml` | DBML schema text | logical entities and relationships |
| `json-schema` | JSON Schema | DTOs or logical structures, depending on export scope |
| `tla` | TLA+ `.tla` + `.cfg` | Int/`now`, multi-entity rules, quantifiers, temporal `property` |

Default to `mermaid` unless the user asks for PlantUML or a rendered image.

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
