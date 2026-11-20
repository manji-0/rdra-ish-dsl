# Writing a PRD or a Design Document from the Model

Use this reference when the model is to become a document a person reads: a PRD for the
product side, or a design document for the engineers. It says which diagrams and tables
go into which section, and in what order.

**This is the base structure, not a mandate.** When the user has their own instructions
for writing documents, such as a template, a style guide, a document skill, or a
CLAUDE.md section, follow those. Use this reference only to fill what they leave open:
which `rdra-ish` output answers a section they ask for, and the embedding rules below.

## Who Reads What

| Document | Reader | Question | Diagrams it takes |
|---|---|---|---|
| PRD | Product manager, business owner | Why build it, for whom, and what counts as done? | Business-facing kinds only: `context`, `requirement`, `business`, `flow`, and `usecase --actor` when the roles are the question |
| Design document | Engineer | What does each use case touch, in what order, under which rules? | `context`, `event`, `robustness`, `screen`, `sequence`, `state`, `er` |

Keep the technical kinds (`robustness`, `sequence`, `er`, `state`) out of a PRD. Its
reader decides scope and acceptance, not structure. The UC複合図 (`usecase`) and the
whole `rdra` diagram are for an RDRA review; neither document starts from them.

## PRD

| Section | Content | Command |
|---|---|---|
| 1. Background and goals | Each goal with its metric, baseline, and target | `list --kind goal --format markdown` |
| 2. Users | The system context; who takes part in each BUC | `diagram --kind context`; `list --kind buc-roles --format markdown` |
| 2a. (Many roles) What each role does | One UC複合図 per actor | `diagram --kind usecase --actor <ActorId>` |
| 3. Requirements | The goal, requirement, and BUC tree; the requirement list | `diagram --kind requirement`; `list --kind requirement --format markdown` |
| 4. Non-goals | Requirements with `status "out-of-scope"` | `list --kind requirement` (the out-of-scope rows) |
| 5. User journey | The business flow across BUCs, then each BUC's steps | `diagram --kind business`; `diagram --kind flow --buc <BucId>` per BUC |
| 6. Acceptance criteria | The business rules, each a test condition | `list --kind rule --format markdown` |
| 7. Non-functional requirements | NFRs and constraints | `list --kind nfr --format markdown` |

## Design Document

An engineer reads in three steps: the structure, then the processing, then the
specification each write must keep.

| Section | Content | Command |
|---|---|---|
| 1. Overview | The system context; the events between systems | `diagram --kind context`; `diagram --kind event` |
| 2. Per BUC | Structure: screens, controls, and the entities each control touches | `diagram --kind robustness --buc <BucId>` |
| | Screen transitions, when the BUC declares any | `diagram --kind screen --buc <BucId>` |
| | The BUC's processing, use case after use case | `diagram --kind sequence --buc <BucId>` |
| | A table of its use cases: screen, control, guards | `list --kind usecase --format json` |
| | Each use case's guards and writes in order, folded | `diagram --kind sequence --usecase <UcId>` |
| 3. State models | One state diagram per entity; the reachable states of the main ones | `diagram --kind state --entity <EntityId>`; `states --entity <EntityId> --format table` |
| 4. Data model | The ER diagram; the column list | `diagram --kind er` (or `er --entity <EntityId>` per aggregate, see below); `list --kind entity --format markdown` |
| 5. Rules | The rules each write must keep | `list --kind rule --format markdown` |
| 6. CRUD and permissions | Which API writes which entity; who holds which permission | `csv --kind api-matrix --format markdown`; `list --kind actor-permission-audit --format markdown` |
| 7. Non-functional requirements | NFRs and constraints | `list --kind nfr --format markdown` |

## Embedding Rules

- **Generate; do not hand-edit.** Embed the `.mmd` text as a fenced `mermaid` block and
  keep its `%%` header: the header names the kind, the scope, and the model, so a reader
  can regenerate it. Fix the model, not the diagram, and regenerate after each model change.
  `diagram --all --out <dir>` writes the per-BUC and per-entity set in one run.
- **Stay under Mermaid's limits.** A block over 50,000 characters or 500 edges does not
  render (`diagram` warns on stderr). Scope every embedded diagram: `--buc` for the BUC
  sections, and for the data model `er --entity <EntityId>` per aggregate when the whole
  `er` warns.
- **Tables are tables.** Put goals, requirements, roles, rules, CRUD, and permissions in
  `list` or `csv` output with `--format markdown`, not in a diagram.
- **Fold the long tail.** Per-use-case sequence diagrams go under `<details>`; the BUC
  sequence above them is the one a reader reads first.
- **Images leave the text behind.** When a diagram goes out as an image (a slide, a wiki
  without Mermaid), render it with `--title`, and `--legend` for a flowchart. A renderer
  drops the `%%` header, so the image would otherwise not say what it shows.
- **Run `check` first.** A diagram of a model with errors may draw states that cannot
  happen; `state` refuses to.

The generated samples follow this structure: `samples/clinic-ops/prd.md` and
`samples/clinic-ops/design-doc.md` in the repository, written by
`scripts/gen-clinic-ops-docs.py`.
