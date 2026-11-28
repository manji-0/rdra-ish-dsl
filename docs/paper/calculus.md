# LC: a core calculus for the reductions (roadmap T12, T13)

<!-- derived-from ../theory/semantics.md -->
<!-- derived-from ../theory/cones.md -->
<!-- derived-from ../theory/native.md#the-check -->
<!-- derived-from ../../proofs/lean/README.md -->

This document defines LC, a small calculus of data-carrying lifecycle models. It restates the
theorems that Paper 1 depends on over LC with no Rust symbols: A1, A8, A15, A18, A23 and N9 of
[docs/theory](../theory/README.md). It also records the decision on how the paper links LC to the
code (T13). This document describes main at `9e4a123`.

LC is the fragment of the language of [semantics.md](../theory/semantics.md) that the reductions
need: entities with a bounded number of slots, finite-domain columns, foreign keys (owners), and
guarded actions with per-instance weak fairness. The constructs that LC leaves out are listed in
[What LC leaves out](#what-lc-leaves-out), with how each one maps into LC or why it needs its own
argument.

## 1. Syntax

**Signature.** A signature `Σ` gives:

- a finite set `Ent` of entities;
- for each entity `E`, a finite set `Col(E)` of columns, each `c` with a finite domain `D_c` and a
  base value `base_c ∈ D_c`;
- for each entity `E`, a finite set `Own(E)` of owners. Each owner `o` has a parent entity
  `par(o) ∈ Ent` and is either *mandatory* or *optional*.

**Bounds.** A bound `n : Ent → ℕ_{>0}` gives the number of slots of each entity. `[k]` is
`{0, …, k−1}`.

**States.** A state `s` assigns:

- `ex_E(s, i) ∈ {absent, present, deleted}` to each slot `i ∈ [n_E]`;
- `c(s, i) ∈ D_c` to each column `c ∈ Col(E)` and slot `i`;
- `o(s, i) ∈ [n_{par(o)}] ∪ {⊥}` to each owner `o ∈ Own(E)` and slot `i`.

A *location* is a triple such as `(c, E, i)`. A *variable* is a column, an owner, or an existence,
taken over all slots of its entity. `V` is the set of variables.

**Terms and predicates.** Parameters `p, q` are typed by an entity, written `p : E`.

```
t ::= d                    (a literal of some D_c)
    | c[p] | o[p]          (a column or an owner of the row p binds)
    | id(p)                (the slot p binds)
    | ⊥
φ ::= t = t | t ≠ t | t ∈ L | ¬φ | φ ∧ φ | φ ∨ φ
    | ∀x:E. φ | ∃x:E. φ | #{x:E | φ} ⋈ k        (⋈ ∈ {≤, ≥, =})
```

Quantifiers range over the present slots of `E`. Comparisons are two-valued. A comparison with `⊥`
on one side holds for `=` only against `⊥`, and holds for `≠` against every other value.

**Actions.** An action `a = (P_a, role_a, g_a, F_a)` has:

- parameters `P_a`, each `p : E` with a role: `new` (the row the action creates), `ex` (an
  existing row), or `par(o, q)` (the parent of the row `q` along owner `o`);
- a guard `g_a`, a predicate whose free parameters are in `P_a`;
- an effect `F_a`, a set of updates, at most one per location form `(x, p)`:
  - `create p` and `delete p`;
  - `c[p] := d` (a literal), `c[p] :∈ L` (a choice among `L ⊆ D_c`), and
    `c[p] :∈ δ(c[p])` (a transition table `δ : D_c → 2^{D_c}`);
  - `o[p] := id(q)` (with `q : par(o)`), `o[p] := ⊥` (optional `o` only), and
    `o[p] :∈ [n_{par(o)}]` (any row).

**Rules and properties.** A *state rule* is `∀x₁:E₁ … ∀x_k:E_k. φ` over present slots. A
*liveness property* `P = leads_to(x, y)` with binders `x̄ : Ē` and a set `Φ_P` of fair actions
states `□(trig ∧ x ⇒ ◇y)` per binding, where `trig` says that every bound slot is present.
`eventually(y)` is `leads_to(true, y)`.

**Models.** A model `M = (Σ, Init, A, Rules, Props)` adds an initial condition `Init`, which says
for each entity whether its slots start present (pre-existing) or absent (created), and for each
column and owner of a present slot which values it may start at. `Init` is a product of per-location
value sets.

## 2. Semantics

**Bindings.** A binding `b` of an action `a` in state `s` maps each parameter to a slot of its
entity, admissibly:

- `new`: the *lowest* slot whose existence is `absent`; no binding exists when there is none;
- `ex`: any present slot;
- `par(o, q)`: any present slot `j` with `o(s, b(q)) = j`.

**Steps.** `s →_{a,b} s'` when `b` is admissible in `s`, `g_a` holds at `(s, b)`, and `s'` is one
of the successors of `F_a`: every update is applied to `s` at the same time, and each choice forks
the successor. `delete p` sets the existence to `deleted` and resets every column of the row to its
base value. It keeps the owners. `create p` sets the existence to `present`, and the created row's
columns start at their base values unless the effect writes them. A slot never leaves `deleted` and
never returns to `absent`.

**The system.** `T_n(M) = (S, I, →)`, where `S` is the set of states, `I = Init`, and `→` is the
union of the steps. `S` is finite. `Reach(T)` is the set of reachable states.

**Behaviours and fairness.** A behaviour is an infinite sequence `s₀ s₁ …` with `s₀ ∈ I` and, for
every `i`, either `s_i → s_{i+1}` or `s_{i+1} = s_i` (a stutter). A *fair instance* of `P` is a pair
`(a, κ)` of a fair action `a ∈ Φ_P` and the slots `κ` of its parameters other than `new`. The
instance is *enabled* at `s` when some admissible binding that extends `κ` has a successor
`s' ≠ s`. A step `s → s'` *takes* the instance when `s' ≠ s` and some binding that extends `κ`
moves `s` to `s'` (assumption τ: a self-loop takes nothing). Two fairness conditions apply:

- `Fair_P`: weak fairness per instance. Every instance that is enabled at every state from some
  point on is taken infinitely often.
- `WfNext`: `Fair_P ∧ WF(Next)`. If from some point on every state has a successor other than
  itself, the state changes infinitely often.

A state rule holds when it holds in every reachable state. `P` holds under a fairness condition `F`
when, for every behaviour that satisfies `F` and every binding `b`, each position where
`trig_b ∧ x_b` holds is followed (at it or later) by a position where `y_b` holds. The goal is read
on the slot whatever its existence, so a delete can meet a goal that the base values satisfy.

This is D2 to D21 of [semantics.md](../theory/semantics.md), restricted to the constructs above.

## 3. Theorems over LC

The statements below use the notation of Section 1 only. The last column of the table in
[Section 5](#5-map-of-the-theorems) names the claim and the Lean theorem for each one.

### 3.1 Read and write sets, and the cone

For an action `a`, `Rd(a)` is the set of variables that its guard and its transition tables read,
together with the existence of each parameter's entity. `Wr(a)` is the set of variables that its
updates write. A `delete` writes the existence and every column and owner of its entity. Both sets
are syntactic.

**Locality (Local).** A guard depends only on the values of `Rd(a) ∪ Wr(a)` at the bound rows. An
effect leaves every variable outside `Wr(a)` unchanged, and its values on `Wr(a)` depend only on
`Rd(a) ∪ Wr(a)`. This holds in LC by construction. In the code it is a fact about the builder (see
[Section 4](#4-the-link-to-the-code-t13)).

**Definition 1 (cone).** For a seed `σ ⊆ V` and a set `K` of kept actions, `W ⊇ σ` is *closed*
when every action in `A_W = K ∪ {a : Wr(a) ∩ W ≠ ∅}` has `Rd(a) ⊆ W`, and `W` holds the existence of
the entity of each of its variables and of the parent of each of its owners. `cone(σ, K)` is the
least closed set. `T|W` keeps the actions of `A_W`, restricted to their updates on `W`, and projects
the states onto `W`. `W` *frees an entity* `E` when some action of `A_W` has a `new` parameter on
`E` and `ex_E ∉ W`. `W` is *exact* when it frees no entity.

**Theorem 1 (A1, exact cones).** If `W` is exact, then `Reach(T|W) = π_W(Reach(T))`, every finite
trace of `T|W` lifts to a trace of `T` with the same actions and bindings, and every behaviour of
`T` projects to a behaviour of `T|W` up to stutters. So every check whose reads lie in `W` gets the
same verdict on `T|W` as on `T`. A cone that is not exact over-approximates: `π_W(Reach(T)) ⊆
Reach(T|W)`, so its `ok` for a state rule is sound (A2).

### 3.2 The independent sub-lifecycle and its quotient

Let `C` be a cone system with variables `V_C`, and let `seeds` be the variables that its checks
read.

**Definition 2 (split).** A split is a partition `V_C = T_v ⊎ S_v` and a partition of the writing
actions into `T_a ⊎ S_a`. Write `obs = (⋃_{a ∈ T_a} Rd(a) ∪ seeds) ∩ S_v` and let `inputs ⊆ T_v`
be the `T_v` variables that some `S_a` action reads, with their existences. A variable is
*monotone* when its write graph (the pairs of values that the effects can write, over all
actions) has no cycle other than self-loops. The split is *admissible* when:

- (S1) every `S_a` action writes only `S_v`;
- (S2) every `T_a` action writes only `T_v` and reads `S_v` only in `obs`;
- (S3) every read of an `S_a` action is in `S_v ∪ inputs`;
- (S4) every input is monotone;
- (S5) every read of a seed in `S_v` is in `obs`, and every fair or kept action is in `T_a`.

These are Lemma 5 (a) to (e) of [cones.md](../theory/cones.md#the-reduction-by-an-independent-sub-lifecycle).
The tool computes the least split that contains a chosen anchor variable. The theorems below need
only admissibility, which every build checks.

**Definition 3 (extended sub-lifecycle).** `X` is the system over `S_v ∪ inputs` whose edges are:

- *silent* edges: the steps of the `S_a` actions;
- *rises*: the `T_a` actions that write an input, restricted to `S_v ∪ inputs`. A rise leaves
  `S_v` unchanged.

A silent edge leaves the inputs unchanged. For a state `(t, s)` of `C` (`t` over `T_v`, `s` over
`S_v`), `x(t, s) = (s, t|inputs)` is the matching state of `X`.

**Definition 4 (stable partition and quotient).** A partition `B` of `Reach(X)` is *stable* when:

- (B1) two states of a block agree on `obs` and on the inputs;
- (B2) divergence-blind stuttering stability under the silent edges: for blocks `B ≠ B'`, either
  every state of `B` reaches `B'` by silent edges whose states before the last all lie in `B`, or
  no state of `B` does;
- (B3) strong stability under rises: two states of a block reach the same blocks by one rise.

`R`, the *reduced cone*, keeps `T_v`, replaces `s` by a representative `ρ` of the block of
`x(t, s)`, runs the `T_a` actions unchanged, moves the block along a `T_a` step that changes the
inputs by the matching rises, and replaces the `S_a` actions by *class moves* from a block to each
block that its representative reaches by (B2).

**Theorem 2 (A8, the reduced cone is stuttering equivalent).** Let the split be admissible, `B`
stable, and each representative a reachable state of its block. Then
`(t, s) ~ (t, ρ(block(x(t, s))))` is a divergence-blind stuttering bisimulation between `C` and `R`.
It preserves `T_v`, `obs`, and the label (action and binding) of each `T_a` step. Hence `C` and `R`
agree on:

1. which valuations of `T_v ∪ obs` are reachable: state rules and their vacuity;
2. which `T_a` labels occur on an edge from a state with given `T_v ∪ obs` values: actions that
   never run, and assertions after an action;
3. whether from each reachable state some path takes an edge that changes given `T_v` values
   (deadlock in the sense of D25, where a silent cycle counts as stuck);
4. every liveness property whose reads lie in `T_v ∪ obs` and whose fair actions lie in `T_a`,
   under `Fair_P`.

**Theorem 3 (A15, weak fairness of `Next` on the reduced cone).** Add the hypotheses that `X` and
`R` have finitely many reachable states and that every block whose silent edges between distinct
states contain a cycle is marked *can loop*. Search `R` with the stop rule "no fair instance is
enabled, and either no move leaves the state or its block can loop". If that search finds no
violating lasso, `C` satisfies the property under `WfNext`.

`WF(Next)` is not invariant under stuttering, so Theorem 2 alone does not give Theorem 3. A
behaviour of `C` can step forever inside one block, where its image in `R` stops. The *can loop*
mark lets the image stop exactly there.

**Certificates and fallback.** Admissibility (S1–S5), stability (B1–B3), the *can loop* marks, and
the rise table that `R`'s moves read are checked by every build on the computed data. A failed check
makes the tool decide the cone unreduced and print a warning. The theorems need only these
properties, not the algorithms that compute the split and the partition.

### 3.3 Instance symmetry

**Definition 5 (the state-dependent group).** The *classes* are the entities, where an identity
start (the child's slot `i` owned by the parent's slot `i`) couples a child with its parent. Some
classes are *fixed* (left out of the group). For a state `s`, `G_s` is the product, over the classes
that are not fixed, of the permutations of the slots that are *not absent* in `s`. `g ∈ G_s` acts
on a state by moving each slot's columns and existence with the slot, and by relabelling each owner
on a non-absent row by the parent's permutation. An owner on an absent row keeps its value. `g` acts
on a binding by `g(b)(p) = g(b(p))`.

Two choices separate `G_s` from the fixed process-index groups of Ip–Dill and Emerson–Sistla:

- absent slots stay fixed, because creation takes the *lowest* absent slot (a permutation that moved
  an absent slot would not commute with creation);
- owners are relabelled, so foreign keys move with the rows they name.

**Hypotheses.** A class is fixed unless each condition below holds for it. The tool checks the
conditions syntactically and fixes every class that fails one.

- (G1) A term that names a row of the class (`id(p)`, an owner) is compared only by `=` or `≠`, and
  only with a term that names a row of the same class, or (an optional owner) with `⊥`. Such a term
  appears in no `∈ L`.
- (G2) An update writes an owner of the class only from a parameter of the parent's class or `⊥`. A
  choice over an owner ranges over every row. No column is written from `id(p)` of a moving class.
- (G3) `Init` is closed under every `G_s`: no start pins a slot.
- (G4) A creation writes every mandatory owner of the created row.
- (G5) A delete keeps the row's owners (part of the semantics).
- (G6) No guard reads the owner of the row that the action creates, in the state before the step,
  when that owner's parent class moves.

Let `W` be the set of states in which every absent slot holds the base value of each of its columns
and owners. `W` contains `I`, is closed under steps, and is closed under every `G_s`.

**Theorem 4 (A18, symmetry is a bisimulation).** Under (G1)–(G6), for every `s ∈ W`, `g ∈ G_s`, and
step `s →_{a,b} t`: `g(s) →_{a,g(b)} g(t)` is a step, and `g ∈ G_t`. So `≈ = {(s, g(s)) : g ∈ G_s}`
is an equivalence and a bisimulation on `W`. A breadth-first search that stores one member of each
class (any member, as a function of the state) reaches exactly the classes of `Reach(T)`.

The restriction to `W` is needed. Off `W`, an absent row's optional owner can name a row. The step
that creates the row leaves the owner as it is, and `g` does not relabel it on the absent row, so
the commutation fails (`SymActionsExample.lean::off_W_counterexample`). Commutation on the
reachable states alone is not enough either: the quotient can store an unreachable image
(`SymmetryLivenessExample.lean::ExampleReachOnly`).

**Theorem 5 (A23, liveness up to symmetry).** An *annotated run* of the quotient is a sequence of
stored states `r_n`, concrete successors `t_n` (a step of `r_n`, or `r_n` itself), and permutations
`σ_n ∈ G_{t_n}` with `r_{n+1} = σ_n(t_n)`. A binding and each fair instance move along the run by
the `σ_n`. The run is *thread-fair* when each moved instance that is enabled from some point on is
taken infinitely often. Assume Theorem 4's commutation on `W`, that the group acts on fair instances
compatibly with the labels, and that the trigger, the stop rule, and (after a trigger) the goal read
the same on `s` and `g(s)`. Assume also that the reachable states and every `G_s` are finite. Then
`P` holds for every binding under `Fair_P` (or `WfNext`) iff no thread-fair annotated lasso of the
canonical quotient violates it for any binding. A violating lasso of the quotient lifts to a
violating lasso of `T` after at most `d` copies of its cycle, where `d` is the order of the cycle's
composite permutation.

TLC's symmetry sets come with a warning that liveness checking under symmetry can miss violations.
Theorem 5 avoids this by tracking the permutation on each edge of the quotient and the thread of
each instance along it.

**Theorem 6 (N9, liveness by fair components).** Let `G` be the complete reachable graph of a finite
system, `N_b` the states where `y_b` fails, and `E_N` the edges between distinct states of `N_b`. A
*stop* is a state of `N_b` with no fair instance enabled where the stop rule holds (`true` under
`Fair_P`, "no move" under `WfNext`). A strongly connected component of `(N_b, E_N)` is *fair* when
it has at least two states and every fair instance enabled at all of its states labels one of its
edges. `P` fails for `b` iff some reachable state with `trig_b ∧ x_b ∧ ¬y_b` reaches a stop or a
fair component by edges of `E_N`. The same holds for the search on (state, binding) pairs of a run up
to symmetry (`SearchSym.lean::oAccepts_iff`).

## 4. The link to the code (T13)

**Decision.** The paper claims the theorems for LC, and claims for the tool a *validated
translation*. It does not claim a verified implementation. Extending the Lean model to the builder
would need a deep embedding of the Rust builder and of `restrict_to`
([proofs/lean/README.md, "What is not covered"](../../proofs/lean/README.md#what-is-not-covered)).
That is a separate project, so the paper names it as future work.

The translation is validated in four ways:

1. **Run-time certificates with fallback.** For every reduced cone, every build checks the split's
   admissibility (S1–S5, `reduce/lemma5.rs::split_faults`), the partition's stability (B1–B3,
   `partition/reference.rs::stable`), the *can loop* marks (`reduce/hypotheses.rs::loop_faults`),
   the rise table and the layout of `X` (`rise_faults`, `layout_faults`), and the stub binding of
   `X` (`extended.rs::StubCheck`). On a failure it decides the cone unreduced. The certificates
   are executable checkers whose soundness for the hypotheses is proved in Lean
   (`Lemma5Check.lean::check_sound`, `Sweep.stable_of_cert`). `scripts/check-lean-certificates.sh`
   and `scripts/check-lean-partitions.sh` run the Lean-compiled checkers on every split and every
   flat partition of the samples and the corpus.
2. **Self-certifying counterexamples.** Every printed counterexample is replayed through the step
   relation of the model's system before it prints. A trace that does not replay is a `tool_error`.
3. **One syntax for the Lean slot model and LC.** The action model of Lemmas 5 and 6 is read off
   the slot model of Theorem 4, and `Local`, `shape` and `new_writes` are theorems of that model
   (`SymActModel.lean`). That the code's actions are this syntax is read from the code.
4. **Differential testing.** A reference interpreter rebuilds the system from the semantic model
   without the builder on a subset of the language. The engine corpus has 91 cases with
   hand-written verdicts. CI runs TLC beside the native checker, compares reduced runs with
   unreduced runs on random models, and compares symmetric runs with plain runs.

**What stays read from the code.** These items are not certified at run time and not derived in
Lean. The paper lists them as threats to validity. The list is the open gap of claims A8, A18 and
A23 in [docs/theory/README.md](../theory/README.md#known-limitations).

- Eight fields of `Lemma6Hyp` (`kept_rd`, `folded_og`, `folded_names`, `other_names`, `eff_names`,
  `exact`, `rise_eff`, `eff_s`) and half of `cover`: facts about which guards `restrict_to` keeps
  and about the folds. The folds are an optimization of `X` that LC does not have.
- That `Group::apply`, the syntactic scan `breaks.rs::scan` and `each_successor` are the slot
  model's action, hypotheses and successor function.
- Tarjan's components, the bit sets, and the stopped-graph cut of the liveness search (N9, N10).
- The nested reduction (an inner reduction of `X`) and a run of `S` up to the group `H`. Their
  arguments are sketches.

## What LC leaves out

| Construct of the language | Definition | How it relates to LC |
|---|---|---|
| Raised events: latches and counters (`raises` / `triggers`, `.counted(k)`) | D11 | A latch is a Bool column that the raiser sets and the handler's guard reads; a counter is a column over `{0, …, k, over}`. Both are LC columns. The cone treats them as *marks* that it may leave out, which makes a cone inexact (A2, A28). The paper's cone theorem covers exact cones only. |
| Split use cases and run marks | D10 | A run mark is an Enum column `{none, 1, …, k−1}`; the step `UC_i` binds its row by the mark (role `Earlier`). LC can express this with a column and an `ex` parameter guarded by the mark. |
| Copies between columns (`sets(UC, E.c == F.d)`) | D8 | Outside LC's update forms. In the code a copy is a choice over the target's domain with a guard that pins it. The cone lemma's case (iv) handles a dropped copy guard. |
| The clock `now` and `Tick` | D12 | A global column with a transition table `k ↦ k+1`. |
| `drives` propositions, variation columns, `.when` arms | D8 | Columns, choices and transition tables guarded by predicates. |
| Folds in `X` | Definition 4 of cones.md | An optimization: an `S_a` guard over `T` rows is replaced by one bit per slot. Not in LC; its correctness is a hypothesis read from the code (above). |
| Nested reduction, `S` up to `H` | cones.md, A8 and A22 | Not in the paper's theorems. The Lean development has `Nested.lean` for towers; the code's outer representative is a sketch. |
| Scopes, `can_reach`, `stable`, `unchanged`, `precedes` | D19, D22, D24 | Scopes remove actions. The other forms are decided on plain (unreduced, non-symmetric) cones or on exact cones, and do not need the theorems above. |

## 5. Map of the theorems

| Paper | Claim | Lean (`proofs/lean/RdraIsh/`) | Status in the code |
|---|---|---|---|
| Theorem 1 (exact cones) | A1, A2 | none (paper proof, P-full) | Tested against the projection of the whole system |
| Definition 2 admissibility | Lemma 5 | `Split.lean::lemma5_of_closed`, `Lemma5Check.lean::check_sound` | Certified at run time on every split |
| Theorem 2 (reduction) | A8 | `Reduction.lean::a8_dbsb`, `ReductionLabels.lean::a8_branching`, `Plain.lean::a8_plain_iff`, `Stages.lean::a8_staged` | Flat case mechanized from Lemma 6; Lemma 6 from `Lemma6Hyp`; nested case sketched |
| Stability (B1–B3) | A8 | `Sweep.lean::stable_of_cert` | Certified at run time on every partition |
| Theorem 3 (`WF(Next)`) | A15 | `Fairness.lean::a15`, `Search.lean::a15_search`, `Monotone.lean::a15_monotone` | Flat case mechanized; *can loop* certified at run time |
| Theorem 4 (symmetry) | A18, A20 | `SymActions.lean::symOn`, `Symmetry.lean::a18_dbsb`, `quotient_covers`, `quotient_sound`, `Sym.sub` | (G6) outside guards and the non-moving-parent case on paper |
| Theorem 5 (liveness up to symmetry) | A23 | `SymmetryLiveness.lean::a23`, `a23_on`, `lasso_closure` | Search mechanized as `SearchSym.lean::oAccepts_on_iff` |
| Theorem 6 (fair components) | N9 | `Search.lean::accepts_iff`, `liveHolds_iff_not_accepts` | Tarjan and bit sets read from the code |
