# Step 1: BUC Skeleton

<!-- derived-from ../../../docs/incremental-modeling.md#stage-1-buc-skeleton -->

Use this context when BUC names exist and the next task is to express business value:
who participates, which actions are visible, and which BUC owns each action.

## Goal

Define actor and use-case coverage without committing to data structures, screens,
APIs, or lifecycle internals.

## Ask For

- Who initiates, performs, supports, or receives value from this BUC?
- What user-visible actions compose the BUC?
- Are any actions triggered by a system event rather than a human actor?
- Which actions are intentionally out of scope for this BUC?

## Procedure

1. Declare stable `actor` and `extsystem` vocabulary in `shared/actors.rdra`.
2. In the BUC file, declare the `buc` and candidate `usecase` nodes.
3. Add `performs(Actor, Buc)` for coarse BUC-level participation.
4. Add `contains(Buc, UseCase)` for each visible action.
5. Add `performs(Actor, UseCase)` only when the action-level actor differs or matters
   for later access review.
6. Leave data CRUD, screens, APIs, permissions, and states for later steps.

## Minimal Pattern

```rdra
actor Customer "Customer"
actor Staff "Staff"

usecase PlaceOrder "Place Order"
usecase CancelOrder "Cancel Order"

performs(Customer, BucOrder)
contains(BucOrder, PlaceOrder)
contains(BucOrder, CancelOrder)
```

## Validation

```sh
rdra-ish check src/
rdra-ish diagram src/ --kind rdra --format mermaid --buc BucOrder
```

## Achievement Conditions

- Every important BUC has at least one performer or a documented reason for being
  event-started later.
- Every important use case belongs to exactly the intended BUC.
- Use-case names describe business actions, not UI buttons or API endpoints.
- Data and implementation omissions are acceptable and visible as next questions.

## Next Step

Load `steps/02-data-touchpoints.md` when reviewers can name the
business objects each use case creates, reads, updates, or deletes.
