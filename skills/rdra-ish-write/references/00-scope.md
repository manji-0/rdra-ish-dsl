# Step 0: Scope Sketch

Use this context when the request names a business area or problem but does not yet
identify stable BUCs, actors, use cases, or data.

## Goal

Create only enough structure to name the business area and one or more candidate BUCs.
This step protects business intent before implementation detail enters the model.

## Ask For

<!-- stage-spec: asks scope -->
- Which business area or value stream is in scope?
- What are the candidate BUC names?
- Which BUC should be modeled first?
<!-- /stage-spec -->

Also ask:

- Which requirement, source, stakeholder, owner, priority, or acceptance criteria
  should be preserved now?

## Procedure

1. Create or update `shared/biz.rdra` with a `business` declaration.
2. Create one BUC file under `buc/` for each candidate BUC that is stable enough to
   name.
3. Add `belongs(Buc, Business)` for each BUC.
4. Add stable `requirement` metadata and `motivates(Requirement, Buc)` only when
   the requirement source is explicit.
5. Avoid actors, use cases, entities, APIs, states, and rules unless they are needed
   only as names for discussion.

## Minimal Pattern

```rdra
module shared.biz

business Commerce "Commerce"
requirement ReqOrder "Order processing must be reliable"
  priority "high"
  source "sales-ops"
  stakeholder "Order Desk"
  owner "Product"
  status "proposed"
  acceptance criteria "Order can be submitted and audited."
```

Requirement metadata values are string literals. `source`, `stakeholder`, and
`acceptance criteria` may repeat.

```rdra
module buc.order

import shared.biz

buc BucOrder "Process Order"
belongs(BucOrder, Commerce)
motivates(ReqOrder, BucOrder)
```

## Validation

<!-- stage-spec: review scope -->
```sh
rdra-ish check src/
rdra-ish list src/ --kind buc --format table
rdra-ish diagram src/ --kind rdra --format mermaid --buc BucOrder
```
<!-- /stage-spec -->

Also run:

```sh
rdra-ish lint src/ --format table
rdra-ish list src/ --kind requirement --format table
```

## Achievement Conditions

- The business scope has a named `business`.
- Candidate BUCs are named with reviewable labels.
- Each BUC belongs to the intended business area.
- Stable requirement metadata is captured without inventing missing stakeholder
  detail.
- Any missing actor/usecase/data detail is recorded as a next-step question rather
  than invented.

## Next Step

Load `references/01-buc-skeleton.md` when at least one BUC is stable enough
to ask who performs it and which visible actions compose it.
