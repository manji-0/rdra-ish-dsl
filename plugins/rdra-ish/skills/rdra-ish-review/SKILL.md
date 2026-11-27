---
name: rdra-ish-review
description: Review RDRA DSL files for syntax errors, semantic inconsistencies, missing relationships, staged refinement gaps, business flow coverage, requirement/NFR traceability, screen field mappings, API contracts, exports, ADR links, and lint readiness
license: MIT
---

## When to use

Give a verdict on a model or a change to it: a list of findings with severity, each with
its source, and what to fix. To answer one narrow question (stage readiness, access, API
boundaries, states), use skill `rdra-ish-buc-analyze`.

## Steps

1. Run `rdra-ish check <src-dir>/`. Fix load and type errors (`error: path:line:col: ...`)
   before the semantic review. A lifecycle finding prints its name:
   `error[<Name>]` or `warning[<Name>]`. Paste the `bounds:` line and the last line
   exactly as printed.
2. Run `rdra-ish lint <src-dir>/ --format table` for review readiness.
3. Find the stage with `rdra-ish next <src-dir>/`. Report a gap the next stage fills as a
   question, not a defect; early stages protect business intent, later ones check
   boundaries, data, lifecycle, and rules.
4. Walk `references/checklist.md`, area by area.
5. When the change claims a contract, run its export, `rdra-ish verify` for properties and
   two-row rules, and `rdra-ish check --code` when the model has a `[code.<lang>]` table.
6. Write the findings in the shape of `references/output-format.md`. Mark each finding
   with the command and diagnostic that printed it, or `(review)` when you concluded it
   yourself.

## References

- `references/checklist.md`: what to look for in each area, with links to the analysis
  references that explain each diagnostic.
- `references/output-format.md`: the template, severities, and evidence rules.
- `../rdra-ish-write/references/syntax.md`: predicate signatures.
