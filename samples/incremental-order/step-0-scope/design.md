# Step 0 Design

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-0-scope-sketch -->
<!-- derived-from ./requirements-analysis.md -->

## Model Slice

The design keeps one shared business file and one BUC-local file:

- `src/shared/biz.rdra` declares `StoreOperations`.
- `src/buc/buc_store_restock.rdra` declares `BucStoreRestock`.

## Validation

Run:

```sh
rdra-ish check samples/incremental-order/step-0-scope/src
rdra-ish list samples/incremental-order/step-0-scope/src --kind buc --format table
```

## Summary

<!-- derived-from #model-slice -->

No use cases or entities are required yet.
