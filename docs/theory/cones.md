# Cones of influence, reductions, and symmetry

<!-- derived-from ./semantics.md -->
<!-- derived-from ./claims.md#a-cones-of-influence-and-reductions -->

This document states, for rows A1 to A28 of the [claims inventory](./claims.md), when a run
on a smaller system decides what `T_β(M)` (D13) would: a cone of influence, a cone reduced by
an independent sub-lifecycle, a run up to slot permutations, and a run at more instances. It
uses the notation of [semantics.md](./semantics.md) and does not redefine it.

This document describes main at `91524792`. Code is cited as in
[semantics.md](./semantics.md): paths are relative to `crates/rdra-ish-core/src/lifecycle/`
unless they start with `cli/` (`crates/rdra-ish-cli/`), `emit/` (`crates/rdra-ish-emit/src/`)
or `core-root/` (`crates/rdra-ish-core/src/`). Test marks, such as "(main only)" and
"(release jobs)", are those of the [claims inventory](./claims.md#how-to-read-a-row). Every
statement about behavior is a reading of the code, and every measured number is quoted from
the doc comment or test named beside it.

Each result has four parts: the statement, its hypotheses with the code that establishes
each one, the proof (marked **Proof** when every step is given, **Proof sketch** when a step
is named but not derived), and the evidence that checks it. A step that does not go through,
or a guarantee the code does not give, is named where it arises. The evidence classes and
open gaps of each claim are in the rows of the
[claims inventory, section (a)](./claims.md#a-cones-of-influence-and-reductions).

---

## Contents

1. [Notation added here](#notation-added-here)
2. [The cone of a seed](#the-cone-of-a-seed)
3. [Projection and lifting](#projection-and-lifting)
4. [Safety cones](#safety-cones)
5. [Property cones and fairness](#property-cones-and-fairness)
6. [`can_reach` cones](#can_reach-cones)
7. [The reduction by an independent sub-lifecycle](#the-reduction-by-an-independent-sub-lifecycle)
8. [`WF_vars(Next)` on a reduced cone](#wf_varsnext-on-a-reduced-cone)
9. [Instance symmetry](#instance-symmetry)
10. [Bound escalation](#bound-escalation)
11. [Caps, the memory limit, and `not decided`](#caps-the-memory-limit-and-not-decided)
12. [Related work](#related-work)
13. [Threats to validity](#threats-to-validity)

---

## Notation added here

<!-- derived-from ./semantics.md#notation -->

`T = T_β(M) = (S, I, →)` throughout, with variables `V` (D2) and actions `A` (D6). Read `T`
as the system the build returns: the theorems here relate a cone to that system, and some
steps use facts about the builder's output. That it is `T_β(M)` of D13 is claim
[I1](./claims.md#0-the-build), which no theorem here needs. For an
action `a`:

- `Rd(a)` is `cone.rs::TransitionSystem::plain_reads`: the existence of each `Existing` and
  `Parent` parameter, every variable a guard reads other than a copy guard and a mark guard,
  every transition source, and `now` for `Tick`.
- `Mk(a)` is `cone.rs::TransitionSystem::mark_reads`: a list of pairs `(μ, R_μ)`, one per
  raised mark `μ` that `a` reads (D11), with `R_μ` the existence of the `Triggered` parameter
  `μ` selects and the other variables its guards read.
- `Cp(a)` is `cone.rs::TransitionSystem::copy_reads`: a list of pairs `(τ, R_τ)`, one per copy
  guard (`GuardWhy::Copy`, the guard of `sets(UC, Child.col == Parent.col)`), with `τ` the
  column it pins in the successor and `R_τ` what it reads.
- `Wr(a)` is `cone.rs::TransitionSystem::action_writes`, the union of `effect_writes` over the
  effects: the existence for `Create`; the existence, every column, and every owner of the
  entity for `Delete`; the variable for `Assign`, `Transition`, `Choose`; `now` for `Tick`.
  `Wr(a)` is syntactic. It over-approximates what a step can change: D8 keeps a deleted row's
  owners, but `Wr` lists them (the over-listing only adds actions to a cone).
- `ex(v)` is `cone.rs::TransitionSystem::existences`: the existence of `v`'s entity, and for an
  owner also the existence of its parent.

For `W ⊆ V`, `π_W : S → S|W` projects a state onto `W` (`TransitionSystem::project`), and
`π_W(σ)` applies it to each state of a trace or behavior. Two behaviors are *stutter
equivalent* when they agree after every finite run of repeated states is collapsed to one
state.

`A_W(K) = K ∪ {a ∈ A : Wr(a) ∩ W ≠ ∅}` is the set of actions *taken* by `W` with kept set
`K`.

---

## The cone of a seed

<!-- derived-from ../state-derivation.md#exploration-by-cone-of-influence -->

**Definition 1 (closure).** For a seed `σ ⊆ V` and a kept set `K ⊆ A`, a set `W ⊇ σ` is
*closed* when, with `A_W = A_W(K)`:

- (C1) `Rd(a) ⊆ W` for every `a ∈ A_W`;
- (C2) for every `a ∈ A_W` and `(μ, R_μ) ∈ Mk(a)`: if `exact_W(a, μ)` then `μ ∈ W` and
  `R_μ ⊆ W`, where `exact_W(a, μ)` holds when `a ∈ K`, or `μ` is a counter of a counted edge
  (`is_counted`), or every action `b` with `μ ∈ Wr(b)` is in `A_W`;
- (C3) for every action `a` and `(τ, R_τ) ∈ Cp(a)`: `τ ∈ W ⇒ R_τ ⊆ W`;
- (C4) `ex(v) ⊆ W` for every `v ∈ W`.

`cone(σ, K)` is the least closed set (`cone.rs::TransitionSystem::cone`).

**Lemma 1.** The least closed set exists, and the loop of `TransitionSystem::cone` returns it.

*Proof.* Each condition has the form "if a property of `W` holds, then some variables are in
`W`", and each premise is monotone in `W`: `A_W` grows with `W`, and so `exact_W(a, μ)` grows
with `W`, since its third disjunct quantifies over writers that must be in `A_W`. The loop starts at
`σ` and adds a variable only when some condition requires it for the current set `s`. By
induction on the iterations, every added variable is in every closed `W ⊇ σ`: if `s ⊆ W`
then `A_s ⊆ A_W` and `exact_s ⇒ exact_W`, so every variable a condition requires of `s` is
required of `W` too. The loop stops on a set that violates no condition, since an
iteration that finds a violation adds a variable. `V` is finite, so it stops, and its result
is a closed set contained in every closed set: the least one. ∎

**The seeds.** Each kind of check builds its seed and kept set differently.

| Target | Seed and kept set | Code |
|---|---|---|
| Entity view `E` | `Exists(E)` and `E`'s columns; `K` = actions with a written (`New` or `Existing`) parameter on `E` | `target_seed`, `Target::Entity` |
| Invariant | binder existences and the body's reads; `K = ∅` | `target_seed`, `Target::Invariant` |
| `after(UC)` | binder existences, the post-condition's reads, and `Wr(a)` of each checked step `a`; `K` = those steps | `target_seed`, `Target::AfterAssert` |
| Property `P` | `property_reads(P)`, and for each fair action `a`: `Wr(a)` and the existence of each `New` parameter's entity; `K` = the fair actions | `property_seed` |
| `can_reach` | the fixed point of `keep_writers(·, true)`: `K` = every action writing into the cone, the seed grown by the existence of each entity such an action creates | `reach_seed`, `keep_writers` |

The function `closed_cone` (`keep_writers(·, false)`) is called only by the TLA+
printer (`emit/tla/module.rs`). The same closure feeds verdicts through
`cone.rs::TransitionSystem::exact_target_cone(target, slots)`, which is `keep_writers(·, slots)`
and then `cone`. With `slots = false` it decides an entity cone that leaves a mark out
(`target_cone`, [Safety cones](#safety-cones)) and the last round of `recheck_on` when its
caller passes `slots = false`, as `targets.rs` and `cli/src/verify/bfs.rs::recheck_cones` do.
With `slots = true` it is the last round of
`recheck_on(…, true, …)` from `cli/src/verify/escalate.rs::escalate`, and
`select.rs::TransitionSystem::select` takes `exact_target_cone(t, true)` to name the entities
the targets of a scoped run read.

**Definition 2 (deviations).** Let `W = cone(σ, K)` and `C = T|W =
restrict_to(W, K)`. `W` has

- a *freed entity* `E` when some `a ∈ A_W` has a `New` parameter on `E` and `Exists(E) ∉ W`
  (`cone.rs::TransitionSystem::cone_frees_slots`);
- a *left-out mark* `μ` when some `a ∈ A_W` has `(μ, R_μ) ∈ Mk(a)` and `μ ∉ W`
  (`cone.rs::TransitionSystem::cone_leaves_marks` decides it for a cone;
  `trace.rs::TransitionSystem::free_triggers` names the use cases a trace runs this way);
- a *dropped copy* when some `a ∈ A_W` has `(τ, R_τ) ∈ Cp(a)` with `τ ∉ W`.

`W` is *exact* when it has no freed entity and no left-out mark. A dropped copy does not
spoil exactness (Lemma 3, case (iv)).

**What `restrict_to` keeps.** `restrict.rs::TransitionSystem::restrict_to_mapped` keeps the
actions of `A_W` and drops the rest; for a kept action it keeps the effects that write `W`,
every guard whose reads are in `W` except a copy guard whose target is outside `W` and a guard
that names a dropped parameter, and every parameter except a `New` parameter on an entity whose
existence is outside `W`, a `Triggered` parameter whose mark is outside `W` (its guards go with
it), and a parameter on an entity neither kept nor stubbed. A cone has no stub: by (C4) the
parent of every owner in `W` has its existence in `W`.

---

## Projection and lifting

<!-- derived-from ../design/cone-soundness.md#a-propertys-cone-decides-under-its-own-fairness -->

Everything in this section and the next three rests on two lemmas about one step.

**Lemma 2 (projection).** Let `W = cone(σ, K)` and `C = T|W`. Then `π_W(I_T) = I_C`, and for
every step `s →_{a,p} s'` of `T`:

- if `a ∉ A_W` then `π_W(s') = π_W(s)`;
- if `a ∈ A_W` then `π_W(s) →_{a|W, p|W} π_W(s')` is a step of `C`, where `p|W` drops the
  parameters `restrict_to` drops.

Hence `π_W(Reach(T)) ⊆ Reach(C)`, and every trace or behavior of `T` projects, after its
stutter steps are allowed, to one of `C`. No hypothesis is needed.

*Proof.* `I_T` is a product over components of per-component value sets (D4), and
`restrict_to` keeps each kept variable's declaration with its start and base; the projection of
a product onto some of its factors is the product of those factors, so `π_W(I_T) = I_C`.

If `a ∉ A_W`, then `Wr(a) ∩ W = ∅`; every effect writes only variables of `Wr(a)` (D8), so no
component in `W` changes.

If `a ∈ A_W`: (1) Every parameter `a|W` keeps binds the same slot under the same admissibility
rule (D7). `Existing`, `Parent`, and `Triggered` read their entity's existence, which is in `W`
for a kept parameter; `New` takes the lowest absent slot, read from `Exists(E) ∈ W`;
`Earlier` admits any slot and is selected by a `GuardWhy::Run` guard
(`build/actions.rs`), which is an ordinary guard. (2) Every guard of `a|W` is a guard of `a`
whose reads lie in `W`, so it has the same value on `π_W(s)` and `π_W(s')` as on `s` and
`s'`. (3) Every effect of `a|W` is an effect of `a` writing `W`; its new value is a literal, a
parameter's slot, a transition arm applied to the source (`Rd(a) ⊆ W` by (C1)), or `now + 1`
(`now ∈ Rd(a)`). The fork `s'` took of each `Transition` and `Choose` on `W` is a fork of
`a|W`, and every successor guard of `a|W` holds on it by (2). So `π_W(s') ∈ Post_{a|W}(π_W(s),
p|W)`. Induction on the length of a trace gives the rest. ∎

**Lemma 3 (lifting).** Let `W` be exact. For every state `s ∈ S` (reachable or not) and every
step `π_W(s) →_{a|W, p} r'` of `C`, there is a step `s →_{a, p} s'` of `T` with
`π_W(s') = r'`. The action and the parameters are the same.

*Proof.* (i) *No parameter is dropped.* A `New` parameter is dropped only on a freed entity, and a
`Triggered` one only with a left-out mark; `W` has neither. An `Existing` or `Parent` parameter's
existence is in `Rd(a) ⊆ W`; an `Earlier` parameter's entity holds the run mark its `Run`
guard reads, whose existence is in `W` by (C4); a `Triggered` parameter's existence is in
`R_μ ⊆ W` by (C2) with the mark exact. So `p` binds every parameter of `a`, each admissibly in
`s` because admissibility reads only existences in `W`.
(ii) *Every guard of `a` other than a dropped copy guard holds at `s` and in the successor
built below.* A guard that is neither a copy nor a mark guard reads `Rd(a) ⊆ W`; a mark guard
reads `μ` and `R_μ`, in `W` since the mark is exact; a copy guard with `τ ∈ W` reads
`R_τ ⊆ W` by (C3). Each of these guards is a guard of `a|W` and holds on the `W`-part.
(iii) *Effects.* Build `s'` from `s` by applying every effect of `a`. On `W` take the forks
`r'` took. A `Transition` writes a variable it reads, so every `Transition` of `a` is on `W`.
A `Choose` outside `W` takes any listed value, except as in (iv). Then `π_W(s') = r'`.
(iv) *A dropped copy guard.* It reads `τ' = source`, with `τ ∉ W`. The builder writes a copy
target only through a `Choose` over the target's whole domain and rejects a second write of
the same column in the action (`build/sets.rs`, `prior_write` gives `ConflictingEffects`), and
`core-root/analysis/effect.rs::copyable` admits a copy only between columns with the same values and a
nullable source only into a nullable target. So the `Choose` on `τ` has a fork equal to the
source's value in `s'`, which does not depend on `τ`. Take that fork. Every guard of `a` holds at
`(s, s')`, so `s' ∈ Post_a(s, p)`. ∎

**Theorem A1 (exact cones).** If `W = cone(σ, K)` is exact then

1. `Reach(C) = π_W(Reach(T))`;
2. every finite trace of `C` lifts to a finite trace of `T` with the same actions and
   parameters, step for step, and the same projection;
3. every behavior of `C` lifts to a behavior of `T` with the same projection, and every
   behavior of `T` projects to a behavior of `C` (stutter steps allowed).

So for every check whose reads are in `W` (`restrict_to` keeps exactly those), the verdict on
`C` is the verdict on `T`.

*Proof.* `⊆` of (1) and the second half of (3) are Lemma 2. For `⊇` and (2), induct on a trace
`r₀ … r_k` of `C`: `r₀ ∈ I_C = π_W(I_T)` gives `s₀ ∈ I_T` with `π_W(s₀) = r₀`, and Lemma 3 lifts
each step from the current `s_i`, whatever `s_i` is. The first half of (3) is the same
construction continued forever. For a state rule, an `always`, or `can_reach`'s trigger, the
value at `s` depends on `π_W(s)` only, so a violation exists in one system iff it exists in
the other. For an `after` assertion, the checked step's writes are in the seed, so a step of
`T` changes the state iff its projection does (D17 checks state-changing steps). ∎

*Hypotheses and code.* Exactness: `cone_frees_slots` and `cone_leaves_marks` are false for the
cone. Per trace, `trace.rs::TransitionSystem::annotate` records the freed entities a trace
creates past the bound (`over_bound`) and the left-out marks it runs without
(`free_triggers`), and `trace.rs::Trace::vouched` holds when both are empty (Corollary A28′
below). The builder facts of (iv): `build/sets.rs` and `core-root/analysis/effect.rs::copyable`.

*Evidence.* P-full (this section). D: `core/tests/cone.rs::cone_exploration_equals_the_projection_of_the_whole_system`
compares the cone's reachable set with the projection of the whole system's;
`::a_parent_existence_joins_the_cone_without_its_columns` checks (C4). The comparison
(`core/tests/cone.rs::assert_cones_exact`) asserts equality of the reachable set with the
projection only on cones with neither deviation. On a cone that frees slots or leaves a mark
out it asserts the inclusion `π_W(Reach(T)) ⊆ Reach(C)` of Theorem A2, and it asserts that no
entity cone that frees no slots leaves a mark out ([Safety cones](#safety-cones)).
`::a_cone_that_leaves_a_mark_out_keeps_the_whole_systems_verdict_and_view` runs it on a cone
with a left-out mark, so the exactness condition is tested against a cone that has one.

---

## Safety cones

<!-- derived-from ../state-derivation.md#exploration-by-cone-of-influence -->
<!-- derived-from ../formal-verification.md#known-verification-gaps -->

**Theorem A2 (over-approximating cones).** For every `W = cone(σ, K)`, exact or not,
`π_W(Reach(T)) ⊆ Reach(C)`. Hence `C ⊨ R ⇒ T ⊨ R` for every state rule and `after` assertion
`R` whose reads are in `W` (for an `after` assertion, the checked steps must be in `K` and
their writes in the seed, as the seeds table gives them).

*Proof.* The inclusion is Lemma 2. A state rule reads `W`, so a violating state `s ∈ Reach(T)`
projects to a violating state `π_W(s) ∈ Reach(C)`. For `after(UC)`, D17 checks only steps that
change the state. Let `s →_{a,p} s'` be a reachable step of `T` with `a` a checked step of `UC`,
`s' ≠ s`, and the post-condition false. The seed holds `Wr(a)` and `a ∈ K ⊆ A_W` (the `after` row
of the seeds table), so the variables the step changes are in `W`, and `π_W(s') ≠ π_W(s)`. By
Lemma 2, `π_W(s) →_{a|W, p|W} π_W(s')` is a step of `C`. It changes the state, and the
post-condition reads `W`, so the step violates the assertion in `C`. A step of `T` outside `A_W`
projects to a stutter, which D17 does not check. ∎

Theorem A2 covers *universal* checks only: a violation in `T` gives a violation in `C`. The
findings of an entity cone are existential or branching. `StateDiag::NoWitness`,
`::NeverRuns`, `::ArmNeverTaken`, and `::ValueNeverChosen` say that no reachable state or edge
has a property. `::Deadlock` says that a reachable state is stuck: no path from it changes the lifecycle column or deletes the row ([semantics.md, D25](./semantics.md#d25)). An over-approximating `C`
reaches more states and edges than `π_W(T)` does, so it can satisfy such a check that `T`
fails, and it can also fail one that `T` satisfies. Those checks need exactness (Theorem A1),
or A8's bisimulation on a reduced cone. No entity cone leaves a mark out
(`cone.rs::TransitionSystem::target_cone`, below); an entity cone can still free slots,
and then its findings are those of a larger bound (Proposition A28).

A counterexample of a non-exact cone is a run of `C`. What it says about `T` depends on the
deviation.

**Proposition A28 (freed slots lift at a larger bound).** Let `W` have no left-out mark and let
`F` be its freed entities. Let `ρ` be a trace of `C` that binds `c_E` `New` parameters on each
`E ∈ F`. Then `ρ` lifts, step for step, to a trace of `T_{β'}(M)` with `n'_E = max(n_E, c_E)` for
`E ∈ F` and `n'_E = n_E` otherwise. In particular, when `c_E ≤ n_E` for every `E ∈ F`, it lifts to
`T_β(M)` itself.

*Proof.* No variable of `W` belongs to an entity of `F`, by (C4) and `Exists(E) ∉ W`, so `C` is
the same system at `β` and `β'`. Lift as in Lemma 3, except that a `New` parameter on `E ∈ F` is
bound to the lowest absent slot of `E` in the lifted state. The lifted trace runs only actions
of `A_W`, so `E`'s slots are taken only by these `c_E` bindings; slots are never reused (D7), so
the `j`-th binding finds slot `j - 1` absent whenever `j ≤ n'_E`. No guard of an action in `A_W`
reads `Exists(E)` (it is outside `W` and every non-copy, non-mark guard reads `Rd ⊆ W`), and no
other step of the proof of Lemma 3 reads `F`. ∎

`trace.rs::TransitionSystem::over_bound` lists exactly the `E ∈ F` with `c_E > n_E`, and
`cli/src/state_findings.rs` prints `note: … creates more E instances than the instance bound
allows`. So a safety counterexample with an empty `over_bound` is a reachable violation of
`T_β(M)`, and one with a non-empty `over_bound` is one of `T_{β'}(M)`.

*Evidence.* P-full (this section). D:
`core/tests/trace.rs::a_trace_that_takes_more_slots_than_a_freed_cone_counts_is_marked` (a
forbidden's cone that does not count Payment's slots finds a trace that creates a second
Payment at one instance, and its `over_bound` is `["Payment"]`), and
`core/tests/cone.rs::an_entity_the_view_does_not_read_never_runs_out_of_slots` (on
`samples/personal-info` at one instance, the card and address views reach the states of use
cases that also create an `AuditLog` the views do not read). Neither test lifts a trace to
`T_{β'}(M)`; the lift is the proof's.

**Corollary A28′ (exactness per trace).** Let `ρ` be a trace of `C = T|W` none of whose steps
runs an action that reads a mark outside `W` (`free_triggers` of `ρ` is empty). Then `ρ` lifts,
step for step, as in Proposition A28, whether or not `W` has a left-out mark elsewhere.

*Proof sketch.* The proofs of Lemma 3 and Proposition A28 use exactness one step at a time:
case (i) needs that the step's own action has no dropped `Triggered` parameter, and case (ii)
that its own mark guards read marks in `W` together with what they read. Let the step's action
`a ∈ A_W` read a mark `μ ∈ W`. Every writer `b` of `μ` has `Wr(b) ∩ W ≠ ∅`, so `b ∈ A_W`, so
`exact_W(a, μ)` holds by its third disjunct, and (C2) gives `R_μ ⊆ W`. So `restrict_to` keeps
the `Triggered` parameter and the mark guards of `a`, and Lemma 3's cases (i) and (ii) go
through for that step. No step of `ρ` runs an action that reads a mark outside `W`. ∎ That `free_triggers` lists every step whose
action reads a mark outside the cone (`trace.rs`) is the one fact the corollary takes from the
code.

**Left-out marks: candidates and refinement.** A cone leaves `μ` out when some writer of `μ`
is not taken; then `restrict_to` drops the handler's guard on `μ` and its `Triggered`
parameter, and `C` runs the handler without the event. Nothing bounds the bound at which `T`
would allow it: the event order (D11) can forbid the run at every `β`. A counterexample that
runs such a handler is a candidate only. The code decides every candidate again before it
counts, as counterexample-guided refinement does (Clarke et al. 2000):

- `targets.rs::TransitionSystem::recheck_on` decides a state rule or an `after` assertion by
  rounds. Each round runs the current cone, reduced as `verify` reduces one. A counterexample
  the cone vouches for (`Trace::vouched`: no `free_triggers`, and with `slots` no `over_bound`)
  is the verdict, a lifted trace by Corollary A28′. Otherwise `TransitionSystem::refine` adds
  to the kept actions every action that raises an event the trace ran without, and with
  `slots` adds the existence of each entity the trace created past the bound, and the next
  round runs. A round that names nothing new runs the exact cone
  (`cone.rs::TransitionSystem::exact_target_cone`: `keep_writers` to its fixed point, so no
  mark is left out and, with `slots`, no slot is freed). A round without a counterexample
  decides the check: it holds when the run ended, and is undecided when a cap stopped it.
  The loop ends because each round keeps more actions or reads more variables, and the exact
  cone bounds both. That the exact cone's counterexample is vouched for is a `debug_assert!`
  only; it follows from `exact_target_cone`, whose own `debug_assert!`s state the same.
- `verify` (`cli/src/verify/bfs.rs::recheck_cones`, called before the prover reads the safety
  verdicts) and `check` (`targets.rs`, the verdict of an invariant or `after` target) recheck,
  without `slots`, each violation whose trace has a free trigger; only the vouched violations
  and the rechecked ones count (`BfsCone::violations`), so `cli/src/verify/refuted.rs::refuted` and the
  run's result read no candidate. A freed-slot counterexample without a free trigger still
  prints, with its `over_bound` note, as Proposition A28 allows, and `cli/src/verify/refuted.rs::instances`
  raises the count of each entity in `over_bound` to the rows the trace creates of it, keeping
  every other entity's count.
- `cli/src/verify/escalate.rs::escalate` rechecks with `slots`, so a counterexample it prints is a run of
  `T_{β+k}(M)` itself, and it prints the instances of the cone that found it.
- A property cone with a left-out mark counts only its `ok`: `cli/src/property_cone.rs::inexact` gives
  `Why::LeavesMarks` (after `Why::FreesSlots`), so a counterexample there is `not decided` on
  the cone and runs on the whole model, and `explore/properties.rs::TransitionSystem::decide_property`
  decides an explored form's violation again on `T` when `cone_frees_slots` or
  `cone_leaves_marks` holds.

The recheck guards the counterexamples only. For a universal check a candidate cannot yield a
wrong `ok`: a state rule, an `after` assertion, or a property passes on `C` only if it passes
on `T` (Theorems A2 and A4.1). What the recheck prevents is a candidate printed or counted as a
counterexample, and an `at k instances` that understates the bound a freed-slot
counterexample needs. Evidence:
`core/tests/trace.rs::a_candidate_counterexample_refines_its_cone_with_the_events_raisers`
(the refined cone adds the raiser, finds the vouched trace, and lies inside the exact one;
the test asserts neither the number of rounds nor that the cone is strictly smaller),
`::a_recheck_that_counts_every_slot_finds_only_runs_within_the_bound`, and
`core/tests/cone.rs::a_cone_that_leaves_a_mark_out_keeps_the_whole_systems_verdict_and_view`.

**Entity-cone findings are read on cones exact for marks.** For the existential and branching
findings of an entity cone, a left-out mark would produce a wrong `ok`.
`targets.rs::entity_findings` and `findings.rs::state_model_findings` decide `NoWitness`,
`Deadlock`, `NeverRuns`, arms never taken, and values never chosen on the entity's cone, as
formal-verification.md, "State models with TLC alone", says, and neither reads a trace's
`free_triggers`. A left-out mark lets `C` run a handler that `T` holds back, and that adds
edges and states. An added edge can leave a state that is a deadlock of `T`. An added state can
witness a lifecycle state that `T` never reaches, or run a use case that never runs in `T`. In
each case the finding is missing, and a missing finding has no trace for a per-trace check to
read. `NeverRuns`, arms, and values are combined over every entity cone that keeps the action
(`targets.rs::RunsSoFar`), and one cone that fires the action clears the finding, so a single
cone with a left-out mark would hide the finding for the whole run.

So the cone is made exact for marks instead of checking traces:
`cone.rs::TransitionSystem::target_cone` returns `exact_target_cone(target, false)` for an
entity whose own cone would leave a mark out, so every cone that feeds the state-model findings
and `RunsSoFar` has no left-out mark, and Theorem A1 or A8 applies to it given its freed
slots. `core/tests/cone.rs::assert_cones_exact` asserts that no entity cone that frees no slots
leaves a mark out, on every sample it runs; `target_cone` makes every entity cone exact for
marks, freed slots or not. The model below, a handler the cone takes while its raiser
starts from a state never reached, is the engine-corpus case
`cli/tests/engine_corpus/cone_entity_mark_left_out.rdra`, whose `expect` lines hold both
engines to Order's deadlock at `placed` and the missing witness of `shipped`.
`core/tests/cone.rs::a_cone_that_leaves_a_mark_out_keeps_the_whole_systems_verdict_and_view`
is a smaller model of the same shape (Doc, Note, `Review` raising the event `Finalize`
handles, and `Close` reading `Doc.status == final`): it asserts that Note's own cone leaves
the mark out, that `target_cone` gives Note a cone that does not, and runs
`assert_cones_exact` on the model. The corpus model: Order's exit `ShipOrder`
reads `Payment.status == paid`. `ConfirmPayment`, triggered by `EvPaid`, writes `Payment.status`.
The raiser `ReadNotice` writes only `Notice.status` and the `EvPaid` mark, and it leaves
`received`, a value no use case reaches. Order's cone takes `ConfirmPayment`, because it writes the Payment status that
`ShipOrder` reads. `ConfirmPayment` is not in `K`, since it writes no Order parameter. `A_W`
does not hold `ReadNotice`, since it writes nothing in `W`. So the mark `EvPaid` is not exact
by (C2) and is left out.
On that cone, Payment would reach `paid`, and Order's deadlock and the `NoWitness` of
`shipped` would disappear. Payment's own cone may still report its `NoWitness`, so the run as a
whole could stay red, but the verdict on Order would be wrong. `target_cone` gives Order its
exact cone, which keeps every action that writes into it and so `ReadNotice`, the writer of the
mark `ConfirmPayment` reads.

A freed entity gives a weaker version of the problem, which remains: by Proposition A28 the
cone's findings are those of a larger bound, so the result depends on the bound, not on the
event order.

**Proposition A3 (counters are never left out).** If `W` holds the counter `κ` of a counted
edge `triggers(Ev, UC).counted(k)`, then every raiser and the handler of that edge are in
`A_W`, and every mark guard on `κ` of an action in `A_W` is exact.

*Proof.* The raisers and the handler write `κ` (D11: a raise moves `i` to `i+1`, the handler
moves `i` to `i-1`), so they are in `A_W` once `κ ∈ W`. A mark guard on `κ` is exact by the
`is_counted` disjunct of (C2). ∎

The claim's second half says that a cone that holds a raiser holds the counter. This needs a
raise to read `κ`. `build/events.rs::count_up` builds the raise as an `Effect::Transition { var:
κ, … }` on the last action of the raiser. Its arms `0..=k` map `i` to `i+1`, and no arm leaves
`over`. `count_down` builds the handler's decrement as a `Transition` on `κ` on its first action
in the same way. A `Transition`'s source is in `plain_reads` (Notation: "every transition
source"), so `κ ∈ Rd(a)` for the raising action and for the handler. Once either action is in
`A_W`, (C1) brings `κ` into `W`. `count_up` returns early when the raiser's last action has no
parameter on the entity, but that case does not arise. `build/events.rs::edge` takes the
entities whose marks the handler reads (`reads`) from `common`, the entities that every
raiser's last action binds. `countable` rejects a raiser that binds two rows of such an entity.
So `count_up` always adds the effect. `count_down` also adds a `Transition` on `κ` for every
`GuardWhy::Raised` guard that `read_mark` puts on `κ`. So an action that reads the counter's
mark has `κ ∈ Rd(a)`, and no counter can be left out, even without the `is_counted` disjunct
of (C2). Both halves are proved.

*Evidence.* P-full (by reading `count_up`, `count_down`, and `plain_reads`); D: `core/tests/reduce.rs::a_counted_mark_keeps_its_raiser_and_handler_out_of_the_sub_lifecycle`, which checks the reduction's split of an entity's cone (the counter, its raiser and its handler stay in `T`), not the cone: that a cone holding `κ` holds every raiser and the handler, and that a mark guard on `κ` is exact, rest on the reading above.

**Proposition A27 (covered cones).** `ok (checked in Cone_X)` (D23) is printed for a safety cone
`C_s = T|W_s` when `cli/src/verify/tlc.rs::Covered::covers` holds against a cone `C_ℓ = T|W_ℓ` that
passed: the checks of `C_s` are among those of `C_ℓ`, `W_s ⊆ W_ℓ`, `K_s ⊆ K_ℓ`, and every action
of `C_s` keeps, in `C_ℓ`, as many guards and parameters as in `C_s` (the `steps` field of `ConeSystem`). Then
`T ⊨ R` for every check `R` of `C_s`.

*Proof.* `Covered::covers` requires every check of `C_s` to be among the checks of `C_ℓ`'s module,
and `C_ℓ` passed. So `C_ℓ ⊨ R` for each such `R`, and `T ⊨ R` follows from Theorem A2 applied to
`C_ℓ`. For an `after` check, the inclusions `K_s ⊆ K_ℓ` and `W_s ⊆ W_ℓ` keep its checked steps
in `K_ℓ` and their writes in `W_ℓ`, as A2 requires. `cli/src/verify/jobs.rs::cone_jobs` merges the
kept sets of the targets that share `W_ℓ` into one `K_ℓ`. Lemma 2, which A2 rests on, uses only
(C1) and (C4) for the actions of `A_{W_ℓ}(K_ℓ)`. `A_W(K ∪ K') = A_W(K) ∪ A_W(K')`, so these hold
for the merged set. ∎

The step-shape condition is not needed for the verdict. It is what makes `covers`' own
argument hold, `Reach(C_s) ⊆ π_{W_s}(Reach(C_ℓ))`. The `steps` field of `ConeSystem` maps each action of
`restrict_to(W, K)` to the pair `(guards.len(), params.len())`. The guards and parameters
`restrict_to` keeps are monotone in `W` (What `restrict_to` keeps), so `W_s ⊆ W_ℓ` makes the
sets `C_s` keeps subsets of those `C_ℓ` keeps. Equal counts therefore mean equal sets. No guard
that `C_ℓ` keeps for such an action is a copy guard with a target outside `W_s`, so the
dropped-copy case does not arise.

*Evidence.* P-full relative to the covering cone's `ok`, because the proof reduces to Theorem
A2. `cone_jobs` reduces `C_ℓ` (`reduced_cone_in`), and when it is reduced its `ok` rests on
Theorem A8 (Lemma 6 argued; (B1)–(B3) certified at run time), as every reduced cone's verdict
does. No test names A27.

---

## Property cones and fairness

<!-- derived-from ../design/cone-soundness.md#a-propertys-cone-decides-under-its-own-fairness -->
<!-- derived-from ../formal-verification.md#temporal-properties-on-a-cone -->

Let `P` be a property with fair actions `Φ` (D20) and `W = cone(property_seed(P))`, so `K = Φ`
and `Wr(a) ⊆ W` and the existence of each `New` entity of `a` is in `W` for every `a ∈ Φ`.

**Lemma 4 (fair instances are decided on the cone).** For every `a ∈ Φ`, every instance `(a, κ)`,
and every `s ∈ S`: `(a, κ)` is enabled at `s` in `T` iff `(a|W, κ)` is enabled at `π_W(s)` in
`C`; and a step of `a` changes `s` iff it changes `π_W(s)`.

*Proof.* `a` has no dropped parameter (its `New` entities are seeded, its marks are exact because
`a ∈ K`, its other parameters' existences are read) and no dropped guard (a copy target is in
`Wr(a) ⊆ W`). So Lemma 3's argument applies to `a` alone, from any `s`, whether or not `W` is
exact elsewhere: a successor of `π_W(s)` under `a|W` lifts to one of `s` under `a`, and Lemma 2
gives the converse. `Wr(a) ⊆ W` gives the second part. ∎

**Theorem A4 (property cones).**

1. (`ok`, no hypothesis beyond the seed.) If `C ⊨ P` under `Plain` then `T ⊨ P` under `Plain`,
   hence under `WfNext` (D21).
2. (Counterexamples, `W` exact.) If `C ⊭ P` under `Plain`, then `T ⊭ P` under `Plain`.

*Proof.* (1) Let `σ` be a behavior of `T` satisfying `Fair_P` that violates `P` at binding `b`.
By Lemma 2, `π_W(σ)` is a behavior of `C` (non-`A_W` steps become stutters). It satisfies
`Fair_P`: if `(a, κ)` is enabled at every state of `π_W(σ)` from some point, it is enabled at every
state of `σ` from there by Lemma 4, so `σ` takes it infinitely often, and each such step is a
step of `a|W` that changes the projection (Lemma 4). The trigger, cause, and goal read
`property_reads(P) ⊆ W`, so `π_W(σ)` violates `P` at `b`.

(2) Lift a violating `Plain` behavior of `C` step by step with Lemma 3; the lift keeps every action
label, so a fair step stays the fair step. A behavior that stops at `r` lifts to one that stops
at the lifted `s`; under `Plain` a stop is allowed iff no fair instance is enabled, which Lemma 4
transfers. For `always`, a violating finite prefix suffices: a finite system with finitely many
weakly fair action instances has, from every state, an infinite behavior that satisfies all of
them and `WF_vars(Next)` (schedule the enabled instances round-robin; machine closure of weak
fairness, Abadi and Lamport 1991), so the prefix extends to a behavior satisfying both. ∎

The second part needs exactness only for the non-fair actions of `A_W`: Lemma 4 makes the fair ones
exact by construction.

**Theorem A5 (the runs of one property).** For `eventually` and `leads_to`, with `whole` meaning
`cone_keeps_every_write(W)` (every `Wr(a) ⊆ W`):

1. `Cone_P` (`Plain`) `ok` ⇒ `T ⊨ P` under `WfNext`. The run reads the *reduced* cone `R`, not
   `C`. `cli/src/property_cone.rs::PropertyCone::bundle` prints `ConeRun::Plain` from `self.reduced`.
   `cli/src/verify/native.rs::native_property` runs `p.cone.reduced`, and `Plain::of` asserts
   (`shadow_assert!`) that its run is of `cone.reduced`. So the implication is Theorem A8.4
   (`R ⊨ P ⇔ C ⊨ P` under `Plain`), then Theorem A4.1, then D21. A8 is a sketch: it assumes
   (B1)–(B3), which every build certifies on the partition it uses, and Lemma 6, which is
   argued. When nothing reduces, `reduced` is `C` itself and A4.1 alone suffices.
2. `Cone_P_WF` (`WfNext` on `C`) counterexample, `W` exact ⇒ `T ⊭ P` under `WfNext`.
3. `Cone_P_WF` `ok` and `whole` ⇒ `T ⊨ P` under `WfNext`. On a reduced cone, the first
   `WfNext` run reads `R` (`decide_liveness_reduced`), and its `ok` is `C`'s by Theorem A15,
   which assumes A8's hypotheses. Otherwise the run reads `C`
   (`cli/src/verify/native.rs::native_property_after`).

*Proof.* (2) Lift the prefix with Lemma 3. If the lasso's cycle changes `C`'s state infinitely often,
its lift changes `T`'s, so `WF_vars(Next)` holds on `T`, and `Fair_P` transfers as in A4.2. If the
behavior stops at `r`, no step of `C` leaves `r`. From the lifted `s`, extend by any maximal path
of `T` steps that leave `π_W` at `r`: a step from a state `s''` with `π_W(s'') = r` that changed
`π_W` would project (Lemma 2) to a step of `C` leaving `r`. The path either ends at a state with no
successor other than itself or, `T` being finite, can be continued forever through a cycle; in
both cases `WF_vars(Next)` holds. Every fair instance is disabled at `r`, so at every state of the
extension (Lemma 4), and the goal reads `W` and stays false.
(3) `whole` implies that every `a ∈ A` writes into `W`, so `A_W = A` and every mark is exact; and
every created entity's existence is in `Wr ⊆ W`. So `W` is exact and every step of `T` that changes
the state changes `π_W`. Then `WF_vars(Next)` of `T` projects to `WF_vars(Next)` of `C`, and
the proof of A4.1 goes through under `WfNext`. ∎

`cli/src/property_cone.rs::after_plain` and `::after_wf` implement exactly these implications, with
"`W` exact" read as `cli/src/property_cone.rs::inexact` being `None` (neither `cone_frees_slots` nor
`cone_leaves_marks`); the `debug_assert!(!whole || inexact.is_none())` in `PropertyCone::under`
is the second sentence of (3).
When neither run decides, `RdraSpec_P` runs on `T`, or the verdict is `not decided`.

**Proposition A6.** On a cone with a freed entity or a left-out mark, `cli/src/property_cone.rs::counterexample`
maps every counterexample to `Undecided(why)`, with `why` the `Why::FreesSlots` or `Why::LeavesMarks`
that `inexact` gives, and `explore/properties.rs::decide_property` runs the explored forms (`stable`,
`unchanged`, `precedes`, and `always`) again on `T` after a violation. Its `always` arm runs only from
tests: its one caller in the CLI, `cli/src/verify/bfs.rs::explored_properties`, passes only the forms
`LtlKind::is_explored` names, and `verify` decides an `always` on its reduced property cone through
`cli/src/verify/native.rs::Answer::of`, where an inexact cone's counterexample is `Undecided(why)` and
`RdraSpec_P` runs on `T` (A5). Only `ok` is taken from such a cone. The argument depends on the form:

- `stable`, `unchanged`, and `precedes` read the unreduced cone, so their `ok` is sound by
  Theorems A4.1 and A2. Both engines run them there. `cli/src/verify/jobs.rs` builds their explored
  jobs on `restrict_to(cone)` (`LtlKind::is_explored`). `cli/src/verify/bfs.rs::explored_properties`
  calls `decide_property`, which also uses `restrict_to` for every form except `always`.
- `always` reads the reduced cone (`decide_property` calls `reduced_cone` for `Ltl::Always`). So
  do `eventually`, `leads_to`, and `always` in `verify`, through the `Plain` run of A5.1. For
  these forms the `ok` is sound by Theorem A8 (points 1 and 4), then A4.1 and A2. A8 assumes
  (B1)–(B3), certified at run time, and Lemma 6, which is argued.

*Evidence for A4 to A6.* Theorem A4, a statement about the unreduced cone `C`: P-full, given
Lemmas 2 to 4. A5.1, A6, and A5.3 on a reduced cone: P-sketch, because the printed verdict is
read on the reduced cone and rests on Theorems A8 and A15, whose Lemma 6 is argued, not proved.
A5.2: P-full. D:
`cli/tests/engine_equivalence.rs::a_property_cone_decides_what_the_whole_model_decides` (release jobs); corpus rules 1
to 3 (`cli/tests/engine_corpus.rs`); `core/tests/explored.rs::a_cone_that_frees_slots_decides_a_violation_on_the_whole_model`;
the unit tests in `cli/src/property_cone.rs`.

---

## `can_reach` cones

<!-- derived-from ../formal-verification.md#recoverability-on-a-cone -->

**Theorem A7.** Let `W` be the cone of `reach_seed(i)`. Then `W` is exact, and for every
`s ∈ Reach(T)` and binding `b`, the formula of D22 holds at `s` in `T` iff it holds at `π_W(s)`
in `C`. A `can_reach` cone is never reduced.

*Proof.* At the fixed point of `keep_writers(seed, true)`, every action writing into `W` is in
`K`, so `A_W = K`; every mark of an action in `K` is exact by (C2); and the seed holds the
existence of every entity an action of `K` creates. So `W` has no left-out mark and no freed
entity. Given `s ∈ Reach(T)`, a path `π_W(s) = t₀ → … → t_r` of `C` with `pres_b` along it and
`g_b(t_r)` lifts by Lemma 3 to a path of `T` from `s` with the same projections, and `pres_b`,
`g_b` read `W`. Conversely a witnessing path of `T` projects (Lemma 2), after its stutter steps are
removed, to one of `C` with the same values of `pres_b` and `g_b` at the retained states, and its
last state is retained. The trigger reads `W`. Theorem A1.1 matches the reachable states. A
reduced cone keeps no `can_reach`: `reduce/mod.rs::TransitionSystem::drop_unreadable_checks`
clears them (its comment: the reduction "says nothing of `AG EF`"). Whether Theorem A8 would
in fact preserve D22's `E[pres U (pres ∧ g)]` is not needed, since no reduced run decides one. ∎

*Evidence.* P-full. D: `core/tests/reach.rs`.

---

## The reduction by an independent sub-lifecycle

<!-- derived-from ../design/cone-soundness.md#the-reduction-by-an-independent-sub-lifecycle -->
<!-- derived-from ../formal-verification.md#reduction-by-an-independent-sub-lifecycle -->
<!-- derived-from ../design/event-order.md -->

Let `C` be a cone system (`TransitionSystem::cone_system`) and `seeds` the seeds and kept sets of
the targets sharing it. `reduce/mod.rs::TransitionSystem::reduced_with` builds the reduction.

**Definition 3 (split).** `reduce/split.rs::TransitionSystem::split_with` chooses, among the
anchors (each variable the seeds read), the split with the fewest `T`-variables. For one anchor,
`split_at` computes the least set `T_v` of variables and `T_a` of actions such that the anchor and
every counter are in `T_v`; an action that is kept, writes `T_v`, or reads a non-monotone variable
of `T_v` is in `T_a` and has `Wr ⊆ T_v`; and a variable and its entity's existence share a side.
Here `Rd` includes the mark reads and the copy reads. Then `S_v = V_C ∖ T_v`, `S_a` the writing
actions outside `T_a`, and:

- `obs = (⋃_{a ∈ T_a} Rd(a) ∪ ⋃ seeds) ∩ S_v`;
- `inputs`: the `T_v` variables an `S_a` action reads raw, with their existences;
- `folds`: the `S_a` actions whose reads of `T` fold into one bit per slot of a key
  (`reduce/fold.rs`, `FoldKey::Param` and `FoldKey::Linked`).

*Monotone* is `reduce/split.rs::TransitionSystem::monotone_vars`: the write edges of the variable, as
pairs of values over all effects of `C`, form no cycle other than self-loops, and the variable is
not a counter and not a column or owner of a deleted entity.

**Lemma 5 (split).** (a) Every `S_a` action writes only `S_v`. (b) Every `T_a` action writes only
`T_v` and reads `S_v` only in `obs`. (c) Every read of an `S_a` action is in `S_v`, `inputs`, or a
fold. (d) Every input is monotone. (e) Every seed's read of `S_v` is in `obs`, and every kept action
is in `T_a`.

*Proof.* (a), (b), (e) are the closure conditions of `split_at` and the definition of `obs`. For
(c), an action reading a non-monotone `T_v` variable joins `T_a`, so an `S_a` action reads in `T_v`
only monotone variables, which become inputs unless a fold takes them. For (d), an input is such a
variable or the existence of one's entity (or, for an owner variable, of its parent), which only a
create (absent to present) and a delete (present to deleted) write. ∎

*Run-time certificate.* Every build checks (a)–(e) and the side rule of Definition 3 on the
split before the reduction uses it (`reduce/lemma5.rs::split_faults`, called by `reduced_with`).
The check reads the split's sets and the seeds, and computes from each action its `Rd` (mark reads
and copy reads included) and `Wr`, and monotonicity with `monotone_vars`; it takes nothing from
`split_at`. It also checks that the sets are a split: `T_v` and `S_v` partition `V_C`, each
writing action is in exactly one of `T_a` and `S_a` and no other action is in `S_a`, `inputs ⊆
T_v`, `obs ⊆ S_v`, and each fold is of an `S_a` action. For (c), a variable an `S_a` action reads
outside `S_v` and the inputs is one its fold reads (`Fold::reads`), and each `T_v` variable it reads
outside the fold (`raw_reads`) is an input. On a fault `reduced_with` reduces nothing, so the cone
is decided unreduced, with a warning; a debug or `shadow` build panics. So (a)–(e) hold of every
split a reduction uses, nested ones included, whatever `split_at` computed. What the check does not
show is that `action_reads`, `copy_reads` and `action_writes` are the reads and writes of the step
relation, and that `monotone_vars` is sound (A10).

*Evidence.* M: `proofs/lean/RdraIsh/Split.lean` proves the lemma in Lean over an action-level
model of `C` (`Actions.lean`: variables with their entities' existences, valuations, actions
with `Rd`, `Wr`, guard conjuncts and an effect). Definition 3 for one anchor is the least set
`Mem` closed under its conditions, with `split_at`'s skip of an action that writes nothing and
is not kept. A pass of all conditions at once, repeated `|V| + |A| + 1` times, computes it
(`computed_iff`), and any closed pair whose members the conditions derive, in whatever order,
is that least set (`mem_eq_of_sound`). Lemma 5 (a)–(e) holds of every closed pair, with `obs`
and `inputs` as defined above (`lemma5_of_closed`), so of the split of every anchor and of the
one `split_with` picks. (a), (b) and (e) follow from the closure conditions and the definition
of `obs`. (c) follows from the hypothesis `FoldData.cover`: every `T_v` read of an `S_a` action
is a raw read (`raw_reads`), an existence of one, or a read its fold covers. (d) follows from
the closure, `raw_reads ⊆ Rd`, and the hypothesis that an existence of a monotone variable is
monotone (`FoldData.ex_mono`); `monotone_vars` itself is a parameter, modelled on its own in
`proofs/lean/RdraIsh/Monotone.lean`, where an existence that only creates and deletes write is
monotone (`Monotone.lean::existence_monotone`). With the locality of `Rd`
and `Wr` (`Local`: a guard conjunct depends only on `Rd ∪ Wr`, an effect changes only `Wr`,
and its value there depends only on `Rd ∪ Wr`), (a) and (b) give the hypothesis `Split` of
the Lean proof of Theorem A8 (`split_of_lemma5`, `split_computed`), which needs nothing of
the folds; the cone system over `T_v × S_v` reaches a pair iff the action-level system reaches
its valuation (`cone_reach_iff`). The proof does not read
the code. `Local` is a theorem for the action model read off the slot-level model of Theorem A18
(`proofs/lean/RdraIsh/SymActModel.lean::local_`), whose read and write sets are computed from the
syntax of guards and effects as `action_reads`, `copy_reads` and `effect_writes` compute them,
except that a `Triggered` parameter's existence always counts as read; that the code's actions
are that syntax is read from the code. That `raw_reads` and the folds
meet `FoldData` is a hypothesis.

**Definition 4 (extended sub-lifecycle and quotient).** `X` is
`reduce/extended.rs::Extended::build`: `restrict_to(S_v ∪ inputs)` without the havoc of `Triggered`
parameters, with one bit variable per fold group in place of the folded guards, and a free `Rise` and
`Fall` action per bit. An edge of `X` is *silent* when its action is an `S_a` action and a *rise*
otherwise (a restricted `T_a` action that writes an input, or a flip). The run of `X` (under its
own reduction, nested) gives a state set `X_n`, and `reduce/partition.rs::quotient_blocks` returns a
partition `B` of `X_n` meant to satisfy:

- (B1) two states of a block agree on `obs`, the raw inputs, and the bits;
- (B2) divergence-blind stuttering stability under the silent edges: for blocks `B ≠ B'`, either
  every state of `B` reaches `B'` by a path of silent edges whose states before the last are all in
  `B`, or none does (Groote and Vaandrager 1990);
- (B3) strong stability under rises: two states of a block reach the same blocks by one rise.

`R` is `C` with `S_v` replaced by one representative per block (`Reduction::reps`), `T_a` actions run
unchanged, a `T` step that changes the inputs moving the block by one rise per stage
(`Reduction::stages`, `::recanon`; a missing rise panics in `moves_of_into`), and `S_a` actions
replaced by class moves (`Classes::moves`).

For a state `(t, s)` of `C` write `x(t, s) = (s, inp(t), bits(t))`, the extended state it
corresponds to, where `bits(t)` is `Reduction::push_bits`. `X` may also hold *stubs*: a parent in
`T` of an owner variable in `S_v` whose existence is not in `S_v ∪ inputs` is kept by `restrict_to`
as an entity of its existence alone, which no action of `X` writes. `x(t, s)` gives each stub the
value of `X`'s first initial state (`Reduction::key` appends that tail), so a stub reads nothing of
`t`. A stub's existence is constant in `X`, absent when a use case creates the parent, so a kept
parameter on a stub whose role needs a present row (`Existing`, `Parent`, `Triggered`) would
not bind where the cone binds it. `Extended::build` therefore binds every such parameter to any
slot (`reduce/extended.rs::bind_stub_params`, which gives it the role `Earlier { step: 0 }` with
no run guard). Only two kinds of action of `X` keep such a parameter: a folded `S_a` action,
where it is one of the fold's `T` parameters, and a rise.
Every build checks the built `X` against this definition (`extended.rs::StubCheck`): no
parameter of any action of `X` (folded and unfolded `S_a` actions, rises, and the bits' flips)
on a stub needs a present row, `bind_stub_params` binds no parameter of an unfolded `S_a`
action, nor a folded one's outside its fold, and `restrict_to` keeps every fold's key. On a
fault `reduced_with` reduces nothing, so the cone is decided unreduced, with a warning; a debug
or `shadow` build panics (Lemma 6, "Parameters").

**Lemma 6 (the extended system covers the cone).** For every `(t, s) ∈ Reach(C)`: `x(t, s) ∈ X_n`
(up to the inner reduction's canonical form); `(t, s) → (t, s')` is an `S_a` step of `C` iff
`x(t, s) → x(t, s')` is a silent edge of `X`; and a `T_a` step `(t, s) → (t', s)` that changes the input
bytes is matched by a sequence of rises from `x(t, s)` through the stages to `x(t', s)`.

*Proof sketch.* Induct on reachability. Initial states: `X`'s initial states are those of
`restrict_to`, which projects `I_C` (Lemma 2's argument); a bit starts where the cone starts
(`fold_initial`). An `S_a` action reads only `S_v`, inputs, and folds (Lemma 5c), and its guards on
those are kept by `restrict_to` with havoc off, so it has the same successors on `S_v` in `C` at
`(t, s)` and in `X` at `x(t, s)` provided each folded guard equals its bit at `bits(t)`, which
step (a) below derives, and provided the two systems bind the parameters alike, which the
matrix below settles. A `T_a` step that changes the input bytes has its rises by step (c).

**Parameters.** A step of `C` binds each parameter to a candidate slot of its role
(`explore/moves.rs::candidate_slots`: a present slot for `Existing`, `Parent` and `Triggered`,
any slot for `Earlier`, the lowest absent slot for `New`), and the guards filter the bindings.
`C` at `(t, s)` and `X` at `x(t, s)` must admit bindings with the same successors. The cases
are the product of three things:

- the *kind* of the action: a *folded* `S_a` action (one of `Split::folds`), an *unfolded*
  `S_a` action, a *rise* (a `T_a` action that writes a raw input, which `restrict_to` keeps),
  a *bit writer* (a `T_a` action that writes no raw input but a variable a bit reads, which
  `X` does not hold), and a *steady* `T_a` action (`Reduction::is_steady`: one that writes
  neither, which `X` does not hold either);
- the *side* of the parameter's entity: `S` (its existence is in `S_v`, and with it every
  variable of the entity, Definition 3); *input* (its existence is in `inputs`, so `X` holds
  the entity with its variables in `S_v ∪ inputs`); *stub* (Definition 4); *dropped* (a `T`
  entity that is neither, of which `X` holds no variable);
- the parameter's *role* in `C` (`types.rs::ParamRole`: `New`, `Existing`, `Parent`, `Earlier`,
  `Triggered`), with, for a fold, whether it is the key; a node fold adds a parameter of its
  own, the *node row*.

Four facts settle most cells.

- **(P1) A row of `S` binds alike.** `X` keeps an `S` entity with its slots and variables, and
  the parameter with its role. `x(t, s)` and `(t, s)` agree on `S_v ∪ inputs`, so the
  parameter has the same candidate slots in both, and a guard kept in `X` reads the same values.
- **(P2) A row of an input binds alike, under fewer guards.** `X` keeps the entity and its
  existence, and no function changes the role but `fold_param`, for a fold's key. `x(t, s)`
  holds `inp(t)`, so the candidate slots are the same. `restrict_to` keeps a guard only when it
  reads `S_v ∪ inputs` and names no dropped parameter, so `X` drops guards and adds none: every
  binding of `C` satisfies the guards `X` keeps.
- **(P3) A row of a stub binds to any slot.** Only the stub's existence is in `X`, and it is no
  variable of `S_v ∪ inputs`, so `restrict_to` drops every guard that reads a variable of the
  stub. `bind_stub_params` gives a parameter on a stub whose role needs a present row the role
  `Earlier { step: 0 }`, which binds any slot with no guard. An `Earlier` parameter binds any
  slot already, and its run guard reads the stub's mark and goes. A `New` one is dropped, as
  on a dropped entity (P4), since its entity's existence is not in `S_v ∪ inputs`.
- **(P4) A row of a dropped entity goes.** `restrict_to_mapped` drops every parameter
  `restrict.rs::keeps_param` rejects (one on an entity it neither keeps nor stubs, and a `New`
  one whose entity's existence it does not hold), and every effect that writes no variable of
  `S_v ∪ inputs`. With `havoc` off, as `Extended::build` calls it, every dropped parameter is
  in `havoced`, a `New` one included, so every guard that names it goes too, also one that
  names a created row by `Term::Index` alone (corpus `cone_rise_names_a_dropped_new_row`); an
  `after` step that names a dropped parameter goes with it. Only a rise has a dropped `New`
  parameter, since an `S_a` action creates rows of `S_v` alone.

(P2) to (P4) only drop constraints, so `X` admits every binding of `C` and perhaps more. A cell
must show that what was dropped does not change the successor, and for a folded action that the
bit says what the dropped guards said. A cell that cannot occur gives the code's reason. The
last column names the test that runs the step checks of the evidence below on a model that
fills the cell (`core/tests/reduce.rs`, with the use case that fills it); each such test
asserts that its models fill its cells and none that cannot occur (`Reduction::param_cells`).

*Folded `S_a` action.* In `fold_of`, a guard that names a `T` parameter and reads only `T` is a
folded guard. Any other guard that names one must be the link `v[k] = Index(p)`, with `v` a
variable of `S_v`, and `p` is then the key. No effect names a `T` parameter but the key. So a
binding of the `S` parameters, the key at slot `c`, and any slots of the other `T` parameters
has a successor in `C` iff the guards outside the fold hold and the other `T` parameters can be
bound on candidate slots so that every folded guard holds; by step (a) that is the bit at `c`.

| Side | Role | What the code does | Why no successor is lost | Test |
|---|---|---|---|---|
| `S` | any, key or not | kept with its role | (P1) | `New`, `Existing`, `Existing` key: `::the_reduction_models_bind_where_their_cones_bind`; `Parent`: `::stub_and_dropped_rows_bind_where_the_cone_binds_them` (`Grant`); `Triggered`: `::input_rows_bind_where_the_cone_binds_them` (`StampIssued`); `Earlier`: none |
| input | key: `Existing`, `Parent`, `Triggered` | `fold_param` gives it the role `Earlier { step: 0 }` and adds the guard `bit[key] = true`; the link guard reads `S_v` and stays | `X` binds the key to any slot its link guard allows and the bit holds at, `C` to a candidate slot its link guard allows and the other `T` parameters complete; by step (a) these are the same slots, and the successor depends on the key's slot and the `S` rows alone | `::input_rows_bind_where_the_cone_binds_them`: `Existing` (`StampNoted`), `Parent` (`Grant`, under shops), `Triggered` (`Stamp`) |
| input | not the key: `Existing`, `Parent`, `Triggered` | kept with its role; a folded guard whose reads are all inputs stays beside the bit (P2) | A binding of `C` satisfies every folded guard, so it satisfies the kept ones on the same input values and makes the bit true. A binding of `X` has the bit true, so by step (a) some binding of `C` with the same key satisfies every folded guard, and no effect names the parameter, so the successors agree | `Triggered`: `::the_reduction_models_bind_where_their_cones_bind` (`CLAIM_CASE`); `Existing`, `Parent`: `::input_rows_bind_where_the_cone_binds_them` (`GrantNoted`, `Grant`, with the desks read raw) |
| stub | key: `Existing`, `Parent`, `Triggered` | as for an input key; `bind_stub_params` leaves it, since its role no longer needs a present row | as for an input key: the key's presence in `C` is a condition of the bit (`Fold::t_params` marks it), which reads it at `t`, not at the stub | `Parent`, `Existing`: `::stub_and_dropped_rows_bind_where_the_cone_binds_them` (`Grant`, `UseNoted`); `Triggered`: `::the_reduction_models_bind_where_their_cones_bind` (`ORDER_SHIP`) |
| stub | not the key: `Existing`, `Parent`, `Triggered` | bound to any slot (P3) | the bit stands for the parameter's presence and its guards, and no effect names it | `Triggered`: `::the_reduction_models_bind_where_their_cones_bind` (`TRIGGERED_ON_A_STUB`); `Existing`, `Parent`: none |
| dropped | not the key: `Existing`, `Parent`, `Triggered` | dropped with its guards, all folded (P4) | as for a stub | `::stub_and_dropped_rows_bind_where_the_cone_binds_them`: `Existing`, `Parent` (`GrantNoted`, `Grant` with the voucher unread), `Triggered` (`UseRung`) |
| dropped | key | cannot occur in a reduction that is used | a linked or assigned key is compared with, or assigned to, a variable of an `S` row whose domain names its entity, and `restrict_to` keeps the parent of an owner variable of `S_v ∪ inputs` as an input or a stub (only owner variables have a `Domain::Instance`, `build/vars.rs`). The acting-row key of `fold_of` (the first written parameter) has no such link, and can be a `T` row no variable of `S_v` or the inputs names. `split_with` leaves such an action unfolded (`Fold::key_kept`, through `restrict.rs::keeps_param`) and recomputes the inputs, so the action reads `T` raw: the key's existence becomes an input and the parameter is an unfolded action's `Existing` row on an input, a row of the unfolded table. Should a fold's key still be dropped, `fold_param` and `fold_linked` leave `X` as it is and the build records `Fault::KeyDropped`, so the cone is decided unreduced (Definition 4) | corpus `cone_fold_key_dropped`; `core/tests/reduce.rs::a_fold_whose_key_the_restriction_drops_leaves_its_action_unfolded` |
| dropped, `S` | node row | cannot occur | the node quantifies over a `T` entity (`as_node`), and its link compares a variable of `S_v` with the bound row (`strip_links`), an owner variable whose parent `restrict_to` keeps as an input or a stub, as for a linked key; `fold_linked` maps the entity with `Remap::e`, which expects it kept | — |
| stub, input | node row | `fold_linked` adds a parameter on the node's entity with the role `Earlier { step: 0 }`, and puts `outside ∧ link = q ∧ bit[q]` where the node stood | the link fixes `q` at the row the `S` row names, and by step (a) the bit there is the node's body there | stub: `::stub_and_dropped_rows_bind_where_the_cone_binds_them` (`UseOpen`); input: `::input_rows_bind_where_the_cone_binds_them` (`Check`) |
| input, stub, dropped | `New`, `Earlier` | cannot occur | a `New` row comes with its `Create` (`build/actions.rs`), and an `Earlier` row with the assignment of its run mark (`run_marks`), which `run_mark_vars` gives every entity of a step before the last; either writes `T`, and `split_at` puts an action that writes `T` in `T_a` | — |

*Unfolded `S_a` action.*

| Side | Role | What the code does | Why no successor is lost | Test |
|---|---|---|---|---|
| `S` | any | kept with its role | (P1) | `::the_reduction_models_bind_where_their_cones_bind`, all five roles |
| input | `Existing`, `Parent`, `Triggered` | kept with its role; its guards are raw reads, so in `S_v ∪ inputs`, and kept | (P2), with no guard dropped | `Existing`, `Parent`: `::input_rows_bind_where_the_cone_binds_them` (`GrantNoted`, `Grant`); `Triggered`: `::stub_and_dropped_rows_bind_where_the_cone_binds_them` (`RingNote`, in another target's cone) |
| input | `New`, `Earlier` | cannot occur | as for a folded action | — |
| stub, dropped | any | cannot occur | `raw_reads` holds the existence of every `Existing` and `Parent` row and the reads of every guard; a `Triggered` row's `GuardWhy::Raised` guard reads its mark, whose `existences` hold the entity's existence; `split_with` makes them inputs. Every build checks it for a stub (`extended.rs::StubCheck`) | — |

*Rise.* `X` evaluates the rise at `x(t, s)`, which holds `s`; step (c) shows what it writes.

| Side | Role | What the code does | Why no successor is lost | Test |
|---|---|---|---|---|
| `S` | `Existing`, `Parent`, `Triggered` | kept with its role | (P1) | `Parent`, `Triggered`: `::the_reduction_models_bind_where_their_cones_bind`; `Existing`: `::input_rows_bind_where_the_cone_binds_them` (`GrantNoted`, in another target's cone) |
| `S` | `New`, `Earlier` | cannot occur | the `Create` of a `New` row and the run mark of an `Earlier` row write `S`, and a `T_a` action writes only `T_v` (Lemma 5b) | — |
| input | any | kept with its role | (P2); step (c) | `Existing`: `::the_reduction_models_bind_where_their_cones_bind` (`CLAIM_CASE`); `New`, `Parent`, `Triggered`, `Earlier`: `::input_rows_bind_where_the_cone_binds_them` (`OpenDesk`, `SheetNote`, `ClosedNote`, `CloseNote`) |
| stub | any | bound to any slot, or dropped if `New` (P3) | any slot includes the one `C` binds; step (c) | `::stub_and_dropped_rows_bind_where_the_cone_binds_them`: `Existing`, `Parent`, `Triggered`, `Earlier`, `New` (`SheetNote`, `RingNote`, `PaidNote`, `MarkNote`, `OpenNote`) |
| dropped | any | dropped with its guards (P4) | step (c) | `::stub_and_dropped_rows_bind_where_the_cone_binds_them`: all five (`ShopNote`, `ShopOpenNote`, `MemoNote`, `RingNote`, `CloseNote`) |

*Steady `T_a` action and bit writer*, any side and role: `X` holds neither. `R` runs the action
on `(t, ρ)` with `C`'s binding rule. It reads `S_v` only in `obs` (Lemma 5b; `Rd` holds the
existence a present slot's selection reads), so its bindings and successors at `(t, s)` and
`(t, ρ)` agree by (B1). A steady step leaves the input bytes as they are (Theorem A8, point 2).
A bit writer's step that changes the bits has only the flips of step (c) to match, which bind
no parameter of the action (Theorem A8, point 5). Test: the `assert_reduction_exact` tests,
which compare the states on `T_v ∪ obs`; `lemma6_steps_hold` records a steady action's cells
from its steps, and a bit writer's from the steps whose flips it matched.

The cells marked none are reachable as far as the code shows, and no test model fills them; each
uses the argument of its row, which the filled cells of the same row check.

**(a) `Fold::holds` computes the folded guard.** Let `f` be a fold of action `a`. For a key
`FoldKey::Param(k)` and a state `t`, `holds(f, t, c)` holds iff some binding of `a`'s `T`
parameters, with `k` at `c` when `k` is one of them, puts each on a candidate slot of its role
at `t` and makes every folded guard true. Four things make it so.

1. `holds` sets the key to `c` and ranges each other `T` parameter over every slot of its
   entity (`holds_fast`; the reference in `reduce/fold/reference.rs` computes the same).
2. It then requires a present row for each `T` parameter that `fold_of` marks with
   `matches!(role, Existing | Parent | Triggered)`, the key included. That is `candidate_slots`
   for those roles. A `T` parameter of an `S_a` action is never `New` or `Earlier` (the folded
   table's last row), so no other candidate rule arises.
3. It evaluates the folded guards with every parameter outside the `T` ones and the key at
   slot 0. The folded guards name no such parameter: `fold_of` collects the `S` parameters they
   name into `s_keys`, gives up when there are two or when a linked or assigned `T` parameter
   would be the key as well, and otherwise makes the one there is the key. The folded guards read only `T`
   variables and no successor (`fold_of` gives up on one that reads the successor or is a copy),
   so they read `t` alone.
4. A group's bit is the disjunction of its folds' `holds` (`Reduction::push_bits`). The folds of
   a group share their restricted parameters, guards, effects and key (`Signature::Param`), so a
   step of any of them at `c` is a step of the group's first action in `X`, which
   `SilentStep::acts` and `lift` read back as the variant it stands for.

For a node fold (`FoldKey::Linked`), `holds` is the row's presence and the body at it. `C` reads
the node as `∃ e. link = e ∧ body(e)`, and `Pred::Exists` ranges over present rows only
(`expr.rs::quantify`). The link comparison reads the owner's domain, so an unlinked optional
owner names no slot on either side (`expr.rs::compare`). `X` reads `link = q ∧ bit[q]`, which
holds for the one `q` the link names when that row is present and the body holds there.

**(b) A dropped `T` parameter bound to slot 0 loses no successor.** `Extended::cone_params`
renumbers a class move's parameters into the cone's, with slot 0 for a parameter the
restriction dropped. It is called in `Quotient::of` alone, on the class moves of `R`. A class
move's successor is the state with its `S` part replaced by the target class's representative
(`moves_of_into` writes `reps[to]`), whatever the parameters. `Quotient::of` keeps one move per
`(class, action, parameters, target)`, so two moves of `X` that differ only in a dropped
parameter's slot leave the same target. The TLA+ printer reads only the targets of each `S`
action's class moves (`emit/src/tla/module.rs`). The lift does not read them either: it takes
the parameters of the path in `X`, and for a folded or merged action, or one whose parameters
the restriction renumbered (`Reduction::in_cone_params` is false), it finds the cone's move to
the same state among `plain_moves`, checks it with `is_move` (A16), and prints that one. An
unfolded `S_a` action drops no parameter (its table above), so its parameters are the cone's.
The one reader of a class move's parameters by the cone action's numbering, the reason
`cone_params` renumbers them, is the BFS's view bookkeeping, `explore/bfs.rs::Bfs::observe`:
for each written row of the action outside `S` it compares the row's pattern before and after
the step (`a_class_move_of_a_folded_action_binds_the_cones_params` is its test). A class move
writes no `T` variable, so at whatever slot a dropped parameter points, that row is present on
both sides with the same columns or absent on both, and `observe` records nothing for it (a
`shadow` build asserts the first case, `Twin::UnchangedRow`). In `R` slot 0 is a label on an
edge whose successor is right.

**(c) A rise writes what the `T` step writes.** Let `(t, s) → (t', s)` be a step of `C` by a
`T_a` action `a` at a binding `b` that changes the input bytes. Some effect of `a` writes a raw
input when the raw inputs change, so `restrict_to` keeps `a`. Bind its restriction in `X` at
`x(t, s)` as `b` binds the kept parameters, as the rise table allows. The guards `X` keeps hold
there by (P1) and (P2), and they are a subset of `C`'s. The effects `X` keeps are those that
write `S_v ∪ inputs`, which for a `T_a` action are the inputs (Lemma 5b). Each but a `Tick`
writes a variable `v` of the row of its parameter `p`. The existence of `v`'s entity is an input (`split_with`
adds the existences of every input), so `p` is a row of an input and keeps its slot `b(p)`. The
value written is:

- a literal (`Rhs::Lit`), or a slot (`Rhs::Param(q)`). Only an owner variable takes a slot (the
  builder's `Rhs::Param` sites, in `build/links.rs`, all assign owners), and its parent's existence is an input by `existences`, so `q` is a row of an input and keeps `b(q)`;
- for a `Transition`, an arm's target, chosen by `v`'s own value, which `x(t, s)` holds;
- for a `Choose`, one of its listed values;
- for a `Create`, presence at `b(p)`, the lowest absent slot of an input existence in both
  systems; for a `Delete`, deletion at `b(p)`, with the row's columns reset, none of which is an
  input, since `monotone_vars` makes a deleted entity's columns cyclic;
- for a `Tick`, `now + 1` up to `int_max`, read from `now` itself: when `now` is an input
  (`monotone_vars` counts the clock as rising), `restrict_to` keeps it as `X`'s clock, and
  `x(t, s)` holds `t`'s value.

`GuardWhy::ArmCondition` and `GuardWhy::Copy` guards filter the successors, and `X` keeps those
that read `S_v ∪ inputs` only, so it keeps `C`'s successor on the inputs among its own. So `X`
has an edge from `x(t, s)` to the state with `s`, the raw inputs of `t'`, and the bits of `t`:
the first stage of `Reduction::stages`. Each later stage flips one bit slot by the free `Rise`
or `Fall` of its group (`reduce/extended.rs::flips`, a parameter that binds any slot and a guard on
the bit alone). A step that changes only bits, with the raw inputs unchanged, has only these
stages. No stage changes `s`, since a `T_a` action writes no `S_v`.

The step named and not derived here is the nested case: for a nested reduction `x(t, s)` is in
`X_n` only up to the inner reduction's canonical form, and the induction over the depth is not
written. ∎

*Evidence.* The `S_a` half is checked against the cone's own step
relation, in both directions, on every reachable state of every complete reduced cone that folds
(`core/tests/reduce.rs`). `::reduced_domain_cones_hold_each_fold_bit_where_its_action_runs`: the
bit holds at the slot every move of a folded action binds its key to (for a node fold, the row its
link names), and every move of an `S_a` action, folded or not, is a silent step of `X` from
`x(t, s)` to the same state by an action the step stands for, so `X` drops no step. `::reduced_domain_cones_step_by_a_true_fold_bit_only_where_the_cone_steps`:
from the state's projection `x(t, s)` (`Reduction::key`), every silent step of `X` is a move of `C`
to the same state, by an action the step stands for, with the params it binds outside the folded
`T` params, so a true bit adds no step. Both run on the domain samples' 71 folds at 1 and 2
instances, about 3,100 moves each, none of which binds a key past slot 0;
`::case_fold_bits_hold_exactly_where_their_actions_run_past_slot_0` runs both on the case fold at 2
and 3 instances, where about 1,700 of 2,800 moves bind slot 1 or 2. A bit planted false at slot 1
fails the first, and one planted true at a present row 1 fails the second.
`::a_triggered_param_on_a_stub_binds_in_the_extended_sub_lifecycle` (main only) runs both halves on a fold
with a `Triggered` parameter beside its key on a stub, at 1 and 2 instances, and checks that the
reduced cone finds the cone's violation, decides what the cone decides
(`assert_reduction_exact`), and that every reduced state's path lifts to a real path of the
cone. `::a_rise_with_a_param_on_a_stub_is_a_rise_of_the_extended_sub_lifecycle`, for a rise
with a `Parent` parameter on a stub, at 1 and 2 instances, checks through
`assert_reduction_exact` that the reduced cone decides what the cone decides (the states on
`T_v ∪ obs`, the views and the verdicts) and that its traces lift.

The matrix of "Parameters" has its own three tests, which run the helper `lemma6_steps_hold`
on every complete reduced cone of their models, folded or not. It runs both halves above, and
the rise half: from every reachable state, every `T` step that changes the input bytes is a
rise of `X` by the same action to the successor's raw inputs, then one flip per bit slot that
changes (`SilentSteps::rises_through`); the same step with an `S` variable of the successor
moved must be no rise, which shows the check reads the state it is given. `::stub_and_dropped_rows_bind_where_the_cone_binds_them`
and `::input_rows_bind_where_the_cone_binds_them` run it at 1 instance on models built to fill
the cells with a row on a stub, a dropped entity, or an input.
`::the_reduction_models_bind_where_their_cones_bind` runs it at 1 and 2 instances on the models
of the tests above, `RISE_ON_A_STUB` among them. Each asserts the cells that the actions these checks ran on fill
(`Reduction::param_cells`) and that none fills a cell the matrix rules out. In every build, `moves_of_into` panics when the
representative of a class lacks a rise a `T` step needs. Corpus cases
`cone_triggered_param_on_a_stub` and `cone_rise_param_on_a_stub` hold BFS and TLC to the cone's
counterexample on both. `::a_param_left_on_a_stub_makes_the_cone_decide_unreduced` drops
`bind_stub_params` on those two cases: the check names `Issue`'s and `Note`'s parameter, and the
cone is decided unreduced, with the cone's verdicts. `::no_extended_sub_lifecycle_leaves_a_param_on_a_stub`
builds the top-level `X` of every target and property cone of the samples, the domain samples
and the corpus, with and without inputs, and the check passes on each.
`::a_fold_whose_key_the_restriction_drops_leaves_its_action_unfolded` runs corpus case
`cone_fold_key_dropped` at 1 and 2 instances: `GrantNoted`'s fold would be keyed by its acting
`Existing` Desk row, which `restrict_to(S_v ∪ inputs)` drops; the split leaves it unfolded with
Desk's existence an input, the check passes, and `assert_reduction_exact` holds; with the
unfolding dropped, the check names the key (`Fault::KeyDropped`) and the cone is decided
unreduced, with the cone's verdicts. `::a_rise_guard_on_a_dropped_new_row_goes_with_the_row`
runs corpus case `cone_rise_names_a_dropped_new_row`, a rise whose condition joins a row of
`S_v` to the order it creates, which `X` drops with that guard, through
`assert_reduction_exact` at 1 and 2 instances.
`reduce/extended.rs::tests::a_stub_param_no_fold_or_rise_stands_for_is_left_and_reported` counts
`Note`'s rise among the `S_a` actions: `bind_stub_params` leaves its parameter's role, and the
check reports it as unfolded.

The whole lemma, rises included, is checked step by step by `reduce/lemma6.rs::check` on every
reduced target and property cone whose unreduced cone completes under 200,000 states, nested
reductions included (`core/tests/reduce_steps.rs`). It explores `X` plainly and maps each state
of `C` to `x(t, s)` by `Reduction::key`. Every reachable state maps into `Reach X` (an initial one
to an initial state of `X`). Every `S_a` step of `C` is a silent edge of `X` by an action that
stands for it, or leaves `x` unchanged. Every `T_a` step leaves `S_v` alone, and one that changes
the input bytes is a rise of `X` at each stage of `Reduction::stages`, ending at `x(t', s)`.
Every silent edge of `X` from `x(t, s)` other than a self-loop is the image of an `S_a` step of
`C` from `(t, s)`. It then checks Theorem A8's relation without the reduction's class tables: it
computes the blocks of `Reach X` again with the sweeps of `partition/reference.rs` from the
projection on `obs`, the raw inputs and the bits, labels each state of `C` and of `R` by its `T`
part and the block of its `X` state, and requires the two runs to have the same labelled states,
initial labels and labelled edges up to stuttering. `::reduced_cones_step_as_their_cones_step`
runs it on every corpus case at its flags and on every sample root at the flags of its `// expect
verify` lines: 53 reductions, 10 of them nested, all with the class level; 25 more reduced
cones of the samples pass 200,000 states unreduced and are not checked.
`::the_stub_cases_step_as_their_cones_step` runs it on `cone_triggered_param_on_a_stub` and
`cone_rise_param_on_a_stub`, and again with `bind_stub_params` dropped in the `X` the check
builds: the first then fails on an `Issue` step that has no silent edge in `X`, and the second on
a `Note` step that has no rise. `::a_removed_class_move_fails_the_class_level` removes the class moves of one class the
reduced run reaches, and the class level fails. `scripts/reduction-diff.py lemma6` runs the
check on random models: CI runs it on the 522 PR seeds, of which 361 reduce a cone. The check
shares `Reduction::key` (and so `Fold::holds`), `Extended::build` and the step relation with the
code it checks.

TLC checks the first conjunct and the `C ⇒ X` direction of the others on every reached state of
each of the 21 reduced corpus cones
(`cli/tests/engine_corpus.rs::tlc_refinement_holds_on_every_reduced_cone_of_the_corpus`,
[tlc-agreement.md](./tlc-agreement.md#the-reduction-step-by-step)), with its own semantics of
the unreduced cone: each state's key is a state of `X_n`, and each `S_a` step and rise of the
cone is a class move or a rise of the printed reduced module, or a stutter. With the stub
parameters left unbound and the stub check off, it fails on both stub cases at the step `X`
drops (`Issue`, and `Note`'s rise).

M: `proofs/lean/RdraIsh/Lemma6.lean` proves the lemma over the action-level model of Lemma 5's
evidence, for an `X` built as this definition builds it (`Restrict.lean`). A role is a guard
conjunct on its entity's existence (`candidate_slots`). Each parameter has a side, and `X` gives
it a role: kept on `S` and on an input, `Earlier { step: 0 }` for a fold's key off `S`
(`fold_param`) and for a stub parameter that needs a present row of a rise or a folded action
(`bind_stub_params`), dropped on a dropped entity and for `New` on a stub. `X` keeps the
conjuncts `restrict_to` keeps and reads one bit per fold at the key's slot; a rise writes the
raw inputs, a flip one bit, and `stages` lists the raw inputs and then each bit that changes.
From `Lemma6Hyp` it proves both `S_a` halves, with the cone's binding equal to the edge's
outside the fold's free `T` parameters (`s_fwd`, `s_back`), and the rise half through the
stages (`rise_fwd`). So the hypotheses `Lemma6By`, `Lemma6Staged` and `Lemma6` of the Lean
proof of Theorem A8 are theorems (`lemma6_by`, `lemma6_staged`, `lemma6`). `Lemma6Hyp` holds
per-action facts read from the code, among them (the Lean README lists every field):

- `Local` and Lemma 5 (a)–(c), which `split_faults` certifies;
- a `New` parameter's existence is written;
- the conjuncts `X` keeps read `S_v ∪ inputs`, and every other conjunct of an `S_a` action is
  folded;
- step (a): `Fold::holds` computes the bit exactly (`exact`);
- what the folded conjuncts, the other conjuncts and the effects name (`fold_of`);
- a folded action's effect reads `S_v ∪ inputs`;
- step (c): a rise's effect on the inputs reads `S_v ∪ inputs`.

The stub and dropped cells of an unfolded `S_a` action, for every role that reads its row (an
`Earlier` parameter binds any slot on both sides), and a `New` `T` parameter of a folded one,
are derived from Lemma 5 (a)(c) (`unfolded_side`). The cone is the cone without the steps
of an action that writes nothing and is not kept, which `X` does not hold either: such steps
stutter, and the two reach the same states (`coneW_reach_iff`). `Lemma6Example.lean`
discharges `Lemma6Hyp` on a model with a folded action whose free `T` parameter is on a stub,
an unfolded action with a kept conjunct, and a rise with a parameter on a stub. With
`bind_stub_params` off, the `C ⇒ X` half fails there (`stub_bug`).

The model leaves out node folds, groups of folds sharing a bit, the renumbering of step (b)
(a binding of `X` keeps the cone's positions, and nothing states that `X`'s conjuncts and
effects ignore the slots of dropped parameters), the rule that a fold whose key the
restriction drops stays unfolded (`X` reads the bit at the key's position whatever its side,
so `Fold::key_kept` and `Fault::KeyDropped` play no part), and the nested case. It takes `X`'s initial
states to be the projections of the cone's, and does not derive them from `restrict_to` and
`fold_initial`. It reads the model, not `Extended::build`.

The tests above are evidence on these models and bounds, and the Lean proof is a proof over the
model, not over the code. cone-soundness.md, "Proof obligation", states the lemma in points 2,
3, and 5.

**Theorem A8 (the reduced cone is stuttering equivalent to the cone).** Assume (B1) to (B3) for
`B` and Lemma 6. Let `(t, s) ~ (t, ρ)` when `ρ` is the representative of the block of `x(t, s)`.
Then `~` is a divergence-blind stuttering bisimulation between `C` and `R` that preserves `T_v`,
`obs`, and the action label of every `T_a` step. Consequently `C` and `R` agree on:

1. which valuations of `T_v ∪ obs` are reachable (invariants, `always`, witnesses, vacuity). The
   inclusion of `C`'s in `R`'s, which every `ok` of an invariant reads, needs every `S_a` step of
   `C` to be a silent edge of `X`, including the steps whose parameters lie on a stub (Lemma 6,
   "Parameters");
2. which `T_a` action labels an edge from a state with given `T_v ∪ obs` values (`NeverRuns`, arms
   taken, `after` assertions on `T_a` actions);
3. whether from every reached state some path takes an edge that changes given `T_v` values
   (deadlocks, as [semantics.md, D25](./semantics.md#d25) defines them: a silent cycle counts
   as stuck, which this divergence-blind bisimulation keeps);
4. every property `P` whose reads are in `T_v ∪ obs` and whose fair actions are in `T_a`, under
   `Plain`.

*Proof.* Five points, numbered as in cone-soundness.md, "Proof obligation", with Lemma 6 made
explicit.

1. Related states agree on `T_v` (same `t`) and on `obs` (B1).
2. A `T_a` step that leaves the *input bytes* unchanged. The input bytes are
   `Reduction::input_bytes`: the raw inputs followed by the bits `Reduction::push_bits`
   computes. A step that changes only a bit, with the raw inputs unchanged, is treated under
   point 5, where `Reduction::stages` gives it a stage of its own. The step reads `T_v` and `obs`
   only and writes `T_v` only
   (Lemma 5b), so it is enabled at `(t, s)` iff at `(t, ρ)`, with the same successors on `T_v`, and the
   block of `x` is unchanged.
3. An `S_a` step `(t, s) → (t, s')`: by Lemma 6 it is a silent edge of `X`. If `x(t, s')` is in the
   same block it is a stutter of `R`. Otherwise it enters a block `B'`; by (B2) a silent path from
   `ρ` through its own block enters `B'`, and its last edge is a class move of `R` into `B'`.
   Conversely a class move from `(t, ρ)` into `B'` comes from such a path from `ρ`; by (B2) a path
   from `x(t, s)` through its block enters `B'`, and by Lemma 6 each of its edges is an `S_a` step of
   `C` from the corresponding state, whose intermediate states are related to `(t, ρ)`.
4. Every initial `(t₀, s₀)` is related to `(t₀, rep(block(x(t₀, s₀))))`, which is how
   `Classes::init` builds `R`'s initial states; `R` has no other.
5. A `T_a` step that changes the input bytes (a raw input, a bit, or both): by Lemma 6 it is a
   sequence of rises from `x(t, s)`, one per stage of `stages`; by (B3)
   and (B1) the representative has rises into the same blocks with the same input bytes, stage by
   stage, which is what `recanon` looks up. A fold is covered by the same argument with its bit
   in place of the variables it folds, since the folded guard is a function of them and the key
   (that `Fold::holds` computes it exactly is step (a) of Lemma 6). The monotone rule is not used here: it bounds the input histories, which keeps the
   extended system finite and small.

Items 1 to 3 of the consequences are existential statements over finite paths labelled by
`T_v ∪ obs` and `T_a` actions, which a stuttering bisimulation preserves (Browne, Clarke and
Grumberg 1988; for the divergence-blind variant, De Nicola and Vaandrager 1995). For item 4, a
behavior of `C` that steps inside one block forever maps to a behavior of `R` that stops there. Under
`Plain` a stop is allowed iff no fair instance is enabled; fair instances are `T_a` actions (Lemma
5e), their enabledness reads `T_v ∪ obs`, which is constant on the block, so if one were enabled the
`C` behavior, satisfying `Fair_P`, would take it and leave the block's image. The converse lifts a
stop of `R` to a stop of `C` at the same `T_v ∪ obs`, and a cycle of `R` to an infinite `C` behavior
by point 3. ∎

What the proof does not cover is `WF_vars(Next)`, which a stop inside a diverging block violates;
the next section treats it.

*Hypotheses and code.* (B1)–(B3): `reduce/partition.rs::quotient_blocks`, certified by
`reduce/partition/reference.rs::stable` before use (below);
Lemma 5: `split_at`, `split_with`, certified by `reduce/lemma5.rs::split_faults` before use; Lemma 6: `Extended::build`, `reduce/fold.rs`; the checks a
reduced cone keeps: `drop_unreadable_checks` (A14).

*Evidence.* P-sketch (Lemma 6 is argued for the code and proved in Lean over an action model; Lemma 5 is checked at run time on each split
and (B1)–(B3) on each partition the reduction uses, R). For Lemma 5:
`reduce/lemma5.rs::a_planted_split_fails_the_clause_it_breaks` plants a split that breaks
each clause and the side rule in turn, and the last through the switch `split_sides`, on which a
reduced cone is decided unreduced; `core/tests/reduce.rs::every_split_passes_its_check` checks
every top-level split of the samples, the domain samples and the corpus cases, with and without
inputs. M: `Twin::Partition` (worklist against
sweeps), `Twin::ClassMoves`, `Twin::FoldHolds`, the `shadow_assert!` that a steady action leaves the
inputs. M also: the Lean proof in `proofs/lean/` proves the theorem and its four
consequences from Lemma 5 (a)(b), Lemma 6, the shape of `X` and (B1)–(B3) as hypotheses:
`ReductionLabels.lean::a8_branching` (the relation is a divergence-blind branching
bisimulation that keeps every `T_a` label, with its binding), `a8_edges_iff` (consequence 2),
`a8_stuck_iff` (consequence 3), `Plain.lean::a8_plain_iff` (consequence 4, for
`□(trig ⇒ ◇goal)` under `Fair_P` over infinite behaviours), `Stages.lean::a8_staged` (point
5 with `recanon` over the stages), and `Nested.lean::a8_nested` (a nested `X`, by induction
on depth). `Lemma6.lean::a8_actions` derives the hypotheses `Split`, Lemma 6 and the shape of `X` over the action model (Lemma 6's evidence), so A8 with `recanon` holds there from `Lemma6Hyp`, (B1)–(B3) and the rise table; it does not derive Lemma 6 from the code. In the nested case its outer
representative is a reachable state of `X` and its outer class moves and rises are read on
`X`, where the code takes an inner representative and reads them on the inner run.
R: every build checks, on each run of `X` before the reduction uses it, three hypotheses the
Lean proof takes (`reduce/hypotheses.rs`). On a fault the cone is decided unreduced, with a
warning; a debug or `shadow` build panics. The run is `X`'s own when `X` has no inner
reduction and is not run up to `H`, and there the checks are the Lean hypotheses literally.
Otherwise they read the graph `of_graph` reads, the inner run or the orbit graph, and the step
from it to `X` stays argued. An unobserved `S` has no rise table to check.

- `rise_faults` checks the rise table (`RiseTable`) and `X`'s side of the stages
  (`Lemma6Staged`). Each rise of a class's representative sets the raw inputs or flips one
  bit slot, and `stages` lists it as exactly one stage. The table holds exactly the rises of
  each class's first state, the representative, under the input bytes they end at and with
  the class they enter. That every other state of the class rises as the representative does
  is (B3), which the stability certificate below checks.
- `layout_faults` checks that the engines read the input bytes with `X`'s raw width and length.
- `inner_faults` checks the inner split of a nested reduction. Every rise is in its `T_a`, and
  every `obs` variable and input of `X` is on its `T` side or in its `obs`. With the inner
  partition's (B1) and Lemma 5 (b) of the inner split, this fixes on the inner class the
  representative's `obs` and input bytes, which rises it takes, and the input bytes they end
  at. It does not fix the block a rise enters, which the code reads on the inner run.

The checks recompute what they check from the run's edges, the state bytes and the split's
sets. They do not show that a `T_a` step of `C` has rises through every stage it lists: a
missing stage is a `recanon` miss in `moves_of_into`, which panics in every build.
`reduce/hypotheses.rs::tests::a_planted_stage_or_rise_table_fails_its_check` plants faulty stages
and tables. `::a_planted_fault_makes_a_reduced_cone_decide_unreduced` plants each fault in
`of_graph` and `Classes::of` and sees the cone decided unreduced.
`core/tests/reduce.rs::every_quotient_passes_the_checks_of_its_hypotheses` reduces every cone
of the samples, the domain samples and the corpus cases at `Bounds::STATES`, rises and nested
reductions included.
D: `core/tests/reduce.rs::reduced_cones_decide_what_the_cones_decide`,
`::reduced_domain_cones_decide_what_the_cones_decide`, `::a_violation_behind_the_sub_lifecycle_lifts_to_a_longer_real_trace`;
`scripts/reduction-diff.py`; `cli/tests/engine_equivalence.rs` on reduced cones.

**On (B1)–(B3).** `quotient_blocks` starts from the blocks of `obs` and
inputs (B1), refines with `Stutter::blocks`, then splits by the set of blocks each state reaches
by one rise and refines again, until a pass splits nothing. The reference
(`reduce/partition/reference.rs`) computes the same definition by sweeps: every block a splitter in
each sweep, the loop ending only when a sweep splits nothing, so its output satisfies (B2) and (B3)
when it returns. A `shadow` build compares the worklist's partition with the sweeps' up to
2,500,000 states (`SHADOW_CAP`).

`reduce/partition.rs::of_graph` certifies, in every build, each partition before the
reduction uses it, with `reference::stable`: (B1) every block lies inside one block of the
labels the partition starts from; (B2) one sweep of `stuttering_blocks` with every block a
splitter splits nothing; (B3) every state of a block enters by a rise the same set of blocks as
the block's first state. A partition that fails is reported as a tool bug on stderr and replaced
by the sweeps' partition. So (B1)–(B3) hold of every partition a reduction uses. M: Lean
proves the step from the check to (B1)–(B3) over a model of it
(`proofs/lean/RdraIsh/Sweep.lean::stable_of_cert`): on a complete run of a flat `X`, the
states one sweep marks for a splitter are exactly those that leave their block into it by a
silent path through the block (`Sweep.lean::marked_iff`), so a sweep that splits nothing is
(B2), and a partition that passes the check is `Stable` with each block's first state as
representative; a `Stable` partition passes it (`Sweep.lean::cert_of_stable`).
`Sweep.lean::check_sound` proves a Boolean checker over the run's lists sound for the same
check. Read from the code and not proved: that the depth-first marking of `stuttering_blocks`
computes the marked set, that the run is `X`'s reachable graph, and that the sweeps'
fallback partition meets the check when they stop (its last sweep and its last pass over the
rises split nothing).
Coarseness is not certified and is not needed: a finer stable partition is still a
bisimulation. Evidence: `reduce/partition.rs::the_stability_certificate_rejects_a_partition_that_is_not_stable`
(a partition that mixes labels, one where a block's states reach different blocks silently, one
where they rise into different blocks, and a silent cycle without an exit beside a state that
leaves) and the random-graph test below, which also certifies every worklist partition it builds.

**A9 to A14 and A16, A17.**

- **A9** (worklist = sweeps): M (`Twin::Partition`, skipped above 2,500,000 states) and the test
  `reduce/partition.rs::the_worklist_partition_is_the_partition_of_repeated_sweeps` on random graphs.
  No verdict rests on it: the certificate above checks the partition used,
  so a worklist that returned another stable partition would cost states, not soundness, and an
  unstable one falls back to the sweeps.
- **A10** (monotone inputs): Lemma 5c–d, checked at run time on every split (`split_faults`, R),
  and point 5 of Theorem A8. Monotonicity is not used in A8; it
  keeps `X` finite and small. It is used in A15 (case 3 and `s_cycles`). M: Lean proves Lemma 5
  (c)(d) (`proofs/lean/RdraIsh/Split.lean::lemma5_of_closed`, Lemma 5's evidence above), and,
  over a model of the write graph `monotone_vars` builds and of `each_successor`, that a step
  from a pre-state that meets `Pre` (a created row absent with its cells at base, a deleted row
  present, values in their domains, the clock below `int_max`) moves each cell of a variable
  that is not cyclic along that graph (`proofs/lean/RdraIsh/Monotone.lean::succ_covers`), so a
  variable it marks monotone changes finitely often along every behaviour
  (`Monotone.lean::monotone_finite_changes`).
- **A11** (folds): Lemma 6, whose step (a) derives the fold equivalence from `fold_of` and
  `Fold::holds`, which the `Twin::FoldHolds` twin checks against its reference. Two Kani
  harnesses (`reduce/fold.rs::proofs`) prove step (a) for one action at small bounds. A count that reads `T` is never
  node-folded (counting-rules.md): `reduce/fold.rs::as_node` makes a node only of an existential (or an
  And/Or of them), and `strip_links` returns `None` on a `Pred::Count` in its body.
- **A12** (counters on `T`): `split_at` seeds `T_v` with every counter, and `monotone_vars` marks
  counters cyclic, so no counter is an input. P-full by reading. M: Lean,
  `proofs/lean/RdraIsh/Split.lean::no_counter_input` (every input is monotone, so a counter that
  `monotone_vars` marks non-monotone is none).
- **A13** (one reduction for both engines): `reduced_with` rejects a reduction past
  `TLA_CELL_BUDGET` (530,000 cells, `Reduction::tla_cells`) and an identity quotient
  (`Classified::Nothing`, `Reduction::shrinks`); the one `Reduction` value feeds the native checker and
  the TLA+ printer. Past the budget, a split with inputs or folds is tried again without inputs
  (`reduce/mod.rs::TransitionSystem::reduced_in` on `Unreduced::OverBudgetWithInputs`), and the
  partition stops building a quotient once its classes times the observed variables pass the
  budget (`Classified::TooLarge` in `reduce/partition.rs`). True by construction; the
  rejection past the budget is read from the code:
the tests count one cone's cells and keep a cone below the budget, and none makes
`reduced_with` reject one past it.
- **A14** (dropped checks have their own cones): by Lemma 5e a target's own check reads only `T_v ∪
  obs`, an `after` target's steps are kept and so in `T_a`, and a property's fair actions are kept. So
  `drop_unreadable_checks` drops only checks that ride along in the cone system and are not its
  targets; each has its own target and cone. `Covered::covers` requires the covered checks to be in the
  covering module, so a dropped check is never covered by a cone that dropped it. P-full by reading;
  R: Lemma 5e is checked at run time on every split (`split_faults`); no test targets the drop
  itself.
- **A16** (lifted traces): `reduce/lift.rs::TransitionSystem::lift` replaces each class move by the
  silent path (B2) promises, recursively through nested reductions, and the violation is re-read on
  the lifted last state (`LiftFault` when it no longer violates). Every lifted step, a `T` step and
  an `S` step alike, is checked against the unreduced cone's moves in every build (`is_move` over
  `plain_moves`), and a step that is not one is a `ReplayError`, reported as a fault, not a
  counterexample (test `core/tests/reduce.rs::a_lifted_step_that_is_no_move_of_the_cone_is_a_replay_error`).
  `lift`'s look-ups are checked too: a start with no class, or whose class holds no initial `S`
  state, or whose concrete `S` path up to `H` starts elsewhere, is `ReplayError::NotInitial`
  (test `::a_lift_from_no_initial_state_is_a_replay_error`, a start with no class); a `T`
  step's rise or a class move's target that the extended sub-lifecycle lacks is `NoMove`, and so
  is an `S` step of the extended sub-lifecycle whose action has no counterpart in the cone; an
  inner lift that returns the wrong number of groups is `Length`. A class move with no silent
  path behind it is `NoMove` too: `reduce/partition.rs::Succ::shortest_path` returns `None` (test
  `core/tests/reduce.rs::a_class_move_with_no_s_path_is_a_replay_error`, which forges a move
  back to the class just left). The lifted start state is built from the run's initial state
  and an initial `S` state of its class, and is not itself checked against the unreduced cone's
  initial states. What still panics on the lift's way, `lift_stop`'s `expect`s among it, is
  listed with the reason each holds in the
  [claims inventory](./claims.md#panics-left-on-a-lifts-way).
- **A17** (the cache): the cached quotient is keyed by the extended system, the caps, and the group;
  `Twin::CacheHit` recomputes every hit in a `shadow` build. M.

---

## `WF_vars(Next)` on a reduced cone

<!-- derived-from ../design/cone-soundness.md#wf_varsnext-on-a-reduced-cone -->
<!-- derived-from ../formal-verification.md#wf_varsnext-on-a-reduced-cone -->

**Definition 5 (classes that can loop).** `Reduction::can_loop(c)` is the mark `reduce/partition.rs::divergence`
computes, or `Classes::unobserved` when `obs = ∅`:

1. the silent edges of `X_n` between two different states of block `c` contain a cycle (a depth-first
   search over in-block edges with `x ≠ y`; a cycle exists iff the search meets a back edge);
2. a state of `c` holds an inner class that can loop;
3. a silent orbit self-edge (a run of `S` up to `H`) starts in `c`;
4. `obs = ∅` and some non-input variable of `X` is not monotone (`s_cycles`).

When every non-input variable of `X` is monotone, no class is marked (`of_graph`, `s_cycles` false).

**The run.** `explore/liveness.rs::ComponentRun::decide_liveness_reduced` searches `R`'s explored graph
with `StopRule::Looping`: a state may stop when no fair instance is enabled and either no move leaves it
or its class can loop.

**Theorem A15.** Assume Theorem A8's hypotheses. If the run finds no counterexample and `R`'s graph is
complete, then `C ⊨ P` under `WfNext`.

*Proof.* Suppose `σ` is a behavior of `C` that satisfies `Fair_P ∧ WF_vars(Next)`, has a state where
`trig_b ∧ cause_b` holds, and never meets `goal_b` from there. Map each state to its `~`-image. By
points 2, 3, 5 of Theorem A8 each step of `σ` is a step of `R` or leaves the image unchanged, so the image
is a path of `R`'s graph from an initial state, and it agrees with `σ` on `T_v ∪ obs`, hence on the
trigger, the cause, the goal, and the enabledness of every fair instance (fair actions are `T_a`
actions, Lemma 5e). A `T_a` step is the same step on both sides, so a fair instance is taken in `σ` iff
its image is taken.

- *The image changes infinitely often.* From some point it stays in one strongly connected component
  `K` of the goal-free subgraph and uses only its edges. A fair instance is taken infinitely often in
  `σ`, hence labels an edge of `K`, or is disabled infinitely often, hence disabled at some state of
  `K`. `K` has an edge, so it admits a fair cycle, which the search finds.
- *The image changes finitely often, `σ` infinitely often.* From some point `T_v` is fixed at `t`, the
  block is fixed at `c`, and every state-changing step of `σ` is an `S_a` step inside `c`. `obs` is
  constant on `c` (B1), so each fair instance's enabledness is constant from that point; one that is
  enabled would be taken (`Fair_P`) and change `T_v`, so all are disabled. It remains to show that
  `c` can loop. (i) If infinitely many steps change the state of `X_n` (by Lemma 6, via the inner
  canonical form), they form an infinite walk inside `c` in a finite graph whose edges join different
  states, so they contain a cycle: case 1 (or case 3 when the run of `S` was up to `H` and the walk
  closes on an orbit). (ii) Otherwise, from some point every step changes only the inner sub-lifecycle,
  and the same argument one level down gives case 2. (iii) If `obs = ∅`, `X` is not explored; an
  infinite path of silent steps changes some non-input variable infinitely often. Each change of a
  variable `v` is an edge of `v`'s write graph (every effect contributes its edges to
  `monotone_vars`), and a variable whose graph is acyclic changes at most `|dom(v)| - 1` times per slot,
  so `v` is not monotone: case 4. The same fact justifies leaving every class unmarked when
  `s_cycles` is false. So `(t, ρ)` may stop.
- *`σ` stays at `(t, s)` forever.* `WF_vars(Next)` says no step leaves `(t, s)` in `C`. A `T_a` move of
  `R` from `(t, ρ)` reads only `T_v ∪ obs` and would leave `(t, s)` too (point 2 or 5). A class move into
  `c' ≠ c` would, by (B2) and Lemma 6, be matched by a silent path from `x(t, s)` into `c'`, whose
  first state-changing edge leaves `(t, s)`. So no move leaves `(t, ρ)`, and it may stop.

In every case a triggered image state reaches a fair end through goal-free states, which is a
counterexample of the run. ∎

The proof does not use that the mark is tight; extra marks only add stops, so extra candidates.

**Candidates.** A counterexample of the run is lifted (`reduce/lift.rs::lift_stop`, `::lift_cycle`) to `C`,
replayed there (`replay_lasso`), checked to fail `P` (`explain_property`) and to pass `unfair_loop` under
`WfNext`; otherwise it is `ReducedVerdict::Spurious` and `cli/src/verify/native.rs::native_property_after` runs
`Cone_P_Full` and `Cone_P_WF` on `C`. `lift_cycle` gives up after `MAX_COPIES` (4,096) copies of the loop,
which also yields `Spurious`, as does a lift that fails its own step checks (`Spurious::Fails`) or
finds no S path behind a class move (`Succ::shortest_path` returns `None`). A printed
counterexample of this run is therefore a lasso of
`C` that the checker certified. Whether it is a counterexample of `T` is Theorem A5.2 (needs `W` exact).

*Evidence.* P-full relative to (B1)–(B3), certified at run time, and Lemma 6, argued
and not proved. M: `proofs/lean/RdraIsh/Fairness.lean::a15` proves the theorem in Lean for the
flat case, from Lemma 5 (a)(b), Lemma 6 both ways, the shape of `X` and (B1)–(B3) as
hypotheses (as `a8_dbsb` does), finitely many reachable states of `X` and `R`, and the
hypothesis that every block whose silent edges between different states contain a cycle can
loop (case 1 of Definition 5). The run's verdict is read as "no lasso of `R` that satisfies
`Fair_P` and stops only where no move leaves or the class can loop violates `P`", and
`Search.lean::a15_search` takes instead the verdict of the search over components itself: N9,
proved in `Search.lean` (`accepts_iff_lasso`), makes the two the same.
Cases 2 and 3 of Definition 5 and the proof's (ii) belong to the nested case, which is not
modelled. The proof's (iii) is proved: a cell whose steps follow an acyclic graph changes
finitely often along every behaviour
(`proofs/lean/RdraIsh/Monotone.lean::finite_changes_of_behaviour`), and when every cell of `S`
follows one along the silent edges, no block of any partition has a silent cycle, so `a15`
holds with no class marked, which is case 4 with `s_cycles` false (`Monotone.lean::no_silentCycle`,
`Monotone.lean::a15_monotone`). The code's unobserved `S` is one class for all input bytes,
which is not (B1); the reading of its `R` as the reduced cone of the partition by input bytes
is argued.
`ExampleFair.eventually_flag` discharges every hypothesis on a cone whose one block holds a
silent cycle. R: every build checks that hypothesis, case 1, on each explored run of `X`
(`reduce/hypotheses.rs::loop_faults`): it peels the silent edges between different states
inside the unmarked classes, and a state left over lies on or after a cycle of its class. The
marks it checks are `divergence`'s, or all false when `s_cycles` says nothing loops. On a fault
the cone is decided unreduced, with a warning; a debug or `shadow` build panics. On a flat `X`
not run up to `H` this is case 1 literally; otherwise the check reads the stored graph, and
cases 2 and 3 stay argued. An unobserved `S` is not explored in a release build. There,
`s_cycles` is checked only in a test build, on a plain run of `X` under the test's own caps; a
run past them is skipped (`partition.rs::unobserved_loops`).
`reduce/hypotheses.rs::tests::a_planted_loop_mark_fails_its_check` and
`::a_planted_fault_makes_a_reduced_cone_decide_unreduced` (a back edge that marks nothing, and
`s_cycles` false) plant the fault. R also: lift, replay, and fairness check of each candidate. D: corpus cases `cone_reduced_class_loops`, `cone_reduced_inner_class_loops`,
`cone_reduced_loop_fair_enabled`, `cone_reduced_loop_unreached`, `cone_reduced_no_loop_capped`;
`cli/tests/engine_corpus.rs::a_reduced_wf_next_run_decides_what_the_unreduced_one_decides`;
`reduce/partition.rs::an_unobserved_sub_lifecycle_can_loop_unless_every_var_only_rises`.

---

## Instance symmetry

<!-- derived-from ../design/instance-symmetry.md#the-group -->
<!-- derived-from ../design/instance-symmetry.md#the-canonical-form -->
<!-- derived-from ../design/instance-symmetry.md#soundness-per-check -->
<!-- derived-from ../design/instance-symmetry.md#counterexamples -->
<!-- derived-from ../design/instance-symmetry.md#composition-with-74 -->
<!-- derived-from ../formal-verification.md#instance-symmetry -->

**Definition 6 (the group).** `explore/symmetry/mod.rs::Group::of` partitions the entities of a system
into classes (an owner with `Start::Identity` couples a child with its parent; `now` is in none) and
leaves a class out when `explore/symmetry/breaks.rs::scan` finds a `Rule` against one of its entities,
or the class belongs to a block of classes naming each other's rows that has more joint
arrangements than `JOINT_CAP` (5,040) (`Group::cap_joint_blocks`, `Rule::JointTooLarge`; a cost
cap, not a soundness condition), or the system is reduced and `Reduction::fixed_why` names it (`Rule::Reduced`), or the run is of `S` up to
`H` and `Extended::fixed_why` names it (`Rule::Observed`). For a state `s`, `G_s` is the product over the
remaining classes of the permutations of the slots that are not absent in `s` (for a coupled class, not
absent in either entity). `g ∈ G_s` acts by `Group::apply`: a slot's variables move with the slot, and
an owner value on a non-absent row is relabelled by the parent's permutation (`Group::relabel`); an owner
on an absent row keeps its value. A guard's `Term::Index(p)` is the slot of parameter `p`, and `g` maps a
binding `p` to `g(p)`.

**Hypotheses.**

- (G1) A term naming a row (`Term::Index`, an owner) is compared only by `=` or `≠`, and only with a term
  naming a row of the same class, or an optional owner with `null`. Scan rules `OrderedId`,
  `IdAgainstLiteral`, `MixedIds`.
- (G2) An effect writes an owner only from a parameter of the parent's class or `null`, and a `Choose`
  or `Transition` on an owner ranges over every row. Rules `OwnerLiteral`, `MixedIds`, `OwnerChoice`.
- (G3) `I` is closed under every `G_s`. D4 makes `I` a product in which every `Start::Any` variable takes
  every value of its domain and created entities start absent; rule `PinnedStart` leaves out a class
  whose start pins a slot (an identity start between unequal counts, a pre-existing row whose owner
  starts at a row).
- (G4) No step leaves a mandatory owner at its base value on a non-absent row. Rule `UnlinkedCreate`.
- (G5) A deleted row keeps its owners (D8, `DeletedRowOwner`), in the native checker, z3, and the TLA+
  export, so `apply` relabels them with the parent's rows.
- (G6) No predicate reads the owner of an absent row. Rule `AbsentOwner`: the scan leaves the parent's class out when a guard reads, in the state before the step, an
  owner on a `New` parameter, the row the step creates (`explore/symmetry/breaks.rs`, `absent_owners`). The scan
  applies that rule to action guards only. Within a guard it is the one way to reach an absent
  row: `Forall`, `Exists` and `Count` range over present slots, `Existing`, `Parent` and
  `Triggered` bind present slots, and `Earlier` binds a slot by its run mark, which only a
  created row holds. The predicates outside guards (invariants, `after` assertions,
  properties, `can_reach` goals) need no rule: every read of them that a verdict depends on is
  gated by presence, whatever the predicate reads. An invariant binds present slots
  (`expr.rs::TransitionSystem::find_violation`) and quantifies over present slots; an `after`
  assertion reads the state after its step, where each parameter's row is present (created
  by the step or present before it) or deleted; a property's trigger is
  `present ∧ link` (`lasso.rs::TransitionSystem::triggers`), its other reads are taken where
  the trigger holds or, for the second state of a step, where the bound rows are present
  (`step_breaks`), and the goal of `eventually` and `leads_to` counts only on states after a
  trigger, where a row present then is present or deleted, since a slot never becomes absent
  again; `can_reach` reads its link and goal only where the bound rows are present
  (`reach.rs`). An owner on a present or deleted row is one `apply` relabels (G5), so these
  reads take the same value on `s` and `g(s)`. A goal is still *evaluated* on a slot whatever
  its existence (native.md, "The goal on absent rows"), but that value reaches no verdict
  unless the rows were present at the trigger. Test:
  `explore/symmetry/g6.rs::reads_outside_guards_meet_no_absent_owner`, on a model of shops and
  orders at 2 instances that reads the order's owner through a cross `forbidden` with
  `.along`, an `at_most` invariant, two `after` assertions, and `leads_to`, `eventually`,
  `can_reach`, `stable` and `precedes` with `.along`: each gated read is the same on every
  reachable state of the plain run and its canonical form, the `eventually` and `leads_to`
  verdicts and the safety findings of the plain run and the run up to symmetry agree, and a
  goal given a read of an order's key by hand differs on rows never created and changes none
  of them.

Two facts of the builder enter the theorem besides (G1)–(G6), and the scan checks neither: every
`New` parameter is created by its action (`build/actions.rs`), and every `Earlier` parameter carries
the run-mark conjunct `mark = k` of `run_marks`, with `k ≥ 1` and the mark's base `none`.
`restrict_to` keeps both. A run-mark guard is a plain read (`cone.rs::plain_reads`), so its mark is
in the cone of every action the cone keeps, and a `New` parameter stays only with its existence, and
so with its `Create`. The
`Earlier { step: 0 }` parameters of the extended sub-lifecycle (`reduce/extended.rs`) do not carry
such a conjunct; they bind entities that `H` leaves out (A22).

Let `W` be the states in which every absent row holds the base value of each of its variables, its
owners among them. `W` holds the initial states, since a created row starts absent at its base
values. It is closed under steps, since a step writes only rows that are present or deleted after
it. It is closed under every `G_s`, since `apply` keeps an absent row's values. So it holds every
image `g(s)` of a reachable `s`, which is what the quotient stores.

**Theorem A18.** Under (G1)–(G6) and the two facts of the builder, for every `s ∈ W`, every
`g ∈ G_s`, and every step `s →_{a,p} t`: `g(s) →_{a,g(p)} g(t)` is a step and `g ∈ G_t`. Hence
`≈ = {(s, g(s)) : g ∈ G_s}` is an equivalence and a bisimulation on `W`, and a breadth-first search
that stores one member of each `≈`-class reaches exactly the classes of `Reach(T)` (or of the cone it
runs on), each at its least depth.

Outside `W` the commutation can fail. Take an absent row whose optional owner names a row, and a
step that creates the row without writing that owner, which (G4) allows for an owner whose base is
`null`. `apply` keeps the value on the absent row, so the step from `g(s)` leaves it, while `g(t)`
relabels it (`proofs/lean/RdraIsh/SymActionsExample.lean::off_W_counterexample`).

*Proof.* Absent slots of `t` are absent in `s` (a slot leaves `absent` only), so `g` fixes them and
`g ∈ G_t`; `G_s = G_{g(s)}` since `g` preserves the absent set, so `≈` is reflexive, symmetric, and
transitive. *Bindings.* `New` takes the lowest absent slot, which `g` fixes; `Existing`, `Parent`, and
`Triggered` admit a present slot, `Earlier` a slot by its run mark, and `g` preserves presence and moves
marks with their slots. In `W` an absent row holds the base mark, which no run-mark conjunct accepts.
So every bound row other than a `New` one is present or deleted in `s`, and a `New` row is created,
so every bound row is present or deleted in `t`. *Predicates.* A column or mark of slot `i` in `s` is
the same value at slot `g(i)` in `g(s)`. By (G1) a row id is compared only by `=`/`≠` with a row id of
its class, which a bijection preserves, or an optional owner with `null`, which relabelling preserves.
An owner read on a non-absent row is relabelled consistently with the row ids it is compared with
(G5 for deleted rows). None is read on an absent row: (G6) forbids the pre-state owner of a `New` row,
the other bound rows are not absent, and `Forall`, `Exists`, and `Count` range over the present slots,
a set `g` permutes. So every guard of `a` has the same value at `(s, t, p)` and `(g(s), g(t), g(p))`.
*Effects.* Each effect commutes with `g`: a literal write is unchanged; an owner written from a
parameter's slot is relabelled with that slot (G2); a create or delete moves with its slot; a `Choose` on
an owner ranges over all rows (G2), so its set of forks is closed under `g`. An owner the step does not
write on a row it creates holds its base value in `s ∈ W`, which (G4) makes `null` (unchanged by
relabelling) or rules out. So `g(t) ∈ Post_a(g(s), g(p))`. The converse is the same statement for
`g⁻¹ ∈ G_{g(s)}` at `g(s) ∈ W`. With (G3) the initial classes are the classes of initial states. A
bisimulation that relates states at equal depth (it maps each step to one step) preserves BFS depth. ∎

Leaving more classes out gives a subgroup of `G_s`, and the statement holds for every subgroup, so
the scan can always leave a class out and lose only reduction (**A20**, P-full). A *witness*
shows that a rule is needed. It is a model on which dropping the rule (`explore/symmetry/mod.rs::dropped`,
set per test or by `RDRA_ISH_DROP_RULE` in a `shadow` build) breaks the bisimulation or the
verdict. `explore/symmetry/mod.rs::Rule` has eleven variants, and the witnesses are as follows.

| Rule | Witness |
|---|---|
| `OrderedId`, `IdAgainstLiteral`, `MixedIds`, `OwnerLiteral`, `OwnerChoice`, `UnlinkedCreate` | `explore/symmetry/witnesses.rs::*_has_a_witness`, one unit test each |
| `AbsentOwner` | `explore/symmetry/witnesses.rs::absent_owner_has_a_witness`: a guard on `Place` that reads the new order's key before the step lets orders go only under the shop in slot 0, which a permutation of the shops does not preserve |
| `JointTooLarge` | none needed. `explore/symmetry/witnesses.rs::joint_too_large_is_a_cost_cap` (release jobs) drops it and shows that the quotient is still a bisimulation with the plain run's verdicts |
| `Observed` | 2 of its 6 reasons, `obs` and `input`: `cli/tests/engine_corpus.rs::each_h_reason_a_case_shows_has_a_witness`, in `shadow` builds only. The other 4 have none |
| `Reduced` | none for any of its 7 reasons |
| `PinnedStart` | `explore/symmetry/witnesses.rs::pinned_start_has_a_witness`: with it dropped the quotient stays a bisimulation with the same verdicts, but a canonical initial form need not be an initial state, and the lifted counterexample fails the always-on replay (`NotInitial`). So the rule guards the traces, not the verdicts |

So eleven reasons have no witness: the seven of `Reduced` (`explore/symmetry/snapshot.rs::G_T`,
the reasons of `Reduction::fixed_why`) and four of the six of `Observed`
(`explore/symmetry/snapshot.rs::H`, those of `Extended::fixed_why`).
`explore/symmetry/snapshot.rs::KEPT` lists ten of them as kept without a witness of their own,
the snapshot being their witness: six `Reduced` reasons (`S var`, `input`, `named by S`,
`named by an input`, `bit key`, `S action param`) and the four `Observed` reasons
(`named by obs`, `named by an input`, `stub`, `named by T`). `Reduced:S` is left out of `KEPT`
because `S var` implies it: an `S` entity's existence is an `S` variable, and
`reduce/mod.rs` asserts, in every build, that no entity has the reason `S` without `S var`.
The snapshot reports that every scan rule (the nine of `explore/symmetry/snapshot.rs::SCAN`,
`PinnedStart` among them) fires in no cone of the samples and the engine corpus. A missing
witness does not affect soundness, which the subgroup argument gives: the risk is a reason
that is missing, not one without a witness. Whether each rule and reason is needed is open.

**Canonical forms (A21).** `explore/symmetry/canon.rs::Canon::canonical` returns the least layout over
`G_s` while it branches on at most `BRANCH_CAP` (4,096) tied orders, and some member of the class past
that. Theorem A18 needs only "some member": the run reaches every class, and a class stored twice
under two forms only inflates the printed count, which `about` then marks as an upper bound. The
argument is P-full given A18 for the checks whose value on a state is invariant under `G_s`.
Those are the first group of the next paragraph: state rules, `always`, `after` assertions, view
patterns, and `NeverRuns`. The checks that read the annotated quotient (deadlocks, liveness)
follow a binding through the stored permutations. Their
soundness past the cap is part of A23, which is a sketch. M: `Twin::Canonical` compares the canonical form with the least layout over every element of
`G_s` when `|G_s| ≤ 6^4 = 1,296`; D: `explore/symmetry/tests.rs::past_the_branch_cap_a_state_is_cut_and_stays_in_its_orbit`,
`explore/symmetry/witnesses.rs::past_the_branch_cap_the_verdict_stands` (release jobs: `formal-verify`).

**Which checks read a run up to symmetry soundly.** A check whose value on a state is invariant under
`G_s` (every state rule and `always`: they quantify over all bindings), a check of the concrete triple
`(state, params, successor)` before canonicalization (`after`, view patterns), and a check of which
actions label some edge (`NeverRuns`) is decided on the run up to symmetry exactly as on the plain run,
by Theorem A18. Checks that compare two stored states slot by slot across an edge (deadlocks) or
follow one binding along a path (liveness) read an *annotated* quotient: each edge keeps the
permutation `σ` that took its concrete successor to the stored one, and a binding moves by `σ`
(`explore/orbit/liveness.rs::OrbitRun::decide_liveness`, `explore/orbit/findings.rs`). `stable`,
`unchanged`, `precedes` and `can_reach` are not run up to symmetry:
`explore/properties.rs::TransitionSystem::decide_property` runs `stable`, `unchanged` and `precedes`
on the plain restricted cone, and `verify` calls no orbit reader for them (`decide_always` reads
only `always`). An `always` runs up to symmetry only on a cone the sub-lifecycle reduction
leaves whole (`cli/src/verify/jobs.rs::PropertyJob::symmetric`: `reduction().is_none()` for
`LtlKind::Always`); `eventually` and `leads_to` run up to symmetry on any cone whose group moves
some rows.

**Theorem A23 (liveness up to symmetry).** An *annotated run* of the quotient is a sequence of
stored states `r_n`, concrete successors `t_n` (a step of `r_n`, or `t_n = r_n`, a stutter) and
permutations `σ_n ∈ G_{t_n}` with `r_{n+1} = σ_n(t_n)`. The binding moves along it by the `σ_n`, and
so does each fair instance; the instance's sequence is its *thread*. A thread's instance is taken by
the step `r_n → t_n`, so an orbit self-edge (`t_n ≠ r_n`, `σ_n(t_n) = r_n`) is a move. Assume:

- every step from a state of a set `W` commutes with every `g ∈ G_s` and keeps `g` in `G_t`, where
  `W` holds the initial states and is closed under steps and under every `G_s` (Theorem A18 gives
  this for the states whose absent rows hold base values), and (G3);
- `g` maps a label of a fair instance to a label of its image;
- the trigger and the stop rule read the same on `s` and on `g(s)`, with the binding moved by `g`,
  for `g ∈ G_s`, and so does the goal on every state at or after a state where the trigger holds.

Then a property holds for every binding under `Fair_P` and the stop rule iff no annotated run of
the canonical quotient violates it for any binding while it is thread-fair and stutters forever
only where the rule allows. The stop rule of `WfNext`, `¬Moves`, is one the group preserves. With
finitely many reachable states and every `G_s` finite, the same holds with annotated runs replaced
by annotated lassos.

*Proof sketch.* A run lifts through the accumulated frame: `c_0` is an initial state with
`r_0 = θ_0(c_0)`, `c_{n+1} = θ_n⁻¹(t_n)` and `θ_{n+1} = σ_n θ_n`. Then `c` is a behaviour, and each
step `c_n → c_{n+1}` is the step `r_n → t_n` read through `θ_n⁻¹`. A behaviour projects to the
canonical run through frames that store each `θ_n(c_{n+1})` as `can`'s member of its orbit: `θ_0`
takes `c_0` to `can(c_0)`, and `θ_{n+1} = σ_n θ_n` with `σ_n` the permutation that takes
`θ_n(c_{n+1})` to its canonical form. Each `g ∈ G_s` preserves D20's enabledness and taking, so
the thread of `θ_0(f)` is fair iff `f` is fair on `c`. On a cycle of the quotient the group is
constant: a step only enlarges `G_s` (a slot leaves `absent` only), a stored target has the group
of its concrete successor, and the cycle returns to its start. So the composite `π` of the
cycle's permutations lies in `G_r`, some power `π^d` with `d > 0` acts as the identity, and `d`
copies of the cycle close the concrete lasso. On the cycle a thread is periodic. It is fair iff,
within one period, it takes its instance on some edge or meets a stored state where the instance
is not enabled. `can` need only pick a member of the orbit, so the theorem holds past
`BRANCH_CAP` too. Commutation on reachable states alone is not enough: a stored state is an image
`g(s)` of a reachable `s`, which need not be reachable when `g` was not in the group of an earlier
state on the path to `s`.

*The search.* `explore/orbit/liveness.rs` does not enumerate lassos. It accepts a strongly
connected component of the goal-free (state, binding) pairs, reached from a failing trigger pair,
when the component has a cycle, is expanded, and every piece of its threads takes its instance on
some edge or meets a pair where it is not enabled. The pieces are the components of the thread
graph over the whole component, with the direction of its edges forgotten. This is the analogue
of N9 for the annotated quotient, and it decides the property:

- A thread edge inside a component can be walked back. The edge and a path of pairs back close a
  walk whose composite permutation lies in the group of its first state, and some power of it
  acts as the identity. So a piece is strongly connected, and from every node of a piece a path
  of threads reaches its covered node.
- (⇐) In an accepted component, a loop that walks the thread of each instance enabled at its
  first pair to a covered node, one instance after another, covers every thread in each round.
  The stored path, the path to the component and that loop repeated are a thread-fair violating
  annotated run, and the lift carries it to a fair violating behaviour.
- (⇒) A fair violating lasso, seen through frames that stay put on a stutter, is a sequence of
  pairs that repeats. It settles at a stop pair, or its repeated pairs lie in one component with a
  cycle. A piece of that component with no covered node has a node over each of its pairs, so the
  thread of the lasso through it would be enabled forever and never taken.

The code builds the pieces from the nodes of enabled instances only, with union-find, and marks a
node that an edge joins to a node whose instance is not enabled; that is the same criterion. The
rule per piece on a single cycle is `threadFair_iff_pieces`.

*Evidence for A23.* P-sketch. M:
- The Lean proof `proofs/lean/RdraIsh/SymmetryLiveness.lean::a23` (`a23_iff` on runs, `a23_on`
  for commutation on a set `W` as above), with `lasso_closure` and `threadFair_iff_pieces`.
  - `ExampleSymLive` discharges every hypothesis on two interchangeable instances. Its quotient
    cycle has the swap as its composite and closes after two copies.
  - `ExampleReachOnly` is a system with commutation on its reachable states whose canonical
    quotient reaches a state its plain run does not.
  - `proofs/lean/RdraIsh/SearchSym.lean` proves the search above: it accepts iff some binding has
    a fair violating behaviour (`oAccepts_iff`), iff N9's search on the plain system accepts
    (`oAccepts_iff_accepts`), so its verdict is `a23`'s quotient side
    (`not_oAccepts_iff_lassos`). It adds one hypothesis to `a23`'s: the fair instances enabled
    at each state lie in a list. With the commutation only on the set `W`
    (`SymOn`), the search runs on the system restricted to `W`, which holds every stored state
    and has the plain system's edges there, and its verdict is the plain system's
    (`liveHolds_iff_not_oAccepts_on`, `oAccepts_on_iff_accepts`).
    `proofs/lean/RdraIsh/SymActSearch.lean::orbit_verdict` composes this with A18 from (G1)–(G6):
    on the slot-level model the orbit search's verdict is the plain one, with no commutation
    assumed outside `W`. `TEdge.reverse` walks a thread edge back, and `threadsCovered_iff`
    equates the code's union-find with the pieces. `SearchExample.lean` runs the search on
    `ExampleSymLive`.
  - The proof covers liveness and not the entity-cone findings, and does not read the code.
- The twins `Twin::OrbitLiveness` and `Twin::OrbitStateModels`. `Twin::OrbitLiveness` compares
  with the plain run's search, so it checks the quotient, not the fair-component rule, as its
  description says.

D: corpus rule 8,
`core/tests/reduce.rs::an_entity_cone_up_to_symmetry_decides_what_its_plain_run_left_open` (main
only).

**`DeletedRowOwner` (A19).** `Earlier` binds a row by its run mark whatever its existence, so a later step
of a split use case reads the key of a row an earlier step deleted. If a delete reset the key to its
base value (slot 0), that value would not move with `g` while the parent it named did, and the step of
Theorem A18 would fail for every `g` moving slot 0. (G5) removes the case. D: corpus
`sym_deleted_row_owner`.

**Counterexamples.** The stored path `r₀ → … → r_k` lifts forward: `c₀ = r₀`, `θ₀ = id`; for each stored
edge recompute the move from `r_i`, take the first successor `t` that canonicalizes to `r_{i+1}` with
`σ_i(t) = r_{i+1}`, and set `c_{i+1} = θ_i(t)`, `θ_{i+1} = θ_i ∘ σ_i⁻¹`. By Theorem A18 each concrete step
is a step of the system, and each `θ_i` fixes the absent slots of `r_i`, so creations replay under the
lowest-absent-slot rule. `explore/orbit.rs::replays` replays the lifted trace from an initial state
and returns the `ReplayError` if it does not replay, which the run reports as a tool error
([native.md, N16](./native.md#counterexamples-what-is-checked-before-printing)).
`explore/orbit.rs::lift_path`, which the safety, `always` and deadlock lifts share, returns
`ReplayError::NoMove` when a stored edge has no move of its source whose successor
canonicalizes to the stored target. The lasso lift of `explore/orbit/liveness.rs`
(`Product::lift`) returns `ReplayError::NoMove` when an edge is not the move at its place or its
permutation does not take that move to the stored target, which `lasso` reports as
`LassoFault::Replay` (test
`explore/orbit/liveness.rs::an_edge_its_permutation_does_not_explain_is_a_replay_error`).

**Composition with the reduction (A22).** On a reduced cone, `R` runs up to `G_T`, which leaves out every
entity `Reduction::fixed_why` names (each `S` entity, each entity holding an `S` variable, an input, or a bit
key, each parent such a key names, and each entity an `S` action binds). Every `g` then fixes the
representative and the class tables' keys, so Theorem A18 applies to `R` as a system. The run of `S` up to
`H` leaves out the entities `Extended::fixed_why` names, so every `h ∈ H` fixes `obs` and the inputs; the
`≈_H`-classes are then a label-preserving bisimulation of `X`, finer than (B1)–(B3)'s coarsest partition,
so `quotient_blocks` on the orbit graph yields unions of orbits with the same blocks. A silent orbit
self-edge stands for a concrete cycle inside its class and marks it (Definition 5, case 3). P-sketch; R:
`assert!` that `H` moves no read variable; M: `Twin::SUpToH`, `Twin::ReducedOrbitRun`; D:
`core/tests/reduce.rs::reduced_cones_up_to_h_decide_what_the_cones_decide`,
`cli/tests/engine_corpus.rs::each_h_reason_a_case_shows_has_a_witness` (shadow builds only).

*Evidence for A18.* P-full given (G1)–(G6) and the two facts of the builder. M: the Lean proof
`proofs/lean/RdraIsh/SymActions.lean::step_comm` derives the commutation on `W` over a slot-level
model of the cone system. In that model (G1)–(G4) and (G6) are conditions on the syntax of guards
and effects, as `breaks.rs::scan` states them, (G5) is the semantics of a delete, and the two
facts of the builder are hypotheses. `symOn` gives the `SymOn` that A23 assumes, and
`a18_holds_iff` the verdict of an invariant `G_s` preserves on the canonical quotient. The same
model, read as an action model (`proofs/lean/RdraIsh/SymActModel.lean::model`), meets the
locality `Local` that Lemmas 5 and 6 assume (`local_`). The model
reads an action's effects in list order, where `each_successor` applies the deletes and then the
choices last. It does not model owner `Transition`s, which the scan leaves out with `OwnerChoice`.
It covers neither the reduced cone nor the extended sub-lifecycle (A22). Kani: the harness
`explore/symmetry/mod.rs::apply_of_a_composition_is_the_composition_of_applies` proves that `apply` is a group
action on a 2-slot parent and a 2-slot child; it does not call `canonical`, and no Kani harness
covers the canonical form. `Twin::Canonical`,
`Twin::OrbitRun`, `Twin::ReducedOrbitRun` (up to 200,000 states). D: corpus `sym_*` cases, rule 8;
for (G6) outside guards, `explore/symmetry/g6.rs::reads_outside_guards_meet_no_absent_owner`.

---

## Bound escalation

<!-- derived-from ../formal-verification.md#proofs-for-any-number-of-rows -->

**What the code does (A25).** `cli/src/verify/escalate.rs::escalate` takes each state rule `i` that the prover
left unproved and that BFS passed at `β` (`passed_at`: a complete run of its cone with no violation of
it other than ones its recheck found to hold, or a cone covered by one that passed, and not a run up to
symmetry). For `k = 1, 2` it builds `T_{β+k}(M)` with `grown` (every count plus `k`, `None` past 255) and
keeps it only when `same_system` holds (no build error, the same entities, variable count, and rules by
name and site), and explores the rule's cone reduced (`explore_targets_reduced`). A counterexample its
cone cannot vouch for with `slots` (a free trigger, or more rows of an entity than the cone counts) is
decided again by `recheck_on(…, true, …)`, the refinement loop of [Safety cones](#safety-cones). It
prints `ok at n`, `counterexample at n`, or `not decided at n`, where `n = cone_instances` is
`cli/src/state_findings.rs::Instances::of(restrict_to(cone))`, the slot count of each entity of the cone that
decided (the rule's cone, or the last cone of the recheck), printed as one number when every entity
has it and else per entity, as `Loan=4 Member=3`.

**Proposition A25.** A `counterexample at n` is a reachable violation of `T_{β+k}(M)`.

*Proof.* The printed counterexample is one the cone that found it vouches for with `slots`: its trace
runs no use case without its event and creates no more rows of an uncounted entity than `β + k`
allows (`Trace::vouched(true)`), so it lifts to `T_{β+k}(M)` itself by Corollary A28′. The run reads
the reduced cone, and Theorem A8.1 says that a valuation of `T_v ∪ obs` breaking the rule is reachable
in `R` iff it is reachable in `C`, so a violation found on `R` is one of `C`. The printed trace is the
lift of A16, checked step by step against `C` in every build. ∎

Under `--instances E=k`, `n` gives each entity's own count, not a maximum over the cone's
entities
(`cli/tests/prove_cli.rs::escalation_names_each_entitys_instances_under_per_entity_bounds`).

**Memory.** Each escalated run is charged to `pool.with_room(min(pool.left(), GROWTH × peak))`
(`core-root/memory.rs::MemoryPool::with_room`), with `GROWTH = 10` and `peak` the store the rule's cone held at `β`;
`verify` always gives BFS a pool (`cli/src/verify/mod.rs` charges its caps to `MemoryPool::share_of`), so the
`None => caps` arm does not occur under `verify`. The bound is on the run's BFS state stores, a child pool of
BFS's share: the reduction's run of `S` draws on the same child pool
(`reduce/partition.rs` runs it under the caps it is given, with `without_views`, and checks it
for `MemoryCapReached`); the partition and the lift are outside it. The factor 10 is measured: on the engine corpus's escalated cases one
instance more multiplied the store by 1.1 to 6.4 and two more by 1.4 to 10.1 (`cli/src/verify/escalate.rs::GROWTH`'s doc).
The runs go one at a time after every other BFS and TLC run, so their stops do not depend on the
schedule. P: none (the statement is a description of the code); D: one unit test of `grown`,
and `cli/tests/prove_cli.rs::escalation_reports_no_counterexample_past_the_bound_it_prints` and
`::escalation_names_each_entitys_instances_under_per_entity_bounds` (both with z3).

---

## Caps, the memory limit, and `not decided`

<!-- derived-from ../formal-verification.md#memory-state-caps-and-threads -->

**Proposition A24.** Let a run of system `U` (a cone, reduced or not, or `T`) stop at a state cap, the
memory limit, or a time budget.

1. A state BFS admitted is a reachable state of `U`: BFS admits only initial states and successors of
   admitted states. So a safety violation found before the stop is a counterexample of `U`, with the
   same meaning (Theorems A1, A2, A28) as on a complete run.
2. No `ok` is printed: `cli/src/verify/bfs.rs::BfsCone::verdict` maps a `Capped` run with no violation of
   the whole system (`BfsCone::violations`) to `None` (`not decided`), and `decide_liveness` and `decide_liveness_reduced` return `Undecided` when the
   graph is incomplete and no counterexample was found.
3. A liveness lasso on an incomplete graph is built from expanded states only
   (`ComponentRun::expanded`: on an incomplete run, the states before the source of the last edge), whose
   successor sets are complete; it is replayed before it prints.

*Proof sketch.* (1) and (2) are the code paths named. (3) needs that every state with index below the last
edge's source was fully expanded, which holds for the one-thread BFS order; for the parallel BFS it is
theme (c)'s claim that the run computes the one-thread result. ∎

`not decided` means: the run made no claim either way. `BfsCone::verdict` reports a
counterexample found before the stop, and only the cone's other checks stay `not decided`, as
formal-verification.md says. That a stopped native run is not recorded as `ok` for the per-run
comparison with TLC is theme (d)'s (claim T2). D: `cli/tests/engine_corpus.rs::a_capped_graph_decides_nothing_the_full_graph_contradicts` (release jobs)
runs the whole model of every corpus case under every cap below 64 and at each twelfth of its
size, and decides each
`always`, `eventually` and `leads_to` there, never a state rule or `after` assertion, so
it tests the claim for properties only. For safety cones,
`cli/tests/prove_cli.rs::a_proof_leaves_an_after_assert_and_a_state_cap_undecided` (with z3)
shows (2): a cone stopped by the memory limit and one stopped by a state cap print
`not decided`, though z3 proves their rules. (1) for a safety cone stopped at a cap rests on
the code path alone; `cli/tests/verify_cli.rs::a_safety_cone_stops_once_every_check_on_it_has_failed`
(main only) checks a counterexample from a run that stopped early, once every check on it
failed, against TLC's whole run, which is a stop of another kind.

**A26 (what the memory limit bounds).** The limit is charged by the BFS state stores and, under
`--with-tlc`, the TLC heaps. The passes over a finished graph (liveness, `can_reach`, deadlocks, the
partition), the lift, and z3 are not charged. This is a statement of scope, not a guarantee, and
formal-verification.md, "Memory, state caps, and threads", says that runs were measured past the limit.
Evidence: none beyond the reading of `escalate` and the guide.

---

## Related work

- **Cone of influence.** The reduction of a model to the variables a property transitively depends on is
  standard (Clarke, Grumberg and Peled 1999, chapter on abstraction and the cone of influence; Kurshan
  1994 describes the related localization reduction). Theorem A1 is that construction for guarded actions with
  parameters, where the "dependency" relation must also cover slot allocation (freed entities) and event
  marks. Over-approximating cones (A2) are an existential abstraction in the sense of Clarke, Grumberg and
  Long 1994: they preserve universal safety, and their counterexamples need a check against the concrete
  system, as in counterexample-guided refinement (Clarke et al. 2000); A28 is such a check for freed slots,
  and `recheck_on` is a refinement loop for left-out marks.
- **Stuttering equivalence.** Browne, Clarke and Grumberg 1988 show that stuttering-equivalent finite
  structures satisfy the same CTL\* formulas without next; De Nicola and Vaandrager 1995 relate the
  divergence-blind and divergence-sensitive variants and their logics; Groote and Vaandrager 1990 give the
  partition refinement that `quotient_blocks` follows. The reduction of A8 quotients only one component
  (`S`) of a product and keeps the other (`T`) concrete, closer to compositional minimization than to
  minimizing the whole state graph; the `_WF` treatment (A15) is a direct answer to divergence blindness
  under a fairness assumption that is not stutter-invariant on the quotient.
- **Symmetry.** Emerson and Sistla 1996 and Ip and Dill 1996 reduce a state graph by a group of
  permutations of process indices, store one representative per orbit, and lift counterexamples; Emerson and
  Sistla 1997 treat fairness under symmetry, which is the setting of A23's annotated quotient. A18 differs in
  that the group depends on the state (`G_s` fixes absent slots, to respect lowest-slot creation) and acts on
  foreign keys by relabelling.
- **Partial-order reduction.** Godefroid 1996 and Peled 1993 reduce interleavings of independent
  transitions. The sub-lifecycle reduction targets the same product blow-up (`T` interleaved with an
  independent `S`), but by quotienting `S`'s states rather than pruning interleavings, which is why its
  correctness argument is a bisimulation and not an ample-set condition.
- **Fairness.** Weak fairness per action instance and `WF_vars(Next)` are those of TLA (Lamport 1994,
  2002); A4 and A5 use that `WF_vars(A)` restricts to a cone when `A` and everything it reads are inside,
  and that `WF_vars(Next)` does not. Machine closure (Abadi and Lamport 1991) gives the extension of a
  finite prefix used for `always` in A4.

**References.**

- M. Abadi, L. Lamport. The existence of refinement mappings. *Theoretical Computer Science* 82(2):253–284,
  1991.
- M. C. Browne, E. M. Clarke, O. Grumberg. Characterizing finite Kripke structures in propositional
  temporal logic. *Theoretical Computer Science* 59(1–2):115–131, 1988.
- E. M. Clarke, O. Grumberg, D. E. Long. Model checking and abstraction. *ACM TOPLAS* 16(5):1512–1542,
  1994.
- E. M. Clarke, O. Grumberg, S. Jha, Y. Lu, H. Veith. Counterexample-guided abstraction refinement. *CAV
  2000*, LNCS 1855, 154–169.
- E. M. Clarke, O. Grumberg, D. Peled. *Model Checking*. MIT Press, 1999.
- R. De Nicola, F. Vaandrager. Three logics for branching bisimulation. *Journal of the ACM*
  42(2):458–487, 1995.
- E. A. Emerson, A. P. Sistla. Symmetry and model checking. *Formal Methods in System Design*
  9(1–2):105–131, 1996.
- E. A. Emerson, A. P. Sistla. Utilizing symmetry when model-checking under fairness assumptions: an
  automata-theoretic approach. *ACM TOPLAS* 19(4):617–638, 1997.
- P. Godefroid. *Partial-Order Methods for the Verification of Concurrent Systems*. LNCS 1032, Springer,
  1996.
- J. F. Groote, F. Vaandrager. An efficient algorithm for branching bisimulation and stuttering
  equivalence. *ICALP 1990*, LNCS 443, 626–638.
- C. N. Ip, D. L. Dill. Better verification through symmetry. *Formal Methods in System Design*
  9(1–2):41–75, 1996.
- R. P. Kurshan. *Computer-Aided Verification of Coordinating Processes*. Princeton University Press,
  1994.
- L. Lamport. The temporal logic of actions. *ACM TOPLAS* 16(3):872–923, 1994.
- L. Lamport. *Specifying Systems*. Addison-Wesley, 2002.
- D. Peled. All from one, one for all: on model checking using representatives. *CAV 1993*, LNCS 697,
  409–423.

---

## Threats to validity

- **Proofs about a reading of the code.** Every lemma models a Rust function by its doc comment
  and body. A function read only through its doc (named in its proof: `Fold::holds` for Lemma 6)
  may differ from it. A function can behave differently through a callee that its proof does
  not name. The proofs rest on that reading; the tests and twins named with each result are
  the only check of it against the running code.
- **Syntactic over-approximations.** `Wr` lists a deleted entity's owners although D8 keeps them.
  This only adds actions and variables to cones and makes owners non-monotone; every proof above
  tolerates it, and a change that made `Wr` an under-approximation would break Lemma 2. A debug or
  `shadow` build holds every step to it: `TransitionSystem::check_writes` panics when a step
  changes a byte of a variable `action_writes` leaves out, so every debug test that explores and
  CI's shadow job (the `writes` twin, on the samples and the corpus) check `Wr` against the steps
  they take. A release build does not check it, and a step no run takes is not checked.
- **Partition and Lemma 6.** A8, A15, and every reduced cone's verdict rest on (B1)–(B3), which
  every build certifies on the partition it uses (that the check gives (B1)–(B3) is proved in
  Lean over a model of it, `proofs/lean/RdraIsh/Sweep.lean::stable_of_cert`, given that the
  run is `X`'s reachable graph and that `stuttering_blocks` marks what the model marks); and
  on Lemma 6, argued over a reading of
  the code and proved in Lean only over an action model of the cone and `X`, from
  hypotheses read from the code. Its fold half is checked against the cone's step relation on the
  domain samples and the built models only; its parameter matrix is checked cell by cell on
  built models, with three reachable cells that no model fills; and its parameters on a stub
  also on two corpus cases. Among those verdicts are the printed `ok` of a property's `Plain` run (A5.1) and of a
  property on a cone that frees slots or leaves a mark out (A6).
- **Differential evidence compares implementations, not specifications.** The shadow twins
  compare two implementations of one definition and skip large inputs; a `shadow` build reports
  every skipped call, and the corpus harness fails a case on one except where
  `cli/tests/engine_corpus.rs::PAST_A_TWIN_CAP` names the run. The corpus and sample comparisons
  check agreement with TLC on the cones the code chooses; a soundness bug in the cone or the
  reduction that TLC's module shares (one `Reduction` value feeds both, A13) is invisible to
  them. Only `scripts/reduction-diff.py`, `core/tests/cone.rs`, `core/tests/reduce.rs`
  (`assert_reduction_exact_in` and the tests that call it) and
  `cli/tests/engine_corpus.rs::a_reduced_wf_next_run_decides_what_the_unreduced_one_decides`
  compare a reduced or restricted system with a less reduced one; none of them compares with a
  module TLC reads independently of the reduction.
- **Hypotheses checked per trace and per cone.** A check's cone may leave a mark out: its `ok`
  is sound by Theorem A2, and its counterexample counts only once vouched for (Corollary A28′),
  after the refinement of `recheck_on` where needed. That `free_triggers` lists every step that
  runs an action reading a mark outside the cone is a reading, not tested on its own, and that
  the exact cone's counterexample is vouched for is a `debug_assert!` only. A finding a cone
  fails to report has no trace, so a per-trace check never sees it; entity cones are exact for
  marks per cone instead.
- **Symmetry hypothesis (G6).** `Rule::AbsentOwner` enforces it for a guard's pre-state read
  through a `New` parameter, the one way a guard reaches an absent row; no sample or corpus cone
  fires the rule. For invariants, `after` assertions, properties and `can_reach` goals, which
  the rule does not scan, (G6) holds by the presence gates of their evaluators, whatever the
  predicate reads (Hypotheses, (G6)). That argument is a reading of the evaluators, tested on one
  built model (`explore/symmetry/g6.rs::reads_outside_guards_meet_no_absent_owner`); a new
  evaluator that read a predicate on an ungated binding would fall outside it, and no rule or
  run-time check would catch that.
- **Panics left in the lifts.** Every printed trace replays before it prints
  ([native.md, N16](./native.md#counterexamples-what-is-checked-before-printing)). What still
  panics in a release build on a lift's way, and the reason or assumption each panic rests on,
  is listed in the [claims inventory](./claims.md#panics-left-on-a-lifts-way).
- **Bounded results.** Everything here, escalation included, is a statement at a bound. `ok at n`
  at `β + 1` and `β + 2` adds two bounded results; it is not an induction.
