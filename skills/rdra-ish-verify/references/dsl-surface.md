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

usecase CapturePay "Pay"
usecase ShipOrder "Ship"
transitions(CapturePay, Order.status, pending -> paid)
transitions(ShipOrder, Order.status, paid -> shipped)

property ShippedOnceCaptured
  leads_to(Order.status == paid, Order.status == shipped).fair(ShipOrder)
```

- Label string after the name is **optional**.
- Each atom names its entity (`Order.status`); an atom whose column no single entity
  owns is `UnresolvedPropertyAtom`.
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
- Only `verify` evaluates properties, each in its own TLC run on its cone (the whole model
  only when the cone does not decide it); `check` prints a `note:` that it did not.

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

invariant(Cert).when(status == revoked).none(Assign.status == active)
// equivalent: invariant(Cert, Assign).when(Cert.status == revoked).none(Assign.status == active)
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
- Two relates between the same pair of entities (distinct `.role`s, or one each way) need
  `.by(role)` on every join of the pair: `.along(Order, Address).by(ship_to)`,
  `.none(Address).by(bill_to)`, `holds(C, Address.status == open).by(ship_to)`, and a copy's
  `sets(...).by(ship_to)`. Without it the join is `AmbiguousOwnerLink`, which names the
  candidates and the rewrite. `.by(Address)` names a key without `.role` by its parent id.
- `.none(Related)` / `.has(Related)` ask for no linked row, or some, and read only the link.
- A `holds` guard reads an entity two hops from the use case's row through
  `.along(A, B, C)`, with `.by(role)` after it per hop. A compared entity that no key joins
  to the use case's rows is `UnjoinedGuardRead` (warning): any row of it satisfies the guard.
- Diagnostic ids may still say `Cross*` even though surface syntax is
  multi-entity `forbidden` / `invariant`.

## Effects That Feed Axes

```rdra
transitions(SubmitOrder, Order.status, draft -> submitted)
sets(Restock, Item, stock == 3)
drives(ReserveStock, Inventory, stock < selling, true)  // drives the comparison as a Bool
```

Several `transitions` of one use case from one state are the actor's choice: one run takes
one arm, every arm carries the use case's other effects, and its guards and
`after(UC).assert` apply to each arm. Two values `sets` writes to one column in one use case
are `ConflictingEffects`, not a choice.

`sets(Register, Permit, kind == Application.kind)` copies the value of the row the step
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
sets(Acquire, Disclosure, acquired == true)
condition Acquired "権利を取得している"
holds(Acquired, Disclosure.acquired == true)
transitions(Decide, Disclosure.status, searched -> file).when(Acquired)
transitions(Decide, Disclosure.status, searched -> secret)
forbidden(Disclosure, status == file, acquired == false)
```

Use Int / Money / Decimal columns and `sets(UC, E, col == N)` when the quantity itself
must be checked; `check` and TLC both evaluate a comparison on the values over
`0..--int-range-max`. Drive the comparison with `drives(UC, E, cmp, true|false)` only when
the model does not track the values.

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
sets(Receive, Notice, scope == limited)
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
sets(Receive, Notice, scope == limited)
transitions(Draft, Notice.status, open -> drafted)
transitions(Reopen, Notice.status, drafted -> open)
sets(Reopen, Notice, final == true)
sets(Reopen, Notice, scope == limited)
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
