# System Context, Requirements, and Business

Use this reference for the questions above the use cases: who uses the system, what
requirements drive it, and how the business divides into BUCs.

## Commands

```sh
# システムコンテキスト — the system, its actors, and its external systems
rdra-ish diagram src/ --kind context --format mermaid
rdra-ish diagram src/ --kind context --buc <BucId> --format mermaid

# 要求モデル — requirements, NFRs, constraints, and what they bind
rdra-ish diagram src/ --kind requirement --format mermaid

# ビジネスコンテキスト / BUC — businesses, their BUCs, and who takes part in each
rdra-ish diagram src/ --kind business --format mermaid
rdra-ish diagram src/ --kind business --buc <BucId> --format mermaid
```

## Reading the System Context

- Actors and external systems sit in システム価値, systems in システム. The whole model
  draws every actor, external system, and system, linked or not.
- Every edge is derived and followed by `%% derived ... via <ids>`:
  - `actor__A -.->|performs| system__S` through each BUC, use case, or step A performs.
  - `extsystem__X -.->|raises <Event>| system__S` when the event triggers one of the
    system's use cases or BUCs.
  - `system__S -.->|triggers <Event>| extsystem__X` when one of its use cases raises the
    event.
- With one declared system, every edge ends there. With several, an edge ends at each
  system that contains an API the use cases behind it invoke. When no system is declared,
  or none can be traced, it ends at `system__Model["システム"]`, with a
  `%% synthesized system__Model: <reason>` line. A traced model has no `system__Model`.
- An actor or external system without an edge takes part in nothing yet.

## Reading the Requirement Model

- Requirements, goals, and NFRs sit in システム価値; qualities, constraints, and BUCs in
  システム外部環境; use cases in システム境界; APIs, systems, entities, and DTOs in システム.
- The edges are declared: `motivates` (requirement to BUC), `achieves` (requirement to
  goal), `applies_to` (NFR to use
  case, API, or system), `constrains` (constraint to use case, API, system, entity, or
  DTO), and `qualifies` (NFR or constraint to quality).
- The DSL has no requirement-to-actor link, so the diagram draws none; ADRs are left out
  and named in `omitted:`.
- `--buc` draws what links to the BUC, its use cases and their APIs, the entities those
  read and write, and the systems containing those APIs.

## Reading the Business Diagram

- Each business is `subgraph business__<id>` holding its BUCs in システム外部環境; actors
  and external systems sit in システム価値. A BUC of no business, or of several, sits
  beside them, the latter with its `belongs` edges.
- Only `performs(Actor, Buc)` is declared. An actor of one of the BUC's use cases or
  steps, an external system whose event (`raises <Event>`) starts the BUC or one of its
  use cases, and an external system a use case's event reaches (`triggers <Event>`) are
  derived, each followed by `%% derived ... via <node>`.
- `--buc` draws the named BUCs and their businesses: the RDRA BUC diagram.

## When To Use

- Start a review with `context` to see the boundary, and `requirement` to see why the
  BUCs exist.
- Use `business` to check that every BUC belongs to a business and has its actors.
- Go down to `usecase --buc <BucId>` (`references/usecase-and-flow.md`) for what happens
  inside a BUC.
