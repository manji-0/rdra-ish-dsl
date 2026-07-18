---
name: rdra-ish-write
description: Write RDRA DSL files from requirements using correct syntax and file structure, including staged abstract-to-concrete refinement, business flows, requirements/NFRs, conceptual models, screen fields, API contracts, data modeling annotations, ADR links, lint/format checks, and exports
license: MIT
---

## Write RDRA DSL

Create RDRA DSL files from requirements or specifications.

### Context Loading Rule

Use hierarchical context. Keep this top-level skill as the routing layer, then load
only the step file needed for the current abstraction level. If the current level is
unclear, load the previous, current, and next step files, classify the model, then
continue with the single most relevant step.

| Step | Concern | Load | Main output |
|---|---|---|---|
| 0 | Scope sketch | `references/00-scope.md` | `business`, candidate `buc`, early `requirement` metadata |
| 1 | BUC skeleton | `references/01-buc-skeleton.md` | actors, BUC ownership, contained use cases, optional `flow`/`step` order |
| 2 | Data touchpoints | `references/02-data-touchpoints.md` | concepts/domain objects, coarse entities, UC CRUD |
| 3 | Interaction boundary | `references/03-interaction-boundary.md` | screens, fields, APIs/DTO contracts, systems, media, permissions |
| 4 | Entity structure | `references/04-entity-structure.md` | columns, indexes, constraints, relations, ownership, coordination |
| 5 | Lifecycle | `references/05-lifecycle.md` | events, states, transitions, event-started BUCs, effects |
| 6 | Rules | `references/06-rules.md` | local guardrails, local obligations, then comparison/cross-entity constraints |

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
  stable requirements/NFRs/qualities/constraints, reusable concepts/domain
  objects/aggregates/value objects/entities, ADRs, systems, locations, timings,
  media, permissions, cross-BUC lifecycle, and cross-BUC / cross-entity rules.
- BUC-local flow goes in `buc/buc_<name>.rdra`: `buc`, `flow`, `step`,
  `usecase`, `screen`, `field`, BUC-local `api`/`dto`, CRUD, `displays`,
  `invokes`, API contract links, `raises`, `triggers`, `coordinates`, access
  constraints, UC conditions, compensations, and `sets`.
- Do not put BUC-specific predicates in shared files.
- Every file starts with `module <dotted.name>`, and the dotted module should mirror
  the file path.
- Keep broad imports during exploration; narrow imports after shared files split.

### Syntax Quick Reference

#### Instance Declarations

```rdra
actor Customer "Customer"
business Commerce "Commerce"
requirement ReqOrder "Order processing must be reliable"
buc BucOrder "Process Order"
flow CheckoutFlow "Checkout Flow"
step ReviewCart "Review Cart"
usecase PlaceOrder "Place Order"
screen CheckoutScreen "Checkout"
field OrderIdField "Order ID"
api PlaceOrderApi "Place Order API" method POST path "/orders"
dto PlaceOrderRequest "Place Order Request"
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
| `requirement` / `adr` / `nfr` / `quality` / `constraint` | traceability, decisions, and non-functional intent |
| `business` / `buc` / `flow` / `step` / `usecase` | business value, flow order, and action boundaries |
| `screen` / `field` / `api` / `dto` / `system` | UI items, payloads, interaction, and implementation boundaries |
| `concept` / `domain_object` / `aggregate` / `valueobject` / `entity` | conceptual and logical data modeling |
| `state` / `event` | lifecycle and causality |
| `location` / `timing` / `medium` / `permission` | context and access vocabulary |

#### Predicate Signatures

| Predicate | Signature | Meaning |
|---|---|---|
| `performs` | `(Actor, UseCase\|Buc)` | actor performs a use case or BUC |
| `contains` | `Buc == UseCase` / `Buc == Flow` / `Flow == Step` / `System == Api` / `Screen == Field` | composition |
| `precedes/branches/excepts/repeats` | `Step == Step` | business flow order, alternatives, exceptions, loops |
| `covers` | `(Step, UseCase\|Api\|Event)` | business step covers model behavior |
| `belongs` | `Buc == Business` | BUC ownership, optionally chained with `.when/.where/.by` |
| `uses` | `Actor == ExtSystem` | actor uses an external system |
| `reads/writes/creates/updates/deletes` | `(UseCase\|Api, Entity)` | data touchpoint or API operation |
| `displays` | `UseCase == Screen` | UI path |
| `shows` | `Screen == Entity` | screen data exposure |
| `invokes` | `UseCase == Api` | UC delegates to an API boundary |
| `maps_field` | `(Field, Entity, "column")` | screen field to logical data mapping |
| `request/response/error_response` | `Api == Dto` | API payload contract |
| `coordinates` | `(UseCase, Entity, Entity)` | UC coordinates cross-system consistency |
| `compensates` | `(UseCase, UseCase\|Event)` | compensation behavior |
| `owns` | `System == Entity` | explicit intended ownership before CRUD is complete |
| `raises` | `UseCase == Event` | UC emits a domain event |
| `triggers` | `(Event, UseCase\|Buc)` | event starts a UC or a BUC boundary |
| `transitions` | `(Entity.col, Event, from -> to)` | event moves enum column from -> to |
| `sets` | `(UseCase\|Event, Entity, col == val)` | explicit column effect |
| `sets` | `(UseCase\|Event, Entity, col op rhs, true\|false)` | comparison proposition effect |
| `forbidden` | `(Entity, col == val\|col op rhs, ...)` | forbidden reachable state combination |
| `invariant` | `(Entity).when(...).then(...)` | required co-occurrence inside one entity |
| `required` | `(Entity, col == val\|col op rhs, ...)` | always-required state facts |
| `exclusive` | `(Entity, col == val\|col op rhs, ...)` | mutually exclusive state facts |
| `forbidden` (multi-entity) | `(Entity..., Entity.col == val\|Entity.col op rhs, ...)[.along(...)]` | forbidden combination across entities |
| `invariant` (multi-entity) | `(Entity...).when(...).then(...)[.along(...)]` | required co-occurrence across entities |
| `when` | `(...).none/has(...)` | to-many quantifier (prefer `Entity.col`) |
| `property` | `Name [label] always\|eventually\|leads_to(...)` | temporal path property (label optional) |
| `after` | `(UseCase).assert(...)` | postcondition on events raised by UC |
| `relate` | `(Entity, Entity, 1:1\|1:N\|N:1\|N:M)` | ER relation, auto-generates FK |
| `has_permission` | `Actor == Permission` | actor-side grant |
| `requires_permission` | `(UseCase\|Api, Permission)` | UC/API required authority |
| `requires_medium` | `(UseCase\|Api, Medium)` | UC/API required operation medium |
| `applies_to/qualifies/constrains` | `(Nfr\|Quality\|Constraint, target)` | non-functional and quality scope |
| `maps_to` | `(Concept\|DomainObject\|Aggregate\|ValueObject, Entity)` | conceptual-to-logical mapping |
| `motivates` / `decides` | `Requirement == Buc` / `Adr == target` | requirement and decision traceability |

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
- Treat system ownership as derived from `contains(System, Api)` plus API CRUD by
  default. Add `owns(System, Entity)` only to record intentional ownership before
  CRUD APIs are complete, and review any difference from derived ownership.
- Model screen constraints indirectly through `displays`, `invokes`,
  `requires_permission`, and `requires_medium`.
- Use `field` + `maps_field` when screen input/output mapping matters; do not
  overload `entity` columns as UI fields.
- Keep conceptual vocabulary separate from persistence. Use
  `concept`/`domain_object`/`aggregate`/`valueobject` for business terms, `entity`
  for logical data structures, and `maps_to` when the mapping is intentional.
- Put API method/path/idempotency/mode/auth metadata on `api`, and use `dto`
  declarations plus `request`/`response`/`error_response` relations when OpenAPI or
  payload review is needed.
- Use `nfr`/`quality`/`constraint` plus `applies_to`, `qualifies`, and `constrains`
  for performance, availability, SLO, audit/logging, retention, privacy, and other
  non-functional requirements.
- Link ADRs with `decides` only when a real design decision affects a BUC, use case,
  API, system, entity, requirement, NFR, or conceptual model element.
- Use `rdra-ish csv src/ --kind permission-callables` to review which operations each
  permission enables; use `has_permission` for actor-side assignment and verify it with
  `rdra-ish csv src/ --kind actor-permission-audit`.
- Use `rdra-ish csv src/ --kind business-inputs` after entity columns are modeled to review
  which non-derived fields each actor is expected to input through performed use cases
  and invoked APIs.
- Prefer `triggers(Event, Buc)` when an event starts a downstream BUC. Add
  `triggers(Event, UseCase)` later when the concrete entry action is known.
- Use `sets` to make lifecycle effects explicit for `Enum`, `Bool`, nullable columns,
  and comparison propositions.
- Add local rules in this order: `forbidden` / `exclusive` first, `invariant` /
  narrow `required` next, comparison and cross-entity constraints last.
- Use multi-entity `forbidden` / `invariant` when a rule mentions columns from more
  than one entity; qualify multi-entity columns as `Entity.column`.
- Add `.along(EntityA, EntityB, ...)` only when the rule is about instances linked by
  a declared `relate` path. Current `states` reports these relation-scoped rules as
  `CrossConstraintNotEvaluated` instead of evaluating the global cross-product; TLC
  evaluates them via `*_owner` when exporting TLA+.
- Use `when(...).none/has(...)` for quantifiers; use `property` / `after.assert` for
  temporal and postcondition checks; validate Int/`now`/temporal with
  `export --kind tla`.

### Common Mistakes

- Swapping predicate argument order: CRUD predicates take `(UseCase|Api, Entity)`.
- Quoting `relate` cardinality when unquoted forms like `N:1` are preferred.
- Adding quotes inside `Enum(...)` values.
- Forgetting `module`, or using a module name that does not match the file path.
- Adding FK columns manually when `relate` already generates them.
- Declaring `api` without `invokes(UseCase, Api)`.
- Declaring `system` without `contains(System, Api)`.
- Adding `owns(System, Entity)` that contradicts API CRUD ownership without
  reviewing the warning.
- Adding a cross-system `relate` without `coordinates(UseCase, Entity, Entity)`.
- Using `entity` for every business noun when a `concept` or `domain_object` would
  keep the model intentionally conceptual.
- Adding `field` without `contains(Screen, Field)` or mapping actor-entered values
  when the entity column exists.
- Adding API method/path metadata without DTO request/response links when payload
  review or OpenAPI export is expected.
- Forgetting `rdra-ish lint` and `rdra-ish fmt --check` before final handoff.
- Checking only `screen-constraints` for access review; also run
  `permission-callables` and `actor-permission-audit`.
- Modeling event-started BUCs only as `triggers(Event, UseCase)` when the BUC
  boundary itself should remain swappable between human and event initiation.
- Writing bare column names in multi-entity cross constraints; use `Entity.column`.
- Using `.along(...)` as an implementation shortcut for a global rule; it marks a
  relation-scoped rule whose linked-instance reachability is not evaluated yet.
