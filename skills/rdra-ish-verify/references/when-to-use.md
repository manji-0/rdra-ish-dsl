# When To Use TLA+ vs `states`

<!-- derived-from ../../../docs/formal-verification.md -->
<!-- derived-from ../../../docs/state-derivation.md -->

## Decision Table

| Concern | Prefer | Why |
|---|---|---|
| Enum / Bool / Nullable reachable patterns | `rdra-ish states` | Fast BFS over finite axes |
| Terminal / unreachable enum variants | `states` | Built-in diagnostics |
| Comparison as Bool proposition (`sets(..., cmp, true/false)`) | `states` | Proposition axes |
| Int / Money / Decimal arithmetic | `export --kind tla` | `IntRange` + TLC Integers |
| `col < now` / DateTime vs `now` | TLA | Global `now` + Assign/TickNow; Safety finds violations (not baked into Next) |
| `property` `always` / `eventually` / `leads_to` | TLA | Path properties; not evaluated by `states` |
| `after(UC).assert(...)` | TLA (and local after-check) | Primed postconditions on SpecActions |
| `when(...).none/has(...)` | TLA | Finite `Entity_Ids` quantifiers |
| Multi-entity `forbidden` / `invariant` + `.along` | TLA for linked instances; `states` may emit `CrossConstraintNotEvaluated` | TLC uses `*_owner` when `relate` exists |

## Two Layers (do not mix expectations)

| Layer | Tool | Continuous values |
|---|---|---|
| Abstract propositions | `states` | `stock < selling` is a Bool axis driven by `sets(..., cmp, bool)` |
| Arithmetic model checking | TLA / TLC | Int / Money / Decimal / `now` are `IntRange` variables |

A `forbidden(Item, stock < selling)` without a driving `sets` is inert under
`states`, but TLA export still emits arithmetic Safety when Int axes exist.

## Scalar vs Multi-instance Export

- **Scalar** (default): one variable per axis — no multi-entity rules / quantifiers.
- **Multi-instance**: triggered by multi-entity `forbidden` / `invariant` or
  `when(...).none/has`. Emits `InstanceCount`, `Entity_Ids`, function-valued
  axes, and `Child_owner` from `relate`.
