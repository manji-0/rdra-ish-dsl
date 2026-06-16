# Layered and Boundaryless Graphs

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

# Boundaryless relationship graph — whole model
rdra-ish diagram src/ --kind boundaryless-graph --format mermaid

# Boundaryless relationship graph — scoped to one BUC
rdra-ish diagram src/ --kind boundaryless-graph --buc <BucId> --format mermaid

# Focus a large graph
rdra-ish diagram src/ --kind rdra --format mermaid --view-preset business
rdra-ish diagram src/ --kind rdra --format mermaid --node-kind flow --node-kind step --edge-kind covers
```

## Reading RDRA Layered Output

- Four vertical layers = system value, external environment, system boundary, and system.
- `flow` and `step` nodes show business order; `api`, `dto`, and `entity` nodes are
  included in the system layer; screens, fields, events, and use cases stay in the
  system boundary layer.
- Object labels include kind prefixes such as actor, BUC, usecase, screen, API,
  DTO, field, entity, event, and state. DSL ids stay unchanged.
- Dashed arrows show interaction, CRUD, event, lifecycle, and constraint relationships.

## Reading Boundaryless Output

- Actor = rounded box.
- BUC or use case = rectangle.
- Entity = logical data cylinder.
- Screen = double-border.
- Field = form/input-like node.
- Event = diamond.
- Solid arrows usually show `performs` and `contains`.
- Dashed arrows show CRUD, `displays`, `raises`, and other relationship predicates.

## When To Use

- Use `rdra --buc <BucId>` for scope, BUC skeleton, and data-touchpoint reviews.
- Use `boundaryless-graph` when a layered graph is too sparse or layer placement hides
  relationship density.
- Add `rdra-ish csv src/ --kind matrix` when the question is CRUD coverage rather than
  visual connectivity.
- Add `rdra-ish lint src/ --format table` when the graph reveals orphan or unused
  nodes.
