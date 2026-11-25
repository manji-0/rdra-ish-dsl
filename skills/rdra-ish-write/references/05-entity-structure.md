# Step 5: Entity Structure

<!-- derived-from ../../../docs/incremental-modeling.md#stage-5-entity-structure -->

Turn coarse entities into columns, keys, and relations, keeping the system boundaries of
step 4.

## Ask

<!-- stage-spec: asks entity-structure -->
- What fields identify each entity?
- Which fields are business state, and which are ordinary data?
- What parent/child relationships should own or group records?
- Does a use case need one transaction across related entities?
<!-- /stage-spec -->

Also ask which fields are required, nullable, unique, or indexed, the cardinality of each
relation, and whether a relation crosses systems.

## Do

1. Add columns and the annotations that reflect real constraints:

   | Annotation | Meaning |
   |---|---|
   | `@pk`, `@pk(a, b)` | Primary key; a composite one is a valid `relate` target |
   | `@unique`, `@unique(a, b)` | Unique set, checked as `Unique_<Entity>_<columns>` (below) |
   | `@index`, `@index(a, b)` | Index for a read shape |
   | `@check(expr)` | Invariant `Check_<Entity>_<column>_<n>`; a created row must meet it, so give a `@default` or a creating `sets` |
   | `@null`, `@default(v)` | Nullable; start value, one of the column's values |
   | `@derived(expr)` | Not stored; a `sets` on it is an error, and a rule that reads an arithmetic one is `DerivedColumnRead` |
   | `@end(v)` | An end state: an instance may stay there |
   | `@label("..")`, `@soft_delete`, `@history`, `@tenant` | Display name, soft delete, history, tenant |
   | `@generated(identity\|uuid\|ulid)`, `@default(now())`, `@on_update(now())`, `@version` | Storage detail; `@version` is one `Int` optimistic-lock column |

   Give a type the storage detail the schema needs: `String(255)`, `Decimal(10, 2)`,
   `DateTime(tz)` (`InvalidTypeArgs` otherwise); a function on a column of the wrong type is
   `InvalidColumnFunction`. A key unique within a tenant is `@unique(tenant_id, code)`.
   Write `@check` and `@derived` unquoted.
   An integer `@default` on a column a rule reads lies in `0..--int-range-max`, or
   `check` fails `IntLiteralOutOfBounds_<Entity>_<value>`. Write `table "order_lines"`
   under the entity when the table name differs; only the SQL and DBML exports read it.
2. Type a column by a `variation` when an actor chooses its value:
   `method: PaymentMethod`. Keep an inline `Enum` for a lifecycle column. A variation
   column adds no state axis, and `transitions` rejects it; `lint` `enum-attribute` flags an
   inline Enum nothing moves.
3. Add `relate` with its cardinality ([syntax.md](syntax.md#predicates)). The lifecycle
   checks each key:
   - `1:1`: `Unique_<Child>_<role>`, which breaks only at two instances (`verify`).
   - `1..*:1` (every parent has a child): `MinCard_<Child>_<role>`, so the use case that
     creates the parent creates its first child in the same transaction.
   - `0..3:1` (at most three): `MaxCard_<Child>_<role>`; guard the use case that adds a
     child with a count such as `.at_most(2, Loan)`.
   - A key with no `.on_delete`, or `restrict` / `no_action`: `deletes(UC, Parent)` waits
     until no child names the row, and `DanglingKey_<Child>_<role>` checks it. `cascade`,
     `set_null`, and `set_default` are not run yet: the delete leaves the children.
   - Two relates between one pair: chain `.role(name)` on each (`DuplicateRelateRole` when
     they clash), and name it on every rule, guard, and copy that joins the pair
     (`AmbiguousOwnerLink` otherwise).
   - `.references(cols)` names a parent unique set other than the primary key
     (`ReferencesNotUnique` when it is not one).
4. A unique set is checked when the lifecycle holds every column (an Enum, Bool, variation,
   compared Int, or key), and needs two instances to break. Guard the use case that writes
   it, such as `holds(AmFree, Room.open == true).none(Booking.slot == am)`. A set with a
   `String` column, or on a `@soft_delete` entity, is left to the database.
5. Declare each list or search read's shape:
   `reads(ListOrders, Order).by(customer_id, status).order(created_at.desc).page(cursor)`, or
   `.count()` / `.sum(total)`. A shape that does not fit is `InvalidQueryShape`.
   `lint` `query-index-missing` names the `@index` a read, a timer's sweep by state, a
   `.has` / `.none` over a large entity, or an unjoined guard read wants. Give an entity
   `rows 5000` when its size is known; under 10,000 rows the warning is skipped.
6. Add `owns(System, Entity)` only to show intended ownership before the API CRUD exists.
7. Check `csv --kind business-inputs`: the columns each actor path is inferred to enter.

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

## Validate

<!-- stage-spec: review entity-structure -->
```sh
rdra-ish check src/
rdra-ish diagram src/ --kind er --format mermaid --buc BucOrder
```
<!-- /stage-spec -->

Also `rdra-ish lint src/`, `rdra-ish csv src/ --kind business-inputs`, and
`rdra-ish export src/ --kind sql --dialect postgres --out out/schema.sql`. On a
`@soft_delete` entity the SQL export makes each unique set a partial index over live rows.

## Done when

- Each entity has stable keys and reviewable columns; direction and cardinality are
  intended; no hand-written key conflicts with a generated one.
- Each declared read is served by a key or an `@index`, or `query-index-missing` is
  reviewed.
- Ownership warnings from `owns` and cross-system relations are reviewed.
- `business-inputs` rows are plausible, or each surprise is explained.

Next: `references/06-lifecycle.md`, when state columns, events, or effects need modeling.
