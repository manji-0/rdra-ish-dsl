# Step 1 Requirements Analysis

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-1-buc-skeleton -->
<!-- derived-from ../step-0-scope/requirements-analysis.md -->

## Current Question

Who gets value from this BUC, and which user-visible actions compose it?

## Requirement Notes

- Operations staff maintain store restock dates.
- Staff can change the next restock date for a store.
- Staff can change the parent organization of a store when operations ownership
  changes.
- Data objects and transaction boundaries are still unknown.

## Open Questions

- Which business objects are created, read, updated, or deleted by each use case?
- Is parent organization change a local update or a coordinated boundary?

## Summary

<!-- derived-from #requirement-notes -->

The BUC has one actor and two use cases, but no entities yet.
