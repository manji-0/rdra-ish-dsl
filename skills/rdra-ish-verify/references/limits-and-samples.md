# Limits and Samples

<!-- derived-from ../../../docs/formal-verification.md -->

## Emitter Approximations

- **`.along`**: with usable `relate` N:1 / 1:N, Safety filters by `Child_owner`;
  without a link, quantifies over the instance product (stronger than linked-only).
- **Quantifiers**: finite `Entity_Ids` with `InstanceCount` (currently fixed at 2).
- **Int arithmetic**: `IntRange` currently fixed at `0..5`. Undriven Int axes get
  nondet `Assign_*` actions. `@default(0)` is valid on Int columns.
- **`now`**: global Int clock with `TickNow`
  (`\E t \in IntRange: t > now /\ now' = t`). Columns compared to `now`
  (including DateTime) become Int axes.
- Nullable Money/Decimal used in arithmetic promote to Int axes (not Nullable).
- BFS `states` still ignores Int / `now` as axes.

## Bundled Samples

Canonical files live next to this skill under `samples/` (shipped with
`npx skills add` / `gh skill install`). Each file is **standalone** — do not
`check` the whole `samples/` directory (ids such as `Order` collide).

In the `rdra-ish-dsl` repository, `samples/formal-verification/` and
`samples/formal-verification-fail/` are symlinks to these same files.

| Skill path | Focus |
|---|---|
| `samples/order.rdra` | Lifecycle Safety + `after.assert` + `leads_to` / `eventually` |
| `samples/int_stock.rdra` | Int axes, arithmetic `forbidden`, Int temporal property |
| `samples/now_coupon.rdra` | `now` / `TickNow`, DateTime promoted to Int |
| `samples/cross_order_payment.rdra` | Multi-instance + `.along` + `Payment_owner` |
| `samples/quantifier_none.rdra` | `when(...).none` + `Assign_owner` |
| `samples/fail/order.rdra` | Intentionally unsafe (negative TLC checks) |

## Canonical Doc

Full mapping tables (monorepo): `docs/formal-verification.md`.
