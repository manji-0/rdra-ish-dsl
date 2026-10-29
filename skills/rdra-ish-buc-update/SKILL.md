---
name: rdra-ish-buc-update
description: Update an existing BUC by adding or modifying use cases, business flows, requirements, screens, fields, API contracts, NFRs, events, entities, ADR links, and predicates while preserving staged refinement
license: MIT
---

## Update an existing BUC

Given a description of what to add or change, modify the relevant `.rdra` files while
keeping the model consistent.

Preserve the model's current abstraction level. If the BUC is still a skeleton, do not
force columns, APIs, or lifecycle rules: ask for the next missing information and apply
the smallest stage-appropriate diff. Keep the business-to-technical refinement order
intact, so that every technical detail is justified by the business value, actors, use
cases, and data touchpoints above it. The stage files of skill `rdra-ish-write`
(`../rdra-ish-write/references/00-scope.md` to
`../rdra-ish-write/references/06-rules.md`) hold the syntax for each stage.

### Step 1 — Read the existing BUC

Read `buc/buc_<name>.rdra` and the shared files it imports. Identify:

- Which use cases already exist, and who performs each (directly, or through
  `performs(Actor, Buc)`)
- Which entities are already referenced
- What screens and events are already declared
- Which flow/step, field, DTO, requirement/NFR, and ADR elements already exist
- Which predicates are already defined
- Whether the model still uses the small layout (`shared/actors.rdra`,
  `shared/biz.rdra`, `shared/entities.rdra`) or has split shared files

Run `rdra-ish check src/` before editing, so you know which findings the change
introduces.

### Step 2 — Classify the change

| Change type | Where to edit |
|-------------|---------------|
| New use case in existing BUC | `buc/buc_<name>.rdra` |
| New business flow order, branch, exception, or loop | `buc/buc_<name>.rdra` with `flow`, `step`, `precedes`, `branches`, `excepts`, `repeats`, `covers` |
| New requirement metadata | stable shared requirement file, then `motivates(Requirement, Buc)` |
| New screen for an existing UC | `buc/buc_<name>.rdra` |
| New screen field or input/output mapping | owning BUC/screen file; add `field`, `contains(Screen, Field)`, `maps_field` |
| New API method/path or payload | owning API file; add API metadata, `dto`, `request`, `response`, `error_response` |
| New NFR/quality/constraint | stable shared file; scope with `applies_to`, `qualifies`, or `constrains` |
| New ADR impact | stable shared ADR file; connect with `decides(Adr, Target)` |
| New entity column | `shared/entities.rdra` |
| New entity entirely | `shared/entities.rdra` + add `relate` if needed |
| New value an actor chooses | shared `variation`, then type the column by it |
| New state or state edge | the Enum variant on the entity, then `transitions(Entity.col, UseCase, from -> to)` next to the entity (cross-BUC) or in the BUC file; states are never declared |
| New event | `shared/entities.rdra` (if cross-BUC) or `buc/buc_<name>.rdra`, with its `raises` and its consumer |
| New actor | `shared/actors.rdra` |
| New system/API ownership | shared vocabulary for `system`, BUC/shared API file for `api`, then `contains(System, Api)` |
| New Business-BUC context | shared vocabulary for `medium`, then `belongs(...).by(...)` |
| New schedule | shared `timer`, then `triggers(Timer, UseCase)` in the owning BUC file |
| New business rule that selects when a use case applies | `condition` with `combines` (trace link) or a `holds` body (guard), and `applies_to(Condition, UseCase)` |
| New external system traffic | shared `extsystem`, then `raises(ExtSystem, Event)` or `triggers(Event, ExtSystem)` |
| New permission or medium constraint | shared `permission` / `medium` vocabulary, then `has_permission`, `requires_permission`, or `requires_medium` in the owning BUC file |
| New event-started BUC | target BUC file, then `triggers(Event, Buc)`; add `triggers(Event, EntryUC)` only after the entry UC is known |
| New entity rule | shared rules file or shared entity area; use `forbidden`, `invariant`, `required`, or `exclusive` for single-entity state facts; use multi-entity `forbidden` / `invariant` with `Entity.column` conditions for multi-entity rules, and add `.along(...)` only for relation-scoped linked-instance intent; use `when(...).none/has`, `property`, and `after.assert` when quantifiers or temporal/postcondition checks matter |
| Cross-system entity relation handling | BUC file that owns the coordinating use case |
| Remove a use case | Remove its `contains`, `performs`, CRUD, `displays`, `raises`, `transitions`, and `sets`; check no other BUC uses it |

If the model still uses the small layout, keep using it unless the change makes a
shared file hard to review. If shared files are already split, place new shared
definitions near their owning area and mirror path/module names.

Also classify the abstraction transition:

| Current state | Concern | Next useful update | Ask the user for |
|---------------|---------|--------------------|------------------|
| BUC exists but no actors | Biz intent/value | actor coverage | who performs or receives value from each use case |
| Actors/use cases exist but no CRUD | Biz object touchpoints | data touchpoints | entities and CRUD intent per use case |
| CRUD exists but no screens/API | Tech interaction boundary | interaction boundary | screens, external interfaces, API endpoints, owning systems, access/media constraints |
| Entities have only `id` | Tech data design | entity structure | fields, keys, relationships, cardinality, chosen values |
| Structured entities have lifecycle fields | Tech lifecycle design | lifecycle | which use case moves which state, events, use-case effects |
| `check` passes on the lifecycle | Tech-enforced rules | local guardrails | invalid or mutually exclusive state facts via `forbidden` / `exclusive` |
| Local guardrails are stable | Tech-enforced rules | obligations and advanced rules | `invariant`, narrow `required`, comparison propositions, or cross-entity rules when needed |

### Step 3 — Apply the minimal diff

Add only what is needed. Do not remove or rename existing predicates unless the
requirement explicitly says to delete functionality. The examples extend a BUC
`BucOrder` with an `Order` entity, so they name ids the model already declares.

**Adding business flow order:**

```rdra
flow OrderFlow "Order flow"
step ReviewCart "Review cart"
step SubmitOrder "Submit order"

contains(BucOrder, OrderFlow)
contains(OrderFlow, ReviewCart)
contains(OrderFlow, SubmitOrder)
precedes(ReviewCart, SubmitOrder)
covers(SubmitOrder, PlaceOrder)
```

Use `branches`, `excepts`, and `repeats` only when the requirement explicitly
introduces an alternative, exception, or loop.

**Adding a use case:**

```rdra
usecase ChangeAddress "Change Delivery Address"

contains(BucOrder, ChangeAddress)
performs(Customer, ChangeAddress)
updates(ChangeAddress, Order)
displays(ChangeAddress, OrderScreen)
```

If the BUC has `performs(Actor, Buc)`, that actor now performs the new use case too and
needs its permissions. When that is wrong, move the actor to direct `performs` on the
use cases it runs.

**Adding a column to an existing entity:** append it inside the entity block in
`shared/entities.rdra` (`delivery_note: String @null`), then add a `sets` in the BUC
file for every use case that writes it.

**Adding a new entity with a relationship:**

```rdra
entity Shipment "Shipment" {
  id: Int @pk
  status: Enum(packing, shipped) @default(packing)
}
relate(Shipment, Order, N:1)
```

`relate(Child, Parent, N:1)` generates `order_id` on Shipment; do not declare it. Then
add CRUD predicates in the BUC file for the use cases that touch it.

**Adding a state edge:**

```rdra
transitions(Shipment.status, ShipOrder, packing -> shipped)
```

The use case that moves the state is the second argument. A new state is a new Enum
variant; it needs an arm into it, or `check` reports `Witness_Shipment_status_<value>`.

**Adding or changing API/system boundaries:**

```rdra
system OrderSystem "Order System"
api ChangeAddressApi "Change Address API" method PATCH path "/orders/{id}" idempotency idempotent mode sync auth bearer
dto ChangeAddressRequest "Change Address Request" {
  address: String
}

contains(OrderSystem, ChangeAddressApi)
invokes(ChangeAddress, ChangeAddressApi)
request(ChangeAddressApi, ChangeAddressRequest)
updates(ChangeAddressApi, Order)
```

System entity sets are derived from API CRUD. Add `owns(System, Entity)` only when
intentional ownership must be visible before complete API operations exist, and
review the warning if explicit ownership disagrees with derived ownership.

**Adding a screen field:**

```rdra
field AddressField "Delivery address"
  access editable
  required true
  source actor

contains(OrderScreen, AddressField)
maps_field(AddressField, Order, "address")
```

**Adding Business-BUC context:**

```rdra
medium StoreTerminal "Store Terminal"

belongs(BucOrder, Commerce)
  .by(StoreTerminal)
```

**Adding access constraints:**

```rdra
permission OrderWrite "Order Write"

has_permission(Customer, OrderWrite)
requires_permission(ChangeAddress, OrderWrite)
requires_medium(ChangeAddress, StoreTerminal)
requires_permission(ChangeAddressApi, OrderWrite)
```

Use `requires_*` on the use case for constraints that apply to the whole interaction,
and on the API for constraints specific to that backend boundary. Every actor of the
use case needs the permission: its direct performers and the performers of each BUC
that contains it. Screen constraint patterns are derived through `displays` and
`invokes`; do not hand-write a separate screen predicate for them. Check the grants with
`rdra-ish csv src/ --kind actor-permission-audit` and the mapping with
`rdra-ish csv src/ --kind permission-callables`; `rdra-ish check src/ --strict` turns a
gap into an error.

**Adding non-functional or decision traceability:**

```rdra
quality Performance "Performance"
nfr ChangeAddressLatency "Change address latency"
  metric p95_latency_ms
  target "<=300"
  slo "99.9%"

qualifies(ChangeAddressLatency, Performance)
applies_to(ChangeAddressLatency, ChangeAddressApi)

adr AdrAddressApi "Change addresses through the order API"
decides(AdrAddressApi, ChangeAddressApi)
```

**Starting a BUC from an event:**

```rdra
event OrderPlaced "Order Placed"
raises(PlaceOrder, OrderPlaced)
triggers(OrderPlaced, BucFulfillment)
```

If the entry action is already known, also add `contains(BucFulfillment, PickOrder)`
and `triggers(OrderPlaced, PickOrder)`.

**Handling a cross-system relation:**

```rdra
coordinates(ChangeCustomer, Order, CustomerAccount)
invokes(ChangeCustomer, OrderApi)
invokes(ChangeCustomer, CustomerApi)
```

Use this only when the two entities are related and belong to different derived system
boundaries.

**Adding cross-entity rules:**

```rdra
forbidden(Order, Shipment,
  Order.status == cancelled,
  Shipment.status == shipped)
  .along(Order, Shipment)
```

Multi-entity rules qualify every column as `Entity.column`. Use `.along(...)` only when
the rule is about instances linked by a declared `relate` path; without a `relate` that
binds each adjacent pair the rule is `UnbindableScope`. Without `.along`, the rule holds
for every pair of instances. `check` tests it at one instance per entity; run
`rdra-ish verify src/` to test it at two.

**Removing a use case:**

1. Delete the `usecase` declaration
2. Delete `contains(Buc, <UC>)` and every `performs` that names it
3. Delete all CRUD, `displays`, `raises`, `transitions`, and `sets` predicates referencing it
4. If the UC was also referenced by `triggers`, `applies_to`, `after`, or
   `compensates` elsewhere, update those too
5. Run `check`: a state that only the removed use case entered, and that another
   `transitions` arm still leaves, now fails as `Witness_*`

### Step 4 — Consistency checks after the change

- Every new `usecase` has a `contains` predicate and a performer (direct or BUC-level)
  or is triggered by an event or timer
- Every new `flow` belongs to a BUC, every `step` belongs to a flow, and covered
  behavior exists
- Every new `usecase` has at least one CRUD predicate
- Every new `field` belongs to a screen; actor-entered fields are mapped or have an
  explicit reason to remain external
- API method/path metadata has request/response DTOs when contract review is in scope
- Every new `entity` column that can change has a `sets` or participates in `transitions`
- No dangling imports (if you removed the last use of a symbol, remove its `import`)
- No manually added FK columns for a `relate`-covered relationship
- Every API used as a system boundary has `contains(System, Api)`
- Every explicit `owns(System, Entity)` is either satisfied by API CRUD or accepted
  as future ownership with diagnostics reviewed
- Every new `permission` used by an actor or operation is declared once in shared
  vocabulary unless intentionally local, and each actor of a constrained use case holds it
- Every `requires_medium` references a declared `medium`, and screen constraints are
  checked with `rdra-ish csv src/ --kind screen-constraints`
- Every cross-system `relate` has a `coordinates(UseCase, Entity, Entity)` when a use
  case handles the consistency, and that use case invokes APIs on both system sides
- Every cross-entity rule qualifies columns as `Entity.column`, and every `.along(...)`
  names a declared `relate` path
- The diff does not jump more than one abstraction level unless the user supplied the
  missing information explicitly
- New shared files follow path/module correspondence, e.g.
  `shared/lifecycle/order.rdra` with `module shared.lifecycle.order`

### Step 5 — Validate

```sh
rdra-ish check src/
rdra-ish lint src/ --format table
```

Fix every reported error before declaring the update done, and compare the findings
with the ones from Step 1: a new `error[...]` or `warning[...]` is the change's.
