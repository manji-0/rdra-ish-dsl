# Step 2: Data Touchpoints

Use this context when actors and use cases are stable enough to ask which business
objects each action touches.

## Goal

Add coarse entities and CRUD predicates so reviewers can see object responsibility
without forcing final columns, APIs, or persistence design.

## Ask For

- Which business objects are created, read, updated, or deleted by each use case?
- Which nouns are conceptual only, and which are logical data structures?
- Which entities are shared across BUCs?
- Are aggregates or value objects already known?
- Which operations are likely to require an API or transaction boundary later?

## Procedure

1. Add `concept`, `domain_object`, `aggregate`, or `valueobject` declarations for
   business concepts that should not yet imply database tables.
2. Add coarse `entity` declarations only for logical data structures, usually with
   only an `id: Int @pk` column.
3. Add `maps_to(Concept|DomainObject|Aggregate|ValueObject, Entity)` when the
   conceptual-to-logical mapping is already known.
4. Add direct UC CRUD predicates: `creates`, `reads`, `updates`, `deletes`, or
   `writes`.
5. Prefer direct use-case CRUD here. Delay `api` until the boundary is meaningful.
6. Keep column details, FK relations, `system`, and `coordinates` out unless already
   stable and necessary for review.
7. Use the CRUD matrix to spot overloaded or empty use cases.
8. Do not rely on `business-inputs` yet unless entity columns already exist; at this
   stage it is usually enough to confirm actor/use-case/entity responsibility.

## Minimal Pattern

```rdra
domain_object ShoppingCart "Shopping Cart"

entity Order "Order" {
  id: Int @pk
}

entity Cart "Cart" {
  id: Int @pk
}

creates(PlaceOrder, Order)
updates(PlaceOrder, Cart)
updates(CancelOrder, Order)
maps_to(ShoppingCart, Cart)
```

## Validation

```sh
rdra-ish check src/
rdra-ish lint src/ --format table
rdra-ish list src/ --kind concept --format table
rdra-ish list src/ --kind domain-object --format table
rdra-ish csv src/ --kind matrix
rdra-ish diagram src/ --kind er --format mermaid --buc BucOrder
```

## Achievement Conditions

- The CRUD matrix tells a plausible business story.
- Every data-changing use case has at least one entity touchpoint.
- Read-only use cases are intentionally read-only, not accidentally empty.
- Conceptual/domain nouns are not forced into `entity` just because they exist in
  the language of the business.
- Entity names represent logical data structures, not tables designed too early.
- API and system boundaries are either not needed yet or noted for Step 3.

## Next Step

Load `references/03-interaction-boundary.md` when the team needs UI/API
paths, permission/media constraints, or implementation boundary discussion.
