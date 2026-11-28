# Agreement with TLC

<!-- derived-from ./semantics.md -->
<!-- derived-from ./claims.md#d-agreement-with-tlc -->
<!-- derived-from ../engine-agreement.md -->
<!-- derived-from ../formal-verification.md#mapping -->

This document states what assurance exists that the native verifier and TLC, reading the
TLA+ module rdra-ish prints, give the same verdicts, and what kind of assurance each piece
is. It owns rows T1 to T16 of the [claims inventory](./claims.md#d-agreement-with-tlc) and
uses the notation of [semantics.md](./semantics.md) (D1 to D25) without redefining it. It
describes the code at commit `5ae62f2c` (origin/main, 2026-11-28, the merge of #542).

It was first written against `fb6928c7`, then pinned to `fec53d6a`; [Appendix: the pinned
commit](#appendix-the-pinned-commit) says what changed in the cited code up to `5ae62f2c` and
what was read again. Paths follow the convention of the
[claims inventory](./claims.md#how-to-read-a-row): `core` is
`crates/rdra-ish-core/src/lifecycle`, `cli` is `crates/rdra-ish-cli`, `emit` is
`crates/rdra-ish-emit/src`. How the document changed after it was first written is in
[Errata](#errata). `unread@pin` marks a statement whose cited code was not read at the pinned commit ([claims inventory](./claims.md#how-to-read-a-row)).

**The central point [read].** Agreement with TLC is evidence, not a theorem. No part of the TLA+
translation is proved. What exists is (1) an argument, written here for the first time,
that each construct of the printed module denotes the system the build returned, which is
`T_β(M)` (D13) when claim [I1](./claims.md#0-the-build) holds, (2)
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
- **[read]**: a statement of what the code, tests, workflows, or notes at the pinned commit
  contain or do, taken from reading them (or, where it says so, from one `gh api` query).
  Nothing was run for it. A defect stated as [read] (P12, P21, P23, P24) was a reading of the
  code when this document found it; the fixes of P12, P21 and P24 reproduced it first, and
  the index records no reproduction for P23. A number taken from a note or code comment rather than counted
  in the code says where it comes from.

The tag is added to the legend above the four classes because most of sections 2 to 5 is
neither an argument nor a test result but a description of what the evidence is. The
tagging unit is a paragraph with a bold lead, a list item, a table, or a subsection: a tag
at the start of a subsection or on the sentence that introduces a list or table covers it
until the next tag. Tags combine with `;` (`[paper; test]` means both kinds of support),
and a tag may carry a qualifier after a colon or `;` (`[assumed: TLC]`, `[read; not
reproduced]`, `[paper; see Status]`, `[test; no paper argument; …]`); the class is always
one of the five above. The Related work section cites literature and the Errata section
records changes to this document; neither carries a tag.

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

**Scope [read].** The proposition needs both verdicts to be about the same `T'`. Since #503 the
per-run comparison pairs runs by that system (`RunOn`: property, graph, fairness). One native
run has no TLC run on its system: for a liveness property on a reduced cone, the native
`WfNext` run may be `decide_liveness_reduced` on the reduced cone, while TLC's `_WF` module is
printed from the unreduced cone ([§2](#2-the-agreement-rules)). That run is compared only
through the final verdict, where a difference can also mean a fault in the reduction (A8) or
in the reduced-graph `_WF` argument (A15), and agreement shows that those and the native
search together gave TLC's verdict on the unreduced cone (P23, fixed in #503).

The proposition runs in one direction [paper]. Agreement `v_N = v_L` shows only that the two
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
   `dec(x)` (D16); each `after` `PROPERTY` holds on `⟦𝓜⟧` iff D17 holds (with the
   stuttering exception of [P22](./README.md#open-problems)); each temporal `PROPERTY`
   holds on `⟦𝓜⟧` iff D19 or D21 holds under the fairness of D20 that the print mode
   selects;
5. *(Latches)* for a module that checks `precedes(p, q)`, the latch is a history variable:
   adding it changes neither which `X`-valuations are reachable nor the steps between them.

Status [read]: **not proved**. The argument below is a paper argument per construct, read from
`emit/tla/module.rs`, `emit/tla/pred.rs`, `emit/tla/mod.rs`, and `emit/tla/names.rs` beside the native step
relation in `core/explore/moves.rs` and the evaluator `core/expr.rs`. The argument for step 3
needs an invariant of the builder: no action writes one `(var, param)` with a multi-target
`Transition` and also an `Assign` after it or a `Choose` before it
([Effects](#per-construct-argument)). Since #485 the build enforces it: an action that
writes one slot twice fails with `ConflictingEffects` (`Action::slots_written_twice`,
checked by `build/mod.rs::Builder::one_write_per_slot`, which `Builder::finish` calls; P24,
fixed). It also assumes that no two params of one action that can bind one slot write the
same variable, which no check enforces ([Effects](#per-construct-argument)). Steps 1 and 2, and step 3 per binding on unreduced
cones, are also checked as graph equality on every sample cone that fits the comparison's
cap, with edges compared with their params since #507
([Sample comparison](#the-sample-comparison)); step 3 on a reduced cone's class moves, which
print no params, and step 4 are checked by action and by verdict comparison only.

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

**Effects [paper; read for the P24 cases; the invariant enforced since #485].** `module.rs::effects` folds the effects into one
`v' = [v EXCEPT …]` per written variable and `UNCHANGED` for the rest, and
`moves.rs::each_successor` applies them to a copy of the pre-state. The fold is per
`(var, param)`: the printer writes one `v' = [v EXCEPT ![p0] = …, ![p1] = …]` with the params in
ascending order, and the native step writes by slot in effect order. If two params of one
action bound one slot and both wrote `v`, TLA+'s `EXCEPT` would keep the higher param's value
and the native step the later effect's, and on a delete the printer resets only the deleted
param's entries while the native step drops every choice keyed by that param. Neither
`candidate_slots` nor `bindings_for` keeps two params of one entity apart, and
`Action::slots_written_twice` and `Builder::one_write_per_slot` compare `(var, param)` pairs,
not slots. No builder path was found that gives one action two params of one entity that
write the same variable, but none was shown to be impossible either: the argument below
assumes it, and it is not checked. Case by case:

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
  or the pre-state value, since `prev` is the pre-state when the slot held a `Choose`). On a state whose current source has two or
  more targets, all four orders agree. The no-match case cannot arise in an action built
  from `transitions`, whose `TransitionFrom` pre-guard holds the column at a source. The
  post-guards (`build/transitions.rs::arm_guards`: one guard per `Transition` or `Choose`
  effect that has a `.when` arm or value) filter the successors the same way in both
  engines, so they reconcile the two effect readings only when they leave a single value:
  when both the `Choose` and the `Transition` carry such guards, both engines keep exactly
  the current source's target `t`, if the `Choose` guard allows it; when the `Transition`'s
  arms carry no `.when`, the native step keeps every enabled `Choose` value and the printed
  step keeps only `t`. So step 3 holds for multi-target transitions only under a builder
  invariant: no action has an `Assign` after, or a `Choose` before, such a `Transition` on
  the same `(var, param)` (or, more weakly, the post-guards reconcile it). Open problem P24
  records where the builder excludes each shape and the one shape it was not found to
  exclude. Since #485 the build enforces the invariant: an action that writes one slot
  twice fails with `ConflictingEffects`, the created-row shape included.
- `Delete`: both reset `Exists` to `deleted` and every column of `decl.columns` to base,
  and keep owners (D8). Both drop a choice on the deleted slot (`moves.rs` retains only
  choices on other params; the printer overwrites the slot with the base value).
- `Tick`: the action's guard `now < m` is an ordinary guard (`build/vars.rs`), and the
  effect prints `now' = now + 1`.

Hence step 3 for unreduced systems, under the assumption about params stated above. Step 3 includes self-loops: a step whose successor
equals its pre-state satisfies the action formula in both, and `vars' = vars` makes it a
stuttering step in TLA+. That matters for fairness and `after` (below).

**Predicates and null [paper; test].** `pred.rs` mirrors D5: quantifiers range over present
slots; `Count` is `Cardinality` of the present slots that satisfy the body; `In`/`Not In`
on a slot is `\in`/`\notin` of rendered values, and otherwise the disjunction of
equalities. In `numeric`, `null` is `-1`, which lies outside `IntRange`; `=` and `#` compare
it as a value, so `null` equals only `null` and differs from every number, as D5 says; an
ordered comparison conjoins `x # -1` for each nullable operand, so `null` is never
ordered. A slot index read as a number prints as `(p - 1)`, the slot itself, since a TLA+
index is the slot plus one, as `expr.rs::compare` reads `Operand::Slot`. Non-numeric operands compare by rendered value, which is index equality because
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
(P1, fixed in #477, concerned what that cone's counterexample means, not agreement). Tested by
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
engines agree; D17 states the exemption since #475, and since #494 the language reference
does too and a last step that only stutters is reported as vacuous (P22).

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
soundness (A8, theme (a)). This document gives no paper argument that the printed class
move (the `is_class_move` branch of `action`), the `Rise` lookup, and the bits denote the
native reduced step (`moves_of_into` with a reduction); `bits`, `rise_expr`, and the native
reduced step were not read for it (unread@pin). Step 3 on reduced systems therefore rests on the graph
comparison of reduced sample cones alone. The sample comparison does compare the reduced cone's violated
checks with the unreduced cone's, through TLC on both
([Sample comparison](#the-sample-comparison)), but only on cones whose unreduced BFS fits
its cap. TLC never reads a reduced module with `WF_vars(Next)`: the `_WF` module is printed
from the unreduced cone (`property_cone.rs::PropertyCone::bundle`), while the native
`WfNext` run may run on the reduced cone, which is compared only through the final verdict
(P23, [§2](#2-the-agreement-rules)).

**What the module does not state [read].** `can_reach` prints as a comment: TLC cannot state
`AG EF` (P19, fixed in #478 outside TLC: `core/tests/oracle.rs` checks `can_reach` and
deadlocks against an oracle of its own, on main only). Deadlocks, the finding of D25, are not TLC's `CHECK_DEADLOCK` (always
`FALSE`) but the `Deadlock_*` invariants of `print_state_models`, which are one-sided by
design (T6).

### Status of T1

[read] The table sums up the argument above and the tests of [§3](#3-differential-evidence).

| Step of T1 | Paper argument | Test | Assumed |
|---|---|---|---|
| 1, 2, 3 (states, init, steps) on unreduced systems | above; step 3 for multi-target transitions under the builder invariant of P24, which the build enforces since #485, and for every effect under the assumption, not checked, that no two params of one action that can bind one slot write one variable | graph equality on every sample cone under 300,000 states, at 1 and 2 instances, edges with their params since #507; the 36 `bfs_and_tlc_*` tests of `engine_equivalence.rs` | TLC's `-dump dot` lists every edge, self-loops included |
| 3 on reduced systems | none (not argued here) | graph equality on reduced sample cones, class moves by action alone; the reduced cone's violated set against the unreduced one's | the quotient tables (common-mode) |
| 4, state rules and `after` | above; P22 | verdict and violated-set comparison on every compared cone; each TLC trace replayed through the native step relation | TLC |
| 4, temporal properties and fairness | above | verdicts only: 23 `TEMPORAL_CASES` at 2 instances (under `-lncheck final` since #539, [Batch JVMs](#batch-jvms)) and the corpus at each case's flags | TLC's liveness checking under quantified weak fairness |
| 5, latch | above | verdicts only: `explored_along` (a `TEMPORAL_CASES` fixture) and the corpus case `safety_precedes_latch`. The graph comparison prints `PrintMode::Safety` modules, which declare no latch (`mod.rs::checked_properties` checks no property in that mode), so it never sees one | |

[read] Until #507 no step of T1 was checked on a binding: the graph comparison compared
edges as `(from, to, action)`, so two bindings with the same effect were one edge, and a
fault that attributed a step to the wrong fair instance was visible only through a liveness
verdict (P25). Since #507 the comparison prints the cone with one `Next` disjunct per
binding (`tla::print_bound`), TLC names each edge after it, and edges compare as
`(from, to, action, params)` (`engine_equivalence.rs`, `type Move`). A class move of a
reduced cone prints with no params, so its edges still compare by action alone.

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
  (`RunOn`: property, graph, fairness; by `ConeRun` label before #503), then the final
  verdicts. For a liveness property
  whose `Plain` run on a reduced cone finds a counterexample, `native.rs::Plain::of`
  (called from `native_property`) first runs `ComponentRun::decide_liveness_reduced` on the
  reduced cone (every class that
  can loop taken as a stop; A15), and when that returns `Holds` or `Violated`,
  `native_property_after` records it as the native `WfNext` run. TLC's `WfNext` run is
  always the `_WF` module, which `PropertyCone::bundle(ConeRun::WfNext)` prints with
  `PrintMode::Property` from the unreduced cone (`verify/jobs.rs`). Before #503 the two were
  paired by label, which compared the reduced-graph search with TLC on the unreduced cone
  as one run (P23; see [Scope](#what-agreement-can-show)). Now that native run, printed
  `WF_vars(Next) on the reduced cone (native only)`, meets no TLC run and is compared only
  through the final verdict.

`differ` treats a pair as a disagreement when both are `Some`, they differ, and neither is
`ToolError`. Each disagreement prints `error[EngineDisagreement]` and makes the run's
`result:` `tool_error` (`cli/src/verify/mod.rs`). State models are compared separately
(`state_models.rs::state_model_disagreements`, one-sided for deadlocks).

**Caps are asymmetric [read].** Native cone runs stop at `--max-states` or the memory limit,
never at the time budget: the cone `Caps` that `verify/mod.rs` builds carry no deadline, and
the only `.until(…)` in `verify/` is `WholeRun::caps`, which only whole-model runs use
(`Native::capped` says "stopped at the state cap (--max-states) or the memory limit"). TLC
cone runs have no cap at all: `verify/tlc.rs::run_cfg` runs TLC to the end, and only
whole-model runs (`run_whole`) are killed at the budget or the state cap. So a native cone
run can stop where TLC's run of the same module decides.

**Decided against undecided: the rule and the code [read].** Rule 1 says `not decided` from
either engine is no disagreement. The code keeps it for final verdicts, where `Decision`
maps `Undecided` to `None`, and since #469 per run too:

- **P21 (fixed in #469) [read].** `verify/native.rs::Answer::outcome` maps `Answer::Stopped`
  to `None`, so a native run stopped at a cap is recorded in `Native::runs` as not decided and
  compared with nothing. When this document was written it mapped `Stopped` to
  `Outcome::Ok`: every push into `Native::runs` but the `WfNext` run on the reduced cone used
  it (the `Plain` and `Full` runs of `native_property_after` and its unreduced `WfNext` run,
  the `Plain` run of `always_up_to_symmetry`, and the `Plain` and `WfNext` runs of
  `liveness_up_to_symmetry`), so a native run stopped at a cap beside a TLC counterexample on
  the same run printed `error[EngineDisagreement]`, and in `liveness_up_to_symmetry` a stopped
  plain run read as `Ok` and gave a false `tool_error` without TLC. Both failed closed. The
  fix reproduced it with `temporal_unmet.rdra` at `--max-states 3`.
- **P12 (fixed in #471) [read].** `compare.rs::unreplayed` replays every TLC trace of every
  job, whatever the native verdict, and a job with one that does not replay prints
  `error[EngineDisagreement]` and ends `tool_error`. When this document was written, a safety
  cone's TLC violated set was built with `collect::<Result<_, _>>().ok()`, so a trace that
  failed to replay dropped the comparison, and the display path (`tlc.rs::model_traces`)
  replayed only the first trace of each check, printing an unnamed `error:` line that changed
  neither `result:` nor the exit status; a later trace that did not replay was reported
  nowhere.

**The corpus harness's own reading (P13, fixed in #472) [read].** In
`cli/tests/engine_corpus.rs::check_case`, rule 2 (`rule_2`) fails a native `Cone_` line
without TLC's line for the same module when the header gives its checks a TLC verdict, and
rule 8 (`rule_8`) fails a check the plain run decides and the symmetric run does not decide
the same way, `not decided` included; `rule_2_fails_a_tlc_line_the_header_expects_and_the_run_lacks`
and `rule_8_fails_a_check_symmetry_leaves_undecided` test both on canned output. Before #472
rule 2 `continue`d past such a module and rule 8 compared only checks both runs decided.
Rule 1 in the harness reads the printed verdicts, so it inherits the run-time comparison
above.

**Known disagreements [read].** The corpus has none: no case carries a `known` line (0 of 86).
No run-time disagreement is open as an issue at the pinned commit, as far as the notes say.

## 3. Differential evidence

<!-- derived-from ../engine-agreement.md#the-corpus -->
<!-- derived-from ../engine-agreement.md#the-sample-comparison -->
<!-- derived-from ../engine-agreement.md#the-reduction-on-random-models -->
<!-- derived-from ../engine-agreement.md#batch-jvms -->
<!-- derived-from ../engine-agreement.md#mutation-run -->
<!-- derived-from ../cli-reference.md#partial-runs -->

Differential testing (McKeeman 1998) runs two implementations of one specification on the
same input and treats a difference as a fault in one of them. Its power is bounded by the
inputs, and it is blind to faults the implementations share. Each source below is
described by its inputs, what it compares, and what it cannot detect. The numbers are
those recorded in the notes and code at the pinned commit; none was re-measured.

[read] for every description in this section of what a test, script, or job runs and
compares; what the test shows when it passes is [test].

### The corpus

`cli/tests/engine_corpus.rs` over `cli/tests/engine_corpus/*.rdra` (T7).

- **Inputs.** 86 cases with 210 `expect` lines; families `cap` 2, `cone` 12, `fair` 9,
  `presence` 9, `safety` 25, `sym` 10, `width` 2, `z3` 17. One case (`cap_memory_limit`)
  skips TLC. 38 cases carry a `bfs=` list.
- **Bounds.** TLC runs at the case's flags: 2 instances and Int `0..5` by
  default; 1 instance in the six cases with `--instance-count 1`; per-entity counts above
  2 in three cases, two entities raised in each of the first two and one in the third
  (`presence_per_entity_slots`: Loan and Room at 3 over `--instance-count 1`; `safety_count_per_entity`: Loan at 4, Shelf at 3;
  `sym_per_entity_bounds`: Room at 3, Booking at 2); one entity held at 1 instance
  (`--instances X=1`, the others at 2) in four cases (`sym_arm_reorders`,
  `sym_deleted_row_owner`, `sym_lift_compose`, `sym_relink_loop`); `--int-range-max` in four
  cases (5, 6 or 7 in three `z3_` cases, 255 in `width_nullable_int_top`). Since #476 the
  `bfs=` runs at 1, 3 and 4 instances run TLC beside BFS too (P14, narrowed).
- **Compares.** Each engine against the hand-written header, rules 1 to 3 against TLC,
  rule 8 against the symmetric native run.
- **Detects.** The header is the only check of common-mode code: the planted-bug table of
  [What the corpus catches](../engine-agreement.md#what-the-corpus-catches) records two
  planted bugs in the cone ladder that moved both engines and were caught by the headers
  alone.
- **Cannot detect.** A trap no case names. A header written wrong in the same way as the
  code (the header is reasoning by the same people who wrote the code). Deleting a case is
  caught since #472: `every_corpus_header_is_well_formed` compares the cases with
  `tests/engine_corpus/CASES` both ways (P15, fixed; before, it asserted at least 30 cases
  where 79 existed).

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
  disjunct per binding (`tla::print_bound`, since #507; P25); the state-model findings of every entity on
  both graphs; the violated invariants and `after` assertions, rerunning TLC without each
  found check until no new one appears; and each TLC trace replayed through the native
  step relation (`replay_violations`). `a_corrupted_tlc_trace_does_not_replay` checks that
  the replay rejects a corrupted trace.
- **Unreduced and reduced.** `check_one_cone` compares the unreduced cone only when its BFS
  completes under `STATE_CAP` (300,000 states), and the reduced cone only when its run
  completes under the same cap. A cone joins `VERIFY_FALLBACK` (25 cones; insurance-claim-ops' Investigation joined in #526) when its reduced
  run at 2 instances passes the cap. A cone whose unreduced BFS passes 300,000 states while
  its reduced BFS fits is compared reduced only; since #476 `VERIFY_REDUCED_ONLY` lists those
  (30 cones), and the test fails when the list and the run differ. On such a cone TLC reads
  the Rust-computed quotient tables, so the comparison says nothing independent about the
  reduction there (P14). A reduced cone's class move prints with no params, so its edges
  compare by action alone.
- **Cannot detect.** Temporal properties (not printed in `PrintMode::Safety`); anything at
  3 or more instances; any cone of the 25 at 2 instances.

`a_property_cone_decides_what_the_whole_model_decides` (T3) runs the property ladder on 23
`TEMPORAL_CASES` (7 sample files and 16 inline fixtures; T3 said 26), at 2 instances, each run of each property
against TLC's run of the same module, and the cone's final verdict against the whole
model's TLC verdict. It compares verdicts, not graphs. Its native `WfNext` run is always
`Fairness::WfNext` on the unreduced cone, as TLC's is; it never calls
`decide_liveness_reduced`, so the reduced-graph `_WF` search is compared with TLC only
under `verify --with-tlc` (the corpus harness passes `--with-tlc` to every case; the
`cone_reduced_*` cases are written for A15, and this reading traced the path through
`cone_reduced_no_loop_capped`; P23).

### `verify --with-tlc` on the sample roots

`scripts/verify-samples.sh --with-tlc --tlc-roots scripts/tlc-roots.txt` in CI's
`formal-verify` job runs TLC beside the native checker on the five roots that have a
`VERIFY_FALLBACK` cone (clinic-ops, patent-ops, food-permit-ops, insurance-claim-ops,
oidc-auth) and on every header line with flags, at each root's header bounds. The domain
roots' temporal properties are compared with TLC only here. It is held to the run-time
rules of [§2](#2-the-agreement-rules), whose gaps P12, P21 and P23 were fixed in #471,
#469 and #503.

For oidc-auth the header bounds do not reach the 13 `VERIFY_FALLBACK` cones at 2 instances.
Its headers (`samples/oidc-auth/buc/buc_ciba.rdra`) run `verify --instance-count 1 --prove
--deny-undecided`, and at 2 instances only `verify --deny-undecided --entity
BackchannelRequest --entity DeviceAuthorization --entity LogoutNotice --entity SigningKey`.
Under `--entity`, a run checks the selected entities' views and the invariants and `after`
assertions that bind them (cli-reference.md, "Partial runs"); the 13 cones are the views of
eight browser-flow entities and five invariants over Interaction, AuthorizationCode, Grant,
RefreshToken, and AuthorizationRequest, none of which that line selects. So no CI job runs
TLC on those 13 cones at 2 instances, not even as a verdict comparison (P14, extended). The
other four roots' fallback cones run at the default 2 instances here.

### Random models

`scripts/reduction-diff.py` (T9) generates models of three to five entities with chains,
quantified guards, `.along`, a split use case, `chooses`, a timer, `after` assertions and
properties, at 2 or 3 instances, and compares `verify --deny-undecided` reduced against
unreduced. With `--with-tlc` both runs also run TLC, and a seed with
`error[EngineDisagreement]` is listed as `disagreed`. In CI, a pull request runs 522 seeds
without TLC, and main runs 45 fixed seeds with TLC, some at 3 instances (`ci.yml`). The 522
and 45 are counts of the seed lists (`scripts/reduction-diff-pr-seeds.txt` and the main
step's `--seeds`). How many of them reduce a cone (284 and 33, measured 2026-11-25) comes
only from a comment in the `formal-verify` job of `ci.yml`; it was not re-measured. TLC
therefore sees 45 random models per main run, the same 45 each time. The release gate's
item 3 (seeds `0:2000` with TLC) was waived for v3.0.0 (P16). The generator draws no `Count`
predicate (A11), so count predicates get no random coverage. Counted triggers have a
generator mode, `reduction-diff.py --counted` (`.counted(k)` with `k` of 1 or 2 on every
`triggers` line), which no workflow passes.

### Batch JVMs

`RDRA_TLC_BATCH=both` (T11) runs each `engine_equivalence` job routed to a batch JVM
(`Route::Batch`, which every cone of at most `BATCH_STATES`, 5,000, states takes, and a property cone's
`Cone_P`, `Cone_P_Full` and `Cone_P_WF` runs) and again as a process with the same fingerprint
and seed, and fails on any difference in exit status, log, parsed run, invariant scan, or
dumped graph (`assert_same_run`). A larger cone and a property's whole-model run are
routed to a process from the start (`Route::Process`, `check_cone`), so they are never
compared this way. Since #486 the main-only `tlc-batch-both` job of `ci.yml` runs
`engine_equivalence` that way (scheduled or by hand, never on a pull request). The
`#[ignore]`d `recorded_tlc_runs_are_the_same_in_a_batch_jvm` replays recorded `verify` runs
the same way, also by hand. In CI, `RDRA_REQUIRE_TLC_BATCH=1` only requires that some job
ran batched without falling back. This bears on the test harness only: `verify --with-tlc`
does not use batch JVMs.

Since #539 (af6354a4) every TLC run `engine_equivalence.rs::run_tlc_on` starts,
batched or a process, which includes every cone, state-model and property run of the
comparisons, and every replay of `recorded_tlc_runs_are_the_same_in_a_batch_jvm` pass
`-lncheck final` (`LIVENESS_AT_THE_END`), so TLC checks liveness once, at the end
of the search, rather than also at times its clock sets; its doc comment gives the reason,
a warm JVM and a fresh process that found a violation at different points.
`verify --with-tlc` passes no `-lncheck` (`cli/src/verify/tlc.rs`). The harness's TLC
evidence on temporal properties (T3) and the batch comparison (T11) are therefore under
end-only liveness checking, which a user's `--with-tlc` run does not use; that TLC's
liveness verdict on a finished search does not depend on when it checks is assumed (TLC),
not tested. The safety and state-model runs of the graph comparison (T5) pass the flag as
well. Their cfg (`emit/tla/mod.rs::cfg_without` in `PrintMode::Safety`) lists invariants
and, as `PROPERTY` lines, the `after` assertions, whose form `[][…]_vars` TLC checks on each
step rather than in its liveness check; that the flag leaves those runs unchanged is
likewise assumed (TLC).

### Mutation run

`cargo mutants` (T15) runs by hand only (`mutants.yml`, `workflow_dispatch`), in two
configurations (docs/engine-agreement.md, "Mutation run"):

- **The liveness ladder** (`.cargo/mutants.toml`): mutants of `property_cone.rs`,
  `verify/native.rs`, `core/cone.rs` and `core/explore/liveness.rs`, killed by the corpus
  with `tlc` and `z3` off `PATH` (the workflow checks that they are gone). It measures how
  well the corpus headers alone catch faults in the cone ladder and the liveness search, not
  agreement with TLC. Two kill rates are recorded (the first since #486): 139 of 209 viable mutants
  (66.5%) at `b0f593ba` and 151 of 240 (62.9%) at `6f2d29a1`.
- **The TLC side** (`-f config=tlc`, `.cargo/mutants-tlc.toml`, since #508): 548 mutants of
  the TLA+ printer (`emit/tla/{mod,module,names,pred}.rs`), the comparison
  (`verify/compare.rs`), and the TLC readers (`tlc_output.rs`, `tlc_replay.rs`), killed with
  `tlc` and `z3` on `PATH` by the corpus, the unit tests of the TLC output reader, the
  comparison and the TLC driver, and the `verify_cli` tests that run TLC beside the native
  checker. `verify/tlc.rs` itself is not mutated. This is the run that measures how well
  agreement catches a fault on the TLC side. Its kill rate is still pending: the run of
  2026-11-28 at `cb1beccb` is listed with no result yet.

Mutation analysis in the sense of DeMillo, Lipton and Sayward (1978) reports the fraction of
non-equivalent mutants killed; the recorded rates count every viable mutant, equivalent ones
included, so they are lower bounds on that fraction.

### What each source can and cannot detect

[read] The table restates the subsections above.

| Source | Native fault | Printer fault | TLC fault | Common-mode fault | Bounds and inputs |
|---|---|---|---|---|---|
| Corpus headers | yes, on the trap | yes, on the trap | yes, on the trap | yes, on the trap | 86 hand cases, TLC at each case's flags and at the `bfs=` counts |
| Corpus rules 1 to 3 | where TLC differs | where native differs | where native differs | no | as above |
| Sample graph comparison | states, edges with params, initial states, safety verdicts | same | same | no | 40 roots, 1 and 2 instances, cones under 300,000 states |
| Temporal cone comparison | liveness and ladder verdicts | same | same | no | 23 cases, 2 instances |
| `verify --with-tlc` in CI | verdicts | verdicts | verdicts | no | 5 domain roots and flagged header lines, header bounds |
| Random models with TLC | verdicts | verdicts | verdicts | no | 45 fixed seeds, main only |
| Mutation run, liveness | cone ladder and liveness only, through headers | no | no | ladder only | corpus, by hand, 66.5% and 62.9% recorded |
| Mutation run, TLC side | no | printer, comparison, TLC readers | no | no | corpus and CLI tests with TLC, by hand, rate pending |

## 4. Mechanised evidence

<!-- derived-from ../engine-agreement.md#kani-harnesses -->

**Kani (T13) [read].** There are 16 `#[kani::proof]` harnesses at the pinned commit, all in
`core`: two in `expr.rs` (the comparison of D5), four in `explore/store.rs`, three in
`explore/segments.rs`, three in `explore/packing.rs`, one in `explore/arena.rs`, two in
`explore/table.rs`, one in `explore/symmetry/mod.rs`. Bounded model checking (CBMC, as Kani
uses it) proves each statement for every input up to its bound. For agreement, the two
`expr.rs` harnesses matter: they fix the native reading of null and ordering that
`pred.rs::numeric` must mirror. Nothing ties `pred.rs` to them except tests. No harness
concerns the printer, the decoder, the TLC readers, or the comparison.

**Shadow twins (T14) [read].** A `shadow` build runs 26 reference twins beside fast paths
(`crates/rdra-ish-core/src/shadow.rs::TWINS`). The ones on the path TLC is compared against are the move
generation (`EachEnabled`), the edge store (`Edges`, `StepEnds`), and the reduced class
moves (`ClassMoves`). They compare a fast native path with a slower native one; they say
nothing about TLC or the printer. Their value for agreement is reach: the `shadow` job runs
them on samples and corpus cases larger than the TLC comparison covers, below each twin's
cap (P17).

## 5. Process gates

<!-- derived-from ../engine-agreement.md#mutation-run -->

[read] for this section; the branch-protection statement is from one `gh api` query.

| Job (`.github/workflows/ci.yml`) | Runs on a pull request | Runs on main | Compares with TLC |
|---|---|---|---|
| `engine-equivalence`: `engine_equivalence`, `proof_equivalence`, `engine_corpus` with `RDRA_REQUIRE_TLC=1` | when `changes` says `verify` | every 3 hours, skipped when the commit was checked | yes |
| `formal-verify`: `verify-samples.sh --with-tlc --tlc-roots`, reduced against unreduced on random models | when `changes` says `verify`; random models without TLC | with TLC on 45 seeds | yes |
| `shadow` | as `changes` decides | yes | yes: the corpus on a `shadow` build with `RDRA_REQUIRE_TLC=1` |
| `tlc-batch-both`: `engine_equivalence` with `RDRA_TLC_BATCH=both` | never | yes | yes: batch JVM against a TLC process |
| `kani` | when `scripts/kani-relevant.sh` matches a changed file | yes | no |
| `mutants.yml` | never | never; by hand | with `-f config=tlc` |

Every TLC run in CI is in the flake's devShell. `ci.yml` sets `shell: nix develop --command
bash -eo pipefail {0}` as the default, and only two jobs override it with plain `bash`, the
`changes` job and the `kani` job (Kani is not in nixpkgs, so that job runs on the runner's
rustup outside the devShell, as its comment says); neither runs TLC. Of the other
workflows, `mutants.yml` takes TLC off `PATH` on purpose in its liveness configuration and
requires it in the TLC one, `portability.yml` runs with no nix
and no TLC (its comment), and `release.yml`, `publish-pypi.yml` and `verus.yml` do not
mention TLC. The devShell's TLC is
`flake.nix`'s `tlcFor`: the nixpkgs `tlaplus` package overridden to the 1.8.0 pre-release
jar (build of 2026-11-19, rev `bc1b506`, by the flake's comment) on JDK 25. The CI evidence
is about that TLC; the notes also name 1.7.4, which nixpkgs ships.

**None of these is a required check.** `gh api repos/manji-0/rdra-ish-dsl/branches/main/protection`
answers "Branch not protected", and the repository has no rulesets (checked 2026-11-28).
A merge with a red or skipped job is possible; the project's practice is to merge on green
pull-request checks, which is a convention, not a gate. Main runs on a 3-hour schedule, so
a merge reaches the main-only steps (random models with TLC, the full `kani` set) hours
later or, if GitHub drops a scheduled run, later still (P16, confirmed).

Two release-gate statements bore on agreement (P16, extended; narrowed in #486, #497 and #508).
The CHANGELOG for v3.0.0 records item 2 of the gate ("`verify --with-tlc` on every root of
`samples.txt` reports no disagreement") as held on the evidence that `formal-verify` ran "the
TLC roots with `--with-tlc`", which is five roots and the flagged header lines, not every
root; the other roots were compared through `engine_equivalence` instead. Since #497 an
erratum under that entry says so. And native-temporal-verification.md said the `--with-tlc`
step moves to `release.yml` once the gate passes; `release.yml` runs no `verify`, and the step
still runs in `formal-verify`. That note now says the step did not move and that the five
roots are what runs.

## Residual risk

<!-- derived-from #what-agreement-can-show -->
<!-- derived-from #status-of-t1 -->
<!-- derived-from ./claims.md#0-the-build -->
<!-- derived-from #what-each-source-can-and-cannot-detect -->

Classes of divergence between `T_β(M)` and what the engines decide that the current
evidence would not catch, most consequential first. Each item is an assessment drawn from
the sections it cites, and carries the tag of the statements it rests on.

1. **Common-mode faults outside the corpus traps [read].** A `T` from the builder that is not
   `T_β(M)` of D13 (a failure of claim I1), a wrong
   cone, a wrong ladder decision (the staging of a property cone's runs), a wrong quotient
   table, or a wrong name in `TlaNames`, which the printer and the decoder both use, moves both
   engines. P40 and P54 (a value the build wrapped, fixed in #470;
   [z3.md](./z3.md#uniformity-in-the-bound),
   [native.md](./native.md#the-state-store-and-its-encoding)) and P32 (an entity cone that
   left a raised mark out, fixed in #477; [cones.md](./cones.md#safety-cones)) were faults of
   this kind. The 210 hand-written `expect` lines see these on the 86 traps they name. For the builder,
   the reference interpreter of P26 (`core/tests/reference/`, #492) is a second check
   [test]: it reads a subset of the language from the `SemanticModel` without
   `TransitionSystem::build`, explores it by brute force on plain values (`i64` integers,
   Enum values by name, no packing, cone, reduction or symmetry), and compares the initial
   states, reachable states, edges labelled by use case, rule names and violated rules with
   the native run of the whole model. The subset is entities with Enum, Bool, Int-typed and
   nullable String columns (`@null`, `@default`, `@check`); `creates`, `updates` and
   `deletes` of one entity per use case, so one transaction group, and not both a create and a
   delete; literal, `null` and
   `present` `sets`, with `.when`; `transitions` with choices and `.when`; `applies_to`
   guards on the written entity; and the four single-entity rule forms with `.when`. It runs
   on the corpus cases and formal-verification samples in the subset at three bounds, on 300
   random models of the subset in every pull request and 2,700 more on main. With z3 present
   (CI requires it) it also checks the rules z3 proves at `m`, with lemmas mined without a
   twin, against the reference at `m + 1` and `m + 3`: one fixed model per pull request and
   200 random models on main. The description of #492 records its failing output at
   `fc505d39`, a commit of its branch on main `8b3b6658`, before #470 and #481: `a_value_past_a_byte_is_not_wrapped`
   on the nullable Int at 255 (P40) and the nullable Enum of 256 (P54), and
   `a_rule_proved_at_one_int_range_holds_at_wider_ones` on P43's rule; no test repeats that
   run, and the tests on main pin the fixed behaviour. What it leaves to the `expect` lines: every
   construct outside the subset ([claims.md, I1](./claims.md#0-the-build) lists what
   `RefModel::read` refuses; among it owners and keys, split use cases, triggers, copies,
   `chooses`, `drives`, `now`, multi-entity rules, quantifiers, `after` assertions, property
   verdicts); and, since it compares whole-model runs, the cone choice, the ladder, the
   quotient tables and `TlaNames`. It shares the parser, `build_model`, the rule names, and
   D2's reading of which Int-typed columns are numbers with the builder.
   The theory index's [conclusion](./README.md#conclusion) takes this item and the next as
   the main residual risks of all four documents.
2. **A wrong verdict on any input no test ran [read; runtime].** Since #518 and #522
   (P20) every trace the native checker prints, safety or liveness, plain, reduced or
   symmetric, replays through the step relation of the system it is about before it prints,
   and one that does not is a fault (`LiftFault`, `LassoFault`, `TraceFault`) that ends the
   run `tool_error` (D23): the reduced lift checks every lifted step against the cone's steps
   in every build (`reduce/lift.rs::lift` returns `ReplayError`), and a symmetric path goes
   through `orbit::replays`. Since #529 the lifts' own look-ups fault too
   (`explore/orbit.rs::lift_path` and `reduce/lift.rs::lift` return `NoMove`, `NotInitial` or
   `Length`), and `explore/properties.rs::ComponentRun::decide_property` checks in every build
   that its path breaks the property at its last state and, on a reduced system, reads an
   `always` again on the lifted last state (`PropertyVerdict::Fault`); since #531 the lasso
   lift of a run up to symmetry (`explore/orbit/liveness.rs::Product::lift`) returns `NoMove`
   as well; and since #541 so does `reduce/lift.rs::lift` for a class move with no S path behind
   it (`reduce/partition.rs::Succ::shortest_path` returns `None`, where it ended in a `panic!`
   that this item, pinned at `5f59fae2`, did not name). What remains is narrower: the `expect`s and `panic!`s the index's P20 row lists, with
   the reason each holds or the assumption it rests on (`reduce/lift.rs::lift_stop`, `Product::new`, the searches before a
   lasso lift, callers' preconditions, the rebuilt extended sub-lifecycle that `lift` explores
   again, which holds if `Extended::rebuild` reproduces the reduction's build, and two reads of a
   lifted path that hold whatever path of the system's own states is lifted), would stop the command rather than print a fault; and a
   wrong `ok` has no run-time guard at all. TLC sees a wrong `ok` only where a test or a user
   runs `--with-tlc`.
3. **Bounds [read].** In CI, TLC runs at 3 or more instances only on the corpus (the three
   cases with per-entity flags, and since #476 the `bfs=` runs of 38 cases, at 1 and 3
   instances and, for `safety_at_most_bound` alone, at 4) and on the 3-instance seeds among the 45 main random seeds; never on
   the 25 fallback cones at 2 instances in `engine_equivalence` (`verify --with-tlc` runs
   the 12 of clinic-ops, patent-ops, food-permit-ops, and insurance-claim-ops at 2
   instances, verdicts only, and the 13 of oidc-auth at 1 instance only, so those 13 never
   meet TLC at 2 instances); and never as a graph comparison on unreduced cones above
   300,000 states. Open issue #202 proposes comparing large cones with TLC at 2 instances
   by random walks.
   A divergence that needs more rows than the tested bounds is invisible to every
   differential source (P14).
4. **Predicate shapes no test reaches in a discriminating state [paper; read].** `pred.rs` and `expr.rs`
   are separate implementations of D5 with no shared harness. A difference on a shape
   (say an ordered comparison of a nullable Int against `now`, or a count inside a
   quantifier on the primed state) shows only if some tested model reaches a state where
   the two readings differ.
5. **Fairness per instance [read].** Since #507 the graph comparison compares each edge with
   its params on unreduced sample cones (P25), so a step attributed to the wrong binding shows
   there. A reduced cone's class moves still compare by action alone, and which fair instance
   a step counts for is still checked only through a liveness verdict, on 23 temporal cases
   and the `fair_*` corpus cases.
6. **The comparison code itself [read].** `compare.rs`, `tlc_output.rs`, `tlc_replay.rs` and
   `TlaNames` have no Kani harness and no shadow twin. Since #508 the TLC-side mutation run
   mutates them (`names.rs` for `TlaNames`), but it runs by hand and its kill rate is
   pending. P12 (a disagreement in violated sets that went unreported) and P23 (a per-run
   comparison of runs on two different systems) were places of this kind, fixed in #471 and
   #503.
7. **Effect orders the builder may not exclude [paper; read].** Closed by #485. Step 3 of T1
   for multi-target transitions rests on a builder invariant (P24). On the one shape this
   document did not find excluded, a created row's slot written twice, the two effect
   readings differed, and the post-guards removed the difference only when the `transitions`
   arms carried `.when` guards; #485 reproduced the `EngineDisagreement` and made the build
   reject any action that writes one slot twice (`ConflictingEffects`). What remains is that
   the check lives in the builder, not in the printer, so the printer's reading is right only
   for the actions the builder lets through.
8. **TLC [assumed: TLC].** TLC's fingerprint-based state store can, with small probability, merge two
   distinct states on a hash collision and so miss states (Yu, Manolios and Lamport 1999
   discuss TLC's design; TLC prints its own collision estimate, which
   `cli/src/tlc_output.rs` does not parse). A TLC fault that matches a native fault is not detectable.
9. **Checks with no TLC counterpart [read].** `can_reach` and the one-sided deadlock check (P19,
   fixed in #478: `core/tests/oracle.rs::can_reach_and_deadlocks_agree_with_a_naive_oracle` (main only),
   main only, checks both against an exploration of its own, which shares the step relation
   and the evaluator; TLC still checks neither).

## Threats to validity

[read] for the facts each item names; the items themselves are the author's assessment.

- **Internal.** The argument for T1 is a reading of the printer beside the native step
  relation at one commit. It is not mechanised, and it relies on invariants of the builder
  (that only pre-existing entities start off base, and the effect-order invariant of P24,
  which the build enforces since #485) that no assertion enforces in the printer. A misreading is possible; each step names the functions so it can be
  re-checked.
- **Construct.** "Agreement" in the tests is agreement of verdicts and of labelled graphs
  (with each edge's params since #507, except a reduced cone's class moves), not equivalence
  of the modules. The decoder is the same code in the
  printing and in the comparison, so a consistent renaming is invisible (harmless) and a
  decoder that maps two TLC states to one native state would show as a count mismatch,
  which the comparison checks.
- **External.** The inputs are 40 sample roots, 86 corpus cases, 23 temporal cases, the 36
  `bfs_and_tlc_*` tests of `engine_equivalence.rs`, and 45 random models with TLC. The samples were written alongside the tools,
  so they exercise what the authors thought of. The random generator has a fixed shape
  (no counts, three to five entities).
- **Conclusion.** No numbers in this document were re-measured; the counts were recounted
  from the code at the pin. The P12, P21, P23, and P24 statements were readings of the code
  when this document found them. The fixes of P12 (#471), P21 (#469) and P24 (#485)
  reproduced each first, as the index's rows record; P23's fix (#503) records no
  reproduction.

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

## Claims and open problems

<!-- derived-from #1-the-translation -->
<!-- derived-from #2-the-agreement-rules -->

Rows T1 to T16 of the [claims inventory](./claims.md#d-agreement-with-tlc) cite this
document. The open problems this document confirmed, extended, or added, in the
[index](./README.md#open-problems), with what became of each. [read]

| # | What this document found |
|---|---|
| P12 | **Fixed in #471.** Confirmed; restated: the display path prints an unnamed `error:` line for the first non-replaying trace only, which does not change `result:` or the exit status. |
| P13 | **Fixed in #472.** Confirmed as stated. |
| P14 | **Narrowed in #476**: the corpus's `bfs=` runs take TLC, and `VERIFY_REDUCED_ONLY` lists the 30 cones compared reduced only. Confirmed; extended: unreduced cones over 300,000 states were silently compared reduced only; oidc-auth's 13 `VERIFY_FALLBACK` cones meet TLC at 1 instance only, in `engine_equivalence` and in `verify --with-tlc` alike (issue #202 proposes a fix; still open). |
| P15 | **Fixed in #472.** Confirmed as stated (at least 30 asserted, 79 present); the cases are now compared with `CASES` both ways. |
| P16 | **Narrowed in #486, #497 and #508.** Confirmed; extended: release-gate item 2 recorded as held on a narrower check (the CHANGELOG erratum of #497 says so; #508 added the TLC-side mutation run); the `--with-tlc` step did not move to `release.yml`, and native-temporal-verification.md now says so. Open: branch protection and the owner's decision on the gate proposal. |
| P21 | **Fixed in #469.** Confirmed; extended to the unreduced `WfNext` runs and to a false `tool_error` under `--symmetry always` without TLC. The reduced `WfNext` run is not affected: it records no entry when undecided. |
| P22 | **Fixed in #475 and #494.** New (prose): D17 omitted that a step with `s' = s` is checked by neither engine. Both halves read; `Probe::Loop` is set in every build. |
| P23 | **Fixed in #503.** New (evidence): the per-run comparison pairs the native `WfNext` run on the reduced cone (`decide_liveness_reduced`) with TLC's `_WF` module printed from the unreduced cone, so Proposition 0 does not apply on that path. |
| P24 | **Fixed in #485.** New (evidence): for a `Transition` with a multi-target source, the printer's and the native effect application agree only under a builder invariant on effect order that nothing enforced as such; one shape was not found to be excluded, and on it the post-guards reconciled the two only when the `transitions` arms carried `.when` guards. #485 reproduced it and made the build reject an action that writes one slot twice. |
| P26 | New (evidence), from the owner's review: agreement cannot see a common-mode fault (item 1 of [Residual risk](#residual-risk)). Partly closed by the reference interpreter of that item, for a subset of the language. |


## Appendix: the pinned commit

[read] The document was first written against `fb6928c7` (origin/main, 2026-11-28), later
than the index's first pin `b83b9544`; between those two no path it cites changed in a part
it relies on. It was then pinned to `fec53d6a`, then to `5f59fae2`, and it now describes `5ae62f2c` (the last
two paragraphs of this appendix). `git diff --stat fb6928c7 fec53d6a` over the cited
paths shows changes in the printer (`emit/tla/mod.rs`, `module.rs`, `names.rs`), the TLC
readers (`tlc_output.rs`), the comparison and the runs (`verify/compare.rs`, `native.rs`,
`jobs.rs`, `tlc.rs`, `bfs.rs`, `end.rs`, `refuted.rs`, `escalate.rs`, `property_cone.rs`),
`ci.yml`, `mutants.yml`, `.cargo/mutants-tlc.toml` (new), `scripts/verify-samples.sh`,
`scripts/kani-relevant.sh`, the corpus, and the equivalence tests. Every `file.rs::symbol`
this document cites, and the run functions it names, were checked against `git diff fb6928c7
fec53d6a`; each cited item a hunk touches was read again:

- the printer: `emit/tla/mod.rs::print` (now `print_with`, and `print_bound` of #507), the
  bound `Next` of `module.rs`, and the `can_reach` comment of `module.rs::checks` (#498);
  `mod.rs::domain_set`, `names.rs::TlaNames::decoder` and `explore/moves.rs::initial_states`
  changed only in integer casts. The effect, action, guard, `init` and fairness code the T1
  argument reads did not change, so that argument carries over;
- the comparison and the runs: `compare.rs::disagreements` and `unreplayed` (#471, #503),
  `native.rs::Answer::outcome`, `Plain::of`, `native_property_after`,
  `always_up_to_symmetry` and `liveness_up_to_symmetry` (#469, #503, and the faults of P20),
  `property_cone.rs::after_plain` and `after_wf` (a cone that leaves a mark out is inexact,
  #477), `explore/bfs.rs::Bfs::take` (the `Probe::Loop` branch is unchanged),
  `reduce/lift.rs::lift` (#518, #522), `End::result` (#522, #524), and
  `build/mod.rs::Builder::one_write_per_slot` (#485);
- the tests: `engine_corpus.rs::check_case`, `rule_2`, `rule_8` and the `CASES` check
  (#472, #476), and `engine_equivalence.rs`'s edge comparison and `VERIFY_REDUCED_ONLY`
  check (#476, #507);
- `ci.yml` (`tlc-batch-both`, the seeds), `mutants.yml`, both mutants configurations, and
  docs/engine-agreement.md's kill-rate tables;
- the counts: corpus cases, `expect` lines, families and flags, `bfs_and_tlc_*` tests,
  `TEMPORAL_CASES`, `VERIFY_FALLBACK`, `VERIFY_REDUCED_ONLY`, `TWINS`, and Kani harnesses.

No other cited item changed, so its earlier read carries over; this includes
`tlc_output.rs`'s `TlcRun::outcome` and `verdict`, `PropertyCone::bundle`, `verify/tlc.rs`'s
`run_cfg`, `run_whole` and `model_traces`, `tlc_replay.rs::explain_violation`, and
`pred.rs`. The reduced-cone printer (`bits`, `rise_expr`) was not read at either pin
(`unread@pin` where cited).

[read] From `fec53d6a` to `5f59fae2` the cited paths change by #529 (`reduce/lift.rs::lift`,
`explore/orbit.rs::lift_path`, `explore/properties.rs::ComponentRun::decide_property` and
`explore/graph.rs`: the lifts' look-ups and the `always` checks fault), #531 and #536
(`explore/orbit/liveness.rs::Product::lift`), #532 (`refuted.rs`, `escalate.rs`: instances per
entity) and #535 (`engine_corpus.rs`: an escalated line with per-entity counts indexes no
`bfs=` verdict). Each was read at `5f59fae2`; residual risk item 2 was rewritten for #529 and
#531, and nothing else here cites what changed in the crates. #534 changes a z3 test and #537 a symmetry
test, which this document does not cite. #533 moves
`oracle.rs::can_reach_and_deadlocks_agree_with_a_naive_oracle` into the slow list of
`.config/nextest.toml` (`[profile.pr] default-filter`), which the two "main only" marks of the
oracle's comparison rest on (§1, "What the module does not state", and Residual risk); both
were checked against it. `.config/nextest.toml` is among the paths this appendix tracks.

[read] From `5f59fae2` to `5ae62f2c` (the merge of #542) the cited paths change by #539
(af6354a4: `engine_equivalence.rs::run_tlc_on` and the replay of
`recorded_tlc_runs_are_the_same_in_a_batch_jvm` pass `-lncheck final`, `LIVENESS_AT_THE_END`)
and #541 (ba47c45c: `reduce/lift.rs::lift` and `reduce/partition.rs::Succ::shortest_path`, a
class move with no S path behind it is the `NoMove` of that step, with a test in
`core/tests/reduce.rs`); `.config/nextest.toml` does not change. Both were read at
`5ae62f2c`. [Batch JVMs](#batch-jvms), the T1 summary, and residual risk item 2 were
rewritten for them; at `5f59fae2` item 2 should also have named `shortest_path`'s `panic!`.

## Errata

Changes to statements of this document after it was first written, newest last.

- **2026-11-28, after an adversarial review** (commits `b75044db`, `25185bd9`, and
  `7227d440`). The revisions that followed the review were checked against the same commit as
  the first version, `fb6928c7`.
- **2026-11-28, after the owner's review.** The paragraph on what the document cites and what
  changed between `b83b9544` and `fb6928c7` moved from the opening to
  [Appendix: the pinned commit](#appendix-the-pinned-commit), unchanged. Item 1 of
  [Residual risk](#residual-risk) now also names the staging of a property cone's runs and
  `TlaNames`, and P40, P54, and P32 as faults of its kind.
- **2026-11-28, after the owner's review of semantics.md (P60).** semantics.md now defines
  `T_β(M)` from the model's text, and the build implementing it is claim I1. Proposition 0
  is unchanged: its `v*` is the verdict on the system `T'` both engines read, which is built.
  The common-mode code paragraph and Residual risk item 1 now name I1 as the claim a builder
  fault breaks.
- **2026-11-28, final audit: pin `fec53d6a`.** The pin moved from `fb6928c7` to `fec53d6a`
  ([Appendix](#appendix-the-pinned-commit) lists what was read again). The fixes merged in
  between are now described as fixed, in the past tense, with their PRs and tests: P12
  (#471), P13 and P15 (#472), P14 narrowed (#476), P16 narrowed (#486, #497), P20 (#518,
  #522), P21 (#469), P22 (#494), P23 (#503), P24 (#485, which the paragraphs on step 3 had
  called unenforced while the Effects paragraph said it was enforced), and P25 (#507: edges
  are compared with their params, so "without bindings" was false). The mutation-run section
  describes both configurations: the liveness ladder's recorded kill rates and the TLC-side
  run of #508, whose kill rate is pending. The `tlc-batch-both` job and the CHANGELOG
  erratum are named. The counts were recounted: 86 corpus cases with 210 `expect` lines, 36
  `bfs_and_tlc_*` tests, 26 twins; the `bfs=` runs take TLC. Residual risk items 2, 5, 6
  and 7 were rewritten for P20, P25, the TLC-side mutation run, and P24; item 2 names the
  internal `expect`s the lift still has. The deadlock text cites D25. `VERIFY_FALLBACK` has
  25 cones since #526.
- **2026-11-28, re-pin to `5f59fae2`** (the merge of #537). The Appendix says what changed
  after `fec53d6a` and what was read. Residual risk item 2 no longer lists
  `explore/orbit.rs::lift_path` and the look-ups of `reduce/lift.rs::lift` as panicking, nor
  an `always` counterexample as checked by `debug_assert_eq!` only: since #529 both fault, and
  since #531 so does the symmetric lasso lift.
- **2026-11-28, second audit at `5f59fae2`.** The Bounds item said each of the three per-entity cases raises
  two entities; `sym_per_entity_bounds` raises Room only, Booking staying at 2. Residual
  risk item 2 follows the index's completed P20 list.
- **2026-11-28, third audit at `5f59fae2`.** Batch JVMs said `RDRA_TLC_BATCH=both` compares
  every `engine_equivalence` job; it compares the jobs routed to a batch JVM, and the
  invariant scan as well. A paragraph there, the T1 summary, and T3's, T5's and T11's rows say
  that since af6354a4 (`engine_equivalence.rs`, read at that commit) the harness runs TLC
  under `-lncheck final`, which `verify --with-tlc` does not. P16 is narrowed in #486, #497
  and #508, as the index has it. The two P19 mentions give its status. Residual risk item 1
  cites the failing run recorded in #492's description, where it said the interpreter
  failed before #470 and #481 with no record named, and refers to I1 for the out-of-subset
  list; item 3 says the `bfs=` runs are at 1 and 3 instances, and at 4 for one case.
- **2026-11-28, fourth audit; re-pin to `5ae62f2c`** (the merge of #542). The Appendix says
  what #539 and #541 changed after `5f59fae2`; the `-lncheck final` statements, which said
  "after the pin", now cite #539. Residual risk item 2 stopped at `5f59fae2` while citing
  `reduce/lift.rs::lift`, and at that commit it should also have named
  `Succ::shortest_path`'s `panic!`; it now credits #541 and names the `expect`s a lift reaches
  outside the lift's files. The Appendix said the document cites nothing #533 changed, but
  the oracle's "main only" marks rest on the line #533 added to `.config/nextest.toml`. T1's
  Effects argument assumed, without saying so, that no two params of one action that can bind
  one slot write one variable; the Status paragraph, Effects and the Status table now state
  it as an unchecked assumption. Effects covers `Create`, Predicates a slot index read as a
  number, Fairness the `TRUE` of a scope that keeps no fair action, and the `WF_vars(Next)`
  sentence says it is of `print` only. The Status paragraph names `emit/tla/names.rs`.
