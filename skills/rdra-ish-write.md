---
name: rdra-write
description: Write RDRA DSL files from requirements using correct syntax and file structure
---

## Write RDRA DSL

Create RDRA DSL files from requirements or specifications.

### File layout

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

### Syntax reference

#### Instance declaration

```
<kind> <Id> "display name"
```

| kind | meaning |
|------|---------|
| `actor` | human user |
| `extsystem` | external system |
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
| `api` | API layer endpoint invoked by a use case; operates entities |

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
| `contains` | `(Buc, UseCase)` | BUC contains a use case |
| `belongs` | `(Buc, Business)` | BUC belongs to a business domain |
| `motivates` | `(Requirement, Buc)` | requirement motivates a BUC |
| `relate` | `(Entity, Entity, "1:1"\|"1:N"\|"N:1"\|"N:M")` | ER relationship (auto-generates FK) |
| `transitions` | `(Event, State, State)` | state transition: event moves from → to |
| `sets` | `(UseCase\|Event, Entity, "col", "val")` | explicit column effect for state-pattern derivation |

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
```

#### API layer (`api` / `invokes`)

Use `api` when you need to express that a screen calls a backend API layer — the
sequence diagram then renders `Actor → Screen → API → Entity` lanes.

```
api OrderApi "Order API"
invokes(PlaceOrder, OrderApi)   // usecase delegates to the API
creates(OrderApi, Order)        // the API operates the entity
displays(PlaceOrder, OrderScreen)
```

- Declare `api` in the same BUC file as the use case that invokes it (or in `shared/`
  if multiple BUCs share it).
- CRUD predicates (`creates`, `updates`, etc.) are attached to the `api`, not the
  `usecase`. You may still attach CRUD directly to a `usecase` for the same entity
  (mixed form) — the sequence diagram handles both.
- `api` nodes are intentionally omitted from the RDRA overview (`--kind rdra`).

#### Imports

```
import shared.actors            // flat import — all symbols available directly
import shared.actors as a       // namespaced — reference as a.Customer
import shared.actors.{Staff}    // selective import
import shared.actors.{Staff as S}  // selective import with alias
```

### Step-by-step

1. **Extract domain concepts** — list nouns (entity candidates) and verbs (use case candidates) from the requirements
2. **Write shared files first**
   - `actors.rdra`: declare `actor` and `extsystem`
   - `biz.rdra`: declare `business`
   - `entities.rdra`: write entity column definitions, `relate`, and state/event declarations
3. **Write one BUC file per business use case**
   - Import shared definitions with `import shared.*`
   - Declare `buc`, `usecase`, `screen`, `event`, `state`
   - Write predicates in order: `performs` → `belongs` → `contains` → per-UC CRUD + `displays`
4. **Add `sets` where needed** — for every `Enum` column without state transitions and every nullable column that a use case modifies, add a `sets` predicate
5. **Validate** — run `rdra-ish check src/` and fix all errors before declaring done

### Common mistakes

- Swapping predicate argument order (e.g. `reads(Product, Browse)` — CRUD predicates take `(UseCase|Api, Entity)`)
- Writing `relate` cardinality without quotes (`N:1` instead of `"N:1"`)
- Adding quotes inside `Enum(...)` values — they are bare identifiers, not strings
- Forgetting the `module` declaration or using a dotted name that does not match the file path
- Adding FK columns manually when a `relate` already auto-generates them
- Attaching CRUD to a `usecase` when the intent is to go through an `api` — use `invokes` + CRUD on the `api`
- Forgetting `invokes(UseCase, Api)` — declaring an `api` without `invokes` triggers an `ApiNeverInvoked` warning
