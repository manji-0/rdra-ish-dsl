# Engine agreement

`verify` decides every check with the native checker. TLC (`--with-tlc`) and z3 (`--prove`)
are second opinions. This page states when the three must agree, and describes the
adversarial corpus that checks it: small models, each built around one way the engines
could differ, with the verdict each engine must give written in the model.

**Rule for contributors.** A change to an engine adds a corpus case or updates one. This
covers the native checker, the cone ladder, the reduction, the TLA+ printer, the decoder,
and the z3 encoding. A new case names the trap the change could fall into. An updated case
says in its header why the verdict moved.

## Agreement rules

<!-- constrained-by ./formal-verification.md#temporal-properties-on-a-cone -->
<!-- constrained-by ./formal-verification.md#proofs-for-any-number-of-rows -->

The corpus harness (`crates/rdra-ish-cli/tests/engine_corpus.rs`) checks these rules on
every case, whatever the case's header says.

**Native against TLC**, on `verify --with-tlc`:

1. Each check that both engines decide gets the same verdict. `not decided` from either
   engine is no disagreement, since that engine gave no verdict. `verify --with-tlc`
   starts no TLC run for `stable`, `unchanged`, `precedes`, `can_reach`, or a state-model
   finding, so those have no TLC verdict in the corpus. TLC can check the first three on
   the specs `export` writes, and `engine_equivalence`'s `explored_agrees` compares them
   there; the corpus's `safety_` cases rest on their stated verdicts.
2. The rule holds per run as well as for the final verdict. Each property line names its
   runs (`Cone_P`, `Cone_P_Full`, `Cone_P_WF`, `RdraSpec_P`). The native and TLC lines must
   agree on each run that both decided. A final verdict can agree while two runs disagree
   in ways that cancel.
3. The run prints no `error[EngineDisagreement]` and no `error[LassoFault]`. A native
   counterexample replays through the step relation, fails its formula, and passes its
   fairness check before it prints. A lasso that fails one of these is a tool bug.

**Native against z3**, on `verify --prove` and BFS at 1, 2, and 3 instances:

4. If z3 proves a state rule, BFS finds no violation of it at any tested bound. A proof
   claims every number of rows and every Int value, which includes each bound.
5. If BFS finds a violation, z3 does not prove the rule. This is rule 4 read the other way.
6. A z3 counterexample to induction (CTI) is no disagreement by itself. The induction may be
   too weak, and the CTI's state need not be reachable. The case records `z3=unproved` next
   to the BFS verdicts.

**The native checker against itself**, in process:

7. A graph that a state cap stopped partway never gives `ok`, and a counterexample on it is
   also a counterexample on the full graph. The test
   `a_capped_graph_decides_nothing_the_full_graph_contradicts` explores the whole model of
   every case under every state cap from 1 up to its size. It decides each `always`,
   `eventually`, and `leads_to` under both fairness readings. Under `verify --prove` an
   invariant cone that the memory limit stopped prints `ok` when z3 proves every rule in
   it. That `ok` is z3's verdict, held to rule 4, not the stopped graph's.

**Each engine against the header.** Every case states its expected verdict per engine, so
the corpus also catches a drift that all engines share. Two parts are shared. The cone
ladder decides both the native and the TLC verdict from each engine's runs, so a wrong
step in it moves both together (see [What the corpus catches](#what-the-corpus-catches)).
The model builder feeds every engine, so one mistake there is wrong in all of them at once.
An Int `@default` outside the range once started at 0 in every engine
([#180](https://github.com/manji-0/rdra-ish-dsl/issues/180)), and an Enum or Bool
`@default` outside its values at the first value
([#187](https://github.com/manji-0/rdra-ish-dsl/issues/187)).

### Int columns and `now`

<!-- constrained-by ./formal-verification.md#known-verification-gaps -->

BFS and TLC range Int, Money, Decimal, Date, and DateTime columns over `0..=int_max`
(default 5). `now` starts at 0, and `Tick` runs while `now < int_max`. At the top `Tick`
is disabled: the clock stops there and does not wrap.

z3 has no top. `now` starts at the same base (0). `Tick` always runs and adds 1. A write of
any value (`sets(.., col == present)`, a copy) takes any non-negative number. Both encodings
start from the same base values, because both read the transition system's `Init`. So every
state BFS reaches is a state of the z3 encoding, and a z3 proof covers every bound.

The direction that is allowed to differ is the other one. A rule can fail only past the top
(`expires_at < now` with `expires_at` at 5), so BFS at `0..5` finds nothing and z3 leaves it
unproved. That is no disagreement. `z3_int_and_now_boundary.rdra` and
`z3_now_past_the_top.rdra` pin both sides, and a Date column no use case writes starts at
0 beside `now` in both encodings, so z3 proves `issued_on > now` impossible where BFS finds
nothing.

## The corpus

<!-- derived-from ./formal-verification.md#sample-expectations -->

Each case is one `.rdra` file under `crates/rdra-ish-cli/tests/engine_corpus/`. The file
name starts with the trap's family: `fair_`, `presence_`, `cone_`, `safety_`, `cap_`, or
`z3_`. The harness reads every `*.rdra` file in the directory, in name order. No list needs
an update when you add a file.

### Header grammar

The header is the run of `//` lines at the top of the file. Each line is one of the forms
below. A line that starts with `trap`, `flags`, `tlc`, `expect`, or `known` and does not
parse fails the suite. Other `//` lines are free text that explains the trap.

```
// trap: <what the case checks, in one line>            exactly once
// flags: <verify flags>                                 optional
// tlc: skip, <why>                                      optional; the case never runs TLC
// rejects: <text>                                       optional, repeatable
// expect <Check>: native=<V> tlc=<V> [z3=<Z>] [bfs=<V>,<V>,<V>]
// known <Check>: <engine>=<observed> <issue URL>
```

- `<V>` is `ok`, `counterexample`, `not_decided`, or `none`. `none` means the engine prints
  no verdict for the check. For `tlc`, that covers a form TLC does not check. For `native`,
  it covers a check that should not exist, such as a rule that a build error should stop.
- `<Z>` is `proved`, `unproved`, or `out_of_scope`. Every state rule that `--prove` reports
  needs a `z3=`. `z3=proved` also needs `bfs=`, because a proof must not contradict any of
  the bounded verdicts.
- `bfs=` lists the native verdicts at 1, 2, and 3 instances per entity. The second entry
  equals `native=`. The harness runs `verify --instance-count 1` and `3` only for a case
  with a `bfs=`.
- `<Check>` is the name the run prints: `error[<Check>]`, a `Cone_` line's bracket list, a
  `property <Check> (BFS on its cone)` line, or a `proved:` line. State-model findings
  (`Deadlock_*`, `Witness_*`) are checks too. Every check the run names needs an `expect`
  line. Every `expect` line needs a check in the run, unless its native verdict is `none`
  or a `known` line covers it.
- `rejects` says the model fails to load with an `error:` line that contains `<text>`. A
  case without one fails on any load error, which has no check name. A case with one
  fails when the model loads without that error, or prints a load error that no `rejects`
  line covers. No engine runs on such a model, so every `expect` line says
  `native=none tlc=none`.
- `flags` go to every `verify` run of the case. They must not name `--with-tlc` or
  `--prove`, which the harness adds.

### Known disagreements

A wrong verdict that is not fixed yet stays visible and keeps the suite green. The
`expect` line states the right verdict. A `known` line names the engine, the verdict it
gives today, and the issue that tracks it:

```
// expect Forbidden_Item_0: native=none tlc=none
// known Forbidden_Item_0: native=counterexample https://github.com/manji-0/rdra-ish-dsl/issues/<n>
```

The harness prints each known disagreement on a `known` line. A case fails when the
engine gives a third verdict. It also fails when the engine gives the stated verdict, so
the `known` line goes away in the fix's own change. A known line also covers an
`EngineDisagreement` or `LassoFault` line that names its check.

### Running it

```sh
nix develop -c cargo test --release -p rdra-ish-cli --test engine_corpus -- --nocapture
```

The test prints one line per case: `ok`, `FAIL` with each reason, or `known`. Without
`tlc` or `z3` on `PATH` it skips that engine's verdicts and says so.
`RDRA_REQUIRE_TLC=1` and `RDRA_REQUIRE_Z3=1` make a missing engine a failure, as CI sets
them in the `engine-equivalence` job. The main test's name starts with `bfs_and_tlc_`, so
the debug `test` step skips it, as it skips the other TLC comparisons. The header check
and the capped-graph test run in both steps.

The suite runs four cases at a time. Locally (Apple silicon, with a reduction sweep
loading the machine) it takes about 25 s in release, measured.

### Adding a case

1. Write the smallest model that falls into the trap. Put `// trap:` first, then a few
   lines that say what a wrong engine would answer and why the right answer differs.
2. Reason out the verdict of each check by hand, and write the `expect` lines from that
   reasoning. Do not copy them from a run.
3. Run the suite. A `FAIL` that says a check has no `expect` line lists what the run
   names. Add each one with the verdict you expect.
4. If an engine gives a verdict other than the one you reasoned out, find out who is
   wrong. If the engine is wrong and the fix is not part of your change, open an issue with
   the model and every engine's verdict. Then add a `known` line that links the issue.
5. Check that the case discriminates. Break the engine the way the trap describes, and
   confirm the case fails. Then revert.

## Cases by trap

| Case | Trap | Expected |
|---|---|---|
| `fair_selfloop_not_enabling` | A self-loop is neither a step nor enabling | counterexample (both runs) |
| `fair_enabled_by_leaving_step` | A fair step that leaves the goal-false region still enables | ok |
| `fair_enabled_disabled_forever` | Enabled and disabled forever, alternately, is not forced | counterexample |
| `fair_stop_vs_wf_next` | A stop Fair_P allows and WF_vars(Next) forbids | `PaidShips` ok via `_WF`; `ShippedReopens` counterexample |
| `fair_triggered_handler` | A fair handler keyed by its `Triggered` row | fair ok; unfair counterexample (whole model) |
| `fair_counted_exposes_lost_run` | The latch hides a missing raise that a counted handler exposes | counted counterexample, deadlock, no witness; latch ok |
| `fair_split_use_case` | A split use case is fair per step (`Earlier`) | fair ok; unfair counterexample (whole model) |
| `fair_creating_use_case` | A fair use case that creates (`New`) and runs out of slots | counterexample; voucher-per-bill ok |
| `fair_same_edge_two_bindings` | One edge with a fair and a non-fair label | counterexample |
| `presence_deleted_row_owes_goal` | A deleted row still owes its goal | counterexample |
| `presence_goal_on_base_values` | A goal holds on a deleted row's base values | ok |
| `presence_created_later` | A row triggers from its creation, not before | ok |
| `presence_no_slot_reuse` | No slot reuse, so create and delete cannot loop | ok (whole model) |
| `presence_first_absent_asymmetry` | The lowest absent slot makes slots asymmetric | counterexample on Order#2 only |
| `presence_along_key_moves` | `.along` whose key moves after the trigger | counterexample |
| `cone_receipt_frees_slots` | The receipt model: the cone frees slots, `_WF` ok, whole fails | counterexample (whole model) |
| `cone_handler_mark_left_out` | The cone leaves a handler's raised mark out | counterexample |
| `cone_outside_monotone` | Outside var monotone, or rising and falling with no reuse | ok (whole model) |
| `cone_independent_loop` | An independent loop outside the cone | unfair counterexample; fair ok |
| `cone_copy_from_parent` | A copy from the parent into the cone | ok |
| `cone_reduced_class_loops` | A reduced class loops inside itself and looks like a stop | counterexample |
| `cone_reduced_loop_unreached` | A reduced class can loop, but not from the states the cone reaches; the reduced `_WF` counterexample does not lift | ok |
| `cone_reduced_no_loop_capped` | No class can loop and the unreduced cone passes `--max-states`; `_WF` decides on the reduced cone | ok |
| `cone_reduced_loop_fair_enabled` | A reduced class loops, but a fair step is enabled there; `_WF` decides ok on the reduced cone with marks present | ok |
| `cone_reduced_inner_class_loops` | The loop is inside a class of a nested reduction, invisible to the outer one | counterexample |
| `cone_always_frees_slots` | `always` on a cone that frees slots | ok (whole model) |
| `safety_precedes_latch` | `precedes` reads a latch | ok; never-set latch counterexample |
| `safety_stable_across_delete` | `stable` and `unchanged` across delete and create | ok; `PendingStays` counterexample |
| `safety_can_reach_frees_slots` | `can_reach` where slots run out | counterexample; per-bill ok |
| `safety_vacuity` | A trigger that never holds | ok; `always` counterexample |
| `safety_delete_restrict` | A delete the key's children reject (no `.on_delete`, `restrict`) waits until no child names the row | ok, z3 proved |
| `safety_delete_cascade` | `.on_delete(cascade)` keeps its unguarded one-row delete and gets no `DanglingKey_*`; the line the delete marks stays | counterexample, z3 unproved |
| `safety_delete_set_null` | `.on_delete(set_null)` keeps its unguarded one-row delete and gets no `DanglingKey_*`; the linked line the delete marks stays | counterexample, z3 unproved |
| `safety_counted_once_per_raise` | A counted handler runs once per raise; the latch runs twice after one | counted ok, z3 proved; latch counterexample |
| `safety_event_overflow` | A raise past `k` is `EventOverflow`, and the raiser never waits | `k = 2` counterexample; `k = 3` ok, z3 proved |
| `safety_unique_sets` | A unique column set over column vars, owner vars, and a nullable Int that two rows leave null | single column and junction counterexample; guarded set and null set ok, z3 proved |
| `cap_temporal_budget_zero` | `--temporal-budget 0` | not decided on both engines |
| `cap_memory_limit` | `--memory 2KiB` stops runs partway (native only) | not decided |
| `z3_two_ok_three_fails` | True at 2 instances, false at 3 | z3 unproved; BFS ok, ok, counterexample |
| `z3_inductive` | Inductive rule, beside a real violation | proved / unproved; BFS agrees |
| `z3_cti_true_rule` | A true rule with a CTI across unrelated entities | z3 unproved; BFS ok |
| `z3_lemma` | A `.lemma()` closes that CTI | both proved |
| `z3_int_and_now_boundary` | Int at the top, `now` that stops at the top | as stated per rule |
| `z3_now_past_the_top` | The same `now` rule at Int 0..6 | counterexample; z3 unproved |
| `z3_int_default_out_of_range` | An Int default outside the range | `IntLiteralOutOfBounds`; no engine runs |
| `z3_enum_bool_default_outside_values` | An Enum or Bool default outside its values | the model does not load; no engine runs |

The state-cap trap has no file of its own. It is rule 7, which the capped-graph test
checks on every case's whole model.

## What the corpus catches

<!-- derived-from #agreement-rules -->

When the corpus was written, five deliberate bugs were planted in the checker, one at a
time, and the suite was run on each. Each bug was reverted afterwards.

| Planted bug | Where | Cases that failed | Caught by |
|---|---|---|---|
| A self-loop counts as enabling and as a move | `fair_steps` in `explore/liveness.rs` | `fair_selfloop_not_enabling` | the header, rule 1, rule 2, and `EngineDisagreement` |
| `WF_vars(Next)` stops read as plain stops | `avoid` in `explore/liveness.rs` | 17 of 34 cases and the capped-graph test | `LassoFault` (rule 3), the header, and rule 7 |
| A `_WF` ok is final on any cone, not only one that holds every written var | `after_wf` in `property_cone.rs` | 7 cases (receipt model, handler mark, independent loop, split use case, triggered handler, both cap cases) | the header only: native and TLC share the ladder and agree on the wrong ok |
| A counterexample on a cone that frees slots is taken as the whole model's | `counterexample` in `property_cone.rs` | `cone_always_frees_slots` | the header only: both engines report the cone's counterexample |
| On a capped graph, every state counts as expanded | `expanded` in `explore/liveness.rs` | `cap_memory_limit` and the capped-graph test | `LassoFault` and rule 7 |

The third and fourth rows are the reason every case states its verdicts. The shared ladder
moved seven verdicts in both engines at once, and the shared frees-slots rule moved one.
Rules 1 to 3 saw nothing, and only the stated verdicts failed.

## Mutation run

<!-- derived-from #what-the-corpus-catches -->
<!-- constrained-by ./design/native-temporal-verification.md#phases -->

The planted bugs above were a one-time check. The mutation run repeats it for every
mutant `cargo mutants` makes in `property_cone.rs`, `verify/native.rs`, `cone.rs`, and
`explore/liveness.rs`. `.cargo/mutants.toml` holds the files and the test command, which
is the whole `engine_corpus` test (the stated verdicts, the header check, and the
capped-graph test). The run keeps `tlc` and `z3` off `PATH`, so a mutant is caught only
when a stated verdict or the capped-graph test fails. That is the check that sees a wrong
`ok` the native checker and TLC share through the cone ladder.

The run takes too long for a pull request, so it starts only by hand. Run it at the exit
of phase 4 and again before the release gate (phase 6) of the
[native verification plan](./design/native-temporal-verification.md#phases):

```sh
gh workflow run mutants.yml                 # every mutant, over 4 shards
gh workflow run mutants.yml -f re=liveness  # only the mutants whose name matches
```

The `summary` job lists the `missed` and `timeout` mutants, and each shard's `mutants.out`
is an artifact with each mutant's diff and log. A missed mutant is a change to the checker
that the corpus did not notice. Kill it with a new corpus case that the mutant gets wrong,
or record why it is equivalent to the original code. A timeout is a mutant whose corpus
ran past 60 seconds. Unmutated, the corpus takes about a second, so a timeout means the
mutant made the checker hang. A hang is noticed, so a timeout needs no new case. Each
test process runs under a 2 GiB address-space cap. A mutant that makes the checker
allocate without end fails its test at the cap and counts as caught; without the cap it
fills the runner and the whole shard is cancelled. A shard that uploads no results is
named at the top of the summary, and its mutants are in none of the counts.
