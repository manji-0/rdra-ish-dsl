---
name: rdra-ish-review
description: Review RDRA DSL files for syntax errors, semantic inconsistencies, missing relationships, and staged refinement gaps
---

## Review RDRA DSL

Review existing RDRA DSL files for syntax correctness, semantic consistency, and coverage completeness.

Judge gaps relative to the model's current abstraction stage. A missing `screen`,
column, API, or lifecycle may be acceptable in an early-stage BUC, but it should be
reported as the next refinement question rather than silently ignored.
Read the stages as a progression from business concerns to technical concerns. Early
review should protect business intent and value coverage; later review should focus on
API/system boundaries, persistence structure, reachable lifecycle states, and rules.

### Steps

1. **Run `rdra-ish check <src-dir>/`** to surface syntax and type errors first.
   Fix all reported errors before proceeding to semantic review.

2. **Check file layout**
   - Small models start with `shared/actors.rdra`, `shared/biz.rdra`,
     `shared/entities.rdra`, and one file per BUC under `buc/`
   - Split shared files only when reviewability or ownership requires it
   - If split, shared files are grouped by responsibility, e.g.
     `shared/entities/order.rdra`, `shared/lifecycle/order.rdra`, `shared/rules.rdra`
   - BUC-specific predicates stay in `buc/buc_<name>.rdra`, not in shared files
   - Every file has a `module` declaration whose dotted name matches the file path

3. **Classify refinement stage**
   - Scope: BUC/business declarations only
   - BUC skeleton: actors/use cases/predicates, little or no CRUD
   - Data touchpoints: CRUD exists, entity structure is still coarse
   - Interaction boundary: screens/API are being modeled
   - Entity structure: columns/relationships/cardinality are modeled
   - Lifecycle: states/events/transitions/sets are modeled
   - Business rules: forbidden/invariant constraints are modeled
   - Concern shift: Scope and BUC skeleton are business-facing; interaction boundary
     and deeper stages are technical commitments derived from that business model

4. **Check actor and BUC coverage**
   - Every BUC has at least one `performs` predicate (a BUC with no actor is orphaned)
   - Every BUC has a `belongs` predicate
   - Every `extsystem` is referenced by at least one `uses` predicate

5. **Check use case coverage**
   - Every `usecase` is referenced by a `contains` predicate
   - Every `usecase` has at least one `displays` predicate (flag missing ones for intentional review)
   - Every data-changing `usecase` either has early-stage direct CRUD or invokes an API with CRUD
   - If the use case declares `sets`, verify it has direct CRUD or invokes an API operating that entity

6. **Check access and context constraints**
   - Actor authority is declared with `permission` and attached with `has_permission(Actor, Permission)`
   - UC/API requirements use `requires_permission(UseCase|Api, Permission)` and
     `requires_medium(UseCase|Api, Medium)`; do not model these as free-form notes
   - If `belongs(Buc, Business)` has contextual `.when(...)`, `.where(...)`, or `.by(...)`
     clauses, typed references should point to `timing`, `location`, and `medium`
   - Run `rdra-ish csv <src-dir>/ --kind screen-constraints` when reviewing UI access:
     screen patterns are derived from `displays` plus invoked API constraints
   - Run `rdra-ish csv <src-dir>/ --kind actor-permission-audit` when reviewing actor
     grants: `missing` means a required UC/API permission is not assigned to that actor,
     and `excess` means the actor has a permission no modeled performer path currently
     requires
   - Flag a screen as under-specified when a constrained UC/API path reaches it but the
     actor permission model or required medium is missing from the same review slice

7. **Check API/system boundaries**
   - Every declared `api` is invoked by at least one use case unless it is intentionally future-facing
   - Every stable API belongs to one system via `contains(System, Api)`
   - System entity sets are derived from API CRUD; do not look for or add direct system→entity ownership
   - A `relate` edge crossing two derived system entity sets needs `coordinates(UseCase, Entity, Entity)`
   - The coordinating use case must invoke APIs on both system sides, and each API must operate the corresponding entity
   - Treat `CrossSystemEntityRelation`, `CoordinationMissingApi`, and `CoordinationNotCrossSystem` warnings as design review findings

8. **Check entity consistency**
   - `relate` is defined in the correct direction with the right cardinality
   - No manually declared FK columns that duplicate a `relate`-generated FK
   - Parent entities are declared before child entities in the same file

9. **Check events and state transitions**
   - Run `rdra-ish diagram <src-dir> --kind event-flow --format mermaid` and review the
     warnings printed to stderr before looking at the diagram itself:
     - `EventNeverRaised`: the event has no `raises` predicate — add one or remove the event
     - `EventNeverConsumed`: the event is raised but drives no `transitions` and `triggers`
       nothing — add the missing predicate or investigate a modelling gap
     - `TriggeredUseCaseUnreachable`: the triggered UC belongs to no BUC — add a `contains`
   - Every `state` referenced in `transitions` is declared
   - Review unreachable enum variants or state-pattern warnings from `rdra-ish check`
     and `rdra-ish states`; a state can still be valid as an initial state even if it
     never appears as a transition target
   - Prefer `triggers(Event, Buc)` when the event starts a downstream BUC boundary.
     Add `triggers(Event, UseCase)` as the concrete entry refinement when the entry UC is known
   - When `triggers(Event, UseCase)` is used, verify the triggered UC is `contains`-ed in
     the correct downstream BUC and that the cross-BUC flow is intentional

10. **Check `sets` predicate coverage**
   - Every `Enum` column that lacks a `transitions` predicate should have a `sets` for each use case that modifies it
   - Every `@null` column updated by a use case should have a `sets` with `"present"` / `"null"` / a PostgreSQL type
   - Every `Bool` column toggled by a use case should have a `sets` with `"true"` or `"false"`

11. **Check imports**
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

## Refinement stage
- Current stage: <stage>
- Appropriate omissions: <gaps acceptable at this stage>
- Next questions: <information needed to move one stage deeper>

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
| `reads/writes/creates/updates/deletes` | Api | Entity | — |
| `displays` | UseCase | Screen | — |
| `shows` | Screen | Entity | — |
| `raises` | UseCase | Event | — |
| `triggers` | Event | UseCase / Buc | — |
| `contains` | Buc | UseCase | — |
| `contains` | System | Api | — |
| `coordinates` | UseCase | Entity | Entity |
| `belongs` | Buc | Business | — |
| `motivates` | Requirement | Buc | — |
| `has_permission` | Actor | Permission | — |
| `requires_permission` | UseCase / Api | Permission | — |
| `requires_medium` | UseCase / Api | Medium | — |
| `relate` | Entity | Entity | cardinality string (`"1:1"` / `"1:N"` / `"N:1"` / `"N:M"`) |
| `transitions` | Event | State (from) | State (to) |
| `sets` | UseCase / Event | Entity | column name string | value string |
| `sets` | UseCase / Event | Entity | comparison expression | boolean literal |
