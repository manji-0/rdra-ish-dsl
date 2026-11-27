# Writing a PRD or a design document from the model

This is the base structure, not a mandate. When the user has their own document
instructions (a template, a style guide, a document skill, a CLAUDE.md section), follow
them, and use this file only for what they leave open.

## Who reads what

| Document | Reader | Question | Diagrams |
|---|---|---|---|
| PRD | Product manager, business owner | Why build it, for whom, and what counts as done? | `context`, `requirement`, `business`, `flow`, and `usecase --actor` when roles matter |
| Design document | Engineer | What does each use case touch, in what order, under which rules? | `context`, `event`, `robustness`, `screen`, `sequence`, `state`, `er` |

Keep `robustness`, `sequence`, `er`, and `state` out of a PRD. Neither document starts
from `usecase` or `rdra`; those are for an RDRA review.

## PRD

| Section | Content | Command |
|---|---|---|
| 1. Background and goals | Goals with metric, baseline, target | `list --kind goal --format markdown` |
| 2. Users | The system context; who takes part in each BUC | `diagram --kind context`; `list --kind buc-roles --format markdown` |
| 2a. What each role does (many roles) | One UC複合図 per actor | `diagram --kind usecase --actor <ActorId>` |
| 3. Requirements | The goal, requirement, BUC tree; the list | `diagram --kind requirement`; `list --kind requirement --format markdown` |
| 4. Non-goals | Requirements with `status "out-of-scope"` | the out-of-scope rows of `list --kind requirement` |
| 5. User journey | The business flow, then each BUC's steps | `diagram --kind business`; `diagram --kind flow --buc <BucId>` |
| 6. Acceptance criteria | The business rules | `list --kind rule --format markdown` |
| 7. Non-functional requirements | NFRs and constraints | `list --kind nfr --format markdown` |

## Design document

Order it as structure, then processing, then the rules each write keeps.

| Section | Content | Command |
|---|---|---|
| 1. Overview | System context; events between systems | `diagram --kind context`; `diagram --kind event` |
| 2. Per BUC | Screens, controls, and the entities each touches | `diagram --kind robustness --buc <BucId>` |
| | Screen transitions, when declared | `diagram --kind screen --buc <BucId>` |
| | Processing, use case after use case | `diagram --kind sequence --buc <BucId>` |
| | Use case table | `list --kind usecase --format json` |
| | Each use case's guards and writes, folded | `diagram --kind sequence --usecase <UcId>` |
| 3. State models | One state diagram per entity; reachable states of the main ones | `diagram --kind state --entity <EntityId>`; `states --entity <EntityId> --format table` |
| 4. Data model | ER diagram; column list | `diagram --kind er` (or `--entity <EntityId>` per aggregate); `list --kind entity --format markdown` |
| 5. Rules | The rules each write keeps | `list --kind rule --format markdown` |
| 6. CRUD and permissions | Which API writes which entity; who holds which permission | `csv --kind api-matrix --format markdown`; `list --kind actor-permission-audit --format markdown` |
| 7. Non-functional requirements | NFRs and constraints | `list --kind nfr --format markdown` |

## Embedding rules

- Generate; do not hand-edit. Embed the `.mmd` as a fenced `mermaid` block with its `%%`
  header, so a reader can regenerate it. Fix the model, not the diagram.
- Stay under Mermaid's limits (50,000 characters, 500 edges). Scope every embedded
  diagram: `--buc` per BUC, and `er --entity <EntityId>` per aggregate when the whole `er`
  warns.
- Put goals, requirements, roles, rules, CRUD, and permissions in `list` or `csv` tables
  with `--format markdown`, not in diagrams.
- Fold the per-use-case sequence diagrams under `<details>`; the BUC sequence comes first.
- Render an image that leaves the text behind with `--title` (and `--legend` on a
  flowchart): a renderer drops the `%%` header.
- Run `check` first. `state` refuses a model with lifecycle errors; other kinds draw it.

The repository's `samples/clinic-ops/prd.md` and `samples/clinic-ops/design-doc.md`
follow this structure; `scripts/gen-clinic-ops-docs.py` writes them.
