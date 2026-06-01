---
name: rdra-ish-write
description: Write RDRA DSL files from requirements using correct syntax and file structure, including staged abstract-to-concrete refinement
---

## Write RDRA DSL

Create RDRA DSL files from requirements or specifications.

<!-- derived-from ../../docs/language-reference.md#cross-entity-constraints -->

### Context Loading Rule

Use hierarchical context. Keep this top-level skill as the routing layer, then load
only the step file needed for the current abstraction level. If the current level is
unclear, load the previous, current, and next step files, classify the model, then
continue with the single most relevant step.

| Step | Concern | Load | Main output |
|---|---|---|---|
| 0 | Scope sketch | `references/00-scope.md` | `business`, candidate `buc` |
| 1 | BUC skeleton | `references/01-buc-skeleton.md` | actors, BUC ownership, contained use cases |
| 2 | Data touchpoints | `references/02-data-touchpoints.md` | coarse entities and UC CRUD |
| 3 | Interaction boundary | `references/03-interaction-boundary.md` | screens, APIs, systems, media, permissions |
| 4 | Entity structure | `references/04-entity-structure.md` | columns, keys, relations, coordination |
| 5 | Lifecycle | `references/05-lifecycle.md` | events, states, transitions, event-started BUCs, effects |
| 6 | Rules | `references/06-rules.md` | forbidden, invariant, and cross-entity constraints |

Do not skip ahead because a downstream detail is tempting. Model the smallest useful
increment, run validation, then advance one level.

### Core Workflow

1. Locate the existing model root and current abstraction level.
2. Load the matching step file from `references/`.
3. Ask only for information required by that step, unless the codebase already answers it.
4. Edit the smallest set of `.rdra` files needed for the step.
5. Run the validation commands from the step file.
6. Stop when the step's achievement conditions are met, or report the remaining open items.

### File Layout

Start small. Split files only when reviewability or ownership demands it.

```text
src/
  shared/
    actors.rdra      # module shared.actors
    biz.rdra         # module shared.biz
    entities.rdra    # module shared.entities
  buc/
    buc_<name>.rdra  # module buc.<name>
```

Growth pattern:

```text
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
  reusable entities, systems, locations, timings, media, permissions, cross-BUC
  lifecycle, and cross-BUC / cross-entity rules.
- BUC-local flow goes in `buc/buc_<name>.rdra`: `buc`, `usecase`, `screen`,
  BUC-local `api`, CRUD, `displays`, `invokes`, `raises`, `triggers`,
  `coordinates`, access constraints, and `sets`.
- Do not put BUC-specific predicates in shared files.
- Every file starts with `module <dotted.name>`, and the dotted module should mirror
  the file path.
- Keep broad imports during exploration; narrow imports after shared files split.

### Syntax Quick Reference

#### Instance Declarations

```rdra
actor Customer "Customer"
business Commerce "Commerce"
buc BucOrder "Process Order"
usecase PlaceOrder "Place Order"
entity Order "Order" {
  id: Int @pk
  status: Enum(draft, submitted) @default(draft)
  submitted_at: DateTime @null
}
```

Kinds commonly used by this skill:

| Kind | Use |
|---|---|
| `actor` / `extsystem` | people and outside systems |
| `business` / `buc` / `usecase` | business value and action boundaries |
| `screen` / `api` / `system` | interaction and implementation boundaries |
| `entity` / `state` / `event` | data, lifecycle, and causality |
| `location` / `timing` / `medium` / `permission` | context and access vocabulary |

#### Predicate Signatures

| Predicate | Signature | Meaning |
|---|---|---|
| `performs` | `(Actor, UseCase\|Buc)` | actor performs a use case or BUC |
| `contains` | `(Buc, UseCase)` / `(System, Api)` | composition |
| `belongs` | `(Buc, Business)` | BUC ownership, optionally chained with `.when/.where/.by` |
| `uses` | `(Actor, ExtSystem)` | actor uses an external system |
| `reads/writes/creates/updates/deletes` | `(UseCase\|Api, Entity)` | data touchpoint or API operation |
| `displays` | `(UseCase, Screen)` | UI path |
| `shows` | `(Screen, Entity)` | screen data exposure |
| `invokes` | `(UseCase, Api)` | UC delegates to an API boundary |
| `coordinates` | `(UseCase, Entity, Entity)` | UC coordinates cross-system consistency |
| `raises` | `(UseCase, Event)` | UC emits a domain event |
| `triggers` | `(Event, UseCase\|Buc)` | event starts a UC or a BUC boundary |
| `transitions` | `(Event, State, State)` | event moves state from -> to |
| `sets` | `(UseCase\|Event, Entity, "col", "val")` | explicit column effect |
| `sets` | `(UseCase\|Event, Entity, col op rhs, true\|false)` | comparison proposition effect |
| `forbidden` | `(Entity, (col, val)\|col op rhs, ...)` | forbidden reachable state combination |
| `invariant` | `(Entity).when(...).then(...)` | required co-occurrence inside one entity |
| `cross_forbidden` | `(Entity..., (Entity.col, val)\|Entity.col op rhs, ...)` | forbidden combination across entities |
| `cross_invariant` | `(Entity...).when(...).then(...)` | required co-occurrence across entities |
| `relate` | `(Entity, Entity, "1:1"\|"1:N"\|"N:1"\|"N:M")` | ER relation, auto-generates FK |
| `has_permission` | `(Actor, Permission)` | actor-side grant |
| `requires_permission` | `(UseCase\|Api, Permission)` | UC/API required authority |
| `requires_medium` | `(UseCase\|Api, Medium)` | UC/API required operation medium |

#### Imports

```rdra
import shared.actors
import shared.actors as a
import shared.actors.{Staff}
import shared.actors.{Staff as S}
```

### Cross-Step Principles

- Preserve staged refinement: direct UC CRUD is acceptable in early stages; prefer
  `api` plus `invokes` once an implementation boundary matters.
- Treat API CRUD as an atomic entity-operation boundary. Split APIs by consistency
  contract, not by screen button count.
- Treat system ownership as derived from `contains(System, Api)` plus API CRUD; do
  not create direct system-to-entity ownership predicates.
- Model screen constraints indirectly through `displays`, `invokes`,
  `requires_permission`, and `requires_medium`.
- Use `rdra-ish csv src/ --kind permission-callables` to review which operations each
  permission enables; use `has_permission` for actor-side assignment and verify it with
  `rdra-ish csv src/ --kind actor-permission-audit`.
- Prefer `triggers(Event, Buc)` when an event starts a downstream BUC. Add
  `triggers(Event, UseCase)` later when the concrete entry action is known.
- Use `sets` to make lifecycle effects explicit for `Enum`, `Bool`, nullable columns,
  and comparison propositions.
- Use `cross_forbidden` / `cross_invariant` when a rule mentions columns from more
  than one entity; qualify multi-entity columns as `Entity.column`.

### Common Mistakes

- Swapping predicate argument order: CRUD predicates take `(UseCase|Api, Entity)`.
- Writing `relate` cardinality without quotes.
- Adding quotes inside `Enum(...)` values.
- Forgetting `module`, or using a module name that does not match the file path.
- Adding FK columns manually when `relate` already generates them.
- Declaring `api` without `invokes(UseCase, Api)`.
- Declaring `system` without `contains(System, Api)`.
- Adding a cross-system `relate` without `coordinates(UseCase, Entity, Entity)`.
- Checking only `screen-constraints` for access review; also run
  `permission-callables` and `actor-permission-audit`.
- Modeling event-started BUCs only as `triggers(Event, UseCase)` when the BUC
  boundary itself should remain swappable between human and event initiation.
- Writing bare column names in multi-entity cross constraints; use `Entity.column`.
