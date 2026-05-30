# Step 6 Requirements Analysis

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-6-business-rules -->
<!-- derived-from ../step-5-lifecycle/requirements-analysis.md -->

## Current Question

Which reachable state combinations are invalid or required?

## Requirement Notes

- A scheduled restock must have a next restock date.
- A blocked restock must not keep a next restock date.
- The lifecycle effects added in Step 5 should satisfy these rules.

## Review Notes

- If `rdra-ish states` reports a violation, the issue is in the transition or
  `sets` effect rather than in the entity structure.
- The rules intentionally come last so earlier steps can be reviewed without
  pretending all lifecycle detail is known.

## Summary

<!-- derived-from #requirement-notes -->
<!-- derived-from #review-notes -->

The model now checks business invariants against the derived state space.
