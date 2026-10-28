# Step 4: Entity Structure

Use this context when coarse entities and interaction boundaries exist, and the next
task is to commit to fields, identifiers, relationships, and cross-system consistency.

## Goal

Turn business objects into reviewable entity structures without losing the API/system
boundaries established in Step 3.

## Ask For

- Which fields identify each entity?
- Which fields are required, nullable, unique, or label-like?
- Which indexes, composite unique constraints, checks, soft-delete/history/tenant
  markers, or derived columns are review-relevant?
- Which relationships connect the entities and with what cardinality?
- Which side should receive the generated FK?
- Do any relations cross derived system boundaries?
- Is explicit `owns(System, Entity)` needed to record future ownership before API
  operations are complete?
- Which API/use case coordinates cross-boundary consistency?

## Procedure

1. Replace coarse entity shells with columns and stable annotations.
2. Use `@pk`, `@pk(a, b)`, `@unique`, `@unique(a, b)`, `@index`,
   `@index(a, b)`, `@check(expr)`, `@null`, `@default(v)`, `@label("...")`,
   `@soft_delete`, `@history`, `@tenant`, and `@derived(expr)` where they
   reflect real constraints. Write `@check` and `@derived` unquoted: `@check(total >= 0)`,
   `@derived(total - discount)`. `@check` becomes an invariant that `check` and `verify`
   test. A composite `@pk(a, b)` is a valid `relate` target: the child gets one key
   column per parent key column (`{role}_{column}`).
3. Type a column by a `variation` when an actor chooses its value (a payment method,
   a channel, a severity): declare `variation PaymentMethod "Payment Method" { cash
   card }` and write `method: PaymentMethod @default(card)`. Keep an inline
   `Enum(...)` for a lifecycle column or a value that use cases set. A variation
   column is an attribute: it adds no `states` axis, and `transitions` rejects it.
   `lint` `enum-attribute` flags an inline Enum that nothing moves.
4. Add `relate(Child, Parent, N:1)`, `relate(A, B, 1:1)`, or the correct
   cardinality. Chain `.optional()`, `.on_delete(...)`, or `.on_update(...)`
   when FK behavior matters. `relate(A, B, 1:1)` puts a unique key column on `A`.
   Write `1..*:1` when every parent must have a child; the lifecycle checks it.
   Chain `.role(name)` when two relates join one child to one parent. Chain
   `.key(col)` to adopt a column the child already declares. Do not hand-write
   a column that `relate` generates: it is `FkConflict` unless you adopt it with `.key`.
5. Re-check API CRUD after entity refinement; APIs should still operate the right
   aggregate or consistency boundary.
6. Add `owns(System, Entity)` only when intended ownership must be visible before
   complete API CRUD exists; otherwise derive ownership from API operations.
7. If a relation crosses two derived system entity sets, add or review
   `coordinates(UseCase, Entity, Entity)` and ensure that use case invokes APIs on
   both sides.
8. Run `business-inputs` to review which non-derived columns are inferred as actor-entered
   fields for each performed use case/API path.
9. Keep lifecycle states and business rules out unless needed to define a field.

## Minimal Pattern

```rdra
entity Order "Order" {
  id: Int @pk
  number: String @unique @index @label("Order No.")
  tenant_id: Int @tenant
  total: Money @check(total >= 0)
  discount: Money @default(0)
  net_total: Money @derived(total - discount)
  submitted_at: DateTime @null
  deleted_at: DateTime @null @soft_delete
}

entity CustomerAccount "Customer Account" {
  id: Int @pk
}

relate(Order, CustomerAccount, N:1).optional().on_delete(set_null).on_update(cascade)
```

Cross-system coordination:

```rdra
coordinates(ChangeOrderCustomer, Order, CustomerAccount)
invokes(ChangeOrderCustomer, OrderApi)
invokes(ChangeOrderCustomer, CustomerApi)
// When OrderSystem is declared in Step 3 but CRUD APIs are not complete yet:
owns(OrderSystem, Order)
```

## Validation

```sh
rdra-ish check src/
rdra-ish lint src/
rdra-ish diagram src/ --kind er --format mermaid --buc BucOrder
rdra-ish diagram src/ --kind sequence --format mermaid --buc BucOrder
rdra-ish diagram src/ --kind business-area --format mermaid --buc BucOrder
rdra-ish diagram src/ --kind technical-area --format mermaid --buc BucOrder
rdra-ish csv src/ --kind api-matrix
rdra-ish csv src/ --kind business-inputs
rdra-ish csv src/ --kind actor-permission-audit
rdra-ish export src/ --kind dbml --out out/schema.dbml
rdra-ish export src/ --kind json-schema --out out/json-schema.json
```

## Achievement Conditions

- Each important entity has stable identifiers and reviewable attributes.
- Relationship direction and cardinality are intentional.
- No duplicate manual FK conflicts with generated FK columns.
- Indexes, uniqueness, checks, FK optionality/actions, history/tenant/derived
  metadata are visible in list/export output when modeled.
- Cross-system relations are coordinated or explicitly marked as unresolved.
- Explicit ownership warnings from `owns` are reviewed.
- Sequence and API matrix still agree with the entity design.
- `business-inputs` rows are plausible, or surprising fields are explained as derived by
  defaults, FK relations, APIs, events, or `sets`.

## Next Step

Load `references/05-lifecycle.md` when fields representing state, events, or
state-changing effects need to be modeled.
