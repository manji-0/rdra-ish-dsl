# Language Reference

This document is the complete reference for the `rdra-ish` DSL. The DSL describes
RDRA (Relationship-Driven Requirements Analysis) models: you declare model elements
as typed instances and connect them with predicate calls.

A source file (`.rdra`) consists of an optional `module` declaration, zero or more
`import` declarations, and a sequence of instance declarations, entity bodies,
predicate calls, and constraint declarations, in any order.

## File Structure and Comments

Comments use `//` and run to the end of the line. Block comments use `/* ... */`.
Legacy `#` line comments are not accepted. If an older model uses `#` for comments,
replace those line-comment prefixes with `//`; see the repository
[changelog](../CHANGELOG.md) for the migration note.

---

## Instance Declarations

An instance declaration introduces a named element:

```
<kind> <Id> "Label"
```

- `<kind>` is one of the kinds in the table below.
- `<Id>` is a unique identifier within its kind (an upper-camel-case identifier by
  convention).
- `"Label"` is a human-readable display string used in generated diagrams and lists.

An instance may also carry an optional description:

```
<kind> <Id> "Label" description "Description text"
```

`description` is stored as element metadata for all instance kinds, including
`business`, `buc`, `usecase`, `system`, and `api`. Diagram emitters use labels by
default so diagrams stay compact. Use `rdra-ish diagram --show-description` to render
descriptions as review annotations where supported.

`requirement` declarations may also carry requirement-specific metadata after the
label or description:

```
requirement ReqCheckout "Checkout must be reliable"
  description "The checkout flow must preserve customer intent."
  priority "must"
  source "Customer interview"
  stakeholder "Store Operations"
  owner "Product Owner"
  acceptance criteria "A payment timeout leaves the cart recoverable."
  status "proposed"
  risk "high"
  rationale "Checkout failures directly block revenue."
```

`source`, `stakeholder`, and `acceptance criteria` may be repeated. Single-value
fields use the last value when repeated. `acceptance_criteria "..."` is accepted as
an identifier-friendly spelling of `acceptance criteria "..."`.

Use `rdra-ish list --kind requirement` to review requirement metadata in table,
CSV, or JSON form.

| kind | Description |
|---|---|
| `actor` | A human actor who performs use cases. |
| `extsystem` | An external system used by actors or use cases. |
| `system` | An internal system boundary. It groups APIs; its entity set is derived from those APIs' CRUD targets. |
| `requirement` | A requirement that motivates business use cases. |
| `adr` | An architecture decision record. It captures context, selected/rejected options, reasons, and consequences, then links the decision to impacted model elements with `decides`. |
| `nfr` | A non-functional requirement with measurable or reviewable quality metadata such as latency, SLO, availability, resilience, audit, retention, or privacy classification. |
| `quality` | A quality attribute category such as performance, availability, resilience, auditability, observability, or privacy. |
| `constraint` | A non-functional guardrail or compliance constraint, commonly used for audit, logging, retention, privacy, or operational restrictions. |
| `concept` | A business concept that may or may not become a persisted data structure. |
| `domain_object` | A domain model object that represents behavior or business identity before database design. |
| `aggregate` | A domain consistency boundary that groups domain objects, value objects, or concepts. |
| `valueobject` | A value object without independent identity, modeled separately from database columns or tables. |
| `business` | A business (a coarse-grained area of activity) that BUCs belong to. |
| `buc` | A business use case: a unit of business value composed of use cases. |
| `flow` | A business flow: an ordered business narrative inside a BUC. A flow is composed of steps and can make UC/event ordering explicit without turning the diagram into the only source of truth. |
| `step` | A business flow step. A step is a business-language action that can cover a use case, API, or event while remaining distinct from implementation boundaries. |
| `usagescene` | A usage scene describing a context of use. |
| `usecase` | A use case: a concrete interaction that reads/writes entities and displays screens. |
| `screen` | A screen (UI surface) displayed by use cases. |
| `field` | A screen field: a first-class UI input/output item. It can carry editability, requiredness, and actor/system source metadata, and can map to an Entity column without becoming one. |
| `event` | A domain event, used as the trigger of state transitions and as a `raises`/`triggers` endpoint. |
| `entity` | A logical data model/table-like persistent structure. Use conceptual model elements for domain concepts that are not yet, or should not be, tables. |
| `state` | A state machine node, linked to an entity's Enum column. |
| `condition` | A condition. |
| `variation` | A variation. |
| `api` | An API layer endpoint invoked by a use case; operates entities on behalf of the use case and defines an atomic data operation boundary. API declarations may also carry HTTP contract metadata. Appears in the RDRA layered graph and sequence diagram as a named API lane, but is intentionally omitted from the boundaryless graph. |
| `dto` | A request, response, or error payload shape used by API contracts. DTOs may carry a column-like field body but are kept separate from database entities. |
| `location` | A place, channel, organization point, or usage scene for Business-BUC context. |
| `timing` | A timing, trigger, or business situation for Business-BUC context. |
| `medium` | A physical medium, device, terminal, or operation medium for Business-BUC context. |
| `permission` | A permission or role-like authority assignable to actors. |

---

## Entity Bodies

An `entity` or `dto` declaration may be followed by a brace-delimited body of
field definitions:

```
entity Order "Order" {
  id:           Int      @pk
  total:        Money
  ordered_at:   DateTime
  delivered_at: DateTime @null
  status:       Enum(pending, paid, shipped, delivered, cancelled) @default(pending)
  note:         String   @null
}
```

Each field has the form `name: Type [annotations...]`. For entities these fields
are database columns. For DTOs they are contract fields; `@null` means the field
is optional/nullable in the payload.

### Column types

| Type | Description |
|---|---|
| `Int` | Integer. |
| `String` | Text. |
| `Money` | Monetary amount. |
| `DateTime` | Date and time. |
| `Date` | Date. |
| `Bool` | Boolean. A boolean column is a **state axis** (see [state-derivation.md](./state-derivation.md)). |
| `Decimal` | Decimal number. |
| `Enum(a, b, c)` | Enumeration over the listed variants. An Enum column is a **state axis** and can be linked to a state machine. |

### Column annotations

| Annotation | Description |
|---|---|
| `@pk` | Marks the column as the primary key. FK columns are auto-generated against this key by `relate` (single-column `@pk` only). |
| `@pk(a, b)` | Declares a composite primary key over the named columns. Auto-FK generation is **not** supported for composite keys — declare FK columns explicitly on the many side. |
| `@unique` | Adds a unique constraint. |
| `@unique(a, b)` | Declares a composite unique constraint over the named columns. |
| `@index` | Adds an index for the column. |
| `@index(a, b)` | Declares a composite index over the named columns. |
| `@check("expr")` | Declares a check constraint expression for review/export. The expression is stored verbatim. |
| `@null` | Makes the column nullable. A nullable column is a **state axis** with values `{null, present}`. |
| `@default(v)` | Declares a default value `v`. Used as the seed value in state derivation. |
| `@label("...")` | Overrides the column's display label. |
| `@soft_delete` | Marks the column as the soft-delete marker, such as `deleted_at` or `is_deleted`. |
| `@history` | Marks the column as part of history/versioning, such as `valid_from`, `valid_to`, or `version`. |
| `@tenant` | Marks the column as the tenant-scope discriminator. |
| `@derived("expr")` | Marks the column as derived and stores the derivation expression verbatim. |

Composite `@pk(...)`, `@unique(...)`, and `@index(...)` annotations are written on
one column line for locality, but semantically they belong to the entity as a whole.
`@check(...)` and `@derived(...)` are expression-carrying metadata; the current model
stores them for review/export and does not parse them into executable SQL.
`rdra-ish export --kind dbml` projects logical `entity` declarations to DBML tables,
including indexes, composite unique constraints, generated FK columns, and `relate`
delete/update options. Metadata without a native DBML equivalent is preserved as
notes so the review signal is not lost.
`rdra-ish export --kind json-schema` also emits Entity schemas under `$defs` using
`Entity.<id>` names, with RDRA-specific data-modeling metadata preserved as
`x-rdra-ish-*` extensions.

---

## Conceptual Model

Use conceptual model elements when the business language has important nouns that
should not be collapsed into database tables too early:

```
concept PatientIdentity "Patient identity"
concept CarePlan "Care plan"
domain_object Appointment "Appointment"
aggregate SchedulingAggregate "Scheduling aggregate"
valueobject TimeSlot "Time slot"

contains(SchedulingAggregate, Appointment)
contains(SchedulingAggregate, TimeSlot)
contains(SchedulingAggregate, PatientIdentity)
```

Conceptual elements can stand alone. This is useful for terms that matter during
requirements and domain analysis but are not persisted directly. When a
conceptual/domain element does have a logical data model representation, declare
the mapping explicitly:

```
entity AppointmentTable "appointment table" {
  id: Int @pk
  starts_at: DateTime
}

maps_to(Appointment, AppointmentTable)
maps_to(TimeSlot, AppointmentTable)
```

This keeps the conceptual model and logical data model connected without making
`entity` mean every business concept.

---

## Relationship Predicates

Predicates connect declared instances. Arguments are referenced by their `<Id>`. When
an id is ambiguous across kinds (for example an `event` and a `usecase` share a name),
qualify the argument with a kind prefix — see [Kind-Qualified References](#kind-qualified-references).

| Predicate | Signature | Semantics |
|---|---|---|
| `performs` | `(Actor, UseCase \| Buc)` | The actor performs the given use case or BUC. |
| `uses` | `Actor == ExtSystem` | The actor uses the external system. |
| `invokes` | `UseCase == Api` | The use case invokes the API layer. Drives the `Screen → Api → Entity` lane in the sequence diagram. |
| `request` | `Api == Dto` | The API accepts the DTO as a request payload. |
| `response` | `Api == Dto` | The API returns the DTO as a successful response payload. |
| `error_response` | `Api == Dto` | The API may return the DTO as an error payload. |
| `applies_to` | `(Nfr, UseCase \| Api \| System)` | The non-functional requirement applies to the use case, API, or system boundary. Use this for performance, availability, or SLO requirements tied to an operation or system. |
| `qualifies` | `(Nfr \| Constraint, Quality)` | The non-functional requirement or constraint belongs to the given quality category. |
| `constrains` | `(Constraint, UseCase \| Api \| System \| Entity \| Dto)` | The constraint applies to the target model element. Use this for audit/logging/retention/privacy rules that constrain system behavior or data handling. |
| `maps_to` | `(Concept \| DomainObject \| Aggregate \| ValueObject, Entity)` | Maps a conceptual/domain model element to its logical data model representation. The source can exist without a mapping when it is not table-backed. |
| `reads` | `(UseCase \| Api, Entity)` | The use case or API reads the entity. |
| `writes` | `(UseCase \| Api, Entity)` | The use case or API writes the entity (treated as an update for state derivation). |
| `creates` | `(UseCase \| Api, Entity)` | The use case or API creates the entity. Seeds the initial state pattern. |
| `updates` | `(UseCase \| Api, Entity)` | The use case or API updates the entity. Produces a transition operation in state derivation. |
| `deletes` | `(UseCase \| Api, Entity)` | The use case or API deletes the entity. Marks the source pattern terminal. |
| `displays` | `UseCase == Screen` | The use case displays the screen. |
| `shows` | `Screen == Entity` | The screen shows information from the entity. |
| `maps_field` | `(Field, Entity, "column")` | Maps a screen field to a specific Entity column. Use with `contains(Screen, Field)` when screen-item level input/output mapping matters. |
| `raises` | `UseCase == Event` | The use case raises the domain event. Links use cases to `transitions`. |
| `triggers` | `(Event, UseCase \| Buc)` | The event triggers a concrete use case or starts a BUC boundary. |
| `outbox` | `(Event)` | Marks a raised event as intentionally published outside the local model, suppressing the raised-but-unconsumed warning. |
| `contains` | `(Buc, UseCase \| Flow)`, `Flow == Step`, `Screen == Field`, `System == Api`, or `(Aggregate, DomainObject \| ValueObject \| Concept)` | The use case or flow composes the BUC, the step composes the flow, the field belongs to the screen, the API belongs to the system boundary, or the aggregate owns conceptual/domain parts. |
| `owns` | `System == Entity` | Optionally declares that a system is responsible for an entity even before API operations are fully modeled. API CRUD still records concrete access; `owns` records intended ownership. |
| `precedes` | `Step == Step` | The first business step normally occurs before the second. |
| `branches` | `Step == Step` | The first step may branch to the second as an alternate path. |
| `excepts` | `Step == Step` | The first step may route to the second as an exception path. |
| `repeats` | `Step == Step` | The first step may loop back to the second. |
| `covers` | `(Step, UseCase \| Api \| Event)` | The business step covers the referenced use case, API, or event. Use this to make UC ordering and event chains explicit in the DSL while keeping the step vocabulary business-facing. |
| `compensates` | `UseCase == UseCase` | The first use case compensates for or rolls back the business effect of the second. Use this for alternative or exception flow review without forcing compensation into CRUD/state derivation. |
| `coordinates` | `(UseCase, Entity, Entity)` | The use case coordinates consistency for a relation crossing system boundaries. The use case must invoke APIs on both system sides that operate the corresponding entities. |
| `belongs` | `Buc == Business` | The BUC belongs to the business. |
| `has_permission` | `Actor == Permission` | The actor has the permission type. This provides a base vocabulary for later UC/API permission constraints. |
| `requires_permission` | `(UseCase \| Api, Permission)` | The use case or API requires the permission type. |
| `requires_medium` | `(UseCase \| Api, Medium)` | The use case or API requires the operation medium. |
| `motivates` | `Requirement == Buc` | The requirement motivates the BUC. |
| `decides` | `(Adr, Buc \| UseCase \| Api \| System \| Entity \| Requirement \| Nfr \| Constraint \| Concept \| DomainObject \| Aggregate \| ValueObject \| Dto)` | The ADR records a design decision that affects the target element. Use this for impact review and design traceability without making the target depend on a document file path. |
| `relate` | `(Entity, Entity, Card)` | Declares an ER relationship and auto-generates the FK columns. `Card` is one of `1:1`, `1:N`, `N:1`, `N:M` (unquoted). FK options can be chained with `.optional()`, `.on_delete(action)`, and `.on_update(action)`. |
| `transitions` | `(Entity.column, Event, from -> to)` | A lifecycle edge on an Enum column: on the event, the entity moves from the first variant to the second. |
| `after` | `(UseCase).assert(...)` | A temporal anchor assertion checked against the use case's immediate `sets` and `raises`/`transitions` effects. |
| `when` | `(conditions...).none/has(conditions...)` | A to-many quantifier constraint (replaces `forbidden_when`). Prefer qualified columns such as `Cert.status == revoked`. |
| `sets` | `(UseCase \| Event, Entity, col == val)` | An explicit column effect, consumed by state pattern derivation. Use `present` (not PostgreSQL type names) for nullable non-null. |
| `sets` | `(UseCase \| Event, Entity, col op rhs, true \| false)` | Drives the truth value of a comparison proposition (derived Bool axis) in state pattern derivation. |

`relate(A, B, N:1)` generates a `b_id` FK on `A`. With
`.optional().on_delete(set_null).on_update(cascade)`, the generated FK column is
nullable, marked `fk_optional`, and carries the delete/update actions for CSV/list
review. `relate(A, B, 1:N)` applies the same options to the FK generated on `B`.

### Use case conditions

Use cases can carry review-facing execution contracts:

```rdra
usecase CapturePayment "Capture payment"
  precondition "The order is authorized."
  guard "Payment provider is available."
  postcondition "Payment is captured or a business error is raised."
  alternative "Customer chooses another payment method."
  error "Authorization expires before capture."
```

`precondition`, `postcondition`, `guard`, `alternative` / `alternative_flow`, and
`error` / `error_condition` / `business_error` may be repeated. These clauses are
stored on the use case and shown by `rdra-ish list --kind usecase`; they are not yet
evaluated by the state-pattern engine. Use `compensates(RefundPayment, CapturePayment)`
to make a compensation relationship explicit between use cases.

### Business flows

Business flows are optional, first-class narrative structure for cases where BUC
membership alone is too flat:

```
buc BucCheckout "Checkout"
flow CheckoutFlow "Checkout flow"
step ReviewCart "Review cart"
step AuthorizePayment "Authorize payment"
step PaymentFailed "Payment failed"
usecase CapturePayment "Capture payment"
event PaymentRejected "Payment rejected"

contains(BucCheckout, CheckoutFlow)
contains(CheckoutFlow, ReviewCart)
contains(CheckoutFlow, AuthorizePayment)
precedes(ReviewCart, AuthorizePayment)
excepts(AuthorizePayment, PaymentFailed)
repeats(PaymentFailed, ReviewCart)
covers(AuthorizePayment, CapturePayment)
covers(PaymentFailed, PaymentRejected)
```

Use `precedes` for the main path, `branches` for alternatives, `excepts` for error
or exception routes, and `repeats` for loops. `covers` is intentionally separate
from `contains`: a step is not the use case or event itself; it is the business
meaning that anchors one or more model elements.

### Screen fields and input/output mapping

Use `field` when a screen item needs to be reviewed as a model element rather
than only inferred from CRUD:

```
screen CheckoutScreen "Checkout screen"
field ShippingAddress "Shipping address" access editable required true source actor
field OrderTotal "Order total" access readonly required true source system

entity Order "Order" {
  id: Int @pk
  shipping_address: String
  total: Money
}

contains(CheckoutScreen, ShippingAddress)
contains(CheckoutScreen, OrderTotal)
maps_field(ShippingAddress, Order, "shipping_address")
maps_field(OrderTotal, Order, "total")
```

Supported field clauses are:

| Clause | Description |
|---|---|
| `access` | `editable` or `readonly` by convention. Project-specific tokens are accepted. |
| `required` | `true` or `false`, representing whether the field is required on the screen. |
| `source` / `input` / `derived` | `actor` for actor-entered fields or `system` for system-derived fields by convention. Project-specific tokens are accepted. |

`shows(Screen, Entity)` remains a coarse screen-to-entity information link.
`maps_field(Field, Entity, "column")` is the fine-grained mapping when a concrete
screen item corresponds to a logical data column.

### API contracts

<!-- derived-from #relationship-predicates -->

API declarations can carry transport-facing contract metadata:

```
api CreateOrder "Create order"
  method POST
  path "/orders"
  idempotency "idempotent"
  mode sync
  auth bearer
```

Supported API clauses are:

| Clause | Description |
|---|---|
| `method` | HTTP method, for example `GET`, `POST`, `PUT`, or `DELETE`. |
| `path` | HTTP path as a string literal. |
| `idempotency` | Idempotency policy such as `idempotent`, `non_idempotent`, or a project-specific token. |
| `mode` | Interaction mode such as `sync` or `async`. |
| `auth` / `auth_scheme` | Authentication scheme such as `bearer`, `oauth2`, `api_key`, or a project-specific token. |

DTOs model payload shapes separately from database entities:

```
dto CreateOrderRequest "Create order request" {
  customer_id: Int
  note: String @null
}

dto OrderResponse "Order response" {
  order_id: Int
}

dto ErrorResponse "Error response" {
  code: String
  message: String
}

request(CreateOrder, CreateOrderRequest)
response(CreateOrder, OrderResponse)
error_response(CreateOrder, ErrorResponse)
```

This remains a contract model first: the DSL stores API intent and payload shapes
without requiring every OpenAPI detail to be authored by hand. `rdra-ish export
--kind openapi` projects APIs that have both `method` and `path` into OpenAPI
operations, with DTOs emitted as `components.schemas`.
Use `rdra-ish export --kind json-schema` when the payload shapes themselves need
to be handed to tooling independently of an OpenAPI operation document; DTOs are
emitted under `$defs` using `Dto.<id>` names.
Use `rdra-ish export --kind asyncapi` when event causality needs a machine-readable
catalog: events become AsyncAPI channels/messages, `raises` and `outbox` become
`send` operations, and `triggers` / `transitions` become `receive` operations.
Protocol, server, and exact event payload schemas are intentionally not inferred
until the DSL has explicit vocabulary for them.

### Architecture decision records

<!-- derived-from #relationship-predicates -->

`adr` models a design decision as a first-class element. It is separate from a
requirement: a requirement says what must be true, while an ADR says which design
option was chosen, which options were rejected, and why.

```
adr AdrCustomerOutbox "Publish customer changes through outbox"
  description "Decision record for external customer-change publication."
  adr_status accepted
  context "External subscribers need customer changes."
  decision "Publish customer changes through a transactional outbox."
  consequence "Delivery becomes eventually consistent."
  accepted "Transactional outbox"
  rejected "Synchronous callback"
  reason "Avoid coupling write latency to external subscribers."

system CustomerSystem "Customer System"
entity Customer "Customer" { id: Int @pk }
api PublishCustomerChanged "Publish customer changed"

decides(AdrCustomerOutbox, CustomerSystem)
decides(AdrCustomerOutbox, Customer)
decides(AdrCustomerOutbox, PublishCustomerChanged)
```

Use `rdra-ish list --kind adr` for decision records with their impacted targets,
and `rdra-ish list --kind adr-impact` for one row per ADR-target pair.

### Non-functional requirements

<!-- derived-from #relationship-predicates -->

Use `nfr` for a measurable or reviewable quality objective. Use `quality` to name
the quality attribute, and `applies_to` to connect the NFR to the use case, API,
or system it governs:

```
quality Performance "Performance"
quality Availability "Availability"

nfr CheckoutLatency "Checkout latency"
  metric p95_latency_ms
  target "<=300"
  window "5m"
  slo "99.9%"
  availability multi_az
  resilience retryable

applies_to(CheckoutLatency, Checkout)
applies_to(CheckoutLatency, CheckoutApi)
applies_to(CheckoutLatency, CoreSystem)
qualifies(CheckoutLatency, Performance)
```

Supported NFR clauses are:

| Clause | Description |
|---|---|
| `metric` | Metric name, such as `p95_latency_ms`, `error_rate`, or `availability_ratio`. |
| `target` | Target value as a string literal, such as `"<=300"` or `"<0.1%"`. |
| `window` | Measurement window, such as `"5m"` or `"30d"`. |
| `slo` | Service-level objective value or label. |
| `availability` | Availability design or expectation, such as `multi_az` or `active_active`. |
| `resilience` | Fault-tolerance behavior, such as `retryable`, `degraded_mode`, or `manual_recovery`. |
| `audit` | Audit requirement marker. |
| `logging` | Logging requirement marker. |
| `retention` | Retention period or policy. |
| `privacy` / `privacy_classification` | Privacy classification for the governed data or behavior. |

Use `constraint` when the rule is more of a guardrail than a metric:

```
constraint AuditRetention "Audit retention"
  audit enabled
  logging structured
  retention "7y"
  privacy restricted

constrains(AuditRetention, CoreSystem)
qualifies(AuditRetention, Availability)
```

This keeps non-functional vocabulary first-class without forcing every quality
concern into a database entity, API endpoint, or state transition.

### Access constraints

Actor permissions are declared with `permission` and attached to actors with
`has_permission(Actor, Permission)`. Use cases and APIs can then declare the
permission and medium they require:

```
permission ScheduleWrite "Schedule Write"
medium StaffTerminal "Staff Terminal"

has_permission(Staff, ScheduleWrite)
requires_permission(BookAppointment, ScheduleWrite)
requires_medium(BookAppointment, StaffTerminal)
requires_permission(BookingApi, ScheduleWrite)
```

Screen access constraints are derived rather than declared directly. A screen inherits
the constraints of each use case that displays it, and also the constraints of each API
that the use case invokes. Use `csv --kind screen-constraints` to inspect those
screen × use-case/API paths. `check` also compares `requires_permission` against
`has_permission` on the actors that can perform the use case path. For APIs, this
check is performed per invoking use case so a shared API must be authorized on every
modeled invocation path. Use `csv --kind actor-permission-audit` or
`list --kind actor-permission-audit` to inspect the full actor × permission projection:
`missing` means a required permission is not assigned to that actor, and `excess` means
the actor has a permission that no modeled performer path currently requires.

### `belongs` context

`belongs(Buc, Business)` declares the Business area a BUC belongs to. The mapping can
also carry optional When / Where / By context with method-chain clauses:

```
timing AppointmentRequested "Appointment Requested"
location FrontDesk "Front Desk"
medium FrontDeskTerminal "Front Desk Terminal"

belongs(BucAppointmentScheduling, ClinicOps)
  .when("patient requests a booking")
  .when(AppointmentRequested)
  .where(FrontDesk)
  .where("patient portal")
  .by(FrontDeskTerminal)
  .by("tablet")
```

- `.when(...)` records the timing, trigger, or business situation where the BUC applies.
- `.where(...)` records the place, channel, organization point, or usage scene.
- `.by(...)` records the physical medium, device, terminal, or other operation medium.
- Each argument may be a string literal or a reference to the corresponding typed element:
  `.when(timing::...)`, `.where(location::...)`, or `.by(medium::...)`.
- Multiple `.when(...)`, `.where(...)`, or `.by(...)` clauses accumulate as alternative context values.

### `relate` and FK generation

`relate(Child, Parent, N:1)` generates a foreign-key column on the child entity
referencing the parent's primary key (for example `parent_id`). The cardinality drives
the crow's-foot notation in the ER diagram and the FK-induced graph used for
[transaction boundary inference](#see-also).

Auto-FK requires a **single-column** `@pk` on the parent. If the parent uses
`@pk(a, b)` (composite), the model reports `CompositePkFkUnsupported` — declare the
foreign-key column(s) explicitly on the child instead.

### `transitions` and the state machine

`transitions(Order.status, event::E, from -> to)` declares one edge of an entity's state
machine. The Enum column is required; state labels are the Enum variants
(case-insensitively). The use case that drives a transition
is found through `raises(UseCase, Event)`. See
[state-derivation.md](./state-derivation.md) for how transitions feed derivation.

### `api` and the API layer

For sequence diagrams, `performs(Actor, UseCase)` is more specific than
`performs(Actor, Buc)`. If a use case has a direct actor, the sequence diagram uses
that actor as the participant for the use-case flow. If no direct actor is declared,
the emitter falls back to the actor on the containing BUC.

The `api` element models the backend API layer that sits between a screen and the
entities it operates. Declare an api once per API endpoint or boundary:

```
api OrderApi "Order API"
```

Then connect it to a use case with `invokes`, and to entities with CRUD predicates:

```
invokes(PlaceOrder, OrderApi)
creates(OrderApi, Order)
creates(OrderApi, OrderLine)
updates(PlaceOrder, Cart)   // direct write still allowed (mixed form)
```

**Sequence diagram behaviour** (`--kind sequence`):
- If a use case invokes at least one API, the sequence diagram renders
  `Actor → Screen → Api → Entity` lanes with the API as the source of DB writes.
- If an API is invoked by multiple use cases, the API's CRUD boundary applies to each
  invoking use case. Split read-only and write APIs when those operations have different
  consistency contracts.
- If a use case has no `invokes` (legacy), the existing `System` participant is used
  unchanged — full backward compatibility.
- API write groups are rendered as `transaction (API atomic boundary)`. Direct
  use-case writes still use FK-grouped transaction blocks
  (`group transaction (inferred from FK)`).
- FK-grouped direct writes use the full `relate` graph for connectivity. Two written
  sibling entities that share an unwritten parent entity are still grouped as one
  inferred transaction.

**System boundary behaviour**:
- `contains(System, Api)` assigns an API to an internal system boundary.
- A system's entity set is the union of entities operated by its APIs and entities
  explicitly declared with `owns(System, Entity)`.
- Use `owns` to model a deliberate responsibility boundary before every API operation
  exists. The tool warns when explicit ownership and API CRUD imply different
  boundaries, but the ownership declaration still keeps the entity visible in the
  system boundary.
- If entities derived for different systems have a `relate` edge between them, the tool
  warns unless a use case declares `coordinates(UseCase, EntityA, EntityB)`.
- A coordinating use case must invoke an API in each entity's system, and each invoked
  API must operate the corresponding entity.
- If the same API or entity is derived into multiple systems, the tool emits a warning
  because the ownership boundary is ambiguous.

**Other diagrams**:
- `api` nodes are included in the RDRA layered graph (`--kind rdra`), where they appear
  in the system layer.
- `api` nodes are intentionally omitted from the boundaryless relationship graph
  (`--kind boundaryless-graph`) to keep that view focused on business and data links.
- `api` nodes are also omitted from the event-flow diagram (`--kind event-flow`),
  because an API is not a participant in the raises/triggers/transitions causal chain.

**CSV/list output**:
- `list --kind api` — id/label table of all declared apis.
- `list --kind system` — id/label table of all declared systems.
- `csv --kind api` — CSV file of api id and label.
- `csv --kind api-matrix` — api × entity CRUD matrix.

**Diagnostics** (emitted as warnings when running `diagram --kind sequence`):
- `ApiNeverInvoked` — an api is declared but no use case invokes it.
- `ApiInvokedButNoEntity` — an api is invoked but operates no entity.
- `ApiInMultipleSystems` — an api is assigned to multiple systems.
- `EntityInMultipleSystems` — an entity is operated by APIs in multiple systems.
- `EntityOwnedByMultipleSystems` — an entity has multiple explicit `owns` declarations.
- `OwnedEntityWithoutApiOperation` — a system owns an entity but no API in that system operates it yet.
- `ApiOperatesEntityOutsideOwner` — an API operates an entity explicitly owned by another system.
- `CrossSystemEntityRelation` — a `relate` edge crosses derived system entity sets.
- `CoordinationNotCrossSystem` — `coordinates` is declared for a pair that does not cross two system boundaries.
- `CoordinationMissingApi` — the coordinating use case does not invoke an API on one side of the boundary.

### `triggers` and event-driven BUC chaining

`triggers(event::E, buc::B)` declares that an event starts a downstream BUC. This is the
standard abstract form when the business handoff is known but the concrete entry use
case is not yet fixed, or when a BUC may later be changed from human-initiated to
event-initiated without reshaping the BUC itself.

`triggers(event::E, usecase::UC)` declares that an event causes a concrete downstream
use case to execute. This is the refined form when the entry action is known. The tool
validates that a triggered use case belongs to at least one BUC via `contains`; a warning
is emitted otherwise. Use `diagram --kind event-flow` to visualise the full
`raises -> Event -> triggers` chain alongside state transitions. State derivation uses
immediate same-entity effects from the upstream use case or event as guards for the
triggered use case when those effects target known state axes.

`outbox(event::E)` declares that a raised event is intentionally published outside the
local model boundary, such as a domain event sent to an outbox, external subscriber, or
manual downstream process. It suppresses only the warning for a raised event that has no
local `transitions` or `triggers` consumer; it does not make an unraised event count as
raised.

### Event-triggered BUCs

<!-- constrained-by #relationship-predicates -->
<!-- derived-from ./rdra-ish-interpretation.md#business-flow -->

The standard way to describe a BUC that starts from an event is:

1. Declare the target BUC.
2. Connect the event to that BUC with `triggers(Event, TargetBuc)`.
3. When the entry action becomes clear, add `contains(TargetBuc, EntryUseCase)` and
   optionally refine the flow with `triggers(Event, EntryUseCase)`.

The BUC target is the reviewable boundary of the handoff. The use-case target is the
more concrete entry point inside that boundary. Both forms may coexist: the BUC edge
says "this event starts that business-value slice", while the use-case edge says "this
is the current entry action".

```rdra
buc BucBillingClaims "Billing Claims"
usecase GenerateClaim "Generate Claim"
event EncounterSigned "Encounter Signed"

triggers(event::EncounterSigned, BucBillingClaims)
contains(BucBillingClaims, GenerateClaim)
triggers(event::EncounterSigned, GenerateClaim)
```

If the triggered use case is a system-triggered step, `performs` may be omitted. If it
declares `requires_permission` directly or through an invoked API, `check` reports a
warning until an actor path with the required permission is modeled.

---

## Value Vocabulary for `sets`

The fourth argument of `sets` is a string whose interpretation depends on the target
column's type:

```
// Enum column variant
sets(usecase::Capture, Payment, status == captured)

// Bool column
sets(usecase::Enable, Switch, enabled == true)

// Set a nullable column to non-null without recording a type
sets(usecase::Login, UserAccount, last_login_at == present)

// Set a nullable column to non-null, recording a PostgreSQL-specific type
sets(usecase::Deliver, Order, delivered_at == present)
sets(usecase::Tag, Doc, metadata == present)

// Set a nullable column to null
sets(usecase::Logout, Session, token == null)

// Event as origin: expanded to every UC that raises the event
// (equivalent to sets on each raising UC, but kept close to the event definition)
sets(event::EvDeliver, Order, delivered_at == present)
```

| Value | Target column | Meaning |
|---|---|---|
| Enum variant name | `Enum` column | Set the column to that variant. |
| `true` / `false` | `Bool` column | Set the boolean value. |
| `present` | `@null` column | Make the column non-null (value present). |
| `null` | `@null` column | Make the column null. |

PostgreSQL type names are **not** valid in `sets`; use `present` for a nullable non-null
value. Display/storage types belong on entity annotations or DBML export inference.

### Driving comparison propositions with `sets`

A **comparison proposition** (`col op rhs`, e.g. `stock < selling`, `expired_at < now`) is
a derived Bool axis in the state space. Because continuous values such as `Int` or `DateTime`
are not tracked in the abstract state space, the truth value of a comparison proposition
**must be driven explicitly** using the four-argument form of `sets`:

```
// Tell the model that Sell causes (stock < selling) to become true
sets(Sell,   Stock, stock < selling, true)

// Tell the model that Refund causes (stock < selling) to become false
sets(Refund, Stock, stock < selling, false)
```

The third argument is the bare comparison expression (no quotes); the fourth argument is
the bare boolean literal `true` or `false`. Value equality (`col == variant`) must not use
`, false` — that form is rejected (`SetsFalseOnEquals`); omit the fourth argument and set
a different value instead. If no `sets` drives a comparison proposition, it is treated as
**always false** (the comparison never holds) throughout state derivation, and the
state-pattern diagnostics report `UndrivenComparisonProp` for constraints that depend on it.

| Fourth argument | Meaning |
|---|---|
| `true` | The comparison holds after this use case / event executes. |
| `false` | The comparison does not hold after this use case / event executes. |

---

## Entity State Constraints

<!-- derived-from ./state-derivation.md#temporal-anchor-assertions -->

State constraints assert facts about the **reachable state space** of an entity. They
are evaluated after BFS state-pattern derivation: a constraint is only reported as
violated if a reachable pattern actually witnesses the violation. Unreachable bad
states never trigger a diagnostic.

There are four entity-local constraint forms. Each one names a common shape in the
reachable state space:

| Predicate | Constraint shape | Use when |
|---|---|---|
| `forbidden` | no reachable pattern may satisfy all listed conditions | Invalid combinations |
| `invariant` | when guards hold, required conditions must also hold | Conditional co-occurrence |
| `required` | every reachable pattern must satisfy all listed conditions | Always-required facts |
| `exclusive` | no reachable pattern may satisfy two or more listed conditions | Mutual exclusion |

Entity-local equality conditions are written as `column == value` comparison expressions.

### `forbidden` — tuple-variadic forbidden states

```
// A single forbidden value
forbidden(Order, status == cancelled)

// An AND-combination: forbidden only when both hold simultaneously
forbidden(Order, status == delivered, refunded == true)

// A comparison expression as a condition
forbidden(Stock, stock < selling)

// Mixing tuples and a comparison expression
forbidden(Stock, status == on_sale, stock < selling)

// Comparison against the built-in `now` keyword
forbidden(Coupon, expired_at < now)
```

Each condition is either a `column == value` tuple or a comparison expression. All
conditions are combined with **AND**: the state is forbidden only when **every** condition
holds at once. If any reachable pattern satisfies all conditions, a
`ForbiddenStateViolated` diagnostic is emitted, naming the conditions and the offending
pattern. Multi-axis witnesses include a correlation hint, because independently modeled
state axes can combine in the abstract product space even when a business flow would
normally move those axes together. If one axis is a comparison proposition, set that
proposition to `true` or `false` in the same use case that moves the correlated status
axis.

**Design rationale.** A forbidden state is a *point* (or a sub-cube) in the finite
product state space — fundamentally a conjunction of column assignments. The
tuple-variadic form expresses exactly that: a flat list of `column == value` points,
read as "this exact combination must not exist." There is no antecedent/consequent
asymmetry to model, so a single flat list is the most direct encoding.

### `invariant` — method-chain required co-occurrence

```
invariant(Order)
  .when(status == delivered)
  .then(delivered_at == present)

invariant(Order)
  .when(status == delivered)
  .when(refunded == false)     // multiple .when() = AND
  .then(refund_id == null)

// Comparison expression in the guard
invariant(Coupon)
  .when(expired_at < now)
  .then(status == expired)

// Comparison expression as the requirement
invariant(Stock)
  .when(status == on_sale)
  .then(stock < selling)
```

An invariant is an implication. The `.when(...)` clauses are guards (the antecedent)
and the `.then(...)` clauses are requirements (the consequent). Each clause is either a
`column == value` pair or a bare comparison expression. Within each side the clauses are
combined with **AND**. The rule reads: **whenever all `.when()` guards hold, all
`.then()` requirements must also hold.**

For every reachable pattern that satisfies all the guards but violates any requirement,
an `InvariantViolated` diagnostic is emitted, naming the guards, the requirements, and
the offending pattern. If the witness comes from a use case reached through
`triggers(...)`, the diagnostic can include a flow-order hint when immediate upstream
effects do not prove the required guard coverage.

**Design rationale.** Unlike `forbidden`, an invariant has *two distinct sides* — a
guard and a requirement — joined by implication. A flat tuple list cannot express which
conditions are the trigger and which are the obligation. The method-chain form keeps
the two sides syntactically separate (`.when` vs `.then`) and lets each side accumulate
any number of AND-ed conditions unambiguously, while reading naturally left to right.

### `required` — always-required state facts

```
required(Account, active == true)
required(Order, status == paid, paid_at == present)
required(Coupon, expired_at < now)
```

`required(...)` accepts the same condition vocabulary as `forbidden`: each condition is
either a `column == value` tuple or a comparison expression. All listed conditions are
combined with **AND**. The rule reads: **every reachable pattern of this entity must
satisfy all listed conditions**.

If a reachable pattern is missing any required condition, a `RequiredStateViolated`
diagnostic is emitted.

**Design rationale.** `required` is the no-guard form of an invariant. It is possible to
think of it as "true implies these conditions", but naming it directly keeps global
state facts readable and avoids empty `.when()` chains.

### `exclusive` — mutual exclusion between state facts

```
exclusive(Document, approved == true, rejected == true)
exclusive(Order, cancelled_at == present, delivered_at == present)
exclusive(Stock, stock < reorder_threshold, stock > overstock_threshold)
```

`exclusive(...)` accepts the same condition vocabulary as `forbidden`: each condition is
either a `column == value` tuple or a comparison expression. The listed conditions are
treated as alternatives. A reachable pattern violates the rule when **two or more** of
the listed conditions hold at the same time.

If a reachable pattern satisfies multiple exclusive conditions, an
`ExclusiveStateViolated` diagnostic is emitted.

**Design rationale.** Mutual exclusion can be expanded into pairwise `forbidden`
constraints, but the predicate names the business intent directly. This is useful for
nullable lifecycle timestamps, boolean flags, and comparison propositions that should
not co-occur.

### Cross-Entity Constraints

Cross-entity constraints describe rules that mention columns from more than one entity.
They are parsed, type-checked, and checked by `states` after each participating
entity's reachable patterns have been derived.

Use `Entity.column` when a condition points at a column. The optional leading entity
arguments declare the intended scope; when omitted, the scope is inferred from qualified
column references.

```
forbidden(Order, Payment,
  Order.status == cancelled,
  Payment.amount > Order.total)

invariant(Order, Payment)
  .when(Order.status == paid)
  .then(Payment.status == captured

invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status == paid)
  .then(Payment.status == captured
```

`forbidden(...)` accepts a flat AND-list of conditions. `invariant(...)`
uses the same implication shape as `invariant`: `.when(...)` clauses are guards and
`.then(...)` clauses are required conditions. A condition is either:

- `Entity.column == value` for state-like equality, using the same value vocabulary as
  `sets`.
- A comparison expression such as `Order.total > Payment.amount` or
  `Coupon.expires_at < now`.

When more than one entity is in scope, bare column names are rejected so the rule stays
unambiguous. With a single-entity scope, bare columns are still accepted as a shorthand.

`after(UseCase).assert(Entity.column == value, ...)` checks an assertion at a specific
use-case boundary instead of against the full cross-product of reachable state patterns.
The assertion is evaluated from the use case's immediate `sets` effects and any
`transitions` reached through events raised by that use case. Comparison forms are also
valid, for example `after(ExecuteCertIssue).assert(CertificateOrder.status == executed)`.

### Temporal path properties

Named path properties are checked by TLA+/TLC export (`export --kind tla` /
`verify --backend tlc`), not by `rdra-ish states`:

```
property PaidLeadsToShipped "paid eventually reaches shipped"
  leads_to(Order.status == paid, Order.status == shipped)

property EventuallyTerminal "order finishes"
  eventually(Order.status == delivered \/ Order.status == cancelled)

property NeverBoth "mutex"
  always ~(Order.status == paid /\ Order.status == cancelled)
```

| Form | TLA+ |
|---|---|
| `always(expr)` | `[]expr` |
| `eventually(expr)` | `<>expr` |
| `leads_to(p, q)` | `p ~> q` |

Atoms use `Entity.column == value` / `!=`, or numeric comparisons (`<`, `>`, `<=`, `>=`)
over Int / Money / Decimal columns and literals. Prefer connectives `and` / `or` / `not`
(`/\` `\/` `~` remain aliases). See [formal-verification.md](./formal-verification.md).

The property label string is optional: `property StockOk always(Item.stock >= Item.selling)`.

When any `eventually` / `leads_to` property is exported, the Spec includes
`WF_vars(Next)`. In multi-instance export (triggered by multi-entity rules or
quantifiers), temporal formulas are quantified per instance binder
(`\A i \in Entity_Ids: …`). Equality and comparison forms of
`after(UseCase).assert(...)` become independent TLA `PROPERTY` formulas
`[][raised SpecActions => primed posts]_vars` — they check outcomes and are **not**
injected into SpecAction effects. Comparison forms prefer Int arithmetic when those
columns are axes (including cross-entity RHS); otherwise they require a proposition
axis to be `TRUE` after the action when that axis exists.
`forbidden(EntityA, EntityB, ...)` / `invariant(EntityA, EntityB)` (including `.along`) become Safety
conjuncts. Quantifiers and `.along` use finite `Entity_Ids` instance sets when
exported; see [formal-verification.md](./formal-verification.md).

`sets(UC, Entity, intCol == 3)` assigns an Int effect used by TLA+ export.
Money / Decimal columns used in arithmetic also become `IntRange` axes (nullable
numeric columns promote to Int rather than staying as Nullable).
`@default(0)` is valid on Int columns. BFS `states` still ignores Int axes and treats
comparison propositions as Bool axes driven by `sets(..., cmp, true/false)`.

`when(Entity, conditions...).has/none(...)` or
`when(Entity.col == val).has/none(Related.col == val)` declares a
to-many quantifier rule. The syntax is accepted and type-checked, but `states` does not
track linked related-row counts. It emits `QuantifierConstraintNotEvaluated` when the
related condition has reachable patterns. For `none(...)`, if the related condition is
globally unreachable, the rule is treated as satisfied.

`states` evaluates cross-entity rules by taking the cross-product of each participating
entity's reached state patterns when those entities are not connected by a declared
`relate` path. Conditions that reference actual state axes can produce
`CrossForbiddenViolated` or `CrossInvariantViolated` diagnostics in that global-product
case. Conditions that require values absent from the abstract state space, such as
ordinary numeric amount comparisons, produce `CrossConstraintNotEvaluated` instead.

When the participating entities are connected by `relate(...)`, a global-product
witness is reported as `CrossConstraintNotEvaluated` rather than as a violation. The
state pattern engine does not yet track which concrete rows are linked, so the witness
may be a false positive for linked-instance rules.

Adding `.along(EntityA, EntityB, ...)` declares that the rule is intended to apply only
to instances connected through the listed `relate` path, not to the global cross-product.
The current `states` engine validates that the path is declared, can prove the rule
satisfied when even the broader global cross-product has no witness, and can report a
violation when each adjacent pair in the path shares use-case provenance for the
witness patterns. If a witness exists without shared operation provenance, `states`
reports `CrossConstraintNotEvaluated` rather than treating it as a linked-instance
violation, because concrete row identity and FK reachability are not represented in
state patterns.

### Comparison Expressions in Constraints

`forbidden`, `required`, `exclusive`, multi-entity `forbidden`/`invariant`, and the `.when()` / `.then()`
clauses of `invariant` accept bare **comparison expressions** as
conditions (`col == val`, `col op rhs`).

#### Syntax

```
col op rhs
```

Where:

- `col` is a bare column name on the left-hand side, or `Entity.column` in a
  cross-entity constraint.
- `op` is one of: `<`, `>`, `<=`, `>=`, `==`, `!=`.
- `rhs` is one of:
  - A bare column name or `Entity.column` reference: `stock >= selling`,
    `Payment.amount >= Order.total`
  - An integer literal: `quantity >= 0`
  - The built-in keyword `now`: `expired_at < now`

#### Type rules

| Operator | Permitted column types |
|---|---|
| `<`, `>`, `<=`, `>=` | `Int`, `Money`, `Decimal`, `Date`, `DateTime` |
| `==`, `!=` | All column types |

When `rhs` is a column reference, the left-hand and right-hand columns must belong to
the same type category (numeric with numeric, temporal with temporal). When `rhs` is
`now`, the left-hand column must be of type `Date` or `DateTime`.

#### Semantics and interaction with `sets`

A comparison expression is treated as a **derived Bool proposition axis** in the state
space. Because continuous-valued types (`Int`, `DateTime`, etc.) are abstracted away in
the state-pattern model, the runtime truth value of a comparison cannot be inferred
automatically. Instead, it must be driven explicitly with `sets`:

```
// Declare that the Sell use case causes (stock < selling) to become true
sets(Sell,   Stock, stock < selling, true)

// Declare that the Refund use case causes (stock < selling) to become false
sets(Refund, Stock, stock < selling, false)
```

If no `sets` drives a given comparison proposition, the proposition is treated as
**always false** (the comparison never holds) for the purposes of state-pattern
derivation and constraint checking. In practice this means:

- A `forbidden` condition containing an undriven comparison is never triggered (the
  comparison is never true, so the conjunction is never satisfied).
- A `required` condition containing an undriven comparison is violated in every
  reachable pattern, because the required proposition is always false.
- An `exclusive` condition containing an undriven comparison never contributes to an
  exclusivity violation.
- A `.when()` guard containing an undriven comparison is never satisfied, so the
  invariant never fires.
- A `.then()` requirement containing an undriven comparison is always violated whenever
  its guard is satisfied.

Always add matching `sets` calls whenever you use a comparison expression in a
constraint. The state-pattern diagnostics emit `UndrivenComparisonProp` when a
comparison is used without a matching `sets(..., comparison, true/false)`, because
the constraint will have no practical effect (for `forbidden`, `exclusive`, or
`.when`) or will always fire (for `required` or `.then`).

### Bare identifiers

Column names and values inside `forbidden(...)`, `required(...)`, and `exclusive(...)`
arguments, plus `.when()` / `.then()` clauses, are **bare identifiers** (or comparison
expressions). They use the same value vocabulary as `sets`: Enum variant names,
`true` / `false`, and `present` / `null`.

In **comparison expressions** (e.g. `stock < selling`, `expired_at < now`), column names
on both sides are also bare identifiers. The right-hand side may additionally be an
integer literal or the built-in keyword `now`. `now` is a reserved keyword in this
position — it does not refer to a column named `now`; it denotes the current
date/time and is only valid as the right-hand side of a comparison whose left-hand
column is of type `Date` or `DateTime`.

---

## Kind-Qualified References

When an id is unambiguous, you reference an instance by its bare id. When the same id is
declared for more than one kind — a common and natural situation, e.g. a `state Active`
and an `event Active`, or an `event Cancel` and a `usecase Cancel` — you must
disambiguate the reference with a kind prefix:

```
usecase::Foo      // the use case named Foo
event::Cancel     // the event named Cancel
state::Active      // the state named Active
```

Typical cases:

- In `transitions(Order.status, event::Capture, pending -> paid)`, the event and states
  may collide with use cases of the same name once modules are merged, so qualifiers are
  used.
- In `contains(BucOrder, usecase::Cancel)` and `raises(usecase::Cancel, event::Cancel)`,
  the `Cancel` use case and the `Cancel` event coexist.

Use the qualifier whenever the bare id would be ambiguous after all imported modules are
merged; the compiler reports an ambiguity error otherwise.

---

## Imports and Modules

A file may declare the module it belongs to and import other modules:

```
module shared.actors

import shared.actors             // flat import: brings all names into scope
import shared.actors as a        // namespaced import under alias `a`
import shared.actors.{Staff}     // selective import of a single name
import shared.actors.{Staff as S} // selective import with a local alias
```

- `module <path>` declares the dotted module path of the current file. Each path must be
  unique across the loaded sources.
- `import <path>` brings the module's declarations into flat scope. Unknown module paths
  are errors. Two flat imports that bind the same local name to different modules are
  errors.
- `import <path> as <alias>` makes the imported names available under the alias namespace
  (`alias.Name`). Bare names from that module are **not** in flat scope.
- `import <path>.{Name}` imports only the named declarations into flat scope.
- `import <path>.{Name as Local}` imports a name under a local alias (original name hidden).
- The same id may be declared in **different modules**; disambiguate with aliases /
  namespaces when both are imported. Redeclaring the same kind+id in one module (or the
  same file without a module) remains an error.
- Import visibility is **per file**: only files that contain `import` become closed
  scope. Sibling files without imports keep open-world (legacy) resolution of bare names.
- Reusing the same `as` alias for two modules is an error (`DuplicateAlias`).

When the CLI loads a directory, it merges all `.rdra` files reachable from the entry
files into a single semantic model, resolving imports against the include paths derived
from the input layout.

---

## See Also

- [state-derivation.md](./state-derivation.md) — how reachable state patterns are
  derived, how predicates feed the BFS, and how entity constraints are checked.
- [cli-reference.md](./cli-reference.md) — the `rdra-ish` command-line interface,
  including the `diagram --kind event-flow` command for event causality visualisation.
