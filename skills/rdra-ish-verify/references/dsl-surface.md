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
  eventually((Order.status == delivered) or (Order.status == cancelled))

usecase CapturePay "Pay"
usecase ShipOrder "Ship"
transitions(CapturePay, Order.status, pending -> paid)
transitions(ShipOrder, Order.status, paid -> shipped)

property ShippedOnceCaptured
  leads_to(Order.status == paid, Order.status == shipped).fair(ShipOrder)
```

- Label string after the name is **optional**.
- Each atom names its entity (`Order.status`); an atom whose column no single entity
  owns is the model error `UnresolvedPropertyColumn`, which `check` reports.
- Prefer `and` / `or` / `not` (`/\` `\/` `~` remain aliases).
- Any `eventually` / `leads_to` causes `WF_vars(Next)` in the Spec: some step is taken,
  not a step in particular, so another instance's loop satisfies it.
- `.fair(UC, ...)` after an `eventually` / `leads_to` assumes each listed use case is weakly
  fair per instance it acts on: `Fair_P == \A p0 \in E_Ids : WF_vars(UC(p0))` (a created
  slot bound inside with `\E`), and `P == Fair_P => (...)`. `Spec` is unchanged. On `always`
  it is `FairOnSafetyProperty`; no argument, a non-use-case argument, a duplicate, or a second
  `.fair` is `FairRequiresUseCases`; a use case that changes nothing is `FairOnInertUseCase`.
- A property holds per present instance: `always x` is `[](present => x)`,
  `eventually x` is `[](present => <>x)`, and `leads_to x y` is `(present /\ x) ~> y`.
- A property whose atoms read two or more entities ranges over every combination of their
  rows. `.along(A, B, ..)` (with `.role(role)` after it) limits it to linked rows, as on a
  multi-entity `invariant`: the owner links join `present` in the trigger, so
  `always x` is `[](present /\ link => x)`. Write `.global()` when every combination is
  meant. Neither, with keys relating the entities, draws `lint` `multi-rule-unscoped`;
  `.global()` with `.along` is `GlobalWithAlong`; a path that misses an atom's entity or a
  hop no key binds is `UnbindableScope` (example below the table).
- Only `verify` evaluates properties. `check` and `states` verify safety only, by state
  exploration, and evaluate no `property`; `check` prints a `note:` that it did not.
  `verify` checks `always`, `eventually`, and `leads_to` each in its own TLC run on its cone
  (the whole model only when the cone does not decide it).
- `can_reach(expr)` says the actors can still finish each present instance: from every
  reachable state, some path on which the instance stays present reaches `expr`. It is CTL
  `AG (present => EF expr)`, which TLC cannot check, so `verify` decides it by BFS on its
  cone and `export --kind tla` only names it in a comment. Use it for a trap a
  `Deadlock_*` misses, such as an instance that moves between two states forever once a
  guard is false. A violation prints a shortest trace, then `confined:` with what `expr`
  reads in the states the instance is left with. Deleting the instance does not reach
  `expr`. `.along(..)` joins its link to the trigger, as for the other forms.
  `.fair(...)` on it is `FairOnReachProperty`.

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

| Form | TLA+ |
|---|---|
| `always(expr)` | `[]expr` |
| `eventually(expr)` | `<>expr` |
| `leads_to(p, q)` | `p ~> q` |
| `can_reach(expr)` | none (CTL `AG (present => EF expr)`), decided by BFS |

A property over a Payment and the Order it belongs to:

```rdra
entity Order "Order" {
  id: Int @pk
  status: Enum(draft, paid) @default(draft)
}

entity Payment "Payment" {
  id: Int @pk
  status: Enum(pending, captured) @default(pending)
}

relate(Payment, Order, N:1)

property CapturedPaid
  always((not (Payment.status == captured)) or (Order.status == paid)).along(Payment, Order)
```

## Postconditions

```rdra
after(DeliverOrder).assert(Order.status == delivered, Order.delivered_at == present)
after(Sell).assert(Item.sold >= 1)
```

- Equality and comparison forms become an independent TLA `PROPERTY`
  `AfterAssert_<UC>_<n>`, `[][(\A p : UC(p) => post')]_vars` (not injected into the
  effects). `check` and `states` check the same predicate after the last step of the use
  case, so a use case split into k transaction groups must satisfy it once per run, after
  step k, on that run's rows; the states between two steps are not checked. An atom on
  an entity no step binds is read on every present row (`UnboundAfterRead`).
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

invariant(Cert).when(status == revoked).none(Assign.status == active)

// equivalent: invariant(Cert, Assign).when(Cert.status == revoked).none(Assign.status == active)
```

Prefer qualified columns. The check is `Quantifier_<Entity>_<n>` over the present
instances; the `relate` links each Assign to its Cert, and without one the related side
is independent. `check` sees one instance of each entity, so use `verify` when the rule
is about several related rows (`samples/quantifier_none.rdra`).

## Multi-entity Rules

```rdra
forbidden(Order, Payment, Order.status == cancelled, Payment.status == captured).along(Order, Payment)

invariant(Order, Payment).along(Order, Payment).when(Order.status == paid).then(Payment.status == captured)
```

- Qualify columns as `Entity.column`.
- Add `.along(...)` only for relation-scoped linked-instance intent. Each adjacent pair
  needs a `relate` (it supplies `Child_owner_Parent`), else `UnbindableScope`.
- A rule over every combination of rows says `.global()`; with neither `.along` nor
  `.global()`, `lint` `multi-rule-unscoped` warns and names the `.along(...)` the shortest
  key path gives. `.global()` with `.along` is `GlobalWithAlong`, an error.
- Two relates between the same pair of entities (distinct `.role`s, or one each way) need
  `.role(role)` on every join of the pair: `.along(Order, Address).role(ship_to)`,
  `.none(Address).role(bill_to)`, `holds(C, Address.status == open).role(ship_to)`, and a copy's
  `sets(...).role(ship_to)`. Without it the join is `AmbiguousOwnerLink`, which names the
  candidates and the rewrite. `.role(Address)` names a key without `.role` by its parent id.
  A use case that writes the `Address` and the `Order` puts the written address on the keys
  its guards' or copies' `.role` name, and on neither of two keys without one.
- `.none(Related)` / `.has(Related)` ask for no linked row, or some, and read only the link.
- A `holds` guard reads an entity two hops from the use case's row through
  `.along(A, B, C)`, with `.role(role)` after it per hop. A compared entity that no key joins
  to the use case's rows is `UnjoinedGuardRead` (warning): any row of it satisfies the guard.
- A use case that writes two entities a key path joins only through an entity it does not
  write is `UnjoinedWrite` (warning): two instances can write rows of different rows between.
  Write the entity between, or put `.apart()` on the write meant as a row of its own.
- A multi-entity `forbidden` / `invariant` is named `MultiForbidden_<E>_<n>` / `MultiInv_<E>_<n>`.

## Effects That Feed Axes

```rdra
transitions(SubmitOrder, Order.status, draft -> submitted)
sets(Restock, Item.stock == 3)
drives(ReserveStock, Inventory.stock < selling, true) // drives the comparison as a Bool
```

Several `transitions` of one use case from one state are the actor's choice: one run takes
one arm, every arm carries the use case's other effects, and its guards and
`after(UC).assert` apply to each arm. Two values `sets` writes to one column in one use case
are `ConflictingEffects`, not a choice.

`sets(Register, Permit.kind == Application.kind)` copies the value of the row the step
binds: a row of `Application` the use case writes in the same step, or the parent the permit
belongs to through a `relate`. BFS and TLC read it the same way; TLA+ writes it as a choice of
the column's value and the guard `Permit_kind'[p] = Application_kind[q]`. A source the step
does not bind is `UnboundCopySource`.

`.when(Cond)` on one `transitions` arm guards that arm only: the arm is taken only while the
condition's `holds` body holds, the use case runs when some arm is enabled, and the choice
among enabled arms stays free. A guarded arm that never fires while its use case runs is the
`ArmNeverTaken` warning (`Arm_<UseCase>_<from>_<to>`).

```rdra
entity Disclosure "届出" {
  id: Int @pk
  status: Enum(searched, file, secret) @default(searched)
  acquired: Bool @default(false)
}

usecase Submit "届出"
usecase Acquire "承継"
usecase Decide "判断"
creates(Submit, Disclosure)
sets(Acquire, Disclosure.acquired == true)
condition Acquired "権利を取得している"
holds(Acquired, Disclosure.acquired == true)
transitions(Decide, Disclosure.status, searched -> file).when(Acquired)
transitions(Decide, Disclosure.status, searched -> secret)
forbidden(Disclosure, status == file, acquired == false)
```

Use Int / Money / Decimal columns and `sets(UC, E.col == N)` when the quantity itself
must be checked; `check` and TLC both evaluate a comparison on the values over
`0..--int-range-max`. Drive the comparison with `drives(UC, E.col op rhs, true|false)` only
when the model does not track the values.

## Variation columns

A column typed by a `variation` (`method: PaymentMethod`) is an attribute, not a
lifecycle. `states` and the TLA+ export give it a variable only when a `sets`, an
entity-local or multi-entity constraint, a condition body, an `after(UC).assert`, or a
`property` names it. A created instance takes any value unless the creating use case sets
it, and `@default` does not pin the choice, so TLC checks the rule against each value.
An update keeps the value unless the use case `chooses` it, which again takes every value,
or only the values its `chooses(UC, E.col, value)` lines list. A `transitions` on it is an
error, and so is `chooses` on any other column.

```rdra
variation Scope "補正の範囲" {
  limited "限定"
  free "限定なし"
}

entity Notice "通知" {
  id: Int @pk
  status: Enum(open, drafted) @default(open)
  scope: Scope
}

usecase Receive "受領"
usecase Draft "応答案の作成"
creates(Receive, Notice)
sets(Receive, Notice.scope == limited)
transitions(Draft, Notice.status, open -> drafted)
chooses(Draft, Notice.scope)
```

`.when(Cond)` on a `chooses(UC, E.col, value)` line offers that value only while the
condition's `holds` body holds, read on the state the use case starts from, as an arm guard
is; the use case runs when some listed value is enabled. One use case and column takes
either the value-less line or value lines (`ChoosesMixedForms`). A guarded value never
chosen while its use case runs is the `ValueNeverChosen` warning
(`Choice_<UseCase>_<Entity>_<column>_<value>`).

```rdra
variation Scope "補正の範囲" {
  limited "限定"
  free "限定なし"
}

entity Notice "通知" {
  id: Int @pk
  status: Enum(open, drafted) @default(open)
  final: Bool @default(false)
  scope: Scope
}

usecase Receive "受領"
usecase Draft "応答案の作成"
usecase Reopen "最後の通知"
creates(Receive, Notice)
sets(Receive, Notice.scope == limited)
transitions(Draft, Notice.status, open -> drafted)
transitions(Reopen, Notice.status, drafted -> open)
sets(Reopen, Notice.final == true)
sets(Reopen, Notice.scope == limited)
condition NotFinal "最後の通知でない"
holds(NotFinal, Notice.final == false)
chooses(Draft, Notice.scope, free).when(NotFinal)
chooses(Draft, Notice.scope, limited)
forbidden(Notice, final == true, scope == free)
```

## Conditions

A `condition` with a `holds` body that `applies_to` a use case (or an API that the use case
invokes) is a guard on that use case in `states`, `check`, and the TLA+ export. TLC checks
it. A body applied to a use case that writes nothing is `UnbindableCondition`. A condition
with only `combines` is a trace link and guards nothing.
