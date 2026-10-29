# Step 5: Lifecycle

Use this context when entity structure exists and the next task is to describe which
use cases move entity state, which events and timers start other work, which
conditions guard a use case, and which columns each use case changes.

## Goal

Make lifecycle causality reviewable. A use case drives every state change, as in the
遷移UC column of an RDRA state sheet. An event is only the interface between use cases,
or between a use case and an external system; a timer starts a use case on a schedule.
`check` then explores the reachable states and reports what the model cannot do.

## Ask For

- Which entity values represent lifecycle state, and which ones does an actor merely
  choose?
- Which use case moves each state, from which state to which?
- Which use case raises each domain event, and what consumes it?
- Does any event start another BUC before the entry use case is fixed?
- Does a schedule start any use case?
- Does a business rule (an RDRA 条件) decide when a use case may run?
- Which nullable, enum, boolean, Int, or comparison values change with each use case?
- Which states are end states, and which must always have a way out?

## Procedure

1. Add lifecycle columns such as `Enum(...)`, `Bool`, and nullable timestamp/value
   columns. Values an actor merely chooses are not lifecycle: type those columns by a
   `variation` and leave them out of `transitions`.
2. Add `transitions(Entity.col, UseCase, from -> to)` for lifecycle movement, naming
   the use case that drives each edge. States are not declared: each variant a
   `transitions` names, and each value a creating use case sets, is a state of the
   state model.
3. Declare an `event` and `raises(UseCase, Event)` only for an interface between use
   cases: something must consume the event (`triggers`, or `outbox(Event)` for a
   publication outside the model), else `check` warns that it is never consumed.
4. For downstream automation, model BUC-level causality first:
   `triggers(Event, TargetBuc)`. When the concrete entry action is known, refine with
   `contains(TargetBuc, EntryUseCase)` and `triggers(Event, EntryUseCase)` without
   removing the BUC-level trigger.
5. Connect an external system through events: `raises(ExtSystem, Event)` inbound,
   `triggers(Event, ExtSystem)` outbound.
6. Declare a `timer` and `triggers(Timer, UseCase)` for a schedule that starts a use
   case. Keep that use case in a BUC.
7. Declare a `condition` for a business rule that selects when a use case applies.
   Without a body it is a trace link (`combines` its variation and lifecycle column);
   with `holds(Cond, Entity.col == value, ...)` it guards each use case it
   `applies_to`, and `check` and TLC read the guard.
8. Add `sets(UseCase, Entity, col == val)` for enum, nullable, boolean, and Int
   effects not explained by transitions. A `sets` on a column that `transitions`
   drives is an error unless the use case creates the entity.
9. Add comparison-proposition `sets` when rules depend on a derived comparison and the
   model does not track the values. Prefer Int / Money / Decimal columns plus
   `sets(UC, E, col == N)` when the quantity itself must be checked: a comparison
   reads those values in `states` and in TLA+.
10. Add `after(UseCase).assert(...)` for a postcondition of that use case, and
    `property Name ["label"] always|eventually|leads_to(...)` for a temporal path
    property (checked by `verify` only).

## Minimal Pattern

```rdra
entity Order "Order" {
  id: Int @pk
  status: Enum(draft, submitted, fulfilled) @default(draft)
  submitted_at: DateTime @null
}

usecase CreateOrder "Create Order"
usecase SubmitOrder "Submit Order"
usecase FulfillOrder "Fulfill Order"

creates(CreateOrder, Order)
transitions(Order.status, SubmitOrder, draft -> submitted)
transitions(Order.status, FulfillOrder, submitted -> fulfilled)
sets(SubmitOrder, Order, submitted_at == present)
```

Event-started BUC, a timer, and a condition with a body:

```rdra
event OrderSubmitted "Order Submitted"
raises(SubmitOrder, OrderSubmitted)
triggers(OrderSubmitted, BucFulfillment)

contains(BucFulfillment, PickOrder)
triggers(OrderSubmitted, PickOrder)

timer NightlyExpiry "Nightly expiry sweep"
triggers(NightlyExpiry, ExpireDrafts)

condition CardPayment "The order is paid by card"
holds(CardPayment, Payment.method == card)
applies_to(CardPayment, CapturePayment)
```

## Validation

```sh
rdra-ish check src/
rdra-ish states src/ --entity Order
rdra-ish diagram src/ --kind state --format mermaid --buc BucOrder
rdra-ish diagram src/ --kind event-flow --format mermaid
rdra-ish export src/ --kind asyncapi --out out/asyncapi.json
```

`check` reports, by name, what the lifecycle cannot do:

- `error[Witness_<E>_<col>_<value>]`: no sequence of use cases reaches that state.
- `error[Deadlock_<E>_<col>_<value>]`: an instance can reach that state, which is not
  an end state, and nothing ever moves it off (a guard that never opens, or a missing
  use case). A deadlock that only a free instance slot would clear is a warning.
- `warning[Runs_<UseCase>]`: the use case never runs, with the guard or instance that
  blocks it.

Each prints the `transitions` arm or use case at fault with its `path:line:col`. The
fix is usually in the model (a missing `creates`, `transitions` arm, or `sets`), not in
the rule. Use skill `rdra-ish-verify` to check the same model at two instances and its
temporal properties.

## Achievement Conditions

- Every state change names the use case that drives it.
- Events have clear origins through `raises` unless intentionally external, and raised
  events are consumed by `triggers`, `outbox`, or an external system.
- Event-started BUCs are represented directly with `triggers(Event, Buc)`, and
  concrete triggered use cases are contained by the downstream BUC.
- `check` reports no `Witness_*` or `Deadlock_*` error, and each `Runs_*` warning is
  understood.
- `sets` captures relevant enum, nullable, boolean, Int, and comparison effects.
- AsyncAPI export is meaningful when events, raises/triggers/transitions, and
  outbox-like handoffs are part of the review.

## Next Step

Load `references/06-rules.md` when reachable state combinations need
entity constraints.
