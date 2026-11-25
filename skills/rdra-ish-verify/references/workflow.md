# Verify workflow

<!-- derived-from ../../../docs/cli-reference.md#verify -->
<!-- derived-from ../../../docs/cli-reference.md#export -->
<!-- derived-from ../../../docs/formal-verification.md -->

## Commands

```sh
rdra-ish check samples/order.rdra
rdra-ish verify samples/order.rdra
rdra-ish verify samples/order.rdra --with-tlc
rdra-ish verify samples/order.rdra -o /tmp/rdra-cones
rdra-ish export samples/order.rdra --kind tla -o /tmp/rdra-tla
```

| Command | Does |
|---|---|
| `check` | BFS at one instance per entity; every rule and state-model finding; no `property` |
| `verify` | BFS on each entity's cone at two instances, then the native checker on one cone of influence per group of checks and per temporal property |
| `verify --with-tlc` | The same, with TLC on each cone beside it; a check the two decide differently fails the run |
| `verify -o DIR` | Also writes each cone's `.tla` / `.cfg` pair and `cones.txt`, for a TLC run by hand |
| `export --kind tla` | The whole-model `RdraSpec.tla` and `.cfg` (or `X.tla` for `-o X.tla`), and a `StateModel_<E>` spec per entity cone |

`--instance-count N` and `--int-range-max N` change the bounds. `--instances E=n`
(repeatable) gives one entity its own slot count; `instances = { E = n }` under `[check]` or
`[verify]` in `rdra.toml` does the same. `rdra.toml` refuses an unknown table or key.

## Reading a run

The first line is `bounds:`. `OK: no errors within ...` holds only inside those bounds.
`result: ok|not decided|counterexample|tool_error` is the verdict of the whole run:

| `result:` | Means | Exit |
|---|---|---|
| `ok` | Every check decided and held | 0 |
| `counterexample` | A check, a property, or a state model failed | 1 |
| `not decided` | A cone stopped at the memory limit, `--max-states`, or `--temporal-budget`; the open findings are under `not checked:` | 0 (1 with `--deny-undecided`) |
| `tool_error` | A bug in rdra-ish, or TLC out of heap | 1 |

Cone lines:

- `Cone_<name> [<checks>]: ok (<n> states)` decides every check in the brackets.
- `ok (checked in Cone_<larger>)`: a larger cone that takes every step of this one passed.
- A temporal property's line names its other runs: `Cone_P_WF: ok, needs the whole model's
  WF_vars(Next)` leaves it `not decided` until the `RdraSpec_P [P]:` line that follows.
- `property P (BFS on its cone): ok` or `violated` for `stable`, `unchanged`, `precedes`,
  and `can_reach`.
- `warning[Vacuous_<name>]`: the check passed because what it ranges over never happens.
  Its `ok` says nothing about the model.

An invariant or `after` assertion fails with a shortest counterexample in model terms: one
step per use case run with the rows it binds and its site, and under each step the values
it changed. Here `Pay` writes through two APIs, so it is two steps, and `Cancel` runs
between them:

```text
$ rdra-ish verify samples/cross_order_payment_split.rdra
...
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

An `eventually` or `leads_to` counterexample is a lasso. It ends with `then nothing changes
forever` (a stop) or `then loops back to step K` (steps K to the last repeat forever):

```text
$ rdra-ish verify samples/temporal_unmet.rdra
...
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
result: counterexample
```

`verify` prints the state-model findings of its entity cones first. A `Witness_*` or
`Deadlock_*` error fails the run even when every check holds; read them as
`../../rdra-ish-buc-analyze/references/state-patterns.md` describes.

`error[EngineDisagreement]`, `error[LassoFault]`, and `TLC step <n> ... matches no move of
the cone` are bugs in rdra-ish, not model defects (`result: tool_error`). Report them with
the output and, under `--with-tlc`, the specs directory the run names.

## Scoped runs

- `--entity <E>` (repeatable) checks only `E`'s view and the rules that read it, each on its
  exact cone. `--buc <B>` keeps only the use cases reachable from `B`, so witnesses,
  deadlocks, and use cases that never run are not decided. Both list what they skip under
  `not checked:`, skip temporal properties, and end with `OK: no errors in the selected
  scope, within ...`.
- `--actor <A>`, `--without-buc <B>`, and `--without-usecase <UC>` assume the other use
  cases never run, and still decide every finding under that assumption. Use them when
  another actor's use case (an operator's deletion) hides a deadlock.

```text
$ rdra-ish check samples/partial_scope.rdra --entity Order
bounds: 1 instance per entity, Int 0..5; no state cap per cone, memory 4GiB; entities a check does not read are unbounded
scope: --entity Order
not checked: 1 entity view (Audit), 1 invariant (Forbidden_Audit_1), 0 after-asserts; 1 build error deferred; 1 finding not decided
  deferred error[DerivedColumnRead_Audit_score]: Audit: Audit.score is derived by arithmetic, which the lifecycle cannot evaluate; compare the columns it reads instead
  whether Record can run is not decided: no cone of a selected entity keeps it
OK: no errors in the selected scope, within 1 instance per entity, Int 0..5
```

## Memory, symmetry, and threads

- `verify` runs within `--memory`, else `memory` under `[verify]` in `rdra.toml`, else
  `16GiB`. `check` and `states` use `[check]`, else `4GiB`. There is no state cap unless
  `--max-states <N>` sets one.
- A cone that reaches the limit leaves its findings `not decided: the <E> cone stopped at
  the memory limit`. Narrow the run (`--entity`, `--buc`) or raise `--memory`. The limit
  counts the stored states, not the whole process, which can take up to about twice it:
  give about half the memory you can spare. `check` reports the same stop as the error
  `MemoryCapReached_<E>`, which estimates what finishing needs when it can.
- `verify --symmetry always` (or `symmetry = "always"` under `[verify]`) stores one state
  per permutation of interchangeable rows, so a cone that outgrows the limit can finish. Its
  cone lines read `(about <m> states)`. The default is `never`.
- `RDRA_JOBS=<n>` sets how many cones `check`, `states`, and `verify` explore at once, the
  threads of one large cone, and the z3 processes of `--prove`. By default cones run on the
  cores (at most 8) and a large cone takes a third of the cores (4 to 12). The result does
  not depend on it; lower it when memory is short.
- Under `--with-tlc`, BFS takes half the limit and the TLC runs the rest. A TLC run out of
  heap reruns once at twice its heap, else ends `tool_error`: set `RDRA_TLC_HEAP=4g`.

## Which rules a guard protects

<!-- derived-from ../../../docs/cli-reference.md#guard-dependencies -->

`check --guards` drops each guard in turn (a condition applied to a use case, or the
`.when` conditions of one column's arms), explores again, and prints which rules then fail.
It adds no finding and keeps `check`'s exit status:

```text
$ rdra-ish check samples/split_run_order.rdra --guards
...
guards: 2 guards: 1 protect a rule, 1 no rule depends on; 4 cones explored again
guard: condition Eligible on OpenSettlement   samples/split_run_order.rdra:48:1
  protects: MultiForbidden_Claim_0
guard: the .when arms of Claim.status on Close   samples/split_run_order.rdra:44:1
  no rule depends on this guard; it is either a business rule nobody wrote as a rule, or not needed
rules:
  MultiForbidden_Claim_0 depends on condition Eligible on OpenSettlement
  Unique_Settlement_Claim depends on no guard
OK: no errors within 1 instance per entity, Int 0..5
```

- `protects:` names a rule that fails without the guard. Change the two together.
- `no rule depends on this guard` is common and not a defect. Ask the user: is it a rule
  nobody wrote (propose the `forbidden` or `invariant` it enforces), or a guard the model
  does not need? A guard that serves only a temporal `property` lands here too.
- `not decided` means the cone hit a cap first. Do not count it as protected.
- `depends on no guard`: an effect, a key, or two guards together keep the rule.

Never call a guard wrong because no rule depends on it.
