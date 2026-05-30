---
name: rdra-buc-create
description: Create a new BUC file from a requirements description, using staged refinement from BUC skeleton to data, UI/API, lifecycle, and rules
---

## Create a new BUC

Given a requirement or feature description, produce a complete, validated BUC `.rdra` file and any shared additions needed.

If the requirement is abstract, create only the next useful stage instead of forcing a
complete model. Use `docs/incremental-modeling.md` as the reference flow.
Treat creation as business-to-technical refinement: model value and actors before
adding data touchpoints, UI/API boundaries, entity structure, lifecycle, or rules.

### Stage gate

Before writing, classify the available information:

| If the user provided... | Concern | Create now | Ask before adding... |
|-------------------------|---------|------------|----------------------|
| BUC name and business goal | Biz intent | `buc`, `belongs` | actors and use cases |
| actors and actions | Biz value | `performs`, `usecase`, `contains` | entities and CRUD |
| touched business objects | Biz object touchpoints | coarse `entity`, CRUD predicates | screens and APIs |
| screens/API boundaries | Tech interaction boundary | `screen`, `api`, `system`, `displays`, `shows`, `invokes` | columns, relationships, cross-system coordination |
| fields and relationships | Tech data design | columns, `relate` | lifecycle states/events |
| lifecycle states/events | Tech lifecycle design | `state`, `event`, `transitions`, `raises`, `sets` | constraints |
| invalid/required combinations | Tech-enforced rules | `forbidden`, `invariant` | none; validate diagnostics |

Ask only the questions needed to advance one row. Do not invent detailed columns,
state machines, or API endpoints just to make the BUC look complete.

### Step 1 — Extract concepts at the current abstraction

Read the requirement and list:

- **Actors** — who initiates actions (human users, external systems)
- **Business domain** — which business area this BUC belongs to
- **Use cases** — verbs the actor performs (one `usecase` per user-visible action)
- **Screens** — UI pages shown during the flow, if the user is already at the interaction stage
- **Entities** — data objects created or modified, if the data stage is known
- **Systems/APIs** — internal systems that own API boundaries, if backend ownership is known
- **Events** — domain events raised as side effects, if lifecycle behavior is known
- **States** — status values if an entity lifecycle is known

### Step 2 — Decide what is shared vs. BUC-local

| Goes in `shared/` | Goes in `buc/buc_<name>.rdra` |
|-------------------|-------------------------------|
| `actor`, `extsystem` (if reused across BUCs) | `buc`, `usecase`, `screen` |
| `business`, stable `requirement`, `system` | BUC-local `api` |
| reusable `entity` definitions, `relate` | CRUD, `displays`, `shows`, `invokes`, `coordinates`, `raises`, `sets` |
| cross-BUC `state`, `event`, `transitions` | BUC-local `event`, `state` |
| cross-BUC `forbidden`, `invariant` | predicates scoped to this BUC |

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

When the flow goes through APIs, attach CRUD to the API, not the use case, and let the
use case declare value effects with `sets`:

```
system <System> "<system label>"
api <Api> "<API label>"

contains(<System>, <Api>)
invokes(<UC>, <Api>)
updates(<Api>, <Entity>)
sets(<UC>, <Entity>, "status", "updated")
```

If a `relate` edge crosses two derived system entity sets, add
`coordinates(<UC>, <EntityA>, <EntityB>)` and make `<UC>` invoke APIs on both system
sides.

### Step 4 — Add `sets` for non-transition column effects

For every use case that modifies an `Enum` column without a state machine, a nullable column, or a `Bool` flag, add a `sets` predicate:

```
sets(<UC>, <Entity>, "column_name", "value")
```

See the `sets` value vocabulary in `rdra-write`.

### Step 5 — Update shared files if needed

- New entity → add to `shared/entities.rdra` with column definitions
- New actor → add to `shared/actors.rdra`
- New event/state/transitions → add to `shared/entities.rdra` if cross-BUC
- New system → add to shared vocabulary; its entities are derived from `contains(System, Api)` + API CRUD
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
- BUC skeleton: `rdra-ish diagram src/ --kind rdra --format mermaid --buc <BucId>`
- Data touchpoints: `rdra-ish csv src/ --kind matrix`
- Interaction boundary: `rdra-ish diagram src/ --kind sequence --format mermaid --buc <BucId>`
- Lifecycle/rules: `rdra-ish states src/ --buc <BucId>`
