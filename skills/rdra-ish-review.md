---
name: rdra-review
description: Review RDRA DSL files for syntax errors, semantic inconsistencies, and missing relationships
---

## Review RDRA DSL

Review existing RDRA DSL files for syntax correctness, semantic consistency, and coverage completeness.

### Steps

1. **Run `rdra-ish check <src-dir>/`** to surface syntax and type errors first.  
   Fix all reported errors before proceeding to semantic review.

2. **Check file layout**
   - `shared/actors.rdra`, `shared/biz.rdra`, `shared/entities.rdra` are separated
   - One file per BUC under `buc/`
   - Every file has a `module` declaration whose dotted name matches the file path

3. **Check actor and BUC coverage**
   - Every BUC has at least one `performs` predicate (a BUC with no actor is orphaned)
   - Every BUC has a `belongs` predicate
   - Every `extsystem` is referenced by at least one `uses` predicate

4. **Check use case coverage**
   - Every `usecase` is referenced by a `contains` predicate
   - Every `usecase` has at least one `displays` predicate (flag missing ones for intentional review)
   - Every `usecase` has at least one CRUD predicate (`reads` / `writes` / `creates` / `updates` / `deletes`)

5. **Check entity consistency**
   - `relate` is defined in the correct direction with the right cardinality
   - No manually declared FK columns that duplicate a `relate`-generated FK
   - Parent entities are declared before child entities in the same file

6. **Check events and state transitions**
   - Every `event` is referenced by at least one `raises` or `triggers` predicate
   - Every `state` referenced in `transitions` is declared
   - No unreachable states (states never appearing as the `to` argument of any `transitions`)

7. **Check `sets` predicate coverage**
   - Every `Enum` column that lacks a `transitions` predicate should have a `sets` for each use case that modifies it
   - Every `@null` column updated by a use case should have a `sets` with `"present"` / `"null"` / a PostgreSQL type
   - Every `Bool` column toggled by a use case should have a `sets` with `"true"` or `"false"`

8. **Check imports**
   - Every referenced symbol has a corresponding `import`
   - No imported modules that are never used

### Output format

```
## Syntax errors
- (none — or paste rdra-ish check output)

## Semantic issues
- [high] <file>:<line> — <description>
- [medium] ...
- [low] ...

## Coverage gaps
- BUC `<Id>`: no performs defined
- usecase `<Id>`: not contained in any BUC
- entity `<Id>` column `<col>`: modified by `<UC>` but no sets predicate
- ...

## Suggestions
- (optional: proposals to make the model more accurate or complete)
```

### Predicate signatures quick reference

| predicate | arg1 | arg2 | arg3 |
|-----------|------|------|------|
| `performs` | Actor | UseCase / Buc | — |
| `uses` | Actor | ExtSystem | — |
| `reads/writes/creates/updates/deletes` | UseCase | Entity | — |
| `displays` | UseCase | Screen | — |
| `shows` | Screen | Entity | — |
| `raises` | UseCase | Event | — |
| `triggers` | Event | UseCase | — |
| `contains` | Buc | UseCase | — |
| `belongs` | Buc | Business | — |
| `motivates` | Requirement | Buc | — |
| `relate` | Entity | Entity | cardinality string (`"1:1"` / `"1:N"` / `"N:1"` / `"N:M"`) |
| `transitions` | Event | State (from) | State (to) |
| `sets` | UseCase / Event | Entity | column name string | value string |
