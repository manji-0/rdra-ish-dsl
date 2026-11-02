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
| Requirements and traceability | `goal`, `requirement`, `adr`, `nfr`, `quality`, `constraint` | Why does this exist, and which quality bar applies? | [Requirements and Traceability](#requirements-and-traceability) |
| Business | `business`, `buc`, `flow`, `step`, `timer`, `medium` | Which business value does it serve, in what order? | [Business Layer](#business-layer) |
| Interaction | `actor`, `usecase`, `screen`, `field`, `event` | Who does what, on which screen, and what does it cause? | [Interaction Layer](#interaction-layer) |
| System boundary | `system`, `extsystem`, `api`, `dto`, `permission` | Which boundary owns the operation, and who may call it? | [System Boundary](#system-boundary) |
| Data | `entity` | What is stored, and how is it related? | [Data Layer](#data-layer) |
| Lifecycle and rules | `transitions`, `sets`, `drives`, and constraint declarations | Which states are reachable, and which are forbidden? | [Lifecycle](#lifecycle), [Entity State Constraints](#entity-state-constraints) |

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
- `import <path>` brings the module's declarations into flat scope. An import names a
  module, not a file: a loaded file that declares `module <path>` answers it whatever the file
  is called, so `buc/buc_registration.rdra` with `module buc.registration` answers
  `import buc.registration.{FeePaymentScreen}`. A module no loaded file declares is an error
  (`UnknownImportModule`), whose hint lists the loaded files that declare a module like it. Two flat imports that bind the same local name **and same kind** to
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
entry files into a single semantic model. An import is resolved first by the `module` lines of
the loaded files. A file not loaded yet is found by its path: `import a.b` loads `a/b.rdra`
from the include paths derived from the input layout, so load the directory when a module's
file is named otherwise. The include paths are the parent and the grandparent of each
entry file, after symlinks are resolved, so the CLI and the LSP resolve an import the same
way. An `rdra.toml` above the inputs is discovered and parsed, and has no effect on
resolution yet (see [Project manifest](./cli-reference.md#project-manifest)).

---

## Kind-Qualified References

When an id is unambiguous, you reference an instance by its bare id. When the same id is
declared for more than one kind — a common and natural situation, e.g. an `event Cancel`
and a `usecase Cancel` — you must disambiguate the reference with a kind prefix:

```
usecase::Cancel   // the use case named Cancel
event::Cancel     // the event named Cancel
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

### Metadata clause values

A metadata clause is a keyword and one value: `description`, the requirement, ADR, NFR,
goal, and constraint clauses, the API contract clauses, the field clauses, and the use
case review clauses. Every clause takes the value in either of two forms:

- a quoted string for free text: `rationale "Checkout failures block revenue."`,
  `target "<=300"`;
- a bare token for a single word or number: `priority must`, `metric p95_latency_ms`,
  `target 300`, `slo 99.9`.

`rdra-ish fmt` writes each value the way it was written, so `priority "must"` and
`priority must` both stay as they are. Anything that is not one identifier or one
number (spaces, `-`, `<`, `%`, non-ASCII text) needs the quotes. `required` alone takes
`true` or `false`.

### Kind index

| kind | Description | Reviewed by |
|---|---|---|
| `goal` | An outcome the requirements serve, with the metric that measures it, its baseline and target, and the window it is read over. Its description states the problem it solves. | `lint` `goal-unmeasured`; RDRA and requirement diagrams (`achieves`; the requirement diagram adds the metric and `baseline → target` to the goal); `list --kind goal` |
| `requirement` | A requirement that motivates business use cases and achieves goals. | `lint` `requirement-untraced`, `requirement-no-goal`; RDRA and requirement diagrams (a `status "out-of-scope"` requirement is drawn apart); `list --kind requirement` |
| `adr` | An architecture decision record. It captures context, selected and rejected options, reasons, and consequences, then links the decision to impacted model elements with `decides`. | no diagram draws it, and the `omitted:` line of a diagram that draws what it decides names `adr`; `list --kind adr`, `list --kind adr-impact` |
| `nfr` | A non-functional requirement with measurable or reviewable quality metadata such as latency, SLO, availability, resilience, audit, retention, or privacy classification. | `lint` `nfr-unscoped`; RDRA diagram; `list --kind nfr` |
| `quality` | A quality attribute category such as performance, availability, resilience, auditability, observability, or privacy. | RDRA diagram (`qualifies`); `list --kind quality` |
| `constraint` | A non-functional guardrail or compliance constraint, commonly used for audit, logging, retention, privacy, or operational restrictions. | `lint` `constraint-unscoped`; RDRA diagram; `list --kind constraint` |
| `business` | A business (a coarse-grained area of activity) that BUCs belong to. | RDRA and business diagrams (the business flow draws each business with its BUCs) |
| `buc` | A business use case: a unit of business value composed of use cases. | `lint` `buc-empty`; RDRA, requirement (`motivates`), business (hand-offs between BUCs), and context (`via`) diagrams; `list --kind buc-roles`; every diagram's `--buc` scope |
| `flow` | A business flow: an ordered business narrative inside a BUC. A flow is composed of steps and can make UC and event ordering explicit without turning the diagram into the only source of truth. | `lint` `flow-empty`, `flow-cycle`; RDRA and flow diagrams |
| `step` | A business flow step. A step is a business-language action that can cover a use case, API, or event while remaining distinct from implementation boundaries. | `lint` `step-no-cover`; RDRA and flow diagrams (a lane per actor that `performs` steps) |
| `timer` | An RDRA タイマー: a schedule that starts a use case, such as a nightly sweep. See [Timers](#timers). | `triggers(Timer, UseCase)`; RDRA (external-environment layer), usecase, flow (`covers`), and event diagrams; `lint` `orphan-node` |
| `medium` | A physical medium, device, terminal, or operation medium for Business-BUC context. | `requires_medium` in `csv --kind screen-constraints`; RDRA diagram node. `belongs(...).by(...)` records it, but no output reads that context yet. |
| `actor` | A human actor who performs use cases. | `check` permission paths; RDRA, context, and usecase diagrams; `list --kind buc-roles`; `csv --kind actor-permission-audit` |
| `usecase` | A use case: a concrete interaction that reads and writes entities and displays screens. | `check`; `states`; RDRA, usecase (UC複合図), flow, condition, state (transition labels), and event diagrams; `csv --kind matrix` |
| `screen` | A screen (UI surface) displayed by use cases. | `lint` `screen-no-fields`, `screen-no-transition`; RDRA, usecase, and screen (画面遷移図) diagrams; `csv --kind screen-constraints` |
| `field` | A screen field: a first-class UI input or output item. It can carry editability, requiredness, and actor or system source metadata, and can map to an entity column without becoming one. | `lint` `field-unmapped`, `field-unplaced`; `csv --kind business-inputs` |
| `event` | A domain event, the interface between use cases and between use cases and external systems, and a `raises` and `triggers` endpoint. | `check` raised-but-unconsumed; `states`; RDRA, usecase, flow, and event diagrams, and the context and business edge labels; `export --kind asyncapi`, `tla` |
| `system` | An internal system boundary. It groups APIs; its entity set is derived from those APIs' CRUD targets. | `lint` `stage-readiness` (its `system-boundary` stage needs a `system` and an `api`); `check` boundary diagnostics; RDRA and context diagrams (a context edge goes to the system containing an API its use cases invoke); `csv --kind api-matrix` |
| `extsystem` | An external system. It reaches use cases through events: `raises(ExtSystem, Event)` for inbound, `triggers(Event, ExtSystem)` for outbound. See [External systems](#external-systems). | RDRA (system-value layer), context, usecase, and event diagrams; `list --kind buc-roles`; `export --kind asyncapi` (publisher or subscriber); `check` raised-but-unconsumed; `lint` `orphan-node` |
| `api` | An API layer endpoint invoked by a use case. It operates entities on behalf of the use case, defines an atomic data operation boundary, and may carry HTTP contract metadata. | `lint` `api-unused`, `api-contract-missing`, `api-contract-incomplete`; `check` API diagnostics; RDRA, flow, and condition diagrams (the usecase diagram folds it into its use case); `csv --kind api-matrix`; `export --kind openapi` |
| `dto` | A request, response, or error payload shape used by API contracts. DTOs may carry a column-like field body but are kept separate from database entities. | `lint` `dto-unused`, `naming-dto-field`; `export --kind openapi`, `json-schema` |
| `permission` | A permission or role-like authority assignable to actors. | `check` `requires_permission` against `has_permission`; `csv --kind actor-permission-audit` |
| `entity` | A logical data model: a table-like persistent structure. | `check`; `states`; RDRA, usecase, information, and ER diagrams; `list --kind transition` (its lifecycle columns); `csv --kind matrix`; `export --kind dbml`, `json-schema`, `tla`; `lint` `enum-attribute` |
| `variation` | A closed set of values an actor chooses from (an RDRA バリエーション), such as a payment method. It types attribute columns of entities and DTOs and is never a lifecycle. See [Variations](#variations). | `lint` `orphan-node` (a variation that types no column); `check` `UnknownColumnType`, `InvalidVariationValue`; ER diagram and `csv --kind entity` (the column type); information diagram; condition diagram (with its values); `export --kind dbml`, `json-schema`, `openapi`; `list --kind variation` |
| `condition` | An RDRA 条件: the combination of variations and a state model that selects a business rule, such as "the visit is covered by insurance". It carries no body for traceability only, or a `holds` body that guards the use cases it applies to. See [Conditions](#conditions). | `combines`, `holds`, `applies_to(Condition, UseCase \| Api \| System)`; `lint` `condition-no-rdra-anchor`, `orphan-node`; `check` `ConditionCombinesWithBody`, `ConditionSecondBody`, `ConditionCombinesAttribute`, `AmbiguousCombinesTarget`; RDRA diagram (system layer, edges to variation, state model, and use case); condition and usecase diagrams; `list --kind condition`; `states`, `check`, `export --kind tla`, and `verify` read a body as a guard |

---

## Relationship Predicates

Predicates connect declared instances. Arguments are referenced by their `<Id>`. When
an id is ambiguous across kinds (for example an `event` and a `usecase` share a name),
qualify the argument with a kind prefix — see [Kind-Qualified References](#kind-qualified-references).

| Predicate | Signature | Semantics |
|---|---|---|
| `performs` | `(Actor, UseCase \| Buc \| Step)` | The actor performs the given use case, BUC, or business step. On a BUC it performs every use case of the BUC, with their permissions; an actor that only takes part in a BUC performs the use cases or steps it runs instead (see [`api` and the API layer](#api-and-the-api-layer)). |
| `invokes` | `(UseCase, Api)` | The use case invokes the API layer. The API's writes form one transaction group of the use case. |
| `request` | `(Api, Dto)` | The API accepts the DTO as a request payload. |
| `response` | `(Api, Dto)` | The API returns the DTO as a successful response payload. |
| `error_response` | `(Api, Dto)` | The API may return the DTO as an error payload. |
| `applies_to` | `(Nfr \| Condition, UseCase \| Api \| System)` | The non-functional requirement or condition applies to the use case, API, or system boundary. Use this for performance, availability, or SLO requirements tied to an operation or system, and for the rule a [condition](#conditions) selects on a use case. |
| `combines` | `(Condition, Variation \| Entity.column)` | Links the condition to a variation or to the lifecycle column (a column some `transitions` moves) of an entity: the 条件 sheet's バリエーション and 状態モデル columns. Not allowed on a condition that has a `holds` body. |
| `holds` | `(Condition, comparison, ...)` | The condition's body: a comma list of comparisons, an implicit conjunction, each on a qualified column such as `Payment.method == insurance`. At most one per condition. |
| `qualifies` | `(Nfr \| Constraint, Quality)` | The non-functional requirement or constraint belongs to the given quality category. |
| `constrains` | `(Constraint, UseCase \| Api \| System \| Entity \| Dto)` | The constraint applies to the target model element. Use this for audit/logging/retention/privacy rules that constrain system behavior or data handling. |
| `reads` | `(UseCase \| Api, Entity)` | The use case or API reads the entity. |
| `creates` | `(UseCase \| Api, Entity)` | The use case or API creates the entity. Seeds the initial state pattern. |
| `updates` | `(UseCase \| Api, Entity)` | The use case or API updates the entity. Produces a transition operation in state derivation. |
| `deletes` | `(UseCase \| Api, Entity)` | The use case or API deletes the entity. Marks the source pattern terminal. |
| `displays` | `(UseCase, Screen)` | The use case displays the screen. |
| `shows` | `(Screen, Entity)` | The screen shows information from the entity. |
| `navigates` | `(Screen, Screen)` | The user moves from the first screen to the second. `.via(UseCase)` names the use case whose operation causes the move. `diagram --kind screen` draws it. See [Screen transitions](#screen-transitions). |
| `maps_field` | `(Field, Entity.column)` | Maps a screen field to a specific Entity column. The entity and the column must exist. Use with `contains(Screen, Field)` when screen-item level input/output mapping matters. |
| `raises` | `(UseCase \| ExtSystem, Event)` | The use case or external system raises the event, which `triggers` hands to downstream use cases or BUCs. An external system raising an event is the inbound direction. |
| `triggers` | `(Event \| Timer, UseCase \| Buc \| ExtSystem)` | The event or timer triggers a concrete use case or starts a BUC boundary. An event triggering an external system delivers the event to it (outbound). A timer cannot trigger an external system: the legal pairs are `Event` to any of the three and `Timer` to `UseCase` or `Buc`. |
| `outbox` | `(Event)` | Marks a raised event as intentionally published outside the local model, suppressing the raised-but-unconsumed warning. |
| `contains` | `(Buc, UseCase \| Flow)`, `(Flow, Step)`, `(Screen, Field)`, or `(System, Api)` | The use case or flow composes the BUC, the step composes the flow, the field belongs to the screen, or the API belongs to the system boundary. |
| `owns` | `(System, Entity)` | Optionally declares that a system is responsible for an entity even before API operations are fully modeled. API CRUD still records concrete access; `owns` records intended ownership. |
| `exposes` | `(System, UseCase \| Event \| Entity)` | Publishes the element to other models. A use case takes `.caller(actor \| system \| any)`, default `actor`. An entity takes `.append()` or `.upsert()` and becomes a published entity; without a mode it is `InternalEntityExposed`. See [Publishing to other models](#publishing-to-other-models). |
| `shares` | `(System, Entity)` | Lets other models name an internal entity directly. The owner's `check` reports `SharedInternalEntity` (warning) for as long as the line stays. |
| `precedes` | `(Step, Step)` | The first business step normally occurs before the second. |
| `branches` | `(Step, Step)` | The first step may branch to the second as an alternate path. |
| `excepts` | `(Step, Step)` | The first step may route to the second as an exception path. |
| `repeats` | `(Step, Step)` | The first step may loop back to the second. |
| `covers` | `(Step, UseCase \| Api \| Event \| Timer)` | The business step covers the referenced use case, API, event, or timer. Use this to make UC ordering and event chains explicit in the DSL while keeping the step vocabulary business-facing. |
| `compensates` | `(UseCase, UseCase)` | The first use case compensates for or rolls back the business effect of the second. Use this for alternative or exception flow review without forcing compensation into CRUD/state derivation. |
| `coordinates` | `(UseCase, Entity, Entity)` | The use case coordinates consistency for a relation crossing system boundaries. The use case must invoke APIs on both system sides that operate the corresponding entities. |
| `belongs` | `(Buc, Business)` | The BUC belongs to the business. |
| `has_permission` | `(Actor, Permission)` | The actor has the permission type. This provides a base vocabulary for later UC/API permission constraints. |
| `requires_permission` | `(UseCase \| Api, Permission)` | The use case or API requires the permission type. |
| `requires_medium` | `(UseCase \| Api, Medium)` | The use case or API requires the operation medium. |
| `motivates` | `(Requirement, Buc)` | The requirement motivates the BUC. |
| `achieves` | `(Requirement, Goal)` | The requirement serves the goal. A requirement may achieve several goals. |
| `decides` | `(Adr, Buc \| UseCase \| Api \| System \| Entity \| Requirement \| Nfr \| Constraint \| Dto)` | The ADR records a design decision that affects the target element. Use this for impact review and design traceability without making the target depend on a document file path. |
| `relate` | `(Entity, Entity, Card)` | Declares an ER relationship and auto-generates the FK columns. `Card` is one of `1:1`, `1:N`, `N:1`, `N:M` (unquoted). The child side, the `N` of `N:1` / `1:N` or the left of `1:1`, may be a range `lo..hi` with `lo` 0 or 1 and `hi` 1 or `*`: `relate(OrderLine, Order, 1..*:1)` says every `Order` has at least one `OrderLine`, and the lifecycle checks it as the invariant `MinCard_<Child>_<role>`. A range on the parent side is an error; write `.optional()` for a parent that may be absent. For `N:M`, require `.via(JunctionEntity)` where the junction is distinct from both ends: the model keeps the logical N:M edge **and synthesizes** two `N:1` edges `Junction→A` / `Junction→B` (FK columns, composite unique on the FK pair when missing, TLA owner links, ER crowfoot, `.along` paths). Equivalent manual form: two explicit `N:1` relates from the junction. For `1:1` the left end is the child: it holds the key column, marked unique. FK options can be chained with `.optional()`, `.on_delete(action)`, and `.on_update(action)` (for `N:M`, options apply to both generated FKs). An `.optional()` relate lets the child exist unlinked in `states` and in the TLA+ export; a mandatory relate does not. |
| `transitions` | `(UseCase, Entity.column, from -> to)` | A lifecycle edge on an Enum column: the use case moves the entity from the first variant to the second. `.when(Condition)` guards this arm only. See [Guard one arm](#guard-one-arm-when). |
| `after` | `(UseCase).assert(...)` | A temporal anchor assertion: a predicate on the state that each step of the use case leaves. |
| `when` | `(conditions...).none/has(conditions...)` | A to-many quantifier constraint (replaces `forbidden_when`). Prefer qualified columns such as `Cert.status == revoked`. |
| `sets` | `(UseCase, Entity, col == val)` | An explicit column effect, read by state derivation. Use `present` (not PostgreSQL type names) for nullable non-null. |
| `drives` | `(UseCase, Entity, col op rhs, true \| false)` | Drives the truth value of a comparison proposition (a derived Bool axis) in state derivation. Replaces the deprecated four-argument `sets`. |
| `chooses` | `(UseCase, Entity.column)` | The actor picks the value of a variation-typed column again when the use case runs: any value of the variation may follow, as on a creation. See [Choosing a value again](#choosing-a-value-again-chooses). |
| `chooses` | `(UseCase, Entity.column, value)` | One value the actor may pick; the lines of one use case and column list the choice. `.when(Condition)` guards this value only. See [Guard one value](#guard-one-value-when). |

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
| `navigates` | `.via(UseCase)`, at most once |
| `transitions` | `.when(Condition)`, at most once: the condition's `holds` body guards this arm |
| `chooses` | `.when(Condition)`, at most once, on the form with a value: the condition's `holds` body guards this value |
| `relate` | `.via(...)`, `.optional()`, `.on_delete(...)`, `.on_update(...)`, `.role(...)`, `.key(...)` |
| `forbidden` | `.along(...)`, `.by(...)`, `.label("...")` |
| `invariant` | `.when(...)`, `.then(...)`, `.has(...)`, `.none(...)`, `.along(...)`, `.by(...)`, `.label("...")` |
| `required`, `exclusive` | `.label("...")` |
| `holds` | `.has(...)`, `.none(...)`, `.by(...)` |
| `sets` | `.by(...)`, on a copy (`column == Source.column`) |
| `after` | `.assert(...)`, `.label("...")` |
| `exposes` | `.append()`, `.upsert()` (an entity), `.caller(actor \| system \| any)` (a use case) |
| every other predicate (including `combines`) | none |

---

## Requirements and Traceability

These kinds record why the rest of the model exists. `achieves(Requirement, Goal)`
ties a requirement to the outcome it serves, `motivates(Requirement, Buc)`
ties a requirement to business value, `decides(Adr, ...)` ties a design decision to the
elements it shapes, and `applies_to` / `constrains` scope quality objectives and
guardrails to use cases, APIs, systems, and data.

### Goals and non-goals

<!-- derived-from #relationship-predicates -->

A PRD opens with the problem, the goals and how each is measured, and the non-goals.
A `goal` holds the first two. Its description states the problem the goal solves, and
`metric`, `baseline`, `target`, and `window` say how the outcome is measured. All four
are optional; `lint` reports a goal that has no `metric` or no `target` as
`goal-unmeasured`.

```
goal ShortWait "受付から診察までの待ち時間を短くする"
  description "受付後に診察室へ案内されるまでの待ちが長いと、患者が帰ってしまう。"
  metric "受付から診察開始までの中央値"
  baseline "45分"
  target "30分"
  window "月次"

achieves(ReqSwiftCheckIn, ShortWait)
```

`achieves(Requirement, Goal)` links a requirement to each goal it serves. `lint` reports
a requirement that achieves no goal as `requirement-no-goal`. Use
`rdra-ish list --kind goal` for the goals with the requirements that achieve each.

A non-goal is a requirement that the model rules out. It has no element of its own: write
a `requirement` with `status "out-of-scope"` and a `rationale` that says why. `lint` does
not ask an out-of-scope requirement for a goal.

```
requirement ReqNoOnlineConsultation "オンライン診療は対象外とする"
  status "out-of-scope"
  rationale "診察は来院して受ける前提で業務を分けているため、ビデオ通話による受付・診察・会計は扱わない。"
```

### Requirements

<!-- derived-from #relationship-predicates -->

`requirement` declarations may carry requirement-specific metadata after the label or
description:

```
requirement ReqCheckout "Checkout must be reliable"
  description "The checkout flow must preserve customer intent."
  priority must
  source "Customer interview"
  stakeholder "Store Operations"
  owner "Product Owner"
  acceptance_criteria "A payment timeout leaves the cart recoverable."
  status proposed
  risk high
  rationale "Checkout failures directly block revenue."
```

`source`, `stakeholder`, and `acceptance_criteria` may be repeated. Single-value
fields use the last value when repeated. `acceptance_criteria` is the only spelling:
the two-word `acceptance criteria` (and a bare `acceptance`) fails to parse with a
message that names it.

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
| `target` | Target value, such as `300`, `"<=300"`, or `"<0.1%"`. |
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

If the triggered use case is a system-triggered step, `performs` may be omitted. A use case
that a timer or an event starts and that no `performs(Actor, UseCase)` names is the system's
work: the actors of its BUC do not perform it, and the permissions it requires, directly or
through the APIs it invokes, are the system's, which `check` does not ask any actor to hold.
So a timer-started use case goes through the same API as the actor's use cases that write the
same rows. Once a `performs(Actor, UseCase)` names an actor, that actor must hold them.

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
maps_field(ShippingAddress, Order.shipping_address)
maps_field(OrderTotal, Order.total)
```

Supported field clauses are:

| Clause | Description |
|---|---|
| `access` | `editable` or `readonly` by convention. Project-specific tokens are accepted. |
| `required` | `true` or `false`, representing whether the field is required on the screen. |
| `source` / `input` / `derived` | `actor` for actor-entered fields or `system` for system-derived fields by convention. Project-specific tokens are accepted. |

`shows(Screen, Entity)` remains a coarse screen-to-entity information link.
`maps_field(Field, Entity.column)` is the fine-grained mapping when a concrete
screen item corresponds to a logical data column.

### Screen transitions

`navigates(FromScreen, ToScreen)` declares that the user moves from one screen to
another. `.via(UseCase)` names the use case whose operation causes the move, such as the
button that submits the check-in. The use case is optional, and a pair of screens may have
several `navigates`, one per operation.

```rdra
screen CheckInScreen "受付画面"
screen EligibilityScreen "資格確認画面"
usecase CheckInPatient "受付（初診・再診）"
displays(CheckInPatient, CheckInScreen)

navigates(CheckInScreen, EligibilityScreen).via(CheckInPatient)
```

Both arguments must be screens. `.via(...)` must name exactly one use case, once; anything
else is `NavigatesViaRequiresUseCase`.

`diagram --kind screen` draws each `navigates` as a solid edge labelled with the use case.
Where no `navigates` is declared, it adds a dashed 業務上の順番 edge from the business
flows: for two consecutive steps of one flow (`precedes`, `branches`, or `excepts`) that the
same single actor performs, each screen the first step's use cases display is followed by
each screen the second step's use cases display. That edge is the order the work visits the
screens, not a transition the UI offers. `lint` reports a screen that neither kind of edge
reaches as the info `screen-no-transition`.

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
triggered use cases drive.

The order of `raises` and `triggers` between use cases adds no guard to state derivation:
a use case is limited only by the states it requires and by the guards of its linked
conditions. `states`, `check`, and `verify` therefore let a triggered use case run before
the use case that raises its event. This over-approximates the flow, so a check never
misses a violation the event order could produce, but it can report a run that the event
order prevents. In the model below, `check` reports `AfterAssert_IssueCert_0` with the trace
RequestCert, IssueCert, although in the real flow IssueCert runs only after
ExecuteCertIssue raises `EvOrderExecuted`.

```rdra
transitions(ExecuteCertIssue, CertificateOrder.status, requested -> executed)
raises(ExecuteCertIssue, EvOrderExecuted)
triggers(EvOrderExecuted, IssueCert)
creates(IssueCert, ClientCertificate)
relate(ClientCertificate, CertificateOrder, N:1)
after(IssueCert).assert(CertificateOrder.status == executed)
```

This is the 1.0 semantics, and it holds for a `timer` as for an event: neither orders the
use cases it triggers (#30). Tracking the unhandled events as state would make the order
exact, but it would change the reachable states and the `check` verdict of every model
that uses `triggers`. [Known verification gaps](./formal-verification.md#known-verification-gaps)
lists this gap with the others.

When a rule depends on the event order, state what the order guarantees as a `condition`
guard on the triggered use case. Here the event is raised only once the order is
`executed`, so IssueCert needs an executed order:

```rdra
condition OrderExecuted "実行済みの依頼にだけ証明書を作る"
holds(OrderExecuted, CertificateOrder.status == executed)
applies_to(OrderExecuted, IssueCert)
```

With the guard, `check` and `verify --instance-count 2` pass. The guard also documents the
precondition the event handler relies on, so it stays true if the use case is later started
some other way.

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
transitions(Revoke, Session.status, active -> revoked)

// Published information: a projection of Session that auth writes and never reads.
entity SessionView "Session (published)" {
  session_id: Int @pk
  status: Enum(active, ended)
}
updates(SessionApi, SessionView)
sets(Login, SessionView, status == active)
updates(Revoke, SessionView)
sets(Revoke, SessionView, status == ended)

exposes(AuthSystem, Login)                   // callable on behalf of an actor
exposes(AuthSystem, SessionView).upsert()
```

`exposes(System, UseCase)` publishes a use case. `.caller(actor)`, the default, means
another model calls it on behalf of a human actor; `.caller(system)` means only a service
identity may call it; `.caller(any)` allows both. A use case exposed with `.caller(system)`
or `.caller(any)` is not reported as having no actor, neither for its own permissions
(`UseCasePermissionNoActor`) nor for those of the APIs it invokes
(`ApiPermissionNoActor`), because its caller is the other model's service.

`exposes(System, Event)` publishes an event another model may react to.

`exposes(System, Entity).append()` or `.upsert()` publishes an entity. A published entity
is a projection the owner writes and never reads, such as an append-only log or a silver
table. `.append()` allows `creates` and `sets`; `.upsert()` also allows `updates`.
`deletes` and `transitions` are never allowed. The owner's `check` enforces
these rules:

| Diagnostic | When |
|---|---|
| `PublishedEntityMutated` | an `append` entity gets `updates`, or any published entity gets `deletes` or `transitions` |
| `PublishedEntityReadByOwner` | the owner reads a published entity: `reads`, `shows`, `maps_field`, a `holds` body, `combines`, or a constraint (`forbidden`, `invariant`, `required`, `exclusive`, `when`, `after`) |
| `PublishedEntityKeysInternal` | a `relate` joins a published entity to an internal one, in either direction |
| `PublishedNotInSourceTx` | a use case writes a published entity but makes no internal change, or makes several internal transactions, so the published row has no one transaction to join; or it writes the published entity through an API that makes no internal change, because each API is its own transaction |
| `InternalEntityExposed` | `exposes(System, Entity)` without `.append()` or `.upsert()`; publish a projection instead |
| `PublishedEntityNeverWritten` (warning) | no use case writes the published entity |
| `DuplicateExposure` | a second `exposes` line for an element that is already exposed; the first line stays, the second is the error |
| `SharedEntityAlsoExposed` | `shares(System, Entity)` on a published entity; an entity has one role |
| `ExposesChainInvalid` | a chain method on the wrong kind, such as `.append()` on a use case or `.caller(...)` on an entity |
| `ExposesCallerInvalid` | `.caller(...)` with an argument other than `actor`, `system`, or `any` |

A published row is written in the transaction of the internal change it reports. A direct
write to a published entity therefore joins the writing use case's one internal
transaction group, whether or not a key links them, and `SeparateTxInferred` never fires
for it. In the example, `Revoke` moves `Session` and writes `SessionView` in one action,
where two unrelated writes would otherwise be `Revoke_1` and `Revoke_2`. A write made
through an API stays in that API's transaction, because each API is its own atomic
boundary, so a published write through an API of its own is `PublishedNotInSourceTx`;
route the internal change and the published write through one API. The owner's lifecycle
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
directly instead. To record that an actor takes part in a BUC without running all of it,
write `performs(Actor, UseCase)` for the use cases it runs or `performs(Actor, Step)` for its
steps: `list --kind buc-roles` and the context diagram count any of the three as taking part,
and a step carries no permission. When a permission is missing only because of a
`performs(Actor, Buc)`, the `ActorPermissionMissing` hint names that BUC and these forms. A
use case with no actor at all needs one for its permissions (`UseCasePermissionNoActor`,
`ApiPermissionNoActor`), unless a timer or an event starts it (see [`triggers`](#triggers-and-event-driven-buc-chaining))
or another system calls it (`exposes(..).caller(system)`).

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
- `states` and `verify` run these transactions (`UsecaseTx::groups()`). An entity a use
  case only `sets` or transitions counts as an update. An
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
| `Enum(a, b, c)` | Inline value set. An Enum column is a **state axis** and can be linked to a state machine. Use it for a lifecycle column or a value that use cases set. Each value may carry a label, as a variation value does: `Enum(pending "受付前", paid "支払済")`. Diagrams draw a state by its label, and the `states` table lists the labels under its axes. `lint` `enum-attribute` warns on an inline Enum that no `transitions` and no `sets` moves, because an actor chooses its value: type it by a variation. |
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
A comparison in `@check` and `@derived` reads its right-hand side as every comparison does
(see [Bare identifiers](#bare-identifiers)): `@check(status == open or closed_at == present)`
compares `status` with its value `open` and `closed_at` with `present`. DBML notes, JSON Schema `x-rdra-ish-check-constraints` /
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
  status: Enum(pending, posted) @default(pending)
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
| TLA+ export | A lifecycle column, one per column `transitions` names | An axis only when a `sets`, a constraint, an `after` assertion, or a `property` names the column; `Init` is a free choice over every value, so TLC checks them all |
| update | A `transitions` arm or a `sets` moves it | Keeps its value, unless the use case `chooses` it |
| `export --kind dbml` | One `Enum <Entity>_<column>` per column | One `Enum <VariationId>` shared by every column typed by it |
| `export --kind json-schema`, `openapi` | `enum` of the variants | `enum` of the variation's values |
| ER diagram, `csv --kind entity`, `list --kind entity` | Type `Enum` | Type is the variation id |

#### Choosing a value again: `chooses`

A created instance takes any value of a variation column, unless the creating use case
`sets` it: the actor picks it. An update keeps the value. When a use case lets the actor
pick again, say so with `chooses`:

```rdra
variation AmendmentScope "補正の範囲" {
  limited "削除・減縮・誤記の訂正・釈明に限る補正"
  free "限定のない補正"
}
entity OfficeAction "拒絶理由通知" {
  id: Int @pk
  status: Enum(open, drafted) @default(open)
  amendment: AmendmentScope @default(limited)
}
usecase DraftResponse "応答案の作成"
transitions(DraftResponse, OfficeAction.status, open -> drafted)
chooses(DraftResponse, OfficeAction.amendment)
```

- After the use case the column holds any value of the variation, one run per value, in
  `states`, `check`, and the TLA+ export (`\E v \in {"limited", "free"} : ...`). Two
  `chooses` in one use case choose independently. Each run carries the use case's other
  effects.
- A `chooses` counts as a write of the entity, as a `sets` does, with no CRUD edge needed.
- On the creating use case it is the creation's own choice and changes nothing.
- `chooses` is a new predicate, not `sets(.., col == any)`: a variation may name a value
  `any`, and `sets` fixes one value.
- To offer only some values, list them, one `chooses(UC, E.col, value)` line each; to
  offer a value only in some states, guard its line with `.when` (below).

Only a variation-typed column is chosen. An inline `Enum` is a state model, and the actor
picks the next state with several `transitions` arms from one state; a `Bool` takes a
`sets`. Either is `ChoosesNotVariation`, whose hint names the form to write. A second
argument that is not `Entity.column` is `InvalidChoosesColumnRef`; an unknown entity or
column is `UndefinedSymbol` or `UnknownColumn`. A use case that both `chooses` and `sets`
one column is `ChoosesAndSets`, and a derived column is `DerivedColumnWritten`.

#### Guard one value: `.when`

<!-- constrained-by #guard-one-arm-when -->

The lines `chooses(UseCase, Entity.column, value)` of one use case and column list the
values the actor may pick; `.when(Condition)` on a line offers that value only while the
condition holds. A final office action admits only a limited amendment (特許法17条の2):

```rdra
condition FirstNoticeOnly "最初の拒絶理由通知である"
holds(FirstNoticeOnly, OfficeAction.kind == first)

chooses(DraftResponse, OfficeAction.amendment, free).when(FirstNoticeOnly)
chooses(DraftResponse, OfficeAction.amendment, limited)
```

- **Only the listed values are choices.** The value-less `chooses(UC, E.col)` still offers
  every value of the variation. One use case and column takes one form: a value-less line
  beside value lines, in either order, is `ChoosesMixedForms`.
- **A guarded value is a choice only while its guard holds**, read as a `transitions` arm
  guard reads it ([Guard one arm](#guard-one-arm-when)): on the state the use case starts
  from, so a guard that reads the chosen column itself sees the old value. An instance the
  use case creates is read after the step, as an `applies_to` body reads it. Among the
  enabled values the choice is free. Above, the attorney can always amend within the
  limits, and can amend freely only on a first notice.
- **No enabled value, no run.** A use case whose every listed value is guarded runs only
  where some guard holds, as one whose every arm is guarded does.
- **A value listed twice is enabled by either line**, as an arm declared twice is: an
  unguarded line lifts the guard, two guards are joined with or. It is not an error.
- On the creating use case the listed values restrict the creation's own choice.

A value that is not one of the variation's is `InvalidVariationValue`, whose hint lists the
values, and a quoted value, `chooses(UC, E.col, "a")`, is `QuotedValue` (write it bare).
`.when` on the value-less form guards no value and is `ChoosesWhenWithoutValue`;
write the value, or `applies_to(Cond, UC)` to guard the whole use case. Otherwise `.when`
takes the same condition `transitions` arms take: `WhenRequiresCondition` and
`WhenConditionWithoutBody` apply as for an arm.

A guarded value that is never chosen while its use case runs is the `ValueNeverChosen`
warning of `check` (`Choice_<UseCase>_<Entity>_<column>_<value>`): its guard never holds
where the use case runs. The sequence diagram writes the choice on the write, each guard in
`[...]` after its value:
`amendment in (限定のない補正 [最初の拒絶理由通知である], 削除・減縮・誤記の訂正・釈明に限る補正)`.
`list --kind condition` adds the use case to the condition's `applies_to` column, and the
condition `combines` the chosen variation, `derived ... from when` in the condition diagram.

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
the column, and the value (`InvalidVariationValue` or `InvalidEnumVariant`). A choice among
the values of one column is `Entity.column in (a, b)`, and its negation `not in`
(see [Set membership](#set-membership-in-and-not-in)); other disjunction and negation are not
available yet. A second `holds` on one condition is `ConditionSecondBody`, and `combines`
together with `holds` is `ConditionCombinesWithBody`, in either order.

```rdra
// The guard of a later application's priority claim: the earlier one is still pending.
holds(PriorityBasePending, PatentApplication.status in (filed, formality_order, examining))
```

A body may end in `.has(Related.col == v)` or `.none(Related.col == v)` calls. The guard then
also needs some row (`has`), or no row (`none`), of the related entity to match. A bare entity,
`.has(Related)` or `.none(Related)`, asks for some linked row, or none, whatever its columns
hold: it reads only the link. The rows are
the ones the relate links to the instance the use case runs on, and every row when no relate
joins the two entities. Like the `.none(...)` of an `invariant`, it ranges over the present rows.
It reads them before the use case runs: on a use case that creates the related entity, the
row being created is not one of them, and on a use case that updates a related row, that row
is one of them, read as it was before the update. The quantified row is its own instance even
when the body also reads its entity. A set that names every value of the column holds for
every row, so `.none(Related.status in (<every value>))` is the same guard as `.none(Related)`,
and the lifecycle reads only the link for it (see
[Set membership](#set-membership-in-and-not-in)). Write `.none(Related)`: it says "no related
row" without listing values, and it does not change meaning when the column gains a value.
When two keys join the body's entities, `.by(role)` names the one to follow (see
[Which key a join follows](#which-key-a-join-follows)).

```rdra
// Revoke only while no assignment of the certificate is active.
condition NoActiveAssignment "有効な割当がない証明書だけ失効できる"
holds(NoActiveAssignment, ClientCertificate.status == active).none(TerminalCertAssignment.status == active)
applies_to(NoActiveAssignment, Revoke)
invariant(ClientCertificate).when(status == revoked).none(TerminalCertAssignment.status == active)
```

Without the guard on Revoke, the `invariant(...).when(...).none(...)` rule fails: Revoke can run while an
assignment is active (`samples/formal-verification/quantifier_guard.rdra`). A column named in
a `.has` or `.none` is part of the body for `condition_edges`, so a lifecycle column there links
the state model.

One function, `condition_edges`, gives a condition's links. With a body, an operand on a
variation-typed column links the variation and an operand on a lifecycle column links the state
model. Without a body, the `combines` declarations are the links. `list --kind condition`,
the RDRA diagram, and `lint` all read it, so a body and its links cannot disagree. A body that
names neither a variation column nor a lifecycle column, and guards no use case, is `lint`
`condition-no-rdra-anchor`. A body guards a use case through `applies_to` a use case or an
API, or through `.when` on a `transitions` arm or a `chooses` value; such a body is the use
case's rule even when it reads only `Bool` or nullable columns, so it is not flagged. Do not
add a comparison on the state the use case already moves from to anchor it: the transition
already requires that state. `applies_to(Condition, System)` adds no guard, so it does not
exempt the body.

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

#### Which key a join follows

<!-- constrained-by #relate-and-fk-generation -->

A statement that reads two related entities joins them through a key: an `.along(A, B)`
hop, a `.has(...)` or `.none(...)` (on an `invariant` or a `holds`), the link between an
entity a `holds` body compares and an entity the guarded use case writes, and the source of
a copy (`sets(UC, E, col == F.col)`). Every one of them resolves the key by one rule
(`SemanticModel::join_key`), so `check`, `states`, the TLA+ export and `verify` read the
same rows:

- One key between the two entities (in either direction) is the join. A model with one key
  per pair writes no selector, and a selector does not change such a join.
- Several keys (two relates, or two self-relates of one entity) need `.by(role)`, which
  names the key by its role, or by its parent id when it has no `.role` (`.by(Order)` names
  the key of `relate(Line, Order, N:1)`; case does not matter). The join follows the one key
  the selector names.
- Several keys and no selector naming exactly one is `AmbiguousOwnerLink`, an error that
  names the candidate roles and the statement with `.by(<first role>)` added. The join is
  left unlinked.

`.by(role)` belongs to the call before it:

| Written after | Picks the key of |
|---|---|
| `.along(A, B, ...)` | each hop that several keys join |
| `.has(...)` or `.none(...)` | the link from the quantified row to the rows the statement binds |
| `holds(Cond, ...)` itself, before any `.has/.none` | the link between an entity the body compares and the other entities of the guard |
| `sets(UC, E, col == F.col)` | the key from the target row to the source row |

```rdra
relate(BusinessPermit, PermitApplication, 1:1)                  // registered under
relate(PermitApplication, BusinessPermit, N:1).optional().role(renews)

// A new application gets one permit: no permit registered under it yet.
holds(NoPermitYet, PermitApplication.kind == new).none(BusinessPermit).by(PermitApplication)
// A renewal reads, and links to, the permit it renews.
holds(RenewsValidPermit, BusinessPermit.status == valid).by(renews)
forbidden(BusinessPermit, PermitApplication, BusinessPermit.business_type != PermitApplication.business_type)
  .along(BusinessPermit, PermitApplication).by(PermitApplication)
```

The selector is a chain call rather than a qualified reference such as
`PermitApplication.renews.status`. It keeps every comparison in the `Entity.column` form the
rest of the language reads, it is the call `.along(...)` already took, and a model with one key
per pair, the common case, never writes it.

| Diagnostic | When |
|---|---|
| `AmbiguousOwnerLink` | Several keys join two entities a statement joins, and no `.by(role)` names one. The lifecycle reports it for a guard, a quantifier, and a copy (it depends on the use case a guard is on); the analysis for `.along`. |
| `UnknownOwnerRole` | `.by(role)` names no key on the `.along` path, no key of the quantified entity, no key of an entity the `holds` body compares, or no key between a copy's target and source. |
| `ByWithoutAlong` | `.by(...)` on a `forbidden` or `invariant` follows no `.along`, `.has` or `.none`, or `.by(...)` on a `sets` that copies nothing. |

In LSP, a role is a symbol: go to definition on `.by(renews)` opens the `relate` that
declares `.role(renews)`, and find references and rename cover the `.role(...)` and every
`.by(...)` that names it. A `.by(Order)` that names a key by its parent id is a reference
to the entity `Order`.

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

`transitions(CapturePay, Order.status, pending -> paid)` declares one edge of an entity's
state machine, driven by the use case `CapturePay`. It is the 遷移UC column of an RDRA
state sheet: the use case moves the state, and events stay the interface between use
cases. The Enum column is required; state labels are the Enum variants
(case-insensitively). Two use cases that drive the same edge each get their own line:

```
transitions(CancelOrder, Order.status, pending -> cancelled)
transitions(ExpireOrder, Order.status, pending -> cancelled)
```

Several arms of one use case from one state on one column are a choice. One run of the
use case takes exactly one of them, and the actor decides which:

```
transitions(DecideFiling, Disclosure.status, searched -> decided_file)
transitions(DecideFiling, Disclosure.status, searched -> decided_secret)
transitions(DecideFiling, Disclosure.status, searched -> decided_abandon)
```

- **Every arm carries the rest of the use case.** Its `sets` on other columns, its
  `creates` and `deletes`, and its transitions of other columns or entities apply on each
  arm alike.
- **Two choice columns choose independently.** A use case with arms `searched -> file` and
  `searched -> drop` on `status`, and `none -> fast` and `none -> slow` on `route`, has four
  runs, one per pair. To tie a value of one column to a value of the other, split the use
  case, one per pair.
- **Use case guards apply to every arm.** A condition applied to the use case guards each
  arm alike, and the use case runs only from a state one of its arms leaves. When one
  outcome needs a precondition the others do not, guard that arm with `.when` (below).
- **`after(UC).assert` holds after every arm.** An assert one arm breaks is violated, with a
  trace through that arm.
- A repeated arm is one arm. Arms from different states stay one move per state: the
  state the instance is in picks the arm.

A `sets` is not a choice. Two `sets` of one use case with different values for one column
are a `ConflictingEffects` error; keep one value or split the use case.

#### Several lifecycle columns on one entity

An entity may have several lifecycle columns. Each Enum column that `transitions` names is
its own state model, with its own states, witnesses, deadlocks, and state diagram block. A
disposition whose procedure and publication move independently keeps both on one row:

```rdra
entity Disposition "不利益処分" {
  id: Int @pk
  status: Enum(noticed, decided, finalized) @default(noticed)
  publication: Enum(unpublished, published, ended) @default(unpublished)
}
transitions(Decide, Disposition.status, noticed -> decided)
transitions(Publish, Disposition.publication, unpublished -> published).when(Decided)
transitions(EndPublication, Disposition.publication, published -> ended)
```

- **The columns move independently.** `states` lists every combination the use cases
  reach; `check` and the TLA+ export explore them together, so a guard, a `forbidden`, an
  `invariant`, or an `after(UC).assert` may read any of the columns.
- **A state names its column when the entity has several.** The state of `published` is
  `Disposition_publication_published` (diagram id `state__Disposition_publication_published`),
  so a value two columns share is two states. An entity with one lifecycle column keeps
  `{Entity}_{value}`.
- **A single-tag export picks one column.** `export --kind typescript` tags each state union
  by `status`, else a `*_status` column, else the first lifecycle column by name; the other
  columns are fields of each variant.

#### Guard one arm: `.when`

`.when(Condition)` on a `transitions` line guards that arm only. The condition is the RDRA
条件 the arm depends on, with a label and a `holds` body:

```rdra
condition RightsAcquired "会社が特許を受ける権利を取得している"
holds(RightsAcquired, InventionDisclosure.right_acquired == true)

transitions(DecideFiling, InventionDisclosure.status, searched -> decided_file).when(RightsAcquired)
transitions(DecideFiling, InventionDisclosure.status, searched -> decided_secret)
transitions(DecideFiling, InventionDisclosure.status, searched -> decided_abandon)
```

- **The arm is enabled only while its guard holds** on the state the use case starts from.
  The body reads the arm's own instance and any other entity the way an `applies_to` body
  does, so a body atom on another entity needs some present instance that meets it.
- **The use case runs when some arm is enabled**, and its `applies_to` guards hold. Among
  the enabled arms the choice is free. Above, the committee can always keep the invention
  secret or drop it, and can file only once the right is acquired.
- **One arm, one guard.** `.when` takes one condition, once. To require several
  comparisons, list them in the condition's `holds` body.
- **A guarded single arm differs from `applies_to`.** `applies_to(Cond, UC)` guards every
  step of the use case, from every state it leaves. `.when(Cond)` guards one arm: the use
  case still takes its other arms, from the same state or another, while the condition
  fails. The two coincide only when the arm is all the use case does.
- The same arm declared twice with different guards is enabled when either guard holds.

`.when` with no condition, with two conditions, twice, or with anything that is not a
condition is `WhenRequiresCondition`; an undefined name is `UndefinedSymbol`. A condition
without a `holds` body (one linked by `combines` only) is `WhenConditionWithoutBody`, since
it has nothing to guard with.

A guarded arm that is never taken while its use case runs and some instance reaches its
`from` state is the `ArmNeverTaken` warning of `check` (`Arm_<UseCase>_<from>_<to>`): its
guard never holds where the use case could take it. The state diagram writes the guard in
`[...]` after the use case label, as it does for `applies_to`; the sequence diagram heads the
arm's `alt` branch with it; and `list --kind transition` adds it to the `guards` column. In
the condition diagram the condition `combines` the arm's state model, `derived ... from
when`.

The second argument must be a use case. Passing an event, the earlier form, is a
`TypeMismatch` whose hint names the use case to write and `scripts/migrate-phase3.py`,
which rewrites a model to one line per use case that raises the event. See
[state-derivation.md](./state-derivation.md) for how transitions feed derivation.

---

## Value Vocabulary for `sets`

The third argument of `sets` is an unquoted assignment `column == value`. The value is a
bare word or number, and what it may be depends on the target column's type:

```
// Enum column: a variant
sets(usecase::Capture, Payment, status == captured)

// Bool column
sets(usecase::Enable, Switch, enabled == true)

// Int column: a number
sets(usecase::Restock, Stock, stock == 3)

// Nullable column: make it non-null
sets(usecase::Login, UserAccount, last_login_at == present)

// Nullable column: make it null
sets(usecase::Logout, Session, token == null)
```

The origin of `sets` is always a use case. An event origin is a `TypeMismatch` whose hint
names `scripts/migrate-phase3.py`.

| Value | Target column | Meaning |
|---|---|---|
| Enum variant name | `Enum` column | Set the column to that variant. |
| `true` / `false` | `Bool` column | Set the boolean value. |
| A number | `Int` column | Set the column to that value. |
| `present` | `@null` column | Make the column non-null (value present). |
| `null` | `@null` column | Make the column null. |
| `Entity.column` | A column of the same type | Copy the value of another row's column (see below). |

`sets` records which value a use case leaves, not how it is stored. A storage type name
such as `timestamptz` is not a value; write `present` for a nullable column the use case
fills. The storage type belongs on the column's type and the DBML export.

### Copying a value from another row

The right-hand side of `sets` may be a column written `Entity.column`. The use case then
leaves in the target column the value that the source column holds:

```
relate(BusinessPermit, PermitApplication, N:1)
creates(RegisterPermit, BusinessPermit)
sets(RegisterPermit, BusinessPermit, business_type == PermitApplication.business_type)
```

The source is the row that the step already binds for its entity. The step reads it the
way a `holds` atom on that entity reads it (see
[Conditions as guards](./state-derivation.md#actions)):

- **A row the step writes.** The use case writes the entity in the same transaction group.
  The step reads an updated row before it changes it, and a created row as the step leaves
  it. A copy within one row (`sets(UC, E, shadow == E.flag)`) reads that row.
- **The target's parent.** A `relate` makes the source entity the parent of the target.
  A child created under a mandatory relate reads the parent it is created under. An updated
  child reads the parent that its foreign key names, and the use case waits while an
  `.optional()` key is unlinked. When two keys join the target and the source, the copy
  names the one to follow: `sets(Fix, Permit, kind == Site.kind).by(home)` (see
  [Which key a join follows](#which-key-a-join-follows)).

Any other source is `UnboundCopySource`, an error, and the copy is not applied:

- an entity that the use case does not write and that is not the target's parent;
- a child of the target, because several rows may belong to one target row;
- an entity that the use case writes in another transaction group, which is a step of its
  own;
- the parent of a created child under an `.optional()` relate, because the child is
  created unlinked.

Every value of the source must be a value of the target. The two columns are one Enum
(the same variants), one variation, two Bool columns, two Int-typed columns (Int, Money,
Decimal), two Date-typed columns (Date, DateTime), or two `@null` String columns. A
nullable source needs a nullable target, and a `@derived` source is rejected. Any other
pair is `SetsCopyMismatch`. Only `==` with a qualified column copies: `kind != Parent.kind`
and a bare column (`qty == limit`) set nothing and are `SetsNoEffect`.

A use case leaves one value per column. A copy and a literal `sets` of one column, or two
copies from different columns, are `ConflictingEffects`. A copy and a `chooses` of one
column are `ChoosesAndSets`. A copy into a lifecycle column is `DoubleModeledEnum`, on
the creating use case too: a state model starts at the values that its creations name. A
foreign key column is no copy source or target (`EffectOnNonStateColumn`). On a created row
the copy replaces the actor's choice of a variation column and the `@default` of any other
column.

`check`, `states`, the TLA+ export, and `verify` read a copy the same way. TLA+ writes it as
a choice of the column's value, then the guard `col'[p] = source[q]`.

### Driving comparison propositions with `drives`

A **comparison proposition** (`col op rhs`, e.g. `stock < selling`, `expired_at < now`) is
evaluated on the Int values of its columns over `0..5` and on `now`. Change the columns
with `sets(Restock, Stock, stock == 3)` and the comparison follows. When the model does
not track the values, drive the truth value of the comparison instead with `drives`:

```
// Tell the model that Sell causes (stock < selling) to become true
drives(Sell,   Stock, stock < selling, true)

// Tell the model that Refund causes (stock < selling) to become false
drives(Refund, Stock, stock < selling, false)
```

The third argument is the bare comparison expression (no quotes); the fourth argument is
the bare boolean literal `true` or `false`. `sets` assigns a value and `drives` drives a
comparison, so each predicate has one meaning. A value equality driven true
(`drives(Pay, Order, status == paid, true)`) sets the value, as `sets(Pay, Order, status
== paid)` does; driven `false` it names no value to leave and is `DrivesFalseOnEquals`.
A `drives` whose third argument is not a comparison or whose fourth is not `true` or
`false` is `DrivesNoEffect`; a `sets` with a comparison such as `stock < selling` or
`status != paid` sets nothing and is `SetsNoEffect`, whose hint names `drives`. A use case
leaves one value, so a set membership (`status in (a, b)`) in `sets` or `drives` is
`EffectMembership`. Once any `drives` names a comparison, the comparison is a Bool
variable that only those `drives` change, and the Int columns no longer define it. A use
case that drives a comparison to one truth while its own Int `sets` make it the other is
`ContradictoryDrivenComparison`, an error.

| Fourth argument | Meaning |
|---|---|
| `true` | The comparison holds after this use case runs. |
| `false` | The comparison does not hold after this use case runs. |

`sets(UC, E, cmp, true|false)`, the four-argument `sets` that `drives` replaces, is
deprecated since 0.5.0 and removed in 2.0.0; it still loads as `drives`, with a warning
(see [Deprecated and removed forms](#deprecated-and-removed-forms)).

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

A rule that states nothing to check is an error, so no rule is dropped without a
diagnostic: `forbidden(Order)` and `required(Order)` with no condition are
`RuleWithoutCondition`, `exclusive` with fewer than two conditions is `ExclusiveNeedsTwo`,
an `invariant` with neither `.then(...)` nor `.has/.none(...)` is
`InvariantWithoutConsequence`, `.then(...)` without `.when(...)` is `InvariantWithoutWhen`,
and `after(UseCase)` without `.assert(...)` is `AfterWithoutAssert`.

### Labelling a rule

A constraint says what is forbidden or required, not why. `.label("...")` names the rule in
the business's words, on any of `forbidden`, `invariant` (with `.then(...)` or
`.has/.none(...)`), `required`, `exclusive`, and `after(...).assert(...)`, single- or
multi-entity:

```rdra
entity Appointment "Appointment" {
  id: Int @pk
  status: Enum(scheduled, checkedin) @default(scheduled)
  checked_in_at: DateTime @null
}
invariant(Appointment)
  .when(status == checkedin)
  .then(checked_in_at == present)
  .label("A checked-in appointment records when the patient arrived")
```

The label takes one quoted string, and a rule takes one label (`RuleLabelArg`,
`DuplicateRuleLabel`). State the rule in plain words, not as the formula. A violation of a
labelled rule opens with `「label」: `, and `rdra-ish list --kind rule` lists every labelled
rule next to the [conditions](#conditions) as one business-rule table. The invariant names
(`Forbidden_Order_0`) do not change. `lint` reports an unlabelled rule as the `info` row
`rule-unlabeled`.

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
every entity of the scope. Otherwise the rule is `UnbindableScope`, an error. A path entity
that the scope does not name, such as `J` or the middle of
`.along(AccessSession, Terminal, TerminalCertAssignment, ClientCertificate)`, only links the
ends: the rule counts a pair of scope instances when some instances of the middle entities
connect them. An unlinked
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
  eventually(Order.status == delivered or Order.status == cancelled)

property NeverBoth "mutex"
  always(not (Order.status == paid and Order.status == cancelled))
```

| Form | TLA+ |
|---|---|
| `always(expr)` | `[]expr` |
| `eventually(expr)` | `<>expr` |
| `leads_to(p, q)` | `p ~> q` |

Atoms use `Entity.column == value` / `!=`, `Entity.column in (a, b)` / `not in`, or numeric
comparisons (`<`, `>`, `<=`, `>=`) over Int / Money / Decimal columns and literals. Prefer
connectives `and` / `or` / `not` (`/\` `\/` `~` remain aliases). See [formal-verification.md](./formal-verification.md).

The property label string is optional: `property StockOk always(Item.stock >= Item.selling)`.

When any `eventually` / `leads_to` property is exported, the Spec includes
`WF_vars(Next)`: some step is taken while one can be, but no step in particular. Another
instance's loop satisfies it, so a property about one instance can fail on a run that only
ever moves the others. Temporal formulas are quantified over the present instances of each
entity. `after(UseCase).assert(...)` becomes an independent TLA `PROPERTY`,
`[][(\A p : UC(p) => post')]_vars`, that checks the state each step leaves and is **not**
injected into the effects. Every constraint is its own `INVARIANT`; see
[formal-verification.md](./formal-verification.md).

`sets(UC, Entity, intCol == 3)` assigns an Int effect. Int, Money, Decimal, Date, and
DateTime columns that something names become `0..5` variables in `states` and in the TLA+
export (nullable ones add `null`). The export domain is a fixed approximation (default
`0..5`, override with `--int-range-max`), and an Int literal outside it is
`IntLiteralOutOfBounds`. `@default(0)` is valid on Int columns.

`invariant(Entity).when(conditions...).has/none(Related.col == val)` declares a to-many
quantifier rule. It ranges over the present instances: `none` fails in a reachable state where the
anchor condition holds and some related instance satisfies the related condition. A
`relate` between the two entities links the related instance through its owner; without
one, the related side is independent. The related condition reads the related instance,
also when the related entity is the anchor's own: `invariant(Task).when(status == done)
.none(Task.status == open)` fails while one task is done and another open. A one-entity `invariant` takes the related entity
from the qualified columns of `.has/.none(...)`, so it is the same rule as
`invariant(Entity, Related)`, and `invariant()` takes its entity from the qualified
columns of `.when(...)`. `.has(Related)` and `.none(Related)` name the entity alone and
range over every linked row: `invariant(Order).when(status == closed).has(Invoice)` says a
closed order has an invoice, and reads only the link. A `.has(...)` or `.none(...)` that
names nothing, as `.none()`, is
`QuantifierWithoutCondition`, and `invariant()` with no entity in its scope or its
`.when(...)` is `QuantifierScopeUnknown`. `states` runs one instance per entity, so use
`verify --instance-count 2` for rules that need two related instances. To make a use case
respect such a rule, put the same `.none(...)` on a `holds` guard (see Conditions).

#### Fairness: `.fair(UseCase, ...)`

`.fair(...)` after an `eventually` or `leads_to` names the use cases whose steps the property
assumes are not starved:

```
property GrantEnds "a granted application is registered or dismissed"
  leads_to(Application.status == granted, Application.status in (registered, dismissed)).fair(Register, Dismiss)
```

Each listed use case is weakly fair **per instance it acts on**: if its step on one
application stays enabled, it is eventually taken on that application, however many steps
other applications take meanwhile. The property then holds on every run that meets that
assumption. In TLA+ the assumption is an operator, and the property is the implication:

```
Fair_GrantEnds ==
  /\ \A p0 \in Application_Ids : WF_vars(Register(p0))
  /\ \A p0 \in Application_Ids : WF_vars(Dismiss(p0))
GrantEnds == Fair_GrantEnds => (\A b0 \in Application_Ids : ...)
```

The `\A` ranges over the instances the use case reads or creates under; an instance it
creates is bound inside, `WF_vars(\E p1 \in Patent_Ids : Register(p0, p1))`, so the next
free slot moving on does not reset an application's claim. A use case split into
transaction groups is fair on each of its actions. `Spec`, the `.cfg`, and properties without
`.fair` are unchanged; `Spec /\ Fair_P` satisfies `P` exactly when `Spec` satisfies
`Fair_P => P`.

- Per instance, not per use case: `WF_vars(\E p : Register(p))` is met when Register runs on
  any application, so it does not promise that each application finishes.
- Weak fairness only. A step that is enabled, disabled, and enabled again forever is not
  forced; no strong fairness (`SF`) form exists.
- No model-wide switch: fairness on every use case would change the verdict of every
  existing property and assume every actor always acts, which hides real liveness gaps. Name
  the use cases each property relies on.
- `.fair` also keeps the property on its cone: `verify` checks a property on the cone of
  what it reads and of the listed use cases, and a property whose verdict rests on
  `WF_vars(Next)` alone needs the whole model (see
  [formal-verification.md](./formal-verification.md#temporal-properties-on-a-cone)).
- `check` and `states` run BFS, which evaluates no temporal property, so `.fair` changes
  nothing there; `verify` prints `<Property> assumes weak fairness per instance on ...` next
  to its temporal runs.

Errors: `.fair` on `always(...)` is `FairOnSafetyProperty` (fairness cannot change a safety
verdict); a `.fair` with no argument, an argument that is not a use case, a use case listed
twice, or a second `.fair` is `FairRequiresUseCases`; an undefined name is `UndefinedSymbol`;
any other method after a property is `UnknownChainMethod`. A listed use case that changes
nothing is `FairOnInertUseCase`.

### Comparison Expressions in Constraints

`forbidden`, `required`, `exclusive`, multi-entity `forbidden`/`invariant`, and the `.when()` / `.then()`
clauses of `invariant` accept bare **comparison expressions** as
conditions (`col == val`, `col op rhs`, `col in (a, b)`).

#### Syntax

```
col op rhs
col in (v, ...)
col not in (v, ...)
```

Where:

- `col` is a bare column name on the left-hand side, or `Entity.column` in a
  cross-entity constraint.
- `op` is one of: `<`, `>`, `<=`, `>=`, `==`, `!=`.
- `rhs` is one of:
  - A value of the left-hand column's type, with `==` or `!=`: `status == paid`,
    `status != cancelled`, `active == false`, `paid_at == null`
  - A bare column name or `Entity.column` reference: `stock >= selling`,
    `Payment.amount >= Order.total`
  - An integer literal: `quantity >= 0`
  - The built-in keyword `now`: `expired_at < now`

The type of the left-hand column decides what a bare right-hand side is; see
[Bare identifiers](#bare-identifiers).

#### Set membership: `in` and `not in`

`col in (a, b, c)` holds when `col` is one of the values; it is `col == a or col == b or
col == c`. `col not in (a, b, c)` holds when `col` is none of them; it is `col != a and
col != b and col != c`. Every construct that takes a comparison takes a set membership:
`forbidden`, `required`, `exclusive`, `invariant` with its `.when()`, `.then()`, `.has()`,
and `.none()` (one entity or several, with `Entity.col in (...)`), `holds`,
`after(...).assert(...)`, `@check`, `@derived`, and `property`. `sets` and `drives` do not: a
use case leaves one value (`EffectMembership`). A set that lists every value of the column
is true, and its `not in` false, when the lifecycle builds its system: `.none(Child.status
in (<every value>))` says "no child row" and pulls no column into a cone. It stays valid and
draws no warning, but `.none(Child)` says the same without listing the values.

```rdra
forbidden(Order, status in (paid, shipped), refund_id == present)
invariant(Order).when(status not in (draft, cancelled)).then(submitted_at == present)
holds(Open, Order.status in (draft, submitted))
required(Ticket, priority in (1, 2, 3))
```

| Rule | Error |
|---|---|
| The left-hand column is Enum, variation-typed, Bool, or `@null`; Int, Money, and Decimal columns take integer literals. Any other column type is an error. | `SetColumnNotEnumerable` |
| Each value is a value of the left-hand column's type, read as a bare right-hand side of `==` is. A column name is no value, and a set has no column spelling. The error lists the column's values. | `SetValueNotAValue` |
| Each value appears once. | `DuplicateSetValue` |
| The list names at least one value. | parse error |

`in` is a keyword only between a comparison's left-hand column and `(`, so a column may
still be named `in` (`in in (1, 2)` is valid). `states`, `check`, and TLC read the
disjunction alike; the TLA+ export writes `Order_status[b0] \in {"paid", "shipped"}` and
`\notin`. `list --kind rule`, `list --kind condition`, the sequence diagram's guards, and a
violation print the set as written, `status in (paid, shipped)`, with value labels where
they print labels for `==`.

#### Type rules

| Operator | Permitted column types |
|---|---|
| `<`, `>`, `<=`, `>=` | `Int`, `Money`, `Decimal`, `Date`, `DateTime` |
| `==`, `!=` | All column types |
| `in`, `not in` | `Enum`, variation, `Bool`, `@null`, and `Int`, `Money`, `Decimal` (integer literals) |

When `rhs` is a column reference, the left-hand and right-hand columns must belong to
the same type category (numeric with numeric, temporal with temporal). Two Enum,
variation, or Bool columns compare with `==` / `!=` when they have one set of values,
written `status != Order.previous`. When `rhs` is `now`, the left-hand column must be of
type `Date` or `DateTime`.

#### Semantics and interaction with `sets`

A comparison expression is evaluated on the values of its columns: Int, Money, Decimal,
Date, and DateTime columns that it names become `0..5` variables, and `now` is a clock
that ticks up to 5. Change the values with `sets(UC, Entity, col == N)`. To say that a
use case makes the comparison true or false without modeling the values, drive it
explicitly with `drives`, and the comparison becomes a Bool variable:

```
// Declare that the Sell use case causes (stock < selling) to become true
drives(Sell,   Stock, stock < selling, true)

// Declare that the Refund use case causes (stock < selling) to become false
drives(Refund, Stock, stock < selling, false)
```

A comparison that no `drives` names is not a variable. It reads the Int values, so a
`forbidden(Stock, stock < selling)` is violated in any reachable state where `stock` is
below `selling`, and `required` and `exclusive` read the values the same way. `states`,
`check`, and TLC read it the same way.

### Bare identifiers

Column names and values inside `forbidden(...)`, `required(...)`, and `exclusive(...)`
arguments, plus `.when()` / `.then()` clauses, are **bare identifiers** (or comparison
expressions). They use the same value vocabulary as `sets`: Enum variant names,
variation values, `true` / `false`, and `present` / `null`.

The left-hand column's type decides what a bare right-hand side `x` of `col == x` or
`col != x` is, and what each value of `col in (x, y)` is. Every construct that takes a
comparison reads it the same way: `forbidden`, `required`, `exclusive`, `invariant` with its
`.when()`, `.then()`, `.has()`, and `.none()`, `holds`, `after(...).assert(...)`, `sets`,
`property`, `@check`, and `@derived`.

| Left-hand column | A bare `x` is |
|---|---|
| Enum, variation, or Bool | a value of the column. `x` that is no value is an error, also when a column is named `x`. |
| any other `@null` column | the value `null` or `present`; any other `x` is a column. |
| any other column | a column. |

So `status == paid` is the value `paid` even when the entity also has a column `paid`, and
an Enum value needs no prefix to stay apart from the column names; values only need to be
unique within their column. A comparison of two Enum, variation, or Bool columns qualifies
the right-hand side: `status != Order.previous`. Writing it bare fails with
`ComparisonRhsNotAValue`, whose hint gives the qualified spelling:

```
'previous' is not a value of column 'status' (pending, paid, cancelled), and a bare right-hand side of 'status' is a value
  hint: write `Order.previous` to compare columns
```

`transitions(UseCase, Entity.col, from -> to)` and `@default(v)` take values only.

A value of an Enum or a variation is always a bare token; a quoted string is free text, as
in [metadata clauses](#metadata-clause-values). `chooses(Pay, Order.kind, "a")` and
`@default("open")` on an Enum or variation column are `QuotedValue`, whose hint gives the
bare spelling (`write chooses(Pay, Order.kind, a)`, `write @default(open)`). The comparisons
above, `in (...)` / `not in (...)` lists, `transitions` endpoints, and the values of `Enum(...)`
and `variation` declarations take no quoted value at all: a quoted one is a syntax error.
`@default("4")` on an Int column, or a quoted default of a String column, stays as written.

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
| `state` | Delete the declaration. States are the variants of the lifecycle Enum column named in `transitions(UseCase, Entity.col, from -> to)`; the analyzer derives one `{Entity}_{variant}` state per variant for `lint` and the diagrams, which name it `state__{Entity}_{variant}` (`{Entity}_{column}_{variant}` on an entity with [several lifecycle columns](#several-lifecycle-columns-on-one-entity)). |
| `concept`, `domain_object`, `aggregate`, `value_object` / `valueobject`, and `maps_to` | `entity` for persistent structure, `dto` for payload shapes. |

A top-level `when(...)` rule was removed too: a rule over related rows is an `invariant`,
and a guard is a `holds`. `when(Entity, conditions...).none(...)` and
`when(Entity.col == val).none(...)` fail to load with the rewrite to write,
`invariant(Entity).when(...).none(...)`, whose entity is the leading argument or the entity
of the first qualified column, with the rest of the chain, `.label(...)` included, as
written.

### Deprecated and removed forms

A form leaves the language in two stages:

1. **Deprecated** in a minor release. The model still loads: the parser rewrites the form
   to its current one, and the load reports a `DeprecatedSyntax` warning that names the
   rewritten line, the version that deprecated the form, and the version that removes it.
   `check` and the other commands still succeed. `lint` lists the warning under the rule
   `deprecated-syntax`. `check --deny-deprecated` turns it into an error, for CI.
   `rdra-ish fmt --write` rewrites every deprecated form in place.
2. **Removed** no earlier than the next major release. A model that still uses the form
   fails to load, with a message that gives the rewritten line.

A deprecation warning looks like this:

```text
warning: model.rdra:14:1: deprecated syntax in 'model.rdra': `sets` with a true/false argument drives a comparison, which is now `drives`; write `drives(Sell, Stock, stock < selling, true)` (deprecated since 0.5.0, removed in 2.0.0; `rdra-ish fmt --write` rewrites it)
```

`LEGACY_FORMS` in `crates/rdra-ish-syntax/src/removed.rs` is the one list of these forms,
with each form's status and versions.

| Deprecated | Since | Removed in | Use instead |
|---|---|---|---|
| `sets(UseCase, Entity, comparison, true \| false)` | 0.5.0 | 2.0.0 | `drives(UseCase, Entity, comparison, true \| false)`. `sets` assigns a value; a comparison driven true or false is [`drives`](#driving-comparison-propositions-with-drives). |

The forms below were removed before 1.0, without a deprecated stage.

These spellings were removed in 0.4.0 so that each predicate has one form: the use case
comes first, a column is always `Entity.column`, and each keyword is one word.
`scripts/migrate-syntax-v2.py <workspace>` rewrites all four in place and is safe to run
twice.

| Removed | Use instead |
|---|---|
| `transitions(Entity.column, UseCase, from -> to)` | `transitions(UseCase, Entity.column, from -> to)`. |
| `maps_field(Field, Entity, "column")`, `maps_field(Field, Entity, column)`, `maps_field(Field, Entity, Entity.column)` | `maps_field(Field, Entity.column)`. |
| `acceptance criteria "..."`, `acceptance "..."` | `acceptance_criteria "..."`. |
| `writes(UseCase \| Api, Entity)` | `updates`, or `creates` / `deletes` when that is what the use case does. `writes` was an update under another name; the CRUD matrix now shows `U` where it showed `W`. |

---

## See Also

- [state-derivation.md](./state-derivation.md) — how reachable states are explored,
  how predicates become actions and guards, and how entity constraints are checked.
- [cli-reference.md](./cli-reference.md) — the `rdra-ish` command-line interface,
  including the `diagram --kind event` command for event causality visualisation.
