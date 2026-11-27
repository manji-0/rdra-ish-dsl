# Reading and redrawing a diagram

<!-- derived-from ../../../docs/cli-reference.md#encoding-contract -->

Every kind follows one encoding contract. `docs/cli-reference.md` ("Encoding contract")
is the full statement.

## What each kind draws

| Kind | RDRA diagram | Draws | Leaves out |
|---|---|---|---|
| `context` | システムコンテキスト | One derived edge per actor (labelled with its BUCs) and per external system and direction (labelled with the events) | BUCs, use cases, events as nodes |
| `requirement` | 要求モデル | Goals (metric, `baseline → target`), requirements, BUCs; out-of-scope requirements apart | NFRs, constraints, qualities, entities, systems, ADRs |
| `business` | 業務の流れ図 | Businesses holding BUCs, and a hand-off edge per event between BUCs | Actors, external systems, use cases, steps |
| `usecase` | UC複合図 | Use cases with actors, screens, entities, events, external systems, timers, conditions, transitions | APIs (CRUD folded into the use case), DTOs, permissions, media, fields, BUCs |
| `robustness` | ロバストネス図 | Actors, boundaries (screens, external systems, timers), controls (APIs, or the use case), and entities, linked in that order | Events and conditions as nodes, states, DTOs, permissions, BUCs |
| `sequence` | シーケンス図 | One use case in order: guards as `alt`, writes with `sets` and the lifecycle move, raised events | Other use cases (a triggered one is one message), BUCs, DTOs |
| `flow` | 業務フロー | Flows, steps, `precedes` / `branches` / `excepts` / `repeats`, `covers`, actor lanes | Screens, entities |
| `screen` | 画面遷移図 | Screens, a solid edge per `navigates`, a dashed `業務上の順番` edge from the flows | Use cases, fields, entities as nodes |
| `information` | 情報モデル | Entities, `relate`, state models, variations | Columns |
| `condition` | バリエーション・条件 | Conditions with what they combine and apply to; variations with their values | Entities |
| `er` | (logical data model) | Entities, columns, keys, cardinality | Variation values, state models |
| `state` | 状態モデル | One composite state per state model, start and end states, the use case of each transition | `Enum` columns without `transitions` |
| `event` | システム間連携 | `raises` and `triggers` among use cases, BUCs, external systems, events, timers | States and transitions |
| `rdra` | RDRAGraph | Every element in its layer and every relation | ADRs |

## The header

```text
%% rdra-ish diagram=usecase scope=buc:BucPayment model=samples/ec-site
%% layers: value=システム価値 boundary=システム境界 system=システム
%% kinds: actor extsystem usecase screen event entity state
%% omitted: buc api
flowchart LR
```

- `diagram=` is the kind. `scope=` is `whole`, `buc:<ids>`, `usecase:<ids>` (also what
  `--actor` and `--entity` resolve to on `usecase` and `robustness`), or `entity:<ids>`.
  `model=` is the input paths.
- `layers:` appears on the layered kinds (`rdra`, `usecase`) and names each
  `subgraph layer_<token>`.
- `kinds:` lists the node kinds drawn. `(none)` is an empty diagram, a finding (a BUC
  with no flow), not an error.
- `omitted:` lists linked kinds this kind leaves out. Check it before you conclude the
  model lacks something.

## Nodes

A node is `<kind>__<DSL id><shape>"<label>"<shape>:::<kind>`. The id is the same in every
kind; match nodes across diagrams by id, never by label. Read the kind from the tag.

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

Ids that are no DSL element of their own:

- `state__<Entity>_<variant>`: a state; `state__<Entity>_<column>_<variant>` when the
  entity has several lifecycle columns.
- `statemodel__<Entity>_<column>`: a column that `transitions` drive.
- `variation__<Variation>__<value>`, tagged `:::value`: one value of a variation.
- `lane__<Flow>__<Actor>`: one actor's swimlane in a flow.
- `system__Model`: the system in `context` when no declared system stands for it.

A subgraph with an element id holds its parts: `business__<id>`, `flow__<id>`,
`variation__<id>`. `classDef` lines style the tags and carry no other meaning.

## Edges

- An edge runs from the predicate's first argument to its second. Its label is the DSL
  predicate, or what the relation carries (an event, a use case, CRUD letters, a
  transition).
- An unlabelled edge is the one relation the kind draws between its two end kinds:
  `actor -> usecase` is `performs`, `buc -> usecase` is `contains`, `event -> usecase` is
  `triggers`, a robustness `actor -> screen` is `uses`.
- `-->` is hierarchy and flow order, `---|N:1|` is a `relate`, `-.->` is every other
  association.
- A transition is `usecase__X -.->|transitions <Entity>.<column> <from>-><to>| state__...`.
- An edge followed by `%% derived <from> -> <to> via <ids>` was derived; any other edge is
  declared and is also in `rdra`.

## Comment lines

| Line | Means |
|---|---|
| `%% desc <id>: <text>` | The node's description |
| `%% derived <from> -> <to> via <ids>` | The edge above was derived through those nodes |
| `%% derived <from> -> <to> from holds` | The link above was read from the condition's `holds` body |
| `%% holds <id>: <body>` | The condition's body |
| `%% lane <lane>: actor__<id>` | The actor who performs every step in the lane |
| `%% synthesized system__Model: <reason>` | Why `context` drew a system of its own |
| `%% via <from> -> <to> usecase__<id>` | In `screen`, the use case of that `navigates` edge |
| `%% transition usecase__<id>` | In `state`, the use case of the next arrow |

## Redrawing

- Keep every node with its kind, label, and layer or parent subgraph.
- Keep every edge with its direction and label. Write the implied relation on an
  unlabelled edge when the target notation needs one. A derived edge may look different.
- Show `%% desc` text as a tooltip or note when the target allows.
- Add no element the header does not list. For an omitted kind, generate the kind that
  draws it (`omitted: api` in a UC複合図 means `rdra` draws the APIs).

```sh
rdra-ish diagram src/ --kind usecase --buc BucOrder --out out/usecase
mmdc -i out/usecase.mmd -o out/usecase.svg
```

`mmdc` is Mermaid CLI, outside rdra-ish.
