# Step 2: Data Touchpoints

Use this context when actors and use cases are stable enough to ask which business
objects each action touches.

## Goal

Add coarse entities and CRUD predicates so reviewers can see object responsibility
without forcing final columns, APIs, or persistence design.

## Ask For

- Which business objects are created, read, updated, or deleted by each use case?
- Are any entities only conceptual at this stage?
- Which entities are shared across BUCs?
- Which operations are likely to require an API or transaction boundary later?

## Procedure

1. Add coarse `entity` declarations, usually with only an `id: Int @pk` column.
2. Add direct UC CRUD predicates: `creates`, `reads`, `updates`, `deletes`, or
   `writes`.
3. Prefer direct use-case CRUD here. Delay `api` until the boundary is meaningful.
4. Keep column details, FK relations, `system`, and `coordinates` out unless already
   stable and necessary for review.
5. Use the CRUD matrix to spot overloaded or empty use cases.
6. Do not rely on `business-inputs` yet unless entity columns already exist; at this
   stage it is usually enough to confirm actor/use-case/entity responsibility.

## Minimal Pattern

```rdra
entity Order "Order" {
  id: Int @pk
}

entity Cart "Cart" {
  id: Int @pk
}

creates(PlaceOrder, Order)
updates(PlaceOrder, Cart)
updates(CancelOrder, Order)
```

## Validation

```sh
rdra-ish check src/
rdra-ish csv src/ --kind matrix
rdra-ish diagram src/ --kind er --format mermaid --buc BucOrder
```

## Achievement Conditions

- The CRUD matrix tells a plausible business story.
- Every data-changing use case has at least one entity touchpoint.
- Read-only use cases are intentionally read-only, not accidentally empty.
- Entity names are business objects, not tables designed too early.
- API and system boundaries are either not needed yet or noted for Step 3.

## Next Step

Load `references/03-interaction-boundary.md` when the team needs UI/API
paths, permission/media constraints, or implementation boundary discussion.
