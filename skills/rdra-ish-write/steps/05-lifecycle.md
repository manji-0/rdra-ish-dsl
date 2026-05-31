# Step 5: Lifecycle

<!-- derived-from ../../../docs/incremental-modeling.md#stage-5-lifecycle -->
<!-- derived-from ../../../docs/language-reference.md#event-triggered-bucs -->

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
   columns.
2. Declare `event` and `state` nodes where a state machine is useful.
3. Add `raises(UseCase, Event)` for event origins.
4. Add `transitions(Event, FromState, ToState)` for lifecycle movement.
5. For downstream automation, model BUC-level causality first:
   `triggers(Event, TargetBuc)`.
6. When the concrete entry action is known, refine with
   `contains(TargetBuc, EntryUseCase)` and `triggers(Event, EntryUseCase)` without
   removing the BUC-level trigger.
7. Add `sets(UseCase|Event, Entity, "col", "val")` for enum, nullable, and boolean
   effects not fully explained by transitions.
8. Add comparison-proposition `sets` when rules depend on derived comparisons.

## Minimal Pattern

```rdra
entity Order "Order" {
  id: Int @pk
  status: Enum(draft, submitted, fulfilled) @default(draft)
  submitted_at: DateTime @null
}

event OrderSubmitted "Order Submitted"
state OrderDraft "Order Draft"
state OrderSubmittedState "Order Submitted"

raises(SubmitOrder, OrderSubmitted)
transitions(OrderSubmitted, OrderDraft, OrderSubmittedState)
sets(SubmitOrder, Order, "submitted_at", "timestamptz")
```

Event-started BUC:

```rdra
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
```

## Achievement Conditions

- Events have clear origins through `raises` unless intentionally external.
- Raised events are consumed by `transitions`, `triggers`, or documented open items.
- Event-started BUCs are represented directly with `triggers(Event, Buc)`.
- Concrete triggered use cases are contained by the downstream BUC.
- State derivation explains expected reachable states and reviewed warnings.
- `sets` captures relevant enum, nullable, boolean, and comparison effects.

## Next Step

Load `steps/06-rules.md` when reachable state combinations need
forbidden or invariant constraints.
