# When To Use TLA+ vs `check` and `states`

<!-- derived-from ../../../docs/formal-verification.md -->
<!-- derived-from ../../../docs/state-derivation.md -->

## Decision Table

| Concern | Prefer | Why |
|---|---|---|
| Enum / Bool / Nullable reachable patterns | `rdra-ish states` | Fast BFS at one instance per entity |
| A state no use case reaches, an instance stuck at a state, a use case that never runs | `check` | `Witness_*` and `Deadlock_*` errors, `Runs_*` warnings; `verify` repeats them at two instances |
| Int comparison (`stock < selling`), Int / Money / Decimal arithmetic | `check` or `verify` | Both read Int values over `0..5` |
| `col < now` / DateTime vs `now` | `check` or `verify` | Global `now` + `Tick`; the invariant finds violations |
| `property` `always` / `eventually` / `leads_to` | `verify` | Path properties; `check` and `states` do not evaluate them |
| `after(UC).assert(...)` | `check` or `verify` | Checked after each step of the use case |
| `when(...).none/has(...)` | `check`, or `verify` for two related rows | Quantifiers over present instances |
| Multi-entity `forbidden` / `invariant` + `.along` | `check` for one instance, `verify` for two | Both join instances through the `relate` owners |
| 1:1 key (`Unique_*`) | `verify` | Two children of one parent need two instances; at one it holds vacuously |
| Child-side minimum (`MinCard_*`), `@check` (`Check_*`) | `check` or `verify` | One instance already shows a parent without a child, or a bad value |

## Same System, Different Bounds

| Command | Instances per entity | Reaches |
|---|---|---|
| `states`, `check` | 1 | Per-entity views, every constraint verdict, state-model findings, no temporal properties |
| `export --kind tla`, `verify` | 2 by default | The same verdicts at two instances, plus `property` formulas |

Both read one transition system, so a verdict that differs between them comes from the
bounds, not from a second reading of the model. Choose `verify` when a rule needs two
instances or a temporal `property`, and state the `bounds:` line with every result.
