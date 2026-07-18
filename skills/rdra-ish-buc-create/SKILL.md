---
name: rdra-ish-buc-create
description: Create a new BUC file from a requirements description, using staged refinement from BUC skeleton and business flow to conceptual/data, UI/API contracts, lifecycle, rules, NFRs, and traceability
license: MIT
---

## Create a new BUC

Given a requirement or feature description, produce a complete, validated BUC `.rdra` file and any shared additions needed.

If the requirement is abstract, create only the next useful stage instead of forcing a
complete model. Use `../../docs/incremental-modeling.md` as the reference flow.
Treat creation as business-to-technical refinement: model value and actors before
adding data touchpoints, UI/API boundaries, entity structure, lifecycle, or rules.

### Stage gate

Before writing, classify the available information:

| If the user provided... | Concern | Create now | Ask before adding... |
|-------------------------|---------|------------|----------------------|
| BUC name, business goal, source requirement | Biz intent | `buc`, `belongs`, optional `requirement` metadata and `motivates` | actors and use cases |
| actors, actions, and order | Biz value | `performs`, `usecase`, `contains`; add `flow`/`step` when order matters | entities and CRUD |
| touched business/conceptual objects | Biz object touchpoints | `concept`/`domain_object`/`aggregate`/`valueobject`, coarse `entity`, CRUD predicates, optional `maps_to` | screens and APIs |
| screens/API boundaries | Tech interaction boundary | `screen`, `field`, `api`, `dto`, `system`, `medium`, `permission`, `displays`, `shows`, `maps_field`, `invokes`, API contracts, access constraints | columns, relationships, cross-system coordination |
| fields and relationships | Tech data design | columns, indexes/checks, `relate`, optional `owns`, `maps_to` | lifecycle states/events |
| lifecycle states/events | Tech lifecycle design | `state`, `event`, `transitions`, `raises`, `sets` | local guardrails first |
| invalid or mutually exclusive states | Tech-enforced rules | `forbidden`, `exclusive` | obligations and cross-entity rules |
| conditional/global/cross-entity rules, NFRs, decisions | Tech-enforced rules | `invariant`, narrow `required`, multi-entity `forbidden` / `invariant`, optional `when`/`property`/`after.assert`, `nfr`, `constraint`, `adr` links | none; validate with `states` and TLA+ when needed |

Ask only the questions needed to advance one row. Do not invent detailed columns,
state machines, or API endpoints just to make the BUC look complete.

### Step 1 — Extract concepts at the current abstraction

Read the requirement and list:

- **Actors** — who initiates actions (human users, external systems)
- **Business domain** — which business area this BUC belongs to
- **Requirements/decisions** — stable source, stakeholder, owner, priority, acceptance criteria, ADRs
- **Use cases** — verbs the actor performs (one `usecase` per user-visible action)
- **Business flow** — ordered steps, branches, exceptions, loops, and which UCs/events each step covers
- **Screens** — UI pages shown during the flow, if the user is already at the interaction stage
- **Fields** — screen input/output fields, actor-entered vs system-derived, readonly/editable/required
- **Concepts/entities** — conceptual objects, aggregates/value objects, and logical data objects created or modified
- **Systems/APIs** — method/path, DTOs, errors, idempotency, sync/async mode, auth, and internal systems
- **Context/access** — when/where/by-what-medium the BUC applies, and any actor permissions or UC/API media constraints
- **NFRs/constraints** — performance, availability, SLO, audit/logging/retention/privacy requirements
- **Events** — domain events raised as side effects, if lifecycle behavior is known
- **States** — status values if an entity lifecycle is known

### Step 2 — Decide what is shared vs. BUC-local

| Goes in `shared/` | Goes in `buc/buc_<name>.rdra` |
|-------------------|-------------------------------|
| `actor`, `extsystem` (if reused across BUCs) | `buc`, `usecase`, `screen` |
| `business`, stable `requirement`, `nfr`, `quality`, `constraint`, `adr`, `system`, `location`, `timing`, `medium`, `permission` | BUC-local `flow`, `step`, `api`, `dto`, `field` |
| reusable `concept` / `domain_object` / `aggregate` / `valueobject`, reusable `entity` definitions, `relate` | CRUD, `displays`, `shows`, `maps_field`, `invokes`, API contracts, `coordinates`, access constraints, UC conditions, `raises`, `sets` |
| cross-BUC `state`, `event`, `transitions` | BUC-local `event`, `state` |
| cross-BUC or cross-entity `forbidden` / `invariant` / `required` / `exclusive` rules | predicates scoped to this BUC |

If a shared file already declares the actor or entity you need, import it — do not redeclare.
Start with `shared/actors.rdra`, `shared/biz.rdra`, `shared/entities.rdra`, and one
`buc/buc_<name>.rdra` file. Split `shared/entities.rdra` into
`shared/entities/<area>.rdra` only when it becomes hard to review.

### Step 3 — Write `buc/buc_<name>.rdra`

```
module buc.<name>

import shared.actors
import shared.biz
import shared.entities

// 1. Declare BUC-local instances
buc Buc<Name> "<BUC display name>"

usecase <UC1> "<action 1>"
usecase <UC2> "<action 2>"

screen <Screen1> "<screen name>"

// 2. BUC-level predicates
performs(<Actor>, Buc<Name>)
belongs(Buc<Name>, <Business>)

// 3. UC composition
contains(Buc<Name>, <UC1>)
contains(Buc<Name>, <UC2>)

// 4. Per-UC predicates (CRUD → displays → raises)
creates(<UC1>, <Entity>)
displays(<UC1>, <Screen1>)
raises(<UC1>, event::<Event>)

updates(<UC2>, <Entity>)
displays(<UC2>, <Screen1>)
```

Order predicates as: `performs` → `belongs` → `contains` → per-UC blocks.

When the order, branch, exception, or loop is part of the requirement, add business
flow elements without turning them into implementation steps:

```rdra
flow <Flow> "<flow label>"
step <Step1> "<business step>"
step <Step2> "<business step>"

contains(Buc<Name>, <Flow>)
contains(<Flow>, <Step1>)
contains(<Flow>, <Step2>)
precedes(<Step1>, <Step2>)
covers(<Step2>, <UC1>)
```

If the Business-BUC mapping depends on a timing, place, or physical medium, use a
method chain on `belongs`:

```
timing <When> "<timing>"
location <Where> "<place or channel>"
medium <Medium> "<device or terminal>"

belongs(Buc<Name>, <Business>)
  .when(<When>)
  .where(<Where>)
  .by(<Medium>)
```

When the flow goes through APIs, attach CRUD to the API, not the use case, and let the
use case declare value effects with `sets`:

```
system <System> "<system label>"
api <Api> "<API label>" method POST path "/orders" idempotency idempotent mode sync auth bearer
dto <RequestDto> "<request label>" {
  id: Int
}

contains(<System>, <Api>)
invokes(<UC>, <Api>)
request(<Api>, <RequestDto>)
updates(<Api>, <Entity>)
sets(<UC>, <Entity>, status == updated)
```

If a `relate` edge crosses two derived system entity sets, add
`coordinates(<UC>, <EntityA>, <EntityB>)` and make `<UC>` invoke APIs on both system
sides.

When the operation has authority or device constraints, declare them on the use case
or API. Attach actor-side authority separately:

```
permission <Permission> "<permission>"

has_permission(<Actor>, <Permission>)
requires_permission(<UC>, <Permission>)
requires_medium(<UC>, <Medium>)
requires_permission(<Api>, <Permission>)
```

Screen-level access patterns are derived from `displays(<UC>, <Screen>)` and
`invokes(<UC>, <Api>)`. Validate them with
`rdra-ish csv src/ --kind screen-constraints`. Inspect which use cases and APIs each
permission enables with `rdra-ish csv src/ --kind permission-callables`. Validate
actor-side grants with `rdra-ish csv src/ --kind actor-permission-audit`; review
`missing` and `excess` rows before accepting the BUC.

For a BUC that starts from an event, declare the BUC-level handoff first:

```
triggers(<Event>, <TargetBuc>)
```

When the entry use case is known, add the concrete refinement:

```
contains(<TargetBuc>, <EntryUC>)
triggers(<Event>, <EntryUC>)
```

### Step 4 — Add `sets` for non-transition column effects

For every use case that modifies an `Enum` column without a state machine, a nullable column, or a `Bool` flag, add a `sets` predicate:

```
sets(<UC>, <Entity>, column_name == value)
```

See the `sets` value vocabulary in `rdra-ish-write`.

### Step 5 — Update shared files if needed

- New concept/domain object/aggregate/value object → add to a shared conceptual file,
  then use `maps_to` only when the logical entity mapping is known
- New entity → add to `shared/entities.rdra` with column definitions
- New actor → add to `shared/actors.rdra`
- New event/state/transitions → add to `shared/entities.rdra` if cross-BUC
- New system → add to shared vocabulary; its entities are derived from
  `contains(System, Api)` + API CRUD, or add `owns(System, Entity)` only for
  intentional future ownership before API operations are complete
- New API contract/DTO → add method/path metadata plus `request`, `response`, and
  `error_response` relations
- New NFR/constraint/quality/ADR → add stable shared declaration and scope it with
  `applies_to`, `qualifies`, `constrains`, or `decides`
- New location/timing/medium/permission → add to shared vocabulary when reused across BUCs
- New cross-entity rule → add to `shared/rules.rdra` or the shared file nearest the
  involved entities, qualify columns as `Entity.column`, and add `.along(...)` only
  when the rule is intentionally scoped to linked instances on a declared `relate` path
- New BUC-local API/screen/event → keep it in `buc/buc_<name>.rdra`
- If shared files are already split, mirror paths and modules, e.g.
  `shared/entities/order.rdra` → `module shared.entities.order`

### Step 6 — Validate

```
rdra-ish check src/
```

Fix every reported error. Common errors:
- Wrong predicate argument order
- Missing `import` for a symbol used across files
- `relate` cardinality not quoted
- `module` name does not match file path

For staged work, also run the command that matches the current abstraction:
- Any stage: `rdra-ish lint src/ --format table`
- BUC skeleton: `rdra-ish diagram src/ --kind rdra --format mermaid --buc <BucId>`
- Data touchpoints: `rdra-ish csv src/ --kind matrix`
- Interaction boundary: `rdra-ish diagram src/ --kind sequence --format mermaid --buc <BucId>`
- Access constraints: `rdra-ish csv src/ --kind screen-constraints`
- Permission callables: `rdra-ish csv src/ --kind permission-callables`
- Actor permission assignments: `rdra-ish csv src/ --kind actor-permission-audit`
- Lifecycle/rules: `rdra-ish states src/ --buc <BucId>`
- Int/temporal/multi-entity: `rdra-ish export src/ --kind tla -o out/`
- Contract/data exports: run the relevant form, for example
  `rdra-ish export src/ --kind openapi --out out/openapi.json`,
  `rdra-ish export src/ --kind asyncapi --out out/asyncapi.json`,
  `rdra-ish export src/ --kind dbml --out out/schema.dbml`, or
  `rdra-ish export src/ --kind json-schema --out out/json-schema.json`.
