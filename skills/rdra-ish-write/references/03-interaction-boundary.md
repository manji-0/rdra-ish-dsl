# Step 3: Interaction Boundary

Use this context when data touchpoints exist and the next task is to express how users
or systems reach those actions through screens, APIs, systems, media, and authority.

## Goal

Make the interaction boundary concrete enough to review actor/screen/API/entity paths,
atomic API boundaries, system ownership, media constraints, and actor permission
assignment.

## Ask For

- Which screen or external interface exposes each use case?
- Which fields are actor-entered, readonly, required, or system-derived?
- Does the use case write data directly, or through an API boundary?
- Which APIs are reusable consistency boundaries rather than one-off screen actions?
- Which HTTP method/path, idempotency, sync/async mode, auth scheme, and DTO payloads
  are part of the contract?
- Which internal system owns each stable API?
- Which permission and operation medium are required by the UC and invoked APIs?
- Which actors should hold those permissions?

## Procedure

1. Add `screen`, `field`, `api`, `dto`, `system`, `medium`, and `permission`
   vocabulary as needed.
2. Connect use cases with `displays(UseCase, Screen)` and screens with
   `shows(Screen, Entity)` when data exposure matters.
3. Add `contains(Screen, Field)` and `maps_field(Field, Entity, "column")` when
   screen item mapping matters.
4. Introduce `api` plus `invokes(UseCase, Api)` when backend mediation or atomic
   consistency matters.
5. Put method/path/idempotency/sync-async/auth metadata on the `api`, and connect
   payloads with `request`, `response`, and `error_response`.
6. Move CRUD from the use case to the API when the API owns the entity operation.
7. Use `contains(System, Api)` to group APIs into internal system boundaries.
8. Add `requires_permission` and `requires_medium` to UC/API nodes.
9. Add actor-side `has_permission` where the grant is expected.
10. Use `coordinates(UseCase, Entity, Entity)` only when a relation crosses derived
   system entity sets and the use case coordinates both sides.

## Minimal Pattern

```rdra
screen CheckoutScreen "Checkout"
field OrderIdField "Order ID"
api OrderApi "Order API" method POST path "/orders" idempotency idempotent mode sync auth bearer
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

displays(PlaceOrder, CheckoutScreen)
shows(CheckoutScreen, Order)
contains(CheckoutScreen, OrderIdField)
maps_field(OrderIdField, Order, "id")
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

## Validation

```sh
rdra-ish check src/
rdra-ish diagram src/ --kind sequence --format mermaid --buc BucOrder
rdra-ish list src/ --kind api --format table
rdra-ish list src/ --kind field --format table
rdra-ish csv src/ --kind api-matrix
rdra-ish csv src/ --kind screen-constraints
rdra-ish csv src/ --kind permission-callables
rdra-ish csv src/ --kind actor-permission-audit
rdra-ish csv src/ --kind business-inputs
rdra-ish export src/ --kind openapi --out out/openapi.json
```

## Achievement Conditions

- Sequence output communicates the intended actor -> screen -> API -> entity path.
- Field rows explain actor-entered vs system-derived screen items.
- `maps_field` references real columns; leave a field unmapped until Step 4 when
  the target column is not yet modeled.
- API CRUD represents atomic data operation boundaries.
- OpenAPI export is meaningful when method/path and DTO contracts are modeled.
- Stable APIs belong to systems when system ownership matters.
- Screen constraints are derivable from UC/API requirements; any screen-local
  exception is explicitly justified instead of becoming hidden authority.
- `actor-permission-audit` rows are explained as `ok`, intentional `missing`, or
  intentional `excess`.
- Cross-system relations either have a coordinating use case or are intentionally
  deferred.

## Next Step

Load `references/04-entity-structure.md` when entity fields, identifiers,
relations, and ownership constraints are ready to become design commitments.
