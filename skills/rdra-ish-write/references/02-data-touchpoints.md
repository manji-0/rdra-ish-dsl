# Step 2: Data Touchpoints

Use this context when actors and use cases are stable enough to ask which business
objects each action touches.

## Goal

Add coarse entities and CRUD predicates so reviewers can see object responsibility
without forcing final columns, APIs, or persistence design.

## Ask For

<!-- stage-spec: asks data-touchpoints -->
- Which business objects are created, read, updated, or deleted by each use case?
- Which nouns are logical data structures, and which are only business vocabulary?
- Which entities should be shared across multiple BUCs?
<!-- /stage-spec -->

Also ask:

- Which operations are likely to require an API or transaction boundary later?

## Procedure

1. Add coarse `entity` declarations for the logical data structures, usually with
   only an `id: Int @pk` column. Business nouns that do not need persistence stay
   in labels and descriptions; there is no separate conceptual-model kind.
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

<!-- stage-spec: review data-touchpoints -->
```sh
rdra-ish check src/
rdra-ish csv src/ --kind matrix
rdra-ish diagram src/ --kind er --format mermaid --buc BucOrder
```
<!-- /stage-spec -->

Also run:

```sh
rdra-ish lint src/ --format table
rdra-ish list src/ --kind entity --format table
```

## Achievement Conditions

- The CRUD matrix tells a plausible business story.
- Every data-changing use case has at least one entity touchpoint.
- Read-only use cases are intentionally read-only, not accidentally empty.
- Business nouns are not forced into `entity` just because they exist in the
  language of the business.
- Entity names represent logical data structures, not tables designed too early.
- API and system boundaries are either not needed yet or noted for Step 3.

## Next Step

Load `references/03-interaction-boundary.md` when the team needs UI/API
paths, permission/media constraints, or implementation boundary discussion.
