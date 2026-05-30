# Step 5 Design

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-5-lifecycle -->
<!-- constrained-by ../../../docs/state-derivation.md#operations -->
<!-- derived-from ./requirements-analysis.md -->

## Model Slice

`Store` gains two lifecycle axes:

- `restock_status: Enum(normal, scheduled, blocked) @default(normal)`
- `next_restock_date: DateTime @null`

`RestockScheduled` transitions `Normal` to `Scheduled` and sets
`next_restock_date` to present. `RestockBlocked` transitions `Scheduled` to
`Blocked` and clears `next_restock_date`.

## Validation

Run:

```sh
rdra-ish check samples/incremental-order/step-5-lifecycle/src
rdra-ish states samples/incremental-order/step-5-lifecycle/src --entity Store
rdra-ish diagram samples/incremental-order/step-5-lifecycle/src --kind event-flow --format mermaid
```

## Summary

<!-- derived-from #model-slice -->

State derivation can now show reachable restock patterns before hard business
rules are added.
