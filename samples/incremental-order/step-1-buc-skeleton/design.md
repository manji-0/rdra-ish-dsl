# Step 1 Design

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-1-buc-skeleton -->
<!-- derived-from ./requirements-analysis.md -->

## Model Slice

The shared vocabulary now includes `OpsStaff`. The BUC file declares:

- `ChangeNextRestockDate`
- `ChangeStoreParentOrganization`

Both use cases are contained by `BucStoreRestock`.

## Validation

Run:

```sh
rdra-ish check samples/incremental-order/step-1-buc-skeleton/src
rdra-ish diagram samples/incremental-order/step-1-buc-skeleton/src --kind rdra --format mermaid --buc BucStoreRestock
```

## Summary

<!-- derived-from #model-slice -->

This step is still behavior-oriented: it names actions before naming data.
