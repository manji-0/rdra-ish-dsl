# Incremental Order Sample

This sample shows the same BUC modeled at eight abstraction levels. Each step is
self-contained: run commands against that step's `src/` directory.

<!-- constrained-by ../../docs/incremental-modeling.md#stage-map -->

| Step | Focus | Try |
|---|---|---|
| `step-0-scope` | business area and BUC name | `rdra-ish check samples/incremental-order/step-0-scope/src` |
| `step-1-buc-skeleton` | actors and user-visible use cases | `rdra-ish diagram ... --kind rdra --format mermaid` |
| `step-2-screens` | the screen each use case works on | `rdra-ish diagram ... --kind screen --format mermaid` |
| `step-3-data-touchpoints` | coarse entities, direct use-case CRUD, and what each screen shows | `rdra-ish csv ... --kind matrix` |
| `step-4-api-access-boundary` | APIs, system ownership, and access/media constraints | `rdra-ish csv ... --kind screen-constraints`, `rdra-ish csv ... --kind actor-permission-audit` |
| `step-5-entity-structure` | columns and cross-system relation coordination | `rdra-ish diagram ... --kind er --format mermaid` |
| `step-6-lifecycle` | events, states, transitions, and effects | `rdra-ish states ... --entity Store` |
| `step-7-business-rules` | forbidden and invariant state combinations | `rdra-ish states ... --entity Store` |

Run `rdra-ish next samples/incremental-order/<step>/src` on any step: steps 0 to 2 and
step 5 report their own stage and list what the next step adds as gaps. Step 3 reports
stage 4: its use cases invoke no API, and the API and access boundary gate only asks that
an invoked API belong to a system, so a BUC that needs no API passes it as it stands.
Step 4 adds the APIs, systems, and access vocabulary that the stage offers. Step 6 reports
`stage 6 Lifecycle, required stages complete` and offers the optional rule stages; step 7,
which adds a `forbidden` and an `invariant`, meets 7A and 7B and is offered 7C.

## Reading Order

For each step, read `requirements-analysis.md` first, then `design.md`, then the
`.rdra` files under `src/`.

## Summary

<!-- derived-from #reading-order -->

The important modeling move is that Step 2 settles the screen each use case works on
before any data is named, and Step 3 intentionally uses direct `UseCase -> Entity` CRUD.
Step 4 adds APIs only where a consistency boundary is worth naming, assigns each to a
system, and introduces `medium` / `permission` vocabulary. Screen-level constraints are
derived, not written by hand: run `rdra-ish csv ... --kind screen-constraints` to see
which UC/API permission and medium requirements pass through each screen, and
`rdra-ish csv ... --kind actor-permission-audit` to see which actor-side grants are
inferred as ok, missing, or excess. Step 5 adds columns, and `coordinates` for the
relation that crosses the two systems.
