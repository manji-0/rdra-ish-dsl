# Stage Readiness

<!-- derived-from ../../../docs/incremental-modeling.md#finding-the-current-stage -->

Tell "wrong" from "not modeled yet". A BUC with no entity is fine at the screens stage and
a gap once data touchpoints are under review. Commit to source only what is stable.

## Commands

`next` classifies each BUC by the stage gates and lists what blocks the next stage. Use the
others for evidence a gate does not count, and only for stages the model has reached.

```sh
rdra-ish next src/
rdra-ish next src/ --buc BucOrder --format json
rdra-ish list src/ --kind usecase --format table
rdra-ish csv src/ --kind matrix
rdra-ish csv src/ --kind business-inputs
rdra-ish csv src/ --kind api-matrix
```

## Signals

| Signal | Stage | Ask next |
|---|---|---|
| Business names, tentative BUCs | Scope sketch | Which business area and BUCs come first? |
| BUCs with few actors or use cases | BUC skeleton | Who performs which visible action? Does the order need a `flow`? |
| Use cases, no `displays` | BUC skeleton | Which screen does the actor use in each use case? |
| Screens, empty CRUD matrix | Screens | Which objects does each use case touch, and each screen show? |
| CRUD, no API | Data touchpoints | Which operations need an API, a permission, a medium? |
| APIs, systems, permissions, or media | API and access boundary | Which system owns each API? Which DTOs form its contract? |
| Columns, keys, `relate` | Entity structure | Which fields are state? Which are actor-entered (`business-inputs`)? |
| An inline Enum nothing moves (`lint` `enum-attribute`) | Entity structure | Does an actor choose it? Then type it by a `variation` |
| `Witness_*`, `Deadlock_*`, or `Runs_*` from `check` | Lifecycle | Which use case enters, leaves, or unblocks that state? |
| `check` passes, states reviewed | Lifecycle | Which combinations must never occur (`forbidden`, `exclusive`)? |
| Local rules stable | Rules | Which facts need `invariant`? Is any `required` fact truly global? |
| Multi-entity or comparison rules | Rules | Is each violation fixed, accepted, or open? |

## Analyze

1. Start from the strongest stable signal, not the most advanced detail.
2. Name the stage and its evidence (a command, a row count, a warning).
3. List the omissions this stage allows, and the ones that block the next stage.
4. Ask only the next stage's questions.

```text
Stage: <stage>
Evidence: <commands and signals>
Fine at this stage: <omissions>
Blocking: <what the next stage needs>
Next question: <one question>
```
