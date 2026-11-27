# Engine agreement

This page is for contributors who change an engine: the native checker, the cone ladder,
the reduction, instance symmetry, the TLA+ printer and decoder, or the z3 encoding. It
states when the native checker, TLC (`verify --with-tlc`), and z3 (`verify --prove`) must
agree, and the tests that hold them to it: the adversarial corpus, the sample comparison,
the reduction sweep, the mutation run, and the Kani harnesses. What each engine decides is
in [formal-verification.md](./formal-verification.md).

**Rule for contributors.** A change to an engine adds a corpus case or updates one. A new
case names the trap the change could fall into. An updated case says in its header why the
verdict moved.

## Agreement rules

<!-- constrained-by ./formal-verification.md#temporal-properties-on-a-cone -->
<!-- constrained-by ./formal-verification.md#proofs-for-any-number-of-rows -->
<!-- constrained-by ./formal-verification.md#state-models-with-tlc-alone -->

The corpus harness (`crates/rdra-ish-cli/tests/engine_corpus.rs`) checks these rules on every
case, whatever the case's header says.

**Native against TLC**, on `verify --with-tlc`:

1. Each check that both engines decide gets the same verdict. `not decided` from either
   engine is no disagreement. TLC has no run for `can_reach` (`AG EF`) or for a use case
   that never runs. The other checks that BFS decides alone get a TLC run of their own:
   - `stable`, `unchanged`, and `precedes` run on the unreduced cone (`Cone_<Property>`).
     When that cone frees slots, a counterexample there goes to the whole-model run
     (`RdraSpec_<Property>`) in both engines. The final verdicts must agree.
   - The state models of each entity cone that BFS explored to the end, up to 100,000
     states, run on the `StateModel_<E>` specs
     ([State models with TLC alone](./formal-verification.md#state-models-with-tlc-alone)).
     TLC must name the same states without a witness as BFS. A deadlock TLC reports must be
     one BFS reports, as an error or, at the instance bound, as a warning. A deadlock TLC does
     not report has no TLC verdict, since a cycle in the column's cone hides it; the corpus
     states it as `tlc=none`.
2. The rule holds per run as well as for the final verdict. Each property line names its runs
   (`Cone_P`, `Cone_P_Full`, `Cone_P_WF`, `RdraSpec_P`), and the two engines must agree on
   each run both decided on the same system: the same graph, reduced or not, under the same
   fairness. Two runs can disagree in ways that cancel. The native `WF_vars(Next)` run on a
   reduced cone, noted `WF_vars(Next) on the reduced cone (native only)`, has no TLC run on
   its system, and meets TLC's `Cone_P_WF` only in the final verdict. A native `Cone_` line
   whose checks the header gives a TLC verdict needs TLC's line for the same module; a
   native `RdraSpec_` line may stand alone, since TLC's cone may decide what the native cone
   left to the whole model.
3. The run prints no `error[EngineDisagreement]` and no `error[LassoFault]`. A native
   counterexample replays through the step relation, fails its formula, and passes its
   fairness check before it prints.

**Native against z3**, on `verify --prove` and BFS at 1, 2, and 3 instances:

4. If z3 proves a state rule, BFS finds no violation of it at any tested bound.
5. If BFS finds a violation, z3 does not prove the rule. A rule BFS violates at the run's
   bounds is not tried: the run prints `refuted:` for it, and the case records `z3=refuted`
   with `native=counterexample`, and only with it.
6. A z3 counterexample to induction (CTI) is no disagreement by itself: the induction may be
   too weak, and the CTI's state need not be reachable. The case records `z3=unproved`.

**The native checker against itself**, in process:

7. A graph a state cap stopped partway never gives `ok`, and a counterexample on it is one on
   the full graph. `a_capped_graph_decides_nothing_the_full_graph_contradicts` explores the
   whole model of every case under every state cap from 1 up to its size, and decides each
   `always`, `eventually`, and `leads_to` under both fairness readings. Under `--prove`, an
   invariant cone the memory limit stopped prints `ok` when z3 proves every rule in it; that
   `ok` is z3's, held to rule 4.

**The native checker up to instance symmetry**, on `verify --symmetry always`:

8. With the test-only `RDRA_ISH_FORCE_ORBITS=1`, which runs up to symmetry every cone a plain
   run would otherwise decide, once with reduction off (`RDRA_ISH_NO_REDUCE=1`) and once with
   it on, each check the plain run decides gets the plain run's verdict, not `not decided`, at
   2 instances and at every bound the case runs BFS at. A lifted counterexample that does not replay, or whose steps do
   not bind the rows they name, panics.

**The native checker past the run's bounds**, on `verify --prove`:

9. A rule z3 leaves unproved and BFS passes is checked again at more instances
   ([Escalation past the bound](./formal-verification.md#proofs-for-any-number-of-rows)). Each
   verdict the `escalated:` line decides at `n` instances is the `bfs=` entry for `n`, and the
   run prints the escalation's error, whose trace says `at <n> instances`, exactly when the
   line has a counterexample. That error is not the native verdict at the run's bounds.

**Each engine against the header.** Every case states its expected verdict per engine, so the
corpus also catches a drift all engines share. The cone ladder decides both the native and the
TLC verdict, and the model builder feeds every engine, so one mistake in either moves every
engine at once ([What the corpus catches](#what-the-corpus-catches)). An Int `@default`
outside the range once started at 0 in every engine (#180), and an Enum or Bool `@default`
outside its values at the first value (#187).

### Int columns and `now`

<!-- constrained-by ./formal-verification.md#known-verification-gaps -->

BFS and TLC range Int, Money, Decimal, Date, and DateTime columns over `0..=int_max` (default
5). `now` starts at 0, and `Tick` runs while `now < int_max`, so the clock stops at the top.

z3 has no top. `now` starts at 0 and `Tick` always runs. A write of any value takes any
non-negative number. Both encodings read the same `Init`, so every state BFS reaches is a
state of the z3 encoding, and a z3 proof covers every bound.

The other direction may differ. A rule that fails only past the top (`expires_at < now` with
`expires_at` at 5) passes BFS at `0..5` and stays unproved in z3. `z3_int_and_now_boundary`
and `z3_now_past_the_top` pin both sides.

## The corpus

<!-- derived-from ./formal-verification.md#sample-expectations -->

Each case is one `.rdra` file under `crates/rdra-ish-cli/tests/engine_corpus/`. The file name
starts with the trap's family: `fair_`, `presence_`, `cone_`, `safety_`, `sym_`, `cap_`, or
`z3_`. The harness reads every `*.rdra` file in the directory. `CASES` in the same directory
lists every case's file stem, and `every_corpus_header_is_well_formed` fails when the two
differ, so a deleted or renamed case does not go unnoticed.

### Header grammar

The header is the run of `//` lines at the top of the file. A line that starts with `trap`,
`flags`, `tlc`, `expect`, or `known` and does not parse fails the suite. Other `//` lines are
free text that explains the trap.

```
// trap: <what the case checks, in one line>            exactly once
// flags: <verify flags>                                 optional
// tlc: skip, <why>                                      optional; the case never runs TLC
// rejects: <text>                                       optional, repeatable
// expect <Check>: native=<V> tlc=<V> [z3=<Z>] [bfs=<V>,<V>,<V>[,<V>]]
// known <Check>: <engine>=<observed> <issue URL>
```

- `<V>` is `ok`, `counterexample`, `not_decided`, or `none`. `none` means the engine prints no
  verdict for the check: for `tlc`, a form TLC does not check or a deadlock TLC does not
  report; for `native`, a check that should not exist, such as a rule a build error stops, or a
  deadlock BFS reports only as a warning.
- `<Z>` is `proved`, `unproved`, `out_of_scope`, or `refuted`. Every state rule `--prove` reports needs a
  `z3=`, and `z3=proved` also needs `bfs=`.
- `bfs=` lists the native verdicts at 1, 2, and 3 instances per entity, and at 4 with a fourth
  entry. The second entry equals `native=`. The harness runs 1 and 3 instances only for a case
  with a `bfs=`, and 4 only for a four-entry one, with TLC beside BFS at each: rules 1 to 3
  hold there too, so TLC's verdict at that bound equals the `bfs=` entry wherever both
  engines decide.
- `<Check>` is the name the run prints: `error[<Check>]`, a `Cone_` line's bracket list, a
  `property <Check> (BFS on its cone)` line, a `proved:` line, or a `Deadlock_*` or `Witness_*`
  finding. Every check the run names needs an `expect` line, and every `expect` line needs a
  check in the run, unless its native verdict is `none` or a `known` line covers it.
- `rejects` says the model fails to load with an `error:` line that contains `<text>`. No
  engine runs on it, so every `expect` line says `native=none tlc=none`.
- `flags` go to every `verify` run of the case. They must not name `--with-tlc` or `--prove`,
  which the harness adds.

### Known disagreements

A wrong verdict that is not fixed yet stays visible and keeps the suite green. The `expect`
line states the right verdict, and a `known` line names the engine, the verdict it gives today,
and the issue:

```
// expect Forbidden_Item_0: native=none tlc=none
// known Forbidden_Item_0: native=counterexample https://github.com/manji-0/rdra-ish-dsl/issues/<n>
```

The harness prints each as a `known` line. A case fails when the engine gives a third verdict,
or the stated one, so the `known` line goes away in the fix's own change. A `known` line also
covers an `EngineDisagreement` or `LassoFault` line that names its check. No case has a `known`
line today.

### Running it

```sh
nix develop -c cargo test --release -p rdra-ish-cli --test engine_corpus -- --nocapture
```

The test prints one line per case: `ok`, `FAIL` with each reason, or `known`. Without `tlc` or
`z3` on `PATH` it skips that engine's verdicts and says so; `RDRA_REQUIRE_TLC=1` and
`RDRA_REQUIRE_Z3=1` make a missing engine a failure, as CI sets them.
`RDRA_CORPUS_CASE=<text>` runs only the cases whose name contains `<text>`. The main test,
`bfs_and_tlc_and_z3_agree_on_the_adversarial_corpus`, runs four cases at a time; its prefix
keeps it out of CI's debug `test` step. The header check and the capped-graph test run in both
steps.

### Adding a case

1. Write the smallest model that falls into the trap. Put `// trap:` first, then a few lines
   that say what a wrong engine would answer and why the right answer differs.
2. Reason out each check's verdict by hand, and write the `expect` lines from that reasoning,
   not from a run.
3. Run the suite. A `FAIL` that says a check has no `expect` line lists what the run names.
   Add each one.
4. If an engine disagrees with your reasoning, find out who is wrong. If the engine is wrong
   and the fix is not part of your change, open an issue with the model and every engine's
   verdict, and add a `known` line that links it.
5. Check that the case discriminates: break the engine the way the trap describes, confirm the
   case fails, and revert.
6. Add the file stem to `CASES`, in sorted order.

## Cases by trap

Each row gives the case, its trap, its `flags`, and every `expect` line of its header. A verdict
stands for both the native checker and TLC unless it names TLC apart. `cex` is
`counterexample` and `nd` is `not_decided`; `BFS a/b/c[/d]` is the `bfs=` list. `cap_memory_limit`
skips TLC, since `verify` refuses `--with-tlc` under `64MiB`.

| Case | Trap | Flags | Expected |
|---|---|---|---|
| `cap_memory_limit` | The memory limit stops a run partway, which must leave a check not decided, never ok | `--memory 2KiB` | `PaidShipsUnfair` nd, TLC none; `NoShip` cex, TLC none |
| `cap_temporal_budget_zero` | `--temporal-budget 0` stops the whole-model run before it decides | `--temporal-budget 0` | `PaidShipsUnfair` nd; `PaidShips` ok |
| `cone_always_frees_slots` | `always` on a cone that frees slots |  | `NeverD` ok |
| `cone_copy_from_parent` | A fair use case copies its parent's column into the cone |  | `IssueEventually`, `IssueEventuallyPlain` ok |
| `cone_entity_mark_left_out` | An entity cone that leaves a raised mark out, and its state-model findings (P32) |  | `Deadlock_Order_status_placed`, `Deadlock_Payment_status_unpaid`, `Witness_Order_status_shipped`, `Witness_Payment_status_paid`, `Witness_Notice_status_received`, `Witness_Notice_status_read` cex |
| `cone_handler_mark_left_out` | A handler whose raised mark the cone leaves out |  | `ShipsUnfair`, `ShipsFair`, `Deadlock_Order_status_paid` cex |
| `cone_independent_loop` | An independent loop outside the cone keeps the whole model's `WF_vars(Next)` met |  | `PaidShipsUnfair` cex; `PaidShips` ok |
| `cone_outside_monotone` | A var outside the cone that is monotone only because nothing deletes it |  | `PaidShipsStamp`, `PaidShipsNotes` ok |
| `cone_receipt_frees_slots` | The receipt model: a cone that frees slots passes with `WF_vars(Next)`, and the whole model fails |  | `PendingPays` cex |
| `cone_reduced_class_loops` | A reduced class that loops inside itself looks like a stop |  | `JobStarts`, `JobStartsUnfair`, `JobWaits` cex |
| `cone_reduced_inner_class_loops` | The loop is inside a class of a nested reduction, invisible to the outer one | `--instance-count 1` | `JobStarts` cex |
| `cone_reduced_loop_fair_enabled` | A reduced class loops, but the fair step is enabled there, so it is no place to stop | `--instance-count 1` | `JobStarts` ok |
| `cone_reduced_loop_unreached` | A reduced class can loop, but not from the states the cone reaches it in | `--instance-count 1` | `SeenFinishes` ok; `Deadlock_Gate_phase_pa`, `Deadlock_Job_status_waiting` cex |
| `cone_reduced_no_loop_capped` | A reduced sub-lifecycle with no loop, and an unreduced cone past the cap | `--max-states 300` | `JobStarts` ok |
| `fair_counted_exposes_lost_run` | The latch hides a missing raise that counting exposes |  | `OrderComplete`, `Deadlock_Order_delivered_partial`, `Witness_Order_delivered_complete` cex; `ParcelComplete` ok; `EventOverflow_Order_EvDispatched_Deliver` ok, z3 proved, BFS ok/ok/ok |
| `fair_creating_use_case` | A fair use case that creates (a `New` parameter) and runs out of slots |  | `PendingPays` cex; `BillPays` ok |
| `fair_enabled_by_leaving_step` | A fair instance whose only step leaves the goal-false region is still enabled |  | `PaidShips` ok |
| `fair_enabled_disabled_forever` | An action enabled and disabled forever, alternately, is not forced by weak fairness |  | `PaidShips` cex |
| `fair_same_edge_two_bindings` | One edge explained by a fair action and a non-fair one counts for the fair one |  | `LoopsForever` cex |
| `fair_selfloop_not_enabling` | A self-loop is neither a step nor enabling |  | `PaidShips` cex |
| `fair_split_use_case` | A split use case is fair per step, the later step keyed by the earlier row (`Earlier`) |  | `Ends` ok; `EndsUnfairSplit` cex; `Unique_Settlement_Claim` ok, z3 proved, BFS ok/ok/ok |
| `fair_stop_vs_wf_next` | A stop that `Fair_P` allows and `WF_vars(Next)` rules out |  | `PaidShips` ok; `ShippedReopens` cex |
| `fair_triggered_handler` | A fair handler keyed by the row its event was raised for (`Triggered`) |  | `PostedFair` ok; `PostedUnfair` cex; `Deadlock_Ledger_state_open` none |
| `presence_along_key_moves` | `.along` with a key that moves after the trigger |  | `CapturedPaid`, `Deadlock_Order_status_open` cex |
| `presence_created_later` | A row created later triggers from the state that creates it, not before |  | `EventuallyDone` ok |
| `presence_deleted_row_owes_goal` | A row deleted after its trigger still owes its goal, read on its base values |  | `PendingPays` cex |
| `presence_first_absent_asymmetry` | Creation takes the lowest absent slot, so the slots are not symmetric |  | `RushFinishes`, `Deadlock_Order_status_open` cex |
| `presence_goal_on_base_values` | A goal holds on the base values of a deleted row |  | `HeldReleased` ok |
| `presence_goal_present_both_ok` | `.present()` changes nothing where no delete can come before the goal |  | `ReviewApproved`, `ReviewApprovedPresent`, `EventuallyApproved`, `EventuallyApprovedPresent` ok |
| `presence_goal_present_modifier` | `.present()` keeps a deleted row from meeting a goal its base values satisfy |  | `HeldReleased` ok; `HeldReleasedPresent` cex |
| `presence_no_slot_reuse` | A deleted slot is never reused, so creating and deleting cannot go on forever |  | `PaidShips` ok |
| `presence_per_entity_slots` | Per-entity slot counts give each entity its own slot set, in both directions | `--instance-count 1 --instances Loan=3 --instances Room=3` | `Unique_Loan_tag`, `Unique_Room_tag` cex, z3 unproved; `Unique_Member_tag`, `Unique_Booking_tag` ok, z3 unproved |
| `safety_at_least_guard` | An `at_least` guard and the rule it keeps, against a delete |  | `Quantifier_Claim_0` cex, z3 unproved, BFS ok/cex/cex; `Quantifier_Order_1` ok, z3 proved, BFS ok/ok/ok; `Deadlock_Claim_status_open`, `Deadlock_Order_status_open` cex |
| `safety_at_most_bound` | A count bound the instance count hides: the rule needs four rows to fail |  | `Quantifier_Member_0` ok, z3 unproved, BFS ok/ok/ok/cex; `Quantifier_Shelf_1` ok, z3 proved, BFS ok/ok/ok/ok |
| `safety_can_reach_frees_slots` | `can_reach` on a cone that would free slots |  | `OrderCanPay` cex, TLC none; `BillCanPay` ok, TLC none |
| `safety_count_guard_order` | Two count guards that point opposite ways on one row |  | `Forbidden_Shelf_0` ok, z3 proved, BFS ok/ok/ok; `Deadlock_Shelf_status_open` cex |
| `safety_count_per_entity` | A count over an entity with its own slot count, in both directions | `--instances Loan=4 --instances Shelf=3` | `Quantifier_Member_0`, `Quantifier_Shelf_1` cex, z3 unproved |
| `safety_counted_once_per_raise` | A counted handler runs once per raise, where the latch lets one raise drive two runs |  | `Forbidden_Box_0`, `EventOverflow_Box_EvShipped_Receive` ok, z3 proved, BFS ok/ok/ok; `Forbidden_Crate_1` cex, z3 unproved; `BoxBothIn` ok |
| `safety_created_set_beside_transitions` | A guarded `sets` and `transitions` write one created row's lifecycle column in one use case, which the engines read differently (P24); the build rejects it |  | `ConflictingEffects_Order_Place_status` cex, TLC none; `Forbidden_Order_0` none |
| `safety_delete_cascade` | `.on_delete(cascade)` keeps its one-row delete, with no guard and no `DanglingKey_*` |  | `Forbidden_OrderLine_0` cex, z3 unproved |
| `safety_delete_restrict` | A delete the key's children reject waits until no present child names the row |  | `DanglingKey_OrderLine_Order`, `DanglingKey_Payment_Order` ok, z3 proved, BFS ok/ok/ok |
| `safety_delete_set_null` | `.on_delete(set_null)` keeps its one-row delete, with no guard and no `DanglingKey_*` |  | `Forbidden_OrderLine_0` cex, z3 unproved |
| `safety_edge_free_cone` | A cone of checks alone keeps no edge list (#296 2.1) |  | `AfterAssert_Deliver_0`, `AfterAssert_Refund_1` ok; `Forbidden_Ticket_0` cex, z3 unproved; `Witness_Order_status_refunded` cex |
| `safety_event_overflow` | A raise past `k` is `EventOverflow`, and the raiser never waits for it |  | `EventOverflow_Inbox_EvNoted_Read` cex, z3 unproved; `EventOverflow_Outbox_EvQueued_Send` ok, z3 proved, BFS ok/ok/ok |
| `safety_exactly_along` | Counts along a path, and `exactly` in a guard and a rule |  | `Quantifier_Member_0` ok, z3 proved, BFS ok/ok/ok; `Quantifier_Member_1`, `Quantifier_Member_2` ok, z3 unproved, BFS ok/ok/cex; `Deadlock_Member_status_pending` cex |
| `safety_max_card_range` | A card's high end above 1 (`0..2:1`) adds `MaxCard_Copy_Shelf` |  | `MaxCard_Copy_Shelf` ok, z3 unproved, BFS ok/ok/cex |
| `safety_null_columns_cross` | A cross-entity comparison of two null columns (#355) |  | `MultiForbidden_Line_0` ok, z3 proved, BFS ok/ok/ok; `MultiForbidden_Line_1`, `MultiForbidden_Line_2` cex, z3 unproved |
| `safety_null_columns_eq` | The reverse of #355: `==` on two null columns never holds, and `!=` always does |  | `Forbidden_Item_0`, `Required_Item_0`, `Forbidden_Item_2`, `Required_Item_1` ok, z3 proved, BFS ok/ok/ok; `Forbidden_Item_1` cex, z3 unproved; `LimNeverMet` ok; `ABNeverMet` cex |
| `safety_null_columns_ne` | Two `@null` columns that both hold null are not equal, so `!=` holds and `==` fails (#355) |  | `Forbidden_Item_0`, `Required_Item_0` cex, z3 unproved |
| `safety_null_derived_in` | `in` / `not in` in a logical `@derived` are null while the column is; a list with `null` holds there; rules and guards read a null as in no list (#374) |  | `Forbidden_Item_0`, `Forbidden_Item_1`, `Forbidden_Item_3`, `Forbidden_Item_4`, `Required_Item_0`, `Forbidden_Box_6` ok, z3 proved, BFS ok/ok/ok; `Forbidden_Item_2`, `Forbidden_Item_5`, `Forbidden_Box_7` cex, z3 unproved |
| `safety_null_enum_bool_columns` | Two `@null` Enum or Bool columns that both hold null are not equal; a column with null and one without compare value by value (#359) |  | `Forbidden_Item_0`, `Forbidden_Item_1`, `Forbidden_Item_3`, `Forbidden_Item_4` cex, z3 unproved; `Forbidden_Item_2`, `Required_Item_0` ok, z3 proved, BFS ok/ok/ok |
| `safety_null_enum_bool_literals` | A `@null` Enum or Bool column starts null and holds null as a value (#359) |  | `Forbidden_Item_0`, `Forbidden_Item_3`, `Required_Item_3` cex, z3 unproved; `Forbidden_Item_1`, `Forbidden_Item_2`, `Forbidden_Item_4`, `Forbidden_Item_5`, `Required_Item_0`, `Required_Item_1`, `Required_Item_2`, `Unique_Tag_color` ok, z3 proved, BFS ok/ok/ok |
| `safety_precedes_latch` | `precedes` reads a latch, not the current state |  | `ShippedAfterPaid` ok; `ShippedAfterAudit` cex |
| `safety_stable_across_delete` | `stable` and `unchanged` across delete and create |  | `CapturedStays`, `AmountKept` ok; `PendingStays` cex |
| `safety_stable_frees_slots` | `stable` and `unchanged` on a cone that frees slots |  | `StaysBeforeD`, `CKept` ok |
| `safety_unique_sets` | A unique column set over column vars and owner vars, nulls apart |  | `Unique_Badge_color`, `Unique_Enrollment_course_id_student_id` cex, z3 unproved, BFS ok/cex/cex; `Unique_Booking_room_id_slot`, `Unique_Pass_seat`, `Forbidden_Pass_0` ok, z3 proved, BFS ok/ok/ok |
| `safety_vacuity` | A property whose trigger never holds passes on every engine |  | `DoneOpenCloses` ok; `AlwaysDone` cex |
| `sym_arm_reorders` | A guarded arm whose every move swaps the stored rows | `--instances Board=1` | `Forbidden_Job_0` ok, z3 proved, BFS ok/ok/ok |
| `sym_deleted_row_owner` | A deleted row keeps its key, and a later step reads it, so the key must move with its parent's row | `--instances Mall=1` | `Quantifier_Mall_0` cex, z3 unproved; `DanglingKey_Item_Shop` ok, z3 proved, BFS ok/ok/ok |
| `sym_first_absent` | A created row takes the first absent slot, and the group fixes absent slots |  | `Forbidden_Ticket_0` ok, z3 proved, BFS ok/ok/ok |
| `sym_identity_pair` | A pre-existing 1:1 child starts at its parent's slot, so the two permute together |  | `MultiForbidden_Account_0` cex, z3 unproved; `Unique_Balance_Account` ok, z3 proved, BFS ok/ok/ok |
| `sym_lift_compose` | A counterexample whose rows the canonical form reorders at several steps | `--instances Queue=1` | `Quantifier_Queue_0` cex, z3 unproved, BFS ok/cex/cex |
| `sym_master_ties` | Identical pre-existing rows, some named by children and some not |  | `MultiForbidden_Visit_0` cex, z3 unproved; `Deadlock_Visit_status_booked` cex |
| `sym_owner_coupled` | A rule that holds only because a child's key moves with its parent's row |  | `MultiForbidden_Order_0` ok, z3 proved, BFS ok/ok/ok |
| `sym_per_entity_bounds` | Per-entity bounds give two classes different slot counts | `--instances Room=3 --instances Booking=2` | `MultiForbidden_Booking_0` ok, z3 proved, BFS ok/ok/ok |
| `sym_relink_loop` | A move into the class of its own source, a loop the stored graph keeps as a loop | `--instances Ticket=1` | `TicketCloses`, `TicketClosesFair` cex |
| `sym_self_owned` | An entity whose key names its own rows |  | `Forbidden_Task_0` ok, z3 proved, BFS ok/ok/ok; `Deadlock_Task_status_open` cex |
| `z3_cti_true_rule` | A true rule that is not inductive alone; no mined lemma relates its rows, and z3 proves it with a lemma generalized from its CTI (#428) |  | `MultiForbidden_Light_0` ok, z3 proved, BFS ok/ok/ok; `Deadlock_Door_state_closed`, `Deadlock_Light_state_on` cex |
| `z3_enum_bool_default_outside_values` | An Enum or Bool default outside its values; the model does not load |  | `Forbidden_Item_0` none |
| `z3_inductive` | An invariant z3 proves by induction, beside a real violation |  | `Forbidden_Order_0` ok, z3 proved, BFS ok/ok/ok; `Forbidden_Order_1` cex, z3 unproved, BFS cex/cex/cex |
| `z3_int_and_now_boundary` | Int columns and `now` at the top of the range |  | `Forbidden_Item_0`, `Forbidden_Coupon_3` ok, z3 proved, BFS ok/ok/ok; `Forbidden_Item_1` cex, z3 unproved, BFS cex/cex/cex; `Forbidden_Coupon_2` ok, z3 unproved, BFS ok/ok/ok |
| `z3_int_default_out_of_range` | An Int default outside the range, which every engine would read the same wrong way |  | `IntLiteralOutOfBounds_Item_7` cex, TLC none; `Forbidden_Item_0` none |
| `z3_int_listing_guard` | A guard that lists every Int value up to the top holds of no row at 0..5, but z3 has no top (#425) |  | `Forbidden_Item_0` ok, z3 unproved, BFS ok/ok/ok; `Deadlock_Item_status_open` cex, TLC none; `Witness_Item_status_flagged` cex |
| `z3_int_listing_past_the_top` | The rule of `z3_int_listing_whole_range` with two more values of range | `--int-range-max 7` | `Forbidden_Item_0` cex, z3 refuted, BFS cex/cex/cex |
| `z3_int_listing_whole_range` | A rule that lists every Int value up to the top holds at 0..5, but z3 has no top (#425) |  | `Forbidden_Item_0` ok, z3 unproved, BFS ok/ok/ok |
| `z3_lemma` | A `.lemma()` rule supplies the fact a CTI needs, and the rule is proved |  | `MultiForbidden_Light_0`, `MultiForbidden_Light_1` ok, z3 proved, BFS ok/ok/ok; `Deadlock_Door_state_closed`, `Deadlock_Light_state_on` cex |
| `z3_lemma_run_mark` | A `.lemma()` that reads a run mark (`Door.in_run(Open)`) supplies the fact a CTI needs, and the rule is proved (#428) |  | `MultiForbidden_Light_0` ok, z3 proved, BFS ok/ok/ok; `MultiForbidden_Light_1` (the lemma, which no `verify` cone holds) none, z3 proved, BFS ok/none/none; `Deadlock_Light_state_on`, `Deadlock_Sign_state_dark` cex |
| `z3_needs_count` | A true rule whose proof needs two count lemmas ("a closed shelf has at most two copies", "a crowded one at least three") past what BFS reaches at 2 instances, mined from the guards' bounds (#428) |  | `Forbidden_Shelf_0` ok, z3 proved, BFS ok/ok/ok |
| `z3_needs_exists` | A true rule whose proof needs a ∀∃ lemma over a key ("every ready order has a sent shipment"), mined as a count of at least one from Prepare's guard (#428) |  | `Quantifier_Order_0` ok, z3 proved, BFS ok/ok/ok; `Deadlock_Order_status_open` cex |
| `z3_needs_exists_global` | A true rule whose proof needs a ∀∃ lemma over rows no key links ("while a gate is ajar, some guard is on duty"), mined from what every reached state shows (#428) |  | `Quantifier_Gate_0` ok, z3 proved, BFS ok/ok/ok; `Deadlock_Gate_state_shut`, `Deadlock_Guard_state_on` cex |
| `z3_needs_run_mark` | A true rule whose proof needs a lemma that reads a run mark and relates rows no key links, which no mined lemma states; `z3_lemma_run_mark` adds it as a `.lemma()` (#428) |  | `MultiForbidden_Light_0` ok, z3 unproved, BFS ok/ok/ok; `Deadlock_Light_state_on`, `Deadlock_Sign_state_dark` cex |
| `z3_now_past_the_top` | The `now` rule of `z3_int_and_now_boundary` with one more value of range | `--int-range-max 6` | `Forbidden_Coupon_0` cex, z3 unproved, BFS cex/cex/cex |
| `z3_two_ok_three_fails` | An invariant true at 2 instances and false at 3 |  | `Forbidden_Order_0` ok, z3 unproved, BFS ok/ok/cex; `Deadlock_Order_status_c`, `Witness_Order_status_d` cex |

The state-cap trap has no file of its own: it is rule 7, which the capped-graph test checks on
every case's whole model.

## What the corpus catches

<!-- derived-from #agreement-rules -->

When the corpus was written, six bugs were planted in the checker, one at a time, and the
suite was run on each; each was reverted afterwards. The case counts are those of that time.

| Planted bug | Where | Cases that failed | Caught by |
|---|---|---|---|
| A self-loop counts as enabling and as a move | `fair_steps` in `explore/liveness.rs` | `fair_selfloop_not_enabling` | the header, rules 1 and 2, `EngineDisagreement` |
| `WF_vars(Next)` stops read as plain stops | `avoid` in `explore/liveness.rs` | 17 of 34 cases and the capped-graph test | `LassoFault` (rule 3), the header, rule 7 |
| A `_WF` ok is final on any cone, not only one that holds every written var | `after_wf` in `property_cone.rs` | 7 cases | the header only: both engines share the ladder |
| A counterexample on a cone that frees slots is taken as the whole model's | `counterexample` in `property_cone.rs` | `cone_always_frees_slots` | the header only |
| On a capped graph, every state counts as expanded | `expanded` in `explore/liveness.rs` | `cap_memory_limit` and the capped-graph test | `LassoFault`, rule 7 |
| `stable` and `unchanged` read only the edges back to an earlier state (#201) | `breaking_step` in `explore/properties.rs` | `safety_stable_across_delete` | the header, rule 1, `EngineDisagreement` |

The third and fourth rows are why every case states its verdicts: the shared ladder moved
verdicts in both engines at once, rules 1 to 3 saw nothing, and only the stated verdicts failed.

### Instance symmetry

<!-- constrained-by ./design/instance-symmetry.md#verifying-the-reduction -->

Phase 1 of [instance symmetry](./design/instance-symmetry.md) planted these bugs one at a
time and ran the symmetry unit tests (the exhaustive test over every state of four small
systems), the `sym_` cases under rule 8 in a release build, and the `sym_` cases at 1, 2,
and 3 instances on a `shadow` build.

| Planted bug | Where | Caught by |
|---|---|---|
| A deleted row's key is not relabelled | `Group::apply` | three exhaustive tests; rule 8 on `sym_deleted_row_owner`; the `canonical` twin |
| An identity pair is not coupled into one class | `Group::of` | the exhaustive test `an_identity_pair` |
| Absent slots are permuted | `Canon::canonical` | five exhaustive tests; the `canonical` twin on every `sym_` case |
| Ties that a child names are taken in slot order | `Canon::split` | the exhaustive test `a_parent_and_an_owner_child` |
| The lift undoes only the last permutation | `lift` in `explore/orbit.rs` | rule 8 on `sym_lift_compose`, release and `shadow` |
| The scan's ordered-id rule is gone | `breaks.rs` | the unit test `a_guard_that_orders_row_ids_leaves_the_class_out` |
| `compose` ignores its second permutation | `Group::compose` | the Kani harness `apply_of_a_composition_is_the_composition_of_applies` |
| `compose` applies its arguments in the wrong order | `Group::compose` | the unit test `apply_compose_and_inverse_agree_on_three_slots` |

Rule 8 misses the uncoupled identity pair and the permuted absent slots, since the wrong
canonical form still gives the plain verdicts; only the exhaustive test and the `canonical` twin
see them. Phase 3 (a reduced safety cone up to `G_T`) planted one more: dropping the input, key,
and S-action rules of `Reduction::fixed_why` together is caught by the `reduced_orbit_run` twin
on a `shadow` build and by `reduced_domain_cones_decide_what_the_cones_decide`. Phase 4's entity
cones (#370) planted five bugs in `orbit/findings.rs`, caught by the `orbit_state_models` twin and
the case `sym_arm_reorders`.

## The sample comparison

`crates/rdra-ish-cli/tests/engine_equivalence.rs` runs BFS and TLC on every cone of every sample,
at 1 and 2 instances per entity, and compares on each cone:

- the reached and initial states, against the graph `tlc -dump dot,actionlabels` writes;
- the edge sets, as `(from, to, use case)`, self-loops included;
- the state-model findings of every entity: witnesses, use cases that never run, deadlocks;
- the verdict of each invariant and `after` assertion, with every TLC trace replayed through
  the BFS step relation;
- at 2 instances, the findings TLC alone finds on the `StateModel_` specs `export` writes.

A cone whose BFS, reduced as `verify` explores it, passes 300,000 states at 2 instances is
compared at 1 instance only. `VERIFY_FALLBACK` lists these cones and the test fails when the
list changes; [Known verification gaps](./formal-verification.md#known-verification-gaps) gives
each one's size. `scripts/tlc-roots.txt` names the sample roots with such a cone, and CI's
`formal-verify` job runs TLC on them at their header bounds instead (#305). oidc-auth's header
runs 1 instance, so its 13 cones on that list meet TLC at 2 instances nowhere: its browser-flow
cone passes 2,000,000 states there, a TLC run of over an hour.

A cone whose unreduced BFS passes 300,000 states at 2 instances while its reduced one fits is
compared reduced only: TLC reads the quotient tables BFS computed, so on that cone the
comparison checks the reduced system, not the reduction. `VERIFY_REDUCED_ONLY` lists these
cones, and the test fails when the list changes.
`a_property_cone_decides_what_the_whole_model_decides` compares each property's cone verdict
with the whole model's, and the native verdict of each run with TLC's. CI runs the test in
release with `RDRA_REQUIRE_TLC=1`.

### The reduction on random models

`scripts/reduction-diff.py` (#102) generates random models of three to five entities, with
`relate` chains, `has` and `none` guards, `.along` rules, a split use case, `chooses`, a `Date`
deadline and a timer, `after` assertions, and temporal properties, at 2 or 3 instances. Per seed
it runs `verify --deny-undecided` as users run it and again with every cone unreduced
(`RDRA_ISH_NO_REDUCE`, a test hook), and compares the exit code, the finding names, each
check's verdict, the `result:` line, and the state-model summary. State counts and traces
differ by design.

- A seed where one run leaves something open that the other decides is listed under
  `plain_undecided` or `reduced_undecided` and compared on what both decide.
- A diverging model is copied out as `.rdra`, and `minimize` shrinks it while it still
  diverges.
- A seed past the per-run timeout is listed, never dropped. A run that prints
  `error[LassoFault]`, or with `--with-tlc` `error[EngineDisagreement]`, is listed under
  `disagreed`.
- `selfcheck` runs the comparison on the saved output pairs in `scripts/reduction-diff-fixtures`.

CI runs `selfcheck`, then the quick seeds of `scripts/reduction-diff-pr-seeds.txt` without TLC on
a pull request, and quick seeds with `--with-tlc` on main, all with `--fail-on-timeout`. Larger
sweeps run locally:

```sh
cargo build --release -p rdra-ish-cli
nix develop -c python3 -I scripts/reduction-diff.py --timeout 120 \
  run target/release/rdra-ish --seeds 0:500
```

### Batch JVMs

Most of `engine_equivalence`'s TLC runs take milliseconds inside a JVM that takes far longer
to start. The flake's `tlc-batch` (`nix/tlc-batch/RdraTlcBatch.java`, #310) keeps a JVM up and
runs one TLC job after another in it; `crates/rdra-ish-cli/src/tlc_batch.rs` pools one JVM per
test thread. `verify --with-tlc` does not use it.

- **Isolation.** Each job loads TLC through a class loader of its own and runs in a thread of
  its own, and the driver removes TLC's MBeans and its RMI export afterwards. The JVM, its JIT
  code, and its `java.io.tmpdir` are shared, one job at a time.
- **Which runs.** A cone BFS found to have at most 5,000 states, and the runs whose size is not
  known up front (a property's cone, state models, fixtures). Larger cones and whole-model runs
  are processes of their own.
- **Memory.** A batch JVM gets 512 MiB of heap, and the pool has at most as many JVMs as the
  test has workers. The JVM needs `-XX:SoftRefLRUPolicyMSPerMB=0`, because the RMI runtime
  keeps the classes of every exported object behind a soft reference.
- **Fallback.** A job with no answer runs again as a process: the driver did not start, the JVM
  ended under the job, or the job took over 300 s. After three lost jobs in a row every later
  job is a process. A JVM is replaced after 400 jobs, or when its jobs have left 8 threads
  behind (TLC 1.7.4 never shuts down the executor of a liveness counterexample).
- **Comparison.** `RDRA_TLC_BATCH=both` runs each job in a batch JVM and then as a process with
  the fingerprint and seed the first run drew (`-fp`, `-seed`), and fails on any difference in
  the exit status, the log, the parsed run, or the dumped state graph, apart from timestamps,
  timings, process ids, temp paths, timer output, and the heap size. `RDRA_TLC_BATCH=twice`
  compares two process runs the same way, and `RDRA_TLC_BATCH=0` runs every job as a process.
- **In CI.** `RDRA_REQUIRE_TLC_BATCH=1` fails the test when no job ran in a batch JVM or one
  fell back for any reason but the time limit. On `main` (scheduled or started by hand), the
  `tlc-batch-both` job runs `engine_equivalence` once more under `RDRA_TLC_BATCH=both`; a pull
  request does not run it.

## Mutation run

<!-- derived-from #what-the-corpus-catches -->
<!-- constrained-by ./design/native-temporal-verification.md#phases -->

The mutation run repeats the planted bugs for every mutant `cargo mutants` makes in
`property_cone.rs`, `verify/native.rs`, `cone.rs`, and `explore/liveness.rs`.
`.cargo/mutants.toml` holds the files and the test command, the whole `engine_corpus` test. The
run keeps `tlc` and `z3` off `PATH`, so a mutant is caught only when a stated verdict or the
capped-graph test fails: the check that sees a wrong `ok` the native checker and TLC share
through the cone ladder.

It starts only by hand, before a release gate of the
[native verification plan](./design/native-temporal-verification.md#phases):

```sh
gh workflow run mutants.yml                 # every mutant, over 4 shards
gh workflow run mutants.yml -f re=liveness  # only the mutants whose name matches
```

The `summary` job lists the `missed` and `timeout` mutants, and each shard's `mutants.out` is an
artifact with each mutant's diff and log. Kill a missed mutant with a corpus case it gets wrong,
or record why it is equivalent. A timeout is a mutant whose corpus ran past 60 seconds, a hang,
which needs no new case. Each test process runs under a 2 GiB address-space cap, so a mutant
that allocates without end fails at the cap. A shard that uploads no results is named at the top
of the summary.

**Kill rate.** The kill rate is the caught and timed-out mutants over the viable ones (all
but `unviable`). Record each full run here, and the run before a release in its changelog
entry, so that a fall shows:

| Run | Commit | Date | Mutants | Caught | Timeout | Missed | Unviable | Kill rate |
|---|---|---|---|---|---|---|---|---|
| [45212589802](https://github.com/manji-0/rdra-ish-dsl/actions/runs/45212589802) | `b0f593ba` | 2026-11-23 | 251 | 137 | 2 | 70 | 42 | 139 of 209, 66.5% |

The missed mutants of that run: 27 in `cone.rs`, 26 in `explore/liveness.rs`, 12 in
`verify/native.rs`, 5 in `property_cone.rs`. The run mutates only those four files. The TLA+
printer, `compare.rs`, `tlc_output.rs`, and `tlc_replay.rs` are not on the list: the run keeps
`tlc` off `PATH`, so the corpus could not catch a mutant there. Mutating them needs a run with
TLC on `PATH`, at many times the cost.

## Kani harnesses

Kani checks a few leaf functions of the checker on every input up to a bound, on the Rust code
as written. Each harness is a `#[kani::proof]` in a `#[cfg(kani)] mod proofs` at the end of the
file it proves, so builds and tests never compile it.

| Harness | File under `lifecycle/` | Statement | Bound |
|---|---|---|---|
| `numbers_compare_as_i64_and_null_is_never_ordered` | `expr.rs` | `compare_numbers` orders two numbers as `i64` does; as term equality, `null` equals only `null`, and it is never ordered. A rule's `==` between two columns adds a present guard on each nullable one when it is built, so two null columns are not equal there | every pair of `Option<i64>`, every operator |
| `compare_negates_and_mirrors` | `expr.rs` | `compare` does not panic, `!=` is the negation of `==`, and swapping the operands mirrors the operator | every operand kind, every domain, every value |
| `a_block_rounds_up_to_16` | `explore/store.rs` | `block(n)` is a multiple of 16 that holds `n` and wastes under 16 bytes | every `n` up to `usize::MAX - 16` |
| `locate_numbers_rows_in_order` | `explore/segments.rs` | `locate` puts each row in one segment, right after the row before it | every row up to 40, segments of 4, 4 and then 4 |
| `rows_take_what_bytes_with_said_and_never_move` | `explore/segments.rs` | a buffer of segments reads each row back and keeps it at its address, and `bytes_with(1)` before a push is `bytes_with(0)` after it | rows of one byte, up to 9 pushes |
| `segments_hold_every_row_and_no_more_than_a_doubling_vec` | `explore/segments.rs` | the segments that hold `n` rows hold room for all of them and never more than a doubling `Vec` | every `n` up to 2^40, segments of 4 |
| `store_bytes_never_overflows` | `explore/store.rs` | `store_bytes` neither overflows nor panics | every input, every shape |
| `store_bytes_only_grows` | `explore/store.rs` | each term of the memory model only grows with more or wider states and edges, saturating at `u64::MAX` | every count up to `u64::MAX`, segments of 4 and of 2^14 |
| `store_bytes_is_at_most_the_model_before` | `explore/store.rs` | at segments of 2^14 rows, the model never counts more than the one before #296 | every input, every shape |
| `a_push_takes_the_bytes_bytes_with_said_and_reads_back` | `explore/arena.rs` | a pushed state reads back at the address it was pushed to, and `bytes_with` predicts the push | states of one byte, up to 9 pushes |
| `pack_then_unpack_is_the_state` | `explore/packing.rs` | packing by 64-bit words and one cell at a time give the same bytes, and a state that fits reads back whole, so two states pack alike only when equal (#296 2.3) | 4 cells, every width from 0 to 8 bits, every value |
| `a_cell_across_a_word_reads_back` | `explore/packing.rs` | the same, with a cell starting, ending, or straddling bit 64 | seven 8-bit cells and four of every width up to 8 bits |
| `a_patch_is_the_pack` | `explore/packing.rs` | patching a packed state with the cells where another differs gives the other's bytes | the same 11 cells, every pair of states that fit |
| `apply_of_a_composition_is_the_composition_of_applies` | `explore/symmetry/mod.rs` | `apply(g ∘ h) = apply(g) ∘ apply(h)`, keys relabelled on present rows only | a 2-slot parent and a 2-slot child with a key to it |
| `lanes_are_exactly_the_equal_bytes` | `explore/table.rs` | the 8-lane tag match of the shared table sets the top bit of exactly the bytes equal to the tag | every 64-bit word, every tag |
| `growth_keeps_room_for_one_more` | `explore/table.rs` | the shared table's growth rule is hashbrown's as `Seen::bytes_with` reads it: an insert keeps the capacity while there is room, else moves to a capacity of at least one more item whose buckets are a power of two | every length up to the capacity, buckets from 2^2 to 2^40 |

A result holds up to its bound, and for one thread. The symmetry harness stays at 2 slots per
class, where its group commutes, so the exhaustive unit test and the `canonical` twin check the
canonical form. Under Kani a state packs one cell at a time (`Packing::by_words`), and the
`packing` harnesses prove the two paths alike.

### Running them

Kani is not in the flake's nixpkgs. Install the version `KANI_VERSION` in
`.github/workflows/ci.yml` pins, with `cargo` and `rustup` on `PATH`. `cargo kani setup`
downloads CBMC and a nightly toolchain, about 350 MB under `~/.kani` and 1.4 GB under
`~/.rustup`:

```sh
cargo install --locked kani-verifier --version 0.68.0
cargo kani setup
cargo kani -p rdra-ish-core                                        # every harness
cargo kani -p rdra-ish-core --harness compare_negates_and_mirrors  # one harness
```

`store_bytes_is_at_most_the_model_before` and
`apply_of_a_composition_is_the_composition_of_applies` take several minutes each; the others take
under a minute.

### In CI

The `kani` job runs every harness on each run on `main` (scheduled every three hours, or
started by hand). On a pull request it runs when a
changed file is on the list in `scripts/kani-relevant.sh`: the files that hold a harness, the
files whose code the harnesses run, the manifests and the lockfile, and the workflow that pins
Kani. A harness in a new file adds that file to the list.

### Adding a harness

Before a harness lands, plant a bug in the code it proves, check that Kani fails on it, and
revert the bug. A harness that still passes says nothing about that bug.
