# Limits and Samples

<!-- derived-from ../../../docs/formal-verification.md -->

## Printer Approximations

- **Bounds**: `InstanceCount` (default 2, `--instance-count`) and `IntRange` (default
  `0..5`, `--int-range-max`). A `verify` OK says nothing outside them, and a deleted slot
  is never reused.
- **`.along`**: each adjacent pair needs a `relate` that binds it; the invariant joins
  them with `Child_owner_Parent` equalities. Without a `relate` the rule is
  `UnbindableScope`, an error.
- **Event order**: `raises(UC, Ev)` and `triggers(Ev, UC2)` order the two. The last step of
  the raiser sets a latched mark `<E>_raised_<Ev>` on its rows, and the first step of `UC2`
  runs only on a row that carries it (its own row, its parent through the raiser's key, or
  an extra `Triggered` row joined by the one key between the entities). The mark is never
  cleared, so after the first raise `UC2` runs as often as its guards allow; a timer or an
  event an external system raises orders nothing. A use case several events trigger has one
  operator per event, `<UC>_by_<Ev>`, judged as one use case. A cone that does not hold
  every raiser leaves the mark out, so an invariant or property there can still report a
  run the order prevents; the trace then says `runs <UC> without <Ev> having been raised`.
  A guard that only restates the order is not needed; keep one that documents the
  precondition the handler relies on.
- **Quantifiers**: `\A` / `\E` over `Entity_Ids`. A `relate` links the related instance
  through its owner; otherwise the related side is independent.
- **Atomic use case**: one use case is one atomic action per transaction group it
  writes, so `cross_order_payment` passes; a use case spanning k groups is k
  actions `<UC>_1..k`, one run in `invokes` order on one run's rows with other use
  cases between its steps, so `cross_order_payment_split` fails and `split_run_order`
  passes; `quantifier_none` still fails.
- **Int arithmetic**: `IntRange` default `0..5`. A literal outside it is
  `IntLiteralOutOfBounds`, an error in `check` too, and so is an integer `@default`
  outside it on an Int, Money, Decimal, Date, or DateTime column a rule reads
  (`IntLiteralOutOfBounds_<E>_<value>`): raise `--int-range-max` or lower the default.
- **Defaults**: a created row starts at its columns' `@default` (a variation column
  excepted: see `references/dsl-surface.md`). An `Enum` default that is
  not one of its values (`invalid enum value`) or a `Bool` default other than `true` /
  `false` (`invalid bool value`) stops the model from loading in every command.
- **`now`**: a global Int clock with a `Tick` action. Nothing pins a column compared to
  `now`, so `verify` finds violations (`now_coupon` expected fail).
- **Transitions and `sets`**: each use case is one operator `<UC>(p0, ...)`; its
  `transitions` guard the source value and its `sets` assign in the same step.
- **Constraints**: one `INVARIANT` per constraint, named `<Kind>_<Entity>_<n>`
  (`Forbidden_Order_0`), plus `INVARIANT TypeOK`.
- **1:1 keys**: one `INVARIANT Unique_<Child>_<role>` per `relate(.., 1:1)` (role
  defaults to the parent id): no two present children own one parent slot. It holds
  vacuously at one instance, so it fails only at two or more (`verify`, or
  `check --instance-count 2`).
- **Unique sets**: a `@unique` column, a `@unique(a, b)`, and the two keys of a
  `relate(A, B, N:M).via(J)` junction each become one `INVARIANT Unique_<Entity>_<columns>` when
  the lifecycle holds every column's value (an `Enum`, `Bool`, or variation column, a
  compared `Int`, or a key, read as its owner): two present rows may not agree on every
  column, and a null column exempts its row. A set with a `String` column, or on an entity
  with a `@soft_delete` column, is not checked. Like a 1:1 key it needs two instances; keep
  the set free with a guard such as `holds(AmFree, Room.open == true).none(Booking.slot == am)`.
  The repository's `unique_double_booking` and `unique_booking_guarded` samples show both.
- **Child-side minimum**: one `INVARIANT MinCard_<Child>_<role>` per
  `relate(.., 1..*:1)` or `1..1:1`: every present parent has a present child. A use
  case that creates the parent without a child in the same transaction fails it, at one
  instance too, so `check` finds it.
- **Dangling keys**: one `INVARIANT DanglingKey_<Child>_<role>` per key with no
  `.on_delete`, `restrict`, or `no_action` whose parent a use case deletes: every present
  child names a present parent. The key guards the delete, so it holds. `cascade`,
  `set_null`, and `set_default` are not run yet: their delete removes the one row with no
  guard, the children stay, and the key gets no `DanglingKey_*`.
- **Child-side maximum and counts**: one `INVARIANT MaxCard_<Child>_<role>` per
  `relate(.., 0..3:1)` (a high end from 2 to 255), and a `Quantifier_*` per
  `.at_most/.at_least/.exactly(n, ..)`, printed with `Cardinality` (the module extends
  `FiniteSets`). `at_most(n)` needs `n + 1` rows of the counted entity, more than the
  default two instances: `verify --instances Loan=4` gives one entity more slots. Below it
  the rule is `Vacuous_*`. The repository's `counting_rules` sample shows both forms.
- **`@check`**: one `INVARIANT Check_<Entity>_<column>_<n>` per `@check(expr)`. A null
  operand satisfies no comparison but `!=`, as in every rule; `@check(qty == null or qty
  >= 1)` admits null. A `@derived` column
  is never a state var: a Bool one is read as its expression, and an arithmetic one
  cannot be read (`DerivedColumnRead`).
- **Temporal `property`**: one lowering path. `export` lists every name in one `.cfg`;
  `verify` checks each property alone, natively, and under `-o` or `--with-tlc` writes its
  own module (`Cone_<Property>`, `RdraSpec_<Property>`) for TLC, because TLC does not name
  the property its counterexample violates. A violation prints its lasso in model terms.
- **`after.assert`**: independent `[][(\A p : UC(p) => post')]_vars` PROPERTY named
  `AfterAssert_<UC>_<n>` (not injected into the effects).
- **`WF_vars(Next)`**: fairness on whole `Next`, not per action; another instance's loop
  satisfies it. `.fair(UC, ...)` on a property adds weak fairness per instance on the listed
  use cases (no strong fairness). A property is checked on the cone of what it reads and of
  its `.fair` use cases, where `WF_vars(Next)` does not restrict; a property without `.fair`
  reaches the whole model unless its cone holds every written var, and a whole-model run of
  patent-ops, food-permit-ops, or insurance-claim-ops at two instances stops at
  `--temporal-budget` or at the memory limit (or `--max-states`, when given) as not decided.
- **State models**: witnesses, deadlocks, and use cases that never run come from BFS on
  each entity's cone, at `verify`'s bounds (no state cap; `--memory` stops a cone, `--max-states <N>` sets one; `--max-patterns` stops no run of `verify`).
  `--memory` does not yet bound the whole process (#242): when cones stop at it, the process
  can take up to about twice the limit, so give about half the memory you can spare.
  TLC's own `CHECK_DEADLOCK` stays `FALSE`: every bounded model ends with no enabled
  action, and TLC cannot see one instance stuck while others keep moving. `export --kind
  tla` writes the same cones as `StateModel_<E>.tla` with `Deadlock_` and `Witness_`
  invariants, so TLC alone finds them too; a `Witness_` TLC does not report violated is
  the missing witness. A TLC pass on the `Deadlock_` invariants is no proof: a cycle in the
  column's cone hides a deadlock, and `verify` or `check` decides it (`docs/formal-verification.md`, "State models with TLC alone").
- **Cones**: `verify` checks each invariant, `after` assertion, and temporal property on
  its cone of influence, and a temporal property on the whole model only when its cone does
  not decide it (`docs/formal-verification.md`, "Temporal properties on a cone").
- Int, Money, Decimal, Date, and DateTime columns that something names are variables over
  `IntRange`; a `@null` one adds `null`. A comparison over a null operand fails, except
  `!=`, in every rule form including `@check`; write `col == null or ...` to admit null.
- `states` and `check` read the same system, by default at one instance per entity and
  `0..5`, so Int and `now` columns that something names are `0..5` variables there too.
- Error diagnostics of the system stop `export` and `verify`; warnings go to stderr.
- **Partial runs**: `--entity E` checks `E`'s view and the invariants and after-asserts that
  read `E`, each on its exact cone, and defers a build error no selected cone reads.
  `--buc B` drops the use cases outside `B`, so witnesses, deadlocks, and use cases that
  never run are not decided. Both print a `not checked:` block, skip temporal properties
  in `verify`, and end with `OK: no errors in the selected scope, within ...`.
  `--actor A`, `--without-buc B`, and `--without-usecase U` instead assume the use cases
  they leave out never run, and decide witnesses, deadlocks, and use cases that never run
  under that assumption.
- **BFS and TLC agree**: `verify --with-tlc` compares the two engines on every check of a
  run and fails on a disagreement, and the repository test `engine_equivalence` runs both engines on every
  cone of every sample at 1 and 2 instances and compares the reached and initial states, the
  edge sets `(from, to, use case)`, the witness, never-runs, and deadlock findings by name,
  and each check's verdict. A cone over 300,000 BFS states at 2 instances is compared at 1
  instance only; the test pins those cones in `VERIFY_FALLBACK` (today 24 cones: 1 of
  clinic-ops, 2 of patent-ops, 1 of food-permit-ops, 7 of insurance-claim-ops, and 13 of
  oidc-auth).

## Bundled Samples

Canonical files live next to this skill under `samples/` (shipped with
`npx skills add` / `gh skill install`). Each file is **standalone** — do not
`check` the whole `samples/` directory (ids such as `Order` collide).

In the `rdra-ish-dsl` repository, `samples/formal-verification/` and
`samples/formal-verification-fail/` are symlinks to these same files.

A sample that does not pass both commands states its result in `// expect` lines at the
top, for example `// expect verify: fail Forbidden_Order_0`: the exit status and the exact
set of error names. The repository's `scripts/verify-samples.sh` runs `check` and `verify`
on every sample with no TLC and fails when a result differs; with `--with-tlc` it runs
each `verify` again beside TLC and also fails on an engine disagreement. The grammar is in `docs/formal-verification.md` ("Sample
expectations"). The table repeats each header; "ok" means the command passes.

| Skill path | `check` | `verify` | Focus |
|---|---|---|---|
| `samples/order.rdra` | ok | ok | Lifecycle + `after.assert` + `leads_to` / `eventually` |
| `samples/int_stock.rdra` | ok | ok | Int arithmetic invariant / property |
| `samples/now_coupon.rdra` | fail `Forbidden_Coupon_0` | fail, on the same names | `now` and `Tick`; no action pins `expired_at` |
| `samples/cross_order_payment.rdra` | ok | ok | Multi-instance + `.along` (Pay is one atomic action; Cancel deletes the pending Payment) |
| `samples/cross_order_payment_split.rdra` | fail `MultiForbidden_Order_0` | fail, on the same names | Pay through two APIs is two actions in `invokes` order, the capture first, so Cancel runs between them |
| `samples/split_run_order.rdra` | ok | ok | OpenSettlement through two APIs is one run: the settlement is created once, under the claim the first step moved, and `.fair(OpenSettlement, ..)` finishes a started run |
| `samples/quantifier_none.rdra` | fail `Quantifier_Cert_0` | fail, on the same names | `invariant(...).when(...).none(...)` (Revoke while an Assign stays active) |
| `samples/quantifier_guard.rdra` | ok | ok | `holds(..).none(..)` guard on Revoke; the `.none(...)` rule holds |
| `samples/untriggered_sets.rdra` | fail `Forbidden_Order_0` | fail, on the same names | `sets` from a use case that drives no transition, next to `transitions` |
| `samples/after_assert_unmet.rdra` | fail `AfterAssert_DeliverOrder_0` | fail, on the same names | `after(UC).assert` whose postcondition the use case does not set |
| `samples/temporal_fair.rdra` | ok | fail `GrantEndsUnfair` | The same `leads_to` with and without `.fair(Register, Dismiss)`: another application amending forever breaks the unfair one only |
| `samples/order_stability.rdra` | ok | fail `CancelledStaysCancelled`, `AmountKeptOnceDecided` and `ShippedAfterInvoiced` | `stable`, `unchanged(..).when(..)` and `precedes`, each decided by BFS on its cone; the failing three print the shortest trace to the step or state that breaks them |
| `samples/temporal_along.rdra` | ok | fail `CapturedPaidAnyOrder` | The same `always` over a Payment and an Order with and without `.along(Payment, Order)`: only the unscoped one fails, on a payment beside an order it does not belong to |
| `samples/temporal_unmet.rdra` | ok | fail `NeverShipped` and `PendingLeadsToDelivered` | An `always` and a `leads_to` property that both fail; Reopen lets an Order be cancelled and reopened forever, so the `leads_to` lasso loops back |
| `samples/unwitnessed_state.rdra` | fail `Witness_Order_status_refunded Deadlock_Order_status_shipped` | fail, on the same names | A `transitions` endpoint no use case reaches, and a shipped Order whose only way out is Refund; Refund warns `Runs_Refund` |
| `samples/deadlock.rdra` | fail `Deadlock_Order_status_paid` | fail, on the same name | A way out that never opens: Ship's condition holds only while `pending`; Ship warns `Runs_Ship` |
| `samples/can_reach_unmet.rdra` | ok | fail `OrderCanFinish` | Once LoseStock and CloseWindow ran, an Order moves between `pending` and `on_hold` forever; no deadlock, but `verify`'s BFS fails `can_reach` with the states the Order is confined to. `check` evaluates no property |
| `samples/can_reach_met.rdra` | ok | ok | The same model with Restock: every reachable state can still finish |
| `samples/event_order.rdra` | ok | ok | `triggers(EvOrderExecuted, IssueCert)` runs IssueCert only after the event, on the executed order, so the `after(IssueCert).assert` on the order's status holds |
| `samples/partial_scope.rdra` | fail `DerivedColumnRead_Audit_score` | fails before any cone; `--entity Order` passes | Audit's `forbidden` reads an arithmetic `@derived` column; `--entity Order` defers it under `not checked:` |
| `samples/one_to_one_shared.rdra` | ok | fail `Unique_Shipment_Order` | Two use cases link two Shipments to one Order; one instance cannot show it |
| `samples/none_any_child.rdra` | ok | ok | `.none(Invoice)` on Issue says "no invoice yet" and reads the link only; `Unique_Invoice_Order` holds |
| `samples/none_any_child_bypassed.rdra` | ok | fail `Unique_Invoice_Order` | Reissue creates a second Invoice under a closed Order without the guard; one instance cannot show it |
| `samples/min_card_unmet.rdra` | fail `MinCard_OrderLine_Order` | fail, on the same names | `1..*:1`; Place creates an Order without a line |
| `samples/min_card_met.rdra` | ok | ok | Place creates the Order and its first line in one transaction; `check` warns that AddLine never runs at one instance (no free OrderLine slot), `verify` runs it |
| `samples/check_unmet.rdra` | fail `Check_CartLine_qty_0` | fail, on the same names | `@check(qty >= 1)`; ClearQty sets `qty` to 0 |
| `samples/check_met.rdra` | ok | ok | The same check kept by every use case; a `forbidden` reads a Bool `@derived` column |
| `samples/fail/order.rdra` | fail `Inv_Order_0` | fail, on the same names | DeliverOrder never sets `delivered_at` |

## Canonical Doc

Full mapping tables (monorepo): `docs/formal-verification.md`.
