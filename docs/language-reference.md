# Language Reference

This document is the complete reference for the `rdra-ish` DSL. The DSL describes one
requirements model that runs from why a system exists to how its data changes. You
declare each element once as a typed instance and connect elements with predicate
calls. `check` and `lint` then find gaps across layers, and every diagram, CSV, list,
and export is a projection of the same model.

A source file (`.rdra`) consists of an optional `module` declaration, zero or more
`import` declarations, and a sequence of instance declarations, entity bodies,
predicate calls, and constraint declarations, in any order.

## The Review Spine

<!-- derived-from ./rdra-ish-interpretation.md#basic-stance -->

The language follows the RDRA habit of explaining each layer by the layer before it.
Each layer answers one review question, and the sections of this reference follow the
same order:

| Layer | Kinds | Review question | Section |
|---|---|---|---|
| Requirements and traceability | `requirement`, `adr`, `nfr`, `quality`, `constraint` | Why does this exist, and which quality bar applies? | [Requirements and Traceability](#requirements-and-traceability) |
| Business | `business`, `buc`, `flow`, `step`, `timer`, `medium` | Which business value does it serve, in what order? | [Business Layer](#business-layer) |
| Interaction | `actor`, `usecase`, `screen`, `field`, `event` | Who does what, on which screen, and what does it cause? | [Interaction Layer](#interaction-layer) |
| System boundary | `system`, `extsystem`, `api`, `dto`, `permission` | Which boundary owns the operation, and who may call it? | [System Boundary](#system-boundary) |
| Data | `entity` | What is stored, and how is it related? | [Data Layer](#data-layer) |
| Lifecycle and rules | `transitions`, `sets`, and constraint declarations | Which states are reachable, and which are forbidden? | [Lifecycle](#lifecycle), [Entity State Constraints](#entity-state-constraints) |

A kind is part of the language only when some review output consumes it. The
[kind index](#kind-index) lists that output for every kind.

---

## File Structure and Comments

Comments use `//` and run to the end of the line. Block comments use `/* ... */`.
Legacy `#` line comments are not accepted. If an older model uses `#` for comments,
replace those line-comment prefixes with `//`; see the repository
[changelog](../CHANGELOG.md) for the migration note.

---

## Modules and Imports

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
  are errors. Two flat imports that bind the same local name **and same kind** to
  different modules are errors. Same id with different kinds (e.g. imported
  `event Capture` plus local `usecase Capture`) may coexist; use `kind::Id`.
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

When the CLI or the LSP loads a directory, it merges all `.rdra` files reachable from the
entry files into a single semantic model, resolving imports against the include paths
derived from the input layout. The include paths are the parent and the grandparent of each
entry file, after symlinks are resolved, so the CLI and the LSP resolve an import the same
way. An `rdra.toml` above the inputs is discovered and parsed, and has no effect on
resolution yet (see [Project manifest](./cli-reference.md#project-manifest)).

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

- In `contains(BucOrder, usecase::Cancel)` and `raises(usecase::Cancel, event::Cancel)`,
  the `Cancel` use case and the `Cancel` event coexist.

Use the qualifier whenever the bare id would be ambiguous after all imported modules are
merged; the compiler reports an ambiguity error otherwise.

---

## Instance Declarations

An instance declaration introduces a named element:

```
<kind> <Id> "Label"
```

- `<kind>` is one of the kinds in the [kind index](#kind-index).
- `<Id>` is a unique identifier within its kind (an upper-camel-case identifier by
  convention).
- `"Label"` is a human-readable display string used in generated diagrams and lists.

An instance may also carry an optional description:

```
<kind> <Id> "Label" description "Description text"
```

`description` is stored as element metadata for all instance kinds, including
`business`, `buc`, `usecase`, `system`, and `api`. Diagram emitters use labels by
default so diagrams stay compact. `rdra-ish diagram --kind rdra` writes each description as
a `%% desc <id>: <text>` comment line after the node.

### Kind index

| kind | Description | Reviewed by |
|---|---|---|
| `requirement` | A requirement that motivates business use cases. | `lint` `requirement-untraced`; RDRA and requirement diagrams; `list --kind requirement` |
| `adr` | An architecture decision record. It captures context, selected and rejected options, reasons, and consequences, then links the decision to impacted model elements with `decides`. | no diagram draws it, and the `omitted:` line of a diagram that draws what it decides names `adr`; `list --kind adr`, `list --kind adr-impact` |
| `nfr` | A non-functional requirement with measurable or reviewable quality metadata such as latency, SLO, availability, resilience, audit, retention, or privacy classification. | `lint` `nfr-unscoped`; RDRA and requirement diagrams |
| `quality` | A quality attribute category such as performance, availability, resilience, auditability, observability, or privacy. | RDRA and requirement diagrams (`qualifies`) |
| `constraint` | A non-functional guardrail or compliance constraint, commonly used for audit, logging, retention, privacy, or operational restrictions. | `lint` `constraint-unscoped`; RDRA and requirement diagrams |
| `business` | A business (a coarse-grained area of activity) that BUCs belong to. | RDRA and business diagrams |
| `buc` | A business use case: a unit of business value composed of use cases. | `lint` `buc-empty`; RDRA, requirement (`motivates`), business, and context (`via`) diagrams; every diagram's `--buc` scope |
| `flow` | A business flow: an ordered business narrative inside a BUC. A flow is composed of steps and can make UC and event ordering explicit without turning the diagram into the only source of truth. | `lint` `flow-empty`, `flow-cycle`; RDRA and flow diagrams |
| `step` | A business flow step. A step is a business-language action that can cover a use case, API, or event while remaining distinct from implementation boundaries. | `lint` `step-no-cover`; RDRA and flow diagrams (a lane per actor that `performs` steps) |
| `timer` | An RDRA タイマー: a schedule that starts a use case, such as a nightly sweep. See [Timers](#timers). | `triggers(Timer, UseCase)`; RDRA (external-environment layer), usecase, flow (`covers`), and event diagrams; `lint` `orphan-node` |
| `medium` | A physical medium, device, terminal, or operation medium for Business-BUC context. | `requires_medium` in `csv --kind screen-constraints`; RDRA diagram node. `belongs(...).by(...)` records it, but no output reads that context yet. |
| `actor` | A human actor who performs use cases. | `check` permission paths; RDRA, context, business, and usecase diagrams; `csv --kind actor-permission-audit` |
| `usecase` | A use case: a concrete interaction that reads and writes entities and displays screens. | `check`; `states`; RDRA, usecase (UC複合図), flow, condition, requirement, state (transition labels), and event diagrams; `csv --kind matrix` |
| `screen` | A screen (UI surface) displayed by use cases. | `lint` `screen-no-fields`; RDRA and usecase diagrams; `csv --kind screen-constraints` |
| `field` | A screen field: a first-class UI input or output item. It can carry editability, requiredness, and actor or system source metadata, and can map to an entity column without becoming one. | `lint` `field-unmapped`, `field-unplaced`; `csv --kind business-inputs` |
| `event` | A domain event, the interface between use cases and between use cases and external systems, and a `raises` and `triggers` endpoint. | `check` raised-but-unconsumed; `states`; RDRA, usecase, flow, and event diagrams, and the context and business edge labels; `export --kind asyncapi`, `tla` |
| `system` | An internal system boundary. It groups APIs; its entity set is derived from those APIs' CRUD targets. | `lint` `stage-readiness` (its `system-boundary` stage needs a `system` and an `api`); `check` boundary diagnostics; RDRA and context diagrams (a context edge goes to the system containing an API its use cases invoke); `csv --kind api-matrix` |
| `extsystem` | An external system. It reaches use cases through events: `raises(ExtSystem, Event)` for inbound, `triggers(Event, ExtSystem)` for outbound. See [External systems](#external-systems). | RDRA (system-value layer), context, business, usecase, and event diagrams; `export --kind asyncapi` (publisher or subscriber); `check` raised-but-unconsumed; `lint` `orphan-node` |
| `api` | An API layer endpoint invoked by a use case. It operates entities on behalf of the use case, defines an atomic data operation boundary, and may carry HTTP contract metadata. | `lint` `api-unused`, `api-contract-missing`, `api-contract-incomplete`; `check` API diagnostics; RDRA, flow, condition, and requirement diagrams (the usecase diagram folds it into its use case); `csv --kind api-matrix`; `export --kind openapi` |
| `dto` | A request, response, or error payload shape used by API contracts. DTOs may carry a column-like field body but are kept separate from database entities. | `lint` `dto-unused`, `naming-dto-field`; `export --kind openapi`, `json-schema` |
| `permission` | A permission or role-like authority assignable to actors. | `check` `requires_permission` against `has_permission`; `csv --kind actor-permission-audit` |
| `entity` | A logical data model: a table-like persistent structure. | `check`; `states`; RDRA, usecase, information, requirement, and ER diagrams; `csv --kind matrix`; `export --kind dbml`, `json-schema`, `tla`; `lint` `enum-attribute` |
| `variation` | A closed set of values an actor chooses from (an RDRA バリエーション), such as a payment method. It types attribute columns of entities and DTOs and is never a lifecycle. See [Variations](#variations). | `lint` `orphan-node` (a variation that types no column); `check` `UnknownColumnType`, `InvalidVariationValue`; ER diagram and `csv --kind entity` (the column type); information diagram; condition diagram (with its values); `export --kind dbml`, `json-schema`, `openapi`; `list --kind variation` |
| `condition` | An RDRA 条件: the combination of variations and a state model that selects a business rule, such as "the visit is covered by insurance". It carries no body for traceability only, or a `holds` body that guards the use cases it applies to. See [Conditions](#conditions). | `combines`, `holds`, `applies_to(Condition, UseCase \| Api \| System)`; `lint` `condition-no-rdra-anchor`, `orphan-node`; `check` `ConditionCombinesWithBody`, `ConditionSecondBody`, `ConditionCombinesAttribute`, `AmbiguousCombinesTarget`; RDRA diagram (system layer, edges to variation, state model, and use case); condition and usecase diagrams; `list --kind condition`; `states`, `check`, `export --kind tla`, and `verify` read a body as a guard |

---

## Relationship Predicates

Predicates connect declared instances. Arguments are referenced by their `<Id>`. When
an id is ambiguous across kinds (for example an `event` and a `usecase` share a name),
qualify the argument with a kind prefix — see [Kind-Qualified References](#kind-qualified-references).

| Predicate | Signature | Semantics |
|---|---|---|
| `performs` | `(Actor, UseCase \| Buc \| Step)` | The actor performs the given use case, BUC, or business step. |
| `invokes` | `UseCase == Api` | The use case invokes the API layer. The API's writes form one transaction group of the use case. |
| `request` | `Api == Dto` | The API accepts the DTO as a request payload. |
| `response` | `Api == Dto` | The API returns the DTO as a successful response payload. |
| `error_response` | `Api == Dto` | The API may return the DTO as an error payload. |
| `applies_to` | `(Nfr \| Condition, UseCase \| Api \| System)` | The non-functional requirement or condition applies to the use case, API, or system boundary. Use this for performance, availability, or SLO requirements tied to an operation or system, and for the rule a [condition](#conditions) selects on a use case. |
| `combines` | `(Condition, Variation \| Entity.column)` | Links the condition to a variation or to the lifecycle column (a column some `transitions` moves) of an entity: the 条件 sheet's バリエーション and 状態モデル columns. Not allowed on a condition that has a `holds` body. |
| `holds` | `(Condition, comparison, ...)` | The condition's body: a comma list of comparisons, an implicit conjunction, each on a qualified column such as `Payment.method == insurance`. At most one per condition. |
| `qualifies` | `(Nfr \| Constraint, Quality)` | The non-functional requirement or constraint belongs to the given quality category. |
| `constrains` | `(Constraint, UseCase \| Api \| System \| Entity \| Dto)` | The constraint applies to the target model element. Use this for audit/logging/retention/privacy rules that constrain system behavior or data handling. |
| `reads` | `(UseCase \| Api, Entity)` | The use case or API reads the entity. |
| `writes` | `(UseCase \| Api, Entity)` | The use case or API writes the entity (treated as an update for state derivation). |
| `creates` | `(UseCase \| Api, Entity)` | The use case or API creates the entity. Seeds the initial state pattern. |
| `updates` | `(UseCase \| Api, Entity)` | The use case or API updates the entity. Produces a transition operation in state derivation. |
| `deletes` | `(UseCase \| Api, Entity)` | The use case or API deletes the entity. Marks the source pattern terminal. |
| `displays` | `UseCase == Screen` | The use case displays the screen. |
| `shows` | `Screen == Entity` | The screen shows information from the entity. |
| `maps_field` | `(Field, Entity, "column")` | Maps a screen field to a specific Entity column. Use with `contains(Screen, Field)` when screen-item level input/output mapping matters. |
| `raises` | `(UseCase \| ExtSystem, Event)` | The use case or external system raises the event, which `triggers` hands to downstream use cases or BUCs. An external system raising an event is the inbound direction. |
| `triggers` | `(Event \| Timer, UseCase \| Buc \| ExtSystem)` | The event or timer triggers a concrete use case or starts a BUC boundary. An event triggering an external system delivers the event to it (outbound). A timer cannot trigger an external system: the legal pairs are `Event` to any of the three and `Timer` to `UseCase` or `Buc`. |
| `outbox` | `(Event)` | Marks a raised event as intentionally published outside the local model, suppressing the raised-but-unconsumed warning. |
| `contains` | `(Buc, UseCase \| Flow)`, `Flow == Step`, `Screen == Field`, or `System == Api` | The use case or flow composes the BUC, the step composes the flow, the field belongs to the screen, or the API belongs to the system boundary. |
| `owns` | `System == Entity` | Optionally declares that a system is responsible for an entity even before API operations are fully modeled. API CRUD still records concrete access; `owns` records intended ownership. |
| `exposes` | `(System, UseCase \| Event \| Entity)` | Publishes the element to other models. A use case takes `.caller(actor \| system \| any)`, default `actor`. An entity takes `.append()` or `.upsert()` and becomes a published entity; without a mode it is `InternalEntityExposed`. See [Publishing to other models](#publishing-to-other-models). |
| `shares` | `System == Entity` | Lets other models name an internal entity directly. The owner's `check` reports `SharedInternalEntity` (warning) for as long as the line stays. |
| `precedes` | `Step == Step` | The first business step normally occurs before the second. |
| `branches` | `Step == Step` | The first step may branch to the second as an alternate path. |
| `excepts` | `Step == Step` | The first step may route to the second as an exception path. |
| `repeats` | `Step == Step` | The first step may loop back to the second. |
| `covers` | `(Step, UseCase \| Api \| Event \| Timer)` | The business step covers the referenced use case, API, event, or timer. Use this to make UC ordering and event chains explicit in the DSL while keeping the step vocabulary business-facing. |
| `compensates` | `UseCase == UseCase` | The first use case compensates for or rolls back the business effect of the second. Use this for alternative or exception flow review without forcing compensation into CRUD/state derivation. |
| `coordinates` | `(UseCase, Entity, Entity)` | The use case coordinates consistency for a relation crossing system boundaries. The use case must invoke APIs on both system sides that operate the corresponding entities. |
| `belongs` | `Buc == Business` | The BUC belongs to the business. |
| `has_permission` | `Actor == Permission` | The actor has the permission type. This provides a base vocabulary for later UC/API permission constraints. |
| `requires_permission` | `(UseCase \| Api, Permission)` | The use case or API requires the permission type. |
| `requires_medium` | `(UseCase \| Api, Medium)` | The use case or API requires the operation medium. |
| `motivates` | `Requirement == Buc` | The requirement motivates the BUC. |
| `decides` | `(Adr, Buc \| UseCase \| Api \| System \| Entity \| Requirement \| Nfr \| Constraint \| Dto)` | The ADR records a design decision that affects the target element. Use this for impact review and design traceability without making the target depend on a document file path. |
| `relate` | `(Entity, Entity, Card)` | Declares an ER relationship and auto-generates the FK columns. `Card` is one of `1:1`, `1:N`, `N:1`, `N:M` (unquoted). The child side, the `N` of `N:1` / `1:N` or the left of `1:1`, may be a range `lo..hi` with `lo` 0 or 1 and `hi` 1 or `*`: `relate(OrderLine, Order, 1..*:1)` says every `Order` has at least one `OrderLine`, and the lifecycle checks it as the invariant `MinCard_<Child>_<role>`. A range on the parent side is an error; write `.optional()` for a parent that may be absent. For `N:M`, require `.via(JunctionEntity)` where the junction is distinct from both ends: the model keeps the logical N:M edge **and synthesizes** two `N:1` edges `Junction→A` / `Junction→B` (FK columns, composite unique on the FK pair when missing, TLA owner links, ER crowfoot, `.along` paths). Equivalent manual form: two explicit `N:1` relates from the junction. For `1:1` the left end is the child: it holds the key column, marked unique. FK options can be chained with `.optional()`, `.on_delete(action)`, and `.on_update(action)` (for `N:M`, options apply to both generated FKs). An `.optional()` relate lets the child exist unlinked in `states` and in the TLA+ export; a mandatory relate does not. |
| `transitions` | `(Entity.column, UseCase, from -> to)` | A lifecycle edge on an Enum column: the use case moves the entity from the first variant to the second. |
| `after` | `(UseCase).assert(...)` | A temporal anchor assertion: a predicate on the state that each step of the use case leaves. |
| `when` | `(conditions...).none/has(conditions...)` | A to-many quantifier constraint (replaces `forbidden_when`). Prefer qualified columns such as `Cert.status == revoked`. |
| `sets` | `(UseCase, Entity, col == val)` | An explicit column effect, read by state derivation. Use `present` (not PostgreSQL type names) for nullable non-null. |
| `sets` | `(UseCase, Entity, col op rhs, true \| false)` | Drives the truth value of a comparison proposition (a derived Bool axis) in state derivation. |

`relate(A, B, N:1)` generates a key column on `A` named `{role}_{parent key column}`.
With the default role (the parent id in lowercase) and a parent key `id`, that is `b_id`.
`relate(A, B, 1:1)` generates the same column on `A`, marked unique, and
`relate(A, B, 1:N)` generates it on `B`. The column is nullable only under `.optional()`.
With `.optional().on_delete(set_null).on_update(cascade)`, the generated column is
nullable, marked `fk_optional`, and carries the delete/update actions for CSV/list
review. An action other than `cascade`, `restrict`, `set_null`, `set_default`, or
`no_action` is `UnknownRefAction`. [Keys and roles](#relate-and-fk-generation) covers
`.role`, `.key`, and composite parent keys.

### Chain methods

A chain method is a `.name(...)` call after a predicate. Each predicate accepts
only the methods below, and any other name is an `unknown chain method` error
(this includes the removed `.where(...)` and `.when(...)` on `belongs`).

| Predicate | Chain methods |
|---|---|
| `belongs` | `.by(...)` |
| `relate` | `.via(...)`, `.optional()`, `.on_delete(...)`, `.on_update(...)`, `.role(...)`, `.key(...)` |
| `forbidden` | `.along(...)`, `.by(...)` |
| `invariant` | `.when(...)`, `.then(...)`, `.has(...)`, `.none(...)`, `.along(...)`, `.by(...)` |
| `when` | `.has(...)`, `.none(...)` |
| `after` | `.assert(...)` |
| `exposes` | `.append()`, `.upsert()` (an entity), `.caller(actor \| system \| any)` (a use case) |
| every other predicate (including `combines` and `holds`) | none |

---

## Requirements and Traceability

These kinds record why the rest of the model exists. `motivates(Requirement, Buc)`
ties a requirement to business value, `decides(Adr, ...)` ties a design decision to the
elements it shapes, and `applies_to` / `constrains` scope quality objectives and
guardrails to use cases, APIs, systems, and data.

### Requirements

<!-- derived-from #relationship-predicates -->

`requirement` declarations may carry requirement-specific metadata after the label or
description:

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

---

## Business Layer

A `business` is a coarse area of activity. A `buc` is one slice of business value
inside it, placed with `belongs(Buc, Business)` and composed with
`contains(Buc, UseCase)`. Flows and steps make the order inside a BUC reviewable, and
`triggers(Event, Buc)` hands work from one BUC to the next. See
[RDRA-ish Interpretation](./rdra-ish-interpretation.md) for how BUC, business flow, and
use case divide the review.

### `belongs` context

`belongs(Buc, Business)` declares the Business area a BUC belongs to. The mapping can
also carry optional By context with a method-chain clause:

```
medium FrontDeskTerminal "Front Desk Terminal"

belongs(BucAppointmentScheduling, ClinicOps)
  .by(FrontDeskTerminal)
  .by("tablet")
```

- `.by(...)` records the physical medium, device, terminal, or other operation medium.
- Each argument may be a string literal or a reference to a `medium`: `.by(medium::...)`.
- Multiple `.by(...)` clauses accumulate as alternative context values.
- `.when(...)` was removed with `timing`. A schedule that starts work is a `timer`; see
  [Timers](#timers).

### Timers

A `timer` is an RDRA タイマー: a schedule that starts a use case. Connect it with
`triggers(Timer, UseCase)` (or `triggers(Timer, Buc)`) and declare the use case in a BUC
like any other:

```rdra
timer IntakeExpiryCheck "Nightly intake expiry check"
triggers(IntakeExpiryCheck, ExpireIntakeForms)
```

A timer triggers only use cases and BUCs, never an external system. A use case that a
timer triggers and that belongs to no BUC is reported like an event-triggered one. A
`step` may anchor the timer with `covers(Step, Timer)`. A timer sits in the
external-environment layer of the RDRA diagram, and the event diagram draws it
with its `triggers` edges.

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

## Interaction Layer

A use case is the concrete interaction an actor performs. It displays screens,
invokes APIs, touches entities, and raises events. This is the layer most checks
anchor on, because it connects business intent to system effects.

### Use case conditions

Use cases can carry **review-facing metadata** (not machine-checked contracts):

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
stored on the use case and shown by `rdra-ish list --kind usecase`. They are **not**
evaluated by `check`, `states`, or TLA+/TLC. For machine-checked postconditions use
`after(UseCase).assert(...)`. Use `compensates(RefundPayment, CapturePayment)` to make
a compensation relationship explicit between use cases.

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

### `triggers` and event-driven BUC chaining

`triggers(event::E, buc::B)` declares that an event starts a downstream BUC. This is the
standard abstract form when the business handoff is known but the concrete entry use
case is not yet fixed, or when a BUC may later be changed from human-initiated to
event-initiated without reshaping the BUC itself.

`triggers(event::E, usecase::UC)` declares that an event causes a concrete downstream
use case to execute. This is the refined form when the entry action is known. The tool
validates that a triggered use case belongs to at least one BUC via `contains`; a warning
is emitted otherwise. Use `diagram --kind event` to visualise the full
`raises -> Event -> triggers` chain, and `diagram --kind state` for the transitions the
triggered use cases drive. The order of `raises`
and `triggers` between use cases adds no guard to state derivation: a use case is limited
only by the states it requires and by the guards of its linked conditions.

`outbox(event::E)` declares that a raised event is intentionally published outside the
local model boundary, such as a domain event sent to an outbox, external subscriber, or
manual downstream process. It suppresses only the warning for a raised event that has no
local `transitions` or `triggers` consumer; it does not make an unraised event count as
raised.

---

## System Boundary

### External systems

An `extsystem` talks to use cases through events, as RDRA requires. There is no direct
actor link.

```rdra
extsystem LabSystem "Laboratory System"
extsystem MessagingGateway "Messaging Gateway"

raises(LabSystem, EvLabResultReceived)               // inbound: the system emits
triggers(EvLabResultReceived, ReceiveLabResult)
triggers(EvAppointmentScheduled, MessagingGateway)   // outbound: the event is delivered to it
```

An event delivered to an external system counts as consumed, so the raised-but-unconsumed
warning stays quiet. `export --kind asyncapi` lists the raising systems as the channel's
`x-publisher` and the receiving ones as `x-subscriber`. The event diagram draws the
systems and a `triggers` edge from the event to each. `lint` `rdra-extsystem-no-buc` names an external system that no
event in a BUC reaches.

APIs are the operation boundary between a use case and the data it changes. Systems
group APIs, DTOs give them payload contracts, and permissions and media say who may
reach them from where.

### Publishing to other models

A model owned by one team publishes what another team's model may reference. Everything
else stays internal: no other model can name it.

```rdra
system AuthSystem "Authentication System"
usecase Login "Log in"
usecase Revoke "Revoke a session"            // internal
api SessionApi "Session API" method POST path "/sessions"
contains(AuthSystem, SessionApi)
invokes(Login, SessionApi)

entity Session "Session" {                   // internal
  id: Int @pk
  status: Enum(active, revoked) @default(active)
}
creates(SessionApi, Session)
transitions(Session.status, Revoke, active -> revoked)

// Published information: a projection of Session that auth writes and never reads.
entity SessionView "Session (published)" {
  session_id: Int @pk
  status: Enum(active, ended)
}
writes(SessionApi, SessionView)
sets(Login, SessionView, status == active)
writes(Revoke, SessionView)
sets(Revoke, SessionView, status == ended)

exposes(AuthSystem, Login)                   // callable on behalf of an actor
exposes(AuthSystem, SessionView).upsert()
```

`exposes(System, UseCase)` publishes a use case. `.caller(actor)`, the default, means
another model calls it on behalf of a human actor; `.caller(system)` means only a service
identity may call it; `.caller(any)` allows both. A use case exposed with `.caller(system)`
or `.caller(any)` is not reported as having no actor (`UseCasePermissionNoActor`), because
its caller is the other model's service.

`exposes(System, Event)` publishes an event another model may react to.

`exposes(System, Entity).append()` or `.upsert()` publishes an entity. A published entity
is a projection the owner writes and never reads, such as an append-only log or a silver
table. `.append()` allows `creates` and `sets`; `.upsert()` also allows `updates` and
`writes`. `deletes` and `transitions` are never allowed. The owner's `check` enforces
these rules:

| Diagnostic | When |
|---|---|
| `PublishedEntityMutated` | an `append` entity gets `updates` or `writes`, or any published entity gets `deletes` or `transitions` |
| `PublishedEntityReadByOwner` | the owner reads a published entity: `reads`, `shows`, `maps_field`, a `holds` body, `combines`, or a constraint (`forbidden`, `invariant`, `required`, `exclusive`, `when`, `after`) |
| `PublishedEntityKeysInternal` | a `relate` joins a published entity to an internal one, in either direction |
| `PublishedNotInSourceTx` | a use case writes a published entity but makes no internal change, or makes several internal transactions, so the published row has no one transaction to join |
| `InternalEntityExposed` | `exposes(System, Entity)` without `.append()` or `.upsert()`; publish a projection instead |
| `PublishedEntityNeverWritten` (warning) | no use case writes the published entity |
| `DuplicateExposure` | a second `exposes` line for an element that is already exposed; the first line stays, the second is the error |
| `SharedEntityAlsoExposed` | `shares(System, Entity)` on a published entity; an entity has one role |
| `ExposesChainInvalid` | a chain method on the wrong kind, such as `.append()` on a use case or `.caller(...)` on an entity |
| `ExposesCallerInvalid` | `.caller(...)` with an argument other than `actor`, `system`, or `any` |

A published row is written in the transaction of the internal change it reports. A write
to a published entity therefore joins the writing use case's one internal transaction
group, whether or not a key links them, and `SeparateTxInferred` never fires for it. In
the example, `Revoke` moves `Session` and writes `SessionView` in one action, where two
unrelated writes would otherwise be `Revoke_1` and `Revoke_2`. The owner's lifecycle
(`states`, `check`, `export --kind tla`) skips published entities: they hold no state of
their own, only what the owner wrote.

`shares(System, Entity)` lets other models name an internal entity directly, at the cost
of a `SharedInternalEntity` warning on every `check` for as long as the line stays. A
second `shares` of the same entity adds nothing and no second warning.

`rdra-ish list <model> --kind surface` prints the published surface as canonical text,
one line per exposed use case, exposed event, published entity, writer of a published
entity, and shared entity, sorted by kind and id. See [`list`](./cli-reference.md#list).

### `api` and the API layer

A use case's actors are its direct `performs(Actor, UseCase)` actors together with the
actors of every BUC that contains it (`performs(Actor, Buc)`). The permission check
requires each of them to hold the
use case's and its APIs' permissions. So `performs(Actor, Buc)` says the actor may run
every use case of the BUC; an actor who runs only some of them performs those use cases
directly instead.

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

**Transactions**:
- The writes of an API a use case invokes form one atomic group, the API boundary.
  Direct use-case writes group by FK connectivity, and a write with no FK link to the
  others runs as its own transaction (`SeparateTxInferred` warning from `check`).
- If an API is invoked by multiple use cases, the API's CRUD boundary applies to each
  invoking use case. Split read-only and write APIs when those operations have different
  consistency contracts.
- FK-grouped direct writes use the full `relate` graph for connectivity. Two written
  sibling entities that share an unwritten parent entity are still grouped as one
  inferred transaction.
- `states` and `verify` run these transactions (`UsecaseTx::groups()`). A `writes(..)`
  edge, and an entity a use case only `sets` or transitions, both count as an update. An
  entity written in its own transaction after an FK group raises `SeparateTxInferred`.

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
- `api` nodes are also omitted from the event diagram (`--kind event`),
  because an API is not a participant in the raises/triggers causal chain.

**CSV/list output**:
- `list --kind api` — id/label table of all declared apis.
- `list --kind system` — id/label table of all declared systems.
- `csv --kind api` — CSV file of api id and label.
- `csv --kind api-matrix` — api × entity CRUD matrix.

**Diagnostics** (emitted as warnings by `check`):
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
operations, with `{path}` parameters, auth scheme mapping (unknown schemes via
`x-rdra-ish-auth-scheme`), `requires_permission` as
`x-rdra-ish-requires-permission`, and DTOs as `components.schemas`.
Use `rdra-ish export --kind json-schema` when the payload shapes themselves need
to be handed to tooling independently of an OpenAPI operation document; DTOs are
emitted under `$defs` using `Dto.<id>` names.
Use `rdra-ish export --kind asyncapi` when event causality needs a machine-readable
catalog: events become AsyncAPI channels/messages, `raises` and `outbox` become
`send` operations, and `triggers` / `transitions` become `receive` operations.
Protocol, server, and exact event payload schemas are intentionally not inferred
until the DSL has explicit vocabulary for them.

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

---

## Data Layer

`entity` is the logical data model. Its body declares columns, and `relate` declares
relationships and generates foreign keys. Inline Enum, Bool, and nullable columns are
the state axes that the lifecycle layer reasons about. A column whose value an actor
chooses, not a use case, is typed by a [`variation`](#variations) instead. Such a column
is an axis only when a `sets`, a constraint, a condition body, an `after` assertion, or a
`property` names it.

### Entity Bodies

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

#### Column types

| Type | Description |
|---|---|
| `Int` | Integer. |
| `String` | Text. |
| `Money` | Monetary amount. |
| `DateTime` | Date and time. |
| `Date` | Date. |
| `Bool` | Boolean. A boolean column is a **state axis** (see [state-derivation.md](./state-derivation.md)). |
| `Decimal` | Decimal number. |
| `Enum(a, b, c)` | Inline value set. An Enum column is a **state axis** and can be linked to a state machine. Use it for a lifecycle column or a value that use cases set. `lint` `enum-attribute` warns on an inline Enum that no `transitions` and no `sets` moves, because an actor chooses its value: type it by a variation. |
| `<VariationId>` | A [variation](#variations) in type position, such as `method: PaymentMethod`. The column holds one of the variation's values. It is an **attribute**: not a state axis, never a lifecycle column. An id that names no variation is `UnknownColumnType`. |

#### Column annotations

| Annotation | Description |
|---|---|
| `@pk` | Marks the column as the primary key. FK columns are auto-generated against this key by `relate`. |
| `@pk(a, b)` | Declares a composite primary key over the named columns. A `relate` to the entity generates one FK column per key column. |
| `@unique` | Adds a unique constraint. |
| `@unique(a, b)` | Declares a composite unique constraint over the named columns. |
| `@index` | Adds an index for the column. |
| `@index(a, b)` | Declares a composite index over the named columns. |
| `@check(expr)` | A check constraint: `expr` is a condition over the entity's own columns (`and`, `or`, `not` over comparisons), typed like a `forbidden` comparison. A missing column is `UnknownColumn`; arithmetic is `ArithNotInCheck`. The lifecycle checks it as the invariant `Check_<Entity>_<column>_<n>` (see [state-derivation.md](./state-derivation.md#checks)). |
| `@null` | Makes the column nullable. A nullable column is a **state axis** with values `{null, present}`. |
| `@default(v)` | Declares a default value `v`. Used as the seed value in state derivation. |
| `@label("...")` | Overrides the column's display label. |
| `@soft_delete` | Marks the column as the soft-delete marker, such as `deleted_at` or `is_deleted`. |
| `@history` | Marks the column as part of history/versioning, such as `valid_from`, `valid_to`, or `version`. |
| `@tenant` | Marks the column as the tenant-scope discriminator. |
| `@derived(expr)` | Derives the column from the entity's own columns: a condition for a Bool column (`@derived(qty >= 3)`), or one `+`, `-`, or `*` over a column or an integer for an Int, Money, or Decimal column (`@derived(total - discount)`). A non-numeric operand is `ArithTypeMismatch`, and an expression that cannot derive the column's type is `DerivedTypeMismatch`. A derived column takes no `@check`, `@default`, `@pk`, or `@unique` (`DerivedColumnAnnotation`), and a `sets` on it is `DerivedColumnWritten`. |

Composite `@pk(...)`, `@unique(...)`, and `@index(...)` annotations are written on
one column line for locality, but semantically they belong to the entity as a whole.
`@check(...)` and `@derived(...)` take an unquoted expression. The quoted form of earlier
versions (`@check("qty >= 1")`) is a parse error whose message is the unquoted rewrite.
A comparison in `@check` has a column, an integer, or `now` on each side. An Enum value
such as `status == open` reads `open` as a column name and fails with `right-hand side
column 'open' not found`. DBML notes, JSON Schema `x-rdra-ish-check-constraints` /
`x-rdra-ish-derived`, and the entity CSVs print the expression with bare column names:
`rdra-ish fmt` keeps a written `Order.qty` qualifier, and the emitters print `qty`.
`rdra-ish export --kind dbml` projects logical `entity` declarations to DBML tables,
including indexes, composite unique constraints, generated FK columns, and `relate`
delete/update options. Metadata without a native DBML equivalent is preserved as
notes so the review signal is not lost.
`rdra-ish export --kind json-schema` also emits Entity schemas under `$defs` using
`Entity.<id>` names, with RDRA-specific data-modeling metadata preserved as
`x-rdra-ish-*` extensions.

Both exports print every key from the same fact, so they agree on it. DBML prints one
`Ref` per key at its first column, with the composite form `Ref: Stock.(bin_site,
bin_shelf) > Bin.(site, shelf)`. JSON Schema marks each key column with
`x-rdra-ish-foreign-key` and `x-rdra-ish-fk-target`, adds `x-rdra-ish-fk-target-column`
to the columns of a composite key, `x-rdra-ish-fk-role` to a key with an explicit
`.role`, and `x-rdra-ish-fk-min-children: 1` to the child column of a key whose card has
a minimum of 1. The Mermaid ER diagram draws that child end as one-or-more.
DBML and the entity CSVs have no notation for a minimum.

### Variations

A `variation` declares the values an actor chooses from. RDRA keeps these apart from
the state model: a payment method is picked, an order status is driven by use cases.

```rdra
variation PaymentMethod "Payment Method" {
  cash       "Cash"
  card       "Card"
  insurance  "Insurance"
  adjustment             // the label is optional
}

entity PaymentTransaction "Payment Transaction" {
  id:     Int           @pk
  method: PaymentMethod @default(card)
  status: Enum(paypending, payposted) @default(paypending)
}
```

The body lists at least one value; a value is an identifier with an optional label,
and values are unique within the variation. Any entity or DTO column can name the
variation as its type, and `@default(v)` must be one of its values (otherwise
`InvalidVariationValue`). A `sets` value on such a column is checked against the same
list. The type name resolves like a reference: through the file's imports, so a
variation declared in `shared/entities.rdra` is visible wherever that module is
imported.

What a variation column does differently from an inline `Enum`:

| Output | Inline `Enum` column | Variation-typed column |
|---|---|---|
| `states` | One axis; unreached variants are reported unreachable | No axis, no reachability check |
| `transitions` | Allowed | Error (`TransitionsColumnNotEnum`): an attribute has no lifecycle |
| TLA+ export | Status column of the lifecycle | An axis only when a `sets`, a constraint, an `after` assertion, or a `property` names the column; `Init` is a free choice over every value, so TLC checks them all |
| `export --kind dbml` | One `Enum <Entity>_<column>` per column | One `Enum <VariationId>` shared by every column typed by it |
| `export --kind json-schema`, `openapi` | `enum` of the variants | `enum` of the variation's values |
| ER diagram, `csv --kind entity`, `list --kind entity` | Type `Enum` | Type is the variation id |

### Conditions

<!-- derived-from #variations -->

A `condition` is an RDRA 条件: a combination of variations and a state model, tied to the
use cases it applies to. It has two forms, and a condition uses exactly one.

```rdra
// Traceability only (RDRA requirements level): the links are declared.
condition CopayCappedAtLimit "窓口負担は負担割合で計算し、限度額情報があれば限度額までとする"
combines(CopayCappedAtLimit, BurdenRatio)            // 条件 to バリエーション
combines(CopayCappedAtLimit, Appointment.status)     // 条件 to 状態モデル (a lifecycle column)
applies_to(CopayCappedAtLimit, CalculateVisitBill)   // UC to 条件

// With a body: the links are derived from the columns the body names.
condition UnreferredFirstVisit "紹介状なしの初診で、除外に当たらない"
holds(UnreferredFirstVisit, Charge.kind == selectedtreatment, Appointment.referral == unreferred)
applies_to(UnreferredFirstVisit, ApplySelectedTreatmentFee)
```

The second argument of `combines` is a variation or `Entity.column`. A one-part name is a
variation. A two-part name is tried as a variation through the imports first (`shared.BurdenRatio`)
and as an entity column second; when both match it is `AmbiguousCombinesTarget`, and
`variation::Name` picks the variation. The column must be a lifecycle column, one that some
`transitions` moves, wherever in the files that `transitions` sits; an attribute is
`ConditionCombinesAttribute`.

The body is a comma list of comparisons, the same form `forbidden` takes, so it is an
implicit conjunction. Each operand names its entity, and `check` validates the entity,
the column, and the value (`InvalidVariationValue` or `InvalidEnumVariant`). Disjunction and
negation are not available yet. A second `holds` on one condition is `ConditionSecondBody`, and
`combines` together with `holds` is `ConditionCombinesWithBody`, in either order.

One function, `condition_edges`, gives a condition's links. With a body, an operand on a
variation-typed column links the variation and an operand on a lifecycle column links the state
model. Without a body, the `combines` declarations are the links. `list --kind condition`,
the RDRA diagram, and `lint` all read it, so a body and its links cannot disagree. A body that
names neither a variation column nor a lifecycle column is `lint` `condition-no-rdra-anchor`.

A body with `applies_to(Condition, UseCase)` is a guard on that use case: the use case runs
only in a state where every comparison holds. `applies_to(Condition, Api)` guards each use
case that invokes the API, and `applies_to(Condition, System)` adds no guard. `states`,
`check`, and the TLA+ export read the same guard.

- A comparison on an entity that the use case writes reads that instance. When the use case
  creates the entity, the comparison tests the new instance after the use case runs, so
  the condition constrains the creation. `UnreferredFirstVisit` above lets
  `ApplySelectedTreatmentFee` create only a charge whose `kind` is `selectedtreatment`.
- A comparison on any other entity needs some existing instance that satisfies it.
  Comparisons on one such entity share one instance. A direct `relate` links that instance
  to the entities it relates to; without a `relate`, it is independent.
- A body applied to a use case that writes nothing has no action to guard. That is
  `UnbindableCondition`, an error.
- A body names its columns, so a column the body reads that no use case writes draws the
  `UnwrittenConstrainedVar` warning. A condition with no body is a trace link and guards
  nothing.

### RDRA lint rules

<!-- derived-from #conditions -->
<!-- constrained-by ./rdra-ish-interpretation.md#the-rdra-sheets-不整合-columns -->

The RDRA definition sheet has 15 不整合 columns. Each is a check that something is either
referenced but undefined, or defined but connected to no BUC. `crates/rdra-ish-core/src/rdra_rules.rs`
holds them as one table, `RDRA_RULES`, with the sheet column, the mechanism that enforces
the row, a hint, and a witness model that a unit test loads to prove the row fires. Adding a rule is
adding a row.

| Mechanism | What it checks | Where it shows |
|---|---|---|
| `Resolver` | The name is referenced but undefined. A language with symbols already refuses it, so the row names the diagnostic that covers the column. | `check` and `lint` error |
| `BucReach` | Every element of the row's kind is in the forward closure of some BUC, or is the `from` of a relation into that closure (an actor that performs a use case, a condition that applies to one, a timer that triggers one). A variation also counts when it types a column of such an entity. | `lint` warning |
| `TransitionUseCaseInBuc` | Every use case that a `transitions` names is in a BUC's closure. | `lint` warning |

The closure is `reachable_from_buc`, computed once per lint run over every BUC. A kind that a
`BucReach` row owns is no longer reported by `orphan-node`, so one element draws one finding.

| Rule id | Sheet | Column | Mechanism | Fix |
|---|---|---|---|---|
| `rdra-buc-actor-undefined` | BUC | アクター | Resolver `UndefinedSymbol` | declare the actor or fix the name |
| `rdra-buc-extsystem-undefined` | BUC | 外部システム | Resolver `UndefinedSymbol` | declare the extsystem or fix the name |
| `rdra-buc-information-undefined` | BUC | 情報 | Resolver `UndefinedSymbol` | declare the entity or fix the name |
| `rdra-buc-condition-undefined` | BUC | 条件 | Resolver `UndefinedSymbol` | declare the condition or fix the name |
| `rdra-info-related-undefined` | 情報 | 関連情報 | Resolver `UndefinedSymbol` | declare the related entity |
| `rdra-info-statemodel-undefined` | 情報 | 状態モデル | Resolver `UnknownColumn` | name an existing Enum column |
| `rdra-info-variation-undefined` | 情報 | バリエーション | Resolver `UnknownColumnType` | declare the variation or use an inline Enum |
| `rdra-state-uc-undefined` | 状態 | 遷移UC | Resolver `UndefinedSymbol` | declare the use case |
| `rdra-state-target-undefined` | 状態 | 遷移先状態 | Resolver `InvalidEnumVariant` | use a declared Enum variant |
| `rdra-cond-variation-undefined` | 条件 | バリエーション | Resolver `UndefinedSymbol` | declare the variation |
| `rdra-cond-statemodel-undefined` | 条件 | 状態モデル | Resolver `ConditionCombinesAttribute` | combine a lifecycle column |
| `rdra-actor-no-buc` | BUC | アクター 未接続 | `BucReach` actor | `performs(Actor, UseCase | Buc)`, or delete the actor |
| `rdra-extsystem-no-buc` | BUC | 外部システム 未接続 | `BucReach` extsystem | `raises` or `triggers` through an event a BUC uses, or delete it |
| `rdra-entity-no-buc` | BUC | 情報 未接続 | `BucReach` entity | reach it from a use case through an API, or delete it |
| `rdra-condition-no-buc` | BUC | 条件 未接続 | `BucReach` condition | `applies_to(Condition, UseCase)`, or delete it |
| `rdra-variation-no-buc` | rdra-ish | (none) | `BucReach` variation | type a column of a reached entity with it, or delete it |
| `rdra-timer-no-buc` | rdra-ish | (none) | `BucReach` timer | `triggers(Timer, UseCase | Buc)`, or delete it |
| `rdra-transition-uc-no-buc` | rdra-ish | 遷移UC は BUC シートで定義 | `TransitionUseCaseInBuc` | `contains(Buc, UseCase)` for the transitioning use case |

### `relate` and FK generation

`relate(Child, Parent, N:1)` generates a foreign-key column on the child entity
referencing the parent's primary key (for example `parent_id`). The cardinality drives
the crow's-foot notation in the ER diagram and the FK-induced graph used for
[transaction boundary inference](#see-also).

Many-to-many requires an explicit junction entity:

```rdra
entity Order "Order" { id: Int @pk }
entity Product "Product" { id: Int @pk }
entity OrderProduct "Order product" { id: Int @pk }

relate(Order, Product, N:M).via(OrderProduct)
// generates OrderProduct.order_id and OrderProduct.product_id
// adds composite unique (order_id, product_id) when missing
// and synthesizes relate(OrderProduct, Order, N:1) + relate(OrderProduct, Product, N:1)
```

Bare `relate(A, B, N:M)` without `.via` is an error. The equivalent manual form is
still valid: `relate(Junction, A, N:1)` plus `relate(Junction, B, N:1)` (synthesis
skips edges that already exist). Self N:M (`relate(A, A, N:M)`) and `.via` equal to
either endpoint are errors.

The generated column is `{role}_{key column}` for each parent key column, where the
role defaults to the parent id in lowercase (`customeraccount_id`). A composite parent
`@pk(site, shelf)` yields `bin_site` and `bin_shelf`, printed in DBML as
`Ref: Stock.(bin_site, bin_shelf) > Bin.(site, shelf)`. The left end of a `1:1` holds the
key, marked `[unique]`, and DBML prints its ref as `Ref: Shipment.order_id - Order.id`.
Two chain methods shape the key:

- `.role(ship_to)` names the key: columns `ship_to_id`, owner `Order._owner_ship_to`.
  Two relates from one child to one parent need distinct roles, or the second is
  `DuplicateRelateRole`. A column the entity already declares under a generated name is
  `FkConflict`; name the key with `.role` or adopt the column with `.key`.
- `.key(order_ref)` adopts columns the child already declares instead of generating
  them, one per parent key column in key order and of the same type. The adopted
  columns keep their names, take the key's nullability and, for a single-column `1:1`,
  its uniqueness. They are the key in every output, and in the lifecycle they are the
  owner, not a state column.

| Diagnostic | When |
|---|---|
| `DuplicateRelateRole` | Two keys of one child have the same role. |
| `KeyArity` | `.key(...)` names a different number of columns than the parent key has. |
| `KeyColumnUndefined` | `.key(...)` names a column the child does not declare. |
| `KeyTypeMismatch` | An adopted column has a different type than the parent key column. |
| `KeyColumnShared` | An adopted column already holds another relate's key. |
| `KeyOptionOnManyToMany` | `.role` or `.key` on `relate(A, B, N:M)`. An N:M holds no key of its own, so write the junction's keys as two `N:1` relates instead. |
| `UnknownRefAction` | `.on_delete(...)` or `.on_update(...)` names an unknown action. |

A constraint's `.along(A, B)` follows the key between `A` and `B`. When two keys join
them, `.along(A, B).by(ship_to)` names the one to follow (the role, or the parent id for
a key without `.role`); without `.by` the constraint is `AmbiguousOwnerLink`. `.by` that
names no key on the path is `UnknownOwnerRole`, and `.by` without `.along` is
`ByWithoutAlong`. A quantifier (`when(...).has/none(...)`) and a condition take no `.by`
and follow the first key between the pair.

### Cardinality ranges

The child side of a card, the `N` of `N:1` and `1:N` or the left of `1:1`, accepts a
range `lo..hi` where `lo` is `0` or `1` and `hi` is `1` or `*`. A range stands for the
side it ends in: `hi` of `1` for `1`, `*` for `N`. Every card without a range keeps its
kind. A minimum of 1 means every parent has at least one child:

```rdra
relate(OrderLine, Order, 1..*:1)   // every Order has an OrderLine
relate(Shipment, Order, 1..1:1)    // every Order has exactly one Shipment
```

The lifecycle checks that minimum as the invariant `MinCard_<Child>_<role>` (see
[state-derivation.md](./state-derivation.md#checks)). A minimum of 0 is the default and
adds no check. `rdra-ish fmt` prints the card as written. One spelling serves each
side, so a parent that may have no child needs no range, and a child that may have no
parent is `.optional()`.

| Diagnostic | When |
|---|---|
| `MinCardOutOfRange` | The low end is not 0 or 1, or the high end is not 1 or `*` (`2..*:1`). |
| `MinCardOnParentSide` | A range sits on the parent side (`N:0..1`). Write that side as `1` and add `.optional()` when a child may have no parent. |
| `MinCardOnManyToMany` | A range makes the card an N:M, which holds no key of its own (`N:1..*`, `1..*:M`). Give the junction's keys the range instead. |

---

## Lifecycle

Lifecycle predicates turn interaction effects into entity states. `transitions`
declares state machine edges, `sets` declares column effects, and the constraint
declarations in [Entity State Constraints](#entity-state-constraints) say which
combinations must never or must always hold. `rdra-ish states` derives the reachable
patterns, and `export --kind tla` / `verify --backend tlc` check temporal properties.

### `transitions` and the state machine

`transitions(Order.status, CapturePay, pending -> paid)` declares one edge of an entity's
state machine, driven by the use case `CapturePay`. It is the 遷移UC column of an RDRA
state sheet: the use case moves the state, and events stay the interface between use
cases. The Enum column is required; state labels are the Enum variants
(case-insensitively). Two use cases that drive the same edge each get their own line:

```
transitions(Order.status, CancelOrder,  pending -> cancelled)
transitions(Order.status, ExpireOrder,  pending -> cancelled)
```

The second argument must be a use case. Passing an event, the earlier form, is a
`TypeMismatch` whose hint names the use case to write and `scripts/migrate-phase3.py`,
which rewrites a model to one line per use case that raises the event. See
[state-derivation.md](./state-derivation.md) for how transitions feed derivation.

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
```

The origin of `sets` is always a use case. An event origin is a `TypeMismatch` whose hint
names `scripts/migrate-phase3.py`.

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
evaluated on the Int values of its columns over `0..5` and on `now`. Change the columns
with `sets(Restock, Stock, stock == 3)` and the comparison follows. When the model does
not track the values, drive the truth value of the comparison instead with the
four-argument form of `sets`:

```
// Tell the model that Sell causes (stock < selling) to become true
sets(Sell,   Stock, stock < selling, true)

// Tell the model that Refund causes (stock < selling) to become false
sets(Refund, Stock, stock < selling, false)
```

The third argument is the bare comparison expression (no quotes); the fourth argument is
the bare boolean literal `true` or `false`. Value equality (`col == variant`) must not use
`, false` — that form is rejected (`SetsFalseOnEquals`); omit the fourth argument and set
a different value instead. Once any `sets` drives a comparison, the comparison is a Bool
variable that only those `sets` change, and the Int columns no longer define it. A use case
that drives a comparison to one truth while its own Int `sets` make it the other is
`ContradictoryDrivenComparison`, an error.

| Fourth argument | Meaning |
|---|---|
| `true` | The comparison holds after this use case / event executes. |
| `false` | The comparison does not hold after this use case / event executes. |

---

## Entity State Constraints

<!-- derived-from ./state-derivation.md#temporal-anchor-assertions -->

State constraints assert facts about the **reachable state space** of an entity. They
are evaluated on the reachable states of the lifecycle transition system: a constraint is
only reported as violated if a reachable state actually witnesses the violation.
Unreachable bad states never trigger a diagnostic.

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
holds at once. If any reachable state satisfies all conditions, a `ConstraintViolated`
diagnostic is emitted, naming the constraint (`Forbidden_Order_0`), the conditions, and
the offending state. Columns that two use cases change separately can combine in a
reachable state. When the business flow changes correlated columns together, set them in
the same use case.

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

For every reachable state that satisfies all the guards but violates any requirement,
a `ConstraintViolated` diagnostic is emitted, naming the guards, the requirements, and
the offending state.

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

If a reachable state is missing any required condition, a `ConstraintViolated`
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

If a reachable state satisfies multiple exclusive conditions, a
`ConstraintViolated` diagnostic is emitted.

**Design rationale.** Mutual exclusion can be expanded into pairwise `forbidden`
constraints, but the predicate names the business intent directly. This is useful for
nullable lifecycle timestamps, boolean flags, and comparison propositions that should
not co-occur.

### Cross-Entity Constraints

Cross-entity constraints describe rules that mention columns from more than one entity.
They are parsed, type-checked, and checked by `states` and `verify` over every
combination of the participating entities' present instances.

Use `Entity.column` when a condition points at a column. The optional leading entity
arguments declare the intended scope; when omitted, the scope is inferred from qualified
column references.

```
forbidden(Order, Payment,
  Order.status == cancelled,
  Payment.amount > Order.total)

invariant(Order, Payment)
  .when(Order.status == paid)
  .then(Payment.status == captured)

invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status == paid)
  .then(Payment.status == captured)
```

`forbidden(...)` accepts a flat AND-list of conditions. Multi-entity `invariant(...)`
uses the same implication shape as single-entity `invariant`: `.when(...)` clauses are guards and
`.then(...)` clauses are required conditions. A condition is either:

- `Entity.column == value` for state-like equality, using the same value vocabulary as
  `sets`.
- A comparison expression such as `Order.total > Payment.amount` or
  `Coupon.expires_at < now`.

When more than one entity is in scope, bare column names are rejected so the rule stays
unambiguous. With a single-entity scope, bare columns are still accepted as a shorthand.

Without `.along`, the rule holds for every pair of instances, linked or not. Adding
`.along(EntityA, EntityB, ...)` restricts it to instances connected through the listed
`relate` path. Paths may include junction entities from `relate(A, B, N:M).via(J)` — e.g.
`.along(A, J, B)` — because via synthesizes the `J→A` / `J→B` edges. Each adjacent pair
of the path needs a `relate` that binds it (N:1, 1:N, or 1:1), and the path must cover
every entity of the scope. Otherwise the rule is `UnbindableScope`, an error. An unlinked
owner (an `.optional()` relate not yet linked) is not linked for `.along`.

`after(UseCase).assert(Entity.column == value, ...)` checks an assertion at a specific
use-case boundary instead of against every reachable state. The tool checks the predicate
on the state that each step of the use case leaves. A use case whose writes span k
transaction groups is k steps, and the assertion holds after each one. Comparison forms
are also valid, for example
`after(ExecuteCertIssue).assert(CertificateOrder.status == executed)`. An `after` on a
use case that changes nothing is `AssertOnInertUseCase`.

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
`WF_vars(Next)` (fairness is on the whole `Next` disjunction — not configurable
from the CLI). Temporal formulas are quantified over the present instances of each
entity. `after(UseCase).assert(...)` becomes an independent TLA `PROPERTY`,
`[][(\A p : UC(p) => post')]_vars`, that checks the state each step leaves and is **not**
injected into the effects. Every constraint is its own `INVARIANT`; see
[formal-verification.md](./formal-verification.md).

`sets(UC, Entity, intCol == 3)` assigns an Int effect. Int, Money, Decimal, Date, and
DateTime columns that something names become `0..5` variables in `states` and in the TLA+
export (nullable ones add `null`). The export domain is a fixed approximation (default
`0..5`, override with `--int-range-max`), and an Int literal outside it is
`IntLiteralOutOfBounds`. `@default(0)` is valid on Int columns.

`when(Entity, conditions...).has/none(...)` or
`when(Entity.col == val).has/none(Related.col == val)` declares a to-many quantifier
rule. It ranges over the present instances: `none` fails in a reachable state where the
anchor condition holds and some related instance satisfies the related condition. A
`relate` between the two entities links the related instance through its owner; without
one, the related side is independent. `states` runs one instance per entity, so use
`verify --instance-count 2` for rules that need two related instances.

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

A comparison expression is evaluated on the values of its columns: Int, Money, Decimal,
Date, and DateTime columns that it names become `0..5` variables, and `now` is a clock
that ticks up to 5. Change the values with `sets(UC, Entity, col == N)`. To say that a
use case makes the comparison true or false without modeling the values, drive it
explicitly with `sets`, and the comparison becomes a Bool variable:

```
// Declare that the Sell use case causes (stock < selling) to become true
sets(Sell,   Stock, stock < selling, true)

// Declare that the Refund use case causes (stock < selling) to become false
sets(Refund, Stock, stock < selling, false)
```

A comparison that no `sets` drives is not a variable. It reads the Int values, so a
`forbidden(Stock, stock < selling)` is violated in any reachable state where `stock` is
below `selling`, and `required` and `exclusive` read the values the same way. `states`,
`check`, and TLC read it the same way.

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

## Limitations

### Unsupported or limited

| Topic | Limitation |
|---|---|
| Bare `relate(..., N:M)` without `.via` | Error. Use `relate(A, B, N:M).via(Junction)` or two explicit `N:1` relates through a junction entity. |
| `relate(A, A, N:M)` or `.via` equal to an endpoint | Error (`SelfNMRelation` / `ViaMustBeDistinct`). Junction must be a distinct entity. |
| AsyncAPI protocol / server / payload schemas | `export --kind asyncapi` projects event causality only; broker protocol, servers, and exact payload schemas are not inferred. |
| OpenAPI auth / permissions | Unknown `auth` schemes become `apiKey` + `x-rdra-ish-auth-scheme`; `requires_permission` is an extension, not OpenAPI `security`. |

### Removed kinds

These kinds had no review output and were removed. A model that still uses one fails
to load with a message naming the replacement:

| Removed | Use instead |
|---|---|
| `usagescene` | `timer` for a schedule that starts a use case, and `medium` / `.by(...)` for Business-BUC context. |
| `timing` | `timer`. `belongs(...).when(...)` was removed; connect the timer with `triggers(Timer, UseCase)`. |
| `uses` | Connect an external system through events: `raises(ExtSystem, Event)` inbound, `triggers(Event, ExtSystem)` outbound. |
| `location` | Deleted with the `.where(...)` clause of `belongs`; use `medium` / `.by(...)` for Business-BUC context. |
| `state` | Delete the declaration. States are the variants of the lifecycle Enum column named in `transitions(Entity.col, UseCase, from -> to)`; the analyzer derives one `{Entity}_{variant}` state per variant for `lint` and the diagrams, which name it `state__{Entity}_{variant}`. |
| `concept`, `domain_object`, `aggregate`, `value_object` / `valueobject`, and `maps_to` | `entity` for persistent structure, `dto` for payload shapes. |

---

## See Also

- [state-derivation.md](./state-derivation.md) — how reachable states are explored,
  how predicates become actions and guards, and how entity constraints are checked.
- [cli-reference.md](./cli-reference.md) — the `rdra-ish` command-line interface,
  including the `diagram --kind event` command for event causality visualisation.
