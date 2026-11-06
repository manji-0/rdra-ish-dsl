# Formal Verification (TLA+ / TLC)

<!-- derived-from ./state-derivation.md -->
<!-- derived-from ./language-reference.md#entity-state-constraints -->
<!-- derived-from ./language-reference.md#temporal-path-properties -->
<!-- derived-from ./cli-reference.md#export -->
<!-- derived-from ./cli-reference.md#verify -->

RDRA-ish prints its lifecycle transition system as **TLA+** and optionally checks it
with **TLC**. `rdra-ish states` and `check` explore the same system by breadth-first
search (BFS) at one instance per entity. `verify` runs TLC on it at two instances by
default and adds the temporal properties that BFS does not evaluate. See
[state-derivation.md](./state-derivation.md) for the system itself.

## Commands

```
rdra-ish export <INPUTS...> --kind tla [-o <OUT>] [--instance-count N] [--int-range-max N]
rdra-ish verify <INPUTS...> --backend tlc [-o <OUT_DIR>] [--instance-count N] [--int-range-max N]
```

| Command | Role |
|---|---|
| `export --kind tla` | Write `RdraSpec.tla` **and** sibling `RdraSpec.cfg` for the whole model, and the `StateModel_<E>` specs that check the deadlocks and witnesses |
| `verify --backend tlc` | Print one spec per cone of influence and one per temporal property, then run `tlc` / `tlc2` from `PATH` |
| `states`, `check` | BFS on the same system, with no TLC dependency |

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
pair for each property its cone leaves undecided, which TLC runs (see
[Temporal properties on a cone](#temporal-properties-on-a-cone)).
Without `-o`, `verify` writes them to a temp dir. It deletes the dir when every check
passes, and keeps it and prints `specs: <dir>` when one fails. The TLC heap settings
(`RDRA_TLC_HEAP`, `JAVA_TOOL_OPTIONS`) are in [cli-reference.md](./cli-reference.md#verify).

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
| Enum, Bool, `@null`, Int, Money, Decimal, Date, or DateTime column the system reads or writes; a variation-typed column that something names | `<E>_<col>`, a function over `<E>_Ids`. Int-like columns range over `IntRange` (nullable ones add `"null"`). |
| `relate` N:1, 1:N, 1:1 | `<Child>_owner_<Parent>`, a function from child ids to parent ids. An `.optional()` relate adds `0` for an unlinked child. |
| Use case | One operator `<UC>(p0, ...)` per transaction group of `UsecaseTx::groups()` (`<UC>_1`, `<UC>_2`, ... when the writes form several groups). `Next` is the disjunction of `\E` over the parameters. |
| A use case split into k groups | A run mark `<E>_run_<UC>`, a function from the ids of each entity a step before the last writes to `{"none", "1", .., "k-1"}`. `<UC>_1` takes a row at `"none"` and sets `"1"`; `<UC>_i` binds a parameter per earlier row with `<E>_run_<UC>[p] = "i-1"` in place of a presence test, and sets `"i"`, or `"none"` at the last step (see [Runs of a split use case](./state-derivation.md#runs)). |
| A tracked event `Ev` with mark entity `E` | A raised mark `<E>_raised_<Ev>`, a function from `<E>_Ids` to `BOOLEAN`, `FALSE` in `Init` and never `FALSE` again (see [Triggers](./state-derivation.md#triggers)). The last step of a raiser, on each parameter `p` it has on `E`, assigns `<E>_raised_<Ev>' = [<E>_raised_<Ev> EXCEPT ![p] = TRUE]`; the first step of a triggered use case conjoins `<E>_raised_<Ev>[p] = TRUE` on each parameter that is the raiser's row. |
| A `Triggered` parameter | One more `\E p \in <E>_Ids` on the triggered operator with `<E>_exists[p] = "present"` and the mark conjunct, and, when the one direct key joins it to one of the operator's rows, `<Child>_owner_<Parent>[c] = p` (or `= c`). A cone that leaves the mark out prints the operator without the conjunct and without the parameter, and the var is not in the module. |
| A use case several tracked events trigger | One operator per variant, `<UC>_by_<Ev>`, each in `Next`; `AfterAssert_<UC>_<n>` ranges over every variant's last step. |
| `creates` | `FirstAbsent(<E>_exists, p)` and `<E>_exists' = ... "present"`. A deleted slot is never reused. |
| `deletes` | `<E>_exists'` becomes `"deleted"`, columns and owners return to the base value. |
| `sets`, `transitions(UC, E.col, a -> b)` | Primed assignments. A transition guards `col = a`. `UNCHANGED` lists every variable the action does not assign. |
| Variation column of a created instance | `\E v0 \in {values} : col' = ...`, so TLC explores every value. `@default` does not pin it. |
| `applies_to(Condition, UseCase \| Api)` with a `holds` body | A guard conjunct on the use case's operator. A condition on a created instance is evaluated on the primed variable, after the assignments and the `UNCHANGED` that bind every primed variable it reads. |
| Comparison proposition driven by `drives(UC, E.col op rhs, bool)` | A Boolean variable. An undriven comparison is TLC arithmetic on the Int columns. |
| `col in (a, b)` / `col not in (a, b)` | `E_col[b0] \in {"a", "b"}` / `E_col[b0] \notin {"a", "b"}`, the same disjunction of equalities that `states` evaluates. A nullable Int value `present` keeps the `\/` form. |
| `col < now` | A global `now \in IntRange` and a `Tick` action that adds one while `now < max`. |
| Entity-local and multi-entity `forbidden`, `invariant`, `required`, `exclusive` | One `INVARIANT` each, named `<Kind>_<Entity>_<n>` (`Forbidden_Order_0`, `Inv_Order_0`, `MultiForbidden_Order_0`). Each ranges over the present instances. |
| `.along(...)` | Quantified over `<E>_Ids`, joined by `<Child>_owner_<Parent>` equalities. |
| `col: T @check(expr)` | `INVARIANT Check_<Entity>_<column>_<n>` over the present instances. A nullable operand is guarded: `qty: Int @null @check(qty >= 1)` prints `(E_qty[b0] = -1) \/ (E_qty[b0] # -1 /\ E_qty[b0] >= 1)`. |
| `relate(Child, Parent, 1:1)` | `INVARIANT Unique_<Child>_<role>` (role defaults to the parent id): `\A b0, b1 \in <Child>_Ids`, two present children are the same or have different owners. An unlinked optional child owns nothing. |
| `relate(Child, Parent, 1..*:1)` (child-side minimum) | `INVARIANT MinCard_<Child>_<role>`: `\A b0 \in <Parent>_Ids`, a present parent has `\E b1 \in <Child>_Ids` present with `<Child>_owner_<Parent>[b1] = b0`. Checked in every state, so a parent created without a child in the same transaction, or a deleted last child, is a counterexample. |
| Pre-existing child of a `relate` | `Init` links the owner: `\in [<Child>_Ids -> <Parent>_Ids]` for N:1 (start `Any`), `[i \in <Child>_Ids \|-> i]` for 1:1 and for a child-side minimum (start `Identity`). An `.optional()` key under a created parent starts unlinked (`Base`). |
| An action that creates a parent and updates an existing child | The action links the child. An `.optional()` key guards the assignment with an unlinked owner, so a linked child keeps its first parent. A mandatory key assigns without a guard, which moves the child. |
| `invariant(...).when(...).none/has(...)` | `\A` / `\E` over the ids, named `Quantifier_<Entity>_<n>`. A `relate` joins the sides by owner; without one the related side is independent. |
| `after(UC).assert(...)` | `PROPERTY AfterAssert_<UC>_<n>`: `[][(\A p : UC(p) => post')]_vars`. It checks every step of the use case's last action, so a split use case once per run, on the run's rows. |
| `property` with `always`, `eventually`, `leads_to` | A named formula and a `PROPERTY`, quantified per present instance |
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

forbidden(Order, Payment, Order.status == cancelled, Payment.status == captured)
  .along(Order, Payment)
```

Operators:

- `always(expr)` → `[]expr`
- `eventually(expr)` → `<>expr`
- `leads_to(p, q)` → `p ~> q`
- Connectives inside expressions: prefer `and` / `or` / `not` (`/\` `\/` `~` aliases)
- Set membership in an atom: `Order.status in (a, b)` → `\in {"a", "b"}`, `not in` → `\notin`
- Property label string is optional (`property StockOk always(...)`)

`rdra-ish states` does not evaluate path properties, and `check` prints a `note:` that it
did not model-check them. Use `export` or `verify`.

## What `verify` checks

`verify` restricts the system to the cone of influence of each invariant and
`after` assertion, prints that cone, and runs TLC on each cone in parallel. A cone
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
TLC reports a temporal violation as `Temporal properties were violated.` without naming the
property, so each property runs alone: module `Cone_<Property>` with a `.cfg` that names
that one `PROPERTY`. The property cones run with the other cones, in parallel.

A property with `.fair(UC, ...)` (see
[language-reference.md](./language-reference.md#fairness-fairusecase-)) carries its
assumption in its own formula, `Fair_P => (...)`, and its cone keeps the listed use cases
whole. `verify` prints `<P> assumes weak fairness per instance on ...` under the `temporal
properties:` line, since an `ok` for `P` holds only on the runs that do not starve them.
`check` and `states` run BFS, which evaluates no temporal property, with or without `.fair`.

### Temporal properties on a cone

<!-- derived-from ./state-derivation.md#exploration-by-cone-of-influence -->
<!-- constrained-by ./language-reference.md#fairness-fairusecase- -->
A property's cone (`TransitionSystem::property_cone`) is the smallest closed var set that
holds what the formula reads and, for each action of each use case its `.fair` lists,
everything the action reads and writes: its guards with the `applies_to` conditions and the
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
cone, and the whole model only when neither decides:

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
   run that did not finish on food-permit-ops. It prints TLC's progress (distinct states,
   queue, seconds, about once a minute) and stops at `--temporal-budget` seconds (default
   300) or at the first progress line past 2,097,152 distinct states. A run stopped
   after TLC printed its verdict keeps that verdict; otherwise the property is `not
   decided`, listed under `not checked:`, and the run still ends in `OK:`
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
sub-lifecycle (the next section) adds `Cone_P_Full: counterexample` after a counterexample on
the reduced cone, before the `_WF` note. A property without `.fair` rests on
`WF_vars(Next)` alone, so it reaches the whole model unless its cone holds every written
var; name the use cases it relies on to keep it on the cone. On food-permit-ops
`HygieneManagerEventuallyAppointed` runs on a 243-state cone and the whole `verify` takes
18 s; on patent-ops `GrantEnds` runs on a 4,105-state cone; on insurance-claim-ops
`ClaimEventuallyDecided` runs on a 2,975-state cone and `SubrogationEnds` on a 4,783-state one. The test
`a_property_cone_decides_what_the_whole_model_decides` in `engine_equivalence` compares the
cone verdict with the whole-model verdict on the temporal samples and on fixtures for each
case: another entity looping outside the cone, a fair use case whose guard reads another
entity, a guard that toggles forever, a cone that frees slots, a cone that holds every
written var, a fair use case copying from its parent, a copy from an optional parent the
cone leaves out, a `.none` guard on a fair use case whose quantified column another use
case writes, and a condition on a creating use case that reads a column the creation does
not assign.

Before TLC runs, `verify` explores each entity's cone with BFS at the same bounds, capped at
8,388,608 states per cone (`Caps::VERIFY`, named in the `bounds:` line). Each state of a
state model must have a witness there (see
[state-derivation.md](./state-derivation.md#state-models-and-witnesses)). A `NoWitness`
error fails `verify` even when every TLC check passes: the run prints `tlc_result: ok` and
no `OK:` line, and exits 1. A deadlock, an instance that no path moves off a state with a
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
passes it runs unreduced. On clinic-ops the whole `verify` takes about 17 s and 0.5 GB (it
took 2.5 minutes and 3.7 GB before the reduction); its largest cone is `EligibilityCheck`
(905,528 states at 2 instances, which nothing reduces), while `Charge`, past the cap
unreduced, is 38,533 states reduced and `Prescription` (4,266,635 unreduced) is 1,690. On
patent-ops the `OfficeAction` cone (over 8,388,608 states unreduced, 1,341,707 reduced) is under
the cap too, so `verify` decides all four of its stuck-state checks (see the sample's README).

```
$ rdra-ish verify samples/formal-verification/order.rdra --backend tlc
bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); 8388608 states per BFS cone; entities a check does not read are unbounded
cones: 1 for 2 invariant(s) and 1 after-assert(s)
temporal properties: 2, each checked on its cone, and on the whole model only when the cone does not decide it (budget 300 s)
state models (BFS on 1 entity cone): 1 state model, 5 states witnessed, 0 deadlocks, 0 use cases that never run
Cone_Forbidden_Order_0 [Forbidden_Order_0, Inv_Order_0, AfterAssert_DeliverOrder_0]: ok (31 states)
Cone_PaidLeadsToShipped [PaidLeadsToShipped]: not decided (31 states, Cone_PaidLeadsToShipped_WF: ok, needs the whole model's WF_vars(Next))
Cone_EventuallyTerminal [EventuallyTerminal]: not decided (31 states, Cone_EventuallyTerminal_WF: ok, needs the whole model's WF_vars(Next))
RdraSpec_PaidLeadsToShipped [PaidLeadsToShipped]: ok (31 states)
RdraSpec_EventuallyTerminal [EventuallyTerminal]: ok (31 states)
tlc_result: ok
OK: no errors within 2 instances per entity, Int 0..5
```

Both properties of `order.rdra` have no `.fair`, and `DeliverOrder` writes `delivered_at`,
which neither reads, so their cones do not decide them and the whole model (31 states) does.
With `.fair(ShipOrder)` on `PaidLeadsToShipped`, its first cone run would decide it.

A violated invariant or `after` assertion prints in model terms first, the same way
`check` prints it: the rule's `path:line:col`, one line per step with the use case, its
bound instances (`Place [Order#1, Payment#1]`), and its site, and under each step the
values it changed. `verify` gets that trace by decoding TLC's states and replaying them
through the step relation BFS uses, and builds the message with the code `check` uses.
TLC's own `violated: Error: ... is violated.` line and its trace follow, with one
`name[slot] = value` line per variable and slot. The name is the constraint's name, so it
points at the rule that broke. A step that no move of the cone explains is a tool bug, and
`verify` says the engines disagree instead of printing the model-term block. The full
output is in [cli-reference.md](./cli-reference.md#verify).

A violated temporal property prints the same way, under `error[<Property>]`. TLC's
counterexample is a lasso: a finite trace, then either `State N: Stuttering` (the last
state repeats forever) or `Back to state K: <action>` (one more step returns to state K
and the behavior repeats from there). `verify` replays the trace and the step back
through the BFS step relation and prints the step back as the last numbered step,
followed by `then loops back to step K`: the behavior repeats steps K through the last one
forever. K is TLC's state number, since TLC's state K is the state step K starts from. A
stutter ends with `then nothing changes forever`.

The message head names the property, the instance it fails for, and the step that shows
it: where an `always` condition is false, or where an `eventually` instance exists or a
`leads_to` trigger holds and the goal never holds from then on. `verify` evaluates the
property on the infinite behavior the way the module spells it, per instance of each
binder: `always x` is `[](present => x)`, `eventually x` is `[](present => <>x)`, and
`leads_to x y` is `(present /\ x) ~> y`. A lasso that does not replay, or that does not
violate the property, is a tool bug and prints the engine-disagreement line. TLC picks the
lasso, so two runs may print different steps. In this run of `temporal_unmet.rdra`, TLC
ends with `Back to state 7: CancelOrder`, and Order#1 is cancelled and reopened forever:

```
error[PendingLeadsToDelivered]: property PendingLeadsToDelivered is violated: its leads_to trigger holds for Order#1 after step 1, and its goal never holds from then on
  rule: skills/rdra-ish-verify/samples/temporal_unmet.rdra:48:1
  counterexample (8 steps, from TLC):
    1. PlaceOrder [Order#1]                              skills/rdra-ish-verify/samples/temporal_unmet.rdra:15:1
         + Order#1 (status=pending, delivered_at=null)
    2. PlaceOrder [Order#2]                              skills/rdra-ish-verify/samples/temporal_unmet.rdra:15:1
         + Order#2 (status=pending, delivered_at=null)
    3. CancelOrder [Order#1]                             skills/rdra-ish-verify/samples/temporal_unmet.rdra:19:1
         Order#1.status: pending -> cancelled            skills/rdra-ish-verify/samples/temporal_unmet.rdra:40:1
    4. CapturePay [Order#2]                              skills/rdra-ish-verify/samples/temporal_unmet.rdra:16:1
         Order#2.status: pending -> paid                 skills/rdra-ish-verify/samples/temporal_unmet.rdra:37:1
    5. ShipOrder [Order#2]                               skills/rdra-ish-verify/samples/temporal_unmet.rdra:17:1
         Order#2.status: paid -> shipped                 skills/rdra-ish-verify/samples/temporal_unmet.rdra:38:1
    6. DeliverOrder [Order#2]                            skills/rdra-ish-verify/samples/temporal_unmet.rdra:18:1
         Order#2.status: shipped -> delivered            skills/rdra-ish-verify/samples/temporal_unmet.rdra:39:1
         Order#2.delivered_at: null -> present           skills/rdra-ish-verify/samples/temporal_unmet.rdra:43:1
    7. ReopenOrder [Order#1]                             skills/rdra-ish-verify/samples/temporal_unmet.rdra:20:1
         Order#1.status: cancelled -> pending            skills/rdra-ish-verify/samples/temporal_unmet.rdra:41:1
    8. CancelOrder [Order#1]                             skills/rdra-ish-verify/samples/temporal_unmet.rdra:19:1
         Order#1.status: pending -> cancelled            skills/rdra-ish-verify/samples/temporal_unmet.rdra:40:1
    then loops back to step 7
violated: Error: Temporal properties were violated.
```

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
`WF_vars(Next)` while R stands still, so the `_WF` run stays on the unreduced cone, and a
counterexample of the `Cone_<P>` run on R is repeated on the unreduced cone as `Cone_<P>_Full`,
whose lasso the printed trace reads (`verify` reports that run's outcome beside the first's; an
`ok` there is a tool bug, reported as such).

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
bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); 8388608 states per BFS cone; entities a check does not read are unbounded
scope: --entity Order
cones: 1 for 1 invariant(s) and 0 after-assert(s)
temporal properties: none, no whole-model run
not checked: 1 entity view (Audit), 1 invariant (Forbidden_Audit_1), 0 after-asserts; 1 build error deferred; 1 finding not decided
  deferred error[DerivedColumnRead_Audit_score]: Audit: Audit.score is derived by arithmetic, which the lifecycle cannot evaluate; compare the columns it reads instead
  whether Record can run is not decided: no cone of a selected entity keeps it
state models (BFS on 1 entity cone): 1 state model, 2 states witnessed, 0 deadlocks, 0 use cases that never run
Cone_Forbidden_Order_0 [Forbidden_Order_0]: ok (7 states)
tlc_result: ok
OK: no errors in the selected scope, within 2 instances per entity, Int 0..5
```

The rules for each flag and for deferral are in
[cli-reference.md](./cli-reference.md#partial-runs).

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
  with `--int-range-max`). A literal outside that window is
  `IntLiteralOutOfBounds` in `check` and `states` too: raise `--int-range-max` or lower
  the literal. `@default(0)` is accepted for Int columns.
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
  grows past the cap fails it until it is listed. Today 10 cones in `samples/clinic-ops`,
  `samples/patent-ops`, `samples/food-permit-ops` and `samples/insurance-claim-ops` are
  listed. The table gives each listed cone and each cone the cap of 300,000 took off the
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
  `crates/rdra-ish-cli/tests/engine_equivalence.rs` lists them, 11 cones today (19 before the
  reduction of #74, 16 after the patent-ops filing guard, 10 since the cap went from 200,000
  to 300,000, and 11 since the event order of #112). The table under
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

  `verify` still checks these cones at 2 instances: TLC runs on each, and BFS runs on each
  entity cone. Only the agreement of the two engines is tested at 1 instance. This is
  accepted for 1.0. See it: the list in the test, which fails when a cone joins or leaves
  it. What you can do: treat a `verify` verdict on these cones as TLC's alone, and keep a
  new model's cones under 300,000 states at 2 instances if you want the agreement tested.
- **insurance-claim-ops cannot check that a payment deadline follows its reason.** The
  deadline is writable, `sets(UC, E.due_on == N).when(Reason)` with a timer use case that
  waits for `due_on <= now` (#98), but the cone it adds does not decide at the cap: with a
  `PaymentExtension` entity the Claim cone passes 20,000,000 states and `verify` passes 6 GB;
  with the reason and `due_on` on `Claim`, the Claim view passes 20,000,000 states with one
  reason (126 s) and is unfinished after 7 minutes and 5.8 GB with five. Without the deadline
  the Claim cone is 836,427 states reduced. See it:
  [Known verification gaps](../samples/insurance-claim-ops/README.md#known-verification-gaps).
  What you can do: keep the reason out of the checked rules, as the sample does.
- **The `verify` BFS stops at 8,388,608 states per cone.** The BFS pass that finds missing
  witnesses, deadlocks, use cases that never run, arms never taken, and values never chosen
  stops at `Caps::VERIFY`, 2^23 states. A cone that stops there decides none of these
  findings. TLC still checks its invariants. The cap counts the states of the cone reduced
  by its independent sub-lifecycle (see
  [Reduction by an independent sub-lifecycle](#reduction-by-an-independent-sub-lifecycle)).
  Every sample cone is under the cap at 2 instances (the largest, `BusinessPermit` of
  food-permit-ops, has 4,648,771 states reduced; `Charge` of clinic-ops, over 8,388,608
  unreduced, has 38,533), so `verify --deny-undecided` decides every check on all of them;
  the price is insurance-claim-ops's 180 s and 2.1 GB. See it: each undecided finding under
  `not checked:` on stdout and the `warning:` on stderr that counts them (`--deny-undecided` fails the run instead). What
  you can do: rely on `check` at 1 instance for those findings, or shrink the cone, for
  example by removing a guard that pulls another entity's columns in. The whole-model
  temporal run below keeps its own cap of 2,097,152 distinct states.
- **`check` stops at 65,536 states per cone.** A cone past the cap is the
  `StateCapReached_<E>` error, so `check` fails rather than passing on part of the cone.
  See it: the error, whose message names the variables the reached states vary in. What
  you can do: raise the cap with `--max-states`, or narrow the cone by moving a guard that
  reads another entity.
- **Int columns range over `0..5`, and `now` is a bounded clock.** Every Int, Money,
  Decimal, Date, and DateTime column takes values in `IntRange`. `now` starts in that range
  and a `Tick` action raises it to the top, so a run never sees a later time. See it: the
  `Int 0..5` in the `bounds:` line, and `IntLiteralOutOfBounds` for a literal outside it.
  What you can do: `--int-range-max N` on `check`, `states`, `export`, or `verify` widens
  the range.
- **Instances are bounded: by default 1 per entity in `check` and `states`, 2 in `verify`.** A rule that needs two
  instances to break, such as `Unique_*`, holds vacuously in `check`. A rule that needs
  three is not checked by default. Each entity is created at most `InstanceCount` times,
  because a deleted slot is never reused. See it: the `bounds:` line. What you can do: run
  `verify`, or pass `--instance-count N` to `check`, `states`, or `verify` for a rule about
  more instances.
- **Temporal properties are checked only by `verify`, and a scoped run checks none.**
  `check` and `states` evaluate no `property` and print a `note:` that says so. A run with
  `--buc` or `--entity` checks no property: `--buc` drops use cases a property's fairness
  reads, and `--entity` may defer an error inside a property's cone. See it: the `note:`
  from `check`, and `temporal properties: 2, not checked: a scoped run checks none` in a
  scoped `verify`. What you can do: run an unscoped `verify`.
- **A property its cone cannot decide runs on the whole model under a budget.** A property
  whose first cone run finds a counterexample and whose `_WF` run finds none needs the whole
  model's `WF_vars(Next)` (see [Temporal properties on a cone](#temporal-properties-on-a-cone));
  so does a property whose cone frees an entity's slots and finds a counterexample. The
  whole-model run stops at `--temporal-budget` seconds (300 by default) or at 2,097,152
  distinct states, and the property is then `not decided`, which warns and does not fail
  the run unless `--deny-undecided` is given. The `not checked:` line names the cause. At
  the state cap it says that `--temporal-budget` does not lift the cap; at the time budget it
  offers the budget. Both suggest `.fair(UseCase, ...)` naming the use cases the property
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
  `--buc`, or use `--entity`, whose cones are exact and whose findings are decided.

## Workflow

1. Model lifecycle with `transitions` / `sets` and entity-local rules.
2. Run `rdra-ish states` and `rdra-ish check` for quick feedback.
3. `rdra-ish export samples/formal-verification/order.rdra --kind tla -o /tmp/rdra-tla` and inspect the Spec and `.cfg`.
4. Install [TLA+ tools](https://github.com/tlaplus/tlaplus) so `tlc` is on `PATH` (`nix develop` provides it).
5. `rdra-ish verify samples/formal-verification/order.rdra --backend tlc -o /tmp/rdra-tla` to run TLC.

Agent skill: `skills/rdra-ish-verify/` (routing + references + bundled samples).
Install: see [`skills/README.md`](../skills/README.md) (`npx skills add` / `gh skill install`).

## Samples

Canonical files: `skills/rdra-ish-verify/samples/` (each file is **standalone** —
do not `check` the whole directory; ids such as `Order` collide). In this repo,
`samples/formal-verification/` and `samples/formal-verification-fail/` are
symlinks to those skill-bundled files (CLI tests keep working after skill install).

| Sample | `check` and TLC | Focus |
|---|---|---|
| `order.rdra` | pass, pass | Lifecycle invariants + `after.assert` + `leads_to` / `eventually` + `WF_vars` |
| `int_stock.rdra` | pass, pass | Int columns, arithmetic `forbidden`, Int temporal property |
| `now_coupon.rdra` | fail, fail | `now` and `Tick`; the invariant finds `expired_at < now` |
| `cross_order_payment.rdra` | pass, pass | Multi-instance + `.along` (Pay is one atomic action, so Cancel cannot interleave; Cancel deletes the pending Payment) |
| `cross_order_payment_split.rdra` | fail, fail | The same model with Pay through two APIs, the capture first: two transaction groups, so Cancel interleaves between `Pay_1` and `Pay_2`; Cancel does not delete the Payment here |
| `split_run_order.rdra` | pass, pass | OpenSettlement through a claim API and a settlement API: one run in `invokes` order, so the settlement is created once under the claim the first step moved (`Unique_Settlement_Claim`), and a started run finishes under `.fair(OpenSettlement, ..)` |
| `quantifier_none.rdra` | fail, fail | `invariant(...).when(...).none(...)` (Revoke while Assign stays active) |
| `quantifier_guard.rdra` | pass, pass | `holds(..).none(..)` on Revoke makes the `.none(...)` rule of `quantifier_none` hold |
| `untriggered_sets.rdra` | fail, fail | `sets` from a use case that drives no transition (Flag then Ship reaches the forbidden state) |
| `event_order.rdra` | pass, pass | `triggers(EvOrderExecuted, IssueCert)` runs IssueCert only after ExecuteCertIssue raised the event on the order (#30, #112), so the `after(IssueCert).assert` on the order's status holds with no guard |
| `after_assert_unmet.rdra` | fail, fail | `after(DeliverOrder).assert` without the `sets` that makes it true |
| `temporal_fair.rdra` | pass, fail | `leads_to(granted, registered or dismissed)` fails without fairness: another application is ordered to amend and amends forever, and `WF_vars(Next)` holds on that loop. The same property with `.fair(Register, Dismiss)` holds |
| `temporal_unmet.rdra` | pass, fail | `always(Order.status != shipped)` fails once an Order ships, and `leads_to(pending, delivered)` fails because ReopenOrder lets an Order be cancelled and reopened forever, so its lasso loops back. `check` does not model-check properties; each prints its lasso in model terms |
| `one_to_one_shared.rdra` | pass, fail | `relate(.., 1:1)` adds `Unique_Shipment_Order`; Ship and Reship each link a Shipment to the same Order, which takes two instances |
| `none_any_child.rdra` | pass, pass | `.none(Invoice)` on Issue says "no invoice yet" and reads the link only, so `Unique_Invoice_Order` holds on a cone without `Invoice.status` (25 states). The enumerating form `.none(Invoice.status in (<every value>))` builds the same cones |
| `none_any_child_bypassed.rdra` | pass, fail | The same model with Reissue, which creates an Invoice under a closed Order without the guard; `Unique_Invoice_Order` fails at two instances on the cone without `Invoice.status` |
| `min_card_unmet.rdra` | fail, fail | `relate(OrderLine, Order, 1..*:1)` adds `MinCard_OrderLine_Order`; Place creates an Order without a line |
| `min_card_met.rdra` | pass, pass | The same model with Place creating the Order and its first line in one transaction. At one instance `check` warns that AddLine never runs: an Order exists only with the one OrderLine slot taken. At two instances it runs |
| `check_unmet.rdra` | fail, fail | `qty: Int @default(1) @check(qty >= 1)` adds `Check_CartLine_qty_0`; ClearQty sets `qty` to 0 |
| `check_met.rdra` | pass, pass | The same check with a use case that keeps `qty` in range; `forbidden` reads the derived `bulk: Bool @derived(qty >= 3)` as its expression |
| `unwitnessed_state.rdra` | fail, pass | `refunded` has no witness: Refund needs `shipped` and a refund window that holds only while `pending`. TLC passes; the BFS witness check fails `check` and `verify`, a shipped Order is a deadlock (Refund is its only way out), and both warn that Refund never runs (its guards never hold together). TLC alone finds both errors with the exported `StateModel_Order` specs |
| `deadlock.rdra` | fail, pass | An Order stays `paid` forever: Ship leaves `paid` only while a packing condition holds, and it holds only while `pending`. Express ships a pending Order, so `shipped` has a witness. TLC passes; the BFS deadlock check fails `check` and `verify`, and both warn that Ship never runs. TLC alone finds the deadlock with the exported `StateModel_Order` specs |
| `partial_scope.rdra` | fail, not run | Audit's `forbidden` reads the arithmetic `@derived` column `score`: a `DerivedColumnRead` build error stops `check` and `verify` before TLC. With `--entity Order` both defer it under `not checked:` and pass; TLC checks `Forbidden_Order_0` |
| `fail/order.rdra` | fail, fail | Intentionally unsafe (reachable invariant violation) |

BFS and TLC give the same verdict on every row at the same bounds. `check` runs at one
instance per entity, where `Unique_*` holds vacuously.

### Sample expectations

Each sample root in `samples.txt` states what `check` and `verify` must report in
`// expect` comment lines. `scripts/verify-samples.sh <rdra-ish binary>` runs both commands
on every root and fails when a run differs from its header. CI runs it on the release
binary in the `formal-verify` job.

The script reads the header from the root's first `.rdra` file in load order, which
`rdra-ish files <root>` prints first. An `// expect` line in any other file of the root
fails the script. The grammar is:

```
header       = "// expect " command [" " flags] ": " expectation
command      = "check" | "verify"
flags        = CLI words passed to the command as written, e.g. "--entity Order"
check exp    = "ok" | "fail" " " Name { " " Name }
verify exp   = "tlc=" ("ok" | "counterexample") [" fail" " " Name { " " Name }]
             | "fail" " " Name { " " Name }
Name         = [A-Za-z0-9_]+
```

- `fail` means exit status 1, and its absence exit status 0. Any other status, such as a
  `tool_error` exit, is a mismatch.
- `tlc=` must equal the `tlc_result:` line. A `verify` expectation without `tlc=` asserts
  that no `tlc_result:` line is printed: the run stops before TLC, as at a build error.
  `tlc=counterexample` also needs TLC's own `violated: Error: ... violated.` line.
- `fail` needs at least one name. The names must equal the set of names of the run's
  errors, not just be part of it. Every lifecycle error prints as `error[<Name>]: ...`, and
  the script reads the name from there and from TLC's `violated:` line. A name is the
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
  with no `check` line expects `check: ok`, and one with no unflagged `verify` line expects
  `verify: tlc=ok`. A flagged line adds a run and replaces neither default.

```rdra
// unwitnessed_state.rdra: TLC passes, and the BFS state-model findings fail the run.
// expect check: fail Witness_Order_status_refunded Deadlock_Order_status_shipped
// expect verify: tlc=ok fail Witness_Order_status_refunded Deadlock_Order_status_shipped

// partial_scope.rdra: the build error stops both commands before TLC; --entity Order passes.
// expect check: fail DerivedColumnRead_Audit_score
// expect verify: fail DerivedColumnRead_Audit_score
// expect verify --entity Order: tlc=ok
```

The script prints one line per run with the verdict, the wall time, and the maximum RSS
(GNU `time` on Linux, BSD `/usr/bin/time` on macOS), then the count of roots and runs.
Locally (Apple silicon) the 63 runs of the 31 roots take about 220 s: clinic-ops `verify`
takes 105 s at about 2.0 GB (50 s at 1.4 GB before the run marks of `CheckInPatient`, #80)
and patent-ops `verify` 68 s at about 1.2 GB, their TLC JVMs included; food-permit-ops
`verify` takes 16 s, and insurance-claim-ops `verify` about 3 s at about 470 MB. The CI
step has 5 minutes.
