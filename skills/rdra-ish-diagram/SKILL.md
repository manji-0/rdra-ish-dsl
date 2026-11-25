---
name: rdra-ish-diagram
description: Choose the RDRA 3.0 diagram (or CSV or export view) that answers a review question about an RDRA-ish model, generate it as Mermaid, and read or redraw it from its header, ids, and comment lines; and lay out the diagrams and tables of a PRD or design document written from the model
license: MIT
---

## When to use

Use this skill to pick the view that answers a question about a model, generate it, and
read it. Also use it to lay out the diagrams and tables of a PRD or design document.
`rdra-ish diagram` writes Mermaid text (`.mmd`) for a reader that redraws it.

## Steps

1. Name the question, then pick its kind in the table below.
2. Run `rdra-ish check src/` and fix model errors first. Paste its `bounds:` line and
   last line exactly as printed when you report.
3. Generate the kind, scoped with `--buc <BucId>` when the question is about one BUC.
   Whole-model `rdra` and `usecase` diagrams of a large model pass Mermaid's limits
   (50,000 characters, 500 edges) and do not render; `diagram` warns on stderr.
4. Read the `%%` header first, then the body, with the kind's reference.
5. Read matrices and audits from `csv` or `list`, not from a diagram.

`diagram --all -o <dir>` writes the whole review set in one run. Add `--title` (and
`--legend` on a flowchart) when the image goes to someone without the text: a renderer
drops the `%%` header.

## Choose the kind

| Question | Command | Reference |
|---|---|---|
| Who uses the system, and which external systems exchange events with it? | `diagram --kind context` | `references/context-requirement-business.md` |
| Which goals and requirements motivate the BUCs, and what is out of scope? | `diagram --kind requirement` | `references/context-requirement-business.md` |
| How do businesses and BUCs hand off work, and who takes part in each BUC? | `diagram --kind business`; `list --kind buc-roles --format markdown` | `references/context-requirement-business.md` |
| What does each use case show, read, write, raise, and move? | `diagram --kind usecase --buc <BucId>` (or `--usecase`, `--actor`, `--entity`) | `references/usecase-and-flow.md` |
| Which screen reaches which API, and which entities does it touch? | `diagram --kind robustness --buc <BucId>` | `references/usecase-and-flow.md` |
| In what order does a use case run its guards, writes, and events? | `diagram --kind sequence --usecase <UcId>` (or `--buc <BucId>`) | `references/usecase-and-flow.md` |
| In what order are a BUC's steps carried out? | `diagram --kind flow --buc <BucId>` | `references/usecase-and-flow.md` |
| Which screen leads to which? | `diagram --kind screen --buc <BucId>` | `references/usecase-and-flow.md` |
| How do the entities relate, and which hold a state model or variation? | `diagram --kind information` | `references/information-and-data.md` |
| Which conditions select rules, and what values do the variations take? | `diagram --kind condition` | `references/information-and-data.md` |
| What columns, keys, and cardinalities does the data have? | `diagram --kind er` (or `--entity <EntityId>`) | `references/information-and-data.md` |
| What states exist, and which use case moves each? | `diagram --kind state --entity <EntityId>`; `list --kind transition --format markdown` | `references/lifecycle-and-events.md` |
| Which events and timers start other work? | `diagram --kind event` | `references/lifecycle-and-events.md` |
| A relation no other kind draws (permissions, media, DTOs, API paths) | `diagram --kind rdra` with `--node-kind` / `--edge-kind` | `references/rdra-graph.md` |
| How do I read a header, an id, or a `%%` line, or redraw a diagram? | any | `references/reading-diagrams.md` |
| Which output goes into a PRD or design document? | the base structure, unless the user has their own | `references/documents.md` |
| Which contract can other tools consume? | `export --kind openapi`, `asyncapi`, `dbml`, `sql`, `json-schema` | `references/formats-and-output.md` |

Diagrams draw declared facts. For lifecycle findings (`Witness_*`, `Deadlock_*`) read
`../rdra-ish-buc-analyze/references/state-patterns.md`; for `check` versus `verify`,
`../rdra-ish-verify/references/when-to-use.md`.

## References

- `references/reading-diagrams.md`: what each kind draws, the header, ids, shapes, edges,
  `%%` lines, and how to redraw.
- `references/context-requirement-business.md`: `context`, `requirement`, `business`, and
  the role table.
- `references/usecase-and-flow.md`: `usecase`, `robustness`, `sequence`, `flow`, `screen`.
- `references/information-and-data.md`: `information`, `condition`, `er`.
- `references/lifecycle-and-events.md`: `state`, the transition table, `event`.
- `references/rdra-graph.md`: `rdra`, its filters, and API and access questions.
- `references/formats-and-output.md`: output files, rendering, exports, and when an output
  refuses to write.
- `references/documents.md`: the base PRD and design document structure.
