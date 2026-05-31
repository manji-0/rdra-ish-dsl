# RDRA-ish Interpretation

This note explains how RDRA-ish intentionally reads RDRA concepts when modeling
BUCs, business flow, and use cases. It is not a claim that these terms are identical
to the original RDRA method. The point is to keep the DSL internally consistent while
making the difference explicit.

<!-- constrained-by ./language-reference.md#instance-declarations -->
<!-- constrained-by ./language-reference.md#relationship-predicates -->
<!-- constrained-by ./language-reference.md#access-constraints -->
<!-- constrained-by ./language-reference.md#belongs-context -->
<!-- derived-from ./incremental-modeling.md#principle -->

## Basic Stance

RDRA-ish keeps the RDRA habit of explaining system design from business-facing
requirements, but it gives some concepts implementation-oriented responsibilities.
The model is expected to survive beyond requirements discovery and remain useful for
system boundary, API boundary, data, lifecycle, and rule review.

Because of that, RDRA-ish treats BUC, business flow, and UC as three different review
scopes rather than as a strict copy of the original RDRA artifacts:

| Concept | RDRA-ish role |
|---|---|
| BUC | A business-value slice and review container, optionally contextualized by timing/place/medium. |
| Business flow | The concrete flow that realizes a BUC through UCs and events. |
| UC | A concrete interaction and effect boundary that connects actors, screens, APIs, entities, events, permissions, and operation media. |

## BUC

In RDRA-ish, a BUC is the unit that keeps business value, ownership, and review scope
together. It is declared with `buc`, assigned to a business area with `belongs`, and
composed from UCs with `contains`.
When the assignment is contextual, `belongs(Buc, Business).when(...).where(...).by(...)`
records the timing, location/channel, and medium in which that business mapping applies.

This differs from a reading where BUC itself is the detailed business-flow artifact.
In RDRA-ish, the BUC is the business-value frame, and the business flow is its
concrete realization through the UCs and events contained in or connected from that
frame. The BUC is deliberately operational:

- It is the default file and module boundary: one BUC file gathers the local UCs,
  screens, APIs, events, and predicates that explain that slice.
- It is the main filter for diagrams, CRUD matrices, and state derivation.
- It owns the question "what business value are we reviewing right now?"
- It can name where, when, and by what medium the value slice applies without turning
  those context values into screens or APIs.
- It may contain human-initiated UCs and event-triggered UCs when both belong to the
  same value slice.

A BUC should be named from business value, not from a screen, table, endpoint, or
implementation component. If a proposed BUC cannot explain its value independently,
it is probably a UC, an API boundary, or a data operation instead.

## Business Flow

RDRA-ish does not currently have a first-class `business_flow` element. Business flow
is the concrete shape of a BUC, represented in one of two ways:

- as prose and staging guidance outside the DSL, especially while discovering scope;
- as model structure from `contains`, `raises`, `triggers`, `transitions`, and CRUD/API
  relationships once the BUC is concrete enough.

That means business flow is not an independent decomposition unit in the DSL. It is
the BUC-specific flow that explains how the BUC is carried out over time:

- inside a BUC, sequence diagrams show actor, screen, API, and entity interactions;
- when a BUC hands off to another BUC, event-flow diagrams show `UC -> Event -> UC`
  and event-to-state chains;
- state derivation shows which entity states are reachable through declared BUC/UC
  patterns.

This is intentional. RDRA-ish keeps flow review close to generated artifacts so the
same model can answer both business questions ("what happens next?") and design
questions ("which API/entity/state boundary carries that step?").

## Use Case

In RDRA-ish, a UC is a concrete user-visible or event-triggered interaction. It is not
only a sentence in a business scenario; it is the point where the model attaches
observable effects:

- `performs` connects an actor to a UC when the actor is known at that level.
- `displays` and `shows` connect the UC to UI surfaces and visible information.
- `invokes` connects the UC to API boundaries.
- CRUD predicates connect the UC or invoked API to entities.
- `raises`, `triggers`, `transitions`, and `sets` connect the UC to lifecycle effects.
- `requires_permission` and `requires_medium` state authority and medium constraints
  on the UC itself; API-specific constraints can be attached to the invoked API.

This makes a UC smaller and more effect-oriented than a whole business process. A good
UC name should describe one actor-intelligible action such as "Place Order", "Cancel
Reservation", or "Capture Payment". If a UC needs many unrelated actors, screens, API
boundaries, and state changes to make sense, it is probably too large and should be
split under a BUC.

## Reading The Three Together

The practical reading is:

1. Use BUCs to choose the business-value slice under review.
2. Treat business flow as the concrete realization of that BUC.
3. Use UCs to name the concrete interactions inside that flow.
4. Use event-flow, sequence, CRUD, ER, and state diagrams to review the flow rather
   than modeling business flow as a separate primitive.

This leads to a deliberate asymmetry:

| Question | RDRA-ish answer |
|---|---|
| What value or responsibility are we reviewing? | BUC |
| How is that BUC concretely carried out? | Business flow through UCs and events |
| What action happens inside that flow? | UC |
| What order or causality connects actions? | Event-flow, sequence, and prose |
| What data or lifecycle effect does an action have? | CRUD, API, `sets`, `raises`, `transitions` |
| What technical boundary carries the action? | Screen/API/System relationships |
| What authority or medium constrains the action? | `has_permission`, `requires_permission`, `requires_medium`, and screen-constraints CSV |

## Modeling Heuristics

<!-- derived-from #buc -->
<!-- derived-from #business-flow -->
<!-- derived-from #use-case -->

- Create a BUC when the slice has independent business value and can be reviewed by a
  business stakeholder.
- Treat the business flow as the first concrete expansion of that BUC, even though the
  DSL stores it through UCs, events, and generated views rather than a dedicated node.
- Create a UC when there is a user-visible or event-triggered action whose data,
  screen, API, event, or state effects should be reviewable.
- Use `triggers(Event, UseCase)` when flow crosses from one UC to another through a
  domain event, especially when the target UC belongs to a different BUC.
- Prefer prose for early business-flow discovery. Move flow into DSL predicates only
  when it affects reviewable artifacts or consistency checks.
- Do not introduce an API just to mirror a UC. Introduce it when the interaction has a
  meaningful consistency, transaction, ownership, or integration boundary.
- Do not attach permission or device constraints directly to a screen. Declare them on
  the UC/API path and derive screen patterns through `displays` and `invokes`.

## Summary

<!-- derived-from #basic-stance -->
<!-- derived-from #reading-the-three-together -->

RDRA-ish is RDRA-inspired, not RDRA-equivalent. BUCs are business-value containers,
business flow is the concrete realization of a BUC through UCs and events, and UCs are
effect-bearing interactions. This keeps the model useful both for requirements
discussion and for implementation-oriented review.
