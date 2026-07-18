# Formal Verification (TLA+ / TLC)

<!-- derived-from ./state-derivation.md -->
<!-- derived-from ./language-reference.md#entity-state-constraints -->

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
| `export --kind tla` | Write `RdraSpec.tla` and `RdraSpec.cfg` |
| `verify --backend tlc` | Export, then run `tlc` / `tlc2` from `PATH` |
| `states` | Local BFS checks (no TLC dependency) |

If `-o` ends with `.tla`, the sibling `.cfg` is written next to it. If `-o` is a
directory (or has no extension), files are written as `<dir>/RdraSpec.tla` and
`<dir>/RdraSpec.cfg`.

## Mapping

| RDRA | TLA+ |
|---|---|
| Entity status Enum + `state` / `transitions` | `VARIABLES`, `Init`, action disjuncts in `Next` |
| Bool / `@null` columns (multi-axis) | Additional variables with `BOOLEAN` or `{"null","present"}` |
| Int columns used in comparisons / `sets` Int assigns | `EXTENDS Integers`, `IntRange == 0..N`, arithmetic Safety / primed Int effects |
| Comparison propositions driven by `sets` (or `now`) | Boolean `prop_*` variables |
| `invariant` / `forbidden` / `required` / `exclusive` | Safety conjuncts under `Safety` |
| `cross_forbidden` / `cross_invariant` | Safety over the multi-entity variable product |
| `.along(...)` (RelationPath) | Quantified over `Entity_Ids`; with `relate` uses `*_owner` link filter |
| `when(...).none/has(...)` | Finite `Entity_Ids == 1..InstanceCount` with `\A`/`\E`; `relate` adds `*_owner` FK |
| `after(UC).assert` equality | Primed postconditions on SpecActions for events raised by `UC` |
| `after(UC).assert` comparison | Arithmetic when Int axes exist; else `prop' = TRUE` |
| `property` + `always` / `eventually` / `leads_to` | Named formulas listed as `PROPERTY` in the `.cfg` |
| Temporal atoms with Int compares | `stock < selling`, `stock >= 1`, etc. map to TLC arithmetic |
| `col < now` / DateTime vs `now` | Global `now \in IntRange`, `TickNow`, lhs promoted to Int axis |
| Undriven Int axes | Nondet `Assign_*` actions (`\E v \in IntRange: col' = v`) |
| Any `eventually` / `leads_to` property | `Spec` includes `WF_vars(Next)` |

CLI:

```
rdra-ish export … --kind tla -o …
rdra-ish verify … --backend tlc
```

Example temporal / Int syntax:

```rdra
property PaidLeadsToShipped "paid eventually reaches shipped"
  leads_to(Order.status == paid, Order.status == shipped)

property StockOk
  always(Item.stock >= Item.selling)

sets(Restock, Item, stock == 3)
forbidden(Item, stock < selling)

after(DeliverOrder).assert(Order.status == delivered, Order.delivered_at == present)

when(Cert, status == revoked).none(Assign.status == active)

forbidden(Order, Payment, Order.status == cancelled, Payment.status == captured)
  .along(Order, Payment)
```

Operators:

- `always(expr)` → `[]expr`
- `eventually(expr)` → `<>expr`
- `leads_top == q` → `p ~> q`
- Connectives inside expressions: `~`, `/\`, `\/`

Path properties are **not** evaluated by `rdra-ish states`; use `export` / `verify`.

## Approximations and limitations

- **`.along`**: multi-instance export quantifies over `Entity_Ids`. With a usable
  `relate` N:1 / 1:N, Safety is filtered by `Child_owner`. Without a link, a
  warning is recorded and the formula quantifies over the instance product
  (stronger than linked-only intent).
- **Quantifiers (`has` / `none`)**: exported over finite instance sets
  (`InstanceCount`, currently fixed at 2 in the emitter). When `relate`
  declares N:1 / 1:N, a `Child_owner` function links related instances; otherwise
  the related side is quantified independently.
- **Int arithmetic**: comparisons and `sets(..., col == N)` on Int columns use
  TLC Integers on `IntRange` (currently fixed at `0..5` in the emitter).
  Undriven Int axes also get nondet `Assign_*` actions so TLC can explore values.
  `@default(0)` is accepted for Int columns.
- **`now`**: exported as a global Int clock with `TickNow` (`\E t \in IntRange: t > now /\ now' = t`).
  Columns compared to `now` (including DateTime) become Int axes.
- BFS `rdra-ish states` still does not treat Int / `now` as state axes; use
  TLA/TLC for those checks.

## Workflow

1. Model lifecycle with `transitions` / `sets` and entity-local rules.
2. Run `rdra-ish states` for quick feedback.
3. `rdra-ish export --kind tla -o /tmp/rdra-tla` and inspect the Spec.
4. Install [TLA+ tools](https://github.com/tlaplus/tlaplus) so `tlc` is on `PATH`.
5. `rdra-ish verify --backend tlc -o /tmp/rdra-tla` to run TLC.
