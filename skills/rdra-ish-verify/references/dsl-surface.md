# The DSL forms verify checks

<!-- derived-from ../../../docs/language-reference.md#temporal-path-properties -->
<!-- derived-from ../../../docs/formal-verification.md -->

This file covers what `verify` checks and how each form reads. Lifecycle effects
(`transitions`, `sets`, copies, `chooses`, `links`, `.when` on an arm or a value, condition
guards) are written as in `../../rdra-ish-write/references/06-lifecycle.md`, and rules as in
`../../rdra-ish-write/references/07-rules.md`.

## Temporal properties

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
  eventually((Order.status == delivered) or (Order.status == cancelled))

usecase CapturePay "Pay"
usecase ShipOrder "Ship"
transitions(CapturePay, Order.status, pending -> paid)
transitions(ShipOrder, Order.status, paid -> shipped)

property ShippedOnceCaptured
  leads_to(Order.status == paid, Order.status == shipped).fair(ShipOrder)
```

- The label after the name is optional. Each atom names its entity (`Order.status`).
- A property holds per present instance: `always x` is `[](present => x)`, `eventually x`
  is `[](present => <>x)`, and `leads_to(x, y)` is `(present /\ x) ~> y`.
- `eventually` and `leads_to` assume `WF_vars(Next)`: some step is taken, not a step in
  particular, so another instance's loop satisfies it.
- `.fair(UC, ...)` on `eventually` or `leads_to` assumes weak fairness per instance on the
  listed use cases. An `ok` then holds only on runs that do not starve them; say so. On any
  other form `.fair` is an error (`FairOnSafetyProperty`, `FairOnReachProperty`).
- A property over two or more entities ranges over every combination of their rows.
  `.along(A, B)` (then `.role(role)` when two keys join the pair) limits it to linked rows;
  `.global()` says every combination is meant. With neither, `lint` reports
  `multi-rule-unscoped`. `samples/temporal_along.rdra` shows the difference.

| Form | Meaning | Decided by |
|---|---|---|
| `always(p)` | `[]p` | native checker; TLC under `--with-tlc` |
| `eventually(p)` | `<>p` | native checker; TLC under `--with-tlc` |
| `leads_to(p, q)` | `p ~> q` | native checker; TLC under `--with-tlc` |
| `stable(p)` | once `p` holds, every step keeps it | BFS on its cone; TLC under `--with-tlc` |
| `unchanged(E.col, ..).when(p)` | no step changes the columns while `p` holds | BFS on its cone; TLC under `--with-tlc` |
| `precedes(p, q)` | `q` holds only in a state where `p` holds or held before | BFS on its cone; TLC under `--with-tlc` |
| `can_reach(p)` | from every reachable state, the instance can still reach `p` (CTL `AG EF`) | BFS on its cone only |

Order and stability, per present instance:

```rdra
entity Order "Order" {
  id: Int @pk
  status: Enum(draft, approved, cancelled, shipped) @default(draft)
  amount: Int @default(0)
  paid: Bool @default(false)
}

property CancelledStaysCancelled
  stable(Order.status == cancelled)

property AmountKeptOnceApproved
  unchanged(Order.amount).when(Order.status in (approved, shipped))

property ShippedAfterPaid
  precedes(Order.paid == true, Order.status == shipped)
```

- `stable` and `unchanged` fail at a step, `precedes` at a state; the trace has no loop.
- `after(UC).assert` reads only the steps of `UC`. `stable` and `unchanged` read every
  step, so they catch another use case that rewrites the column.
- `unchanged` without `.when` means the columns never change while the row is present.

`can_reach` catches a trap a `Deadlock_*` misses, such as an instance that moves between two
states forever once a guard is false:

```rdra
entity Order "Order" {
  id: Int @pk
  status: Enum(pending, on_hold, delivered, cancelled) @default(pending)
}

usecase Place "Place"
usecase Hold "Hold"
usecase Resume "Resume"
usecase Deliver "Deliver"
creates(Place, Order)
transitions(Hold, Order.status, pending -> on_hold)
transitions(Resume, Order.status, on_hold -> pending)
transitions(Deliver, Order.status, pending -> delivered)

property OrderCanFinish "every order can still be delivered or cancelled"
  can_reach(Order.status in (delivered, cancelled))
```

A violation prints a shortest trace, then `confined:` with the values `p` reads in the
states the instance is left with. Deleting the instance does not reach `p`.

## Postconditions

```rdra
after(DeliverOrder).assert(Order.status == delivered, Order.delivered_at == present)
after(Sell).assert(Item.sold >= 1)
```

- Each becomes `AfterAssert_<UC>_<n>`, checked after the last step of each run of the use
  case, on that run's rows. A use case split into transaction groups satisfies it once per
  run; the states between its steps are not checked.
- An atom on an entity no step binds reads every present row (`UnboundAfterRead`).
- `precondition`, `guard`, and `postcondition` clauses on a use case are review text that
  nothing checks. Use `after(UC).assert(...)` for a machine-checked postcondition.

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

invariant(Cert).when(status == revoked).none(Assign.status == active)
```

- The check is `Quantifier_<Entity>_<n>` over the present rows. The `relate` links each
  `Assign` to its `Cert`; without one the related side is independent.
- `.none(Related)` and `.has(Related)` ask for no linked row, or some, and read only the
  link.
- `.at_most(n, ..)`, `.at_least(n, ..)`, and `.exactly(n, ..)` count the same rows.
  `at_most(n)` fails only with `n + 1` rows; below that it is `Vacuous_*`, whose warning
  names the `--instances Entity=k` to run.
- `check` sees one row of each entity. Use `verify` when the rule is about several related
  rows (`samples/quantifier_none.rdra`).

## Multi-entity rules

```rdra
forbidden(Order, Payment, Order.status == cancelled, Payment.status == captured).along(Order, Payment)

invariant(Order, Payment).along(Order, Payment).when(Order.status == paid).then(Payment.status == captured)
```

- Qualify every column as `Entity.column`. The names are `MultiForbidden_<E>_<n>` and
  `MultiInv_<E>_<n>`.
- `.along(...)` needs a `relate` for each adjacent pair, else `UnbindableScope`.
  `.global()` with `.along` is `GlobalWithAlong`.
- When two keys join one pair, name the key on every join with `.role(role)`, else
  `AmbiguousOwnerLink`, which prints the rewrite.
- A `holds` guard that compares an entity no key joins to the use case's rows is
  `UnjoinedGuardRead`: any row satisfies it. Add the `.along(...)` the warning names.

## Null values

A `@null` Enum, Bool, or Int column holds `null` as one more value. A null operand
satisfies no comparison but `!=`, and two null columns are not equal, in every rule form
including `@check` and `property`. Write `col == null or ...` to admit null.
