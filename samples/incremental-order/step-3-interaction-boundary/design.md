# Step 3 Design

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-3-interaction-boundary -->
<!-- constrained-by ../../../docs/language-reference.md#api-and-the-api-layer -->
<!-- derived-from ./requirements-analysis.md -->

## Model Slice

The BUC now has `StoreMaintenanceScreen`, `StoreAdminApi`, and
`OrganizationLookupApi`.

`ChangeNextRestockDate` keeps direct CRUD because the operation is closed inside
`Store`. `ChangeStoreParentOrganization` invokes two APIs:

- `StoreAdminApi` updates `Store`.
- `OrganizationLookupApi` reads `Organization`.

`contains(StoreAdminSystem, StoreAdminApi)` and
`contains(OrganizationSystem, OrganizationLookupApi)` define system membership.
The system entity sets are derived from API CRUD targets.

Sequence diagrams are write-focused, so the read-only organization lookup is
easiest to confirm with `csv --kind api-matrix`.

## Validation

Run:

```sh
rdra-ish check samples/incremental-order/step-3-interaction-boundary/src
rdra-ish diagram samples/incremental-order/step-3-interaction-boundary/src --kind sequence --format mermaid --buc BucStoreRestock
rdra-ish csv samples/incremental-order/step-3-interaction-boundary/src --kind api-matrix
```

## Summary

<!-- derived-from #model-slice -->

The model now separates simple direct updates from API-mediated consistency
boundaries.
