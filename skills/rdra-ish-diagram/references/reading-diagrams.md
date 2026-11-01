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

- A node is `<kind>__<DSL id><open>"<DSL label>"<close>:::<kind>`, the brackets being the
  Mermaid shape of its kind. The id is the same in every kind, so `usecase__Capture` in a
  UC複合図 is `usecase__Capture` in `rdra`, `flow`, and `state`. Read the kind from the
  `:::<kind>` tag; the shape says the same thing to a reader of the picture.

  | Shape | Brackets | Kinds |
  |---|---|---|
  | stadium | `([".."])` | actor |
  | rounded | `("..")` | usecase |
  | rectangle | `[".."]` | screen, buc, step, system, value, field, dto, flow, medium, permission, quality, adr |
  | hexagon | `{{".."}}` | api |
  | cylinder | `[("..")]` | entity |
  | subroutine | `[[".."]]` | extsystem, statemodel |
  | asymmetric | `>".."]` | event |
  | circle | `((".."))` | timer |
  | double circle | `((("..")))` | state |
  | rhombus | `{".."}` | condition |
  | parallelogram | `[/".."/]` | goal |
  | trapezoid | `[/".."\]` | requirement |
  | parallelogram, leaning left | `[\".."\]` | nfr, constraint, variation |
  | trapezoid, upside down | `[\".."/]` | business |

  Kinds a focused diagram draws together never share a shape, except a `value`, which
  sits inside its variation's subgraph and is a rectangle like `system` in `condition`; in `rdra` a shape repeats
  across layers, so tell those apart by the tag and the layer subgraph.
- A subgraph with an element id is that element holding its parts:
  `business__<id>` (BUCs), `flow__<id>` (steps), `variation__<id>` (values).
- Ids that are not DSL elements of their own:
  - `state__<Entity>_<variant>`: a state of a state model.
  - `statemodel__<Entity>_<column>`: a state model, a column that `transitions` drive.
  - `variation__<Variation>__<value>`, tagged `:::value`: one value of a variation.
  - `lane__<Flow>__<Actor>`, a subgraph tagged by its `lane` kind: one actor's swimlane in
    one flow.
  - `system__Model`: the system in `context` when no declared system can stand for it.
- `classDef <kind> <style>;` lines close a flowchart. Mermaid needs them for the `:::`
  tags. Each kind family has its own fill so a plain render tells kinds apart; the style
  says nothing the `:::<kind>` tag does not.

## Edges

- An edge is `<from> <arrow>|<label>| <to>` or, without a label, `<from> <arrow> <to>`.
  A label is the DSL predicate (`reads`, `updates`, `precedes`, ...) or what the
  relation carries (an event, a use case, CRUD letters, a transition). The edge runs from
  the predicate's first argument to its second.
- An unlabelled edge is the one relation that kind draws between those two kinds:
  `actor -> usecase` is `performs`, `buc -> usecase` is `contains`, `event -> usecase`
  is `triggers`, a robustness `actor -> screen` is `uses` and `timer -> control` `triggers`.
  `rdra_ish_emit::implied_relation` gives the table. A pair with several relations keeps its label: a use case or API to an
  entity (the CRUD verb), step to step, entity to entity (the cardinality), API to DTO.
- `-->` is hierarchy (`contains`, `belongs`, `owns`) and flow order (`precedes`,
  `branches`, `repeats`); `---|N:1|` is a `relate` with its cardinality; `-.->` is every
  other association.
- A transition is `usecase__X -.->|transitions <Entity>.<column> <from>-><to>| state__...`.
- `context` labels an actor's edge (a `performs`) with the BUCs it performs and an
  external system's with the events exchanged; `business` labels a hand-off with its
  event.

## Comment Lines

| Line | Means |
|---|---|
| `%% desc <id>: <text>` | The node's description |
| `%% derived <from> -> <to> via <ids>` | The edge above it was derived through those nodes |
| `%% derived <from> -> <to> from holds` | The `combines` edge above it was read from the condition's `holds` body |
| `%% holds <id>: <body>` | The condition's body, comparisons joined by `, ` |
| `%% lane <lane>: actor__<id>` | The actor who performs every step in the lane |
| `%% synthesized system__Model: <reason>` | Why `context` drew a system node of its own |
| `%% via <from> -> <to> usecase__<id>` | In `screen`, the use case of the `navigates` edge between those screens |

An edge without a `%% derived` line is declared in the model and is the same edge in
`rdra` (a `screen` edge is the unlabelled `rdra` edge `screen__A -.-> screen__B`, a
`navigates`, labelled with its use case).

## Redrawing a Diagram

When you turn a `.mmd` into SVG or another notation:

- Keep every node with its kind, label, and layer or parent subgraph; draw the RDRA icon
  or shape of its kind if you have one.
- Keep every edge with its direction and label; write the implied relation on an
  unlabelled edge if the target notation has no shape or line for it. A derived edge may be drawn differently
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
