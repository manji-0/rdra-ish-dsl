# State Derivation

<!-- derived-from ./language-reference.md#entity-state-constraints -->

This reference is for modelers and contributors who need to know exactly what the lifecycle
explores: *for each entity, which combinations of its state columns are reachable, and does
any reachable state break a declared rule?* It defines the transition system, its bounds, how
it is explored, and every finding it reports. The syntax is in
[language-reference.md](./language-reference.md); how `verify` checks the same system is in
[formal-verification.md](./formal-verification.md).

The tool builds one transition system from the model and explores it by breadth-first search
(BFS). `states`, `check`, `next`, `diagram`, `csv`, `list`, `export`, and `codegen` read the
explored states. `verify` decides its checks on the same system, `export --kind tla` prints it
for TLC, and `verify --prove` encodes it for z3
([Proofs for any number of rows](./formal-verification.md#proofs-for-any-number-of-rows)). No
second reading of the model exists, so the engines cannot disagree on what a use case does.
The code is in `crates/rdra-ish-core/src/lifecycle/`.

---

## The transition system

A transition system has variables, actions, and checks. `lifecycle::TransitionSystem` builds
all three from the model under explicit bounds.

### Variables

Each entity has a fixed number of instance slots (see [Bounds](#bounds)). Every variable holds
one value per slot.

| Variable | Values | Exists when |
|---|---|---|
| Existence | `absent`, `present`, `deleted` | Always, one per entity |
| An Enum column | the declared variants, plus `null` for `@null` | Always |
| A Bool column | `false`, `true`, plus `null` for `@null` | Always |
| A `@null` column of any other type | `null`, `present` | Always |
| An Int, Money, Decimal, Date, or DateTime column | `0..5`, plus `null` for `@null` | A `sets`, a constraint, a `@check`, an applied condition body, a `.when` guard, a `chooses` or `sets` value, an `after` assertion, or a `property` names it |
| A column typed by a [`variation`](./language-reference.md#variations) | the variation's values | Something names it, as for Int |
| A comparison proposition (`stock < selling`) | `false`, `true` | `drives(UC, E.col op rhs, bool)` drives the comparison |
| An owner (a `relate` foreign key) | the parent's slot, or `null` for an optional relate | The entity is the child of an N:1, 1:N, or 1:1 `relate` |
| A run mark `<E>._run_<UC>` | `none`, `1`, .., `k-1` | A use case split into k transaction groups writes the entity before its last group ([Runs](#runs)) |
| A raised mark `<E>._raised_<Ev>` | `false`, `true` | `E` is a mark entity of a tracked event ([Triggers](#triggers)) |
| A counter `<E>._pending_<Ev>_<UC>` | `0`, `1`, .., `k`, `over` | As for a raised mark, for `triggers(Ev, UC).counted(k)` ([Counted triggers](#counted-triggers)) |
| `now` | `0..5` | Something compares a column to `now` |

A `@derived` column is never a variable. A construct that reads a Bool `@derived` column reads
its expression instead; one that reads an arithmetic `@derived` column is the error
`DerivedColumnRead`. An undriven comparison (`stock < selling` with no `drives`) is evaluated
on the Int values of its columns. A foreign key column is the owner variable, not an axis of
`states`.

A new slot starts as follows:

- An entity some use case creates starts `absent`, and the creating use case sets the columns.
- An entity no use case creates is **pre-existing**: every slot starts `present` with its
  columns at their base values, and the info `NoCreationPath` says so. Its variation columns
  start at any value ([vision.md](./vision.md#decisions) says why).
- The owner of a pre-existing child starts linked, by one of three rules:
  - `Any`: an N:1 child starts under any parent slot, or unlinked under `.optional()`.
  - `Identity`: a 1:1 child, or the child of a `1..*:1` key, starts under the parent slot of
    its own index. No two children share a parent, and every parent has a child.
  - `Base`: the owner of a created entity, and an optional owner whose parent some use case
    creates, starts unlinked.

  When some use case creates the parent, a mandatory owner still starts as `Any` or
  `Identity` says, under a parent slot that is `absent`, and draws
  `PreExistingChildOfCreatedParent`.

The base value of a column is its `@default`. Without one it is `null` for a `@null` column of
any type, as in the database, and otherwise the first Enum variant, `false`, or `0`.

### Actions

Each use case becomes one atomic action per transaction group of its writes. The actions
interleave: in each step any enabled action applies.

- **One group, one action.** `Pay` that moves an Order and its Payment in one FK-connected
  group is one action. Nothing runs between the two writes.
- **k groups, k actions in one run.** `tx.rs` infers the groups: the writes of each invoked
  API in `invokes` order, then the FK-connected groups of the direct writes, then each isolated
  write (`UsecaseTx::groups()`; the sequence diagram and the `SeparateTxInferred` warning read
  the same list). The use case becomes `<UC>_1` to `<UC>_k`, run in order on the rows the
  earlier steps bound ([Runs](#runs)). Any other action can run between two steps, since they
  are separate transactions:
  [`cross_order_payment_split`](../samples/formal-verification/cross_order_payment_split.rdra)
  runs `Cancel` between `Pay_1` and `Pay_2`.
- **A published entity is not in the lifecycle.** An entity published with
  `exposes(System, E).append()` or `.upsert()` gets no variables and no view. A direct write to
  it joins the use case's one internal group, so `Login` that creates `Session` and the
  published `LoginRecord` is one action. With no or several internal groups, or a published
  write through an API of its own, `check` reports `PublishedNotInSourceTx`.
- **A use case with no write has no action.**

<a id="runs"></a>**Runs of a split use case.** Every entity a step before the last writes gets a
run mark per slot, `<E>._run_<UC>`: the last step of a pending run that wrote the row, or
`none`. Step 1 takes only rows marked `none` and marks them `1`. Step i > 1 binds one row of
every entity steps 1 to i-1 wrote, as an `Earlier` parameter matched by the mark `i-1`, and
marks it `i`, or `none` at the last step. An `Earlier` parameter is the step's row of that
entity for keys, conditions, copies, and `after` assertions. It is matched by its mark, not its
existence, so a row step 1 deleted still finishes its run. One run per row is in flight at a
time: step 1 waits for a row in a run. A step whose guard fails waits too, since the earlier
transactions are committed, so a row no step can move shows as a `Deadlock` or a use case that
never runs. The mark is not an axis of `states`, and a delete does not reset it. It multiplies
the states only of a cone that holds the use case's steps, by the rows of the runs in flight.

<a id="triggers"></a>**Triggers.** `raises(UC, Ev)` and `triggers(Ev, UC2)` order the two use
cases: `UC2` runs only after `UC` ran, on the rows `UC` ran on (the design is in
[event order](./design/event-order.md)).

- **Tracked and free events.** An event is *tracked* when every `raises` of it names a use case
  with an action. An event with no `raises`, raised by an external system, or raised by a use
  case that writes nothing is *free*, as is a timer. A use case started only by free triggers,
  or by none, runs whenever its states and guards hold; one a free trigger starts beside a
  tracked event is free too. `performs(Actor, UC)` adds no trigger.
- **The mark.** The entities that the last step of every raiser binds are the event's
  candidates, and the candidates some triggered use case reads are its *mark entities*. Each
  gets a raised mark per slot, `<E>._raised_<Ev>`: `false` at the start, `true` once a raiser
  ran on the row, and never `false` again. The last step of each raiser sets it on every
  parameter it has on `E` and never waits. Raisers that bind no entity in common make the event
  free, and the info `UnanchoredTrigger` says so.
- **The guard.** The first step of a triggered use case reads the mark on each parameter that
  is the raiser's row: the step's only parameter on `E`, either its own row of `E` or a
  `Parent` through a key some raiser's last step also binds `E` through. It prints as `event
  <Ev> was raised on the bound row (triggers)`. Two parameters on `E` (`.role(src)` and
  `.role(dst)`) get no guard. With no such parameter the step binds one extra `Triggered`
  parameter, a present row of the first candidate that one direct key links to a row of the
  step ([Joins](#joins)); with no such key it is any raised row, and the info `UnjoinedTrigger`
  says so. It prints as `needs: an existing <E> its event was raised on`.
- **Several events.** A use case several tracked events trigger has one copy of its first step
  per event, `<UC>_by_<Ev>`. The variants are one use case to every finding: `Runs_<UC>` and
  `Arm_<UC>_<from>_<to>` report only when no variant runs or takes the arm, and a variant that
  never runs while another does is the info `TriggerNeverRuns`. `after(UC).assert` is checked
  after the last step of each variant.
- **Findings.** A row whose only way out is a triggered use case waiting for its mark is stuck,
  with the mark guard among what blocks it. A use case whose mark never holds is `NeverRuns`,
  which a self-triggering use case and an event cycle with no free entry get.
- **The mark counts no events.** Two raises on a row are one mark, and a handler that ran once
  can run again without a new raise. The order is "not before the first raise on these rows".
  A use case that in the domain also starts without the event states that with a second event
  ([A use case that also starts on its own](./language-reference.md#a-use-case-that-also-starts-on-its-own)).
  The mark is not an axis of `states`, a delete does not reset it, and it costs one Bool per
  slot only in a cone that reads it
  ([Exploration by cone of influence](#exploration-by-cone-of-influence)).

<a id="counted-triggers"></a>**Counted triggers.** `triggers(Ev, UC).counted(k)`, with `k` from
1 to 16, replaces the latch of that one edge with a counter
([design](./design/counted-events.md)). Each mark entity `E` gets `<E>._pending_<Ev>_<UC>` per
slot, `0` at the start. The last step of every raiser takes it from `i` to `i + 1` on its row,
and from `k` to `over`, and never waits. The first step of `UC` runs only where the counter is
not `0` and takes it from `i` to `i - 1`. At `over` a raise changes nothing and the handler runs
as under the latch. The state rule `EventOverflow_<E>_<Ev>_<UC>` says no present row is at
`over`, so a raise with `k` already pending is a violation with a trace. The counter belongs to
its edge: a latch edge of the same event keeps the latch, and two counted handlers have a
counter each. It is not an axis of `states`, and a delete does not reset it.

`.counted` is the error `UncountedTrigger`, naming the reason, on a free event, on a handler a
free trigger also starts or that writes nothing, on raisers with no entity in common, on a
handler that raises the event itself, or on a raiser that binds a mark entity twice. An edge
whose raiser or handler the run's scope leaves out is not checked in that run.

**Parameters and effects.** An action takes one parameter per entity it writes. A `creates`
takes the lowest `absent` slot; an `updates` or `deletes` takes any `present` slot. The effects
apply together to the pre-state:

| Source | Effect and guard |
|---|---|
| `creates(UC, E)` | The slot becomes `present`, with the columns at their base values; the use case's `sets` and transitions then apply. A `creates(Api, E)` reached through `invokes(UC, Api)` counts as a creation by `UC`. |
| `deletes(UC, E)` | The slot becomes `deleted` and its columns return to the base value; its keys keep the rows they name. A deleted slot is never reused. Per key to `E` whose `.on_delete` is unset, `restrict`, or `no_action`, a guard: no present child names the slot, apart from a child the same step deletes, unlinks, or relinks. A child of a `cascade`, `set_null`, or `set_default` key stays present and names the deleted slot, since the lifecycle does not run those actions; such a key gets no `DanglingKey_*`, the warning `DeleteLeavesChildren` names each use case that deletes a `cascade` parent and not the child, and the warning `DeleteLeavesChildKeys` each that deletes a `set_null` or `set_default` parent and neither deletes the child nor moves its key. An `E` with a `@soft_delete` column gets no guard. |
| `sets(UC, E.col == val)` | `col := val`. A `sets` counts as a write even without a CRUD edge. |
| `sets(UC, E.col == F.src)` | `col` takes the value of `F.src` in the row of `F` the step binds (see Copies below). |
| `drives(UC, E.col op rhs, bool)` | The comparison variable becomes `bool`. |
| `links(UC, Child, Parent)` | Guard: the owner is unlinked. Effect: the owner becomes the parent row the step binds (see Relinking). Counts as a write of the child. |
| `relinks(UC, Child, Parent)` | Guard: the owner is linked (none on a mandatory key). Effect: the owner becomes the parent row the step binds. Counts as a write of the child. |
| `unlinks(UC, Child, Parent)` | Guard: the owner is the parent row the step writes (or an earlier step of its run wrote), else linked. Effect: the owner becomes unlinked. Counts as a write of the child. |
| `transitions(UC, E.col, a -> b)` | Guard `col == a`, effect `col := b`. The arms of one use case on one column make one guard and one effect. Arms from one state to several (`a -> b`, `a -> c`) are a choice. |
| `transitions(UC, E.col, (a, b) -> z)` | One arm per listed source, as if each were written on its own line; a `.when(Cond)` after the list guards each arm. |
| `transitions(UC, E.col, a -> b).when(Cond)` | The arm is taken only when the body of `Cond` holds. The column gets one more guard, on the pre-state and the successor: `\/ (col = a /\ col' = b /\ body)` over its arms (an unguarded arm's body is `TRUE`). |
| a variation column of a created instance | Any value of the variation, unless the creating use case sets it. `@default` never pins it; the info `DefaultOnVariationIgnored` says so. |
| `chooses(UC, E.col)` | Any value of the variation: one successor per value. A column nothing reads has no variable, so the `chooses` adds no successor. |
| `chooses(UC, E.col, v).when(Cond)` | One of the listed values; a guarded value only when the body of `Cond` holds, as a guard on the successor. |
| `sets(UC, E.col == v).when(Cond)` | One listed value per line, read as a guarded `chooses` value. No line enabled, no successor. An unguarded `sets` or a copy of the same column is `ConflictingEffects`. |
| a column the use case does not write | Unchanged. |

**Choices.** An action has one successor per choice it makes. A `transitions` choice, a
`chooses`, and a variation column of a created instance fork the post-state after every other
effect, and two choices multiply. The guards read the pre-state, and `after(UC).assert` is
checked on every successor. An unguarded `sets` is never a choice: two values for one column in
one use case are `ConflictingEffects`.

**Arm guards.** `.when(Cond)` filters the successors of a choice: the explorer forks every arm
or listed value and keeps those whose guard held. So the use case runs when some arm is enabled,
the choice among enabled arms stays free, and an action whose every arm is disabled has no
successor, which `NeverRuns` reports. A guarded arm no edge takes, while its use case runs and
some instance holds its `from`, is the warning `ArmNeverTaken`; a guarded value no edge picks is
`ValueNeverChosen`. One cone that takes it is enough.

**When a guard reads.** An arm, value, or `applies_to` guard reads an updated instance on the
pre-state and a created instance on the post-state, so a guard on the chosen column of a created
instance sees the chosen value. The body's columns become variables.

**Owners.** An action that writes two entities joined by a `relate` binds them. A created child
gets its parent's slot as its owner, and an updated child must belong to the parent the action
also writes. A mandatory `relate` adds the parent as a parameter when a created child needs one;
an `.optional()` one does not, so the child is created unlinked unless the action writes the
parent or a `holds` guard compares a column of the parent through that key ([Joins](#joins)). An
unlinked owner is not linked for `.along`, quantifiers, or owner guards, so a guard that reads
such a parent only through `.has/.none(...)` never holds, and the `Runs_<UseCase>` warning adds a
`note:` row. Compare a parent column in a `holds` guard, write the parent, or drop `.optional()`.

A parent parameter is a row the action reads and is never relinked. Each key has its own parent
parameter, so two keys from `Order` to `Address` (`ship_to`, `bill_to`) range independently. A
parent the action writes is the child's parent only on the keys it takes: the one key between
them, or among several, each key a `.role` of the use case's `holds` guards or copies names. On
every other key the child is bound as if the action did not write the parent.

**Relinking.** An action that creates the parent and updates an existing child links the child
to the new parent. An `.optional()` key adds the guard `owner == null`, so a linked child keeps
its first parent; a mandatory key moves the child. `links` follows the same rule
(`GuardWhy::Link`, "the child is not linked to a parent yet"), so no `links` moves a child.
`relinks` moves one: guard `owner != null`, and the new owner is the parent row the step binds,
the one it already has included. `unlinks` frees one. One use case states one of the three per
key (`ConflictingLinks`).

<a id="joins"></a>**Joins.** A `.along(A, B)` hop, a `.has/.none(...)`, the link between a
`holds` body's instances, and a copy source each read the owner of one key.
`SemanticModel::join_key` picks it: the one key between the two entities, or among several the
one a `.role(role)` names. Several keys and no selector is the error `AmbiguousOwnerLink`
([Which key a join follows](./language-reference.md#which-key-a-join-follows)). A `Triggered`
parameter is linked by the one direct key between its entity and a row of the step; `triggers`
takes no `.role`, so with several keys or none the parameter is unjoined (`UnjoinedTrigger`).

**Conditions as guards.** `applies_to(Condition, UseCase)` with a `holds` body, or
`applies_to(Condition, Api)` for each use case that invokes the API, guards the use case's
action:

- An atom on an entity the action writes reads that parameter, before the effects, or as the
  action leaves it for a created entity.
- An atom on a parent the action only reads follows the key [Joins](#joins) picks, with the
  body's `.role`. Several parent parameters of the entity and no `.role` is
  `AmbiguousOwnerLink`.
- An atom on any other entity reads an existing instance. Atoms on one such entity share one
  instance, which a direct `relate` links to the action's parameters; without one it is
  independent. A compared instance no chain of links reaches is the warning
  `UnjoinedGuardRead`: any present row satisfies the guard.
- With `.along(A, B, ..)`, each path entity the action does not bind is such an instance,
  linked by each hop's key. A path entity that a `.has`/`.none` quantifies and no atom reads is
  bound inside the quantifier: `Forall` for `.none`, `Exists` for `.has`.
- In a use case with k groups, the body is the precondition of the run: step 1 carries it. A
  body applied to an API is the precondition of that API's step, in every use case that
  invokes it, and reads the run's rows through `Earlier` parameters. A `.when` on an arm or a
  value guards only the step that writes its column. No step re-reads an earlier step's guard.
- `.has(...)` and `.none(...)` quantify over the related instances present in the pre-state,
  linked through the key [Joins](#joins) picks. `.has(Related)` and `.none(Related)` read the
  owner and the existence only.
- `applies_to(Condition, System)` adds no guard, and a condition with no body is a trace link.
  A body applied to a use case that writes nothing is `UnbindableCondition`.

The `raises`/`triggers` order is a guard of its own ([Triggers](#triggers)). A condition that
only restates the order is no longer needed; one that is kept documents the precondition the
handler relies on
([`triggers` and event-driven BUC chaining](./language-reference.md#triggers-and-event-driven-buc-chaining)).

**Copies.** `sets(UC, E.col == F.src)` reads the row of `F` a condition atom would read, but only
a row bound to the step: a parameter of the action for `F` (an `Earlier` one for an `F` an
earlier step wrote), or the target's parent through the key `join_key` picks, which joins as a
parameter with the guard `owner[target] == F`. Any other `F` is the error `UnboundCopySource`,
and the copy is not applied. The copy is a choice of the target's value filtered by
`col'[p] = src[q]` (`GuardWhy::Copy`). The analysis checks that every source value is a target
value.

**Clock.** When something compares a column to `now`, a `Tick` action adds one to `now` until
the top of the Int range. A `Date` or `DateTime` column is a point on that clock:
`sets(UC, E.due == 3)` places it at 3. A deadline that depends on the row is one guarded `sets`
per case, and the timer's use case waits with a guard such as `holds(Due, E.due <= now)`
([A deadline that depends on a reason](./language-reference.md#a-deadline-that-depends-on-a-reason)).

### Checks

| Declaration | Check |
|---|---|
| `forbidden`, `invariant`, `required`, `exclusive` | An invariant over the present slots of the entity, in every reachable state. A `.when(Cond)` adds the comparisons of the condition's `holds` body to the rule. |
| Multi-entity `forbidden`, `invariant` | An invariant over every combination of present slots. `.along(A, B)` keeps the combinations the owners link. |
| `invariant(E).when(...).has/none(R.col == v)` | An invariant over the present slots of `E` and the related slots of `R`, linked by an owner or independent without one |
| `relate(Child, Parent, 1:1)` | `Unique_<Child>_<role>`: no two present children belong to one parent slot. Vacuous at one instance. |
| `@unique`, `@unique(a, b)`, `relate(A, B, N:M).via(J)` | `Unique_<Entity>_<columns>` ([Unique column sets](#unique-column-sets)) |
| `relate(Child, Parent, ..)` with no `.on_delete`, `restrict`, or `no_action`, whose parent a use case `deletes` | `DanglingKey_<Child>_<role>`: every present child names a present parent, or none under `.optional()`. None for a `cascade`, `set_null`, or `set_default` key, a parent with a `@soft_delete` column, or a pre-existing child of a created parent. |
| `relate(Child, Parent, 0..3:1)` | `MaxCard_<Child>_<role>`: no present parent owns more than 3 present children. It needs 4 child slots to fail; below that it is `Vacuous_MaxCard_*`. |
| `relate(Child, Parent, 1..*:1)` | `MinCard_<Child>_<role>`: every present parent has a present child, in every state. The info `MinCardNeedsCreate` names each use case that creates the parent and no child in one group; a child the action links by update counts. |
| `col: T @check(expr)` | `Check_<Entity>_<column>_<n>`: every present slot satisfies `expr`. A null operand satisfies no comparison but `!=` ([Null](./language-reference.md#null)), so `qty: Int @null @check(qty >= 1)` fails while `qty` is null. A created slot starts at the base value, so `@check(qty >= 1)` without `@default(1)` or a creating `sets` fails at creation. |
| `after(UC).assert(...)` | A predicate on the state each run of `UC` leaves ([Temporal anchor assertions](#temporal-anchor-assertions)) |
| `property` (`always`, `eventually`, `leads_to`, `stable`, `unchanged`, `precedes`, `can_reach`) | Not evaluated by `check` and `states`, which print a `note:` to use `verify`. `verify` checks every form ([Temporal properties on a cone](./formal-verification.md#temporal-properties-on-a-cone)). |

Every check reads the instances that exist; a rule over an entity with no instance holds in that
state. Comparisons with null follow [Null](./language-reference.md#null): two null columns are
not equal (#360), and a `@null` Enum or Bool column holds null as a value of its own (#367). A
logical `@derived` column is neither true nor false while an operand is null.

### Unique column sets

<!-- constrained-by ./language-reference.md#column-annotations -->

Each unique set whose every column the lifecycle holds by value becomes the invariant
`Unique_<Entity>_<columns>`, the columns joined by `_`: a `@unique` column, a `@unique(a, b)`,
or the two keys of an N:M junction (`Unique_Enrollment_course_id_student_id`). Two present rows
that agree on every column break it.

- **Which columns count.** An Enum or Bool column, a variation column a rule names, and an Int,
  Money, Decimal, Date, or DateTime column a rule compares. A key column reads as its owner, and
  the set must hold every column of a composite key. A set with a column that has no value
  variable, such as a `String`, is left to the database, with no finding; most `@unique`
  columns in `samples/` are `String`.
- **Null.** A row whose column is null agrees with no row, as SQL counts NULLs as distinct.
  That covers a `@null` Int, Enum, or Bool and an unlinked `.optional()` key.
- **Left to other rules.** A set that is a 1:1 key is `Unique_<Child>_<role>` already. A set on
  an entity with a `@soft_delete` column is not checked, since its index covers live rows only.

A use case that writes the set needs a guard that keeps it free, such as
`holds(AmFree, Room.open == true).none(Booking.slot == am)`
(`samples/formal-verification/unique_booking_guarded.rdra`). At one instance per entity the rule
holds with no `Vacuous_` warning.

### Rules that hold vacuously

A check that holds may still say nothing. `check` and `verify` look, on the check's own cone,
for a reachable state that meets each *premise*, and warn `Vacuous_<name>` when none does:

| Rule | Premises |
|---|---|
| Every state rule | Rows of the entities it ranges over exist, linked as its `.along` says. `Unique_*` needs two rows of the child, so it has no premise at one instance per entity. |
| `invariant(...).when(G)`, a quantifier rule with guards | The guards `G` hold for some rows. |
| `invariant(E).none(R...)` | A row of `R` exists beside a row of `E`. |
| `after(UC).assert(...)` | `UC` runs to its last step, and some such step changes the state: a step that changes nothing is not checked, so a last step that only ever stutters leaves the assertion unchecked (`… runs its last step only as a step that changes nothing`). When the assertion reads an entity the step does not bind, some step that changes the state ends with a row of it. |
| `property` | Rows of its entities exist together, and some meet the condition the form speaks from: the trigger of a `leads_to`, what a `stable` keeps, the `.when` of an `unchanged`, the second condition of a `precedes`. |
| `forbidden` of several conditions | Each condition holds for some row, on its own. |

The conjunction of a `forbidden`'s conditions is what the rule claims, so it is no premise.
`forbidden(Order, status == void, flagged == true)` is vacuous when no order is ever `void`,
which is often a typo or a state no use case enters. The warning names the rows each premise
asks for: `no reachable state has a row of Order with Order.status=void`. It never fails a
command, and it is not raised for a violated rule, on a cone a cap stopped, or under `--buc`. At
one instance per entity a rule may be vacuous and still break at two, and the warning says so.
`verify` reads the premises off the cones that decide each check.

---

## Bounds

The bounds are inputs of the system, and every result states them.

| Command | Instances per entity | Int range |
|---|---|---|
| `states`, `check` | `--instance-count` (default 1) | `0..--int-range-max` (default 5) |
| the generators | 1 | `0..5` |
| `export --kind tla`, `verify` | `--instance-count` (default 2) | `0..--int-range-max` (default 5) |

`--instances <Entity>=<n>` gives one entity its own count
([Per-entity instance counts](./cli-reference.md#per-entity-instance-counts)). `states` and
`check` print their bounds on stderr first:

```
bounds: 1 instance per entity, Int 0..5; no state cap per cone, memory 4GiB; entities a check does not read are unbounded
```

With one slot, an entity is created once, and deleting it does not free the slot. An entity a
target does not read has no slot limit ([Exploration by cone of influence](#exploration-by-cone-of-influence)).
An Int literal outside the range, in a `sets`, a rule, a condition, or the `@default` of a column
a rule reads, is `IntLiteralOutOfBounds`.

---

## Exploration by cone of influence

The system is not explored whole. Each **target** (an entity view, an invariant, an `after`
assertion) is explored on its cone of influence:

- **Cone.** The smallest set of variables that holds the target's variables and, for every
  action that writes one of them, every variable the action reads: its guards, the existence of
  the entities it binds, its transition sources, and the clock. An action that writes nothing
  in the set drops out, and an action keeps only its effects on the set. A copy's guard is read
  only when the cone holds the column it writes. A set that lists every value of its column is
  true when the system is built, so `.none(Child.status in (<every value>))` and `.none(Child)`
  read the link alone (#71, #73). Only an Enum, Bool, null-or-present, or existence column
  counts: an Int column's values end at `int_max` and `verify --prove` has no top, so a set
  that lists `0..int_max` stays a set (#425).
- **Exact.** The reachable states of the cone are those of the whole system projected onto it,
  with one exception: an action of the cone that creates an entity the cone does not read drops
  that creation and never runs out of its slots, so the cone reaches a superset. The test
  `cone_exploration_equals_the_projection_of_the_whole_system` in
  `crates/rdra-ish-core/src/lifecycle/tests/cone.rs` checks both.
- **Raised marks.** A cone reads a raised mark ([Triggers](#triggers)) exactly only when every
  raiser's last step is in the cone by its own writes, or the reader is an action the cone keeps
  (an entity view keeps every action with a written parameter on the entity). Otherwise the mark
  stays out: the handler's guard on it and its `Triggered` parameter are dropped, and the handler
  runs unordered there. The cone then reaches a superset, so an `ok` is sound, and a finding can
  be a run the order prevents; its trace carries the `note:` row `runs <UC> without <Ev> having
  been raised`. The view's own witnesses, deadlocks, and `NeverRuns` are judged with the order.
  A counter of a [counted trigger](#counted-triggers) is never left out, so a cone of a raiser
  holds the handler too.
- **Shared.** Targets with the same cone are explored once.
- **Reduced in `verify`.** `verify` explores a cone reduced by its independent sub-lifecycle
  ([Reduction](./formal-verification.md#reduction-by-an-independent-sub-lifecycle)). `check` and
  `states` explore it unreduced.

BFS starts from every initial state, applies every enabled action, and stops when no new state
appears or a cap is hit. A cone that answers only invariants and `after` assertions also stops
once every check has a counterexample (#213); each is the first BFS found, so the shortest. Such
a cone reports no cap and no vacuity, and its `verify` line counts the states reached before it
stopped. A cone that also answers an entity view, a `can_reach`, or a temporal property runs to
the end, and so does a safety cone whose system is also an entity or property cone, which is
explored once for all of them (#209); its line then counts the whole cone. A cone evaluates only the checks something reads from it (#251); a check is always
reported from its own cone, so this changes no finding.

Two caps and the memory limit stop a cone:

- `--max-patterns` caps the distinct patterns of one entity view. Default:
  2^(10 + 8 * (n - 1)) for `n` = `--instance-count`: 1,024 at one instance, 2^18 at two. Only a
  run that builds an entity view gathers patterns, so it stops no `verify` run.
- `--max-states` caps the joint states of one cone. Default: none.
- `--memory` (default `4GiB` for `check` and `states`) stops a cone whose stored states pass the
  limit, which the cones of one run share ([Memory limit](./cli-reference.md#memory-limit)).

The generators (`codegen`, `next`, and the lifecycle gate of `diagram`, `export`, `list`, and
`csv`) explore at `check`'s default caps and `memory` under `[check]` in `rdra.toml`, with no
`--memory` flag. A model `check` passes at its defaults explores the same in them.

At a cap the view is marked `truncated`, and `PatternCapReached`, `StateCapReached`, or
`MemoryCapReached` is an error, since a rule checked on part of a cone is not sound. The
`StateCapReached` and `MemoryCapReached` messages name what the reached states vary in, with the
values reached out of each domain, and the use cases that first reached the most states. A
variable of another entity in that list was pulled into the cone by a guard or a transition
source that reads it:

```
$ rdra-ish check samples/patent-ops --max-states 1000
...
error[StateCapReached_OfficeAction]: state derivation for entity 'OfficeAction': state cap 1000 reached; results are incomplete: the cone has 23 variables, 20 of them vary (values reached/domain: PatentApplication.status 12/12, InventionDisclosure.status 8/8, OfficeAction.status 6/6, ...); new states first reached by: DraftResponse 110, ExpireResponseTerm 107, ...; raise --max-states, or narrow with --entity or --buc
```

`MemoryCapReached` also gives the bytes each state took and an estimate of the memory the cone
needs; [Memory limit](./cli-reference.md#memory-limit) explains the estimate. In `verify` a
stopped cone's findings are `not decided` instead. The example
`crates/rdra-ish-core/examples/lifecycle_measure.rs` prints the size of each cone of every
sample, at the bounds of `check` or of `verify`.

---

## Views

Each entity view lists the patterns some present slot holds, one per distinct assignment of the
entity's state columns.

| Field | Meaning |
|---|---|
| Pattern | Values of the entity's columns, in a present slot |
| `INITIAL` | A create action produces it, or it is the start of a pre-existing entity |
| `TERMINAL` | Some enabled action deletes a slot in it, or no enabled action moves a slot off it |
| `VIA` | The `(BUC, use case)` pairs of the actions on some path to it, the least set closed under the moves between patterns. The BUC is one that holds the use case, the first by id. |
| `reachable` / `bound` | The count of patterns, and the product of the column domain sizes |

An entity with no state column has one pattern, both initial and terminal.

### State models and witnesses

A lifecycle column, one that `transitions` names and that is not a `variation`, has a **state
model**. Its states are the endpoints of its `transitions` and the values a fresh instance starts
at. The null of a `@null` lifecycle column is not a state of the model. Each state is:

- **Witnessed**: some reachable state of the entity's cone has an instance at it.
- **`NoWitness`**: no reachable state has one. `check` prints it once, with each arm or creation
  that would enter it.
- **Not decided**: none was found, but the cone stopped at a cap or `--buc` left use cases out.
  It is listed under `not checked:` and is neither an error nor a warning.

Every other declared Enum variant that no pattern holds draws the warning
`UnreachableEnumVariant`.

```
$ rdra-ish check samples/formal-verification/unwitnessed_state.rdra
bounds: 1 instance per entity, Int 0..5; no state cap per cone, memory 4GiB; entities a check does not read are unbounded
error[Witness_Order_status_refunded]: state Order.status = refunded has no reachability witness: no sequence of use cases from the start reaches it
  state model: Order.status, entered by Refund (shipped -> refunded)   skills/rdra-ish-verify/samples/unwitnessed_state.rdra:31:1
...
```

Each pattern's **witness** is a shortest path from an initial state to the first state in BFS
order with an instance at the pattern. `states --format json` prints it per pattern.
`state_model_findings` in `crates/rdra-ish-core/src/lifecycle/findings.rs` computes the witnesses
and the findings below from one cone's graph.

### Use cases that never run

An action is one use case, or one transaction group of a split use case (`Pay_1`, `Pay_2`). A
generated name that collides with a use case's id or an earlier generated name takes the first
free `<name>_<k>` from `_2`. An action runs when some edge of a cone that keeps it carries its
label, self-loops included. An entity's cone keeps every action with a non-parent parameter on
the entity, with all its guards, so it decides the action exactly. `NeverRuns` is the warning for
an action no such edge carries, printed once per action.

The warning names only what blocks the action:

- **No instance.** A parameter finds no slot in any reachable state. Each prints as `needs:`.
- **Not at once.** Each parameter finds a slot in some state, but no state has them all. At one
  instance this is often the bound: in `min_card_met`, `AddLine` never runs in `check` and runs
  in `verify`.
- **No free slot while enabled.** The guards hold and the other parameters find instances, but a
  created entity has no free slot: `needs: a free E slot within the instance bound`.
- **Guards.** Each guard that holds under no binding of any reachable state prints as `guard:`
  with its site. A condition guard of a split use case that reads an entity only a later group
  creates ends in `(GuardReadsLaterStep)`.
- **Never together.** Every guard holds somewhere, but never all at once.

```
$ rdra-ish check samples/formal-verification/unwitnessed_state.rdra
...
warning[Runs_Refund]: use case Refund can never run: each of its guards holds in some reachable state, but never all of them in one
  declared: use case Refund   skills/rdra-ish-verify/samples/unwitnessed_state.rdra:16:1
```

A cone stopped at a cap, a `--buc` run, or an `--entity` run that selects no cone keeping the
action leaves it undecided, under `not checked:`. A cone that frees an unread entity's slots can
run an action the whole model cannot, so it can miss a warning, never add one.

### Deadlocks

An **end state** of a state model has no way out: no arm leads from it to another state. A state
the column declares with [`@end(...)`](./language-reference.md#column-annotations) is an end
state too, though an arm leaves it. A deletion leaves every state. A **deadlock** is a reachable
state of the entity's cone with a present instance at a state that is not an end state, from
which no path moves that instance: no edge changes its column or deletes it.

The check is per instance, so another use case that keeps running does not hide a stuck instance,
and an action disabled now and enabled later is no deadlock. It is linear in the edges, and each
`(entity, column, state)` is reported once, at the first stuck state in BFS order, with a shortest
path. `check` prints each way out with what blocks it, then the path:

```
$ rdra-ish check samples/formal-verification/deadlock.rdra
bounds: 1 instance per entity, Int 0..5; no state cap per cone, memory 4GiB; entities a check does not read are unbounded
error[Deadlock_Order_status_paid]: deadlock: Order#1 stays at status = paid forever, and paid is not an end state
  leaves paid: Ship (paid -> shipped)   skills/rdra-ish-verify/samples/deadlock.rdra:29:1
    guard: condition Packed             skills/rdra-ish-verify/samples/deadlock.rdra:33:1
  reached by (2 steps):
    1. Place [Order#1]                     skills/rdra-ish-verify/samples/deadlock.rdra:12:1
         + Order#1 (status=pending)
    2. Pay [Order#1]                       skills/rdra-ish-verify/samples/deadlock.rdra:13:1
         Order#1.status: pending -> paid   skills/rdra-ish-verify/samples/deadlock.rdra:28:1
warning[Runs_Ship]: use case Ship can never run: each of its guards holds in some reachable state, but never all of them in one
  declared: use case Ship   skills/rdra-ish-verify/samples/deadlock.rdra:14:1
FAIL: 1 error, 1 warning within 1 instance per entity, Int 0..5
```

- **At one instance.** A way out that needs a row of a created entity may find none, since the
  one slot was used. The error then adds a row saying the deadlock may clear at
  `--instance-count 2`, which `verify` runs.
- **At the bound.** When some way out lacks only a free slot of an entity it creates, in every
  state the instance stays stuck in, the deadlock is a warning:
  `warning[Deadlock_Order_status_awaiting_retry]: deadlock at the bound: ...; RetryPayment needs
  a free Payment slot within the instance bound`. When an invoked API does the creating, the
  message adds `RetryPayment creates Payment via api 'PaymentApi'`.
- **Not decided.** A cone stopped at a cap, or a `--buc` run, lists each state with a way out
  under `not checked:`.

TLC's own deadlock check stays off (`CHECK_DEADLOCK FALSE`): every bounded model ends with no
enabled action, and TLC cannot see one instance stuck while others move. The `StateModel_` specs
check each deadlock as an invariant instead
([State models with TLC alone](./formal-verification.md#state-models-with-tlc-alone)).

```
$ rdra-ish states samples/formal-verification/order.rdra
bounds: 1 instance per entity, Int 0..5; no state cap per cone, memory 4GiB; entities a check does not read are unbounded
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

`states` and `check` print the `bounds:` line on stderr, and `verify` on stdout.

---

## Constraint checking

A violation is reported only when a reachable state witnesses it; a rule that forbids an
unreachable state reports nothing. Each rule reports its first violation, which BFS finds at the
least depth: the diagnostic names the rule (`Forbidden_Order_0`), the state, and a shortest trace
([`check`](./cli-reference.md#check)).

- `forbidden(E, ...)` AND-s its conditions; a reachable state that matches all of them is
  `ConstraintViolated`. A comparison condition (`stock < selling`) reads the Int values, or the
  proposition when a `drives` drives it.
- `invariant(E).when(...).then(...)` AND-s the guards and the requirements; a state that meets
  every guard and misses a requirement is a violation.
- `required(E, ...)` holds when every reachable state matches all conditions, and
  `exclusive(E, ...)` when none matches two or more.

### Temporal anchor assertions

`after(UseCase).assert(...)` checks a predicate on the state each run of the use case leaves. A
use case split into k groups is checked once per run, after its last step
([Runs](#runs)), and the state between two steps is not checked.

```rdra
after(ExecuteCertIssue).assert(CertificateOrder.status == executed, ClientCertificate.status == active)
```

An entity the last step binds is read on that row: the row the step writes, the row an earlier
step wrote, or else its one parent parameter. An entity bound through several parent parameters
is read on every present row, since an `after` has no `.role`. An entity no step binds is read on
every present row too, with the warning `UnboundAfterRead`. A step that ends in a state that
fails the predicate is `AfterAssertViolated`. An `after` on a use case with no action is
`AssertOnInertUseCase`.

### Multi-entity and quantifier constraints

`forbidden(A, B, ...)` and `invariant(A, B)` range over every combination of present slots of `A`
and `B`; `.along(A, B)` keeps the combinations a `relate` links, and needs a `relate` for each
hop, else `UnbindableScope`. A path entity outside the rule is bound under the link.

`invariant(E).when(...).none(R.col == v)` and `.has(...)` range over the present slots of `E` and
the related slots of `R` linked by the key [Joins](#joins) picks, or independent without one.
With `.along(E, .., R)` every entity between is bound inside the quantifier: `Forall` for
`.none`, `Exists` for `.has`. The path runs from the anchor to the counted entity, each entity
once, or the analysis reports `QuantifierAlongShape`. At one instance per entity `states` sees
one `E` and one `R`; use `verify` for rules that need two. The syntax is in
[language-reference.md](./language-reference.md#entity-state-constraints).

---

## Diagnostics

`StateDiag` in `crates/rdra-ish-core/src/lifecycle/diag.rs` lists every diagnostic. `states`,
`check`, `export --kind tla`, and `verify` share the severities. An error makes the command exit
non-zero in every format and the generators refuse to emit, except that `MemoryCapReached`
refuses only `codegen` and the outputs that read the explored states, and is a note on the others.

Every diagnostic has a stable name, printed after the severity:
`error[Deadlock_Order_status_paid]: deadlock: ...`. `states --format table` and `--format json`
print `[error][<name>] ...`. A violated rule or `after` assertion keeps the name of its TLA+
check. The other names join the variant, the ids of the entities whose view reports the finding
(`<E>` below), and the key fields; any character outside `[A-Za-z0-9_]` becomes `_`. The `// expect`
headers of the samples list these names
([Sample expectations](./formal-verification.md#sample-expectations)).

| Diagnostic | Name | Severity | When |
|---|---|---|---|
| `UnreachableEnumVariant { column, variant }` | `UnreachableEnumVariant_<E>_<column>_<variant>` | warning | No reachable pattern holds a declared Enum variant that is not a state of a state model. |
| `NoWitness { entity, column, value, into }` | `Witness_<entity>_<column>_<value>` | error | No reachable state holds a state of a state model. Not raised when a cap stopped the cone or `--buc` drops use cases. |
| `Deadlock { entity, column, value, at, exits, trace }` | `Deadlock_<entity>_<column>_<value>` | error, or warning at the bound | A present instance at a state that is not an end state, from which no path moves it ([Deadlocks](#deadlocks)). Not raised when a cap stopped the cone or `--buc` drops use cases. |
| `VacuousRule { name, unmet, one_instance, site }` | `Vacuous_<name>` | warning | The rule, assertion, or property holds and no reachable state meets one of its premises ([Rules that hold vacuously](#rules-that-hold-vacuously)). |
| `NeverRuns { action, origin, site, why }` | `Runs_<action>` | warning | No edge of any entity cone that keeps the action carries it. `why` is `NoInstance`, `Apart`, `Crowded`, `Guards`, or `Together` ([Use cases that never run](#use-cases-that-never-run)). |
| `ArmNeverTaken { action, column, from, to, conditions, site }` | `Arm_<action>_<from>_<to>` | warning | A `transitions(..).when(Cond)` arm no edge takes while its action runs and some instance holds its `from`. |
| `ValueNeverChosen { action, column, value, conditions, site }` | `Choice_<action>_<column>_<value>` | warning | A `chooses(.., value).when(Cond)` value no edge picks while its action runs. |
| `UnwrittenConstrainedVar { column }` | `UnwrittenConstrainedVar_<E>_<column>` | warning | A rule or guard reads a column no use case writes, so it checks only the start value. |
| `NoCreationPath` | `NoCreationPath_<E>` | info | No use case creates the entity. It starts present. |
| `MinCardNeedsCreate { usecase, child }` | `MinCardNeedsCreate_<E>_<usecase>_<child>` | info | A use case creates the parent of a `1..*` key and no child in the same group. |
| `DeleteLeavesChildren { usecase, parent, child }` | `DeleteLeavesChildren_<E>_<usecase>_<child>` | warning | A use case deletes the parent of a `.on_delete(cascade)` key and not the child, where the database the SQL export describes removes the children. Add `deletes(UC, Child)`. Not drawn for a `@soft_delete` parent. |
| `DeleteLeavesChildKeys { usecase, parent, child, action }` | `DeleteLeavesChildKeys_<E>_<usecase>_<child>` | warning | A use case deletes the parent of a `.on_delete(set_null)` or `.on_delete(set_default)` key, and neither deletes the child nor moves its key, where the database nulls or resets the key. Add `unlinks(UC, Child, Parent)`. Not drawn for a `@soft_delete` parent. |
| `DefaultOnVariationIgnored { column, value, created }` | `DefaultOnVariationIgnored_<E>_<column>` | info | A variation column has a `@default`, and some row starts at any value. |
| `MinCardOfPreExistingParent { child }` | `MinCardOfPreExistingParent_<E>_<child>` | warning | The parent of a `1..*` key starts present and every child absent, so `MinCard_*` fails at the start. |
| `CountAboveBound { rule, entity, n, slots }` | `CountAboveBound_<E>_<rule>` | warning | An `.at_least(n, ..)` or `.exactly(n, ..)` counts more rows than the entity has slots. The message names the `--instances` that gives it `n`. |
| `PreExistingChildOfCreatedParent { parent }` | `PreExistingChildOfCreatedParent_<E>_<parent>` | warning | No use case creates the entity, but one creates the parent of its mandatory `relate`. |
| `IdentityStartSlots { parent }` | `IdentityStartSlots_<E>_<parent>` | error | The entity starts linked one-to-one to its parent's slots, but the bounds (`--instances`) give the two a different slot count. |
| `ConflictingEffects { usecase, column }` | `ConflictingEffects_<E>_<usecase>_<column>` | error | One use case assigns two values to one column. |
| `DoubleModeledEnum { column, usecase }` | `DoubleModeledEnum_<E>_<usecase>_<column>` | error | A use case that does not create the entity `sets` a lifecycle column that `transitions` drive. |
| `ContradictoryDrivenComparison { usecase, proposition, driven }` | `ContradictoryDrivenComparison_<E>_<usecase>_<proposition>` | error | A use case drives a comparison to one truth while its own Int `sets` make it the other. |
| `DrivenComparisonColumnWritten { usecase, column, proposition, drivers }` | `DrivenComparisonColumnWritten_<E>_<usecase>_<column>` | warning | A use case `sets` a column a driven comparison reads without driving the comparison. |
| `IntLiteralOutOfBounds { value, max }` | `IntLiteralOutOfBounds_<E>_<value>` | error | An Int literal or integer `@default` lies outside `0..max`. |
| `DomainTooLarge { column, values }` | `DomainTooLarge_<E>_<column>` | error | A column has more than the 256 values a state holds: a nullable Int at `--int-range-max 255`, a nullable Enum of 256 values, or an Enum of 257. |
| `UnsupportedComparison { comparison }` | `UnsupportedComparison_<E>_<comparison>` | error | An order comparison has a non-numeric operand, or `==` / `!=` compares columns of different domains. |
| `UnbindableScope { constraint }` | `UnbindableScope_<E>_<constraint>` | error | An `.along` names a hop no `relate` binds, or misses an entity the rule names. |
| `UnbindableCondition { condition, usecase }` | `UnbindableCondition_<E>_<condition>_<usecase>` | error | A condition body is applied to a use case that writes nothing, or its `.along` cannot be bound. |
| `UnjoinedGuardRead { condition, usecase, entity, fix }` | `UnjoinedGuardRead_<E>_<condition>_<usecase>` | warning | A `holds` body compares an entity no key joins to a row the use case binds; `fix` names the `.along(...)` to add. |
| `UnjoinedWrite { usecase, a, b, path }` | `UnjoinedWrite_<usecase>_<a>_<b>` | warning | One step writes rows of `a` and `b` that nothing joins, while their key path runs through entities the step does not bind, so one run may write rows of different parents. One instance per entity cannot show it; two can. Fix it by writing the entities between, or silence it with `.apart()`. |
| `GuardReadsLaterStep { condition, usecase, entity, step, apis }` | `GuardReadsLaterStep_<E>_<condition>_<usecase>` | warning | A `holds` body on a split use case compares an entity only a later group creates. The message names `applies_to(Cond, Api)` for that group's APIs. |
| `UnboundAfterRead { anchor, entity }` | `UnboundAfterRead_<E>_<anchor>` | warning | An `after` atom reads an entity no step of the run binds. |
| `UnanchoredTrigger { event, usecase, raisers, site }` | `UnanchoredTrigger_<E>_<event>_<usecase>` | info | The raisers bind no entity in common, so the event is free ([Triggers](#triggers)). |
| `UnjoinedTrigger { event, usecase, entity, site }` | `UnjoinedTrigger_<E>_<event>_<usecase>` | info | The triggered use case binds no row the event is raised on, and no key joins one, so any raised row lets it run. |
| `TriggerNeverRuns { usecase, event, site }` | `TriggerNeverRuns_<E>_<usecase>_<event>` | info | A use case several events trigger never runs after this event, while it runs after another. |
| `UncountedTrigger { event, usecase, why, site }` | `UncountedTrigger_<E>_<event>_<usecase>` | error | A `.counted(k)` edge has no counter; `why` names the reason ([Counted triggers](#counted-triggers)). |
| `UnboundCopySource { usecase, column, source, reason }` | `UnboundCopySource_<E>_<usecase>_<column>` | error | A copy names an `F` the step binds to no one row. The copy is not applied. |
| `AmbiguousOwnerLink { statement, a, b, roles, rewrite }` | `AmbiguousOwnerLink_<E>_<a>_<b>` | error | A join between entities several keys join names no `.role`. The message gives the statement with the `.role` to add. |
| `DerivedColumnRead { column }` | `DerivedColumnRead_<entity>_<column>` | error | A rule, condition, assertion, or property reads an arithmetic `@derived` column. |
| `ConstraintViolated { name, ... }` | the rule's name, such as `Forbidden_Order_0` | error | A reachable state breaks a rule, a unique set, or a counted trigger's `EventOverflow_<E>_<Ev>_<UC>`. |
| `AfterAssertViolated { anchor, ... }` | `AfterAssert_<anchor>_<i>` | error | A step of the anchor use case ends in a state that fails the assertion. |
| `PropertyViolated { name, kind, instance, witness, lasso, ... }` | the property's name, such as `NeverShipped` | error | A property fails. Only `verify` reports it. |
| `GoalUnreachable { name, instance, confined, states, .. }` | the property's name, such as `OrderCanFinish` | error | A `can_reach` property fails; `confined` lists what the goal reads in the states left. Only `verify` reports it. |
| `AssertOnInertUseCase { anchor }` | `AssertOnInertUseCase_<E>_<anchor>` | error | `after(UC).assert` names a use case with no action. |
| `FairOnInertUseCase { property, usecase }` | `FairOnInertUseCase_<E>_<property>_<usecase>` | error | A property's `.fair(...)` names a use case with no action. |
| `GoalMetByDelete { property, entity }` | `GoalMetByDelete_<E>_<property>_<entity>` | info | An `eventually` / `leads_to` goal holds on the base values of a bound row some use case deletes, so the delete meets it; `.present()` asks for a present row. |
| `PatternCapReached { cap, bound }` | `PatternCapReached_<E>` | error | An entity view exceeded `--max-patterns`. |
| `StateCapReached { cap, growth }` | `StateCapReached_<E>` | error | A cone exceeded `--max-states`. The message names what the states vary in. |
| `MemoryCapReached { limit, states, growth, held, .. }` | `MemoryCapReached_<E>` | error | A cone outgrew the [memory limit](./cli-reference.md#memory-limit). The message estimates what finishing needs. In `verify`, the cone's findings are `not decided` instead. |
| `TimeBudgetReached { states }` | `TimeBudgetReached` | error | The whole-model run of a property in `verify` passed `--temporal-budget`. It prints no line of its own: the property is `not decided`. |

---

## Output formats

`rdra-ish states` renders the views in one of three formats (`--format`):

- `table` (default): one table per entity with an axis legend, one column per axis, and the
  `INITIAL`, `TERMINAL`, and `VIA` columns. An Int axis prints its range (`stock[0..5]`), and an
  entity with no state column prints `(no state axes)`.
- `csv`: one row per pattern and column, with `entity_id`, `entity_label`, `pattern_idx`,
  `column`, `value`, `is_initial`, `is_terminal`, and `via`.
- `json`: one object per entity with `axes` (each with its `domain`), `patterns`, `diagnostics`,
  `no_creation_path`, `reachable`, `truncated`, and the entity id and label. Each pattern has a
  `witness`: `start`, `steps` (each with `action`, `usecase`, `bound`, `changes`, and `site`),
  `over_bound`, and `free_triggers` (each use case that ran without its event having been
  raised). A change is `create`, `delete`, `set` (with its `site`), or `tick`.

Every format honors `--entity <id>`, `--buc <id>`, `--max-patterns <n>`, `--max-states <n>`, and
`--memory <size>` ([Partial runs](./cli-reference.md#partial-runs)). Whether an entity is
pre-existing is decided on the whole model.

---

## See Also

- [language-reference.md](./language-reference.md): the syntax of entities, predicates, and rules.
- [cli-reference.md](./cli-reference.md): the `states` subcommand and its options.
- [formal-verification.md](./formal-verification.md): `verify`, TLA+, temporal properties, and proofs.
- [vision.md](./vision.md): the lifecycle rules this layer implements.
