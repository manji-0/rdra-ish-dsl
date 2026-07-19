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
- In multi-instance export, temporals are quantified per binder
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
  `[][raised SpecActions => primed posts]_vars` (not injected into SpecAction
  effects). Comparison forms prefer Int arithmetic when axes exist (cross-entity
  RHS ok); otherwise require a proposition axis `TRUE` after the action.

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
- Add `.along(...)` only for relation-scoped linked-instance intent (`relate`
  supplies `*_owner` in TLA export).
- Diagnostic ids may still say `Cross*` even though surface syntax is
  multi-entity `forbidden` / `invariant`.

## Effects That Feed Axes

```rdra
transitions(Order.status, OrderSubmitted, draft -> submitted)
sets(Restock, Item, stock == 3)
sets(ReserveStock, Inventory, stock < selling, true)  # BFS proposition layer
```

Use Int / Money / Decimal columns + arithmetic rules when the quantity itself
must be checked under TLC.
