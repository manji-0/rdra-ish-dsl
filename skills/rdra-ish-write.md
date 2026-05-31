---
name: rdra-write
description: Write RDRA DSL files from requirements using correct syntax and file structure, including staged abstract-to-concrete refinement
---

## Write RDRA DSL

Create RDRA DSL files from requirements or specifications.

<!-- derived-from ../docs/language-reference.md#access-constraints -->
<!-- derived-from ../docs/language-reference.md#belongs-context -->
<!-- derived-from ../docs/incremental-modeling.md#stage-3-interaction-boundary -->

Prefer incremental refinement unless the user explicitly asks for a fully detailed
model in one pass. Start from the current abstraction level, ask only for the missing
information needed to advance one level, then validate before adding more detail.
Treat the ladder as a deliberate shift from business concerns to technical concerns:
business value and actors first, then data touchpoints, UI/API boundaries, entity
structure, lifecycle, and enforceable rules.

### Abstraction ladder

| Level | Concern | Model focus | Required information before moving on |
|-------|---------|-------------|----------------------------------------|
| 0. Scope | Biz intent | `business`, rough `buc` | business area, candidate BUC names, first BUC to model |
| 1. BUC skeleton | Biz value | `actor`, `usecase`, `performs`, `contains` | actors, user-visible actions, BUC ownership |
| 2. Data touchpoints | Biz object touchpoints | coarse `entity`, CRUD predicates | objects touched by each use case, create/read/update/delete intent |
| 3. Interaction boundary | Tech interaction boundary | `screen`, `api`, `system`, `medium`, `permission`, `displays`, `shows`, `invokes`, access constraints | UI/API boundary, system ownership, required permissions/media |
| 4. Entity structure | Tech data design | columns, `@pk`, `relate` | fields, identifiers, cardinality, ownership |
| 5. Lifecycle | Tech lifecycle design | `Enum`, `Bool`, `@null`, `event`, `state`, `transitions`, `sets` | state-changing use cases/events and column effects |
| 6. Rules | Tech-enforced rules | `forbidden`, `invariant` | invalid combinations and required co-occurrences |

### Information-gathering rule

When information is missing, ask targeted questions for the next abstraction level
instead of inventing detailed entities, columns, APIs, or state machines.

Use these prompts:

- From level 0 to 1: "Who performs this BUC, and what are the user-visible actions?"
- From level 1 to 2: "For each use case, which business objects are created, read, updated, or deleted?"
- From level 2 to 3: "Which screens or API endpoints mediate these use cases, which system owns each API, and are there permission or device/media constraints?"
- From level 3 to 4: "What fields identify each entity, and how are the entities related?"
- From level 4 to 5: "Which fields represent lifecycle state, and which use cases/events change them?"
- From level 5 to 6: "Which reachable state combinations are invalid or require another value to be present?"

See `docs/incremental-modeling.md` for the full staged flow.

### File layout

Start with the small layout. Split files only when reviewability or ownership demands
it; do not create deep folders for a new or uncertain model.

```
src/
  shared/
    actors.rdra      # module shared.actors
    biz.rdra         # module shared.biz
    entities.rdra    # module shared.entities
  buc/
    buc_<name>.rdra  # module buc.<name>
```

- Put shared definitions (actors, businesses, entities) under `shared/`
- One `.rdra` file per BUC
- Every file starts with a `module <dotted.name>` declaration
- File paths should mirror modules: `shared/entities/order.rdra` uses
  `module shared.entities.order`
- BUC files keep the `buc_<name>.rdra` filename and `module buc.<name>`

Growth pattern:

```
src/
  shared/
    actors.rdra
    biz.rdra
    entities/
      order.rdra      # module shared.entities.order
      payment.rdra    # module shared.entities.payment
    lifecycle/
      order.rdra      # module shared.lifecycle.order
    rules.rdra        # module shared.rules
  buc/
    buc_order.rdra
    buc_payment.rdra
```

Placement rules:
- Shared vocabulary goes in `shared/`: actors, external systems, businesses,
  reusable entities, systems, locations, timings, media, permissions, cross-BUC lifecycle, and cross-BUC rules.
- BUC-local flow goes in `buc/buc_<name>.rdra`: `buc`, `usecase`, `screen`,
  BUC-local `api`, CRUD, `displays`, `invokes`, `raises`, `coordinates`, access constraints, and `sets`.
- Do not put BUC-specific predicates in shared files.
- Keep broad imports during exploration; narrow imports after shared files split.
- See `docs/incremental-modeling.md#model-directory-layout` for the full layout rule.

### Syntax reference

#### Instance declaration

```
<kind> <Id> "display name"
<kind> <Id> "display name" description "longer description"
```

| kind | meaning |
|------|---------|
| `actor` | human user |
| `extsystem` | external system |
| `system` | internal system boundary derived from API CRUD targets |
| `requirement` | business requirement |
| `business` | business domain |
| `buc` | business use case |
| `usagescene` | usage scene |
| `usecase` | use case |
| `screen` | UI screen |
| `event` | domain event |
| `entity` | data entity |
| `state` | state (for state machines) |
| `condition` | condition |
| `variation` | variation |
| `api` | API layer endpoint invoked by a use case; operates entities atomically |
| `location` | place, channel, organization point, or usage-scene context |
| `timing` | timing, trigger, or business situation context |
| `medium` | physical device, terminal, channel, or operation medium |
| `permission` | permission or role-like authority assignable to actors |

Id must be UpperCamelCase (e.g. `Customer`, `BucOrder`).

#### Entity column definition

```
entity Order "Order" {
  id:         Int      @pk
  total:      Money
  ordered_at: DateTime
  status:     Enum(draft, paid, shipped) @default(draft)
  note:       String   @null
}
```

Types: `Int` / `String` / `Money` / `DateTime` / `Date` / `Bool` / `Decimal` / `Enum(...)`

Annotations: `@pk` / `@pk(a, b)` (compound PK) / `@unique` / `@null` / `@default(v)` / `@label("...")`

#### Predicate signatures

| predicate | signature | meaning |
|-----------|-----------|---------|
| `performs` | `(Actor, UseCase\|Buc)` | actor performs a use case |
| `uses` | `(Actor, ExtSystem)` | actor uses an external system |
| `invokes` | `(UseCase, Api)` | use case invokes an API layer |
| `reads` | `(UseCase\|Api, Entity)` | use case or API reads entity |
| `writes` | `(UseCase\|Api, Entity)` | use case or API writes entity |
| `creates` | `(UseCase\|Api, Entity)` | use case or API creates entity |
| `updates` | `(UseCase\|Api, Entity)` | use case or API updates entity |
| `deletes` | `(UseCase\|Api, Entity)` | use case or API deletes entity |
| `displays` | `(UseCase, Screen)` | use case displays a screen |
| `shows` | `(Screen, Entity)` | screen shows entity data |
| `raises` | `(UseCase, Event)` | use case raises a domain event |
| `triggers` | `(Event, UseCase)` | event triggers a use case |
| `contains` | `(Buc, UseCase)` / `(System, Api)` | BUC contains a use case, or system contains an API |
| `coordinates` | `(UseCase, Entity, Entity)` | use case handles consistency for a relation crossing systems |
| `belongs` | `(Buc, Business)` | BUC belongs to a business domain |
| `motivates` | `(Requirement, Buc)` | requirement motivates a BUC |
| `has_permission` | `(Actor, Permission)` | actor has a permission type |
| `requires_permission` | `(UseCase\|Api, Permission)` | use case or API requires a permission |
| `requires_medium` | `(UseCase\|Api, Medium)` | use case or API requires an operation medium |
| `relate` | `(Entity, Entity, "1:1"\|"1:N"\|"N:1"\|"N:M")` | ER relationship (auto-generates FK) |
| `transitions` | `(Event, State, State)` | state transition: event moves from → to |
| `sets` | `(UseCase\|Event, Entity, "col", "val")` | explicit column effect for state-pattern derivation |
| `sets` | `(UseCase\|Event, Entity, col op rhs, true\|false)` | explicit comparison-proposition effect |

#### `sets` value vocabulary

Use `sets` when a use case or event changes a column that has no `transitions` predicate (e.g. `Enum` without a state machine, nullable columns, `Bool` flags).

| value | target column | meaning |
|-------|---------------|---------|
| Enum variant name | `Enum` column | set to that variant |
| `"true"` / `"false"` | `Bool` column | set bool value |
| `"present"` | `@null` column | make non-null (value present) |
| `"null"` | `@null` column | set to null |
| PostgreSQL type name (`"timestamptz"`, `"jsonb"`, `"uuid"`, `"inet"`) | `@null` column | non-null with recorded type |

```
sets(usecase::Capture,  Payment, "status", "captured")
sets(usecase::Login,    Session, "last_login_at", "present")
sets(usecase::Deliver,  Order,   "delivered_at", "timestamptz")
sets(usecase::Logout,   Session, "token", "null")
sets(usecase::Sell,     Stock,   stock < selling, true)
```

#### API layer (`api` / `invokes`)

Use `api` when you need to express that a screen calls a backend API layer — the
sequence diagram then renders `Actor → Screen → API → Entity` lanes.

Treat an API as a consistency boundary: it groups data reads and writes that require
the same transaction or reference-integrity guarantee across use cases. Do not create
one API just because there is one use case, screen action, or entity. Prefer API CRUD
for backend-mediated data access; direct use-case CRUD is legacy/early-stage shorthand.
Use `sets(UseCase, Entity, ...)` to declare how the use case changes values through
the invoked API.

```
api OrderApi "Order API"
invokes(PlaceOrder, OrderApi)   // usecase delegates to the API
creates(OrderApi, Order)        // the API operates the entity
displays(PlaceOrder, OrderScreen)
```

- Declare `api` in the same BUC file as the use case that invokes it (or in `shared/`
  if multiple BUCs share it).
- CRUD predicates (`creates`, `updates`, etc.) on the `api` define the API's atomic
  entity boundary. A use case that invokes the API inherits that boundary.
- Attach value effects to the `usecase` with `sets`; this keeps "what this use case
  changes" separate from "which API owns the data operation".
- Split APIs by consistency contract. For example, changing a store's next restock date
  can be direct `updates(ChangeNextRestockDate, Store)`, while changing the store's
  parent organization should use a separate API that reads the organization and updates
  the store or assignment history in one transaction.
- `api` nodes are included in the RDRA layered graph (`--kind rdra`) and omitted from
  the dense boundaryless graph (`--kind boundaryless-graph`).

#### System boundaries (`system` / `contains` / `coordinates`)

Use `system` to group APIs into an internal ownership boundary. A system does not own
entities directly; its entity set is derived from the CRUD targets of its APIs.

```
system StoreSystem "Store System"
system OrgSystem "Organization System"

api StoreApi "Store API"
api OrgApi "Organization API"

contains(StoreSystem, StoreApi)
contains(OrgSystem, OrgApi)

updates(StoreApi, Store)
reads(OrgApi, Organization)
```

If `relate(Store, Organization, "N:1")` crosses derived system boundaries, declare
which use case coordinates the cross-system consistency and invoke APIs on both sides:

```
usecase ChangeParentOrg "Change Store Parent Organization"

coordinates(ChangeParentOrg, Store, Organization)
invokes(ChangeParentOrg, StoreApi)
invokes(ChangeParentOrg, OrgApi)
sets(ChangeParentOrg, Store, "parent_org_changed_at", "timestamptz")
```

The checker warns when a cross-system relation has no `coordinates`, when
`coordinates` is used for a non-cross-system pair, or when the coordinating use case
does not invoke an API that operates each side.

#### Business context and access constraints

Use `location`, `timing`, and `medium` to name reusable context values. Attach them
to the Business-BUC mapping with `belongs(...).when(...).where(...).by(...)` when the
context describes where/when/by-what-medium the BUC applies. Arguments may be string
literals or typed references.

```
timing AppointmentRequested "Appointment Requested"
location FrontDesk "Front Desk"
medium StaffTerminal "Staff Terminal"

belongs(BucBooking, ClinicOps)
  .when(AppointmentRequested)
  .where(FrontDesk)
  .by(StaffTerminal)
```

Use `permission` and `has_permission` for actor-side authority. Use
`requires_permission` and `requires_medium` on a use case or API for constraints that
must hold when the operation runs. Screen constraints are derived from
`displays(UseCase, Screen)` plus the use case's invoked APIs; inspect them with
`rdra-ish csv src/ --kind screen-constraints`.

```
permission ScheduleWrite "Schedule Write"

has_permission(Staff, ScheduleWrite)
requires_permission(BookAppointment, ScheduleWrite)
requires_medium(BookAppointment, StaffTerminal)
requires_permission(BookingApi, ScheduleWrite)
```

#### Imports

```
import shared.actors            // flat import — all symbols available directly
import shared.actors as a       // namespaced — reference as a.Customer
import shared.actors.{Staff}    // selective import
import shared.actors.{Staff as S}  // selective import with alias
```

### Step-by-step

1. **Pick the current abstraction level** — do not skip ahead if the user has only provided coarse intent
2. **Extract domain concepts** — list nouns (entity candidates) and verbs (use case candidates) from the requirements
3. **Ask for the next missing information** — use the abstraction ladder prompts above
4. **Write shared files first**
   - `actors.rdra`: declare `actor` and `extsystem`
   - `biz.rdra`: declare `business`
   - `entities.rdra`: write entity column definitions, `relate`, and state/event declarations
  - declare `system`, `location`, `timing`, `medium`, and `permission` near stable shared vocabulary when reused
5. **Write one BUC file per business use case**
   - Import shared definitions with `import shared.*`
   - Declare `buc`, `usecase`, `screen`, `event`, `state`
   - Write predicates in order: `performs` → `belongs` context → `contains` → actor permissions → per-UC/API CRUD + `displays` + access constraints
6. **Add `sets` where needed** — for every `Enum` column without state transitions and every nullable column that a use case modifies, add a `sets` predicate
7. **Validate at the current level** — run `rdra-ish check src/` and the smallest useful `diagram`, `csv`, or `states` command before declaring done

### Common mistakes

- Swapping predicate argument order (e.g. `reads(Product, Browse)` — CRUD predicates take `(UseCase|Api, Entity)`)
- Writing `relate` cardinality without quotes (`N:1` instead of `"N:1"`)
- Adding quotes inside `Enum(...)` values — they are bare identifiers, not strings
- Forgetting the `module` declaration or using a dotted name that does not match the file path
- Adding FK columns manually when a `relate` already auto-generates them
- Attaching CRUD to a `usecase` when the intent is to go through an `api` — use `invokes` + CRUD on the `api`
- Forgetting `invokes(UseCase, Api)` — declaring an `api` without `invokes` triggers an `ApiNeverInvoked` warning
- Declaring `system` without `contains(System, Api)` — no entity set can be derived
- Adding a cross-system `relate` without `coordinates(UseCase, Entity, Entity)`
- Declaring `coordinates` but invoking only one side's API
- Using a string literal for an access constraint when a shared `permission` or `medium` vocabulary should be reusable
- Expecting screen constraints to be hand-written — derive them through `displays`, `invokes`, `requires_permission`, and `requires_medium`
