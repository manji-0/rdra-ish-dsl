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

## When To Use

- Use `rdra --buc <BucId>` for scope, BUC skeleton, and data-touchpoint reviews.
- Narrow `rdra` with `--node-kind` and `--edge-kind` when the whole graph is too dense
  to read.
- Add `rdra-ish csv src/ --kind matrix` when the question is CRUD coverage rather than
  visual connectivity.
- Add `rdra-ish lint src/ --format table` when the graph reveals orphan or unused
  nodes.
