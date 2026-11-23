# Formal Verification

<!-- derived-from ./state-derivation.md -->
<!-- derived-from ./language-reference.md#entity-state-constraints -->
<!-- derived-from ./language-reference.md#temporal-path-properties -->
<!-- derived-from ./cli-reference.md#export -->
<!-- derived-from ./cli-reference.md#verify -->

Three commands check a model, and each one checks different things:

- **`check` and `states`** verify safety by state exploration: breadth-first search (BFS)
  at one instance per entity over the invariants, the `after` assertions, and the state
  models. They evaluate no `property`.
- **`verify`** checks every `property` and every invariant and `after` assertion at two
  instances by default, each on its own cone, with a native checker that needs no JVM. It
  decides the safety checks, `stable`, `unchanged`, `precedes`, and `can_reach` by BFS, and
  `always`, `eventually`, and `leads_to` on the graph BFS explores, under each property's
  fairness. With `--prove` it also proves state rules for any number of rows with z3.
- **TLC** runs only under `verify --with-tlc`, on the same cones and the same runs, as a
  second opinion. A check the two engines decide differently fails the run.

RDRA-ish prints its lifecycle transition system as **TLA+** for TLC: `export --kind tla`
writes the whole model, and `verify -o` writes the module of each cone. See
[state-derivation.md](./state-derivation.md) for the system itself, and
[Order and stability properties](#order-and-stability-properties) for `stable`,
`unchanged`, and `precedes`.

## Commands

```
rdra-ish export <INPUTS...> --kind tla [-o <OUT>] [--instance-count N] [--int-range-max N]
rdra-ish verify <INPUTS...> [--with-tlc] [-o <OUT_DIR>] [--instance-count N] [--int-range-max N]
```

| Command | Role |
|---|---|
| `export --kind tla` | Write `RdraSpec.tla` **and** sibling `RdraSpec.cfg` for the whole model, and the `StateModel_<E>` specs that check the deadlocks and witnesses |
| `verify` | Decide every invariant, `after` assertion, and property on its own cone with the native checker; no JVM |
| `verify --with-tlc` | The same, and run `tlc` / `tlc2` from `PATH` on one spec per cone and per property run beside it; a check the two decide differently fails the run |
| `states`, `check` | BFS on the same system; safety only, no `property` |

`--instance-count` (default `2`) sets `InstanceCount` (`1..=255`; `1` warns that
singleton ids cannot distinguish instances). `--int-range-max` (default `5`) sets
`IntRange == 0..N` (`1..=255`). Both commands fail on the Error diagnostics of the
transition system they print, for example `IntLiteralOutOfBounds` or `DoubleModeledEnum`.

If `-o` ends with `.tla`, the sibling `.cfg` is written next to it and the file stem is the
module name. If `-o` is a directory (or has no extension), `export` writes
`<dir>/RdraSpec.tla` and `<dir>/RdraSpec.cfg`. Beside them it writes the state-model specs
(see [State models with TLC alone](#state-models-with-tlc-alone)). `verify -o <dir>` writes one
`Cone_<first check>.tla` and `.cfg` pair per cone of an invariant or after-assert, one
`Cone_<Property>` pair per temporal property (its cone without `WF_vars(Next)`) with a
`Cone_<Property>_WF` pair beside it for an `eventually` or `leads_to` (the same cone with
`WF_vars(Next)`), and `cones.txt`, which lists each module TLC may run and the checks it
answers. When the model has temporal properties it also writes the whole-model `RdraSpec`
pair, whose `.cfg` names every property, for a run by hand, and a `RdraSpec_<Property>`
pair for each property its cone leaves undecided, which the whole-model run decides (see
[Temporal properties on a cone](#temporal-properties-on-a-cone)). The native checker reads
none of these files: they are for a TLC run by hand, and for TLC under `--with-tlc`.
Without `-o`, `verify` writes nothing, and `verify --with-tlc` writes them to a temp dir. It
deletes the dir when TLC passes every check, and keeps it and prints `specs: <dir>` when TLC
finds a counterexample or the engines disagree. The TLC heap settings (`RDRA_TLC_HEAP`,
`JAVA_TOOL_OPTIONS`) apply only with `--with-tlc` and are in
[cli-reference.md](./cli-reference.md#verify). `verify` runs within a memory limit, `16GiB`
unless `--memory` or `memory` under `[verify]` in `rdra.toml` says otherwise. BFS takes all
of it; under `--with-tlc` BFS takes half, and TLC runs only as many cones at once as their
`-Xmx` fit in the other half (see [Memory limit](./cli-reference.md#memory-limit)).

The `.cfg` names `SPECIFICATION Spec`, `INVARIANT TypeOK`, one `INVARIANT` per
constraint, one `PROPERTY` per `after` assertion and temporal property, and
`CHECK_DEADLOCK FALSE`. Lifecycle models often end in terminal states with no enabled
action, and `Spec` allows stuttering.

TLC on the whole-model pair therefore checks the invariants and properties only. The
state-model specs beside it check the deadlocks and the witnesses.
When `check` finds lifecycle errors, `export --kind tla` still writes every spec, exits 0,
and prints one note with the error count that names the state-model specs and `verify`.

### State models with TLC alone

<!-- constrained-by #what-verify-checks -->
`check` and `verify` find a deadlock or a state with no witness by BFS on each entity's
cone (see [What `verify` checks](#what-verify-checks)). `export --kind tla` writes the same
cones as TLA+, so TLC alone reaches the states the BFS reaches. TLC finds the same missing
witnesses, and a deadlock it reports is one `check` reports, but a TLC pass does not prove
that no deadlock exists (see below). `check` and `verify` decide the deadlocks. For each
cone whose entities have a state model, `export` writes three files:

| File | Checks |
|---|---|
| `StateModel_<E>.tla` | The cone, reduced as `verify` explores it, with one `Witness_<E>_<col>_<s>` invariant per state `s` of each state model and one `Deadlock_<E>_<col>_<s>` invariant per state with a way out to another state |
| `StateModel_<E>.cfg` | `TypeOK` and the `Deadlock_` invariants |
| `StateModel_<E>_witnesses.cfg` | `TypeOK` and the `Witness_` invariants |

`<E>` is the first entity of the cone with a state model. Entities with one cone share
its spec. The invariants have the names of the findings `check` prints. `export` does not
remove the `StateModel_` files that an earlier export wrote into the same directory, so a
spec of an entity cone that no longer exists stays there. Export into an empty directory.

`Deadlock_<E>_<col>_<s>` says that wherever an instance is at `s`, some step can still
change the column's cone: the column, its entity's existence, and every variable that the
steps writing them read. It is `ENABLED (Next /\ c' # c)`, with `c` the tuple of those
variables. A step outside that cone cannot move the column or open its way out, so a column
that another use case toggles back and forth does not hide the deadlock. TLC reports the
invariant violated in a state with an instance at `s` and no move of the cone left, which
is a deadlock `check` reports. A pass does not prove that the model has no deadlock: a
cycle anywhere in the column's cone keeps a move enabled and hides it (see the first case
below):

```
tlc -continue -config StateModel_Order.cfg StateModel_Order.tla
```

`Witness_<E>_<col>_<s>` says that no instance is ever at `s`. TLC reports it violated
where `s` is reached, so a healthy model violates every `Witness_` invariant and TLC exits
non-zero. A `Witness_` invariant that TLC does not report is a state with no witness:

```
tlc -continue -config StateModel_Order_witnesses.cfg StateModel_Order.tla
```

With `-continue`, TLC reports one violated invariant per state, the first in cfg order.
An invariant violated only in states where an earlier one is violated is not reported.
Rerun without the reported invariants until a run reports no new one, as `verify` does
for its cones.

With `-continue`, TLC also prints a trace for each state that violates an invariant. On the
witnesses cfg that is almost every reached state: 1.5 GB of output for the clinic-ops cone
of `Encounter` (93,425 states). TLC has no flag that leaves the traces out
(`-difftrace` only shortens each one). For a large cone, run the witnesses cfg without
`-continue`: TLC stops at the first violated invariant, and each rerun without the
reported ones finds the next. The `engine_equivalence` test does this above 1,000 states.

The TLC reading differs from `check` in two cases:

- TLC reports a deadlock in a state with no move of the column's cone left. An instance
  can also be stuck while that cone keeps moving forever. `check` reports that deadlock,
  since no path moves the instance off `s`, and TLC passes, since every state still has a
  move of the cone. Any cycle in the cone hides it, even at 1 instance: another column of
  the instance that a guard reads, a related entity that a guard reads, or another
  instance of the entity. For example, Ship moves an Order from `paid` to `shipped` only
  while `review == ok`, and Flip and Flop move `review` between `a` and `b` forever, so a
  paid Order is stuck; Express makes `shipped` reachable from `pending`. `check` reports
  `Deadlock_Order_status_paid`, and TLC on `StateModel_Order.cfg` reports nothing. The
  `engine_equivalence` test pins this case. No TLC invariant expresses the BFS check, which
  asks whether any path from the state moves the instance. An invariant on one step of the
  instance would report an instance that waits for a step of another row.
- `check` reports a deadlock as a warning when only the instance bound blocks its way out.
  TLC reports the invariant violated, as for an error.

`verify` does not run these specs: its BFS reports the same findings, and its output
stays as it was.

## Mapping

| RDRA | TLA+ |
|---|---|
| Entity | `<E>_Ids == 1..InstanceCount` and `<E>_exists`, a function over the ids with values `"absent"`, `"present"`, `"deleted"` |
| Enum, Bool, `@null`, Int, Money, Decimal, Date, or DateTime column the system reads or writes; a variation-typed column that something names | `<E>_<col>`, a function over `<E>_Ids`. Int-like columns range over `IntRange` (nullable ones add `-1` for null). |
| `relate` N:1, 1:N, 1:1 | `<Child>_owner_<Parent>`, a function from child ids to parent ids. An `.optional()` relate adds `0` for an unlinked child. |
| Use case | One operator `<UC>(p0, ...)` per transaction group of `UsecaseTx::groups()` (`<UC>_1`, `<UC>_2`, ... when the writes form several groups). `Next` is the disjunction of `\E` over the parameters. |
| A use case split into k groups | A run mark `<E>_run_<UC>`, a function from the ids of each entity a step before the last writes to `{"none", "1", .., "k-1"}`. `<UC>_1` takes a row at `"none"` and sets `"1"`; `<UC>_i` binds a parameter per earlier row with `<E>_run_<UC>[p] = "i-1"` in place of a presence test, and sets `"i"`, or `"none"` at the last step (see [Runs of a split use case](./state-derivation.md#runs)). |
| A tracked event `Ev` with mark entity `E` | A raised mark `<E>_raised_<Ev>`, a function from `<E>_Ids` to `BOOLEAN`, `FALSE` in `Init` and never `FALSE` again (see [Triggers](./state-derivation.md#triggers)). The last step of a raiser, on each parameter `p` it has on `E`, assigns `<E>_raised_<Ev>' = [<E>_raised_<Ev> EXCEPT ![p] = TRUE]`; the first step of a triggered use case conjoins `<E>_raised_<Ev>[p] = TRUE` on each parameter that is the raiser's row. |
| A `Triggered` parameter | One more `\E p \in <E>_Ids` on the triggered operator with `<E>_exists[p] = "present"` and the mark conjunct, and, when the one direct key joins it to one of the operator's rows, `<Child>_owner_<Parent>[c] = p` (or `= c`). A cone that leaves the mark out prints the operator without the conjunct and without the parameter, and the var is not in the module. |
| A use case several tracked events trigger | One operator per variant, `<UC>_by_<Ev>`, each in `Next`; `AfterAssert_<UC>_<n>` ranges over every variant's last step. |
| `creates` | `FirstAbsent(<E>_exists, p)` and `<E>_exists' = ... "present"`. A deleted slot is never reused. |
| `deletes` | `<E>_exists'` becomes `"deleted"`, columns and owners return to the base value. |
| `sets`, `transitions(UC, E.col, a -> b)` | Primed assignments. A transition guards `col = a` and writes `b`; arms to several targets write through a `CASE` on the source. `UNCHANGED` lists every variable the action does not assign, an entity the action leaves alone by its tuple `<E>_vars`. |
| Variation column of a created instance | `\E v0 \in {values} : col' = ...`, so TLC explores every value. `@default` does not pin it. |
| `applies_to(Condition, UseCase \| Api)` with a `holds` body | A guard conjunct on the use case's operator. A condition on a created instance is evaluated on the primed variable, after the assignments and the `UNCHANGED` that bind every primed variable it reads. |
| Comparison proposition driven by `drives(UC, E.col op rhs, bool)` | A Boolean variable. An undriven comparison is TLC arithmetic on the Int columns. |
| `col in (a, b)` / `col not in (a, b)` | `E_col[b0] \in {"a", "b"}` / `E_col[b0] \notin {"a", "b"}`, the same disjunction of equalities that `states` evaluates. A nullable Int value `present` keeps the `\/` form. |
| `col < now` | A global `now \in IntRange` and a `Tick` action that adds one while `now < max`. |
| Entity-local and multi-entity `forbidden`, `invariant`, `required`, `exclusive` | One `INVARIANT` each, named `<Kind>_<Entity>_<n>` (`Forbidden_Order_0`, `Inv_Order_0`, `MultiForbidden_Order_0`). Each ranges over the present instances. |
| `.along(...)` | Quantified over `<E>_Ids`, joined by `<Child>_owner_<Parent>` equalities. |
| `col: T @check(expr)` | `INVARIANT Check_<Entity>_<column>_<n>` over the present instances. A nullable operand is guarded: `qty: Int @null @check(qty >= 1)` prints `(E_qty[b0] # -1 /\ E_qty[b0] >= 1)`, so a null `qty` violates it. |
| `relate(Child, Parent, 1:1)` | `INVARIANT Unique_<Child>_<role>` (role defaults to the parent id): `\A b0, b1 \in <Child>_Ids`, two present children are the same or have different owners. An unlinked optional child owns nothing. |
| `relate(Child, Parent, 1..*:1)` (child-side minimum) | `INVARIANT MinCard_<Child>_<role>`: `\A b0 \in <Parent>_Ids`, a present parent has `\E b1 \in <Child>_Ids` present with `<Child>_owner_<Parent>[b1] = b0`. Checked in every state, so a parent created without a child in the same transaction, or a deleted last child, is a counterexample. |
| Pre-existing child of a `relate` | `Init` links the owner: `\in [<Child>_Ids -> <Parent>_Ids]` for N:1 (start `Any`), `[i \in <Child>_Ids \|-> i]` for 1:1 and for a child-side minimum (start `Identity`). An `.optional()` key under a created parent starts unlinked (`Base`). |
| An action that creates a parent and updates an existing child | The action links the child. An `.optional()` key guards the assignment with an unlinked owner, so a linked child keeps its first parent. A mandatory key assigns without a guard, which moves the child. |
| `invariant(...).when(...).none/has(...)` | `\A` / `\E` over the ids, named `Quantifier_<Entity>_<n>`. A `relate` joins the sides by owner; without one the related side is independent. |
| `after(UC).assert(...)` | `PROPERTY AfterAssert_<UC>_<n>`: `[][(\A p : UC(p) => post')]_vars`. It checks every step of the use case's last action, so a split use case once per run, on the run's rows. |
| `property` with `always`, `eventually`, `leads_to` | A named formula and a `PROPERTY`, quantified per present instance |
| `property P ... .along(A, B, ..)` | The owner link of each hop joins `present` in the trigger: `[](present /\ link => x)`, `[](present /\ link => <>x)`, `(present /\ link /\ x) ~> y`. Without `.along` a property over several entities ranges over every combination of their rows. |
| `property P stable(p)` | `P == [][\A b0 \in E_Ids : (present /\ p) => (present' => p')]_vars`, an action property TLC checks on every step. With `.along` the link joins `present` before the step only, so a step that moves a key keeps the obligation. |
| `property P unchanged(E.col, ..).when(p)` | `P == [][\A b0 \in E_Ids : (present /\ p) => (present' => E_col'[b0] = E_col[b0] /\ ..)]_vars`; `p` is `TRUE` without `.when`. A column no action writes has no var and drops out. |
| `property P precedes(p, q)` | A latch `P_seen \in [E_Ids -> BOOLEAN]` (`[A_Ids \X B_Ids -> BOOLEAN]` with two binders): `Init` sets it to `present /\ p`, and `P_latch == P_seen' = [b0 \in E_Ids \|-> P_seen[b0] \/ (present /\ p)']` is the last conjunct of every action. `P == [](\A b0 \in E_Ids : (present /\ q) => P_seen[b0])`. The module declares the latch only where its `.cfg` checks `P`. |
| Any `eventually` / `leads_to` property | `Spec` includes `WF_vars(Next)` |
| `property P ... .fair(UC, ...)` | `Fair_P`, one `WF_vars` per action of each listed use case and per instance it acts on (`\A p0 \in E_Ids : WF_vars(UC(p0))`, a created slot bound inside with `\E`), and `P == Fair_P => (...)`. `Spec` is unchanged. |
| Domain of every variable | `INVARIANT TypeOK`, kept apart from the business rules |

Example temporal and Int syntax:

```rdra
property PaidLeadsToShipped "paid eventually reaches shipped"
  leads_to(Order.status == paid, Order.status == shipped)

property StockOk
  always(Item.stock >= Item.selling)

property SettlesOrEnds
  eventually(Order.status in (delivered, cancelled))

sets(Restock, Item.stock == 3)
forbidden(Item, stock < selling)

after(DeliverOrder).assert(Order.status == delivered, Order.delivered_at == present)
after(Sell).assert(Item.sold >= 1)

invariant(Cert).when(status == revoked).none(Assign.status == active)
// equivalent: invariant(Cert, Assign).when(Cert.status == revoked).none(Assign.status == active)

forbidden(Order, Payment, Order.status == cancelled, Payment.status == captured).along(Order, Payment)
```

Operators:

- `always(expr)` → `[]expr`
- `eventually(expr)` → `<>expr`
- `leads_to(p, q)` → `p ~> q`
- `stable(p)`, `unchanged(cols).when(p)` → `[][A]_vars` on every step; `precedes(p, q)` → a
  latch and `[](q => <P>_seen)`
- Connectives inside expressions: prefer `and` / `or` / `not` (`/\` `\/` `~` aliases)
- Set membership in an atom: `Order.status in (a, b)` → `\in {"a", "b"}`, `not in` → `\notin`
- Property label string is optional (`property StockOk always(...)`)

`rdra-ish states` does not evaluate path properties, and `check` prints a `note:` that it
did not model-check them. Use `export` or `verify`.

### Order and stability properties

<!-- constrained-by ./language-reference.md#order-and-stability-stable-unchanged-precedes -->

`stable`, `unchanged`, and `precedes` are safety properties that BFS decides on the graph it
explores. `verify` runs BFS on each one's cone (`TransitionSystem::decide_property`), next to
the BFS on the entity cones, and starts no TLC run for it. It prints
`order and stability properties: <n>, each decided by BFS on its cone` under the bounds and
one line per property:

```
property CancelledStaysCancelled (BFS on its cone): violated
error[CancelledStaysCancelled]: property CancelledStaysCancelled is violated: its stable condition holds for Order#1 before step 3 and is false after it
  counterexample (3 steps):
    1. Place [Order#1]
    2. Cancel [Order#1]
         Order#1.status: draft -> cancelled
    3. Reopen [Order#1]
         Order#1.status: cancelled -> draft
```

- `stable` and `unchanged` are read on every edge, in the order BFS found them. The first
  edge that breaks the property for some binding comes from the shallowest state, so the
  BFS path to its source and the edge are the shortest trace to the step.
- `precedes(p, q)` is read with its latch, which is no var of the system: a state where `q`
  holds and the latch is unset is one that a path reaches through states where `p` never
  held for the binding. The search walks the explored graph restricted to those states, per
  binding, and keeps the shortest walk to a state where `q` holds. That is the walk TLC finds
  on the product with the latch.

A violation is a finite trace, with no `then nothing changes forever` after it. A cone that
frees an entity's slots reaches a superset, so a violation there is decided again by BFS on
the whole model. A cone BFS stops at the state cap with no violation is `not decided`, and
counts with the other findings the cap leaves. A scoped run (`--buc`, `--entity`) checks
none of these properties, as it checks no other.

The TLA+ export spells the same formulas (see [Mapping](#mapping)), and
`engine_equivalence` checks on `order_stability.rdra` and its fixtures that BFS on the cone
and TLC on the whole model give the same verdict, and that a counterexample has the same
length and replays as the same violation. The latch is printed only in a module whose
`.cfg` checks the property, so the cone modules of the rules and the entity cones keep the
state graph the explorer reaches.

## What `verify` checks

`verify` restricts the system to the cone of influence of each invariant and
`after` assertion and decides each cone by BFS, the cones side by side. Under `--with-tlc`
it also prints each cone and runs TLC on it in parallel. A cone
reaches the whole model's states projected onto it. When an action of the cone also
creates an entity the cone does not read, the cone drops that creation and never runs out
of that entity's slots, so it reaches a superset: the `bounds:` line says so (`entities a
check does not read are unbounded`). An `ok` on every cone is the whole-model `ok`, and a
violation on a cone is a violation of the model, possibly with more instances of the
unread entity than the bound. For a temporal property this is the `FreesSlots` case
below. Cones over the same variables share one run. A cone with an independent
sub-lifecycle is explored and printed reduced by it (see
[Reduction by an independent sub-lifecycle](#reduction-by-an-independent-sub-lifecycle)).
This keeps clinic-ops tractable: a whole-model run at `InstanceCount=2` took 62 s and
11 GB on ec-site and did not finish in 300 s on clinic-ops.

A temporal property is checked on a cone of its own, and on the whole model only when that
cone does not decide it (see [Temporal properties on a cone](#temporal-properties-on-a-cone)).
The native checker explores the property's cone and decides the property on the graph it
reached. TLC reports a temporal violation as `Temporal properties were violated.` without
naming the property, so under `--with-tlc` each property runs alone: module
`Cone_<Property>` with a `.cfg` that names that one `PROPERTY`. The property cones run with
the other cones, in parallel.

Under `--with-tlc`, up to 8 TLC cone runs go at once, the property cones and the cones with the most variables
first, since those tend to be the long ones. Each run gets the cores divided by the runs
at once, and no fewer than 2 TLC workers on 4 cores or more: once the short cones are
done, a long cone on one worker would leave the other cores idle. With several workers
TLC reaches the same states, so an `ok` is the `ok` of one worker, but it no longer finds
a violation by the shortest trace. A safety cone that does not pass is therefore run
again on one worker, and `verify` reports only that run, so a counterexample is the one a
single worker finds. A temporal property's lasso differs from run to run on one worker
too.

A cone's system holds every check that reads only the cone's vars, so a larger cone often
holds all the checks of a smaller one. Neither engine runs the smaller cone when the
larger one passes: its line reads `ok (checked in Cone_<larger>)`, with no state count. That
is sound when the larger cone takes every step the smaller one takes, so that it reaches
every state the smaller one reaches. `verify` asks for three things:

- The larger module holds every check of the smaller.
- The larger cone holds the smaller cone's vars and kept actions. For an invariant this
  follows from the cones being closed. For an `after` assertion it does not: its cone holds
  everything the use case writes, and a cone without those writes takes the use case's step
  as a stutter, on which the assertion passes.
- The larger cone leaves each action of the smaller the guards and params the smaller leaves
  it. A cone drops a guard that reads outside it and the param of an entity it leaves out (a
  creation it does not count, a triggering event it does not hold), so a larger cone may hold
  back a step the smaller one takes, and the smaller one may fail a check the larger passes.

A cone that is not covered runs as before. When the larger cone does not pass, the smaller
one runs as well, so a counterexample is read on the smallest cone that shows it. The specs
of a cone that did not run are still written.

A property with `.fair(UC, ...)` (see
[language-reference.md](./language-reference.md#fairness-fairusecase-)) carries its
assumption in its own formula, `Fair_P => (...)`, and its cone keeps the listed use cases
whole. `verify` prints `<P> assumes weak fairness per instance on ...` under the `temporal
properties:` line, since an `ok` for `P` holds only on the runs that do not starve them.
`check` and `states` verify safety only, by state exploration: they evaluate no temporal
property, with or without `.fair`. A `can_reach` property is CTL, which TLC cannot check, so
`verify` decides it by BFS instead (see
[Recoverability on a cone](#recoverability-on-a-cone)).

### Temporal properties on a cone

<!-- derived-from ./state-derivation.md#exploration-by-cone-of-influence -->
<!-- constrained-by ./language-reference.md#fairness-fairusecase- -->
A property's cone (`TransitionSystem::property_cone`) is the smallest closed var set that
holds what the formula reads, the owner vars of its `.along(...)` link, and, for each
action of each use case its `.fair` lists, everything the action reads and writes: its
guards with the `applies_to` conditions and the
quantifier reads inside them, the existence of each instance it binds, the source of each
transition, the clock when it ticks, the columns it assigns, transitions, or chooses, and the
existence of each entity it creates or deletes. Closed means what it means for an invariant's
cone: every action that writes a var of the cone reads only vars of the cone, with two
exceptions. A copy guard (`sets(UC, Child.col == Parent.col)`) whose target column is
outside the cone is dropped with the column, and a creation of an entity outside the cone
is dropped with its slot (the `FreesSlots` case below). The cone decides the property's
verdict under the property's own fairness because, for a fair action `A`, whether `A(p)` is
enabled in the whole model is a function of the cone's vars, every step of `A(p)` that
changes any var changes a var of the cone, and so `WF_vars(A(p))` on the cone's `vars` is
`WF_vars(A(p))` on the whole model's; every other action either writes into the cone, and
then reads only the cone apart from those two exceptions, or leaves the cone unchanged,
which is a stuttering step of the cone's `[Next]_vars`. Every behavior of the whole model
that satisfies `Fair_P` therefore projects onto a behavior of the cone that satisfies
`Fair_P`. A behavior of the cone lifts to one of the whole model on three facts:

- A dropped copy guard can hold in the whole model whatever its source holds: `copyable`
  accepts a copy only between columns that hold the same values, with a nullable source
  only into a nullable target, and rejects any other copy with `SetsCopyMismatch`, so the
  target can take every source value. The step lifts with the target set to the source's
  value. A copy from an optional parent keeps its `Owner` guard and its `Parent` param in
  the cone, so the cone still needs a linked parent where the whole model does.
- A cone step that both a fair `A(p)` and a non-fair `B` explain lifts as `A(p)`, so the
  lifted behavior takes the fair step wherever `WF_vars(A(p))` needs one.
- An `always` counterexample the first run finds without `WF_vars(Next)` extends to a
  behavior that satisfies the whole model's `WF_vars(Next)`: `WF_vars(Next)` and `Fair_P`
  are weak fairness on actions of `Next`, which is machine closed, so every finite prefix
  extends to a behavior that satisfies both, and an `always` is violated by the prefix
  alone.

The lift needs the cone to count the slots of every entity its actions create; the
`FreesSlots` case below is the one where it does not.

What does not restrict to a cone is the `WF_vars(Next)` that `Spec` adds for any `eventually`
or `leads_to`: a step outside the cone satisfies it, so the cone with `WF_vars(Next)` of its
own would rule out runs the whole model allows (another entity looping forever while the
property's instance stands still), and the cone without it admits a stutter the whole model's
`WF_vars(Next)` rules out. `verify` therefore gives such a property up to two runs on the
cone, and the whole model only when neither decides. The native checker runs each one on
the graph BFS explores: a behavior that avoids the goal forever either stops at a state
where the fairness lets it stop or cycles through a strongly connected set of goal-free
states that the fairness admits, and the property fails when a triggered state reaches
such an end. The two runs on the unreduced cone read one graph. Each counterexample
replays through the step relation, fails the property, and passes the fairness check
before it prints; one that does not is a tool bug and prints `error[LassoFault]`. Under
`--with-tlc` TLC runs the same ladder on the modules below:

1. `Cone_<P>`, the cone with `Spec == Init /\ [][Next]_vars` and the property
   `Fair_P => (...)`. An `ok` is the whole model's verdict.
2. `Cone_<P>_WF`, the same cone with `WF_vars(Next)`, when the first run finds a
   counterexample. A counterexample here is a counterexample of the whole model: its loop
   keeps stepping, and its stutter leaves every action of the cone disabled, which the
   whole model's actions outside the cone cannot change. An `ok` here decides only when the
   cone holds every var an action writes, so that every step that changes the whole model
   changes the cone; otherwise the first run's counterexample needs the whole model's
   `WF_vars(Next)` to rule out, and the property is undecided on the cone.
3. `RdraSpec_<P>`, the whole model, for a property its cone leaves undecided. This is the
   run that did not finish on food-permit-ops. The native checker explores the whole model
   once for every such property and stops at `--temporal-budget` seconds (default 300), at
   the `--memory` limit, or at `--max-states` states when given. TLC under `--with-tlc` prints
   its progress (distinct states, queue, seconds, about once a minute) and stops at the same
   budget or at the first progress line past 2,097,152 states, or past `--max-states` when
   given: TLC's heap is not `--memory`'s pool, so its whole-model run keeps a bound. A run stopped after it found a counterexample keeps that verdict;
   otherwise the property is `not decided`, listed under `not checked:`, and the run still
   ends in `OK:`
   with exit status 0, as a BFS cone at its cap does, after a `warning:` on stderr that
   counts what was left undecided. `--deny-undecided` makes that an error with exit
   status 1.

An `always` needs the first run only. A cone that frees an entity's slots (an action of the
cone creates an entity the cone does not read, so the cone does not count its slots) can show
a lasso that creates more instances than the bound allows, so only its `ok` is taken, and a
counterexample sends the property to the whole model. Each property's line names what
happened: `Cone_P [P]: ok (111 states)`, `Cone_P [P]: counterexample (43 states, Cone_P_WF:
counterexample)`, or `Cone_P [P]: not decided (31 states, Cone_P_WF: ok, needs the whole model's
WF_vars(Next))` followed by the `RdraSpec_P [P]:` line. A cone reduced by its independent
sub-lifecycle (the next section) runs `_WF` on the reduced cone first and notes
`Cone_P_WF: ok on the reduced cone` or `Cone_P_WF: counterexample on the reduced cone` when
that decides ([WF_vars(Next) on a reduced cone](#wf_varsnext-on-a-reduced-cone)); otherwise it
adds `Cone_P_Full: counterexample` after a counterexample on the reduced cone, before the `_WF`
note. A property without `.fair` rests on
`WF_vars(Next)` alone, so it reaches the whole model unless its cone holds every written
var; name the use cases it relies on to keep it on the cone. On food-permit-ops
`HygieneManagerEventuallyAppointed` runs on a 243-state cone and the whole `verify` takes
6 s; on patent-ops `GrantEnds` runs on a 75,017-state cone; on insurance-claim-ops
`ClaimEventuallyDecided` runs on a 633,787-state cone and `SubrogationEnds` on a 372-state one. The test
`a_property_cone_decides_what_the_whole_model_decides` in `engine_equivalence` compares the
cone verdict with the whole-model verdict, and the native verdict of each run with TLC's,
on the temporal samples and on fixtures for each case: another entity looping outside the cone, a fair use case whose guard reads another
entity, a guard that toggles forever, a cone that frees slots, a cone that holds every
written var, a fair use case copying from its parent, a copy from an optional parent the
cone leaves out, a `.none` guard on a fair use case whose quantified column another use
case writes, and a condition on a creating use case that reads a column the creation does
not assign.

`verify` also explores each entity's cone with BFS at the same bounds, with no state
cap: `--memory` stops a cone, and `--max-states <N>` caps every BFS run (the `bounds:` line
names the cap or says `no state cap`). The pattern cap is 256 per entity (`--max-patterns`). Each state of a
state model must have a witness there (see
[state-derivation.md](./state-derivation.md#state-models-and-witnesses)). A `NoWitness`
error fails `verify` even when every invariant and property holds: the run prints
`result: counterexample` and no `OK:` line, and exits 1. A deadlock, an instance that no path moves off a state with a
way out (see [state-derivation.md](./state-derivation.md#deadlocks)), fails it the same way;
one whose way out lacks only a free slot is a warning at the bound. TLC's own deadlock check
stays off: every bounded model ends with no enabled action, and an always-enabled loop would
hide a stuck instance. The `StateModel_` specs that `export` writes let TLC alone find the
same missing witnesses and the deadlocks that no cycle of the cone hides (see
[State models with TLC alone](#state-models-with-tlc-alone)). The same pass finds each use case that never runs (see
[state-derivation.md](./state-derivation.md#use-cases-that-never-run)) and prints it as a
warning, which does not fail the run. A cone that stops at the cap decides nothing about its
unwitnessed states, its deadlocks, or its actions with no edge, and `verify` lists each
under `not checked:`. The
pass ends with a `state models (BFS on N entity cones): ...` summary line. The cap counts the
states of the reduced cone (see
[Reduction by an independent sub-lifecycle](#reduction-by-an-independent-sub-lifecycle)); the
sub-lifecycle it explores to reduce a cone has the same cap, and a cone whose sub-lifecycle
passes it runs unreduced. On clinic-ops the whole `verify` takes about 3 s and 1.3 GB (it
took 2.5 minutes and 3.7 GB with TLC before the reduction); its largest cone is `EligibilityCheck`
(905,528 states at 2 instances, which nothing reduces), while `Charge`, past the cap
unreduced, is 38,533 states reduced and `Prescription` (4,266,635 unreduced) is 1,690. On
patent-ops the `OfficeAction` cone (over 8,388,608 states unreduced, 1,341,707 reduced) is under
the cap too, so `verify` decides all four of its stuck-state checks (see the sample's README).

```
$ rdra-ish verify samples/formal-verification/order.rdra
bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); no state cap per BFS cone, memory 16GiB; entities a check does not read are unbounded
cones: 1 for 2 invariant(s) and 1 after-assert(s)
temporal properties: 2, each checked on its cone, and on the whole model only when the cone does not decide it (budget 300 s)
state models (BFS on 1 entity cone): 1 state model, 5 states witnessed, 0 deadlocks, 0 use cases that never run
Cone_Forbidden_Order_0 [Forbidden_Order_0, Inv_Order_0, AfterAssert_DeliverOrder_0]: ok (31 states)
Cone_PaidLeadsToShipped [PaidLeadsToShipped]: not decided (31 states, Cone_PaidLeadsToShipped_WF: ok, needs the whole model's WF_vars(Next))
Cone_EventuallyTerminal [EventuallyTerminal]: not decided (31 states, Cone_EventuallyTerminal_WF: ok, needs the whole model's WF_vars(Next))
RdraSpec_PaidLeadsToShipped [PaidLeadsToShipped]: ok (31 states)
RdraSpec_EventuallyTerminal [EventuallyTerminal]: ok (31 states)
result: ok
OK: no errors within 2 instances per entity, Int 0..5
```

Both properties of `order.rdra` have no `.fair`, and `DeliverOrder` writes `delivered_at`,
which neither reads, so their cones do not decide them and the whole model (31 states) does.
With `.fair(ShipOrder)` on `PaidLeadsToShipped`, its first cone run would decide it.

A violated invariant or `after` assertion prints in model terms, the same way `check`
prints it: the rule's `path:line:col`, one line per step with the use case, its bound
instances (`Place [Order#1, Payment#1]`), and its site, and under each step the values it
changed. The trace is BFS's shortest path to the violation, built with the code `check`
uses. Under `--with-tlc`, TLC's counterexample of a failed run prints after it, under a
`tlc: Cone_<name> (<path>):` line: `verify` decodes TLC's states, replays them through the
same step relation, and prints them in model terms (`counterexample (N steps, from TLC)`),
then TLC's own `violated: Error: ... is violated.` line and its trace, with one
`name[slot] = value` line per variable and slot. The name is the constraint's name, so it
points at the rule that broke. A TLC step that no move of the cone explains is a tool bug,
and `verify` says the engines disagree instead of printing the model-term block. The full
output is in [cli-reference.md](./cli-reference.md#verify).

A violated temporal property prints the same way, under `error[<Property>]`. An `always`
fails at a state, so its counterexample is the shortest trace to it. An `eventually` or
`leads_to` counterexample is a lasso: a finite trace, then either a stop (the last state
repeats forever) or a loop back. `verify` prints a loop's last step as the last numbered
step, followed by `then loops back to step K`: the behavior repeats steps K through the
last one forever. A stop ends with `then nothing changes forever`. TLC's lasso under
`--with-tlc` reads `State N: Stuttering` or `Back to state K: <action>`; `verify` replays
it the same way, and K is TLC's state number, since TLC's state K is the state step K
starts from.

The message head names the property, the instance it fails for, and the step that shows
it: where an `always` condition is false, or where an `eventually` instance exists or a
`leads_to` trigger holds and the goal never holds from then on. `verify` evaluates the
property on the infinite behavior the way the module spells it, per instance of each
binder: `always x` is `[](present => x)`, `eventually x` is `[](present => <>x)`, and
`leads_to x y` is `(present /\ x) ~> y`, with the `.along(...)` link joined to `present`
when the property has one. A native counterexample that does not replay, does not violate
the property, or loops unfairly is a tool bug and prints `error[LassoFault]`; a TLC lasso
that does not replay prints the engine-disagreement line. The native lasso reaches its loop
by the shortest path BFS found, while TLC picks its own, so the two may print different
steps. In this run of `temporal_unmet.rdra`, Order#1 is cancelled and reopened forever:

```
error[PendingLeadsToDelivered]: property PendingLeadsToDelivered is violated: its leads_to trigger holds for Order#1 after step 1, and its goal never holds from then on
  rule: skills/rdra-ish-verify/samples/temporal_unmet.rdra:48:1
  counterexample (3 steps):
    1. PlaceOrder [Order#1]                     skills/rdra-ish-verify/samples/temporal_unmet.rdra:15:1
         + Order#1 (status=pending)
    2. CancelOrder [Order#1]                    skills/rdra-ish-verify/samples/temporal_unmet.rdra:19:1
         Order#1.status: pending -> cancelled   skills/rdra-ish-verify/samples/temporal_unmet.rdra:40:1
    3. ReopenOrder [Order#1]                    skills/rdra-ish-verify/samples/temporal_unmet.rdra:20:1
         Order#1.status: cancelled -> pending   skills/rdra-ish-verify/samples/temporal_unmet.rdra:41:1
    then loops back to step 2
```

### Recoverability on a cone

<!-- derived-from ./state-derivation.md#exploration-by-cone-of-influence -->
<!-- constrained-by ./language-reference.md#recoverability-can_reach -->
`property P can_reach(goal)` is the CTL formula `AG (present => E[present U (present /\
goal)])` per instance of each binder: from every reachable state where the instance is
present (and, under `.along`, linked), some path on which it stays present reaches a state
where the goal holds for it.
No TLA+ temporal formula states it, so TLC does not check it. `export --kind tla` lists it
in a comment, and no `.cfg` names it.

`verify` decides it by BFS on the property's cone, at its bounds; `check` and `states`,
which verify safety only, do not evaluate it. The run
keeps every edge it finds, so the decision is one pass over the explored graph per binding
of the binders. The pass goes backward from the states where the instance is present and
the goal holds, through states where the instance is present, and marks every state that
can still reach the goal. A present state the pass does not mark fails the property. The
first such state in discovery order ends a shortest trace, since BFS reaches states in
depth order. The finding prints that trace, then what the goal reads of the instance in
each state reachable from there. The pass is linear in the graph's states and edges, once
per binding: two bindings at `verify`'s two instances.

The cone must be exact, because `EF` is not monotone in the transitions. A step the cone
allows and the whole model does not could reach the goal and hide a violation. Two kinds of
cone allow such steps. One creates an entity whose slots the cone does not count (the
`FreesSlots` case). The other runs a use case without its event's mark. So the cone of a
`can_reach` keeps every action that writes into it, which brings in the marks it reads, and
reads the existence of each entity such an action creates, until nothing more is added. On
that cone, a path of the cone is a path of the whole model and the other way round, so the
cone's verdict is the whole model's at the same bounds. A reduced cone (the next section)
keeps stuttering equivalence, which says nothing of `AG EF`, so no `can_reach` cone is
reduced. A cone that stops at the state cap decides nothing, since a state it did not expand
has no edges and would look stuck: it reports `StateCapReached` as an invariant's cone
does. A scoped `verify` checks no property, `can_reach` included.

`verify` prints `can_reach properties: N, decided natively by BFS on each one's cone (TLC
cannot check AG EF)` in its header, explores those cones beside the entity cones, and ends
the BFS pass with `can_reach (BFS on N cones): N checked, M violated`. A violation fails the
run with `result: counterexample`:

```
$ rdra-ish verify samples/formal-verification/can_reach_unmet.rdra
bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); no state cap per BFS cone, memory 16GiB; entities a check does not read are unbounded
cones: 0 for 0 invariant(s) and 0 after-assert(s)
temporal properties: none
can_reach properties: 1, decided natively by BFS on each one's cone (TLC cannot check AG EF)
error[OrderCanFinish]: property OrderCanFinish is violated: from the counterexample's last state no path reaches its goal for Order#1
  rule: skills/rdra-ish-verify/samples/can_reach_unmet.rdra:57:1
  counterexample (3 steps):
    1. Place [Order#1]                                                 skills/rdra-ish-verify/samples/can_reach_unmet.rdra:17:1
         + Order#1 (status=pending, in_stock=true, window_open=true)
    2. CloseWindow [Order#1]                                           skills/rdra-ish-verify/samples/can_reach_unmet.rdra:23:1
         Order#1.window_open: true -> false                            skills/rdra-ish-verify/samples/can_reach_unmet.rdra:47:1
    3. LoseStock [Order#1]                                             skills/rdra-ish-verify/samples/can_reach_unmet.rdra:22:1
         Order#1.in_stock: true -> false                               skills/rdra-ish-verify/samples/can_reach_unmet.rdra:46:1
  confined: from there Order#1 stays within 34 reachable states, where its goal never holds:
    status = pending
    status = on_hold
state models (BFS on 1 entity cone): 1 state model, 4 states witnessed, 0 deadlocks, 0 use cases that never run
can_reach (BFS on 1 cone): 1 checked, 1 violated
result: counterexample
Error: verification failed: 1 can_reach error(s) from BFS on their cones
```

The 34 states count the moves of Order#2 too; Order#1 stays at `pending` or `on_hold` in
all of them. The tests in `lifecycle::tests::reach` compare the cone's verdict with the
whole system's on both samples, at one instance and at two, and on a goal behind an event
that is never raised and a goal that needs a slot the bound never frees, where a cone that
is not exact would pass.

### Reduction by an independent sub-lifecycle

<!-- derived-from ./state-derivation.md#exploration-by-cone-of-influence -->
<!-- derived-from #temporal-properties-on-a-cone -->
A cone often holds a second lifecycle that its targets never write and read only in part: the
claim's cone in insurance-claim-ops holds the case's settlement, the charge's cone in
clinic-ops holds the appointment and the encounter. Every state of that lifecycle interleaves
with every state of the target's, and an exact cone cannot be smaller than that product. Since
#74 `verify` explores and prints each cone reduced by such a sub-lifecycle
(`TransitionSystem::reduced`), and the reduced cone decides what the cone decides. `check`
and `states` explore the cone unreduced.

The cone's vars split into a target side T and the rest S:

- T holds one var the targets read (every var they read is tried as the anchor, and the
  split with the largest S wins), the writes of the actions the targets keep (the fair use
  cases of a property, the steps of an `after` assertion), and, by closure, every write of
  an action that writes or reads a var of T. An entity's vars share a side. So every action
  that writes S reads only S: S is a transition system of its own, which the rest of the
  cone never writes and reads only through `obs`, the S vars that T's actions and the
  targets' checks read (guards, transition sources, copy sources, the formula of a property).
  A check that reads S outside `obs`, an `after` assertion on an S action, and a property
  whose `.fair` names an S action are dropped from the reduced cone and decided on their own
  cones, where what they read is T or `obs`.
- S is explored on its own, as a cone with the same reduction applied to it, so a reduction
  nests, and its reachable states are quotiented by divergence-blind stuttering equivalence
  with respect to `obs`: the coarsest partition in which two states agree on `obs`, and
  whenever a step of S takes one of them out of its class into a class `c`, a path of S steps
  takes the other into `c`. The reduced cone keeps one representative S state per class in
  its state, runs T's actions on it unchanged, and replaces S's actions by class moves: from
  a class, by the last action of a path that leaves it, into the class the path enters. In
  TLA+ the sub-lifecycle is one variable `S_class`, a `Rep_<var>` table per observed var that
  the var's name reads, and a `Succ_<A>` table per S action (the snapshot
  `a_reduced_cone_prints_its_sub_lifecycle_as_classes` in `crates/rdra-ish-emit/src/tla/tests.rs`).
  Each table is one tuple literal with a cell per class, which TLC evaluates cell by cell
  whatever the class count; a chain of `@@` entries nests once per entry, and a sub-lifecycle
  of a thousand classes overflowed the JVM's stack. An unobserved S (`obs` empty) is one
  class with no moves. When every state of S is a class of its own and S was not reduced
  inside, the quotient is the identity and would reduce nothing, so the cone stays unreduced
  (#101): it keeps all its checks, its module has no `S_class`, and a property's
  counterexample needs no `_Full` run. A nested reduction still counts: an S whose own
  reduction shrank it is reduced, even when its classes are its states. An identity inner
  reduction does not count (`Reduction::shrinks`), and a split whose S is wholly in `obs` is
  not explored, since its quotient is the identity whatever the inputs do.
- Since #112 an S action may read T vars that only rise, its *inputs*. A var is monotone
  when the cone's writes, taken as edges between its values, form no cycle
  (`TransitionSystem::monotone_vars`): a create and a delete move an existence forward, a
  delete resets the entity's columns and owners (which makes them cyclic), an assignment or
  a choice starts from the base value in the creating action and from any value elsewhere, a
  transition is its arms, and the clock ticks up. A raised mark, an existence, an owner key,
  and a status whose arms all point forward are monotone. The rule is checked on the cone,
  so a model that adds a backward arm loses the reduction and nothing else. The quotient is
  then computed on S *extended* with its inputs (`restrict_to` of S and the inputs, without
  the havoc of a `Triggered` parameter): the S actions are silent, and every T action that
  writes an input is a *rise*, kept with the guards it has on S and the inputs. The
  partition is stuttering-stable under the silent steps and strongly stable under the
  rises, so two states of a class rise into the same class. The reduced cone looks a rise up
  in a table, per class and new input bytes (`Reduction::rises`), whenever a T step changes
  the inputs, and the lift replays the same rise on the extended run.
- A read of T through a handler's `Triggered` row, or through a quantifier over a T entity
  linked to an S row, is *folded* into one Boolean per slot of its key, so the extended
  system sees a bit where it would see every input's history. The param fold
  (`FoldKey::Param`) folds the guards an S action has on T params that only key its row:
  the bit at slot `c` holds when the guards hold for some binding of the other T params,
  each on a present row. The node fold (`FoldKey::Linked`) folds a guard of an action with
  no T param that reads T only through a quantifier whose body is T atoms beside the link
  `owner[p] = row`: the bit is keyed by the linked row, and the guard reads `owner[p] = q ∧
  bit[q]` where the quantifier stood. The variants of one handler whose folds agree share a
  bit. A bit starts where the cone's initial states put it, or stays a raw input when they
  disagree. In the extended system each bit has a free rise and a free fall, so the quotient
  stays correct even when a fold is not monotone.
- The tables a reduced cone hands TLC are bounded. `Reduction::tla_cells` counts one cell per
  class for each observed var and each S action, plus one per rise, and a reduction past
  `TLA_CELL_BUDGET` (200,000 cells) is not used: the split without inputs is tried, and the
  cone stays unreduced when that fails too. Measured with TLC's 1 GB heap, a cone of 2,428
  classes prints 72,840 cells and runs, the insurance-claim-ops `ClaimCase` view's 4,368
  classes print 104,832 at a peak of 912 MB, and a random model's 24,000 classes printed
  326,400 and ran out of heap.

With inputs, the module prints each bit group as an operator `In_<g>(c)` over the T vars, the
`Rise` table (one cell per class, `:>` and `@@` from `TLC`, so such a module `EXTENDS TLC`),
and, in every action that writes an input, the conjunct `S_class' = IF inputs' = inputs THEN
S_class ELSE Rise[S_class][inputs']`, staged as one `LET` per input group after the
`UNCHANGED` that binds the primed inputs. The decoder reads an input from the module's own
var, not from the representative.

**Proof obligation.** Let C be the cone and R the reduced cone. The relation
`(t, s) ~ (t, rep(class(s)))` between their states is a divergence-blind stuttering
bisimulation that preserves T and `obs`:

1. Related states agree on T and on `obs`: a class is a block of the partition, whose first
   refinement is by `obs`.
2. A T step of C from `(t, s)` is a T step of R from `(t, rep)` and back: a T action reads T
   and `obs` only (`obs` is what T's actions read of S) and writes T only, so it is enabled
   in both and leads to related states.
3. An S step of C from `(t, s)` stays in `s`'s class, which is a stutter step of R, or enters
   a class `c'`; then a path of S steps takes `rep` into `c'` (the partition is stable), and
   its last step is a class move of R into `c'`, whose target is related to the step's. A
   class move of R from `(t, rep)` into `c'` is matched by that path from `s`, every state
   of which but the last is related to `(t, rep)`.
4. An initial state `(t0, s0)` of C is related to the initial state `(t0, rep(class(s0)))`
   of R, and R has no other initial state.
5. With inputs, the class of `s` is read at the inputs of `t`. A T step that leaves the
   inputs alone is matched as in 2. A T step that changes them is a rise of the extended
   system from `(s, inp(t))`; strong stability under the rises gives the representative a
   rise into the same class, which `Rise` takes. A fold is sound by the same argument with
   the bit in place of the vars it folds, since the folded guard is a function of them and
   the key. The monotone rule is not used here: it bounds the input histories, which keeps
   the extended system finite and small.

A divergence-blind stuttering bisimulation preserves every formula of CTL\* without the next
operator, and the reduced cone's findings are such formulas over T and `obs`: a state is
reached (a witness, a violated invariant), an edge is taken
(a use case that runs, an arm taken, a violated `after` assertion), and from every reached
state with a present slot some path takes an edge that moves it (no deadlock). A property
`Fair_P => (...)` over T and `obs` whose fair actions are T actions is a stuttering-invariant
formula over T and `obs` under a fairness that reads T and `obs`, so the `Cone_<P>` run, whose
`Spec` admits stuttering, decides on R what it decides on C. What the bisimulation does not
preserve is `WF_vars(Next)`: a class that loops inside itself forever never stutters in C under
`WF_vars(Next)` while R stands still. So the `_WF` run reads R with every class that can loop
marked ([WF_vars(Next) on a reduced cone](#wf_varsnext-on-a-reduced-cone)), and when that does
not decide, a counterexample of the `Cone_<P>` run on R is repeated on the unreduced cone as
`Cone_<P>_Full`, whose lasso the printed trace reads (`verify` reports that run's outcome beside
the first's; an `ok` there is a tool bug, reported as such), and the `_WF` run follows there.

A trace of R is lifted to a path of C before it is printed (`TransitionSystem::lift`): a T step
is itself, and a class move becomes the path of S steps that realizes it from the concrete S
state the lifted path holds, which exists by 3, recursively through nested reductions. The
violation is read again on the lifted last state, so the message is built from a real state,
and a printed counterexample can be longer than the reduced one and name a use case of S on a
step. The tests in `crates/rdra-ish-core/src/lifecycle/tests/reduce.rs` compare the reduced and
unreduced exploration of every sample cone (the states on T and `obs`, the views, the verdicts,
and that every lifted path is a path of the cone), and `engine_equivalence` runs TLC on each
reduced cone against its BFS and checks that each check the reduced cone keeps has the cone's
verdict. `scripts/reduction-diff.py` compares whole `verify` runs, reduced and unreduced, on
random models (see the `engine_equivalence` item under
[Approximations and limitations](#approximations-and-limitations)).

### WF_vars(Next) on a reduced cone

<!-- derived-from #reduction-by-an-independent-sub-lifecycle -->
<!-- constrained-by #temporal-properties-on-a-cone -->
The bisimulation above is blind to divergence, so the reduced cone R cannot answer the
`Cone_<P>_WF` run as it is: a behavior of the cone C that steps inside one class of S
forever meets `WF_vars(Next)`, and its image in R stands still at a state that may have a
move. The native checker runs `Cone_<P>_WF` on R anyway, with every class that *can loop*
taken as a place where a behavior may stand still forever. This section proves that an `ok`
of that run is the `ok` of `Cone_<P>_WF` on C. A counterexample of that run is only a
candidate, and the last paragraph says how it is checked.

**Classes that can loop.** Let `X` be the run of the extended sub-lifecycle from which the
partition is built, with its silent edges (S steps, at fixed inputs) and its rises (T steps).
When S is reduced inside, a state of `X` holds an inner class. A class `c` *can loop* when
one of these holds:

1. The silent edges of `X` between two different states that are both in `c` contain a
   cycle: a strongly connected set of at least two such states.
2. A state of `X` in `c` holds an inner class that can loop, by this definition applied to
   the inner reduction.
3. S is unobserved (`obs` empty), so it is one class and `X` is not explored, and some S var
   is not monotone (`monotone_vars` on the extended sub-lifecycle). When every S var is
   monotone, every var of S changes finitely often, and no infinite path of S steps exists.

The mark costs one strongly connected component pass over the in-class silent edges of `X`,
which `Quotient::of` already holds. It reads nothing outside the sub-lifecycle.

**The run.** On R, under `Fair_P` and the property's goal as before, a state `r = (t, c)` of
`N` may stop when no fair instance is enabled at `r` and either no move leaves `r` or `c`
can loop. A component of `N` admits a fair cycle by the rule it has today. Every stop of
this run is a stop of the `Cone_<P>` run on R, so the run lies between that run and the
blind `WF_vars(Next)` run on R.

**Claim.** Suppose C has a behavior σ that satisfies `Fair_P` and `WF_vars(Next)`, reaches
a state where the trigger holds, and then never meets the goal. Then the run on R finds a
counterexample. So when it finds none on a complete graph, `Cone_<P>_WF` on C is `ok`.

**Proof.** Map each state `(t, s)` of σ to `(t, rep(class(s)))`. By points 2, 3, and 5 of
the proof obligation, each step of σ is a step of R or stays at the same state of R, so σ
maps to a path of R from an initial state. Related states agree on T and `obs`. The trigger,
the goal, and the enabledness of every fair instance read only T and `obs`, since the fair
actions are T actions. A fair instance takes a step of σ exactly when its image takes a step
of R, because a T step is the same step on both sides. Now split σ by what its image does
after the trigger.

- *The image moves infinitely often.* From some point the image visits only the states and
  edges of one strongly connected component `K` of `N` in R. Each fair instance is either
  taken infinitely often by σ, and so labels an edge inside `K`, or disabled infinitely
  often, and so disabled at some state of `K`. `K` has an edge, so `K` admits a fair cycle.
- *The image moves finitely often and σ changes the state infinitely often.* From some
  point σ holds T at `t`, and every step that changes the state is an S step inside one
  class `c`. If infinitely many of them move between states of `X`, those states
  are finitely many, so they contain a cycle of in-class silent edges, which is case 1. If
  from some point none does, σ steps inside one inner class forever, and the same argument
  one level down gives case 2. With `obs` empty, an infinite path of S steps changes some S
  var infinitely often, so that var is not monotone, which is case 3. So `c` can loop. Every
  fair instance is disabled at `(t, s)` from that point on, since `Fair_P` holds and none
  takes a step. Its enabledness is the same at `(t, rep(c))`. So `(t, rep(c))` may stop.
- *σ stands still at `(t, s)` forever.* `WF_vars(Next)` then says that no move leaves
  `(t, s)` in C. A T move of R from `(t, rep(c))` reads only T and `obs`, so it would leave
  `(t, s)` too. A class move from `rep(c)` into `c'` would, by stability, be matched by a
  path of S steps from `s` into `c'`, which has a first step. So no move leaves
  `(t, rep(c))`, and it may stop.

In each case the image of the triggered state reaches a fair end of the run through goal-free
states, so the run finds a counterexample. ∎

The claim does not use that the mark is tight. Marking more classes only adds stops, which
keeps the claim and adds candidates. What `Cone_<P>_WF` on C decides is unchanged: its
counterexample is the whole model's, and its `ok` is the whole model's only when the cone
holds every var an action writes (`cone_keeps_every_write`). Otherwise the property goes to
the whole model, as it does after the unreduced run.

**Candidates.** A stop at a class that can loop need not exist in C. The class may hold
states that can loop and states that cannot, and C may reach it at `t` only in the second
kind (`cone_reduced_loop_unreached` in the corpus). The looping states may also be ones that
C does not reach at `t` at all, since `X` takes its rises without their T guards and folds
bits that rise and fall freely. So a counterexample of the run on R is
lifted to C and certified there before anything uses it. The prefix lifts with `lift`. A
loop of R lifts by lifting the prefix and the loop repeated, until the concrete state at the
end of a copy is one at the end of an earlier copy. That takes at most as many copies as the
class has concrete states. A stop lifts to the concrete state `x` that the lifted prefix
ends at, followed by a walk from `x` of S steps of C that keep T and `obs` as they are at
`x`, to a state with no move or around a cycle. The lifted lasso must replay through the
moves of C, fail the property there, and pass the fairness check under `WF_vars(Next)`. A
lasso that fails a check, or a walk that finds neither, is dropped unprinted, and `verify`
runs `Cone_<P>_Full` and `Cone_<P>_WF` on C as it did before.

### Part of a model

<!-- derived-from ./cli-reference.md#partial-runs -->
`verify --entity E` checks the entity's view and the invariants and after-asserts that
read `E`, each on its own cone, so each verdict is the whole-model verdict for that target.
`verify --buc B` builds the system with only the use cases reachable from `B`, so it cannot
decide whether a state has a witness, a use case runs, or an instance gets stuck. A scoped
run checks no temporal property, because it has no whole model. A build error that no
selected cone reads is deferred: one entity that cannot be exported does not block the
rest. The whole run still fails on it:

```
$ rdra-ish verify samples/formal-verification/partial_scope.rdra --entity Order
bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); no state cap per BFS cone, memory 16GiB; entities a check does not read are unbounded
scope: --entity Order
cones: 1 for 1 invariant(s) and 0 after-assert(s)
temporal properties: none
not checked: 1 entity view (Audit), 1 invariant (Forbidden_Audit_1), 0 after-asserts; 1 build error deferred; 1 finding not decided
  deferred error[DerivedColumnRead_Audit_score]: Audit: Audit.score is derived by arithmetic, which the lifecycle cannot evaluate; compare the columns it reads instead
  whether Record can run is not decided: no cone of a selected entity keeps it
state models (BFS on 1 entity cone): 1 state model, 2 states witnessed, 0 deadlocks, 0 use cases that never run
Cone_Forbidden_Order_0 [Forbidden_Order_0]: ok (7 states)
result: ok
OK: no errors in the selected scope, within 2 instances per entity, Int 0..5
```

The rules for each flag and for deferral are in
[cli-reference.md](./cli-reference.md#partial-runs).

## Proofs for any number of rows

`verify --prove` is experimental. BFS and TLC check a model at a fixed number of instances
and an Int range with a top, so a rule that holds at two instances can still break at three,
and one that holds while every number stays under 6 can break at 6. `--prove` asks z3
whether each state rule holds for **any** number of rows and **any** non-negative Int value,
at any time: `now` has no top. It never finds a counterexample: the bounded engines do that,
as before.

Each entity's rows become an uninterpreted sort, each column a function from rows to its
domain, and each key a function from child rows to parent rows. Each use case's actions
become a step relation built from the same transition system BFS explores: guards, writes,
creates, deletes, and choices. A rule is proved when it is part of an **inductive** set of
formulas: every formula of the set holds in every initial state, and no step from a state
where all of them hold breaks one. Then no number of rows and no number of steps can break
it.

The step relation is the explorer's, with two widenings. A creation may take any absent row,
not only the lowest one. Numbers have no top: an Int column and `now` range over every
non-negative integer, `Tick` always runs, and a write of any value (`sets(.., col ==
present)`, a copy) takes any number. Both add steps and remove none, so a proof stays sound
at every bound. Enum columns keep their declared values. A guard
or write the encoding cannot state is widened the same way (the guard is dropped, the write
takes any value) and printed as a `note:` line. A rule the encoding cannot state is
`out of scope`, never approximated.

Most true rules are not inductive alone: a step breaks them only from a state no run
reaches. The prover therefore adds candidate **lemmas** and keeps the largest set of
rules and lemmas that is inductive together (the Houdini algorithm):

- An absent row holds base values, a row of an entity no use case deletes is never
  deleted, and a present row's key never names an absent row.
- What BFS shows on each entity cone and each rule's cone, at the run's bounds and again at
  one instance per entity: the patterns a present row takes, the values of each column and
  of each pair of columns, the patterns a child row takes with the parent its key names (whole, per parent column,
  and one column of each), and the pairs of patterns two children of one parent take. The
  pairs of one column of each are many on a model with many columns, so they join only
  when a rule stays unproved without them, and the proof runs once more. Each mining run
  stops at 50,000 states. A cone that passes this cap at the run's bounds usually finishes at
  one instance, so mining there as well keeps the candidates a one-instance proof uses. A
  lemma that only the one-instance run offers prints with `(mined at 1 instance per entity)`.

A lemma is only a guess until it survives the same checks, so a capped exploration costs
proofs, not soundness. The lemmas a proof kept print as `lemma:` lines.

When no mined lemma rules a CTI's state out, write the missing fact as a rule with
`.lemma()`:

```rdra
invariant(Order).when(status == placed).none(Shipment).label("a placed order has no shipment yet").lemma()
```

A rule marked `.lemma()` is checked by `check` like any rule, so a wrong lemma fails there
with a trace. `verify` runs no cone for a lemma: the proof keeps a lemma only when it
proves it, so a wrong one never helps a proof, and a lemma that reads two large cones would
join them into one far larger cone. In the proof a lemma is one more candidate. It prints as `proved: lemma <name>` or `unproved: lemma <name>`, and the
`proof: your lemmas: ...` line counts it apart from the rules, so it never counts toward the
`n of m rules` of the `OK:` line.

A lemma reads only what a rule reads, so it cannot name a split use case's run mark
(`_run_<UseCase>`) or an event mark (`_raised_<Event>`). A CTI that turns on a mark usually
has a fact about columns behind it, one step earlier. Find it by following the CTIs back.
insurance-claim-ops' 「賠償の請求は示談書がそろってから完了する」 is an example. Its CTI is a
case between OpenSettlement's two steps (`_run_OpenSettlement=1`) with a completed claim. A
lemma about cases in `investigating` breaks one step earlier, at RequestInvestigation, from
`responded`. Generalizing over every status before negotiation gives the lemma below. With it,
the mined lemmas that pair one column of a claim with one column of its case become
inductive, and together they rule the CTI out, so the rule is proved. Neither the lemma nor
those mined pairs prove it alone:

```rdra
forbidden(Claim, ClaimCase, Claim.completed_on == present, ClaimCase.status in (received, responded, investigating), ClaimCase.coverage in (bodily_liability, property_liability)).along(Claim, ClaimCase).label("賠償責任の事故は示談交渉の前に請求を完了しない").lemma()
```

```
proof (Z3 version 4.8.12 - 64 bit): 2 state rules, for any number of rows and any Int value
proved: Forbidden_Order_0
proved: Inv_Order_0
lemma: an absent Order row holds base values
lemma: no Order row is ever deleted
lemma: every present Order row is one of 5 reached patterns of (status, delivered_at)
lemma: a present Order row's (status, delivered_at) is one of {(pending, null), (paid, null), (shipped, null), (delivered, present), (cancelled, null)}
proof: 2 proved, 0 unproved, 0 out of scope
...
OK: no errors within 2 instances per entity, Int 0..5; 2 of 2 rules proved for any number of rows and any Int value
```

A rule that is not proved prints a **counterexample to induction** (CTI): a step, the rows
it binds, and the rule's rows, before and after. A run mark (`_run_<UseCase>`) or an event
mark (`_raised_<Event>`) the step or the rule reads is printed with the columns. The state before holds the rule and every
kept lemma, but it need not be reachable. It shows which fact a lemma would have to add:

```
unproved: Unique_Shipment_Order: not inductive; Ship(Order#1, Shipment#1) breaks it from a state where it and every proved rule hold (not necessarily reachable)
  Order#1 (present) status=placed -> status=shipped
  Shipment#1 (absent -> present) Order=Order#2 -> Order=Order#1
  Shipment#2 (present) Order=Order#1
```

Here the CTI is also a real run: `one_to_one_shared` links two Shipments to one Order, and
`verify` at two instances reports the violation with its trace. A rule z3 gives up on
(each check of a rule has 10 s, of a lemma 0.5 s) is `unproved` with `z3 gave up on <action>`. The checks of one
step are shared out over several z3 processes (`RDRA_JOBS`); the proof and its CTIs do not
depend on how many. A rule that fails in an
initial state is `unproved` too; the bounded engines report that violation.

Some rules have no `CHECK` in the schema `export --kind sql` writes: a rule over several
entities, a quantifier rule, a minimum cardinality, a rule that reads `now`, and a rule a
`.when(Cond)` guards. The script lists them under `Not enforced by this schema`, because the
application has to keep them. `--prove` marks each such rule with
`[not enforced by the SQL schema: <why>]`, and adds a line that counts them:
`proof: the SQL schema leaves <n> rules to the application: <a> proved, ...`. A proved one is
an obligation the use cases already discharge, for any number of rows.

`--prove` reads the state rules only. After-asserts and temporal properties are not tried.
The result does not change the exit status unless `--deny-unproved` is given. Every rule of
the domain samples proves, insurance-claim-ops' with the lemma above.
`tests/proof_equivalence.rs` checks on every sample that no rule it proves has a BFS
counterexample at one, two, or three instances.

The three engines are held to each other, and to verdicts stated by hand, on an adversarial
corpus of small models, each built around one way they could differ: see
[engine-agreement.md](./engine-agreement.md) for the agreement rules, the corpus, and how
to add a case.

## Approximations and limitations

- **Bounds.** A `verify` OK holds for the stated `InstanceCount` and `IntRange` and says
  nothing outside them. The `.tla` header comment and the `bounds:` line of `verify`
  state both, and so does the final `OK: no errors within ...` line. A scoped run says
  `OK: no errors in the selected scope, within ...` and holds only for the selected
  targets. A deleted slot is never reused, so an entity is created at most
  `InstanceCount` times.
- **Conditions are checked.** A `holds` body is a guard on each use case it applies to.
  A body applied to a use case that writes nothing is `UnbindableCondition`, an error.
- **One use case across entities.** A use case is one atomic action per transaction
  group it writes (`Pay(p0, p1)` moves the Order and its owned Payment in one
  step), so `cross_order_payment` passes. A use case whose writes span k groups is k
  actions `<UC>_1..k`, one run in order on one run's rows, with any other action between
  two of them, so `cross_order_payment_split` (`Pay` through two APIs, the capture first)
  fails: `Cancel` runs between `Pay_1` and `Pay_2`. `split_run_order` passes: the
  settlement step runs once, after the case step, under that case. `quantifier_none`
  still fails.
- **Int arithmetic.** Comparisons and `sets(..., E.col == N)` on Int, Money, Decimal,
  Date, and DateTime columns use TLC integers on `IntRange` (default `0..5`, override
  with `--int-range-max`). A literal or a numeric `@default` outside that window is
  `IntLiteralOutOfBounds` in `check` and `states` too: raise `--int-range-max` or lower
  the value. `@default(0)` is accepted for Int columns.
- **`now`.** `now` is a global Int clock with a `Tick` action. Nothing pins a column
  that `now` is compared to, so the clock can tick past it and TLC finds a violation of
  `forbidden(col < now)` (see `now_coupon.rdra`).
- **`WF_vars(Next)`.** Fairness is on the whole `Next` disjunction, not per action. Another
  instance's endless loop satisfies it, so a property about one instance fails on a run that
  only moves the others (`temporal_fair.rdra`). `.fair(UC, ...)` adds weak fairness per
  instance on the use cases a property names. There is no strong fairness: a step enabled and
  disabled forever is not forced.
- **A property without `.fair` tends to need the whole model.** Its verdict rests on
  `WF_vars(Next)`, which a cone keeps only when it holds every var an action writes (see
  [Temporal properties on a cone](#temporal-properties-on-a-cone)); otherwise the property
  runs on the whole model, which explores every reachable state at once and on
  `samples/patent-ops`, `samples/food-permit-ops`, or `samples/insurance-claim-ops` at
  `InstanceCount=2` stops at the budget or the state cap as `not decided`. A `.fair` that names
  the use cases the property relies on keeps it on its cone.
- **`.along`.** An `.along` path needs a `relate` that binds each adjacent pair. Without
  one, the rule is `UnbindableScope`, an error in every command, not a dropped filter.
- **Quantifiers.** `has` and `none` range over the `InstanceCount` ids. Where a `relate`
  declares the relationship, an owner function links the related instances. Otherwise the
  related side is independent.
- `check` and `states` run BFS on the same system, at one instance per entity unless
  `--instance-count` says otherwise, so `check` finds the same violations as TLC in that
  window. The test `engine_equivalence`
  (`crates/rdra-ish-cli/tests/engine_equivalence.rs`) runs BFS and TLC on every cone of
  every sample, at 1 and at 2 instances per entity. On each cone it compares:
  - The reached states and the initial states. TLC's side is the graph that
    `tlc -dump dot,actionlabels` writes; each node decodes to a model state.
  - The edge sets, as `(from, to, use case)`. Self-loops count. TLC repeats an edge when
    two bindings reach the same successor, so the test compares sets, not counts.
  - The state-model findings of every entity on both graphs: which states are witnessed
    or `NoWitness`, which use cases never run (with the reason), and which states are
    deadlocks (by name and severity).
  - The verdict of each invariant and `after` assertion. Every trace TLC prints for a
    violation must replay through the BFS step relation and end in the check TLC named.
  - At 2 instances, the deadlocks and missing witnesses TLC alone finds on the
    `StateModel_` specs `export` writes, rerun by hand as the specs' cfgs say, and those the
    BFS finds. A witnesses cfg of a cone above 1,000 states is rerun without `-continue`.
    A cone that `VERIFY_FALLBACK` lists is left out of this comparison, by both engines.

  At 2 instances, a cone whose BFS, reduced as `verify` explores it, passes the test's cap
  of 300,000 states is compared at 1 instance only. The test decides this on the run
  `verify` uses: the reduced cone where the cone reduces, the cone itself where it does not.
  It lists these cones in `VERIFY_FALLBACK` and fails when the list changes, so a cone that
  grows past the cap fails it until it is listed. Today 24 cones in `samples/clinic-ops`,
  `samples/patent-ops`, `samples/food-permit-ops`, `samples/insurance-claim-ops` and
  `samples/oidc-auth` are listed. The table gives each listed cone and each cone the cap of 300,000 took off the
  list, at 2 instances, measured with
  `lifecycle_measure -- 9000000 verify <sample>` (`RDRA_CONE_VARS=1` prints the vars of the
  cone and of its sub-lifecycle). "Sub-lifecycle" is the S side's states and classes; T is
  what the reduction cannot move out of the target's side.

  | Cone (first target that reads it) | Reduced | Unreduced (1.2.0) | Sub-lifecycle | Why it is this large |
  |---|---:|---:|---:|---|
  | clinic-ops entity `EligibilityCheck` | 905,528 | 1,784,808 | none | `CheckInPatient` checks the appointment in and creates the check (#80), so the appointment, the patient account and the check share T. Nothing reduces. The event order of #112 took it from 1,784,808 to 905,528 states, since a handler no longer runs before its event (inferred from the unchanged split). |
  | patent-ops entity `OfficeAction` | 1,341,707 | over 9,000,000 | 14,953 / 185 | T is the office action alone. S is the application with its drafting order, disclosure, inventor and firm. The guards `UnderExamination` and `ExaminationOver` read the application's status, and the office action is owned by it, so 185 classes stay apart. The size is the office action's lifecycle at 2 instances times those classes. |
  | patent-ops invariant `Forbidden_OfficeAction_3` (also `Exclusive_OfficeAction_0`) | 1,341,707 | over 9,000,000 | 14,953 / 185 | The vars of `OfficeAction`'s cone; a cone of its own because it keeps the invariants' checks. |
  | food-permit-ops entity `BusinessPermit` | 4,648,771 | 7,813 | 52,444 / 444 | Since #112 the permit's handlers read the marks the disposition's and the renewal's raisers set, which pulls those raisers into the permit's cone. They are the sub-lifecycle, extended with the marks as inputs. |
  | insurance-claim-ops entity `ClaimCase` | 4,580,087 | 63,823 | 93,904 / 4,368 | Since #112 `CloseCase` and `AssessDamage` run on the claim's and the settlement's events, so the claim and the payment join the case's cone. They are the sub-lifecycle, read through two bits per case (a node fold of `CompleteClaim`'s settlement guard and the handlers' param fold). |
  | insurance-claim-ops entity `Payment` | 903,987 | over 9,000,000 | 255,292 / 2,428 | S is the accident case with its investigation and settlement, which the guard of `CompleteClaim` reads, extended with the claim's events that close the case. T is the claim and the payment: the claim's rules read its columns (`period`, `grade_explained`, `fraud_suspected`, `reason_explained`, `late_interest_due`), and the payment reads the claim. |
  | insurance-claim-ops entity `Claim` | 836,427 | over 9,000,000 | 255,292 / 2,428 | As `Payment`, with `senior_approved` and `completed_on` in place of the payment's columns. |
  | insurance-claim-ops invariant `MultiForbidden_Payment_3` (also `AfterAssert_PayClaim_0`, a separate cone of the same size) | 783,427 | over 9,000,000 | 255,292 / 2,428 | As `Payment`, without the payment's status. |
  | insurance-claim-ops invariants `Forbidden_Claim_0` and `Forbidden_Claim_4` (two cones, with the claim's other rules) | 633,787 | over 9,000,000 | 255,292 / 2,428 | As `Claim`, with fewer of the claim's columns (no `completed_on` or `late_interest_due`). |
  | oidc-auth entities `AccessToken`, `AuthorizationCode`, `AuthorizationRequest`, `Consent`, `Grant`, `IdToken`, `Interaction` and `RefreshToken`, and invariants `Forbidden_Interaction_1`, `MultiForbidden_AuthorizationCode_3`, `Quantifier_Grant_0`, `Unique_Grant_AuthorizationRequest` and `Unique_AuthorizationCode_Grant` (13 cones) | over 2,000,000 | over 2,000,000 | none | The browser flow is one cone: the request, the interaction, the session it logs in or reuses, the consent, the grant, its code, and its tokens read each other through guards and raised events. Nothing reduces. The sample's `verify` header runs it at 1 instance (30,277 states) with `--prove`. |
  | patent-ops entity `PatentApplication` (compared at 2 instances since the cap of 300,000) | 293,833 | 2,167,411 | 73 / 73 | The filing guard `DraftsApproved` reads the drafting order and `FileApplication` moves the order to `filed`, so the order joins the application in T. S is the disclosure, its inventor and the firm; S is reduced inside, so its 73 states are already a quotient, and `obs` tells them all apart. |
  | patent-ops entity `Patent` and invariant `Forbidden_Patent_4` (two cones; compared at 2 instances since the cap of 300,000) | 202,569 | 1,494,339 | 73 / 73 | As `PatentApplication`, with the patent's columns in T beside fewer of the application's. |
  | food-permit-ops entity `PermitApplication` (off the list since #112) | 52,597 | 2,094,157 | 348 / 228 | The order lets the inspection's and the permit's side reduce against the application's; 1.2.0 listed this cone. |

  The Reduced and Sub-lifecycle columns are measured on the #112 build with
  `RDRA_REDUCED_ONLY=1 lifecycle_measure -- 9000000 verify <sample>`; the Unreduced column is
  the 1.2.0 measurement, before the event order, and is not measured again.

  Raising the cap from 200,000 to 300,000 took six cones off the list: the patent-ops
  `PatentApplication`, `Patent` and `Forbidden_Patent_4` cones above and three food-permit-ops
  `Inspection` cones of 232,885 states, which #112 reduces to 5,885. The test went from
  194 s to 239 and 243 s locally (18 cores, 8 workers; one run before, two after), and every
  verdict stayed the same. The smallest listed cone, 633,787 states, would need TLC to dump a state graph twice that size for each of the
  insurance-claim-ops cones.

  The insurance-claim-ops cones hold the settlement guard of `CompleteClaim` (see the
  sample's README), which #74 made checkable. Before the reduction the list held 19 cones:
  the clinic-ops views of `Charge` (over 8,388,608 states unreduced, 38,533 reduced),
  `Prescription` (4,266,635; 1,690), `ClinicalOrder` and `LabResult` (619,835; 250),
  `PatientMessage` (441,287; 381) and `CarePlan` (299,467; 269), the invariants
  `Forbidden_ClinicalOrder_1`, `Inv_ClinicalOrder_3` and `Inv_Prescription_4`, and the
  food-permit-ops view of `Disposition` with its two invariants (231,493; 13,230). Except
  for `CarePlan` and the three `Disposition` cones, which the cap of 300,000 admits
  unreduced, these cones are compared at 2 instances reduced only: their unreduced BFS
  passes the cap, so the unreduced cone is compared with TLC at 1 instance, and at 2 the
  reduced cone's BFS, its TLC run, and its verdicts against the unreduced verdicts at 1
  instance. The cap of 300,000 took the cones compared unreduced at 2 instances from 29 to
  30 in clinic-ops (`CarePlan`) and from 14 to 20 in food-permit-ops (the three
  `Disposition` and the three `Inspection` cones), and the reduced cones compared in
  patent-ops from 10 to 13. Every clinic-ops
  cone that reads `Appointment.status` carries the two run marks of `CheckInPatient`, which
  checks the appointment in through one API and creates the eligibility check through
  another (#80): 2.3 to 2.6 times the states it had when the two steps ran in any order. The
  other 30 clinic-ops cones at 2 instances (the largest `CarePlan` at 299,467) are
  compared unreduced, and reduced where they reduce. CI runs the test with `cargo test --release`
  and sets `RDRA_REQUIRE_TLC=1`, so a missing `tlc` fails the job. The debug build took
  315 s on the CI runner, over its 300 s budget; the release build takes 121 s locally
  (Apple silicon, measured).

  `scripts/reduction-diff.py` is the differential test of the reduction on random models
  (#102). Per seed it generates a model of three to five entities: `relate` chains,
  guards with `has` and `none`, `.along` rules, a use case split over two APIs, `chooses`
  with and without `.when`, a `Date` deadline that a timer's use case waits for with a `now`
  comparison, `after` assertions and temporal properties, at 2 or 3 instances per entity. It
  runs `verify --deny-undecided` on the model as users run it and again with every cone
  unreduced (the test hook `RDRA_ISH_NO_REDUCE`, which is not a setting), and compares the
  exit code, the finding names, the verdict of each check, and the state-model summary; the
  state counts and traces differ by design. A diverging model is copied out as `.rdra`, and
  `minimize` shrinks it by deleting lines while it still diverges. A seed past the per-run
  timeout is listed with the side that timed out, never dropped. CI runs a fixed set of
  quick seeds in formal-verify with `--fail-on-timeout`, so a seed that slows down fails the
  job rather than leaving the set; larger sweeps run locally under `nix develop`:

  ```sh
  cargo build --release -p rdra-ish-cli
  nix develop -c python3 -I scripts/reduction-diff.py --timeout 120 \
    run target/release/rdra-ish --seeds 0:500
  ```
- User-facing state diagnostics and constraint names (`MultiForbidden_*`, `MultiInv_*`) for
  multi-entity rules say **multi**/**multi-entity**.

## Known verification gaps

<!-- derived-from #approximations-and-limitations -->
<!-- derived-from ../samples/patent-ops/README.md#known-verification-gaps -->
<!-- derived-from ../samples/food-permit-ops/README.md#known-verification-gaps -->
<!-- derived-from ../samples/insurance-claim-ops/README.md#known-verification-gaps -->
<!-- constrained-by ./language-reference.md#triggers-and-event-driven-buc-chaining -->
A passing `check` or `verify` says nothing about what it does not explore. This section
names each gap that 2.0 keeps, how to see it in a run, and what you can do about it. The
[compatibility contract](./compatibility.md#bounds) promises results only within these
gaps.

- **The order is by row and the mark counts no events.** A use case that a use case's
  event triggers runs only after a raise on its rows (#112), but the record of the raise is
  a latch: after the first raise the handler runs as often as its states and guards allow,
  a second raise queues nothing, and a raiser never waits for its handler. The order ties
  the handler to the raiser's row only through a parameter that is that row or its parent
  through the raiser's key; a handler that binds the entity otherwise runs on any raised
  row (`UnjoinedTrigger`), and raisers with no entity in common order nothing
  (`UnanchoredTrigger`). Timers and the events of external systems are free. In a cone of
  an invariant, an assert, or a property that does not hold every raiser by its own writes,
  the handler's mark is left out and it runs unordered there, as in 1.x
  ([Raised marks](./state-derivation.md#exploration-by-cone-of-influence)): an OK is sound,
  and a finding can be a run the order prevents. See it: the two infos on the `triggers`
  line, and the `note:` row of a counterexample, `runs <UC> without <Ev> having been
  raised`, which names each step that ran without its event. What you can do: a handler
  that must run once per event takes a from-state the raiser sets and the handler leaves,
  as a `transitions` arm; a rule that must be checked with the order reads something the
  raiser writes, which pulls the raiser into its cone. [`triggers` and event-driven BUC
  chaining](./language-reference.md#triggers-and-event-driven-buc-chaining) has the rules
  from the model's side.
- **Some cones are compared with TLC at 1 instance only.** `engine_equivalence` checks that
  BFS and TLC agree on the states, edges, findings, and verdicts of every cone, unreduced
  and reduced. A cone whose BFS, reduced as `verify` explores it, passes the test's cap of
  300,000 states at 2 instances is compared at 1 instance only. `VERIFY_FALLBACK` in
  `crates/rdra-ish-cli/tests/engine_equivalence.rs` lists them, 24 cones today (19 before the
  reduction of #74, 16 after the patent-ops filing guard, 10 since the cap went from 200,000
  to 300,000, 11 since the event order of #112, and 24 with `samples/oidc-auth`). The table under
  [Approximations and limitations](#approximations-and-limitations) gives each cone's size
  and what keeps it large:
  - `samples/clinic-ops`: entity `EligibilityCheck`, 905,528 states, which nothing
    reduces.
  - `samples/patent-ops`: entity `OfficeAction` and invariant `Forbidden_OfficeAction_3`
    (with `Exclusive_OfficeAction_0`). The guards that read the application put it, with
    the drafting order and the disclosure that the filing's guard adds, into the cone: over
    9,000,000 states, 1,341,707 reduced to the office action times 185 classes of the
    application's side.
  - `samples/food-permit-ops`: entity `BusinessPermit`, 4,648,771 states reduced: the
    permit's side times 444 classes of the disposition's and the application's, which the
    permit reads through the events that start `RevokePermit`, `SuspendBusiness` and
    `ExtendPermitTerm` (#112).
  - `samples/insurance-claim-ops`: entities `Claim`, `ClaimCase` and `Payment`, invariants
    `Forbidden_Claim_0`, `Forbidden_Claim_4`, and `MultiForbidden_Payment_3`, and the
    after-assert of `PayClaim`. The settlement guard of `CompleteClaim` joins the case's
    lifecycle to the claim's; reduced by it (255,292 states in 2,428 classes, with the
    claim's events as input bits) they reach 633,787 to 903,987 states. The `ClaimCase`
    view is reduced the other way, by the claim's and the payment's side under the case's
    two bits (93,904 states in 4,368 classes), to 4,580,087.
  - `samples/oidc-auth`: 13 cones of the browser flow, over 2,000,000 states each, which
    nothing reduces. `verify` at 2 instances does not finish them in 15 GB, so the sample's
    header checks them at 1 instance with `--prove`.

  `verify` still checks these cones at 2 instances, except the oidc-auth cones its header
  runs at 1: the native checker decides each, and `--with-tlc` runs TLC on each beside it.
  Only the agreement of the two engines on the states and edges is tested at 1 instance.
  This is accepted for 1.0. See it: the list in the test, which fails when a cone joins or
  leaves it. What you can do: run `verify --with-tlc` for a second opinion on these cones,
  and keep a new model's cones under 300,000 states at 2 instances if you want the agreement
  tested.
- **insurance-claim-ops cannot check that a payment deadline follows its reason.** The
  deadline is writable, `sets(UC, E.due_on == N).when(Reason)` with a timer use case that
  waits for `due_on <= now` (#98), but the cone it adds does not decide at the cap: with a
  `PaymentExtension` entity the Claim cone passes 20,000,000 states and `verify` passes 6 GB;
  with the reason and `due_on` on `Claim`, the Claim view passes 20,000,000 states with one
  reason (126 s) and is unfinished after 7 minutes and 5.8 GB with five. Without the deadline
  the Claim cone is 836,427 states reduced. See it:
  [Known verification gaps](../samples/insurance-claim-ops/README.md#known-verification-gaps).
  What you can do: keep the reason out of the checked rules, as the sample does.
- **The `verify` BFS stops at the memory limit, or at `--max-states`.** The BFS pass that
  finds missing witnesses, deadlocks, use cases that never run, arms never taken, and values
  never chosen has no state cap unless `--max-states` gives one; `--memory` stops it. A cone
  that stops decides none of these findings, and an invariant's cone that stops leaves its
  invariants `not decided`. The count is the states of the cone reduced
  by its independent sub-lifecycle (see
  [Reduction by an independent sub-lifecycle](#reduction-by-an-independent-sub-lifecycle)).
  Every sample cone is under 8,388,608 states, the cap before `--max-states` (default none) (the largest, `BusinessPermit` of
  food-permit-ops, has 4,648,771 states reduced; `Charge` of clinic-ops, over 8,388,608
  unreduced, has 38,533), so `verify --deny-undecided` decides every check on all of them;
  the price is insurance-claim-ops's 180 s and 2.1 GB. See it: each undecided finding under
  `not checked:` on stdout and the `warning:` on stderr that counts them (`--deny-undecided` fails the run instead). What
  you can do: rely on `check` at 1 instance for those findings, or shrink the cone, for
  example by removing a guard that pulls another entity's columns in. Or raise
  `--memory`; `verify --max-states <N>` caps every BFS run, to bound the time. The
  whole-model run of TLC under `--with-tlc` keeps its own cap of 2,097,152 distinct states
  unless `--max-states` is given.
- **`check` has no state cap, and a cone stops at the memory limit.** `check` and `states`
  hold at most `4GiB` of states, `verify`'s BFS half of `16GiB`; `--memory` or `memory` in
  `rdra.toml` changes either. `--max-states <N>` adds a state cap, and a cone past it is the
  `StateCapReached_<E>` error, so `check` fails rather than passing on part of the cone. A
  cone that reaches the memory limit stops the same way: `MemoryCapReached_<E>` in `check`,
  and in `verify` its findings under `not checked:`, named `stopped at the memory limit`. The
  `check` message gives the states the cone held and the bytes each took, and, when the cone's
  variables bound its states, an estimate of the memory to finish it (`at least about
  12GiB`, for the cone alone, from the edges per state seen so far) and the `--memory` to give. The
  bound is the product of the variables' domains, so on a large cone it is loose: the message
  then says it is over `1TiB` and suggests narrowing. See it: the error, whose message names
  the variables the reached states vary in. What you can do: raise `--memory`, or narrow the
  cone by moving a guard that reads another entity.
- **Int columns range over `0..5`, and `now` is a bounded clock.** Every Int, Money,
  Decimal, Date, and DateTime column takes values in `IntRange`. `now` starts in that range
  and a `Tick` action raises it to the top, so a run never sees a later time. See it: the
  `Int 0..5` in the `bounds:` line, and `IntLiteralOutOfBounds` for a literal or a
  `@default` outside it.
  What you can do: `--int-range-max N` on `check`, `states`, `export`, or `verify` widens
  the range.
- **Instances are bounded: by default 1 per entity in `check` and `states`, 2 in `verify`.** A rule that needs two
  instances to break, such as `Unique_*`, holds vacuously in `check`. A rule that needs
  three is not checked by default. Each entity is created at most `InstanceCount` times,
  because a deleted slot is never reused. See it: the `bounds:` line. What you can do: run
  `verify`, or pass `--instance-count N` to `check`, `states`, or `verify` for a rule about
  more instances, or run `verify --prove` to prove a state rule for any number of rows
  ([Proofs for any number of rows](#proofs-for-any-number-of-rows)).
- **A scoped `verify` checks no property.** `check` and `states` verify safety by state
  exploration and evaluate no `property`, as the top of this page says; `verify` checks
  every form ([Recoverability on a cone](#recoverability-on-a-cone) for `can_reach`). A
  run with
  `--buc` or `--entity` checks no property: `--buc` drops use cases a property's fairness
  reads, and `--entity` may defer an error inside a property's cone. See it: the `note:`
  from `check`, and `temporal properties: 2, not checked: a scoped run checks none` in a
  scoped `verify`. What you can do: run an unscoped `verify`.
- **A property its cone cannot decide runs on the whole model under a budget.** A property
  whose first cone run finds a counterexample and whose `_WF` run finds none needs the whole
  model's `WF_vars(Next)` (see [Temporal properties on a cone](#temporal-properties-on-a-cone));
  so does a property whose cone frees an entity's slots and finds a counterexample. The
  whole-model run stops at `--temporal-budget` seconds (300 by default), at the `--memory`
  limit, or at `--max-states` states when given (TLC: 2,097,152 distinct states unless
  given), and the property is then `not decided`, which warns and does not fail
  the run unless `--deny-undecided` is given. The `not checked:` line names the cause. At
  the state cap it says that `--temporal-budget` does not lift the cap and offers
  `--max-states`; at the time budget it offers the budget. Both suggest `.fair(UseCase, ...)` naming the use cases the property
  relies on, which decides it on its cone. On
  patent-ops, food-permit-ops, and insurance-claim-ops at 2 instances a whole-model run does
  not finish (14 million states in 11 minutes, 6.9 million in 300 s, and 2.3 million with 1.7
  million queued at the state cap in 143 s; insurance-claim-ops's two properties are on their
  cones only with `.fair`). See it: `not decided` on the
  property's line and under `not checked:`. What you can do: add `.fair(...)` naming the
  use cases the property relies on, which keeps it on its cone, or raise the budget.
- **food-permit-ops checks its renewal guard just under the cap.** A renewal application
  names the permit it renews (`renews`, a second key between the two entities since #73), and
  `失効後の更新はできない` guards `ApplyRenewal` and `RenewPermit` on that permit's status. The
  guard adds the permit's existence, its status and the `renews` link to the application's
  cone: at 2 instances the view of `PermitApplication` grows from 127,165 to 2,094,157
  states, a quarter of the cap of 8,388,608, and the views of `Inspection` and `Disposition`
  from 14,197 and 23,156 to about 232,000 each, unreduced. Since #112 the reduction takes
  the application's view to 52,597 states, the inspection's to 5,885 and the disposition's to
  138,246, while the permit's view, which now holds the raisers of the events that move the
  permit, is 4,648,771 states reduced, about half the cap. `verify` decides every check
  (36 s and 1.6 GB locally). At 1 instance a renewal needs a second application slot,
  so `check` warns that `ApplyRenewal` and `RenewPermit` never run. The sample also joins a
  disposition's decision to the permit's ledger by an event. Its temporal property, that a
  valid permit without a 食品衛生責任者 ends with one appointed, is checked on its cone. See it:
  [Known verification gaps](../samples/food-permit-ops/README.md#known-verification-gaps). What
  you can do: a change that grows the permit's cone further takes the view past the cap;
  measure it with `lifecycle_measure` first.
- **insurance-claim-ops checks its settlement guard reduced.** A liability claim is completed
  with the 示談書, and the claim and the settlement are both children of the accident case. The
  guard on `CompleteClaim` reads the settlement of the claim's own case
  (`holds(..).along(Claim, ClaimCase, Settlement)`), and the rule `賠償の請求は示談書がそろってから完了する`
  says the same on the claim's rows. Both join the claim's cone to the case's. Without the
  reduction of #74 the views of `Claim`, `Payment`, and `Subrogation` passed 3,000,000 states at
  2 instances and TLC had not finished after 20 minutes. Reduced, the claim's cone is 836,427
  states, and `verify --deny-undecided` decides every check in about 180 s, with the one
  deadlock the sample's header names. The claim's cones and the case's view are in
  `VERIFY_FALLBACK`: `engine_equivalence` compares them with TLC at 1 instance only.
  See it: [Verification](../samples/insurance-claim-ops/README.md#verification). What you can
  do: a change that grows the case's side (255,292 states in 2,428 classes with the claim's
  events) grows every claim cone with it; measure it with `lifecycle_measure` first.
- **`--buc` leaves findings undecided.** `--buc` builds the system without the use cases
  outside the selected BUCs, and those could reach, enable, or unblock a state. A missing
  witness, a use case that never runs, a deadlock, an arm never taken, and a value never
  chosen are therefore listed under `not checked:` and never reported as an error or a
  warning. See it: the `not checked:` lines of a `--buc` run. What you can do: run without
  `--buc`, or use `--entity`, whose cones are exact and whose findings are decided. To ask
  whether one actor can finish on their own, use `--actor`, `--without-buc`, or
  `--without-usecase`: they assume the other use cases never run and decide the findings
  under that assumption ([Partial runs](./cli-reference.md#partial-runs)).

## Workflow

1. Model lifecycle with `transitions` / `sets` and entity-local rules.
2. Run `rdra-ish states` and `rdra-ish check` for quick feedback.
3. `rdra-ish export samples/formal-verification/order.rdra --kind tla -o /tmp/rdra-tla` and inspect the Spec and `.cfg`.
4. `rdra-ish verify samples/formal-verification/order.rdra` to check every rule and property.
5. For a second opinion, install [TLA+ tools](https://github.com/tlaplus/tlaplus) so `tlc` is
   on `PATH` (`nix develop` provides it) and run
   `rdra-ish verify samples/formal-verification/order.rdra --with-tlc -o /tmp/rdra-tla`.

Agent skill: `skills/rdra-ish-verify/` (routing + references + bundled samples).
Install: see [`skills/README.md`](../skills/README.md) (`npx skills add` / `gh skill install`).

## Samples

Canonical files: `skills/rdra-ish-verify/samples/` (each file is **standalone** —
do not `check` the whole directory; ids such as `Order` collide). In this repo,
`samples/formal-verification/` and `samples/formal-verification-fail/` are
symlinks to those skill-bundled files (CLI tests keep working after skill install).

| Sample | `check` and `verify`'s cones | `--prove` | Focus |
|---|---|---|---|
| `order.rdra` | pass, pass | 2 of 2 | Lifecycle invariants + `after.assert` + `leads_to` / `eventually` + `WF_vars` |
| `int_stock.rdra` | pass, pass | 1 of 1 | Int columns, arithmetic `forbidden`, Int temporal property |
| `now_coupon.rdra` | fail, fail | 0 of 1 | `now` and `Tick`; the invariant finds `expired_at < now` |
| `cross_order_payment.rdra` | pass, pass | 1 of 1 | Multi-instance + `.along` (Pay is one atomic action, so Cancel cannot interleave; Cancel deletes the pending Payment) |
| `cross_order_payment_split.rdra` | fail, fail | 0 of 1 | The same model with Pay through two APIs, the capture first: two transaction groups, so Cancel interleaves between `Pay_1` and `Pay_2`; Cancel does not delete the Payment here |
| `split_run_order.rdra` | pass, pass | 2 of 2 | OpenSettlement through a claim API and a settlement API: one run in `invokes` order, so the settlement is created once under the claim the first step moved (`Unique_Settlement_Claim`), and a started run finishes under `.fair(OpenSettlement, ..)` |
| `quantifier_none.rdra` | fail, fail | 0 of 1 | `invariant(...).when(...).none(...)` (Revoke while Assign stays active) |
| `quantifier_guard.rdra` | pass, pass | 1 of 1 | `holds(..).none(..)` on Revoke makes the `.none(...)` rule of `quantifier_none` hold |
| `untriggered_sets.rdra` | fail, fail | 0 of 1 | `sets` from a use case that drives no transition (Flag then Ship reaches the forbidden state) |
| `event_order.rdra` | pass, pass | no state rule | `triggers(EvOrderExecuted, IssueCert)` runs IssueCert only after ExecuteCertIssue raised the event on the order (#30, #112), so the `after(IssueCert).assert` on the order's status holds with no guard |
| `after_assert_unmet.rdra` | fail, fail | no state rule | `after(DeliverOrder).assert` without the `sets` that makes it true |
| `temporal_fair.rdra` | pass, fail | no state rule | `leads_to(granted, registered or dismissed)` fails without fairness: another application is ordered to amend and amends forever, and `WF_vars(Next)` holds on that loop. The same property with `.fair(Register, Dismiss)` holds |
| `order_stability.rdra` | pass, fail | no state rule | `stable`, `unchanged(..).when(..)`, and `precedes` on an Order, decided by BFS on each cone. A shipped order stays shipped, an approved amount stays, and an order ships only after it is paid. Reopen, Refund, and a Ship that reads no invoice break `CancelledStaysCancelled`, `AmountKeptOnceDecided`, and `ShippedAfterInvoiced`, each at the shortest trace to the step or state (#166) |
| `temporal_along.rdra` | pass, fail | no state rule | `always(not captured or paid)` over a Payment and an Order holds with `.along(Payment, Order)`: Pay captures a payment and pays its own order. The same formula without `.along` fails on a captured payment beside another order that is never paid (#165) |
| `temporal_unmet.rdra` | pass, fail | no state rule | `always(Order.status != shipped)` fails once an Order ships, and `leads_to(pending, delivered)` fails because ReopenOrder lets an Order be cancelled and reopened forever, so its lasso loops back. `check` does not model-check properties; each prints its lasso in model terms |
| `one_to_one_shared.rdra` | pass, fail | 0 of 1 | `relate(.., 1:1)` adds `Unique_Shipment_Order`; Ship and Reship each link a Shipment to the same Order, which takes two instances |
| `unique_double_booking.rdra` | pass, fail | 0 of 1 | `@unique(room_id, slot)` adds `Unique_Booking_room_id_slot`, which reads the owner Room beside `slot`; BookAm books any room for the morning, so two Bookings take one room at `am`, which takes two instances |
| `unique_booking_guarded.rdra` | pass, pass | 1 of 1 | The same model with a guard on each booking use case: `.none(Booking.slot == am)` on the Room it binds keeps the slot free |
| `none_any_child.rdra` | pass, pass | 1 of 1 | `.none(Invoice)` on Issue says "no invoice yet" and reads the link only, so `Unique_Invoice_Order` holds on a cone without `Invoice.status` (25 states). The enumerating form `.none(Invoice.status in (<every value>))` builds the same cones |
| `none_any_child_bypassed.rdra` | pass, fail | 0 of 1 | The same model with Reissue, which creates an Invoice under a closed Order without the guard; `Unique_Invoice_Order` fails at two instances on the cone without `Invoice.status` |
| `min_card_unmet.rdra` | fail, fail | 0 of 1 | `relate(OrderLine, Order, 1..*:1)` adds `MinCard_OrderLine_Order`; Place creates an Order without a line |
| `min_card_met.rdra` | pass, pass | 1 of 1 | The same model with Place creating the Order and its first line in one transaction. At one instance `check` warns that AddLine never runs: an Order exists only with the one OrderLine slot taken. At two instances it runs |
| `check_unmet.rdra` | fail, fail | 0 of 1 | `qty: Int @default(1) @check(qty >= 1)` adds `Check_CartLine_qty_0`; ClearQty sets `qty` to 0 |
| `check_met.rdra` | pass, pass | 2 of 2 | The same check with a use case that keeps `qty` in range; `forbidden` reads the derived `bulk: Bool @derived(qty >= 3)` as its expression |
| `unwitnessed_state.rdra` | fail, pass | 1 of 1 | `refunded` has no witness: Refund needs `shipped` and a refund window that holds only while `pending`. TLC passes; the BFS witness check fails `check` and `verify`, a shipped Order is a deadlock (Refund is its only way out), and both warn that Refund never runs (its guards never hold together). TLC alone finds both errors with the exported `StateModel_Order` specs |
| `deadlock.rdra` | fail, pass | no state rule | An Order stays `paid` forever: Ship leaves `paid` only while a packing condition holds, and it holds only while `pending`. Express ships a pending Order, so `shipped` has a witness. TLC passes; the BFS deadlock check fails `check` and `verify`, and both warn that Ship never runs. TLC alone finds the deadlock with the exported `StateModel_Order` specs |
| `can_reach_unmet.rdra` | pass, pass | no state rule | `can_reach(Order.status in (delivered, cancelled))` fails: once LoseStock and CloseWindow have run, an Order moves between `pending` and `on_hold` forever. Each of those states has a way out, so no deadlock is reported. `check` evaluates no property and TLC checks nothing; `verify`'s BFS fails it with a shortest trace and the states the Order is confined to |
| `can_reach_met.rdra` | pass, pass | no state rule | The same model with Restock, so every reachable state can still finish. An Order may still be held and resumed forever: `can_reach` asks for a path, not for progress |
| `partial_scope.rdra` | fail, not run | not run | Audit's `forbidden` reads the arithmetic `@derived` column `score`: a `DerivedColumnRead` build error stops `check` and `verify` before TLC. With `--entity Order` both defer it under `not checked:` and pass; TLC checks `Forbidden_Order_0` |
| `fail/order.rdra` | fail, fail | 0 of 1 | Intentionally unsafe (reachable invariant violation) |

The native checker and TLC give the same verdict on every row at the same bounds.
`check` runs at one
instance per entity, where `Unique_*` holds vacuously. The `--prove` column counts the state
rules `verify --prove` proves for any number of rows. A rule that fails `check` or TLC is
never proved, and `one_to_one_shared`, `none_any_child_bypassed`, and
`unique_double_booking`, which pass `check`, are unproved too.

### Sample expectations

Each sample root in `samples.txt` states what `check` and `verify` must report in
`// expect` comment lines. `scripts/verify-samples.sh <rdra-ish binary>` runs both commands
on every root, with no `tlc` or `tlc2` on `PATH` and no `RDRA_TLC_JAR`, and fails when a run
differs from its header. `scripts/verify-samples.sh --with-tlc <rdra-ish binary>` runs every
`verify` again with `--with-tlc`: each run must match the same header, print
`tlc_result:`, and report no engine disagreement. CI runs both on the release binary in the
`formal-verify` job.

The script reads the header from the root's first `.rdra` file in load order, which
`rdra-ish files <root>` prints first. An `// expect` line in any other file of the root
fails the script. The grammar is:

```
header       = "// expect " command [" " flags] ": " expectation
command      = "check" | "verify"
flags        = CLI words passed to the command as written, e.g. "--entity Order"
check exp    = "ok" | "fail" " " Name { " " Name }
verify exp   = "ok" | "fail" " " Name { " " Name }
             | "tlc=" ("ok" | "counterexample") [" fail" " " Name { " " Name }]
                 (only when flags hold "--with-tlc")
Name         = [A-Za-z0-9_]+
```

- `fail` means exit status 1, and its absence exit status 0. Any other status, such as a
  `tool_error` exit, is a mismatch.
- `tlc=` names TLC's verdict, so it may appear only on a line whose flags hold
  `--with-tlc`, and it must equal the `tlc_result:` line. `tlc=counterexample` also needs
  TLC's own `violated: Error: ... violated.` line. A line without `tlc=` says nothing of
  TLC, and the `--with-tlc` mode of the script only asks that a run that prints `result:`
  prints `tlc_result:` too. Without `--with-tlc` the script skips a line whose flags hold
  `--with-tlc`, since TLC is not on `PATH` there.
- `fail` needs at least one name. The names must equal the set of names of the run's
  errors, not just be part of it. Every lifecycle error prints as `error[<Name>]: ...`, and
  the script reads the name from there and, when TLC ran, from TLC's `violated:` line. A
  name is the
  invariant or `after` property name (`MultiForbidden_Order_0`,
  `AfterAssert_DeliverOrder_0`), the temporal property's name (`NeverShipped`; TLC's
  temporal line names none, so it comes from `error[<Name>]:` only),
  `Witness_<Entity>_<column>_<value>` for a state with no
  witness, `Deadlock_<Entity>_<column>_<value>` for a deadlock error, or the build error's
  name, such as `DerivedColumnRead_Audit_score` (the full list is in
  [state-derivation.md](state-derivation.md#diagnostics)).
- An `error: ...` line without a name, such as a load error, is a mismatch whatever the
  header says.
- Each `(command, flags)` pair runs once, and a pair may appear on one line only. A root
  with no `check` line expects `check: ok`, and one with no `verify` line expects
  `verify: ok`. A root whose `verify` lines all carry flags runs only those: use this
  for a model whose default bounds do not finish, and name the bounds it is checked at
  (`samples/oidc-auth` runs `--instance-count 1 --prove` and a scoped run at two
  instances).

```rdra
// unwitnessed_state.rdra: every rule holds, and the BFS state-model findings fail the run.
// expect check: fail Witness_Order_status_refunded Deadlock_Order_status_shipped
// expect verify: fail Witness_Order_status_refunded Deadlock_Order_status_shipped

// partial_scope.rdra: the build error stops both commands before any cone; --entity Order passes.
// expect check: fail DerivedColumnRead_Audit_score
// expect verify: fail DerivedColumnRead_Audit_score
// expect verify --entity Order: ok

// A line whose flags hold --with-tlc may name TLC's verdict too.
// expect verify --with-tlc: tlc=ok fail Witness_Order_status_refunded Deadlock_Order_status_shipped
```

The script prints one line per run with the verdict, the wall time, and the maximum RSS
(GNU `time` on Linux, BSD `/usr/bin/time` on macOS), then the count of roots and runs.
The script makes 68 runs of the 33 roots. Locally (Apple silicon) clinic-ops `verify` took
105 s at about 2.0 GB (50 s at 1.4 GB before the run marks of `CheckInPatient`, #80) and
patent-ops `verify` 68 s at about 1.2 GB, their TLC JVMs included, and food-permit-ops
`verify` 16 s, when last measured there. insurance-claim-ops `verify` takes about 180 s at
about 2.1 GB, as the gaps above say. The CI step has 20 minutes.
