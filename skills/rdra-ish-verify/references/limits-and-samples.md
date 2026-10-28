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
| `samples/fail/order.rdra` | expected fail | Negative TLC (`check` exits 1 too) |

## Canonical Doc

Full mapping tables (monorepo): `docs/formal-verification.md`.
