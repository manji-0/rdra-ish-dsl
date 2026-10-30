# RDRA Layered Graph

Use this reference when reviewing BUC coverage, business flow, actor/use-case
ownership, traceability, or dense model relationships.

## Commands

```sh
# RDRA layered graph — whole model
rdra-ish diagram src/ --kind rdra --format mermaid

# RDRA layered graph — scoped to one BUC
rdra-ish diagram src/ --kind rdra --buc <BucId> --format mermaid

# RDRA layered graph — union of multiple BUCs
rdra-ish diagram src/ --kind rdra --buc <BucA> --buc <BucB> --format mermaid

# Focus a large graph
rdra-ish diagram src/ --kind rdra --format mermaid --node-kind actor --node-kind buc --node-kind usecase
rdra-ish diagram src/ --kind rdra --format mermaid --node-kind flow --node-kind step --edge-kind covers

# Businesses and their BUCs, with the actors and external systems of each BUC
rdra-ish diagram src/ --kind business --format mermaid
rdra-ish diagram src/ --kind business --buc <BucId> --format mermaid

# UC複合図 — the use cases of one BUC, or the named use cases
rdra-ish diagram src/ --kind usecase --buc <BucId> --format mermaid
rdra-ish diagram src/ --kind usecase --usecase <UcId> --format mermaid
```

## Reading RDRA Layered Output

- The header comes first: `%% rdra-ish diagram=rdra scope=... model=...`, then
  `%% layers:`, `%% kinds:` (the node kinds drawn), and `%% omitted:` (what a
  `--node-kind` / `--edge-kind` filter hid).
- Four subgraphs = the RDRA layers `layer_value["システム価値"]`,
  `layer_environment["システム外部環境"]`, `layer_boundary["システム境界"]`, and
  `layer_system["システム"]`.
- Actors, external systems, requirements, ADRs, and NFRs sit in the system value layer;
  businesses, BUCs, flows, steps, timers, media, permissions, qualities, and constraints
  in the external environment; use cases, screens, fields, and events in the system
  boundary; systems, APIs, DTOs, entities, derived states, variations, and conditions in
  the system layer.
- `flow` and `step` nodes show business order. A condition draws edges to its variation,
  its state model, and the use cases it applies to; a `--buc` view includes the
  conditions applied to its use cases.
- Each node id is `<kind>__<DSL id>` (`usecase__Capture`, `state__Order_paid`) and
  carries `:::<kind>`; the label is the DSL label.
- Edge labels are DSL predicate names. `-->` is hierarchy (`contains`, `belongs`,
  `owns`) or flow order (`precedes`, `branches`, `repeats`), `---` is `relate`
  (labelled with its cardinality), and `-.->` is every other association. A transition
  runs from the use case to the state it enters, labelled
  `transitions <Entity>.<column> <from>-><to>`.
- `%% desc <id>: <text>` follows a node that has a description, and
  `%% derived ...` follows an edge derived from another fact (a condition's
  `combines` derived from its `holds`).

## Reading the Business Diagram

- `--kind business` draws each business as `subgraph business__<id>` holding its BUCs,
  with actors and external systems in the system value layer. A BUC of no business or
  of several sits beside the businesses, the latter with `belongs` edges.
- Only `performs(Actor, Buc)` is declared. An actor of one of the BUC's use cases or
  steps, an external system whose event (`raises <Event>`) starts the BUC or one of its
  use cases, and an external system a use case's event reaches (`triggers <Event>`) are
  derived, each followed by `%% derived ... via <node>`.

## Reading the UC複合図

- `--kind usecase` draws each use case in scope with the actors who perform it, the
  screens it displays and the entities they show, the entities it reads and writes,
  the events it raises or that trigger it with their external systems, the timers that
  trigger it, the conditions that apply to it, and the states it moves. The layers, ids,
  and edge labels are those of `rdra`.
- APIs are folded into the use case that invokes them: an API's CRUD is drawn from the
  use case, followed by `%% derived <uc> -> <entity> via api__<Api>`. An actor who
  performs the use case's BUC or step is drawn performing the use case, followed by
  `%% derived ... via buc__<Buc>` (or `step__<Step>`). A declared edge is never marked
  derived.
- `%% omitted:` names the kinds linked to a drawn use case or screen that the diagram
  leaves out, such as `buc`, `api`, `permission`, and `medium`.
- `--buc` keeps the use cases the BUC contains, directly or through a step of its flows;
  `--usecase` (repeatable, not with `--buc`) keeps the named ones.

## When To Use

- Use `rdra --buc <BucId>` for scope, BUC skeleton, and data-touchpoint reviews.
- Use `usecase --buc <BucId>` to review what each use case of a BUC touches, without the
  API and permission detail.
- Narrow `rdra` with `--node-kind` and `--edge-kind` when the whole graph is too dense
  to read.
- Add `rdra-ish csv src/ --kind matrix` when the question is CRUD coverage rather than
  visual connectivity.
- Add `rdra-ish lint src/ --format table` when the graph reveals orphan or unused
  nodes.
