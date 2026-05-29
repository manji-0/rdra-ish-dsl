---
name: rdra-buc-update
description: Update an existing BUC by adding or modifying use cases, screens, events, entities, and predicates
---

## Update an existing BUC

Given a description of what to add or change, modify the relevant `.rdra` files while keeping the model consistent.

### Step 1 — Read the existing BUC

Read `buc/buc_<name>.rdra` and the shared files it imports. Identify:
- Which use cases already exist
- Which entities are already referenced
- What screens and events are already declared
- Which predicates are already defined

### Step 2 — Classify the change

| Change type | Where to edit |
|-------------|---------------|
| New use case in existing BUC | `buc/buc_<name>.rdra` |
| New screen for an existing UC | `buc/buc_<name>.rdra` |
| New entity column | `shared/entities.rdra` |
| New entity entirely | `shared/entities.rdra` + add `relate` if needed |
| New event or state | `shared/entities.rdra` (if cross-BUC) or `buc/buc_<name>.rdra` |
| New actor | `shared/actors.rdra` |
| Remove a use case | Remove its `contains`, CRUD, `displays`, `raises` predicates; check no other BUC uses it |

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

### Step 5 — Validate

```
rdra-ish check src/
```

Fix every reported error before declaring the update done.
