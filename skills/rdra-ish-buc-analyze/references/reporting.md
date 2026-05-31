# Analysis Reporting

Use this reference when turning analysis into a review comment, design note, or
implementation guidance.

## Concept

A good RDRA-ish analysis report does three things:

1. Separates syntax/type failures from modeling gaps.
2. Judges omissions relative to the current refinement stage.
3. Gives the next useful modeling action, not a laundry list of every possible future
   detail.

## Report Template

```text
## Analysis: <BUC or whole model>

### Current refinement stage
- Stage: <scope | BUC skeleton | data touchpoints | interaction boundary | entity structure | lifecycle | business rules>
- Evidence: <commands, rows, warnings, or model signals>
- Appropriate omissions: <not required yet at this stage>
- Next information needed: <focused questions>

### Findings
- [high] <file or model area> — <blocking issue and evidence>
- [medium] <file or model area> — <meaningful modeling gap>
- [low] <file or model area> — <deferred detail or cleanup>

### State/access/boundary summary
- State patterns: <entity summaries or "not analyzed at this stage">
- Access: <screen-constraints / permission-callables / actor-permission-audit summary>
- API/system boundaries: <api-matrix / diagnostics summary>

### Recommendations
- <smallest concrete model change>
- <next question if information is missing>
```

## Severity Guide

- High: syntax/type errors, duplicate definitions, unresolved imports, orphaned BUC/UC
  that blocks the requested analysis, or state/rule violations that contradict stated
  requirements.
- Medium: missing business ownership, actor grants, API ownership, CRUD coverage, or
  lifecycle effects after the relevant stage has been reached.
- Low: early-stage omissions, future-facing APIs, intentionally deferred screens, or
  cleanup that improves reviewability without changing the model meaning.

## Evidence Rules

- Cite the command or diagnostic that produced each finding.
- Use exact ids for BUCs, use cases, APIs, entities, permissions, and states.
- If the model is not deep enough for a check, say so rather than pretending the check
  passed.
- When `states` output is truncated, mention the cap and narrow the analysis before
  drawing strong conclusions.

## Recommendation Patterns

- Missing BUC actor: add `performs(<Actor>, <BucOrUseCase>)` or ask who performs it.
- Missing business ownership: add `belongs(<Buc>, <Business>)`.
- Empty CRUD row: ask which entity the use case touches, then add the correct CRUD
  predicate.
- Missing screen/API boundary: add `screen`, `api`, `displays`, `invokes`, and API CRUD
  only if the interaction boundary is now known.
- Missing actor grant: add `has_permission(<Actor>, <Permission>)` or revise who
  performs the use case.
- Cross-system relation: add `coordinates(<UseCase>, <EntityA>, <EntityB>)` plus API
  invocations on both sides, or revise system ownership.
- Missing lifecycle effect: add `raises`, `transitions`, or `sets` depending on whether
  the change is event-driven or direct.
