# Step 3: Data Touchpoints

<!-- derived-from ../../../docs/incremental-modeling.md#stage-3-data-touchpoints -->

Add coarse entities, CRUD, and what each screen shows, so reviewers see which object each
action is responsible for. Commit to no columns, APIs, or persistence design yet.

## Ask

<!-- stage-spec: asks data-touchpoints -->
- Which business objects are created, read, updated, or deleted by each use case?
- Which entities does each screen show?
- Which nouns are logical data structures, and which are only business vocabulary?
- Which entities should be shared across multiple BUCs?
<!-- /stage-spec -->

Also ask which operations will need an API or a transaction boundary later.

## Do

1. Declare an `entity` per logical data structure, usually `id: Int @pk` only. A noun
   nothing stores or exchanges stays in labels.
2. Add `creates`, `reads`, `updates`, or `deletes` on the use case. Add an `api` later.
3. Add `shows(Screen, Entity)`, and `maps_field(Field, Entity.column)` where the column
   exists.
4. When two objects are linked twice (a renewal and the permit it renews, beside the permit
   and the application it was granted on), note both links: each later `relate` takes a
   `.role(...)`.

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
shows(CheckoutScreen, Order)
shows(CartScreen, Cart)
```

## Validate

<!-- stage-spec: review data-touchpoints -->
```sh
rdra-ish check src/
rdra-ish csv src/ --kind matrix
rdra-ish diagram src/ --kind er --format mermaid --buc BucOrder
```
<!-- /stage-spec -->

## Done when

- The CRUD matrix tells a plausible story: each data-changing use case touches an entity,
  and a read-only one is read-only on purpose.
- Each screen shows what the actor sees there, or is data-free on purpose.
- Entities are logical structures, not tables designed early.

Next: `references/04-api-access-boundary.md`, when the team needs API paths, permissions,
or media.
