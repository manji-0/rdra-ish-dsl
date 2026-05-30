---
name: rdra-buc-create
description: Create a new BUC file from a requirements description, using staged refinement from BUC skeleton to data, UI/API, lifecycle, and rules
---

## Create a new BUC

Given a requirement or feature description, produce a complete, validated BUC `.rdra` file and any shared additions needed.

If the requirement is abstract, create only the next useful stage instead of forcing a
complete model. Use `docs/incremental-modeling.md` as the reference flow.

### Stage gate

Before writing, classify the available information:

| If the user provided... | Create now | Ask before adding... |
|-------------------------|------------|----------------------|
| BUC name and business goal | `buc`, `belongs` | actors and use cases |
| actors and actions | `performs`, `usecase`, `contains` | entities and CRUD |
| touched business objects | coarse `entity`, CRUD predicates | screens and APIs |
| screens/API boundaries | `screen`, `api`, `displays`, `shows`, `invokes` | columns and relationships |
| fields and relationships | columns, `relate` | lifecycle states/events |
| lifecycle states/events | `state`, `event`, `transitions`, `raises`, `sets` | constraints |
| invalid/required combinations | `forbidden`, `invariant` | none; validate diagnostics |

Ask only the questions needed to advance one row. Do not invent detailed columns,
state machines, or API endpoints just to make the BUC look complete.

### Step 1 — Extract concepts at the current abstraction

Read the requirement and list:

- **Actors** — who initiates actions (human users, external systems)
- **Business domain** — which business area this BUC belongs to
- **Use cases** — verbs the actor performs (one `usecase` per user-visible action)
- **Screens** — UI pages shown during the flow, if the user is already at the interaction stage
- **Entities** — data objects created or modified, if the data stage is known
- **Events** — domain events raised as side effects, if lifecycle behavior is known
- **States** — status values if an entity lifecycle is known

### Step 2 — Decide what is shared vs. BUC-local

| Goes in `shared/` | Goes in `buc/buc_<name>.rdra` |
|-------------------|-------------------------------|
| `actor`, `extsystem` (if reused across BUCs) | `buc`, `usecase`, `screen` |
| `entity` column definitions, `relate` | `event`, `state` (if BUC-specific) |
| `state`, `event`, `transitions` (if shared) | predicates scoped to this BUC |

If a shared file already declares the actor or entity you need, import it — do not redeclare.

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
