# Stage Readiness Analysis

Use this reference when the user asks whether a BUC or model is ready for the next
refinement step, or when the request is broad enough that you need to classify the
current stage before analyzing details.

## Concept

RDRA-ish refinement intentionally moves from business concern to technical concern.
Early omissions are not always defects. A BUC with no API may be perfectly fine at the
data-touchpoint stage, while the same omission is a real gap once the interaction
boundary is being reviewed. Stage readiness analysis separates "wrong" from "not yet
modeled".

The stage question is: what information is already stable enough to commit to source?
Do not force columns, APIs, states, or rules just because later stages exist.

## Commands

```sh
rdra-ish check src/
rdra-ish list src/ --kind buc --format table
rdra-ish list src/ --kind usecase --format table
rdra-ish list src/ --kind actor --format table
rdra-ish csv src/ --kind matrix
rdra-ish csv src/ --kind business-inputs
rdra-ish list src/ --kind api --format table
rdra-ish csv src/ --kind api-matrix
rdra-ish states src/
```

Use the later commands only when earlier signals suggest the model has reached that
stage.

## Stage Signals

| Current signal | Likely stage | Ask next |
|---|---|---|
| Business names exist but BUCs are tentative | Scope sketch | Which business area and candidate BUCs matter first? |
| BUCs exist but actors/use cases are sparse | BUC skeleton | Which actors perform which user-visible actions? |
| Use cases exist but CRUD matrix is empty | BUC skeleton | Which business objects does each use case create/read/update/delete? |
| CRUD exists but entities are coarse | Data touchpoints | Which objects are real domain data versus temporary interaction detail? |
| CRUD exists but sequence output uses only the legacy `System` lane | Data touchpoints | What screens and APIs mediate the work? |
| APIs/screens/permissions appear | Interaction boundary | Which system owns each API and which constraints apply to UC/API paths? |
| Entities have columns, keys, and `relate` | Entity structure | Which fields represent lifecycle state or cross-system coordination? |
| `business-inputs` has surprising or missing field rows | Entity structure | Which fields are actor-entered versus derived by defaults, FK relations, APIs, events, or `sets`? |
| Enum/Bool/nullable columns exist without lifecycle effects | Entity structure | Which events, transitions, or `sets` effects change them? |
| `states` has reviewed reachable patterns | Lifecycle | Which local guardrails (`forbidden`, `exclusive`) should be checked first? |
| local guardrails exist and are stable | Business rules | Which local obligations need `invariant`, and are any `required` facts truly global? |
| `invariant`, `required`, comparison propositions, `cross_forbidden`, or `cross_invariant` exists | Business rules | Are violations fixed, intentionally accepted, not evaluable from state axes/caps, relation-scoped via `.along(...)`, or still unresolved requirements? |

## How To Analyze

1. Start with the strongest stable signal, not the most advanced isolated detail.
2. Classify the stage and name the evidence: command, row count, warning, or missing
   predicate family.
3. Identify omissions that are acceptable for the current stage.
4. Identify omissions that block the next stage.
5. Ask only the next-stage questions; avoid jumping two stages ahead.

## Output Shape

```text
Stage: <stage>
Evidence: <commands and model signals>
Appropriate omissions: <things not required yet>
Blocking gaps: <things needed before the next stage>
Next question: <one focused refinement question>
```
