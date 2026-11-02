# Step 4: API and Access Boundary

Use this context when screens and data touchpoints exist and the next task is to express
which actions go through an API, which system owns it, and which media and authority they
need.

## Goal

Make the system boundary concrete enough to review actor/screen/API/entity paths,
atomic API boundaries, system ownership, media constraints, and actor permission
assignment. Every part of it is optional: a use case can keep its direct CRUD, and a
model with no `permission` or `medium` has nothing to assign. The gate asks only that
every API a use case invokes belongs to a system.

## Ask For

<!-- stage-spec: asks api-access-boundary -->
- Does the use case write data directly, or through an API boundary?
- Is the API a reusable boundary or local to this BUC, and which system owns it?
- Which actor permission and physical/device medium are required for the use case or API?
<!-- /stage-spec -->

Also ask:

- Which HTTP method/path, idempotency, sync/async mode, auth scheme, and DTO payloads
  are part of the contract?
- Which internal system owns each stable API?
- Which actors should hold those permissions?

## Procedure

1. Add `api`, `dto`, `system`, `medium`, and `permission` vocabulary as needed.
2. Introduce `api` plus `invokes(UseCase, Api)` when backend mediation or atomic
   consistency matters.
3. Put method/path/idempotency/sync-async/auth metadata on the `api`, and connect
   payloads with `request`, `response`, and `error_response`.
4. Move CRUD from the use case to the API when the API owns the entity operation.
5. Use `contains(System, Api)` to group each API into the internal system that owns it.
6. Add `requires_permission` and `requires_medium` to UC/API nodes.
7. Add actor-side `has_permission` where the grant is expected. Every actor of a use
   case needs it: the direct performers and the performers of each BUC that contains
   the use case.
8. Use `coordinates(UseCase, Entity, Entity)` only when a relation crosses derived
   system entity sets and the use case coordinates both sides.

## Minimal Pattern

```rdra
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

Also run:

```sh
rdra-ish check src/ --strict   # permission gaps become errors
rdra-ish csv src/ --kind permission-callables
rdra-ish csv src/ --kind business-inputs
rdra-ish export src/ --kind openapi --out out/openapi.json
```

## Achievement Conditions

- The RDRA graph shows the intended actor -> screen -> API -> entity path, and `check`
  prints no unexpected transaction or API warning.
- API CRUD represents atomic data operation boundaries.
- OpenAPI export is meaningful when method/path and DTO contracts are modeled.
- Every API a use case invokes belongs to a system.
- Screen constraints are derivable from UC/API requirements; any screen-local
  exception is explicitly justified instead of becoming hidden authority.
- `actor-permission-audit` rows are explained as `ok`, intentional `missing`, or
  intentional `excess`.
- Cross-system relations either have a coordinating use case or are intentionally
  deferred.

## Next Step

Load `references/05-entity-structure.md` when entity fields, identifiers,
relations, and ownership constraints are ready to become design commitments.
