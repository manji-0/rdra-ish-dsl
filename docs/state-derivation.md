# State Pattern Derivation

<!-- derived-from ./language-reference.md#entity-state-constraints -->

State pattern derivation answers the question: *for each entity, which combinations of
its state-defining column values are actually reachable, given the use cases, events,
and state transitions declared in the model?*

The result is computed by a breadth-first search (BFS) over a finite product space of
abstract column values, seeded from `creates` operations (or defaults when no creation
path exists) and expanded by `updates` / `deletes` / `sets` / `transitions`. The output
is one result per entity, listing the reachable patterns, which are initial, which are
terminal, and through which use cases each was reached.

The implementation lives in `crates/rdra-ish-core/src/state_pattern.rs`.

---

## State Axes

A **state axis** is one column of an entity whose value contributes to its abstract
state. Only three kinds of column become axes; every other column is ignored for state
derivation.

| Column | Axis kind | Abstract values |
|---|---|---|
| `Enum(a, b, c)` | `Enum` | one of the declared variants `{a, b, c}` |
| `Bool` | `Bool` | `{false, true}` |
| `@null` (any base type) | `Nullable` | `{null, present}` (the `present` value may carry a PostgreSQL type for display) |
| comparison expression in `forbidden` / `invariant` / `required` / `exclusive` / `sets` | `Proposition` | `{false, true}` (driven by `sets(..., <expr>, bool)`) |

A non-nullable, non-Enum, non-Bool column (e.g. a plain `Int` primary key) is not a
state axis. An entity with no axes yields exactly one trivial pattern that is both
initial and terminal.

The size of the search space is bounded by the product of the axis cardinalities (each
Enum contributes its variant count, each Bool and Nullable contributes 2). This bound is
reported when the per-entity pattern cap is reached.

### Abstract values

Effects are normalized into abstract values before reachability:

- An Enum variant maps to `Enum(variant)`.
- `true` / `false` map to `Bool(true)` / `Bool(false)`.
- `present` and a typed-present (e.g. `timestamptz`) both map to `Present` — they are
  **equivalent for reachability**; the PostgreSQL type is retained only for display
  (`present:timestamptz`).
- `null` maps to `Null`.
- A `Proposition` axis reuses the `Bool` value representation. Its default value is
  `Bool(false)` (comparison not yet satisfied). A `sets(..., <expr>, true/false)` effect
  advances it to `Bool(true)` or resets it to `Bool(false)`.

---

## Operations

Predicates are compiled into **operations**: a use case acting on an entity to change
its state. Each operation has a kind (`Create`, `Update`, `Delete`), an optional guard,
and a set of effects (column → abstract value).

| Source | Operation produced |
|---|---|
| `creates(UC, E)` | A `Create` operation. Seeds the initial pattern by applying its effects to the base (default) pattern. |
| `updates(UC, E)` / `writes(UC, E)` | An `Update` operation. Expands reachable patterns by applying its effects. |
| `deletes(UC, E)` | A `Delete` operation. Produces no successor; marks the matching pattern terminal. |
| `sets(UC, E, "col", "val")` | A column effect attached to the originating use case's operation on `E`. |
| `sets(UC, E, <expr>, bool)` | A **proposition effect** attached to the originating use case's operation on `E`. Advances or resets the `Proposition` axis whose key is the normalized comparison expression (e.g. `stock<selling`). |
| `transitions(event::Ev, From, To)` + `raises(UC, Ev)` | An `Update` operation on the entity's status column, **guarded** by `status == From`, with effect `status := To`. |

### How `sets` effects attach

Effects from `sets` are grouped by their originating use case. They are merged onto that
use case's operation on the entity:

- If the use case already has a `transitions`-derived guarded operation, the non-status
  effects are merged into that guarded operation so they only take effect from the
  correct source state.
- Otherwise the effects form an unguarded operation.

Effects on the entity's **status column** are ignored when a state machine exists for
it — `transitions` is the source of truth there (see DoubleModeledEnum below).

When `sets` carries a comparison expression instead of a column name (e.g.
`sets(Sell, Stock, stock < selling, true)`), it registers a **`PropositionEffect`**
rather than a `ColumnEffect`. The effect is merged into the same use case's `Update`
operation on the entity, identical to how column effects are merged. The
`Proposition` axis for that expression is created automatically if it does not yet
exist.

### Guard constraints (`AxisConstraint`)

A guard is a list of `AxisConstraint`s, each requiring `column == value`. An operation
applies to a pattern only if **all** its guard constraints hold in that pattern (AND
semantics). Transition-derived operations carry the guard `status == From`, which is
what makes status changes follow the declared state-machine edges rather than jumping
arbitrarily.

---

## The BFS Algorithm

1. **Identify axes** for the entity (Enum / Bool / Nullable columns). In addition,
   collect all **`Proposition` axes** by scanning `forbidden` / `invariant` /
   `required` / `exclusive` constraints and `sets` proposition effects for comparison
   expressions that reference this entity. Each distinct normalized expression (e.g.
   `stock<selling`, `expired_at<now`) becomes one `Proposition` axis with default value
   `Bool(false)`.
2. **Collect operations** from `creates` / `updates` / `deletes` / `writes`, merge in
   `sets` column effects and `sets` proposition effects, and build transition-derived
   guarded `Update`s from `transitions` + `raises`.
3. **Seed.** Build the base pattern from `@default` values (or per-axis fallbacks:
   first Enum variant, `Bool=false`, `Nullable=null`). For each `Create` operation,
   apply its effects to the base pattern and add the result as an **initial** pattern.
   If there are no `Create` operations, seed the base pattern alone and emit
   `NoCreationPath`.
4. **Expand.** Process a worklist of reached patterns. For each operation enabled in the
   current pattern (its guard holds):
   - `Create` is skipped (seed-only).
   - `Delete` marks the current pattern terminal and produces no successor.
   - `Update` applies its effects; if the result is new, it is added to the reached set
     and the worklist. An update with no effects only records provenance. An update that
     does not change the pattern is skipped (no self-loop).
5. **Cap.** If the reached count hits the cap before a new pattern would be added, the
   result is marked `truncated` and `PatternCapReached { cap, bound }` is emitted, where
   `bound` is the theoretical product-space size.
6. **Mark terminals.** A pattern is terminal if it was the source of a `Delete`, or if no
   enabled `Update`/`Delete` would leave it.
7. **Detect unreachable variants.** For each Enum axis, any declared variant not present
   in any reached pattern yields `UnreachableEnumVariant`.
8. **Check constraints.** Evaluate `forbidden`, `invariant`, `required`, and
   `exclusive` against the reached set (below).

Each reached pattern carries **provenance**: the set of `(BUC id, use case id)` pairs
that contributed to reaching it. This is what the `VIA` column shows.

---

## How `transitions` and `sets` Interact

When both a state machine (`transitions`) and explicit `sets` effects touch the **same
Enum column**, the column is "double-modeled." The derivation:

- Emits a `DoubleModeledEnum { column }` diagnostic.
- Treats `transitions` as the source of truth for that column and **ignores** the `sets`
  effects on it.

Non-status effects from the same use case (e.g. a `sets` on a nullable timestamp) are
still merged into the transition-derived guarded operation, so that, for example,
`status := delivered` and `delivered_at := present` happen together and only from the
`shipped` state. This is what keeps `(status=delivered, delivered_at=null)` out of the
reachable set when the deliver use case sets both.

---

## Constraint Checking After BFS

After the reachable set is computed, declared constraints are checked against it.
Per-entity `forbidden`, `invariant`, `required`, and `exclusive` constraints are
checked directly against the entity's reached patterns. `cross_forbidden` and
`cross_invariant` are checked after all entity results are derived by combining the
reached patterns for the participating entities; any violation is attached to each
involved entity's diagnostics.

### `forbidden`

For each `forbidden(E, (col, val), ...)`, the conditions are AND-ed. If any reached
pattern matches **all** conditions, a `ForbiddenStateViolated { conditions, pattern_desc }`
diagnostic is emitted. Forbidding an unreachable state produces no diagnostic.

Comparison expressions in `forbidden` (e.g. `forbidden(Stock, (status, on_sale), stock < selling)`)
are matched as additional AND conditions against the `Proposition` axis for that
expression. A pattern satisfies the condition when the axis value equals the expected
`Bool` (always `Bool(true)` for a bare comparison in `forbidden`).

### `invariant`

For each `invariant(E).when(...).then(...)`, the guards and requirements are each
AND-ed. For every reached pattern where **all guards hold** but **any requirement
fails**, an `InvariantViolated { guards, requireds, pattern_desc }` diagnostic is
emitted.

Comparison expressions in `.when(...)` and `.then(...)` clauses
(e.g. `invariant(Coupon).when(expired_at < now).then(status, expired)`) are evaluated
the same way: each comparison maps to its `Proposition` axis and is checked as a
`Bool` equality against the current pattern value. Guards with comparison propositions
use `Bool(true)` as the required value, enabling guards like "when the proposition
holds".

### `required`

For each `required(E, (col, val), ...)`, the conditions are AND-ed. Every reached pattern
must match **all** conditions. If a reached pattern misses any condition, a
`RequiredStateViolated { conditions, pattern_desc }` diagnostic is emitted.

Comparison expressions in `required` are matched against their `Proposition` axes with
`Bool(true)` as the required value.

### `exclusive`

For each `exclusive(E, (col, val), ...)`, the listed conditions are treated as
alternatives. If a reached pattern satisfies **two or more** listed conditions, an
`ExclusiveStateViolated { conditions, pattern_desc }` diagnostic is emitted with the
co-occurring conditions.

Comparison expressions in `exclusive` are matched against their `Proposition` axes with
`Bool(true)` as the matched value.

### Cross-Entity Constraints

<!-- derived-from ./language-reference.md#entity-state-constraints -->

For `cross_forbidden` / `cross_invariant`, the derivation checks the cross-product of
the participating entities' reached patterns, up to an internal safety cap. Conditions
that reference state axes, such as `(Order.status, paid)` or `Order.status == Payment.status`,
are evaluated from the abstract pattern values. If a condition needs data that is not
present in state patterns (for example `Payment.amount > Order.total` on ordinary
numeric columns), the result receives a `CrossConstraintNotEvaluated` warning rather
than silently treating the rule as satisfied.

When a cross constraint has `.along(EntityA, EntityB, ...)`, the rule is relation-scoped:
it is intended to quantify only over instances connected through the declared `relate`
path. The current derivation can verify the declared path shape, but it does not track
which concrete instances are linked at each reached pattern. If the broader global
cross-product has no violating witness, the relation-scoped rule is known satisfied
because every linked-instance combination is a subset of that product. If the global
product does have a witness, the relation-scoped rule receives `CrossConstraintNotEvaluated`
instead of being reported as a violation.

See [language-reference.md](./language-reference.md#entity-state-constraints) for the
syntax and design rationale.

---

## Diagnostics

| Diagnostic | When |
|---|---|
| `UnreachableEnumVariant { column, variant }` | A declared (or transition-target) Enum variant is never reached by any pattern. |
| `ConflictingEffects { usecase, column }` | One operation assigns two different values to the same column; resolved last-wins. |
| `DoubleModeledEnum { column }` | Both `transitions` and `sets` drive the same Enum column; `transitions` wins. |
| `NoCreationPath` | The entity has no `creates`; the pattern set is seeded from defaults only. |
| `PatternCapReached { cap, bound }` | The per-entity cap was hit; output is truncated. `bound` is the product-space size. |
| `ForbiddenStateViolated { conditions, pattern_desc }` | A reachable pattern matches all conditions of a `forbidden` declaration. |
| `InvariantViolated { guards, requireds, pattern_desc }` | A reachable pattern satisfies an invariant's guards but breaks a requirement. |
| `RequiredStateViolated { conditions, pattern_desc }` | A reachable pattern misses at least one condition of a `required` declaration. |
| `ExclusiveStateViolated { conditions, pattern_desc }` | A reachable pattern satisfies two or more conditions of an `exclusive` declaration. |
| `CrossForbiddenViolated { entities, conditions, pattern_desc }` | A reached cross-entity pattern combination matches all conditions of a `cross_forbidden` declaration. |
| `CrossInvariantViolated { entities, guards, requireds, pattern_desc }` | A reached cross-entity pattern combination satisfies cross-invariant guards but breaks a requirement. |
| `CrossConstraintNotEvaluated { entities, constraint, reason }` | A cross-entity rule cannot be fully evaluated from per-entity abstract state patterns or exceeds the cross-product safety cap. |

---

## Output Formats

`rdra-ish states` renders the per-entity results in one of three formats (`--format`):

### `table` (default)

A human-readable table per entity, prefixed by the axis legend and one column per axis,
plus `INITIAL`, `TERMINAL`, and `VIA` columns:

```
Entity: Order (Order)
  axes: status[pending|paid|shipped|delivered|cancelled], delivered_at[null|present:timestamptz]

  STATUS     DELIVERED_AT         INITIAL  TERMINAL  VIA
  ─────────  ───────────────────  ───────  ────────  ──────────────────────────────────
  pending    null                 yes      no        BucOrder/PlaceOrder
  paid       null                 no       no        BucPayment/Capture, BucOrder/PlaceOrder
  ...
```

Entities with no state axes render `(no state axes)`.

### `csv`

A flat CSV with one row per pattern, suitable for spreadsheets and diffing. Entities
with no axes render an `(no axes)` row. The `present` value of a nullable axis is
rendered with its recorded type (e.g. `present:timestamptz`).

### `json`

A JSON array of per-entity objects, each carrying the entity id/label, the axes, and the
reachable patterns with their `initial` / `terminal` flags and provenance. Use this for
programmatic consumption.

All three formats honor `--entity <id>` (restrict to one entity), `--buc <id>` (restrict
the reachable scope to the union of the named BUCs), and `--max-patterns <n>` (the cap).

---

## See Also

- [language-reference.md](./language-reference.md) — DSL syntax for entities, predicates,
  and state constraints.
- [cli-reference.md](./cli-reference.md) — the `states` subcommand and its options.
