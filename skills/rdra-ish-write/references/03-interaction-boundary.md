# Step 3: Interaction Boundary

Use this context when data touchpoints exist and the next task is to express how users
or systems reach those actions through screens, APIs, systems, media, and authority.

## Goal

Make the interaction boundary concrete enough to review actor/screen/API/entity paths,
atomic API boundaries, system ownership, media constraints, and actor permission
assignment.

## Ask For

- Which screen or external interface exposes each use case?
- Does the use case write data directly, or through an API boundary?
- Which APIs are reusable consistency boundaries rather than one-off screen actions?
- Which internal system owns each stable API?
- Which permission and operation medium are required by the UC and invoked APIs?
- Which actors should hold those permissions?

## Procedure

1. Add `screen`, `api`, `system`, `medium`, and `permission` vocabulary as needed.
2. Connect use cases with `displays(UseCase, Screen)` and screens with
   `shows(Screen, Entity)` when data exposure matters.
3. Introduce `api` plus `invokes(UseCase, Api)` when backend mediation or atomic
   consistency matters.
4. Move CRUD from the use case to the API when the API owns the entity operation.
5. Use `contains(System, Api)` to group APIs into internal system boundaries.
6. Add `requires_permission` and `requires_medium` to UC/API nodes.
7. Add actor-side `has_permission` where the grant is expected.
8. Use `coordinates(UseCase, Entity, Entity)` only when a relation crosses derived
   system entity sets and the use case coordinates both sides.

## Minimal Pattern

```rdra
screen CheckoutScreen "Checkout"
api OrderApi "Order API"
system OrderSystem "Order System"
permission OrderWrite "Order Write"
medium CustomerDevice "Customer Device"

displays(PlaceOrder, CheckoutScreen)
shows(CheckoutScreen, Order)
invokes(PlaceOrder, OrderApi)
contains(OrderSystem, OrderApi)
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
rdra-ish csv src/ --kind api-matrix
rdra-ish csv src/ --kind screen-constraints
rdra-ish csv src/ --kind permission-callables
rdra-ish csv src/ --kind actor-permission-audit
```

## Achievement Conditions

- Sequence output communicates the intended actor -> screen -> API -> entity path.
- API CRUD represents atomic data operation boundaries.
- Stable APIs belong to systems when system ownership matters.
- Screen constraints are derivable from UC/API requirements; no screen-only authority
  model is needed.
- `actor-permission-audit` rows are explained as `ok`, intentional `missing`, or
  intentional `excess`.
- Cross-system relations either have a coordinating use case or are intentionally
  deferred.

## Next Step

Load `references/04-entity-structure.md` when entity fields, identifiers,
relations, and ownership constraints are ready to become design commitments.
