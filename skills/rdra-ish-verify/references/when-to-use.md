# When To Use TLA+ vs `states`

<!-- derived-from ../../../docs/formal-verification.md -->
<!-- derived-from ../../../docs/state-derivation.md -->

## Decision Table

| Concern | Prefer | Why |
|---|---|---|
| Enum / Bool / Nullable reachable patterns | `rdra-ish states` | Fast BFS at one instance per entity |
| Terminal / unreachable enum variants | `states` | Built-in diagnostics |
| Int comparison (`stock < selling`) | `states` or TLA | Both read Int values over `0..5` |
| Int / Money / Decimal arithmetic | `states`, `check`, or `export --kind tla` | `IntRange` + TLC Integers |
| `col < now` / DateTime vs `now` | `states` or TLA | Global `now` + `Tick`; the invariant finds violations |
| `property` `always` / `eventually` / `leads_to` | TLA | Path properties; not evaluated by `states` |
| `after(UC).assert(...)` | `states` or TLA | Checked after each step of the use case; TLA: `[][(\A p : UC(p) => post')]_vars` |
| `when(...).none/has(...)` | `states`, or TLA for two instances | Quantifiers over present instances |
| Multi-entity `forbidden` / `invariant` + `.along` | `states` for one instance, TLA for two | Both join instances through `Child_owner_Parent` |

## Same System, Different Bounds

| Command | Instances per entity | Reaches |
|---|---|---|
| `states`, `check` | 1 | Per-entity views, every constraint verdict, no temporal properties |
| `export --kind tla`, `verify` | 2 by default | The same verdicts at two instances, plus `property` formulas |

Both read one transition system, so a verdict that differs between them comes from the
bounds, not from a second reading of the model. Choose `verify` when a rule needs two
instances (a quantifier or `.along` over several rows) or a temporal `property`.
