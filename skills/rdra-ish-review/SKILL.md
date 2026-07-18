---
name: rdra-ish-review
description: Review RDRA DSL files for syntax errors, semantic inconsistencies, missing relationships, staged refinement gaps, business flow coverage, requirement/NFR traceability, screen field mappings, API contracts, exports, ADR links, and lint readiness
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

1.5. **Run `rdra-ish lint <src-dir>/ --format table`** for review readiness.
   Treat orphan nodes, unused elements, ownership drift, flow coverage gaps,
   incomplete API method/path contracts, unmapped screen fields, and naming findings
   as staged review signals.

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
   - BUC skeleton: actors/use cases/business flow predicates, little or no CRUD
   - Data touchpoints: CRUD exists, entity structure is still coarse
   - Interaction boundary: screens/API are being modeled
   - Entity structure: columns/relationships/cardinality are modeled
   - Lifecycle: states/events/transitions/sets are modeled
   - Business rules: entity and cross-entity constraints are modeled
   - Concern shift: Scope and BUC skeleton are business-facing; interaction boundary
     and deeper stages are technical commitments derived from that business model

4. **Check actor and BUC coverage**
   - Every BUC has at least one `performs` predicate (a BUC with no actor is orphaned)
   - Every BUC has a `belongs` predicate
   - Every `extsystem` is referenced by at least one `uses` predicate

4.5. **Check requirements, flow, and decision traceability**
   - Stable requirement metadata should include priority/source/stakeholder/owner
     when the source material provides it
   - BUCs motivated by explicit requirements should use `motivates(Requirement, Buc)`
   - Ordered, branching, exceptional, or repeated business behavior should use
     `flow`, `step`, `precedes`, `branches`, `excepts`, `repeats`, and `covers`
   - ADRs should be linked with `decides(Adr, Target)` only when a real design
     decision affects the target

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
   - System entity sets are derived from API CRUD. Explicit `owns(System, Entity)` is
     allowed for intended future ownership; review `OwnedEntityWithoutApiOperation`,
     `EntityOwnedByMultipleSystems`, and `ApiOperatesEntityOutsideOwner`
   - APIs with method/path metadata should have `request`, `response`, or
     `error_response` DTOs when payload review or OpenAPI export is in scope
   - A `relate` edge crossing two derived system entity sets needs `coordinates(UseCase, Entity, Entity)`
   - The coordinating use case must invoke APIs on both system sides, and each API must operate the corresponding entity
   - Treat `CrossSystemEntityRelation`, `CoordinationMissingApi`, and `CoordinationNotCrossSystem` warnings as design review findings

8. **Check entity consistency**
   - `relate` is defined in the correct direction with the right cardinality
   - No manually declared FK columns that duplicate a `relate`-generated FK
   - Parent entities are declared before child entities in the same file
   - Index/unique/check/FK optionality/on-delete/on-update/soft-delete/history/tenant
     annotations reflect real reviewable constraints
   - Conceptual/domain objects use `maps_to` only when the logical data mapping is
     intentional

8.5. **Check screen fields and non-functional constraints**
   - Every `field` is contained by a `screen`; actor-entered fields should use
     `source actor`, system-derived fields should use `source system`
   - `maps_field(Field, Entity, "column")` should reference a real column unless the
     unmapped field is intentionally external or derived
   - `nfr`, `quality`, and `constraint` elements should be scoped with `applies_to`,
     `qualifies`, or `constrains`, especially for performance, availability, SLO,
     audit/logging, retention, and privacy

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

11. **Check business rules**
   - Single-entity invalid combinations use `forbidden(Entity, ...)`
   - Mutually exclusive facts use `exclusive(Entity, ...)`
   - Single-entity required co-occurrences use `invariant(Entity).when(...).then(...)`
   - Always-required facts use `required(Entity, ...)` only when the fact is truly global
   - Rules that mention multiple entities use multi-entity `forbidden` or `invariant`
   - Multi-entity conditions qualify columns as `Entity.column`
   - Relation-scoped rules use `.along(EntityA, EntityB, ...)` only when the listed
     entities are connected by a declared `relate` path
   - Review `CrossForbiddenViolated`, `CrossInvariantViolated`, and
     `CrossConstraintNotEvaluated` diagnostics from `states`; the last one means a
     rule condition is outside the abstract state space, exceeded the cross-product cap,
     or uses relation-scoped `.along(...)` linked-instance semantics. For Int/`now`,
     temporal `property`, `after.assert`, and multi-instance `.along` that TLC can
     check, also run `rdra-ish export --kind tla` (and `verify --backend tlc` when TLC
     is available). Diagnostic ids may still say `Cross*` even though surface syntax is
     multi-entity `forbidden` / `invariant`.

12. **Check imports**
   - Every referenced symbol has a corresponding `import`
   - No imported modules that are never used

13. **Check generated artifacts when the model claims a contract**
   - OpenAPI: `rdra-ish export <src-dir>/ --kind openapi --out /tmp/openapi.json`
     when API method/path and DTOs changed
   - AsyncAPI: export when events or event-started BUCs changed
   - DBML/JSON Schema: run the corresponding export when data model or DTO shape changed
   - TLA+: `rdra-ish export <src-dir>/ --kind tla -o /tmp/` when Int/`now`, multi-entity
     rules, quantifiers, `property`, or `after.assert` are part of the change (writes
     both `.tla` and `.cfg`)
   - Diagram snapshots: run the sample artifact script when docs/golden outputs are
     part of the change

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
- api `<Id>`: method/path exists but request/response DTOs are missing
- field `<Id>`: actor-entered but not mapped to data or justified
- nfr `<Id>`: no target scope
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
| `contains` | Buc / Flow / Screen | Flow / Step / Field | — |
| `coordinates` | UseCase | Entity | Entity |
| `owns` | System | Entity | — |
| `belongs` | Buc | Business | — |
| `motivates` | Requirement | Buc | — |
| `request` / `response` / `error_response` | Api | Dto | — |
| `maps_field` | Field | Entity | column string |
| `precedes` / `branches` / `excepts` / `repeats` | Step | Step | — |
| `covers` | Step | UseCase / Api / Event | — |
| `compensates` | UseCase | UseCase / Event | — |
| `applies_to` / `qualifies` / `constrains` | Nfr / Quality / Constraint | target | — |
| `maps_to` | Concept / DomainObject / Aggregate / ValueObject | Entity | — |
| `decides` | Adr | target | — |
| `has_permission` | Actor | Permission | — |
| `requires_permission` | UseCase / Api | Permission | — |
| `requires_medium` | UseCase / Api | Medium | — |
| `relate` | Entity | Entity | cardinality (`1:1` / `1:N` / `N:1` / `N:M`, unquoted preferred) |
| `transitions` | Entity.col | Event | from -> to |
| `sets` | UseCase / Event | Entity | `col == val` or comparison + bool |
| `forbidden` | Entity | condition(s) | — |
| `invariant` | Entity | `.when(...)` / `.then(...)` chains | — |
| `required` | Entity | condition(s) | — |
| `exclusive` | Entity | condition(s) | — |
| `forbidden` (multi-entity) | Entity... | cross-entity condition(s) | optional `.along(...)` |
| `invariant` (multi-entity) | Entity... | `.when(...)` / `.then(...)` chains | optional `.along(...)` |
| `when` | conditions... | `.none` / `.has` conditions | — |
| `property` | Name | optional label | `always` / `eventually` / `leads_to` |
| `after` | UseCase | `.assert(...)` | — |
