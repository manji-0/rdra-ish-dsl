# Step 5: Lifecycle

Use this context when entity structure exists and the next task is to describe state,
events, event-started BUCs, and explicit column effects.

## Goal

Make lifecycle causality reviewable: which use cases raise events, which events
transition states, which events start downstream BUCs, and which columns are changed
by use cases or events.

## Ask For

- Which entity values represent lifecycle state?
- Which use case raises each domain event?
- Which event causes each state transition?
- Does any event start another BUC before the entry use case is fixed?
- Which nullable, enum, boolean, or comparison values change with each use case/event?
- Are unreachable states intentional placeholders?

## Procedure

1. Add lifecycle columns such as `Enum(...)`, `Bool`, and nullable timestamp/value
   columns. Values an actor merely chooses are not lifecycle: type those columns by a
   `variation` and leave them out of `transitions` / `sets`.
2. Add `transitions(Entity.col, UseCase, from -> to)` for lifecycle movement, naming
   the use case that drives each edge (the 遷移UC column of an RDRA state sheet).
   States are not declared: each `transitions` variant becomes a state of its entity.
3. Declare an `event` and `raises(UseCase, Event)` only for an interface between use
   cases: something must consume the event.
4. For downstream automation, model BUC-level causality first:
   `triggers(Event, TargetBuc)`.
5. When the concrete entry action is known, refine with
   `contains(TargetBuc, EntryUseCase)` and `triggers(Event, EntryUseCase)` without
   removing the BUC-level trigger.
6. Add `sets(UseCase, Entity, col == val)` for enum, nullable, and boolean
   effects not fully explained by transitions.
7. Add comparison-proposition `sets` when rules depend on derived comparisons
   (BFS layer). Prefer Int / Money / Decimal columns plus arithmetic rules when the
   quantity itself must be checked; validate those with TLA+ export.
8. Add `after(UseCase).assert(...)` for postconditions of that use case.
9. Add `property Name [label] always|eventually|leads_to(...)` for temporal path
    properties (label string optional).

## Minimal Pattern

```rdra
entity Order "Order" {
  id: Int @pk
  status: Enum(draft, submitted, fulfilled) @default(draft)
  submitted_at: DateTime @null
}

transitions(Order.status, SubmitOrder, draft -> submitted)
sets(SubmitOrder, Order, submitted_at == present)
```

Event-started BUC:

```rdra
event OrderSubmitted "Order Submitted"
raises(SubmitOrder, OrderSubmitted)
triggers(OrderSubmitted, BucFulfillment)

contains(BucFulfillment, PickOrder)
triggers(OrderSubmitted, PickOrder)
```

## Validation

```sh
rdra-ish check src/
rdra-ish states src/ --entity Order
rdra-ish diagram src/ --kind state --format mermaid --buc BucOrder
rdra-ish diagram src/ --kind event-flow --format mermaid
rdra-ish export src/ --kind asyncapi --out out/asyncapi.json
rdra-ish export src/ --kind tla -o out/   # Int/now/temporal/after.assert
```

## Achievement Conditions

- Events have clear origins through `raises` unless intentionally external.
- Raised events are consumed by `triggers`, `outbox`, or documented open items.
- Event-started BUCs are represented directly with `triggers(Event, Buc)`.
- Concrete triggered use cases are contained by the downstream BUC.
- State derivation explains expected reachable states and reviewed warnings.
- `sets` captures relevant enum, nullable, boolean, and comparison effects.
- Int/`now`/temporal/`after.assert` obligations are reviewed via TLA+ when present.
- AsyncAPI export is meaningful when events, raises/triggers/transitions, and
  outbox-like handoffs are part of the review.

## Next Step

Load `references/06-rules.md` when reachable state combinations need
entity constraints.
