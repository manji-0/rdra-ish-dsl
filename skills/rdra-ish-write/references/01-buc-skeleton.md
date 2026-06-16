# Step 1: BUC Skeleton

Use this context when BUC names exist and the next task is to express business value:
who participates, which actions are visible, and which BUC owns each action.

## Goal

Define actor and use-case coverage without committing to data structures, screens,
APIs, or lifecycle internals.

## Ask For

- Who initiates, performs, supports, or receives value from this BUC?
- What user-visible actions compose the BUC?
- Does the business order, branch, exception, or loop need to be reviewed explicitly?
- Are any actions triggered by a system event rather than a human actor?
- Are there known preconditions, guards, postconditions, business errors,
  alternative flows, or compensations for a use case?
- Which actions are intentionally out of scope for this BUC?

## Procedure

1. Declare stable `actor` and `extsystem` vocabulary in `shared/actors.rdra`.
2. In the BUC file, declare the `buc` and candidate `usecase` nodes.
3. Add `performs(Actor, Buc)` for coarse BUC-level participation.
4. Add `contains(Buc, UseCase)` for each visible action.
5. Add `flow` and `step` only when the business sequence itself must be reviewed.
   Connect BUC -> flow -> step with `contains`, then connect ordered steps with
   `precedes`, `branches`, `excepts`, or `repeats`; use `covers(Step, UseCase|Api|Event)`
   to bind flow steps to model behavior.
6. Add `performs(Actor, UseCase)` only when the action-level actor differs or matters
   for later access review.
7. Add `precondition`, `guard`, `postcondition`, or compensation links only when the
   business rule is already known and stable.
8. Leave data CRUD, screens, APIs, permissions, and states for later steps.

## Minimal Pattern

```rdra
actor Customer "Customer"
actor Staff "Staff"

usecase PlaceOrder "Place Order"
usecase CancelOrder "Cancel Order"
flow OrderFlow "Order Flow"
step ReviewCart "Review Cart"
step SubmitOrder "Submit Order"

performs(Customer, BucOrder)
contains(BucOrder, PlaceOrder)
contains(BucOrder, CancelOrder)
contains(BucOrder, OrderFlow)
contains(OrderFlow, ReviewCart)
contains(OrderFlow, SubmitOrder)
precedes(ReviewCart, SubmitOrder)
covers(SubmitOrder, PlaceOrder)
```

## Validation

```sh
rdra-ish check src/
rdra-ish lint src/ --format table
rdra-ish list src/ --kind flow --format table
rdra-ish list src/ --kind step --format table
rdra-ish diagram src/ --kind rdra --format mermaid --buc BucOrder
```

## Achievement Conditions

- Every important BUC has at least one performer or a documented reason for being
  event-started later.
- Every important use case belongs to exactly the intended BUC.
- Business order is explicit with `flow`/`step` when order is part of the
  requirement.
- Use-case names describe business actions, not UI buttons or API endpoints.
- Data and implementation omissions are acceptable and visible as next questions.

## Next Step

Load `references/02-data-touchpoints.md` when reviewers can name the
business objects each use case creates, reads, updates, or deletes.
