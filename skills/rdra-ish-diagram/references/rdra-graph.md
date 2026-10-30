# The Whole RDRA Graph

Use this reference when no narrower kind carries the relation in question (permissions,
media, DTOs, the API path of a use case), when an early model is too small to need
separate views, or to check that another kind's node or edge is a declared one.

## Commands

```sh
# RDRA graph — whole model
rdra-ish diagram src/ --kind rdra --format mermaid

# Scoped to one BUC, or the union of several
rdra-ish diagram src/ --kind rdra --buc <BucId> --format mermaid
rdra-ish diagram src/ --kind rdra --buc <BucA> --buc <BucB> --format mermaid

# Narrow a dense graph to some node and relation kinds
rdra-ish diagram src/ --kind rdra --format mermaid --node-kind actor --node-kind buc --node-kind usecase
rdra-ish diagram src/ --kind rdra --format mermaid --node-kind usecase --node-kind api --edge-kind invokes
rdra-ish diagram src/ --kind rdra --format mermaid --node-kind api --node-kind dto --edge-kind request
```

## Reading the RDRA Graph

- `rdra` is the base graph: every element in its RDRA layer and every relation between
  them. Every other kind is a view of it, and a node or declared edge it draws is the
  one drawn here.
- The four subgraphs are the RDRA layers: `layer_value["システム価値"]` (actors, external
  systems, requirements, NFRs), `layer_environment["システム外部環境"]` (qualities,
  constraints, businesses, BUCs, flows, steps, timers, media, permissions),
  `layer_boundary["システム境界"]` (use cases, screens, fields, events), and
  `layer_system["システム"]` (systems, APIs, DTOs, entities, states, variations,
  conditions). ADRs are not drawn.
- A transition runs from the use case to the state it enters, labelled
  `transitions <Entity>.<column> <from>-><to>`. A `--buc` scope also holds the states its
  use cases move between.
- A condition's `combines` link read from its `holds` body is followed by
  `%% derived ... from holds`; it points at the entity, where `condition` points at the
  state model.
- `omitted:` names the node and relation kinds `--node-kind` / `--edge-kind` hid.

## When To Use

- Prefer the narrower kind the routing guide names; reach for `rdra` when the question
  crosses them.
- Narrow with `--node-kind` and `--edge-kind` before reading a whole-model graph.
- Add `rdra-ish lint src/ --format table` when the graph shows orphan or unused nodes.
