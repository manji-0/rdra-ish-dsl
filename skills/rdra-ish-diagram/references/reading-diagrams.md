# Reading and Redrawing a Diagram

Use this reference to read any `.mmd` that `rdra-ish diagram` writes, or to redraw one
with another tool. Every kind follows the same encoding contract; `docs/cli-reference.md`
("Encoding contract") is the full statement.

## The Header

```text
%% rdra-ish diagram=usecase scope=buc:BucPayment model=samples/ec-site
%% layers: value=システム価値 boundary=システム境界 system=システム
%% kinds: actor extsystem usecase screen event entity state
%% omitted: buc api
flowchart LR
```

- `diagram=` is the kind. `scope=` is `whole`, `buc:<id>[,<id>]`, or
  `usecase:<id>[,<id>]`. `model=` is the input paths.
- `layers:` appears in the layered kinds (`rdra`, `context`, `requirement`, `business`,
  `usecase`) and lists the RDRA layers drawn, each `subgraph layer_<token>["<name>"]`.
- `kinds:` lists the node kinds the body draws; `(none)` means an empty diagram, which is
  a finding (for example, a BUC with no flow), not an error.
- `omitted:` lists the kinds linked to a drawn node that this kind leaves out, and in
  `rdra` what `--node-kind` / `--edge-kind` hid. A kind listed here exists in the model.

## Nodes

- A node is `<kind>__<DSL id>["<DSL label>"]:::<kind>`. The id is the same in every kind,
  so `usecase__Capture` in a UC複合図 is `usecase__Capture` in `rdra`, `flow`, and `state`.
- A subgraph with an element id is that element holding its parts:
  `business__<id>` (BUCs), `flow__<id>` (steps), `variation__<id>` (values).
- Ids that are not DSL elements of their own:
  - `state__<Entity>_<variant>`: a state of a state model.
  - `statemodel__<Entity>_<column>`: a state model, a column that `transitions` drive.
  - `variation__<Variation>__<value>`, tagged `:::value`: one value of a variation.
  - `lane__<Flow>__<Actor>`, a subgraph tagged by its `lane` kind: one actor's swimlane in
    one flow.
  - `system__Model`: the system in `context` when no declared system can stand for it.
- `classDef <kind> stroke:#333;` lines close a flowchart. Mermaid needs them for the
  `:::` tags; the style means nothing.

## Edges

- The label is the DSL predicate (`performs`, `contains`, `covers`, `applies_to`, ...).
  The edge runs from the predicate's first argument to its second.
- `-->` is hierarchy (`contains`, `belongs`, `owns`) and flow order (`precedes`,
  `branches`, `repeats`); `---|N:1|` is a `relate` with its cardinality; `-.->` is every
  other association.
- A transition is `usecase__X -.->|transitions <Entity>.<column> <from>-><to>| state__...`.
- `context` and `business` label an event link with its event: `raises <Event>`,
  `triggers <Event>`.

## Comment Lines

| Line | Means |
|---|---|
| `%% desc <id>: <text>` | The node's description |
| `%% derived <from> -> <to> via <ids>` | The edge above it was derived through those nodes |
| `%% derived <from> -> <to> from holds` | The `combines` edge above it was read from the condition's `holds` body |
| `%% holds <id>: <body>` | The condition's body, comparisons joined by `, ` |
| `%% lane <lane>: actor__<id>` | The actor who performs every step in the lane |
| `%% synthesized system__Model: <reason>` | Why `context` drew a system node of its own |

An edge without a `%% derived` line is declared in the model and is the same edge in
`rdra`.

## Redrawing a Diagram

When you turn a `.mmd` into SVG or another notation:

- Keep every node with its kind, label, and layer or parent subgraph; draw the RDRA icon
  or shape of its kind if you have one.
- Keep every edge with its direction and label. A derived edge may be drawn differently
  (for example dashed or grey), since `%% derived` says it was not declared.
- Show `%% desc` text as a tooltip or note if the target allows it.
- Do not add elements the header does not list. If the reader needs an omitted kind,
  generate the kind that carries it instead (`omitted: api` in a UC複合図 means `rdra`
  draws the APIs).

## Commands

```sh
rdra-ish diagram src/ --kind usecase --buc <BucId> --format mermaid --out out/usecase
mmdc -i out/usecase.mmd -o out/usecase.svg
```

`mmdc` is Mermaid CLI, outside rdra-ish.
