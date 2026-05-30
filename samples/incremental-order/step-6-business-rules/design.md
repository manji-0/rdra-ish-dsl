# Step 6 Design

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-6-business-rules -->
<!-- constrained-by ../../../docs/state-derivation.md#operations -->
<!-- derived-from ./requirements-analysis.md -->

## Model Slice

The rules live near the owning entity:

- `forbidden(Store, (restock_status, blocked), (next_restock_date, present))`
- `invariant(Store).when(restock_status, scheduled).then(next_restock_date, present)`

The first rule rejects blocked stores with a scheduled date. The invariant
requires a date when restock is scheduled.

## Validation

Run:

```sh
rdra-ish check samples/incremental-order/step-6-business-rules/src
rdra-ish states samples/incremental-order/step-6-business-rules/src --entity Store
rdra-ish diagram samples/incremental-order/step-6-business-rules/src --kind state --format mermaid --buc BucStoreRestock
```

## Summary

<!-- derived-from #model-slice -->

This is the most concrete step: the state space is constrained by business
rules.
