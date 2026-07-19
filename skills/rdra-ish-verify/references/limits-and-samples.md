# Limits and Samples

<!-- derived-from ../../../docs/formal-verification.md -->

## Emitter Approximations

- **`.along`**: with usable `relate` N:1 / 1:N, Safety filters by `Child_owner`;
  without a link, quantifies over the instance product (stronger than linked-only).
- **Quantifiers**: finite `Entity_Ids` with `InstanceCount` (currently fixed at 2).
- **Shared events**: per-entity SpecActions can interleave (not one atomic
  multi-entity step) — `cross_order_payment` / `quantifier_none` expected fail.
- **Int arithmetic**: `IntRange` currently fixed at `0..5`. Undriven Int axes get
  nondet `Assign_*`. For lhs of `col < now` / `col <= now`, Assign keeps `v >= now`.
- **`now`**: global Int clock with `TickNow`; for `col < now` / `col <= now`,
  TickNow also requires `t <= col` (non-vacuous Safety without Init=max).
- **`sets(Event, …)`**: effects on the transition event apply in SpecActions.
- **Temporal `property`**: one lowering path; all names are listed in `.cfg` `PROPERTY`.
- **`WF_vars(Next)`**: fairness on whole `Next`, not per action.
- Nullable Money/Decimal used in arithmetic promote to Int axes (not Nullable).
- BFS `states` still ignores Int / `now` as axes.
- Export warnings go to stderr (`warning: tla export: …`) and `.tla` comments.

## Bundled Samples

Canonical files live next to this skill under `samples/` (shipped with
`npx skills add` / `gh skill install`). Each file is **standalone** — do not
`check` the whole `samples/` directory (ids such as `Order` collide).

In the `rdra-ish-dsl` repository, `samples/formal-verification/` and
`samples/formal-verification-fail/` are symlinks to these same files.

| Skill path | TLC intent | Focus |
|---|---|---|
| `samples/order.rdra` | expected pass | Lifecycle + `after.assert` + temporal |
| `samples/int_stock.rdra` | expected pass | Int arithmetic Safety / property |
| `samples/now_coupon.rdra` | expected pass | `now` / constrained Assign + TickNow |
| `samples/cross_order_payment.rdra` | expected fail | Multi-instance + `.along` (interleaving) |
| `samples/quantifier_none.rdra` | expected fail | `when(...).none` (interleaving) |
| `samples/fail/order.rdra` | expected fail | Negative TLC (`check` may exit 0 with warnings) |

## Canonical Doc

Full mapping tables (monorepo): `docs/formal-verification.md`.
