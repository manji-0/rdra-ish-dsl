# Step 1: BUC Skeleton

<!-- derived-from ../../../docs/incremental-modeling.md#stage-1-buc-skeleton -->

Say who gets value from each BUC and which visible actions compose it. Commit to no data,
screen, API, or state yet.

## Ask

<!-- stage-spec: asks skeleton -->
- Who initiates or receives value from this BUC?
- What user-visible actions compose the BUC?
- Are any use cases triggered by a system event rather than a human actor?
<!-- /stage-spec -->

Also ask:

- Does the business order, a branch, an exception, or a loop need review?
- Are preconditions, guards, postconditions, or compensations known?
- Which actions are out of scope?

## Do

1. Declare `actor` and `extsystem` in `shared/actors.rdra`.
2. Declare the `buc` and its `usecase`s, each with `contains(Buc, UseCase)`. Name a use
   case for the business action, not a button or an endpoint.
3. Add performers. Use `performs(Actor, Buc)` only for an actor who runs every use case
   of the BUC ([layout.md](layout.md#who-performs-a-use-case)).
4. Add `flow` and `step` only when the order itself needs review: `contains(Buc, Flow)`,
   `contains(Flow, Step)`, then `precedes`, `branches`, `excepts`, or `repeats`, and
   `covers(Step, UseCase|Api|Event|Timer)`.
5. Add `precondition`, `guard`, `postcondition`, and `compensates(UseCase, UseCase)` only
   when stable. The three clauses are review metadata; no check reads them.

```rdra
actor Customer "Customer"
actor Staff "Staff"

buc BucOrder "Process Order"
usecase PlaceOrder "Place Order"
usecase CancelOrder "Cancel Order"
flow OrderFlow "Order Flow"
step ReviewCart "Review Cart"
step SubmitOrder "Submit Order"

performs(Customer, BucOrder)
performs(Staff, CancelOrder)
contains(BucOrder, PlaceOrder)
contains(BucOrder, CancelOrder)
contains(BucOrder, OrderFlow)
contains(OrderFlow, ReviewCart)
contains(OrderFlow, SubmitOrder)
precedes(ReviewCart, SubmitOrder)
covers(SubmitOrder, PlaceOrder)
```

## Validate

<!-- stage-spec: review skeleton -->
```sh
rdra-ish check src/
rdra-ish diagram src/ --kind rdra --format mermaid --buc BucOrder
```
<!-- /stage-spec -->

Also `rdra-ish list src/ --kind flow --format table`.

## Done when

- Each use case has a performer, directly or through its BUC, or is started by an event
  or timer later.
- Each use case belongs to the intended BUC.
- Order is explicit with `flow` / `step` when the requirement depends on it.

Next: `references/02-screens.md`, once the actors and use cases hold.
