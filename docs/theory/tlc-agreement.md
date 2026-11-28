# Agreement with TLC

<!-- derived-from ./semantics.md -->
<!-- derived-from ./claims.md#d-agreement-with-tlc -->
<!-- derived-from ../engine-agreement.md -->
<!-- derived-from ../formal-verification.md#mapping -->

This document describes main at `91524792`.

It states what assurance exists that the native verifier and TLC, reading the TLA+ module
rdra-ish prints, give the same verdicts, and what kind of assurance each piece is. It owns
rows T1 to T16 of the [claims inventory](./claims.md#d-agreement-with-tlc) and uses the
notation of [semantics.md](./semantics.md) (D1 to D25) without redefining it. Paths follow
the convention of the [claims inventory](./claims.md#how-to-read-a-row): `core` is
`crates/rdra-ish-core/src/lifecycle`, `cli` is `crates/rdra-ish-cli`, `emit` is
`crates/rdra-ish-emit/src`.

**The central point [read].** Agreement with TLC is evidence, not a theorem. No part of the TLA+
translation is proved. What exists is (1) an argument, written here, that each construct of
the printed module denotes the system the build returned, which is `T_β(M)` (D13) when claim
[I1](./claims.md#0-the-build) holds, (2) runtime agreement rules that turn a detected
difference into a tool error, (3) differential tests that run both engines on fixed inputs,
(4) a few mechanised checks of leaf functions, none of which touches the printer, and (5) CI
jobs, which no repository rule requires to pass
([README, Known limitations](./README.md#known-limitations)). Each statement below is tagged:

- **[paper]**: argued in this document from the code as read; a reader can check it against
  the cited functions. It is not machine-checked.
- **[test]**: rests only on a differential or unit test passing on the inputs that test uses.
- **[runtime]**: checked on every user run, which reports a tool error when it fails.
- **[assumed]**: rests on a component the project does not verify: TLC, the JVM, or the
  shared upstream code that both engines read.
- **[read]**: a statement of what the code, tests, workflows, or notes contain or do, taken
  from reading them (or, where it says so, from one `gh api` query). Nothing was run for it.
  A number taken from a note or code comment rather than counted in the code says where it
  comes from.

The [read] tag is there because most of sections 2 to 5 is neither an argument nor a test
result but a description of what the evidence is. The tagging unit is a paragraph with a bold
lead, a list item, a table, or a subsection: a tag at the start of a subsection or on the
sentence that introduces a list or table covers it until the next tag. Tags combine with `;`
(`[paper; test]` means both kinds of support), and a tag may carry a qualifier after a colon
or `;` (`[assumed: TLC]`, `[paper; see Status]`, `[test; no paper argument; …]`); the class
is always one of the five above. The Related work section cites literature and carries no
tag.

---

## Terms

**Module [read].** For a system `T'` (the whole `T_β(M)`, a cone `T|W` of D15, or a cone reduced by
its sub-lifecycle) and a print mode, `emit/tla/mod.rs::print` writes a TLA+ module `𝓜(T')`
and a `.cfg`. `PrintMode::Safety` lists `TypeOK`, the state rules and the `after`
assertions; `Property(i)` and `ConeProperty(i)` check property `i`. Whether `Spec` conjoins
`WF_vars(Next)` does not depend on the checked property: `print` sets it when the mode is
not `ConeProperty` and `T'` has some liveness property (`wf_next = !ConeProperty &&
needs_fairness(ts)`). So a `Safety` module of a system with a liveness property has it, a
`Property(i)` module of a system with none does not, and a `ConeProperty(i)` module never
has it. `⟦𝓜⟧` is the set of behaviors of `Spec` in the standard TLA+ semantics (Lamport
2002).

**Decoder [read].** `emit/tla/names.rs::TlaNames::decoder` maps a TLC state, given as the text of
each variable, to a `State` of `T'`. It skips the `precedes` latches, which are no state of `T'`; on a
reduced cone it reads the class variable and fills the sub-lifecycle's variables from that
class's representative (`Decoder::decode`). It returns nothing unless it used every key.
Write `dec` for it. The printer and the decoder are built from one
`TlaNames` value, so a renaming that is consistent in both is harmless and undetectable.

**Decided [read].** A run is decided when its verdict is `ok` or `counterexample` (D23). TLC
runs are read in two ways (`cli/src/tlc_output.rs`):

- A cone run (`verify/tlc.rs::run_cfg`) always gets `TlcRun::outcome`: `tool_error` when the
  trace read-back failed, else `counterexample` when TLC printed any violation (a temporal
  violation counts even without its lasso), else `ok` when TLC exited successfully and
  printed its success line, else `tool_error`. A cone run is therefore never `not decided`.
- A whole-model run (`verify/tlc.rs::run_whole`) that was killed at the time budget or the
  state cap gets `TlcRun::verdict` instead: decided when TLC had already printed its success
  line, or violations whose temporal ones all end in a lasso; otherwise `not decided`. A
  whole-model run that was not killed gets `TlcRun::outcome`.

**Agreement [read].** Two engines agree on a check when both decided it and the verdicts are equal.
A check one engine leaves undecided, or on which TLC failed as a tool, is no disagreement
([agreement rules](../engine-agreement.md#agreement-rules), rule 1).

**Common-mode code [read].** Code both engines read, so that a fault in it moves both verdicts at
once: `TransitionSystem::build` (model to `T`, whose agreement with D13 is claim
[I1](./claims.md#0-the-build)), the cone selection (`core/cone.rs`), the
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

*Proof.* T1 (1) to (3) make `dec` a bijection between the reachable `X`-valuations of
`𝓜(T')` and the reachable states of `T'` that preserves initial states, labelled steps, and
the truth of each check's state and step predicates. A module with no `precedes` latch has
no other variable, so its reachable states are those valuations. A module that checks
`precedes` also has latches, which `dec` skips (`Decoder::decode` ignores their keys): there
the map from reachable states of `𝓜(T')` to reachable states of `T'` is the projection onto
`X` followed by `dec`, which is onto but not injective (two states may differ only in a
latch). By T1 (5) the projection maps the initial states and steps of `𝓜(T')` onto those of
`T'` and adds no `X`-step, so every behavior of `T'` is the projection of a behavior of
`𝓜(T')` and conversely; and the latch's value at each state is the function of the
behavior's prefix that D19's `precedes` reads. Every verdict of D16 to D21 is a function of
the labelled graph of `T'`, its initial states, those predicates, and (for `precedes`) the
prefix of a behavior; fairness of D20 reads only actions, bindings, and enabledness on that
graph. TLC computes the TLA+ meaning of the check on `⟦𝓜(T')⟧`, which by T1 and the
projection equals `v*`. ∎

**Scope [read].** The proposition needs both verdicts to be about the same `T'`. The per-run
comparison pairs runs by that system (`RunOn`: property, graph, fairness). One native run has
no TLC run on its system: for a liveness property on a reduced cone, the native `WfNext` run
may be `decide_liveness_reduced` on the reduced cone, while TLC's `_WF` module is printed from
the unreduced cone ([§2](#2-the-agreement-rules)). That run is compared only through the
final verdict, where a difference can also mean a fault in the reduction (A8) or in the
reduced-graph `_WF` argument (A15), and agreement shows that those and the native search
together gave TLC's verdict on the unreduced cone.

The proposition runs in one direction [paper]. Agreement `v_N = v_L` shows only that the two
pipelines did not fail differently on that input. Three kinds of fault survive agreement:

1. **Common-mode faults.** A fault in common-mode code changes `T'` itself, so `v_N` and
   `v_L` both equal the verdict of the wrong system. Agreement says nothing about the step
   from the model to `T` (D13) or about which cone decides a check. Only the corpus
   headers, written from hand reasoning, and on a subset of the language the reference
   interpreter see these ([Corpus](#the-corpus), [Residual risk](#residual-risk) item 1).
2. **Compensating faults.** A printer fault and a native fault that happen to cancel on the
   tested inputs.
3. **Faults outside the tested inputs.** Agreement is checked only on the models, bounds,
   and runs that the tests and users ran.

## 1. The translation

<!-- derived-from ../formal-verification.md#mapping -->

### The claim (Proposition T1)

**Proposition T1 [paper; see Status].** Let `T' = (S, I, →)` be a system `verify` hands to TLC, and `𝓜 =
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
   `dec(x)` (D16); each `after` `PROPERTY` holds on `⟦𝓜⟧` iff D17 holds, where D17 exempts a
   step whose successor equals its pre-state ([`after` assertions](#per-construct-argument));
   each temporal `PROPERTY` holds on `⟦𝓜⟧` iff D19 or D21 holds under the fairness of D20
   that the print mode selects;
5. *(Latches)* for a module that checks `precedes(p, q)`, the latch is a history variable:
   adding it changes neither which `X`-valuations are reachable nor the steps between them.

Status [read]: **not proved**. The argument below is a paper argument per construct, read from
`emit/tla/module.rs`, `emit/tla/pred.rs`, `emit/tla/mod.rs`, and `emit/tla/names.rs` beside
the native step relation in `core/explore/moves.rs` and the evaluator `core/expr.rs`. The
argument for step 3 needs an invariant of the builder: no action writes one `(var, param)`
with a multi-target `Transition` and also an `Assign` after it or a `Choose` before it
([Effects](#per-construct-argument)). The build enforces it: an action that writes one slot
twice fails with `ConflictingEffects` (`Action::slots_written_twice`, checked by
`build/mod.rs::Builder::one_write_per_slot`, which `Builder::finish` calls; corpus case
`safety_created_set_beside_transitions`). The argument also assumes that no two params of one
action that can bind one slot write the same variable, which no check enforces
([Effects](#per-construct-argument)). Steps 1 and 2, and step 3 per binding on unreduced
cones, are also checked as graph equality on every sample cone that fits the comparison's
cap, with edges compared with their params ([Sample comparison](#the-sample-comparison));
step 3 on a reduced cone's class moves, which print no params, and step 4 are checked by
action and by verdict comparison only.

### Per-construct argument

**Variables and domains [paper; test].** Each variable of D2 prints as one TLA+ variable,
a function over `<E>_Ids == 1..n_E` (or a scalar for `now`), and its domain prints through
`mod.rs::render_value` and `domain_set`. `render_value` is injective on each domain: the
three existence values, Enum values as quoted names, Bool as `TRUE`/`FALSE`, nullable Enum
and Bool as strings with `"<null>"` first (no Enum value can be spelled `<null>`, since a
value is an identifier), a `Domain::Nullable` column as `"null"` or `"present"`, nullable Int
as `-1` then `0..m`, owners as slot `+1` with `0` for an unlinked optional key. So `dec` is a
bijection on `TypeOK` valuations (step 1).
`TypeOK` is an `INVARIANT` of every cfg `verify` and the comparison check (all print modes
but `Explore`, which lists no invariant), so a step of `𝓜` that leaves the domain would
be reported by TLC [assumed: TLC]. The test `engine_equivalence.rs::tla_names_cover_every_slot`
checks the naming; the graph comparison checks `dec` on every reached state [test].
Slot `s` of a variable is the TLA+ key `name[s + 1]` (`names.rs::TlaNames::canon`, which
the decoder's key table inverts), and `<E>_Ids` is `1..n_E`, so the quantifier
`\E p \in <E>_Ids` ranges over exactly the slots of D7. (`pred.rs::at` only names a
parameter `p<n>` or a bound variable `b<n>`; the `+ 1` lies in the value range `1..n_E`.)

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

**Effects [paper; read for the multi-target cases; the effect-order invariant enforced by the build].**
`module.rs::effects` folds the effects into one `v' = [v EXCEPT …]` per written variable and
`UNCHANGED` for the rest, and `moves.rs::each_successor` applies them to a copy of the
pre-state. The fold is per `(var, param)`: the printer writes one
`v' = [v EXCEPT ![p0] = …, ![p1] = …]` with the params in ascending order, and the native
step writes by slot in effect order. If two params of one action bound one slot and both
wrote `v`, TLA+'s `EXCEPT` would keep the higher param's value and the native step the later
effect's, and on a delete the printer resets only the deleted param's entries while the
native step drops every choice keyed by that param. Neither `candidate_slots` nor
`bindings_for` keeps two params of one entity apart, and `Action::slots_written_twice` and
`Builder::one_write_per_slot` compare `(var, param)` pairs, not slots. No builder path is
known that gives one action two params of one entity that write the same variable, but none
is shown to be impossible either: the argument below assumes it, and it is not checked. Case
by case:

- `Create`: both set the entity's `Exists` to `present` at the new slot, at once and in effect
  order (`Post::Expr` of `Value::PRESENT` against `next.set`).
- `Assign` with a literal or a parameter: the same value; a later assignment to the same
  slot replaces an earlier one in both.
- `Transition` whose every source has one target: both read the source on the pre-state. A
  matching arm writes its target; no matching arm leaves the earlier assigned value
  (`CASE … OTHER -> prev` against "no change" to the copy). The printer's shortcut for one
  target under a `transitions` guard (`from_guarded`) writes the target unconditionally,
  which is equal because the guard holds the column at a source.
- `Choose`: applied after every other effect in both (`\E v \in {...}` against forking
  `choices` after the loop); a plain assignment does not override it in either, and a
  later `Choose` of the same slot replaces an earlier one in both.
- `Transition` with some source that has two or more targets. The engines decide when to
  make a choice differently. Natively the choice depends on the *current* source: two or
  more matching arms are forked after every other effect; one matching arm is written at
  once, so a later `Assign` of the slot overwrites it; no match leaves the slot. The
  printer decides per effect: when *any* source has two or more targets, the whole effect
  becomes `Post::ChooseIn` (`CASE cur = f -> {targets} … OTHER -> {prev}`), so a
  single-target source and the no-match case become one-element choices; a `ChooseIn`
  replaces an earlier `Choose` of the slot, and a later `Expr` (an `Assign`) is dropped.
  Tracing the four orders on a state whose current source has at most one target: an
  `Assign` *before* the transition gives the same successor in both (target, or the
  assigned value through `prev`), and so does a `Choose` *after* it (the `Choose` wins in
  both). An `Assign` *after* it differs (native: the assigned value; printer: the
  transition's target, or on no match the pre-state value), and so does a `Choose`
  *before* it (native: the `Choose`, forked last, wins; printer: the transition's target,
  or the pre-state value, since `prev` is the pre-state when the slot held a `Choose`). On a
  state whose current source has two or more targets, all four orders agree. The no-match
  case cannot arise in an action built from `transitions`, whose `TransitionFrom` pre-guard
  holds the column at a source. The post-guards (`build/transitions.rs::arm_guards`: one guard
  per `Transition` or `Choose` effect that has a `.when` arm or value) filter the successors
  the same way in both engines, so they reconcile the two effect readings only when they leave
  a single value: when both the `Choose` and the `Transition` carry such guards, both engines
  keep exactly the current source's target `t`, if the `Choose` guard allows it; when the
  `Transition`'s arms carry no `.when`, the native step keeps every enabled `Choose` value and
  the printed step keeps only `t`. So step 3 holds for multi-target transitions only under a
  builder invariant: no action has an `Assign` after, or a `Choose` before, such a
  `Transition` on the same `(var, param)`. The build enforces it: an action that writes one
  slot twice fails with `ConflictingEffects`, the created-row shape (a guarded `sets` and a
  `transitions` of one created row's column in one use case) included.
- `Delete`: both reset `Exists` to `deleted` and every column of `decl.columns` to base,
  and keep owners (D8). Both drop a choice on the deleted slot (`moves.rs` retains only
  choices on other params; the printer overwrites the slot with the base value).
- `Tick`: the action's guard `now < m` is an ordinary guard (`build/vars.rs`), and the
  effect prints `now' = now + 1`.

Hence step 3 for unreduced systems, under the assumption about params stated above. Step 3
includes self-loops: a step whose successor equals its pre-state satisfies the action formula
in both, and `vars' = vars` makes it a stuttering step in TLA+. That matters for fairness and
`after` (below).

**Predicates and null [paper; test].** `pred.rs` mirrors D5: quantifiers range over present
slots; `Count` is `Cardinality` of the present slots that satisfy the body; `In`/`Not In`
on a slot is `\in`/`\notin` of rendered values, and otherwise the disjunction of
equalities. In `numeric`, `null` is `-1`, which lies outside `IntRange`; `=` and `#` compare
it as a value, so `null` equals only `null` and differs from every number, as D5 says; an
ordered comparison conjoins `x # -1` for each nullable operand, so `null` is never
ordered. A slot index read as a number prints as `(p - 1)`, the slot itself, since a TLA+
index is the slot plus one, as `expr.rs::compare` reads `Operand::Slot`. Non-numeric
operands compare by rendered value, which is index equality because `render_value` is
injective. An owner against a slot index is `owner = index`, where an unlinked owner `0`
equals no index. The native side of these rules has Kani harnesses (`core/expr.rs`:
`numbers_compare_as_i64_and_null_is_never_ordered`, `compare_negates_and_mirrors`); the
printer's side has none, and the two are tied by the corpus cases `safety_null_*` and the
fixtures `bfs_and_tlc_agree_on_null_in_a_check` and `bfs_and_tlc_agree_on_set_membership`
[test].

**Int-like columns and `now` (T16) [paper].** Both engines take the range from one
`Bounds` value: `Domain::Int` is `[m+1]` natively, and the module prints `IntRange ==
0..m` from `ts.bounds.int_max`. Literals and `@default`s outside the range are build errors
(`IntLiteralOutOfBounds`), so no effect writes outside the range. `Tick` stops at `m` in
both through the same guard. Tests that run both engines on Int-like columns and `now`
[test]: the corpus cases `z3_int_and_now_boundary` (Int `0..5`) and `z3_now_past_the_top`
(`--int-range-max 6`), whose headers give TLC verdicts that rules 1 to 3 hold TLC to; the
fixtures `bfs_and_tlc_agree_on_a_date_sets` (a `Date` column on the `now` clock) and
`bfs_and_tlc_agree_on_two_actions_of_one_name` (its `tick_use_case` model, a deadline passed
by `Tick`), graph comparisons at 1 and 2 instances and Int `0..5`; and the sample graph
comparison, which decodes every Int column of every reached state. TLC runs beside BFS at no
`--int-range-max` but 5 (the default), the 6 and 7 of four `z3_` corpus cases, and the 255 of
`width_nullable_int_top`, which the build rejects.

**Split use cases and run marks [paper; test].** The run mark is a variable like any
other: `UC_1` guards it at `none` and sets `1`, `UC_i` binds the `Earlier` row by the mark
and sets `i` or `none`, and a delete does not touch it in either engine (`build/vars.rs` puts
only column and driven-comparison variables in `decl.columns`, which is what a delete resets).
Tested by `bfs_and_tlc_agree_on_split_use_cases`, `…_on_a_guard_over_every_group`,
`…_on_an_interleaving_between_two_steps`, and the corpus case `fair_split_use_case`.

**Events: latches and counters [paper; test].** Raised latches and counters are variables
of D2 with guards and assignments from the builder; the printer has no code of its own for
them beyond the generic action path. A cone that leaves the mark out drops the conjunct and
the `Triggered` parameter in the system, so both engines read the same over-approximation;
what that cone's counterexample means is a question for the cone, not for agreement
([cones.md](./cones.md#safety-cones)). Tested by the corpus cases `fair_triggered_handler`,
`fair_counted_exposes_lost_run`, `safety_counted_once_per_raise`, `safety_event_overflow`.

**State rules [paper].** `module.rs::checks` prints each rule as `\A b … : (present …) =>
(body)` (`closed`), the binding-over-present-slots reading of D16. A rule over an entity
with no present slot holds vacuously in both.

**`after` assertions [paper].** `AfterAssert_i == [][(\A p : UC(p) => post')]_vars` over the
last step of each variant. Because `UC(p)` is a predicate on state pairs, it holds of
`(s, s')` exactly when `s' ∈ Post_UC(s, p)` (step 3), so the TLA+ property quantifies over
the same steps as D17. `[A]_vars` is true on every step with `vars' = vars`, so neither TLC
nor the native checker checks an `after` on a step whose successor equals its pre-state. The
native checker skips that check explicitly (`core/explore/bfs.rs::Bfs::take` continues before
`check_after` on `Probe::Loop`, which `explore/probe.rs::Probes::read` sets for a successor
equal to its source in every build, independent of `Probing`). D17 and the language
reference state the exemption, and a last step that only stutters is reported as vacuous. The
engines agree.

**Temporal properties [paper].** `checks` prints the binding quantifier `\A b …` in one of
three places: outside the temporal operator for `always`, `eventually` and `leads_to`
(`P == \A b : F`, or `P == Fair_P => (\A b : F)`); inside the action of `[][…]_vars` for
`stable` and `unchanged`; and inside `[]` for `precedes`. Per operator:

- `always(x)`: `[](trig => x)`, D19.
- `eventually(x)`: `[](trig => <>x)`, which is D21 with `cause = true`: `<>` includes the
  current state, as D21 reads `goal` at `j = i`.
- `leads_to(x, y)`: `(trig /\ x) ~> y`, by definition `[]((trig /\ x) => <>y)`, D21.
- With `.present()` the build conjoins `pres` into the goal predicate before printing, so
  the printed goal is `pres /\ x` (`pres /\ y`), D21's goal for the modifier; the steps
  above apply to it unchanged.
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
outside and the `New` parameters inside; when a scope keeps no action of the listed use
cases, `Fair_P` is `TRUE`. In TLA+, `WF_vars(B)` requires `<B>_vars` to occur
infinitely often if it is continuously enabled, where `ENABLED <B>_vars` asks for a
`B`-step that changes `vars`. That is D20 per fair instance: enabled means a successor
`s' ≠ s`, taken means a step of that instance to another state. Two consequences that
the native checker must match, and that have corpus cases:

- a self-loop is neither enabling nor a step (`fair_selfloop_not_enabling`);
- a step that some other action also produces satisfies `<A_a(κ)>_vars` and so counts for
  the fair instance, because `A_a` is a predicate on state pairs, not an action label
  (`fair_same_edge_two_bindings`).

The property is printed as `Fair_P => (…)`. `ConeProperty(i)` omits `WF_vars(Next)`
(`Plain` in D20). The other modes of `print` conjoin it to `Spec` exactly when `T'` has a
liveness property ([Module](#terms)), and `print_state_models` (the `Deadlocks` and
`Witnesses` modules) never does; the `_WF` run's module is `Property(i)` of a liveness
property, so it always has it (`WfNext`). Where a module has it but checks only safety, it
is harmless: `WF_vars(Next)` is a fairness condition on a subaction of `Next`, so `Init /\
[][Next]_vars /\ WF_vars(Next)` is machine closed (Lamport 2002, §8.9; the notion is
Abadi and Lamport's, 1991), and adding it changes no safety verdict. TLC's support for
fairness quantified over a finite constant set is [assumed: TLC].

**Reduced cones [test; no paper argument; the quotient tables are common-mode].** A cone
reduced by its sub-lifecycle prints a class variable and tables computed in Rust
(`module.rs::reduction`, `bits`, `rise_expr`): the observed values of each class's
representative, the class successors per action of the sub-lifecycle, and the `Rise` table
from inputs and bits to the next class. TLC therefore explores the quotient as the Rust
code computed it. It checks the native exploration of that quotient, not the quotient's
soundness (A8). This document gives no paper argument that the printed class move (the
`is_class_move` branch of `action`), the `Rise` lookup, and the bits denote the native
reduced step (`moves_of_into` with a reduction). Step 3 on reduced systems therefore rests on the graph comparison
of reduced sample cones and, on the corpus, on the refinement check of
[The reduction, step by step](#the-reduction-step-by-step), which holds every step of the
unreduced cone to a step of the printed reduced module. The sample comparison does compare the reduced cone's violated
checks with the unreduced cone's, through TLC on both
([Sample comparison](#the-sample-comparison)), but only on cones whose unreduced BFS fits
its cap. TLC never checks a property under `WF_vars(Next)` on a reduced module: the `_WF`
module is printed from the unreduced cone (`property_cone.rs::PropertyCone::bundle`), while
the native `WfNext` run may run on the reduced cone, which is compared only through the
final verdict ([§2](#2-the-agreement-rules)). A reduced safety cone's module does carry
`WF_vars(Next)` when the cone keeps a liveness property (`emit/tla/mod.rs::print_with` adds it
in every mode but `ConeProperty` when `needs_fairness`); its cfg checks safety only, which
the fairness conjunct does not change, as above.

**What the module does not state [read].** `can_reach` prints as a comment: TLC cannot state
`AG EF`. `core/tests/oracle.rs::can_reach_and_deadlocks_agree_with_a_naive_oracle` (main
only) checks `can_reach` and deadlocks against an exploration of its own instead. Deadlocks,
the finding of D25, are not TLC's `CHECK_DEADLOCK` (always `FALSE`) but the `Deadlock_*`
invariants of `print_state_models`, which are one-sided by design (T6).

### Status of T1

[read] The table sums up the argument above and the tests of [§3](#3-differential-evidence).

| Step of T1 | Paper argument | Test | Assumed |
|---|---|---|---|
| 1, 2, 3 (states, init, steps) on unreduced systems | above; step 3 for multi-target transitions under the builder's effect-order invariant, which the build enforces (`ConflictingEffects`), and for every effect under the assumption, not checked, that no two params of one action that can bind one slot write one variable | graph equality on every sample cone under 300,000 states, at 1 and 2 instances, edges with their params; the 36 `bfs_and_tlc_*` tests of `engine_equivalence.rs` | TLC's `-dump dot` lists every edge, self-loops included |
| 3 on reduced systems | none (not argued here) | graph equality on reduced sample cones, class moves by action alone; the reduced cone's violated set against the unreduced one's; on the 21 reduced corpus cones, every step of the unreduced cone a step of the reduced module or a stutter ([The reduction, step by step](#the-reduction-step-by-step)) | the quotient tables (common-mode) outside the corpus |
| 4, state rules and `after` | above, with D17's stuttering exemption | verdict and violated-set comparison on every compared cone; each TLC trace replayed through the native step relation | TLC |
| 4, temporal properties and fairness | above | verdicts only: 23 `TEMPORAL_CASES` at 2 instances (under `-lncheck final`, [Batch JVMs](#batch-jvms)) and the corpus at each case's flags | TLC's liveness checking under quantified weak fairness |
| 5, latch | above | verdicts only: `explored_along` (a `TEMPORAL_CASES` fixture) and the corpus case `safety_precedes_latch`. The graph comparison prints `PrintMode::Safety` modules, which declare no latch (`mod.rs::checked_properties` checks no property in that mode), so it never sees one | |

[read] Step 3 is checked per binding: the graph comparison prints the cone with one `Next`
disjunct per binding (`tla::print_bound`), TLC names each edge after it, and edges compare as
`(from, to, action, params)` (`engine_equivalence.rs`, `type Move`). A class move of a
reduced cone prints with no params, so its edges compare by action alone.

## 2. The agreement rules

<!-- derived-from ../engine-agreement.md#agreement-rules -->
<!-- derived-from ../engine-agreement.md#known-disagreements -->

**What is compared at run time (T2, T3) [runtime].** Under `verify --with-tlc`,
`cli/src/verify/compare.rs::disagreements` compares, per job:

- a safety cone: the two verdicts, and when both are `counterexample`, the set of checks
  each run violates, TLC's read by replaying each TLC trace (`tlc_replay::explain_violation`);
- an `Explored` property (`stable`, `unchanged`, `precedes`): the final verdicts, the
  whole-model run's where a cone left the property open;
- a property cone: each run in `Native::runs` against the TLC run on the same system
  (`RunOn`: property, graph, fairness), then the final verdicts. For a liveness property
  whose `Plain` run on a reduced cone finds a counterexample, `native.rs::Plain::of`
  (called from `native_property`) first runs `ComponentRun::decide_liveness_reduced` on the
  reduced cone (every class that can loop taken as a stop; A15), and when that returns
  `Holds` or `Violated`, `native_property_after` records it as the native `WfNext` run.
  TLC's `WfNext` run is always the `_WF` module, which `PropertyCone::bundle(ConeRun::WfNext)`
  prints with `PrintMode::Property` from the unreduced cone (`verify/jobs.rs`). That native
  run, printed `WF_vars(Next) on the reduced cone (native only)`, meets no TLC run and is
  compared only through the final verdict ([Scope](#what-agreement-can-show)).

`differ` treats a pair as a disagreement when both are `Some`, they differ, and neither is
`ToolError`. Each disagreement prints `error[EngineDisagreement]` and makes the run's
`result:` `tool_error` (`cli/src/verify/mod.rs`). State models are compared separately
(`state_models.rs::state_model_disagreements`, one-sided for deadlocks).

**Every TLC trace replays [runtime].** `compare.rs::unreplayed` replays every TLC trace of
every job, whatever the native verdict, and a job with one that does not replay prints
`error[EngineDisagreement]` and ends `tool_error`.

**Caps are asymmetric [read].** Native cone runs stop at `--max-states` or the memory limit,
never at the time budget: the cone `Caps` that `verify/mod.rs` builds carry no deadline, and
the only `.until(…)` in `verify/` is `WholeRun::caps`, which only whole-model runs use
(`Native::capped` says "stopped at the state cap (--max-states) or the memory limit"). TLC
cone runs have no cap at all: `verify/tlc.rs::run_cfg` runs TLC to the end, and only
whole-model runs (`run_whole`) are killed at the budget or the state cap. So a native cone
run can stop where TLC's run of the same module decides.

**Decided against undecided [read].** Rule 1 says `not decided` from either engine is no
disagreement. The code keeps it for final verdicts, where `Decision` maps `Undecided` to
`None`, and per run: `verify/native.rs::Answer::outcome` maps `Answer::Stopped` to `None`, so a
native run stopped at a cap is recorded in `Native::runs` as not decided and compared with
nothing. This holds for every push into `Native::runs` (the `Plain` and `Full` runs of
`native_property_after` and its unreduced `WfNext` run, the `Plain` run of
`always_up_to_symmetry`, and the `Plain` and `WfNext` runs of `liveness_up_to_symmetry`); the
reduced `WfNext` run records no entry when undecided.

**The corpus harness's own reading [read].** In `cli/tests/engine_corpus.rs::check_case`,
rule 2 (`rule_2`) fails a native `Cone_` line without TLC's line for the same module when the
header gives its checks a TLC verdict, and rule 8 (`rule_8`) fails a check the plain run
decides and the symmetric run does not decide the same way, `not decided` included;
`rule_2_fails_a_tlc_line_the_header_expects_and_the_run_lacks` and
`rule_8_fails_a_check_symmetry_leaves_undecided` test both on canned output. Rule 1 in the
harness reads the printed verdicts, so it inherits the run-time comparison above.

**Known disagreements [read].** The corpus has none: no case carries a `known` line (0 of 89).

## 3. Differential evidence

<!-- derived-from ../engine-agreement.md#the-corpus -->
<!-- derived-from ../engine-agreement.md#the-sample-comparison -->
<!-- derived-from ../engine-agreement.md#the-reduction-on-random-models -->
<!-- derived-from ../cli-reference.md#partial-runs -->

Differential testing (McKeeman 1998) runs two implementations of one specification on the
same input and treats a difference as a fault in one of them. Its power is bounded by the
inputs, and it is blind to faults the implementations share. Each source below is
described by its inputs, what it compares, and what it cannot detect. Counts are counted in
the code; a measured number says where it comes from.

[read] for every description in this section of what a test, script, or job runs and
compares; what the test shows when it passes is [test].

### The corpus

`cli/tests/engine_corpus.rs` over `cli/tests/engine_corpus/*.rdra` (T7).

- **Inputs.** 91 cases with 218 `expect` lines; families `cap` 2, `cone` 16, `fair` 9,
  `presence` 9, `safety` 25, `sym` 10, `width` 2, `z3` 18. One case (`cap_memory_limit`)
  skips TLC. 38 cases carry a `bfs=` list.
- **Bounds.** TLC runs at the case's flags: 2 instances and Int `0..5` by default; 1 instance
  in the seven cases with `--instance-count 1`; per-entity counts above 2 in three cases
  (`presence_per_entity_slots`: Loan and Room at 3 over `--instance-count 1`;
  `safety_count_per_entity`: Loan at 4, Shelf at 3; `sym_per_entity_bounds`: Room at 3,
  Booking at 2); one entity held at 1 instance (`--instances X=1`, the others at 2) in four
  cases (`sym_arm_reorders`, `sym_deleted_row_owner`, `sym_lift_compose`, `sym_relink_loop`);
  `--int-range-max` in five cases (5, 6 or 7 in four `z3_` cases, 255 in
  `width_nullable_int_top`). The `bfs=` runs at 1, 3 and 4 instances run TLC beside BFS too.
- **Compares.** Each engine against the hand-written header, rules 1 to 3 against TLC,
  rule 8 against the symmetric native run.
- **Detects.** The header is the only check of common-mode code outside the reference
  interpreter's subset: the planted-bug table of
  [What the corpus catches](../engine-agreement.md#what-the-corpus-catches) records two
  planted bugs in the cone ladder that moved both engines and were caught by the headers
  alone.
- **Cannot detect.** A trap no case names. A header written wrong in the same way as the
  code (the header is reasoning by the same people who wrote the code). Deleting a case is
  caught: `every_corpus_header_is_well_formed` compares the cases with
  `tests/engine_corpus/CASES` both ways.

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
  states, the edges as `(from, to, action, params)` from a module printed with one `Next`
  disjunct per binding (`tla::print_bound`); the state-model findings of every entity on
  both graphs; the violated invariants and `after` assertions, rerunning TLC without each
  found check until no new one appears; and each TLC trace replayed through the native
  step relation (`replay_violations`). `a_corrupted_tlc_trace_does_not_replay` checks that
  the replay rejects a corrupted trace.
- **Unreduced and reduced.** `check_one_cone` compares the unreduced cone only when its BFS
  completes under `STATE_CAP` (300,000 states), and the reduced cone only when its run
  completes under the same cap. A cone joins `VERIFY_FALLBACK` (25 cones) when its reduced
  run at 2 instances passes the cap. A cone whose unreduced BFS passes 300,000 states while
  its reduced BFS fits is compared reduced only; `VERIFY_REDUCED_ONLY` lists those (30
  cones), and the test fails when the list and the run differ. On such a cone TLC reads the
  Rust-computed quotient tables, so the comparison says nothing independent about the
  reduction there. A reduced cone's class move prints with no params, so its edges compare
  by action alone.
- **Cannot detect.** Temporal properties (not printed in `PrintMode::Safety`); anything at
  3 or more instances; any cone of the 25 at 2 instances.

`a_property_cone_decides_what_the_whole_model_decides` (T3) runs the property ladder on 23
`TEMPORAL_CASES` (7 sample files and 16 inline fixtures), at 2 instances, each run of each
property against TLC's run of the same module, and the cone's final verdict against the whole
model's TLC verdict. It compares verdicts, not graphs. Its native `WfNext` run is always
`Fairness::WfNext` on the unreduced cone, as TLC's is; it never calls
`decide_liveness_reduced`, so the reduced-graph `_WF` search is compared with TLC only under
`verify --with-tlc` (the corpus harness passes `--with-tlc` to every case but
`cap_memory_limit`; the `cone_reduced_*` cases are written for A15). That
`cone_reduced_no_loop_capped` reaches `decide_liveness_reduced` follows from the code; no test
asserts it.

### `verify --with-tlc` on the sample roots

`scripts/verify-samples.sh --with-tlc --tlc-roots scripts/tlc-roots.txt` in CI's
`formal-verify` job runs TLC beside the native checker on the five roots that have a
`VERIFY_FALLBACK` cone (clinic-ops, patent-ops, food-permit-ops, insurance-claim-ops,
oidc-auth) and on every `verify` header line with flags, at each root's header bounds. The
domain roots' temporal properties are compared with TLC only here. It is held to the run-time
rules of [§2](#2-the-agreement-rules).

For oidc-auth the header bounds do not reach the 13 `VERIFY_FALLBACK` cones at 2 instances.
Its headers (`samples/oidc-auth/buc/buc_ciba.rdra`) run `verify --instance-count 1 --prove
--deny-undecided`, and at 2 instances only `verify --deny-undecided --entity
BackchannelRequest --entity DeviceAuthorization --entity LogoutNotice --entity SigningKey`.
Under `--entity`, a run checks the selected entities' views and the invariants and `after`
assertions that bind them (cli-reference.md, "Partial runs"); the 13 cones are the views of
eight browser-flow entities and five invariants over Interaction, AuthorizationCode, Grant,
RefreshToken, and AuthorizationRequest, none of which that line selects. So no CI job runs
TLC on those 13 cones at 2 instances, not even as a verdict comparison.

The other four roots' 12 fallback cones run at the default 2 instances here, but by the code
only the 5 that are cones of invariants or `after` assertions get a TLC run: `verify` gives
an entity target no TLC safety job (`verify/jobs.rs::cone_jobs`), and runs an entity cone's
state models under TLC only when its BFS completed within `STATE_MODEL_TLC_STATES`
(100,000) states (`verify/bfs.rs::state_model_cones`), which a fallback cone, past 300,000
states at 2 instances, does not. So the 7 entity cones among the 12 meet TLC at 1 instance
only, in `engine_equivalence`, unless one has the same variables as an invariant or `after`
cone of the run, whose TLC job then explores the same module. Which of the 7 do is open; the
gap is claims row [T5](./claims.md#d-agreement-with-tlc).

### Random models

`scripts/reduction-diff.py` (T9) generates models of three to five entities with chains,
quantified guards, `.along`, a split use case, `chooses`, a timer, `after` assertions and
properties, at 2 or 3 instances, and compares `verify --deny-undecided` reduced against
unreduced. With `--with-tlc` both runs also run TLC, and a seed with
`error[EngineDisagreement]` is listed as `disagreed`. In CI, a pull request runs the 522 seeds
of `scripts/reduction-diff-pr-seeds.txt` without TLC, and main runs 45 fixed seeds with TLC,
some at 3 instances (`ci.yml`, the `formal-verify` job). The runs fail when they compare or
reduce fewer seeds than a floor (`--min-compared 500 --min-reduced 200` on a pull request,
`40` and `20` on main); a comment there records 284 and 33 seeds that reduce a cone. TLC
therefore sees 45 random models per main run, the same 45 each time. The generator draws no
`Count` predicate (A11), so count predicates get no random coverage. Counted triggers have a
generator mode, `reduction-diff.py --counted` (`.counted(k)` with `k` of 1 or 2 on every
`triggers` line), which no workflow passes.

The first generator rarely draws a param on a stub (claims row
[A8](./claims.md#a-cones-of-influence-and-reductions)): of the 522 pull-request seeds, one binds a
fold's `Triggered` param on a stub and none a rise's; of the 45 main seeds, none. A second
generator, `reduction-diff.py --stubs`, draws cones that reduce with a folded S action's
`Triggered` param on a stub and a rise's `Parent`, `Existing` or `Triggered` param on one,
nested reductions among them. Both workflows run 95 of its seeds without TLC, and the
`shadow` job runs 30 of them with S up to `H`. The runs fail when fewer seeds than a floor
reach each shape. No TLC run sees these models.

### The reduction, step by step

<!-- derived-from ./cones.md#the-reduction-by-an-independent-sub-lifecycle -->

`cli/tests/engine_corpus.rs::tlc_refinement_holds_on_every_reduced_cone_of_the_corpus` (A8,
T1 on reduced systems) has TLC check a reduced cone `R` against its cone `C` step by step,
not only by verdict. [read] for the description; what a pass shows is [test].

- **Modules.** For each reduced cone of a corpus case at the case's header bounds (the cones
  of its targets and of its properties, each distinct pair of modules once), the test prints
  `C`, the cone unreduced, and `R`, the cone reduced by that target's seed alone (as
  `a_reduced_wf_next_run_decides_what_the_unreduced_one_decides` reduces it, not with the
  seeds of every target sharing the cone united), both in `PrintMode::Safety`, and a third
  module from `emit/tla/refine.rs::print_refinement`. That module `EXTENDS` `C` and
  instantiates `R` with its class variable set to `RefClass`, the class of the state's key
  `x(t, s)`: the S variables and raw inputs as `C` holds them, and the input bits through
  `R`'s own `In_<g>` operators. The class comes from a table of every state the extended
  sub-lifecycle's run reaches, with the class of its block
  (`core/reduce/table.rs::TransitionSystem::class_table`, the run and partition that `lift`
  uses), printed as a hash table: the module hashes the key's codes as the printer does and
  searches one bucket. `RefClass` is 0 for a key the table does not list.
- **Checks.** The cfg is `C`'s `Init` and `Next`, `INVARIANT RefReached` (`RefClass # 0`) and
  `PROPERTY RefSteps` (`RefR!Init /\ [][RefR!Next]_RefR!vars`). On every reached state of `C`,
  the key is a state of `X_n` (the first conjunct of
  [Lemma 6](./cones.md#the-reduction-by-an-independent-sub-lifecycle)). Every initial state of
  `C` maps to an initial state of `R`. Every step of `C`, its S part replaced by its class, is a
  step of `R` or leaves `R`'s variables alone: an `S_a` step is a class move of the printed
  `Succ_` table or stays in its class, and a `T` step that changes the inputs moves the class
  as `R`'s `Rise` lookup does, stage by stage. So it checks the `C ⇒ R` half of Theorem A8's
  correspondence on the reached states of `C`, and the printed `Rep_`, `Succ_` and `Rise`
  tables with it.
- **Independence.** TLC explores `C` from `C`'s module, whose agreement with the cone is T1
  for unreduced systems, and computes `R`'s steps from `R`'s module. The check shares with the
  reducer the extended sub-lifecycle and its partition, through the table. It shares no
  class move, `Reduction::recanon`, `plain_moves` or lift. A step the extended sub-lifecycle
  drops leaves its successor's key out of the table, which `RefReached` reports.
- **Nested reductions.** The run of an extended sub-lifecycle that is itself reduced holds
  each state with its inner S part replaced by the inner class's representative. Its table
  lists, for each such state, every state whose inner S part is in that inner class, from the
  inner reduction's own table, so the key a cone state has is listed. Two corpus cones are
  nested. A sub-lifecycle explored up to `H` (#212 phase 2) is refused
  (`NoClassTable::UpToH`): its run stores one form per orbit, which is not a key of `C`.
  A table past 200,000 states (`CLASS_TABLE_CAP`) is refused too. The test pins 21 cones
  checked and none refused.
- **Planted faults.**
  `::tlc_refinement_catches_class_moves_left_out_that_tlc_on_the_reduced_cone_misses` empties
  the first five nonempty cells of `Succ_Use` in `R` on `cone_triggered_param_on_a_stub`'s
  cone of `Order`. TLC on `R` alone prints the same verdicts as on the unplanted `R`, and the
  check reports `RefSteps` violated. `::tlc_refinement_catches_a_param_left_on_a_stub`
  (shadow builds only) runs the check in a process with
  `RDRA_ISH_DROP_RULE=bind_stub_params_unchecked`, which leaves every parameter on a stub
  unbound and skips the stub check, so that the extended sub-lifecycle drops `Issue` and
  `Note`'s rise. `RefReached` is violated on `cone_triggered_param_on_a_stub` at `Issue` and on
  `cone_rise_param_on_a_stub` at `Note`'s rise.
- **Cost.** 17 TLC runs of at most 1,680 table states, four at a time: about 14 s locally
  in release for the two tests that run in every release job.
- **Cannot detect.**
  - The `R ⇒ C` direction, which would hide the S variables outside `obs`. A16's lift and
    replay certify that direction for each printed counterexample.
  - Fairness through the mapping. TLC (tlaplus 1.7.4) does not evaluate `ENABLED` of an
    instantiated action whose variable is substituted by an expression over the outer
    module's variables: a property such as `WF_<<RefR!vars>>(\E p \in Order_Ids :
    RefR!Pay(p))` stops with "the identifier `Ticket_exists` is either undefined or not an
    operator". So A15's `WF_vars(Next)` argument is not checked here.
  - A cone whose `C` TLC cannot explore. The check runs on the corpus only: not on the
    samples' reduced cones (the 30 `VERIFY_REDUCED_ONLY` cones among them), nor on the
    `reduction-diff.py` seeds, which run `verify` through the CLI, and the CLI prints no
    refinement module.
  - A shape no corpus case has, the limit of [the corpus](#the-corpus).

### Batch JVMs

Most of `engine_equivalence`'s TLC runs take milliseconds inside a JVM that takes far longer
to start. The flake's `tlc-batch` (`nix/tlc-batch/RdraTlcBatch.java`) keeps a JVM up and runs
one TLC job after another in it; `cli/src/tlc_batch.rs` pools one JVM per test thread. This
bears on the test harness only: `verify --with-tlc` does not use batch JVMs.

- **Isolation.** Each job loads TLC through a class loader of its own and runs in a thread of
  its own, and the driver removes TLC's MBeans and its RMI export afterwards. The JVM, its JIT
  code, and its `java.io.tmpdir` are shared, one job at a time.
- **Which runs.** A job routed to a batch JVM (`Route::Batch`): every cone of at most
  `BATCH_STATES` (5,000) states, every state-model run and the other runs through `run_tlc`,
  whose size is not known up front (fixtures among them), and a property cone's `Cone_P`,
  `Cone_P_Full` and `Cone_P_WF` runs. A larger cone and a property's whole-model run are
  processes from the start (`Route::Process`, `check_cone`).
- **Memory.** A batch JVM gets 512 MiB of heap, and the pool has at most as many JVMs as the
  test has workers. The JVM needs `-XX:SoftRefLRUPolicyMSPerMB=0`, because the RMI runtime
  keeps the classes of every exported object behind a soft reference.
- **Fallback.** A job with no answer runs again as a process: the driver did not start, the JVM
  ended under the job, or the job took over 300 s (`BATCH_LIMIT`). After three lost jobs in a
  row every later job is a process. A JVM is replaced after 400 jobs, or when its jobs have
  left 8 threads behind (TLC 1.7.4 never shuts down the executor of a liveness
  counterexample).
- **Comparison (T11).** `RDRA_TLC_BATCH=both` runs each batch-routed job in a batch JVM and
  then as a process with the fingerprint and seed the first run drew (`-fp`, `-seed`), and
  fails on any difference in exit status, log, parsed run, invariant scan, or dumped graph
  (`assert_same_run`), apart from timestamps, timings, process ids, temp paths, timer output,
  and the heap size. A job routed to a process from the start is never compared this way.
  `RDRA_TLC_BATCH=twice` compares two process runs the same way, and `RDRA_TLC_BATCH=0` runs
  every job as a process. The `#[ignore]`d `recorded_tlc_runs_are_the_same_in_a_batch_jvm`
  replays recorded `verify` runs the same way, by hand.
- **In CI.** The main-only `tlc-batch-both` job of `ci.yml` runs `engine_equivalence` under
  `RDRA_TLC_BATCH=both` (scheduled or by hand, never on a pull request).
  `RDRA_REQUIRE_TLC_BATCH=1` (`engine_equivalence.rs::assert_batch_ran`) fails the test unless
  some job was answered in a batch JVM and the pool stayed sound (`tlc_batch.rs::Pool::sound`):
  every lost job was one past the time limit, and the pool switched to processes for no other
  reason.

Every TLC run `engine_equivalence.rs::run_tlc_on` starts, batched or a process, passes
`-lncheck final` (`LIVENESS_AT_THE_END`); that includes every cone, state-model and property
run of the comparisons, and every replay of `recorded_tlc_runs_are_the_same_in_a_batch_jvm`.
So TLC checks liveness once, at the end of the search, rather than also at times its clock
sets; the constant's doc comment gives the reason, a warm JVM and a fresh process that found
a violation at different points. `verify --with-tlc` passes no `-lncheck`
(`cli/src/verify/tlc.rs`). The harness's TLC evidence on temporal properties (T3) and the
batch comparison (T11) are therefore under end-only liveness checking, which a user's
`--with-tlc` run does not use; that TLC's liveness verdict on a finished search does not
depend on when it checks is assumed (TLC), not tested. The safety and state-model runs of the
graph comparison (T5) pass the flag as well. Their cfg (`emit/tla/mod.rs::cfg_without` in
`PrintMode::Safety`) lists invariants and, as `PROPERTY` lines, the `after` assertions, whose
form `[][…]_vars` TLC checks on each step rather than in its liveness check; that the flag
leaves those runs unchanged is likewise assumed (TLC).

### Mutation run

`cargo mutants` (T15) runs by hand only (`mutants.yml`, `workflow_dispatch`), in two
configurations; how to start one and read its summary is in
[engine-agreement.md, "Mutation run"](../engine-agreement.md#mutation-run). The kill rate is
the caught and timed-out mutants over the viable ones (all but `unviable`).

- **The liveness ladder** (`.cargo/mutants.toml`): mutants of `cli/src/property_cone.rs`,
  `cli/src/verify/native.rs`, `core/cone.rs` and `core/explore/liveness.rs`, 309 on this tree
  (`cargo mutants --list`), killed by the whole `engine_corpus` test with `tlc` and `z3` off
  `PATH` (the workflow checks that they are gone). A mutant is caught only when a stated
  verdict or the capped-graph test fails: the check that sees a wrong `ok` the native checker
  and TLC share through the cone ladder. It measures how well the corpus headers alone catch
  faults in the cone ladder and the liveness search, not agreement with TLC.
- **The TLC side** (`-f config=tlc`, `.cargo/mutants-tlc.toml`): 571 mutants on this tree
  (`cargo mutants --config .cargo/mutants-tlc.toml --list`) of the TLA+ printer
  (`emit/tla/{mod,module,names,pred}.rs`), the comparison (`cli/src/verify/compare.rs`), and
  the TLC readers (`cli/src/tlc_output.rs`, `cli/src/tlc_replay.rs`), killed with `tlc` and
  `z3` on `PATH` (`RDRA_REQUIRE_TLC=1`, `RDRA_REQUIRE_Z3=1`) by the corpus, the unit tests of
  the TLC output reader, the comparison and the TLC driver, and the `verify_cli` tests that
  run TLC beside the native checker, as the config's `additional_cargo_test_args` filters
  name them. `verify/tlc.rs` itself is not mutated, and `rdra-ish-emit`'s own tests are left
  out: their snapshots fail on any change to the printed text, which says nothing about
  whether agreement catches it. This is the configuration that measures how well agreement
  catches a fault on the TLC side. No run of it has completed, so it has no kill rate.

The completed full runs, all of the liveness configuration (each row's mutant count is that
of its commit):

| Run | Commit | Date | Mutants | Caught | Timeout | Missed | Unviable | Kill rate |
|---|---|---|---|---|---|---|---|---|
| [45212589802](https://github.com/manji-0/rdra-ish-dsl/actions/runs/45212589802) | `b0f593ba` | 2026-11-23 | 251 | 137 | 2 | 70 | 42 | 139 of 209, 66.5% |
| [46034352562](https://github.com/manji-0/rdra-ish-dsl/actions/runs/46034352562) | `6f2d29a1` | 2026-11-27 | 298 | 147 | 4 | 89 | 58 | 151 of 240, 62.9% |

The missed mutants by file (`cone.rs`, `explore/liveness.rs`, `verify/native.rs`,
`property_cone.rs`): 27, 26, 12 and 5 in the first run; 32, 38, 17 and 2 in the second.

Mutation analysis in the sense of DeMillo, Lipton and Sayward (1978) reports the fraction of
non-equivalent mutants killed; the recorded rates count every viable mutant, equivalent ones
included, so they are lower bounds on that fraction.

### What each source can and cannot detect

[read] The table restates the subsections above.

| Source | Native fault | Printer fault | TLC fault | Common-mode fault | Bounds and inputs |
|---|---|---|---|---|---|
| Corpus headers | yes, on the trap | yes, on the trap | yes, on the trap | yes, on the trap | 89 hand cases, TLC at each case's flags and at the `bfs=` counts |
| Corpus rules 1 to 3 | where TLC differs | where native differs | where native differs | no | as above |
| Sample graph comparison | states, edges with params, initial states, safety verdicts | same | same | no | 40 roots, 1 and 2 instances, cones under 300,000 states |
| Temporal cone comparison | liveness and ladder verdicts | same | same | no | 23 cases, 2 instances |
| `verify --with-tlc` in CI | verdicts | verdicts | verdicts | no | 5 domain roots and flagged header lines, header bounds |
| Random models with TLC | verdicts | verdicts | verdicts | no | 45 fixed seeds, main only |
| Refinement of reduced cones | the reduced step and its tables, step by step | the reduced module's tables | where TLC on the two modules differs | the extended sub-lifecycle, on the corpus traps | 21 reduced corpus cones, header bounds, safety steps only |
| Mutation run, liveness | cone ladder and liveness only, through headers | no | no | ladder only | corpus, by hand, rates in [Mutation run](#mutation-run) |
| Mutation run, TLC side | no | printer, comparison, TLC readers | no | no | corpus and CLI tests with TLC, by hand, no completed run |

## 4. Mechanised evidence

<!-- derived-from ../engine-agreement.md#kani-harnesses -->

**Kani (T13) [read].** There are 25 `#[kani::proof]` harnesses, all in `core`: two in
`expr.rs` (the comparison of D5), three in `types.rs` (the value encoding of the domains), four in `explore/store.rs`, three in `explore/segments.rs`,
three in `explore/packing.rs`, one in `explore/arena.rs`, two in `explore/table.rs`, one in
`explore/symmetry/mod.rs`, one in `prove/encode.rs` (the z3 encoding of a choice), three in
`cone.rs` (the read set of a predicate), two in `reduce/fold.rs` (the fold bit of Lemma 6). Bounded model checking (CBMC, as Kani uses it) proves each
statement for every input up to its bound. For agreement, the two `expr.rs` harnesses
matter: they fix the native reading of null and ordering that `pred.rs::numeric` must
mirror. Nothing ties `pred.rs` to them except tests. No harness concerns the printer, the
decoder, the TLC readers, or the comparison.

**Shadow twins (T14) [read].** A `shadow` build runs 26 reference twins beside fast paths
(`crates/rdra-ish-core/src/shadow.rs::TWINS`). The ones on the path TLC is compared against are
the move generation (`EachEnabled`), the edge store (`Edges`, `StepEnds`), and the reduced
class moves (`ClassMoves`). They compare a fast native path with a slower native one; they say
nothing about TLC or the printer. Their value for agreement is reach: the `shadow` job runs
them on samples and corpus cases larger than the TLC comparison covers, below each twin's cap.

## 5. Process gates

<!-- derived-from ../engine-agreement.md#mutation-run -->

[read] for this section.

| Job (`.github/workflows/ci.yml`) | Runs on a pull request | Runs on main | Compares with TLC |
|---|---|---|---|
| `engine-equivalence`: `engine_equivalence` and `engine_corpus` with `RDRA_REQUIRE_TLC=1`, and `proof_equivalence` (z3 against BFS, no TLC) | when `changes` says `verify` | yes | yes |
| `formal-verify`: `verify-samples.sh --with-tlc --tlc-roots`, reduced against unreduced on random models | when `changes` says `verify`; random models without TLC | yes; random models with TLC on 45 seeds | yes |
| `shadow` | when `changes` says `verify` | yes | yes: the corpus on a `shadow` build with `RDRA_REQUIRE_TLC=1` |
| `tlc-batch-both`: `engine_equivalence` with `RDRA_TLC_BATCH=both` | never | yes | yes: batch JVM against a TLC process |
| `kani` | when `changes` says `kani` (`scripts/kani-relevant.sh` matches a changed file) | yes | no |
| `mutants.yml` | never | never; by hand | with `-f config=tlc` |

On main, `changes` says yes to every job on a run started by hand and on each scheduled run
(every 3 hours), except a scheduled run whose commit the last finished run on main checked,
which skips every job.

Every TLC run in CI is in the flake's devShell. `ci.yml` sets `shell: nix develop --command
bash -eo pipefail {0}` as the default, and only two jobs override it with plain `bash`, the
`changes` job and the `kani` job (Kani is not in nixpkgs, so that job runs on the runner's
rustup outside the devShell, as its comment says); neither runs TLC. Of the other
workflows, `mutants.yml` takes TLC off `PATH` on purpose in its liveness configuration and
requires it in the TLC one, `portability.yml` runs with no nix and no TLC (its comment), and
`release.yml`, `publish-pypi.yml` and `verus.yml` do not mention TLC. The devShell's TLC is
`flake.nix`'s `tlcFor`: the nixpkgs `tlaplus` package overridden to the 1.8.0 pre-release
jar (build of 2026-11-19, rev `bc1b506`, by the flake's comment) on JDK 25. The CI evidence
is about that TLC; the notes also name 1.7.4, which nixpkgs ships.

**No job gates a merge** ([README, Known limitations](./README.md#known-limitations)). Main
runs on a 3-hour schedule, so a merge reaches the main-only steps (random models with TLC,
`tlc-batch-both`, the full `kani` set) hours later or, if GitHub drops a scheduled run, later
still.

**The release gate's TLC items.** The release gate of
[native-temporal-verification.md, "Migration"](../design/native-temporal-verification.md#migration)
asks in item 2 for `verify --with-tlc` on every root of `samples.txt` with no disagreement.
CI's `--with-tlc` step (`formal-verify`) covers the five roots of `scripts/tlc-roots.txt` and
the flagged header lines; the other roots meet TLC only through `engine_equivalence`. Item 3
(`reduction-diff.py` with TLC on seeds `0:2000` at 2 and 3 instances) runs in no workflow, and
`release.yml` runs no `verify`.

## Residual risk

<!-- derived-from #what-agreement-can-show -->
<!-- derived-from #status-of-t1 -->
<!-- derived-from ./claims.md#0-the-build -->
<!-- derived-from #what-each-source-can-and-cannot-detect -->

Classes of divergence between `T_β(M)` and what the engines decide that the current
evidence would not catch, most consequential first. Each item is an assessment drawn from
the sections it cites, and carries the tag of the statements it rests on.

1. **Common-mode faults outside the corpus traps [read].** A `T` from the builder that is not
   `T_β(M)` of D13 (a failure of claim I1), a wrong cone, a wrong ladder decision (the staging
   of a property cone's runs), a wrong quotient table, or a wrong name in `TlaNames`, which the
   printer and the decoder both use, moves both engines. A value the build wrapped past a
   byte, or a cone whose over-approximation reports a state the event order forbids as the
   model's ([cones.md](./cones.md#safety-cones)), would be a fault of this kind. The 215 hand-written `expect` lines see these on the 89 traps they name.

   For the builder, the reference interpreter (`core/tests/reference/`) is a second check
   [test]. It reads a subset of the language from the `SemanticModel` without
   `TransitionSystem::build`, explores it by brute force on plain values (`i64` integers,
   Enum values by name, no packing, cone, reduction or symmetry), and compares the initial
   states, reachable states, edges, rule names and violated rules with the native run of the
   whole model. Its subset of the language is the one claim
   [I1](./claims.md#0-the-build) lists. It runs on the corpus cases and formal-verification samples in the subset at
   three bounds, on 300 random models of the subset in every pull request and 2,700 more on
   main. With z3 present (CI requires it) it also checks the rules z3 proves at `m`, with
   lemmas mined without a twin, against the reference at `m + 1` and `m + 3`: two fixed models
   per pull request and 200 random models on main.

   Its discrimination is recorded once: the description of PR #492 shows
   `a_value_past_a_byte_is_not_wrapped` failing on a build that wrapped the nullable Int at 255
   and the nullable Enum of 256, and `a_rule_proved_at_one_int_range_holds_at_wider_ones`
   failing on a build whose guard listed a choice's values at `β`. No test repeats that run;
   the tests pin the current behaviour.

   What it leaves to the `expect` lines: every construct outside the subset
   ([claims.md, I1](./claims.md#0-the-build) lists what `RefModel::read` refuses; among it
   owners and keys, split use cases, triggers, copies, `chooses`, `drives`, `now`,
   multi-entity rules, quantifiers, `after` assertions, property verdicts); and, since it
   compares whole-model runs, the cone choice, the ladder, the quotient tables and `TlaNames`.
   It shares the parser, `build_model`, the rule names, and D2's reading of which Int-typed
   columns are numbers with the builder. This item and the next are the main residual risks
   of all the theory documents.
2. **A wrong verdict on any input no test ran [read; runtime].** Every trace the native
   checker prints, safety or liveness, plain, reduced or symmetric, replays through the step
   relation of the system it is about before it prints, and one that does not is a fault
   (`LiftFault`, `LassoFault`, `TraceFault`) that ends the run `tool_error` (D23). The reduced
   lift checks every lifted step against the cone's steps in every build
   (`reduce/lift.rs::lift` returns `ReplayError`), and a symmetric path goes through
   `orbit::replays`. The lifts' own look-ups fault too: `explore/orbit.rs::lift_path` and
   `reduce/lift.rs::lift` return `NoMove`, `NotInitial` or `Length`, and `reduce/lift.rs::lift`
   returns `NoMove` for a class move with no S path behind it
   (`reduce/partition.rs::Succ::shortest_path` returns `None`). The lasso lift of a run up to
   symmetry (`explore/orbit/liveness.rs::Product::lift`) returns `NoMove` as well.
   `explore/properties.rs::ComponentRun::decide_property` checks in every build that its path
   breaks the property at its last state and, on a reduced system, reads an `always` again on
   the lifted last state (`PropertyVerdict::Fault`).

   What remains is narrower. The `expect`s and `panic!`s left on a lift's way (listed with
   [claims.md](./claims.md#c-the-native-verifier), row N16, with the reason each holds or the
   assumption it rests on: `reduce/lift.rs::lift_stop`, `Product::new`, the searches before a
   lasso lift, callers' preconditions, the rebuilt extended sub-lifecycle that `lift` explores
   again, which holds if `Extended::rebuild` reproduces the reduction's build, and two reads of
   a lifted path that hold whatever path of the system's own states is lifted) would stop the
   command rather than print a fault. A wrong `ok` has no run-time guard at all. TLC sees a
   wrong `ok` only where a test or a user runs `--with-tlc`.
3. **Bounds [read].** In CI, TLC runs at 3 or more instances only on the corpus (the three
   cases with per-entity flags, and the `bfs=` runs of 38 cases, at 1 and 3 instances and, for
   `safety_at_most_bound` alone, at 4), on the one sample header line that raises a count,
   `verify --instances Loan=4` of `samples/formal-verification/counting_rules.rdra`, in
   `formal-verify` (verdicts only), and on the 3-instance seeds among the 45 main random
   seeds. It never runs on the 25 fallback cones at 2 instances in `engine_equivalence`, and
   `verify --with-tlc` runs only 5 of them at 2 instances, verdicts only; the other 20 never
   meet TLC at 2 instances ([the sample roots](#verify---with-tlc-on-the-sample-roots), T5).
   TLC never runs as a graph comparison on unreduced cones above 300,000
   states. Open issue #202 proposes comparing large cones with TLC at 2 instances by random
   walks. A divergence that needs more rows than the tested bounds is invisible to every
   differential source.
4. **Predicate shapes no test reaches in a discriminating state [paper; read].** `pred.rs` and
   `expr.rs` are separate implementations of D5 with no shared harness. A difference on a shape
   (say an ordered comparison of a nullable Int against `now`, or a count inside a
   quantifier on the primed state) shows only if some tested model reaches a state where
   the two readings differ.
5. **Fairness per instance [read].** The graph comparison compares each edge with its params
   on unreduced sample cones, so a step attributed to the wrong binding shows there. A reduced
   cone's class moves compare by action alone, and which fair instance a step counts for is
   checked only through a liveness verdict, on 23 temporal cases and the `fair_*` corpus cases.
6. **The comparison code itself [read].** `compare.rs`, `tlc_output.rs`, `tlc_replay.rs` and
   `TlaNames` have no Kani harness and no shadow twin. The TLC-side mutation run mutates them
   (`names.rs` for `TlaNames`), but it runs by hand and no run of it has completed. A
   disagreement in violated sets that goes unreported, or a per-run comparison of runs on two
   different systems, would be faults of this kind; `unreplayed` and the pairing by `RunOn`
   exclude both as read ([§2](#2-the-agreement-rules)), and only tests and the hand-run
   mutation run check that reading.
7. **Effect orders the builder may not exclude [paper; read].** Step 3 of T1 for multi-target
   transitions rests on a builder invariant, and the build rejects any action that writes one
   slot twice (`ConflictingEffects`; corpus case `safety_created_set_beside_transitions`,
   whose two effect readings differ). The check lives in the builder, not in the printer, so
   the printer's reading is right only for the actions the builder lets through. Two params of
   one action that bind one slot and write one variable are not excluded by any check
   ([Effects](#per-construct-argument)).
8. **TLC [assumed: TLC].** TLC's fingerprint-based state store can, with small probability,
   merge two distinct states on a hash collision and so miss states (Yu, Manolios and Lamport
   1999 discuss TLC's design; TLC prints its own collision estimate, which
   `cli/src/tlc_output.rs` does not parse). A TLC fault that matches a native fault is not
   detectable.
9. **Checks with no TLC counterpart [read].** `can_reach` and the one-sided deadlock check.
   `core/tests/oracle.rs::can_reach_and_deadlocks_agree_with_a_naive_oracle` (main only)
   checks both against an exploration of its own, which shares the step relation and the
   evaluator; TLC checks neither.

## Threats to validity

[read] for the facts each item names; the items themselves are the author's assessment.

- **Internal.** The argument for T1 is a reading of the printer beside the native step
  relation. It is not mechanised, and it relies on invariants of the builder (that only
  pre-existing entities start off base, and the effect-order invariant, which the build
  enforces) that no assertion enforces in the printer. A misreading is possible; each step
  names the functions so it can be re-checked.
- **Construct.** "Agreement" in the tests is agreement of verdicts and of labelled graphs
  (with each edge's params, except a reduced cone's class moves), not equivalence of the
  modules. The decoder is the same code in the printing and in the comparison, so a
  consistent renaming is invisible (harmless) and a decoder that maps two TLC states to one
  native state would show as a count mismatch, which the comparison checks.
- **External.** The inputs are 40 sample roots, 89 corpus cases, 23 temporal cases, the 36
  `bfs_and_tlc_*` tests of `engine_equivalence.rs`, and 45 random models with TLC. The samples
  were written alongside the tools, so they exercise what the authors thought of. The random
  generator has a fixed shape (no counts, three to five entities).
- **Conclusion.** The counts are counted from the code at this commit. The measured numbers
  (the kill rates, and the 284 and 33 reducing seeds of a `ci.yml` comment) are records of
  single runs that no test repeats.

## Related work

- **TLA+ and TLC.** Lamport, *Specifying Systems* (2002), for `WF`, stuttering, `~>`, and
  machine closure (§8.9: weak fairness of a subaction of `Next` conjoined to
  `Init /\ [][Next]_vars` is machine closed); Yu, Manolios and Lamport, "Model checking TLA+
  specifications" (CHARME 1999), for TLC. Abadi and Lamport, "The existence of refinement
  mappings" (TCS 1991), for history variables and the definition of machine closure.
- **Translation validation.** Pnueli, Siegel and Singerman, "Translation validation"
  (TACAS 1998): check each translation's output against its input instead of verifying the
  translator. The sample comparison does this on the reachable fragment, for safety only.
- **Differential testing.** McKeeman, "Differential testing for software" (Digital
  Technical Journal, 1998).
- **Mutation analysis.** DeMillo, Lipton and Sayward, "Hints on test data selection" (IEEE
  Computer, 1978); the mutation score is the measure the mutation run does not record.
