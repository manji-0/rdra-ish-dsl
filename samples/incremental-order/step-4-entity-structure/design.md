# Step 4 Design

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-4-entity-structure -->
<!-- constrained-by ../../../docs/language-reference.md#api-and-the-api-layer -->
<!-- derived-from ./requirements-analysis.md -->

## Model Slice

Entities now have reviewable structure:

- `Store` has `id`, `code`, and `name`.
- `Organization` has `id`, `code`, and `name`.
- `relate(Store, Organization, "N:1")` models store ownership.

Because `Store` is derived into `StoreAdminSystem` and `Organization` is derived
into `OrganizationSystem`, `coordinates(ChangeStoreParentOrganization, Store,
Organization)` documents the cross-system consistency responsibility.

Sequence diagrams show the write side of this flow. The read-only organization
side is still part of the system boundary because API matrix and system
diagnostics consume both reads and writes.

## Validation

Run:

```sh
rdra-ish check samples/incremental-order/step-4-entity-structure/src
rdra-ish diagram samples/incremental-order/step-4-entity-structure/src --kind er --format mermaid
rdra-ish diagram samples/incremental-order/step-4-entity-structure/src --kind sequence --format mermaid --buc BucStoreRestock
```

## Summary

<!-- derived-from #model-slice -->

The entity relationship is now concrete enough for system-boundary diagnostics
to be meaningful.
