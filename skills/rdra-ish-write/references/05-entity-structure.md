# Step 5: Entity Structure

Use this context when coarse entities and the API and access boundary exist, and the next
task is to commit to fields, identifiers, relationships, and cross-system consistency.

## Goal

Turn business objects into reviewable entity structures without losing the API/system
boundaries established in Step 4.

## Ask For

<!-- stage-spec: asks entity-structure -->
- What fields identify each entity?
- Which fields are business state, and which are ordinary data?
- What parent/child relationships should own or group records?
- Does a use case need one transaction across related entities?
<!-- /stage-spec -->

Also ask:

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
   `@soft_delete`, `@history`, `@tenant`, `@derived(expr)`, `@end(v)`,
   `@generated(identity | uuid | ulid)`, `@default(now())`, `@default(uuid())`,
   `@on_update(now())`, and `@version` (an `Int` optimistic-lock column, one per entity)
   where they reflect real constraints. Give a type the storage detail the schema needs:
   `String(255)`, `Decimal(10, 2)`, `DateTime(tz)`; wrong arguments are `InvalidTypeArgs`,
   and a function on a column of the wrong type is `InvalidColumnFunction`. A key unique
   within a tenant is a composite set with the tenant column: `@unique(tenant_id, code)`.
   The lifecycle checks a unique set (and an `N:M` junction's key pair) as
   `Unique_<Entity>_<columns>` when it holds every column's value: an `Enum`, `Bool`, or
   variation column, a compared `Int`, or a key column, read as its parent. A use case that
   can create a second row at the same values fails `check --instance-count 2` and
   `verify`; guard it so the set stays free, such as
   `holds(AmFree, Room.open == true).none(Booking.slot == am)` on the use case that books a
   room's morning. A set with a `String` column, or on a `@soft_delete` entity, is left to
   the database. A `@default` must be one of an `Enum`'s values as declared
   (`@default(open)`, not `Open`) or `true` / `false` on a `Bool`, or the model does not load
   (`InvalidEnumVariant`, `InvalidBoolValue`). An integer `@default` on a column a rule reads
   must lie in `0..--int-range-max`, or `check` fails with
   `IntLiteralOutOfBounds_<Entity>_<value>`.
   Write `entity OrderLine "Order line"` followed by `table "order_lines"` when the physical
   table name differs from the entity id; only the SQL and DBML exports use it. Write `@check` and `@derived` unquoted: `@check(total >= 0)`,
   `@derived(total - discount)`. `@check` becomes the invariant
   `Check_<Entity>_<column>_<n>` that `check` and `verify` test, so a created row must
   satisfy it from the start (give the column a `@default` or a creating `sets`). A
   `@derived` column is not stored state: a `sets` on it is an error, and a rule that
   reads an arithmetic one is `DerivedColumnRead`. A composite `@pk(a, b)` is a valid `relate` target: the child gets one key
   column per parent key column (`{role}_{column}`).
3. Type a column by a `variation` when an actor chooses its value (a payment method,
   a channel, a severity): declare `variation PaymentMethod "Payment Method" { cash
   card }` and write `method: PaymentMethod @default(card)`. Keep an inline
   `Enum(...)` for a lifecycle column or a value that use cases set. A variation
   column is an attribute: it adds no `states` axis, and `transitions` rejects it.
   `lint` `enum-attribute` flags an inline Enum that nothing moves.
4. Add `relate(Child, Parent, N:1)`, `relate(A, B, 1:1)`, or the correct
   cardinality. Chain `.optional()`, `.on_delete(...)`, or `.on_update(...)`
   when FK behavior matters. `relate(A, B, 1:1)` puts a unique key column on `A`, and
   the lifecycle checks it as `Unique_A_<role>` (at two instances, so `verify` finds a
   second child linked to one parent). Write `1..*:1` when every parent must have a
   child; the lifecycle checks it as `MinCard_<Child>_<role>`, so the use case that
   creates the parent must create its first child in the same transaction.
   A `deletes(UC, Parent)` under a key with no `.on_delete`, `restrict`, or `no_action`
   waits until no present child names the row, as the database rejects it otherwise;
   delete the children first. Under `cascade` or `set_null` the lifecycle leaves the
   children, and `DanglingKey_<Child>_<role>` reports them unless a condition such as
   `.none(Child)` guards the deleting use case.
   Chain `.role(name)` when two relates join one child to one parent, or when a second
   relate joins the same two entities the other way. Every rule, guard, quantifier and copy
   that joins such a pair then names its key with `.role(name)` (`AmbiguousOwnerLink`
   otherwise). Chain
   `.key(col)` to adopt a column the child already declares. Do not hand-write
   a column that `relate` generates: it is `FkConflict` unless you adopt it with `.key`.
   Chain `.references(sku)` when the key refers to a parent unique set other than its
   primary key; the named columns must be the parent's `@pk` or a unique set
   (`ReferencesNotUnique` otherwise).
5. Re-check API CRUD after entity refinement; APIs should still operate the right
   aggregate or consistency boundary. When a list or search screen reads many rows, say
   how: `reads(ListOrders, Order).by(customer_id, status).order(created_at.desc).page(cursor)`,
   or `.count()` / `.sum(total)` for a total. A shape that does not fit, such as
   `.page(cursor)` without `.order`, is `InvalidQueryShape`. `lint` `query-index-missing`
   then names the `@index(...)` the read wants, and `export --kind sql` writes the
   suggested indexes and each read's `SELECT` as comments. It also warns at a use case a
   `timer` triggers that finds rows by their state, at a `.has` / `.none` over an
   entity of a million rows or more, and at a guard that compares an entity the use case
   neither writes nor joins by a key (often a missing `.along(...)`). Give an entity `rows 5000` (or `rows "1e6"`) when its
   size is known: under 10,000 rows the warning is skipped, since a scan is cheap.
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
// When OrderSystem is declared in Step 4 but CRUD APIs are not complete yet:
owns(OrderSystem, Order)
```

## Validation

<!-- stage-spec: review entity-structure -->
```sh
rdra-ish check src/
rdra-ish diagram src/ --kind er --format mermaid --buc BucOrder
```
<!-- /stage-spec -->

Also run:

```sh
rdra-ish lint src/
rdra-ish csv src/ --kind api-matrix
rdra-ish csv src/ --kind business-inputs
rdra-ish csv src/ --kind actor-permission-audit
rdra-ish export src/ --kind dbml --out out/schema.dbml
rdra-ish export src/ --kind sql --dialect postgres --out out/schema.sql
rdra-ish export src/ --kind json-schema --out out/json-schema.json
```

## Achievement Conditions

- Each important entity has stable identifiers and reviewable attributes.
- Relationship direction and cardinality are intentional.
- No duplicate manual FK conflicts with generated FK columns.
- Indexes, uniqueness, checks, FK optionality/actions, history/tenant/derived
  metadata are visible in list/export output when modeled. On an entity with a
  `@soft_delete` column, the SQL export makes each declared unique set a partial unique
  index over live rows, so a deleted row frees its value.
- Each declared read shape, each timer-started sweep by state, and each `.has` / `.none`
  over a large entity is served by a key or an `@index`, or `query-index-missing` is
  reviewed. Small and large entities carry a `rows` estimate.
- Cross-system relations are coordinated or explicitly marked as unresolved.
- Explicit ownership warnings from `owns` are reviewed.
- The API matrix and the transaction warnings of `check` still agree with the entity
  design.
- `business-inputs` rows are plausible, or surprising fields are explained as derived by
  defaults, FK relations, APIs, events, or `sets`.

## Next Step

Load `references/06-lifecycle.md` when fields representing state, events, or
state-changing effects need to be modeled.
