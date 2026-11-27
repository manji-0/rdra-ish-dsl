# Agreement with TLC

<!-- derived-from ./semantics.md -->
<!-- derived-from ./claims.md#d-agreement-with-tlc -->
<!-- derived-from ../engine-agreement.md -->
<!-- derived-from ../formal-verification.md#mapping -->

This document states what assurance exists that the native verifier and TLC, reading the
TLA+ module rdra-ish prints, give the same verdicts, and what kind of assurance each piece
is. It describes the code at commit `fb6928c74ccc5b479dea07d92cc1e39c34ddd218`, later than
the index's pin `b83b9544`; no path it cites differs between the two. It owns rows T1 to T16 of the [claims inventory](./claims.md#d-agreement-with-tlc) and
uses the notation of [semantics.md](./semantics.md) (D1 to D23) without redefining it.

The code was read at `fb6928c7` (origin/main, 2026-11-28). The diff from `b83b9544` is empty
on every path this document cites (`crates/rdra-ish-emit/src/tla/`,
`crates/rdra-ish-cli/src/verify/`, `tlc_output.rs`, `tlc_replay.rs`, `property_cone.rs`, the
corpus and equivalence tests, `crates/rdra-ish-core/src/lifecycle/`, the workflows, the
scripts, and the notes), so the statements hold at both. Paths follow the convention of the
[claims inventory](./claims.md#how-to-read-a-row): `core` is
`crates/rdra-ish-core/src/lifecycle`, `cli` is `crates/rdra-ish-cli`, `emit` is
`crates/rdra-ish-emit/src`.

**The central point.** Agreement with TLC is evidence, not a theorem. No part of the TLA+
translation is proved. What exists is (1) an argument, written here for the first time,
that each construct of the printed module denotes what `T_β(M)` (D13) says it should, (2)
runtime agreement rules that turn a detected difference into a tool error, (3) differential
tests that run both engines on fixed inputs, (4) a few mechanised checks of leaf functions,
none of which touches the printer, and (5) CI jobs, none of which is a required check. Each
statement below is tagged:

- **[paper]**: argued in this document from the code as read; a reader can check it against
  the cited functions. It is not machine-checked.
- **[test]**: rests only on a differential or unit test passing on the inputs that test uses.
- **[runtime]**: checked on every user run, which reports a tool error when it fails.
- **[assumed]**: rests on a component the project does not verify: TLC, the JVM, or the
  shared upstream code that both engines read.

---

## Terms

**Module.** For a system `T'` (the whole `T_β(M)`, a cone `T|W` of D15, or a cone reduced by
its sub-lifecycle) and a print mode, `emit/tla/mod.rs::print` writes a TLA+ module `𝓜(T')`
and a `.cfg`. `PrintMode::Safety` lists `TypeOK`, the state rules and the `after`
assertions; `Property(i)` adds `WF_vars(Next)` to `Spec` and checks property `i`;
`ConeProperty(i)` checks property `i` with no `WF_vars(Next)`. `⟦𝓜⟧` is the set of
behaviors of `Spec` in the standard TLA+ semantics (Lamport 2002).

**Decoder.** `emit/tla/names.rs::TlaNames::decoder` maps a TLC state, given as the text of
each variable, to a `State` of `T'`. It skips the `precedes` latches, which are no state of `T'`; on a
reduced cone it reads the class variable and fills the sub-lifecycle's variables from that
class's representative (`Decoder::decode`). It returns nothing unless it used every key.
Write `dec` for it. The printer and the decoder are built from one
`TlaNames` value, so a renaming that is consistent in both is harmless and undetectable.

**Decided.** A run is decided when its verdict is `ok` or `counterexample` (D23). A TLC run is
decided when TLC printed its success line or a complete violation
(`cli/src/tlc_output.rs::TlcRun::verdict`); a TLC run that exits otherwise is `tool_error`
(`TlcRun::outcome`).

**Agreement.** Two engines agree on a check when both decided it and the verdicts are equal.
A check one engine leaves undecided, or on which TLC failed as a tool, is no disagreement
([agreement rules](../engine-agreement.md#agreement-rules), rule 1).

**Common-mode code.** Code both engines read, so that a fault in it moves both verdicts at
once: `TransitionSystem::build` (model to `T`), the cone selection (`core/cone.rs`), the
property-cone ladder (`cli/src/property_cone.rs`, `after_plain`, `after_wf`), the reduction's
quotient tables, and `TlaNames`.

## What agreement can show

<!-- derived-from ../engine-agreement.md#agreement-rules -->

Fix a check `c` and a system `T'`. Let `v_N` be the native verdict, `v_L` TLC's verdict on
`𝓜(T')`, and `v*` the verdict D16 to D21 assign to `c` on `T'`.

**Proposition 0 (transfer) [paper].** If `𝓜(T')` denotes `T'` in the sense of
[Proposition T1](#the-claim-proposition-t1), TLC is correct, and both engines decided `c`,
then `v_L = v*`. Hence `v_N ≠ v_L` implies that the native verifier is wrong on `c`, or T1
fails for this module, or TLC is wrong, or the comparison code misread a verdict.

*Proof.* T1 gives a bijection between the reachable states of `𝓜(T')` and of `T'` that
preserves initial states, labelled steps, and the truth of each check's state and step
predicates. Every verdict of D16 to D21 is a function of that labelled graph, its
initial states, and those predicates (fairness of D20 reads only actions, bindings, and
enabledness on it). TLC computes the TLA+ meaning of the check on `⟦𝓜(T')⟧`, which by T1
equals `v*`. ∎

The proposition runs in one direction. Agreement `v_N = v_L` shows only that the two
pipelines did not fail differently on that input. Three kinds of fault survive agreement:

1. **Common-mode faults.** A fault in common-mode code changes `T'` itself, so `v_N` and
   `v_L` both equal the verdict of the wrong system. Agreement says nothing about the step
   from the model to `T` (D13) or about which cone decides a check. Only the corpus
   headers, written from hand reasoning, see these ([Corpus](#the-corpus)).
2. **Compensating faults.** A printer fault and a native fault that happen to cancel on the
   tested inputs.
3. **Faults outside the tested inputs.** Agreement is checked only on the models, bounds,
   and runs that the tests and users ran.

## 1. The translation

<!-- derived-from ../formal-verification.md#mapping -->

### The claim (Proposition T1)

**Proposition T1.** Let `T' = (S, I, →)` be a system `verify` hands to TLC, and `𝓜 =
𝓜(T')`. Let `X` be the state variables of `𝓜` other than the `precedes` latches, and
`dec` as above (on a reduced cone, `X` includes the class variable). Then:

1. *(Types)* every reachable state of `𝓜` satisfies `TypeOK`, and `dec` is a bijection from
   the valuations of `X` that satisfy `TypeOK` onto `S`;
2. *(Init)* `dec(Init) = I`;
3. *(Steps)* for states `x, x'` with `dec(x) = s`, `dec(x') = s'`, every action `a`, and
   every binding `p` (parameters numbered as slots, TLA+ index `slot + 1`): `(x, x')`
   satisfies `A_a(p)` iff `p` is admissible in `s` (D7) and `s' ∈ Post_a(s, p)` (D8),
   including steps with `s' = s`. Per binding, not only for the disjunction `Next`: the
   fairness of D20 and the `after` check of D17 read `A_a(p)` at a fixed `p`;
4. *(Checks)* each `INVARIANT` holds at `x` iff the state rule of the same name holds at
   `dec(x)` (D16); each `after` `PROPERTY` holds on `⟦𝓜⟧` iff D17 holds (with the
   stuttering exception of [P22](./README.md#open-problems)); each temporal `PROPERTY`
   holds on `⟦𝓜⟧` iff D19 or D21 holds under the fairness of D20 that the print mode
   selects;
5. *(Latches)* for a module that checks `precedes(p, q)`, the latch is a history variable:
   adding it changes neither which `X`-valuations are reachable nor the steps between them.

Status: **not proved**. The argument below is a paper argument per construct, read from
`emit/tla/module.rs`, `emit/tla/pred.rs`, and `emit/tla/mod.rs` beside the native step
relation in `core/explore/moves.rs` and the evaluator `core/expr.rs`. Steps 1 and 2, and the
existential projection of step 3 (some action and binding gives the edge), are also
checked as graph equality on every sample cone that fits the comparison's cap
([Sample comparison](#the-sample-comparison)); step 3 per binding and step 4 are checked
by verdict comparison only.

### Per-construct argument

**Variables and domains [paper; test].** Each variable of D2 prints as one TLA+ variable,
a function over `<E>_Ids == 1..n_E` (or a scalar for `now`), and its domain prints through
`mod.rs::render_value` and `domain_set`. `render_value` is injective on each domain: the
three existence values, Enum values as quoted names, Bool as `TRUE`/`FALSE`, nullable Enum
and Bool as strings with `"<null>"` first (no Enum value can be spelled `<null>`, since a
value is an identifier), nullable Int as `-1` then `0..m`, owners as slot `+1` with `0` for
an unlinked optional key. So `dec` is a bijection on `TypeOK` valuations (step 1).
`TypeOK` is an `INVARIANT` of every cfg `verify` and the comparison check (all print modes
but `Explore`, which lists no invariant), so a step of `𝓜` that leaves the domain would
be reported by TLC [assumed: TLC]. The test `engine_equivalence.rs::tla_names_cover_every_slot`
checks the naming; the graph comparison checks `dec` on every reached state [test].
A binding prints as TLA+ index `slot + 1` (`pred.rs::at`), so the quantifier
`\E p \in <E>_Ids` ranges over exactly the slots of D7.

**Init [paper].** `module.rs::init` prints, for each variable outside the reduced part: the
class variable's initial classes; `Exists` at `present` on a pre-existing entity and at
`absent` otherwise; and the base value under `Start::Base`, the whole domain under
`Start::Any`, the slot itself under `Start::Identity`. `explore/moves.rs::initial_states`
applies `Start` only on pre-existing entities, while `init` applies it to every entity. The
two agree because `build/vars.rs` sets `Start::Any` or `Start::Identity` only on a variable
of a pre-existing entity (variation columns under `Birth::PreExisting`; owners when
`child_pre`). That is an invariant of the builder, not a check in the printer. The product
of independent choices in D4 is the TLA+ function set `[ids -> set]`. Hence step 2.

**Actions and bindings [paper].** `module.rs::action` prints one operator per action with
parameters `p0 …`, and `next` prints `\E p0 \in <E>_Ids, … : A(p0, …)`. The parameter
conjuncts follow D7: `New` is `FirstAbsent(ex, p)`, defined as "`ex[p]` is absent and every
lower slot is not", which is the lowest absent slot; `Existing`, `Parent` and `Triggered`
require `present`; `Earlier` has no conjunct, and its run-mark guard selects the slot,
present or not, as D7 says. Guards print through `pred.rs::Ctx::pred`; guards that read the
successor are printed after the assignments and `UNCHANGED`, which matters only for TLC's
left-to-right evaluation, not for the meaning of the conjunction.

**Effects [paper].** `module.rs::effects` folds the effects into one `v' = [v EXCEPT …]` per
written variable and `UNCHANGED` for the rest, and `moves.rs` applies them to a copy of the
pre-state. Case by case:

- `Assign` with a literal or a parameter: the same value; a later assignment to the same
  slot replaces an earlier one in both.
- `Transition`: both read the source on the pre-state. A matching arm writes its target;
  no matching arm leaves the earlier assigned value (`CASE … OTHER -> prev` against "no
  change" to the copy). Several targets from one source become a choice in both. The
  printer's shortcut for one target under a `transitions` guard (`from_guarded`) writes
  the target unconditionally, which is equal because the guard holds the column at a
  source.
- `Choose` and multi-target transitions: a choice is applied after every other effect in
  both (`\E v \in {...}` against forking `choices` after the loop), and a plain assignment
  does not override a choice on the same slot in either.
- `Delete`: both reset `Exists` to `deleted` and every column of `decl.columns` to base,
  and keep owners (D8). Both drop a choice on the deleted slot (`moves.rs` retains only
  choices on other params; the printer overwrites the slot with the base value).
- `Tick`: the action's guard `now < m` is an ordinary guard (`build/vars.rs`), and the
  effect prints `now' = now + 1`.

Hence step 3 for unreduced systems. Step 3 includes self-loops: a step whose successor
equals its pre-state satisfies the action formula in both, and `vars' = vars` makes it a
stuttering step in TLA+. That matters for fairness and `after` (below).

**Predicates and null [paper; test].** `pred.rs` mirrors D5: quantifiers range over present
slots; `Count` is `Cardinality` of the present slots that satisfy the body; `In`/`Not In`
on a slot is `\in`/`\notin` of rendered values, and otherwise the disjunction of
equalities. In `numeric`, `null` is `-1`, which lies outside `IntRange`; `=` and `#` compare
it as a value, so `null` equals only `null` and differs from every number, as D5 says; an
ordered comparison conjoins `x # -1` for each nullable operand, so `null` is never
ordered. Non-numeric operands compare by rendered value, which is index equality because
`render_value` is injective. An owner against a slot index is `owner = index`, where an
unlinked owner `0` equals no index. The native side of these rules has Kani harnesses
(`core/expr.rs`: `numbers_compare_as_i64_and_null_is_never_ordered`,
`compare_negates_and_mirrors`); the printer's side has none, and the two are tied by the
corpus cases `safety_null_*` and the fixtures `bfs_and_tlc_agree_on_null_in_a_check` and
`bfs_and_tlc_agree_on_set_membership` [test].

**Int-like columns and `now` (T16) [paper].** Both engines take the range from one
`Bounds` value: `Domain::Int` is `[m+1]` natively, and the module prints `IntRange ==
0..m` from `ts.bounds.int_max`. Literals and `@default`s outside the range are build errors
(`IntLiteralOutOfBounds`), so no effect writes outside the range. `Tick` stops at `m` in
both through the same guard.

**Split use cases and run marks [paper; test].** The run mark is a variable like any
other: `UC_1` guards it at `none` and sets `1`, `UC_i` binds the `Earlier` row by the mark
and sets `i` or `none`, and a delete does not touch it in either engine (`build/vars.rs` puts only column and driven-comparison variables in
`decl.columns`, which is what a delete resets). Tested by `bfs_and_tlc_agree_on_split_use_cases`,
`…_on_a_guard_over_every_group`, `…_on_an_interleaving_between_two_steps`, and the corpus
case `fair_split_use_case`.

**Events: latches and counters [paper; test].** Raised latches and counters are variables
of D2 with guards and assignments from the builder; the printer has no code of its own for
them beyond the generic action path. A cone that leaves the mark out drops the conjunct and
the `Triggered` parameter in the system, so both engines read the same over-approximation
(open problem P1 concerns what that cone's counterexample means, not agreement). Tested by
the corpus cases `fair_triggered_handler`, `fair_counted_exposes_lost_run`,
`safety_counted_once_per_raise`, `safety_event_overflow`.

**State rules [paper].** `module.rs::checks` prints each rule as `\A b … : (present …) =>
(body)` (`closed`), the binding-over-present-slots reading of D16. A rule over an entity
with no present slot holds vacuously in both.

**`after` assertions [paper].** `AfterAssert_i == [][(\A p : UC(p) => post')]_vars` over the
last step of each variant. Because `UC(p)` is a predicate on state pairs, it holds of
`(s, s')` exactly when `s' ∈ Post_UC(s, p)` (step 3), so the TLA+ property quantifies over
the same steps as D17, with one difference D17 does not state: `[A]_vars` is true on every
step with `vars' = vars`, so neither TLC nor the native checker checks an `after` on a step
whose successor equals its pre-state. The native checker skips that check explicitly
(`core/explore/bfs.rs::Bfs::take` continues before `check_after` on `Probe::Loop`, which
`explore/probe.rs::Probes::read` sets for a successor equal to its source in every build,
independent of `Probing`). The
engines agree; D17 is incomplete (open problem P22).

**Temporal properties [paper].** `checks` prints, per binding `\A b …` outside the
temporal operator:

- `always(x)`: `[](trig => x)`, D19.
- `eventually(x)`: `[](trig => <>x)`, which is D21 with `cause = true`: `<>` includes the
  current state, as D21 reads `goal` at `j = i`.
- `leads_to(x, y)`: `(trig /\ x) ~> y`, by definition `[]((trig /\ x) => <>y)`, D21.
- `stable(p)`, `unchanged(…).when(p)`: `[][\A b : (trig /\ p) => (pres' => …')]_vars`, D19,
  with the link read before the step only and stuttering steps exempt in both.
- `precedes(p, q)`: a latch `P_seen[b]` initialised to `trig /\ p` on the initial state and
  set to `P_seen[b] \/ (trig /\ p)'` by every action, with `[](\A b : (trig /\ q) =>
  P_seen[b])`. The latch is a history variable (Abadi and Lamport 1991): it reads the
  other variables and constrains none, so it does not change which `X`-valuations are
  reachable or the steps between them (step 5), and `P_seen[b]` at `s_k` equals
  `∃j ≤ k. trig_b(s_j) ∧ p_b(s_j)` by induction on `k`. Hence D19.

`trig` is `present /\ link` (`module.rs::trigger`), D19's `trig_b`.

**Fairness [paper; test].** For a property with `.fair(UC, …)`, `module.rs::fairness`
prints, per action `a` of a listed use case, `\A κ : WF_vars(\E new : A_a(κ, new))`, the
quantifier over the parameters that key a fair instance (`ParamRole::keys_fair_instance`)
outside and the `New` parameters inside. In TLA+, `WF_vars(B)` requires `<B>_vars` to occur
infinitely often if it is continuously enabled, where `ENABLED <B>_vars` asks for a
`B`-step that changes `vars`. That is D20 per fair instance: enabled means a successor
`s' ≠ s`, taken means a step of that instance to another state. Two consequences that
the native checker must match, and that have corpus cases:

- a self-loop is neither enabling nor a step (`fair_selfloop_not_enabling`);
- a step that some other action also produces satisfies `<A_a(κ)>_vars` and so counts for
  the fair instance, because `A_a` is a predicate on state pairs, not an action label
  (`fair_same_edge_two_bindings`).

The property is printed as `Fair_P => (…)`. `ConeProperty(i)` omits `WF_vars(Next)`
(`Plain` in D20), and `Property(i)` and the `_WF` runs conjoin it to `Spec` (`WfNext`). For
a safety module `WF_vars(Next)` is harmless: `Init /\ [][Next]_vars` is machine closed
with respect to `WF_vars(Next)` (Abadi and Lamport 1991), so adding it changes no safety
verdict. TLC's support for fairness quantified over a finite constant set is
[assumed: TLC].

**Reduced cones [test only; the quotient itself is common-mode].** A cone
reduced by its sub-lifecycle prints a class variable and tables computed in Rust
(`module.rs::reduction`, `bits`, `rise_expr`): the observed values of each class's
representative, the class successors per action of the sub-lifecycle, and the `Rise` table
from inputs and bits to the next class. TLC therefore explores the quotient as the Rust
code computed it. It checks the native exploration of that quotient, not the quotient's
soundness (A8, theme (a)). This document gives no paper argument that the printed class
move (the `is_class_move` branch of `action`), the `Rise` lookup, and the bits denote the
native reduced step (`moves_of_into` with a reduction); `bits`, `rise_expr`, and the native
reduced step were not read for it. Step 3 on reduced systems therefore rests on the graph
comparison of reduced sample cones alone. The sample comparison does compare the reduced cone's violated
checks with the unreduced cone's, through TLC on both
([Sample comparison](#the-sample-comparison)), but only on cones whose unreduced BFS fits
its cap.

**What the module does not state.** `can_reach` prints as a comment: TLC cannot state
`AG EF` (P19). Deadlocks are not TLC's `CHECK_DEADLOCK` (always `FALSE`) but the
`Deadlock_*` invariants of `print_state_models`, which are one-sided by design (T6).

### Status of T1

| Step of T1 | Paper argument | Test | Assumed |
|---|---|---|---|
| 1, 2, 3 (states, init, steps) on unreduced systems | above | graph equality on every sample cone under 300,000 states, at 1 and 2 instances, without bindings; 35 `bfs_and_tlc_*` tests | TLC's `-dump dot` lists every edge, self-loops included |
| 3 on reduced systems | none (not argued here) | graph equality on reduced sample cones; the reduced cone's violated set against the unreduced one's | the quotient tables (common-mode) |
| 4, state rules and `after` | above; P22 | verdict and violated-set comparison on every compared cone; each TLC trace replayed through the native step relation | TLC |
| 4, temporal properties and fairness | above | verdicts only: 23 `TEMPORAL_CASES` at 2 instances and the corpus at each case's flags | TLC's liveness checking under quantified weak fairness |
| 5, latch | above | `explored_along`, `safety_precedes_latch` | |

No step of T1 is checked on a binding: the graph comparison compares edges as
`(from, to, action)` (`engine_equivalence.rs`, `type Move`), so two bindings with the same
effect are one edge. A fault that attributes a step to the wrong fair instance is visible
only through a liveness verdict.

## 2. The agreement rules

<!-- derived-from ../engine-agreement.md#agreement-rules -->
<!-- derived-from ../engine-agreement.md#known-disagreements -->

**What is compared at run time (T2, T3) [runtime].** Under `verify --with-tlc`,
`cli/src/verify/compare.rs::disagreements` compares, per job:

- a safety cone: the two verdicts, and when both are `counterexample`, the set of checks
  each run violates, TLC's read by replaying each TLC trace (`tlc_replay::explain_violation`);
- an `Explored` property (`stable`, `unchanged`, `precedes`): the final verdicts, the
  whole-model run's where a cone left the property open;
- a property cone: each run in `Native::runs` against the TLC run of the same `ConeRun`,
  then the final verdicts.

`differ` treats a pair as a disagreement when both are `Some`, they differ, and neither is
`ToolError`. Each disagreement prints `error[EngineDisagreement]` and makes the run's
`result:` `tool_error` (`cli/src/verify/mod.rs`). State models are compared separately
(`state_models.rs::state_model_disagreements`, one-sided for deadlocks).

**Caps are asymmetric.** Native cone runs stop at `--max-states`, the memory limit, or the
time budget. TLC cone runs have no state cap: `verify/tlc.rs::run_cfg` runs TLC to the end,
and only whole-model runs (`run_whole`) are killed at the budget or the state cap. So a
native cone run can stop where TLC's run of the same module decides.

**Decided against undecided: the rule and the code.** Rule 1 says `not decided` from
either engine is no disagreement. The code keeps it for final verdicts, where `Decision`
maps `Undecided` to `None`. It does not keep it per run:

- **P21, confirmed and extended [read; not reproduced].** `verify/native.rs::Answer::outcome`
  maps `Answer::Stopped` to `Outcome::Ok`, and every push into `Native::runs` uses it: the
  `Plain`, `Full` and `WfNext` runs of `native_property_after`, the `Plain` run of
  `always_up_to_symmetry`, and the `Plain` and `WfNext` runs of `liveness_up_to_symmetry`.
  Not read: when `Natives::properties[k]` is `None`, in which case `compare.rs` skips the
  job. `Native::capped` resets the
  decision but not `runs`. So a native run stopped at a cap, beside a TLC counterexample on
  the same `ConeRun`, prints `error[EngineDisagreement]` and fails the run. The same mapping
  has a second consumer that needs no TLC: in `liveness_up_to_symmetry`, after a
  counterexample up to symmetry on a reduced cone, the plain run's first entry is tested for
  `Some(Outcome::Ok)`, and a plain run that stopped reads as `Ok`, so the property is
  reported `tool_error` ("the run up to symmetry finds a counterexample the plain run does
  not") although the plain run decided nothing. Both fail closed: a false tool error, never
  a wrong verdict. No corpus case reaches the path: the two cases with `native=not_decided`
  are `cap_memory_limit`, which skips TLC, and `cap_temporal_budget_zero`, whose budget
  stops only whole-model runs.
- **P12, confirmed and restated [read].** When both engines report a counterexample on a
  safety cone, `compare.rs` builds TLC's violated set with `collect::<Result<_, _>>().ok()`;
  if any TLC trace fails to replay, the sets are not compared. Separately,
  `tlc.rs::model_traces` replays the first TLC trace of each violated check for display and
  prints `error: …; BFS and TLC disagree (a tool bug)` when it does not replay. That line is
  not counted in `disagreements` or `tool_errors`, so the `result:` line and exit status do
  not change. The corpus harness and `verify-samples.sh` fail on any unnamed `error:` line,
  so in CI a non-replaying first trace is caught; a non-replaying later trace of the same
  check is in neither place, and for a user the exit status does not show either.

**The corpus harness's own reading (P13, confirmed) [read].** In
`cli/tests/engine_corpus.rs::check_case`, rule 2 `continue`s when TLC printed no line for a
module the native run names, and rule 8 compares a check only when both the plain and the
symmetric run decided it. Rule 1 in the harness reads the printed verdicts, so it inherits
the run-time comparison above.

**Known disagreements.** The corpus has none: no case carries a `known` line (0 of 79). No
run-time disagreement is open as an issue at the pinned commit, as far as the notes say.

## 3. Differential evidence

<!-- derived-from ../engine-agreement.md#the-corpus -->
<!-- derived-from ../engine-agreement.md#the-sample-comparison -->
<!-- derived-from ../engine-agreement.md#the-reduction-on-random-models -->
<!-- derived-from ../engine-agreement.md#batch-jvms -->
<!-- derived-from ../engine-agreement.md#mutation-run -->

Differential testing (McKeeman 1998) runs two implementations of one specification on the
same input and treats a difference as a fault in one of them. Its power is bounded by the
inputs, and it is blind to faults the implementations share. Each source below is
described by its inputs, what it compares, and what it cannot detect. The numbers are
those recorded in the notes and code at the pinned commit; none was re-measured.

### The corpus

`cli/tests/engine_corpus.rs` over `cli/tests/engine_corpus/*.rdra` (T7).

- **Inputs.** 79 cases with 191 `expect` lines; families `cap` 2, `cone` 11, `fair` 9,
  `presence` 7, `safety` 24, `sym` 10, `z3` 16. One case (`cap_memory_limit`) skips TLC.
  38 cases carry a `bfs=` list.
- **Bounds.** TLC runs once per case, at the case's flags: 2 instances and Int `0..5` by
  default; 1 instance in the cases with `--instance-count 1`; 3 or 4 instances of one entity
  in the three cases with per-entity flags (`presence_per_entity_slots`,
  `safety_count_per_entity`, `sym_per_entity_bounds`); `--int-range-max 6` or `7` in two
  `z3_` cases. The `bfs=` runs at 1, 3 and 4 instances are native only (P14).
- **Compares.** Each engine against the hand-written header, rules 1 to 3 against TLC,
  rule 8 against the symmetric native run.
- **Detects.** The header is the only check of common-mode code: the planted-bug table of
  [What the corpus catches](../engine-agreement.md#what-the-corpus-catches) records two
  planted bugs in the cone ladder that moved both engines and were caught by the headers
  alone.
- **Cannot detect.** A trap no case names. A header written wrong in the same way as the
  code (the header is reasoning by the same people who wrote the code). Deleting cases:
  `every_corpus_header_is_well_formed` asserts at least 30 cases where 79 exist (P15,
  confirmed).

### The sample comparison

`cli/tests/engine_equivalence.rs::a_bfs_and_tlc_reach_the_same_states_and_verdicts` (T5).
This is the closest thing to translation validation (Pnueli, Siegel and Singerman 1998)
the project has: per input, it checks steps 1 to 3 of T1 on the reachable fragment, rather
than the translator once for all inputs.

- **Inputs.** Every cone of every root of `samples.txt` (40 roots), at `Bounds::STATES`
  (1 instance) and `Bounds::VERIFY` (2 instances), with `.lemma()` rules and `can_reach`
  left out (`plan_model`).
- **Compares** (`check_cone`). TLC's distinct-state count and its `-dump dot,actionlabels`
  graph, decoded through `dec`, against the BFS graph: the reached states, the initial
  states, the edges as `(from, to, action)`; the state-model findings of every entity on
  both graphs; the violated invariants and `after` assertions, rerunning TLC without each
  found check until no new one appears; and each TLC trace replayed through the native
  step relation (`replay_violations`). `a_corrupted_tlc_trace_does_not_replay` checks that
  the replay rejects a corrupted trace.
- **Unreduced and reduced.** `check_one_cone` compares the unreduced cone only when its BFS
  completes under `STATE_CAP` (300,000 states), and the reduced cone only when its run
  completes under the same cap. A cone joins `VERIFY_FALLBACK` (24 cones) when its reduced
  run at 2 instances passes the cap. A cone whose unreduced BFS passes 300,000 states while
  its reduced BFS fits is compared reduced only, and nothing lists it. Which sample cones
  are in that case was not measured here. On such a cone TLC reads the Rust-computed quotient tables, so the comparison
  says nothing independent about the reduction there. This extends P14.
- **Cannot detect.** Temporal properties (not printed in `PrintMode::Safety`); bindings;
  anything at 3 or more instances; any cone of the 24 at 2 instances.

`a_property_cone_decides_what_the_whole_model_decides` (T3) runs the property ladder on 23
`TEMPORAL_CASES` (7 sample files and 16 inline fixtures; T3 said 26), at 2 instances, each run of each property
against TLC's run of the same module, and the cone's final verdict against the whole
model's TLC verdict. It compares verdicts, not graphs.

### `verify --with-tlc` on the sample roots

`scripts/verify-samples.sh --with-tlc --tlc-roots scripts/tlc-roots.txt` in CI's
`formal-verify` job runs TLC beside the native checker on the five roots that have a
`VERIFY_FALLBACK` cone (clinic-ops, patent-ops, food-permit-ops, insurance-claim-ops,
oidc-auth) and on every header line with flags, at each root's header bounds. The domain
roots' temporal properties are compared with TLC only here. It is held to the run-time
rules of [§2](#2-the-agreement-rules), including their gaps (P12, P21).

### Random models

`scripts/reduction-diff.py` (T9) generates models of three to five entities with chains,
quantified guards, `.along`, a split use case, `chooses`, a timer, `after` assertions and
properties, at 2 or 3 instances, and compares `verify --deny-undecided` reduced against
unreduced. With `--with-tlc` both runs also run TLC, and a seed with
`error[EngineDisagreement]` is listed as `disagreed`. In CI, a pull request runs 522 seeds
without TLC, and main runs 45 fixed seeds with TLC, some at 3 instances (`ci.yml`) (measured 2026-11-25: 522 compared, 284
reduced; 45 compared, 33 reduced). TLC therefore sees 45 random models per main run, the
same 45 each time. The release gate's item 3 (seeds `0:2000` with TLC) was waived for
v3.0.0 (P16). The generator has no count guard (A11), so counts get no random coverage.

### Batch JVMs

`RDRA_TLC_BATCH=both` (T11) runs each `engine_equivalence` job in a batch JVM and again as a
process with the same fingerprint and seed, and fails on any difference in exit status, log,
parsed run, or dumped graph. It is not set in any workflow; it runs by hand. The
`#[ignore]`d `recorded_tlc_runs_are_the_same_in_a_batch_jvm` replays recorded `verify` runs
the same way, also by hand. In CI, `RDRA_REQUIRE_TLC_BATCH=1` only requires that some job
ran batched without falling back. This bears on the test harness only: `verify --with-tlc`
does not use batch JVMs.

### Mutation run

`cargo mutants` (T15) over `property_cone.rs`, `verify/native.rs`, `core/cone.rs` and
`core/explore/liveness.rs` (`.cargo/mutants.toml`), killed by the corpus with `tlc` and `z3`
off `PATH` (`mutants.yml` checks that they are gone). It measures how well the corpus
headers alone catch faults in the cone ladder and the liveness search. It does not measure
agreement with TLC: TLC does not run, and no file of the printer (`emit/tla/`), the
comparison (`compare.rs`, `tlc.rs`), or the TLC readers (`tlc_output.rs`, `tlc_replay.rs`)
is mutated. The workflow starts only by hand (`workflow_dispatch`), and no kill rate is
recorded in the repository (P16, confirmed). Mutation analysis in the sense of DeMillo,
Lipton and Sayward (1978) would report the fraction of non-equivalent mutants killed; that
number does not exist here.

### What each source can and cannot detect

| Source | Native fault | Printer fault | TLC fault | Common-mode fault | Bounds and inputs |
|---|---|---|---|---|---|
| Corpus headers | yes, on the trap | yes, on the trap | yes, on the trap | yes, on the trap | 79 hand cases, TLC at each case's flags |
| Corpus rules 1 to 3 | where TLC differs | where native differs | where native differs | no | as above |
| Sample graph comparison | states, edges, initial states, safety verdicts | same | same | no | 40 roots, 1 and 2 instances, cones under 300,000 states |
| Temporal cone comparison | liveness and ladder verdicts | same | same | no | 23 cases, 2 instances |
| `verify --with-tlc` in CI | verdicts | verdicts | verdicts | no | 5 domain roots and flagged header lines, header bounds |
| Random models with TLC | verdicts | verdicts | verdicts | no | 45 fixed seeds, main only |
| Mutation run | cone ladder and liveness only, through headers | no | no | ladder only | corpus, by hand, no recorded rate |

## 4. Mechanised evidence

<!-- derived-from ../engine-agreement.md#kani-harnesses -->

**Kani (T13).** There are 16 `#[kani::proof]` harnesses at the pinned commit, all in
`core`: two in `expr.rs` (the comparison of D5), four in `explore/store.rs`, three in
`explore/segments.rs`, three in `explore/packing.rs`, one in `explore/arena.rs`, two in
`explore/table.rs`, one in `explore/symmetry/mod.rs`. Bounded model checking (CBMC, as Kani
uses it) proves each statement for every input up to its bound. For agreement, the two
`expr.rs` harnesses matter: they fix the native reading of null and ordering that
`pred.rs::numeric` must mirror. Nothing ties `pred.rs` to them except tests. No harness
concerns the printer, the decoder, the TLC readers, or the comparison.

**Shadow twins (T14).** A `shadow` build runs 25 reference twins beside fast paths
(`crates/rdra-ish-core/src/shadow.rs::TWINS`). The ones on the path TLC is compared against are the move
generation (`EachEnabled`), the edge store (`Edges`, `StepEnds`), and the reduced class
moves (`ClassMoves`). They compare a fast native path with a slower native one; they say
nothing about TLC or the printer. Their value for agreement is reach: the `shadow` job runs
them on samples and corpus cases larger than the TLC comparison covers, below each twin's
cap (P17).

## 5. Process gates

<!-- derived-from ../engine-agreement.md#mutation-run -->

| Job (`.github/workflows/ci.yml`) | Runs on a pull request | Runs on main | Compares with TLC |
|---|---|---|---|
| `engine-equivalence`: `engine_equivalence`, `proof_equivalence`, `engine_corpus` with `RDRA_REQUIRE_TLC=1` | when `changes` says `verify` | every 3 hours, skipped when the commit was checked | yes |
| `formal-verify`: `verify-samples.sh --with-tlc --tlc-roots`, reduced against unreduced on random models | when `changes` says `verify`; random models without TLC | with TLC on 45 seeds | yes |
| `shadow` | as `changes` decides | yes | yes: the corpus on a `shadow` build with `RDRA_REQUIRE_TLC=1` |
| `kani` | when `scripts/kani-relevant.sh` matches a changed file | yes | no |
| `mutants.yml` | never | never; by hand | no |

Every CI step runs in the flake's devShell (`ci.yml`, `shell: nix develop`), whose TLC is
`flake.nix`'s `tlcFor`: the nixpkgs `tlaplus` package overridden to the 1.8.0 pre-release
jar (build of 2026-11-19, rev `bc1b506`, by the flake's comment) on JDK 25. The CI evidence
is about that TLC; the notes also name 1.7.4, which nixpkgs ships.

**None of these is a required check.** `gh api repos/manji-0/rdra-ish-dsl/branches/main/protection`
answers "Branch not protected", and the repository has no rulesets (checked 2026-11-28).
A merge with a red or skipped job is possible; the project's practice is to merge on green
pull-request checks, which is a convention, not a gate. Main runs on a 3-hour schedule, so
a merge reaches the main-only steps (random models with TLC, the full `kani` set) hours
later or, if GitHub drops a scheduled run, later still (P16, confirmed).

Two release-gate statements bear on agreement (P16, extended). The CHANGELOG for v3.0.0
records item 2 of the gate ("`verify --with-tlc` on every root of `samples.txt` reports no
disagreement") as held on the evidence that `formal-verify` ran "the TLC roots with
`--with-tlc`", which is five roots and the flagged header lines, not every root; the other
roots were compared through `engine_equivalence` instead. And native-temporal-verification.md
says the `--with-tlc` step moves to `release.yml` once the gate passes; `release.yml` runs no
`verify`, and the step still runs in `formal-verify`.

## Residual risk

<!-- derived-from #what-agreement-can-show -->
<!-- derived-from #status-of-t1 -->
<!-- derived-from #what-each-source-can-and-cannot-detect -->

Classes of divergence between `T_β(M)` and what the engines decide that the current
evidence would not catch, most consequential first:

1. **Common-mode faults outside the corpus traps.** A wrong `T` from the builder, a wrong
   cone, a wrong ladder decision, or a wrong quotient table moves both engines. Only the 191
   hand-written `expect` lines see these, and only on the 79 traps they name.
2. **A wrong `ok` on any input no test ran.** Native counterexamples replay, so a wrong
   `counterexample` is caught at run time (with the exceptions of P20); a wrong `ok` has no
   run-time guard, and TLC sees it only where a test or a user runs `--with-tlc`.
3. **Bounds.** In CI, TLC runs at 3 or more instances only on three corpus cases with
   per-entity flags and on the 3-instance seeds among the 45 main random seeds; never on
   the 24 fallback cones at 2 instances in `engine_equivalence` (`verify --with-tlc` runs
   them at the roots' header bounds, verdicts only); and never as a graph comparison on
   unreduced cones above 300,000 states.
   A divergence that needs more rows than the tested bounds is invisible to every
   differential source (P14).
4. **Predicate shapes no test reaches in a discriminating state.** `pred.rs` and `expr.rs`
   are separate implementations of D5 with no shared harness. A difference on a shape
   (say an ordered comparison of a nullable Int against `now`, or a count inside a
   quantifier on the primed state) shows only if some tested model reaches a state where
   the two readings differ.
5. **Fairness per instance.** No test compares bindings, so a fault in which instance a
   step counts for shows only through a liveness verdict, on 23 temporal cases and the
   `fair_*` corpus cases.
6. **The comparison code itself.** `compare.rs`, `tlc_output.rs`, `tlc_replay.rs` and
   `TlaNames` have no Kani harness, no shadow twin, and no mutant; P12 is a place where a
   disagreement in violated sets can go unreported.
7. **TLC.** TLC's fingerprint-based state store can, with small probability, merge two
   distinct states on a hash collision and so miss states (Yu, Manolios and Lamport 1999
   discuss TLC's design; TLC prints its own collision estimate, which
   `cli/src/tlc_output.rs` does not parse). A TLC fault that matches a native fault is not detectable.
8. **Checks with no TLC counterpart.** `can_reach` and the one-sided deadlock check (P19).

## Threats to validity

- **Internal.** The argument for T1 is a reading of the printer beside the native step
  relation at one commit. It is not mechanised, and it relies on invariants of the builder
  (for example that only pre-existing entities start off base) that no assertion enforces
  in the printer. A misreading is possible; each step names the functions so it can be
  re-checked.
- **Construct.** "Agreement" in the tests is agreement of verdicts and of labelled graphs
  without bindings, not equivalence of the modules. The decoder is the same code in the
  printing and in the comparison, so a consistent renaming is invisible (harmless) and a
  decoder that maps two TLC states to one native state would show as a count mismatch,
  which the comparison checks.
- **External.** The inputs are 40 sample roots, 79 corpus cases, 23 temporal cases, 35
  `bfs_and_tlc_*` tests, and 45 random models with TLC. The samples were written alongside the tools,
  so they exercise what the authors thought of. The random generator has a fixed shape
  (no counts, three to five entities).
- **Conclusion.** No numbers in this document were re-measured. The P21 and P12 statements
  are readings of the code, not reproductions; the issue that takes them up should start
  with a reproduction.

## Related work

- **TLA+ and TLC.** Lamport, *Specifying Systems* (2002), for `WF`, stuttering, `~>`, and
  machine closure; Yu, Manolios and Lamport, "Model checking TLA+ specifications" (CHARME
  1999), for TLC. Abadi and Lamport, "The existence of refinement mappings" (TCS 1991), for
  history variables and machine closure.
- **Translation validation.** Pnueli, Siegel and Singerman, "Translation validation"
  (TACAS 1998): check each translation's output against its input instead of verifying the
  translator. The sample comparison does this on the reachable fragment, for safety only.
- **Differential testing.** McKeeman, "Differential testing for software" (Digital
  Technical Journal, 1998).
- **Mutation analysis.** DeMillo, Lipton and Sayward, "Hints on test data selection" (IEEE
  Computer, 1978); the mutation score is the measure the mutation run does not record.

## Claims and open problems

<!-- derived-from #1-the-translation -->
<!-- derived-from #2-the-agreement-rules -->

Rows T1 to T16 of the [claims inventory](./claims.md#d-agreement-with-tlc) cite this
document. The open problems this document confirmed, extended, or added, in the
[index](./README.md#open-problems):

| # | What this document found |
|---|---|
| P12 | Confirmed; restated: the display path prints an unnamed `error:` line for the first non-replaying trace only, which does not change `result:` or the exit status. |
| P13 | Confirmed as stated. |
| P14 | Confirmed; extended: unreduced cones over 300,000 states are silently compared reduced only. |
| P15 | Confirmed as stated (at least 30 asserted, 79 present). |
| P16 | Confirmed; extended: release-gate item 2 recorded as held on a narrower check; the `--with-tlc` step did not move to `release.yml`; native-temporal-verification.md, "How we know it is right", still says CI runs every root with `--with-tlc`. |
| P21 | Confirmed; extended to the `WfNext` runs and to a false `tool_error` under `--symmetry always` without TLC. |
| P22 | New (prose): D17 omits that a step with `s' = s` is checked by neither engine. Both halves read; `Probe::Loop` is set in every build. |
