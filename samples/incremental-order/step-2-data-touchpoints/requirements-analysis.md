# Step 2 Requirements Analysis

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-2-data-touchpoints -->
<!-- derived-from ../step-1-buc-skeleton/requirements-analysis.md -->

## Current Question

Which business objects does each use case touch?

## Requirement Notes

- Changing the next restock date updates a store.
- Changing parent organization updates a store and reads the organization chosen
  as the new parent.
- At this stage, direct use-case CRUD is intentional. API boundaries are not
  known yet.

## Open Questions

- Which screens expose these use cases?
- Does parent organization change need a backend API boundary?
- Which fields identify `Store` and `Organization`?

## Summary

<!-- derived-from #requirement-notes -->

The model now has coarse entities with only identifiers and direct
`UseCase -> Entity` operations.
