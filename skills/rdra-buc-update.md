---
name: rdra-buc-update
description: Update an existing BUC by adding or modifying use cases, screens, events, entities, and predicates while preserving staged refinement
---

## Update an existing BUC

Given a description of what to add or change, modify the relevant `.rdra` files while keeping the model consistent.

<!-- derived-from ../docs/language-reference.md#access-constraints -->
<!-- derived-from ../docs/language-reference.md#belongs-context -->
<!-- derived-from ../docs/incremental-modeling.md#stage-3-interaction-boundary -->

Preserve the model's current abstraction level. If the BUC is still a skeleton, do not
force columns, APIs, or lifecycle rules. Ask for the next missing information required
by `docs/incremental-modeling.md` and apply the smallest stage-appropriate diff.
Keep the business-to-technical refinement order intact: do not introduce technical
details until the business value, actors, use cases, and data touchpoints that justify
them are present.

### Step 1 — Read the existing BUC

Read `buc/buc_<name>.rdra` and the shared files it imports. Identify:
- Which use cases already exist
- Which entities are already referenced
- What screens and events are already declared
- Which predicates are already defined
- Whether the model still uses the small layout (`shared/actors.rdra`,
  `shared/biz.rdra`, `shared/entities.rdra`) or has split shared files

### Step 2 — Classify the change

| Change type | Where to edit |
|-------------|---------------|
| New use case in existing BUC | `buc/buc_<name>.rdra` |
| New screen for an existing UC | `buc/buc_<name>.rdra` |
| New entity column | `shared/entities.rdra` |
| New entity entirely | `shared/entities.rdra` + add `relate` if needed |
| New event or state | `shared/entities.rdra` (if cross-BUC) or `buc/buc_<name>.rdra` |
| New actor | `shared/actors.rdra` |
| New system/API ownership | shared vocabulary for `system`, BUC/shared API file for `api`, then `contains(System, Api)` |
| New Business-BUC context | shared vocabulary for `location` / `timing` / `medium`, then `belongs(...).when(...).where(...).by(...)` |
| New permission or medium constraint | shared `permission` / `medium` vocabulary, then `has_permission`, `requires_permission`, or `requires_medium` in the owning BUC file |
| New event-started BUC | target BUC file, then `triggers(Event, Buc)`; add `triggers(Event, EntryUC)` only after the entry UC is known |
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
| Lifecycle reaches plausible patterns | Tech-enforced rules | business rules | forbidden and required state combinations |

### Step 3 — Apply the minimal diff

Add only what is needed. Do not remove or rename existing predicates unless the requirement explicitly says to delete functionality.

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
relate(<Parent>, <NewEntity>, "1:N")
```

Then in the BUC file add CRUD predicates for the use cases that touch it.

**Adding or changing API/system boundaries:**
```
system <System> "<system label>"
api <Api> "<API label>"

contains(<System>, <Api>)
invokes(<UC>, <Api>)
updates(<Api>, <Entity>)
sets(<UC>, <Entity>, "column", "value")
```

System entity sets are derived from API CRUD. Do not invent direct system-entity
ownership predicates.

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
`rdra-ish csv src/ --kind actor-permission-audit`.

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

**Removing a use case:**
1. Delete the `usecase` declaration
2. Delete `contains(Buc, <UC>)`
3. Delete all CRUD, `displays`, `raises`, `sets` predicates referencing it
4. If the UC was also referenced by `triggers` or `performs` elsewhere, update those too

### Step 4 — Consistency checks after the change

- Every new `usecase` has a `contains` predicate
- Every new `usecase` has at least one CRUD predicate
- Every new `entity` column that can change has a `sets` or participates in `transitions`
- No dangling imports (if you removed the last use of a symbol, remove its `import`)
- No manually added FK columns for a `relate`-covered relationship
- Every API used as a system boundary has `contains(System, Api)`
- Every new `permission` used by an actor or operation is declared once in shared vocabulary unless intentionally local
- Every `requires_medium` references a declared `medium`, and screen constraints are checked with `rdra-ish csv src/ --kind screen-constraints`
- Actor grants are checked with `rdra-ish csv src/ --kind actor-permission-audit`; review both `missing` and `excess`
- Every cross-system `relate` has a `coordinates(UseCase, Entity, Entity)` when a use case handles the consistency
- Every `coordinates` use case invokes APIs on both system sides
- The diff does not jump more than one abstraction level unless the user supplied the
  missing information explicitly
- New shared files follow path/module correspondence, e.g.
  `shared/lifecycle/order.rdra` with `module shared.lifecycle.order`

### Step 5 — Validate

```
rdra-ish check src/
```

Fix every reported error before declaring the update done.
