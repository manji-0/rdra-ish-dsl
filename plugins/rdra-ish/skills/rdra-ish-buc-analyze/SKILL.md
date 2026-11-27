---
name: rdra-ish-buc-analyze
description: Choose the right RDRA-ish BUC or whole-model analysis path for refinement readiness, coverage gaps, business flow, requirement/NFR traceability, screen field mappings, API contracts, access, system boundaries, exports, and state patterns
license: MIT
---

## When to use

Answer one analysis question about a BUC or the whole model: where it is in refinement,
what is missing, whether access or API boundaries hold, or what its lifecycle reaches. For
a verdict over every area, use skill `rdra-ish-review`.

## Steps

1. Run `rdra-ish check <src-dir>/` first, unless the user only asks what a command does.
   Paste the `bounds:` line and the last line exactly as printed.
2. Pick the question's reference below and run only its commands.
3. Judge each gap at the model's stage. Screens and data touchpoints are business-facing.
   The API boundary, entity structure, lifecycle, and rules are technical commitments.
4. Report with `../rdra-ish-review/references/output-format.md`: each finding with the
   command and diagnostic behind it, the `bounds:` line, and everything under
   `not checked:` as not checked.
5. End with the next refinement question or the smallest model change.

| The question | Load |
|---|---|
| Where are we in refinement? What should we ask next? | `references/stage-readiness.md` |
| What is missing in this BUC or model? | `references/coverage-gaps.md` |
| Are permissions and media right? | `references/access-permissions.md` |
| Are API and system boundaries sound? | `references/api-system-boundaries.md` |
| Which states are reachable? What do `Witness_*`, `Deadlock_*`, `Runs_*` mean? | `references/state-patterns.md` |
| Do temporal properties or two-row rules hold? | skill `rdra-ish-verify` |

## References

- `references/stage-readiness.md`: stage signals and the next question.
- `references/coverage-gaps.md`: missing links between actors, BUCs, use cases, data,
  fields, and requirements.
- `references/access-permissions.md`: the three access CSVs.
- `references/api-system-boundaries.md`: API ownership and cross-system coordination.
- `references/state-patterns.md`: `states`, and every lifecycle finding of `check`.
- `../rdra-ish-review/references/output-format.md`: the template and evidence rules.
