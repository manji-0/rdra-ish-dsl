# Use cases, flows, and screens

Use these views for what happens inside a BUC.

```sh
rdra-ish diagram src/ --kind robustness --buc BucOrder --format mermaid
rdra-ish diagram src/ --kind sequence --usecase PlaceOrder --format mermaid
rdra-ish diagram src/ --kind usecase --actor Customer --format mermaid
rdra-ish diagram src/ --kind flow --buc BucOrder --format mermaid
rdra-ish diagram src/ --kind screen --buc BucOrder --format mermaid
```

`usecase` and `robustness` take one scope: `--buc`, `--usecase` (repeatable), `--actor`
(the use cases the actor performs), or `--entity` (the use cases that read or write it).

## Robustness (`robustness`)

The main structural view of a BUC.

- Nodes: actors; boundaries (`screen`, `extsystem`, `timer`); controls (`api`, or the
  `usecase` when it invokes no API); `entity`.
- Edges follow one grammar: `actor --> screen`, then `screen -->|<use case label>| control`,
  then `control -->|CRUD| entity`. The screen edge's label names the use case; `CRUD` is the
  control's operation letters.
- Dotted edges carry events and timers: `control -.->|<event>| control`, the same to or
  from an `extsystem`, and an unlabelled `timer -.-> control` for `triggers`.
- No edge runs from an actor to a control or from a boundary to an entity. A use case with
  no screen has no incoming screen edge, and its actor is not drawn.

## Sequence (`sequence`)

- Takes exactly one `--usecase`, or `--buc` for the BUC's use cases one after another.
- Participants: the starter (actor, external system, timer, or triggering use case), the
  screen, the controls, the entities, and the systems an event reaches.
- `alt <guards>` holds the work. The guards are the conditions with a `holds` body applied
  to the use case and the `from` states of each moved column. `else guard fails` replies
  `rejected`.
- Each write names the operation, the move (`status A -> B`), the `sets` values, and each
  re-chosen column (`chooses`) as `col in (A, B)`. Arms to several states are an `alt` after
  the write, one branch per arm. An arm's or value's `.when(Cond)` shows in `[...]`.
- `Note over` names a transaction. A raised event is one `-)` message to the control it
  triggers.

## UC複合図 (`usecase`)

- Layers: actors and external systems in システム価値, use cases with their screens and
  events in システム境界, timers in システム外部環境, entities, conditions, and states in
  システム.
- Edges: `performs`, `displays`, `shows`, CRUD, `raises` / `triggers`, `-.->|applies_to|`
  from a condition, `-.->|when|` from a condition that guards an arm or `chooses` value of
  the use case, and `transitions <Entity>.<column> <from>-><to>` to the state entered.
- An API's CRUD is drawn from the use case that invokes it, with
  `%% derived <uc> -> <entity> via api__<Api>`. An actor who performs the BUC or a step
  is drawn performing the use case, with `%% derived ... via buc__<Buc>`.
- `--buc` keeps the use cases the BUC contains, directly or through a step.

## Business flow (`flow`)

- `flowchart TD`; each flow is `subgraph flow__<id>` holding its steps.
- Between steps: `-->|precedes|`, `-->|branches|`, `-.->|excepts|`, `-->|repeats|`. An
  unlabelled `-.->` (`covers`) runs from a step to the use cases, APIs, events, and timers
  it covers.
- A step one actor performs (`performs(Actor, Step)`) sits in `lane__<Flow>__<Actor>`. A
  step several actors perform has an unlabelled `-.->` from each. Without
  `performs(Actor, Step)` there are no lanes.
- A BUC without a flow gives `%% kinds: (none)`.

## Screen transitions (`screen`)

- `screen__A -->|<use case label>| screen__B` is `navigates(A, B).via(UseCase)`; match the
  use case by the `%% via screen__A -> screen__B usecase__<id>` line, not the label.
  `-->|navigates|` has no `.via`.
- `screen__A -.->|業務上の順番| screen__B` with a `%% derived` line is no transition: two
  consecutive steps in one actor's lane display A, then B, and no `navigates` joins them.
  Treat it as a candidate `navigates`.
- A screen with no edge is `lint` `screen-no-transition`.
- `--buc` keeps the BUC's screens and flows; a `navigates` to another BUC's screen shows
  only without `--buc`.

For CRUD coverage use `rdra-ish csv src/ --kind matrix`, not a diagram.
