# Limits and Samples

<!-- derived-from ../../../docs/formal-verification.md -->

## Printer Approximations

- **Bounds**: `InstanceCount` (default 2, `--instance-count`) and `IntRange` (default
  `0..5`, `--int-range-max`). A `verify` OK says nothing outside them, and a deleted slot
  is never reused.
- **`.along`**: each adjacent pair needs a `relate` that binds it; the invariant joins
  them with `Child_owner_Parent` equalities. Without a `relate` the rule is
  `UnbindableScope`, an error.
- **Quantifiers**: `\A` / `\E` over `Entity_Ids`. A `relate` links the related instance
  through its owner; otherwise the related side is independent.
- **Atomic use case**: one use case is one atomic action per transaction group it
  writes, so `cross_order_payment` passes; a use case spanning k groups is k
  actions `<UC>_1..k`, so `cross_order_payment_split` fails; `quantifier_none`
  still fails.
- **Int arithmetic**: `IntRange` default `0..5`. A literal outside it is
  `IntLiteralOutOfBounds`, an error in `check` too.
- **`now`**: a global Int clock with a `Tick` action. Nothing pins a column compared to
  `now`, so TLC finds violations (`now_coupon` expected fail).
- **Transitions and `sets`**: each use case is one operator `<UC>(p0, ...)`; its
  `transitions` guard the source value and its `sets` assign in the same step.
- **Constraints**: one `INVARIANT` per constraint, named `<Kind>_<Entity>_<n>`
  (`Forbidden_Order_0`), plus `INVARIANT TypeOK`.
- **1:1 keys**: one `INVARIANT Unique_<Child>_<role>` per `relate(.., 1:1)` (role
  defaults to the parent id): no two present children own one parent slot. It holds
  vacuously at one instance, so only `verify` (two or more) can fail it.
- **Child-side minimum**: one `INVARIANT MinCard_<Child>_<role>` per
  `relate(.., 1..*:1)` or `1..1:1`: every present parent has a present child. A use
  case that creates the parent without a child in the same transaction fails it, at one
  instance too, so `check` finds it.
- **`@check`**: one `INVARIANT Check_<Entity>_<column>_<n>` per `@check(expr)`. A null
  operand leaves a comparison unknown, which passes (SQL semantics). A `@derived` column
  is never a state var: a Bool one is read as its expression, and an arithmetic one
  cannot be read (`DerivedColumnRead`).
- **Temporal `property`**: one lowering path. `export` lists every name in one `.cfg`; `verify`
  runs each property in its own module `<whole>_<Property>`, because TLC does not name the
  property its counterexample violates, and prints the lasso in model terms.
- **`after.assert`**: independent `[][(\A p : UC(p) => post')]_vars` PROPERTY named
  `AfterAssert_<UC>_<n>` (not injected into the effects).
- **`WF_vars(Next)`**: fairness on whole `Next`, not per action.
- **State models**: witnesses, deadlocks, and use cases that never run come from BFS on
  each entity's cone, at the same bounds as TLC (capped at 2,097,152 states per cone).
  TLC's own `CHECK_DEADLOCK` stays `FALSE`: every bounded model ends with no enabled
  action, and TLC cannot see one instance stuck while others keep moving.
- **Cones**: `verify` checks each invariant and `after` assertion on its cone of
  influence and the temporal properties on the whole model.
- Int, Money, Decimal, Date, and DateTime columns that something names are variables over
  `IntRange`; a `@null` one adds `null`, and a comparison over a null operand passes.
- `states` and `check` read the same system at one instance per entity, so Int and `now`
  columns that something names are `0..5` variables there too.
- Error diagnostics of the system stop `export` and `verify`; warnings go to stderr.
- **Partial runs**: `--entity E` checks `E`'s view and the invariants and after-asserts that
  read `E`, each on its exact cone, and defers a build error no selected cone reads.
  `--buc B` drops the use cases outside `B`, so witnesses, deadlocks, and use cases that
  never run are not decided. Both print a `not checked:` block, skip temporal properties
  in `verify`, and end with `OK: no errors in the selected scope, within ...`.
- **BFS and TLC agree**: the repository test `engine_equivalence` runs both engines on every
  cone of every sample at 1 and 2 instances and compares the reached and initial states, the
  edge sets `(from, to, use case)`, the witness, never-runs, and deadlock findings by name,
  and each check's verdict. A cone over 200,000 BFS states at 2 instances is compared at 1
  instance only; the test pins those cones in `VERIFY_FALLBACK` (today 8 clinic-ops cones).

## Bundled Samples

Canonical files live next to this skill under `samples/` (shipped with
`npx skills add` / `gh skill install`). Each file is **standalone** — do not
`check` the whole `samples/` directory (ids such as `Order` collide).

In the `rdra-ish-dsl` repository, `samples/formal-verification/` and
`samples/formal-verification-fail/` are symlinks to these same files.

A sample that does not pass both commands states its result in `// expect` lines at the
top, for example `// expect verify: tlc=counterexample fail Forbidden_Order_0`: the exit
status, the `tlc_result:` line, and the exact set of error names. The repository's
`scripts/verify-samples.sh` runs `check` and `verify` on every sample and fails when a
result differs. The grammar is in `docs/formal-verification.md` ("Sample
expectations"). The table repeats each header; "ok" means the command passes.

| Skill path | `check` | `verify` | Focus |
|---|---|---|---|
| `samples/order.rdra` | ok | `tlc=ok` | Lifecycle + `after.assert` + `leads_to` / `eventually` |
| `samples/int_stock.rdra` | ok | `tlc=ok` | Int arithmetic invariant / property |
| `samples/now_coupon.rdra` | fail `Forbidden_Coupon_0` | `tlc=counterexample` | `now` and `Tick`; no action pins `expired_at` |
| `samples/cross_order_payment.rdra` | ok | `tlc=ok` | Multi-instance + `.along` (Pay is one atomic action; Cancel deletes the pending Payment) |
| `samples/cross_order_payment_split.rdra` | fail `CrossForbidden_Order_0` | `tlc=counterexample` | Pay through two APIs is two actions, so Cancel runs between them |
| `samples/quantifier_none.rdra` | fail `Quantifier_Cert_0` | `tlc=counterexample` | `when(...).none` (Revoke while an Assign stays active) |
| `samples/untriggered_sets.rdra` | fail `Forbidden_Order_0` | `tlc=counterexample` | `sets` from a use case that drives no transition, next to `transitions` |
| `samples/after_assert_unmet.rdra` | fail `AfterAssert_DeliverOrder_0` | `tlc=counterexample` | `after(UC).assert` whose postcondition the use case does not set |
| `samples/temporal_unmet.rdra` | ok | `tlc=counterexample`, `NeverShipped` and `PendingLeadsToDelivered` | An `always` and a `leads_to` property that both fail; Reopen lets an Order be cancelled and reopened forever, so the `leads_to` lasso loops back |
| `samples/unwitnessed_state.rdra` | fail `Witness_Order_status_refunded Deadlock_Order_status_shipped` | `tlc=ok`, fails on the same names | A `transitions` endpoint no use case reaches, and a shipped Order whose only way out is Refund; Refund warns `Runs_Refund` |
| `samples/deadlock.rdra` | fail `Deadlock_Order_status_paid` | `tlc=ok`, fails on the same name | A way out that never opens: Ship's condition holds only while `pending`; Ship warns `Runs_Ship` |
| `samples/partial_scope.rdra` | fail `DerivedColumnRead_Audit_score` | fails before TLC; `--entity Order` gives `tlc=ok` | Audit's `forbidden` reads an arithmetic `@derived` column; `--entity Order` defers it under `not checked:` |
| `samples/one_to_one_shared.rdra` | ok | `tlc=counterexample`, `Unique_Shipment_Order` | Two use cases link two Shipments to one Order; one instance cannot show it |
| `samples/min_card_unmet.rdra` | fail `MinCard_OrderLine_Order` | `tlc=counterexample` | `1..*:1`; Place creates an Order without a line |
| `samples/min_card_met.rdra` | ok | `tlc=ok` | Place creates the Order and its first line in one transaction; `check` warns that AddLine never runs at one instance (no free OrderLine slot), `verify` runs it |
| `samples/check_unmet.rdra` | fail `Check_CartLine_qty_0` | `tlc=counterexample` | `@check(qty >= 1)`; ClearQty sets `qty` to 0 |
| `samples/check_met.rdra` | ok | `tlc=ok` | The same check kept by every use case; a `forbidden` reads a Bool `@derived` column |
| `samples/fail/order.rdra` | fail `Inv_Order_0` | `tlc=counterexample` | DeliverOrder never sets `delivered_at` |

## Canonical Doc

Full mapping tables (monorepo): `docs/formal-verification.md`.
