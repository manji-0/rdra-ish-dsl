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
rdra-ish export <INPUTS...> --kind tla [-o <OUT>] [--instance-count N] [--int-range-max N]
rdra-ish verify <INPUTS...> --backend tlc [-o <OUT_DIR>] [--instance-count N] [--int-range-max N]
```

| Command | Role |
|---|---|
| `export --kind tla` | Write `RdraSpec.tla` **and** sibling `RdraSpec.cfg` |
| `verify --backend tlc` | Export, then run `tlc` / `tlc2` from `PATH` |
| `states` | Local BFS checks (no TLC dependency) |

`--instance-count` (default `2`) sets multi-instance `InstanceCount` (`>= 1`;
`0` is rejected, `1` warns that singleton ids cannot distinguish multi-entity
instances).
`--int-range-max` (default `5`) sets `IntRange == 0..N` (`>= 1`; `0` is rejected).
When either approximation is used, export/verify always warn with the active bounds.

If `-o` ends with `.tla`, the sibling `.cfg` is written next to it. If `-o` is a
directory (or has no extension), files are written as `<dir>/RdraSpec.tla` and
`<dir>/RdraSpec.cfg`. The `.cfg` sets `CHECK_DEADLOCK FALSE` because lifecycle
models often have terminal states with no enabled `Next` action (Spec still
allows stuttering via `[][Next]_vars`).

## Mapping

| RDRA | TLA+ |
|---|---|
| Entity status Enum + `transitions(Entity.col, Ev, a -> b)` | `VARIABLES`, `Init`, action disjuncts in `Next` |
| Bool / `@null` columns (multi-axis) | Additional variables with `BOOLEAN` or `{"null","present"}` |
| Int / Money / Decimal / Date / DateTime used in comparisons or `sets` Int assigns | `EXTENDS Integers`, `IntRange == 0..N`, arithmetic Safety / primed effects (nullable numeric columns promote to Int, not Nullable; Date/DateTime column-vs-column cmps are arithmetic, not frozen BOOL props) |
| Comparison propositions driven by `sets(..., cmp, bool)` only | Boolean `prop_*` variables (BFS layer; skipped when the same cmp is arithmetic) |
| Prop axis never primed by any action | `[TLA_FATAL:frozen_proposition_axis]` (export/verify refuse; vacuous Safety) |
| Entity-local `invariant` / `forbidden` / `required` / `exclusive` | Safety conjuncts (`\A i \in Entity_Ids: …` when multi-instance) |
| Multi-entity `forbidden` / `invariant` | Safety over the multi-entity variable product |
| `.along(...)` (RelationPath) | Quantified over `Entity_Ids` (including intermediate junction binders); with `relate` / `.via` uses pairwise `*_owner` filters (`Child_owner`, or `Child_Parent_owner` when a child has multiple parents) |
| `when(...).none/has(...)` | Finite `Entity_Ids == 1..InstanceCount` with `\A`/`\E`; `relate` adds `*_owner` FK |
| `after(UC).assert` equality | Independent `PROPERTY`: `[][raised actions => primed posts]_vars` (not injected into SpecActions) |
| `after(UC).assert` comparison | Same PROPERTY shape; Int arithmetic when axes exist (cross-entity RHS allowed); else `prop' = TRUE` |
| `property` + `always` / `eventually` / `leads_to` | Named formulas listed as `PROPERTY` in the `.cfg` |
| Multi-instance temporal formulas | Per-instance quantifiers: `[](\A i: …)`, `\A i: <>(…)`, `\A i: (p ~> q)` |
| Temporal atoms with Int compares | `stock < selling`, `stock >= 1`, etc. map to TLC arithmetic |
| `col < now` / DateTime vs `now` | Global `now \in IntRange`, `TickNow`, lhs promoted to Int axis |
| Undriven Int axes | Synthetic nondet `Assign_*` actions (`\E v \in IntRange: col' = v`) — not modeled use cases |
| `sets` Int literal or comparison RHS IntLit outside `0..int_range_max` | `[TLA_FATAL:int_literal_out_of_range]` (export/verify refuse) |
| TypeOK vs Safety | `.cfg` has separate `INVARIANT TypeOK` and `INVARIANT Safety` (domain vs business rules) |
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

- **`.along`**: multi-instance export quantifies over `Entity_Ids`, including
  intermediate binders for multi-hop paths such as `.along(A, Junction, B)`
  (from `relate(A, B, N:M).via(Junction)`). Pairwise `*_owner` filters are applied;
  a child with multiple parents uses `{Child}_{Parent}_owner`. Missing binders or
  missing relate links emit a warning and drop the filter (global product) —
  never silently. Junction entities without state axes still get `Junction_Ids`.
- **Quantifiers (`has` / `none`)**: exported over finite instance sets
  (`InstanceCount`, default **2**, override with `--instance-count`). When `relate`
  declares N:1 / 1:N (including via-synthesized edges), a `*_owner` function links
  related instances; otherwise the related side is quantified independently.
- **Shared events across entities**: each entity still gets its own SpecAction for
  the same event (e.g. `Order_EvPay` and `Payment_EvPay`). Steps can interleave;
  they are not one atomic multi-entity action — `cross_order_payment` /
  `quantifier_none` are expected TLC fails under that semantics.
- **Int arithmetic**: comparisons and `sets(..., col == N)` on Int / Money / Decimal
  use TLC Integers on `IntRange` (default **`0..5`**, override with `--int-range-max`).
  Literals outside that window are `[TLA_FATAL:int_literal_out_of_range]` (raise
  `--int-range-max` or lower the literal). Undriven Int axes get synthetic nondet
  `Assign_*` actions so TLC can explore values; `verify` notes when a counterexample
  mentions `Assign_*`. `@default(0)` is accepted for Int columns. Boundedness is an
  approximation — `verify` OK does not prove behaviour outside `IntRange`.
  Domain violations report as `TypeOK`; business rules as `Safety`.
- **Approximation banner**: whenever multi-instance or integer axes are used,
  export/verify always emit
  `tla approximation: InstanceCount=… IntRange=0..…; verify OK does not prove outside this window`
  on stderr and as a `\\* WARNING:` comment in the `.tla`.
- **`now`**: exported as a global Int clock with `TickNow`. Columns compared to
  `now` (including DateTime) become Int axes. **Safety is not baked into Next**:
  `Assign_*` and `TickNow` may choose values that violate `forbidden(col < now)`;
  TLC is expected to find those counterexamples (see `now_coupon.rdra`).
- **`WF_vars(Next)`**: fairness is on the whole `Next` disjunction, not per action.
  Sufficient for simple `eventually` / `leads_to` on small models; not a substitute
  for per-action weak fairness.
- BFS `rdra-ish states` still does not treat Int / `now` as state axes; use
  TLA/TLC for those checks. Undriven comparison props and NotEvaluated multi-entity /
  quantifier / temporal-comparison diagnostics are **`[error]`** under `states`
  (fail-closed). For mixed Enum+Int models, `check` / `states` /
  `diagram` / `csv` / `list` / non-TLA `export --allow-tla-only` demote those
  TLA-only gaps for exit while keeping PatternCap / forbidden fail-closed.
  Int comparison samples such as `int_stock.rdra` are for `verify`
  (or `--allow-tla-only`), not a green default `states` exit.
- Export warnings (skipped mappings, `.along` without `relate`, unmapped
  `after.assert`, …) are printed on stderr by `export --kind tla` / `verify` and
  also embedded as `\\* WARNING:` comments in the `.tla`. Fatal obligation drops
  are tagged `[TLA_FATAL:…]` and fail export/verify.
- `after(UC).assert(...)` becomes an independent TLA `PROPERTY`
  `[][actions => primed posts]_vars` — it does **not** inject assignments into
  SpecActions (so asserts check outcomes rather than enabling assumptions).
- User-facing state diagnostics for multi-entity rules say **multi-entity**
  (enum / table ids may still use a historical `Cross*` prefix). The DSL
  surface remains multi-entity `forbidden` / `invariant`.

## Workflow

1. Model lifecycle with `transitions` / `sets` and entity-local rules.
2. Run `rdra-ish states` for quick feedback on Enum/Bool/Nullable axes.
3. `rdra-ish export --kind tla -o /tmp/rdra-tla` and inspect the Spec + `.cfg`.
4. Install [TLA+ tools](https://github.com/tlaplus/tlaplus) so `tlc` is on `PATH` (`nix develop` provides it).
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
| `untriggered_sets.rdra` | expected fail | `sets` from a use case that raises no transition event (Flag then Ship reaches the forbidden state) |
| `fail/order.rdra` | expected fail | Intentionally unsafe (reachable forbidden → `check`/`states` exit 1; TLC fails Safety) |

`rdra-ish check` fails closed on reachable forbidden / undriven comparisons; use TLC
for temporal properties and Int arithmetic that `check` only notes as TLA-only.
