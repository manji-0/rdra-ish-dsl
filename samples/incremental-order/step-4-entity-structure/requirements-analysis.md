# Step 4 Requirements Analysis

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-4-entity-structure -->
<!-- derived-from ../step-3-interaction-boundary/requirements-analysis.md -->

## Current Question

What structure and ownership does the data need?

## Requirement Notes

- A store has a code and belongs to one organization.
- An organization has a code.
- Parent organization change crosses the store administration system and the
  organization system.
- The use case coordinates that cross-system relation by invoking APIs on both
  sides.

## Open Questions

- Which store fields represent lifecycle state?
- Which use cases or events change those lifecycle fields?

## Summary

<!-- derived-from #requirement-notes -->

This step makes ownership and cross-system consistency explicit.
