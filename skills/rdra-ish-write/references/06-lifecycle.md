# Step 6: Lifecycle

Use this context when entity structure exists and the next task is to describe which
use cases move entity state, which events and timers start other work, which
conditions guard a use case, and which columns each use case changes.

## Goal

Make lifecycle causality reviewable. A use case drives every state change, as in the
遷移UC column of an RDRA state sheet. An event is only the interface between use cases,
or between a use case and an external system; a timer starts a use case on a schedule.
`check` then explores the reachable states and reports what the model cannot do.

## Ask For

<!-- stage-spec: asks lifecycle -->
- Which Enum column and variants represent lifecycle state?
- Which use case causes each state change?
- Does any event start another BUC, and is the concrete entry use case known yet?
- Which nullable or boolean fields change together with the lifecycle?
- Are the Enum variants the states you want to see? Diagrams derive one state per variant.
<!-- /stage-spec -->

Also ask:

- Which entity values represent lifecycle state, and which ones does an actor merely
  choose?
- Which use case raises each domain event, and what consumes it?
- Does a schedule start any use case?
- Does a business rule (an RDRA 条件) decide when a use case may run?
- Which nullable, enum, boolean, Int, or comparison values change with each use case?
- Which states are end states, and which must always have a way out?

## Procedure

1. Add lifecycle columns such as `Enum(...)`, `Bool`, and nullable timestamp/value
   columns. Values an actor merely chooses are not lifecycle: type those columns by a
   `variation` and leave them out of `transitions`. A created row takes any value; an
   update keeps it unless the use case lets the actor pick again with
   `chooses(UseCase, Entity.column)`. To offer only some values, write one
   `chooses(UseCase, Entity.column, value)` line per value.
2. Add `transitions(UseCase, Entity.col, from -> to)` for lifecycle movement, naming
   the use case that drives each edge. States are not declared: each variant a
   `transitions` names, and each value a creating use case sets, is a state of the
   state model. An entity may have several lifecycle columns (a procedure `status` and
   a `publication` that moves on its own): give each its own `transitions`, and do not
   split the entity or fake one with guarded `sets`. Each column is its own state model,
   and guards and rules may read any of them. A use case that moves two of them takes one
   arm of each in one step, so it runs only where both columns have an arm: list an arm for
   every state of the other column it should run from.
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
   case. Keep that use case in a BUC. A timer has no parameter. When the deadline depends on
   a reason, declare one timer per period, let each trigger its own use case, and guard
   that use case with a `condition` on the reason (see the third pattern below).
7. Declare a `condition` for a business rule that selects when a use case applies.
   Without a body it is a trace link (`combines` its variation and lifecycle column);
   with `holds(Cond, Entity.col == value, ...)` it guards each use case it
   `applies_to`, and `check` and TLC read the guard. When the rule holds back only one
   outcome of a use case's choice, guard that arm instead:
   `transitions(UseCase, Entity.col, from -> to).when(Cond)`. The other arms stay free.
   When it holds back one value the actor picks, guard that value:
   `chooses(UseCase, Entity.col, value).when(Cond)` beside the unguarded values. Keep
   one use case; do not split it per value. A body that reads a related entity
   (`holds(Renewable, Permit.status == valid)`) follows the `relate` between it and the
   entity the use case writes; when two relates join them, name the one with
   `.role(role)` after the body (`holds(Renewable, Permit.status == valid).role(renews)`). On a
   use case that creates the child of an `.optional()` key, such a guard also links the
   new row to the parent it reads. "No related row" is `.none(Related)`. An entity two
   hops away, such as the settlement of the case a claim belongs to, needs the path:
   `holds(SettlementSigned, Settlement.status == agreed).along(Claim, ClaimCase,
   Settlement)`. Without it the guard reads any settlement, and `check` warns
   `UnjoinedGuardRead` with the `.along(...)` to add.
8. Add `sets(UseCase, Entity.col == val)` for enum, nullable, boolean, Int, and Date
   effects not explained by transitions (`chooses` and `sets` on one column in one use
   case are an error). A `sets` on a column that `transitions`
   drives is an error unless the use case creates the entity. A `Date` or `DateTime`
   column takes a number, a point on the clock `now` ticks through
   (`sets(Issue, Coupon.expires_at == 3)`), so a rule that reads `expires_at < now` has a
   deadline to reach; a column the use case stamps with the current time is `@null` and
   set to `present`.
   When the value comes from another row, copy it: `sets(Register, Permit.kind ==
   Application.kind)`. The source must be a row the step binds: an entity the use case
   writes in the same or an earlier transaction group, or the parent of the target through a `relate`
   (a created child reads the parent it is created under; with two relates to the source,
   name one: `sets(Fix, Permit.kind == Site.kind).role(home)`). Any other source is
   `UnboundCopySource`; give the target a `relate` to the source instead of leaving the
   value a free choice. The two columns hold one set of values (`SetsCopyMismatch`).
9. Add `drives(UC, E.col op rhs, true|false)` when rules depend on a derived comparison and
   the model does not track the values. Prefer Int / Money / Decimal columns plus
   `sets(UC, E.col == N)` when the quantity itself must be checked: a comparison
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
transitions(SubmitOrder, Order.status, draft -> submitted)
transitions(FulfillOrder, Order.status, submitted -> fulfilled)
sets(SubmitOrder, Order.submitted_at == present)
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

A deadline that depends on a reason: one timer per period, each use case guarded by the
reason it serves.

```rdra
variation ExtensionReason "Extension reason" {
  none "No extension"
  disaster_area "Disaster area"
  foreign_survey "Overseas survey"
}

entity Claim "Claim" {
  id: Int @pk
  status: Enum(waiting, overdue) @default(waiting)
  extension: ExtensionReason @default(none)
}

buc BucClaim "Claim"
usecase FileClaim "File a claim"
usecase RecordExtension "Record the extension reason"
usecase FlagOverdue30 "Flag overdue after 30 days"
usecase FlagOverdue60 "Flag overdue after 60 days"
usecase FlagOverdue120 "Flag overdue after 120 days"
contains(BucClaim, FileClaim)
contains(BucClaim, RecordExtension)
contains(BucClaim, FlagOverdue30)
contains(BucClaim, FlagOverdue60)
contains(BucClaim, FlagOverdue120)

creates(FileClaim, Claim)
updates(RecordExtension, Claim)
chooses(RecordExtension, Claim.extension)
transitions(FlagOverdue30, Claim.status, waiting -> overdue)
transitions(FlagOverdue60, Claim.status, waiting -> overdue)
transitions(FlagOverdue120, Claim.status, waiting -> overdue)

timer After30Days "30 days after filing"
timer After60Days "60 days after filing"
timer After120Days "120 days after filing"
triggers(After30Days, FlagOverdue30)
triggers(After60Days, FlagOverdue60)
triggers(After120Days, FlagOverdue120)

condition NoExtension "No extension was recorded"
holds(NoExtension, Claim.extension == none)
condition DisasterArea "A disaster-area extension was recorded"
holds(DisasterArea, Claim.extension == disaster_area)
condition ForeignSurvey "An overseas-survey extension was recorded"
holds(ForeignSurvey, Claim.extension == foreign_survey)
applies_to(NoExtension, FlagOverdue30)
applies_to(DisasterArea, FlagOverdue60)
applies_to(ForeignSurvey, FlagOverdue120)
```

A created row that copies a value from the row it is created under:

```rdra
variation Kind "Kind" {
  food "Food"
  drink "Drink"
}
entity Application "Application" {
  id: Int @pk
  kind: Kind
}
entity Permit "Permit" {
  id: Int @pk
  kind: Kind
}
relate(Permit, Application, N:1)

usecase Apply "Apply"
usecase Register "Register"

creates(Apply, Application)
creates(Register, Permit)
sets(Register, Permit.kind == Application.kind)
```

## Validation

<!-- stage-spec: review lifecycle -->
```sh
rdra-ish check src/
rdra-ish states src/ --entity Order
rdra-ish diagram src/ --kind state --format mermaid --buc BucOrder
rdra-ish diagram src/ --kind event --format mermaid
```
<!-- /stage-spec -->

Also run:

```sh
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
- `sets` captures relevant enum, nullable, boolean, Int, and Date effects, and `drives`
  the comparison effects.
- AsyncAPI export is meaningful when events, raises/triggers/transitions, and
  outbox-like handoffs are part of the review.

## Next Step

Load `references/07-rules.md` when reachable state combinations need
entity constraints.
