# Incremental Order Sample

This sample shows the same BUC modeled at seven abstraction levels. Each step is
self-contained: run commands against that step's `src/` directory.

<!-- constrained-by ../../docs/incremental-modeling.md#stage-map -->

| Step | Focus | Try |
|---|---|---|
| `step-0-scope` | business area and BUC name | `rdra-ish check samples/incremental-order/step-0-scope/src` |
| `step-1-buc-skeleton` | actors and user-visible use cases | `rdra-ish diagram ... --kind rdra --format mermaid` |
| `step-2-data-touchpoints` | coarse entities and direct use-case CRUD | `rdra-ish csv ... --kind matrix` |
| `step-3-interaction-boundary` | screens, APIs, system ownership, and access/media constraints | `rdra-ish csv ... --kind screen-constraints` |
| `step-4-entity-structure` | columns and cross-system relation coordination | `rdra-ish diagram ... --kind er --format mermaid` |
| `step-5-lifecycle` | events, states, transitions, and effects | `rdra-ish states ... --entity Store` |
| `step-6-business-rules` | forbidden and invariant state combinations | `rdra-ish states ... --entity Store` |

## Reading Order

For each step, read `requirements-analysis.md` first, then `design.md`, then the
`.rdra` files under `src/`.

## Summary

<!-- derived-from #reading-order -->

The important modeling move is that Step 2 intentionally uses direct
`UseCase -> Entity` CRUD. Step 3 adds APIs only where a consistency boundary is
worth naming, and also introduces `timing` / `location` / `medium` /
`permission` vocabulary. Screen-level constraints are derived, not written by hand:
run `rdra-ish csv ... --kind screen-constraints` to see which UC/API permission and
medium requirements pass through each screen. Step 4 adds systems by assigning APIs
to system boundaries.
