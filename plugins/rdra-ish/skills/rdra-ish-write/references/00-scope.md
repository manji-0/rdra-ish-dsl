# Step 0: Scope Sketch

<!-- derived-from ../../../docs/incremental-modeling.md#stage-0-scope-sketch -->

Name the business area and its candidate BUCs, and nothing more. Use this step when the
request names a business problem but no stable BUC, actor, or data.

## Ask

<!-- stage-spec: asks scope -->
- Which business area or value stream is in scope?
- What are the candidate BUC names?
- Which BUC should be modeled first?
<!-- /stage-spec -->

Also ask which requirement source, stakeholder, owner, priority, or acceptance criteria
to keep now.

## Do

1. Declare the `business` in `shared/biz.rdra`.
2. Create one `buc/buc_<name>.rdra` per BUC stable enough to name, with
   `belongs(Buc, Business)`.
3. Add a `requirement` and `motivates(Requirement, Buc)` only when the source is explicit.
4. Leave actors, use cases, and data as open questions.

```rdra
module shared.biz

business Commerce "Commerce"

requirement ReqOrder "Order processing must be reliable"
  priority "high"
  source "sales-ops"
  stakeholder "Order Desk"
  owner "Product"
  acceptance_criteria "Order can be submitted and audited."
  status "proposed"
```

A metadata value is a quoted string, or a bare word or number (`priority must`).
`source`, `stakeholder`, and `acceptance_criteria` may repeat.

```rdra
module buc.order

import shared.biz

buc BucOrder "Process Order"
belongs(BucOrder, Commerce)
motivates(ReqOrder, BucOrder)
```

## Validate

<!-- stage-spec: review scope -->
```sh
rdra-ish check src/
rdra-ish list src/ --kind buc --format table
rdra-ish diagram src/ --kind rdra --format mermaid --buc BucOrder
```
<!-- /stage-spec -->

Also `rdra-ish list src/ --kind requirement --format table`.

## Done when

- The business and each candidate BUC have reviewable labels, and each BUC `belongs` to it.
- Requirement metadata is what the source gives, with no invented stakeholder.
- Missing actors, use cases, and data are listed as questions.

Next: `references/01-buc-skeleton.md`, once a BUC is stable enough to ask who performs it.
