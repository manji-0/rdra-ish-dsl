# Step 2 Design

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-2-data-touchpoints -->
<!-- constrained-by ../../../docs/language-reference.md#relationship-predicates -->
<!-- derived-from ./requirements-analysis.md -->

## Model Slice

`Store` and `Organization` are declared as coarse entities. CRUD is attached
directly to use cases:

- `updates(ChangeNextRestockDate, Store)`
- `reads(ChangeStoreParentOrganization, Organization)`
- `updates(ChangeStoreParentOrganization, Store)`

This is a legal early modeling form because CRUD predicates accept
`UseCase | Api` as the source.

## Validation

Run:

```sh
rdra-ish check samples/incremental-order/step-2-data-touchpoints/src
rdra-ish csv samples/incremental-order/step-2-data-touchpoints/src --kind matrix
rdra-ish diagram samples/incremental-order/step-2-data-touchpoints/src --kind er --format mermaid
```

## Summary

<!-- derived-from #model-slice -->

This step captures data touchpoints without forcing API or table detail.
