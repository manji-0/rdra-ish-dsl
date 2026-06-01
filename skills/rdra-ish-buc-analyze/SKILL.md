---
name: rdra-ish-buc-analyze
description: Choose the right RDRA-ish BUC or whole-model analysis path for refinement readiness, coverage gaps, access, system boundaries, and state patterns
---

## Choose BUC Analysis

Choose the analysis path that matches the user's question and the model's current
refinement stage. Keep this top-level skill as the routing layer, then load the
reference file for the concrete analysis task.

<!-- derived-from ../../docs/language-reference.md#cross-entity-constraints -->
<!-- derived-from ../../docs/state-derivation.md#constraint-checking-after-bfs -->

### Routing Guide

| User intent | Analyze first | Load |
|---|---|---|
| "Where are we in refinement?" | Stage signals and next missing information | `references/stage-readiness.md` |
| "What is incomplete in this BUC/model?" | BUC, actor, use-case, CRUD, inferred actor inputs, and layout gaps | `references/coverage-gaps.md` |
| "Are permissions/media correct?" | screen/access paths, permission callables, actor grants | `references/access-permissions.md` |
| "Are API/system boundaries sound?" | API ownership, API CRUD, cross-system relations, coordination | `references/api-system-boundaries.md` |
| "Are lifecycle states/rules valid?" | reachable state patterns, terminals, truncation, per-entity rule diagnostics, and cross-entity rule inventory | `references/state-patterns.md` |
| "Summarize findings for review" | severity, evidence, next questions, and concrete recommendations | `references/reporting.md` |

### Default Workflow

1. Run `rdra-ish check <src-dir>/` first unless the user only wants a narrow command
   explanation.
2. Identify whether the model is still business-facing or has entered technical
   commitment: data touchpoints, interaction boundary, entity structure, lifecycle, or
   rules.
3. Load one reference file from the routing guide and run only the commands needed for
   that analysis.
4. Report evidence with command output signals, not just intuition.
5. End with the next useful refinement question or the smallest concrete model change.

### Quick Command Palette

```sh
rdra-ish check src/
rdra-ish list src/ --kind buc --format table
rdra-ish list src/ --kind usecase --format table
rdra-ish list src/ --kind actor --format table
rdra-ish csv src/ --kind matrix
rdra-ish csv src/ --kind actor-inputs
rdra-ish csv src/ --kind api-matrix
rdra-ish csv src/ --kind screen-constraints
rdra-ish csv src/ --kind permission-callables
rdra-ish csv src/ --kind actor-permission-audit
rdra-ish states src/
rdra-ish states src/ --buc <BucId>
rdra-ish states src/ --entity <EntityId>
```

### Reference Files

- `references/stage-readiness.md` — classify the current abstraction stage and decide
  what information should be requested next.
- `references/coverage-gaps.md` — find missing BUC ownership, actor/use-case links,
  CRUD touchpoints, layout issues, and import hygiene.
- `references/access-permissions.md` — analyze permission/media requirements and actor
  grant coverage with the three access CSVs.
- `references/api-system-boundaries.md` — review API invocation, system ownership,
  API/entity matrices, and cross-system coordination.
- `references/state-patterns.md` — interpret `states`, lifecycle warnings,
  terminal/unreachable patterns, truncation, and rule diagnostics.
- `references/reporting.md` — structure analysis output with severity, evidence, and
  recommendations.
