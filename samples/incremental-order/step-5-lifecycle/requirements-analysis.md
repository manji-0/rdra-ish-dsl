# Step 5 Requirements Analysis

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-5-lifecycle -->
<!-- derived-from ../step-4-entity-structure/requirements-analysis.md -->

## Current Question

Which store values represent lifecycle state, and which actions change them?

## Requirement Notes

- Restock status starts as `normal`.
- Changing the next restock date schedules a restock and sets the date.
- Operations staff can block a scheduled restock when the store is temporarily
  closed.
- Blocking restock clears the next restock date.

## Open Questions

- Which combinations of restock status and date should be invalid?
- Are there required co-occurrences, such as scheduled restock requiring a date?

## Summary

<!-- derived-from #requirement-notes -->

The model now has events, states, transitions, and explicit column effects.
