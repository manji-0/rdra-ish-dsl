---
name: rdra-ish-review
description: Review RDRA DSL files for syntax errors, semantic inconsistencies, missing relationships, staged refinement gaps, business flow coverage, requirement/NFR traceability, screen field mappings, API contracts, exports, ADR links, and lint readiness
license: MIT
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
   Fix all reported errors before proceeding to semantic review. A load or consistency
   diagnostic prints as `error: path:line:col: ...` or `warning: ...`; a lifecycle
   finding carries its stable name, `error[<Name>]: ...` or `warning[<Name>]: ...`. The
   first line, `bounds:`, names the bounds every lifecycle verdict holds under, and a
   clean run ends with `OK: no errors within 1 instance per entity, Int 0..5`.

2. **Run `rdra-ish lint <src-dir>/ --format table`** for review readiness.
   Treat orphan nodes, unused elements, ownership drift, flow coverage gaps,
   incomplete API method/path contracts, unmapped screen fields, naming findings, and
   the `rdra-*` rules (an actor, external system, entity, condition, variation, or timer
   that no BUC reaches, or a transitioning use case outside every BUC) as staged review
   signals.

3. **Check file layout**
   - Small models start with `shared/actors.rdra`, `shared/biz.rdra`,
     `shared/entities.rdra`, and one file per BUC under `buc/`
   - Split shared files only when reviewability or ownership requires it
   - If split, shared files are grouped by responsibility, e.g.
     `shared/entities/order.rdra`, `shared/lifecycle/order.rdra`, `shared/rules.rdra`
   - BUC-specific predicates stay in `buc/buc_<name>.rdra`, not in shared files
   - Every file has a `module` declaration whose dotted name matches the file path

4. **Classify refinement stage**
   - Scope: BUC/business declarations only
   - BUC skeleton: actors/use cases/business flow predicates, little or no CRUD
   - Data touchpoints: CRUD exists, entity structure is still coarse
   - Interaction boundary: screens/API are being modeled
   - Entity structure: columns/variations/relationships/cardinality are modeled
   - Lifecycle: transitions/sets/events/timers/conditions are modeled
   - Business rules: entity and cross-entity constraints are modeled
   - Concern shift: Scope and BUC skeleton are business-facing; interaction boundary
     and deeper stages are technical commitments derived from that business model

5. **Check actor and BUC coverage**
   - Every use case has a performer, directly or through `performs(Actor, Buc)`, or is
     started by an event or timer
   - Every `performs(Actor, Buc)` is meant for every use case of the BUC: a use case's
     actors are its direct performers plus the performers of each BUC that contains it,
     and each of them must hold the use case's permissions
   - Every BUC has a `belongs` predicate
   - Every `extsystem` is connected through an event: `raises(ExtSystem, Event)` or `triggers(Event, ExtSystem)`
   - Every `timer` triggers a use case or BUC: `triggers(Timer, UseCase|Buc)`
   - Every `condition` is applied to a use case (`applies_to(Condition, UseCase)`) and either `combines` a variation or lifecycle column or has a `holds` body that names one (`lint` `condition-no-rdra-anchor`)

6. **Check requirements, flow, and decision traceability**
   - Stable requirement metadata should include priority/source/stakeholder/owner
     when the source material provides it
   - BUCs motivated by explicit requirements should use `motivates(Requirement, Buc)`
   - Ordered, branching, exceptional, or repeated business behavior should use
     `flow`, `step`, `precedes`, `branches`, `excepts`, `repeats`, and `covers`
   - ADRs should be linked with `decides(Adr, Target)` only when a real design
     decision affects the target

7. **Check use case coverage**
   - Every `usecase` is referenced by a `contains` predicate
   - Every `usecase` has at least one `displays` predicate (flag missing ones for intentional review)
   - Every data-changing `usecase` either has early-stage direct CRUD or invokes an API with CRUD
   - `precondition`, `guard`, and `postcondition` clauses are review metadata that no
     check evaluates; a machine-checked postcondition is `after(UseCase).assert(...)`

8. **Check access and context constraints**
   - Actor authority is declared with `permission` and attached with `has_permission(Actor, Permission)`
   - UC/API requirements use `requires_permission(UseCase|Api, Permission)` and
     `requires_medium(UseCase|Api, Medium)`; do not model these as free-form notes
   - If `belongs(Buc, Business)` has contextual `.by(...)` clauses, typed
     references should point to `medium`
   - Run `rdra-ish csv <src-dir>/ --kind screen-constraints` when reviewing UI access:
     screen patterns are derived from `displays` plus invoked API constraints
   - Run `rdra-ish csv <src-dir>/ --kind actor-permission-audit` when reviewing actor
     grants: `missing` means a required UC/API permission is not assigned to that actor,
     and `excess` means the actor has a permission no modeled performer path currently
     requires. `rdra-ish check <src-dir>/ --strict` turns each gap into an error
   - Flag a screen as under-specified when a constrained UC/API path reaches it but the
     actor permission model or required medium is missing from the same review slice

9. **Check API/system boundaries**
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

10. **Check entity consistency**
    - `relate` is defined in the correct direction with the right cardinality
    - No manually declared FK columns that duplicate a `relate`-generated FK (`FkConflict`
      unless adopted with `.key(...)`); two relates from one child to one parent carry
      distinct `.role(...)`s (`DuplicateRelateRole`)
    - A parent that must have a child uses `1..*:1`, checked as `MinCard_<Child>_<role>`;
      `MinCardNeedsCreate` names each use case that creates such a parent without a child.
      A `1:1` is checked as `Unique_<Child>_<role>`, which only `verify` can break
    - Index/unique/check/FK optionality/on-delete/on-update/soft-delete/history/tenant
      annotations reflect real reviewable constraints; each `@check` is the invariant
      `Check_<Entity>_<column>_<n>`
    - A column whose value an actor chooses is typed by a `variation`, not an inline `Enum`

11. **Check screen fields and non-functional constraints**
    - Every `field` is contained by a `screen`; actor-entered fields should use
      `source actor`, system-derived fields should use `source system`
    - `maps_field(Field, Entity, "column")` should reference a real column unless the
      unmapped field is intentionally external or derived
    - `nfr`, `quality`, and `constraint` elements should be scoped with `applies_to`,
      `qualifies`, or `constrains`, especially for performance, availability, SLO,
      audit/logging, retention, and privacy

12. **Check events and state transitions**
    - Run `rdra-ish diagram <src-dir> --kind event-flow --format mermaid` and review the
      warnings printed to stderr before looking at the diagram itself:
      - `EventNeverRaised`: the event has no `raises` predicate — add one or remove the event
      - `EventNeverConsumed`: the event is raised but `triggers` nothing and is not an
        `outbox` publication — add the consumer, or delete the event and its `raises`
      - `TriggeredUseCaseUnreachable`: the triggered UC belongs to no BUC — add a `contains`
    - Every `transitions` names the use case that drives the edge, and every `from -> to`
      variant is a variant of the Enum column
    - Review the state-model findings of `rdra-ish check`:
      - `error[Witness_<E>_<col>_<value>]`: no sequence of use cases reaches the state
      - `error[Deadlock_<E>_<col>_<value>]`: an instance reaches a state that has a way
        out and is never moved off it (a `deadlock at the bound` warning when only a free
        instance slot is missing)
      - `warning[Runs_<UseCase>]`: the use case never runs, with the guard or instance
        that blocks it
      - `warning[UnreachableEnumVariant_...]`: a declared variant that is no state of the
        state model and that nothing reaches
    - Prefer `triggers(Event, Buc)` when the event starts a downstream BUC boundary.
      Add `triggers(Event, UseCase)` as the concrete entry refinement when the entry UC is known
    - When `triggers(Event, UseCase)` is used, verify the triggered UC is `contains`-ed in
      the correct downstream BUC and that the cross-BUC flow is intentional

13. **Check `sets` predicate coverage**
    - Every inline `Enum` column that lacks a `transitions` predicate should have a `sets`
      for each use case that modifies it; an Enum that nothing moves is an attribute the
      actor chooses, so type it by a `variation` (`lint` `enum-attribute`)
    - Every `@null` column updated by a use case should have a `sets` with `present` or `null`
    - Every `Bool` column toggled by a use case should have a `sets` with `true` or `false`
    - The values are bare words; a quoted value or a PostgreSQL type name is rejected

14. **Check business rules**
    - Single-entity invalid combinations use `forbidden(Entity, ...)`
    - Mutually exclusive facts use `exclusive(Entity, ...)`
    - Single-entity required co-occurrences use `invariant(Entity).when(...).then(...)`
    - Always-required facts use `required(Entity, ...)` only when the fact is truly global
    - Rules that mention multiple entities use multi-entity `forbidden` or `invariant`
    - Multi-entity conditions qualify columns as `Entity.column`
    - Relation-scoped rules use `.along(EntityA, EntityB, ...)` only when the listed
      entities are connected by a declared `relate` path (`.by(role)` when two keys join
      a pair)
    - Review each broken rule `check` reports: `error[<rule name>]` (such as
      `CrossForbidden_Order_0`) with the rule's site and a shortest counterexample of use
      case steps. An `UnbindableScope` error means an `.along(...)` path that no `relate`
      binds. For temporal `property` and rules that need two instances, also run
      `rdra-ish verify <src-dir>/` (skill `rdra-ish-verify`) when TLC is available.
      Constraint names may still say `Cross*` even though surface syntax is multi-entity
      `forbidden` / `invariant`.

15. **Check imports**
    - Every referenced symbol has a corresponding `import`
    - No imported modules that are never used

16. **Check generated artifacts when the model claims a contract**
    - OpenAPI: `rdra-ish export <src-dir>/ --kind openapi --out /tmp/openapi.json`
      when API method/path and DTOs changed
    - AsyncAPI: export when events or event-started BUCs changed
    - DBML/JSON Schema: run the corresponding export when data model or DTO shape changed
    - Formal verification: `rdra-ish verify <src-dir>/` (skill `rdra-ish-verify`) when
      Int/`now`, multi-entity rules, quantifiers, `property`, or `after.assert` are part
      of the change
    - Diagram snapshots: run the sample artifact script when docs/golden outputs are
      part of the change

### Output format

```text
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
- usecase `<Id>`: no performer, directly or through its BUC
- usecase `<Id>`: not contained in any BUC
- entity `<Id>` column `<col>`: modified by `<UC>` but no sets predicate
- api `<Id>`: method/path exists but request/response DTOs are missing
- field `<Id>`: actor-entered but not mapped to data or justified
- nfr `<Id>`: no target scope
- ...

## Lifecycle findings
- `error[<Name>]` / `warning[<Name>]`: <what it means and the fix>, under `bounds: ...`

## Suggestions
- (optional: proposals to make the model more accurate or complete)
```

### Predicate signatures quick reference

| predicate | arg1 | arg2 | arg3 |
|-----------|------|------|------|
| `performs` | Actor | UseCase / Buc / Step | — |
| `reads/writes/creates/updates/deletes` | UseCase / Api | Entity | — |
| `displays` | UseCase | Screen | — |
| `shows` | Screen | Entity | — |
| `raises` | UseCase / ExtSystem | Event | — |
| `triggers` | Event / Timer | UseCase / Buc / ExtSystem (not from a Timer) | — |
| `outbox` | Event | — | — |
| `contains` | Buc | UseCase / Flow | — |
| `contains` | Flow / Screen / System | Step / Field / Api | — |
| `coordinates` | UseCase | Entity | Entity |
| `owns` | System | Entity | — |
| `belongs` | Buc | Business | — (chain `.by(Medium)`) |
| `motivates` | Requirement | Buc | — |
| `request` / `response` / `error_response` | Api | Dto | — |
| `maps_field` | Field | Entity | column string |
| `precedes` / `branches` / `excepts` / `repeats` | Step | Step | — |
| `covers` | Step | UseCase / Api / Event / Timer | — |
| `compensates` | UseCase | UseCase | — |
| `applies_to` | Nfr / Condition | UseCase / Api / System | — |
| `qualifies` | Nfr / Constraint | Quality | — |
| `constrains` | Constraint | UseCase / Api / System / Entity / Dto | — |
| `combines` | Condition | Variation / Entity.column | — |
| `holds` | Condition | comparison(s) | — |
| `decides` | Adr | target | — |
| `has_permission` | Actor | Permission | — |
| `requires_permission` | UseCase / Api | Permission | — |
| `requires_medium` | UseCase / Api | Medium | — |
| `relate` | Entity | Entity | cardinality (`1:1` / `1:N` / `N:1` / `N:M`, child side may be a range such as `1..*`); `N:M` requires `.via(Junction)`; chain `.optional()`, `.role(x)`, `.key(cols)` |
| `transitions` | Entity.col | UseCase | from -> to |
| `sets` | UseCase | Entity | `col == val` or comparison + bool |
| `forbidden` | Entity | condition(s) | — |
| `invariant` | Entity | `.when(...)` / `.then(...)` chains | — |
| `required` | Entity | condition(s) | — |
| `exclusive` | Entity | condition(s) | — |
| `forbidden` (multi-entity) | Entity... | cross-entity condition(s) | optional `.along(...)` / `.by(role)` |
| `invariant` (multi-entity) | Entity... | `.when(...)` / `.then(...)` chains | optional `.along(...)` / `.by(role)` |
| `when` | conditions... | `.none` / `.has` conditions | — |
| `property` | Name | optional label | `always` / `eventually` / `leads_to` |
| `after` | UseCase | `.assert(...)` | — |
