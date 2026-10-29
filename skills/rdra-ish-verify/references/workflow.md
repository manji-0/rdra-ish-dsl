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
rdra-ish verify samples/order.rdra       # needs tlc or tlc2 on PATH
rdra-ish export samples/order.rdra --kind tla -o /tmp/rdra-tla   # the whole-model spec, to read
```

| Command | Role |
|---|---|
| `check`, `states` | BFS on the transition system at one instance per entity; no TLC |
| `verify --backend tlc` | BFS on each entity's cone at the TLC bounds, then TLC on one spec per cone of influence and one per temporal property |
| `export --kind tla` | Write the whole-model `.tla` **and** sibling `.cfg` for reading or a run by hand |

### Output Paths

- `export -o X.tla` names the module `X` and writes `X.cfg` beside it; `-o DIR` writes
  `DIR/RdraSpec.tla` and `DIR/RdraSpec.cfg`.
- `verify -o DIR` writes one `Cone_<first check>.tla` / `.cfg` pair per cone, one
  `RdraSpec_<Property>` pair per temporal property (TLC runs each alone, because it does
  not name the property a temporal counterexample violates), `cones.txt`, and, when the
  model has temporal properties, the whole-model `RdraSpec` pair for a run by hand.
- Without `-o`, `verify` uses a temp dir. A passing run deletes it; a failed run ends
  with `specs: <dir> (kept because a check failed)`.
- `verify --entity <E>` (repeatable) checks only `E`'s view and the invariants and
  after-asserts that read it, each on its exact cone. `verify --buc <B>` keeps only the
  use cases reachable from `B`, so witnesses, deadlocks, and use cases that never run are
  not decided. Both list what they skip under `not checked:`, skip temporal properties,
  and end with `OK: no errors in the selected scope, within ...`.
- When TLC runs out of heap the result is `tool_error`: set `RDRA_TLC_HEAP=4g` (default
  `1g` per cone, `2g` per temporal property run), or put `-Xmx` in `JAVA_TOOL_OPTIONS`,
  which wins.

## Reading a Run

Every result opens with its `bounds:` line, and `OK: no errors within ...` holds only
inside those bounds. Stdout carries one `tlc_result: ok|counterexample|tool_error` line.
Each finding prints its stable name after the severity, so cite that name.

**An invariant or `after` assertion fails.** Each cone line names the checks it answers.
The failure prints first in model terms, the same renderer `check` uses: one step per
use case with the instances it binds and its site, and under each step the values it
changed with the `transitions` arm or `sets` that wrote them. TLC's own `violated:`
line and raw trace follow. Here `Pay` writes through two APIs, so it is two actions, and
`Cancel` runs between them:

```text
$ rdra-ish verify samples/cross_order_payment_split.rdra
bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); 2097152 states per BFS cone; entities a check does not read are unbounded
cones: 1 for 1 invariant(s) and 0 after-assert(s)
temporal properties: none, no whole-model run
state models (BFS on 2 entity cones): 2 state models, 5 states witnessed, 0 deadlocks, 0 use cases that never run
Cone_CrossForbidden_Order_0 [CrossForbidden_Order_0]: counterexample (43 states)
tlc_result: counterexample
Error: TLC verification failed
...
error[CrossForbidden_Order_0]: invariant CrossForbidden_Order_0 is violated: multi-entity forbidden state is reachable across [Order, Payment]: Order.status=cancelled AND Payment.status=captured witnessed by Order[1](status=cancelled); Payment[1](status=captured)
  rule: samples/cross_order_payment_split.rdra:43:1
  counterexample (3 steps, from TLC):
    1. Place [Order#1, Payment#1]                            samples/cross_order_payment_split.rdra:16:1
         + Order#1 (status=pending)
         + Payment#1 (status=pending, owner Order=Order#1)
    2. Pay_1, transaction 1 of Pay [Payment#1]               samples/cross_order_payment_split.rdra:17:1
         Payment#1.status: pending -> captured               samples/cross_order_payment_split.rdra:38:1
    3. Cancel [Order#1]                                      samples/cross_order_payment_split.rdra:18:1
         Order#1.status: pending -> cancelled                samples/cross_order_payment_split.rdra:37:1
violated: Error: Invariant CrossForbidden_Order_0 is violated.
...
```

**A temporal property fails.** It prints the same way under `error[<Property>]`, from
the run of that property alone. TLC's counterexample is a lasso: it ends with `then
nothing changes forever` (a stutter) or `then loops back to step K` (the behavior
repeats steps K to the last one forever). TLC picks the lasso, so two runs may print
different steps. One run of `samples/temporal_unmet.rdra` printed:

```text
error[PendingLeadsToDelivered]: property PendingLeadsToDelivered is violated: its leads_to trigger holds for Order#2 after step 2, and its goal never holds from then on
  rule: samples/temporal_unmet.rdra:48:1
  counterexample (8 steps, from TLC):
    1. PlaceOrder [Order#1]                              samples/temporal_unmet.rdra:15:1
         + Order#1 (status=pending, delivered_at=null)
    2. PlaceOrder [Order#2]                              samples/temporal_unmet.rdra:15:1
         + Order#2 (status=pending, delivered_at=null)
    ...
    6. CancelOrder [Order#2]                             samples/temporal_unmet.rdra:19:1
         Order#2.status: pending -> cancelled            samples/temporal_unmet.rdra:40:1
    7. ReopenOrder [Order#2]                             samples/temporal_unmet.rdra:20:1
         Order#2.status: cancelled -> pending            samples/temporal_unmet.rdra:41:1
    8. CancelOrder [Order#2]                             samples/temporal_unmet.rdra:19:1
         Order#2.status: pending -> cancelled            samples/temporal_unmet.rdra:40:1
    then loops back to step 7
violated: Error: Temporal properties were violated.
```

**A state model fails before TLC.** `verify` first explores each entity's cone by BFS.
A state no reachable state holds (`error[Witness_<E>_<col>_<value>]`) or an instance that
can never leave a state with a way out (`error[Deadlock_<E>_<col>_<value>]`) fails the
run even when TLC passes: `tlc_result: ok`, no `OK:` line, exit 1. A use case that never
runs (`warning[Runs_<action>]`) and a deadlock that only a free instance slot would
clear (`deadlock at the bound`) are warnings:

```text
$ rdra-ish verify samples/deadlock.rdra
bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); 2097152 states per BFS cone; entities a check does not read are unbounded
cones: 0 for 0 invariant(s) and 0 after-assert(s)
temporal properties: none, no whole-model run
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
tlc_result: ok
Error: verification failed: 1 state-model error(s) from BFS on the entity cones
```

**The engines disagree.** `error: TLC step <n> (label <action>) matches no move of the
cone; BFS and TLC disagree (a tool bug)` is a bug in rdra-ish, not a model defect.
Report it with the raw trace that follows.

**Part of a model.** A scoped run lists what it left out, and a build error that no
selected cone reads is deferred rather than failing the run:

```text
$ rdra-ish check samples/partial_scope.rdra --entity Order
bounds: 1 instance per entity, Int 0..5; 4096 states per cone; entities a check does not read are unbounded
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
  (`eventually` / `leads_to`, which need the fairness `WF_vars(Next)` adds).
- An `UnbindableScope` error means a `.along(...)` names entities that no `relate`
  binds; declare the `relate`, or drop `.along` for a rule over every pair.
- Prefer one coherent model per verify run.
