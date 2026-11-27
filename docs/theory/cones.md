# Cones of influence, reductions, and symmetry

<!-- derived-from ./semantics.md -->
<!-- derived-from ./claims.md#a-cones-of-influence-and-reductions -->

This document states, for rows A1 to A28 of the [claims inventory](./claims.md), when a run
on a smaller system decides what `T_β(M)` (D13) would: a cone of influence, a cone reduced by
an independent sub-lifecycle, a run up to slot permutations, and a run at more instances. It
uses the notation of [semantics.md](./semantics.md) and does not redefine it.

The code read is commit `fb6928c74ccc5b479dea07d92cc1e39c34ddd218` (origin/main,
2026-11-28). The other theory documents pin `b83b9544`; `git diff --stat b83b9544 fb6928c7 --
crates scripts` touches 30 files, all in codegen, the LSP, the plugin manifest and two
scripts, and none under `lifecycle/`, `verify/`, `property_cone.rs`, or `emit/tla`, so every
statement here holds at both commits. How the document changed after it was first written is
in [Errata](#errata). Paths are relative to
`crates/rdra-ish-core/src/lifecycle/` unless they start with `cli/`
(`crates/rdra-ish-cli/src/`) or `cli/tests/` (`crates/rdra-ish-cli/tests/`), as in
[semantics.md](./semantics.md). No tool was run for this document; every statement about
behavior is a reading of the code, and every measured number is quoted from the note or
pull request named beside it.

Each result has four parts: the statement, its hypotheses with the code that establishes
each one, the proof (marked **Proof** when every step is given, **Proof sketch** when a step
is named but not derived), and the evidence that checks it. A step that does not go through,
or a guarantee the code does not give, is listed as an open problem of the
[index](./README.md#open-problems) and named where it arises.

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
12. [Summary of evidence](#summary-of-evidence)
13. [Related work](#related-work)
14. [Threats to validity](#threats-to-validity)
15. [Errata](#errata)

---

## Notation added here

<!-- derived-from ./semantics.md#notation -->

`T = T_β(M) = (S, I, →)` throughout, with variables `V` (D2) and actions `A` (D6). For an
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
  owners, but `Wr` lists them (open problem P9 records the prose; the over-listing only adds
  actions to a cone).
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

`closed_cone` (`keep_writers(·, false)`, #261) is called only by the TLA+ printer
(`emit/tla/module.rs`), not by any verdict.

**Definition 2 (deviations).** Let `W = cone(σ, K)` and `C = T|W =
restrict_to(W, K)`. `W` has

- a *freed entity* `E` when some `a ∈ A_W` has a `New` parameter on `E` and `Exists(E) ∉ W`
  (`cone.rs::TransitionSystem::cone_frees_slots`);
- a *left-out mark* `μ` when some `a ∈ A_W` has `(μ, R_μ) ∈ Mk(a)` and `μ ∉ W`
  (`trace.rs::TransitionSystem::free_triggers` names the use cases a trace runs this way);
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
`analysis/effect.rs::copyable` admits a copy only between columns with the same values and a
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

*Hypotheses and code.* Exactness: `cone_frees_slots` is false and `free_triggers` of every
trace is empty, the latter checked per trace, not per cone. The builder facts of (iv):
`build/sets.rs` and `analysis/effect.rs::copyable`.

*Evidence.* P-full (this section). D: `core/tests/cone.rs::cone_exploration_equals_the_projection_of_the_whole_system`
compares the cone's reachable set with the projection of the whole system's;
`::a_parent_existence_joins_the_cone_without_its_columns` checks (C4). The comparison
(`core/tests/cone.rs::assert_cones_exact`) separates only the cones that free slots: it asserts
equality for every other cone, a cone with a left-out mark included. It therefore shows that no
cone of its samples has a left-out mark that changes the reachable set; it does not test the
exactness condition of A1 against a cone that has one.

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
has a property. `::Deadlock` says that no path leaves a state. An over-approximating `C`
reaches more states and edges than `π_W(T)` does, so it can satisfy such a check that `T`
fails, and it can also fail one that `T` satisfies. Those checks need exactness (Theorem A1),
or A8's bisimulation on a reduced cone. Their soundness on a cone with a left-out mark is open
problem P32, below.

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
`cli/state_findings.rs` prints `note: … creates more E instances than the instance bound
allows`. So a safety counterexample with an empty `over_bound` is a reachable violation of
`T_β(M)`, and one with a non-empty `over_bound` is one of `T_{β'}(M)`.

**Left-out marks have no lifting statement.** A cone leaves `μ` out when some writer of `μ` is
not taken; then `restrict_to` drops the handler's guard on `μ` and its `Triggered` parameter, and
`C` runs the handler without the event. Nothing bounds the bound at which `T` would allow it:
the event order (D11) can forbid the run at every `β`. Such a counterexample is a candidate
only. The code treats it as follows.

- `verify/bfs.rs::BfsCone::verdict` prints it as `counterexample` (and fails the run), with the
  trace's `note: … runs UC without Ev having been raised`.
- `verify/refuted.rs::refuted` marks the rule `refuted` and skips its proof.
  `verify/escalate.rs::escalate` prints it as `counterexample at k instances` and fails the run.
  Neither reads `free_triggers` or `over_bound`.
- `property_cone.rs::counterexample` and `explore/properties.rs::TransitionSystem::decide_property`
  re-decide a counterexample on `T` only when `cone_frees_slots`; a property cone whose
  non-fair actions leave a mark out reports its counterexample as `counterexample`.

This is open problem P1, confirmed at all four sites. For a universal check P1 yields no
wrong `ok`. A state rule, an `after` assertion, or a property passes on `C` only if it passes on
`T` (Theorems A2 and A4.1). `refuted.rs` and `escalate.rs` also ignore `over_bound`, so
`at k instances` can understate the bound a freed-slot counterexample needs (P11).

**Entity-cone findings on a cone with a left-out mark (P32).** For the existential and
branching findings of an entity cone, a left-out mark can produce a wrong `ok`.
`targets.rs::entity_findings` and `findings.rs::state_model_findings` decide `NoWitness`,
`Deadlock`, `NeverRuns`, arms never taken, and values never chosen on the entity's cone, as
formal-verification.md, "State models with TLC alone", says. Neither function reads
`free_triggers`, `over_bound`, or `cone_frees_slots`. Outside `cone.rs`, which defines
`cone_frees_slots`, the code reads those three names only in `property_cone.rs`,
`explore/properties.rs`, `verify/jobs.rs`, `trace.rs`, and the trace printers
(`cli/state_findings.rs`, `rdra-ish-emit`'s `state_pattern.rs`); `verify/tlc.rs` (the explored
jobs) reads none of them, only the `frees_slots` field that `verify/jobs.rs` sets from
`cone_frees_slots`. A left-out mark lets `C` run a handler
that `T` holds back, and that adds edges and states. An added edge can leave a state that is a
deadlock of `T`. An added state can witness a lifecycle state that `T` never reaches, or run a
use case that never runs in `T`. In each case the finding is missing. A missing finding has no
trace, so the per-trace `free_triggers` note never appears. `NeverRuns`, arms, and values are
combined over every entity cone that keeps the action (`targets.rs::RunsSoFar`), and one cone
that fires the action clears the finding. So a single cone with a left-out mark can hide the
finding for the whole run.

The following model is constructed by hand and has not been run. Order's exit `ShipOrder`
reads `Payment.status == paid`. `ConfirmPayment`, triggered by `EvPaid`, writes `Payment.status`.
The raiser `ReadNotice` writes only Notice and Payment marks and starts from a state that is
never reached. Order's cone takes `ConfirmPayment`, because it writes the Payment status that
`ShipOrder` reads. `ConfirmPayment` is not in `K`, since it writes no Order parameter. `A_W`
does not hold `ReadNotice`, since it writes nothing in `W`. So the mark `EvPaid` is not exact
by (C2) and is left out.
In `C`, Payment reaches `paid`. Order's deadlock and the `NoWitness` of `shipped` disappear.
Payment's own cone may still report its `NoWitness`, so the run as a whole can stay red, but the
verdict on Order is wrong.

A freed entity gives a weaker version of the problem. By Proposition A28, the cone's findings
are those of a larger bound, so the result depends on the bound, not on the event order.
Problem P32 is being fixed in a separate change. This document records the problem as it
appears at the pinned commit.

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

*Evidence.* P-full (by reading `count_up`, `count_down`, and `plain_reads`); D: `core/tests/reduce.rs::a_counted_mark_keeps_its_raiser_and_handler_out_of_the_sub_lifecycle`.

**Proposition A27 (covered cones).** `ok (checked in Cone_X)` (D23) is printed for a safety cone
`C_s = T|W_s` when `verify/tlc.rs::Covered::covers` holds against a cone `C_ℓ = T|W_ℓ` that
passed: the checks of `C_s` are among those of `C_ℓ`, `W_s ⊆ W_ℓ`, `K_s ⊆ K_ℓ`, and every action
of `C_s` keeps, in `C_ℓ`, as many guards and parameters as in `C_s` (`ConeSystem::steps`). Then
`T ⊨ R` for every check `R` of `C_s`.

*Proof.* `Covered::covers` requires every check of `C_s` to be among the checks of `C_ℓ`'s module,
and `C_ℓ` passed. So `C_ℓ ⊨ R` for each such `R`, and `T ⊨ R` follows from Theorem A2 applied to
`C_ℓ`. For an `after` check, the inclusions `K_s ⊆ K_ℓ` and `W_s ⊆ W_ℓ` keep its checked steps
in `K_ℓ` and their writes in `W_ℓ`, as A2 requires. `cli/verify/jobs.rs::cone_jobs` merges the
kept sets of the targets that share `W_ℓ` into one `K_ℓ`. Lemma 2, which A2 rests on, uses only
(C1) and (C4) for the actions of `A_{W_ℓ}(K_ℓ)`. `A_W(K ∪ K') = A_W(K) ∪ A_W(K')`, so these hold
for the merged set. ∎

The step-shape condition is not needed for the verdict. It is what makes `covers`' own
argument hold, `Reach(C_s) ⊆ π_{W_s}(Reach(C_ℓ))`. `ConeSystem::steps` maps each action of
`restrict_to(W, K)` to the pair `(guards.len(), params.len())`. The guards and parameters
`restrict_to` keeps are monotone in `W` (What `restrict_to` keeps), so `W_s ⊆ W_ℓ` makes the
sets `C_s` keeps subsets of those `C_ℓ` keeps. Equal counts therefore mean equal sets. No guard
that `C_ℓ` keeps for such an action is a copy guard with a target outside `W_s`, so the
dropped-copy case does not arise.

*Evidence.* P-full relative to the covering cone's `ok`, because the proof reduces to Theorem
A2. `cone_jobs` reduces `C_ℓ` (`reduced_cone_in`), and when it is reduced its `ok` rests on
Theorem A8 (P7, P31), as every reduced cone's verdict does. No test names A27. The claims
inventory had no row for this verdict, so A27 is added.

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
   `C`. `property_cone.rs::PropertyCone::bundle` prints `ConeRun::Plain` from `self.reduced`.
   `verify/native.rs::native_property` runs `p.cone.reduced`, and `Plain::of` asserts
   (`shadow_assert!`) that its run is of `cone.reduced`. So the implication is Theorem A8.4
   (`R ⊨ P ⇔ C ⊨ P` under `Plain`), then Theorem A4.1, then D21. A8 is a sketch: it assumes
   (B1)–(B3) (P7) and Lemma 6 (P31). When nothing reduces, `reduced` is `C` itself and A4.1
   alone suffices.
2. `Cone_P_WF` (`WfNext` on `C`) counterexample, `W` exact ⇒ `T ⊭ P` under `WfNext`.
3. `Cone_P_WF` `ok` and `whole` ⇒ `T ⊨ P` under `WfNext`. On a reduced cone, the first
   `WfNext` run reads `R` (`decide_liveness_reduced`), and its `ok` is `C`'s by Theorem A15,
   which assumes A8's hypotheses. Otherwise the run reads `C`
   (`verify/native.rs::native_property_after`).

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

`property_cone.rs::after_plain` and `::after_wf` implement exactly these implications; the
`debug_assert!(!whole || !frees_slots)` in `PropertyCone::under` is the second sentence of (3).
When neither run decides, `RdraSpec_P` runs on `T`, or the verdict is `not decided`.

**Proposition A6.** On a cone with a freed entity, `property_cone.rs::counterexample` maps every
counterexample to `Undecided(Why::FreesSlots)`, and `explore/properties.rs::decide_property` runs the
explored forms (`stable`, `unchanged`, `precedes`, and `always`) again on `T` after a violation. Only
`ok` is taken from such a cone. The argument depends on the form:

- `stable`, `unchanged`, and `precedes` read the unreduced cone, so their `ok` is sound by
  Theorems A4.1 and A2. Both engines run them there. `verify/jobs.rs` builds their explored
  jobs on `restrict_to(cone)` (`LtlKind::is_explored`). `verify/bfs.rs::explored_properties`
  calls `decide_property`, which also uses `restrict_to` for every form except `always`.
- `always` reads the reduced cone (`decide_property` calls `reduced_cone` for `Ltl::Always`). So
  do `eventually`, `leads_to`, and `always` in `verify`, through the `Plain` run of A5.1. For
  these forms the `ok` is sound by Theorem A8 (points 1 and 4), then A4.1 and A2. A8 assumes P7
  and P31.

The same treatment is not given to a left-out mark (P1).

*Evidence for A4 to A6.* Theorem A4, a statement about the unreduced cone `C`: P-full, given
Lemmas 2 to 4. A5.1, A6, and A5.3 on a reduced cone: P-sketch, because the printed verdict is
read on the reduced cone and rests on Theorems A8 and A15 (P7, P31). A5.2: P-full. D:
`cli/tests/engine_equivalence.rs::a_property_cone_decides_what_the_whole_model_decides`; corpus rules 1
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

*Monotone* is `split.rs::TransitionSystem::monotone_vars`: the write edges of the variable, as
pairs of values over all effects of `C`, form no cycle other than self-loops, and the variable is
not a counter and not a column or owner of a deleted entity.

**Lemma 5 (split).** (a) Every `S_a` action writes only `S_v`. (b) Every `T_a` action writes only
`T_v` and reads `S_v` only in `obs`. (c) Every read of an `S_a` action is in `S_v`, `inputs`, or a
fold. (d) Every input is monotone. (e) Every seed's read of `S_v` is in `obs`, and every kept action
is in `T_a`.

*Proof.* (a), (b), (e) are the closure conditions of `split_at` and the definition of `obs`. For
(c), an action reading a non-monotone `T_v` variable joins `T_a`, so an `S_a` action reads in `T_v`
only monotone variables, which become inputs unless a fold takes them. (d) is checked at run time:
`split_with` asserts `inputs.iter().all(monotone)`. ∎

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
corresponds to, where `bits(t)` is `Reduction::push_bits`.

**Lemma 6 (the extended system covers the cone).** For every `(t, s) ∈ Reach(C)`: `x(t, s) ∈ X_n`
(up to the inner reduction's canonical form); `(t, s) → (t, s')` is an `S_a` step of `C` iff
`x(t, s) → x(t, s')` is a silent edge of `X`; and a `T_a` step `(t, s) → (t', s)` that changes the input
bytes is matched by a sequence of rises from `x(t, s)` through the stages to `x(t', s)`.

*Proof sketch.* Induct on reachability. Initial states: `X`'s initial states are those of
`restrict_to`, which projects `I_C` (Lemma 2's argument); a bit starts where the cone starts
(`fold_initial`). An `S_a` action reads only `S_v`, inputs, and folds (Lemma 5c), and its guards on
those are kept by `restrict_to` with havoc off, so it has the same successors on `S_v` in `C` at
`(t, s)` and in `X` at `x(t, s)` provided each folded guard equals its bit at `bits(t)`. A
`T_a` action that writes an input keeps in `X` only its guards on `S_v` and inputs, which hold where
they hold in `C`; it writes the raw inputs, and each changed bit slot then has a free flip. Two steps
are named and not derived here: that `Fold::holds` computes the folded guard exactly (the
`Twin::FoldHolds` twin compares it with `reduce/fold/reference.rs`, an implementation of the same
definition), and that a fold's key binding (`Extended::cone_params` binds a dropped `T` parameter to
slot 0) loses no successor. ∎

cone-soundness.md, "Proof obligation", uses Lemma 6 in points 2, 3, and 5 without stating it.
Open problem P31 records the gap.

**Theorem A8 (the reduced cone is stuttering equivalent to the cone).** Assume (B1) to (B3) for
`B` and Lemma 6. Let `(t, s) ~ (t, ρ)` when `ρ` is the representative of the block of `x(t, s)`.
Then `~` is a divergence-blind stuttering bisimulation between `C` and `R` that preserves `T_v`,
`obs`, and the action label of every `T_a` step. Consequently `C` and `R` agree on:

1. which valuations of `T_v ∪ obs` are reachable (invariants, `always`, witnesses, vacuity);
2. which `T_a` action labels an edge from a state with given `T_v ∪ obs` values (`NeverRuns`, arms
   taken, `after` assertions on `T_a` actions);
3. whether from every reached state some path takes an edge that changes given `T_v` values
   (deadlocks);
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
   (that `Fold::holds` computes it exactly is one of the two steps Lemma 6 names and does not
   derive). The monotone rule is not used here: it bounds the input histories, which keeps the
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

*Hypotheses and code.* (B1)–(B3): `reduce/partition.rs::quotient_blocks` (open problem P7, below);
Lemma 5: `split_at`, `split_with`; Lemma 6: `Extended::build`, `reduce/fold.rs`; the checks a
reduced cone keeps: `drop_unreadable_checks` (A14).

*Evidence.* P-sketch (Lemma 6 and B1–B3 are assumed). M: `Twin::Partition` (worklist against
sweeps), `Twin::ClassMoves`, `Twin::FoldHolds`, the `shadow_assert!` that a steady action leaves the
inputs. D: `core/tests/reduce.rs::reduced_cones_decide_what_the_cones_decide`,
`::reduced_domain_cones_decide_what_the_cones_decide`, `::a_violation_behind_the_sub_lifecycle_lifts_to_a_longer_real_trace`;
`scripts/reduction-diff.py`; `cli/tests/engine_equivalence.rs` on reduced cones.

**On (B1)–(B3) (P7, confirmed).** `quotient_blocks` starts from the blocks of `obs` and inputs
(B1), refines with `Stutter::blocks`, then splits by the set of blocks each state reaches by one
rise and refines again, until a pass splits nothing. The reference twin
(`reduce/partition/reference.rs`) computes the same definition by sweeps: every block a splitter in
each sweep, the loop ending only when a sweep splits nothing, so its output satisfies (B2) and (B3)
when it returns. What is not checked is that the worklist computes the same partition outside
test builds: a `shadow` build compares the two up to 2,500,000 states (`SHADOW_CAP`), a release build
never, and no check of (B1)–(B3) on the returned partition exists. P7 holds as stated; this narrows
it to "worklist = sweeps" plus the sweeps' own correctness, which is argued here by inspection only.

**A9 to A14 and A16, A17.**

- **A9** (worklist = sweeps): M (`Twin::Partition`, skipped above 2,500,000 states) and the test
  `reduce/partition.rs::the_worklist_partition_is_the_partition_of_repeated_sweeps` on random graphs.
- **A10** (monotone inputs): Lemma 5c–d and point 5 of Theorem A8. Monotonicity is not used in A8; it
  keeps `X` finite and small. It is used in A15 (case 3 and `s_cycles`).
- **A11** (folds): Lemma 6, with the fold equivalence twin-checked. A count is never node-folded
  (counting-rules.md); not re-read.
- **A12** (counters on `T`): `split_at` seeds `T_v` with every counter, and `monotone_vars` marks
  counters cyclic, so no counter is an input. P-full by reading.
- **A13** (one reduction for both engines): `reduced_with` rejects a reduction past
  `TLA_CELL_BUDGET` (530,000 cells, `Reduction::tla_cells`) and an identity quotient
  (`Classified::Nothing`, `Reduction::shrinks`); the one `Reduction` value feeds the native checker and
  the TLA+ printer. True by construction.
- **A14** (dropped checks have their own cones): by Lemma 5e a target's own check reads only `T_v ∪
  obs`, an `after` target's steps are kept and so in `T_a`, and a property's fair actions are kept. So
  `drop_unreadable_checks` drops only checks that ride along in the cone system and are not its
  targets; each has its own target and cone. `Covered::covers` requires the covered checks to be in the
  covering module, so a dropped check is never covered by a cone that dropped it. P-full by reading;
  no test targets it.
- **A16** (lifted traces): `reduce/lift.rs::TransitionSystem::lift` replaces each class move by the
  silent path (B2) promises, recursively through nested reductions, and the violation is re-read on
  the lifted last state. The lifted `T` steps are checked against the cone's moves by `debug_assert!`
  only, and an unfolded `S` step is not checked at all (P20, confirmed).
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

**Candidates.** A counterexample of the run is lifted (`lift.rs::lift_stop`, `::lift_cycle`) to `C`,
replayed there (`replay_lasso`), checked to fail `P` (`explain_property`) and to pass `unfair_loop` under
`WfNext`; otherwise it is `ReducedVerdict::Spurious` and `verify/native.rs::native_property_after` runs
`Cone_P_Full` and `Cone_P_WF` on `C`. `lift_cycle` gives up after `MAX_COPIES` (4,096) copies of the loop,
which also yields `Spurious`. A printed counterexample of this run is therefore a lasso of `C` that the
checker certified. Whether it is a counterexample of `T` is Theorem A5.2 (needs `W` exact).

*Evidence.* P-full relative to (B1)–(B3) (P7) and Lemma 6 (P31). R: lift, replay, and fairness check of
each candidate. D: corpus cases `cone_reduced_class_loops`, `cone_reduced_inner_class_loops`,
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
- (G6) No predicate reads the owner of an absent row. *Not checked by the scan* (P30).

**Theorem A18.** Under (G1)–(G6), for every reachable `s`, every `g ∈ G_s`, and every step `s →_{a,p} t`:
`g(s) →_{a,g(p)} g(t)` is a step and `g ∈ G_t`. Hence `≈ = {(s, g(s)) : g ∈ G_s}` is an equivalence and a
bisimulation, and a breadth-first search that stores one member of each `≈`-class reaches exactly the
classes of `Reach(T)` (or of the cone it runs on), each at its least depth.

*Proof.* Absent slots of `t` are absent in `s` (a slot leaves `absent` only), so `g` fixes them and
`g ∈ G_t`; `G_s = G_{g(s)}` since `g` preserves the absent set, so `≈` is reflexive, symmetric, and
transitive. *Bindings.* `New` takes the lowest absent slot, which `g` fixes; `Existing`, `Parent`, and
`Triggered` admit a present slot, `Earlier` a slot by its run mark, and `g` preserves presence and moves
marks with their slots. *Predicates.* A column or mark of slot `i` in `s` is the same value at slot
`g(i)` in `g(s)`. By (G1) a row id is compared only by `=`/`≠` with a row id of its class, which a
bijection preserves, or an optional owner with `null`, which relabelling preserves; an owner read on a
non-absent row is relabelled consistently with the row ids it is compared with (G5 for deleted rows),
and by (G6) none is read on an absent row. `Forall`, `Exists`, and `Count` range over the present slots,
a set `g` permutes. So every guard of `a` has the same value at `(s, t, p)` and `(g(s), g(t), g(p))`.
*Effects.* Each effect commutes with `g`: a literal write is unchanged; an owner written from a
parameter's slot is relabelled with that slot (G2); a create or delete moves with its slot; a `Choose` or
`Transition` on an owner ranges over all rows (G2), so its set of forks is closed under `g`; (G4) keeps
an unrelabelled base owner off non-absent rows. So `g(t) ∈ Post_a(g(s), g(p))`. The converse is the
same statement for `g⁻¹ ∈ G_s`. With (G3) the initial classes are the classes of initial states. A
bisimulation that relates states at equal depth (it maps each step to one step) preserves BFS depth. ∎

Leaving more classes out gives a subgroup of `G_s`, and the statement holds for every subgroup, so
the scan can always leave a class out and lose only reduction (**A20**, P-full). A *witness*
shows that a rule is needed. It is a model on which dropping the rule (`symmetry/mod.rs::dropped`,
set per test or by `RDRA_ISH_DROP_RULE` in a `shadow` build) breaks the bisimulation or the
verdict. `symmetry/mod.rs::Rule` has ten variants, and the witnesses are as follows.

| Rule | Witness |
|---|---|
| `OrderedId`, `IdAgainstLiteral`, `MixedIds`, `OwnerLiteral`, `OwnerChoice`, `UnlinkedCreate` | `explore/symmetry/witnesses.rs::*_has_a_witness`, one unit test each |
| `JointTooLarge` | none needed. `witnesses.rs::joint_too_large_is_a_cost_cap` (release builds only) drops it and shows that the quotient is still a bisimulation with the plain run's verdicts |
| `Observed` | 2 of its 6 reasons, `obs` and `input`: `cli/tests/engine_corpus.rs::each_h_reason_a_case_shows_has_a_witness`, in `shadow` builds only. The other 4 have none |
| `Reduced` | none for any of its 7 reasons |
| `PinnedStart` | none |

`explore/symmetry/snapshot.rs::KEPT` lists 11 rules and reasons that the owner kept on
2026-11-25 without a witness of their own: `PinnedStart`, six `Reduced` reasons, and four
`Observed` reasons. `Reduced:S` is not in that list, and no witness for it was found. The
snapshot reports that `PinnedStart` and every scan rule except the `Observed` and `Reduced`
reasons fire in no cone of the samples. A missing witness does not affect soundness, which
the subgroup argument gives. It leaves open whether the rule is needed.

**Canonical forms (A21).** `explore/symmetry/canon.rs::Canon::canonical` returns the least layout over
`G_s` while it branches on at most `BRANCH_CAP` (4,096) tied orders, and some member of the class past
that. Theorem A18 needs only "some member": the run reaches every class, and a class stored twice
under two forms only inflates the printed count, which `about` then marks as an upper bound. The
argument is P-full given A18 for the checks whose value on a state is invariant under `G_s`.
Those are the first group of the next paragraph: state rules, `always`, `after` assertions, view
patterns, and `NeverRuns`. The checks that read the annotated quotient (deadlocks, `stable`,
`unchanged`, `precedes`, liveness) follow a binding through the stored permutations. Their
soundness past the cap is part of A23, which is a sketch. M: `Twin::Canonical` compares the canonical form with the least layout over every element of
`G_s` when `|G_s| ≤ 6^4 = 1,296`; D: `explore/symmetry/tests.rs::past_the_branch_cap_a_state_is_cut_and_stays_in_its_orbit`,
`explore/symmetry/witnesses.rs::past_the_branch_cap_the_verdict_stands`.

**Which checks read a run up to symmetry soundly.** A check whose value on a state is invariant under
`G_s` (every state rule and `always`: they quantify over all bindings), a check of the concrete triple
`(state, params, successor)` before canonicalization (`after`, view patterns), and a check of which
actions label some edge (`NeverRuns`) is decided on the run up to symmetry exactly as on the plain run,
by Theorem A18. Checks that compare two stored states slot by slot across an edge (deadlocks, `stable`,
`unchanged`) or follow one binding along a path (liveness, `precedes`) read an *annotated* quotient: each
edge keeps the permutation `σ` that took its concrete successor to the stored one, and a binding moves by
`σ` (`explore/orbit/liveness.rs::OrbitRun::decide_liveness`, `explore/orbit/findings.rs`). A cycle whose
permutations compose to `π` is a concrete path from a state to its `π`-image, closed after at most the
order of `π` repetitions. The theorem for this reading is not written out here (**A23**: P-sketch; M:
`Twin::OrbitLiveness`, `Twin::OrbitStateModels`; D: corpus rule 8,
`core/tests/reduce.rs::an_entity_cone_up_to_symmetry_decides_what_its_plain_run_left_open`). The
`Twin::OrbitLiveness` twin compares with the plain run's search and so checks the quotient, not the
fair-component rule (P9).

**`DeletedRowOwner` (A19).** `Earlier` binds a row by its run mark whatever its existence, so a later step
of a split use case reads the key of a row an earlier step deleted. If a delete reset the key to its
base value (slot 0), that value would not move with `g` while the parent it named did, and the step of
Theorem A18 would fail for every `g` moving slot 0. (G5) removes the case. D: corpus
`sym_deleted_row_owner`.

**Counterexamples.** The stored path `r₀ → … → r_k` lifts forward: `c₀ = r₀`, `τ₀ = id`; for each stored
edge recompute the move from `r_i`, take the first successor `t` that canonicalizes to `r_{i+1}` with
`σ_i(t) = r_{i+1}`, and set `c_{i+1} = τ_i(t)`, `τ_{i+1} = τ_i ∘ σ_i⁻¹`. By Theorem A18 each concrete step
is a step of the system, and each `τ_i` fixes the absent slots of `r_i`, so creations replay under the
lowest-absent-slot rule. `explore/orbit.rs::replays` replays the lifted trace and panics if it does not
replay (P20: a panic, not `LassoFault`).

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
`cli/tests/engine_corpus.rs::each_h_reason_a_case_shows_has_a_witness`.

*Evidence for A18.* P-full given (G1)–(G6). M: the Kani harness
`explore/symmetry/mod.rs::apply_of_a_composition_is_the_composition_of_applies` proves that `apply` is a group
action on a 2-slot parent and a 2-slot child; it does not call `canonical` (P8). `Twin::Canonical`,
`Twin::OrbitRun`, `Twin::ReducedOrbitRun` (up to 200,000 states). D: corpus `sym_*` cases, rule 8.

---

## Bound escalation

<!-- derived-from ../formal-verification.md#proofs-for-any-number-of-rows -->

**What the code does (A25).** `cli/verify/escalate.rs::escalate` takes each state rule `i` that the prover
left unproved and that BFS passed at `β` (`passed_at`: a complete run of its cone without a violation, or a
cone covered by one that passed with no finding, and not a run up to symmetry). For `k = 1, 2` it builds
`T_{β+k}(M)` with `grown` (every count plus `k`, `None` past 255) and keeps it only when `same_system`
holds (no build error, the same entities, variable count, and rules by name and site), explores the rule's
cone reduced (`explore_targets_reduced`), and prints `ok at n`, `counterexample at n`, or `not decided at n`,
where `n = cone_slots` is `restrict_to(cone).most_slots()`, the largest slot count among the cone's entities.

**Proposition A25.** If the cone of rule `i` at `β + k` is exact, a `counterexample at n` is a reachable
violation of `T_{β+k}(M)`. If it has a freed entity and no left-out mark, it is a reachable violation of
`T_{β'}(M)` for the `β'` of Proposition A28. If it has a left-out mark, nothing follows.

*Proof.* Theorem A1 (or A28) on `T_{β+k}(M)` for the cone. The run reads the reduced cone, and
Theorem A8.1 says that a valuation of `T_v ∪ obs` breaking the rule is reachable in `R` iff it
is reachable in `C`. So a violation found on `R` is one of `C`. The printed trace is the lift of
A16, whose evidence is runtime checks only. The existence of the violation does not depend on
it. ∎

Two gaps, both confirmed by reading. P1: `escalate` reads neither `free_triggers` nor `over_bound` and calls
every counterexample "a real one". P11: `n` excludes the freed entity, whose count the trace can exceed
(the trace's `over_bound` note says so, but the `escalated:` line does not), and under `--instances E=k`
`n` is a maximum over entities, not the bound of any one of them.

**Memory.** Each escalated run is charged to `pool.with_room(min(pool.left(), GROWTH × peak))`
(`memory.rs::MemoryPool::with_room`), with `GROWTH = 10` and `peak` the store the rule's cone held at `β`;
`verify` always gives BFS a pool (`cli/verify/mod.rs` charges its caps to `MemoryPool::share_of`), so the
`None => caps` arm does not occur under `verify`. The bound is on the run's BFS state store, a child pool of
BFS's share: the reduction's run of `S` and its partition, and the lift, are outside it (the caps "stay
plain", per the comment in `escalate`). The factor 10 is measured: on the engine corpus's escalated cases one
instance more multiplied the store by 1.1 to 6.4 and two more by 1.4 to 10.1 (`escalate.rs::GROWTH`'s doc,
#421). The runs go one at a time after every other BFS and TLC run, so their stops do not depend on the
schedule. P: none (the statement is a description of the code); D: one unit test of `grown`.

---

## Caps, the memory limit, and `not decided`

<!-- derived-from ../formal-verification.md#memory-state-caps-and-threads -->

**Proposition A24.** Let a run of system `U` (a cone, reduced or not, or `T`) stop at a state cap, the
memory limit, or a time budget.

1. A state BFS admitted is a reachable state of `U`: BFS admits only initial states and successors of
   admitted states. So a safety violation found before the stop is a counterexample of `U`, with the
   same meaning (Theorems A1, A2, A28) as on a complete run.
2. No `ok` is printed: `cli/verify/bfs.rs::BfsCone::verdict` maps an empty finding list on a `Capped` run to
   `None` (`not decided`), and `decide_liveness` and `decide_liveness_reduced` return `Undecided` when the
   graph is incomplete and no counterexample was found.
3. A liveness lasso on an incomplete graph is built from expanded states only
   (`ComponentRun::expanded`: on an incomplete run, the states before the source of the last edge), whose
   successor sets are complete; it is replayed before it prints.

*Proof sketch.* (1) and (2) are the code paths named. (3) needs that every state with index below the last
edge's source was fully expanded, which holds for the one-thread BFS order; for the parallel BFS it is
theme (c)'s claim that the run computes the one-thread result. ∎

`not decided` means: the run made no claim either way. P10 (formal-verification.md says a stopped cone
"leaves its invariants `not decided`", but a counterexample found before the stop is reported) is
confirmed by `BfsCone::verdict`. P21 (a stopped native run recorded as `ok` for the per-run comparison) is
theme (c)'s and (d)'s. D: `cli/tests/engine_corpus.rs::a_capped_graph_decides_nothing_the_full_graph_contradicts`
runs every corpus case under every cap from 1 to its size.

**A26 (what the memory limit bounds).** The limit is charged by the BFS state stores and, under
`--with-tlc`, the TLC heaps. The passes over a finished graph (liveness, `can_reach`, deadlocks, the
partition), the lift, and z3 are not charged. This is a statement of scope, not a guarantee, and
formal-verification.md, "Memory, state caps, and threads", says that runs were measured past the limit.
Evidence: none beyond the reading of `escalate` and the guide.

---

## Summary of evidence

| Claim | Statement here | Proof | Other evidence | Open |
|---|---|---|---|---|
| A1 | Theorem A1 | full | D (does not single out left-out marks) | P32 |
| A2 | Theorem A2; left-out marks | full (`ok` of universal checks); none for left-out-mark counterexamples or for existential and branching findings | D | P1, P32 |
| A3 | Proposition A3 | full (`count_up`, `count_down`) | D | |
| A4 | Theorem A4 (on the unreduced cone) | full | D | P1 |
| A5 | Theorem A5 | sketch for parts 1 and 3 (read on the reduced cone: A8, A15); full for part 2 | D, unit tests | P1, P7, P31 |
| A6 | Proposition A6 | sketch for `always`, `eventually`, `leads_to` (A8); full for `stable`, `unchanged`, `precedes` | D | P1, P7, P31 |
| A7 | Theorem A7 | full | D | |
| A8 | Theorem A8 | sketch: assumes (B1)–(B3) and Lemma 6 | M, D | P7, P31 |
| A9 | — | none | M (≤ 2,500,000 states), D | P7 |
| A10, A11 | Lemma 6, point 5 | sketch | M, D | P31 |
| A12, A13, A14 | by reading | full (by construction) | D (A12, A13) | |
| A15 | Theorem A15 | full relative to A8's hypotheses | R, D | P7, P31 |
| A16 | lift | none | R (partial), D | P20 |
| A17 | — | none | M, D | |
| A18 | Theorem A18 | full given (G1)–(G6) | M, D | P8, P30 |
| A19 | `DeletedRowOwner` | full (as the case (G5) removes) | D | |
| A20, A21 | subgroup; canonical form | full given A18 (A21: checks invariant under `G_s`; annotated-quotient checks are A23) | D | |
| A22, A23 | composition; annotated quotient | sketch | R, M, D | P9 |
| A24 | Proposition A24 | sketch | D | P10 |
| A25 | Proposition A25 | full given A1, A28, A8 | unit test | P1, P11 |
| A26 | scope | none | none | |
| A27 | Proposition A27 | full relative to the covering cone's `ok` (Theorem A2; A8 when that cone is reduced) | none | P7, P31 |
| A28 | Proposition A28 | full | D (via A2) | P11 |

---

## Related work

- **Cone of influence.** The reduction of a model to the variables a property transitively depends on is
  standard (Clarke, Grumberg and Peled 1999, chapter on abstraction and the cone of influence; Kurshan
  1994 describes the related localization reduction). Theorem A1 is that construction for guarded actions with
  parameters, where the "dependency" relation must also cover slot allocation (freed entities) and event
  marks. Over-approximating cones (A2) are an existential abstraction in the sense of Clarke, Grumberg and
  Long 1994: they preserve universal safety, and their counterexamples need a check against the concrete
  system, as in counterexample-guided refinement (Clarke et al. 2000); A28 is such a check for freed slots,
  and P1 is the absence of one for left-out marks.
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

- **Proofs about a reading of the code.** Every lemma models a Rust function by its doc comment and body as
  read at `fb6928c7`. A function read only through its doc (named in its proof: `Fold::holds` for Lemma 6)
  may differ from it. No statement was checked by running the tools.
- **Syntactic over-approximations.** `Wr` lists a deleted entity's owners although D8 keeps them. This only
  adds actions and variables to cones and makes owners non-monotone; every proof above tolerates it, but a
  later change that made `Wr` an under-approximation would break Lemma 2 silently, since nothing checks `Wr`
  against the effects' semantics.
- **Assumed partition correctness.** A8, A15, and every reduced cone's verdict rest on (B1)–(B3), which the
  release build does not check (P7), and on Lemma 6, which no note states (P31). Among those verdicts are the
  printed `ok` of a property's `Plain` run (A5.1) and of a property on a cone that frees slots (A6).
- **Differential evidence compares implementations, not specifications.** The shadow twins compare two
  implementations of one definition and skip large inputs without failing (P17). The corpus and sample
  comparisons check agreement with TLC on the cones the code chooses; a soundness bug in the cone or the
  reduction that TLC's module shares (one `Reduction` value feeds both, A13) is invisible to them. Only
  `scripts/reduction-diff.py` and `core/tests/cone.rs` compare a reduced or restricted system with a less
  reduced one.
- **Hypotheses checked per trace, not per cone.** Exactness is a property of the cone, but the code reports a
  left-out mark only on a trace that uses it (`free_triggers`). A cone with a left-out mark whose counterexample
  does not use it is exact for that trace; one that uses it is P1. A finding the cone fails to report (an
  entity cone's `NoWitness`, `Deadlock`, `NeverRuns`, arm, or value) has no trace, so the per-trace check never
  sees it (P32).
- **Symmetry hypothesis (G6).** Not enforced (P30). If a model reached it, a run up to symmetry could print a
  wrong verdict, `ok` included; none is known.
- **Bounded results.** Everything here, escalation included, is a statement at a bound. `ok at n` at
  `β + 1` and `β + 2` adds two bounded results; it is not an induction.

## Errata

Changes to statements of this document after it was first written, newest last.

- **2026-11-28, after an adversarial review** (commits `7a3d13df` and `073bdf11`). The
  corrections re-read the code at `fb6928c7` as well; the crates at the head of the pull
  request that added this document, `05679a5c`, do not differ from it.
- **2026-11-28, after the owner's review.** The P32 paragraph of [Safety cones](#safety-cones)
  listed `verify/tlc.rs` among the files that read `free_triggers`, `over_bound`, or
  `cone_frees_slots`; at `fb6928c7` it reads only the `frees_slots` field that
  `verify/jobs.rs` sets from `cone_frees_slots`, and the list now says so. The proof of
  Theorem A8 opened with "Points 1 to 5 of cone-soundness.md"; it now states that its five
  points are numbered as there, and point 5 adds what cone-soundness.md's point 5 says of folds
  and of the monotone rule, so the proof reads without that note.
