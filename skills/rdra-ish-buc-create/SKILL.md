---
name: rdra-ish-buc-create
description: Create a new BUC file from a requirements description, using staged refinement from BUC skeleton and business flow to data, UI/API contracts, lifecycle, rules, NFRs, and traceability
license: MIT
---

## Create a new BUC

Given a requirement or feature description, produce a validated BUC `.rdra` file and
any shared additions it needs.

If the requirement is abstract, create only the next useful stage instead of forcing a
complete model: invented columns, state machines, or endpoints read as decisions to the
reviewers. Treat creation as business-to-technical refinement: model value and actors
before data touchpoints, UI/API boundaries, entity structure, lifecycle, or rules. The
stage files of skill `rdra-ish-write` (`../rdra-ish-write/references/00-scope.md` to
`../rdra-ish-write/references/06-rules.md`) hold the syntax and validation for each stage; in the
`rdra-ish-dsl` repository, `docs/incremental-modeling.md` describes the same flow.

### Stage gate

Before writing, classify the available information:

| If the user provided... | Concern | Create now | Ask before adding... |
|-------------------------|---------|------------|----------------------|
| BUC name, business goal, source requirement | Biz intent | `buc`, `belongs`, optional `requirement` metadata and `motivates` | actors and use cases |
| actors, actions, and order | Biz value | `usecase`, `contains`, `performs`; add `flow`/`step` when order matters | entities and CRUD |
| touched business objects | Biz object touchpoints | coarse `entity`, CRUD predicates | screens and APIs |
| screens/API boundaries | Tech interaction boundary | `screen`, `field`, `api`, `dto`, `system`, `medium`, `permission`, `displays`, `shows`, `maps_field`, `invokes`, API contracts, access constraints | columns, relationships, cross-system coordination |
| fields and relationships | Tech data design | columns, `variation`s, indexes/checks, `relate`, optional `owns` | lifecycle |
| which use case moves which state, events between use cases, schedules | Tech lifecycle design | `transitions`, `sets`, `event`, `raises`, `triggers`, `timer`, `condition` | local guardrails first |
| invalid or mutually exclusive states | Tech-enforced rules | `forbidden`, `exclusive` | obligations and cross-entity rules |
| conditional/global/cross-entity rules, NFRs, decisions | Tech-enforced rules | `invariant`, narrow `required`, multi-entity `forbidden` / `invariant`, optional `when`/`property`/`after.assert`, `nfr`, `constraint`, `adr` links | none; validate with `check`, and `verify` when needed |

Ask only the questions needed to advance one row.

### Step 1 — Extract business terms at the current abstraction

Read the requirement and list:

- **Actors** — who initiates actions (human users); external systems that send or
  receive events
- **Business domain** — which business area this BUC belongs to
- **Requirements/decisions** — stable source, stakeholder, owner, priority, acceptance criteria, ADRs
- **Use cases** — verbs the actor performs (one `usecase` per user-visible action), and
  which actor performs each
- **Business flow** — ordered steps, branches, exceptions, loops, and which UCs/events each step covers
- **Screens** — UI pages shown during the flow, if the user is already at the interaction stage
- **Fields** — screen input/output fields, actor-entered vs system-derived, readonly/editable/required
- **Entities** — logical data objects created or modified, and values an actor chooses
  (variations)
- **Systems/APIs** — method/path, DTOs, errors, idempotency, sync/async mode, auth, and internal systems
- **Context/access** — the operation medium, and any actor permissions or UC/API media constraints
- **NFRs/constraints** — performance, availability, SLO, audit/logging/retention/privacy requirements
- **Lifecycle** — which use case moves which status value, if known; events that hand
  work to another use case; schedules (timers); business rules (conditions) that decide
  when a use case applies

### Step 2 — Decide what is shared vs. BUC-local

| Goes in `shared/` | Goes in `buc/buc_<name>.rdra` |
|-------------------|-------------------------------|
| `actor`, `extsystem` (if reused across BUCs) | `buc`, `usecase`, `screen` |
| `business`, stable `requirement`, `nfr`, `quality`, `constraint`, `adr`, `system`, `timer`, `medium`, `permission` | BUC-local `flow`, `step`, `api`, `dto`, `field` |
| reusable `entity` and `variation` definitions, `relate` | CRUD, `displays`, `shows`, `maps_field`, `invokes`, API contracts, `coordinates`, access constraints, conditions applied here, `raises`, `sets` |
| cross-BUC `event`, and `transitions` on shared entities | BUC-local `event` |
| cross-BUC or cross-entity `forbidden` / `invariant` / `required` / `exclusive` rules | predicates scoped to this BUC |

If a shared file already declares the actor or entity you need, import it; do not
redeclare. Start with `shared/actors.rdra`, `shared/biz.rdra`, `shared/entities.rdra`,
and one `buc/buc_<name>.rdra` file. Split `shared/entities.rdra` into
`shared/entities/<area>.rdra` only when it becomes hard to review.

### Step 3 — Write `buc/buc_<name>.rdra`

A BUC file at the data-touchpoint stage, with the shared files it imports:

```rdra
module shared.actors

actor Customer "Customer"
actor Clerk "Order Clerk"
```

```rdra
module shared.biz

business Commerce "Commerce"
```

```rdra
module shared.entities

entity Order "Order" {
  id: Int @pk
}
```

```rdra
module buc.order

import shared.actors
import shared.biz
import shared.entities

// 1. BUC-local instances
buc BucOrder "Process Order"

usecase PlaceOrder "Place Order"
usecase CancelOrder "Cancel Order"

screen OrderScreen "Order Screen"

// 2. BUC-level predicates
belongs(BucOrder, Commerce)
contains(BucOrder, PlaceOrder)
contains(BucOrder, CancelOrder)

// 3. Who performs which use case
performs(Customer, PlaceOrder)
performs(Customer, CancelOrder)
performs(Clerk, CancelOrder)

// 4. Per-UC predicates (CRUD, then displays, then raises)
creates(PlaceOrder, Order)
displays(PlaceOrder, OrderScreen)

updates(CancelOrder, Order)
displays(CancelOrder, OrderScreen)
```

Order predicates as: `belongs` → `contains` → `performs` → per-UC blocks.

Write `performs(Actor, Buc)` only for an actor who runs every use case of the BUC. A use
case's actors are its direct performers plus the performers of every BUC that contains
it, and `check` requires each of them to hold the use case's permissions; above, Clerk
cancels but does not place orders, so Clerk performs CancelOrder directly.

When the order, branch, exception, or loop is part of the requirement, add business
flow elements without turning them into implementation steps (`flow`, `step`,
`contains(BucOrder, Flow)`, `precedes`, `covers(Step, UseCase)`; see Step 1 of
`rdra-ish-write`).

At the later stages, add to the same file:

- **Medium context**: `belongs(BucOrder, Commerce).by(StoreTerminal)` with a declared
  `medium`. A schedule that starts a use case is a `timer` with
  `triggers(Timer, UseCase)`.
- **API boundary**: declare `system` and `api` (with `method`/`path` and DTOs),
  `contains(System, Api)`, `invokes(UseCase, Api)`, and move the CRUD onto the API. If a
  `relate` edge crosses two derived system entity sets, add
  `coordinates(UseCase, EntityA, EntityB)` and make the use case invoke APIs on both
  sides.
- **Access**: `requires_permission` and `requires_medium` on the use case or API,
  `has_permission` on each actor that performs it. Screen access is derived from
  `displays` and `invokes`; do not hand-write a screen predicate.
- **Event-started BUC**: `triggers(Event, TargetBuc)` first; when the entry use case is
  known, add `contains(TargetBuc, EntryUC)` and `triggers(Event, EntryUC)`. An external
  system sends with `raises(ExtSystem, Event)` and receives with
  `triggers(Event, ExtSystem)`.

### Step 4 — Add lifecycle effects

Name the use case that drives each state edge with
`transitions(Order.status, CancelOrder, pending -> cancelled)`; states are the Enum
variants and are never declared. For every use case that changes a nullable column, a
`Bool` flag, an Int, or an inline `Enum` that has no `transitions`, add
`sets(UseCase, Entity, column == value)`. A value the actor chooses and no use case
sets (a payment method, a channel) is not a state: type that column by a `variation`.

### Step 5 — Update shared files if needed

- New entity or variation → `shared/entities.rdra` with column definitions
- New actor or external system → `shared/actors.rdra`
- New cross-BUC event or `transitions` on a shared entity → the shared file nearest
  the entity (`shared/lifecycle/<area>.rdra` once split)
- New system → shared vocabulary; its entities are derived from
  `contains(System, Api)` + API CRUD, or add `owns(System, Entity)` only for
  intentional future ownership before API operations are complete
- New API contract/DTO → method/path metadata plus `request`, `response`, and
  `error_response` relations
- New NFR/constraint/quality/ADR → stable shared declaration scoped with
  `applies_to`, `qualifies`, `constrains`, or `decides`
- New timer/medium/permission → shared vocabulary when reused across BUCs
- New cross-entity rule → `shared/rules.rdra` or the shared file nearest the
  involved entities; qualify columns as `Entity.column`, and add `.along(...)` only
  when the rule is scoped to instances linked by a declared `relate` path
- New BUC-local API/screen/event → keep it in `buc/buc_<name>.rdra`
- If shared files are already split, mirror paths and modules, e.g.
  `shared/entities/order.rdra` → `module shared.entities.order`

### Step 6 — Validate

```sh
rdra-ish check src/
```

Fix every reported error. Common errors:

- Wrong predicate argument order
- Missing `import` for a symbol used across files
- `module` name does not match file path
- A removed word (`state`, `timing`, `uses`, `location`): the parse error names the
  replacement
- An event named in `transitions`: name the use case that drives the edge

For staged work, also run the command that matches the current abstraction:

- Any stage: `rdra-ish lint src/ --format table`
- BUC skeleton: `rdra-ish diagram src/ --kind rdra --format mermaid --buc <BucId>`
- Data touchpoints: `rdra-ish csv src/ --kind matrix`
- Interaction boundary: `rdra-ish diagram src/ --kind sequence --format mermaid --buc <BucId>`
- Access: `rdra-ish csv src/ --kind screen-constraints`,
  `rdra-ish csv src/ --kind permission-callables`, and
  `rdra-ish csv src/ --kind actor-permission-audit`; review `missing` and `excess`
  rows, or run `rdra-ish check src/ --strict` to make the gaps errors
- Lifecycle/rules: `rdra-ish check src/` reports states no use case reaches
  (`Witness_*`), stuck instances (`Deadlock_*`), and use cases that never run
  (`Runs_*`). `rdra-ish states src/ --entity <EntityId>` shows one entity's reachable
  patterns. `--buc <BucId>` drops the other BUCs' use cases, so it leaves those three
  findings undecided under `not checked:`.
- Two instances and temporal properties: `rdra-ish verify src/` (skill `rdra-ish-verify`)
- Contract/data exports: run the relevant form, for example
  `rdra-ish export src/ --kind openapi --out out/openapi.json`,
  `rdra-ish export src/ --kind asyncapi --out out/asyncapi.json`,
  `rdra-ish export src/ --kind dbml --out out/schema.dbml`, or
  `rdra-ish export src/ --kind json-schema --out out/json-schema.json`.
