# Formats and Output

Use this reference when choosing Mermaid, PlantUML text, or rendered image output.

## Format Guide

| Format | Output | Requirement |
|---|---|---|
| `mermaid` | `.mmd` text | None |
| `puml` | `.puml` text | None |
| `svg` | `.svg` file | `plantuml.jar` plus Java in `PATH`; set `PLANTUML_JAR=` |
| `png` | `.png` file | Same as `svg` |

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
```

## Notes

- If no extension is supplied, `rdra-ish` appends the default extension for the chosen
  format.
- SVG and PNG rendering fail when `PLANTUML_JAR` is unset or points to a missing jar.
- Prefer filtered diagrams (`--buc` or `--usecase`) before producing rendered assets.
