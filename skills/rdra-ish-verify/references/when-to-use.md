# When to use `verify` instead of `check` and `states`

<!-- derived-from ../../../docs/formal-verification.md -->
<!-- derived-from ../../../docs/state-derivation.md -->

`check`, `states`, and `verify` explore one transition system by BFS. Only the bounds
differ, so a verdict that differs between them comes from the bounds.

| Command | Instances per entity | Decides |
|---|---|---|
| `states`, `check` | 1 | Every rule, `after.assert`, and state-model finding; no `property` |
| `verify`, `export --kind tla` | 2 | The same at two instances, plus every `property` |

| Concern | Prefer | Why |
|---|---|---|
| Reachable Enum / Bool / nullable patterns | `states` | One view per entity |
| A state no use case reaches, an instance stuck, a use case that never runs | `check` | `Witness_*`, `Deadlock_*`, `Runs_*`; `verify` repeats them at two instances |
| Int / Money / Decimal comparison and arithmetic, `col < now` | `check` or `verify` | Both read the values over `0..--int-range-max` and tick `now` |
| `after(UC).assert(...)` | `check` or `verify` | Read after the last step of each run of the use case |
| `MinCard_*`, `Check_*` | `check` or `verify` | One instance already shows a parent without a child, or a bad value |
| `.none/.has` over several rows, multi-entity rules with `.along` | `check` for one row, `verify` for two | Both join rows through the `relate` keys |
| 1:1 key, `@unique` set, N:M junction pair (`Unique_*`) | `verify`, or `check --instance-count 2` | Two rows must share the key; at one instance it holds vacuously |
| `UnjoinedWrite_*` warning | `verify` | Only two instances show rows of different parents |
| `.at_most(n, ..)`, a `0..n:1` card | `verify --instances E=<n+1>` | The rule breaks only with `n + 1` rows of `E` |
| `property` of any form | `verify` | `check` and `states` evaluate no `property`; `check` prints a `note:` |
| A rule for any number of rows | `verify --prove` | See `prove.md` |

State the `bounds:` line with every result.
