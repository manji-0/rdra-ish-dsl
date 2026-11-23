# Export and Verify Workflow

<!-- derived-from ../../../docs/cli-reference.md#export -->
<!-- derived-from ../../../docs/cli-reference.md#verify -->
<!-- derived-from ../../../docs/formal-verification.md -->

## Commands

Prefer a bundled sample under this skill's `samples/` directory, run from the skill
root:

```sh
rdra-ish check samples/order.rdra
rdra-ish states samples/order.rdra       # the reachable patterns, optional
rdra-ish verify samples/order.rdra       # native, no JVM
rdra-ish verify samples/order.rdra --with-tlc   # also TLC, from tlc or tlc2 on PATH
rdra-ish export samples/order.rdra --kind tla -o /tmp/rdra-tla   # the whole-model spec, to read
```

| Command | Role |
|---|---|
| `check`, `states` | BFS on the transition system, by default at one instance per entity (`--instance-count`, `--int-range-max` change it); no `property` |
| `verify` | At two instances by default: BFS on each entity's cone, then the native checker on one cone of influence per group of checks and one per temporal property; no JVM |
| `verify --with-tlc` | The same, and TLC on the spec of each cone and property run beside it; a check the two decide differently fails the run |
| `export --kind tla` | Write the whole-model `.tla` **and** sibling `.cfg` for reading or a run by hand, and a `StateModel_<E>` spec per entity cone that checks its deadlocks and witnesses with TLC |

### Output Paths

- `export -o X.tla` names the module `X` and writes `X.cfg` beside it; `-o DIR` writes
  `DIR/RdraSpec.tla` and `DIR/RdraSpec.cfg`. Beside them go `StateModel_<E>.tla`, its
  `.cfg` (a violated `Deadlock_` invariant is a deadlock; a pass is no proof, `verify`
  decides), and `StateModel_<E>_witnesses.cfg`
  (TLC reports every reached state's `Witness_` invariant violated; one it does not report
  is a missing witness; rerun with `-continue` without the reported ones until none is new).
- `verify -o DIR` writes, for a TLC run by hand, one `Cone_<first check>.tla` / `.cfg`
  pair per cone, one `Cone_<Property>` pair per temporal property with a
  `Cone_<Property>_WF` pair beside it for an `eventually` or `leads_to` (TLC does not name
  the property a temporal counterexample violates, so each runs alone), `cones.txt`, and,
  when the model has temporal properties, the whole-model `RdraSpec` pair and a
  `RdraSpec_<Property>` pair for each property its cone leaves undecided. The run labels
  name these modules, whichever engine runs them; the whole-model run stops at
  `--temporal-budget`.
- A finding `verify` leaves undecided (a temporal property past `--temporal-budget`, or a
  state, deadlock, or use case a BFS cone left at its cap) prints a `warning:` and passes.
  In CI, add `--deny-undecided` so it fails instead.
- Without `-o`, `verify` writes no spec. `verify --with-tlc` uses a temp dir: a passing
  run deletes it; a failed run ends with `specs: <dir> (kept because a check failed)`.
- `verify --entity <E>` (repeatable) checks only `E`'s view and the invariants and
  after-asserts that read it, each on its exact cone. `verify --buc <B>` keeps only the
  use cases reachable from `B`, so witnesses, deadlocks, and use cases that never run are
  not decided. Both list what they skip under `not checked:`, skip temporal properties,
  and end with `OK: no errors in the selected scope, within ...`.
- To check what one actor can reach by their own use cases, when another actor's use case
  (an operator's deletion, say) leaves every state and hides a deadlock, run
  `check --actor <A>`, `--without-buc <B>`, or `--without-usecase <UC>`. These assume the
  other use cases never run, decide deadlocks, witnesses, and use cases that never run
  under that assumption, and name the use cases left out on the `scope:` line.
- Under `--with-tlc`, when TLC runs out of heap the result is `tool_error`: set
  `RDRA_TLC_HEAP=4g` (default `1g` per cone, `2g` per temporal property run), or put
  `-Xmx` in `JAVA_TOOL_OPTIONS`, which wins. `verify --with-tlc` starts only as many TLC
  runs at once as their heaps fit in half its memory limit (`--memory`, or `memory` under
  `[verify]` in `rdra.toml`; default `16GiB`).
- A finding listed as `not decided: the <E> cone stopped at the memory limit (...)` means
  the cone's states outgrew BFS's share of that limit (all of it, half under `--with-tlc`). Raise `--memory`, or narrow the cone
  as for the state cap (`verify --max-states <N>` raises it); `check` reports the same stop as `MemoryCapReached_<E>` (default
  limit `4GiB`, `memory` under `[check]`).
- `verify` explores the cones side by side on the machine's cores
  (at most 8). `RDRA_JOBS=2` lowers that when memory is short; the result does not change.

## Reading a Run

Every result opens with its `bounds:` line, and `OK: no errors within ...` holds only
inside those bounds. Every run carries one `result: ok|not decided|counterexample|tool_error` line, the verdict of
the whole run: `ok` means exit 0 with nothing undecided, and a state-model error fails it as
`counterexample` does. `--with-tlc` adds TLC's own `tlc_result:` line after it.
Each finding prints its stable name after the severity, so cite that name.

**An invariant or `after` assertion fails.** Each cone line names the checks it answers.
The failure prints first in model terms, the same renderer `check` uses: one step per
use case with the instances it binds and its site, and under each step the values it
changed with the `transitions` arm or `sets` that wrote them; the trace is the shortest one BFS
found. Under `--with-tlc`, TLC's counterexample, its `violated:` line, and its raw trace
follow under a `tlc: <module> (<path>):` line. Here `Pay` writes through two APIs, so it is
two actions, and `Cancel` runs between them:

```text
$ rdra-ish verify samples/cross_order_payment_split.rdra
bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); no state cap per BFS cone, memory 16GiB; entities a check does not read are unbounded
cones: 1 for 1 invariant(s) and 0 after-assert(s)
temporal properties: none
state models (BFS on 2 entity cones): 2 state models, 5 states witnessed, 0 deadlocks, 0 use cases that never run
Cone_MultiForbidden_Order_0 [MultiForbidden_Order_0]: counterexample (31 states)
error[MultiForbidden_Order_0]: invariant MultiForbidden_Order_0 is violated: multi-entity forbidden state is reachable across [Order, Payment]: Order.status=cancelled AND Payment.status=captured witnessed by Order[1](status=cancelled); Payment[1](status=captured)
  rule: samples/cross_order_payment_split.rdra:44:1
  counterexample (3 steps):
    1. Place [Order#1, Payment#1]                            samples/cross_order_payment_split.rdra:17:1
         + Order#1 (status=pending)
         + Payment#1 (status=pending, owner Order=Order#1)
    2. Pay_1, transaction 1 of Pay [Payment#1]               samples/cross_order_payment_split.rdra:18:1
         Payment#1.status: pending -> captured               samples/cross_order_payment_split.rdra:39:1
    3. Cancel [Order#1]                                      samples/cross_order_payment_split.rdra:19:1
         Order#1.status: pending -> cancelled                samples/cross_order_payment_split.rdra:38:1
result: counterexample
Error: verification failed: 1 invariant or after-assert violation(s)
```

**A temporal property fails.** It prints the same way under `error[<Property>]`, from
the run of that property alone. An `eventually` or `leads_to` counterexample is a lasso:
it ends with `then nothing changes forever` (a stop) or `then loops back to step K` (the
behavior repeats steps K to the last one forever). `samples/temporal_unmet.rdra` prints:

```text
error[PendingLeadsToDelivered]: property PendingLeadsToDelivered is violated: its leads_to trigger holds for Order#1 after step 1, and its goal never holds from then on
  rule: samples/temporal_unmet.rdra:48:1
  counterexample (3 steps):
    1. PlaceOrder [Order#1]                     samples/temporal_unmet.rdra:15:1
         + Order#1 (status=pending)
    2. CancelOrder [Order#1]                    samples/temporal_unmet.rdra:19:1
         Order#1.status: pending -> cancelled   samples/temporal_unmet.rdra:40:1
    3. ReopenOrder [Order#1]                    samples/temporal_unmet.rdra:20:1
         Order#1.status: cancelled -> pending   samples/temporal_unmet.rdra:41:1
    then loops back to step 2
```

**A state model fails.** `verify` also explores each entity's cone by BFS and
prints what it finds first.
A state no reachable state holds (`error[Witness_<E>_<col>_<value>]`) or an instance that
can never leave a state with a way out (`error[Deadlock_<E>_<col>_<value>]`) fails the
run even when every check holds: `result: counterexample`, no `OK:` line, exit 1. A use case that never
runs (`warning[Runs_<action>]`) and a deadlock that only a free instance slot would
clear (`deadlock at the bound`) are warnings. When staying at a state is correct, such as
a stock row waiting at `free` to be used, declare it with `@end(free)` on the column
instead of adding a transition out:

```text
$ rdra-ish verify samples/deadlock.rdra
bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); no state cap per BFS cone, memory 16GiB; entities a check does not read are unbounded
cones: 0 for 0 invariant(s) and 0 after-assert(s)
temporal properties: none
error[Deadlock_Order_status_paid]: deadlock: Order#1 stays at status = paid forever, and paid is not an end state
  leaves paid: Ship (paid -> shipped)   samples/deadlock.rdra:29:1
    guard: condition Packed             samples/deadlock.rdra:33:1
  reached by (2 steps):
    1. Place [Order#1]                     samples/deadlock.rdra:12:1
         + Order#1 (status=pending)
    2. Pay [Order#1]                       samples/deadlock.rdra:13:1
         Order#1.status: pending -> paid   samples/deadlock.rdra:28:1
warning[Runs_Ship]: use case Ship can never run: each of its guards holds in some reachable state, but never all of them in one
  declared: use case Ship   samples/deadlock.rdra:14:1
state models (BFS on 1 entity cone): 1 state model, 3 states witnessed, 1 deadlock, 1 use case that never runs
nothing to check
result: counterexample
Error: verification failed: 1 state-model error(s) from BFS on the entity cones
```

**The engines disagree, or a counterexample fails its own check.**
`error[EngineDisagreement]: ...` (under `--with-tlc`), `error: TLC step <n> (label
<action>) matches no move of the cone; BFS and TLC disagree (a tool bug)`, and
`error[LassoFault]: ...` are bugs in rdra-ish, not model defects, and the run ends with
`result: tool_error`. Report them with the output and the specs directory it names.

**Part of a model.** A scoped run lists what it left out, and a build error that no
selected cone reads is deferred rather than failing the run:

```text
$ rdra-ish check samples/partial_scope.rdra --entity Order
bounds: 1 instance per entity, Int 0..5; 65536 states per cone; entities a check does not read are unbounded
scope: --entity Order
not checked: 1 entity view (Audit), 1 invariant (Forbidden_Audit_1), 0 after-asserts; 1 build error deferred; 1 finding not decided
  deferred error[DerivedColumnRead_Audit_score]: Audit: Audit.score is derived by arithmetic, which the lifecycle cannot evaluate; compare the columns it reads instead
  whether Record can run is not decided: no cone of a selected entity keeps it
OK: no errors in the selected scope, within 1 instance per entity, Int 0..5
```

## Reporting Tips

- Cite the name after the severity; it equals the `INVARIANT` or `PROPERTY` name in the
  `.cfg`.
- Distinguish invariants (`forbidden`, `invariant`, keys, `@check`) from liveness
  (`eventually` / `leads_to`, which need the fairness `WF_vars(Next)` adds). A liveness
  `ok` under `.fair(...)` holds only on runs that do not starve the listed use cases; say so,
  as `verify` does in its `<P> assumes weak fairness per instance on ...` line.
- An `UnbindableScope` error means a `.along(...)` names entities that no `relate`
  binds; declare the `relate`, or drop `.along` for a rule over every pair.
- Prefer one coherent model per verify run.
