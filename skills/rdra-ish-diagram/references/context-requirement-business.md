# System Context, Requirements, and Business

Use this reference for the questions above the use cases: who uses the system, what
requirements drive it, and how the business divides into BUCs.

## Commands

```sh
# システムコンテキスト — the system, its actors, and its external systems
rdra-ish diagram src/ --kind context --format mermaid
rdra-ish diagram src/ --kind context --buc <BucId> --format mermaid

# 要求モデル — the value tree: goals, requirements, BUCs
rdra-ish diagram src/ --kind requirement --format mermaid

# 業務の流れ図 — businesses, their BUCs, and the hand-offs between BUCs
rdra-ish diagram src/ --kind business --format mermaid
rdra-ish diagram src/ --kind business --buc <BucId> --format mermaid

# 担当表 — who takes part in each BUC, as a table to paste into a PRD
rdra-ish list src/ --kind buc-roles --format markdown
```

## Reading the System Context

- The diagram has no layer subgraphs and no `layers:` header line. The whole model draws
  every actor, external system, and system, linked or not.
- Every edge is derived and followed by `%% derived ... via <ids>`:
  - `actor__A -.->|<BUC labels>| system__S`: one edge per actor, labelled with the labels
    of the BUCs it performs (`、`-joined), directly or through a use case or step.
  - `extsystem__X -.->|<event labels>| system__S`: one edge, labelled with the events X
    raises that trigger one of the system's use cases or BUCs.
  - `system__S -.->|<event labels>| extsystem__X`: one edge, labelled with the events one
    of its use cases raises to X.
  - `via` lists the BUC or event ids the label names, so read it to find the element.
- With one declared system, every edge ends there. With several, an edge ends at each
  system that contains an API the use cases behind it invoke. When no system is declared,
  or none can be traced, it ends at `system__Model["システム"]`, with a
  `%% synthesized system__Model: <reason>` line. A traced model has no `system__Model`.
- An actor or external system without an edge takes part in nothing yet.

## Reading the Requirement Model

- Read it left to right: goal, requirement, BUC. There are no layer subgraphs. A goal
  node adds its metric and `baseline → target` on a second line, so a goal without them is
  unmeasured.
- `requirement -->|motivates| buc` is declared. `goal -->|achieved_by| requirement` is the
  declared `achieves` read from the goal, with a `%% derived` line.
- A requirement with `status "out-of-scope"` is in `subgraph group_out_of_scope`
  (`やらないこと`) with dashed edges: a non-goal, kept visible.
- NFRs, constraints, qualities, entities, and systems are not drawn; `omitted:` names
  them. Read them with `list --kind nfr --format markdown` and
  `list --kind constraint --format markdown`.
- A requirement with no goal edge, or a goal with no requirement, is a gap in the tree.
- `--buc` draws the BUCs, the requirements that motivate them, and their goals.

## Reading the Business Flow

- Each business is `subgraph business__<id>` holding its BUCs. A BUC of several businesses
  sits in the first by id with `-->|belongs|` edges to the others; a BUC of no business
  sits outside every subgraph.
- `buc__A -->|<event label>| buc__B` is a hand-off: a use case of A raises the event, and
  it starts a use case of B (or B itself). `%% derived ... via event__<Id>` names the
  event. One edge per event.
- No actor or external-system node is drawn. A hand-off with no event (a BUC feeding the
  next through data only) has no edge; a BUC with no edge at all is a gap in the model to
  review, not a layout accident.
- `--buc` draws the named BUCs, the BUCs they hand off to or from, and their businesses.

## Reading the Role Table

`list --kind buc-roles` has a row per actor, then per external system, and a column per
BUC, ordered by business. A cell says how the row takes part: `担当` (an actor performs the
BUC, a use case in it, or a step in it), `イベント送信` (an external system raises an event
that starts the BUC's use cases), `イベント受信` (an event a use case raises reaches the
external system). An empty row is an actor or external system that takes part in nothing; an
empty column is a BUC nobody performs.

## When To Use

- Start a review with `context` to see the boundary, and `requirement` to see why the
  BUCs exist.
- Use `business` to check that every BUC belongs to a business and connects to the BUCs
  around it, and `list --kind buc-roles` to check that every BUC has its actors.
- Go down to `usecase --buc <BucId>` (`references/usecase-and-flow.md`) for what happens
  inside a BUC.
