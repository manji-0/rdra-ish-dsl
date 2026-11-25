# System context, requirements, and business

Use these views for the questions above the use cases: who uses the system, why the BUCs
exist, and how the business divides into BUCs.

```sh
rdra-ish diagram src/ --kind context --format mermaid
rdra-ish diagram src/ --kind requirement --format mermaid
rdra-ish diagram src/ --kind business --buc BucOrder --format mermaid
rdra-ish list src/ --kind buc-roles --format markdown
```

## System context (`context`)

- No layer subgraphs. The whole model draws every actor, external system, and system,
  linked or not.
- Every edge is derived, with a `%% derived ... via <ids>` line naming the BUCs or events
  behind its label:
  - `actor__A -.->|<BUC labels>| system__S`: the BUCs A performs, `、`-joined.
  - `extsystem__X -.->|<event labels>| system__S`: events X raises that start the system's
    work.
  - `system__S -.->|<event labels>| extsystem__X`: events the system sends to X.
- An edge ends at each system that contains an API the use cases behind it invoke. With
  no traceable system it ends at `system__Model`, with a `%% synthesized` line.
- An actor or external system with no edge takes part in nothing yet.

## Requirement model (`requirement`)

- Read left to right: goal, requirement, BUC. A goal shows its metric and
  `baseline → target` on a second line; a goal without them is unmeasured.
- `requirement --> buc` is `motivates`. `goal --> requirement` is `achieves`, read from the
  goal, with a `%% derived` line.
- A requirement with `status "out-of-scope"` sits in `subgraph group_out_of_scope`
  (`やらないこと`) with dashed edges.
- NFRs and constraints are not drawn. Read them with `list --kind nfr --format markdown`
  and `list --kind constraint --format markdown`.
- A requirement with no goal, or a goal with no requirement, is a gap.

## Business flow (`business`)

- Each business is `subgraph business__<id>` holding its BUCs. A BUC of several
  businesses sits in the first by id, with `-->` (`belongs`) edges to the others.
- `buc__A -->|<event label>| buc__B` is a hand-off: a use case of A raises the event, and
  it starts B or a use case of B. `%% derived ... via event__<Id>` names it.
- A hand-off through data only has no edge. A BUC with no edge is a gap to review.
- `--buc` draws the named BUCs, the BUCs they hand off to or from, and their businesses.

## Role table (`list --kind buc-roles`)

One row per actor, then per external system, and one column per BUC. A cell reads
`担当` (performs the BUC, a use case, or a step), `イベント送信` (raises an event that
starts the BUC's work), or `イベント受信` (receives an event the BUC raises). An empty row
takes part in nothing; an empty column is a BUC nobody performs.
