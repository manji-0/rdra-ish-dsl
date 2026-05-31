# Step 0: Scope Sketch

Use this context when the request names a business area or problem but does not yet
identify stable BUCs, actors, use cases, or data.

## Goal

Create only enough structure to name the business area and one or more candidate BUCs.
This step protects business intent before implementation detail enters the model.

## Ask For

- What business area, value stream, or operating domain is in scope?
- What candidate BUC names should be reviewed?
- Which BUC should be refined first?

## Procedure

1. Create or update `shared/biz.rdra` with a `business` declaration.
2. Create one BUC file under `buc/` for each candidate BUC that is stable enough to
   name.
3. Add `belongs(Buc, Business)` for each BUC.
4. Avoid actors, use cases, entities, APIs, states, and rules unless they are needed
   only as names for discussion.

## Minimal Pattern

```rdra
module shared.biz

business Commerce "Commerce"
```

```rdra
module buc.order

import shared.biz

buc BucOrder "Process Order"
belongs(BucOrder, Commerce)
```

## Validation

```sh
rdra-ish check src/
rdra-ish list src/ --kind buc --format table
rdra-ish diagram src/ --kind rdra --format mermaid --buc BucOrder
```

## Achievement Conditions

- The business scope has a named `business`.
- Candidate BUCs are named with reviewable labels.
- Each BUC belongs to the intended business area.
- Any missing actor/usecase/data detail is recorded as a next-step question rather
  than invented.

## Next Step

Load `references/01-buc-skeleton.md` when at least one BUC is stable enough
to ask who performs it and which visible actions compose it.
