# Step 6: Lifecycle

<!-- derived-from ../../../docs/incremental-modeling.md#stage-6-lifecycle -->

Say which use case moves each state, which events and timers start other work, which
conditions guard a use case, and which columns each use case changes. A use case drives
every state change, as in the 遷移UC column of an RDRA state sheet; an event only connects
use cases, or a use case and an external system. `check` then explores the reachable
states.

## Ask

<!-- stage-spec: asks lifecycle -->
- Which Enum column and variants represent lifecycle state?
- Which use case causes each state change?
- Does any event start another BUC, and is the concrete entry use case known yet?
- Which nullable or boolean fields change together with the lifecycle?
- Are the Enum variants the states you want to see? Diagrams derive one state per variant.
<!-- /stage-spec -->

Also ask which values an actor merely chooses, which schedules start a use case, which
business rule (条件) decides when a use case may run, and which states are end states.

## States

- Write `transitions(UseCase, Entity.col, from -> to)` for each edge. Each variant a
  `transitions` names, and each value a creating use case sets, is a state.
- `transitions(Cancel, Order.status, (pending, held) -> cancelled)` is one arm per source.
- Several arms of one use case from one state are the actor's choice: a run takes one arm.
- An entity may have several lifecycle columns, each its own state model. A use case that
  moves two takes one arm of each in one step, so it runs only where both have an arm.
- Mark a state an instance may rest in, though an arm leaves it, with `@end(free)` on the
  column; otherwise `check` reports a deadlock there. A state no arm leaves is an end state
  already, and `@end` on it draws `lint` `end-redundant`.

## Events and timers

- Declare an `event` with `raises(UseCase, Event)` only when something consumes it:
  `triggers`, or `outbox(Event)` for a publication outside the model.
- For a downstream BUC, write `triggers(Event, TargetBuc)` first. Once the entry use case is
  known, add `contains(TargetBuc, EntryUseCase)` and `triggers(Event, EntryUseCase)`, and
  keep the BUC-level trigger. The entry use case runs after a raise as often as its states
  allow; `triggers(Event, EntryUseCase).counted(2)` makes it once per raise, with a queue
  of 2 (`verify` checks `EventOverflow_<E>_<Event>_<EntryUseCase>`).
- An external system receives with `triggers(Event, ExtSystem)` and sends with
  `raises(ExtSystem, Event)`.
- A schedule is a `timer` with `triggers(Timer, UseCase)`, the use case in a BUC. A timer
  has no parameter: its use case's guards pick the rows.

## Conditions

A `condition` is an RDRA 条件. With only `combines` it is a trace link. With
`holds(Cond, Entity.col == value, ...)` and `applies_to(Cond, UseCase)` it guards the use
case in `check`, `verify`, and the TLA+ export; a body applied to a use case that writes
nothing is `UnbindableCondition`.

- Guard one arm, not the use case, with `transitions(..).when(Cond)`, or one pickable value
  with `chooses(UC, E.col, value).when(Cond)`. Keep one use case; do not split it per value.
  An arm the guard never lets run warns `ArmNeverTaken` (`Arm_<UseCase>_<from>_<to>`); a
  value never chosen warns `ValueNeverChosen`.
- A body that reads a related entity follows the `relate`; name one of two keys with
  `.role(r)` after the body. "No related row" is `.none(Related)`.
- An entity two hops away needs the path:
  `holds(SettlementSigned, Settlement.status == agreed).along(Claim, ClaimCase, Settlement)`.
  Without it the guard reads any row, and `check` warns `UnjoinedGuardRead`.

## Effects

- `sets(UseCase, Entity.col == value)` for an Enum without `transitions`, a `Bool`, a
  `@null` column (`present` or `null`), an Int, or a Date. A `sets` on a column `transitions`
  drives is an error unless the use case creates the entity. A Date takes a clock mark
  (`sets(Issue, Coupon.expires_at == 3)`); a column stamped with the current time is `@null`
  and set to `present`.
- One `sets` per value with `.when(Cond)` lets the row pick:
  `sets(Extend, Extension.due_on == 3).when(ExpertInquiry)`. Keep the guards disjoint; an
  unguarded `sets` beside them is `ConflictingEffects`.
- A variation column takes any value on a created row, whatever its `@default`, unless the
  creating use case `sets` it. An update keeps the value unless the use case lets the actor
  pick again: `chooses(UseCase, Entity.col)`, or one `chooses(UseCase, Entity.col, value)`
  line per offered value, never both forms (`ChoosesMixedForms`). `chooses` and `sets` on
  one column in one use case are an error, and so is `chooses` on a column no variation
  types.
- Copy a value from a row the step binds: `sets(Register, Permit.kind == Application.kind)`.
  The source is an entity the use case writes, or the target's parent through a `relate`;
  any other is `UnboundCopySource`. Both columns hold one set of values
  (`SetsCopyMismatch`).
- Give an existing row a parent with `links(Assign, Case, Handler)` (the key must be
  `.optional()`), move it with `relinks`, drop it with `unlinks`. A use case states one of
  the three per key (`ConflictingLinks`).
- A use case that writes two entities joined only through a third it does not write draws
  `UnjoinedWrite`: also write the entity between, or mark a write `.apart()`.
- `drives(UC, E.col op rhs, true|false)` makes a comparison a Bool when the model does not
  track the values. Prefer Int columns and `sets(UC, E.col == N)` when the quantity matters.

## Example

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
transitions(SubmitOrder, Order.status, draft -> submitted)
transitions(FulfillOrder, Order.status, submitted -> fulfilled)
sets(SubmitOrder, Order.submitted_at == present)
```

An event-started BUC, a timer, and a guard:

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

A deadline that depends on a reason: the reason sets the deadline on the row it creates,
and the timer's use case waits for the clock. The clock counts marks, not days: pick marks
that keep the periods' order (60 days at 2, 90 at 3).

```rdra
variation ExtensionReason "Extension reason" {
  disaster_area "Disaster area (60 days)"
  expert_inquiry "Expert inquiry (90 days)"
}

entity Claim "Claim" {
  id: Int @pk
  period: Enum(running, extended, overdue) @default(running)
}

entity Extension "Extension" {
  id: Int @pk
  reason: ExtensionReason
  due_on: Date
}

relate(Extension, Claim, 1:1)

buc BucClaim "Claim"
usecase FileClaim "File a claim"
usecase ExtendPeriod "Extend the payment period"
usecase FlagOverdue "Flag the claim overdue"
contains(BucClaim, FileClaim)
contains(BucClaim, ExtendPeriod)
contains(BucClaim, FlagOverdue)

creates(FileClaim, Claim)
creates(ExtendPeriod, Extension)
updates(ExtendPeriod, Claim)
transitions(ExtendPeriod, Claim.period, running -> extended)

condition DisasterArea "A disaster-area survey"
holds(DisasterArea, Extension.reason == disaster_area)
condition ExpertInquiry "An expert inquiry"
holds(ExpertInquiry, Extension.reason == expert_inquiry)
sets(ExtendPeriod, Extension.due_on == 2).when(DisasterArea)
sets(ExtendPeriod, Extension.due_on == 3).when(ExpertInquiry)

timer PaymentDeadline "Payment deadline"
triggers(PaymentDeadline, FlagOverdue)
updates(FlagOverdue, Claim)
condition DeadlinePassed "The extended deadline has come"
holds(DeadlinePassed, Extension.due_on <= now)
transitions(FlagOverdue, Claim.period, extended -> overdue).when(DeadlinePassed)

forbidden(Claim, Extension, Claim.period == overdue, Extension.due_on > now).along(Extension, Claim).label("No claim is overdue before its deadline")
```

## Validate

<!-- stage-spec: review lifecycle -->
```sh
rdra-ish check src/
rdra-ish states src/ --entity Order
rdra-ish diagram src/ --kind state --format mermaid --buc BucOrder
rdra-ish diagram src/ --kind event --format mermaid
```
<!-- /stage-spec -->

`check` names what the lifecycle cannot do: `Witness_*` (a state nothing reaches),
`Deadlock_*` (an instance stuck at a state with a way out), `Runs_*` (a use case that never
runs), `Vacuous_*` (a rule that says nothing). Each points at the arm or use case at fault;
the fix is usually a missing `creates`, arm, or `sets`. Read them with
`../../rdra-ish-buc-analyze/references/state-patterns.md`.

## Done when

- Every state change names its use case; every raised event is consumed.
- An event-started BUC has `triggers(Event, Buc)`, and its entry use case is in that BUC.
- `check` reports no `Witness_*` or `Deadlock_*`, and each `Runs_*` is understood.

Next: `references/07-rules.md`, when reachable combinations need rules.
