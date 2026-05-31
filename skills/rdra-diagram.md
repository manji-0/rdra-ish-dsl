---
name: rdra-diagram
description: Generate RDRA DSL diagrams (RDRA layered graph, boundaryless graph, ER, state, sequence) in Mermaid or PlantUML format, choosing diagrams by refinement stage
---

## Generate diagrams

Choose the diagram kind and format, apply BUC filters if needed, and generate output.

<!-- derived-from ../docs/cli-reference.md#diagram -->
<!-- derived-from ../docs/cli-reference.md#csv -->
<!-- derived-from ../docs/language-reference.md#access-constraints -->

For incremental modeling, choose the diagram that matches the current abstraction
stage instead of generating every view. The early views should answer business
questions first; later views expose technical boundaries, persistence, lifecycle, and
rules.

### Refinement-stage diagram guide

| Stage | Concern | Use first | Purpose |
|-------|---------|-----------|---------|
| Scope / BUC skeleton | Biz intent/value | `rdra --buc <BucId>` | actor, BUC, and use-case coverage |
| Data touchpoints | Biz object touchpoints | `rdra --buc <BucId>` plus `er --buc <BucId>` | CRUD-connected entities |
| Interaction boundary | Tech interaction boundary | `sequence --buc <BucId>` plus `csv --kind screen-constraints` | actor/screen/API/entity path and access/media constraints |
| Entity structure | Tech data design | `er` | columns, PK/FK, and cardinality |
| Lifecycle | Tech lifecycle design | `state --buc <BucId>` and `event-flow` | states, events, transitions, triggers |
| Business rules | Tech-enforced rules | no new diagram first; run `states` | validate constraints before visual polish |

### Diagram kind guide

| Need | Kind | What it shows |
|------|------|---------------|
| RDRA original-style layered graph | `rdra` | Maps model nodes onto system value, external environment, boundary, and system layers |
| Dense relationship inspection without layers | `boundaryless-graph` | Flat graph of business/data relationships; API nodes omitted by design |
| Database table structure and relationships | `er` | Entities, columns, FK relationships |
| Entity lifecycle | `state` | State nodes and transition events |
| Write operations and transaction boundaries | `sequence` | Sequence of writes per use case; shows `Actor→Screen→API→Entity` lanes when `invokes` is used |
| Event causality chains | `event-flow` | UC→Event→UC and Event→State chains |
| Screen access/media paths | CSV `screen-constraints` | Derived Screen × UC/API rows with permission and medium requirements |

### Format guide

| Format | Output | Requirement |
|--------|--------|-------------|
| `mermaid` | `.mmd` text | None — paste into any Mermaid renderer |
| `puml` | `.puml` text | None — paste into PlantUML renderer |
| `svg` | `.svg` file | `plantuml.jar` + Java in `PATH`; set `PLANTUML_JAR=` |
| `png` | `.png` file | Same as `svg` |

Default to `mermaid` unless the user asks for a rendered image.

### Commands

```sh
# RDRA layered graph — whole model
rdra-ish diagram src/ --kind rdra --format mermaid

# Boundaryless relationship graph — whole model
rdra-ish diagram src/ --kind boundaryless-graph --format mermaid

# RDRA layered graph — scoped to one BUC
rdra-ish diagram src/ --kind rdra --buc <BucId> --format mermaid

# Boundaryless relationship graph — scoped to one BUC
rdra-ish diagram src/ --kind boundaryless-graph --buc <BucId> --format mermaid

# RDRA layered graph — union of multiple BUCs
rdra-ish diagram src/ --kind rdra --buc <BucA> --buc <BucB> --format mermaid

# ER diagram — whole model
rdra-ish diagram src/ --kind er --format mermaid

# ER diagram — entities reachable from a BUC
rdra-ish diagram src/ --kind er --buc <BucId> --format mermaid

# State diagram — whole model
rdra-ish diagram src/ --kind state --format mermaid

# State diagram — scoped to a BUC
rdra-ish diagram src/ --kind state --buc <BucId> --format mermaid

# Sequence diagram — write operations (whole model)
rdra-ish diagram src/ --kind sequence --format mermaid

# Sequence diagram — scoped to a BUC
rdra-ish diagram src/ --kind sequence --buc <BucId> --format mermaid

# Screen access/media constraint paths
rdra-ish csv src/ --kind screen-constraints

# Write to a specific file (extension added automatically)
rdra-ish diagram src/ --kind er --format mermaid --out docs/er
# → writes docs/er.mmd

# Render to SVG (requires plantuml.jar)
PLANTUML_JAR=/path/to/plantuml.jar rdra-ish diagram src/ --kind rdra --format svg --out docs/rdra
```

### Reading the output

Object labels include kind prefixes such as `👤` actor, `📦` BUC, `✅` usecase,
`🖥️` screen, `🔌` API, `🗄️` entity, `⚡` event, and `🔄` state. IDs stay unchanged.

**RDRA layered graph (`--kind rdra`)**
- Four vertical layers = system value, system external environment, system boundary, system
- `api` nodes are included in the system layer; screens and use cases stay in the system boundary layer
- Dashed arrows = interaction / CRUD / event / lifecycle relationships

**Boundaryless relationship graph (`--kind boundaryless-graph`)**
- Rounded box = Actor
- Rectangle = BUC or UseCase
- Database cylinder = Entity
- Double-border = Screen
- Diamond = Event
- Solid arrow = `performs` / `contains`
- Dashed arrow = CRUD / `displays` / `raises`

**ER diagram (`--kind er`)**
- Each entity box lists columns with PK/FK markers
- Crow's foot notation for cardinality from `relate` predicates
- FK columns are auto-generated by `relate` — do not add them manually

**State diagram (`--kind state`)**
- Nodes = `state` declarations
- Arrows labelled with event display names
- `[*]` initial state derived from `creates` predicates

**Sequence diagram (`--kind sequence`)**
- Each use case block shows write operations in order
- Participant lifelines are grouped by layer: system value, system boundary, system
- **With `invokes`**: renders `Actor → Screen → API → Entity` lanes; the API is the source of writes
- **Without `invokes`** (legacy): renders the `System` participant lane unchanged
- Shaded `rect` = transaction group. The note distinguishes `inferred from FK` from `API atomic boundary`
- `Note right of ...: FK非連結` = entities written outside a common FK chain — consider modeling them through an API boundary

**Boundaryless relationship graph (`--kind boundaryless-graph`)**
- Use this when you want a flat graph for dense link inspection rather than the RDRA-style layer structure.

### Tips

- Use `--buc` to reduce diagram size when the whole-model graph is too large to read
- For ER diagrams, scope to a BUC to show only the entities that BUC touches
- The sequence diagram only shows write operations (`creates` / `updates` / `deletes`) and `displays` — `reads` are omitted
- Sequence diagram transaction warnings are also printed to stderr as `warning:` lines
- API diagnostics (`ApiNeverInvoked`, `ApiInvokedButNoEntity`) are printed to stderr when running `--kind sequence`
- System diagnostics (`CrossSystemEntityRelation`, `CoordinationMissingApi`,
  `CoordinationNotCrossSystem`, `ApiInMultipleSystems`, `EntityInMultipleSystems`) are
  printed by `check` and sequence output. Use them to verify `coordinates` and API calls.
- Access and medium requirements are not expanded in diagrams yet. Use
  `rdra-ish csv src/ --kind screen-constraints` to inspect the derived constraints that
  pass through each screen via `displays` and `invokes`.
