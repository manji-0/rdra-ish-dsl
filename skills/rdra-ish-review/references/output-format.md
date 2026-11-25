# Output Format

The shape of a review or an analysis, its severities, and the evidence each finding
needs. Skills `rdra-ish-review` and `rdra-ish-buc-analyze` both use it.

## Template

```text
## Check
<the bounds: line and the last line of check, exactly as printed>

## Stage
- Stage: <stage>, from <command or signal>
- Fine at this stage: <omissions the next stages fill>
- Next questions: <what moves the model one stage deeper>

## Findings
- [high] <file>:<line> — <finding> (<source>)
- [medium] ...
- [low] ...

## Lifecycle
- `error[<Name>]` / `warning[<Name>]`: <meaning and fix>
- not checked: <what a scope or a cap left open>

## Recommendations
- <the smallest model change, or the question to ask>
```

## Severity

- **High**: a syntax or type error, a duplicate or unresolved symbol, a `module` that does
  not match its path, or a rule or state finding that contradicts a stated requirement.
- **Medium**: a gap at a stage the model has reached: a missing `belongs`, performer, CRUD,
  grant, API owner, field mapping, lifecycle effect, or unscoped NFR.
- **Low**: an omission the next stage fills, a future-facing API, or a cleanup that does
  not change meaning.

## Evidence

- Name the source of each finding. A finding a command printed names the command and the
  diagnostic (`check: error[Deadlock_Order_status_paid]`, `lint: api-contract-missing`,
  `actor-permission-audit: missing Clerk/OrderWrite via CancelOrder`). A finding you
  reached by reading the model is marked `(review)`; never present it as a command's.
- Use the model's exact ids.
- Count rows or findings only after counting them in the output (a CSV's data rows,
  without the header).
- Give the `bounds:` line a lifecycle result holds under. If `check` printed no
  `Witness_*`, `Deadlock_*`, or `Runs_*`, say so.
- Report everything under `not checked:` as not checked, never as passed.
- When a cap or the memory limit stopped a cone (`PatternCapReached`,
  `MemoryCapReached`, `StateCapReached`), say so, name the columns and use cases the
  message lists as growing the cone, and narrow (`--entity`, `--buc`) before raising a
  limit. A `MemoryCapReached` message may name the `--memory` that would finish.

## Recommendations

Give the smallest change that closes the gap, or the question whose answer does. Do not
list every future detail.

| Gap | Change |
|---|---|
| No performer | `performs(Actor, UseCase)`, or ask who performs it |
| No business | `belongs(Buc, Business)` |
| Empty CRUD row | Ask which entity the use case touches |
| Missing grant | `has_permission(Actor, Permission)`, or change who performs it |
| Cross-system relation | `coordinates(UseCase, A, B)` plus an API on each side |
| Unmapped field | `maps_field(Field, Entity.column)`, or mark it external |
| Missing effect | A `transitions` arm or `sets` on the use case that makes the change |
