# Formal Verification

<!-- derived-from ./state-derivation.md -->
<!-- derived-from ./language-reference.md#entity-state-constraints -->
<!-- derived-from ./language-reference.md#temporal-path-properties -->
<!-- derived-from ./cli-reference.md#export -->
<!-- derived-from ./cli-reference.md#verify -->

This guide is for modelers who check a model's lifecycle: what `check`, `verify`, and
`export --kind tla` decide, how to read what they print, and where a passing run stops
proving anything. Each flag is in [cli-reference.md](./cli-reference.md#verify). The
transition system the commands explore is in [state-derivation.md](./state-derivation.md).
How the engines are held to each other is in [engine-agreement.md](./engine-agreement.md).
The soundness arguments are in [Cone soundness](./design/cone-soundness.md).

Three engines read one transition system:

- **The native checker** decides everything by breadth-first search (BFS). `check` and
  `states` run it at one instance per entity on the invariants, the `after` assertions,
  and the state models. `verify` runs it at two instances on every rule and every
  `property`, each on its own cone. No JVM starts.
- **TLC** runs only under `verify --with-tlc`, on the same cones and runs, as a second
  opinion. A check the two engines decide differently fails the run.
- **z3** runs only under `verify --prove`. It tries to prove each state rule for any number
  of rows.

Paths in the transcripts below are shortened to the repository root.

## Workflow

1. Model the lifecycle with `transitions`, `sets`, and entity rules.
2. Run `rdra-ish check` for fast feedback: one instance per entity, safety only.
3. Run `rdra-ish verify samples/formal-verification/order.rdra` to check every rule and
   property at two instances.
4. Add `--prove` to prove the state rules for any number of rows
   ([Proofs for any number of rows](#proofs-for-any-number-of-rows)).
5. Add `--with-tlc` for a second opinion. `nix develop` puts `tlc` on `PATH`; other ways
   are in [Getting TLC](./cli-reference.md#getting-tlc).
6. Run `rdra-ish export samples/formal-verification/order.rdra --kind tla -o /tmp/rdra-tla`
   to read the TLA+.

The agent skill `skills/rdra-ish-verify/` walks the same loop. Install it as
[`skills/README.md`](../skills/README.md) says.

## Commands

| Command | Decides | Engine |
|---|---|---|
| `check`, `states` | Invariants, `after` assertions, witnesses, deadlocks, and use cases that never run, at one instance per entity. No `property`: `check` prints a `note:` that says to use `verify`. | native BFS |
| `verify` | Every invariant, `after` assertion, and `property`, each on its own cone, and the state models of each entity cone, at two instances | native BFS |
| `verify --with-tlc` | The same, with TLC on each cone and run beside it | native BFS and TLC |
| `verify --prove` | Each state rule, for any number of rows and every Int range the model builds at | z3, beside the above |
| `export --kind tla` | Nothing. Writes `RdraSpec.tla` and `RdraSpec.cfg` for the whole model, and the `StateModel_<E>` specs, for a TLC run by hand | none |

`--instance-count` (default 2 in `verify` and `export`, 1 in `check`) and `--int-range-max`
(default 5) set the bounds. Every result names them. `verify -o <dir>` writes one module per
cone and run, and `cones.txt` maps each module to its checks. The native checker reads none
of these files. The file list is in [cli-reference.md](./cli-reference.md#verify).

The `.cfg` of the whole model names `SPECIFICATION Spec`, `INVARIANT TypeOK`, one `INVARIANT`
per rule, one `PROPERTY` per `after` assertion and temporal property, and `CHECK_DEADLOCK
FALSE`. A bounded lifecycle ends in states with no enabled action, so TLC's own deadlock
check stays off. The `StateModel_` specs check the deadlocks and witnesses instead. When
`check` finds lifecycle errors, `export --kind tla` still writes every spec, exits 0, and
prints one note with the error count.

### State models with TLC alone

<!-- constrained-by #what-verify-checks -->
<!-- constrained-by ./state-derivation.md#triggers -->
`check` and `verify` decide deadlocks and states with no witness by BFS on each entity's cone.
That cone holds every raiser of an event its use cases wait for, so no handler runs there
without its event and the findings are the whole model's.
`export --kind tla` writes the same cones as TLA+, so TLC alone can look for them. For each
cone whose entities have a state model, it writes three files:

| File | Checks |
|---|---|
| `StateModel_<E>.tla` | The cone, reduced as `verify` explores it, with one `Witness_<E>_<col>_<s>` invariant per state `s` of each state model and one `Deadlock_<E>_<col>_<s>` invariant per state with a way out |
| `StateModel_<E>.cfg` | `TypeOK` and the `Deadlock_` invariants |
| `StateModel_<E>_witnesses.cfg` | `TypeOK` and the `Witness_` invariants |

`<E>` is the first entity of the cone with a state model. The invariants have the names of
the findings `check` prints. `export` does not remove old `StateModel_` files, so export into
an empty directory.

- `Deadlock_<E>_<col>_<s>` says that wherever an instance is at `s`, some step can still
  change the column's cone: the column, its entity's existence, and every variable the steps
  that write them read, the raised marks of [Triggers](./state-derivation.md#triggers)
  included. TLC reports it violated where an instance is at `s` and no move of that cone is
  left. Such a deadlock is one `check` reports.
- `Witness_<E>_<col>_<s>` says that no instance is ever at `s`. A healthy model violates every
  one, so TLC exits non-zero. A `Witness_` invariant TLC does not report is a state with no
  witness.

```
tlc -continue -config StateModel_Order.cfg StateModel_Order.tla
tlc -continue -config StateModel_Order_witnesses.cfg StateModel_Order.tla
```

With `-continue`, TLC reports one violated invariant per state, the first in cfg order.
Rerun without the reported invariants until no new one appears, as `verify` does. TLC also
prints a trace per violating state, which on the witnesses cfg is nearly every state. For a
large cone, run the witnesses cfg without `-continue` and rerun once per finding.

A TLC pass does not prove that no deadlock exists. An instance can be stuck while the
column's cone keeps moving forever: another column a guard reads, a related entity, or
another instance. `check` reports that deadlock, since no path moves the instance, and TLC
passes. A deadlock that only the instance bound causes is a warning in `check` and a
violation in TLC.

Under `--with-tlc`, `verify` runs these specs on each entity cone BFS explored to the end, up
to 100,000 states. TLC must name the same states without a witness, and each deadlock it
reports must be one BFS reports. Any other difference is `error[EngineDisagreement]`. One
line per module says what TLC found:

```
tlc: StateModel_Order (13 states): 3 deadlock and 4 witness checks; found Deadlock_Order_status_shipped, Witness_Order_status_refunded
```

## Mapping

How each construct prints in TLA+. The native checker and z3 read the same transition
system.

| RDRA | TLA+ |
|---|---|
| Entity | `<E>_Ids == 1..InstanceCount` and `<E>_exists`, a function over the ids with values `"absent"`, `"present"`, `"deleted"` |
| Enum, Bool, `@null`, Int, Money, Decimal, Date, or DateTime column the system reads or writes; a variation-typed column that something names | `<E>_<col>`, a function over `<E>_Ids`. Int-like columns range over `IntRange`, and a nullable one adds `-1` for null. A `@null` Enum column ranges over its values and `"<null>"`, and a `@null` Bool column over `"<null>"`, `"false"`, and `"true"`, so TLC never compares a string with a boolean. `==` between a `@null` Enum or Bool column and one without null compares value by value: `(E_a[b0] = "red" /\ E_b[b0] = "red") \/ ...`. |
| `relate` N:1, 1:N, 1:1 | `<Child>_owner_<Parent>`, a function from child ids to parent ids. An `.optional()` relate adds `0` for an unlinked child. |
| Use case | One operator `<UC>(p0, ...)` per transaction group, `<UC>_1`, `<UC>_2`, ... when the writes form several. `Next` is the disjunction of `\E` over the parameters. |
| A use case split into k groups | A run mark `<E>_run_<UC>` with values `{"none", "1", .., "k-1"}` on each entity a step before the last writes. `<UC>_1` takes a row at `"none"`; `<UC>_i` binds each earlier row by `<E>_run_<UC>[p] = "i-1"` (see [Runs of a split use case](./state-derivation.md#runs)). |
| A tracked event `Ev` with mark entity `E` | A raised mark `<E>_raised_<Ev>`, `FALSE` in `Init` and never `FALSE` again (see [Triggers](./state-derivation.md#triggers)). The last step of a raiser sets it on its row; the first step of a triggered use case conjoins `<E>_raised_<Ev>[p] = TRUE`. |
| A `Triggered` parameter | One more `\E p \in <E>_Ids` on the triggered operator, with the mark conjunct and, when one direct key joins it to a row of the operator, the owner equality. A cone that leaves the mark out drops the conjunct and the parameter. |
| A use case several tracked events trigger | One operator per variant, `<UC>_by_<Ev>`. `AfterAssert_<UC>_<n>` ranges over every variant's last step. |
| `creates` | `FirstAbsent(<E>_exists, p)` and `<E>_exists' = ... "present"`. A deleted slot is never reused. |
| `deletes` | `<E>_exists'` becomes `"deleted"`, the columns return to the base value, and the owners keep the rows they name. A key with no `.on_delete`, `restrict`, or `no_action` adds a guard that no present child names the row, apart from a child the same step deletes, unlinks, or relinks. |
| `sets`, `transitions(UC, E.col, a -> b)` | Primed assignments. A transition guards `col = a` and writes `b`; arms to several targets write through a `CASE` on the source. `UNCHANGED` lists every variable the action does not assign. |
| Variation column of a created instance | `\E v0 \in {values} : col' = ...`. `@default` does not pin it. |
| `applies_to(Condition, UseCase \| Api)` with a `holds` body | A guard conjunct. On a created instance it reads the primed variables. |
| `drives(UC, E.col op rhs, bool)` | A Boolean variable. An undriven comparison is arithmetic on the Int columns. |
| `col in (a, b)` / `col not in (a, b)` | `E_col[b0] \in {"a", "b"}` / `E_col[b0] \notin {"a", "b"}`. A nullable Int `present` keeps the `\/` form. |
| `col < now` | A global `now \in IntRange` and a `Tick` action that adds one while `now < max` |
| `forbidden`, `invariant`, `required`, `exclusive`, local or multi-entity | One `INVARIANT` each, `<Kind>_<Entity>_<n>` (`Forbidden_Order_0`, `Inv_Order_0`, `MultiForbidden_Order_0`), over the present instances |
| `.along(...)` | Quantified over `<E>_Ids`, joined by `<Child>_owner_<Parent>` equalities |
| `col: T @check(expr)` | `INVARIANT Check_<Entity>_<column>_<n>`. A nullable operand is guarded: `qty: Int @null @check(qty >= 1)` prints `(E_qty[b0] # -1 /\ E_qty[b0] >= 1)`, so a null `qty` violates it. `==` between two nullable columns guards both, `(E_qty[b0] # -1) /\ (E_lim[b0] # -1) /\ (E_qty[b0] = E_lim[b0])`, and `!=` is its negation, so two null columns are not equal ([Null](./language-reference.md#null)). |
| `relate(Child, Parent, 1:1)` | `INVARIANT Unique_<Child>_<role>`: two present children are the same or have different owners |
| `relate(Child, Parent, ..)` with no `.on_delete`, `restrict`, or `no_action`, whose parent a use case deletes | `INVARIANT DanglingKey_<Child>_<role>`: a present child names a present parent, or none under `.optional()`. A `cascade`, `set_null`, or `set_default` key gets none, since the lifecycle does not run those actions. |
| `@unique`, `@unique(a, b)`, the key pair of a `relate(A, B, N:M).via(J)` junction | `INVARIANT Unique_<Entity>_<columns>`: two present rows differ in some column of the set. A null Int (`-1`), a null Enum or Bool (`"<null>"`), or an unlinked key (`0`) exempts its row ([Unique column sets](./state-derivation.md#unique-column-sets)). |
| `relate(Child, Parent, 1..*:1)` | `INVARIANT MinCard_<Child>_<role>`: a present parent has a present child, in every state |
| Pre-existing child of a `relate` | `Init` links the owner: any parent for N:1 (start `Any`), `[i \in <Child>_Ids \|-> i]` for 1:1 and a child-side minimum (start `Identity`). An `.optional()` key under a created parent starts unlinked. |
| An action that creates a parent and updates an existing child | The action links the child. An `.optional()` key links only an unlinked child; a mandatory key moves it. |
| `invariant(...).when(...).none/has(...)` | `\A` / `\E` over the ids, named `Quantifier_<Entity>_<n>` |
| `after(UC).assert(...)` | `PROPERTY AfterAssert_<UC>_<n>`: `[][(\A p : UC(p) => post')]_vars` on the last action of each run |
| `always`, `eventually`, `leads_to` | A named formula and a `PROPERTY`, per present instance: `[](present => x)`, `[](present => <>x)`, `(present /\ x) ~> y`. `.along` joins the owner link to `present`. With `.present()` the goal is `present /\ x` (`present /\ y`). |
| `stable(p)` | `[][\A b0 \in E_Ids : (present /\ p) => (present' => p')]_vars` |
| `unchanged(E.col, ..).when(p)` | `[][\A b0 \in E_Ids : (present /\ p) => (present' => E_col'[b0] = E_col[b0] /\ ..)]_vars` |
| `precedes(p, q)` | A latch `P_seen`, set by every action once `present /\ p` holds, and `[](\A b0 : (present /\ q) => P_seen[b0])`. Only a module whose `.cfg` checks `P` declares it. |
| Any `eventually` / `leads_to` | `Spec` includes `WF_vars(Next)` |
| `.fair(UC, ...)` | `Fair_P`, one `WF_vars` per action of each listed use case and per instance, and `P == Fair_P => (...)`. `Spec` is unchanged. |
| `can_reach` | No formula: TLC cannot state `AG EF`. A comment lists it. |
| Domain of every variable | `INVARIANT TypeOK` |

### Order and stability properties

<!-- constrained-by ./language-reference.md#order-and-stability-stable-unchanged-precedes -->

`stable`, `unchanged`, and `precedes` are safety properties. `verify` decides each by BFS on
its own cone and prints one line per property:

```
$ rdra-ish verify samples/formal-verification/order_stability.rdra
...
property CancelledStaysCancelled (BFS on its cone): violated
error[CancelledStaysCancelled]: property CancelledStaysCancelled is violated: its stable condition holds for Order#1 before step 3 and is false after it
  rule: skills/rdra-ish-verify/samples/order_stability.rdra:64:1
  counterexample (3 steps):
    1. Place [Order#1]                          skills/rdra-ish-verify/samples/order_stability.rdra:18:1
         + Order#1 (status=draft, paid=false)
    2. Cancel [Order#1]                         skills/rdra-ish-verify/samples/order_stability.rdra:21:1
         Order#1.status: draft -> cancelled     skills/rdra-ish-verify/samples/order_stability.rdra:42:1
    3. Reopen [Order#1]                         skills/rdra-ish-verify/samples/order_stability.rdra:22:1
         Order#1.status: cancelled -> draft     skills/rdra-ish-verify/samples/order_stability.rdra:43:1
```

- `stable` and `unchanged` are read on every edge. The first breaking edge BFS finds gives
  the shortest trace.
- `precedes(p, q)` is read with its latch: a state where `q` holds and no earlier state of the
  path had `p` for that binding. The trace is the shortest such path.

A violation is a finite trace. On a cone that frees an entity's slots (see
[What `verify` checks](#what-verify-checks)), a violation is decided again on the whole
model. A cone stopped at `--max-states` or the memory limit is `not decided`. A scoped run
(`--buc`, `--entity`) checks none of these properties. Under `--with-tlc`, TLC checks each on
the same cone, module `Cone_<Property>`.

## What `verify` checks

`verify` restricts the system to the cone of influence of each check and decides each cone
by BFS, side by side:

- **Safety cones.** Each invariant and `after` assertion has a cone. Cones over the same
  variables share one run, `Cone_<first check>`. A cone reaches the whole model's states
  projected onto it, so an `ok` on every cone is the whole model's `ok`.
- **A cone that frees slots.** When an action of the cone creates an entity the cone does not
  read, the cone does not count that entity's slots and reaches a superset. The `bounds:` line
  says `entities a check does not read are unbounded`. An `ok` still holds. A violation
  there may need more instances of that entity than the bound.
- **A cone that leaves a raised mark out.** A cone that keeps an event's handler but not
  every raiser runs the handler without the event. An `ok` still holds. A violation whose
  trace does so is only a candidate: the cone takes in the event's raisers and runs again,
  until its counterexample runs no use case without its event or the cone is the check's exact
  cone. The cone's line says what that refined cone found: `note: <check>: the cone's
  counterexample is a candidate only, since it runs a use case without its event having been
  raised; on the cone refined with the event's raisers: ok`. The line's verdict is the refined
  cone's; TLC is compared with the cone's own.
- **Covered cones.** When a larger cone holds every check, variable, action, guard, and
  parameter of a smaller one, the smaller one does not run, and its line reads
  `ok (checked in Cone_<larger>)`. When the larger cone fails, the smaller one runs too, so
  the trace comes from the smallest cone.
- **Property cones.** Each `property` runs on its own cone, and on the whole model only when
  its cone cannot decide it ([Temporal properties on a cone](#temporal-properties-on-a-cone)).
- **Entity cones.** Each entity's cone is explored at the same bounds for its state models:
  each state needs a witness ([State models and witnesses](./state-derivation.md#state-models-and-witnesses)),
  no instance may get stuck ([Deadlocks](./state-derivation.md#deadlocks)), and each use case
  should run ([Use cases that never run](./state-derivation.md#use-cases-that-never-run)).
  A state with no witness or a deadlock fails `verify` even when every check holds. A use
  case that never runs and a deadlock at the bound are warnings.

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

Neither property of `order.rdra` has `.fair`, and `DeliverOrder` writes `delivered_at`, which
neither reads, so their cones cannot decide them and the whole model does. With
`.fair(ShipOrder)` on `PaidLeadsToShipped`, its first cone run would decide it.

**Reading a counterexample.** A violation prints in model terms, as `check` prints it: the
rule's site, then one line per step with the use case, the instances it binds
(`Place [Order#1, Payment#1]`), and its site, and under each step the values it changed. The
trace is BFS's shortest path to the violation. An `eventually` or `leads_to` counterexample
is a lasso: it ends with `then loops back to step K` (steps K to the last repeat forever) or
`then nothing changes forever`. The head names the instance and the step where the trigger
holds and the goal never follows:

```
$ rdra-ish verify samples/formal-verification/temporal_unmet.rdra
...
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

Every native counterexample replays through the step relation and fails its property before
it prints. One that does not is a tool bug, `error[LassoFault]`. Every other printed trace
replays too: a state rule's, an `after` assertion's, an `always`, `stable`, `unchanged`, or
`precedes` property's, a `can_reach` failure's, a deadlock's, and a `states` view's witness.
Each replays from an initial state through the run's own steps, on a plain, a reduced, or a
symmetric run, and on a reduced run each lifted step is checked against the cone's steps in
every build. A trace that fails is a tool error with no trace: `error[LiftFault_<check>]` for
a rule or an assertion, `error[LassoFault]` for a property, and `error[TraceFault_<finding>]`
for a deadlock, a `can_reach` failure, or a witness. A reduced lasso whose lift fails counts
as spurious, and the property is decided on the unreduced cone.

**Under `--with-tlc`.** TLC runs each cone and property run beside BFS, as many at once as
their heaps fit TLC's share of the [memory limit](./cli-reference.md#memory-limit), each
with up to 8 workers. A safety cone that fails runs again on one worker, so its trace is the
shortest. `verify` decodes TLC's trace, replays it through the same step relation, and prints
it in model terms (`counterexample (N steps, from TLC)`), then TLC's own `violated:` line.
The output is in [With TLC](./cli-reference.md#with-tlc).

### Temporal properties on a cone

<!-- derived-from ./state-derivation.md#exploration-by-cone-of-influence -->
<!-- constrained-by ./language-reference.md#fairness-fairusecase- -->
<!-- constrained-by ./design/cone-soundness.md#a-propertys-cone-decides-under-its-own-fairness -->
A property's cone holds what the formula reads, the owner links of its `.along`, and
everything each action of its `.fair` use cases reads and writes, closed under the actions
that write into it. On that cone the property's verdict under its own fairness `Fair_P` is the
whole model's ([why](./design/cone-soundness.md#a-propertys-cone-decides-under-its-own-fairness)).

`WF_vars(Next)`, which `Spec` adds for any `eventually` or `leads_to`, does not restrict to a
cone: a step outside the cone satisfies it. So a property gets up to three runs, and the
native checker decides each on the graph BFS explores:

1. `Cone_<P>`: the cone without `WF_vars(Next)`. An `ok` decides. An `always` needs only
   this run.
2. `Cone_<P>_WF`: the same cone with `WF_vars(Next)`, after a counterexample in run 1. A
   counterexample decides. An `ok` decides only when the cone holds every variable an action
   writes.
3. `RdraSpec_<P>`: the whole model, for a property its cone leaves undecided. It stops at
   `--temporal-budget` seconds (default 300), at the memory limit, or at `--max-states`.
   Stopped before a counterexample, the property is `not decided`: it is listed under `not
   checked:`, the run prints `result: not decided` and a `warning:`, and exits 0 unless
   `--deny-undecided` is given.

A cone that frees slots can show a lasso that creates more instances than the bound, and one
that leaves a raised mark out a lasso that runs a use case without its event, so only its `ok`
counts and a counterexample goes to the whole model. Each line names what happened:
`Cone_P [P]: ok (111 states)`, `Cone_P [P]: counterexample (43 states, Cone_P_WF:
counterexample)`, or `Cone_P [P]: not decided (31 states, Cone_P_WF: ok, needs the whole
model's WF_vars(Next))` followed by the `RdraSpec_P [P]:` line. A reduced cone may add
`Cone_P_Full` ([WF_vars(Next) on a reduced cone](#wf_varsnext-on-a-reduced-cone)).

A property without `.fair` rests on `WF_vars(Next)` alone, so it reaches the whole model
unless its cone holds every written variable. **Name the use cases a property relies on with
`.fair(UC, ...)`** to keep it on its cone. `verify` prints `<P> assumes weak fairness per
instance on ...` under the `temporal properties:` line, since its `ok` holds only on runs
that do not starve them:

```
$ rdra-ish verify samples/formal-verification/temporal_fair.rdra
...
temporal properties: 2, each checked on its cone, and on the whole model only when the cone does not decide it (budget 300 s)
  GrantEnds assumes weak fairness per instance on Register, Dismiss
state models (BFS on 1 entity cone): 1 state model, 5 states witnessed, 0 deadlocks, 0 use cases that never run
Cone_GrantEnds [GrantEnds]: ok (43 states)
Cone_GrantEndsUnfair [GrantEndsUnfair]: counterexample (43 states, Cone_GrantEndsUnfair_WF: counterexample)
```

### Recoverability on a cone

<!-- derived-from ./state-derivation.md#exploration-by-cone-of-influence -->
<!-- constrained-by ./language-reference.md#recoverability-can_reach -->
`property P can_reach(goal)` is the CTL formula `AG (present => E[present U (present /\
goal)])` per instance: from every reachable state where the instance is present, some path on
which it stays present reaches the goal. No TLA+ temporal formula states it, so TLC does not
check it, and `check` and `states` do not evaluate it.

`verify` decides it by BFS on its cone. One backward pass per binding marks every state that
can still reach the goal; a present state left unmarked fails the property, and the first one
in BFS order ends a shortest trace. The cone must be exact, since a step the whole model lacks
could reach the goal and hide a violation. So a `can_reach` cone keeps every action that
writes into it, counts the slots of what they create, and is never reduced. A cone stopped at
`--max-states` or the memory limit decides nothing: the property is listed under `not
checked:` and counts toward `--deny-undecided`.

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

The 34 states count the moves of Order#2 too. Order#1 stays at `pending` or `on_hold` in all
of them.

### Reduction by an independent sub-lifecycle

<!-- derived-from ./state-derivation.md#exploration-by-cone-of-influence -->
<!-- derived-from #temporal-properties-on-a-cone -->
<!-- constrained-by ./design/cone-soundness.md#the-reduction-by-an-independent-sub-lifecycle -->
A cone often holds a second lifecycle that its targets never write and read only in part:
the claim's cone in insurance-claim-ops holds the case's settlement. Every state of that
lifecycle interleaves with every state of the target's. `verify` explores each such cone
reduced by the sub-lifecycle (#74). `check` and `states` explore it unreduced.

The cone splits into the target side T and the sub-lifecycle S, which T reads only through
`obs`. S is explored on its own and its states are grouped into classes that T cannot tell
apart; the reduced cone keeps one state per class. A sub-lifecycle may also read T variables
that only rise, such as a raised mark or a forward-only status (#112). The reduced cone
decides what the cone decides, and a trace is lifted back to a path of the full cone before
it prints, so a printed counterexample can be longer and name a use case of S. The split
rules, the TLA+ tables (`S_class`, `Rep_<var>`, `Succ_<A>`, `Rise`), and the proof are in
[the design note](./design/cone-soundness.md#the-reduction-by-an-independent-sub-lifecycle).

What a modeler sees:

- `--max-states` counts the states of the reduced cone.
- A check that reads S outside `obs`, an `after` assertion on an S action, or a property
  whose `.fair` names an S action runs on a cone of its own.
- A reduction whose TLA+ tables pass 530,000 cells is not used, so the native checker and
  TLC check the same cones. Under `--with-tlc` a cone's heap follows its module size.
- No `can_reach` cone is reduced.

### WF_vars(Next) on a reduced cone

<!-- derived-from #reduction-by-an-independent-sub-lifecycle -->
<!-- constrained-by #temporal-properties-on-a-cone -->
<!-- constrained-by ./design/cone-soundness.md#wf_varsnext-on-a-reduced-cone -->
The reduction is blind to divergence: a behavior that loops forever inside one class of S
meets `WF_vars(Next)`, while its image on the reduced cone stands still. So the `_WF` run on a
reduced cone lets a behavior stop wherever a class *can loop*. Let `X` be the run of the
sub-lifecycle the classes come from. A class `c` can loop when:

1. the S steps of `X` between two different states in `c` contain a cycle;
2. a state of `X` in `c` holds an inner class that can loop, for a nested reduction; or
3. S is unobserved (`obs` empty), so it is one class and `X` is not explored, and some S
   variable is not monotone.

An `ok` of that run is the `ok` of `Cone_<P>_WF` on the full cone
([proof](./design/cone-soundness.md#wf_varsnext-on-a-reduced-cone)). A counterexample is only
a candidate: `verify` lifts it to the full cone and replays it there. A candidate that does
not lift is dropped, and `verify` runs `Cone_<P>_Full` and `Cone_<P>_WF` on the full cone.
The line then notes `WF_vars(Next) on the reduced cone (native only): ok` or `…: counterexample`,
or `Cone_P_Full: counterexample` before the `_WF` note. TLC runs no module of the reduced run's
system: `WF_vars(Next)` on the reduced module would not let a behavior stop where a class can
loop. So `--with-tlc` compares that run only through the final verdict, against
`Cone_P_WF` on the full cone.

### Instance symmetry

<!-- constrained-by ./design/instance-symmetry.md -->
The slots of an entity are interchangeable unless the model tells them apart.
`verify --symmetry always` (experimental; default `never`, or `symmetry` under `[verify]` in
`rdra.toml`) stores one state per class of states that differ only in which slot holds which
row. It runs only a cone that no plain run decides: a safety cone, the cone of an `eventually`
or `leads_to`, an unreduced `always` cone, and an entity cone whose plain run stopped at the
memory limit (#370). Such a run prints `about` before its count, the sum of its orbit sizes:

```
Cone_PaidLeadsToShipped [PaidLeadsToShipped]: not decided (about 31 states, Cone_PaidLeadsToShipped_WF: ok, needs the whole model's WF_vars(Next))
```

An entity cone runs again up to symmetry only when its plain run stopped with a finding left
open, in the room the plain run gave back. If that run completes, it decides the cone's
witnesses, use cases that never run, `.when` arms taken, and deadlocks; if it stops too, the
plain run's findings stand.

The verdicts are the plain run's. A counterexample, or a deadlock's trace, is lifted back to
concrete slots and replayed before it prints, so it can name another instance than `never`
names. Which cones run, what breaks the symmetry, and `RDRA_ISH_SYMMETRY_STATS` are in
[Instance symmetry](./cli-reference.md#instance-symmetry).

### Memory, state caps, and threads

<!-- constrained-by ./cli-reference.md#memory-limit -->
- **Memory.** `verify` runs within `--memory`, else `memory` under `[verify]` in
  `rdra.toml`, else `16GiB`. BFS takes all of it, or half under `--with-tlc`. `check` and
  `states` default to `4GiB`. A cone past the limit stops: in `verify` its findings are
  `not decided`, in `check` it is the error `MemoryCapReached_<E>`.
- **What the limit counts.** It counts the BFS state stores and, under `--with-tlc`, the TLC
  heaps, not the whole process (#237). The passes over a finished cone (liveness, `can_reach`,
  deadlocks, the reduction's partition), a growing run's spare buffers, and z3 are outside it,
  and a run has been measured well past the limit. Under `--prove` each z3 process (one per
  `RDRA_JOBS`) takes about 0.3 to 0.5 GiB more, beside BFS's passes and TLC, and holds one
  of the cores the TLC runs are scheduled on. When cones stop at the limit, set `--memory` to
  about half the memory you can spare ([Memory limit](./cli-reference.md#memory-limit)).
- **A stopped entity cone gives back its room.** `verify` reads the findings of an entity
  cone that stopped at the limit at once, then frees what it held, so the cones after it can
  run (#361). Under `--symmetry always` that cone runs again up to symmetry in the same room.
- **State caps.** `verify` has no state cap unless `--max-states <N>` gives one, which caps
  every BFS run. `--max-patterns` stops no `verify` run.
- **Threads.** `RDRA_JOBS` sets how many cones run at once (default: the cores, at most 8) and
  how many threads expand one cone (default: a third of the cores, at least 4 and at most 12,
  never more than the cores; #371). The output does not depend on either.

### Part of a model

<!-- derived-from ./cli-reference.md#partial-runs -->
`verify --entity E` checks the entity's view and the invariants and `after` assertions that
read `E`, each on its own cone, so each verdict is the whole model's. `verify --buc B` keeps
only the use cases reachable from `B`, so it cannot decide whether a state has a witness, a
use case runs, or an instance gets stuck. A scoped run checks no property. A build error that
no selected cone reads is deferred:

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

The rules for each flag are in [Partial runs](./cli-reference.md#partial-runs).

## Proofs for any number of rows

`verify --prove` is experimental. BFS and TLC check a fixed number of instances and an Int
range with a top, so a rule that holds at two instances can break at three. `--prove` asks z3
whether each state rule holds for **any** number of rows and **every** Int range the model
builds at. It never finds a counterexample; the bounded engines do that.

Every line that says a rule or lemma is proved, the `proof (...)` header, and the clause the
`OK:` line ends with state the same claim, for example `for any number of rows and every
--int-range-max from 4 to 254, in the whole model`:

- **Rows**: any number; the encoding has no bound on them.
- **Int**: every `--int-range-max` at which the model builds. The least is the first one every
  Int literal of the model fits (a literal past the range is `IntLiteralOutOfBounds`); the
  greatest is 255, or 254 with a nullable Int column, which past that holds more values than
  a state does (`DomainTooLarge`). Inside the encoding an Int column has no top, and `now`
  none either, so the proof holds at each of them. A model with no Int column names no range.
- **Scope**: `in the whole model`, or `in the selected scope: <flags>` under `--buc`,
  `--entity`, `--without-usecase`, `--without-buc`, or `--actor`. The prover then encodes the
  system the scope leaves, which lacks the use cases it drops, so a rule proved there may
  still break in the whole model.

This is the scope `Sc(M, β)` of Theorem 1 in [the z3 notes](theory/z3.md#the-soundness-theorem).

Each entity's rows become an uninterpreted sort, each column a function, and each use case a
step relation built from the transition system BFS explores. A rule is proved when it belongs
to an **inductive** set of formulas: every formula holds initially, and no step from a state
where all hold breaks one. Every proof also has a canary candidate, `false`, which must fall
at initiation: it survives only when the encoding's declarations and initial states are
inconsistent, and then every check would pass and every rule would read `proved`. The
encoding over-approximates BFS, so BFS's initial state satisfies them, and a canary that
survives is a bug in rdra-ish; then no rule is proved and each reads `unproved: <rule>: the
canary `false` survived initiation: ...`. The step relation is widened in two ways, which only add steps: a
creation may take any absent row, and numbers have no top. A guard or write the encoding cannot
state is widened the same way and printed as a `note:`. A rule the encoding cannot state is
`out of scope`. A count is stated exactly when it needs at most 8 witness rows:
`.at_least(k, ..)` up to `k = 8`, and `.at_most(k, ..)`, `.exactly(k, ..)` and `MaxCard_*` up to
`k = 7`, since they also state that `k + 1` rows do not exist. A larger count is `out of scope`.

Most true rules are not inductive alone, so the prover adds candidate **lemmas** and keeps a
set of rules and lemmas that is inductive together (the Houdini algorithm): it drops each
candidate a check breaks, or z3 gives up on, until none falls. Over one fixed set of
candidates with exact answers, that is the largest inductive subset; the kept set here may be
smaller, since a dropped candidate never returns, some lemmas join only once a rule falls, and
each round of CTI lemmas starts over. That costs proofs, never soundness. The
lemmas come from facts about absent and deleted rows and keys, and from what BFS shows on each
cone at the run's bounds and at one instance: the patterns a row takes, the values of each
column and pair of columns, and the patterns of a child with its parent (whole, per parent
column, one column of each, and one child column with two parent columns when its pairs do
not already imply it). From each count guard (`.at_most`, `.at_least`, `.exactly`, and
`exists` as at least one) over a key to the row its step moves come count lemmas: a parent
row with a value that step writes has at most or at least the guard's number of children by
that key, all of them or those the guard pins, when no reached state shows otherwise. A
parent value no state reached gets one too, since the count a guard asks for often needs
more rows than the run's bounds. From the run's bounds come ∀∃ lemmas too: while a present
row of one entity holds a value, some present row of another entity holds one, for each such
pair of column values that every reached state showing the first keeps; no key need link the
rows. The lemmas of one column of each, of one child column with two parent columns, of
counts, and ∀∃ wait: they join only when a rule stays unproved without them, the ∀∃ ones
only when a rule stays unproved with the others in, and until then z3 sees nothing of them,
so a proof that needs none of them sends z3 the same checks whatever of them a model offers. Each mining run stops at 50,000 states. A lemma is only a guess until it survives the same checks, so a capped
exploration costs proofs, not soundness. The lemmas a proof kept print as `lemma:` lines, and
one that only the one-instance run offered ends in `(mined at 1 instance per entity)`:

```
$ rdra-ish verify samples/formal-verification/order.rdra --prove
...
proof (Z3 version 5.1.0 - 64 bit): 2 state rules, for any number of rows, in the whole model
proved: Forbidden_Order_0 (for any number of rows, in the whole model)
proved: Inv_Order_0 (for any number of rows, in the whole model)
lemma: an absent Order row holds base values (for any number of rows, in the whole model)
lemma: no Order row is ever deleted (for any number of rows, in the whole model)
lemma: every present Order row is one of 5 reached patterns of (status, delivered_at) (for any number of rows, in the whole model)
lemma: a present Order row's (status, delivered_at) is one of {(pending, null), (paid, null), (shipped, null), (delivered, present), (cancelled, null)} (for any number of rows, in the whole model)
proof: 2 proved, 0 unproved, 0 out of scope
...
OK: no errors within 2 instances per entity, Int 0..5; 2 of 2 rules proved for any number of rows, in the whole model
```

A rule that is not proved prints a **counterexample to induction** (CTI): a step, the rows it
binds, and the rule's rows before and after, with any run mark or event mark they read. The
state before holds the rule and every kept lemma but need not be reachable:

```
$ rdra-ish verify samples/formal-verification/one_to_one_shared.rdra --prove --instance-count 1
...
unproved: Unique_Shipment_Order: not inductive; Ship(Order#1, Shipment#1) breaks it from a state where it and every proved rule hold (not necessarily reachable)
  Order#1 (present) status=placed -> status=shipped
  Shipment#1 (absent -> present) Order=Order#2 -> Order=Order#1
  Shipment#2 (present) Order=Order#1
```

Here the CTI is also a real run: at two instances BFS reaches the violation and reports it
with a trace. A rule BFS violates in the same run is not tried, since no induction can hold a
rule that a reachable state breaks. It prints with the instances per entity its cone ran at,
counts as `refuted` on the `proof:` line, and is BFS's error, not an unproved rule, so
`--deny-unproved` does not count it:

```
$ rdra-ish verify samples/formal-verification/one_to_one_shared.rdra --prove
...
refuted: Unique_Shipment_Order (BFS counterexample at 2 instances)
proof: 0 proved, 0 unproved, 0 out of scope, 1 refuted
...
```

**Lemmas from a CTI.** When a rule stays unproved, the prover tries to rule its CTI's state
out itself, in the manner of IC3. The CTI's state before the step, over the step's rows and
the rule's, is a set of facts: each row's columns and marks, the keys between those rows, and
that two rows of one entity differ. The clause "no rows with all of these" is weakened, one
fact dropped at a time in a fixed order, while it still holds in every initial state and no
step breaks it from a state where it, the rule, and the kept lemmas hold. A state of more
than 40 facts, or a clause that is not inductive even with every fact, gives none. A clause
that survives joins the proof as a candidate, which runs again (at most twice more), so it is
kept only when it is inductive with the rest, and `proved` means what it did. It prints as
`lemma: from the CTI of <rule>: <text>`, where the text is `.lemma()` source to copy into the
model when the language states it (rows of distinct entities, columns, marks, and keys):

```
$ rdra-ish verify crates/rdra-ish-cli/tests/engine_corpus/z3_cti_true_rule.rdra --prove
...
proved: MultiForbidden_Light_0 [not enforced by the SQL schema: several entities]
...
lemma: from the CTI of MultiForbidden_Light_0: forbidden(Door, Light, Door.state == ajar, Light.state == off).global().lemma()
```

The proof certificate does not carry such a lemma, so a model whose proof needed one runs the
proof in full each time instead of reusing the cached certificate.

**Adding a lemma.** When no lemma the prover finds rules a CTI's state out, write the missing
fact as a rule with `.lemma()`. `check` checks it like any rule, so a wrong lemma fails there with a
trace. `verify` runs no cone of its own for it, and the proof keeps it only when it proves it.
A lemma whose reads lie in another selected rule's cone is checked by that cone, so there a
false lemma is `refuted: lemma <name>` with a counterexample and fails the run, as in `check`.
It prints as `proved: lemma <name>`, `unproved: lemma <name>` or `refuted: lemma <name>`, the `proof: your lemmas: ...` line counts
it, and it never counts toward the `n of m rules` of the `OK:` line. A lemma, and no other
rule, can read a row's marks ([Marks in a lemma](./language-reference.md#marks-in-a-lemma)):
`Door.in_run(Open)` when a CTI holds a row between the steps of a split use case, and
`Sign.raised(EvShut)` when it turns on an event latch. A CTI that turns on a mark often has a
fact about columns one step earlier; read the mark only when a reachable state has the same
columns, as in the engine corpus's `z3_lemma_run_mark`.
insurance-claim-ops proves 「賠償の請求は示談書がそろってから完了する」 with this lemma, which
generalizes over every case status before negotiation:

```rdra
forbidden(Claim, ClaimCase, Claim.completed_on == present, ClaimCase.status in (received, responded, investigating), ClaimCase.coverage in (bodily_liability, property_liability)).along(Claim, ClaimCase).label("賠償責任の事故は示談交渉の前に請求を完了しない").lemma()
```

**Budgets.** Each z3 check has a budget of resource units (`rlimit`), a count of solver steps,
not of time: 20,000,000 for a rule, a `.lemma()` rule included, and 2,000,000 for a lemma the
prover adds (helper, mined, or from a CTI). So the proof is the same on a
busy machine and an idle one. A unit is z3's own count, so the limits hold for the z3 versions
they were measured on: 5.1.0 today, the one `flake.nix` and `flake.lock` pin. Under any other
version, or one whose `z3 --version` line it cannot read, `verify --prove` prints `note: z3
<version> is not a version the proof's resource limits were measured on (5.1.0); ...` on
stderr. The version must match exactly, since a patch release can change what a check
spends, and the note changes nothing else. The release test `proof_equivalence` meters every
check: it fails when a check that keeps a rule or lemma spends more than a tenth of its limit,
when a check runs out of its limit (z3 answers `unknown` and `get-info :reason-unknown` says
`canceled`), or when a check that answers spends nine tenths of it. With the lemma limit at
50,000, insurance-claim-ops has 44 lemma checks that run out, and the test fails on them. On
every sample on 2 solvers, and on the formal-verification samples, insurance-claim-ops, and
oidc-auth on 1 and 3, a check that keeps its candidate spends at most 161,000 units for a
rule (1/124 of its limit) and 116,000 for a lemma (1/17); on 4 solvers, which the test does
not run, a lemma check has spent 144,000 (1/14). No check runs out. The checks z3 gives up on
by itself (on 2 solvers, 303 lemma checks and 14 rule checks) stop on a quantifier it cannot
decide, not on the budget: the test prints what they spend and does not assert it, since that
depends on what the solver checked before. On 2 solvers it is at most 1,887,000 units for a
lemma, in food-permit-ops, and 10,000 for a rule; food-permit-ops has such lemma checks from
1,200,000 units up on 1 to 6 solvers, and on 3 one of them runs out, which the test, running
food-permit-ops on 2, does not see. The rounds that find the inductive set run z3 without model-based quantifier
instantiation (MBQI), which settles the checks that keep a rule several times faster; the
counterexamples to induction are then found with it on. A rule z3 gives up on is `unproved` with
`z3 gave up on <action>`. The checks are shared out over several z3 processes (`RDRA_JOBS`); the result does
not depend on how many. The set a proof keeps is stored under the user's cache directory as a certificate. When
the next run encodes the same rounds (the same z3 and rdra-ish versions, rules, lemmas, and steps), it offers
the certificate first and checks it again in one round: every candidate in it holds initially and no step
breaks one from a state where all of them hold. A certificate that passes and holds every rule is the proof;
any other costs that round, and the proof runs from the start. A cached verdict is never trusted, and the
output is the same either way. The proof starts once BFS has decided the safety cones and runs beside the property
and whole-model passes; its lines print where they would after them. Each z3 process takes
about 0.3 to 0.5 GiB that `--memory` does not count
([Memory, state caps, and threads](#memory-state-caps-and-threads)).

**What a proof changes.** `--prove` reads the state rules only, not `after` assertions or
temporal properties. An invariant cone that BFS stopped at the memory limit before a violation
reads `ok` when z3 proves every rule it checks, and counts as decided:

```
$ rdra-ish verify samples/formal-verification/check_met.rdra --instance-count 4 --memory 1KiB --prove
...
Cone_Forbidden_CartLine_0 [Forbidden_CartLine_0, Check_CartLine_qty_0]: ok (16 states, BFS stopped at the memory limit, decided by proof: --prove proved every check for any number of rows)
```

A cone that also checks an `after` assertion, or a rule left unproved or out of scope, stays
`not decided`, as does a cone stopped at `--max-states`. A proof changes the exit status
under `--deny-unproved`, and through the escalation below. A rule that the schema of `export --kind sql` does not
enforce is marked `[not enforced by the SQL schema: <why>]`, and a `proof: the SQL schema
leaves <n> rules to the application: ...` line counts them; a proved one is an obligation the
use cases already discharge.

**Escalation past the bound.** A rule the proof leaves unproved and BFS passes at the run's
instances is checked again on its own cone at one and then two instances more per entity (each
`--instances` count grows too), after every other BFS run, one rule at a time. Each such run
may hold ten times the store the rule's cone held at the run's bounds, and never more than the
memory BFS has left; the escalation is bounded by memory, with no state count. Each such rule prints one line after the proof's:

```
escalated: Forbidden_Order_0: counterexample at 3 instances (BFS on its cone; ok at 2)
escalated: Forbidden_Item_0: ok at 3 instances, ok at 4 instances (BFS on its cone; ok at 2)
```

A counterexample there is reachable, so it is an error, whether or not `--deny-unproved` is
set. One the cone cannot vouch for, which runs a use case without its event or creates more rows
of an entity than the cone counts, is a candidate: the cone takes in the event's raisers and
counts the entity's slots, and runs again until its counterexample needs neither (at most the
rule's exact cone, which holds every raiser and counts every slot); that cone's instances are
the ones printed. It prints with its trace, whose header names the instances it needed
(`counterexample (4 steps, at 3 instances):`), and the run fails with `<n> invariant or
after-assert violation(s) at more instances than the run's`. The cone line keeps the verdict at
the run's bounds, and TLC is compared with that one. A run stopped by that cap, the memory
limit, or `--max-states` reads `not decided at <n> instances`; it decides nothing and leaves the exit
status alone. The escalation stops at the first counterexample or stop, and at two instances
more.

## Approximations and limitations

How the model differs from the system it describes. What a result's bounds leave unchecked is
in [Known verification gaps](#known-verification-gaps).

- **`.on_delete` runs only as a reject.** A key with no `.on_delete`, `restrict`, or
  `no_action` makes the parent's `deletes` wait until no present child names the row. A step
  removes one child row, so a delete the database would accept once one statement removes
  every child stays disabled while a second child exists. The lifecycle does not run
  `cascade`, `set_null`, or `set_default`: the delete removes the one row and the children
  stay, while a database built from the SQL export runs the action. Such a key gets no
  `DanglingKey_*`; the warning `DeleteLeavesChildren` names each use case that deletes a
  `cascade` parent and not the child, and `DeleteLeavesChildKeys` each that deletes a
  `set_null` or `set_default` parent and neither deletes the child nor moves its key.
- **A use case is atomic per transaction group.** `Pay` that moves an Order and its Payment
  in one group is one step, so `cross_order_payment` passes. A use case whose writes span k
  groups is k steps of one run, with any other action between two of them, so
  `cross_order_payment_split` fails: `Cancel` runs between `Pay_1` and `Pay_2`.
- **Conditions are guards.** A `holds` body is a guard on each use case it applies to. A body
  applied to a use case that writes nothing is the error `UnbindableCondition`.
- **`now` is free.** Nothing pins a column that `now` is compared to, so the clock can tick
  past it (`now_coupon.rdra`).
- **Fairness is weak and on `Next`.** `WF_vars(Next)` is satisfied by another instance's
  endless loop, so a property about one instance fails on a run that moves only the others
  (`temporal_fair.rdra`). `.fair(UC, ...)` adds weak fairness per instance. There is no strong
  fairness: a step enabled and disabled forever is not forced.
- **Joins need keys.** An `.along` path needs a `relate` for each hop, or the rule is the
  error `UnbindableScope`. `has` and `none` range over the instance ids; a `relate` links the
  related rows, and without one the related side is independent.
- **Names.** Multi-entity rules are named `MultiForbidden_*` and `MultiInv_*`.

## Known verification gaps

<!-- derived-from #approximations-and-limitations -->
<!-- derived-from ../samples/patent-ops/README.md#known-verification-gaps -->
<!-- derived-from ../samples/food-permit-ops/README.md#known-verification-gaps -->
<!-- derived-from ../samples/insurance-claim-ops/README.md#known-verification-gaps -->
<!-- constrained-by ./language-reference.md#triggers-and-event-driven-buc-chaining -->
A passing `check` or `verify` says nothing about what it does not explore. Each gap below
says how to see it in a run and what to do. The
[compatibility contract](./compatibility.md#bounds) promises results only within these gaps.

- **Bounds.** A result holds for its instance count and Int range only. `check` and `states`
  run one instance per entity, `verify` two, so a rule that needs two rows to break, such as
  `Unique_*`, holds vacuously in `check`, and one that needs three is not checked by default.
  A deleted slot is never reused. Every Int, Money, Decimal, Date, and DateTime column ranges
  over `0..5`, and `now` stops at the top. A literal or a numeric `@default` outside the
  range is the error `IntLiteralOutOfBounds`. See it: the `bounds:` line and the `OK:` line. What
  to do: raise `--instance-count`, `--instances <Entity>=<n>`, or `--int-range-max`, or prove
  the state rules with `--prove`.
- **Events order by row; a latch counts no events.** A use case an event triggers runs only
  after a raise on its rows (#112). The record is a latch: after the first raise the handler
  runs as often as its guards allow, and a raiser never waits. `triggers(Ev, UC).counted(k)`
  counts the raises of one edge instead ([Counted triggers](./state-derivation.md#counted-triggers)).
  A handler that binds the entity otherwise runs on any raised row (`UnjoinedTrigger`), and
  raisers with no entity in common order nothing (`UnanchoredTrigger`). Timers and external
  events are free. A cone that does not hold every raiser by its own writes leaves the mark
  out, and the handler runs unordered there. An `ok` of a rule, an `after` assertion, or a
  temporal property is still sound. A counterexample that runs the handler without its event
  is a candidate, never reported as one: the cone takes in the raisers of that event and runs
  again, and repeats while a counterexample still runs a handler without its event. It ends at
  the check's exact cone at the latest, which keeps every action that writes into it and so
  holds every raiser
  ([Exploration by cone of influence](./state-derivation.md#exploration-by-cone-of-influence)).
  The refined cone's counterexample is the one reported; if it passes, the check holds, and if it
  stops, the check is `not decided`. A property on such a cone counts only its `ok` and runs on
  the whole model otherwise. An entity's findings are read on its exact cone whenever its own
  cone would leave a mark out, since there the extra handler runs could hide a deadlock, a state
  with no witness, or a use case that never runs. The exact cone can be much larger.
  See it: the infos on the `triggers` line, and the `note:` under a cone line, `on the cone
  refined with the event's raisers: ...`; a TLC trace of the cone's module can still carry the `note:` row `runs <UC>
  without <Ev> having been raised`. What to do: give the handler a from-state the raiser sets,
  make the rule read something the raiser writes, or use `.counted(k)`
  ([`triggers` and event-driven BUC chaining](./language-reference.md#triggers-and-event-driven-buc-chaining)).
- **A cone can stop at the memory limit or `--max-states`.** A stopped cone decides none of
  its witnesses, deadlocks, use cases that never run, arms never taken, or values never
  chosen. A counterexample it found before the stop is reported: BFS reaches only reachable
  states, so it is real (after the exact-cone check above when it runs a use case without its
  event). The cone's other invariants and `after` assertions stay `not decided`, and its line
  reads `not decided` when it found none. See it: `not checked:` on stdout and the
  `warning:` on stderr; `--deny-undecided` fails the run instead. In `check` the stop is the
  error `MemoryCapReached_<E>` or `StateCapReached_<E>`, whose message names the variables the
  states vary in. What to do: raise `--memory`, narrow the cone by moving a guard that reads
  another entity, or try `--symmetry always`. The limit does not bound the whole process
  ([Memory, state caps, and threads](#memory-state-caps-and-threads)).
- **A property its cone cannot decide runs on the whole model under a budget.** See
  [Temporal properties on a cone](#temporal-properties-on-a-cone). See it: `not decided` on the property's line and under `not checked:`, which names the cause
  and the flag to raise. What to do: add `.fair(...)` naming the use cases the property relies
  on, or raise `--temporal-budget`.
- **A scoped run checks no property.** `--buc` drops use cases a property's fairness reads, and
  `--entity` may defer an error inside a property's cone. See it: `temporal properties: 2, not
  checked: a scoped run checks none`. What to do: run `verify` unscoped.
- **`--buc` leaves findings undecided.** The use cases it drops could reach, enable, or unblock
  a state, so missing witnesses, never-run use cases, deadlocks, arms never taken, and values
  never chosen go under `not checked:`. What to do: run without `--buc`, or use `--entity`,
  whose cones are exact. `--actor`, `--without-buc`, and `--without-usecase` assume the other
  use cases never run and decide the findings under that assumption
  ([Partial runs](./cli-reference.md#partial-runs)).
- **Some cones are compared with TLC at 1 instance only.** `engine_equivalence` compares the
  native checker with TLC's state graph on every sample cone, but a cone whose BFS, reduced as
  `verify` explores it, passes 300,000 states at two instances is compared at one instance
  only. `verify` still checks it at two, and `--with-tlc` runs TLC on it. `VERIFY_FALLBACK` in
  `crates/rdra-ish-cli/tests/engine_equivalence.rs` lists these 24 cones:

  | Sample | Cones | States at 2 instances | Why it is large |
  |---|---|---:|---|
  | clinic-ops | entity `EligibilityCheck` | 905,528 | `CheckInPatient` checks the appointment in and creates the check (#80), so they share T. Nothing reduces. |
  | patent-ops | entity `OfficeAction`, invariant `Forbidden_OfficeAction_3` | 1,341,707 reduced | The office action times 185 classes of the application's side, which the examination guards read |
  | food-permit-ops | entity `BusinessPermit` | 4,648,771 reduced | The permit's side times 444 classes of the disposition's and the application's, read through the events of #112 |
  | insurance-claim-ops | entities `Claim`, `Payment`; invariants `Forbidden_Claim_0`, `Forbidden_Claim_4`, `MultiForbidden_Payment_3`; `AfterAssert_PayClaim_0` | 633,787 to 903,987 reduced | The claim and the payment times 2,428 classes of the case, which `CompleteClaim`'s settlement guard reads |
  | insurance-claim-ops | entity `ClaimCase` | reduced | The case's side, reduced by the claim's and the payment's side in 4,368 classes under two bits per case (#112) |
  | oidc-auth | 8 entities and 5 invariants of the browser flow | over 2,000,000 | The request, interaction, session, consent, grant, code, and tokens read each other. Nothing reduces; the sample's header runs `verify` at 1 instance with `--prove`. |

  See it: the list in the test, which fails when a cone joins or leaves it. What to do: run
  `verify --with-tlc` for a second opinion on these cones, and keep a new model's cones under
  300,000 states at two instances if you want the agreement tested.
- **Sample-specific gaps.** insurance-claim-ops cannot check that a payment deadline follows
  its reason, and checks its settlement guard only on the reduced cone
  ([Verification](../samples/insurance-claim-ops/README.md#verification)). food-permit-ops
  checks its renewal guard on large cones. Each sample's README names its gaps: [patent-ops](../samples/patent-ops/README.md#known-verification-gaps),
  [food-permit-ops](../samples/food-permit-ops/README.md#known-verification-gaps), and
  [insurance-claim-ops](../samples/insurance-claim-ops/README.md#known-verification-gaps).

## Samples

The canonical files are in `skills/rdra-ish-verify/samples/`. Each file stands alone, so do not
`check` the directory as one model. `samples/formal-verification/` and
`samples/formal-verification-fail/` link to them, except `unique_double_booking.rdra`,
`unique_booking_guarded.rdra`, and `counting_rules.rdra`, which live only in
`samples/formal-verification/`.

The second column gives `check`'s verdict, then the verdict of `verify`'s cones. A state with no
witness or a deadlock fails both commands even where every cone passes. The `--prove` column
counts the state rules proved. The `// expect` header of each file states the exit status.

| Sample | `check`, `verify` cones | `--prove` | Focus |
|---|---|---|---|
| `order.rdra` | pass, pass | 2 of 2 | Invariants, `after` assertion, `leads_to` and `eventually` under `WF_vars(Next)` |
| `int_stock.rdra` | pass, pass | 1 of 1 | Int columns, arithmetic `forbidden`, an Int temporal property |
| `now_coupon.rdra` | fail, fail | 0 of 1 | `now` and `Tick`: the invariant finds `expired_at < now` |
| `cross_order_payment.rdra` | pass, pass | 1 of 1 | Multi-instance `.along`; `Pay` is one atomic step, so `Cancel` cannot interleave |
| `cross_order_payment_split.rdra` | fail, fail | 0 of 1 | `Pay` through two APIs: `Cancel` runs between `Pay_1` and `Pay_2` |
| `split_run_order.rdra` | pass, pass | 2 of 2 | A use case through two APIs runs as one run in `invokes` order, and finishes under `.fair` |
| `quantifier_none.rdra` | fail, fail | 0 of 1 | `invariant(...).when(...).none(...)`: Revoke while Assign stays active |
| `quantifier_guard.rdra` | pass, pass | 1 of 1 | A `holds(..).none(..)` guard on Revoke makes that rule hold |
| `untriggered_sets.rdra` | fail, fail | 0 of 1 | A `sets` from a use case that drives no transition reaches the forbidden state |
| `event_order.rdra` | pass, pass | no state rule | `triggers` runs IssueCert only after the event is raised on the order, so its `after` assertion needs no guard |
| `after_assert_unmet.rdra` | fail, fail | no state rule | `after(DeliverOrder).assert` without the `sets` that makes it true |
| `temporal_fair.rdra` | pass, fail | no state rule | `leads_to` fails without fairness, since another row amends forever; with `.fair(Register, Dismiss)` it holds |
| `order_stability.rdra` | pass, fail | no state rule | `stable`, `unchanged(..).when(..)`, and `precedes`, each held and broken |
| `temporal_along.rdra` | pass, fail | no state rule | `always` over a Payment and an Order holds with `.along` and fails without it |
| `temporal_unmet.rdra` | pass, fail | no state rule | An `always` and a `leads_to` that fail; the second prints a lasso |
| `one_to_one_shared.rdra` | pass, fail | 0 of 1 | `relate(.., 1:1)` adds `Unique_Shipment_Order`, which takes two instances to break |
| `unique_double_booking.rdra` | pass, fail | 0 of 1 | `@unique(room_id, slot)` breaks when two bookings take one room's morning |
| `unique_booking_guarded.rdra` | pass, pass | 1 of 1 | A `.none(Booking.slot == am)` guard keeps the slot free |
| `counting_rules.rdra` | pass, pass | 2 of 2 | `relate(Loan, Member, 0..3:1)` and `.at_most(3, ..)`; vacuous at the default bounds, and the warning names `--instances Loan=4` |
| `none_any_child.rdra` | pass, pass | 1 of 1 | `.none(Invoice)` reads the link only, so the cone leaves `Invoice.status` out |
| `none_any_child_bypassed.rdra` | pass, fail | 0 of 1 | The same model with an unguarded Reissue |
| `min_card_unmet.rdra` | fail, fail | 0 of 1 | `relate(OrderLine, Order, 1..*:1)`; Place creates an Order without a line |
| `min_card_met.rdra` | pass, pass | 1 of 1 | Place creates the Order and its first line in one transaction |
| `check_unmet.rdra` | fail, fail | 0 of 1 | `@check(qty >= 1)`; ClearQty sets `qty` to 0 |
| `check_met.rdra` | pass, pass | 2 of 2 | The same check kept in range; a `forbidden` reads a Bool `@derived` column |
| `unwitnessed_state.rdra` | fail, pass | 1 of 1 | `refunded` has no witness and `shipped` is a deadlock; the rule's cone passes |
| `deadlock.rdra` | fail, pass | no state rule | An Order stays `paid` forever: Ship's guard holds only while `pending` |
| `can_reach_unmet.rdra` | pass, fail | no state rule | `can_reach` fails: an Order can move between `pending` and `on_hold` forever |
| `can_reach_met.rdra` | pass, pass | no state rule | The same model with Restock, so every state can still finish |
| `partial_scope.rdra` | fail, not run | not run | A `DerivedColumnRead` build error stops both commands; `--entity Order` defers it and passes |
| `fail/order.rdra` | fail, fail | 0 of 1 | A reachable invariant violation |

Under `--with-tlc`, TLC gives the native verdict on every check it runs.

### Sample expectations

Each sample root in `samples.txt` states what `check` and `verify` must report in `// expect`
comment lines. `scripts/verify-samples.sh <binary>` runs both commands on every root,
with no TLC, and fails when a run differs from its header. With `--with-tlc` it runs every
`verify` again beside TLC: each run must match the same header, print `tlc_result:`, and report
no engine disagreement. With `--tlc-roots FILE` as well, TLC runs only on the roots FILE lists
and on every header line that carries flags. CI runs the script on a `shadow` build without
TLC, and in `formal-verify` with `--with-tlc --tlc-roots scripts/tlc-roots.txt` (#305).

The header is read from the root's first `.rdra` file in load order, which `rdra-ish files
<root>` prints first. An `// expect` line in any other file fails the script. The grammar is:

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

- `fail` means exit status 1; its absence, exit status 0. Any other status is a mismatch.
- `fail` names the run's errors, all of them and no others: the `<Name>` of each
  `error[<Name>]:` line, and of TLC's `violated:` line when TLC ran. A name is a rule or
  `after` name (`MultiForbidden_Order_0`, `AfterAssert_DeliverOrder_0`), a property name, a
  `Witness_<Entity>_<column>_<value>`, a `Deadlock_<Entity>_<column>_<value>`, or a build
  error's name ([Diagnostics](./state-derivation.md#diagnostics)).
- `tlc=` names TLC's verdict and must equal the `tlc_result:` line. `tlc=counterexample` also
  needs TLC's own `violated:` line. A line without `tlc=` says nothing of TLC. Without
  `--with-tlc` the script skips a line whose flags hold `--with-tlc`.
- An `error: ...` line without a name, such as a load error, is a mismatch.
- Each `(command, flags)` pair runs once and appears on one line only. A root with no `check`
  line expects `check: ok`, and one with no `verify` line expects `verify: ok`. A root whose
  `verify` lines all carry flags runs only those: use this for a model whose default bounds do
  not finish (`samples/oidc-auth`).

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

The script prints one line per run with the verdict, the wall time, and the maximum RSS, then
the count of roots and runs.

## Engine equivalence in batch JVMs

The `engine_equivalence` test runs TLC several hundred times, mostly on small cones, and keeps
JVMs up between runs to save their start-up. `verify --with-tlc` does not use the batch JVMs.
How they work, what a job shares, and how the two ways are compared are in
[Batch JVMs](./engine-agreement.md#batch-jvms).
