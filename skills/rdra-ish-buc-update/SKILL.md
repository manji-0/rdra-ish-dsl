---
name: rdra-ish-buc-update
description: Update an existing BUC by adding or modifying use cases, business flows, requirements, screens, fields, API contracts, concepts, NFRs, events, entities, ADR links, and predicates while preserving staged refinement
---

## Update an existing BUC

Given a description of what to add or change, modify the relevant `.rdra` files while keeping the model consistent.

Preserve the model's current abstraction level. If the BUC is still a skeleton, do not
force columns, APIs, or lifecycle rules. Ask for the next missing information required
by `../../docs/incremental-modeling.md` and apply the smallest stage-appropriate diff.
Keep the business-to-technical refinement order intact: do not introduce technical
details until the business value, actors, use cases, and data touchpoints that justify
them are present.

### Step 1 — Read the existing BUC

Read `buc/buc_<name>.rdra` and the shared files it imports. Identify:
- Which use cases already exist
- Which entities are already referenced
- What screens and events are already declared
- Which flow/step, field, DTO, requirement/NFR, concept, and ADR elements already exist
- Which predicates are already defined
- Whether the model still uses the small layout (`shared/actors.rdra`,
  `shared/biz.rdra`, `shared/entities.rdra`) or has split shared files

### Step 2 — Classify the change

| Change type | Where to edit |
|-------------|---------------|
| New use case in existing BUC | `buc/buc_<name>.rdra` |
| New business flow order, branch, exception, or loop | `buc/buc_<name>.rdra` with `flow`, `step`, `precedes`, `branches`, `excepts`, `repeats`, `covers` |
| New requirement metadata | stable shared requirement file, then `motivates(Requirement, Buc)` |
| New screen for an existing UC | `buc/buc_<name>.rdra` |
| New screen field or input/output mapping | owning BUC/screen file; add `field`, `contains(Screen, Field)`, `maps_field` |
| New API method/path or payload | owning API file; add API metadata, `dto`, `request`, `response`, `error_response` |
| New concept/domain term | shared conceptual file; add `maps_to` only when logical data mapping is known |
| New NFR/quality/constraint | stable shared file; scope with `applies_to`, `qualifies`, or `constrains` |
| New ADR impact | stable shared ADR file; connect with `decides(Adr, Target)` |
| New entity column | `shared/entities.rdra` |
| New entity entirely | `shared/entities.rdra` + add `relate` if needed |
| New event or state | `shared/entities.rdra` (if cross-BUC) or `buc/buc_<name>.rdra` |
| New actor | `shared/actors.rdra` |
| New system/API ownership | shared vocabulary for `system`, BUC/shared API file for `api`, then `contains(System, Api)` |
| New Business-BUC context | shared vocabulary for `location` / `timing` / `medium`, then `belongs(...).when(...).where(...).by(...)` |
| New permission or medium constraint | shared `permission` / `medium` vocabulary, then `has_permission`, `requires_permission`, or `requires_medium` in the owning BUC file |
| New event-started BUC | target BUC file, then `triggers(Event, Buc)`; add `triggers(Event, EntryUC)` only after the entry UC is known |
| New entity rule | shared rules file or shared entity area; use `forbidden`, `invariant`, `required`, or `exclusive` for single-entity state facts; use `cross_forbidden` / `cross_invariant` with `Entity.column` conditions for multi-entity rules, and add `.along(...)` only for relation-scoped linked-instance intent |
| Cross-system entity relation handling | BUC file that owns the coordinating use case |
| Remove a use case | Remove its `contains`, CRUD, `displays`, `raises` predicates; check no other BUC uses it |

If the model still uses the small layout, keep using it unless the change makes a
shared file hard to review. If shared files are already split, place new shared
definitions near their owning area and mirror path/module names.

Also classify the abstraction transition:

| Current state | Concern | Next useful update | Ask the user for |
|---------------|---------|--------------------|------------------|
| BUC exists but no actors | Biz intent/value | actor coverage | who performs or receives value from the BUC |
| Actors/use cases exist but no CRUD | Biz object touchpoints | data touchpoints | entities and CRUD intent per use case |
| CRUD exists but no screens/API | Tech interaction boundary | interaction boundary | screens, external interfaces, API endpoints, owning systems, access/media constraints |
| Entities have only `id` | Tech data design | entity structure | fields, keys, relationships, cardinality |
| Structured entities have lifecycle fields | Tech lifecycle design | lifecycle | states, events, use-case effects |
| Lifecycle reaches plausible patterns | Tech-enforced rules | local guardrails | invalid or mutually exclusive state facts via `forbidden` / `exclusive` |
| Local guardrails are stable | Tech-enforced rules | obligations and advanced rules | `invariant`, narrow `required`, comparison propositions, or cross-entity rules when needed |

### Step 3 — Apply the minimal diff

Add only what is needed. Do not remove or rename existing predicates unless the requirement explicitly says to delete functionality.

**Adding business flow order:**
```
flow <Flow> "<flow label>"
step <StepA> "<business step>"
step <StepB> "<business step>"

contains(Buc<Name>, <Flow>)
contains(<Flow>, <StepA>)
contains(<Flow>, <StepB>)
precedes(<StepA>, <StepB>)
covers(<StepB>, <ExistingUC>)
```

Use `branches`, `excepts`, and `repeats` only when the requirement explicitly
introduces an alternative, exception, or loop.

**Adding a use case:**
```
usecase <NewUC> "<action description>"

contains(Buc<Name>, <NewUC>)
<crud>(<NewUC>, <Entity>)
displays(<NewUC>, <Screen>)
```

**Adding a column to an existing entity:**
```
// in shared/entities.rdra — append inside the entity block
  <col_name>: <Type>  @null   // or other annotations
```

Then add `sets` in the BUC file for every use case that writes this column.

**Adding a new entity with a relationship:**
```
// in shared/entities.rdra
entity <NewEntity> "<label>" {
  id:   Int @pk
  ...
}
relate(<Parent>, <NewEntity>, 1:N)
```

Then in the BUC file add CRUD predicates for the use cases that touch it.

**Adding or changing API/system boundaries:**
```
system <System> "<system label>"
api <Api> "<API label>" method PATCH path "/orders/{id}" idempotency idempotent mode sync auth bearer
dto <RequestDto> "<request label>" {
  id: Int
}

contains(<System>, <Api>)
invokes(<UC>, <Api>)
request(<Api>, <RequestDto>)
updates(<Api>, <Entity>)
sets(<UC>, <Entity>, column == value)
```

System entity sets are derived from API CRUD. Add `owns(System, Entity)` only when
intentional ownership must be visible before complete API operations exist, and
review the warning if explicit ownership disagrees with derived ownership.

**Adding a screen field:**
```
field <Field> "<field label>"
  access editable
  required true
  source actor

contains(<Screen>, <Field>)
maps_field(<Field>, <Entity>, "column_name")
```

**Adding Business-BUC context:**
```
timing <When> "<timing>"
location <Where> "<place or channel>"
medium <Medium> "<device or terminal>"

belongs(Buc<Name>, <Business>)
  .when(<When>)
  .where(<Where>)
  .by(<Medium>)
```

**Adding access constraints:**
```
permission <Permission> "<permission>"

has_permission(<Actor>, <Permission>)
requires_permission(<UC>, <Permission>)
requires_medium(<UC>, <Medium>)
requires_permission(<Api>, <Permission>)
```

Use `requires_*` on the use case for constraints that apply to the whole interaction,
and on the API for constraints specific to that backend boundary. Screen constraint
patterns are derived through `displays` and `invokes`; do not hand-write a separate
screen predicate for them. Actor-side grant gaps are derived with
`rdra-ish csv src/ --kind actor-permission-audit`; use
`rdra-ish csv src/ --kind permission-callables` to confirm the permission maps to the
intended use cases and APIs.

**Adding non-functional or decision traceability:**
```
nfr <Nfr> "<label>"
  metric "latency"
  target "p95 <= 300ms"
  slo "99.9%"

quality <Quality> "<quality label>"
qualifies(<Quality>, <System>)
applies_to(<Nfr>, <Api>)

adr <Adr> "<decision label>"
decides(<Adr>, <Api>)
```

**Starting a BUC from an event:**
```
triggers(<Event>, <TargetBuc>)
```

If the entry action is already known:
```
contains(<TargetBuc>, <EntryUC>)
triggers(<Event>, <EntryUC>)
```

**Handling a cross-system relation:**
```
coordinates(<UC>, <EntityA>, <EntityB>)
invokes(<UC>, <ApiForEntityA>)
invokes(<UC>, <ApiForEntityB>)
```

Use this only when `<EntityA>` and `<EntityB>` are related and belong to different
derived system boundaries.

**Adding cross-entity rules:**
```
forbidden(<EntityA>, <EntityB>,
  (<EntityA>.<column>, <value>),
  <EntityB>.<column> > <EntityA>.<column>)

invariant(<EntityA>, <EntityB>)
  .along(<EntityA>, <EntityB>)
  .when(<EntityA>.<column>, <value>)
  .then(<EntityB>.<column>, <value>)
```

Use bare column names only when the rule has a single-entity scope. Multi-entity rules
must qualify columns as `Entity.column`.
Use `.along(...)` only when the rule is about instances linked by a declared `relate`
path; current `states` reports such relation-scoped rules as `CrossConstraintNotEvaluated`
instead of checking the global cross-product.

**Removing a use case:**
1. Delete the `usecase` declaration
2. Delete `contains(Buc, <UC>)`
3. Delete all CRUD, `displays`, `raises`, `sets` predicates referencing it
4. If the UC was also referenced by `triggers` or `performs` elsewhere, update those too

### Step 4 — Consistency checks after the change

- Every new `usecase` has a `contains` predicate
- Every new `flow` belongs to a BUC, every `step` belongs to a flow, and covered
  behavior exists.
- Every new `usecase` has at least one CRUD predicate
- Every new `field` belongs to a screen; actor-entered fields are mapped or have an
  explicit reason to remain external.
- API method/path metadata has request/response DTOs when contract review is in scope.
- Every new `entity` column that can change has a `sets` or participates in `transitions`
- No dangling imports (if you removed the last use of a symbol, remove its `import`)
- No manually added FK columns for a `relate`-covered relationship
- Every API used as a system boundary has `contains(System, Api)`
- Every explicit `owns(System, Entity)` is either satisfied by API CRUD or accepted
  as future ownership with diagnostics reviewed.
- Every new `permission` used by an actor or operation is declared once in shared vocabulary unless intentionally local
- Every `requires_medium` references a declared `medium`, and screen constraints are checked with `rdra-ish csv src/ --kind screen-constraints`
- Permission-to-callable mappings are checked with `rdra-ish csv src/ --kind permission-callables`
- Actor grants are checked with `rdra-ish csv src/ --kind actor-permission-audit`; review both `missing` and `excess`
- Every cross-system `relate` has a `coordinates(UseCase, Entity, Entity)` when a use case handles the consistency
- Every `coordinates` use case invokes APIs on both system sides
- Every cross-entity rule qualifies columns as `Entity.column` and names or implies
  the participating entities
- Every `.along(...)` cross-entity rule names a declared `relate` path and is treated
  as `CrossConstraintNotEvaluated` until linked-instance reachability is implemented
- The diff does not jump more than one abstraction level unless the user supplied the
  missing information explicitly
- New shared files follow path/module correspondence, e.g.
  `shared/lifecycle/order.rdra` with `module shared.lifecycle.order`

### Step 5 — Validate

```
rdra-ish check src/
rdra-ish lint src/ --format table
```

Fix every reported error before declaring the update done.
