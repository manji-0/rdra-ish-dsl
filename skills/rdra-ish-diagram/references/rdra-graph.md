# The whole RDRA graph

Use `rdra` when no narrower kind carries the relation (permissions, media, DTOs, the API
path of a use case), or to confirm that another kind's edge is declared. Every other kind
is a view of it.

```sh
rdra-ish diagram src/ --kind rdra --buc BucOrder --format mermaid
rdra-ish diagram src/ --kind rdra --format mermaid --node-kind usecase --node-kind api --edge-kind invokes
rdra-ish diagram src/ --kind rdra --format mermaid --node-kind api --node-kind dto --edge-kind request
```

## Reading it

- Four layer subgraphs:
  - `layer_value["システム価値"]`: actors, external systems, requirements, NFRs.
  - `layer_environment["システム外部環境"]`: qualities, constraints, businesses, BUCs,
    flows, steps, timers, media, permissions.
  - `layer_boundary["システム境界"]`: use cases, screens, fields, events.
  - `layer_system["システム"]`: systems, APIs, DTOs, entities, states, variations,
    conditions.
- ADRs are not drawn.
- An edge between two kinds that one predicate joins is unlabelled; a label stays where
  the kinds allow several (CRUD, `request` / `response`, step order, cardinality).
- A transition runs from the use case to the state it enters. `--buc` keeps the states
  its use cases move between.
- A condition's `combines` link read from its `holds` body points at the entity here (at
  the state model in `condition`), with `%% derived ... from holds`.
- `--node-kind` and `--edge-kind` hide nodes and their edges without changing the model;
  `omitted:` names what they hid. Narrow before reading a whole-model graph.

## API and access questions

A diagram does not expand the screen × use case × API constraint matrix. Read the tables:

```sh
rdra-ish csv src/ --kind api-matrix
rdra-ish csv src/ --kind business-inputs
rdra-ish csv src/ --kind actor-permission-audit
```

`check` prints the transaction and API-boundary warnings. Read the access CSVs with
`../../rdra-ish-buc-analyze/references/access-permissions.md`, and API ownership and
coordination with `../../rdra-ish-buc-analyze/references/api-system-boundaries.md`.
