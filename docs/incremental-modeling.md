# Incremental Modeling Flow

This guide describes how to build an RDRA model from coarse intent to concrete
behavior without forcing all details up front. Each stage has a small modeling goal,
a validation command, and a focused set of questions that unlocks the next stage.

<!-- constrained-by ./language-reference.md -->
<!-- constrained-by ./language-reference.md#access-constraints -->
<!-- constrained-by ./language-reference.md#belongs-context -->
<!-- constrained-by ./cli-reference.md -->
<!-- constrained-by ./rdra-ish-interpretation.md#basic-stance -->
<!-- derived-from ../README.md#recommended-modeling-loop -->

## Principle

The stages can also be read as a shift of concern from **business intent** to
**technical design**. Early stages keep the model close to business language: value
streams, actors, and user-visible work. Later stages add technical commitments:
data touchpoints, UI/API boundaries, ownership, access/media constraints, persistence
structure, lifecycle, and rules that the implementation must preserve.

Start with the smallest model that can answer the current question. Move to the next
stage only when the current abstraction is stable enough to make the added detail
useful.

The terms BUC, business flow, and UC are used in the RDRA-ish sense described in
[RDRA-ish Interpretation](./rdra-ish-interpretation.md): BUCs are business-value
review containers, business flow is the concrete realization of a BUC through UCs and
events, and UCs are effect-bearing interactions. The flow is reviewed through prose
plus generated sequence, event-flow, CRUD, ER, and state views.

The model can stay intentionally incomplete while it is being explored:

- warnings are review signals, not always blockers;
- `check` errors are blockers because the semantic model cannot be trusted;
- BUC filters let you validate one slice without finishing the whole system;
- Mermaid output is the preferred early review format because it is text and easy to
  diff.

## Model Directory Layout

<!-- constrained-by ./language-reference.md#modules-and-imports -->

Use a layout that keeps stable shared vocabulary separate from BUC-local flow. The
default layout is intentionally small:

```text
src/
  shared/
    actors.rdra      # module shared.actors
    biz.rdra         # module shared.biz
    entities.rdra    # module shared.entities
  buc/
    buc_<name>.rdra  # module buc.<name>
```

Keep this layout until a file becomes hard to review. Split only when the split makes
ownership clearer.

### Placement Rules

| Artifact | Default location | Split when |
|---|---|---|
| `actor`, `extsystem` | `shared/actors.rdra` | many external systems justify `shared/external.rdra` |
| `business`, stable `requirement`, reusable `location` / `timing` / `medium` / `permission` | `shared/biz.rdra` or another shared vocabulary file | context and authority vocabulary grows large enough to deserve its own file |
| `entity`, `relate`, shared lifecycle states/events | `shared/entities.rdra` | entities naturally form bounded groups |
| `buc`, `usecase`, `screen`, BUC-local `api`, BUC-local events | `buc/buc_<name>.rdra` | keep one BUC per file; do not split a single BUC early |
| CRUD, `displays`, `invokes`, access constraints, `raises`, `sets` | the BUC file that explains the use case | never put BUC-specific predicates in shared files |
| cross-BUC `event`, `state`, `transitions` | shared file near the owning entity | multiple BUC files raise or trigger the same event |
| entity constraint predicates (`forbidden`, `invariant`, `required`, `exclusive`) | shared file near the constrained entity | constraints become numerous enough for `shared/rules.rdra` |

### Growth Pattern

When the model grows, prefer this progression:

```text
src/
  shared/
    actors.rdra
    biz.rdra
    entities.rdra
  buc/
    buc_order.rdra
    buc_payment.rdra
```

Then split shared files by responsibility:

```text
src/
  shared/
    actors.rdra
    biz.rdra
    entities/
      order.rdra        # module shared.entities.order
      payment.rdra      # module shared.entities.payment
    lifecycle/
      order.rdra        # module shared.lifecycle.order
    rules.rdra          # module shared.rules
  buc/
    buc_order.rdra      # module buc.order
    buc_payment.rdra    # module buc.payment
```

For imports, start broad during exploration and narrow later:

```rdra
import shared.actors
import shared.biz
import shared.entities
```

After splitting, import the smallest stable module that keeps the BUC readable:

```rdra
import shared.actors
import shared.biz
import shared.entities.order
import shared.lifecycle.order
```

### Naming Rules

- The file path should mirror the module path: `shared/entities/order.rdra` uses
  `module shared.entities.order`.
- BUC file names should stay `buc_<name>.rdra`; the module should be `buc.<name>`.
- Prefer stable business names over UI or implementation names for BUC and entity ids.
- Do not declare the same shared concept in two files; import it where needed.
- Do not manually add FK columns that `relate` will generate.

## Stage Map

<!-- derived-from ./language-reference.md#entity-state-constraints -->

| Stage | Concern | Abstraction | Main question | Add | Validate |
|---|---|---|---|---|---|
| 0 | Biz intent | Scope sketch | What business area are we modeling? | `business`, rough `buc`, candidate `actor` | `list --kind buc`, `diagram --kind rdra` |
| 1 | Biz value | BUC skeleton | Who gets value from each BUC? | `performs`, `belongs`, `contains`, rough `usecase` | `check`, BUC-scoped RDRA diagram |
| 2 | Biz object touchpoints | Data touchpoints | Which data objects does each use case touch? | coarse `entity`, CRUD predicates | CRUD matrix, ER diagram |
| 3 | Tech interaction boundary | Interaction boundary | What UI/API boundary mediates the work, and with which authority/media constraints? | `screen`, `displays`, `shows`, optional `api`/`invokes`, `permission`, `medium`, `has_permission`, `requires_*` | sequence diagram, screen-constraints CSV, actor-permission audit |
| 4 | Tech data design | Entity structure | What structure and ownership does the data need? | columns, `@pk`, `relate`, cardinality | ER diagram, sequence TX warnings |
| 5 | Tech lifecycle design | Lifecycle | Which states and event-triggered BUC entries are reachable through the BUCs? | `Enum`, `Bool`, `@null`, `event`, `state`, `transitions`, `raises`, `triggers`, `sets` | `check`, `states`, state diagram, event-flow |
| 6A | Tech-enforced rules | Local guardrails | Which simple states or facts must never co-occur? | `forbidden`, `exclusive` over Enum / Bool / nullable axes | `states` diagnostics |
| 6B | Tech-enforced rules | Local obligations | Which reachable states imply or require other facts? | `invariant`, narrow `required` | `states` diagnostics |
| 6C | Tech-enforced rules | Advanced rules | Which comparison or cross-entity facts are reviewable from the abstract state space? | comparison propositions, multi-entity `forbidden` / `invariant` | `states` diagnostics |

## Stage 0: Scope Sketch

Create only enough to name the business area and candidate BUCs.

```rdra
module shared.biz

business Commerce "Commerce"
```

```rdra
module buc.order

import shared.biz

buc BucOrder "Process Order"
belongs(BucOrder, Commerce)
```

Ask the user:

- Which business area or value stream is in scope?
- What are the candidate BUC names?
- Which BUC should be modeled first?

Validation:

```sh
rdra-ish check src/
rdra-ish list src/ --kind buc --format table
rdra-ish diagram src/ --kind rdra --format mermaid --buc BucOrder
```

Move on when BUC names and business ownership look stable enough for review.

## Stage 1: BUC Skeleton

Add actors and use cases, but do not require entities or screens yet.

```rdra
actor Customer "Customer"

usecase PlaceOrder "Place Order"
usecase CancelOrder "Cancel Order"

performs(Customer, BucOrder)
contains(BucOrder, PlaceOrder)
contains(BucOrder, CancelOrder)
```

Ask the user:

- Who initiates or receives value from this BUC?
- What user-visible actions compose the BUC?
- Are any use cases triggered by a system event rather than a human actor?

Validation:

```sh
rdra-ish check src/
rdra-ish diagram src/ --kind rdra --format mermaid --buc BucOrder
```

Move on when every important action is represented as a use case and each use case
belongs to the intended BUC.

## Stage 2: Data Touchpoints

Add coarse entities and CRUD predicates. At this stage, entities may have only `id`
columns; detailed attributes can wait.

```rdra
entity Order "Order" {
  id: Int @pk
}

entity Cart "Cart" {
  id: Int @pk
}

creates(PlaceOrder, Order)
updates(PlaceOrder, Cart)
updates(CancelOrder, Order)
```

Ask the user:

- Which business objects are created, read, updated, or deleted by each use case?
- Are any entities only conceptual at this stage?
- Which entities should be shared across multiple BUCs?

Validation:

```sh
rdra-ish check src/
rdra-ish csv src/ --kind matrix
rdra-ish diagram src/ --kind er --format mermaid --buc BucOrder
```

Move on when the CRUD matrix tells a plausible story, even if entity columns are still
coarse.

## Stage 3: Interaction Boundary

Add screens, optional APIs, and access/media constraints once the use-case/data
relationship is clear.

```rdra
screen CheckoutScreen "Checkout"
api OrderApi "Order API"
permission OrderWrite "Order Write"
medium CustomerDevice "Customer Device"

displays(PlaceOrder, CheckoutScreen)
shows(CheckoutScreen, Order)
invokes(PlaceOrder, OrderApi)
creates(OrderApi, Order)
requires_permission(PlaceOrder, OrderWrite)
requires_medium(PlaceOrder, CustomerDevice)
```

Ask the user:

- Which screen or external interface does each use case expose?
- Does the use case write data directly, or through an API boundary?
- Is the API a reusable boundary or local to this BUC?
- Which actor permission and physical/device medium are required for the use case or API?

Validation:

```sh
rdra-ish check src/
rdra-ish diagram src/ --kind sequence --format mermaid --buc BucOrder
rdra-ish list src/ --kind api --format table
rdra-ish csv src/ --kind api-matrix
rdra-ish csv src/ --kind screen-constraints
rdra-ish csv src/ --kind actor-permission-audit
```

Move on when sequence output communicates the intended actor/screen/API/entity path
and the screen-constraints plus actor-permission audit CSVs show the expected
permission/media paths and actor-side assignments.

### BUC Context and Access Rules

Use `belongs(Buc, Business).when(...).where(...).by(...)` when the BUC belongs to a
business area only in a specific timing, place, channel, or operating medium.

```rdra
timing OrderSubmitted "Order Submitted"
location CustomerPortal "Customer Portal"
medium CustomerDevice "Customer Device"

belongs(BucOrder, Commerce)
  .when(OrderSubmitted)
  .where(CustomerPortal)
  .by(CustomerDevice)
```

Use `has_permission(Actor, Permission)` for what an actor can do. Use
`requires_permission` and `requires_medium` on UC/API nodes for what an operation
requires. Screen requirements are not written directly; they are derived from
`displays(UC, Screen)` and the constraints on the UC plus APIs reached by
`invokes(UC, Api)`. `rdra-ish check` warns when a required permission has no modeled
actor path, when an actor on that path does not hold the permission, or when an actor
has a permission that no modeled path currently requires. Use
`rdra-ish csv src/ --kind actor-permission-audit` when you want the inferred assignment
matrix instead of warning text.

### API Boundary Rules

Use `api` as the boundary that groups data reads and writes requiring the same
transaction or reference-consistency guarantee. It is not automatically one API per
use case, one API per screen action, or one API per entity.

Prefer direct use-case CRUD when the operation is closed inside one entity and does not
need a reusable consistency boundary. Introduce a separate API when the operation must
validate or update multiple records together, preserve a cross-entity reference, or
share the same consistency contract across multiple use cases.

For example, changing a convenience store's next restock date is an update closed inside
the store entity:

```rdra
usecase ChangeNextRestockDate "Change Next Restock Date"
updates(ChangeNextRestockDate, Store)
```

Changing the store's parent organization is a different boundary because it must keep
the store, organization reference, and any assignment history consistent in one
operation:

```rdra
usecase ChangeStoreParentOrg "Change Store Parent Organization"
api StoreParentOrgApi "Store Parent Organization API"

invokes(ChangeStoreParentOrg, StoreParentOrgApi)
reads(StoreParentOrgApi, Organization)
updates(StoreParentOrgApi, Store)
creates(StoreParentOrgApi, StoreOrgAssignmentHistory)
```

When multiple use cases require that same parent-organization consistency, let them
invoke `StoreParentOrgApi` rather than duplicating the CRUD predicates on each use
case. If another store operation updates only ordinary store attributes, keep it direct
or give it a separate API only when it has its own transaction or reference contract.

## Stage 4: Entity Structure

Refine entities with columns, primary keys, relationships, and cardinality.

```rdra
entity OrderLine "Order Line" {
  id: Int @pk
  qty: Int
  unit_price: Decimal
}

relate(OrderLine, Order, N:1)
```

Ask the user:

- What fields identify each entity?
- Which fields are business state, and which are ordinary data?
- What parent/child relationships should own or group records?
- Does a use case need one transaction across related entities?

Validation:

```sh
rdra-ish check src/
rdra-ish diagram src/ --kind er --format mermaid
rdra-ish diagram src/ --kind sequence --format mermaid --buc BucOrder
```

Move on when FK relationships and transaction warnings match the intended persistence
boundary.

## Stage 5: Lifecycle

Add lifecycle axes and events after the structural model is stable.

<!-- constrained-by ./state-derivation.md#state-axes -->
<!-- constrained-by ./state-derivation.md#operations -->

```rdra
entity Order "Order" {
  id: Int @pk
  status: Enum(pending, paid, cancelled) @default(pending)
  paid_at: DateTime @null
}

event Capture "Capture Payment"
event Cancel "Cancel Order"

transitions(Order.status, event::Capture, pending -> paid)
transitions(Order.status, event::Cancel, pending -> cancelled)

raises(PlaceOrder, event::Capture)
raises(CancelOrder, event::Cancel)
sets(event::Capture, Order, paid_at == present)
```

Ask the user:

- Which Enum column and variants represent lifecycle state?
- Which use case or event causes each state change?
- Does any event start another BUC, and is the concrete entry use case known yet?
- Which nullable or boolean fields change together with the lifecycle?
- Do you need optional `state` labels for diagrams, or are Enum variants enough?

Validation:

```sh
rdra-ish states src/ --entity Order
rdra-ish diagram src/ --kind state --format mermaid --buc BucOrder
rdra-ish diagram src/ --kind event-flow --format mermaid
rdra-ish check src/
```

Move on when reachable patterns explain the expected lifecycle and unexpected terminal
or unreachable states have been reviewed. Model an event-triggered BUC first with
`triggers(Event, TargetBuc)`. When the entry action is clear, add
`contains(TargetBuc, EntryUseCase)` and optionally `triggers(Event, EntryUseCase)`.

## Stage 6A: Local Guardrails

Add low-risk constraints only after there is enough lifecycle behavior to evaluate them.
Start with rules that are easy to read as invalid local states: forbidden combinations
and mutually exclusive facts on one entity.

```rdra
forbidden(Order, status == paid, paid_at == null)

exclusive(Document, approved == true, rejected == true)
```

Ask the user:

- Which combinations must never be reachable?
- Which facts must be mutually exclusive?
- Are these facts already represented as Enum, Bool, or nullable state axes?

Validation:

```sh
rdra-ish states src/
rdra-ish states src/ --entity Order --format json
```

Move on when diagnostics either disappear or are accepted as known modeling gaps. Keep
the first guardrail pass small; it should catch obviously impossible states without
requiring a complete business rule catalogue.

## Stage 6B: Local Obligations

Add implication-style rules after local guardrails are stable. Use `invariant` for
conditional co-occurrence, and use `required` only for facts that truly must hold in
every reachable pattern.

```rdra
invariant(Order)
  .when(status == paid)
  .then(paid_at == present)

required(Account, active == true)
```

Ask the user:

- Which values must co-occur once a condition is true?
- Which facts must be true in every reachable state?
- Is the rule too broad, or should it be an `invariant` with a guard instead of
  `required`?

Validation:

```sh
rdra-ish states src/
rdra-ish states src/ --entity Order --format json
```

Move on when obligation diagnostics match the intended lifecycle. If `required` reports
too many violations, prefer narrowing it into an `invariant` before adding more rules.

## Stage 6C: Comparison and Cross-Entity Rules

Add comparison propositions and cross-entity constraints last. These rules are powerful
but carry more modeling obligations: comparison propositions need explicit `sets`, and
cross-entity rules are evaluated from the participating entities' reached patterns.

```rdra
sets(SellItem, Inventory, stock < selling, true)
forbidden(Inventory, stock < selling)

forbidden(Order, Payment,
  Order.status == cancelled,
  Payment.status == captured)

invariant(Order, Payment)
  .when(Order.status == paid)
  .then(Payment.status == captured
```

Ask the user:

- Which comparisons matter even though the concrete numeric or date values are not
  tracked in the abstract state space?
- Which rules really mention more than one entity?
- Is a cross-entity rule a global cross-product rule, or only intended along a declared
  relation path?

Validation:

```sh
rdra-ish states src/
rdra-ish states src/ --entity Order --format json
```

Move on when advanced diagnostics either disappear, are accepted as known modeling gaps,
or are intentionally reported as not evaluable from the current abstract state axes.

## Choosing the Next Question

Use the current abstraction level to decide what to ask next:

| If the model has... | Ask next for... |
|---|---|
| BUCs but no actors | initiating actors and external systems |
| Actors/use cases but no CRUD | data objects touched by each use case |
| CRUD but no screens | visible UI surfaces or external interfaces |
| direct CRUD but cross-usecase transaction or reference consistency is required | API endpoints and `invokes` relationships |
| entities with only `id` | columns, keys, and relationships |
| Enum/Bool/nullable columns but no `sets` or `transitions` | use-case effects and events |
| states but unreachable variants | missing `raises`, `transitions`, or `sets` |
| stable reachable states | local guardrails (`forbidden`, `exclusive`) |
| local guardrails are stable | local obligations (`invariant`, narrow `required`) |
| local obligations are stable | comparison propositions or cross-entity constraints |

## Summary

<!-- derived-from #principle -->
<!-- derived-from #stage-map -->
<!-- derived-from #api-boundary-rules -->
<!-- derived-from #choosing-the-next-question -->

Incremental modeling works best when each stage answers one question and leaves later
detail out until it becomes useful. The model should move from BUC intent, to use-case
coverage, to data touchpoints, to interaction boundaries and consistency-oriented API
boundaries, to entity structure, to lifecycle, and finally to business-rule constraints.
