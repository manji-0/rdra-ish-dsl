# Formal Verification (TLA+ / TLC)

<!-- derived-from ./state-derivation.md -->
<!-- derived-from ./language-reference.md#entity-state-constraints -->
<!-- derived-from ./language-reference.md#temporal-path-properties -->
<!-- derived-from ./cli-reference.md#export -->
<!-- derived-from ./cli-reference.md#verify -->

RDRA-ish can export entity lifecycles and state constraints to **TLA+** and
optionally check them with **TLC**. This complements `rdra-ish states` (fast BFS
reachability) with classical model checking for invariants and temporal properties.

## Commands

```
rdra-ish export <INPUTS...> --kind tla [-o <OUT>]
rdra-ish verify <INPUTS...> --backend tlc [-o <OUT_DIR>]
```

| Command | Role |
|---|---|
| `export --kind tla` | Write `RdraSpec.tla` **and** sibling `RdraSpec.cfg` |
| `verify --backend tlc` | Export, then run `tlc` / `tlc2` from `PATH` |
| `states` | Local BFS checks (no TLC dependency) |

If `-o` ends with `.tla`, the sibling `.cfg` is written next to it. If `-o` is a
directory (or has no extension), files are written as `<dir>/RdraSpec.tla` and
`<dir>/RdraSpec.cfg`.

## Mapping

| RDRA | TLA+ |
|---|---|
| Entity status Enum + `transitions(Entity.col, Ev, a -> b)` | `VARIABLES`, `Init`, action disjuncts in `Next` |
| Bool / `@null` columns (multi-axis) | Additional variables with `BOOLEAN` or `{"null","present"}` |
| Int / Money / Decimal used in comparisons or `sets` Int assigns | `EXTENDS Integers`, `IntRange == 0..N`, arithmetic Safety / primed effects (nullable numeric columns promote to Int, not Nullable) |
| Comparison propositions driven by `sets(..., cmp, bool)` only | Boolean `prop_*` variables (BFS layer; skipped when the same cmp is arithmetic) |
| Entity-local `invariant` / `forbidden` / `required` / `exclusive` | Safety conjuncts (`\A i \in Entity_Ids: …` when multi-instance) |
| Multi-entity `forbidden` / `invariant` | Safety over the multi-entity variable product |
| `.along(...)` (RelationPath) | Quantified over `Entity_Ids`; with `relate` uses `Child_owner` link filter |
| `when(...).none/has(...)` | Finite `Entity_Ids == 1..InstanceCount` with `\A`/`\E`; `relate` adds `*_owner` FK |
| `after(UC).assert` equality | Independent `PROPERTY`: `[][raised actions => primed posts]_vars` (not injected into SpecActions) |
| `after(UC).assert` comparison | Same PROPERTY shape; Int arithmetic when axes exist (cross-entity RHS allowed); else `prop' = TRUE` |
| `property` + `always` / `eventually` / `leads_to` | Named formulas listed as `PROPERTY` in the `.cfg` |
| Multi-instance temporal formulas | Per-instance quantifiers: `[](\A i: …)`, `\A i: <>(…)`, `\A i: (p ~> q)` |
| Temporal atoms with Int compares | `stock < selling`, `stock >= 1`, etc. map to TLC arithmetic |
| `col < now` / DateTime vs `now` | Global `now \in IntRange`, `TickNow`, lhs promoted to Int axis |
| Undriven Int axes | Nondet `Assign_*` actions (`\E v \in IntRange: col' = v`) |
| Any `eventually` / `leads_to` property | `Spec` includes `WF_vars(Next)` |

Example temporal / Int syntax:

```rdra
property PaidLeadsToShipped "paid eventually reaches shipped"
  leads_to(Order.status == paid, Order.status == shipped)

property StockOk
  always(Item.stock >= Item.selling)

sets(Restock, Item, stock == 3)
forbidden(Item, stock < selling)

after(DeliverOrder).assert(Order.status == delivered, Order.delivered_at == present)
after(Sell).assert(Item.sold >= 1)

when(Cert, status == revoked).none(Assign.status == active)
// equivalent: when(Cert.status == revoked).none(Assign.status == active)

forbidden(Order, Payment, Order.status == cancelled, Payment.status == captured)
  .along(Order, Payment)
```

Operators:

- `always(expr)` → `[]expr`
- `eventually(expr)` → `<>expr`
- `leads_to(p, q)` → `p ~> q`
- Connectives inside expressions: prefer `and` / `or` / `not` (`/\` `\/` `~` aliases)
- Property label string is optional (`property StockOk always(...)`)

Path properties are **not** evaluated by `rdra-ish states`; use `export` / `verify`.

## Two layers for Int / `now`

| Layer | Tool | How continuous values appear |
|---|---|---|
| Abstract propositions | `rdra-ish states` | `stock < selling` is a Bool axis; drive with `sets(..., cmp, true/false)` |
| Arithmetic model checking | `export --kind tla` / `verify` | Int / Money / Decimal and `now` become `IntRange` variables; comparisons are TLC arithmetic |

Do not mix the layers in one expectation: a `forbidden(Item, stock < selling)` without
`sets(..., stock < selling, …)` is inert under `states`, but TLA export still emits
arithmetic Safety when Int axes exist.

## Scalar vs multi-instance export

- **Scalar** (default): one variable per axis. Used when the model has no multi-entity
  `forbidden` / `invariant` and no `when(...).none/has`.
- **Multi-instance**: triggered by those constructs. Emits `InstanceCount`,
  `Entity_Ids`, function-valued axes (`status \in [Ids -> …]`), and `Child_owner`
  from `relate(..., N:1)` / `1:N`. Entity-local Safety and temporal properties are
  quantified per instance binder.

## Approximations and limitations

- **`.along`**: multi-instance export quantifies over `Entity_Ids`. With a usable
  `relate` N:1 / 1:N, Safety is filtered by `Child_owner`. Without a link, a
  warning is recorded and the formula quantifies over the instance product
  (stronger than linked-only intent).
- **Quantifiers (`has` / `none`)**: exported over finite instance sets
  (`InstanceCount`, currently fixed at 2 in the emitter). When `relate`
  declares N:1 / 1:N, a `Child_owner` function links related instances; otherwise
  the related side is quantified independently.
- **Shared events across entities**: each entity still gets its own SpecAction for
  the same event (e.g. `Order_EvPay` and `Payment_EvPay`). Steps can interleave;
  they are not one atomic multi-entity action — `cross_order_payment` /
  `quantifier_none` are expected TLC fails under that semantics.
- **Int arithmetic**: comparisons and `sets(..., col == N)` on Int / Money / Decimal
  use TLC Integers on `IntRange` (currently fixed at `0..5` in the emitter).
  Undriven Int axes get nondet `Assign_*` actions so TLC can explore values.
  `@default(0)` is accepted for Int columns. Boundedness is an approximation —
  `verify` OK does not prove behaviour outside `IntRange`.
- **`now`**: exported as a global Int clock with `TickNow`. Columns compared to
  `now` (including DateTime) become Int axes. **Safety is not baked into Next**:
  `Assign_*` and `TickNow` may choose values that violate `forbidden(col < now)`;
  TLC is expected to find those counterexamples (see `now_coupon.rdra`).
- **`WF_vars(Next)`**: fairness is on the whole `Next` disjunction, not per action.
  Sufficient for simple `eventually` / `leads_to` on small models; not a substitute
  for per-action weak fairness.
- BFS `rdra-ish states` still does not treat Int / `now` as state axes; use
  TLA/TLC for those checks.
- Export warnings (skipped mappings, `.along` without `relate`, unmapped
  `after.assert`, …) are printed on stderr by `export --kind tla` / `verify` and
  also embedded as `\\* WARNING:` comments in the `.tla`. Fatal obligation drops
  are tagged `[TLA_FATAL:…]` and fail export/verify.
- `after(UC).assert(...)` becomes an independent TLA `PROPERTY`
  `[][actions => primed posts]_vars` — it does **not** inject assignments into
  SpecActions (so asserts check outcomes rather than enabling assumptions).
- Diagnostic ids may still use a historical `Cross*` prefix even though the DSL
  surface is multi-entity `forbidden` / `invariant`.

## Workflow

1. Model lifecycle with `transitions` / `sets` and entity-local rules.
2. Run `rdra-ish states` for quick feedback on Enum/Bool/Nullable axes.
3. `rdra-ish export --kind tla -o /tmp/rdra-tla` and inspect the Spec + `.cfg`.
4. Install [TLA+ tools](https://github.com/tlaplus/tlaplus) so `tlc` is on `PATH`.
5. `rdra-ish verify --backend tlc -o /tmp/rdra-tla` to run TLC.

Agent skill: `skills/rdra-ish-verify/` (routing + references + bundled samples).
Install: see [`skills/README.md`](../skills/README.md) (`npx skills add` / `gh skill install`).

## Samples

Canonical files: `skills/rdra-ish-verify/samples/` (each file is **standalone** —
do not `check` the whole directory; ids such as `Order` collide). In this repo,
`samples/formal-verification/` and `samples/formal-verification-fail/` are
symlinks to those skill-bundled files (CLI tests keep working after skill install).

| Sample | TLC intent | Focus |
|---|---|---|
| `order.rdra` | expected pass | Lifecycle Safety + `after.assert` + `leads_to` / `eventually` + `WF_vars` |
| `int_stock.rdra` | expected pass | Int axes, arithmetic `forbidden`, Int temporal property |
| `now_coupon.rdra` | expected fail | `now` / unconstrained Assign + TickNow; Safety finds `expired_at < now` |
| `cross_order_payment.rdra` | expected fail | Multi-instance + `.along` (independent SpecActions allow Cancel then Capture) |
| `quantifier_none.rdra` | expected fail | `when(...).none` (Revoke while Assign stays active) |
| `fail/order.rdra` | expected fail | Intentionally unsafe (`check` may warn with exit 0; TLC should fail Safety) |

`rdra-ish check` on fail samples can exit 0 with warnings only — use TLC for the
negative verdict.
