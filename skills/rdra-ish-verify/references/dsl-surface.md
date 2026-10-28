# DSL Surface for Formal Verification

<!-- derived-from ../../../docs/language-reference.md#temporal-path-properties -->
<!-- derived-from ../../../docs/formal-verification.md -->

## Temporal Properties

```rdra
property PaidLeadsToShipped "paid eventually reaches shipped"
  leads_to(Order.status == paid, Order.status == shipped)

property StockOk
  always(Item.stock >= Item.selling)

property EventuallyTerminal
  eventually(Order.status == delivered or Order.status == cancelled)
```

- Label string after the name is **optional**.
- Prefer `and` / `or` / `not` (`/\` `\/` `~` remain aliases).
- Any `eventually` / `leads_to` causes `WF_vars(Next)` in the Spec.
- Temporals are quantified over the present instances
  (`[](\A i: …)`, `\A i: <>…`, `\A i: (p ~> q)`).

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
  `[][(\A p : UC(p) => post')]_vars` (not injected into the effects). `states` checks the
  same predicate after each step of the use case.

UC `precondition` / `postcondition` / `guard` strings are **review metadata only**
(`list --kind usecase`); they are not checked by `states` or TLA. Use
`after(UC).assert(...)` for machine-checked postconditions.

## Quantifiers

```rdra
when(Cert, status == revoked).none(Assign.status == active)
// equivalent: when(Cert.status == revoked).none(Assign.status == active)
```

Prefer qualified columns. Scope uses entity-prefix collection for the first
argument form.

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
- Diagnostic ids may still say `Cross*` even though surface syntax is
  multi-entity `forbidden` / `invariant`.

## Effects That Feed Axes

```rdra
transitions(Order.status, SubmitOrder, draft -> submitted)
sets(Restock, Item, stock == 3)
sets(ReserveStock, Inventory, stock < selling, true)  # drives the comparison as a Bool
```

Use Int / Money / Decimal columns + arithmetic rules when the quantity itself
must be checked under TLC.

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
