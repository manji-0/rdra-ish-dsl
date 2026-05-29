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
Annotations: `@pk` / `@unique` / `@null` / `@default(v)` / `@label("...")`

#### Predicate signatures

| predicate | signature | meaning |
|-----------|-----------|---------|
| `performs` | `(Actor, UseCase\|Buc)` | actor performs a use case |
| `uses` | `(Actor, ExtSystem)` | actor uses an external system |
| `reads` | `(UseCase, Entity)` | use case reads entity |
| `writes` | `(UseCase, Entity)` | use case writes entity |
| `creates` | `(UseCase, Entity)` | use case creates entity |
| `updates` | `(UseCase, Entity)` | use case updates entity |
| `deletes` | `(UseCase, Entity)` | use case deletes entity |
| `displays` | `(UseCase, Screen)` | use case displays a screen |
| `shows` | `(Screen, Entity)` | screen shows entity data |
| `raises` | `(UseCase, Event)` | use case raises a domain event |
| `triggers` | `(Event, UseCase)` | event triggers a use case |
| `contains` | `(Buc, UseCase)` | BUC contains a use case |
| `belongs` | `(Buc, Business)` | BUC belongs to a business domain |
| `motivates` | `(Requirement, Buc)` | requirement motivates a BUC |
| `relate` | `(Entity, Entity, "N:1"\|"1:1"\|"N:M")` | ER relationship (auto-generates FK) |
| `transitions` | `(Event, State, State)` | state transition: event moves from → to |

#### Imports

```
import shared.actors            // flat import — all symbols available directly
import shared.actors as a       // namespaced — reference as a.Customer
import shared.actors.{Staff}    // selective import
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
4. **Validate** — run `rdra check src/` and fix all errors before declaring done

### Common mistakes

- Swapping predicate argument order (e.g. `reads(Product, Browse)` — CRUD predicates take `(UseCase, Entity)`)
- Writing `relate` cardinality without quotes (`N:1` instead of `"N:1"`)
- Adding quotes inside `Enum(...)` values — they are bare identifiers, not strings
- Forgetting the `module` declaration or using a dotted name that does not match the file path
