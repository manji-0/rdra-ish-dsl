---
name: rdra-ish-diagram
description: Choose the RDRA 3.0 diagram (or CSV or export view) that answers a review question about an RDRA-ish model, generate it as Mermaid, and read or redraw it from its header, ids, and comment lines
license: MIT
---

## Choose an RDRA-ish View

`rdra-ish diagram` draws the RDRA 3.0 diagrams of a model as Mermaid text (`.mmd`). The
text is written for a reader that redraws it: you, a person with Mermaid CLI, or another
tool that draws SVG. It carries the RDRA meaning in text, so a redrawing may change every
shape, colour, and direction, and must keep the nodes, their kinds and layers, and the
edges with their labels.

Pick the one kind that answers the question, then load only its reference.

### Routing Guide

| The question | Kind | Load |
|---|---|---|
| Who uses the system, and which external systems does it exchange events with? | `context` | `references/context-requirement-business.md` |
| Which requirements, NFRs, and constraints exist, and what do they bind? | `requirement` | `references/context-requirement-business.md` |
| Which BUCs make up each business, and which actors and external systems take part? | `business`, or `business --buc <BucId>` for one BUC | `references/context-requirement-business.md` |
| What does each use case show, read, write, raise, and move, and who performs it? | `usecase --buc <BucId>` or `usecase --usecase <UcId>` | `references/usecase-and-flow.md` |
| Which screen reaches which API or use case, and which entities does each touch (the main structural view)? | `robustness --buc <BucId>` or `robustness --usecase <UcId>` | `references/usecase-and-flow.md` |
| In what order does one use case run: guards, writes, events? | `sequence --usecase <UcId>` (exactly one) | `references/usecase-and-flow.md` |
| In what order are a BUC's steps carried out, and which use cases carry each? | `flow --buc <BucId>` | `references/usecase-and-flow.md` |
| How do the entities relate, and which hold a state model or a variation? | `information` | `references/information-and-data.md` |
| Which conditions select business rules, and what values do the variations take? | `condition` | `references/information-and-data.md` |
| What columns, keys, and cardinalities does the data have? | `er` | `references/information-and-data.md` |
| What states exist, and which use case moves each? | `state` | `references/lifecycle-and-events.md` |
| Which events, timers, and external systems start other work? | `event` | `references/lifecycle-and-events.md` |
| Everything at once, or a relation no other kind draws (permissions, media, DTOs, API paths) | `rdra`, narrowed with `--node-kind` / `--edge-kind` | `references/rdra-graph.md` |
| How do I read a header, an id, or a `%%` line, or redraw a diagram? | any | `references/reading-diagrams.md` |
| Which actor inputs which fields, or which APIs sit in each system? | `csv --kind business-inputs` or `csv --kind api-matrix` | `references/access-and-apis.md` |
| Where are the transaction and API-boundary problems? | `check` | `references/access-and-apis.md` |
| Are rules broken, or states unreached or stuck? | `check`, then `states` | `references/lifecycle-and-events.md` |
| Do temporal or two-instance rules hold? | `verify` (skill `rdra-ish-verify`) | `references/lifecycle-and-events.md` |
| What contract can reviewers or other tools consume? | `export --kind openapi`, `asyncapi`, `dbml`, or `json-schema` | `references/formats-and-output.md` |

Every kind takes `--buc <BucId>` (repeatable) to draw the part of the model those BUCs
reach. Prefer it before a whole-model diagram.

### What Each Kind Carries

| Kind | RDRA diagram | Carries | Leaves out |
|---|---|---|---|
| `context` | システムコンテキスト | The system, its actors, and its external systems; every edge derived | BUCs, use cases, events (named in edge labels and `%% derived`) |
| `requirement` | 要求モデル | `motivates`, NFR `applies_to`, `constrains`, `qualifies` | ADRs; a requirement-to-actor link (not in the DSL) |
| `business` | ビジネスコンテキスト / BUC | Businesses holding BUCs, and each BUC's actors and external systems | Use cases, steps, events (named in `%% derived`) |
| `usecase` | UC複合図 | Use cases with actors, screens, entities, events, external systems, timers, conditions, and transitions | APIs (CRUD folded into the use case), DTOs, permissions, media, fields, BUCs |
| `robustness` | ロバストネス図 | Actors, boundaries (screens, external systems, timers), controls (APIs, or the use case), entities; edges only actor to screen to control to entity | Events and conditions (named in edge labels), states, DTOs, permissions, media, fields, BUCs |
| `sequence` | シーケンス図 | One use case: starter, screen, controls, entities, external systems; guards as `alt`; writes with `sets` and the lifecycle move; raised events | Other use cases (a triggered one is one message), BUCs, DTOs, permissions |
| `flow` | 業務フロー | Flows, steps, `precedes` / `branches` / `excepts` / `repeats`, `covers`, actor lanes | Screens and entities |
| `information` | 情報モデル | Entities, `relate`, state models, variations | Columns (`er`) |
| `condition` | バリエーション・条件 | Conditions with what they combine and apply to; variations with their values | Entities (a state model stands for its column) |
| `state` | 状態モデル | One composite state per state model, start and end states, the use case of each transition | `Enum` columns without `transitions` (named in `omitted:`) |
| `event` | システム間連携 | `raises` and `triggers` among use cases, BUCs, external systems, events, and timers | States and transitions |
| `er` | (logical data model) | Entities, columns, keys, cardinality | Variation values, state models |
| `rdra` | RDRAGraph | Every element in its layer and every relation | ADRs |

`state` is the only kind that refuses to write while `check` reports a lifecycle error.
The others draw declared facts and print one note.

### Reading Any Diagram

- The `%%` header comes first: `%% rdra-ish diagram=<kind> scope=<scope> model=<paths>`,
  then `%% layers:` (layered kinds), `%% kinds:` (node kinds the body draws), and
  `%% omitted:` (linked kinds the diagram leaves out). Check `omitted:` before you
  conclude that something is missing from the model.
- A node id is `<kind>__<DSL id>`, the same in every kind, and each flowchart node
  carries `:::<kind>`. Match nodes across diagrams by id, never by label.
- An edge label is the DSL predicate. An edge followed by `%% derived <from> -> <to> ...`
  was derived by rdra-ish; any other edge is declared and is also in `rdra`.
- `%% desc`, `%% holds`, `%% lane`, and `%% synthesized` lines carry what shapes cannot.
  `references/reading-diagrams.md` lists them all.

### Default Workflow

1. Name the review question, then pick its kind from the routing guide.
2. Run `rdra-ish check src/` first; fix model errors before reading diagrams.
3. Generate the diagram with `--buc <BucId>` when the question is about one BUC.
4. Read the header, then the body; load the kind's reference for its edges.
5. Use CSV views for matrices and audits instead of reading them off a diagram.

### Reference Files

- `references/reading-diagrams.md` — the encoding contract: header, ids, edges, `%%`
  lines, and how to redraw a diagram.
- `references/context-requirement-business.md` — `context`, `requirement`, `business`.
- `references/usecase-and-flow.md` — `usecase` (UC複合図), `robustness`, `sequence`, and `flow`.
- `references/information-and-data.md` — `information`, `condition`, `er`, and the data
  exports.
- `references/lifecycle-and-events.md` — `state`, `event`, `states`, `check`, and TLA+.
- `references/rdra-graph.md` — `rdra`, the whole graph, and its filters.
- `references/access-and-apis.md` — API boundaries and the access CSV views.
- `references/formats-and-output.md` — Mermaid output, rendering, and contract exports.
