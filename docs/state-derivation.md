# State Derivation

<!-- derived-from ./language-reference.md#entity-state-constraints -->

State derivation answers the question: *for each entity, which combinations of its
state columns are reachable, and does any reachable state break a declared rule?*

The tool builds one transition system from the model and explores it by breadth-first
search (BFS). `states`, `check`, `diagram`, `csv`, `list`, `export`, and the
TypeScript unions read the explored states. `export --kind tla` and `verify` print the
same transition system as TLA+ for TLC (see [formal-verification.md](./formal-verification.md)).
No second reading of the model exists, so BFS and TLC cannot disagree on what a use case
does. The test `engine_equivalence` checks that on every sample.

The implementation lives in `crates/rdra-ish-core/src/lifecycle/`.

---

## The transition system

A transition system has variables, actions, and checks. `lifecycle::TransitionSystem`
builds all three from the model under explicit bounds.

### Variables

Each entity has a fixed number of instance slots (see [Bounds](#bounds)). Every variable
holds one value per slot.

| Variable | Values | Exists when |
|---|---|---|
| Existence | `absent`, `present`, `deleted` | Always, one per entity |
| An Enum column | the declared variants | Always |
| A Bool column | `false`, `true` | Always |
| A `@null` column of any other type | `null`, `present` | Always |
| An Int, Money, Decimal, Date, or DateTime column | `0..5` (plus `null` for `@null`) | A `sets`, a constraint, an applied condition body, an `after` assertion, or a `property` names it |
| A column typed by a [`variation`](./language-reference.md#variations) | the variation's values | Something names it, as for Int |
| A comparison proposition (`stock < selling`) | `false`, `true` | `sets(UC, E, cmp, bool)` drives the comparison |
| An owner (a `relate` foreign key) | the parent's slot, or `null` for an optional relate | The entity is the child of an N:1, 1:N, or 1:1 `relate` |
| `now` | `0..5` | Something compares a column to `now` |

An undriven comparison (`stock < selling` with no matching `sets`) is not a variable. Both
engines evaluate it on the Int values of its columns. A foreign key column is the owner
variable and is not a state column, so it is not an axis of `states`.

A new slot starts as follows:

- An entity that some use case creates starts `absent`. The creating use case sets the
  columns.
- An entity that no use case creates is **pre-existing**. Every slot starts `present`,
  every column at its base value, and `NoCreationPath` (info) says so. A variation column
  of a pre-existing entity starts at any value. [vision.md](./vision.md#decisions)
  records why.

The base value of a column is its `@default`. Without one it is the first Enum variant,
`false`, `null`, or `0`.

### Actions

Each use case becomes one atomic action per transaction group of its writes. The
actions interleave: in each step the tool picks any enabled action and applies it.

- **One action when the writes form one group.** `Pay` that moves an Order and its
  Payment in one FK-connected group is one action. Nothing can run between the two
  writes.
- **k actions when the writes form k groups.** The groups that `tx.rs` infers bound
  atomicity: FK-connected or API-bounded groups, then each isolated write.
  The use case becomes `<UC>_1` to `<UC>_k` in transaction order, and the actions
  interleave freely with every other action.
  [`cross_order_payment_split`](../samples/formal-verification/cross_order_payment_split.rdra)
  routes `Pay` through two APIs, so `Cancel` runs between `Pay_1` and `Pay_2`.
- **A use case with no write has no action.**

An action takes one parameter per entity it writes. A `creates` takes the lowest
`absent` slot. An `updates`, `writes`, or `deletes` takes any `present` slot. The
effects apply together to the pre-state:

| Source | Effect and guard |
|---|---|
| `creates(UC, E)` | The slot becomes `present`, with the columns at their base values. The creating use case's `sets` and transitions then apply. |
| `deletes(UC, E)` | The slot becomes `deleted` and its columns return to the base value. A deleted slot is never reused. |
| `sets(UC, E, col == val)` | `col := val`. A `sets` counts as a write even without a CRUD edge. |
| `sets(UC, E, cmp, bool)` | The comparison variable becomes `bool`. |
| `transitions(E.col, UC, a -> b)` | Guard `col == a`, effect `col := b`. |
| a variation column of a created instance | The instance takes any value of the variation, unless the creating use case sets it. `@default` never pins the choice. |
| a column the use case does not write | Unchanged. |

A `creates(Api, E)` reached through `invokes(UC, Api)` counts as a creation by `UC`.

**Owners.** An action that writes two entities joined by a `relate` binds them. A
created child gets its parent's slot as its owner. An action that updates a child
requires that the child belongs to the parent the action also writes. A mandatory
`relate` adds the parent as a parameter when a created child needs one. An
`.optional()` `relate` does not: the child is created unlinked, and a parent
that the action writes links it. An unlinked owner is not linked for `.along`,
quantifiers, or owner guards.

**Conditions as guards.** `applies_to(Condition, UseCase)` with a `holds` body, or
`applies_to(Condition, Api)` for each use case that invokes the API, guards the use
case's action:

- An atom on an entity the action writes reads that parameter. The action reads the
  parameter before the effects, except for a created entity, where it reads the
  instance as the action leaves it, so the condition constrains the creation.
- An atom on any other entity reads an existing instance. The atoms on one such
  entity share one instance, which a direct `relate` owner links to the parameters it
  relates to. Without a `relate`, the instance is independent.
- In a use case with k groups, the first group that writes an entity the body reads
  carries the guard, else the first group.
- `applies_to(Condition, System)` adds no guard, and a condition with no body is a
  trace link.
- A body applied to a use case that writes nothing is `UnbindableCondition`.

The order of `raises` and `triggers` between use cases adds no guard.

**Clock.** When something compares a column to `now`, a `Tick` action adds one to
`now` until it reaches the top of the Int range.

### Checks

| Declaration | Check |
|---|---|
| `forbidden`, `invariant`, `required`, `exclusive` | An invariant over the present slots of the entity, evaluated in every reachable state. |
| Multi-entity `forbidden`, `invariant` | An invariant over every combination of present slots. `.along(A, B)` restricts it to combinations that the `relate` owners link. |
| `when(E, ...).has/none(R, ...)` | An invariant over the present slots of `E` and the related slots of `R`. A `relate` links them through its owner; without one, `R` is independent. |
| `after(UC).assert(...)` | A predicate on the state each step of `UC` leaves, checked on every step of every action of `UC`. An entity the step does not write is read as it stands. |
| `property` with `always`, `eventually`, `leads_to` | Data only. BFS does not evaluate it, and `check` prints a `note:` that TLC does. |

Every check reads the instances that exist. A rule over an entity with no instance
holds vacuously in that state.

---

## Bounds

The bounds are inputs of the system, and every result states them.

| Command | Instances per entity | Int range |
|---|---|---|
| `states`, `check`, and the generators | 1 | `0..5` |
| `export --kind tla`, `verify` | `--instance-count` (default 2) | `0..--int-range-max` (default 5) |

`states` prints its bounds on stderr first:

```
bounds: 1 instance per entity, Int 0..5, --max-patterns 256 per target cone (4096 states per cone); entities a check does not read are unbounded
```

With one slot, an entity is created once, and deleting it does not free the slot.
An entity that a target does not read has no slot limit: a use case that creates an
audit row on every write never stops for want of a free slot. An Int literal outside `0..5` in a `sets`, a constraint, or a condition is
`IntLiteralOutOfBounds`.

---

## Exploration by cone of influence

`states` does not explore the whole system once. It explores each **target** on its
cone of influence:

- **Targets.** Each entity view, each invariant, and each `after` assertion.
- **Cone.** The smallest set of variables that holds the target's variables and, for
  every action that writes one of them, every variable the action reads: its guards,
  the existence of the entities it selects, its transition sources, and the clock.
  Actions that write nothing in the set drop out, and an action keeps only its effects
  on the set.
- **Exact.** The reachable states of the cone are the reachable states of the whole
  system projected onto the cone. The cone is not an approximation. The test
  `cone_exploration_equals_the_projection_of_the_whole_component` in
  `crates/rdra-ish-core/src/lifecycle/tests/cone.rs` checks that on `ec-site`,
  `personal-info`, and the formal-verification samples.
- **Shared.** Targets with the same cone are explored once.

BFS starts from the single initial state, applies every enabled action, and stops when
no new state appears or a cap is hit.

`--max-patterns` (default 256) caps the distinct patterns of one entity view within a
cone. A cone also stops after 16 joint states per allowed pattern, 4096 at the default.
When a cap is hit, the view is marked `truncated` and `PatternCapReached` or
`StateCapReached` is an error: a rule checked on a partial set is not sound. The
largest cone of any sample, in `clinic-ops`, reaches 184 states.

---

## Views

Each entity view lists the patterns that some present slot holds, one per distinct
assignment of the entity's state columns.

| Field | Meaning |
|---|---|
| Pattern | Values of the entity's columns, in a present slot. |
| `INITIAL` | A create action produces the pattern, or it is the start of a pre-existing entity. |
| `TERMINAL` | Some enabled action deletes a slot in the pattern, or no enabled action moves a slot off it. |
| `VIA` | The `(BUC, use case)` pairs of the actions on some path to the pattern: the least set closed under the moves between patterns, so it does not depend on the order BFS finds them. |
| `reachable` / `bound` | The count of patterns, and the product of the column domain sizes. |

An entity with no state column has one trivial pattern that is both initial and terminal.
An Enum variant that no pattern holds draws `UnreachableEnumVariant`.

```
$ rdra-ish states samples/formal-verification/order.rdra
Entity: Order (注文)
  axes: status[pending|paid|shipped|delivered|cancelled], delivered_at[null|present]

  STATUS     DELIVERED_AT  INITIAL  TERMINAL  VIA
  ─────────  ────────────  ───────  ────────  ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
  pending    null          yes      no        BucOrderLifecycle/PlaceOrder
  paid       null          no       no        BucOrderLifecycle/CapturePay, BucOrderLifecycle/PlaceOrder
  cancelled  null          no       yes       BucOrderLifecycle/CancelOrder, BucOrderLifecycle/PlaceOrder
  shipped    null          no       no        BucOrderLifecycle/CapturePay, BucOrderLifecycle/PlaceOrder, BucOrderLifecycle/ShipOrder
  delivered  present       no       yes       BucOrderLifecycle/CapturePay, BucOrderLifecycle/DeliverOrder, BucOrderLifecycle/PlaceOrder, BucOrderLifecycle/ShipOrder

  reachable: 5 / bound: 10
```

The command prints the `bounds:` line on stderr.

---

## Constraint checking

A violation is reported only when a reachable state witnesses it. Unreachable bad states
never trigger a diagnostic. Each diagnostic names the constraint, for example
`Forbidden_Order_0`, and the offending state.

### `forbidden`

For each `forbidden(E, col == val, ...)`, the conditions are AND-ed. If a reachable state
matches **all** of them, `ConstraintViolated` names the state. Forbidding an
unreachable state produces no diagnostic.

Comparison conditions (`forbidden(Item, stock < selling)`) evaluate on the Int values,
or on the comparison variable when a `sets` drives it.

### `invariant`

For each `invariant(E).when(...).then(...)`, the guards and the requirements are each
AND-ed. A reachable state that satisfies every guard but misses a requirement is a
`ConstraintViolated`.

### `required` and `exclusive`

`required(E, ...)` holds when every reachable state matches all conditions.
`exclusive(E, ...)` holds when no reachable state matches two or more of the listed
conditions.

### Temporal anchor assertions

`after(UseCase).assert(...)` checks a predicate on the state that each step of the use
case leaves. A use case split into k groups is checked after each group.

```rdra
after(ExecuteCertIssue)
  .assert(CertificateOrder.status == executed, ClientCertificate.status == active)
```

A step that ends in a state that fails the predicate is `AfterAssertViolated`, naming
the state the step reached. Equality and comparison forms both work. An `after` on a
use case with no action is `AssertOnInertUseCase`.

### Multi-entity and quantifier constraints

`forbidden(A, B, ...)` and `invariant(A, B)` evaluate over every combination of present
slots of `A` and `B`. With `.along(A, B)`, only the combinations that a `relate`
links count. Both forms get a real verdict, not a warning. `.along` needs a
`relate` that binds each adjacent pair, else the rule is `UnbindableScope`.

`when(E, ...).none(R, ...)` and `.has(R, ...)` evaluate over the present slots. At one
instance per entity, `states` sees one `E` and one `R`. Use `verify` with
`--instance-count 2` or more for rules that need two instances.

See [language-reference.md](./language-reference.md#entity-state-constraints) for the
syntax.

---

## Diagnostics

`StateDiag` in `crates/rdra-ish-core/src/lifecycle/diag.rs` lists every diagnostic.
`states`, `check`, `export --kind tla`, and `verify` share the severities. An error
makes the command exit non-zero in every format, and the generators refuse to emit.

| Diagnostic | Severity | When |
|---|---|---|
| `UnreachableEnumVariant { column, variant }` | warning | No reachable pattern holds a declared Enum variant. |
| `UnwrittenConstrainedVar { column }` | warning | A constraint or a condition guard reads a column that no use case writes, so it checks only the start value. |
| `NoCreationPath` | info | No use case creates the entity. It starts present. |
| `ConflictingEffects { usecase, column }` | error | One use case assigns two values to one column. |
| `DoubleModeledEnum { column, usecase }` | error | A use case that does not create the entity `sets` a lifecycle column that `transitions` drive. The creating use case's `sets` of the status is the start state and is not an error. |
| `ContradictoryDrivenComparison { usecase, proposition, driven }` | error | A use case drives a comparison to one truth while its own Int `sets` make it the other. |
| `IntLiteralOutOfBounds { value, max }` | error | An Int literal lies outside `0..max`. |
| `UnsupportedComparison { comparison }` | error | A comparison has a non-numeric operand. |
| `UnbindableScope { constraint }` | error | A multi-entity rule's `.along` names entities that no `relate` binds. |
| `UnbindableCondition { condition, usecase }` | error | A condition body is applied to a use case that writes nothing. |
| `UnresolvedPropertyAtom { property, column }` | error | A `property` atom names a column that no single entity owns. |
| `ConstraintViolated { name, ... }` | error | A reachable state breaks a `forbidden`, `invariant`, `required`, `exclusive`, multi-entity, or quantifier rule. |
| `AfterAssertViolated { anchor, ... }` | error | A step of the anchor use case ends in a state that fails the assertion. |
| `AssertOnInertUseCase { anchor }` | error | `after(UC).assert` names a use case with no action. |
| `PatternCapReached { cap, bound }` | error | An entity view exceeded `--max-patterns`. Raise the flag or narrow `--buc`. |
| `StateCapReached { cap }` | error | A cone exceeded its joint state cap. |

---

## Output formats

`rdra-ish states` renders the views in one of four formats (`--format`).

- `table` (default). One table per entity with an axis legend, one column per axis, and
  the `INITIAL`, `TERMINAL`, and `VIA` columns. An Int axis prints its values
  (`stock[0..5]`). An entity with no state column prints `(no state axes)`.
- `csv`. One row per pattern and column, with `entity_id`, `pattern_idx`, `column`,
  `value`, `is_initial`, `is_terminal`, and `via`.
- `json`. One object per entity with `axes` (each with its `domain`), `patterns`,
  `diagnostics`, `no_creation_path`, and the entity id and label. An Int domain lists
  `"0"` to `"5"`.
- `typescript`. One union per entity. The union reads the same views.

All formats honor `--entity <id>`, `--buc <id>`, and `--max-patterns <n>`. `--buc`
keeps the use cases reachable from the named BUCs. It does not change how an entity
starts: whether an entity is pre-existing is decided on the whole model.

---

## See Also

- [language-reference.md](./language-reference.md) — DSL syntax for entities, predicates,
  and state constraints.
- [cli-reference.md](./cli-reference.md) — the `states` subcommand and its options.
- [formal-verification.md](./formal-verification.md) — the TLA+/TLC printing of the
  same system, with two instances, and temporal `property` formulas.
- [vision.md](./vision.md) — the six lifecycle rules this layer implements.
