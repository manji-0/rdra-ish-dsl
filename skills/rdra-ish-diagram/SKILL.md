---
name: rdra-ish-diagram
description: Choose the right RDRA-ish diagram, CSV review, or export view for the modeling situation, then load focused references for commands and interpretation
license: MIT
---

## Choose RDRA-ish Views

Choose the smallest view that answers the current review question. Keep this
top-level skill as the routing layer, then load only the reference file needed for
the selected view.

### Routing Guide

| Situation | Use first | Load |
|---|---|---|
| Scope or BUC skeleton review | `rdra --buc <BucId>` | `references/rdra-graph.md` |
| Businesses, BUCs, and who takes part | `business`, optionally `--buc <BucId>` | `references/rdra-graph.md` |
| Dense relationship inspection | `rdra` with `--node-kind` / `--edge-kind` | `references/rdra-graph.md` |
| Business object/data touchpoint review | `rdra --buc <BucId>` plus CRUD matrix | `references/rdra-graph.md` |
| Use case inputs and outputs (UC複合図) | `usecase --buc <BucId>` or `usecase --usecase <UcId>` | `references/rdra-graph.md` |
| Entity structure and relationships | `er`, optionally `--buc <BucId>` | `references/er.md` |
| Actor-entered field review | `csv --kind business-inputs` | `references/access-and-apis.md` |
| System API/entity ownership review | `csv --kind api-matrix` | `references/access-and-apis.md` |
| Screen/API/system boundary review | `rdra --buc <BucId>`, `check`, plus access CSVs | `references/access-and-apis.md` |
| API contract or payload handoff | `export --kind openapi` plus API/DTO graph filter | `references/formats-and-output.md` |
| Data contract handoff | `export --kind dbml` or `json-schema` plus ER | `references/formats-and-output.md` |
| Event contract handoff | `export --kind asyncapi` plus `event` | `references/lifecycle-and-events.md` |
| Diagram size or review focus | `--buc` scope, or `--node-kind` / `--edge-kind` on `rdra` | `references/formats-and-output.md` |
| Lifecycle and event causality | `state --buc <BucId>` plus `event` | `references/lifecycle-and-events.md` |
| Business rules or state constraints | `check` (rule counterexamples, unreached and stuck states), then `states` | `references/lifecycle-and-events.md` |
| Temporal `property` and rules that need two instances | Prefer skill `rdra-ish-verify` (`verify`) | `references/lifecycle-and-events.md` |
| Output format/rendering choice | Mermaid text only; render images from the `.mmd` outside rdra-ish | `references/formats-and-output.md` |

### Default Workflow

1. Identify the current modeling stage and the question being asked.
2. Pick one primary view from the routing guide.
3. Load the matching reference file for commands, filters, and reading notes.
4. Prefer `--buc` filters before generating whole-model diagrams.
5. Use CSV views for access and matrix checks instead of trying to read those details
   from diagrams alone.

### Quick Choices

- Use `rdra` when the question is "what business value/use cases are covered?"
- Use `business` when the question is "which BUCs make up each business, and which
  actors and external systems take part in each?"
- Use `usecase` when the question is "what does each use case show, read, write, raise,
  and move, and who performs it?"
- Use `er` when the question is "what entities, columns, and relationships exist?"
- Use `csv --kind business-inputs` when the question is "which actor inputs which fields
  for which use cases?"
- Use `csv --kind api-matrix` when the question is "which APIs and entities sit inside
  each system?"
- Use `rdra --buc <BucId>` and `check` when the question is "what actor/screen/API/entity
  path happens, and where are the transaction boundaries?"
- Use `state` when the question is "what lifecycle states exist, and which use case
  moves each one?"
- Use `event` when the question is "what events, timers, and external systems
  start other work?"
- Use `states` when the question is "which abstract patterns are reachable?"
- Use `verify` (skill `rdra-ish-verify`) when the question is "do temporal or
  two-instance rules hold under TLC?"
- Use exports when the question is "what contract artifact can reviewers consume?"
- Use `screen-constraints`, `permission-callables`, and `actor-permission-audit` when
  the question is about authority, medium, or actor grant coverage.

### Reference Files

- `references/rdra-graph.md` — RDRA layered graph, the business and BUC diagram, the
  UC複合図, and early-stage model inspection.
- `references/er.md` — ER diagram commands and relationship reading.
- `references/access-and-apis.md` — API boundaries plus access/media CSV views.
- `references/lifecycle-and-events.md` — state diagrams, event diagrams,
  `states`, and TLA+ export.
- `references/formats-and-output.md` — Mermaid output and contract exports.
