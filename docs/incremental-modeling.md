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
streams, actors, the screens they work on, and the data that work touches. Later stages
add technical commitments: API boundaries, ownership, access/media constraints,
persistence structure, lifecycle, and rules that the implementation must preserve. The
order follows RDRA's layers from the outside in: the screen sits with the use case on
the system boundary, nearest the actor, and the information the system keeps sits inside
it.

Start with the smallest model that can answer the current question. Move to the next
stage only when the current abstraction is stable enough to make the added detail
useful.

The terms BUC, business flow, and UC are used in the RDRA-ish sense described in
[RDRA-ish Interpretation](./rdra-ish-interpretation.md): BUCs are business-value
review containers, business flow is the concrete realization of a BUC through UCs and
events, and UCs are effect-bearing interactions. The flow is reviewed through prose
plus generated RDRA, event, CRUD, ER, and state views.

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
| `business`, stable `requirement`, reusable `timer` / `medium` / `permission` | `shared/biz.rdra` or another shared vocabulary file | context and authority vocabulary grows large enough to deserve its own file |
| `entity`, `relate`, shared lifecycle events | `shared/entities.rdra` | entities naturally form bounded groups |
| `buc`, `usecase`, `screen`, BUC-local `api`, BUC-local events | `buc/buc_<name>.rdra` | keep one BUC per file; do not split a single BUC early |
| CRUD, `displays`, `invokes`, access constraints, `raises`, `sets` | the BUC file that explains the use case | never put BUC-specific predicates in shared files |
| cross-BUC `event`, `transitions` | shared file near the owning entity | multiple BUC files raise or trigger the same event |
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

<!-- stage-spec: map -->
| Stage | Concern | Abstraction | Main question | Add | Validate |
|---|---|---|---|---|---|
| 0 | Biz intent | Scope sketch | What business area are we modeling? | `business`, rough `buc`, `belongs`, candidate `actor`, optional `requirement` and `motivates` | `check`, `list --kind buc --format table`, `diagram --kind rdra` |
| 1 | Biz value | BUC skeleton | Who gets value from each BUC? | `performs`, `contains`, rough `usecase`, `flow` / `step` when order matters | `check`, `diagram --kind rdra` |
| 2 | Biz actor boundary | Screens | What does the actor see and do in each use case? | `screen`, `displays`, `navigates`, optional `field` | `check`, `diagram --kind rdra`, `diagram --kind screen`, `list --kind screen --format table` |
| 3 | Biz object touchpoints | Data touchpoints | Which data objects does each use case touch, and which does each screen show? | coarse `entity`, CRUD predicates (`creates`, `reads`, `updates`, `deletes`), `shows`, optional `maps_field` | `check`, `csv --kind matrix`, `diagram --kind er` |
| 4 | Tech system boundary | API and access boundary | Which use cases work through an API, which system owns it, and which permission and medium does the work need? | optional `api` / `invokes`, `dto`, `system`, `permission`, `medium`, `has_permission`, `requires_*` | `check`, `diagram --kind rdra`, `list --kind api --format table`, `csv --kind api-matrix`, `csv --kind screen-constraints`, `csv --kind actor-permission-audit` |
| 5 | Tech data design | Entity structure | What structure and ownership does the data need? | columns, `@pk`, `variation`, `relate`, cardinality, optional `owns` / `coordinates` | `check`, `diagram --kind er` |
| 6 | Tech lifecycle design | Lifecycle | Which states and event-triggered BUC entries are reachable through the BUCs? | `Enum`, `Bool`, `@null`, `event`, `transitions`, `raises`, `triggers`, `timer`, `sets`, `condition` | `check`, `states`, `diagram --kind state`, `diagram --kind event` |
| 7A | Tech-enforced rules | Local guardrails | Which simple states or facts must never co-occur? | `forbidden`, `exclusive` over Enum / Bool / nullable axes | `check`, `states` |
| 7B | Tech-enforced rules | Local obligations | Which reachable states imply or require other facts? | `invariant`, narrow `required` | `check`, `states` |
| 7C | Tech-enforced rules | Advanced rules | Which comparison or cross-entity facts are reviewable from the abstract state space? | comparison propositions, multi-entity `forbidden` / `invariant` | `check`, `states` |
<!-- /stage-spec -->

## Finding the Current Stage

`rdra-ish next` places each BUC on this map and prints what the next stage asks for:

```sh
rdra-ish next src/
rdra-ish next src/ --buc BucOrder
```

Each stage has a gate whose clauses name the elements that fail them, such as a use case
with no CRUD or an entity with only its key. A blocking clause that fails is a gap and
leaves the stage open; an advisory clause that fails is a review note, which never holds a
BUC back, so a use case that is data-free on purpose or an entity that stands alone is a
question, not a blocker. A stage whose clauses pass but which owns an error of `check` is
blocked until the model is fixed. A BUC is at the last met stage of the required prefix,
0 to 6. Stages 7A to 7C are optional: a BUC with a lifecycle and no rule is complete, and
`next` offers the rule stages with their questions. For the next stage, `next` prints its
main question, the gaps, notes and findings, the questions to ask the user, and the
validation commands of the stage section below. The gates are listed in the
[CLI Reference](./cli-reference.md#next).

Different BUCs can be at different stages. Refine one BUC at a time, and run `next`
again after each stage to see what is left and what the next question is. A met gate
means the elements exist; whether the stage is stable enough to move on is still a
review decision, as each stage's "Move on when" says.

<!-- stage-spec: heading scope -->
## Stage 0: Scope sketch
<!-- /stage-spec -->

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

<!-- stage-spec: ask scope -->
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
<!-- /stage-spec -->

Move on when BUC names and business ownership look stable enough for review.

<!-- stage-spec: heading skeleton -->
## Stage 1: BUC skeleton
<!-- /stage-spec -->

Add actors and use cases, but do not require screens or entities yet.

```rdra
actor Customer "Customer"

usecase PlaceOrder "Place Order"
usecase CancelOrder "Cancel Order"

performs(Customer, BucOrder)
contains(BucOrder, PlaceOrder)
contains(BucOrder, CancelOrder)
```

<!-- stage-spec: ask skeleton -->
Ask the user:

- Who initiates or receives value from this BUC?
- What user-visible actions compose the BUC?
- Are any use cases triggered by a system event rather than a human actor?

Validation:

```sh
rdra-ish check src/
rdra-ish diagram src/ --kind rdra --format mermaid --buc BucOrder
```
<!-- /stage-spec -->

Move on when every important action is represented as a use case and each use case
belongs to the intended BUC.

<!-- stage-spec: heading screens -->
## Stage 2: Screens
<!-- /stage-spec -->

Add the screens the actor works on, before deciding which data each use case touches.
In RDRA the screen sits with the use case on the system boundary, outside the information
the system keeps: it is what the actor sees and does, so it is settled first. A use case
that an event or timer starts, with no actor at a screen, needs no screen.

```rdra
screen CartScreen "Cart"
screen CheckoutScreen "Checkout"

displays(PlaceOrder, CheckoutScreen)
displays(CancelOrder, CartScreen)
navigates(CartScreen, CheckoutScreen).via(PlaceOrder)
```

Add a `field` when a screen item needs review as a model element, such as an input the
actor must enter. Leave its entity column mapping (`maps_field`) for the next stage.

<!-- stage-spec: ask screens -->
Ask the user:

- Which screen does the actor work on in each use case?
- Which screen leads to which, and through which use case?
- Which items on a screen does the actor enter or check, and which does the system fill in?
- Is any use case started by an event or timer, with no actor at a screen?

Validation:

```sh
rdra-ish check src/
rdra-ish diagram src/ --kind rdra --format mermaid --buc BucOrder
rdra-ish diagram src/ --kind screen --format mermaid --buc BucOrder
rdra-ish list src/ --kind screen --format table
```
<!-- /stage-spec -->

Move on when each actor-facing use case names the screen it works on and the screen
transitions read as the actor's path through the BUC.

<!-- stage-spec: heading data-touchpoints -->
## Stage 3: Data touchpoints
<!-- /stage-spec -->

Add coarse entities and CRUD predicates, and join the screens to the entities they show.
At this stage, entities may have only `id` columns; detailed attributes can wait.

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
shows(CheckoutScreen, Order)
shows(CartScreen, Cart)
```

<!-- stage-spec: ask data-touchpoints -->
Ask the user:

- Which business objects are created, read, updated, or deleted by each use case?
- Which entities does each screen show?
- Which nouns are logical data structures, and which are only business vocabulary?
- Which entities should be shared across multiple BUCs?

Validation:

```sh
rdra-ish check src/
rdra-ish csv src/ --kind matrix
rdra-ish diagram src/ --kind er --format mermaid --buc BucOrder
```
<!-- /stage-spec -->

Move on when the CRUD matrix tells a plausible story, even if entity columns are still
coarse.

<!-- stage-spec: heading api-access-boundary -->
## Stage 4: API and access boundary
<!-- /stage-spec -->

Add APIs, the systems that own them, and access/media constraints once the
use-case/screen/data path is clear. Each of them is optional: a use case can keep its
direct CRUD, and a model with no `permission` or `medium` has no access constraints to
review. The gate asks only that every API a use case invokes belongs to a system.

```rdra
api OrderApi "Order API"
system OrderSystem "Order System"
permission OrderWrite "Order Write"
medium CustomerDevice "Customer Device"

contains(OrderSystem, OrderApi)
invokes(PlaceOrder, OrderApi)
creates(OrderApi, Order)
requires_permission(PlaceOrder, OrderWrite)
requires_medium(PlaceOrder, CustomerDevice)
```

<!-- stage-spec: ask api-access-boundary -->
Ask the user:

- Does the use case write data directly, or through an API boundary?
- Is the API a reusable boundary or local to this BUC, and which system owns it?
- Which actor permission and physical/device medium are required for the use case or API?

Validation:

```sh
rdra-ish check src/
rdra-ish diagram src/ --kind rdra --format mermaid --buc BucOrder
rdra-ish list src/ --kind api --format table
rdra-ish csv src/ --kind api-matrix
rdra-ish csv src/ --kind screen-constraints
rdra-ish csv src/ --kind actor-permission-audit
```
<!-- /stage-spec -->

Move on when the RDRA graph and the API matrix show the intended actor/screen/API/entity
path and the screen-constraints plus actor-permission audit CSVs show the expected
permission/media paths and actor-side assignments.

### BUC Context and Access Rules

Use `belongs(Buc, Business).by(...)` when the BUC belongs to a
business area only through a specific operating medium.

```rdra
medium CustomerDevice "Customer Device"

belongs(BucOrder, Commerce)
  .by(CustomerDevice)
```

A schedule that starts a use case is a `timer` connected with
`triggers(Timer, UseCase)`, not belongs context.

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

<!-- stage-spec: heading entity-structure -->
## Stage 5: Entity structure
<!-- /stage-spec -->

Refine entities with columns, primary keys, relationships, and cardinality.

```rdra
entity OrderLine "Order Line" {
  id: Int @pk
  qty: Int
  unit_price: Decimal
}

relate(OrderLine, Order, N:1)
```

<!-- stage-spec: ask entity-structure -->
Ask the user:

- What fields identify each entity?
- Which fields are business state, and which are ordinary data?
- What parent/child relationships should own or group records?
- Does a use case need one transaction across related entities?

Validation:

```sh
rdra-ish check src/
rdra-ish diagram src/ --kind er --format mermaid --buc BucOrder
```
<!-- /stage-spec -->

Move on when FK relationships and the transaction warnings of `check` match the intended persistence
boundary.

<!-- stage-spec: heading lifecycle -->
## Stage 6: Lifecycle
<!-- /stage-spec -->

Add lifecycle axes and events after the structural model is stable.

<!-- constrained-by ./state-derivation.md#variables -->
<!-- constrained-by ./state-derivation.md#actions -->

```rdra
entity Order "Order" {
  id: Int @pk
  status: Enum(pending, paid, cancelled) @default(pending)
  paid_at: DateTime @null
}

usecase CapturePayment "Capture Payment"
contains(BucOrder, CapturePayment)
updates(CapturePayment, Order)

transitions(CapturePayment, Order.status, pending -> paid)
transitions(CancelOrder, Order.status, pending -> cancelled)
sets(CapturePayment, Order, paid_at == present)
```

Each transition names the use case that moves the state, as the 遷移UC column of an RDRA
state sheet does. Declare an `event` only when something consumes it: another BUC or use
case it `triggers`, or an `outbox` publication.

<!-- stage-spec: ask lifecycle -->
Ask the user:

- Which Enum column and variants represent lifecycle state?
- Which use case causes each state change?
- Does any event start another BUC, and is the concrete entry use case known yet?
- Which nullable or boolean fields change together with the lifecycle?
- Are the Enum variants the states you want to see? Diagrams derive one state per variant.

Validation:

```sh
rdra-ish check src/
rdra-ish states src/ --entity Order
rdra-ish diagram src/ --kind state --format mermaid --buc BucOrder
rdra-ish diagram src/ --kind event --format mermaid
```
<!-- /stage-spec -->

Move on when reachable patterns explain the expected lifecycle and unexpected terminal
or unreachable states have been reviewed. Model an event-triggered BUC first with
`triggers(Event, TargetBuc)`. When the entry action is clear, add
`contains(TargetBuc, EntryUseCase)` and optionally `triggers(Event, EntryUseCase)`.

<!-- stage-spec: heading local-guardrails -->
## Stage 7A: Local guardrails
<!-- /stage-spec -->

Add low-risk constraints only after there is enough lifecycle behavior to evaluate them.
Start with rules that are easy to read as invalid local states: forbidden combinations
and mutually exclusive facts on one entity.

```rdra
forbidden(Order, status == paid, paid_at == null)

exclusive(Document, approved == true, rejected == true)
```

<!-- stage-spec: ask local-guardrails -->
Ask the user:

- Which combinations must never be reachable?
- Which facts must be mutually exclusive?
- Are these facts already represented as Enum, Bool, or nullable state axes?

Validation:

```sh
rdra-ish check src/
rdra-ish states src/ --entity Order
```
<!-- /stage-spec -->

Move on when diagnostics either disappear or are accepted as known modeling gaps. Keep
the first guardrail pass small; it should catch obviously impossible states without
requiring a complete business rule catalogue.

<!-- stage-spec: heading local-obligations -->
## Stage 7B: Local obligations
<!-- /stage-spec -->

Add implication-style rules after local guardrails are stable. Use `invariant` for
conditional co-occurrence, and use `required` only for facts that truly must hold in
every reachable pattern.

```rdra
invariant(Order)
  .when(status == paid)
  .then(paid_at == present)

required(Account, active == true)
```

<!-- stage-spec: ask local-obligations -->
Ask the user:

- Which values must co-occur once a condition is true?
- Which facts must be true in every reachable state?
- Is the rule too broad, or should it be an `invariant` with a guard instead of `required`?

Validation:

```sh
rdra-ish check src/
rdra-ish states src/ --entity Order
```
<!-- /stage-spec -->

Move on when obligation diagnostics match the intended lifecycle. If `required` reports
too many violations, prefer narrowing it into an `invariant` before adding more rules.

<!-- stage-spec: heading advanced-rules -->
## Stage 7C: Advanced rules
<!-- /stage-spec -->

Add comparison propositions and cross-entity constraints last. These rules are powerful
but carry more modeling obligations: a comparison needs `sets` that move its columns (or
`drives` that drive it), and a cross-entity rule ranges over every combination of the
participating entities' instances unless `.along` links them.

```rdra
drives(SellItem, Inventory, stock < selling, true)
forbidden(Inventory, stock < selling)

forbidden(Order, Payment,
  Order.status == cancelled,
  Payment.status == captured)

invariant(Order, Payment)
  .when(Order.status == paid)
  .then(Payment.status == captured)
```

<!-- stage-spec: ask advanced-rules -->
Ask the user:

- Which comparisons matter, and which use cases move the numeric or date values they read?
- Which rules really mention more than one entity?
- Is a cross-entity rule a global cross-product rule, or only intended along a declared relation path?

Validation:

```sh
rdra-ish check src/
rdra-ish states src/ --entity Order
```
<!-- /stage-spec -->

Move on when advanced diagnostics either disappear or are accepted as known modeling gaps.

## Choosing the Next Question

`rdra-ish next` answers this per BUC from the stage gates. Use the current abstraction
level to decide what to ask next:

| If the model has... | Ask next for... |
|---|---|
| BUCs but no actors | initiating actors and external systems |
| Actors/use cases but no screens | the screen the actor works on in each use case |
| screens but no CRUD | data objects touched by each use case and shown on each screen |
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
coverage, to the screens the actor works on, to data touchpoints, to consistency-oriented
API boundaries and access constraints, to entity structure, to lifecycle, and finally to
business-rule constraints.
