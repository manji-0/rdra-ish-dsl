# Step 3 Requirements Analysis

<!-- constrained-by ../../../docs/incremental-modeling.md#stage-3-interaction-boundary -->
<!-- derived-from ../step-2-data-touchpoints/requirements-analysis.md -->

## Current Question

Which UI and API boundaries mediate the work?

## Requirement Notes

- Operations staff use a store maintenance screen.
- Next restock date is a simple store-only update and can remain direct.
- Parent organization change must validate the organization and update the
  store through a backend boundary.
- The organization lookup and store update belong to different internal systems.

## Open Questions

- What fields identify stores and organizations?
- Is there an explicit relationship between store and organization?
- Are assignment history records required?

## Summary

<!-- derived-from #requirement-notes -->

This step introduces APIs and systems only where the consistency boundary is
worth naming.
