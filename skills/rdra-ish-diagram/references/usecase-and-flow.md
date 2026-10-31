# Use Cases and Business Flows

Use this reference for what happens inside a BUC: the robustness diagram, the sequence diagram of one use case, and the UC複合図
of its use cases, and the business flow of its steps.

## Commands

```sh
# ロバストネス図 — the use cases of one BUC, or the named use cases, as boundary / control / entity
rdra-ish diagram src/ --kind robustness --buc <BucId> --format mermaid
rdra-ish diagram src/ --kind robustness --usecase <UcId> --format mermaid

# シーケンス図 — one use case in order; needs exactly one --usecase
rdra-ish diagram src/ --kind sequence --usecase <UcId> --format mermaid

# UC複合図 — the use cases of one BUC, or the named use cases
rdra-ish diagram src/ --kind usecase --buc <BucId> --format mermaid
rdra-ish diagram src/ --kind usecase --usecase <UcId> --format mermaid

# 業務フロー — the flows of one BUC
rdra-ish diagram src/ --kind flow --buc <BucId> --format mermaid
```

## Reading the ロバストネス図

- Nodes: actors; boundaries (`screen`, `extsystem`, `timer`); controls (`api`, or
  `usecase` when the use case has no API or reads and writes entities itself); `entity`.
  A node's class says which; the ids are those of `rdra`.
- Edges follow one grammar: `actor -->|uses| screen`, then
  `screen -->|<use case label>| control`, then `control -->|CRUD| entity`. The use case
  label on the screen edge is how to find the use case. `CRUD` is the letters of the
  control's operations on the entity, `writes` shown as `U`.
- Dotted edges carry events and timers: `control -.->|<event>| control` when one use case
  raises an event that starts another, and between a control and an `extsystem` or a
  `timer`.
- There is no edge from an actor to a control or from a boundary to an entity. A use case
  that displays no screen has no incoming screen edge, and its actor is not drawn.
- Use it as the main structural view of a BUC. Reach for the UC複合図 when the conditions,
  states, or what a screen shows matter.

## Reading the シーケンス図

- Read it after the robustness diagram: it says what one control does, in order. Participants
  are the performing actors (or the external system, timer, or use case that starts it), the
  screen, the controls, the entities, and the systems an event reaches.
- `alt <guards>` holds the work. The guards are the conditions with a `holds` body applied to
  the use case and the `from` state of each transition; `else guard fails` replies `rejected`.
- Each write names the operation, the lifecycle move (`status A -> B`), and the columns
  `sets` fixes. A `Note over` names a transaction. An event the use case raises is a `-)`
  message to the control it triggers, shown as one message and not expanded.
- It needs exactly one `--usecase`; `--buc` is refused.

## Reading the UC複合図

- Each use case in scope sits in システム境界 with the screens it displays and its events;
  actors and external systems in システム価値; timers in システム外部環境; the entities it
  touches, the conditions on it, and the states it moves in システム. The ids are those of
  `rdra`.
- Edges: `performs`, `displays`, `shows` (screen to entity), `reads` / `writes` /
  `creates` / `updates` / `deletes`, `raises` / `triggers` (with the events' external
  systems), `applies_to` (condition to use case), and
  `transitions <Entity>.<column> <from>-><to>` to the state entered.
- APIs are folded into the use case that invokes them: an API's CRUD is drawn from the
  use case, followed by `%% derived <uc> -> <entity> via api__<Api>`. An actor who
  performs the use case's BUC or step is drawn performing the use case, followed by
  `%% derived ... via buc__<Buc>` (or `step__<Step>`). A declared edge is never marked
  derived.
- `omitted:` names the kinds linked to a drawn use case or screen that are left out,
  such as `buc`, `api`, `permission`, and `medium`; `rdra` draws them.
- `--buc` keeps the use cases the BUC contains, directly or through a step of its flows;
  `--usecase` (repeatable, not with `--buc`) keeps the named ones.

## Reading the Business Flow

- The type line is `flowchart TD`. Each flow is `subgraph flow__<id>` holding its steps.
- `-->|precedes|`, `-->|branches|`, `-.->|excepts|`, and `-->|repeats|` run between steps;
  `-.->|covers|` runs from a step to the use cases, APIs, events, and timers it covers,
  which sit outside the flows.
- A step of a flow that exactly one actor performs (`performs(Actor, Step)`) sits in
  `subgraph lane__<Flow>__<Actor>`, whose first line is
  `%% lane lane__<Flow>__<Actor>: actor__<Actor>`. A step several actors perform has a
  `-.->|performs|` edge from each. Without `performs(Actor, Step)` there are no lanes.
- `--buc` draws the flows the BUC contains. A BUC without a flow gives an empty diagram
  (`%% kinds: (none)`).

## When To Use

- Use `usecase --buc <BucId>` to review what each use case of a BUC shows and touches,
  without the API and permission detail.
- Use `flow --buc <BucId>` when the order of the work matters: every step should cover a
  use case, and every exception should rejoin the flow.
- Add `rdra-ish csv src/ --kind matrix` when the question is CRUD coverage rather than
  connectivity.
