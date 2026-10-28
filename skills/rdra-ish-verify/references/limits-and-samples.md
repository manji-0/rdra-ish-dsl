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
- **Temporal `property`**: one lowering path; all names are listed in `.cfg` `PROPERTY`.
- **`after.assert`**: independent `[][(\A p : UC(p) => post')]_vars` PROPERTY named
  `AfterAssert_<UC>_<n>` (not injected into the effects).
- **`WF_vars(Next)`**: fairness on whole `Next`, not per action.
- **Cones**: `verify` checks each invariant and `after` assertion on its cone of
  influence and the temporal properties on the whole model.
- Nullable Money/Decimal used in arithmetic promote to Int variables (not Nullable).
- `states` and `check` read the same system at one instance per entity, so Int and `now`
  columns that something names are `0..5` variables there too.
- Error diagnostics of the system stop `export` and `verify`; warnings go to stderr.
- **Partial runs**: `--entity E` checks `E`'s view and the invariants and after-asserts that
  read `E`, each on its exact cone, and defers a build error no selected cone reads.
  `--buc B` drops the use cases outside `B`, so witnesses, deadlocks, and use cases that
  never run are not decided. Both print a `not checked:` block, skip temporal properties
  in `verify`, and end with `OK: no errors in the selected scope, within ...`.

## Bundled Samples

Canonical files live next to this skill under `samples/` (shipped with
`npx skills add` / `gh skill install`). Each file is **standalone** — do not
`check` the whole `samples/` directory (ids such as `Order` collide).

In the `rdra-ish-dsl` repository, `samples/formal-verification/` and
`samples/formal-verification-fail/` are symlinks to these same files.

| Skill path | TLC intent | Focus |
|---|---|---|
| `samples/order.rdra` | expected pass | Lifecycle + `after.assert` + temporal |
| `samples/int_stock.rdra` | expected pass | Int arithmetic invariant / property |
| `samples/now_coupon.rdra` | expected fail | `now` and `Tick`; no action pins `expired_at` |
| `samples/cross_order_payment.rdra` | expected pass | Multi-instance + `.along` (atomic Pay) |
| `samples/cross_order_payment_split.rdra` | expected fail | Pay through two APIs is two actions (in-between state) |
| `samples/quantifier_none.rdra` | expected fail | `when(...).none` (interleaving) |
| `samples/untriggered_sets.rdra` | expected fail | `sets` from a use case that drives no transition, next to `transitions` |
| `samples/after_assert_unmet.rdra` | expected fail | `after(UC).assert` whose postcondition the use case does not set (fails, never vacuous) |
| `samples/unwitnessed_state.rdra` | expected fail (TLC passes) | A `transitions` endpoint no use case reaches: `NoWitness` and `Deadlock` (shipped, whose only way out is Refund) fail `check` and `verify`; Refund warns `NeverRuns` (guards never hold together) |
| `samples/deadlock.rdra` | expected fail (TLC passes) | A state with a way out that never opens: `Deadlock` (Order stays `paid`) fails `check` and `verify`; Ship warns `NeverRuns` |
| `samples/partial_scope.rdra` | expected fail (TLC does not run); pass with `--entity Order` | Audit's `forbidden` reads an arithmetic `@derived` column: `DerivedColumnRead` stops `check` and `verify`. `--entity Order` defers it under `not checked:` and passes in the selected scope |
| `samples/one_to_one_shared.rdra` | expected fail | `relate(.., 1:1)` is the invariant `Unique_<Child>_<role>`; two use cases link two children to one parent (`check` passes: one instance) |
| `samples/min_card_unmet.rdra` | expected fail | `relate(.., 1..*:1)` is the invariant `MinCard_<Child>_<role>`; a use case creates the parent without a child (`check` exits 1 too) |
| `samples/min_card_met.rdra` | expected pass | The same model with the parent and its first child created in one transaction; `check` warns AddLine never runs at one instance (no free child slot), `verify` runs it |
| `samples/check_unmet.rdra` | expected fail | `@check(qty >= 1)` is the invariant `Check_CartLine_qty_0`; a use case sets `qty` to 0 (`check` exits 1 too) |
| `samples/check_met.rdra` | expected pass | The same check kept by every use case; a `forbidden` reads a Bool `@derived` column |
| `samples/fail/order.rdra` | expected fail | Negative TLC (`check` exits 1 too) |

## Canonical Doc

Full mapping tables (monorepo): `docs/formal-verification.md`.
