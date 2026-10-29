# DSL Surface for Formal Verification

<!-- derived-from ../../../docs/language-reference.md#temporal-path-properties -->
<!-- derived-from ../../../docs/formal-verification.md -->

## Temporal Properties

```rdra
entity Order "Order" {
  id: Int @pk
  status: Enum(pending, paid, shipped, delivered, cancelled) @default(pending)
}
entity Item "Item" {
  id: Int @pk
  stock: Int @default(3)
  selling: Int @default(1)
}

property PaidLeadsToShipped "paid eventually reaches shipped"
  leads_to(Order.status == paid, Order.status == shipped)

property StockOk
  always(Item.stock >= Item.selling)

property EventuallyTerminal
  eventually(Order.status == delivered or Order.status == cancelled)
```

- Label string after the name is **optional**.
- Each atom names its entity (`Order.status`); an atom whose column no single entity
  owns is `UnresolvedPropertyAtom`.
- Prefer `and` / `or` / `not` (`/\` `\/` `~` remain aliases).
- Any `eventually` / `leads_to` causes `WF_vars(Next)` in the Spec.
- A property holds per present instance: `always x` is `[](present => x)`,
  `eventually x` is `[](present => <>x)`, and `leads_to x y` is `(present /\ x) ~> y`.
- Only `verify` evaluates properties, each in its own TLC run on the whole model;
  `check` prints a `note:` that it did not.

| Form | TLA+ |
|---|---|
| `always(expr)` | `[]expr` |
| `eventually(expr)` | `<>expr` |
| `leads_to(p, q)` | `p ~> q` |

## Postconditions

```rdra
after(DeliverOrder).assert(Order.status == delivered, Order.delivered_at == present)
after(Sell).assert(Item.sold >= 1)
```

- Equality and comparison forms become an independent TLA `PROPERTY`
  `AfterAssert_<UC>_<n>`, `[][(\A p : UC(p) => post')]_vars` (not injected into the
  effects). `check` and `states` check the same predicate after each step of the use
  case, so a use case split into k transaction groups must satisfy it after each group.
- An `after` on a use case that changes nothing is `AssertOnInertUseCase`.

UC `precondition` / `postcondition` / `guard` strings are **review metadata only**
(`list --kind usecase`); they are not checked by `states` or TLA. Use
`after(UC).assert(...)` for machine-checked postconditions.

## Quantifiers

```rdra
entity Cert "Certificate" {
  id: Int @pk
  status: Enum(active, revoked) @default(active)
}
entity Assign "Assignment" {
  id: Int @pk
  status: Enum(active, ended) @default(active)
}
relate(Assign, Cert, N:1)

when(Cert, status == revoked).none(Assign.status == active)
// equivalent: when(Cert.status == revoked).none(Assign.status == active)
```

Prefer qualified columns. The check is `Quantifier_<Entity>_<n>` over the present
instances; the `relate` links each Assign to its Cert, and without one the related side
is independent. `check` sees one instance of each entity, so use `verify` when the rule
is about several related rows (`samples/quantifier_none.rdra`).

## Multi-entity Rules

```rdra
forbidden(Order, Payment,
  Order.status == cancelled,
  Payment.status == captured)
  .along(Order, Payment)

invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status == paid)
  .then(Payment.status == captured)
```

- Qualify columns as `Entity.column`.
- Add `.along(...)` only for relation-scoped linked-instance intent. Each adjacent pair
  needs a `relate` (it supplies `Child_owner_Parent`), else `UnbindableScope`.
- Two relates between the same pair of entities (distinct `.role`s) need `.by(role)` on
  the rule: `.along(Order, Address).by(ship_to)`. Without it the rule is
  `AmbiguousOwnerLink`.
- Diagnostic ids may still say `Cross*` even though surface syntax is
  multi-entity `forbidden` / `invariant`.

## Effects That Feed Axes

```rdra
transitions(Order.status, SubmitOrder, draft -> submitted)
sets(Restock, Item, stock == 3)
sets(ReserveStock, Inventory, stock < selling, true)  // drives the comparison as a Bool
```

Use Int / Money / Decimal columns and `sets(UC, E, col == N)` when the quantity itself
must be checked; `check` and TLC both evaluate a comparison on the values over
`0..--int-range-max`. Drive the comparison with the four-argument `sets` only when the
model does not track the values.

## Variation columns

A column typed by a `variation` (`method: PaymentMethod`) is an attribute, not a
lifecycle. `states` and the TLA+ export give it a variable only when a `sets`, an
entity-local or multi-entity constraint, a condition body, an `after(UC).assert`, or a
`property` names it. A created instance takes any value unless the creating use case sets
it, and `@default` does not pin the choice, so TLC checks the rule against each value.
A `transitions` on it is an error.

## Conditions

A `condition` with a `holds` body that `applies_to` a use case (or an API that the use case
invokes) is a guard on that use case in `states`, `check`, and the TLA+ export. TLC checks
it. A body applied to a use case that writes nothing is `UnbindableCondition`. A condition
with only `combines` is a trace link and guards nothing.
