---
name: rdra-ish-write
description: Write RDRA DSL files from requirements using correct syntax and file structure, including staged abstract-to-concrete refinement, business flows, requirements/NFRs, screen fields, API contracts, data modeling annotations, ADR links, lint/format checks, and exports
license: MIT
---

## Write RDRA DSL

Create RDRA DSL files from requirements or specifications.

### Context Loading Rule

This file is the routing layer. Load only the step file for the model's current
abstraction level. If the level is unclear, read the previous, current, and next step
files, classify the model, then continue with the single most relevant step.

<!-- stage-spec: skill-table -->
| Stage | Concern | Main question | Add |
|---|---|---|---|
| 0 Scope sketch | Biz intent | What business area are we modeling? | `business`, rough `buc`, `belongs`, candidate `actor`, optional `requirement` and `motivates` |
| 1 BUC skeleton | Biz value | Who gets value from each BUC? | `performs`, `contains`, rough `usecase`, `flow` / `step` when order matters |
| 2 Data touchpoints | Biz object touchpoints | Which data objects does each use case touch? | coarse `entity`, CRUD predicates (`creates`, `reads`, `updates`, `deletes`) |
| 3 Interaction boundary | Tech interaction boundary | What UI/API boundary mediates the work, and with which authority/media constraints? | `screen`, `displays`, `shows`, `field`, optional `api` / `invokes`, `dto`, `system`, `permission`, `medium`, `has_permission`, `requires_*` |
| 4 Entity structure | Tech data design | What structure and ownership does the data need? | columns, `@pk`, `variation`, `relate`, cardinality, optional `owns` / `coordinates` |
| 5 Lifecycle | Tech lifecycle design | Which states and event-triggered BUC entries are reachable through the BUCs? | `Enum`, `Bool`, `@null`, `event`, `transitions`, `raises`, `triggers`, `timer`, `sets`, `condition` |
| 6A Local guardrails | Tech-enforced rules | Which simple states or facts must never co-occur? | `forbidden`, `exclusive` over Enum / Bool / nullable axes |
| 6B Local obligations | Tech-enforced rules | Which reachable states imply or require other facts? | `invariant`, narrow `required` |
| 6C Advanced rules | Tech-enforced rules | Which comparison or cross-entity facts are reviewable from the abstract state space? | comparison propositions, multi-entity `forbidden` / `invariant` |
<!-- /stage-spec -->

The step files are `references/00-scope.md` for stage 0, `references/01-buc-skeleton.md`
for 1, `references/02-data-touchpoints.md` for 2, `references/03-interaction-boundary.md`
for 3, `references/04-entity-structure.md` for 4, `references/05-lifecycle.md` for 5, and
`references/06-rules.md` for 6A to 6C.

Do not skip ahead because a downstream detail is tempting: a guessed column or state
machine reads as a commitment to reviewers. Model the smallest useful increment, run
validation, then advance one level.

### Core Workflow

1. Locate the existing model root and current abstraction level. `rdra-ish next src/`
   prints the stage of each BUC and the gaps that block the next one;
   `rdra-ish next src/ --buc <BucId> --format json` gives the next stage's `asks` and
   `gaps` to work from.
2. Load the matching step file from `references/`.
3. Ask only for information required by that step, unless the codebase already answers it.
4. Edit the smallest set of `.rdra` files needed for the step.
5. Run the validation commands from the step file. `check` prints a `bounds:` line first
   and ends with `OK: no errors within 1 instance per entity, Int 0..5` on success.
6. Stop when the step's achievement conditions are met, or report the remaining open items.
   In the report, paste the `bounds:` line and the last line of `check` exactly as
   printed, in a code block. Do not paraphrase them as "OK" or "no errors".

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
  stable requirements/NFRs/qualities/constraints, reusable entities and variations, ADRs,
  systems, timers, media, permissions, cross-BUC lifecycle, and cross-BUC / cross-entity
  rules.
- BUC-local flow goes in `buc/buc_<name>.rdra`: `buc`, `flow`, `step`,
  `usecase`, `screen`, `field`, BUC-local `api`/`dto`, CRUD, `displays`,
  `invokes`, API contract links, `raises`, `triggers`, `coordinates`, access
  constraints, conditions applied to the BUC's use cases, compensations, and `sets`.
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
| `entity` | logical data modeling |
| `event` | the interface between use cases, or between a use case and an external system |
| `timer` | schedule that triggers a use case |
| `variation` / `condition` | values an actor chooses; the variation x state-model combination a use case applies under |
| `medium` / `permission` | context and access vocabulary |

States are not declared: they are the variants a `transitions` names. The removed
words `state`, `location`, `timing`, `uses`, `usagescene`, and the conceptual-model
kinds fail to parse, with a hint naming the replacement.

#### Predicate Signatures

| Predicate | Signature | Meaning |
|---|---|---|
| `performs` | `(Actor, UseCase\|Buc\|Step)` | actor performs a use case, every use case of a BUC, or a business step |
| `contains` | `Buc == UseCase` / `Buc == Flow` / `Flow == Step` / `System == Api` / `Screen == Field` | composition |
| `precedes/branches/excepts/repeats` | `Step == Step` | business flow order, alternatives, exceptions, loops |
| `covers` | `(Step, UseCase\|Api\|Event\|Timer)` | business step covers model behavior |
| `belongs` | `Buc == Business` | BUC ownership, optionally chained with `.by(Medium\|"text")` |
| `reads/writes/creates/updates/deletes` | `(UseCase\|Api, Entity)` | data touchpoint or API operation |
| `displays` | `UseCase == Screen` | UI path |
| `shows` | `Screen == Entity` | screen data exposure |
| `invokes` | `UseCase == Api` | UC delegates to an API boundary |
| `maps_field` | `(Field, Entity, "column")` | screen field to logical data mapping |
| `request/response/error_response` | `Api == Dto` | API payload contract |
| `coordinates` | `(UseCase, Entity, Entity)` | UC coordinates cross-system consistency |
| `compensates` | `UseCase == UseCase` | the first use case compensates the second |
| `owns` | `System == Entity` | explicit intended ownership before CRUD is complete |
| `raises` | `(UseCase\|ExtSystem, Event)` | a use case emits an event; an external system raising one is inbound |
| `triggers` | `(Event\|Timer, UseCase\|Buc\|ExtSystem)` | an event or timer starts a use case or a BUC; an event delivered to an external system is outbound (a timer never triggers one) |
| `outbox` | `(Event)` | the event is published outside the model, so it needs no local consumer |
| `transitions` | `(Entity.col, UseCase, from -> to)` | the use case moves the Enum column from -> to |
| `sets` | `(UseCase, Entity, col == val)` | explicit column effect |
| `sets` | `(UseCase, Entity, col op rhs, true\|false)` | comparison proposition effect |
| `forbidden` | `(Entity, col == val\|col op rhs, ...)` | forbidden reachable state combination |
| `invariant` | `(Entity).when(...).then(...)` | required co-occurrence inside one entity |
| `required` | `(Entity, col == val\|col op rhs, ...)` | always-required state facts |
| `exclusive` | `(Entity, col == val\|col op rhs, ...)` | mutually exclusive state facts |
| `forbidden` (multi-entity) | `(Entity..., Entity.col == val\|Entity.col op rhs, ...)[.along(...)[.by(role)]]` | forbidden combination across entities |
| `invariant` (multi-entity) | `(Entity...).when(...).then(...)[.along(...)[.by(role)]]` | required co-occurrence across entities |
| `when` | `(...).none/has(...)` | to-many quantifier (prefer `Entity.col`) |
| `property` | `Name ["label"] always\|eventually\|leads_to(...)` | temporal path property, checked by `verify` only |
| `after` | `(UseCase).assert(...)` | predicate on the state each step of the use case leaves |
| `relate` | `(Entity, Entity, 1:1\|1:N\|N:1\|N:M)` | ER relation and generated key; see below |
| `has_permission` | `Actor == Permission` | actor-side grant |
| `requires_permission` | `(UseCase\|Api, Permission)` | UC/API required authority |
| `requires_medium` | `(UseCase\|Api, Medium)` | UC/API required operation medium |
| `applies_to` | `(Nfr\|Condition, UseCase\|Api\|System)` | NFR scope; a condition's rule applies to the use case (a body guards it) |
| `qualifies` / `constrains` | `(Nfr\|Constraint, Quality)` / `(Constraint, UseCase\|Api\|System\|Entity\|Dto)` | quality category and guardrail scope |
| `combines` | `(Condition, Variation\|Entity.column)` | traceability link to a variation or a lifecycle column |
| `holds` | `(Condition, Entity.col == value, ...)` | condition body (implicit AND); links are then derived, so no `combines` |
| `motivates` / `decides` | `Requirement == Buc` / `Adr == target` | requirement and decision traceability |

`relate(Child, Parent, N:1)` generates the key column `{role}_{parent key column}` on the
child (`order_id`); `1:N` puts it on the right entity, and `1:1` on the left, marked
unique. `N:M` needs `.via(Junction)`. The child side may be a range: `1..*:1` says every
parent has a child. Chain `.optional()`, `.on_delete(..)`, `.on_update(..)`,
`.role(x)` (name the key), or `.key(cols)` (adopt declared columns).

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
  contract, not by screen button count. The lifecycle runs a use case as one atomic
  action per transaction group, so the split decides what can interleave.
- Treat system ownership as derived from `contains(System, Api)` plus API CRUD by
  default. Add `owns(System, Entity)` only to record intentional ownership before
  CRUD APIs are complete, and review any difference from derived ownership.
- A use case's actors are its direct performers plus the performers of every BUC that
  contains it. So `performs(Actor, Buc)` says the actor runs every use case of the BUC,
  and the permission check requires each of those actors to hold each use case's
  permissions. When an actor runs only some of the use cases, write
  `performs(Actor, UseCase)` for those instead.
- Model screen constraints indirectly through `displays`, `invokes`,
  `requires_permission`, and `requires_medium`.
- Use `field` + `maps_field` when screen input/output mapping matters; do not
  overload `entity` columns as UI fields.
- Model persistent structure with `entity` (and `dto` for payloads). There is no
  separate conceptual-model kind.
- Put API method/path/idempotency/mode/auth metadata on `api`, and use `dto`
  declarations plus `request`/`response`/`error_response` relations when OpenAPI or
  payload review is needed.
- Use `nfr`/`quality`/`constraint` plus `applies_to`, `qualifies`, and `constrains`
  for performance, availability, SLO, audit/logging, retention, privacy, and other
  non-functional requirements.
- Link ADRs with `decides` only when a real design decision affects a BUC, use case,
  API, system, entity, requirement, NFR, or constraint.
- Review access with `rdra-ish csv src/ --kind permission-callables` (what each
  permission enables) and `rdra-ish csv src/ --kind actor-permission-audit` (whether
  each actor holds what its use cases need). `rdra-ish check src/ --strict` makes a
  permission gap an error.
- Prefer `triggers(Event, Buc)` when an event starts a downstream BUC. Add
  `triggers(Event, UseCase)` later when the concrete entry action is known.
- A use case drives each state change: `transitions(Order.status, CapturePay, pending ->
  paid)`. An event only connects use cases (or a use case and an external system), so
  declare one only when something consumes it.
- Type a column by a `variation` (`method: PaymentMethod`) when an actor chooses its
  value; keep an inline `Enum(...)` for lifecycle columns and values that use cases set.
- A `condition` is an RDRA 条件. Without a body it is a trace link; with a `holds` body
  it guards each use case it `applies_to` (Step 5).
- `check` evaluates every rule, quantifier, and `after(UC).assert` at one instance per
  entity. A temporal `property`, and a rule that needs two instances, need `verify`
  (skill `rdra-ish-verify`).

### Common Mistakes

- Swapping predicate argument order: CRUD predicates take `(UseCase|Api, Entity)`.
- Naming an event in `transitions` or as the origin of `sets`: both take a use case,
  and the event form is a `TypeMismatch`.
- Declaring `state`, `timing`, `location`, or `uses`: they were removed. States come
  from `transitions`, a schedule is a `timer`, and an external system talks to use cases
  through `raises` and `triggers`.
- Adding quotes inside `Enum(...)` values or around `sets` values (`flag == true`,
  `at == present`, not `"true"`).
- Quoting the `@check(...)` or `@derived(...)` expression: write `@check(qty >= 1)`.
- Writing an inline `Enum(...)` for a value the actor picks (payment method, channel):
  `states` then reports its unchosen variants unreachable. Declare a `variation`.
- Forgetting `module`, or using a module name that does not match the file path.
- Adding FK columns manually when `relate` already generates them (`FkConflict`, unless
  you adopt the column with `.key(...)`).
- Writing `performs(Actor, Buc)` for an actor who runs only some of the BUC's use cases.
- Declaring `api` without `invokes(UseCase, Api)`, or `system` without
  `contains(System, Api)`.
- Adding `owns(System, Entity)` that contradicts API CRUD ownership without
  reviewing the warning.
- Adding a cross-system `relate` without `coordinates(UseCase, Entity, Entity)`.
- Using `entity` for every business noun, including nouns that are never
  persisted or exchanged.
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
