# Language reference

This is the reference for the `rdra-ish` DSL. One model runs from why a system exists to how
its data changes. You declare each element once as a typed instance and connect elements
with predicate calls. `check` and `lint` find gaps across layers, and every diagram, table,
and export is a projection of the same model. The [CLI reference](./cli-reference.md) says
what each command does with it.

A source file (`.rdra`) holds an optional `module` declaration, `import` declarations, and,
in any order, instance declarations, entity bodies, predicate calls, and rules (`forbidden`,
`invariant`, `required`, `exclusive`, `after`, `property`). The kind `constraint` is
something else: an NFR-style guardrail (see [Requirements and traceability](#requirements-and-traceability)).

Each construct below gives its syntax, its meaning, its rules and diagnostics, and an
example. A diagnostic named here is an `error` unless it says otherwise.

## The review spine

<!-- derived-from ./rdra-ish-interpretation.md#basic-stance -->

Each layer is explained by the layer before it and answers one review question. The sections
follow the same order.

| Layer | Kinds | Review question | Section |
|---|---|---|---|
| Requirements and traceability | `goal`, `requirement`, `adr`, `nfr`, `quality`, `constraint` | Why does this exist, and which quality bar applies? | [Requirements and traceability](#requirements-and-traceability) |
| Business | `business`, `buc`, `flow`, `step`, `timer`, `medium` | Which business value does it serve, in what order? | [Business layer](#business-layer) |
| Interaction | `actor`, `usecase`, `screen`, `field`, `event` | Who does what, on which screen, and what does it cause? | [Interaction layer](#interaction-layer) |
| System boundary | `system`, `extsystem`, `api`, `dto`, `permission` | Which boundary owns the operation, and who may call it? | [System boundary](#system-boundary) |
| Data | `entity`, `variation`, `condition` | What is stored, and how is it related? | [Data layer](#data-layer) |
| Lifecycle and rules | `transitions`, `chooses`, `sets`, `drives`, `links`, the rules, `after(...).assert(...)`, `property` | Which states are reachable, and which are forbidden? | [Lifecycle](#lifecycle), [Entity state constraints](#entity-state-constraints) |

A kind is part of the language only when some review output reads it; the
[kind index](#kind-index) names that output.

---

## File structure and comments

`//` starts a comment that runs to the end of the line; `/* ... */` is a block comment. A `#`
line comment is a parse error: replace it with `//` (see the [changelog](../CHANGELOG.md)).

---

## Modules and imports

```rdra
module shared.actors

import shared.actors                // every name, in flat scope
import shared.actors as a           // names under the alias: a.Staff
import shared.actors.{Staff}        // one name, in flat scope
import shared.actors.{Staff as S}   // one name under a local alias
```

- `module <path>` declares the file's dotted module path, unique across the loaded files.
- An import names a module, not a file: a loaded file that declares `module <path>` answers it,
  whatever it is called. A module no loaded file declares is `UnknownImportModule`, whose hint
  lists the loaded files that declare a similar module.
- `as <alias>` keeps the names out of flat scope. A `{Name as Local}` hides the original name.
- Two flat imports that bind the same local name and the same kind to different modules are an
  error. The same id of different kinds may coexist (an imported `event Capture` beside a local
  `usecase Capture`); write `kind::Id` to pick one.
- The same id may be declared in different modules; tell them apart with aliases. Redeclaring a
  kind and id in one module, or in one file without a module, is an error.
- Visibility is per file: only a file with an `import` becomes a closed scope. Files without
  imports resolve bare names across every loaded file.
- One `as` alias for two modules is `DuplicateAlias`.

**Loading.** The CLI and the LSP merge every `.rdra` file reachable from the inputs into one
model. An import resolves first by the `module` lines of the loaded files. A file not loaded
yet is found by path: `import a.b` loads `a/b.rdra` from the include paths, which are the
parent and grandparent of each input file after symlinks are resolved. Load the directory when
a module's file is named otherwise. `rdra.toml` does not change resolution (see
[Project manifest](./cli-reference.md#project-manifest)).

---

## Kind-qualified references

A bare id references an instance when it is unambiguous. When one id names several kinds
after all imports are merged, prefix the kind; a bare reference is an ambiguity error:

```rdra
contains(BucOrder, usecase::Cancel)
raises(usecase::Cancel, event::Cancel)
```

---

## Instance declarations

```
<kind> <Id> "Label" [description "Text"] [clauses...]
```

- `<kind>` is one of the [kind index](#kind-index).
- `<Id>` is unique within its kind; UpperCamelCase by convention (`lint` `naming-id`).
- `"Label"` is the display string of diagrams and lists.
- `description` is stored on every kind. `diagram` writes it as a `%% desc <id>: <text>`
  line after the node.

### Metadata clause values

A metadata clause is a keyword and one value: `description`, the requirement, ADR, NFR, goal,
and constraint clauses, the API contract clauses, the field clauses, and the use case review
clauses. The value is either:

- a quoted string for free text: `rationale "Checkout failures block revenue."`, `target "<=300"`;
- a bare token for one identifier or one number: `priority must`, `metric p95_latency_ms`,
  `target 300`, `slo 99.9`.

Anything else (spaces, `-`, `<`, `%`, non-ASCII text) needs quotes. `fmt` keeps the form
written. The field clause `required` takes only `true` or `false`.

### Kind index

| kind | What it is | Reviewed by |
|---|---|---|
| `goal` | An outcome the requirements serve, with its metric, baseline, target, and window; its description states the problem. | `lint` `goal-unmeasured`; RDRA and requirement diagrams; `list --kind goal` |
| `requirement` | A requirement that motivates BUCs and achieves goals. | `lint` `requirement-untraced`, `requirement-no-goal`; RDRA and requirement diagrams; `list --kind requirement` |
| `adr` | An architecture decision record linked by `decides` to what it affects. | no diagram (a diagram's `omitted:` names `adr`); `list --kind adr`, `adr-impact` |
| `nfr` | A non-functional requirement with measurable quality metadata. | `lint` `nfr-unscoped`; RDRA diagram; `list --kind nfr` |
| `quality` | A quality attribute category (performance, availability, ...). | RDRA diagram (`qualifies`); `list --kind quality` |
| `constraint` | A non-functional guardrail: audit, logging, retention, privacy. | `lint` `constraint-unscoped`; RDRA diagram; `list --kind constraint` |
| `business` | A coarse area of activity BUCs belong to. | RDRA and business diagrams |
| `buc` | A business use case: a unit of business value made of use cases. | `lint` `buc-empty`; RDRA, requirement, business, and context diagrams; `list --kind buc-roles`; every `--buc` scope |
| `flow` | An ordered business narrative inside a BUC, made of steps. | `lint` `flow-empty`, `flow-cycle`; RDRA and flow diagrams |
| `step` | A business-language action in a flow that can cover a use case, API, event, or timer. | `lint` `step-no-cover`; RDRA and flow diagrams |
| `timer` | An RDRA タイマー: a schedule that starts a use case. See [Timers](#timers). | RDRA, usecase, flow, and event diagrams; `lint` `orphan-node` |
| `medium` | A device, terminal, or operation medium. | `csv --kind screen-constraints`; RDRA diagram. `belongs(...).by(...)` records it; no output reads that yet. |
| `actor` | A human who performs use cases. | `check` permission paths; RDRA, context, and usecase diagrams; `list --kind buc-roles`; `csv --kind actor-permission-audit` |
| `usecase` | A concrete interaction that reads and writes entities and displays screens. | `check`; `states`; most diagrams; `csv --kind matrix` |
| `screen` | A UI surface use cases display. | `lint` `screen-no-fields`, `screen-no-transition`; RDRA, usecase, and screen diagrams; `csv --kind screen-constraints` |
| `field` | A screen input or output item that may map to a column. | `lint` `field-unmapped`, `field-unplaced`; `csv --kind business-inputs` |
| `event` | A domain event between use cases and external systems. | `check` event links; `states`; RDRA, usecase, flow, and event diagrams; `export --kind asyncapi`, `tla` |
| `system` | An internal system boundary that groups APIs; its entities are derived from their CRUD. | `check` boundary diagnostics; RDRA and context diagrams; `csv --kind api-matrix`; `next` stage 4 |
| `extsystem` | An external system, reached through events. See [External systems](#external-systems). | RDRA, context, usecase, and event diagrams; `list --kind buc-roles`; `export --kind asyncapi`; `lint` `orphan-node` |
| `api` | An API-layer endpoint a use case invokes: an atomic data-operation boundary with optional HTTP metadata. | `lint` `api-unused`, `api-contract-missing`, `api-contract-incomplete`; `check` API diagnostics; RDRA, flow, and condition diagrams; `csv --kind api-matrix`; `export --kind openapi` |
| `dto` | A request, response, or error payload shape, kept apart from entities. | `lint` `dto-unused`, `naming-dto-field`; `export --kind openapi`, `json-schema` |
| `permission` | An authority assignable to actors. | `check` `requires_permission` against `has_permission`; `csv --kind actor-permission-audit` |
| `entity` | A logical data model: a table-like persistent structure. | `check`; `states`; RDRA, usecase, information, and ER diagrams; `list --kind transition`; `csv --kind matrix`; `export --kind dbml`, `sql`, `json-schema`, `tla`; `lint` `enum-attribute` |
| `variation` | A closed set of values an actor chooses from (an RDRA バリエーション). Types attribute columns; never a lifecycle. See [Variations](#variations). | `lint` `orphan-node`; `check` `UnknownColumnType`, `InvalidVariationValue`; ER, information, and condition diagrams; `csv --kind entity`; `export --kind dbml`, `json-schema`, `openapi`; `list --kind variation` |
| `condition` | An RDRA 条件: variations and a state model that select a business rule. With a `holds` body it guards what it applies to. See [Conditions](#conditions). | `lint` `condition-no-rdra-anchor`, `condition-applied-without-body`, `orphan-node`; `check` `ConditionCombinesWithBody`, `ConditionSecondBody`, `ConditionCombinesAttribute`, `AmbiguousCombinesTarget`; RDRA, condition, and usecase diagrams; `list --kind condition`; `states`, `check`, `export --kind tla`, `verify` read a body as a guard |

---

## Relationship predicates

Predicates connect declared instances by id (kind-qualified when ambiguous).

| Predicate | Signature | Meaning |
|---|---|---|
| `performs` | `(Actor, UseCase \| Buc \| Step)` | The actor performs it. On a BUC, every use case of the BUC with their permissions (see [`api` and the API layer](#api-and-the-api-layer)). |
| `invokes` | `(UseCase, Api)` | The use case calls the API. The API's writes are one transaction of the use case. |
| `request`, `response`, `error_response` | `(Api, Dto)` | The API's request, success, or error payload. |
| `applies_to` | `(Nfr \| Condition, UseCase \| Api \| System)` | An NFR governs the target, or a [condition](#conditions) guards it. |
| `combines` | `(Condition, Variation \| Entity.column)` | Links a condition to a variation or a lifecycle column. Not on a condition with a `holds` body. |
| `holds` | `(Condition, comparison, ...)` | The condition's body: comparisons, AND-ed. At most one per condition. |
| `qualifies` | `(Nfr \| Constraint, Quality)` | The NFR or constraint belongs to the quality category. |
| `constrains` | `(Constraint, UseCase \| Api \| System \| Entity \| Dto)` | The constraint applies to the target. |
| `reads` | `(UseCase \| Api, Entity)` | Reads the entity. Chain methods give the [query shape](#query-shapes). |
| `creates` | `(UseCase \| Api, Entity)` | Creates a row. |
| `updates` | `(UseCase \| Api, Entity)` | Updates a row. |
| `deletes` | `(UseCase \| Api, Entity)` | Deletes one row; see [Deleting a parent](#deleting-a-parent). |
| `displays` | `(UseCase, Screen)` | The use case displays the screen. |
| `shows` | `(Screen, Entity)` | The screen shows the entity's information. |
| `navigates` | `(Screen, Screen)` | The user moves between screens; see [Screen transitions](#screen-transitions). |
| `maps_field` | `(Field \| Dto.field, Entity.column)` | Maps a field to a column; see [Screen fields](#screen-fields-and-inputoutput-mapping) and [Mapping DTO fields](#mapping-dto-fields-to-columns). |
| `raises` | `(UseCase \| ExtSystem, Event)` | Raises the event. From an external system, the inbound direction. |
| `triggers` | `(Event \| Timer, UseCase \| Buc \| ExtSystem)` | Starts the use case or BUC, or delivers the event to the external system (outbound). A timer triggers only a use case or BUC. |
| `outbox` | `(Event)` | The event is published outside the model; silences the raised-but-unconsumed warning only. |
| `contains` | `(Buc, UseCase \| Flow)`, `(Flow, Step)`, `(Screen, Field)`, `(System, Api)` | Composition. |
| `owns` | `(System, Entity)` | The system is responsible for the entity, before API operations say so. |
| `exposes` | `(System, UseCase \| Event \| Entity)` | Publishes the element; see [Publishing to other models](#publishing-to-other-models). |
| `shares` | `(System, Entity)` | Lets other models name an internal entity (warning `SharedInternalEntity`). |
| `precedes`, `branches`, `excepts`, `repeats` | `(Step, Step)` | Main path, alternative, exception route, loop back. |
| `covers` | `(Step, UseCase \| Api \| Event \| Timer)` | The step anchors the element. |
| `compensates` | `(UseCase, UseCase)` | The first undoes the business effect of the second. Review only; the lifecycle does not read it. |
| `coordinates` | `(UseCase, Entity, Entity)` | The use case keeps a relation across system boundaries consistent; it must invoke an API on each side that operates the entity there. |
| `belongs` | `(Buc, Business)` | The BUC belongs to the business; see [`belongs` context](#belongs-context). |
| `has_permission` | `(Actor, Permission)` | The actor holds the permission. |
| `requires_permission`, `requires_medium` | `(UseCase \| Api, Permission \| Medium)` | The use case or API needs it; see [Access constraints](#access-constraints). |
| `motivates` | `(Requirement, Buc)` | The requirement motivates the BUC. |
| `achieves` | `(Requirement, Goal)` | The requirement serves the goal; several goals allowed. |
| `decides` | `(Adr, Buc \| UseCase \| Api \| System \| Entity \| Requirement \| Nfr \| Constraint \| Dto)` | The decision affects the target. |
| `relate` | `(Entity, Entity, Card)` | An ER relationship that generates key columns; see [`relate` and FK generation](#relate-and-fk-generation). |
| `transitions` | `(UseCase, Entity.column, from -> to)` | A lifecycle edge; see [`transitions`](#transitions-and-the-state-machine). |
| `chooses` | `(UseCase, Entity.column[, value])` | The actor picks a variation value; see [Choosing a value again](#choosing-a-value-again-chooses). |
| `sets` | `(UseCase, Entity.column == value)` | A column effect; see [Value vocabulary for `sets`](#value-vocabulary-for-sets). |
| `drives` | `(UseCase, Entity.column op rhs, true \| false)` | Drives a comparison's truth; see [`drives`](#driving-comparison-propositions-with-drives). |
| `links`, `relinks`, `unlinks` | `(UseCase, Entity, Entity)` | Link, move, or unlink a child; see [Linking](#linking-moving-and-unlinking-an-existing-child-links-relinks-unlinks). |
| `after` | `(UseCase).assert(...)` | An assertion on the state each run of the use case leaves; see [Cross-entity constraints](#cross-entity-constraints). |

### Deleting a parent

`deletes(UC, E)` removes one row. What happens to its children depends on the key's
`.on_delete`:

| `.on_delete` | The lifecycle |
|---|---|
| none, `restrict`, `no_action` | The delete is enabled only when no present child names the row, as the database rejects it otherwise (none is the SQL default, which the export keeps). A child the same step deletes, unlinks, or relinks does not count. Each such key whose parent a use case deletes gets the invariant `DanglingKey_<Child>_<role>`: every present child names a present parent (or none, under `.optional()`). |
| `cascade`, `set_null`, `set_default` | The action does not run yet. The delete is enabled, and the children stay present and still name the deleted row, where every rule sees them. No `DanglingKey_*`. The info `DeleteLeavesChildren` names each use case that deletes a `cascade` parent and not the child; add `deletes(UC, Child)` to remove one child row per run. |

- A step removes one child row per run, so at two instances an Order with two lines still
  blocks the delete, where one SQL statement could remove both.
- A parent with a `@soft_delete` column is marked, not removed: no guard, no `DanglingKey_*`.
- A pre-existing child of a created parent starts naming a parent that does not exist
  (`PreExistingChildOfCreatedParent`), so its key gets no `DanglingKey_*`.
- `.on_update` is not read.
- **A deleted row keeps its keys.** Its columns reset to their base values and each key still
  names its parent. Only a later step that binds the deleted row reads the key: a step of a
  split use case whose earlier step deleted it (see [Runs of a split use case](./state-derivation.md#runs)).
  The native checker, z3, and the TLA+ export read it the same way.

```rdra
relate(Item, Shop, N:1)
invokes(Cancel, ItemApi)
deletes(ItemApi, Item)
invokes(Cancel, ShopApi)
updates(ShopApi, Shop)
transitions(Cancel, Shop.st, open -> flagged)   // Cancel_2 flags the deleted item's shop
```

### Chain methods

A chain method is a `.name(...)` call after a predicate. Any other name is an `unknown chain
method` error (`UnknownChainMethod`), including the removed `.where(...)` and `.when(...)` on
`belongs`.

| Predicate | Chain methods |
|---|---|
| `belongs` | `.by(...)` |
| `navigates` | `.via(UseCase)`, once |
| `transitions` | `.when(Condition)`, once: guards this arm |
| `chooses` | `.when(Condition)`, once, on the form with a value: guards this value |
| `relate` | `.via(...)`, `.optional()`, `.on_delete(...)`, `.on_update(...)`, `.role(...)`, `.key(...)`, `.references(...)` |
| `forbidden` | `.when(Condition)`, `.along(...)`, `.role(...)`, `.label("...")`, `.global()`, `.lemma()` |
| `invariant` | `.when(...)`, `.then(...)`, `.has(...)`, `.none(...)`, `.at_most(n, ...)`, `.at_least(n, ...)`, `.exactly(n, ...)`, `.along(...)`, `.role(...)`, `.label("...")`, `.global()`, `.lemma()` |
| `required`, `exclusive` | `.label("...")` |
| `holds` | `.has(...)`, `.none(...)`, `.at_most(n, ...)`, `.at_least(n, ...)`, `.exactly(n, ...)`, `.along(...)`, `.role(...)` |
| `sets` | `.role(...)` on a copy; `.when(Condition)`, once, on a literal value |
| `after` | `.assert(...)`, `.label("...")` |
| `property` | `.along(...)`, `.role(...)`, `.global()`, `.fair(UseCase, ...)`; `.when(...)` directly after `unchanged(...)` |
| `exposes` | `.append()`, `.upsert()` (an entity), `.caller(actor \| system \| any)` (a use case) |
| `reads` | `.by(...)`, `.order(...)`, `.page(...)`, `.count()`, `.sum(...)`, each once |
| `links`, `relinks`, `unlinks` | `.role(...)` |
| `creates`, `updates`, `deletes` | `.apart()` on a use case's write: the row need not join the use case's other rows, so no `UnjoinedWrite` |
| `triggers` | `.counted(k)`, once, from an event to a use case; see [Counting raises](#counting-raises-counted) |
| every other predicate | none |

---

## Requirements and traceability

`achieves(Requirement, Goal)` ties a requirement to its outcome, `motivates(Requirement, Buc)`
to business value, `decides(Adr, ...)` a decision to what it shapes, and `applies_to` /
`constrains` scope quality objectives and guardrails.

### Goals and non-goals

<!-- derived-from #relationship-predicates -->

A `goal` holds a PRD's problem and how its outcome is measured: the description states the
problem, and `metric`, `baseline`, `target`, and `window` (all optional) the measure. `lint`
reports a goal with no `metric` or no `target` (`goal-unmeasured`) and a requirement that
achieves no goal (`requirement-no-goal`).

```rdra
goal ShortWait "受付から診察までの待ち時間を短くする"
  description "受付後に診察室へ案内されるまでの待ちが長いと、患者が帰ってしまう。"
  metric "受付から診察開始までの中央値"
  baseline "45分"
  target "30分"
  window "月次"

achieves(ReqSwiftCheckIn, ShortWait)
```

A non-goal is a `requirement` with `status "out-of-scope"` and a `rationale`. `lint` asks it for
no goal, and the requirement diagram draws it apart.

```rdra
requirement ReqNoOnlineConsultation "オンライン診療は対象外とする"
  status "out-of-scope"
  rationale "診察は来院して受ける前提で業務を分けているため、ビデオ通話による受付・診察・会計は扱わない。"
```

### Requirements

<!-- derived-from #relationship-predicates -->

```rdra
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

`source`, `stakeholder`, and `acceptance_criteria` may repeat; other clauses keep the last
value. `acceptance criteria` and `acceptance` fail to parse with a message naming
`acceptance_criteria`.

### Architecture decision records

<!-- derived-from #relationship-predicates -->

An `adr` records which option was chosen, which were rejected, and why; a requirement says
what must be true.

```rdra
adr AdrCustomerOutbox "Publish customer changes through outbox"
  adr_status accepted
  context "External subscribers need customer changes."
  decision "Publish customer changes through a transactional outbox."
  consequence "Delivery becomes eventually consistent."
  accepted "Transactional outbox"
  rejected "Synchronous callback"
  reason "Avoid coupling write latency to external subscribers."

decides(AdrCustomerOutbox, CustomerSystem)
decides(AdrCustomerOutbox, Customer)
```

### Non-functional requirements

<!-- derived-from #relationship-predicates -->

`nfr` is a measurable quality objective, `quality` names the attribute, and `applies_to` scopes
the NFR. `constraint` is a guardrail rather than a metric, scoped with `constrains`.

```rdra
quality Performance "Performance"

nfr CheckoutLatency "Checkout latency"
  metric p95_latency_ms
  target "<=300"
  window "5m"
  slo "99.9%"
  availability multi_az
  resilience retryable

applies_to(CheckoutLatency, Checkout)
qualifies(CheckoutLatency, Performance)

constraint AuditRetention "Audit retention"
  audit enabled
  logging structured
  retention "7y"
  privacy restricted

constrains(AuditRetention, CoreSystem)
```

| Clause | Meaning |
|---|---|
| `metric` | Metric name (`p95_latency_ms`, `error_rate`). |
| `target` | Target value (`300`, `"<=300"`, `"<0.1%"`). |
| `window` | Measurement window (`"5m"`, `"30d"`). |
| `slo` | Service-level objective. |
| `availability` | Availability design (`multi_az`, `active_active`). |
| `resilience` | Fault tolerance (`retryable`, `degraded_mode`, `manual_recovery`). |
| `audit`, `logging` | Audit and logging markers. |
| `retention` | Retention period or policy. |
| `privacy` / `privacy_classification` | Privacy classification. |

---

## Business layer

A `business` is a coarse area of activity. A `buc` is one slice of business value in it,
placed with `belongs(Buc, Business)` and composed with `contains(Buc, UseCase)`. Flows and steps
order the work inside a BUC, and `triggers(Event, Buc)` hands work to the next BUC. See
[RDRA-ish Interpretation](./rdra-ish-interpretation.md).

### `belongs` context

```rdra
medium FrontDeskTerminal "Front Desk Terminal"
belongs(BucAppointmentScheduling, ClinicOps).by(FrontDeskTerminal).by("tablet")
```

`.by(...)` records the medium, device, or terminal: a string or a `medium` reference
(`.by(medium::...)`). Several `.by` clauses are alternatives. `.when(...)` was removed with
`timing`; a schedule is a [`timer`](#timers).

### Timers

```rdra
timer IntakeExpiryCheck "Nightly intake expiry check"
triggers(IntakeExpiryCheck, ExpireIntakeForms)
```

- A timer triggers a use case or a BUC, never an external system. Put its use case in a BUC
  like any other; one in no BUC (by `contains` or a covering step) is reported as an
  event-triggered one is.
- A timer is a free trigger: its use case runs whenever `now` and its guards allow, with no
  record of the timer firing. So is a use case a timer starts beside an event
  ([Triggers](./state-derivation.md#triggers)).
- A step may anchor it with `covers(Step, Timer)`.

#### A deadline that depends on a reason

<!-- constrained-by #a-value-that-follows-a-condition-when -->

A timer has no parameter: it is about the rows its use case binds, which a condition on that
use case or arm reads, `Date` columns compared with `now` included. So a deadline is a `Date`
column on the row; the timer's use case waits for `due_on <= now`; and the use case that
starts the period sets `due_on` from the reason, one guarded `sets` per reason
([A value that follows a condition](#a-value-that-follows-a-condition-when)):

```rdra
variation ExtensionReason "Extension reason" {
  disaster_area "Disaster area (60 days)"
  expert_inquiry "Expert inquiry (90 days)"
}
entity Claim "Claim" {
  id: Int @pk
  period: Enum(running, extended, overdue) @default(running)
}
entity Extension "Extension" {
  id: Int @pk
  reason: ExtensionReason
  due_on: Date
}
relate(Extension, Claim, 1:1)

creates(ExtendPeriod, Extension)
updates(ExtendPeriod, Claim)
transitions(ExtendPeriod, Claim.period, running -> extended)
condition DisasterArea "A disaster-area survey"
holds(DisasterArea, Extension.reason == disaster_area)
condition ExpertInquiry "An expert inquiry"
holds(ExpertInquiry, Extension.reason == expert_inquiry)
sets(ExtendPeriod, Extension.due_on == 2).when(DisasterArea)
sets(ExtendPeriod, Extension.due_on == 3).when(ExpertInquiry)

timer PaymentDeadline "Payment deadline"
triggers(PaymentDeadline, FlagOverdue)
updates(FlagOverdue, Claim)
condition DeadlinePassed "The extended deadline has come"
holds(DeadlinePassed, Extension.due_on <= now)
transitions(FlagOverdue, Claim.period, extended -> overdue).when(DeadlinePassed)

invariant(Extension).when(reason == expert_inquiry).then(due_on == 3)
forbidden(Claim, Extension, Claim.period == overdue, Extension.due_on > now).along(Extension, Claim)
```

- Each `sets(..).when(..)` reads the reason the actor picked on the created row. The `invariant`
  fails if the expert inquiry's line sets another mark.
- `.when(DeadlinePassed)` reads the claim's extension through the `relate`. Without it the
  `forbidden` fails: the timer flags a claim before its deadline.
- The clock counts marks, not days. `now` starts at 0 and ticks to the top of the Int range
  (`0..5` by default), shared by every row. Pick marks that keep the periods' order (60 days is
  2, 90 is 3). The checks say which deadline comes first and that nothing passes it early.

The older form also works: one timer per period, each use case guarded by the reason. It needs
no `Date` column, but no rule can say when the period ends.

### Business flows

```rdra
contains(BucCheckout, CheckoutFlow)
contains(CheckoutFlow, ReviewCart)
contains(CheckoutFlow, AuthorizePayment)
precedes(ReviewCart, AuthorizePayment)
excepts(AuthorizePayment, PaymentFailed)
repeats(PaymentFailed, ReviewCart)
covers(AuthorizePayment, CapturePayment)
covers(PaymentFailed, PaymentRejected)
```

A flow is optional. `precedes` is the main path, `branches` an alternative, `excepts` an
exception route, `repeats` a loop. A step is not the use case or event it covers: `covers`
anchors model elements to a business meaning, apart from `contains`.

### Event-triggered BUCs

<!-- constrained-by #relationship-predicates -->
<!-- derived-from ./rdra-ish-interpretation.md#business-flow -->

To describe a BUC that an event starts:

1. Declare the target BUC.
2. Write `triggers(Event, TargetBuc)`: the event starts that slice of value.
3. Once the entry action is known, add `contains(TargetBuc, EntryUseCase)` and, optionally,
   `triggers(Event, EntryUseCase)`. Both edges may stay.

```rdra
triggers(event::EncounterSigned, BucBillingClaims)
contains(BucBillingClaims, GenerateClaim)
triggers(event::EncounterSigned, GenerateClaim)
```

A use case a timer or event starts and that no `performs(Actor, UseCase)` names is the
system's work: the actors of its BUC do not perform it, and `check` asks no actor to hold its
permissions or those of its APIs. Once a `performs(Actor, UseCase)` names an actor, that actor
must hold them.

---

## Interaction layer

A use case is the concrete interaction: it displays screens, invokes APIs, touches entities,
and raises events. Most checks anchor here.

### Use case conditions

```rdra
usecase CapturePayment "Capture payment"
  precondition "The order is authorized."
  guard "Payment provider is available."
  postcondition "Payment is captured or a business error is raised."
  alternative "Customer chooses another payment method."
  error "Authorization expires before capture."
```

These clauses are review text, shown by `list --kind usecase` and not evaluated by `check`,
`states`, or `verify`. Each may repeat; `alternative_flow` spells `alternative`, and
`error_condition` and `business_error` spell `error`. For a checked postcondition write
`after(UseCase).assert(...)`; for a guard, a [condition](#conditions). `compensates(Refund,
Capture)` records a compensation.

### Screen fields and input/output mapping

```rdra
field ShippingAddress "Shipping address"
  access editable
  required true
  source actor

contains(CheckoutScreen, ShippingAddress)
maps_field(ShippingAddress, Order.shipping_address)
```

| Clause | Meaning |
|---|---|
| `access` | `editable` or `readonly` by convention; any token is accepted. |
| `required` | `true` or `false`. |
| `source` / `input` / `derived` | `actor` or `system` by convention; any token is accepted. |

`shows(Screen, Entity)` is the coarse link; `maps_field(Field, Entity.column)` maps one item to
one column, which must exist. `lint` `field-column-unwritten` warns on a field mapped to a
`@null` column that no `sets` and no `chooses` fills: add `sets(UseCase, Entity.column ==
present)` to the use case that records it.

### Screen transitions

```rdra
navigates(CheckInScreen, EligibilityScreen).via(CheckInPatient)
```

`navigates(From, To)` says the user moves between the screens; `.via(UseCase)`, optional,
names the operation that causes it. A pair of screens may have one `navigates` per operation.
Both arguments must be screens. A `.via` that does not name exactly one use case, once, is
`NavigatesViaRequiresUseCase`. `diagram --kind screen` draws it, and, where none is declared,
the order a flow visits the screens ([Kinds](./cli-reference.md#kinds)).

### `triggers` and event-driven BUC chaining

- `triggers(event::E, buc::B)` says the event starts a downstream BUC, when the entry use case
  is not fixed yet.
- `triggers(event::E, usecase::UC)` says the event runs a concrete use case. A triggered use
  case in no BUC is a warning.

**Order.** A use case an event triggers runs only after a use case that raises the event ran,
on the rows it ran on. The last step of each raiser records the event on its rows; the first
step of the triggered use case runs only on a row that carries the record: its own row of that
entity, its parent through the raiser's key, or an extra row joined to its own by the one key
between them ([Triggers](./state-derivation.md#triggers)). Below, `IssueCert` runs only after
`ExecuteCertIssue` raised `EvOrderExecuted` on the order, so `check` and `verify
--instance-count 2` pass:

```rdra
transitions(ExecuteCertIssue, CertificateOrder.status, requested -> executed)
raises(ExecuteCertIssue, EvOrderExecuted)
triggers(EvOrderExecuted, IssueCert)
creates(IssueCert, ClientCertificate)
relate(ClientCertificate, CertificateOrder, N:1)
after(IssueCert).assert(CertificateOrder.status == executed)
```

- The record is a latch: once raised on a row, the triggered use case runs there as often as
  its states and guards allow, and a second raise changes nothing (`.counted(k)` counts, see
  [Counting raises](#counting-raises-counted)).
- A use case several events trigger runs after any of them; `check` and `verify` judge it as
  one use case, and a trace names the copy that ran after `Ev` as `<UC>_by_<Ev>`.
- A raiser never waits for the use cases its event triggers.
- The infos `UnanchoredTrigger` and `UnjoinedTrigger` report how a trigger's rows tie to the
  raiser's; [Known verification gaps](./formal-verification.md#known-verification-gaps) names
  what the order leaves open.
- A guard that only restates the order is not needed. A guard that states what the handler
  needs (`holds(OrderExecuted, CertificateOrder.status == executed)` applied to `IssueCert`)
  stays true if the use case is later started another way.

`outbox(event::E)` marks an event published outside the model (an outbox, an external
subscriber). It silences only the warning for a raised event with no local `transitions` or
`triggers` consumer; an unraised event is still unraised.

#### A use case that also starts on its own

`triggers` has no opt-out: a use case a tracked event triggers runs only after that event,
whoever performs it. When it also starts without the event, model where: a second event,
raised by the use case that marks that point, with a second `triggers`. In
[insurance-claim-ops](../samples/insurance-claim-ops/README.md), `AssessDamage` follows either
settlement decision and also a survey report:

```rdra
raises(ReportSurvey, EvSurveyReported)
triggers(EvSurveyReported, AssessDamage)
```

Dropping the use-case edge (keeping `triggers(Ev, Buc)`) frees the use case, a model change a
reviewer sees.

#### Counting raises: `.counted`

<!-- derived-from ./design/counted-events.md#semantics -->

```rdra
raises(ReceivePayment, EvPaymentReceived)
triggers(EvPaymentReceived, PostToLedger).counted(2)
```

`triggers(Ev, UC).counted(k)` makes the edge count: each raise on a row adds one pending run,
each run of `UC` takes one, and `UC` runs only while one is pending. `k`, 1 to 16, is how many
may be pending per row. Use it for "once per event": every payment posted once, no
notification lost.

- A raise never waits. A raise that finds `k` pending is the violation
  `EventOverflow_<E>_<Ev>_<UC>` (`<E>` the entity it is raised on), reported with a trace and
  tried by `verify --prove`. If `k` is the real capacity, it is a defect; if not, raise `k`.
  After an overflow the row keeps no count, and `UC` runs there as under the latch.
- Counting only removes runs, so it cannot make a passing `forbidden` fail. It exposes a run the
  latch allowed and the model never earned, through `leads_to`, `can_reach`, a deadlock, or a
  state with no witness: with `PostToLedger` fair, `leads_to(Payment.status == received,
  Payment.status == posted).fair(PostToLedger)` says every raise is handled.
- Each counted edge has its own count; handlers without `.counted` keep the latch. Each costs
  one value of `k + 2` per row in every cone that holds the raiser or the handler, and such a
  cone holds both. `codegen` prints no function for a use case that raises or handles one.

| Diagnostic | When |
|---|---|
| `UncountedTrigger` | The edge has no count to keep: the event is free, a timer or free event also starts the use case, the raisers share no entity, the use case raises the event itself, or a raiser binds the event's entity twice. The message names which. |
| `CountedTriggerInvalid` | `.counted` on a timer, BUC, or external system, a value outside 1 to 16, or a second `.counted` on one line. |

---

## System boundary

APIs are the operation boundary between a use case and the data it changes. Systems group
APIs, DTOs give them payloads, and permissions and media say who may reach them from where.

### External systems

```rdra
extsystem LabSystem "Laboratory System"
raises(LabSystem, EvLabResultReceived)               // inbound
triggers(EvLabResultReceived, ReceiveLabResult)
triggers(EvAppointmentScheduled, MessagingGateway)   // outbound: delivered to it
```

An external system talks to use cases only through events; there is no actor link. An event it
raises is a free trigger: the use case runs whenever its states and guards allow, as a timer's
does ([Triggers](./state-derivation.md#triggers)). An event delivered to it counts as consumed.
`lint` `rdra-extsystem-no-buc` names one no event in a BUC reaches.

### Publishing to other models

A model publishes what another model may reference; everything else stays internal.

```rdra
system AuthSystem "Authentication System"
api SessionApi "Session API"
contains(AuthSystem, SessionApi)
invokes(Login, SessionApi)
creates(SessionApi, Session)                  // Session is internal
transitions(Revoke, Session.status, active -> revoked)

// A projection of Session that auth writes and never reads.
entity SessionView "Session (published)" {
  session_id: Int @pk
  status: Enum(active, ended)
}
updates(SessionApi, SessionView)
sets(Login, SessionView.status == active)
updates(Revoke, SessionView)
sets(Revoke, SessionView.status == ended)

exposes(AuthSystem, Login)                    // callable on behalf of an actor
exposes(AuthSystem, SessionView).upsert()
```

- `exposes(System, UseCase)` publishes a use case. `.caller(actor)` (default) means another
  model calls it for a human actor, `.caller(system)` only a service identity, `.caller(any)`
  both. Under `system` or `any`, the use case is not reported for having no actor
  (`UseCasePermissionNoActor`, `ApiPermissionNoActor`).
- `exposes(System, Event)` publishes an event.
- `exposes(System, Entity).append()` or `.upsert()` publishes an entity: a projection the owner
  writes and never reads. `.append()` allows `creates` and `sets`; `.upsert()` also `updates`;
  `deletes` and `transitions` never.
- A published row is written in the transaction of the internal change it reports: a direct
  write joins the use case's one internal transaction whether or not a key links them (no
  `SeparateTxInferred`), so `Revoke` above is one action. A write through an API stays in that
  API's transaction, so route the internal change and the published write through one API.
- The owner's lifecycle (`states`, `check`, `export --kind tla`) skips published entities.
- `shares(System, Entity)` lets other models name an internal entity, at the cost of a
  `SharedInternalEntity` warning on every `check`; a second `shares` adds nothing.
- `list --kind surface` prints the published surface ([Surface text](./cli-reference.md#surface-text)).

| Diagnostic | When |
|---|---|
| `PublishedEntityMutated` | An `append` entity gets `updates`, or any published entity gets `deletes` or `transitions`. |
| `PublishedEntityReadByOwner` | The owner reads a published entity: `reads`, `shows`, `maps_field`, a `holds` body, `combines`, or a rule. |
| `PublishedEntityKeysInternal` | A `relate` joins a published entity and an internal one. |
| `PublishedNotInSourceTx` | A use case writes a published entity with no internal change, or with several internal transactions; or through an API that makes no internal change. |
| `InternalEntityExposed` | `exposes(System, Entity)` without `.append()` or `.upsert()`. |
| `PublishedEntityNeverWritten` (warning) | No use case writes the published entity. |
| `DuplicateExposure` | A second `exposes` of one element; the first stays. |
| `SharedEntityAlsoExposed` | `shares` on a published entity. |
| `ExposesChainInvalid` | A chain method on the wrong kind (`.append()` on a use case, `.caller` on an entity). |
| `ExposesCallerInvalid` | `.caller(...)` other than `actor`, `system`, `any`. |

### `api` and the API layer

```rdra
api OrderApi "Order API"
invokes(PlaceOrder, OrderApi)
creates(OrderApi, Order)
creates(OrderApi, OrderLine)
updates(PlaceOrder, Cart)          // a direct write is allowed too
```

**Actors.** A use case's actors are its direct `performs(Actor, UseCase)` actors and the actors
of every BUC that contains it. Each must hold the use case's and its APIs' permissions. So
`performs(Actor, Buc)` means the actor may run every use case of the BUC; an actor who runs only
some performs those use cases, or their steps (`performs(Actor, Step)` carries no permission).
`list --kind buc-roles` and the context diagram count all three forms. When a permission is
missing only because of `performs(Actor, Buc)`, the `ActorPermissionMissing` hint says so. A
use case with no actor needs one for its permissions (`UseCasePermissionNoActor`,
`ApiPermissionNoActor`), unless a timer or event starts it
([`triggers`](#triggers-and-event-driven-buc-chaining)) or another system calls it
(`exposes(..).caller(system)`).

**Transactions.**

- An invoked API's writes are one atomic group. Direct writes group by key connectivity over
  the whole `relate` graph (two written siblings of an unwritten parent group together); a
  write with no key link runs alone (`SeparateTxInferred` warning).
- An API invoked by several use cases is the same boundary in each. Split read-only and write
  APIs when they have different consistency contracts.
- `states` and `verify` run the groups (`UsecaseTx::groups()`) as steps, in order: the APIs in
  `invokes` order, then the key groups, then the isolated writes. One run is its steps in order
  on the rows the earlier steps bound, and other use cases may run between steps
  ([Runs of a split use case](./state-derivation.md#runs)). The first step carries the use
  case's `applies_to` guards; a guard applied to an API is on that API's step. An entity a use
  case only `sets` or transitions counts as updated.

**System boundaries.**

- `contains(System, Api)` places an API in a system. A system's entities are those its APIs
  operate plus those it `owns`.
- `owns` keeps an entity in the boundary before every operation exists.
- A `relate` between entities of different systems warns unless a use case declares
  `coordinates(UseCase, A, B)`, which must invoke an API in each system that operates its
  entity.

| Diagnostic (warning) | When |
|---|---|
| `ApiNeverInvoked` | No use case invokes the API. |
| `ApiInvokedButNoEntity` | An invoked API operates no entity. |
| `ApiInMultipleSystems` | An API is in several systems. |
| `EntityInMultipleSystems` | APIs of several systems operate the entity. |
| `EntityOwnedByMultipleSystems` | Several `owns` of one entity. |
| `OwnedEntityWithoutApiOperation` | A system owns an entity no API of it operates yet. |
| `ApiOperatesEntityOutsideOwner` | An API operates an entity another system owns. |
| `CrossSystemEntityRelation` | A `relate` crosses system entity sets. |
| `CoordinationNotCrossSystem` | `coordinates` names a pair that does not cross two systems. |
| `CoordinationMissingApi` | The coordinating use case invokes no API on one side. |

### API contracts

<!-- derived-from #relationship-predicates -->

```rdra
api CreateOrder "Create order"
  method POST
  path "/orders"
  idempotency "idempotent"
  mode sync
  auth bearer

dto CreateOrderRequest "Create order request" {
  customer_id: Int
  note: String @null
}
request(CreateOrder, CreateOrderRequest)
response(CreateOrder, OrderResponse)
error_response(CreateOrder, ErrorResponse)
```

| Clause | Meaning |
|---|---|
| `method` | HTTP method (`GET`, `POST`, `PUT`, `DELETE`). |
| `path` | HTTP path, a string. |
| `idempotency` | `idempotent`, `non_idempotent`, or any token. |
| `mode` | `sync`, `async`, or any token. |
| `auth` / `auth_scheme` | `bearer`, `oauth2`, `api_key`, or any token. |

A DTO body has the column form of an entity body; `@null` makes a field optional. An API with
only one of `method` and `path` is `lint` `api-contract-incomplete`, with neither
`api-contract-missing`. [`export`](./cli-reference.md#export) says how `openapi`, `asyncapi`, and
`json-schema` project these; event protocols, servers, and payload schemas are not modeled.

#### Mapping DTO fields to columns

```rdra
maps_field(OrderResponse.order_id, Order.id)
maps_field(OrderResponse.customer_id, Order.customer_id)
```

A DTO field maps to one column, which may be a generated key column. A second `maps_field` for
it is `DuplicateFieldMapping`; a field or column that does not exist is `UnknownColumn`. Types
are not compared. `export --kind json-schema` marks the field `x-rdra-ish-maps-to: "Order.id"`.
A read by an API with a [query shape](#query-shapes) selects the columns its `response` fields
map, in field order, each under its field name (`SELECT "id" AS "order_id", "customer_id" FROM
"Order" ...`); a read by a use case, or one whose response maps no column, selects every column.

### Access constraints

```rdra
permission ScheduleWrite "Schedule Write"
medium StaffTerminal "Staff Terminal"
has_permission(Staff, ScheduleWrite)
requires_permission(BookAppointment, ScheduleWrite)
requires_medium(BookAppointment, StaffTerminal)
requires_permission(BookingApi, ScheduleWrite)
```

- `check` compares `requires_permission` with `has_permission` on every actor that can perform
  the use case, and an API's per invoking use case, so a shared API must be authorized on every
  path (`ActorPermissionMissing`; `--strict` makes these errors).
- A screen inherits the constraints of each use case that displays it and of each API that use
  case invokes; `csv --kind screen-constraints` lists those paths.
- `csv` / `list --kind actor-permission-audit` shows `missing` (required, not assigned) and
  `excess` (assigned, required by no performer path).

---

## Data layer

`entity` is the logical data model: its body declares columns, and `relate` declares
relationships and generates keys. Inline `Enum`, `Bool`, and `@null` columns are the state axes
the lifecycle reasons about. A column whose value an actor chooses is typed by a
[`variation`](#variations); it is an axis only when a `sets`, a rule, a condition body, an
`after` assertion, or a `property` names it.

### Entity bodies

```rdra
entity Order "Order"
  table "orders"
  rows 5000 {
  id: Int @pk
  total: Money
  delivered_at: DateTime @null
  status: Enum(pending, paid, shipped, delivered, cancelled) @default(pending)
}
```

- Each column is `name: Type [annotations...]`. A DTO body has the same form, its fields
  payload fields.
- `table "name"` names the table in `export --kind sql` and `dbml`, with its keys and indexes.
  Every other output and every rule name keeps the entity id.
- `rows <n>` estimates the table size: `lint` `query-index-missing` skips an entity under
  10,000 rows, and a `.has` / `.none` over 1,000,000 or more wants an index of its own (see
  [Query shapes](#query-shapes)). Bare (`rows 5000`) or quoted, where `_`, `,`, and an exponent
  are allowed (`rows "1_000_000"`, `rows "5e7"`). A value that is not a whole number is
  `InvalidRowEstimate`.

#### Column types

| Type | Meaning |
|---|---|
| `Int` | Integer. |
| `String`, `String(n)` | Text, at most `n` characters. |
| `Money`, `Money(p, s)` | Amount, `p` digits, `s` after the point (`Money(12)` is `Money(12, 0)`). |
| `Decimal`, `Decimal(p, s)` | Decimal number. |
| `DateTime`, `DateTime(tz)`, `DateTime(local)` | Date and time, with or without the zone. |
| `Date` | Date. |
| `Bool` | Boolean; a state axis. |
| `Enum(a, b, c)` | Inline value set; a state axis, and a state model when `transitions` drive it. A value may carry a label: `Enum(pending "受付前", paid "支払済")`, which diagrams and the `states` table show. `lint` `enum-attribute` warns on one nothing moves. |
| `<VariationId>` | A [variation](#variations): one of its values. An attribute, never a state axis or lifecycle. An id that names no variation is `UnknownColumnType`. |

Type arguments are for the schema: `export --kind sql`, `dbml`, and `json-schema` print them.
`check`, `states`, and `verify` ignore them: an `Int` or `Decimal` in a rule ranges over
`0..--int-range-max`. Arguments on another type, a length or precision that is not positive, a
scale above the precision, or a `DateTime` argument other than `tz` / `local` is
`InvalidTypeArgs`.

#### Column annotations

| Annotation | Meaning |
|---|---|
| `@pk`, `@pk(a, b)` | The primary key, or a composite one over the named columns. A `relate` generates one key column per key column. |
| `@unique`, `@unique(a, b)` | A unique constraint, single or composite. The lifecycle checks it as `Unique_<Entity>_<columns>` when it holds every column's value; a key column such as `room_id` reads as its key's owner. A set with a column the lifecycle holds no value of (a `String`), or on an entity with a `@soft_delete` column, is left to the database ([Unique column sets](./state-derivation.md#unique-column-sets)). Two rows whose column is null do not agree. |
| `@index`, `@index(a, b)` | An index, single or composite. |
| `@check(expr)` | A check over the entity's own columns (`and`, `or`, `not` over comparisons), typed like a `forbidden` comparison. A missing column is `UnknownColumn`; arithmetic is `ArithNotInCheck`. Checked as `Check_<Entity>_<column>_<n>` ([Checks](./state-derivation.md#checks)). |
| `@null` | Nullable: `null` is one of the column's values. A nullable `Int`, `String`, ... is an axis `{null, present}`; a nullable `Enum` or `Bool` holds `null` beside its values (see [Null](#null)). |
| `@default(v)` | The start value. On an `Enum`, one of its values (else `InvalidEnumVariant`); on a `Bool`, `true` or `false` (else `InvalidBoolValue`); on a variation column, one of its values (else `InvalidVariationValue`); `@default(null)` is none of these. An integer default on a numeric or date column a rule reads lies in `0..--int-range-max` (else `IntLiteralOutOfBounds_<Entity>_<value>`). A variation column starts at any value unless the creating use case sets it; its `@default` does not pick the start (info `DefaultOnVariationIgnored`, see [Actions](./state-derivation.md#actions)). |
| `@default(now())`, `@default(uuid())` | A database-computed default: the current date or time on a `Date` / `DateTime`, a random UUID on a `String`. The lifecycle reads such a `@null` column as present at creation. Another type is `InvalidColumnFunction`. |
| `@on_update(now())` | Each update writes the current date or time (`Date` / `DateTime`); the SQL export leaves it to a trigger or the application. |
| `@generated(identity \| uuid \| ulid)` | How a key is made: `identity` (an `Int` the database numbers), `uuid` (a `String` with a random UUID), `ulid` (a `String` the application fills). A key column copies its parent column's type. A wrong type or value is `InvalidColumnFunction`. |
| `@version` | An `Int` optimistic-lock version: each update checks it and adds 1. SQL starts it at `DEFAULT 1` unless it has a `@default`. Another type is `InvalidColumnFunction`; a second `@version`, or one on a `@derived` column, is an error. |
| `@label("...")` | The column's display label. |
| `@soft_delete` | The soft-delete marker (`deleted_at`, `is_deleted`). |
| `@history` | Part of history or versioning (`valid_from`, `version`). |
| `@tenant` | The tenant discriminator. A key unique within a tenant is a composite set with it: `code: String @unique(tenant_id, code)`. |
| `@end(v, ...)` | States of an `Enum` or variation column a row may stay at for good: no `Deadlock` there, even when an arm leaves it, and the state diagram draws an end state. An undeclared value is `UnknownEndValue`; `@end` on a column with no values is `EndOnNonEnumColumn`. `lint` `end-redundant` warns on a value no arm leaves ([Deadlocks](./state-derivation.md#deadlocks)). |
| `@derived(expr)` | Derived from the entity's own columns: a condition for a `Bool` (`@derived(qty >= 3)`), or one `+`, `-`, or `*` over a column or integer for an `Int`, `Money`, or `Decimal` (`@derived(total - discount)`). A non-numeric operand is `ArithTypeMismatch`; an expression that cannot give the column's type is `DerivedTypeMismatch`. It takes no `@check`, `@default`, `@pk`, or `@unique` (`DerivedColumnAnnotation`); a `sets` on it is `DerivedColumnWritten`. The lifecycle cannot evaluate an arithmetic one (`DerivedColumnRead`). |

- A composite `@pk(...)`, `@unique(...)`, or `@index(...)` is written on one column line but
  belongs to the entity.
- `@check(...)` and `@derived(...)` take an unquoted expression. The quoted form
  (`@check("qty >= 1")`) is a parse error whose message gives the rewrite. A comparison there
  reads its right-hand side as every comparison does ([Bare identifiers](#bare-identifiers)):
  `@check(status == open or closed_at == present)`.
- `fmt` keeps a written `Order.qty` qualifier; the exports print bare column names.
  [`export`](./cli-reference.md#export) says how DBML, SQL, and JSON Schema print each
  annotation and key.

### Variations

```rdra
variation PaymentMethod "Payment Method" {
  cash "Cash"
  card "Card"
  adjustment            // the label is optional
}

entity PaymentTransaction "Payment Transaction" {
  id: Int @pk
  method: PaymentMethod @default(card)
}
```

A variation lists the values an actor chooses from: at least one, each an identifier with an
optional label, unique within the variation. RDRA keeps them apart from the state model: a
payment method is picked, an order status is driven. A value, an `Enum` variant, or a column
may be named with a kind keyword (`field`, `screen`, `goal`, ...). Any entity or DTO column may
take the variation as its type; `@default` and `sets` values are checked against its values
(`InvalidVariationValue`). The type name resolves through the file's imports, like a reference.

| Output | Inline `Enum` column | Variation-typed column |
|---|---|---|
| `states` | One axis; unreached variants reported | No axis, no reachability check |
| `transitions` | Allowed | `TransitionsColumnNotEnum`: an attribute has no lifecycle |
| TLA+ export | A lifecycle column per column `transitions` names | An axis only when a `sets`, a rule, an `after` assertion, or a `property` names it; `Init` chooses freely among every value |
| an update | A `transitions` arm or `sets` moves it | Keeps its value unless the use case `chooses` it |
| `export --kind dbml` | One `Enum <Entity>_<column>` per column | One `Enum <VariationId>` shared |
| `export --kind json-schema`, `openapi` | `enum` of the variants | `enum` of the values |
| ER diagram, `csv` / `list --kind entity` | Type `Enum` | Type is the variation id |

#### Choosing a value again: `chooses`

A created row takes any value of a variation column, unless the creating use case `sets` it.
An update keeps the value. `chooses(UseCase, Entity.column)` lets the actor pick again:

```rdra
transitions(DraftResponse, OfficeAction.status, open -> drafted)
chooses(DraftResponse, OfficeAction.amendment)
```

- After the use case the column holds any value, one run per value, in `states`, `check`, and
  TLA+ (`\E v \in {"limited", "free"} : ...`). Two `chooses` in one use case are independent.
  Each run carries the use case's other effects.
- A `chooses` counts as a write of the entity, with no CRUD edge needed. On the creating use
  case it changes nothing.
- It is not `sets(.., E.col == any)`: a variation may have a value `any`, and `sets` fixes one
  value.
- To offer some values, list them, one `chooses(UC, E.col, value)` each; to offer one only in
  some states, guard it with `.when` (below).

| Diagnostic | When |
|---|---|
| `ChoosesNotVariation` | The column is an inline `Enum` (pick the next state with several `transitions` arms) or a `Bool` (write `sets`). The hint names the form. |
| `InvalidChoosesColumnRef` | The second argument is not `Entity.column`. |
| `UndefinedSymbol`, `UnknownColumn` | An unknown entity or column. |
| `ChoosesAndSets` | The use case both `chooses` and `sets` the column. |
| `DerivedColumnWritten` | The column is `@derived`. |

#### Guard one value: `.when`

<!-- constrained-by #guard-one-arm-when -->

`chooses(UseCase, Entity.column, value)` lines of one use case and column list the values the
actor may pick; `.when(Condition)` offers a value only while the condition holds. A final
office action admits only a limited amendment (特許法17条の2):

```rdra
condition FirstNoticeOnly "最初の拒絶理由通知である"
holds(FirstNoticeOnly, OfficeAction.kind == first)

chooses(DraftResponse, OfficeAction.amendment, free).when(FirstNoticeOnly)
chooses(DraftResponse, OfficeAction.amendment, limited)
```

- Only the listed values are choices. A value-less line beside value lines is
  `ChoosesMixedForms`.
- A guarded value is a choice only while its guard holds, read as an arm's guard is
  ([Guard one arm](#guard-one-arm-when)): on the state the use case starts from (a guard on the
  chosen column sees the old value), and on a created row after the step. Among enabled values
  the choice is free.
- No enabled value, no run. A value listed twice is enabled by either line (an unguarded line
  lifts the guard).
- On the creating use case the listed values restrict the creation's choice.

| Diagnostic | When |
|---|---|
| `InvalidVariationValue` | Not a value of the variation; the hint lists them. |
| `QuotedValue` | A quoted value (`chooses(UC, E.col, "a")`); write it bare. |
| `ChoosesWhenWithoutValue` | `.when` on the value-less form; write the value, or `applies_to(Cond, UC)`. |
| `WhenRequiresCondition`, `WhenConditionWithoutBody` | As for an arm. |
| `ValueNeverChosen` (warning, `Choice_<UseCase>_<Entity>_<column>_<value>`) | A guarded value never chosen while its use case runs. |

The sequence diagram writes each guard in `[...]` after its value; `list --kind condition` adds
the use case to the condition's `applies_to`; the condition diagram shows the condition
combining the variation, `derived ... from when`. `sets` takes `.when` the same way
([A value that follows a condition](#a-value-that-follows-a-condition-when)).

### Conditions

<!-- derived-from #variations -->

A `condition` is an RDRA 条件: variations and a state model tied to the use cases they apply
to. It takes one of two forms:

```rdra
// Traceability only: the links are declared.
condition CopayCappedAtLimit "窓口負担は負担割合で計算し、限度額情報があれば限度額までとする"
combines(CopayCappedAtLimit, BurdenRatio)           // 条件 to バリエーション
combines(CopayCappedAtLimit, Appointment.status)    // 条件 to 状態モデル
applies_to(CopayCappedAtLimit, CalculateVisitBill)

// With a body: the links are derived from the columns it names, and it guards.
condition UnreferredFirstVisit "紹介状なしの初診で、除外に当たらない"
holds(UnreferredFirstVisit, Charge.kind == selectedtreatment, Appointment.referral == unreferred)
applies_to(UnreferredFirstVisit, ApplySelectedTreatmentFee)
```

**`combines`.** The second argument is a variation or `Entity.column`. A two-part name is tried
as a variation through the imports first (`shared.BurdenRatio`), then as a column; both matching
is `AmbiguousCombinesTarget` (`variation::Name` picks the variation). A column must be a
lifecycle column, moved by some `transitions` anywhere in the files; an attribute is
`ConditionCombinesAttribute`.

**`holds`.** A comma list of comparisons, AND-ed, each naming its entity; `check` validates the
entity, column, and value (`InvalidVariationValue`, `InvalidEnumVariant`). A choice is
`Entity.column in (a, b)` and its negation `not in` ([Set membership](#set-membership-in-and-not-in));
no other disjunction or negation exists. A second `holds` is `ConditionSecondBody`; `combines`
with `holds` is `ConditionCombinesWithBody`.

```rdra
holds(PriorityBasePending, PatentApplication.status in (filed, formality_order, examining))
```

**Links.** One function, `condition_edges`, gives a condition's links: with a body, an operand
on a variation-typed column links the variation, and one on a lifecycle column links the state
model (also inside `.has` / `.none`); without a body, the `combines`. `list --kind condition`,
the diagrams, and `lint` all read it. A body that names neither and guards no use case is `lint`
`condition-no-rdra-anchor`. A body that guards (by `applies_to` a use case or API, `.when` on an
arm or value, or a rule's `.when(Cond)`) is not flagged, even on `Bool` or nullable columns.
`applies_to(Condition, System)` adds no guard and exempts nothing. Do not compare the state the
use case moves from just to anchor it; the transition requires it already.

**Guards.** A body applied to a use case lets it run only where every comparison holds;
applied to an API, it guards each use case that invokes it. `states`, `check`, and the TLA+
export read the same guard. In a split use case ([Runs](./state-derivation.md#runs)), a body
applied to the use case is the precondition of the run, carried and read by the first step; a
body applied to an API is on that API's step. No later step re-reads it.

- A comparison on an entity the use case writes reads that row. On a created entity it tests
  the new row after the step, so it constrains the creation: `UnreferredFirstVisit` lets the use
  case create only a `selectedtreatment` charge.
- A comparison on any other entity needs some present row that meets it; comparisons on one
  such entity share a row. A direct `relate` links that row to the rows it relates to; with no
  `relate` it is independent.
- `.along(A, B, C)` names the path instead: the guard reads the row of each path entity the
  keys join, starting from the bound row. Only the hops link. A `.role(role)` after it picks a
  hop's key ([Which key a join follows](#which-key-a-join-follows)).
- A compared entity no key joins to a bound row is the warning `UnjoinedGuardRead`: any row
  anywhere satisfies it. The warning names the `.along` of the shortest key path, or writing the
  entity in the use case.
- A body on a use case that writes nothing is `UnbindableCondition`. A column the body reads
  that no use case writes is the warning `UnwrittenConstrainedVar`. A condition with no body is
  a trace link and guards nothing.

```rdra
// The claim is completed only when the settlement of its own case is agreed.
holds(SettlementSigned, Settlement.status == agreed).along(Claim, ClaimCase, Settlement)
applies_to(SettlementSigned, CompleteClaim)
```

**Quantified bodies.** A body may end in `.has(Related.col == v)` or `.none(Related.col == v)`:
some, or no, related row matches. A bare entity, `.has(Related)` / `.none(Related)`, reads only
the link. The rows are the present rows of the related entity the relate links to the bound
row (every row when no relate joins them), read before the use case runs: a row it creates is
not among them, and a row it updates is read as it was. The counted row is its own instance,
even when the body also reads its entity. `.none(Related.status in (<every value>))` is
`.none(Related)`; write the latter. When two keys join, `.role(role)` names one.

```rdra
condition NoActiveAssignment "有効な割当がない証明書だけ失効できる"
holds(NoActiveAssignment, ClientCertificate.status == active).none(TerminalCertAssignment.status == active)
applies_to(NoActiveAssignment, Revoke)
invariant(ClientCertificate).when(status == revoked).none(TerminalCertAssignment.status == active)
```

Without the guard on `Revoke`, the invariant fails
(`samples/formal-verification/quantifier_guard.rdra`).

- `.at_most(n, Related...)`, `.at_least(n, Related...)`, `.exactly(n, Related...)` count the
  same rows and compare with `n`, 1 to 255; each row counts once
  ([Counted quantifiers](#counted-quantifiers)):
  `holds(ShelfUnderLimit, Shelf.open == true).at_most(2, Copy.status == open)`.
- A `.has` / `.none` whose entity a `.along` path names, and the body does not compare, counts
  along the path: the rows of that entity, and of every path entity back to the nearest bound or
  compared one, that the hops' keys chain to. `.none` holds when no chain reaches a matching row
  (no chain at all passes); `.has` when some chain does. A `.role` after the quantifier is not
  read on a counted entity. An entity the body compares stays a witness even if a quantifier
  names it. A path that names the counted entity before any bound or compared entity, goes past
  it to an entity nothing counts, or joins a compared entity through it is
  `UnbindableCondition`, and the error says how to write it.

```rdra
// A member withdraws while no loan on any of their cards is open.
holds(NoOpenLoan, Member.status == active).none(Loan.status == open).along(Member, Card, Loan)
applies_to(NoOpenLoan, Withdraw)
```

**Writes.** A use case that writes two entities related only through one it does not write
(`Account` and `Store` through `Device`) is the warning `UnjoinedWrite`: one run may write rows
of different `Device` rows. Write the entity between (`updates(Register, Device)`), or mark a
write meant as a row of its own with `.apart()`: `updates(Register, Store).apart()`.

### RDRA lint rules

<!-- derived-from #conditions -->
<!-- constrained-by ./rdra-ish-interpretation.md#the-rdra-sheets-不整合-columns -->

The RDRA definition sheet has 15 不整合 columns, each a check that something is referenced but
undefined or defined but connected to no BUC. `crates/rdra-ish-core/src/rdra_rules.rs` holds
them as one table, `RDRA_RULES`: the sheet column, the mechanism, a hint, and a witness model a
unit test loads to prove the row fires.

| Mechanism | What it checks | Where it shows |
|---|---|---|
| `Resolver` | The name is referenced but undefined; the row names the diagnostic that covers it. | `check` and `lint` error |
| `BucReach` | Every element of the kind is in the forward closure of some BUC (`reachable_from_buc`), or is the `from` of a relation into it (an actor that performs a use case, a condition that applies to one, a timer that triggers one). A variation also counts when it types a column of such an entity. Replaces `orphan-node` for its kinds. | `lint` warning |
| `TransitionUseCaseInBuc` | Every use case a `transitions` names is in a BUC's closure. | `lint` warning |

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
| `rdra-actor-no-buc` | BUC | アクター 未接続 | `BucReach` actor | `performs(Actor, UseCase \| Buc)`, or delete the actor |
| `rdra-extsystem-no-buc` | BUC | 外部システム 未接続 | `BucReach` extsystem | `raises` or `triggers` through an event a BUC uses, or delete it |
| `rdra-entity-no-buc` | BUC | 情報 未接続 | `BucReach` entity | reach it from a use case through an API, or delete it |
| `rdra-condition-no-buc` | BUC | 条件 未接続 | `BucReach` condition | `applies_to(Condition, UseCase)`, or delete it |
| `rdra-variation-no-buc` | rdra-ish | (none) | `BucReach` variation | type a column of a reached entity with it, or delete it |
| `rdra-timer-no-buc` | rdra-ish | (none) | `BucReach` timer | `triggers(Timer, UseCase \| Buc)`, or delete it |
| `rdra-transition-uc-no-buc` | rdra-ish | 遷移UC は BUC シートで定義 | `TransitionUseCaseInBuc` | `contains(Buc, UseCase)` for the transitioning use case |

### `relate` and FK generation

```rdra
relate(OrderLine, Order, N:1)                     // OrderLine.order_id
relate(Shipment, Order, 1:1)                      // Shipment.order_id, unique
relate(Order, Product, N:M).via(OrderProduct)     // OrderProduct.order_id, OrderProduct.product_id
relate(Stock, Bin, N:1)                           // Bin @pk(site, shelf): Stock.bin_site, Stock.bin_shelf
relate(Order, Address, N:1).role(ship_to)         // Order.ship_to_id
```

`relate(Child, Parent, Card)` declares an ER relationship and generates the child's key
columns, `{role}_{parent key column}` per parent key column. The role defaults to the parent id
in lowercase (`customeraccount_id`). `Card` is `1:1`, `1:N`, `N:1`, or `N:M`, unquoted. In `N:1`
the left is the child; in `1:N` the right; in `1:1` the left, its key marked unique. The
cardinality drives the ER notation and the key graph that transaction inference uses.

- **Options.** `.optional()` makes the key nullable (`fk_optional`): the child may exist
  unlinked in the lifecycle and TLA+; a mandatory key may not. `.on_delete(action)` and
  `.on_update(action)` take `cascade`, `restrict`, `set_null`, `set_default`, or `no_action`
  (else `UnknownRefAction`); see [Deleting a parent](#deleting-a-parent).
- **`N:M`** needs `.via(Junction)`, a third entity. The model keeps the logical edge and
  synthesizes `relate(Junction, A, N:1)` and `relate(Junction, B, N:1)` (keys, a composite
  unique on the pair when missing, owner links, ER crowfoot, `.along` paths); options apply to
  both keys. Writing the two `N:1` relates yourself is the same, and synthesis skips edges that
  exist. The pair is checked as `Unique_OrderProduct_order_id_product_id`; a use case that
  creates a junction row needs a guard that keeps the pair free
  ([Unique column sets](./state-derivation.md#unique-column-sets)).
- **`.role(name)`** names the key: columns `ship_to_id`, owner `Order._owner_ship_to`. Two keys
  from one child to one parent need distinct roles.
- **`.key(col, ...)`** adopts columns the child declares instead of generating them, one per
  parent key column in order, of the same type. They keep their names, take the key's
  nullability and (single-column `1:1`) uniqueness, and are the key in every output; in the
  lifecycle they are the owner, not a state column.
- **`.references(col, ...)`** makes the key refer to parent columns other than the primary key:
  `.references(sku)` gives `product_sku`, typed as `sku`. They must be a unique set of the
  parent (its `@pk`, a `@unique(...)`, or one `@unique` column). `.key(...)` then adopts one
  column per referenced column.

| Diagnostic | When |
|---|---|
| `NMRelation`, `SelfNMRelation`, `ViaMustBeDistinct` | `N:M` without `.via`, `relate(A, A, N:M)`, or `.via` equal to an end. |
| `DuplicateRelateRole` | Two keys of one child have the same role. |
| `FkConflict` | The child already declares a column under a generated name; name the key with `.role` or adopt the column with `.key`. |
| `KeyArity` | `.key(...)` names another number of columns than the parent key has. |
| `KeyColumnUndefined` | `.key(...)` names a column the child does not declare. |
| `KeyTypeMismatch` | An adopted column's type differs from the parent key column's. |
| `KeyColumnShared` | An adopted column already holds another relate's key. |
| `KeyOptionOnManyToMany` | `.role` or `.key` on `N:M`; write the junction's keys as two `N:1` relates. |
| `UnknownRefAction` | An unknown `.on_delete` / `.on_update` action. |
| `ReferencesNotUnique` | `.references(...)` names columns that are not the primary key or a unique set. |

#### Which key a join follows

<!-- constrained-by #relate-and-fk-generation -->

A statement that reads two related entities joins them through a key: an `.along(A, B)` hop (on
a `forbidden`, `invariant`, `property`, or `holds`), a `.has` / `.none`, the link between an
entity a `holds` body compares and an entity the guarded use case writes, a copy
(`sets(UC, E.col == F.col)`), and `links` / `relinks` / `unlinks`. One rule
(`SemanticModel::join_key`) resolves each, so `check`, `states`, the TLA+ export, and `verify`
read the same rows:

- One key between the two entities, either direction, is the join; no selector is needed.
- Several keys need `.role(role)`, which names a key by its role, or by its parent id when it
  has no role (`.role(Order)` for `relate(Line, Order, N:1)`; case-insensitive).
- Several keys and no selector naming exactly one is `AmbiguousOwnerLink`: the error names the
  candidate roles and the statement with `.role(<first role>)` added, and the join stays
  unlinked.

`.role` names the same row whether or not the use case binds the entity. A use case that creates
a child under two keys to one parent it does not write (`ship_to`, `bill_to`) binds one parent
row per key, possibly different; a comparison or copy source on that parent reads the row of the
key its `.role` names.

A parent the use case writes joins a child it writes through one key the same way. With one key,
the written row is the child's parent on it. With several, it is the parent only on the keys a
`.role` of the use case names (of a `holds` guard applied to it, or of a copy in it); on the
other keys a created child takes a parent row of its own and an updated child keeps its row. So
`holds(BillsEast, Address.region == east).role(bill_to)` on a use case that updates the billing
address and ships the order reads the `bill_to` address, and the order may ship to another.
With several keys and no `.role` naming any, it is `AmbiguousOwnerLink`; the error names the
`holds` body or copy to fix, or says a guard or copy must name the key.

| `.role(role)` written after | Picks the key of |
|---|---|
| `.along(A, B, ...)` | each hop several keys join |
| `.has(...)` or `.none(...)` | the link from the counted row to the bound rows |
| `holds(Cond, ...)` itself, before any `.has` / `.none` | the link between a compared entity and the guard's other entities |
| `sets(UC, E.col == F.col)` | the key from the target row to the source row |
| `links`, `relinks`, `unlinks(UC, Child, Parent)` | the key from the child to the parent |

```rdra
relate(BusinessPermit, PermitApplication, 1:1)                       // registered under
relate(PermitApplication, BusinessPermit, N:1).optional().role(renews)

holds(NoPermitYet, PermitApplication.kind == new).none(BusinessPermit).role(PermitApplication)
holds(RenewsValidPermit, BusinessPermit.status == valid).role(renews)
forbidden(BusinessPermit, PermitApplication, BusinessPermit.business_type != PermitApplication.business_type).along(BusinessPermit, PermitApplication).role(PermitApplication)
```

| Diagnostic | When |
|---|---|
| `AmbiguousOwnerLink` | Several keys join two entities and no `.role` names one. The lifecycle reports it for a guard, quantifier, copy, or written parent and child (per guarded use case); the analysis for `.along`. |
| `UnknownOwnerRole` | `.role(role)` names no key on the path, of the counted entity, of a compared entity, between a copy's target and source, or between a link's child and parent. |
| `UnjoinedGuardRead` (warning) | See [Conditions](#conditions). |
| `UnjoinedWrite` (warning) | A use case writes two entities related only through entities it does not write or bind. `check` at one instance cannot show it; `verify` at two can. |
| `RoleWithoutJoin` | `.role` on a `forbidden` or `invariant` with no `.along`, `.has`, or `.none`, or on a `sets` that copies nothing. |
| `QuantifierAlongShape` | An `invariant` quantifier's `.along` does not run from the anchor to the counted entity, each once. The hint writes the shortest key path. |

In the LSP a role is a symbol: go to definition on `.role(renews)` opens the `relate` that
declares it, and find references and rename cover every `.role(...)` that names it. A
`.role(Order)` that names a key by its parent id references the entity `Order`.

### Cardinality ranges

```rdra
relate(OrderLine, Order, 1..*:1)    // every Order has an OrderLine
relate(Shipment, Order, 1..1:1)     // every Order has exactly one Shipment
relate(Loan, Member, 0..3:1)        // every Member has at most 3 Loans
```

The child side of a card (the `N` of `N:1` and `1:N`, the left of `1:1`) takes `lo..hi`: `lo`
is 0 or 1, `hi` is 1, 2 to 255, or `*`. `hi` of 1 stands for `1`; above 1 or `*`, for `N`. A
minimum of 1 is checked as the invariant `MinCard_<Child>_<role>`
([Checks](./state-derivation.md#checks)), a maximum above 1 as `MaxCard_<Child>_<role>`, a count
of present children as `.at_most(hi, Child)` counts ([Counted quantifiers](#counted-quantifiers)).
`0` and `*` add no check. The ER, DBML, SQL, and Mermaid outputs draw a side with a maximum as
many; no DDL states it. `fmt` prints the card as written. A parent that may have no child needs
no range; a child that may have no parent is `.optional()`.

| Diagnostic | When |
|---|---|
| `MinCardOutOfRange` | `lo` is not 0 or 1, or `hi` is not 1, 2 to 255, or `*` (`2..*:1`, `0..0:1`). |
| `MinCardOnParentSide` | A range on the parent side (`N:0..1`); write `1` and `.optional()`. |
| `MinCardOnManyToMany` | A range that makes the card `N:M` (`N:1..*`, `1..*:M`); give the junction's keys the range. |

### Query shapes

```rdra
reads(ListOrders, Order).by(customer_id, status).order(created_at.desc).page(cursor)
reads(OrderTotals, Order).by(customer_id).sum(total)
```

A `reads` can say how it finds its rows. `lint` names the index each read wants, and
`export --kind sql` writes the index and the `SELECT` ([SQL export](./cli-reference.md#sql-export)).

| Method | Meaning |
|---|---|
| `.by(a, b)` | Compares each column to a value: a declared column or a generated key column. |
| `.order(a, b.desc)` | The sort, first key first; `.desc` descending, bare or `.asc` ascending. |
| `.page(cursor)` | Keyset paging after the last row's sort key; needs `.order`. |
| `.page(offset)` | `LIMIT` and `OFFSET`. |
| `.count()`, `.sum(column)` | One row: the count, or the sum of an `Int`, `Money`, or `Decimal` column. No `.order` or `.page`. |

**The index a read wants:** the entity's `@tenant` column, the `.by` columns, the sort, and for
`.page(cursor)` the primary key columns the sort does not name. An index serves it when its
leading columns are the equality columns in any order, then the sort in its direction or all
reversed. The primary key, unique sets, `@index`es, and foreign keys count; a read one of them
serves, or the index of a longer read serves, wants nothing. Three lookups want an index with no
`reads`:

- **A timer's sweep.** A use case a timer triggers finds every row at the states its arms move
  off: the `@tenant` and state columns (its arms on one column are one lookup). An actor's use
  case moves a row it holds: a key lookup.
- **A `.has` / `.none` over a large entity** (1,000,000 or more `rows`): the key and the columns
  it compares with `==`.
- **A guard over an unjoined entity.** A `holds` body without `.along` that compares an entity
  the use case neither writes nor joins by one key looks across the table: the `@tenant` and the
  `==` columns. When the guard means a related row, join it with `.along`.

`lint` `query-index-missing` warns at a lookup no index serves, unless the entity declares fewer
than 10,000 `rows`, and names the `@index(...)` to add. A sort of mixed directions needs an
index `@index` cannot declare; the hint points at the one `export --kind sql` writes.

| Diagnostic | When |
|---|---|
| `InvalidQueryShape` | Wrong arguments, a method twice, or a misfit: `.page(cursor)` without `.order`, `.count()` / `.sum` with `.order` or `.page`, `.sum` on a non-number. |
| `UnknownColumn` | `.by`, `.order`, or `.sum` names an unknown column. |

---

## Lifecycle

`transitions` declares state machine edges, `chooses` lets the actor pick a variation value,
`sets` and `drives` declare column effects, `links` moves keys, and the
[rules](#entity-state-constraints) say which combinations must never or must always hold.
`states` derives the reachable patterns, `check` checks the rules, and `verify` also checks the
temporal properties.

### `transitions` and the state machine

```rdra
transitions(CancelOrder, Order.status, pending -> cancelled)
transitions(ExpireOrder, Order.status, pending -> cancelled)
transitions(CancelOrder, Order.status, (pending, held, confirmed) -> cancelled)
```

`transitions(UseCase, Entity.column, from -> to)` is one edge of the column's state machine,
driven by the use case: the 遷移UC column of an RDRA state sheet. The column is an `Enum`; the
states are its values, matched case-insensitively. Each use case that drives an edge gets its
own line.

- `(a, b) -> z` is one arm per listed source, as if each were its own line, everywhere (`check`,
  `states`, the diagrams, `list --kind transition`, TLA+); a `.when` after it guards each. `(a)
  -> z` is `a -> z`. There is no `*`: write the sources out.
- **A choice.** Several arms of one use case from one state on one column: one run takes exactly
  one, and the actor decides. Every arm carries the rest of the use case (its `sets`,
  `creates`, `deletes`, and other transitions). Two choice columns choose independently (four
  runs for two pairs); to tie values together, split the use case. Guards applied to the use
  case guard every arm, and it runs only from a state an arm leaves. `after(UC).assert` must
  hold after every arm.
- A repeated arm is one arm. Arms from different states are one move per state.
- A `sets` is not a choice: two `sets` of one column with different values are
  `ConflictingEffects`.
- The first argument is a use case. An event (the earlier form) is `TypeMismatch`, whose hint
  names the use case to write and `scripts/migrate-phase3.py`, which rewrites a model to one line
  per raiser. A column that is not an `Enum` is `TransitionsColumnNotEnum`.

#### Several lifecycle columns on one entity

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

- Each `Enum` column a `transitions` names is its own state model, with its own witnesses,
  deadlocks, and state-diagram block. The columns move independently; `check` and TLA+ explore
  them together, so any guard or rule may read any of them.
- A state names its column when the entity has several: `Disposition_publication_published`
  (`state__Disposition_publication_published`). With one, `{Entity}_{value}`.
- A use case that moves two columns takes one arm of each in one step, and runs only where both
  have an arm. `Go` with `a -> b` on `status` and `y -> z` on `period` does not run at `(a, x)`.
  Add an arm per state it should move from (`x -> z`); a self-loop `x -> x` lets it run and
  leaves the period, which may then be a `Deadlock_E_period_y`.
- `codegen` tags each state type by `status`, else a `*_status` column, else the first lifecycle
  column by name; the others are fields.

#### Guard one arm: `.when`

```rdra
condition RightsAcquired "会社が特許を受ける権利を取得している"
holds(RightsAcquired, InventionDisclosure.right_acquired == true)

transitions(DecideFiling, InventionDisclosure.status, searched -> decided_file).when(RightsAcquired)
transitions(DecideFiling, InventionDisclosure.status, searched -> decided_secret)
transitions(DecideFiling, InventionDisclosure.status, searched -> decided_abandon)
```

- An arm with `.when(Condition)` is enabled only while the condition's body holds on the state
  the use case starts from. The body reads the arm's instance and other entities as an
  `applies_to` body does.
- The use case runs when some arm is enabled and its `applies_to` guards hold; among enabled
  arms the choice is free. Above, the committee can always keep the invention secret or drop
  it, and can file only once the right is acquired.
- `.when` takes one condition, once; put several comparisons in its body.
- Unlike `applies_to(Cond, UC)`, which guards every step from every state, `.when` guards one
  arm; the use case still takes its other arms while it fails.
- One arm declared twice with different guards is enabled when either holds.
- The state diagram writes the guard in `[...]` after the label; the sequence diagram heads the
  arm's `alt` branch with it; `list --kind transition` adds it to `guards`; the condition
  diagram shows the condition combining the state model, `derived ... from when`.

| Diagnostic | When |
|---|---|
| `WhenRequiresCondition` | `.when` with no argument, two, twice, or not a condition. |
| `UndefinedSymbol` | An undefined name. |
| `WhenConditionWithoutBody` | A condition with no `holds` body. |
| `ArmNeverTaken` (warning, `Arm_<UseCase>_<from>_<to>`) | A guarded arm never taken while its use case runs and some instance is at its `from`. |

### Linking, moving and unlinking an existing child: `links`, `relinks`, `unlinks`

```rdra
relate(Case, Handler, N:1).optional()
creates(OpenCase, Case)
transitions(Assign, Case.status, new -> assigned)
links(Assign, Case, Handler)
invariant(Case).when(status == assigned).has(Handler).label("An assigned case has a handler")

updates(Reassign, Handler)
sets(Reassign, Handler.busy == true)
relinks(Reassign, Case, Handler)
transitions(Release, Case.status, assigned -> new)
unlinks(Release, Case, Handler)
```

The three take `(UseCase, Child, Parent)`, follow the same key (the one from the child to the
parent, or the one `.role(role)` names; see [Which key a join follows](#which-key-a-join-follows)),
and bind the parent by the same rule. Each writes the child (no `updates` line needed) and reads
the parent.

| Form | Guard | Effect |
|---|---|---|
| `links` | The child is unlinked on the key. The key must be `.optional()`. | Links it to the parent the step binds. |
| `relinks` | The child is linked. A mandatory key is allowed (the guard is then empty). | Links it to the parent the step binds; the same parent is allowed, so the move can run at one instance. |
| `unlinks` | The child is linked, to the bound parent when the step writes one. The key must be `.optional()`. | The child is unlinked. |

- **The bound parent** is the row the use case writes (`updates(Reassign, Handler)`), the row an
  earlier step of its run wrote ([Runs](./state-derivation.md#runs)), or else any present
  parent.
- A use case that only links an assigned case never runs (`Runs_<UseCase>`). `check` names the
  blocking guard: "the child is not linked to a parent yet (links)", "the child is linked to a
  parent (relinks)", "the child is linked to a parent, the bound one when the step writes it
  (unlinks)".
- Rules read the link as any other: `.has` / `.none`, `.along`, and `holds` joins see the parent
  after the step. The form itself checks no cardinality: two children linked to a `1:1` parent
  break `Unique_<Child>_<role>`, and taking the only child of a `1..*` parent breaks
  `MinCard_<Child>_<role>`. A guard such as `holds(Free, Handler.busy == false).none(Case)` keeps
  the parent free.
- A child the same use case creates needs no `links`: a parent the use case writes, or a `holds`
  guard that compares one, links it.

| Diagnostic | When |
|---|---|
| `LinkWithoutKey` | No key makes `Parent` the parent of `Child`; the hint declares the relate or swaps the two. |
| `LinkOnMandatoryKey` | The key of a `links` or `unlinks` is not `.optional()`. |
| `AmbiguousOwnerLink` | Several keys join the two and no `.role` names one. |
| `UnknownOwnerRole` | `.role(role)` names no key between the two. |
| `ConflictingLinks` | A second `links`, `relinks`, or `unlinks` of one use case on one key. One key linked and another unlinked is fine. |

---

## Value vocabulary for `sets`

```rdra
sets(usecase::Capture, Payment.status == captured)        // Enum: a value
sets(usecase::Enable, Switch.enabled == true)              // Bool
sets(usecase::Restock, Stock.stock == 3)                   // Int, Money, Decimal: a number
sets(usecase::Issue, Coupon.expires_at == 3)               // Date, DateTime: a point on the clock
sets(usecase::Login, UserAccount.last_login_at == present) // @null: present
sets(usecase::Logout, Session.token == null)               // @null: null
sets(RegisterPermit, BusinessPermit.business_type == PermitApplication.business_type)  // a copy
```

`sets(UseCase, Entity.column == value)` says which value a use case leaves, not how it is
stored. The origin is a use case; an event is `TypeMismatch`, naming `scripts/migrate-phase3.py`.

| Value | Column | Meaning |
|---|---|---|
| A value name | `Enum`, variation | That value. |
| `true` / `false` | `Bool` | That value. |
| A number | `Int`, `Money`, `Decimal` | That value, within the Int range. |
| A number | `Date`, `DateTime` | That point on the clock `now` ticks through. |
| `present` / `null` | `@null` | Non-null, or null. |
| `Entity.column` | A column of the same type | A copy (below). |

- A storage type name (`timestamptz`) is no value: write `present` for a nullable column the
  use case fills (`PgTypeNameInSets`).
- A date column set to a number has a deadline: `sets(Issue, Coupon.expires_at == 3)` makes
  `expires_at < now` false until the clock passes 3, so `forbidden(Coupon, status == active,
  expires_at < now)` reads a column a use case writes, not one no use case writes
  (`UnwrittenConstrainedVar`). `now` is not a value; a column stamped with the current time is
  `@null` set to `present` ([Clock](./state-derivation.md#actions)).
- A comparison (`Stock.stock < selling`, `Order.status != paid`) or a bare column (`status ==
  paid`) sets nothing: `SetsNoEffect`, whose hint names `drives`. A set membership is
  `EffectMembership`: a use case leaves one value.
- A use case leaves one value per column: two values are `ConflictingEffects`. A `sets` of a
  lifecycle column by a use case that does not create the entity is `DoubleModeledEnum`; the
  creating use case's `sets` is the start state.

### Copying a value from another row

`sets(UC, E.col == F.src)` leaves in `E.col` the value `F.src` holds. The source is the row the
step already binds for `F`, read as a `holds` atom on `F` is ([Actions](./state-derivation.md#actions)):

- **A row the step writes** (same transaction group): an updated row before the change, a
  created row as the step leaves it. A copy within one row (`sets(UC, E.shadow == E.flag)`)
  reads that row.
- **A row an earlier step of the run wrote** (an earlier group, such as an API invoked before
  the target's), read before the step changes anything ([Runs](./state-derivation.md#runs)).
- **The target's parent** by a `relate`: a child created under a mandatory key reads the parent
  it is created under; an updated child reads the parent its key names, and waits while an
  `.optional()` key is unlinked. With two keys, `.role` names one:
  `sets(Fix, Permit.kind == Site.kind).role(home)`.

| Diagnostic | When |
|---|---|
| `UnboundCopySource` | Any other source: an entity the use case does not write that is not the target's parent; a child of the target; an entity written in a later group (order the `invokes` lines); the parent of a child created unlinked. The copy is not applied. |
| `SetsCopyMismatch` | The two columns are not one `Enum` (same values), one variation, two `Bool`, two of `Int` / `Money` / `Decimal`, two of `Date` / `DateTime`, or two `@null` `String`; a nullable source with a non-null target; a `@derived` source. |
| `SetsNoEffect` | `!=`, or a bare right-hand side (`E.qty == limit`). |
| `ConflictingEffects` | A copy and a literal of one column, or two copies from different columns. |
| `ChoosesAndSets` | A copy and a `chooses` of one column. |
| `DoubleModeledEnum` | A copy into a lifecycle column, on the creating use case too. |
| `EffectOnNonStateColumn` | A key column as source or target. |

On a created row the copy replaces the actor's choice of a variation and the `@default`. All
engines read a copy alike; TLA+ chooses the column's value, then guards `col'[p] = source[q]`.

### A value that follows a condition: `.when`

<!-- constrained-by #guard-one-value-when -->

```rdra
sets(ExtendPeriod, Extension.due_on == 3).when(ExpertInquiry)
sets(ExtendPeriod, Extension.due_on == 2).when(DisasterArea)
```

`sets(UseCase, Entity.column == value).when(Condition)` leaves the value only while the
condition's body holds, so the row's own columns pick what the use case leaves.

- The guard reads as a `chooses` value guard does ([Guard one value](#guard-one-value-when)): a
  created row after the step (it sees the value the actor picks), an updated row on the state
  the use case starts from.
- The lines of one column are one choice: the use case leaves the value of a line whose guard
  holds. No line enabled, no run. Two lines that hold at once each leave their value, one run
  each, so keep the guards disjoint. A value two lines give is enabled by either.
- All engines read the lines as a guarded `chooses`; TLA+ picks the value, then guards `\/
  (col'[p] = v /\ guard)` over the lines. The diagrams draw a `when` edge derived `via sets
  Entity.column value`; the sequence diagram writes the guard in `[...]` after the value.

| Diagnostic | When |
|---|---|
| `ConflictingEffects` | An unguarded `sets` or a copy of the column beside a guarded line. |
| `ChoosesAndSets` | A `chooses` of the column. |
| `SetsWhenOnCopy` | `.when` on a copy; guard the use case with `applies_to` instead. |
| `WhenRequiresCondition`, `WhenConditionWithoutBody` | As for an arm. |
| `ValueNeverChosen` (warning, `Choice_<UseCase>_<Entity>_<column>_<value>`) | A guarded value never left while its use case runs. |

### Driving comparison propositions with `drives`

```rdra
drives(Sell, Stock.stock < selling, true)     // after Sell, stock < selling holds
drives(Refund, Stock.stock < selling, false)  // after Refund, it does not
```

A comparison proposition (`stock < selling`, `expired_at < now`) is evaluated on its columns'
Int values over `0..5` and on `now`; `sets` on the columns moves it. When the model does not
track the values, `drives(UseCase, Entity.column op rhs, true | false)` drives its truth
instead. Once any `drives` names a comparison, it is a `Bool` variable only `drives` change, and
the columns no longer define it.

- `drives(Pay, Order.status == paid, true)` sets the value, as `sets` does. Driven `false`, an
  equality names no value: `DrivesFalseOnEquals`.
- A second argument that is not a comparison on `Entity.column`, or a third that is not `true` /
  `false`, is `DrivesNoEffect`. A set membership is `EffectMembership`.
- A use case whose own `sets` make the comparison the other truth is
  `ContradictoryDrivenComparison`. A use case that `sets` a column a driven comparison reads, and
  does not drive it, is the warning `DrivenComparisonColumnWritten`.
- The forms `sets(UC, E, col == v)`, `drives(UC, E, cmp, b)`, and `sets(UC, E, cmp, b)` fail to
  load with the rewrite ([Deprecated and removed forms](#deprecated-and-removed-forms)).

---

## Entity state constraints

<!-- derived-from ./state-derivation.md#temporal-anchor-assertions -->

Rules assert facts about the **reachable** states of the lifecycle: a rule is reported only
when a reachable state breaks it. This reference calls `forbidden`, `invariant`, `required`, and
`exclusive` rules, as `list --kind rule` does; a violation is the diagnostic
`ConstraintViolated`, printed under the rule's name. The kind `constraint` is an NFR guardrail.

| Rule | Shape | Name |
|---|---|---|
| `forbidden(E, c1, c2, ...)` | No reachable state meets every condition. | `Forbidden_<E>_<n>` |
| `invariant(E).when(g...).then(r...)` | Where every guard holds, every requirement holds. | `Inv_<E>_<n>` |
| `required(E, c1, ...)` | Every reachable state meets every condition. | `Required_<E>_<n>` |
| `exclusive(E, c1, c2, ...)` | No reachable state meets two or more. | `Exclusive_<E>_<n>` |

A condition is `column == value` or a [comparison expression](#comparison-expressions-in-constraints).

| Diagnostic | When |
|---|---|
| `RuleWithoutCondition` | `forbidden(Order)` or `required(Order)` with no condition. |
| `ExclusiveNeedsTwo` | `exclusive` with fewer than two conditions. |
| `InvariantWithoutConsequence` | An `invariant` with neither `.then(...)` nor a quantifier. |
| `InvariantWithoutWhen` | `.then(...)` without `.when(...)`. |
| `AfterWithoutAssert` | `after(UseCase)` without `.assert(...)`. |
| `Vacuous_<name>` (warning) | The rule holds and says nothing: no reachable state has the rows it ranges over, its `.when` guards never hold, or one condition of a several-condition `forbidden` is never met; an `after` whose use case never runs; a `property` whose rows never exist or whose `leads_to` trigger never holds. A `forbidden` whose state is unreachable draws none: that is its claim ([Rules that hold vacuously](./state-derivation.md#rules-that-hold-vacuously)). |

### Labelling a rule

```rdra
invariant(Appointment).when(status == checkedin).then(checked_in_at == present).label("A checked-in appointment records when the patient arrived")
```

`.label("...")` names a rule in the business's words: on `forbidden`, `invariant`, `required`,
`exclusive`, and `after(...).assert(...)`, one entity or several. It takes one quoted string,
once (`RuleLabelArg`, `DuplicateRuleLabel`). Say the rule in plain words, not the formula. A
violation of a labelled rule opens with `「label」: `; `list --kind rule` lists it beside the
[conditions](#conditions); the rule's name does not change. `lint` reports an unlabelled rule
as the info `rule-unlabeled`.

### Forbidden states: `forbidden`

```rdra
forbidden(Order, status == cancelled)
forbidden(Order, status == delivered, refunded == true)   // only when both hold
forbidden(Stock, status == on_sale, stock < selling)
forbidden(Coupon, expired_at < now)
forbidden(Coupon, status == active).when(PastDeadline)    // adds PastDeadline's body
```

The conditions are AND-ed: the state is forbidden only when every one holds. A violation
names the rule (`Forbidden_Order_0`), the conditions, and the state. Columns that two use cases
change separately can combine in a reachable state; set correlated columns in one use case.
A `.when(Condition)` adds the condition's body to the list ([`.when(Condition)` on a
rule](#whencondition-on-a-rule)).

### Implications: `invariant`

```rdra
invariant(Order).when(status == delivered).then(delivered_at == present)
invariant(Order).when(status == delivered).when(refunded == false).then(refund_id == null)
invariant(Coupon).when(expired_at < now).then(status == expired)
invariant(Stock).when(status == on_sale).then(stock < selling)
```

An implication: whenever every `.when` guard holds, every `.then` requirement must hold. Each
side's clauses are AND-ed. A reachable state that meets the guards and breaks a requirement is
reported with both.

#### `.when(Condition)` on a rule

```rdra
condition PastDeadline "the deadline has passed"
holds(PastDeadline, Coupon.expires_at < now)
applies_to(PastDeadline, Expire)

invariant(Coupon).when(PastDeadline).then(status == expired).label("Past its deadline, a coupon is expired")
// the same rule as
invariant(Coupon).when(expires_at < now).then(status == expired)
```

A `.when(...)` of an `invariant`, or after a `forbidden`'s arguments, may name a condition. The
rule reads the condition's comparisons as if written there (more guards, or more conditions),
so one business rule stated as a condition both guards the use case it `applies_to` and names
the state the rule reads.

- Every rule kind takes it: entity-local or multi-entity (the body may name any of the rule's
  entities, which join as inline comparisons do), and quantifiers
  (`invariant(Order).when(Paid).none(Payment.status == pending)`). `.when(Cond)` and
  `.when(column == value)` mix on an invariant; a `forbidden` takes a condition only.
- The `holds` may come later in the file.
- The rule's name and `list --kind rule` row are the inline form's; `list` prints the
  contributed comparisons in the rule's text.
- The body is read for comparisons only: a body with `.has`, `.none`, `.along`, or `.role`, or
  one on an entity-local rule that compares another entity, is `RuleWhenConditionScope`, whose
  hint says what to write on the rule. A body-less condition is `WhenConditionWithoutBody`; a
  `.when(X)` where `X` is no condition is `RuleWhenArg`.
- A condition only a rule reads keeps `lint` `rdra-condition-no-buc` until it `applies_to` a use
  case of a BUC; it is not `condition-no-rdra-anchor`.

### Facts that always hold: `required`

```rdra
required(Account, active == true)
required(Order, status == paid, paid_at == present)
required(Coupon, expired_at < now)
```

Every reachable state of the entity meets every listed condition: an invariant with no guard.

### Mutual exclusion: `exclusive`

```rdra
exclusive(Document, approved == true, rejected == true)
exclusive(Order, cancelled_at == present, delivered_at == present)
exclusive(Stock, stock < reorder_threshold, stock > overstock_threshold)
```

No reachable state meets two or more of the conditions: pairwise `forbidden`s under one name.

### Cross-entity constraints

```rdra
forbidden(Order, Payment, Order.status == cancelled, Payment.amount > Order.total)
invariant(Order, Payment).when(Order.status == paid).then(Payment.status == captured)
invariant(Order, Payment).along(Order, Payment).when(Order.status == paid).then(Payment.status == captured)
```

The leading entity arguments are the rule's scope, over every combination of their present
rows; `check`, `states`, and `verify` check it. A rule of one entity without `.along` is that
entity's (`Forbidden_Order_0`); a rule of several, or with `.along`, is cross-entity
(`MultiForbidden_Order_0`, `MultiInv_Order_0`).

- With several entities a condition names `Entity.column`; a bare column is rejected
  (`CrossConstraintColumnNeedsEntity`). With one, bare columns are shorthand.
- Without `.along`, the rule holds for every pair of rows, linked or not. `.along(A, B, ...)`
  limits it to rows connected through the listed `relate` path, junctions of `.via` included
  (`.along(A, J, B)`). Each adjacent pair needs a binding `relate` (`N:1`, `1:N`, `1:1`), and the
  path must cover every scope entity, else `UnbindableScope`. A path entity outside the scope
  only links the ends. An unlinked `.optional()` owner is not linked.
- At one instance `check` sees one pair, so a rule broader than meant passes `check` and fails
  `verify`. When keys relate the entities, `lint` `multi-rule-unscoped` names the `.along` of the
  shortest key path. `.global()` says the rule is meant for every combination; with `.along` it
  is `GlobalWithAlong`.
- `.lemma()` on a `forbidden` or `invariant` marks a fact that helps `verify --prove`. `check`
  checks it; `--prove` lists it apart and uses it once proved; `verify` runs no cone for it
  ([Proofs for any number of rows](./formal-verification.md#proofs-for-any-number-of-rows)).

<a id="marks-in-a-lemma"></a>**Marks in a lemma.** A `.lemma()` rule can also read a row's
marks, which no other rule reads, so a fact the proof needs about a run in flight or a raised
event can be stated:

| Condition | Holds when |
|---|---|
| `E.in_run(UC)` | A step of the split use case `UC` is in progress on the row: its run mark ([Runs](./state-derivation.md#runs)) is not `none`. |
| `E.in_run(UC, k)` | The row's run of `UC` has done step `k` and not the next (`k` from 1 to the steps minus one). |
| `E.raised(Ev)` | The row's latch for `Ev` is set ([Triggers](./state-derivation.md#triggers)). |

```rdra
// Open sets a door ajar, then lights a sign; no light is off while a door is between the two.
forbidden(Light, Door, Light.state == off, Door.in_run(Open)).global().lemma()
invariant(Sign).when(Sign.raised(EvShut)).then(Sign.state == dark).lemma()
```

- A mark goes among a `forbidden`'s conditions or in an `invariant`'s `.when(...)` and
  `.then(...)`, and names its entity. Anywhere else, or in a rule without `.lemma()`, it is
  `MarkOutsideLemma`, so the meaning of every other rule stays as it was.
- A mark the lifecycle has no var for is the error `NoSuchMark`: a use case in one
  transaction, an entity no step before the last writes, a `k` past the last step but one, or
  an event the row keeps no latch for (only an uncounted `triggers` reads a latch). A `k` of 0
  is `MarkStepOutOfRange`.

**`after(UseCase).assert(...)`** checks the state each run of the use case leaves, instead of
every reachable state:

```rdra
// Pay captures through PaymentApi and marks the order through OrderApi: two transactions, one
// run. The assert reads that run's order and payment after the second.
after(Pay).assert(Order.status == paid, Payment.status == captured)
```

- A use case of k transaction groups is k steps run in order ([Runs](./state-derivation.md#runs)).
  The assertion is read once per run, after the last step, on the run's rows; the state between
  steps is not checked. Its name is `AfterAssert_<UseCase>_<i>`.
- A last step that changes nothing (every column it sets already holds that value) is not
  checked, as in TLA+, where `[][…]_vars` admits a stuttering step. When the last step only ever
  runs that way, the assertion was never evaluated and `Vacuous_AfterAssert_*` says so.
- An atom on an entity no step writes or binds reads every present row: the warning
  `UnboundAfterRead`. Write the entity in the use case.
- An `after` on a use case that changes nothing is `AssertOnInertUseCase`.

### Temporal path properties

<!-- constrained-by ./formal-verification.md#temporal-properties-on-a-cone -->

```rdra
property PaidLeadsToShipped "paid eventually reaches shipped"
  leads_to(Order.status == paid, Order.status == shipped)

property EventuallyTerminal "order finishes"
  eventually((Order.status == delivered) or (Order.status == cancelled))

property NeverBoth "mutex"
  always(not ((Order.status == paid) and (Order.status == cancelled)))
```

`property <Id> ["Label"] <formula>` is checked by `verify` only, natively on its own cone, and
also by TLC under `--with-tlc` where TLA+ can state it. `check` and `states` evaluate no
property; a rule `check` must enforce is a `forbidden`, `invariant`, or `after` assertion.

| Form | TLA+ |
|---|---|
| `always(expr)` | `[](present => expr)` |
| `eventually(expr)` | `[](present => <>expr)` |
| `leads_to(p, q)` | `(present /\ p) ~> q` |
| `can_reach(expr)` | none: CTL `AG (present => E[present U (present /\ expr)])`, a comment |
| `stable(p)` | `[][(present /\ p) => (present' => p')]_vars` |
| `unchanged(E.col, ..).when(p)` | `[][(present /\ p) => (present' => col' = col)]_vars` |
| `precedes(p, q)` | `[](present /\ q => <P>_seen)`, a latch per instance |

- Atoms are `Entity.column == value` / `!=`, `in (...)` / `not in (...)`, or `<`, `>`, `<=`,
  `>=` over Int, Money, and Decimal columns and literals. Connectives are `and`, `or`, `not`
  (`/\`, `\/`, `~` are aliases).
- A formula ranges over the present rows of each entity, every combination unless `.along`
  limits it ([Scope](#scope-of-a-property-over-several-entities)).
- `present` is read where a property starts to speak of a row: all of `always`, the trigger
  of `eventually` and `leads_to`. Their goal is read on the row whether it is present or
  not, and a deleted row's columns read their base values ([Deleting a
  parent](#deleting-a-parent)). So deleting a row meets a goal its base values satisfy: with
  `pending` the `@default`, `leads_to(Order.status == paid, Order.status == pending)` holds
  where every paid order is eventually deleted. A goal the base values do not satisfy is
  never met by a deleted row: deleting a pending order breaks
  `leads_to(Order.status == pending, Order.status == paid)`. `can_reach` reads presence along
  the whole path, so a delete never finishes it. To keep a delete from meeting a goal, state
  the goal on a value other than the column's base value.
- With any `eventually` / `leads_to`, the exported Spec includes `WF_vars(Next)`: some step is
  taken while one can be, but none in particular, so another instance's loop meets it. Name the
  use cases a property relies on with [`.fair`](#fairness-fairusecase-).
- In TLA+, `after(UC).assert(...)` is its own `PROPERTY`, `[][(\A p : UC(p) => post')]_vars`, and
  each rule its own `INVARIANT` ([formal-verification.md](./formal-verification.md)).
- `Int`, `Money`, `Decimal`, `Date`, and `DateTime` columns something names are `0..5` variables
  (`null` added when nullable); `--int-range-max` changes the top. An Int literal outside it, or
  an integer `@default` outside it on a column a rule reads, is
  `IntLiteralOutOfBounds_<Entity>_<value>`: the model fails `check` and `verify` stops. `0` and
  the top are valid.

**Quantifier rules.** `invariant(Entity).when(conditions...).has(Related.col == val)` /
`.none(...)` is a to-many rule over present rows: `none` fails where the anchor's guard holds
and some related row matches. A `relate` links the related row through its owner; without one
it is independent. The related condition reads the related row, also of the anchor's own entity
(`invariant(Task).when(status == done).none(Task.status == open)` fails while one task is done
and another open). A one-entity `invariant` takes the related entity from the qualified
columns, the same as `invariant(Entity, Related)`. `.has(Related)` / `.none(Related)` read only
the link (`invariant(Order).when(status == closed).has(Invoice)`). `.none()` is
`QuantifierWithoutCondition`. Its name is `Quantifier_<Anchor>_<i>`. `check` and `states` run one
instance per entity, so use `--instance-count 2` or `verify` for rules that need two related
rows; to make a use case keep such a rule, put the same `.none` on a `holds` guard.

#### Counted quantifiers

```rdra
invariant(Member).at_most(3, Loan.status == open)               // at most three open loans
invariant(Claim).when(status == assessed).at_least(2, Estimate)  // two estimates or more
```

`.at_most(n, ...)`, `.at_least(n, ...)`, `.exactly(n, ...)` count the related rows linked as for
`.has` that match, per anchor row where the `.when` holds. A bare entity counts every linked
row. `.along` and `.role` work as on `.has`, and a `holds` body takes the same forms. Each rule
has one spelling:

| Written | Diagnostic | Write instead |
|---|---|---|
| `.at_least(0, ..)` | `QuantifierCountSpelling` | nothing: it always holds |
| `.at_least(1, ..)` | `QuantifierCountSpelling` | `.has(..)` |
| `.at_most(0, ..)`, `.exactly(0, ..)` | `QuantifierCountSpelling` | `.none(..)` |
| `n` not a whole number from 1 to 255 | `QuantifierCountInvalid` | a whole number |

- `.at_most(n)` can fail only with `n + 1` rows, so it needs `n + 1` slots. With fewer it is
  `Vacuous_<rule>`, naming `--instances Entity=k` ([Per-entity instance counts](./cli-reference.md#per-entity-instance-counts)).
- `.at_least(n)` and `.exactly(n)` with fewer than `n` slots fail wherever they apply, an
  artifact of the bound: the warning `CountAboveBound` names the same flag.
- TLC counts with `Cardinality` (the module extends `FiniteSets`). `--prove` unrolls up to 8
  rows: `.at_most(7, ..)` is the largest it proves. See
  [design/counting-rules.md](./design/counting-rules.md).

**Along a path.** `.along(Anchor, .., Related)` after the quantifier counts the related rows,
and every entity between, that the hops' keys chain to the anchor's row. `.none` holds when no
chain reaches a match (no chain passes); `.has` when one does. The path runs from the anchor to
the counted entity, each once (else `QuantifierAlongShape`, which writes the shortest path); a
hop no key joins is `UnbindableScope`. A `.role` after the `.along` picks a hop's key; one after
the quantifier is not read.

```rdra
// Without the path no key joins Member and Loan, and the rule would count every loan.
invariant(Member).when(status == withdrawn).none(Loan.status == open).along(Member, Card, Loan)
```

#### Scope of a property over several entities

<!-- constrained-by #cross-entity-constraints -->

```rdra
// Without .along, a captured payment beside another, unpaid order breaks it.
property CapturedPaid "a captured payment's order is paid"
  always((not (Payment.status == captured)) or (Order.status == paid)).along(Payment, Order)
```

A property whose atoms read several entities ranges over every combination of their rows.
`.along(A, B, ..)` limits it to rows the path's keys link (`.role` picks a hop's key), as on a
multi-entity `invariant`. The link joins the trigger, beside `present`:

| Form | TLA+ with `.along` |
|---|---|
| `always(x)` | `[](present /\ link => x)` |
| `eventually(x)` | `[](present /\ link => <>x)` |
| `leads_to(x, y)` | `(present /\ link /\ x) ~> y` |
| `stable(p)` | `[][present /\ link /\ p => (present' => p')]_vars` |
| `unchanged(cols).when(p)` | `[][present /\ link /\ p => (present' => cols' = cols)]_vars` |
| `precedes(p, q)` | the latch is set where `present /\ link /\ p`, and `[](present /\ link /\ q => <P>_seen)` |

`link` is each hop's owner link (`Payment_owner_Order[b0] = b1`). A path entity no atom reads only
links the ends. The path must cover every entity the atoms read, and each hop needs a key, else
`UnbindableScope` and the property is not checked. A counterexample names the linked rows.
`.global()` says every combination is meant (`GlobalWithAlong` with `.along`); with neither and
keys relating the entities, `lint` `multi-rule-unscoped` warns. `.along` and `.global()` may come
before or after `.fair(...)`.

#### Order and stability: `stable`, `unchanged`, `precedes`

```rdra
property CancelledStaysCancelled
  stable(Order.status == cancelled)

property AmountKeptOnceApproved
  unchanged(Order.amount).when(Order.status in (approved, shipped))

property ShippedAfterPaid
  precedes(Order.paid == true, Order.status == shipped)
```

- `stable(p)`: once `p` holds for a present instance, every step into a state where it is still
  present keeps `p`. (`after(UC).assert` reads only that use case's steps; `stable` reads every
  step.)
- `unchanged(E.col, ..).when(p)`: a step from a state where the instance is present and `p`
  holds, into one where it is present, leaves each column as it is. Columns are `Entity.column`
  or a bare column one entity owns. Without `.when`, the columns never change while present.
  `.when(p)` comes directly after `unchanged(...)`. A logical `@derived` column is rejected;
  name the columns it reads.
- `precedes(p, q)`: where the instance is present and `q` holds, `p` holds now or held earlier,
  possibly in other columns or, with `.along`, other entities. A latch remembers `p`: in TLA+ the
  Boolean `<Property>_seen` per instance (per tuple with several binders), set in `Init` and by
  every action whose successor has the instance present with `p`.

Each is a safety property. `verify` decides it by BFS on its cone (`stable` and `unchanged` on
every edge, `precedes` on every state the latch can reach unset), and TLC checks it on the same
cone under `--with-tlc`. A violation prints the shortest trace to the breaking step or state,
with no loop. `export --kind tla` writes `stable` and `unchanged` as action properties (`Action
property <P> is violated` in TLC) and `precedes` as `[](...)` over the latch (`Invariant <P> is
violated`); a module declares the latch only where its `.cfg` checks the property. `.along`,
`.role`, and `.global()` scope them as the others; the link joins the trigger before the step
only, so a step that moves a key and breaks `p` still breaks the property. `.fair(...)` on them
is `FairOnSafetyProperty`.

#### Fairness: `.fair(UseCase, ...)`

```rdra
property GrantEnds "a granted application is registered or dismissed"
  leads_to(Application.status == granted, Application.status in (registered, dismissed)).fair(Register, Dismiss)
```

`.fair(...)` after an `eventually` or `leads_to` names the use cases the property assumes are not
starved. Each is weakly fair **per instance it acts on**: if its step on one application stays
enabled, it is eventually taken on that application. The property holds on every run that meets
the assumption. In TLA+ it is the implication:

```
Fair_GrantEnds ==
  /\ \A p0 \in Application_Ids : WF_vars(Register(p0))
  /\ \A p0 \in Application_Ids : WF_vars(Dismiss(p0))
GrantEnds == Fair_GrantEnds => (\A b0 \in Application_Ids : ...)
```

- The `\A` ranges over the rows the use case reads or creates under; a created row is bound
  inside (`WF_vars(\E p1 \in Patent_Ids : Register(p0, p1))`), so the next free slot moving on
  resets nothing. A split use case is fair on each action, later ones per run, so a started run
  is fair to finish.
- Per instance, not per use case: `WF_vars(\E p : Register(p))` would not promise each
  application finishes.
- Weak fairness only; there is no `SF` form. There is no model-wide switch: it would change
  every property's verdict and hide real liveness gaps.
- `Spec`, the `.cfg`, and properties without `.fair` are unchanged; `Spec /\ Fair_P` satisfies
  `P` exactly when `Spec` satisfies `Fair_P => P`.
- `.fair` keeps the property on its cone: `verify` checks it on the cone of what it reads and of
  the listed use cases; one whose verdict rests on `WF_vars(Next)` alone needs the whole model
  ([Temporal properties on a cone](./formal-verification.md#temporal-properties-on-a-cone)).
  `verify` prints `<Property> assumes weak fairness per instance on ...`.

#### Recoverability: `can_reach(...)`

```rdra
property OrderCanFinish "every order can still be delivered or cancelled"
  can_reach(Order.status in (delivered, cancelled))
```

`can_reach(expr)`: for each present instance, from every reachable state, some path, which may
move other instances, reaches a state where `expr` holds for it, with the instance present
throughout (deleting it does not finish it). This is CTL
`AG (present => E[present U (present /\ expr)])`. It takes
`.along`, `.role`, and `.global()`. An instance may still loop forever: an order that can be held
and resumed forever passes while it can also be delivered. It finds what the others miss: a
`Deadlock_*` needs a state with no way out, and `eventually` / `leads_to` need `.fair` and fail
when an actor may choose another branch forever.

TLC cannot check CTL, so `verify` decides it by BFS on its cone, within its bounds; `export
--kind tla` lists it in a comment that no `.cfg` names. A violation prints a shortest trace to a
state from which the goal is unreachable, then what the goal reads of the instance in the states
it is confined to:

```
error[OrderCanFinish]: property OrderCanFinish is violated: from the counterexample's last state no path reaches its goal for Order#1
  ...
  confined: from there Order#1 stays within 34 reachable states, where its goal never holds:
    status = pending
    status = on_hold
```

See [Recoverability on a cone](./formal-verification.md#recoverability-on-a-cone) and the
samples `can_reach_unmet.rdra` and `can_reach_met.rdra`.

**Property diagnostics.**

| Diagnostic | When |
|---|---|
| `FairOnSafetyProperty` | `.fair` on `always`, `stable`, `unchanged`, or `precedes`. |
| `FairOnReachProperty` | `.fair` on `can_reach`. |
| `FairRequiresUseCases` | `.fair` with no argument, a non-use-case, a use case twice, or a second `.fair`. |
| `FairOnInertUseCase` | A listed use case changes nothing. |
| `UndefinedSymbol` | An undefined name. |
| `UnresolvedPropertyColumn` | An atom whose column no single entity owns (`Order.missing`, or a bare `status` two entities declare). |
| `UnknownChainMethod` | A method other than `.fair`, `.along`, `.role`, `.global`, or a `.when` not directly after `unchanged(...)`. |
| `InvalidTemporalProperty` | A column of `unchanged(...)` that is not a column. |
| `DuplicateProperty` | Two properties with one id. |

### Comparison expressions in constraints

`forbidden`, `required`, `exclusive`, multi-entity rules, `invariant`'s `.when` / `.then` /
`.has` / `.none`, `holds`, `after(...).assert(...)`, `@check`, `@derived`, and `property` take
comparison expressions as conditions.

#### Syntax

```
col op rhs
col in (v, ...)
col not in (v, ...)
```

- `col` is a bare column, or `Entity.column` in a cross-entity rule.
- `op` is `<`, `>`, `<=`, `>=`, `==`, or `!=`.
- `rhs` is a value of `col`'s type (with `==` / `!=`: `status == paid`, `active == false`,
  `paid_at == null`), a column (`stock >= selling`, `Payment.amount >= Order.total`), an integer
  literal (`quantity >= 0`), or `now` (`expired_at < now`). `col`'s type decides what a bare
  `rhs` is ([Bare identifiers](#bare-identifiers)).

#### Set membership: `in` and `not in`

```rdra
forbidden(Order, status in (paid, shipped), refund_id == present)
invariant(Order).when(status not in (draft, cancelled)).then(submitted_at == present)
holds(Open, Order.status in (draft, submitted))
required(Ticket, priority in (1, 2, 3))
```

`col in (a, b)` is `col == a or col == b`; `col not in (a, b)` is `col != a and col != b`. Every
construct that takes a comparison takes it, except `sets` and `drives` (`EffectMembership`). A
set of every value of the column is true (its `not in` false) when the system is built:
`.none(Child.status in (<every value>))` means "no child row" and pulls no column into a cone;
`.none(Child)` says it without listing values. `in` is a keyword only between a left-hand column
and `(`, so a column may be named `in`. TLA+ writes `Order_status[b0] \in {"paid", "shipped"}`
and `\notin`; `list`, the sequence diagram, and violations print the set as written, with value
labels where they print labels for `==`.

| Rule | Diagnostic |
|---|---|
| The column is `Enum`, variation, `Bool`, or `@null`, or `Int` / `Money` / `Decimal` with integer literals. | `SetColumnNotEnumerable` |
| Each value is a value of the column's type, read as a bare `==` right-hand side; a column is no value. The error lists the values. | `SetValueNotAValue` |
| Each value appears once. | `DuplicateSetValue` |
| The list has at least one value. | parse error |

#### Type rules

| Operator | Column types |
|---|---|
| `<`, `>`, `<=`, `>=` | `Int`, `Money`, `Decimal`, `Date`, `DateTime` |
| `==`, `!=` | every type |
| `in`, `not in` | `Enum`, variation, `Bool`, `@null`; `Int`, `Money`, `Decimal` with integer literals |

Two columns compare within one category (numeric with numeric, temporal with temporal). Two
`Enum`, variation, or `Bool` columns compare with `==` / `!=` when they have one set of values
(`status != Order.previous`). `now` needs a `Date` or `DateTime` column. Otherwise:
`ComparisonTypeMismatch`, `ComparisonOpNotOrdered`, `ComparisonNowRequiresTemporal`, or, in the
lifecycle, `UnsupportedComparison`.

#### Null

A `@null` column holds `null` as one of its values, whatever its type. A row created with no
value and no `@default` for it starts at `null`, as the SQL row does. This holds for `Enum` and
`Bool` too: `color: Enum(red, blue) @null` starts at `null`, not `red`; `active: Bool @null` at
`null`, not `false`. `states` lists `null` among the column's values (`color[null|red|blue]`).
On a lifecycle column the null is no state of the state model.

A comparison is true or false, never unknown:

- A null column meets no comparison with a value, a column, or `now`: `qty >= 1`, `qty == 3`,
  `qty < Order.limit`, `color == red`, `active == false` are false. It meets every `!=`: `qty !=
  3`, `color != red` are true.
- Two null columns are not equal: `qty == lim` and `color == Item.shade` are false, `qty != lim`
  true, as in SQL. A nullable and a non-null column of one set of values compare by value.
- `col == null` and `col == present` ask about null itself. `col in (...)` is the `or` of its
  equalities: `color in (red, blue)` is false and `not in` true while null; `color in (red,
  null)` holds there.
- Every rule form reads comparisons this way: `forbidden`, `required`, `exclusive`, `invariant`
  (`.when`, `.then`, `.has`, `.none`), `holds`, `after(...).assert(...)`, `property`, `@check`.
  So `required(Item, qty >= 0)` fails on a null `qty`, `forbidden(Item, status == done, qty <
  1)` does not fire on it, and `qty: Int @null @check(qty >= 1)` fails while `qty` is null, at
  creation too. A rule that admits null says so: `@check(qty == null or qty >= 1)`.
- A logical `@derived` column is a value, not a rule: `bulk: Bool @derived(qty >= 3)` is neither
  `true` nor `false` while `qty` is null (`bulk == null` holds). `warm: Bool @null @derived(color
  == red)` is null while `color` is.
- A unique set reads null the same way: two null rows do not agree, so neither breaks
  `Unique_<Entity>_<columns>` ([Unique column sets](./state-derivation.md#unique-column-sets)).
- `sets(UC, E.col == null)` writes null. `@default(null)` is no value of an `Enum` or `Bool`.

#### Semantics and interaction with `sets`

A comparison is evaluated on its columns' values: numeric and date columns it names are `0..5`
variables, and `now` is a clock that ticks to 5. `sets(UC, Entity.col == N)` changes the values.
A comparison no `drives` names is no variable: `forbidden(Stock, stock < selling)` is violated in
any reachable state where `stock` is below `selling`, in `states`, `check`, and `verify` alike.
To say a use case makes it true or false without modeling the values, use
[`drives`](#driving-comparison-propositions-with-drives).

### Bare identifiers

A bare right-hand side `x` of `col == x` or `col != x`, and each value of `col in (x, y)`, is
read by the left-hand column's type, in every construct that takes a comparison (`sets`
included):

| Left-hand column | A bare `x` is |
|---|---|
| `Enum`, variation, or `Bool` | a value of the column (`null` and `present` too when `@null`); any other `x` is an error, even when a column is named `x` |
| any other `@null` column | `null` or `present`; any other `x` is a column |
| any other column | a column |

So `status == paid` is the value `paid` even beside a column `paid`; values need to be unique
only within their column. Comparing two `Enum`, variation, or `Bool` columns qualifies the right
side; bare, it is `ComparisonRhsNotAValue`:

```
'previous' is not a value of column 'status' (pending, paid, cancelled), and a bare right-hand side of 'status' is a value
  hint: write `Order.previous` to compare columns
```

`transitions` endpoints and `@default(v)` take values only. A value of an `Enum` or variation is a
bare token; a quoted string is free text ([metadata clauses](#metadata-clause-values)).
`chooses(Pay, Order.kind, "a")` and `@default("open")` on an `Enum` or variation column are
`QuotedValue`, whose hint gives the bare spelling. In comparisons, `in` lists, `transitions`
endpoints, and `Enum(...)` / `variation` declarations a quoted value is a syntax error.
`@default("4")` on an `Int`, or a quoted `String` default, stays as written. `now` is a keyword
on the right of a comparison whose column is a `Date` or `DateTime`; it never names a column.

---

## Limitations

### Unsupported or limited

| Topic | Limitation |
|---|---|
| `relate(..., N:M)` without `.via`, `relate(A, A, N:M)`, `.via` equal to an end | Errors (`NMRelation`, `SelfNMRelation`, `ViaMustBeDistinct`); use a distinct junction. |
| AsyncAPI | Event causality only; protocols, servers, and payload schemas are not inferred. |
| OpenAPI | Unknown `auth` schemes become `apiKey` with `x-rdra-ish-auth-scheme`; `requires_permission` is an extension, not `security`. |
| `.on_delete(cascade \| set_null \| set_default)`, `.on_update` | Not run by the lifecycle; see [Deleting a parent](#deleting-a-parent). |

### Removed kinds

These kinds had no review output. A model that uses one fails to load with the replacement:

| Removed | Use instead |
|---|---|
| `usagescene` | `timer` for a schedule; `medium` / `.by(...)` for context. |
| `timing` | `timer`, connected with `triggers(Timer, UseCase)`; `belongs(...).when(...)` is gone. |
| `uses` | Events: `raises(ExtSystem, Event)` inbound, `triggers(Event, ExtSystem)` outbound. |
| `location` | Gone with `belongs(...).where(...)`; use `medium` / `.by(...)`. |
| `state` | Delete it. States are the values of the `Enum` column `transitions` names; each is `{Entity}_{value}` (`state__{Entity}_{value}` in diagrams; `{Entity}_{column}_{value}` with [several lifecycle columns](#several-lifecycle-columns-on-one-entity)). |
| `concept`, `domain_object`, `aggregate`, `value_object` / `valueobject`, `maps_to` | `entity` for persistent structure, `dto` for payloads. |

A top-level `when(...)` rule was removed too. `when(Entity, conditions...).none(...)` and
`when(Entity.col == val).none(...)` fail with the rewrite, `invariant(Entity).when(...).none(...)`,
the entity being the leading argument or that of the first qualified column, the rest of the
chain kept.

### Deprecated and removed forms

A form leaves the language in two stages:

1. **Deprecated** in a minor release. The model loads: the parser rewrites the form, and the
   `DeprecatedSyntax` warning names the rewritten line, the version that deprecated it, and the
   version that removes it. Commands still succeed; `lint` lists it as `deprecated-syntax`;
   `check --deny-deprecated` makes it an error; `fmt --write` rewrites it.
2. **Removed** no earlier than the next major release. The model fails to load, with the
   rewritten line.

`LEGACY_FORMS` in `crates/rdra-ish-syntax/src/removed.rs` lists these forms with their status
and versions. No form is deprecated now. The forms below were removed before or in 1.0.0,
without a deprecated stage; each load error gives the rewritten line.

**A rule names its entities first** (1.0.0). `forbidden`, `required`, and `exclusive` whose first
argument is a condition, and `invariant()` with no entity, are removed. The error puts the
entities the conditions name in front, those of each `.when` before those of each `.then`:
``write `forbidden(Order, Ticket, Order.status == paid, Ticket.status == pending)` ``. A rule over
several entities keeps its name. `scripts/migrate-syntax-v3.py` rewrites these.

| Removed | Use instead |
|---|---|
| `forbidden(A.col == v, B.col == w)`; `required` / `exclusive` alike | `forbidden(A, B, A.col == v, B.col == w)`; `forbidden(A, A.col == v)` for one entity |
| `invariant().when(A.col == v).then(B.col == w)` | `invariant(A, B).when(...).then(...)`; `invariant(A).when(...).none(...)` for a quantifier |

**A column is `Entity.column`** (1.0.0). `scripts/migrate-syntax-v3.py <workspace>` rewrites these
in place and is safe to run twice.

| Removed | Use instead |
|---|---|
| `sets(UseCase, Entity, column == value)`, `sets(UseCase, Entity, column == Source.column)` | `sets(UseCase, Entity.column == value)`, `sets(UseCase, Entity.column == Source.column)`; a `.by(...)` after a copy becomes `.role(...)` |
| `drives(UseCase, Entity, column op rhs, true \| false)` | `drives(UseCase, Entity.column op rhs, true \| false)` |
| `sets(UseCase, Entity, column op rhs, true \| false)` | [`drives`](#driving-comparison-propositions-with-drives)`(UseCase, Entity.column op rhs, true \| false)` |

**`.by` that picks a key is `.role`** (1.0.0); `belongs(Buc, Business).by(Medium)` keeps `.by`.
`scripts/migrate-syntax-v3.py` rewrites it.

| Removed | Use instead |
|---|---|
| `holds(Cond, ...).by(role)`, `.has(...).by(role)`, `.none(...).by(role)`, `.along(...).by(role)`, `sets(UC, E.col == F.col).by(role)` | the same with `.role(role)` |

**One form per predicate** (0.4.0). `scripts/migrate-syntax-v2.py <workspace>` rewrites these in
place and is safe to run twice.

| Removed | Use instead |
|---|---|
| `transitions(Entity.column, UseCase, from -> to)` | `transitions(UseCase, Entity.column, from -> to)` |
| `maps_field(Field, Entity, "column")`, `maps_field(Field, Entity, column)`, `maps_field(Field, Entity, Entity.column)` | `maps_field(Field, Entity.column)` |
| `acceptance criteria "..."`, `acceptance "..."` | `acceptance_criteria "..."` |
| `writes(UseCase \| Api, Entity)` | `updates`, or `creates` / `deletes`; the CRUD matrix shows `U` where it showed `W` |

---

## See also

- [cli-reference.md](./cli-reference.md): what each command does.
- [state-derivation.md](./state-derivation.md): how reachable states are explored, how predicates
  become actions and guards, and every finding name.
- [formal-verification.md](./formal-verification.md): `verify` and the TLA+ export.
