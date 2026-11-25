# Step 4: API and Access Boundary

<!-- derived-from ../../../docs/incremental-modeling.md#stage-4-api-and-access-boundary -->

Say which actions go through an API, which system owns it, and which permission and medium
the work needs. Every part is optional: a use case may keep its direct CRUD. The gate asks
only that every API a use case invokes belongs to a system.

## Ask

<!-- stage-spec: asks api-access-boundary -->
- Does the use case write data directly, or through an API boundary?
- Is the API a reusable boundary or local to this BUC, and which system owns it?
- Which actor permission and physical/device medium are required for the use case or API?
<!-- /stage-spec -->

Also ask which method, path, idempotency, sync or async mode, auth scheme, and DTOs form the
contract, and which actors hold each permission.

## Do

1. Add `api` and `invokes(UseCase, Api)` where backend mediation or atomic consistency
   matters, and move the CRUD from the use case to the API.
2. Split APIs by consistency contract, not by button. A use case runs as one atomic action
   per transaction group, in its `invokes` order, so the split decides what can interleave.
3. Put `method`, `path`, `idempotency`, `mode`, and `auth` on the `api`. Link payloads with
   `request`, `response`, and `error_response`. Map a response field to its column with
   `maps_field(PlaceOrderResponse.order_id, Order.id)`; mapping one field twice is
   `DuplicateFieldMapping`.
4. Group each API in its system with `contains(System, Api)`. A system's entities derive
   from its APIs' CRUD.
5. Put `requires_permission` and `requires_medium` on the use case for the whole
   interaction, or on the API for that backend operation. Never on a screen: screen access
   derives from `displays` and `invokes`.
6. Grant `has_permission` to every actor of the use case
   ([layout.md](layout.md#who-performs-a-use-case)).
7. When a `relate` crosses two systems' entities, add `coordinates(UseCase, Entity,
   Entity)`, and have that use case invoke an API on each side.

```rdra
api OrderApi "Order API"
  method POST
  path "/orders"
  idempotency idempotent
  mode sync
  auth bearer

dto PlaceOrderRequest "Place Order Request" {
  order_id: Int
}

dto PlaceOrderResponse "Place Order Response" {
  order_id: Int
  status: String
}

system OrderSystem "Order System"
permission OrderWrite "Order Write"
medium CustomerDevice "Customer Device"

invokes(PlaceOrder, OrderApi)
contains(OrderSystem, OrderApi)
request(OrderApi, PlaceOrderRequest)
response(OrderApi, PlaceOrderResponse)
creates(OrderApi, Order)
requires_permission(PlaceOrder, OrderWrite)
requires_permission(OrderApi, OrderWrite)
requires_medium(PlaceOrder, CustomerDevice)
has_permission(Customer, OrderWrite)
```

## Validate

<!-- stage-spec: review api-access-boundary -->
```sh
rdra-ish check src/
rdra-ish diagram src/ --kind rdra --format mermaid --buc BucOrder
rdra-ish list src/ --kind api --format table
rdra-ish csv src/ --kind api-matrix
rdra-ish csv src/ --kind screen-constraints
rdra-ish csv src/ --kind actor-permission-audit
```
<!-- /stage-spec -->

Also `rdra-ish check src/ --strict` (a permission gap is an error) and
`rdra-ish export src/ --kind openapi --out out/openapi.json`. Read the CSVs with
`../../rdra-ish-buc-analyze/references/access-permissions.md` and the boundary warnings
with `../../rdra-ish-buc-analyze/references/api-system-boundaries.md`.

## Done when

- The RDRA graph shows the intended actor, screen, API, entity path, with no unexpected
  transaction or API warning from `check`.
- Each invoked API belongs to a system, and its CRUD is one consistency boundary.
- Each `actor-permission-audit` row is `ok`, or an intended `missing` or `excess`.
- Each cross-system relation has a coordinating use case, or is listed as open.

Next: `references/05-entity-structure.md`, once fields, keys, and relations are ready to
commit to.
