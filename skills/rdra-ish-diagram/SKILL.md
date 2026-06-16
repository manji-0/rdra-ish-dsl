---
name: rdra-ish-diagram
description: Choose the right RDRA-ish diagram, CSV review, diff, or export view for the modeling situation, then load focused references for commands and interpretation
---

## Choose RDRA-ish Views

Choose the smallest view that answers the current review question. Keep this
top-level skill as the routing layer, then load only the reference file needed for
the selected view.

### Routing Guide

| Situation | Use first | Load |
|---|---|---|
| Scope or BUC skeleton review | `rdra --buc <BucId>` | `references/layered-and-boundaryless.md` |
| Dense relationship inspection | `boundaryless-graph` | `references/layered-and-boundaryless.md` |
| Business object/data touchpoint review | `rdra --buc <BucId>` plus CRUD matrix | `references/layered-and-boundaryless.md` |
| Entity structure and relationships | `er`, optionally `--buc <BucId>` | `references/er.md` |
| Actor-entered field review | `business-area --buc <BucId>` plus `business-inputs` | `references/sequence-and-access.md` |
| System API/entity ownership review | `technical-area --buc <BucId>` plus API matrix | `references/sequence-and-access.md` |
| Screen/API/system boundary review | `sequence --buc <BucId>` plus access CSVs | `references/sequence-and-access.md` |
| API contract or payload handoff | `export --kind openapi` plus API/DTO graph filter | `references/formats-and-output.md` |
| Data contract handoff | `export --kind dbml` or `json-schema` plus ER | `references/formats-and-output.md` |
| Event contract handoff | `export --kind asyncapi` plus `event-flow` | `references/lifecycle-and-events.md` |
| Diagram size or review focus | view presets, node/edge filters, or diff diagram | `references/formats-and-output.md` |
| One concrete use case flow | `sequence --usecase <UseCaseId>` | `references/sequence-and-access.md` |
| Lifecycle and event causality | `state --buc <BucId>` plus `event-flow` | `references/lifecycle-and-events.md` |
| Business rules or state constraints | `states`, including cross-entity rule diagnostics when state axes are involved | `references/lifecycle-and-events.md` |
| Output format/rendering choice | Mermaid by default; PlantUML/SVG/PNG only when requested | `references/formats-and-output.md` |

### Default Workflow

1. Identify the current modeling stage and the question being asked.
2. Pick one primary view from the routing guide.
3. Load the matching reference file for commands, filters, and reading notes.
4. Prefer `--buc` or `--usecase` filters before generating whole-model diagrams.
5. Use CSV views for access and matrix checks instead of trying to read those details
   from diagrams alone.

### Quick Choices

- Use `rdra` when the question is "what business value/use cases are covered?"
- Use `boundaryless-graph` when layered placement hides dense relationships.
- Use `er` when the question is "what entities, columns, and relationships exist?"
- Use `business-area` when the question is "which actor inputs which fields for which use cases?"
- Use `technical-area` when the question is "which APIs and entities sit inside each system?"
- Use `sequence` when the question is "what actor/screen/API/entity path happens?"
- Use `state` when the question is "what lifecycle states and transitions exist?"
- Use `event-flow` when the question is "what events cause other work or transitions?"
- Use exports when the question is "what contract artifact can reviewers consume?"
- Use `screen-constraints`, `permission-callables`, and `actor-permission-audit` when
  the question is about authority, medium, or actor grant coverage.

### Reference Files

- `references/layered-and-boundaryless.md` — RDRA layered graph, boundaryless graph,
  and early-stage model inspection.
- `references/er.md` — ER diagram commands and relationship reading.
- `references/sequence-and-access.md` — sequence diagrams plus access/media CSV views.
- `references/lifecycle-and-events.md` — state diagrams, event-flow diagrams, and
  when to prefer `states`.
- `references/formats-and-output.md` — Mermaid/PlantUML/SVG/PNG output choices.
