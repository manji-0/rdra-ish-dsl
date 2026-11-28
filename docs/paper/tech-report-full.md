---
title: "rdra-ish: Semantics, Reductions, Proofs and Evidence of a Requirements-Model Checker"
subtitle: "Technical report accompanying Paper 1 and Paper 2 (issue #594, roadmap T20)"
---

<!-- This file is the report's own text: front matter, introduction, summary of results, and the
chapter guide. scripts/paper/build-tech-report.py assembles the full report as
docs/paper/tech-report-full.md: this file, then each chapter's source document at the build's
commit, with links rewritten. The theory documents stay the source of truth. Do not edit the
assembled file. -->

# Abstract

rdra-ish is a textual requirements language in the RDRA tradition, with a checker for the lifecycle
rules and temporal properties of a model. This report collects in one place the semantics of the
language, the soundness arguments for every reduction the checker uses, the prover for state rules
over any number of rows, the agreement of the native checker with TLC, the inventory of claims with
the evidence behind each, the Lean development, and the full evaluation. Paper 1 (sound reductions
for fair liveness) and Paper 2 (what each kind of evidence catches) cite it for proofs and tables
that do not fit in them.

# 1 Introduction

A model `M` at a bound `β` (slots per entity, Int range) denotes a finite transition system
`T_β(M)`. `rdra-ish verify` decides the model's state rules, step assertions, temporal properties
under per-instance weak fairness, reachability goals and deadlocks on that system. It answers `ok`,
`counterexample`, `not decided` or `tool_error`. `verify --prove` tries to prove state rules for every
bound in a range with z3. `verify --with-tlc` runs TLC on the same printed systems and fails on any
disagreement.

Explicit-state checking at useful bounds needs three reductions, and the report proves each:

1. a **cone of influence** per check (Chapter 2);
2. the **sub-lifecycle quotient**, a divergence-blind stuttering bisimulation of one part of a cone,
   with a per-block mark that keeps it sound for `WF_vars(Next)` (Chapter 3);
3. **instance symmetry** under a state-dependent group that fixes absent slots and relabels foreign
   keys, with fairness carried along each edge's permutation (Chapter 4).

The report states each guarantee as a numbered claim (I1–I3, A1–A28, B1–B18, N1–N18, T1–T16) with
the kind of evidence behind it and its open gap (Chapter 8). It is explicit about what is proved,
what is mechanized, what is tested, what is checked at run time, and what is assumed.

## 1.1 Summary of results

| Result | Where | Evidence |
|---|---|---|
| Exact cones decide what the whole system decides; inexact cones over-approximate | Ch. 2, A1, A2 | Paper proof; tests against the projection of the whole system |
| The reduced cone is a divergence-blind stuttering bisimulation of the cone | Ch. 3, A8 | Paper proof; Lean for the flat case (`a8_dbsb`, `a8_branching`); run-time certificates of the split and the partition |
| Liveness under `WF_vars(Next)` on the reduced cone with the *can loop* mark | Ch. 3, A15 | Paper proof; Lean for the flat case (`a15`); run-time check of the marks |
| The orbit relation of `G_s` is a bisimulation on states whose absent rows hold base values | Ch. 4, A18 | Paper proof; Lean over a slot model (`symOn`, `a18_dbsb`) |
| Fair liveness on the annotated quotient | Ch. 4, A23 | Paper proof; Lean (`a23`, `oAccepts_on_iff`) |
| The fair-component search decides liveness | Ch. 5, N9 | Paper proof; Lean (`accepts_iff`) |
| Any number of threads computes the one-thread run | Ch. 5, N4 | Paper proof; 24 of 24 runs byte-identical (Ch. 10) |
| `proved` holds for every bound in the printed range | Ch. 6, B2 | Paper proof relative to z3; every `unsat` behind 63 `proved` rules re-checked by cvc5 (Ch. 10) |
| The native checker and TLC agree | Ch. 7, T2–T16 | 50 of 50 cones with equal verdicts and state counts; CI on every pull request |

## 1.2 The calculus LC

Chapter 1 opens with the calculus LC of [calculus.md](./calculus.md): entities, slots, owners and
guarded actions with per-instance weak fairness. Paper 1 states its theorems over LC. The rest of
Chapter 1 defines the full language (D1–D25), which extends LC with event latches and counters,
split use cases and their run marks, copies between columns, the clock, and scopes.

## 1.3 How the report is assembled

The chapters are the theory documents of the repository at the report's commit:

| Chapter | Source |
|---|---|
| 1. The calculus LC and the semantics | [calculus.md](./calculus.md), [semantics.md](../theory/semantics.md) |
| 2–4. Cones, the sub-lifecycle reduction, instance symmetry | [cones.md](../theory/cones.md) |
| 5. The native checker | [native.md](../theory/native.md) |
| 6. Proofs for any number of rows | [z3.md](../theory/z3.md), with the evaluation of [eval/prove.md](./eval/prove.md): the claim "any number of rows" is an evaluated heuristic, not a decision procedure (roadmap T16) |
| 7. Agreement with TLC | [tlc-agreement.md](../theory/tlc-agreement.md), with [eval/tlc-baseline.md](./eval/tlc-baseline.md) |
| 8. The claims inventory | [claims.md](../theory/claims.md), and the overview with residual risks and known limitations, [README.md](../theory/README.md) |
| 9. The Lean development | [proofs/lean/README.md](../../proofs/lean/README.md) |
| 10. Evaluation | [eval/](./eval/README.md): ablation, scaling, least instance counts, determinism, TLC, the prover, the mutation runs |
| 11. Extensions | [extensions.md](./extensions.md): certificates for `proved` and `ok`, the code chain, the agent study, external models |
| A. Defect record | [evidence/defects.md](./evidence/defects.md) |
| B. Earlier measurements | [evidence/measurements.md](./evidence/measurements.md) |
| C. Related work | [related-work.md](./related-work.md) |

`python3 -I scripts/paper/build-tech-report.py` writes `docs/paper/tech-report-full.md`, and
`pandoc docs/paper/tech-report-full.md -o tech-report.tex` gives LaTeX.

## 1.4 What the report does not claim

The report carries the known limitations of the theory overview at its commit, row by row, and its
two residual risks. The first is common-mode faults in the parts every engine shares (the builder,
the cone choice, the quotient tables, the TLA+ names). The second is that `ok` itself is not checked
at run time. Two kinds of step are argued on paper only, and the report marks them where they occur:
the nested case of the reduction and the run of a sub-lifecycle up to a subgroup.

<!-- Generated by scripts/paper/build-tech-report.py. Do not edit: edit the sources. -->


# Chapter 1 The calculus LC

*Source: `docs/paper/calculus.md`.*

## LC: a core calculus for the reductions (roadmap T12, T13)

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

### 1. Syntax

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

### 2. Semantics

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

### 3. Theorems over LC

The statements below use the notation of Section 1 only. The last column of the table in
[Section 5](#5-map-of-the-theorems) names the claim and the Lean theorem for each one.

#### 3.1 Read and write sets, and the cone

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

#### 3.2 The independent sub-lifecycle and its quotient

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
the rise table that `R`'s moves read are checked by every build on the computed data. A failed
split, loop-mark or rise check makes the tool decide the cone unreduced and print a warning. A
partition that fails its check is replaced by the sweeps' partition, with a warning. The theorems need only these
properties, not the algorithms that compute the split and the partition.

#### 3.3 Instance symmetry

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

### 4. The link to the code (T13)

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
   `X` (`extended.rs::StubCheck`). On a failed partition check it uses the sweeps' partition; on
   any other failure it decides the cone unreduced. The run-time checks are Rust. Lean proves sound
   a model of the partition check (`Sweep.stable_of_cert`) and two separate checkers
   (`Lemma5Check.lean::check_sound`, `PartitionCheck.lean::PCert.stable_of_check`), which
   `scripts/check-lean-certificates.sh` and `scripts/check-lean-partitions.sh` run on the dumped
   top-level splits and flat partitions of the samples and the corpus. Nested runs, runs up to a
   subgroup and capped runs are not dumped.
2. **Self-certifying counterexamples.** Every printed counterexample is lifted to the unreduced
   cone and replayed through the step relation of the system the tool built before it prints. A
   trace that does not replay is a `tool_error`. Replay cannot catch a fault in the builder itself.
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

### What LC leaves out

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

### 5. Map of the theorems

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


# Chapter 1 (cont.) Semantics of the full language

*Source: `docs/theory/semantics.md`.*

## Semantics and notation

<!-- derived-from ../language-reference.md#entity-state-constraints -->
<!-- derived-from ../state-derivation.md#the-transition-system -->

This document gives the meaning of an rdra-ish model as a finite transition system. It also
gives the meaning of the model's rules and properties on that system, and of every verdict
the tools print. The theme documents listed in the [index](../theory/README.md) state their guarantees
in this notation. The [claims inventory](../theory/claims.md) refers to the definitions by number
(D1, D2, …), and each definition has an anchor of its own (`semantics.md#d1` … `#d25`).
This document describes main at `91524792`.

**What the definitions rest on.** D1 to D13 and D24 define `T_β(M)` from the model's text.
Each clause names the surface construct it gives meaning to (an entity, a column and its
annotations, a `relate`, a `sets`, a `transitions` line, a `chooses`, a `.when`, a
`creates`, a `links`, a copy, a `raises`/`triggers` pair, a `deletes`, a split use case) and
says what that construct contributes to the variables, the initial states, or the steps.
The definitions follow [language-reference.md](../language-reference.md),
[state-derivation.md](../state-derivation.md), and the design notes
[event-order.md](../design/event-order.md) and [counted-events.md](../design/counted-events.md).
Where those notes leave a point open, the definition makes a choice, and a remark under the
definition marks it. No definition is stated in terms of what a function of the code
computes. Code appears only in **Implemented by** notes, which say where to look. Whether
the code computes `T_β(M)` is a claim with evidence, not a definition: claim
[I1](../theory/claims.md#0-the-build) says that `TransitionSystem::build` returns `T_β(M, σ)`.

The definitions apply to a model `M` that loads and builds at `β` with no diagnostic of
severity error. Examples are `IntLiteralOutOfBounds`, `DomainTooLarge`, `ConflictingEffects`,
`UnboundCopySource`, and `AmbiguousOwnerLink`; the whole list is
[state-derivation.md, "Diagnostics"](../state-derivation.md#diagnostics). Which models are
well-formed is the language's static semantics. This document does not restate it, and
there it relies on the code's list (`lifecycle::diag::StateDiag`). For a pair `(M, β)`
with an error, `T_β(M)` is undefined. The tools print the error and give no verdict.

D14 to D23 define the meaning of rules, properties, and verdicts on a transition system
`T`, and D25 that of the deadlock finding. They apply to `T_β(M)` and to every system
derived from it (a cone, a reduced cone, a quotient), and they cite code where a verdict is
printed.

Code is cited by Rust path (`lifecycle::types::Bounds`) or as `file.rs::symbol`, with file
paths relative to `crates/rdra-ish-core/src/lifecycle/` unless they start with `cli/`
(`crates/rdra-ish-cli/`), as in the [claims inventory](../theory/claims.md). **Implemented by**
notes are pointers, not statements.

---

### Notation

<!-- derived-from ../state-derivation.md#variables -->

- `Ent` is the finite set of entities of `M` that the lifecycle holds. An entity
  published with `exposes(System, E).append()` or `.upsert()` is not in `Ent`.
- For a set `X`, `X⊥` is `X ∪ {⊥}`, where `⊥` is the value `null`.
- `[k] = {0, …, k-1}`. Slot `i` of entity `E` prints as `E#(i+1)`.
- A predicate `φ` with free *binders* `x₁ … x_k` over entities `E₁ … E_k` is evaluated
  under a *binding* `b ∈ [n_{E₁}] × … × [n_{E_k}]`. `φ_b(s)` is its value in state `s`.
- `s →_a s'` is a step of action `a`, and `s → s'` is a step of some action. `Reach(T)` is
  the set of states reachable from the initial states of `T`.
- `UC` is a use case of `M`. Its *writes* are the lines naming it as the writer: `creates`,
  `updates`, `deletes`, `sets`, `drives`, `links`, `relinks`, `unlinks`, `transitions`,
  `chooses`, and the CRUD lines of every API an `invokes(UC, Api)` names.
- The *join key* between two entities is the one `relate` between them. When there are
  several, it is the one a `.role(r)` of the line names. Several keys and no `.role` is the
  error `AmbiguousOwnerLink`
  ([Which key a join follows](../language-reference.md#which-key-a-join-follows)).

### Bounds

<!-- derived-from ../state-derivation.md#bounds -->

<a id="d1"></a>**D1 (Bounds).** A bound is `β = (n, m)`. `n : Ent → {1, …, 255}` gives the
slots per entity, and `m ∈ {1, …, 255}` is the top of the integer range `[m+1] = {0, …, m}`.
`--instance-count` sets `n` uniformly, `--instances E=k` overrides one entity, an
`instances` table under the command's section of `rdra.toml` gives per-entity counts that
`--instances` overrides (`cli/src/export.rs::model_bounds`), and `--int-range-max` sets `m`. Every result the tools print names `β` on its `bounds:` line.
Defaults are `n ≡ 1`, `m = 5` for `states` and `check`, and `n ≡ 2`, `m = 5` for `verify` and
`export --kind tla`. A pair `(M, β)` is also ill-formed when some variable of D2 would have
more than 256 values: a nullable Int-like column at `m = 255`, a nullable Enum of 256
values, or an Enum of 257 (`DomainTooLarge`).

*Implemented by* `lifecycle::types::Bounds` (`instances`, `per_entity`, `int_max`,
`Bounds::STATES`, `Bounds::VERIFY`). Every call that turns the flags into bounds goes
through `cli/src/export.rs::checked_bounds` (for `check`, `states`, `verify`, and `export`).
It rejects `--instance-count 0` and `--int-range-max 0`, as well as values above 255. `Bounds::int_max` is a
public `u8` field, so a library caller could still build at `m = 0`. That bound is outside
D1 and has no meaning here. The domain-size check is `build/vars.rs::Builder::column_var`.

The z3 encoding of `verify --prove` reads neither `n` nor, for most Int terms, `m`
(theme (b)).

### State variables

<!-- derived-from ../state-derivation.md#variables -->

<a id="d2"></a>**D2 (Variables and domains).** `T_β(M)` has one variable per row of the
table below, held per slot of its entity. A variable `v` of entity `E` is therefore `n_E`
state components `v[0] … v[n_E - 1]`. `V` is the set of variables a given system holds.

An Int-like column (Int, Money, Decimal, Date, DateTime) is *read as a number* when any of
the following holds:

- a `sets` writes an integer to it;
- it is compared with an integer literal, with `now`, or by an order comparison (`<`, `<=`,
  `>`, `>=`) in a rule, a `@check`, a condition body that `applies_to` or `.when` uses, an
  `after` assertion, or a property;
- it is compared by `==` or `!=` with, or copied to or from, another Int-like column in a
  `sets` copy, a rule, or a condition.

A column is *named* when any of these, or a `chooses`, mentions it.

| Declaration in `M` | Variable | Domain | In `V` when |
|---|---|---|---|
| an entity `E ∈ Ent` | `Exists(E)` | `{absent, present, deleted}` | always |
| a column of type Enum | the column | its declared values, with `⊥` first under `@null` | always |
| a column of type Bool | the column | `{false, true}`; `{⊥, false, true}` under `@null` | always |
| a column of type Int, Money, Decimal, Date, or DateTime | the column | `[m+1]`, or `[m+1]⊥` under `@null` | it is read as a number |
| a column of such a type under `@null`, not read as a number | the column | `{⊥, present}` | always |
| a column of any other type under `@null` (String, Uuid, …) | the column | `{⊥, present}` | always |
| a column typed by a `variation` | the column | the variation's values | it is named |
| `drives(UC, E.c op r, b)` on a comparison `c op r` | the proposition | `{false, true}` | some `drives` names it |
| `relate(Child, Parent, …)` of shape N:1, 1:N, or 1:1 | the owner of `Child` | `[n_Parent]`, plus `⊥` under `.optional()` | always |
| a use case split into `k ≥ 2` transaction groups (D10) | the run mark `E._run_UC` | `{none, 1, …, k-1}` | a step before the last writes `E` |
| a tracked event `Ev` (D11) | the latch `E._raised_Ev` | `{false, true}` | `E` is a mark entity of `Ev` |
| `triggers(Ev, UC).counted(k)` (D11) | the counter `E._pending_Ev_UC` | `{0, …, k, over}` | `E` is a mark entity of `Ev` |
| a predicate reads `now` | `now`, one global component | `[m+1]` | always |

A state is an assignment `s` of a domain value to every component. Some columns have no
variable: a String column with no `@null`, a non-null Int-like column not read as a number,
a `@derived` column, a foreign-key column (the owner stands for it), and every column of a
published entity. A predicate that reads a Bool `@derived` column reads its defining
expression ([state-derivation.md, "Variables"](../state-derivation.md#variables)).

*Choice.* Two points follow the code (`build/naming.rs::Named`), because the notes say only
that such a column "exists when something names it". First, which columns are read as a
number. Second, that an Int-like `@null` column not read as a number is `{⊥, present}`. One
consequence: an `==` between two columns in a temporal property names them but does not
read them as numbers. The reference interpreter (`core/tests/reference/`) shares this
reading with the build, so claim I1 has no independent evidence for it.

*Implemented by* `lifecycle::types::VarKind`, `Domain`, `Domain::size`, and
`build/vars.rs`. The byte layout of a state (`TransitionSystem::offsets`, `width`) is not
part of the semantics.

<a id="d3"></a>**D3 (Base value).** Each variable has a base value `base(v)`:

- for a column, its `@default`; else `⊥` under `@null`; else the first declared Enum value,
  `false`, or `0`. On a `{⊥, present}` column, a non-null `@default` or a default function
  (`now()`, `uuid()`) gives `present`. On a number column, a `@default` that is no integer
  (`9.99`, a date) is schema text and gives the unset value;
- for `Exists`, `absent`;
- for a run mark, a latch, and a counter: `none`, `false`, and `0`;
- for a proposition of `drives`, `false`;
- for `now`, `0`;
- for an owner, `⊥` under `.optional()` and parent slot 0 otherwise. D4 overrides it on
  pre-existing children, and D8 sets it when a child is created.

A slot that is `absent` or `deleted` holds the base value in every column. A variation
column's base is its `@default`, else its first value. Only an absent or deleted slot shows
that base: a pre-existing row starts at any value (D4), and a created row takes any value the
use case does not set (D8). So the `@default` never pins a live row
(`DefaultOnVariationIgnored`).

*Implemented by* `VarDecl::base`.

### Initial states

<!-- derived-from ../state-derivation.md#variables -->

<a id="d4"></a>**D4 (Initial states).** An entity is *created* when some use case has
`creates(UC, E)`, directly or through an API that `invokes(UC, Api)` names, and
*pre-existing* otherwise. `I_β(M)` is the set of states in which every component is at its
base value, except that:

1. every slot of a pre-existing entity is `present`;
2. on a pre-existing slot, every variation column takes any value of its variation, chosen
   independently per slot ([vision.md](../vision.md#decisions) says why);
3. the owner of a pre-existing child starts by the shape of its `relate`:
   - **Any.** For an N:1 child, the owner takes any parent slot, chosen independently per
     slot, and also `⊥` under `.optional()`.
   - **Identity.** For a 1:1 child or the child of a `1..*:1` key, slot `i` is owned by
     parent slot `i`.
   - **Base.** The owner of a created entity, and an `.optional()` owner whose parent is
     created, start at base (D3): `⊥` (unlinked) under `.optional()`, and parent slot 0 on a
     mandatory key, which only absent rows hold.

   A mandatory owner of a pre-existing child whose parent is created still starts by Any or
   Identity, under a parent slot that is `absent`. The info
   `PreExistingChildOfCreatedParent` reports it.

`I_β(M)` is the product of these independent choices. No rule is assumed or checked to hold
in `I` before exploration. An initial state is a reachable state like any other, so a rule
an initial state breaks is violated with a trace of zero steps.

*Implemented by* `TransitionSystem::initial_states` (`explore/moves.rs`), `Birth`, and
`Start::{Any, Identity, Base}`.

### Predicates

<!-- derived-from ../language-reference.md#null -->

<a id="d5"></a>**D5 (Predicate evaluation).** A predicate is a comparison expression of the
language ([Comparison expressions in constraints](../language-reference.md#comparison-expressions-in-constraints)),
or a `.has`/`.none`/`.at_least`/`.at_most`/`.exactly` quantifier over related rows, combined
with `not`, `and`, and `or`. It is evaluated in two-valued logic: every predicate is true or
false in every state, and `not`, `and`, and `or` are Boolean.

- A quantifier over entity `E` ranges over the slots of `E` that are `present`. A count
  compares `|{i ∈ [n_E] : Exists(E)[i] = present ∧ φ(i)}|` with its bound.
- **Numbers and null.** When either side of a comparison is numeric (a column read as a
  number, an integer literal, or `now`), both sides are read as integers and `⊥` as "no
  number". `⊥` is `==` only to `⊥`, is `!=` to every number, and is never ordered. So when
  `x = ⊥`, `x < 3`, `x >= 1`, and `x == 3` are false and `x != 3` is true. Otherwise the two
  sides compare as values of one domain.
- **Two columns.** `==` and `!=` between two nullable columns hold only when both are
  non-null, as in SQL, so two `⊥` columns are not equal. `col == null` holds on `⊥`.
- **Owners.** An owner compared with a row is "linked to that row". An unlinked owner (`⊥`)
  is linked to no row.
- **Sets.** `t in (v₁, …)` is the disjunction of `t == vᵢ`, and `not in` is its negation.

A consequence the reader must keep in mind: `not (x < 3)` is true when `x = ⊥`, so `not`
does not commute with the SQL reading of a comparison. The language reference
([Null](../language-reference.md#null)) gives the full table: every comparison operator with
`⊥` on the left, the right, or both, and under `not`. The lint warning
`negated-order-admits-null` flags `not` over an ordering with a nullable operand in a
`@check` or a `property`, the only forms that take `not` (claim I2). A logical `@derived` column
is not a predicate under this definition: it reads its comparison as SQL does and is `⊥`
while an operand is `⊥`, under `not` too.

*Implemented by* `lifecycle::types::Pred`, `TransitionSystem::eval` (`expr.rs`),
`expr.rs::compare`, `build/preds/mod.rs::columns_cmp` (the two-column rule), and
`build/preds/atoms.rs::lists_whole_domain`; `lifecycle/tests/null_table.rs` computes the
reference's table through them and compares it with the page; `build/preds/checks.rs::passes`
is the `@derived` reading. `lists_whole_domain` folds a set that lists every value of a
finite domain to `true` and never folds an Int set. The fold does not
change a predicate's value under this definition.

### Actions

<!-- derived-from ../state-derivation.md#actions -->

<a id="d6"></a>**D6 (Action).** A use case with no write has no action. A use case with
writes has one action per *transaction group* of its writes. The groups, in order, are:

1. the writes of each invoked API, in `invokes` order;
2. the direct writes, split into the groups that `relate` keys connect;
3. each remaining isolated write.

A write to a published entity joins the use case's one internal group.

An action `a` has:

- parameters `p₁ … p_r`, one per entity its group writes plus the rows it reads (D7);
- guards `G_a`, a conjunction of predicates over the parameters (D7, D8, D9, D10, D11, and
  the guards below);
- effects `F_a` (D8), applied simultaneously to the pre-state.

**Conditions as guards.** `applies_to(Cond, UC)` with a `holds` body guards `UC`'s action,
or the first step of a split use case. `applies_to(Cond, Api)` guards the step that writes
through that API, in every use case that invokes it. The body's atoms read rows as follows:

- an atom on an entity the step writes reads that parameter, in the pre-state for an
  updated row and in the post-state for a created row;
- an atom on a parent the step only reads follows the join key;
- an atom on any other entity reads one existing row per entity, linked to the step's rows
  by a direct `relate` when there is one and independent otherwise. A row that no chain of
  links reaches is the warning `UnjoinedGuardRead`.

With `.along(A, B, …)`, each path entity the step does not bind is such a row, linked by each
hop's join key. `.has` and `.none` quantify over the related rows present in the pre-state
([state-derivation.md, "Conditions as guards"](../state-derivation.md#actions)).
`applies_to(Cond, System)` adds no guard.

*Implemented by* `lifecycle::types::Action` (`ActionOrigin::UseCase`, `Action::guards`,
`Action::effects`), `UsecaseTx::groups` (`tx.rs`), and `build/actions.rs`.

<a id="d7"></a>**D7 (Parameters and bindings).** A binding of `a` in `s` maps each parameter
to a slot. The role of a parameter comes from the line that introduces it:

| Line in `M` | Role | Admissible slot |
|---|---|---|
| `creates(UC, E)` | `New` | the lowest slot of `E` whose `Exists` is `absent`; none if there is none |
| `updates`, `deletes`, `sets`, `transitions`, `chooses`, `drives`, `links`/`relinks`/`unlinks` on an `E` the use case does not create | `Existing` | any `present` slot |
| a mandatory `relate` from a created child, a copy source that is the target's parent, a condition atom on a parent | `Parent{key}` | any `present` parent slot, with the guard that the child's owner on `key` names it, or for a created child, that the child is created under it |
| a row of `E` that an earlier step of a split use case wrote (D10) | `Earlier{i}` | the slot whose run mark is `i`, present or not |
| the first step of a handler with no row of a mark entity (D11) | `Triggered` | a `present` slot of a mark entity whose latch or counter admits the handler |

Two keys between one pair of entities give independent parent parameters. A parent the
action also writes is the child's parent only on the join key.

*Choice.* Taking the lowest absent slot for `New` is a canonical choice: slots are created in
index order, and a deleted slot is never reused (`Exists` never leaves `deleted`). The z3
encoding lets a creation take any absent row, which only adds steps
([Remarks](#remarks-on-the-definitions)).

*Implemented by* `ParamRole` and `explore/moves.rs::candidate_slots`.

<a id="d8"></a>**D8 (Effects).** Each construct contributes a guard and an effect on its
parameter `p`, applied to the pre-state `s`:

| Construct in `M` | Guard | Effect |
|---|---|---|
| `creates(UC, E)` | — | `Exists[p] := present`; every column of `p` stays at base unless a line below writes it |
| a variation column of a created row that `UC` does not write | — | one successor per value of the variation (a choice) |
| `deletes(UC, E)` | D9 | `Exists[p] := deleted`; every column of `p` returns to base; owners that name `p` keep naming it |
| `sets(UC, E.c == v)`, `v` a value, a number, `true`, `false`, or `null` | — | `c[p] := v` (a number `k` on a nullable column is `k`, not `⊥`) |
| `sets(UC, E.c == present)`, `c` of domain `{⊥, present}` | — | `c[p] := present` |
| `sets(UC, E.c == present)`, `c` a nullable Enum, Bool, or number column | — | one successor per non-null value of `c`'s domain, which for a number column is `[m+1]` (a choice) |
| `sets(UC, E.c == present)`, `c` a non-null column of any type | — | none |
| `sets(UC, E.c == vᵢ).when(Cᵢ)`, one or more lines for `c` | the step has some successor | one successor per line whose body `Cᵢ` holds, with `c[p] := vᵢ`; the body is read on the post-state for a created row and on the pre-state for an updated row |
| `sets(UC, E.c == F.d)` (copy) | `F`'s row `q` is a parameter of the step, an `Earlier` row, or `p`'s parent on the join key | `c[p] := d[q]`, with `d[q]` read before the step for an updated or earlier row and after it for a row the step creates |
| `drives(UC, E.c op r, b)` | — | the proposition of `c op r` on `p` becomes `b` |
| a created child under a parent the step binds | — | `owner[p] := q` |
| an existing child the step updates, under a parent the step creates | `owner[p] = ⊥` under `.optional()` | `owner[p] := q` |
| `links(UC, Child, Parent)` | `owner[p] = ⊥` | `owner[p] := q` |
| `relinks(UC, Child, Parent)` | `owner[p] ≠ ⊥` (none on a mandatory key) | `owner[p] := q` |
| `unlinks(UC, Child, Parent)` | `owner[p] = q` for the parent row the step or an earlier step of its run wrote, else `owner[p] ≠ ⊥` | `owner[p] := ⊥` |
| `transitions(UC, E.c, a -> b)`, one or more arms on `c` | `c[p]` is the source of some arm | one successor per arm whose source is `c[p]`, with `c[p] := b` (a choice when a source has several arms) |
| `transitions(UC, E.c, (a₁, a₂) -> b)` | as one arm per listed source | as one arm per listed source |
| `.when(C)` on an arm | — | the arm's successor is kept only when the body of `C` holds, read as for `sets … .when` |
| `chooses(UC, E.c)` | — | one successor per value of the variation |
| `chooses(UC, E.c, v₁, …)` with `.when(Cᵢ)` on values | — | one successor per listed value, a guarded value kept only when its body holds |
| a variable no line of the step writes | — | unchanged |

The effects other than choices apply together to the pre-state. Then every choice forks the
post-state, and two choices multiply. Arm, value, and copy guards filter the forked
successors. `Post_a(s, p)` is the set of successors of `s` under `(a, p)`. It is empty when
a pre-state guard fails or every successor is filtered out. A use case leaves one value per
column. Two unguarded values, a copy beside a value, or a `chooses` beside a `sets` of one
column is an error (`ConflictingEffects`, `ChoosesAndSets`), and so is any step that writes
one `(variable, parameter)` twice.

*Choice.* The notes do not say how a `.when` arm guard combines with a second write of the
same column in one step. Such a step is an error, so the question does not arise.

*Implemented by* `lifecycle::types::Effect` (`Create`, `Delete`, `Assign`, `Transition`,
`Choose`), `GuardWhy::{ArmCondition, Copy, Link}`, `build/sets.rs`,
`build/transitions.rs`, `build/actions.rs`, and `explore/moves.rs::each_successor`.

<a id="d9"></a>**D9 (Delete guards).** For each `relate(Child, E, …)` whose `.on_delete` is
unset, `restrict`, or `no_action`, a `deletes(UC, E)` is guarded by: no present child names
the slot, apart from a child the same step deletes, unlinks, or relinks. The lifecycle does
not execute `cascade`, `set_null`, or `set_default`. The children of such a key stay present
and name a deleted parent, and the key gets no `DanglingKey_*`. The SQL export does write
`ON DELETE CASCADE`, `SET NULL`, or `SET DEFAULT`, so a database built from it removes the
children or clears their key where `T_β(M)` keeps them. Generated Go, Rust, and TypeScript
code executes no action either: it lists an unchecked `OnDelete_*` obligation. The warning
`DeleteLeavesChildren` (`cascade`) and the warning `DeleteLeavesChildKeys` (`set_null`,
`set_default`) name each use case that deletes such a parent and neither deletes the child
nor, for the latter two, moves its key (claim I3). An `E` with a `@soft_delete` column gets no
guard and neither warning.

*Implemented by* `GuardWhy::Restrict`; the warnings by
`build/preds/entity_rules.rs::cascade_deletes`.

<a id="d10"></a>**D10 (Split use cases and runs).** A use case whose writes form `k ≥ 2`
transaction groups (D6) is the actions `UC_1 … UC_k`. Each entity that a step before the
last writes gets a run mark, `E._run_UC`. It records the last finished step of a pending run
on that row:

- `UC_1` binds only rows whose mark is `none` and sets the mark to `1`;
- `UC_i` (`i > 1`) binds each such row as `Earlier{i-1}` and sets the mark to `i`, or to
  `none` at `i = k`;
- any action may run between two steps, since each step is its own transaction.

A run never aborts. A step whose guard fails waits forever, which shows as a deadlock or a
use case that never runs. A delete does not reset the mark, so a run finishes on a row an
earlier step deleted. A condition applied to the use case guards step 1 only. A condition
applied to an API guards that API's step.

*Implemented by* `VarKind::Run` and `build/actions.rs` (`run_marks`).

<a id="d11"></a>**D11 (Triggers).** `raises(UC, Ev)` and `triggers(Ev, UC2)` order the two
use cases. An event is *tracked* when every `raises` of it names a use case with an action.
It is *free* otherwise: it has no `raises`, an external system or a timer raises it, or a
raiser writes nothing. The *candidates* of a tracked event are the entities that the last
step of every raiser binds. Its *mark entities* are the candidates some handler reads.

- **Latch.** The last step of every raiser sets `E._raised_Ev := true` on each of its
  parameters on a mark entity `E`. It never waits for the handler.
- **Guard.** The first step of a handler `triggers(Ev, UC2)` is guarded by the latch on its
  own row of `E` when it has exactly one row of `E` that can be the raiser's. Otherwise it
  binds a `Triggered` parameter (D7), linked to its rows by the one direct key when there is
  one.
- **Variants.** A use case triggered by several tracked events has one copy of its first
  step per event, `UC2_by_Ev`.
- **Free events.** A free event has no mark and orders nothing.

**The latch is never reset, by design.** Nothing sets the latch back to `false`: neither the
handler, nor a delete, nor a second raise. So after the first raise on a row, the handler may
run there any number of times, including zero, with no further raise. The order the latch
imposes is "not before the first raise on these rows", nothing more. This is the decision
recorded in [event-order.md](../design/event-order.md#decisions-in-one-place), question 1 and
rule 9, with the reasons in its answer 1. A modeler who needs one handler run per raise opts in with
`triggers(Ev, UC2).counted(k)`
([Counting raises](../language-reference.md#counting-raises-counted),
[counted-events.md](../design/counted-events.md)):

- **Counter.** On that edge a counter `E._pending_Ev_UC2` replaces the latch. A raise moves
  it from `i` to `i+1` and from `k` to `over`. The handler needs a counter other than `0` and
  moves it from `i` to `i-1`. `over` stays `over`, and the handler then runs as under the
  latch.
- **Overflow.** The state rule `EventOverflow_<E>_<Ev>_<UC2>` says no present row is at
  `over`.

*Implemented by* `VarKind::Raised`, `GuardWhy::Raised`, `InvKind::Overflow`, and
`build/events.rs` (`count_up`, `count_down`).

<a id="d12"></a>**D12 (Clock).** When some predicate reads `now`, the system has the action
`Tick`. Its guard is `now < m` and its effect is `now := now + 1`. A Date or DateTime column
is a point on that clock: `sets(UC, E.due == 3)` places it at 3.

*Implemented by* `ActionOrigin::Clock` and `build/vars.rs::Builder::tick`.

### The transition system

<a id="d13"></a>**D13 (Transition system).** `T_β(M) = (S, I, →)`, where `S` is the set of
assignments of D2, `I = I_β(M)` (D4), and `s → s'` holds iff some action `a` of D6, D10, D11,
or D12 and some binding `p` admissible in `s` (D7) give `s' ∈ Post_a(s, p)` (D8, D9). `S` is
finite for every `β`. A *step* is a triple `(s, (a, p), s')`. A step with `s' = s` is
possible (an action that rewrites a value with itself), and every liveness check treats it as
stuttering (D20).

*Implemented by* `TransitionSystem::build`. BFS (`check`, `states`, `verify`), the TLA+
printer (`export --kind tla`, `verify --with-tlc`), and the z3 encoding (`verify --prove`)
all read the one value it returns. None of them reads the model a second time. So the
engines share any departure of that value from `T_β(M)`, and agreement between them cannot
show one. That the value is `T_β(M, σ)` is claim [I1](../theory/claims.md#0-the-build).

<a id="d24"></a>**D24 (Scope).** A *scope* `σ` is the set of use cases a run keeps. `--buc`
keeps the use cases reachable from the named BUCs. `--without-usecase`, `--without-buc`, and
`--actor` leave use cases out. `T_β(M, σ)` is `T_β(M)` with every action of a left-out use
case removed, and `T_β(M) = T_β(M, ALL)`. `--entity` removes no action; it selects which
checks run. Every verdict of a scoped run is a verdict of `T_β(M, σ)`, and the `OK:` line
says `in the selected scope`. The definitions below are stated for `T_β(M)` and apply to
`T_β(M, σ)` unchanged. Theme (b) cites D24 for what a proof under a scope covers (claim B2).

*Implemented by* `select.rs::Scope`, passed to `TransitionSystem::build`.

<a id="d14"></a>**D14 (Behaviors).** A *finite trace* is `s₀ → s₁ → … → s_k` with `s₀ ∈ I`. A
*behavior* is an infinite sequence `σ = s₀ s₁ …` with `s₀ ∈ I` in which, for each `i`,
`s_i → s_{i+1}` or `s_{i+1} = s_i` (a stuttering step), as in TLA+. A behavior *stops* at
`s` when from some point on it stutters at `s` forever. A state with no successor other than
itself is *terminal*, and a behavior may stop there under every fairness assumption below.

<a id="d15"></a>**D15 (Cone).** Let `W ⊆ V` be a set of variables closed under the reads of
the actions that write it (`cone.rs`). `T|W` is the system restricted to `W`: actions that
write nothing in `W` are dropped, the others keep their effects on `W`, and states are
projected to `W` (`TransitionSystem::restrict_to`). Theme (a) defines the closure. It states
when `Reach(T|W) = π_W(Reach(T))` holds and when only `⊇` holds. This document needs only
the definition, because every verdict below is defined on `T_β(M)` itself.

### State rules

<!-- derived-from ../state-derivation.md#checks -->

<a id="d16"></a>**D16 (State rule).** A state rule has binders `E₁ … E_k` and a body `φ`. It
holds in `s` iff `φ_b(s)` holds for every binding `b` of present slots. `T` satisfies it iff
it holds in every `s ∈ Reach(T)`. A rule over an entity with no present slot holds. Every
rule form denotes one: `forbidden`, `invariant`, `required`, `exclusive` (local and
multi-entity, where `.along` adds owner equalities to the body's guard), quantifier rules,
`@check`, `@unique` and N:M `.via` (`Unique_*`), 1:1 keys (`Unique_<Child>_<role>`),
cardinality ranges (`MinCard_*`, `MaxCard_*`), restricting keys of a deleted parent
(`DanglingKey_*`), and counted triggers (`EventOverflow_*`)
([state-derivation.md, "Checks"](../state-derivation.md#checks)).

The bodies of the written forms, with conditions `c₁ … c_r` read by D5 and every binder
ranging over present slots
([language-reference.md, "Entity state constraints"](../language-reference.md#entity-state-constraints)):

| Form | Body `φ` | Name |
|---|---|---|
| `forbidden(E…, c₁, …, c_r)` | `¬(c₁ ∧ … ∧ c_r)` | `Forbidden_<E>_<n>`, `MultiForbidden_<E>_<n>` |
| `invariant(E…).when(g₁…).then(r₁…)` | `(g₁ ∧ …) ⇒ (r₁ ∧ …)` | `Inv_<E>_<n>`, `MultiInv_<E>_<n>` |
| `required(E, c₁, …, c_r)` | `c₁ ∧ … ∧ c_r` | `Required_<E>_<n>` |
| `exclusive(E, c₁, …, c_r)` | `∧_{i<j} ¬(c_i ∧ c_j)` | `Exclusive_<E>_<n>` |
| `invariant(E).when(g…).has(R.c…)` / `.none(…)` / `.at_least(k, …)` | `g ⇒ ∃r. link(e, r) ∧ c` / `g ⇒ ¬∃r. …` / `g ⇒ Count ≥ k` | `Quantifier_<E>_<n>` |

`.along(A, B, …)` conjoins the owner equalities of the path to the guard side, so the rule
speaks only of linked rows. A `.when(Cond)` adds the comparisons of `Cond`'s `holds` body.

*Implemented by* `lifecycle::types::Invariant`, `InvKind`, and
`expr.rs::find_violation`.

<a id="d17"></a>**D17 (After assertion).** `after(UC).assert(ψ)` holds iff `ψ` holds in `s'`
for every `s ∈ Reach(T)` and every step `s →_a s'` with `s' ≠ s`, where `a` is the *last*
step of `UC` (of every variant `UC_by_Ev`). In `s'` the action's parameters are bound to
their slots, and the scope entities the action does not bind are universally quantified over
present slots of `s'`. States between two steps of a run are not checked, and neither is a
last step that changes nothing. The native run skips the check on a self-loop
(`explore/bfs.rs`, `Probe::Loop`, in every build). TLC checks the `PROPERTY`
`[][(\A p : UC(p) => post')]_vars`, which every stuttering step satisfies. This is the
intended reading, as in TLA+: the language reference says so, and D18 reports an assert
whose last step only ever stutters as vacuous.

<a id="d18"></a>**D18 (Vacuity).** A rule *holds vacuously* when no reachable state meets one
of its premises
([state-derivation.md, "Rules that hold vacuously"](../state-derivation.md#rules-that-hold-vacuously)).
Vacuity is a warning about a rule that holds and never changes a verdict. An after-assert's
premise is a step of its use case's last step with `s' ≠ s`, the steps D17 checks. A last
step that runs only with `s' = s` is reported (`UnmetPremise::OnlyStutters`), and so is one
that never runs (`targets.rs::unmet_of_assert`).

### Temporal properties

<!-- derived-from ../language-reference.md#temporal-path-properties -->
<!-- derived-from ../language-reference.md#order-and-stability-stable-unchanged-precedes -->
<!-- derived-from ../language-reference.md#goals-on-present-rows-present -->

A property has binders `E₁ … E_k`, a link predicate `ℓ` (the owner equalities of `.along`,
or `true` without it), a formula, and a set of fair use cases. For a binding `b`, define the
*trigger*

```
trig_b(s)  ≝  (∀j. s(Exists(E_j))[b_j] = present)  ∧  ℓ_b(s)
```

`pres_b(s)` is the first conjunct alone. Operands are evaluated by D5 on the slot whatever its
existence, so on an absent or deleted slot they read base values. The property holds iff it
holds for every binding. *Implemented by* `lifecycle::types::Property` and
`lasso.rs::TransitionSystem::triggers`.

<a id="d19"></a>**D19 (Safety forms).** On `T`:

| Form | Holds iff | Code |
|---|---|---|
| `always(x)` | `∀s ∈ Reach(T). trig_b(s) ⇒ x_b(s)` | `ComponentRun::unsafe_state` |
| `stable(p)` | for every step `s → s'` with `s ∈ Reach(T)`: `trig_b(s) ∧ p_b(s) ∧ pres_b(s') ⇒ p_b(s')` | `TransitionSystem::step_breaks` |
| `unchanged(c₁…c_r).when(p)` | for every step `s → s'` with `s ∈ Reach(T)`: `trig_b(s) ∧ p_b(s) ∧ pres_b(s') ⇒ ∀j. c_j(s') = c_j(s)` | `TransitionSystem::step_breaks` |
| `precedes(p, q)` | for every finite trace `s₀ … s_k`: `trig_b(s_k) ∧ q_b(s_k) ⇒ ∃j ≤ k. trig_b(s_j) ∧ p_b(s_j)` | `ComponentRun::unlatched_state` |

`stable` and `unchanged` read the link before the step only. Steps with `s' = s` break
neither form. `precedes` is the safety property "`q` not before `p`" per binding, and its
TLA+ form is a latch `P_seen` per binding.

<a id="d20"></a>**D20 (Fairness).** For a property `P` with `.fair(UC₁, …)`, a *fair
instance* is a pair `(a, κ)`: an action `a` of some `UC_i` (every step of a split use case),
and the slots `κ` its non-`New` parameters bind. `(a, κ)` is *enabled* at `s` when some
binding of `a` that extends `κ` has a successor `s' ≠ s`. It is *taken* by a step `s → s'`
with `s' ≠ s` when some binding of `a` that extends `κ` moves `s` to `s'`. A step is a pair of
states, so it takes every fair instance that has such a move. A move that changes no variable
neither enables nor takes an instance. This is how `⟨A⟩_vars` reads it in TLA+ and how
`lasso.rs::TransitionSystem::unfair_loop` reads the moves. So a behavior that stays at one state forever through a self-loop of a fair instance does not take
it. Two fairness conditions on a behavior `σ` are used:

- `Fair_P`: every fair instance that is enabled at every state of `σ` from some point on is
  taken infinitely often. This is weak fairness per instance,
  `∧_{(a,κ)} WF_vars(∃ new slots : a(κ, …))` in the TLA+ output.
- `WF_vars(Next)`: if from some point on a step to another state is enabled at every state,
  infinitely many steps change the state.

There is no strong fairness. No `SF` form exists in the language, in the TLA+ printer, or in
`lasso.rs::Fairness`. Its two values are `Plain` (`Fair_P` alone) and `WfNext`
(`Fair_P ∧ WF_vars(Next)`). A property with no `.fair` has `Fair_P = true`. *Implemented by*
`Property::fair_instance`, `ParamRole::keys_fair_instance`, and
`explore/liveness.rs::fair_steps`.

<a id="d21"></a>**D21 (Liveness forms).** Let `goal` and `cause` be `goal = x`,
`cause = true` for `eventually(x)`, and `goal = y`, `cause = x` for `leads_to(x, y)`. Under a
fairness condition `F`, the property holds on `T` iff the following is true: for every
behavior `σ` of `T` that satisfies `F`, every binding `b`, and every `i` with
`trig_b(σ_i) ∧ cause_b(σ_i)`, some `j ≥ i` has `goal_b(σ_j)`. So `eventually(x)` is
`□(trig ⇒ ◇x)` per binding, not `◇x`, and `goal` is read at `j = i` too. `goal_b(σ_j)` does
not require `pres_b(σ_j)`. On a slot deleted between `i` and `j` it reads the base values D8
leaves, so a delete meets a goal those values satisfy and no other goal. This is the defined
semantics: language-reference.md, "Temporal path properties", states it, and the TLA+ formulas
`[](present => <>x)` and `(present /\ x) ~> y` read it the same way.

With `.present()`, the goal asks for a present binding:

| Form | `cause` | `goal` |
|---|---|---|
| `eventually(x).present()` | `true` | `pres_b ∧ x` |
| `leads_to(x, y).present()` | `x` | `pres_b ∧ y` |

The definition above applies unchanged with this `goal`. `trig_b` is the same, so the link
`ℓ` stays in the trigger only. A deleted slot is never reused (D7), so once a bound slot is
deleted at some `k ≥ i` before any `j` with `goal_b(σ_j)`, no later `j` has `pres_b(σ_j)`,
and the behavior fails the property if it satisfies `F`. The TLA+ forms are
`[](present /\ link => <>(present /\ x))` and `(present /\ link /\ x) ~> (present /\ y)`.
`.present()` on any other form is the error `PresentOnNonGoalProperty`. *Implemented by*
`build/preds/temporal.rs::Builder::goal`, which conjoins `pres_b` into the goal predicate,
so the native search, the lasso check, and the TLA+ printer all read one predicate. `pres_b`
reads only the binders' `Exists` variables, which every property's cone already holds
(`cone.rs::check_reads`). So `.present()` changes neither the cone nor the reduction, and
the symmetry scan reads no row id in it.

The verdict a property is meant to have is the one under `WfNext`. The exported `Spec`
conjoins `WF_vars(Next)` whenever the model has an `eventually` or `leads_to`, and `P` is
printed as `Fair_P ⇒ (…)`. `Plain` is a stronger requirement (more behaviors). So `ok` under
`Plain` implies `ok` under `WfNext`, and a counterexample under `WfNext` is one under `Plain`.
`formal-verification.md`, "Temporal properties on a cone", uses exactly these two
implications to order its three runs. Theme (c) states how the native checker decides each
run (`ComponentRun::decide_liveness`).

<a id="d22"></a>**D22 (Recoverability).** `can_reach(g)` holds iff

```
∀s ∈ Reach(T). trig_b(s) ⇒ ∃ s = t₀ → t₁ → … → t_r.  (∀j. pres_b(t_j)) ∧ g_b(t_r)
```

That is CTL `AG (trig ⇒ E[pres U (pres ∧ g)])` per binding, decided on the explored graph by
one backward pass per binding (`reach.rs::reach_failure`, which asserts a complete graph in
every build). The trigger alone reads the link. No fairness applies, and an instance
may also loop forever. *Implemented by* `lifecycle::types::Reach`.

### Verdicts

<!-- derived-from ../formal-verification.md#what-verify-checks -->

A verdict is a statement about `T_β(M, σ)` and one property or rule. For `proved`, it is a
statement about every `T_{β'}(M, σ)` in the proof's scope. Below, "within `β`" means "of
`T_β(M, σ)`". Every word the tools print for a check is one of these. A verdict means what
this table says only as far as the build returns `T_β(M, σ)` (claim
[I1](../theory/claims.md#0-the-build)). The tools' own output says nothing about that claim.

<a id="d23"></a>**D23 (Verdict vocabulary).**

| Printed | Where | Meaning |
|---|---|---|
| `ok` | a cone or property line of `verify` (`cli/src/tlc_output.rs::Outcome::Ok`) | The checks of the line hold on `T_β(M)` (D16, D17, D19, D21 under the fairness of D20, D22). For a line of a cone the verdict is derived from the cone, and theme (a) owns the step from cone to model. |
| `ok (checked in Cone_X)` | a covered safety cone (`cli/src/verify/bfs.rs`) | `ok`, decided by the run of a cone `Cone_X` that holds every variable, action, guard, and parameter of this one. |
| `counterexample` | a line, or `result:` (`RunResult::Counterexample`) | A finite trace (safety) or a lasso (liveness) of the explored system that breaks a check. It is a behavior of `T_β(M)` except where theme (a) says otherwise: a counterexample of a cone that frees slots or leaves a raised mark out counts, for a state rule or `after`, only once a refined or exact cone vouches for its trace (claim A2), and for a property only once the whole model decides it (claim A6). Every printed trace replays through the step relation of the system it is about before it prints; one that does not is a `tool_error` (claim N16). |
| `violated` | `property P (BFS on its cone): violated`; `can_reach (BFS on N cones): k checked, m violated` (`cli/src/verify/bfs.rs`) | `counterexample`, in the lines of the forms only BFS decides (`stable`, `unchanged`, `precedes`, `can_reach`). |
| `not decided` | a line (`cli/src/verify/native.rs::Answer::Stopped`, `RunResult::NotDecided`) | The run stopped before it found a counterexample. No claim is made either way. The line's note names the stop: `stopped at the state cap (--max-states) or the memory limit`, `stopped at the state cap of N states`, `stopped at the memory limit of L`, `stopped at the time budget of S s`, or, for a property its cone cannot decide, `needs the whole model's WF_vars(Next)` and `its counterexample may create instances past the bound`. |
| `tool_error` | a line or `result:` | A tool fault, not a fact about the model: a counterexample or trace that does not certify itself (`LassoFault` for a property, `LiftFault` for a state rule or `after`, `TraceFault` for a deadlock, a `can_reach` failure, or a witness), a disagreement between the native checker and TLC, or a TLC failure without a violation. |
| `result: ok` | last lines | Every check of the run is `ok`, with no tool error, nothing undecided, and, under `--deny-unproved`, nothing unproved (`cli/src/verify/end.rs::End::result`). A trace that fails to replay counts as a tool error there. |
| `result: not decided` | last lines | No failure and no tool error, but some finding a cap or budget left open, or an unproved rule under `--deny-unproved`. The exit status is 0 unless `--deny-undecided` (or `--deny-unproved`) is given. The `OK:` line then holds only for what was decided. |
| `MemoryCapReached_*`, `StateCapReached_*`, `PatternCapReached_*` | `check`, `states` | An error: the cone stopped, so its rules are not checked. `check` does not report it as a pass. |
| `proved: R` | `verify --prove` (`prove/mod.rs::Verdict::Proved`) | `R` belongs to a set of formulas that is inductive for the z3 encoding of the built system. That encoding widens D13: any absent row for `New`, and Int terms and `now` over all non-negative integers. Hence, if z3 is sound and the hypotheses of [z3.md, Theorem 1](../theory/z3.md#the-soundness-theorem) hold, `T_{β'}(M, σ) ⊨ R` for every `β'` in the proof's scope `Sc(M, β)` ([z3.md, Definition 3](../theory/z3.md#the-soundness-theorem)). That scope contains `β` whenever the run builds, and it is every bound at which the model builds; its conditions are owned by z3.md (claim B2). The `proved:` line, like every line that claims a proof, prints that scope: `for any number of rows and every --int-range-max from L to M`, where `L` and `M` are the least and the greatest `m'` at which the build reports no error (`cli/src/prove.rs::Claim::of`). Under a scope `σ` (D24) the encoded system is the scoped one, and the claim ends `in the selected scope: <flags>`; otherwise it ends `in the whole model`. |
| `unproved: R: not inductive; …` / `unproved: R: z3 gave up on A` / `unproved: R: an initial state violates it` | `verify --prove` (`Verdict::Unproved`, printed by `cli/src/prove.rs`) | Not proved. `NotInductive`: a counterexample to induction exists from a state where `R` and every kept candidate hold, not necessarily reachable; the printed line says so ("it, every proved rule, and every kept lemma hold"). `Unknown`: z3 answered `unknown` or ran out of its resource limit on the step of action `A`. `Initial`: an initial state of the encoding, possibly with more rows or larger Int values than `β`, breaks `R`. None of these says anything about the bounded verdict. |
| `out of scope: R: why` | `verify --prove` (`Verdict::OutOfScope`) | The encoding cannot state `R`. No claim. |
| `refuted: R (BFS counterexample at k instances)` | `verify --prove` (`Verdict::Refuted`) | A safety cone of the same run reached a state that breaks `R`, so the prover did not try it (`cli/src/verify/refuted.rs`). The violation is BFS's error, not an unproved rule. Only a violation the cone vouches for, or one a rerun found again (`recheck_cones`, the refinement loop of claim A2), refutes a rule. `k` is the count of every entity of the cone when they share it; when the counts differ (under `--instances E=k`, or because the trace creates more rows of an entity the cone does not count than the bound), each is printed, as `Order=1 Payment=2 instances` (`cli/src/state_findings.rs::Instances`). |
| `escalated: R: ok at k instances` / `counterexample at k instances` / `not decided at k instances` | `verify --prove` (`cli/src/verify/escalate.rs`) | For a rule BFS passed and the prover left unproved, BFS on its cone at one and two more instances per entity. `counterexample` there fails the run. It is a violation of `T_{β'}(M)` for some larger `β'` under the conditions of `counterexample` above. A counterexample the cone cannot vouch for, one that runs a use case without its event or creates more instances than the cone counts, is decided again (`recheck_on`) and its instances are printed: `k` is one number when every entity of the cone that decided has it, else each entity's own, as `ok at Loan=3 Member=2 instances` (claim A25). |
| `decided` | prose, design notes | Not printed as a verdict. A check is decided when its verdict is `ok` or `counterexample`. |

`stopped at the memory limit` and `stopped at the state cap` describe why a run is
`not decided`; they are not separate verdicts.

<a id="d25"></a>**D25 (Deadlock finding).** For an entity `E`, a lifecycle column `v` of `E`,
and a slot `k`, a reachable state `s` of the explored system with row `k` present is
*stuck at `v`* when no path from `s` takes an edge that changes `v` at `k` or deletes row
`k` (`findings.rs::TransitionSystem::can_leave`). A value `x` of `v` is *open* when `@end`
does not name it and some `transitions` arm of `v` leads from `x` to a value other than `x`
(`findings.rs::deadlocks`, which keeps a value only when `has_way_on` holds of its `exits`).
Every other value is an end state: one `@end` names, one no arm leaves, one whose only arms
lead back to itself, and one whose only way out is a use case that deletes the row. Such a
value gets no deadlock finding, even where a state holds it with no path out.
`Deadlock_<E>_<v>_<x>` reports the first reachable state, in BFS order, that is stuck at `v`
with row `k` holding an open value `x`. A deletion counts as leaving `x` only once `x` is
open: it is one of the edges `can_leave` reads, never what makes `x` open. The definition reads
paths, not single steps: a state whose only paths run forever through steps that leave `v`
and row `k` alone, a silent cycle, is stuck exactly as a state with no step left is. This is
on purpose. It is the reading the cone reduction keeps, since a divergence-blind stuttering
bisimulation keeps whether a path reaches an edge that changes `T_v` ([cones.md, A8,
consequence 3](../theory/cones.md#the-reduction-by-an-independent-sub-lifecycle)) and does not keep
whether a state has a successor. TLC's
deadlock is the other reading: a state with no successor at all. The two differ on a silent
cycle, and the engine corpus records the cases (`cone_reduced_loop_unreached`).

---

### Prose that differs from the definitions

Each row is a place where modeler-facing prose or a code comment differs from D1 to D25, with
how the two relate. In each case the definition follows the code. A departure of
the code from the definitions is a different matter: claim [I1](../theory/claims.md#0-the-build)
covers it. None of these rows is a soundness problem by itself.

| # | Prose | Definition and code | Status |
|---|---|---|---|
| S1 | The doc comments of `crates/rdra-ish-core/src/model/constraints.rs::TemporalFormula::Eventually` and `::LeadsTo` give `<>expr` and `p ~> q`. | D21: `□(present ∧ link ⇒ ◇x)` per binding (`lasso.rs::fails_at`, `explore/liveness.rs::liveness_search`). | `language-reference.md`, "Temporal path properties", and `formal-verification.md`, "Mapping", state the code's form: `eventually(expr)` is `[](present => <>expr)` and `leads_to(p, q)` is `(present /\ p) ~> q`. |
| S2 | `state-derivation.md`, "Variables": Int columns range over `0..5`. | D2: `[m+1]`, with `m` from `--int-range-max`; 5 is only the default. | The language reference and state-derivation.md, "Bounds", mention `--int-range-max`. Listed so that theme writers quote `m`, not 5. |

### Remarks on the definitions

- **Operands on absent rows.** D21 reads `goal` on a slot whatever its existence (D22 does
  not: its path, goal state included, keeps the row present), and D8 resets a deleted slot's
  columns to base. So `eventually(Order.status == draft)`, with `draft` the base value, is met
  by deleting the order. TLC reads the TLA+ formula the same way (`[](present => <>x)`), so
  the engines agree. language-reference.md, "Temporal path properties", says so, with an
  example each way and how to state a goal a delete does not meet. `.present()` (D21) states
  such a goal per property.
- **Initial states are not assumed to satisfy anything.** D4 makes the initial set a product.
  That includes N:1 owners of pre-existing children under a created parent that is still
  absent (`PreExistingChildOfCreatedParent`).
- **Lowest-slot creation is a symmetry choice.** D7 creates in index order. The TLA+ module
  does the same (`FirstAbsent`). The z3 encoding lets a creation take any absent row
  (`formal-verification.md`, "Proofs for any number of rows"), which only adds steps.
- **A latch, not a queue.** D11's latch lets a handler run any number of times after one
  raise, and a raiser never waits. For safety this over-approximates every consuming
  delivery. It can hide only a liveness or reachability finding that needs "one run per
  raise", which `.counted(k)` states.
- **The definitions are parametric in `β`.** `n` enters D2 (key domains), D4 (Identity
  starts), and D7 (slots). `m` enters D2 (Int domains), D8 (`present` on a number), and D12
  (`Tick`). Nothing else depends on `β`. Theme (b)'s Lemma U uses this.


# Chapter 2–4 Cones of influence, the sub-lifecycle reduction, instance symmetry

*Source: `docs/theory/cones.md`.*

## Cones of influence, reductions, and symmetry

<!-- derived-from ./semantics.md -->
<!-- derived-from ./claims.md#a-cones-of-influence-and-reductions -->

This document states, for rows A1 to A28 of the [claims inventory](../theory/claims.md), when a run
on a smaller system decides what `T_β(M)` (D13) would: a cone of influence, a cone reduced by
an independent sub-lifecycle, a run up to slot permutations, and a run at more instances. It
uses the notation of [semantics.md](../theory/semantics.md) and does not redefine it.

This document describes main at `91524792`. Code is cited as in
[semantics.md](../theory/semantics.md): paths are relative to `crates/rdra-ish-core/src/lifecycle/`
unless they start with `cli/` (`crates/rdra-ish-cli/`), `emit/` (`crates/rdra-ish-emit/src/`)
or `core-root/` (`crates/rdra-ish-core/src/`). Test marks, such as "(main only)" and
"(release jobs)", are those of the [claims inventory](../theory/claims.md#how-to-read-a-row). Every
statement about behavior is a reading of the code, and every measured number is quoted from
the doc comment or test named beside it.

Each result has four parts: the statement, its hypotheses with the code that establishes
each one, the proof (marked **Proof** when every step is given, **Proof sketch** when a step
is named but not derived), and the evidence that checks it. A step that does not go through,
or a guarantee the code does not give, is named where it arises. The evidence classes and
open gaps of each claim are in the rows of the
[claims inventory, section (a)](../theory/claims.md#a-cones-of-influence-and-reductions).

---

### Contents

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

### Notation added here

<!-- derived-from ./semantics.md#notation -->

`T = T_β(M) = (S, I, →)` throughout, with variables `V` (D2) and actions `A` (D6). Read `T`
as the system the build returns: the theorems here relate a cone to that system, and some
steps use facts about the builder's output. That it is `T_β(M)` of D13 is claim
[I1](../theory/claims.md#0-the-build), which no theorem here needs. For an
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

`Rd`, `Mk` and `Cp` take each guard's reads from `cone.rs::TransitionSystem::pred_reads`. Three
Kani harnesses in `cone.rs` prove that it covers what `eval` reads: two states, and two
successors, that agree on `pred_reads(e)` give `e` the same value under any binding, for atoms,
`Not`, `And` and `Or` of at most one part, and one quantifier, on a system of 11 state bytes
([engine-agreement.md, Kani harnesses](../engine-agreement.md#kani-harnesses)). That a step
changes only `Wr(a)`, and that what it writes is a function of `Rd(a) ∪ Wr(a)`, has no harness:
debug and `shadow` builds check the first on every step they take (`check_writes`, claim A1).

For `W ⊆ V`, `π_W : S → S|W` projects a state onto `W` (`TransitionSystem::project`), and
`π_W(σ)` applies it to each state of a trace or behavior. Two behaviors are *stutter
equivalent* when they agree after every finite run of repeated states is collapsed to one
state.

`A_W(K) = K ∪ {a ∈ A : Wr(a) ∩ W ≠ ∅}` is the set of actions *taken* by `W` with kept set
`K`.

---

### The cone of a seed

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

### Projection and lifting

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

### Safety cones

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
has a property. `::Deadlock` says that a reachable state is stuck: no path from it changes the lifecycle column or deletes the row ([semantics.md, D25](../theory/semantics.md#d25)). An over-approximating `C`
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
Theorem A8, as every reduced cone's verdict does: proved for a flat reduction relative to its
hypotheses, and sketched for a nested one or one run up to `H`. No test names A27.

---

### Property cones and fairness

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
   (`R ⊨ P ⇔ C ⊨ P` under `Plain`), then Theorem A4.1, then D21. A8 is proved for a flat
   reduction relative to its hypotheses ((B1)–(B3), which every build certifies on the
   partition it uses, and Lemma 6, from the fields of `Lemma6Hyp`), and sketched for a nested
   one. When nothing reduces, `reduced` is `C` itself and A4.1 alone suffices.
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
  (B1)–(B3), certified at run time, and Lemma 6; it is proved for a flat reduction and
  sketched for a nested one or one run up to `H`.

*Evidence for A4 to A6.* Theorem A4, a statement about the unreduced cone `C`: P-full, given
Lemmas 2 to 4. A5.1, A6, and A5.3 on a reduced cone: the printed verdict is read on the reduced cone and
rests on Theorems A8 and A15, so P-full for a flat reduction, relative to their hypotheses, and
P-sketch for a nested one or one run up to `H`.
A5.2: P-full. D:
`cli/tests/engine_equivalence.rs::a_property_cone_decides_what_the_whole_model_decides` (release jobs); corpus rules 1
to 3 (`cli/tests/engine_corpus.rs`); `core/tests/explored.rs::a_cone_that_frees_slots_decides_a_violation_on_the_whole_model`;
the unit tests in `cli/src/property_cone.rs`.

---

### `can_reach` cones

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

### The reduction by an independent sub-lifecycle

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

*Hypotheses.* The proof reads `split_at`'s result as a pair `(T_v, T_a)` closed under the
conditions of Definition 3 (`Split.lean::Closed`): the anchor and the counters are in `T_v`; an
action that writes something or is kept, and that is kept, writes `T_v`, or reads a
non-monotone variable of `T_v`, is in `T_a`; every variable a `T_a` action writes is in `T_v`;
a variable is in `T_v` iff its entity's existence is. It also takes three facts of the folds
and of `raw_reads` (`Split.lean::FoldData`):

- `raw_rd`: an action's raw reads are among its `Rd`;
- `cover`: every `T_v` read of an `S_a` action is a raw read, the existence of a raw `T_v` read
  (a `Triggered` row, whose existence `existences` of its mark brings back), or a read its fold
  takes;
- `ex_mono`: the existence of a monotone variable is monotone.

*Proof.* Each part is one closure condition or definition (`Split.lean::lemma5_of_closed`).

- (a) Let `a ∈ S_a` write `v`. If `v ∈ T_v`, then `a` writes something and writes `T_v`, so the
  closure puts `a` in `T_a`, which `S_a` excludes. So `v ∈ S_v` (`s_writes_s`).
- (b) A `T_a` action writes only `T_v` by the closure. A read of it in `S_v` is in `obs` by the
  definition of `obs`.
- (c) Let `a ∈ S_a` read `v ∈ T_v`. By `cover`, `v` is a raw read of `a`, and so an input; or
  the existence of a raw `T_v` read of `a`, which `inputs` holds with its existences; or a read
  of `a`'s fold.
- (d) Let `u` be a raw `T_v` read of some `a ∈ S_a`. Then `u ∈ Rd(a)` (`raw_rd`). If `u` were
  not monotone, `a` would write something and read a non-monotone variable of `T_v`, so the
  closure would put it in `T_a`. So `u` is monotone. An input is such a `u` or an existence of
  one, which `ex_mono` makes monotone.
- (e) A seed's read in `S_v` is in `obs` by the definition of `obs`. A kept action is in `T_a`
  by the closure.

`obs ⊆ S_v` and `inputs ⊆ T_v` hold by definition. ∎

`split_at` returns such a closed pair. One pass applies every condition at once, and passes
from the anchor and the counters reach a pair that a pass leaves unchanged, which is closed
and is the least closed pair (`Split.lean::computed_iff`). `split_at`'s in-place loop applies
the conditions one insert at a time, in some order. A closed pair whose members all follow
from the conditions is that least pair, whatever the order (`Split.lean::mem_eq_of_sound`).
That each insert of the loop applies one condition is read from the code. The run-time certificate
below checks (c) and (d) on the split directly, so a split that reaches the reduction meets them whether or not the facts hold.

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

*Evidence.* P-full relative to the closure of `split_at`'s result and the three facts of the
folds (the proof above); R: the certificate above. M: `proofs/lean/RdraIsh/Split.lean` proves the lemma in Lean over an action-level
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
meet `FoldData` is a hypothesis. M also: `scripts/check-lean-certificates.sh` dumps every
top-level split of the samples, the domain samples and the corpus with its actions' reads and
writes, and runs on each a checker proved sound for `Lemma5` in Lean
(`proofs/lean/RdraIsh/Lemma5Check.lean::check_sound`); it runs by hand, not in CI.

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

**Lemma 6 (the extended system covers the cone).** Write `C°` for `C` without the steps of
the actions outside `T_a` that write nothing. For every `(t, s) ∈ Reach(C)`:

1. an initial `(t₀, s₀)` of `C` gives an initial `x(t₀, s₀)` of `X`, and `x(t, s) ∈ X_n` (up
   to the inner reduction's canonical form);
2. (`C ⇒ X`) an `S_a` step `(t, s) → (t, s')` of `C°` by `a` at binding `b` is a silent edge
   `x(t, s) → x(t, s')` of `X` by `a` at `b`;
3. (`X ⇒ C`) a silent edge `x(t, s) → (s', inp(t))` of `X` by `a` at `b'` is an `S_a` step
   `(t, s) → (t, s')` of `C°` by `a` at some `b` that equals `b'` outside the fold's free `T`
   parameters (the `T` parameters of `a`'s fold other than its key);
4. a `T_a` step `(t, s) → (t', s)` is matched by one rise of `X` per stage of
   `Reduction::stages(inp(t), inp(t'))`, from `x(t, s)` to `x(t', s)`. A step that keeps the
   input bytes has no stage.

`C` and `C°` reach the same states. A step of an action that writes nothing changes nothing,
since an effect leaves every variable outside `Wr(a)` as it is (`Local`). `X` holds no such
action, since `restrict_to` drops an action whose effects write nothing in `S_v ∪ inputs`
(`Lemma6.lean::coneW_reach_iff`). Items 2 and 3 with `C` for `C°` differ only by those
stutters.

*Hypotheses.* The proof is for a flat reduction (no inner reduction of `X`, and `X` not run up
to `H`) and a fold of a parameter (`FoldKey::Param`) with a bit of its own. It reads `C` and `X`
as `proofs/lean/RdraIsh/Restrict.lean` models them:

- a parameter's role is a guard conjunct on its entity's existence (`candidate_slots`);
- each parameter's entity is on a *side*: `S` (existence in `S_v`), *input* (existence in
  `inputs`), *stub* or *dropped* (Definition 4);
- `X` gives a parameter a role (`xrole`): kept on `S` and on an input; `Earlier { step: 0 }`
  for a fold's key off `S` and for a stub parameter of a rise or a folded action whose role
  needs a present row; none on a dropped entity and for `New` on a stub;
- `X`'s guard is those roles, the conjuncts `restrict_to` keeps, and the fold's bit at the
  key's slot (`xguard`);
- a silent edge is an `S_a` action under that guard; a rise is a `T_a` action that writes an
  input, under that guard, which writes the raw inputs, or the free flip of one bit;
- `X`'s initial states are the projections of `C`'s (the initial states below show that the
  code builds them so), and `X` reads a variable outside `S_v ∪ inputs` at the value of its
  first initial state, so a stub reads nothing of `t`.

It takes these facts of every action `a`, the sixteen fields of `Restrict.lean::Lemma6Hyp`. The
last column says how the code meets each one: R and M as in the claims inventory, and "read"
for a reading of the code that this section gives and nothing checks.

| Field | Fact | How the code meets it |
|---|---|---|
| `local_` | `Local`: a guard conjunct reads only `Rd(a) ∪ Wr(a)` and depends only on what it reads; the effect leaves every variable outside `Wr(a)` as it is, and its value on `Wr(a)` depends only on `Rd(a) ∪ Wr(a)` | M: proved for the action model read off the slot syntax of Theorem A18 (`SymActModel.lean::local_`, under `Built`); Kani proves that a predicate reads only its `pred_reads` (T13); a debug or `shadow` build holds every step to `Wr(a)` (A1). That the code's actions are that syntax is read |
| `lemma5` | Lemma 5 (a)(b) | R: `split_faults`; the proof of Lemma 5 above |
| `reads` | Lemma 5 (c), with a fold's reads those of `Fold::reads` | R: `split_faults` checks (c) with `Fold::reads` |
| `shape` | `a`'s guard is its role conjuncts followed by its other conjuncts `og` | M: proved for the slot model (`SymActModel.lean::shape`); that `candidate_slots` filters by role before the guards is read |
| `new_writes` | a `New` parameter's entity's existence is in `Wr(a)` | M: proved for the slot model under `Built` (`SymActModel.lean::new_writes`); that a `New` row comes with its `Create` is read from `build/actions.rs` |
| `kept_rd` | a conjunct `X` keeps reads only `S_v ∪ inputs` | read: `restrict_to` keeps a guard only when it reads `S_v ∪ inputs` (P2) |
| `cover` | every conjunct in `og` of an `S_a` action is kept or folded | R in part: a guard `fold_of` does not fold is read raw (`raw_reads`), and `split_faults` checks that every `T_v` variable an `S_a` action reads raw is an input, so the guard reads `S_v ∪ inputs`. Read: it names no `T` parameter but the key (`fold_of`), which `StubCheck` checks `restrict_to` keeps, and no other dropped parameter (the tables of "Parameters"), so `restrict_to` keeps it |
| `rise_eff` | a `T_a` action's value on an input depends only on `S_v ∪ inputs` | read: step (c) below |
| `fold_sa` | a fold is of an `S_a` action | R: `split_faults` |
| `folded_og` | a fold's folded guards are guards of its action | read: `fold_of` collects them from the action's guards |
| `exact` | the bit at slot `c` holds at `t` iff some binding with the key at `c` puts each `T` parameter whose role needs a present row on one and makes every folded guard true; stated for every slot number, where the code has bits for the key entity's slots alone | read: step (a) below; M: `Twin::FoldHolds` checks `holds_fast` against its reference |
| `folded_names` | the folded guards depend only on the slots of the `T` parameters and the key | read: `fold_of`'s `s_keys` (step (a), point 3) |
| `other_names` | no other guard depends on a free `T` parameter's slot | read: `fold_of` gives up on any other guard that names a `T` parameter, unless it is the link `v[k] = Index(p)`, and then `p` is the key |
| `eff_names` | no effect depends on a free `T` parameter's slot | read: `fold_of`'s `touched_t`: the one `T` parameter an effect may reference is the key |
| `eff_s` | a folded action's effect on `Wr(a)` depends only on `S_v ∪ inputs` | read: its effects write `S_v` (Lemma 5 (a)) from a literal, a slot, the variable's own value or a listed choice, the forms of step (c) |
| `bind` | `bind_stub_params` runs | R: `extended.rs::StubCheck` checks the built `X` against it |

D for all of them at once: `reduce/lemma6.rs::check` and `lemma6_steps_hold` check the lemma
itself on the tested cones (Evidence below).

*Proof.* Steps 1 to 10, each with the Lean theorem it corresponds to
(`proofs/lean/RdraIsh/Lemma6.lean`, namespace `Act.Ext`) and the fields it uses.

1. *Agreement* (`xval_inp`). `x(t, s)` and `(t, s)` give every variable of `S_v ∪ inputs` the
   same value: `x(t, s)` holds `s` and the raw inputs of `t`.
2. *Guards* (`role_of_guard`, `og_of_guard`, `guard_of`; `shape`). `a`'s guard holds at a
   binding iff each parameter's slot is a candidate of its role at the value of its entity's
   existence, and every conjunct of `og` holds.
3. *Kept conjuncts* (`kept_iff`; `kept_rd`, `local_`). A conjunct `X` keeps reads only
   `S_v ∪ inputs` and depends only on what it reads, so by step 1 it has the same value at
   `(t, s)` and at `x(t, s)`, at every binding.
4. *Sides of an unfolded action* (`unfolded_side`; `shape`, `local_`, `reads`, `lemma5`). A
   parameter of an unfolded `S_a` action whose role reads its row is on `S` or an input. Its
   role conjunct reads its entity's existence, which is in `Rd(a) ∪ Wr(a)` (`Local`). In
   `Rd(a)`, it is in `S_v` or `inputs` by (c), since `a` has no fold. In `Wr(a)`, it is in
   `S_v` by (a). This derives the stub and dropped cells of the unfolded table.
5. *Roles* (`xrole_cand`, `hstub_sa`, `hstub_ta`; `bind`). Let a slot be a candidate of a
   parameter's role at `(t, s)`. Then it is a candidate of the role `X` gives the parameter at
   `x(t, s)`:
   - on `S` and on an input, the role is the same and the existence has the same value
     (step 1);
   - a fold's key off `S` gets `Earlier { step: 0 }`, which admits every slot;
   - on a stub, an `Earlier` parameter admits every slot, and one whose role needs a present
     row is rebound to `Earlier { step: 0 }` when its action is a rise or a folded action
     (`bind`). Of an `S_a` action only a folded one has such a parameter (step 4);
   - `X` gives no role to a parameter it drops.
6. *Guards, `C ⇒ X`* (`xguard_of_guard`; `exact`, `folded_og`). Let `a`'s guard hold at `b` and
   `(t, s)`. The roles of `X` hold by step 5 and the kept conjuncts by step 3. For a fold, `b`
   itself is a binding with the key at `b(key)` that puts each `T` parameter whose role needs a
   present row on one (its role conjunct, step 2) and makes every folded guard true (they are
   conjuncts of `og`, `folded_og`). So by `exact` the bit at `b(key)` holds at `t`, and
   `x(t, s)` holds that bit.
7. *Effects on `S_v`* (`eff_x`, `eff_agree`; `eff_s`, `local_`, `reads`, `lemma5`,
   `eff_names`). An `S_a` action at `b` writes the same `S_v` values from `x(t, s)` as from
   `(t, s)`. Outside `Wr(a)` both keep their values, which agree by step 1. On `Wr(a)`, the
   value of a folded action depends only on `S_v ∪ inputs` (`eff_s`), and that of an unfolded
   one only on `Rd(a) ∪ Wr(a)` (`Local`), which lies in `S_v ∪ inputs` by (c) and (a); step 1
   gives the rest. Two bindings that agree outside the free `T` parameters give the same
   effect: for a fold by `eff_names`, and an unfolded action has no free parameters.
8. *Item 2* (`s_fwd`). An `S_a` step of `C°` by `a` at `b` has `a`'s guard at `(t, s)`, so `X`'s
   guard at `x(t, s)` and `b` (step 6). `X`'s successor holds the same `S_v` values (step 7)
   and the same input bytes, by the definition of a silent edge. `a` writes no `T_v`
   (Lemma 5 (a)), so the cone's successor is `(t, s')` and `X`'s is `x(t, s')`.
9. *Item 3* (`s_back`; `cover`, `exact`, `new_writes`, `folded_names`, `other_names`). Let
   `X`'s guard hold at `b'` and `x(t, s)`. A binding `b` at which `a`'s guard holds at `(t, s)`
   is built as follows.
   - *Unfolded `a`.* Take `b = b'`. A role that reads no row admits every slot. A parameter
     whose role reads its row is on `S` or an input (step 4), where `X` keeps its role and the
     existence has the same value (step 1). Every conjunct of `og` is kept or folded (`cover`),
     so kept, and holds at `(t, s)` by step 3.
   - *Folded `a`.* The bit at `b'(key)` holds at `t`, so by `exact` some binding `b''` with
     `b''(key) = b'(key)` puts each `T` parameter whose role needs a present row on one and
     makes every folded guard true at `(t, s)`. Take `b` equal to `b''` on the free `T`
     parameters and to `b'` elsewhere. Then `b` equals `b''` on every `T` parameter and on the
     key. An `S` parameter is not free, so `b` binds it as `b'` does, and `X` keeps its role
     (step 1). A `T` parameter is never `New`: by `new_writes` its existence would be in
     `Wr(a)`, so in `S_v` by (a). An `Earlier` one admits every slot, and the others are on
     present rows under `b''`. A folded guard holds at `b''` and depends only on the `T`
     parameters and the key (`folded_names`), where `b` and `b''` agree. Any other conjunct of
     `og` is kept (`cover`), holds at `b'` and `x(t, s)`, so at `b'` and `(t, s)` (step 3), and
     does not depend on the free parameters (`other_names`), so it holds at `b`.

   So `a`'s guard holds at `b` and `(t, s)` (step 2). The cone's successor has the `S_v` values
   `X` wrote, by both points of step 7, and keeps `t`, since `a` writes no `T_v` (Lemma 5 (a)).
   `b` equals `b'` outside the free `T` parameters, which is the relation `stands` of
   "Parameters".
10. *Item 4* (`rise_fwd`, `flips_end`; `rise_eff`, `bind`). Let a `T_a` action `a` step from
    `(t, s)` to `(t', s)` at `b`. The stages are the raw inputs of `t'` with the bits of `t`,
    when the raw inputs change, then one flip per bit that differs, in the order of `keys`. If
    the raw inputs change, some input `v` changes, so `v ∈ Wr(a)` (`Local`) and `a` is a rise.
    Its guard holds in `X` at `x(t, s)` and `b` (step 6, with `bind` for the stub
    parameters), and its value on each input is the one it has at `(t, s)` (`rise_eff` and
    step 1), which is `t'`'s. That edge leaves `S` alone and keeps the bits. Then each bit that
    differs is flipped by its free flip, whose guard is only that the bit has the other value,
    and a bit that agrees is skipped. Every key is in `keys`, so the flips end at the bits of
    `t'`.

Item 1 follows by induction on reachability (`Reduction.lean`, `x_reach`). An initial state maps
to an initial state by the model of `X`'s initial states. An `S_a` step keeps `t` (Lemma 5
(a)) and is a silent edge (item 2). A `T_a` step keeps `s` (Lemma 5 (b)) and is a sequence of
rises (item 4). `X` has the shape Definition 4 gives it: a silent edge keeps the input bytes
and a rise keeps `S` (`extended`). Items 1 to 4 are the hypotheses `Lemma6Fwd`, `Lemma6`,
`Lemma6By` and `Lemma6Staged` of the Lean proof of Theorem A8 (`lemma6_fwd`, `lemma6`,
`lemma6_by`, `lemma6_staged`). ∎

*The parts of the code the proof does not model.* Each has its own argument, except the nested
case.

- *Initial states.* `X` is `restrict_to(S_v ∪ inputs)` of `C` with the bit variables added.
  `restrict_to` keeps each kept variable's declaration with its start and base, and `C`'s
  initial states are a product over components, so their projection onto `S_v ∪ inputs` is
  `X`'s initial states without the bits (Lemma 2's proof). `split_with` keeps a fold only when
  `fold_initial` gives it one value at every initial state and every slot of its key.
  `Extended::build` sets a bit variable's base to true iff some fold of its group has the
  value true, which is `push_bits`' disjunction at every initial state. So each `x(t₀, s₀)` is
  initial in `X`.
- *Groups of folds sharing a bit.* A group's bit is the disjunction of its folds' `holds`, and
  its folds share their restricted parameters, guards, effects and key (`Signature::Param`,
  step (a), point 4). In step 6 the fold of the cone's action makes its own `holds` true, so
  the group's bit. In step 9 a true group bit makes some fold's `holds` true, and that fold's
  action gives the cone's step. `X` runs the group's first action, which has the same guards
  and effects, and `SilentStep::acts` reads back the variant it stands for.
- *Node folds* (`FoldKey::Linked`). Step (a)'s last paragraph and the node rows of the folded
  table: `X` reads `link = q ∧ bit[q]`, which holds for the one `q` the link names when that
  row is present and the body holds there, which is the cone's `∃ e. link = e ∧ body(e)`.
- *Parameter positions.* A binding of `X` keeps the cone's positions in the model. The code
  renumbers a class move's parameters with slot 0 for a dropped one, and step (b) shows that
  the successor does not depend on it.
- *A fold whose key the restriction drops.* The model reads the bit at the key's position
  whatever its side. In the code `split_with` leaves such an action unfolded, and a fold that
  keeps a dropped key is `Fault::KeyDropped`, on which the cone is decided unreduced
  (Definition 4).
- *The nested case.* For a nested reduction `x(t, s)` is in `X_n` only up to the inner
  reduction's canonical form. That needs Theorem A8 of the inner cone at each level, and the
  induction over the depth is named here, not written.
- *A run up to `H`.* When `S` is run up to `H` (A22), `X_n` holds `X`'s states up to `H`'s
  canonical form, and the certificates read the orbit graph, not `X`'s own run. That step is
  A22's, which is a sketch.

*Why the code meets the hypotheses.* The rest of this section reads the code for the fields of
`Lemma6Hyp` marked "read" above: the tables of "Parameters" for the sides and roles, step (a)
for `exact` and for what `fold_of` lets a fold name, step (b) for the parameter positions, and
step (c) for `rise_eff`.

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

*Evidence.* P-full for a flat reduction, relative to the fields of `Lemma6Hyp` (the proof and the
parts it does not model, above); P-sketch for a nested reduction, whose induction over the depth
is not written, and for one run up to `H` (the last part not modelled, above). M: the Lean proof below. R: the fields the table marks R. D: the `S_a` half is checked against the cone's own step
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
[tlc-agreement.md](../theory/tlc-agreement.md#the-reduction-step-by-step)), with its own semantics of
the unreduced cone: each state's key is a state of `X_n`, and each `S_a` step and rise of the
cone is a class move or a rise of the printed reduced module, or a stutter. With the stub
parameters left unbound and the stub check off, it fails on both stub cases at the step `X`
drops (`Issue`, and `Note`'s rise).

M: `proofs/lean/RdraIsh/Lemma6.lean` proves items 1 to 4 from `Lemma6Hyp` over the model of
`Restrict.lean`, step by step as above, and `Lemma6Example.lean` discharges `Lemma6Hyp` on a
model with a folded action whose free `T` parameter is on a stub, an unfolded action with a kept
conjunct, and a rise with a parameter on a stub. With `bind_stub_params` off, the `C ⇒ X` half
fails there (`stub_bug`). The Lean proof reads the model, not `Extended::build`, and the tests
above are evidence on their models and bounds. cone-soundness.md, "Proof obligation", states the
lemma in points 2, 3, and 5.

**Theorem A8 (the reduced cone is stuttering equivalent to the cone).** Assume Lemma 5 (a)(b),
Lemma 6, the shape of `X` (a silent edge keeps the input bytes, a rise keeps `S`), and (B1) to
(B3) for `B` on the reachable states of `X`, with the representative of each block a reachable
state of it. For `R`'s `T` move by `recanon` (point 5) assume also the rise table: every rise of
a representative is entered under its block and the input bytes it ends at. Let `(t, s) ~ (t, ρ)` when `ρ` is the representative of the block of `x(t, s)`.
Then `~` is a divergence-blind stuttering bisimulation between `C` and `R` that preserves `T_v`,
`obs`, and the action label of every `T_a` step. Consequently `C` and `R` agree on:

1. which valuations of `T_v ∪ obs` are reachable (invariants, `always`, witnesses, vacuity). The
   inclusion of `C`'s in `R`'s, which every `ok` of an invariant reads, needs every `S_a` step of
   `C` to be a silent edge of `X`, including the steps whose parameters lie on a stub (Lemma 6,
   "Parameters");
2. which `T_a` action labels an edge from a state with given `T_v ∪ obs` values (`NeverRuns`, arms
   taken, `after` assertions on `T_a` actions);
3. whether from every reached state some path takes an edge that changes given `T_v` values
   (deadlocks, as [semantics.md, D25](../theory/semantics.md#d25) defines them: a silent cycle counts
   as stuck, which this divergence-blind bisimulation keeps);
4. every property `P` whose reads are in `T_v ∪ obs` and whose fair actions are in `T_a`, under
   `Plain`.

*Proof.* Five points, numbered as in cone-soundness.md, "Proof obligation", with Lemma 6 made
explicit. Each names the Lean theorem it corresponds to (in `Reduction.lean` unless another
file is named). First, `x(t, s)` is a reachable state of `X` for every reachable `(t, s)` (item 1
of Lemma 6, `x_reach`), so (B1)–(B3) apply to it, and the representative `ρ` of its block is
reachable, in that block, and agrees with `x(t, s)` on `obs` and the input bytes by (B1)
(`rep_facts`).

1. Related states agree on `T_v` (same `t`) and on `obs` (B1) (`rel_preserves_obs`).
2. A `T_a` step that leaves the *input bytes* unchanged. The input bytes are
   `Reduction::input_bytes`: the raw inputs followed by the bits `Reduction::push_bits`
   computes. A step that changes only a bit, with the raw inputs unchanged, is treated under
   point 5, where `Reduction::stages` gives it a stage of its own. The step reads `S_v` only in
   `obs`, keeps `S_v` and writes `T_v` only (Lemma 5 (b)), and `ρ` agrees with `s` on `obs`, so
   it is enabled at `(t, s)` iff at `(t, ρ)`, with the same successors on `T_v`, and the block
   of `x` is unchanged (`sim_forward` and `sim_backward`, the `T_a` case with no stage).
3. An `S_a` step `(t, s) → (t, s')`: by Lemma 6 (item 2) it is a silent edge of `X`. If
   `x(t, s')` is in the same block it is a stutter of `R`. Otherwise it enters a block `B'`;
   by (B2), applied to `x(t, s)` and `ρ`, a silent path from `ρ` through its own block enters
   `B'`, and its last edge is a class move of `R` into `B'` (`sim_forward`). Conversely a class
   move from `(t, ρ)` into `B'` comes from such a path from `ρ`; by (B2) a path from `x(t, s)`
   through its block enters `B'`. Each of its edges keeps the input bytes (the shape of `X`),
   so it starts at some `x(t, s₁)`, and by Lemma 6 (item 3) it is an `S_a` step of `C` from
   `(t, s₁)`, whose states before the last are related to `(t, ρ)` (`lift_silent`,
   `sim_backward`).
4. Every initial `(t₀, s₀)` is related to `(t₀, rep(block(x(t₀, s₀))))`, which is how
   `Classes::init` builds `R`'s initial states; `R` has no other (`init_forward`,
   `init_backward`).
5. A `T_a` step `(t, s) → (t', s)` that changes the input bytes (a raw input, a bit, or both):
   by Lemma 6 (item 4) it is a sequence of rises from `x(t, s)` to `x(t', s)`, one per stage of
   `stages`. By (B3), applied stage by stage (`b3_star`), `ρ` has a sequence of rises whose
   states lie in the same blocks. A rise keeps `S` and ends at its stage's input bytes, and by
   (B1) a block fixes the input bytes, so the end of `ρ`'s sequence is in the block of
   `x(t', s)`. `recanon` folds the rise table over the stages: from a block it takes the block
   the representative's rise to the next stage's input bytes enters, which the rise table
   holds, and the representative of that block rises alike by (B3). So `recanon` gives the
   block of `x(t', s)` (`Stages.lean`, `recanon_of_through`, `staged_step_iff`). The step is
   enabled at `(t, ρ)` as in point 2. Conversely a `T_a` move of `R` from `(t, ρ)` is a step
   of `C` from `(t, s)` by Lemma 5 (b), and by the same argument it ends at the block of
   `x(t', s)` (`sim_backward`). A fold is covered by the same argument with its bit in place of
   the variables it folds: the bit is among the input bytes, and Lemma 6 relates the folded
   action's steps to it. The monotone rule is not used here: it bounds the input histories,
   which keeps the extended system finite and small.

Points 1 to 5 make `~` a divergence-blind stuttering bisimulation that preserves `T_v` and `obs`
(`a8_dbsb`), and branching, with each `T_a` step showing its action and binding on both sides,
since points 2 and 5 take the same action at the same binding (`ReductionLabels.lean`,
`a8_branching`; with `recanon`, `Stages.lean`, `a8_staged`).

Items 1 to 3 of the consequences are existential statements over finite paths labelled by
`T_v ∪ obs` and `T_a` actions, which a stuttering bisimulation preserves (Browne, Clarke and
Grumberg 1988; for the divergence-blind variant, De Nicola and Vaandrager 1995). In Lean they
are `a8_holds_iff` (1) and, in `ReductionLabels.lean`, `a8_reachable_obs_iff` (1),
`a8_edges_iff` (2) and `a8_stuck_iff` (3), whose change relation is irreflexive, as D25's is. For item 4, a behavior of `C` that steps
inside one block forever maps to a behavior of `R` that stops there. Under `Plain` a stop is
allowed iff no fair instance is enabled; fair instances are `T_a` actions (Lemma 5e), their
enabledness reads `T_v ∪ obs`, which is constant on the block, so if one were enabled the `C`
behavior, satisfying `Fair_P`, would take it and leave the block's image. The converse lifts a
stop of `R` to a stop of `C` at the same `T_v ∪ obs`, and a cycle of `R` to an infinite `C`
behavior by point 3, segment by segment (`Plain.lean`: `a8_plain_sound`, that the property on `R`
gives it on `C`, from the `C ⇒ X` half of Lemma 6; `a8_plain_complete`, the converse, with
`seg_exists` for the segments; `a8_plain_iff`, for `□(trig ⇒ ◇goal)` under `Fair_P` over infinite behaviours). ∎

*The nested case.* When `X` is itself reduced, the partition is that of the inner run read
through each level, and `Nested.lean::a8_nested` proves the theorem for it by induction on the
depth, from each level's A8 and an inner map between the levels, with the outer
representative a reachable state of `X`. The code takes the outer representative among the
inner run's states, whose `S` bytes are an inner representative's, and reads the outer class
moves and rises on the inner run. The steps that tie these to `X` (the inner split's
certificate below, the inner partition's (B1), and Theorem A8 of the inner cone for the
representative's inner `T` part) are named, and no argument assembles them. So the theorem
is proved step by step for a flat reduction and sketched for a nested one or one run up to `H`.

What the proof does not cover is `WF_vars(Next)`, which a stop inside a diverging block violates;
the next section treats it.

*Hypotheses and code.* (B1)–(B3): `reduce/partition.rs::quotient_blocks`, certified by
`reduce/partition/reference.rs::stable` before use (below);
Lemma 5: `split_at`, `split_with`, certified by `reduce/lemma5.rs::split_faults` before use; Lemma 6: `Extended::build`, `reduce/fold.rs`; the checks a
reduced cone keeps: `drop_unreadable_checks` (A14).

*Evidence.* P-full for a flat reduction (no inner reduction of `X`, and `S` not run up to
`H`), relative to the hypotheses above: Lemma 5 (a)(b),
checked at run time on each split (R) and proved above; Lemma 6, proved above from the fields of
`Lemma6Hyp`, of which the table there says which are checked and which are read from the code;
the shape of `X`, which Lemma 6's proof derives; (B1)–(B3) with a reachable representative,
certified on each partition the reduction uses (R, and below); and the rise table, certified on
each run of `X` (R, below). P-sketch for a nested reduction (the nested case above) and for one whose `S` is run up to
`H`, where `X_n` holds `X`'s states up to `H` and the certificates read the orbit graph
(A22, a sketch). For Lemma 5:
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
by the sweeps' partition. So (B1)–(B3) hold of every partition a reduction uses.

**Proposition (the certificate gives (B1)–(B3)).** Let the run be complete: its states are the
reachable states of a flat `X`, numbered, its silent edges and rises are exactly `X`'s between
them, and two states with the same initial label agree on `obs` and the input bytes
(`proofs/lean/RdraIsh/Sweep.lean::Complete`). If a partition passes `stable`, then it meets
(B1)–(B3), and the first state of each block is a reachable state of it
(`Sweep.lean::stable_of_cert`).

*Proof.* (B1): two states of a block have the same initial label, so they agree on `obs` and the
input bytes. (B2): a sweep marks, for a splitter `B'`, the states outside `B'` with a silent
edge into it and, backwards, the states with a silent edge to a marked state of their own block
(`Sweep.lean::Marked`). By induction on the marking, a state is marked iff a silent path through
its block followed by one silent edge leaves it into `B'` (`Sweep.lean::marked_iff`; the path
lies in `X` because the run is complete). A sweep that splits nothing marks all or none of each
block, which is (B2). (B3): every state of a block enters by a rise the blocks its block's
first state enters, so any two states of a block enter the same blocks, and a rise of the run
is one of `X`. The first state of a block is a state of the run, so reachable, and in its
block. ∎

A `Stable` partition passes the check when the labels are exactly the values of `obs` and the
input bytes (`Sweep.lean::cert_of_stable`).
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
- **A10** (monotone inputs): an `S_a` action reads `T_v` only in the inputs and its fold
  (Lemma 5 (c)), and every input is monotone (Lemma 5 (d)); both are proved above and checked
  at run time on every split (`split_faults`, R). Theorem A8's proof reads the inputs only as
  part of the input bytes, through Lemma 6 and point 5, and does not use their monotonicity,
  so A8 holds with inputs, under its hypotheses, flat and nested as there. Monotonicity keeps
  `X` finite and small, and A15 uses it (case 4 and `s_cycles`). That a variable
  `monotone_vars` marks monotone changes finitely often is proved over a model of its write
  graph and of `each_successor` (`proofs/lean/RdraIsh/Monotone.lean`), in two steps:
  1. A step from a pre-state that meets `Pre` (a created row absent with its cells at base, a
     deleted row present, values in their domains, the clock below `int_max`) moves each cell
     of a variable that is not cyclic along an edge of that graph, or leaves it
     (`Monotone.lean::succ_covers`): each effect contributes its own edges, as the doc of
     `monotone_vars` lists them (a create from absent to present, a delete from present to
     deleted and its columns to base, a write from base in the creating action and from any
     value elsewhere, a transition's arms, the clock up).
  2. A graph with no cycle but self-loops orders the values, and each change moves a cell
     forward in that order, so a cell changes at most `|dom(v)| - 1` times
     (`Monotone.lean::monotone_finite_changes`).

  That the guards give `Pre` at each step is read from the code, and the write-graph model is
  not joined to the action model of Lemma 5, so `ex_mono` stays a hypothesis there; an
  existence that only creates and deletes write is monotone in the write-graph model
  (`Monotone.lean::existence_monotone`). The three properties the claim names of
  `monotone_vars` (a column cyclic when a delete resets it, owners cyclic though a delete keeps
  them, the starting values listed through the checked `narrow::id8`) are read from it.
- **A11** (folds): a folded guard is a function of its bit, the fact `exact` of Lemma 6, which
  step (a) derives from a reading of `fold_of` and `Fold::holds`, and which the `Twin::FoldHolds`
  twin checks against its reference. Lemma 6's proof uses it in steps 6 and 9, for a fold with
  a bit of its own; groups of folds sharing a bit and node folds are argued in the parts it does
  not model. So A8 holds with folds, under its hypotheses, flat and nested as there. A count that reads `T` is never
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
  [claims inventory](../theory/claims.md#panics-left-on-a-lifts-way).
- **A17** (the cache): the cached quotient is keyed by the extended system, the caps, and the group;
  `Twin::CacheHit` recomputes every hit in a `shadow` build. M.

---

### `WF_vars(Next)` on a reduced cone

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

**Theorem A15.** Assume Theorem A8's hypotheses, that `X` and `R` have finitely many reachable
states, and that every block whose silent edges between two different states contain a cycle
from a reachable state can loop (case 1 of Definition 5). If the run finds no counterexample
and `R`'s graph is complete, then `C ⊨ P` under `WfNext`. The steps below name their Lean
theorems in `Fairness.lean` (`a15`, `a15_image`).

*Proof.* Suppose `σ` is a behavior of `C` that satisfies `Fair_P ∧ WF_vars(Next)`, has a state where
`trig_b ∧ cause_b` holds, and never meets `goal_b` from there. Map each state to its `~`-image. By
points 2, 3, 5 of Theorem A8 each step of `σ` is a step of `R` or leaves the image unchanged, so the image
is a path of `R`'s graph from an initial state, and it agrees with `σ` on `T_v ∪ obs`, hence on the
trigger, the cause, the goal, and the enabledness of every fair instance (fair actions are `T_a`
actions, Lemma 5e). A `T_a` step is the same step on both sides, so a fair instance is taken in `σ` iff
its image is taken (`image_step`, `t_step_img`, `takes_fwd`, `enabled_back`).

- *The image changes infinitely often.* From some point it stays in one strongly connected component
  `K` of the goal-free subgraph and uses only its edges. A fair instance is taken infinitely often in
  `σ`, hence labels an edge of `K`, or is disabled infinitely often, hence disabled at some state of
  `K`. `K` has an edge, so it admits a fair cycle, which the search finds (the lasso of a fair
  behaviour on a finite system, `Behaviour.lean::lasso_of_behaviour`; the search's verdict,
  `Search.lean::a15_search`).
- *The image changes finitely often, `σ` infinitely often.* From some point `T_v` is fixed at `t`, the
  block is fixed at `c`, and every state-changing step of `σ` is an `S_a` step inside `c`. `obs` is
  constant on `c` (B1), so each fair instance's enabledness is constant from that point; one that is
  enabled would be taken (`Fair_P`) and change `T_v`, so all are disabled. It remains to show that
  `c` can loop. (i) If infinitely many steps change the state of `X_n` (by Lemma 6, via the inner
  canonical form), they form an infinite walk inside `c` in a finite graph whose edges join different
  states, so they contain a cycle: case 1 (`silentCycle_of_divergence`; or case 3 when the run
  of `S` was up to `H` and the walk closes on an orbit, which the Lean proof does not model). (ii) Otherwise, from some point every step changes only the inner sub-lifecycle,
  and the same argument one level down gives case 2. (iii) If `obs = ∅`, `X` is not explored; an
  infinite path of silent steps changes some non-input variable infinitely often. Each change of a
  variable `v` is an edge of `v`'s write graph (every effect contributes its edges to
  `monotone_vars`), and a variable whose graph is acyclic changes at most `|dom(v)| - 1` times per slot,
  so `v` is not monotone: case 4. The same fact justifies leaving every class unmarked when
  `s_cycles` is false. So `(t, ρ)` may stop.
- *`σ` stays at `(t, s)` forever.* `WF_vars(Next)` says no step leaves `(t, s)` in `C`. A `T_a` move of
  `R` from `(t, ρ)` reads only `T_v ∪ obs` and would leave `(t, s)` too (point 2 or 5). A class move into
  `c' ≠ c` would, by (B2) and Lemma 6, be matched by a silent path from `x(t, s)` into `c'`, whose
  first state-changing edge leaves `(t, s)`. So no move leaves `(t, ρ)`, and it may stop
  (`moves_back`).

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

*Evidence.* P-full for a flat reduction, relative to Theorem A8's hypotheses (each with how
the code meets it, under Theorem A8 and Lemma 6), finitely many reachable states of `X` and `R`,
and case 1 of Definition 5 for every block with a silent cycle (R, below). P-sketch for a
nested reduction or one run up to `H`: (i) reads Lemma 6 through the inner canonical form, and (ii), with cases 2
and 3 of Definition 5, applies "the same argument one level down", which is not written.
P-sketch also for case 4 (`obs = ∅`): the proof's (iii) is the step-by-step argument of A10
for `monotone_vars`, but the code's `R` there has one class for all input bytes, and its
reading as the reduced cone of the partition by input bytes is not written (below). M: `proofs/lean/RdraIsh/Fairness.lean::a15` proves the theorem in Lean for the
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
is not written.
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

### Instance symmetry

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

**The hypotheses as the proof reads them.** The proof reads (G1)–(G4) and (G6) as conditions on
the syntax of guards and effects, in the form `explore/symmetry/breaks.rs::scan` checks them, and
the Lean model `proofs/lean/RdraIsh/SymActions.lean` states them in that form (`Hyp`):

- (G1) as above, and a term naming rows of a class that moves appears in no `In` (rule `OrderedId`).
- (G2) as above, and a data column is not written from the slot of a parameter whose class moves
  (rule `MixedIds`).
- (G3): an identity start couples its child's class with its parent's, and the owner of a
  pre-existing row that starts at its base names no row. That `I` is closed under every `G_s` is
  then step 2 of the proof.
- (G4): a creation writes every owner of the created row whose base names a row.
- (G6): no guard reads, in the state before the step, an owner on a `New` parameter whose
  parent's class moves. The scan leaves the parent's class out when a guard makes such a read.
  The Lean model's `G6` is stronger: it forbids every such read, whatever the parent's class.

(G1)–(G4) constrain only terms and effects naming rows of a class that moves, so they hold of a
class the scan leaves out. The Lean model's `G6` does not mention the group. A guard that reads
the pre-state owner of a `New` row whose parent's class does not move is outside the Lean model,
and step 4 covers it on paper. Two more hypotheses enter. `WF`: a term reads, and an effect
writes, a column or an owner on a row of its own entity, and every parameter it names exists.
That the builder's typing gives `WF` is read from the code. `Built`: the two facts of the
builder above. (G5) is the
semantics of a delete: it resets the row's columns, not its run marks, and keeps its owners.

Outside `W` the commutation can fail. Take an absent row whose optional owner names a row, and a
step that creates the row without writing that owner, which (G4) allows for an owner whose base is
`null`. `apply` keeps the value on the absent row, so the step from `g(s)` leaves it, while `g(t)`
relabels it (`proofs/lean/RdraIsh/SymActionsExample.lean::off_W_counterexample`).

*Proof.* Each step names the theorem of `proofs/lean/RdraIsh/SymActions.lean` that proves it over
the slot model, where (G1)–(G6), `WF` and `Built` are the hypotheses above; steps 6 and 7 use
`proofs/lean/RdraIsh/Symmetry.lean`.

1. *The group* (`SymActions.lean::act_mul`, `SymActions.lean::mem_act`,
   `SymActions.lean::mem_step`). `apply` composes: `(gh)(s) = g(h(s))`. A `g ∈ G_s` fixes the absent
   slots of `s` and maps present and deleted slots to slots of the same existence, so `g(s)` has
   the absent set of `s` and `G_{g(s)} = G_s`. Absent slots of `t` are absent in `s`, since a slot
   leaves `absent` only, so `g` fixes them and `g ∈ G_t`. So `≈` is reflexive (the identity),
   symmetric (`g⁻¹ ∈ G_s = G_{g(s)}`), and transitive (for `g ∈ G_s` and `h ∈ G_{g(s)} = G_s`,
   `hg ∈ G_s`).
2. *The set `W`* (`W_init`, `SymActions.lean::W_step`,
   `SymActions.lean::W_act`, `SymActions.lean::init_act`). An initial state is in `W`: a created
   entity starts absent at its base values. A step keeps `W`: it writes only rows its
   parameters bind, and by `Built` (step 3) each is present or deleted after the step. `apply` keeps an
   absent row's values, since it fixes the slot and relabels an owner only on a row that is not
   absent, so `W` is closed under every `G_s`. By (G3), `I` is closed under every `G_s`: D4 gives
   every `Start::Any` variable every value of its domain, an identity start moves the child's slot
   with the parent's, and a pre-existing row's owner at its base names no row, which relabelling
   leaves as it is.
3. *Bindings* (part of `SymActions.lean::step_comm`). `New` takes the lowest absent slot, which `g`
   fixes; `Existing`, `Parent`, and `Triggered` admit a present slot, `Earlier` a slot by its run
   mark, and `g` preserves presence and moves marks with their slots. In `W` an absent row holds the
   base mark, which no run-mark conjunct accepts (`Built`). So every bound row other than a `New`
   one is present or deleted in `s`, and a `New` row is created (`Built`), so every bound row is
   present or deleted in `t`. So `g(p)` binds `a` at `g(s)` exactly when `p` binds it at `s`.
4. *Predicates* (`SymActions.lean::eval_mv`). A column or mark of slot `i` in `s` is the same value
   at slot `g(i)` in `g(s)`. By (G1) a row id is compared only by `=`/`≠` with a row id of its
   class, which a bijection preserves, or an optional owner with `null`, which relabelling
   preserves, and appears in no `In`. An owner read on a non-absent row is relabelled consistently
   with the row ids it is compared with (G5 for deleted rows). The other bound rows are not
   absent (step 3), and `Forall`, `Exists`, and `Count` range over the present slots, a set `g`
   permutes. The one read of an absent row's owner is the pre-state owner of a `New` row, which
   (G6) allows only when the parent's class does not move. That row is absent, so in `s ∈ W` it
   holds the owner's base, and `g` fixes its slot and keeps its values, so `g(s)` reads the same
   base. As a row id of a class that does not move, it may be compared with any term that
   names no row of a class that moves (G1), and the rest of this step gives such a term the same
   value at `(g(s), g(t), g(p))` as at `(s, t, p)`. So the comparison is unchanged. This case is paper only: the Lean model's `G6`
   excludes it. So every guard of `a` has the same value at `(s, t, p)` and
   `(g(s), g(t), g(p))`.
5. *Effects* (`SymActions.lean::post_comm`). For `s ∈ W`, the successor of `g(s)` under `g(p)` is
   `g` of the successor of `s` under `p`, location by location: a literal write is unchanged; a
   data column is written only from a literal or from the slot of a parameter whose class does not
   move (G2), which `g` fixes; an owner written from a parameter's slot is relabelled with that slot
   (G2); a create or delete moves with its slot, and a delete keeps the owners (G5); a `Choose` on
   an owner ranges over all rows (G2), so its set of forks is closed under `g`, the pick moving
   with `g`. An owner the step does not write on a row it creates holds its base value in `s ∈ W`,
   which (G4) makes `null` (unchanged by relabelling) or rules out.
6. *The step and the bisimulation* (`SymActions.lean::step_comm`, `SymActions.lean::symOn`,
   `Symmetry.lean::a18_dbsb`). Steps 3 to 5 give `g(s) →_{a,g(p)} g(t)`, and step 1 gives
   `g ∈ G_t`. The converse is the same statement for `g⁻¹ ∈ G_{g(s)}` at `g(s) ∈ W` (step 2). So
   `≈` maps each step from a state of `W` to one step and back. `symOn` packages steps 1, 2 and
   6 as a `SymOn` on `W`, and `a18_dbsb` proves that `≈` is then a bisimulation of the system
   restricted to `W`, which has `T`'s steps from every state of `W`.
7. *The quotient* (`Symmetry.lean::quotient_covers`, `Symmetry.lean::quotient_sound`,
   `SymActions.lean::SymOn.quotient_reach_iff`). Let the run store `can(t) = π(t)(t)`,
   `π(t) ∈ G_t`, for each initial state and each successor `t` of a stored state. By induction
   on reachability, every reachable `s` has an image `g(s)`, `g ∈ G_s`, that the run stores: for
   an initial `s`, `can(s)`; for a step `s → t` with `g(s)` stored, `g(s) → g(t)` is a step
   (step 6), the run stores `can(g(t)) = π(g(t))(g(t))`, and `π(g(t)) g ∈ G_t` (step 1). By the
   same induction on the run, every stored state is such an image of a reachable state: an
   initial one is `can(c)` of an initial `c`, and a successor is one by step 6 read backwards.
   (G3) makes each stored initial state an initial state itself, which the replay of a lifted
   counterexample needs (`PinnedStart`). Both hold on the system restricted
   to `W`, whose reachable states are `T`'s and whose stored states lie in `W` (step 2), and
   `SymOn.quotient_reach_iff` transfers them to `T`. *Depth.* By induction on `d`, the classes
   of the states at depth `≤ d` are the classes the search stores at depth `≤ d`, since `≈` maps
   each step to one step in both directions; so each class is first stored at its least depth.
   This step has no Lean theorem. ∎

**Leaving a class out (A20).** Let `G'_s ⊆ G_s` be the group when the scan leaves out more
classes: the permutations of the slots not absent in `s` of the classes it keeps. `G'_s` is
closed under composition and inverse. `G'_{g(s)} = G'_s` for `g ∈ G'_s`, since `g` fixes the
same slots. A step keeps `g ∈ G'_t`, since the slots of the classes left out stay fixed and
step 1 of the proof keeps the rest. These are the hypotheses of
`proofs/lean/RdraIsh/Symmetry.lean::Sym.sub`. It gives a symmetry with the same commutation,
since every `g ∈ G'_s` is in `G_s`. Over the slot model, the hypotheses of A18 reach the same
end another way. Each of (G1)–(G4) in `Hyp` holds of a class that does not move, and `G6` does
not mention the group. So marking more classes as not moving keeps `Hyp`, and
`SymActions.lean::symOn` holds of the system with the smaller group. That `Hyp` keeps under marking a class as not moving is read off its definition,
case by case. No Lean theorem states it. So the scan can always leave a class out and lose only
reduction (**A20**, P-full). A *witness*
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
patterns, and `NeverRuns`. For invariants, `proofs/lean/RdraIsh/Symmetry.lean::quotient_holds_iff`
proves it for any `can(s) = π(s)(s)`, `π(s) ∈ G_s`, from A18's commutation, and
`proofs/lean/RdraIsh/SymActions.lean::a18_holds_iff` from A18's hypotheses. The checks that read
the annotated quotient (deadlocks, liveness) follow a binding or a slot through the stored
permutations. For liveness, Theorem A23 takes (Canon), any member of the orbit as a function of
the state, which `Canon::canonical` still is past the cap: it keeps the first branch. So A23
gives liveness past the cap, P-full relative to A23's hypotheses. Deadlocks are entity-cone
findings, which A23 covers only by a sketch, so their soundness past the cap is P-sketch.
M: `Twin::Canonical` compares the canonical form with the least layout over every element of
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
permutations `σ_n ∈ G_{t_n}` with `r_{n+1} = σ_n(t_n)`, whose first state is an image `g(c)`,
`g ∈ G_c`, of an initial state `c` (`proofs/lean/RdraIsh/SymmetryLiveness.lean::ARun`). The run
the code stores has `r_0 = can(c)`, `r_{n+1} = can(t_n)` and `σ_n = π(t_n)` (`RunOf`). The binding
moves along it by the `σ_n`, and so does each fair instance; the instance's sequence is its
*thread*. A thread's instance is taken by the step `r_n → t_n` (D20 with τ: `t_n ≠ r_n` and the
step has a label of the instance), so an orbit self-edge (`t_n ≠ r_n`, `σ_n(t_n) = r_n`) is a
move. The run is *thread-fair* when every thread enabled at every `r_n` from some point on is
taken infinitely often (`AFair`). It *stops only where the rule allows* when, from any point
after which `t_n = r_n` forever, the stop rule holds at `r_n` (`AStops`). It *violates* the
property for a binding `b` when the trigger of the moved binding holds at some `r_i` and the goal
of the moved binding fails at every `r_j`, `j ≥ i` (`AViolates`). An *annotated lasso* repeats
`(r_n, t_n, σ_n)` with a period `q > 0` from some `k` on (`ALasso`).

Hypotheses, each named by the Lean structure that states it:

- (Sym) Theorem A18's commutation on a set `W`. Every step from a state of `W` commutes with every
  `g ∈ G_s` and keeps `g ∈ G_t`. Also, `G_{g(s)} = G_s` for `g ∈ G_s`, each `G_s` is closed under
  composition and inverse, the initial states are closed under every `G_s` (G3), and `W` holds
  the initial states and is closed under steps and under every `G_s` (`SymOn`).
  `proofs/lean/RdraIsh/SymActions.lean::symOn` derives it from A18's hypotheses, with `W` the
  states whose absent rows hold base values.
- (SymLive) `g` acts on fair instances and on bindings, with the composition and inverse laws,
  and maps a label of a fair instance to a label of its image (`SymLive`).
- (LiveEqui) For `g ∈ G_s`, with the binding moved by `g`, three parts hold (`LiveEqui`). The
  trigger reads the same on `s` and `g(s)`. The goal reads the same on the states of a set
  `after(b)` that holds wherever the trigger of `b` holds and that every step keeps. The stop
  rule reads the same on `s` and `g(s)`.
- (Canon) `can(s) = π(s)(s)` with `π(s) ∈ G_s`: any member of the orbit, as a function of `s`
  (`SearchSym.lean::Canon`).
- (Finite) The reachable states lie in a finite list, and every `G_s` is finite
  (`FiniteGroups`).

Then a property holds for every binding under `Fair_P` and the stop rule iff no annotated lasso
of the canonical quotient violates it for any binding while it is thread-fair and stops only
where the rule allows (`SymmetryLiveness.lean::a23_on`; `SymmetryLiveness.lean::a23` with (Sym) on
every state). Without (Finite) the same holds with annotated runs in place of lassos
(`SymmetryLiveness.lean::a23_iff`). The stop rule of `Plain` is `True`, and that of `WfNext`,
`¬Moves`, is one the group preserves (`stop_moves_equi`).

*Proof.* The theorems named are those of `proofs/lean/RdraIsh/SymmetryLiveness.lean` unless
stated otherwise.

1. *Restriction to `W`* (`restrict_reach_iff`, `restrict_behaviour_iff`, `restrict_enabled_iff`,
   `restrict_takes_iff`, `restrict_moves_iff`, `restrict_liveHolds_iff`, `SymOn.toSym`). Let
   `T|W` have `T`'s initial states and `T`'s steps from the states of `W`. `W` holds the initial
   states and is closed under steps, so `T|W` has `T`'s reachable states and behaviours. At a
   state of `W` it has the same enabled instances, taken instances and moves. So each liveness
   verdict of `T` is that of `T|W`, under a stop rule that agrees with `T`'s on `W`. On `T|W` the
   commutation holds at every state, since a step of `T|W` starts in `W`. Steps 2 to 7 take it on
   every state, on `T|W`.
2. *The group preserves fairness* (`takes_act`, `enabled_act`, `moves_act`, `stop_moves_equi`).
   Let `g ∈ G_s`. Then `s → s'` takes `f` iff `g(s) → g(s')` takes `g(f)`. Forward: `s' ≠ s`
   gives `g(s') ≠ g(s)`, since `apply` is injective. A label `l` of `f` with `s →_l s'` gives
   `g(s) →_{g(l)} g(s')` by (Sym), and `g(l)` is a label of `g(f)` by (SymLive). Backward: the
   same for `g⁻¹ ∈ G_{g(s)}`. An instance is enabled where a step takes it, so `f` is enabled at
   `s` iff `g(f)` is enabled at `g(s)`. For the same reason `Moves(s)` iff `Moves(g(s))`, so
   `¬Moves` meets the stop part of (LiveEqui).
3. *Frames* (`Framed`, `orbitSeq_frame`, `Framed.fair_iff`, `Framed.stutter_iff`,
   `Framed.stops_iff`, `Framed.violates_iff`). Call `(Θ_n)` frames of an annotated run `(r, t, σ)`
   over a sequence `c` when `Θ_n ∈ G_{c_n}`, `r_n = Θ_n(c_n)`, `t_n = Θ_n(c_{n+1})` and
   `Θ_{n+1} = σ_n Θ_n`. Then the thread of `Θ_0(f)` is at `Θ_n(f)` at step `n`, and the moved
   binding of `Θ_0(b)` is `Θ_n(b)`. Through step 2 at each `n`, three facts follow:
   - The run is thread-fair iff `c` satisfies `Fair_P`. Each thread is the image under `Θ_0` of
     an instance of `c`, and each instance of `c` has its image, `Θ_0` being a bijection.
   - `t_n = r_n` iff `c_{n+1} = c_n`. So, as the group preserves the stop rule, the run stops
     only where the rule allows iff `c` does.
   - The run violates the property for `Θ_0(b)` iff `c` violates it for `b`. The trigger reads
     the same by (LiveEqui). Every state of `c` at or after one where the trigger holds is in
     `after(b)`, by (LiveEqui) along the steps of `c` (a stutter keeps the state). There the goal
     reads the same.
4. *Lift* (`liftSeq`, `ARun.lift`). An annotated run with `r_0 = Θ_0(c_0)`, `c_0` initial and
   `Θ_0 ∈ G_{c_0}`, has frames over a behaviour: `c_{n+1} = Θ_n⁻¹(t_n)` and
   `Θ_{n+1} = σ_n Θ_n`, as `explore/orbit.rs::lift_path` accumulates its frame. By induction on
   `n`, `Θ_n ∈ G_{c_n}` and `r_n = Θ_n(c_n)`. Then `Θ_n⁻¹ ∈ G_{r_n}`, and (Sym) maps the step or
   stutter `r_n → t_n` by `Θ_n⁻¹` to `c_n → c_{n+1}`. A step keeps `Θ_n` in the group, so
   `Θ_n ∈ G_{c_{n+1}}`, and `σ_n ∈ G_{t_n} = G_{c_{n+1}}`, so `Θ_{n+1} ∈ G_{c_{n+1}}`. Last,
   `r_{n+1} = σ_n(t_n) = Θ_{n+1}(c_{n+1})`.
5. *Projection* (`projFrame`, `project`). A behaviour `c` has frames of a run the code stores:
   `Θ_0 = π(c_0)`, `Θ_{n+1} = π(Θ_n(c_{n+1})) Θ_n`, `r_n = Θ_n(c_n)`, `t_n = Θ_n(c_{n+1})` and
   `σ_n = π(t_n)`. `Θ_n ∈ G_{c_n}` by the induction of step 4, with `π(t_n) ∈ G_{t_n}`. `t_n` is
   a step or a stutter of `r_n` by (Sym). By (Canon), `r_{n+1} = π(t_n)(t_n) = can(t_n)` and
   `r_0 = can(c_0)`.
6. *On runs* (`a23_lift`, `a23_project`, `a23_iff`). Take a thread-fair annotated run that stops
   only where the rule allows and violates the property for `b'`. Step 4 lifts it to a behaviour,
   and step 3 makes that behaviour satisfy `Fair_P`, stop only where the rule allows, and violate
   the property for `Θ_0⁻¹(b')`. Conversely, step 5 projects a behaviour with those properties for
   `b` to a run the code stores, and step 3 gives it those properties for `Θ_0(b)`.
7. *Lassos* (`Behaviour.lean::lasso_of_behaviour`, `project_lasso`, `a23`, `a23_on`). Since a
   lasso is a run, one direction is step 6. For the other, fold a behaviour `c` that satisfies
   `Fair_P`, stops only where the rule allows and violates the property at index `i` into a
   lasso with the same three properties. With finitely many reachable states, some segment
   `[n_0, m)` with `n_0 ≥ i` and `c_m = c_{n_0}` holds every state and every step that `c`
   takes after `n_0` (`Behaviour.lean::recurrent_segment`). The lasso runs `c` up to `m` and
   then that segment forever.
   - An instance enabled at every state of the segment is enabled at every state after `n_0`.
     So `Fair_P` makes `c` take it after `n_0`, on a step that the segment holds.
   - If the lasso stutters forever at `q`, the segment holds `q` alone, so `c` stays at `q`
     after `n_0`, and the stop rule holds at `q`.
   - Each state of the lasso after `i` is a state of `c` after `i`, where the goal fails.

   Step 5 projects that lasso to an annotated lasso. Let the lasso repeat with period `q` from
   `k`. The frames `Θ_{k+jq}` lie in the finite `G_{c_k}`, so two of them are equal,
   `Θ_{k+j_1q} = Θ_{k+j_2q}` with `j_1 < j_2` (`repeats_in`). `Θ_{n+1}` is a function of `Θ_n`
   and of `c_{n+1}`, which is periodic. So the frames are periodic, with period `(j_2 − j_1)q`,
   from `k + j_1q` (`periodic_of_recurrence`), and with them `r`, `t` and `σ`. Step 3 carries
   the three properties to it. Step 1 gives `a23_on` from `a23`. ∎

**The lemmas on a quotient cycle.** These lemmas serve the code's lasso lift and its per-cycle
rule, which steps 1 to 7 do not use.

- *The group is constant on a cycle* (`ARun.mem_succ`, `ARun.mem_const`). Along an annotated run,
  `g ∈ G_{r_n}` gives `g ∈ G_{r_{n+1}}`: a step keeps `g`, and `G_{σ_n(t_n)} = G_{t_n}`. So the
  groups only grow. On a lasso from `k` with period `q` they come back after `q` steps, so
  `G_{r_n} = G_{r_k}` for `n ≥ k`, and each `σ_n ∈ G_{r_{n+1}} = G_{r_k}`.
- *Finite order* (`order_exists`). The composite `P = σ_{k+q−1} ⋯ σ_k` lies in the finite
  `G_{r_k}`, so two of its powers are equal, `P^{j_1} = P^{j_2}`. As `apply` is injective,
  `P^d` with `d = j_2 − j_1 > 0` acts as the identity on states, instances and bindings.
- *Lasso closure* (`lasso_closure`). Over `d` copies of the cycle, the lift of step 4 comes back
  to its concrete state. So the lift of an annotated lasso is a concrete lasso of period `dq`.
- *Thread pieces* (`threadFair_iff_pieces`). On an annotated lasso every thread is periodic, with
  one period for all threads. A thread is fair iff, within one period, it takes its instance on
  some edge or meets a stored state where the instance is not enabled.

`can` need only pick a member of the orbit, as a function of the state, so the theorem holds
past `BRANCH_CAP` too (A21). Commutation on reachable states alone is not enough: a stored state
is an image `g(s)` of a reachable `s`, which need not be reachable when `g` was not in the group
of an earlier state on the path to `s` (`SymmetryLivenessExample.lean::reach_only_counterexample`).

*Discharging the hypotheses on the code's runs.* (Sym) is Theorem A18 (`symOn`). (SymLive):
`Group::apply` moves a binding slot by slot. A fair instance `(a, κ)` restricts a binding to the
slots of `a`'s non-`New` parameters (`types.rs::Property::fair_instance`), so when a step's
binding `p` extends `κ`, `g(p)` extends `g(κ)`. This is read from the code. (LiveEqui): the
trigger, `present ∧ link`, reads rows that are present. The scan applies (G1) to the predicates
of properties as to guards (`explore/symmetry/breaks.rs::scan`). So step 4 of A18's proof,
applied to the trigger, makes it read the same on `s` and `g(s)`. Take `after(b)` to be the
states at which every row of `b` is present or deleted. The trigger implies it, and a step keeps
it, since a slot never becomes absent again. The trigger of N9's search is D21's
`trig_b ∧ cause_b`. `cause_b` is a predicate of the property too, so the scan applies (G1) to
it, and it is read where the trigger holds, where the rows of `b` are present. On it, the goal reads owners only on rows that
`apply` relabels (G5), so by (G1) it reads the same. (G6) above argues this for every read of a
property. The stop rule of `Plain` is `True`; that of `WfNext` is step 2's `¬Moves`. This
discharge is paper only: the Lean model has no properties, and
`SymActSearch.lean::orbit_verdict` takes (LiveEqui) as a hypothesis. (Canon) is read from the
code. `explore/symmetry/canon.rs::Canon::canonical` returns `apply(π, s)` for a `π ∈ G_s` that it
computes from `s` alone; past `BRANCH_CAP` it keeps the first branch. `Twin::Canonical` checks
the least layout when `|G_s| ≤ 1,296`. (Finite): the domains are bounded, and `G_s` is a product
of permutation groups of finitely many slots. The fair instances enabled at a state are
finitely many, since each is an action and a tuple of slots.

*The search.* `explore/orbit/liveness.rs::Product::search` does not enumerate lassos. It searches
pairs `(r, b)` of a stored state and a binding. `proofs/lean/RdraIsh/SearchSym.lean` defines what
the search computes, and names the code that computes each part
([proofs/lean/README.md](../../proofs/lean/README.md#the-liveness-searches)):

- A real edge from `r` to a concrete successor `t ≠ r` takes `(r, b)` to `(can(t), π(t)(b))`
  (`REdge`, `pn`). The goal-free pairs and the real edges between two of them form a graph
  (`PN`, `PEdge`). Its components are the sets of mutually reachable pairs (`SameP`).
- A thread node `(u, x)` is a pair `u` and a fair instance `x`. An edge along `t` inside the
  component takes it to `(pn(t)(u), π(t)(x))` (`TEdge`). A *piece* is a component of that graph
  with the direction of its edges forgotten (`Conn`). A node is *covered* when its instance is
  not enabled at its stored state, or an edge of the component from it takes its instance
  (`Covered`).
- A component is *fair* when it has a cycle (a second pair, or a pair with an edge to itself,
  `Candidate`) and every piece holds a covered node (`FairPComp`). A *stop pair* is goal-free,
  has no fair instance enabled, and meets the stop rule (`PStop`).
- The search *accepts* when a stored pair where the trigger holds and the goal fails reaches, by
  pair edges, a stop pair or a pair of a fair component (`OAccepts`).

**Theorem A23 for the search.** Assume the hypotheses of A23, a finite list of the fair instances
enabled at each state, and a stored graph that is the complete run of the quotient. Then the
search accepts iff some binding has a behaviour of `T` that satisfies `Fair_P`, stops only where
the rule allows, and violates the property (`SearchSym.lean::oAccepts_on_iff`). That holds iff
N9's search on `T` accepts for some binding (`SearchSym.lean::oAccepts_on_iff_accepts`). So the
property holds for every binding iff the search does not accept
(`SearchSym.lean::liveHolds_iff_not_oAccepts_on`), iff no annotated lasso as in A23 violates it
(`SearchSym.lean::not_oAccepts_on_iff_lassos`). With A18 from (G1)–(G6), this is
`SymActSearch.lean::orbit_verdict` (and `SymActSearch.lean::orbit_verdict_wfNext` under
`WF_vars(Next)`).

*Proof.* The theorems named are those of `proofs/lean/RdraIsh/SearchSym.lean` unless stated
otherwise.

1. *Restriction* (`quotient_restrict_W`, `restrict_rEdge_iff`, `restrict_lists`). The search runs
   on `T|W`. Its stored states lie in `W`, since each is an image of a reachable state and `W` is
   closed under every `G_s`. From a state of `W`, `T|W` has `T`'s real edges and `T`'s enabled and
   taken instances (A23, step 1). The lists of reachable states and enabled instances given for
   `T` serve `T|W`. Steps 2 to 5 take the commutation on every state, on `T|W`.
2. *A thread edge can be walked back* (`TEdge.reverse`, `Conn.star`). Let a thread edge go from
   `a` to `b` inside a component. The pairs of `a` and `b` lie in that component, so a path of
   pair edges leads back from the pair of `b` to the pair of `a`. The edge and that path close a
   walk at the pair of `a`. As on a quotient cycle, the composite permutation `P` of the walk lies
   in the group of the stored state of `a`, and some `P^d`, `d > 0`, acts as the identity on
   instances. So `d` rounds of the walk, followed from `b`, bring the thread back to `a`. A piece
   is therefore strongly connected: from each node of a piece a path of thread edges reaches every
   other node.
3. *(⇐) An accepted pair gives a fair violating behaviour* (`Candidate.cycle`,
   `FairPComp.cover_one`, `FairPComp.cover`, `run_of_parts`, `behaviour_of_oAccepts`). Let a
   stored pair `s`, where the trigger holds and the goal fails, reach an end `e` by pair edges.
   - If `e` is a stop pair: take the stored path to the state of `s`, the pair path to `e`, and
     a stutter at `e` forever. The run violates the property, since the binding the trigger holds
     for moves along goal-free pairs. It is thread-fair, since from some point it stays at `e`,
     where no fair instance is enabled. It stops only at `e`, where the rule holds.
   - If `e` lies in a fair component: for a pair `u` of the component and an instance `y`, the
     piece of `(u, y)` holds a covered node. By step 2 a path of thread edges reaches that node.
     A walk inside the component follows the path, then the covering edge or a closed walk (the
     component has a cycle), then returns to `e` (`FairPComp.cover_one`). Take the finite list of
     the instances enabled at the stored state of `e`. One such walk per instance on the list,
     each starting where the previous walks left that instance's thread, is a closed walk at `e`
     with at least one edge. On it the thread of every instance enabled at `e` is covered
     (`FairPComp.cover`). This is the one use of the list of enabled instances. The run repeats
     that loop forever. At the start of each round every thread is at `e`, with some instance. If
     that instance is not enabled there, the thread is covered at once; if it is enabled, the
     thread is covered within the round. So a thread enabled forever is taken infinitely often,
     and the run is thread-fair. It never stutters forever, and it stays in goal-free pairs.
   - The parts form an annotated run (`run_of_parts`). Step 6 of A23 lifts it to a fair violating
     behaviour of `T` (`SymmetryLiveness.lean::a23_lift`).
4. *(⇒) A fair violating behaviour makes the search accept* (`skipFrame`, `skipFrame_mem`,
   `oAccepts_of_lasso`, `oAccepts_of_behaviour`). Fold the behaviour into a lasso `c` (A23,
   step 7), violating for `b_0`. Take frames that stay put on a stutter: `Θ_0 = π(c_0)`;
   `Θ_{n+1} = Θ_n` when `c_{n+1} = c_n`, else `π(Θ_n(c_{n+1})) Θ_n`. Then `Θ_n ∈ G_{c_n}`, as
   in A23's step 5. The pairs `P_n = (Θ_n(c_n), Θ_n(b_0))` are stored pairs. They are equal
   across a stutter, and joined by a pair edge across a move: the edge is real, by the
   injectivity of `apply`. From the violation on they are goal-free, by (LiveEqui) on
   `after(b_0)`. They repeat, since the frames at the base state of the lasso lie in one finite
   group, as in A23's step 7. Two cases remain:
   - If the lasso settles at `c_A`, then `P_A` is a stop pair. An instance enabled at `c_A` would
     be enabled forever and never taken, since a stutter takes nothing, against `Fair_P`. The
     stop rule holds at `c_A`, and so at its image.
   - Otherwise the repeated pairs lie in one component, which has a cycle. Take a node `a_0` of a
     piece. Its pair lies in the component, so a path of pair edges leads from it to `P_A`. That
     path carries `a_0` by thread edges to a node `(P_A, Θ_A(f))` of the same piece. Along the
     lasso from `A`, the thread of `f` moves by thread edges inside the component, so it stays in
     the piece. If no node of the piece were covered, then by step 2 of A23 at each frame, `f`
     would be enabled at every `c_m`, `m ≥ A`, and never taken, against `Fair_P`. So the
     component is fair.

   The pair at the violation reaches `P_A` by pair edges along the lasso.
5. *Verdicts* (`oAccepts_iff`, `oAccepts_iff_accepts`, `not_oAccepts_iff_lassos`, and their `_on`
   forms). Steps 3 and 4 give the first equivalence. `Search.lean::accepts_iff` (N9) gives the
   second. Theorem A23 gives the last. ∎

*The code's pieces* (`threadsCovered_iff`, `fairPComp_iff_code`).
`explore/orbit/liveness.rs::Product::threads_covered` builds the pieces with union-find over the
nodes of enabled instances only. It marks a node in three cases: an edge of the component takes
its instance; an edge goes from it to a node whose instance is not enabled; or an edge comes into
its pair from a pair where no enabled instance moves to its instance. This is the criterion of
the search. Take an enabled node of a piece that holds a covered node. Either the path to that
node stays among enabled nodes and ends at an edge that takes its instance, or the path leaves
the enabled nodes at an edge whose enabled end the code marks in its second or third case.
Conversely, each mark of the code is a covered node of the full thread graph, or one edge away
from one.

*Read from the code, not derived.* The theorem states the search's acceptance criterion on a
complete graph. These steps from it to the code are read from the code:

- Tarjan's algorithm (`explore/graph.rs::Adjacency::components`) computes the sets of mutually
  reachable pairs.
- The graph is complete. N10's cut at `expanded` on a capped graph is not modelled.
- `OrbitRun::is_real` is `REdge`'s `t ≠ r`. `explore/probe.rs::Probe` classifies a move as a
  stutter (`Loop`) exactly when its successor is its source, and as an orbit self-edge
  (`OrbitSelf`) when its canonical form is the source.
- The permutation the run stores for an edge is `π(t)` of the edge's concrete successor
  (`RunOf`), and `Canon::canonical` is a function of the state.
- The bit sets and sorted lists that hold `always`, `taken` and the enabled instances, and the
  choice of binding and start, are not modelled: the verdicts quantify over all.
- The counterexample the code builds (the walk, the fair loop, the lasso lift and its
  `MOST_COPIES`) is not modelled. It certifies itself at run time (N11, N16).

The orbit search on a reduced cone, and the run of `S` up to `H` (A22), are not modelled.

**The entity-cone findings up to symmetry (P-sketch).**
`explore/orbit/findings.rs::OrbitRun::state_model_findings` reads the witnesses, the runs and
the never-runs off stored states and action labels, as a plain run does. A18 makes those the
same on every member of an orbit. A `.when` arm compares a slot across an edge, so it reads the
edge's concrete successor: the stored target taken back through the edge's permutation. The
deadlock pass follows one slot along paths, so it runs on (stored state, slot) pairs, with the
edge's permutation `σ` taking slot `j` of the concrete successor to slot `σ(j)` of the stored
target. A slot is a binding of one row. Steps 3 to 5 of A23, with a slot in place of a binding,
carry each pair of the plain pass to a pair of the stored graph through the frames, and back. One
step is not derived: that the deadlock criterion of the pass (an instance stuck at a value, its
exits and what blocks each) reads the same on `(s, j)` and `(g(s), g(j))`, as (LiveEqui) asks of a
trigger and a goal, so that the pairs pass finds a deadlock exactly at the images of the plain
pass's. No theorem states it, and the Lean model has no deadlock pass.

*Evidence for A23.* For liveness, P-full relative to (Sym), (SymLive), (LiveEqui), (Canon),
(Finite), the list of enabled instances and the complete graph, by the two proofs above. For the
entity-cone findings, P-sketch. M:
- The Lean proofs named in each step. `SymmetryLiveness.lean::a23_on` and
  `SearchSym.lean::liveHolds_iff_not_oAccepts_on` take the commutation on `W` only, and
  `SymActSearch.lean::orbit_verdict` composes them with A18 from (G1)–(G6), so no commutation
  outside `W` is assumed.
  - `ExampleSymLive` (`SymmetryLivenessExample.lean`) discharges every hypothesis on two
    interchangeable instances. Its quotient cycle has the swap as its composite and closes after
    two copies. `SearchExample.lean` runs the search on it.
  - `ExampleReachOnly` is a system with commutation on its reachable states whose canonical
    quotient reaches a state its plain run does not.
  - The proofs cover liveness, not the entity-cone findings, and do not read the code.
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
([native.md, N16](../theory/native.md#counterexamples-what-is-checked-before-printing)).
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

*Evidence for A18.* P-full given (G1)–(G6) in the form `breaks.rs::scan` states them, `WF`,
and the two facts of the builder (`Built`), by the proof above. Three of its parts are paper
only: the step on least depth, (G6) outside guards, and a guard's read of a `New` row's owner
whose parent's class does not move. M: the Lean proof
`proofs/lean/RdraIsh/SymActions.lean::step_comm` derives the commutation on `W` over a slot-level
model of the cone system. In that model (G1)–(G4) and (G6) are conditions on the syntax of guards
and effects, as `breaks.rs::scan` states them, (G5) is the semantics of a delete, and `WF` and
the two facts of the builder are hypotheses. `symOn` gives the `SymOn` that A23 assumes, and
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

### Bound escalation

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

### Caps, the memory limit, and `not decided`

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

### Related work

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

### Threats to validity

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
  on Lemma 6, proved step by step and in Lean from the sixteen fields of `Lemma6Hyp`, of which
  `kept_rd`, `rise_eff`, `folded_og`, `exact`, `folded_names`, `other_names`, `eff_names`,
  `eff_s` and half of `cover` are read from the code and checked by no run, and the part of
  `local_`, `shape` and `new_writes` that ties the slot model to the code's actions is read too.
  The nested case of Lemma 6 and Theorem A8 is sketched. Its fold half is checked against the cone's step relation on the
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
  ([native.md, N16](../theory/native.md#counterexamples-what-is-checked-before-printing)). What still
  panics in a release build on a lift's way, and the reason or assumption each panic rests on,
  is listed in the [claims inventory](../theory/claims.md#panics-left-on-a-lifts-way).
- **Bounded results.** Everything here, escalation included, is a statement at a bound. `ok at n`
  at `β + 1` and `β + 2` adds two bounded results; it is not an induction.


# Chapter 5 The native checker

*Source: `docs/theory/native.md`.*

## The native verifier

<!-- constrained-by ./semantics.md -->
<!-- derived-from ./claims.md#c-the-native-verifier -->

This document states how rdra-ish's native verifier decides a check on a graph that its
breadth-first search (BFS) explored, and on what evidence each step rests. It owns rows N1
to N18 of the [claims inventory](../theory/claims.md#c-the-native-verifier) and uses the notation
of [semantics.md](../theory/semantics.md) (D1 to D25) without redefining it. `T` is the system the
build returns; that it is `T_β(M)` (D13) is claim [I1](../theory/claims.md#0-the-build), not a step
of any proof here.

This document describes main at `91524792`. Code is cited as in
[semantics.md](../theory/semantics.md): paths are relative to `crates/rdra-ish-core/src/lifecycle/`
unless they start with `cli/` (`crates/rdra-ish-cli/`) or name the crate root (`core-root/`,
for `crates/rdra-ish-core/src/`). Test marks, such as "(main only)" and "(release jobs)", are
those of the [claims inventory](../theory/claims.md#how-to-read-a-row).

---

### Scope and terms

<!-- derived-from ../design/native-temporal-verification.md#one-graph-three-kinds-of-question -->

The native verifier is the BFS of `explore/` together with the readers that decide checks
on the graph it stores. It decides every check `verify`, `check` and `states` print except
`proved` and the TLC side of `--with-tlc`. Which system it explores (a cone, a reduced
cone, a quotient by slot permutations) is theme (a); this document takes the explored
system `T` as given and asks what the readers decide about `T`.

Terms used below:

- A **run** is one BFS of a system `T` (`ComponentRun`, built by
  `TransitionSystem::bfs_on` in `explore/mod.rs`). It stores a **graph** `G = (S_G, E_G)`:
  the admitted states numbered `0, 1, …` in admission order, and one **edge**
  `(from, to, binding)` per move read, self-loops included.
- `idx(s)` is the number of state `s`; **BFS order** is the order of `idx`.
- A run is **complete** when no cap stopped it (`ComponentRun::complete`), else
  **stopped**. Stops: the state cap, the memory limit, the time budget, the pattern cap
  (views only), and, for a safety cone run with `Caps::until_all_violated`, the point where
  every check it evaluates has failed.
- `expanded(G)` is `|S_G|` on a complete graph and, on a stopped one, the source of the last
  recorded edge (`ComponentRun::expanded`). States below it had every move recorded.
- A **shortest trace** to a state or step is a finite trace (D14) of least length among
  those that end there.

The questions the readers ask fall into four kinds, as in the design note: a reachable
state (`always`, state rules, `precedes`), a reachable step (`after`, `stable`,
`unchanged`), a reachable fair end (`eventually`, `leads_to`), and a backward closure
(`can_reach`, deadlocks).

### Explicit-state exploration

<!-- derived-from ../design/native-temporal-verification.md#one-graph-three-kinds-of-question -->

**N1 (BFS).** If a run of `T` is complete, then `S_G = Reach(T)`, every step `s → s'` of
`T` with `s ∈ Reach(T)` is an edge of `G` (one per binding and fork that produces it), and
for each non-initial state the parent edge (`ComponentRun::parent`) lies on a shortest trace to it.

*Algorithm.* `Bfs::go` admits the initial states (`TransitionSystem::initial_states`, D4)
in the order that function lists them. `Bfs::explore` then reads the states in admission
order; for each it computes the moves (`TransitionSystem::moves_into`, D6 to D8) and, in
move order, resolves each successor against the store (`Bfs::resolve`): a known state gets
its number, an unknown one is admitted with the current state and the move's binding as its
parent (`Bfs::admit_packed`). Every move becomes an edge (`Bfs::push`).

*Proof sketch.* Admission order is the queue order of textbook BFS, so the depth of the
admitted states is non-decreasing in `idx`, and the first edge that reaches a state comes
from a state of least depth among its predecessors; the parent chain therefore has the
state's depth as its length. Every reachable state is admitted because each admitted state
is expanded before a complete run ends and `moves_into` lists every successor. The one
step this sketch does not derive is that `moves_into` enumerates exactly the successors of
D13 (every admissible binding of D7, every fork of D8); that is evidenced differentially
(TLC on the same cone, `cli/tests/engine_equivalence.rs::a_bfs_and_tlc_reach_the_same_states_and_verdicts`,
which runs in the release jobs only and needs TLC; marks as in
[claims.md](../theory/claims.md#how-to-read-a-row))
and by the twin `Twin::EachEnabled` (`explore/moves/reference.rs`), which compares the
planned enumeration with a filter over all bindings. Two further premises are not part of
this sketch: that the built `T` is `T_β(M)` of D13 (claim I1), and that the edge list reads
back the edges it recorded (every edge number is a checked conversion, through
`explore/edges.rs::edge_number` except an orbit self-edge's index, [The state store and its
encoding](#the-state-store-and-its-encoding)).

### The state store and its encoding

<!-- derived-from ../design/native-bfs-parallel.md#the-shared-table -->

A state of `T` is a byte per cell (`expr.rs::State`), one cell per slot of each variable of
D2. The store keeps it packed: each cell in `ceil(log2 |Domain|)` bits, cells end to end,
the total rounded up to bytes (`explore/packing.rs::Packing::of`). States are found by
their packed bytes in a hash table (`explore/arena.rs::Seen`) under the FxHash-style
multiply-rotate hash of `core-root/fasthash.rs` with a fixed seed (`explore/arena.rs::hash`).

**N2 (Store).** Suppose (i) every domain has at most 256 values, so that each value of
`T_β(M)` is a distinct byte and fits its cell (`explore/packing.rs::bits_for` caps a cell at 8
bits), and (ii) every cell starts at a bit offset that `Packing` holds exactly
(`explore/packing.rs::offsets`, a `u32` per cell). Then on states whose cells hold values
inside their domains, packing is injective and unpacking inverts it; the patch path
(`Packing::patch`, which writes only the cells a move changed into the source's packed row)
equals packing the successor, and `Packing::cell` reads the cell back; the arena, segment and
table read back what they stored.

Both preconditions are enforced by the build. (i): a column whose domain has more than 256
values is the build error `DomainTooLarge` and gets no variable (`build/vars.rs`), so no value
past a byte exists. `types.rs::Domain::of_number` returns `None` past a byte (`u8::try_from`)
instead of wrapping, and `Domain::of_variant` goes through `Value::at`, which panics rather
than wrap. Without this check a nullable Int column at `m = 255` (257 values) or an Enum
column with 256 values and `@null`, or with 257 or more values, would wrap a value onto
another, often onto `Value::NULL`; the built `T` would then depart from `T_β(M)` (claim I1),
and every engine, every replay against `T`, and the store's own checks would inherit the
fault. (ii): `build/mod.rs` sums the packed bits of the whole system in a `u64`
(`explore/packing.rs::packed_bits`) and stops with the build error `StateTooWide` past
`MOST_PACKED_BITS = u32::MAX`, so every offset of a cone, a restriction of that system, fits
the `u32`. For a system the build does not make directly (a reduced cone, which holds
representatives and fold bits, or the extended sub-lifecycle of a reduction), the guard is the
checked add of `offsets`, which panics past `u32::MAX` bits rather than wrap; this document
does not show that those systems stay within the whole system's width. Every claim below that
reads a stored state ("by N1", "by N2") holds under both preconditions.

The offsets matter because `pack_into` and `unpack_into` count offsets as they go, while
`Packing::patch` and `Packing::cell` read the stored offsets. On a plain run every successor
takes the patch path (`explore/probe.rs::Probes::read`); on a run up to symmetry the successor is the
canonical form packed whole (`States::pack_into`), and `Packing::cell` serves the deadlock
readers (`findings.rs::can_leave`, `explore/orbit/findings.rs`). An offset that did not hold its cell
would merge distinct successors and unpack stored rows to states `T` does not have, which
would reach every verdict and warning that reads the store.

The edge index is checked the same way: every edge number (`first`, the edge being pushed, a
parent edge) goes through `explore/edges.rs::edge_number`, which panics past the last `u32`
below the `INITIAL` marker instead of wrapping. The one exception is the index of an orbit
self-edge: `explore/bfs.rs::Bfs::take` converts it with a `u32::try_from(…).expect` of its own,
which panics past `u32::MAX` but, unlike `edge_number`, does not exclude the `INITIAL` marker. A
wrapped index in `EdgeList::first` would attribute edges to the wrong sources, and liveness,
`can_reach` and deadlocks would read a graph that is not `G`. A run also stops, as at the state
cap, before it numbers a state `u32::MAX`, the marker of an unresolved move and of an
unnumbered component (`Bfs::admit_packed`, `MOST_STATES`).

*Evidence.* Kani proves the round trip, the patch path, that the arena and the segments
read back what was pushed, and the table's tag match and growth rule (no harness inserts
into the table and looks a row up), on small widths (4 or 11 cells, widths up to 8 bits; the
harnesses are listed in row N2). The table lookup compares whole packed rows
(`Seen::find_hashed`), so a hash collision costs time and never merges two states; the hash's
quality does not bear on correctness. The seed is fixed and no table is iterated into the
output (`core-root/fasthash.rs` states the rule for `FastMap`), so the hash does not bear on determinism
either. `Packing::pack` panics when a value does not fit its bits, in release builds too; that
check cannot catch a value wrapped to a byte before it reaches the store, which is what (i)
prevents. The shadow twins check the store against a reference: `Twin::Packed` compares
`States::patch` with a whole pack and `States::cell` with the unpacked row
(`explore/arena.rs`) on every read, and `Twin::Seen` keys its reference table on unpacked
states (`core-root/shadow.rs` gives neither a cap); `shadow` builds run only in tests and CI.
The bounds are pinned by tests, not by Kani:

- Wide states. `cli/tests/verify_cli.rs::a_state_past_bit_65535_keeps_its_verdicts` (main
  only) runs the fixtures `cli/tests/fixtures/wide_state/` at
  `--instance-count 255 --int-range-max 255`: one entity `Row` with 34 `Int @default(0)`
  columns, a cone state of 255 × (34 × 8 + 2) = 69,870 bits, with cells past bit 65,535. One
  model has two creating use cases that differ only in `c32` and a `forbidden` over every
  column, and must give `counterexample`; the other has one creating use case and must give
  `ok`. The test asserts the verdicts written in the fixtures at 255 instances only.
  `explore/packing.rs::tests::a_state_past_bit_65535_packs_patches_and_reads_back` checks the patch, the
  unpack and `Packing::cell` on either side of the first cell past bit 65,535.
  `build/mod.rs::width_tests::a_state_past_the_packed_width_is_a_build_error` checks the
  threshold function `too_wide` one bit past a width and at `MOST_PACKED_BITS`, and that the
  wide model builds without `StateTooWide`.
- Wide domains. The engine-corpus cases `width_nullable_int_top` (the top value of a nullable Int
  column, one value past a byte) and `width_nullable_enum_256`,
  `cli/tests/check_cli.rs::an_enum_column_takes_no_more_values_than_a_state_holds`, and the
  reference interpreter's `core/tests/reference/mod.rs::a_value_past_a_byte_is_not_wrapped`,
  which decodes each native state through `Domain`'s documented encoding rather than the
  builder's tables.
- Edge numbers. `explore/edges.rs::tests::an_edge_number_past_u32_panics`.

### Memory accounting and the stop rule

<!-- derived-from ../formal-verification.md#memory-state-caps-and-threads -->
<!-- derived-from ../design/native-bfs-parallel.md#the-memory-pool -->

**N3 (Charge).** Before each admission `Bfs::admit_packed` computes the bytes the store
would hold with one more state (`Bfs::bytes_with_one_more`) from the lengths and capacities
of the run's own collections, and asks the run's `core-root/memory.rs::Charge` to cover
them; a refusal stops the run before the state is stored. The count is
`kept_with(1) + Seen::bytes_with(1)` (`explore/store.rs`): the states, the steps (parents
and edges), the orbit self-edges and permutations, the bindings' bytes, and the layers,
each from its length and capacity, plus the table. It is a function of the
sequence of admissions and edges alone (`explore/store.rs` models it as `store_bytes`), so
for a fixed sequence it is the same on every machine and thread count. Kani proves
`store_bytes` never overflows; that each of its terms only grows, from which `store_bytes`,
a saturating sum of products of those terms, all at least 0, does too (the harness
`store_bytes_only_grows` does not call `store_bytes` itself); and that without the lists of
segments it never counts more than a simpler model that charges an 8-byte parent per state
and a 12-byte edge in doubling `Vec`s (the lists, 24 bytes a slot past `2 * SEG` rows, can make
it count more at a power of two, as `store_bytes_is_at_most_the_model_before`'s comment says);
the unit
test `store_bytes_models_the_store_a_run_counts` (main only) holds the model above what a run counts.
The charge covers the state store only: the passes over a finished graph (liveness,
`can_reach`, deadlocks, the partition) and the workers' scratch are outside it, as
formal-verification.md says.

**N15 (Stops give `not decided`).** On a stopped graph no reader prints `ok`: the run's
result is `not decided` unless a violation was found in the part explored, and a
counterexample found there is one of `T`.

*Proof sketch.* Every reader below returns `Undecided` (or `NotDecided`, or leaves the
finding open) on a stopped graph unless it found a violation in the part explored: N5 and the
`after` check by their code paths, N8 by the `complete` guard of `answer_targets`, N14 by
`Progress::NotDecided`, N9 by N10. The CLI maps that to `not decided`
(`cli/src/verify/end.rs::End::result`: with no failure and no tool error, a capped,
undecided or denied-unproved finding makes the result `RunResult::NotDecided`). A stopped
run's safety counterexamples are paths of `G`, hence traces of `T`, and its liveness
counterexamples are those of `T` by N10. ∎ The step not derived is that every reader takes
one of those paths, which is read from the code. The time budget (`Caps::deadline`, read
every 256 admissions) is the one stop whose position depends on the machine.

### Parallel exploration

<!-- derived-from ../design/native-bfs-parallel.md#why-the-run-is-the-one-thread-run -->
<!-- derived-from ../design/native-bfs-parallel.md#holding-it-correct -->

`Bfs::explore` hands chunks of 128 consecutive unread states (`CHUNK`) to worker threads,
which expand them (`explore/bfs.rs::expand`): compute moves, canonical forms under symmetry, packed
successors and their hashes, a `Probe` per move, and a dense binding number. The caller,
the **reader**, takes the expanded chunks back in hand-out order (`Workers::next`) and does
everything that changes the run: numbering, admission, edges, bindings, violations,
charges, and stops (`Bfs::take`, `Bfs::resolve`).

**N4 (Same run on any thread count).** Let `R_k` be the run of `T` with `k` workers and
`R_0` the run with none (`Threads::Fixed(0)`). If no time budget stops either and each runs
alone in its memory pool, `R_k` and `R_0` have the same states, numbering, parents, edges,
bindings, diagnostics and stop point.

*Proof.* Induction on the moves in reading order `m_1, m_2, …` (by state, then by move
within the state). Let `Σ_j` be the reader's state after `m_j`. Claim: `Σ_j` is the same in
`R_k` and `R_0` for every `j`.

1. *The moves of a state, and their order, are a function of the state.*
   `explore/moves.rs::TransitionSystem::moves_into` always calls
   `reduce/mod.rs::TransitionSystem::moves_of_into`. That function goes through
   `each_raw_move`, which iterates the actions in index order and, per action, the bindings
   `each_enabled` yields and the successors `each_successor` yields; without a reduction
   that is all it does. With one, it skips the actions `Reduction::skips` names (a lookup in
   the reduction's `skip` table), rewrites the S part of a T step's successor through
   `Reduction::recanon` and `Reduction::write`, and appends the class moves
   `r.moves[class]`, where the class comes from `Reduction::class_into`. These read the
   state `s`, the system's immutable fields (the `class_of`, `rises`, `reps` and `moves`
   tables) and buffers they clear or overwrite first (`out.reduced`, `scratch`); `reduce/`
   has no interior mutability outside the quotient cache (`reduce/cache.rs`), which this path
   does not touch. The one lazily filled field on the path, `explore/plan.rs::plans`, is a
   `OnceLock` of a function of the actions alone. Canonicalization (`Canon`) and packing
   carry no state between calls. A worker and the reader call the same functions,
   so the move list read at `m_j` is the same, and so is each move's `Probe::Loop` or
   `Probe::OrbitSelf` flag, packed successor, hash and binding number. Chunk boundaries
   differ between `R_k` and `R_0` (the reader expands what no worker holds), and nothing
   below depends on them.
2. *The table only grows and never renumbers.* `Bfs` never removes a state; `Seen` inserts
   each state once under the number it was admitted with.
3. *Each resolution returns what `R_0`'s lookup returns.* For a `Probe::Miss` the reader
   looks the successor up itself, in its own table, which by the hypothesis is `R_0`'s at
   `m_j`. For `Probe::SameAs(i)` the successor equals that of an earlier move `i` of the
   same chunk, which was a `Miss` (only misses enter the chunk's dedup map) and which the
   reader resolved at some `m_{j'}`, `j' < j`; the state found or admitted there is in the
   table at `m_j` with that number, so `R_0`'s lookup returns it. A `Probe::Hit(i)` occurs
   only when workers probe the shared table (`Probing::On` or `Forced`), never in a release
   build (`Probing::of_build`); a hit names a row the reader published before the chunk was
   handed out and compares the bytes, so by (2) it is `R_0`'s answer as well. A `Miss`
   under probing is then looked up only among the states admitted after publication
   (`Recent`), which is correct exactly when the shared table's lookup has no false
   negatives; that property is the concurrency argument of native-bfs-parallel.md and is
   checked only by the twins named below.
4. *Every other update reads only `Σ`.* Binding ids are assigned in first-seen order on the
   reader (`Bindings::id`, `id_numbered`); violations keep the first per name
   (`Bfs::violation`); the stops are evaluated on the reader at the admission that causes
   them, and the charge requested is a function of `Σ` (N3).
5. *Work past a stop is discarded.* Chunks expanded after the stop are dropped unread.

Hence `Σ_j` agrees for all `j`, and with it everything the readers below compute. ∎

*Steps that rest on something other than this proof.*

- *The pool.* "Alone" matters: `Charge::cover` succeeds or fails against what the other
  runs of the command hold (`MemoryPool::charge`), and with `RDRA_JOBS` cones run side by
  side. Two mechanisms restore the one-job result, and they use two different tests, here
  called *crowded in the pool* and *met the limit*. In the entity pass,
  `targets.rs::TransitionSystem::run_cones` defines the result as that of the cones run one
  at a time in cone order, each with the room the earlier results leave: a run that was
  crowded in the pool (the `crowded` field of `core-root/memory.rs::Watched`, set when a charge is refused
  although it fits the pool's room alone) or whose peak exceeds that room runs again under
  `pool.with_room(room)`, an explicit room computed from the results before it. The quotients
  are reduced one at a time in target order (`cli/src/verify/jobs.rs`), so what the quotient
  cache holds is the same on any job count. native-bfs-parallel.md, "The memory pool",
  argues the same for that rerun. The CLI's safety and property passes
  (`cli/src/verify/bfs.rs::bfs_cones`, the `crowded` flag of `Native` in
  `cli/src/verify/native.rs`) use the other test: `cli/src/verify/bfs.rs::crowded` is
  `met_memory_limit`, true for any `MemoryCapReached` diagnostic, crowded in the pool or
  not. A run that met the limit beside others would stop at another point than it does alone,
  and a safety cone with partner properties would print another state count on another job
  count. So the first run's memory stop marks the cone, and every rerun of both passes goes
  alone under `Caps::alone`, a child
  pool with the room the command's pool has left (`MemoryPool::whole`), which refuses by its
  own room whatever the parent holds. The reruns come after every run of the pass has ended,
  and the results a pass keeps (`BfsCone`, `Plain`, `Native`, `Diag`, the entity pass's
  `Bfs`) hold no charge, the quotient cache being dropped at the end of the entity pass; so
  the room left is the whole room, which a debug build asserts at each rerun. A rerun's stop
  point is then a function of the run alone (N4), the same as at one job, where every run of
  a pass already had the pool to itself; a run that met no limit beside others meets none
  alone, with at least as much room. Evidence:
  `cli/tests/memory_cli.rs::safety_and_property_passes_stop_where_they_stop_one_at_a_time` (main only)
  (1, 3, 8 jobs at 7, 8, 10 MiB),
  `cli/tests/memory_cli.rs::cones_side_by_side_stop_where_they_stop_one_at_a_time` (main only) and
  `::a_stopped_entity_cone_leaves_its_room_on_any_number_of_threads` (main only).
- *The table under probing* (step 3) holds only on the probe paths, which release builds
  do not take.

*Evidence by twin.*

| Twin | Fast path | Taken in a release build | Twin runs |
|---|---|---|---|
| `Twin::SameAs` | `Probe::SameAs` (chunk dedup) | yes | every `shadow` run |
| `Twin::DenseBinding` | `Bindings::id_numbered` | yes | every `shadow` run |
| `Twin::Seen` | `Seen` lookups | yes | every `shadow` run |
| `Twin::Hit` | `Probe::Hit` | no (`Probing::Off`) | `shadow` runs under `RDRA_ISH_FORCE_PROBES` (one CI run, clinic-ops) |
| `Twin::Recent` | the reader's `Recent` lookup | no | `shadow` runs under `RDRA_ISH_FORCE_PROBES` |
| `Twin::SeenGrowth` | the growth of `Seen`'s index, after every insert: the shared table's (`table::Writer`) under probing, the plain one's otherwise | yes | every `shadow` run |

The unit test `explore/tests.rs::a_run_is_the_same_on_any_number_of_threads` (main only) compares 1, 2,
3, 4, 7 and 17 workers with `R_0`, on a complete and a capped run, with probing on at 4
and 17 workers (`probes_at`), not under `shadow`. The loom model, the test-only jitter and a
Miri CI job named in native-bfs-parallel.md, "Holding it correct", do not exist; the table
and arena tests are Miri-aware (`cfg(miri)` in `explore/table.rs`, `explore/arena.rs`) but
no workflow runs Miri.

**N17 (Output independent of `RDRA_JOBS`).** Follows from N4 for each run, from the
in-order collection of `in_parallel` results, and, for runs that met the memory limit, from
the reruns above. Evidence: as N4.

### Safety

<!-- derived-from ../design/native-temporal-verification.md#one-graph-three-kinds-of-question -->

**N5 (State rules and `always`).** On a complete `G`, a state rule `R` (D16) is violated
iff some `s ∈ S_G` violates it, and `always(x)` (D19) is violated iff some `s ∈ S_G` and
binding `b` have `trig_b(s) ∧ ¬x_b(s)`. The reported trace is a shortest trace to a
violating state.

*Code.* State rules are evaluated at admission (`Bfs::check_invariants`, before the state
is stored), on the unpacked successor; the first violation per rule name is kept, and its
trace is the parent path (`ComponentRun::trace_of`). `always` is evaluated after the run by
`explore/properties.rs::ComponentRun::unsafe_state`: the first state in BFS order where some binding
fails, and its parent path.

*Proof.* Iff: by N1, `S_G = Reach(T)`. Shortest: admission order is BFS order, so the first
violating state admitted has least depth, and its parent path has that length (N1). For
`always`, `ComponentRun::decide_property` re-evaluates the path's states with
`TransitionSystem::property_failure` and checks, in every build (a `PropertyVerdict::Fault`
otherwise), that the failure is at the last state; that holds because every earlier
state of the path has a smaller `idx` and so breaks no binding. ∎ On a stopped graph a violation found is a path of `G`, hence a trace
of `T`; no violation gives `Undecided`.

**`after` assertions (D17).** `Bfs::check_after` evaluates each `after(UC)` on every move of
the last step of `UC` read from an expanded state, on the move's concrete successor, before
the successor is resolved. The trace is the parent path to the move's source and the move
itself (`Reach::Step`); the first violating move in reading order has a source of least
depth, so the trace is shortest.

**Stuttering steps of an `after` assertion.** `Bfs::take` skips `check_after` on a move whose
successor equals its source (`Probe::Loop`), so an `after` assertion is never evaluated on a
step with `s' = s`. This is D17's reading, and TLC's: the TLA+ printer emits `[][… => …]_vars`
(`crates/rdra-ish-emit/src/tla/module.rs`), whose subscript exempts stuttering steps. A last
step that only ever stutters makes the assertion vacuous (N18). The model of
`core/tests/vacuous.rs::an_assert_after_a_step_that_only_stutters_holds_vacuously` shows it:
`Touch` sets `Order.status := open` on a row created `open`, and
`after(Touch).assert(Order.flag == true)` with `flag` false and never written. Every `Touch`
step is a self-loop, so the assertion holds and the vacuity check reports
`Vacuous_AfterAssert_Touch_0`; with `Touch` setting `closed` the assertion is violated. The
test asserts the vacuity at one and two instances and the violation at one.

#### Step forms and order forms

<!-- derived-from ../design/native-temporal-verification.md#one-graph-three-kinds-of-question -->

**N6 (`stable`, `unchanged`).** On a complete `G` the property is violated iff some edge
`(s, s')` with `s ≠ s'` and some binding `b` have `step_breaks(p, b, s, s')`
(`lasso.rs::TransitionSystem::step_breaks`, the condition of D19). The reported trace is a
shortest trace ending with a breaking step.

*Proof.* Iff: by N1 every step from a reachable state is an edge; steps with `s' = s` break
neither form by D19, and the code skips them (`breaking_step` requires `e.from != e.to`).
Shortest: `breaking_step` takes the first breaking edge in edge order; edges are stored in
the order of their sources' `idx`, so its source has least depth among the sources of
breaking edges, and the trace (parent path plus the edge) has depth + 1 steps. ∎

**N7 (`precedes(p, q)`).** On a complete `G` the property is violated iff for some binding
`b` there is a path of `G` from an initial state, through states where `trig_b ∧ p_b` is
false, to a state where `trig_b ∧ q_b` holds (and, being on the path, `trig_b ∧ p_b` is
false there too).

*Proof.* This restates D19 on `G` by N1. `ComponentRun::unlatched_state` runs a
multi-source BFS per binding from the initial states where `trig ∧ p` fails, through such
states, and stops at the first state where `trig ∧ q` holds; the shortest path over all
bindings is reported (the first binding on a tie). A BFS on the subgraph finds a shortest
path in it, and a violation trace must lie in it, so the trace is shortest. ∎ The
check in `decide_property` holds because a binding that failed earlier on the
chosen path would have a shorter path of its own.

### Recoverability

<!-- derived-from ../formal-verification.md#recoverability-on-a-cone -->

**N8 (`can_reach(g)`).** On a complete `G`, `can_reach(g)` (D22) fails for binding `b` iff
some `s ∈ S_G` with `trig_b(s)` is not in `Can_b`, the least set containing the states
where `pres_b ∧ g_b` holds and closed under predecessors that satisfy `pres_b`.

*Proof.* `Can_b` is the least fixpoint of `X = (pres ∧ g) ∪ (pres ∩ pre(X))`, the CTL
`E[pres U (pres ∧ g)]`, and on `G = Reach(T)` with every step an edge (N1) it is exactly the
set of D22. `reach.rs::TransitionSystem::reach_failure` marks the goal states and closes
backwards through `Adjacency::mark_backward` with `within = pres`; self-loops are dropped
from the adjacency, which does not change reachability. The first trigger state not marked,
in BFS order over all bindings, ends the reported trace, which is therefore shortest. ∎

A stopped graph must not be read: an unexpanded state has no out-edges and would look
stuck. The guard is the match arm `Target::Reach(i) if !cones[at].run.complete` in
`targets.rs::TransitionSystem::answer_targets`, which records the property as
undecided; `reach_failure` and `TransitionSystem::verdict` also `assert!` completeness in
every build. The tests, the naive oracle of `core/tests/oracle.rs` and what they leave open
(among it, that the reported state is the first in BFS order rests on the proof above) are
in [row N8](../theory/claims.md#c-the-native-verifier).

### Deadlocks

<!-- derived-from ../state-derivation.md#deadlocks -->

**N14 (Deadlock).** The finding is the one [semantics.md, D25](../theory/semantics.md#d25)
defines. On a complete entity-cone graph, an open value `v` of a lifecycle column `c` of `E`
(not `@end`, and with some `transitions` arm from `v` to another value: `findings.rs::exits`
and `has_way_on`, which `findings.rs::deadlocks` filters on; a value whose only way out is a
delete is an end state and never reported) is reported stuck iff some `s ∈ S_G` and slot `k`
have `E[k]` present, `c[k] = v`, and no path from `s` through states where `E[k]` is present
reaches an edge that changes `c[k]` or makes `E[k]` non-present. The state reported is the
least such `s` in BFS order (over all slots).

*Proof.* `findings.rs::TransitionSystem::can_leave` marks the sources of leaving edges
(source present, and target not present or with another value) and closes backwards
through present predecessors; this is `E[present U leave]` per slot, as in N8. A state not
marked has no such path, which is the definition. The reported state is the minimum over
`(idx, k)`. ∎ On a stopped graph every candidate value is `Progress::NotDecided`. Both
`can_leave` and the trace read packed cells through `Packing::cell`, so they rest on N2's
preconditions.
The evidence is in [row N14](../theory/claims.md#c-the-native-verifier). The naive oracle there picks
the open values with the checker's own filter (`exits`, `has_way_on`), so a fault in which
values count as open passes it.

### Vacuity

**N18 (Vacuity).** `targets.rs::TransitionSystem::vacuity` reports a state rule or `after`
assertion as vacuous only on a complete run of its cone, only when the run reports no
violation of it, and only when no stored state meets one of its premises
(`unmet_of_rule`, `unmet_of_assert`). It is a warning and changes no verdict (D18).

*Proof sketch.* On a
complete plain run "no stored state" is "no reachable state" by N1 and N2, so only under
N2's preconditions, which the build enforces: a store that merged two states would print a
false vacuity warning for the premise only the merged-away state met. On a reduced run or a
run up to symmetry the stored states are representatives; that a premise met by some
member of a class is met by its representative is theme (a). The `after` premise is read
from the step's ends (`ComponentRun::step_ends`): on a run that keeps its edges, from the edge
list, where an edge moves when its ends differ or it is an orbit self-edge
(`explore/graph.rs::ComponentRun::step_ends_with`); on a run that keeps none (one under
`Caps::stop_when_all_violated`), from what `Bfs::push` recorded on every move
(`explore/edges.rs::StepEnds::read`).
`Ends::moved` records whether a move of the step changes the state (an orbit self-edge does),
and only such moves count, so the premise agrees with the check, which is never evaluated on a
self-loop (`Probe::Loop`). A step that only stutters is its own vacuity reason
(`UnmetPremise::OnlyStutters`). ∎ The step not derived here is the one to representatives on a
reduced or symmetric run, which is theme (a).

Properties have a check of the same kind, `targets.rs::TransitionSystem::property_vacuity`,
on a complete run of the property's cone: it warns when no stored state, under some binding
of the binders to present rows, meets the `.along` link together with the form's own
condition (the trigger of a `leads_to`, what a `stable` keeps, the `.when` of an
`unchanged`, what a `precedes` asks for first). It is read off the same run that decides
the property and changes no verdict.

### Liveness

<!-- derived-from ../design/native-temporal-verification.md#liveness-by-fair-components -->

#### The fairness model

The fairness conditions are those of D20: `Fair_P`, weak fairness per fair instance
`(a, κ)`, where `κ` is the slots of `a`'s non-`New` parameters
(`types.rs::Property::fair_instance`), and `WF_vars(Next)`. A run decides a liveness form
under `Plain` (`Fair_P` alone) or `WfNext` (`Fair_P ∧ WF_vars(Next)`)
(`lasso.rs::Fairness`). There is no strong fairness anywhere in the language, the
printer, or the checker. On `G` define, for a state `s`:

- `En(s)`: the fair instances that label an edge from `s` to a state `≠ s`
  (`explore/liveness.rs::FairSteps::enabled`); `Moves(s)`: some edge leaves `s` to a state `≠ s`
  (`FairSteps::moves_on`).
- For a set `C` of states, `Tk(C)`: the fair instances that label an edge `(u, v)` with
  `u ≠ v` and `u, v ∈ C`.

By N1 and the definition of enabledness in D20 (some binding extending `κ` has a successor
`≠ s`), `En(s)` on a complete `G` is the set of fair instances enabled at `s` in `T`, and a
step `s → s'` with `s ≠ s'` takes every fair instance that labels an edge from `s` to `s'`.

**Assumption τ (a step that changes nothing takes nothing).** This document names τ the clause
of [D20](../theory/semantics.md#d20) by which a fair instance `(a, κ)` is taken only by a step
`s → s'` with `s' ≠ s` (some binding of `a` extending `κ` moves `s` to `s'`), although D13
allows a step with `s' = s`. N9's (⇒) direction uses it twice; the counterexamples of (⇐), and
with them N10 and N11, do not depend on it. The code and the TLA+ output read D20 the same way:
`fair_steps` keeps only edges with `from ≠ to`, and the printer's `WF_vars` takes `⟨A⟩_vars`
steps only. D14's behaviors are sequences of states without action labels, so a step `s → s'`
takes every fair instance that labels some edge from `s` to `s'` (`En`, `Tk` above;
`lasso.rs::unfair_loop` likewise), as `⟨A⟩_vars`, a predicate on the pair of states, reads it.
Without τ, a behavior that stutters forever at a state where an instance `(a, κ)` has both a
self-loop and a move to another state would take `(a, κ)` infinitely often on the self-loop
and satisfy `Fair_P`, while the code does not count that state as a stop; an `ok` would then
not be licensed by D20 and D21.

#### The check

Fix an `eventually(x)` or `leads_to(x, y)` with `cause`, `goal` as in D21, a binding `b`,
and a fairness `F`. Let `N_b = {s ∈ S_G : ¬goal_b(s)}` and `E_N` the edges of `G` with both
ends in `N_b` and different ends. A **stop** is `s ∈ N_b` with `En(s) = ∅` (`Plain`) or
`¬Moves(s)` (`WfNext`). A strongly connected component `C` of `(N_b, E_N)` is **fair** when
`|C| ≥ 2` and every fair instance enabled at every state of `C` is in `Tk(C)`. The **ends**
are the stops and the states of fair components. On the reduced graph with looping classes
(`decide_liveness_reduced`, under `StopRule::Looping`), a state of `N_b` with no fair instance
enabled is also a stop when its class can loop (`Reduction::can_loop`), not only when it has no
move; that this is sound is theme (a).

**N9 (Liveness by fair components).** On a complete `G`, if some `s ∈ S_G` with
`trig_b(s) ∧ cause_b(s) ∧ ¬goal_b(s)` reaches an end by a path of `E_N` (of length zero
included), the property fails for `b` under `F` (⇐). Under assumption τ the converse holds
too (⇒), so the property fails iff such an `s` exists.

The hypotheses are N1 (the graph is `T`'s reachable states and all their edges, finitely many)
and D20's reading of a taken instance, τ included. `proofs/lean/RdraIsh/Search.lean` states the
check over them: the reachable states lie in a list, a component is mutual reachability by
`E_N` (`SameComp`), a fair component is `FairComp`, and a stop is a state of `N_b` with no fair
instance enabled where the stop rule holds (`IsStop`). Under `Plain` the rule is `True`
(`isStop_true_iff`). Under `WfNext` it is `¬Moves`, and a state with no move has no fair
instance enabled, so `¬Moves` alone is the stop defined above (`isStop_moves_iff`). The steps
below name the theorem of `Search.lean` that proves each.

*Proof.* (⇐) This direction does not use τ: it needs only that a step to another state
takes the instances that label it, which D20 states. Let `s` reach an end `e` inside `N_b`. Take the parent path to `s` (N1), then
the path to `e`. If `e` is a stop, stutter at `e` forever: under `Plain` no fair instance is
enabled at `e`, so `Fair_P` holds; under `WfNext` no step to another state exists either,
so `WF_vars(Next)` holds. If `e` is in a fair component `C`, follow a closed walk of `E_N`
inside `C` that visits every state of `C` and, for each fair instance enabled at every state
of `C`, an edge of `C` labelled by it (it exists by fairness of `C` and strong connectivity),
and repeat it forever (`FairComp.cycle`; `lasso_of_cycle` folds the path to `s`, the path to
`e` and that walk into a lasso). An instance not enabled everywhere in `C` is disabled infinitely
often, so weak fairness does not constrain it; the others are taken infinitely often. The
walk changes state infinitely often (`|C| ≥ 2`), so `WF_vars(Next)` holds. The behavior
reaches `s`, where the trigger and cause hold, and from there stays in `N_b`, so the goal
never holds at or after `s`; it violates D21 (`lasso_of_accepts`).

(⇒) This direction uses τ twice, marked below. Let `σ` be a behavior satisfying `F` and
`i` a position with `trig_b ∧ cause_b` at
`σ_i` and no goal at any `j ≥ i`. Then `σ_i ∈ S_G` (it is reachable) and every `σ_j`,
`j ≥ i`, is in `N_b`. Two cases, since `S_G` is finite. (a) From some point `σ` stutters at
a state `e` forever. Under `Plain`, a fair instance enabled at `e` would be enabled forever
and never taken (stuttering takes nothing: τ), against `Fair_P`, so `En(e) = ∅`; under
`WfNext`, a step from `e` to another state would violate `WF_vars(Next)`, so `¬Moves(e)`.
So `e` is a stop. (b) `σ` changes state infinitely often. Let `I` be the states visited
infinitely often and the non-stuttering steps taken infinitely often; `I` is strongly
connected within `(N_b, E_N)`, has at least two states, and lies in one component `C`. A
fair instance enabled at every state of `C` is enabled at every state of `σ` from some
point on, so `Fair_P` makes `σ` take it infinitely often, on a step inside `I ⊆ C`, and by
τ on a step that changes the state, an edge of `E_N`; so it is in `Tk(C)`, and `C` is fair. In
both cases `σ_i` reaches the end by the non-stuttering steps of `σ`, which are edges of
`E_N` (`accepts_of_behaviour`; with (⇐), `accepts_iff`, and read as D21's verdict,
`liveHolds_iff_not_accepts`). ∎

*Code.* `explore/liveness.rs::ComponentRun::liveness_search` runs `ComponentRun::avoid` per binding:
the components by iterative Tarjan (`explore/graph.rs::Adjacency::components`), the `always` and
`taken` bit sets per component, the stops, and a backward BFS from the ends over the
predecessors in `E_N` (`toward`). The binding whose failing trigger state comes first in BFS
order is chosen. For weak fairness a single pass over the maximal components suffices,
because enlarging a cycle can only add states where an instance is disabled and edges where
it is taken; strong fairness would need the recursive decomposition of Lichtenstein–Pnueli
and Emerson–Lei, which is why the design note keeps it out of scope.

*Lean.* `proofs/lean/RdraIsh/Search.lean` states the check on a system whose reachable states
lie in a list, with a component as mutual reachability by `E_N` and the stop rule as a predicate
(`True`, `¬ Moves`, or a looping class for `StopRule::Looping`), and proves both directions
(`accepts_iff`): the check accepts iff a behavior that satisfies `F` violates the property.
`SearchSym.lean` proves the same for the search on (state, binding) pairs of a run up to symmetry
(`oAccepts_iff`), also with the commutation only on Theorem A18's set `W` (`oAccepts_on_iff`). Neither reads the code: Tarjan's components, the bit sets and N10's cut are
read from it ([proofs/lean/README.md](../../proofs/lean/README.md#the-liveness-searches)).

**N10 (Stopped graph).** On a stopped `G`, a state at or after `expanded(G)` is never a
stop, and a component that holds one is never fair (`avoid`'s `open` and the `s < expanded`
test). Then every end found has its out-edges fully recorded, so `En`, `Moves` and `Tk` are
exact there, and the (⇐) direction of N9 goes through unchanged on the recorded edges: a
counterexample found is one of `T`. The (⇒) direction does not, so no counterexample gives
`Undecided`. The cut at `expanded(G)` is conservative: a stop can land at the first move of
a state, after states with no moves, and those are treated as unexpanded too.

*Proof.* Let the search on a stopped `G` accept: some `s` reaches an end `e` by edges of `E_N`
that `G` records. Each state on that path is reached in `T` by its recorded edges, since a
recorded edge is a step of `T`, and N1's parent path reaches `s`. If `e` is a stop, it is
expanded, so every move of `e` in `T` is recorded: `En(e)` and `Moves(e)` read on `G` are those
of `T`, and `e` is a stop of `T`. If `e` lies in a fair component `C` of the recorded graph,
every state of `C` is expanded. So `En` at each state of `C` is that of `T`, and the edges of
`C`, recorded steps between states of `C`, are steps of `T`. `C` is then strongly connected by
steps of `T` between goal-free states, holds at least two states, and takes every instance
enabled at all of its states. That is what N9's (⇐) uses of a fair component: it needs neither
that `C` is a maximal component of `T`'s graph nor that `T` has no other edges. So the
construction of N9's (⇐) gives a behaviour of `T` that satisfies `F` and violates the property.
It does not use τ. No Lean theorem states this: `Search.lean` takes the graph complete, and the
cut at `expanded` (`avoid`'s `open` and the `s < expanded` test) is read from the code.

**N11 (Self-certification).** Before a native lasso prints, `decide_liveness` replays it
through the step relation (`lasso.rs::TransitionSystem::replay_lasso`: the first state must
be initial, each step and the back edge must be a move of the named action), re-evaluates
the formula on the infinite behavior it stands for (`explain_property`, `fails_at`), and
checks fairness from the moves at the lasso's states rather than from the search's data
(`unfair_loop`: the instances enabled at every loop state minus those some loop step takes
must be empty, and under `WfNext` a stutter must have no move). A failure is
`LassoFault`, printed as a tool error, never as a finding. This is runtime evidence (R): a
wrong counterexample cannot print, a wrong `ok` can. The fairness check is independent of
`avoid` in code, but both read the same `moves` and the same `fair_instance`, so a fault in
those would pass both.

**The lasso.** `ComponentRun::walk` builds the prefix (parent path), the bridge (the
`toward` pointers), and for a component a loop (`fair_loop`) that repeatedly walks inside
the component to the nearest state where an uncovered instance is disabled or the nearest
edge that takes one, and then back to the end it started from. The loop is in `N_b` and
covers each fair instance, so it is the lasso of the (⇐) proof.

**Where `ok` comes from.** `Answer::of` maps `PropertyVerdict` to the line;
`cli/src/verify/native.rs::native_property_after` orders the runs (`Plain` on the reduced
cone, then `WfNext` on the reduced graph with looping classes, then the unreduced cone, then
`WfNext` there). The two implications of D21 (an `ok` under `Plain` is one under `WfNext`;
a counterexample under `WfNext` is one under `Plain`) and the cone's own fairness are theme
(a).

#### The goal on absent rows

`fails_at` and `avoid` evaluate the goal on a slot whatever its existence, so a behavior
that deletes the row meets `eventually(E.c == v)` when `v` is `c`'s base value, while
`can_reach` keeps the slot present along the path. TLC reads the printed formula the same
way, and language-reference.md and D21 state the reading. A `.present()` goal has the
presence of every bound row in its predicate, conjoined by the build
(`build/preds/temporal.rs::Builder::goal`), so the same evaluation never meets it on a
deleted row.

### Counterexamples: what is checked before printing

<!-- derived-from ../design/native-temporal-verification.md#counterexamples -->
<!-- derived-from ../formal-verification.md#what-verify-checks -->

**N16** claims that every printed counterexample replays. The table records, per check and
per kind of run, what the code does to a trace before it prints. *Replayed* means matched step by step,
action, parameters and successor, against `moves` from an initial state
(`explore/orbit.rs::replays`, which calls `trace.rs::TransitionSystem::replay`, or
`lasso.rs::TransitionSystem::replay_lasso` for a lasso). *Re-evaluated* means the check is
evaluated again on the trace's last state or step. *Plain* is an unreduced run without
symmetry; *reduced* a run of a cone reduced by its independent sub-lifecycle, whose
trace is lifted by `reduce/lift.rs::TransitionSystem::lift`, which checks every lifted step
against the unreduced cone's moves in every build and returns a `ReplayError` on the first
that is not one; *symmetric* a run up to slot permutations, lifted by
`explore/orbit.rs::lift_path` (safety, `always`, deadlocks) or the lasso lift of
`explore/orbit/liveness.rs`. The shared path for stored traces is
`explore/graph.rs::ComponentRun::lifted_path` (behind `trace_to`, `trace_of` and
`trace_along_to`): it replays the stored path on the system the run explored, then lifts it. A
failure is a fault, a tool error that the run counts as such and prints without a trace:
`LiftFault` for a state rule or `after` assertion, `LassoFault` for a property, `TraceFault`
for a deadlock or `can_reach`.

| Check | Plain | Reduced | Symmetric |
|---|---|---|---|
| State rule, `after` | replayed (`explore/mod.rs::ComponentRun::trace_violations`, `trace_of`; `LiftFault` carrying `LassoFault::Replay` on failure); not re-evaluated: the violation was evaluated at admission on the same states | replayed on the reduced system, then lifted step by step (`LiftFault` on failure); re-evaluated on the lifted last state, and a lifted state that no longer violates is `LiftFault` carrying `LassoFault::Satisfies` | lifted (`lift_path`), replayed on the system the run explored, lifted through the reduction when that system is reduced (`explore/orbit.rs`, `LiftFault` on either failure); re-evaluated on the lifted last state (`LassoFault::Satisfies` as in the reduced column) |
| `always` | re-evaluated on the stored path, failing at its last state, in every build (`explore/properties.rs::ComponentRun::decide_property`, `property_failure`; `PropertyVerdict::Fault` with `LassoFault::Satisfies` otherwise), then replayed (`trace_along_to`; `LassoFault::Replay` on failure) | re-evaluated on the reduced path as in the plain column, replayed on the reduced system and lifted step by step (`trace_along_to`), then re-evaluated on the lifted last state (`LassoFault::Satisfies` if it no longer fails) | only on a cone the reduction leaves whole (`cli/src/verify/jobs.rs::PropertyJob::symmetric`), so no lift through a reduction is needed: lifted, replayed (`LassoFault::Replay`) and re-evaluated on the lifted path (`OrbitRun::decide_always`, `LassoFault::Satisfies`) |
| `stable`, `unchanged`, `precedes` | as `always`, plain | never reduced (`decide_property` restricts without reduction) | not run up to symmetry |
| `eventually`, `leads_to` | replayed, formula re-evaluated, fairness re-checked (N11; `LassoFault` on failure) | the reduced lasso is never printed; what prints is either the `WfNext` lasso lifted to the unreduced cone and replayed, re-evaluated and fairness-checked there (`decide_liveness_reduced`; `Spurious` otherwise, `Spurious::Fails` for a lift that fails its step checks), or the lasso of the rerun on the unreduced cone (`Cone_P_Full`); if that rerun stops, the property is `not decided` | run up to symmetry on any cone, but on a reduced cone a counterexample of that run is decided again by the plain runs and never printed (`cli/src/verify/native.rs::liveness_up_to_symmetry`); on a cone the reduction leaves whole, replayed, re-evaluated and fairness-checked after lifting (`explore/orbit/liveness.rs`, `LassoFault` on failure, `LassoFault::Unclosed` past 10,000 copies) |
| `can_reach` | replayed (`targets.rs::TransitionSystem::verdict`, `trace_to`; `TraceFault` on failure); the finding is not decided again on the trace | never reduced (`explore_cone` keeps a `can_reach` cone whole) | not run up to symmetry |
| Deadlock | replayed (`targets.rs::read_entity`, `trace_to`; `TraceFault` in the deadlock's place on failure); not decided again on the trace | `verify` reduces entity cones (`explore_targets_reduced_in`), `check` does not: replayed on the reduced system and lifted step by step (`TraceFault`); the finding is decided on the reduced graph (cones.md, Theorem A8, consequence 3) | lifted (`lift_path`), replayed, and lifted through the reduction when the cone is reduced (`explore/orbit/findings.rs::trace_to_slot`, `TraceFault` on failure) |

Three consequences.

- *What a replay catches.* A replay checks a trace against `T`, so a store that merged two
  states (N2) turns a wrong counterexample into a tool error, though not a wrong `ok`. A replay
  cannot catch a fault in the build of `T` (claim I1): it replays against `T` itself.
- *Every printed trace replays.* It replays on the system the run explored and, when that
  system is reduced or a quotient, lifts with every step checked against the unreduced cone;
  the lifts' own look-ups, the `always` checks, and the search for a class move's S path
  (`reduce/partition.rs::Succ::shortest_path`, which returns `None`, reported as the
  `ReplayError::NoMove` of that step; test
  `core/tests/reduce.rs::a_class_move_with_no_s_path_is_a_replay_error`) are faults when they
  fail (test `core/tests/reduce.rs::a_lift_from_no_initial_state_is_a_replay_error`). A reduced
  liveness counterexample whose unreduced rerun stops becomes `not decided` rather than printing
  the lifted lasso.
- *What is still unchecked or panics.* Deadlock and `can_reach` traces replay, but the
  finding is not decided again on them. The lifted start of a reduced trace is built from an
  initial S state of the start's class (a start with none is `ReplayError::NotInitial`) and is
  not itself checked against the unreduced cone's initial states. Some code on a lift's way
  holds invariants of its own by release panics; the
  [claims inventory](../theory/claims.md#panics-left-on-a-lifts-way) lists them, with the reason each
  holds or the assumption it rests on. A failure there stops the command; none can print a
  trace that does not replay. The lasso
  lift of `explore/orbit/liveness.rs` checks that an edge is the move at its place and that
  its permutation takes the move to the stored target: `Product::lift` returns
  `ReplayError::NoMove` otherwise, which `lasso` reports as `LassoFault::Replay` (test
  `explore/orbit/liveness.rs::an_edge_its_permutation_does_not_explain_is_a_replay_error`).

#### The lift's mechanics

The soundness of each reduction is theme (a) (`cones.md`); this section states
only what the lift computes.

- **Reduced, finite path** (`lift`). The S part of the start becomes an initial S state of
  its class; each T step is kept and the S rises it causes are inserted; each class move
  becomes a shortest S path (`Succ::shortest_path`) from the current concrete S state into
  the target class, or, when there is none, the `NoMove` of that step. The S path is lifted again under an inner reduction, and, when S was
  explored up to its group `H`, first lifted to concrete S states (`OrbitRun::lift_path`).
- **Reduced, loop** (`lift_cycle`). The prefix and `c` copies of the loop are lifted for
  `c = 4, 8, …, 4,096` (`MAX_COPIES`), until the concrete state at the end of one copy
  repeats at the end of an earlier one; the lasso closes there. The lift of a copy is a
  function of the concrete state it starts from, so the copies close within as many copies
  as there are concrete states in the loop's start class; past 4,096 it gives
  `Spurious::Unclosed` and the CLI falls back to the unreduced cone.
- **Reduced, stop** (`lift_stop`). From the lifted last state, a breadth-first walk of at
  most 2^18 states (`MAX_WALK`) through S steps that keep T and `obs` finds a state with no
  move or a cycle; none gives `Spurious::NoLoop`.
- **Symmetric** (`explore/orbit.rs::lift_path`). Each stored edge is recomputed from its source as
  the first move of its action and parameters whose successor `t` canonicalizes to the
  stored target; with `σ` the permutation that takes `t` there, the concrete step is the
  move read through the accumulated `θ`, and `θ := θ ∘ σ⁻¹`. The lasso lift of
  `explore/orbit/liveness.rs` takes instead the move at the edge's place among its source's moves and
  the permutation the annotated run stored for the edge, and returns `ReplayError::NoMove`
  when they do not match the edge. A liveness loop on
  (state, binding) pairs (**N12**) closes after at most the order of the loop's composed
  permutation, capped at 10,000 copies (`explore/orbit/liveness.rs::MOST_COPIES`). **N13**:
  `lift_path` reports a stored edge that is no move of its source as `ReplayError::NoMove`,
  and so does the lasso lift of `explore/orbit/liveness.rs`; that such a failure is a tool
  error and never a printed counterexample is N16 (the table above), and what still panics on
  the way is in the [claims inventory](../theory/claims.md#panics-left-on-a-lifts-way).

The `Twin::OrbitLiveness` twin compares the orbit search with the plain run's `avoid`
(`ComponentRun::liveness_reference`), so it tests the quotient against N9's implementation,
not N9 itself; it skips runs past 200,000 states (`explore/orbit/reference.rs::SHADOW_CAP`), as do
`Twin::OrbitRun` and `Twin::OrbitStateModels`. The twin's description says it checks the
quotient only, and a `shadow` build reports each skipped call.

### Features that add no case to the verifier

<!-- derived-from ../design/counted-events.md#the-transition-system -->
<!-- derived-from ../design/counting-rules.md#native-checker -->
<!-- derived-from ../design/event-order.md#checks-and-findings -->

Raised marks, counted triggers, and counted quantifiers reach the verifier as ordinary
variables, guards and effects, so every statement above applies to them without a new
case. A raised latch or counter is a `VarKind::Raised` variable whose raise and handling
are `Effect::Transition` arms, with the overflow rule an ordinary `Invariant`
(counted-events.md, "The transition system"). A counted quantifier is evaluated by `eval`
over every slot of its entity (counting-rules.md, "Native checker"). Deadlocks read view
columns only, so a mark is never a stuck value; `.fair(UC)` on a triggered use case makes
each of its variants a fair action whose enabledness reads its mark (event-order.md,
"Checks and findings").

### Related work

- **Explicit-state model checking.** The store, BFS and per-state checks follow SPIN
  (Holzmann, "The model checker SPIN", IEEE TSE 23(5), 1997) and TLC (Yu, Manolios,
  Lamport, "Model checking TLA+ specifications", CHARME 1999). Like TLC and unlike SPIN's
  default, exploration is breadth-first, which gives shortest safety traces; unlike TLC, which stores 64-bit fingerprints and accepts a small probability of merging
  two states, and unlike SPIN's bitstate mode, rdra-ish stores each state exactly and
  compares full packed rows, so a hash collision costs time, not soundness; what the store
  rests on is the encoding (N2), not hashing.
- **Fair cycle detection.** Deciding a liveness property under weak fairness by strongly
  connected components of the graph restricted to the goal's complement goes back to
  Lichtenstein and Pnueli ("Checking that finite state concurrent programs satisfy their
  linear specification", POPL 1985) and Emerson and Lei ("Modalities for model checking:
  branching time logic strikes back", Science of Computer Programming 8, 1987), whose fair-
  SCC computation for Fair CTL is the shape of N9 (fair components, then a backward
  closure). On-the-fly alternatives are nested depth-first search (Courcoubetis, Vardi,
  Wolper, Yannakakis, "Memory-efficient algorithms for the verification of temporal
  properties", FMSD 1, 1992) and SCC-based emptiness checks (Couvreur, "On-the-fly
  verification of linear temporal logic", FM 1999). rdra-ish is not on-the-fly: it decides
  liveness after the graph is built, which needs the whole graph in memory but lets the
  same graph answer every property of the cone and gives deterministic lassos.
- **Parallel and distributed state-space search.** Stern and Dill ("Parallelizing the Murφ
  verifier", CAV 1997, LNCS 1254) distribute Murφ's reachability over a distributed-memory
  machine or a network of workstations by message passing.
  Barnat, Brim and Chaloupka ("Parallel breadth-first search LTL model-checking", ASE 2003)
  detect accepting cycles of an LTL product with a distributed, level-synchronized BFS over
  an MPI cluster, through back-level edges; their subject is the cycle check, not only the
  parallel BFS. Holzmann and Bošnački ("The design of a multicore extension of the SPIN
  model checker", IEEE TSE 33(10), 2007) extend SPIN to shared-memory multicores. Laarman,
  van de Pol and Weber ("Boosting multi-core reachability performance with shared hash
  tables", FMCAD 2010) replace static partitioning with one lockless shared hash table that
  all workers probe; rdra-ish's shared table (`explore/table.rs`: control bytes and indices
  in `AtomicU64` and `AtomicU32` arrays, written only by the reader) is of that kind, but
  release builds turn probing off (N4, step 3). rdra-ish parallelizes only expansion: one reader
  commits every state in sequential order, which gives up scaling of the commit step in
  exchange for the exact one-thread run (N4), shortest traces included. TLC's workers take
  states from a shared queue without such a reader. An issue reports that with several
  workers the diameter TLC prints varies between runs, a collaborator explains it by a state
  being discovered first along a longer path, and a maintainer answers that a deterministic
  diameter would need synchronization that limits scaling
  ([tlaplus/tlaplus#883](https://github.com/tlaplus/tlaplus/issues/883)). That TLC's traces
  with several workers can then also differ between runs and not be shortest is our
  reading, not a statement from TLC's documentation.
- **Symmetric counterexamples.** Ip and Dill ("Better verification through symmetry",
  FMSD 9(1/2), 1996) and Emerson and Sistla ("Symmetry and model checking", FMSD 9(1/2),
  1996) explore the quotient by a symmetry group and relate paths of the quotient to paths
  of the original system, which is what `explore/orbit.rs::lift_path` does when it carries the
  accumulated permutation along a stored path (N12, N13). rdra-ish then replays the lifted
  trace on `T` rather than relying on that correspondence alone.
- **TLA+ fairness.** `WF_vars(A)` and stuttering steps are Lamport's ("The temporal logic of
  actions", ACM TOPLAS 16(3), 1994; *Specifying Systems*, 2002). D20's per-instance weak
  fairness is the conjunction of `WF_vars` over instances that the TLA+ printer emits; the
  exemption of stuttering steps from action properties (`[][A]_vars`) is D17's reading of
  `after`, and the same exemption in `WF_vars(A)`, which only `⟨A⟩_vars` steps satisfy, is
  [assumption τ](#the-fairness-model).

### Threats to validity

- **The step relation is trusted.** Every proof above assumes `moves_into` enumerates the
  steps of the built `T` (N1) and that the built `T` is `T_β(M)` of D13 (claim I1). The
  evidence is differential (TLC, which reads the same `TransitionSystem`, so a fault in the
  build of `T` would pass both) and one twin. A reference interpreter (`core/tests/reference/`)
  checks I1 without reading the build's output, on a subset of the language: it explores the
  model on its own and compares initial states, reachable states, labelled edges and broken
  rules with the native run of the whole model, on the engine corpus and on random models.
- **Bounded mechanical evidence.** The Kani harnesses cover widths far below 65,536 bits and no
  domain of more than 256 values; the store's bounds are pinned by the tests named in N2, not by
  Kani. `Twin::Packed` and `Twin::Seen` would detect a wrong offset but no `shadow` input is that
  wide; other shadow twins skip runs above 200,000 or 2,500,000 states, and a `shadow` build
  reports each skip. The probe twins run in one CI job, on code release builds do not run.
- **Readers share code with the checks they certify.** `unfair_loop` and `avoid` both use
  `moves` and `Property::fair_instance`; `explain_property` and `liveness_search` both use
  `holds_bound` and `triggers` (N11). Self-certification catches a fault in the search, not in
  these.
- **Fixed-width integers.** A value that wraps silently in a release build gives a wrong verdict
  within the documented bounds, which is why the value encoding, the bit offsets of the packing,
  and the edge index are checked (N2). The narrowing casts of the lifecycle code go through the
  checked `id8` and `id32` (`narrow.rs`), which panic rather than wrap. The CI jobs that verify
  the samples, the corpus and engine agreement build with the `ci-checked` profile of the
  workspace `Cargo.toml` (release with `overflow-checks`), so an arithmetic overflow panics
  there; the `release` profile, which users run, wraps silently.
- **Fairness reading.** Every liveness `ok` rests on N9's (⇒) direction, hence on
  [assumption τ](#the-fairness-model).
- **Determinism of memory stops beside other runs** rests on the reruns of N4.
- **Parts argued only in part.** That `reduce/lift.rs::lift_stop` finds a stop whenever one
  exists is not argued: past `MAX_WALK` states, or with no stop found, it gives
  `Spurious::NoLoop`, and the CLI then decides the property on the unreduced cone, so a missed
  stop costs time, not soundness. Determinism under `core-root/fasthash.rs` rests on the rule
  quoted in N2 (no table is iterated into the output) being kept by every user of `FastMap`.
  `cli/src/verify/native.rs::native_property_after` is argued only for the order of its runs
  and what it records for the comparison, and `cli/src/verify/shared.rs::Shared::up_to_symmetry`
  only for its use of `PropertyJob::symmetric`, which the N16 table cites.


# Chapter 6 Proofs for any number of rows

*Source: `docs/theory/z3.md`.*

## Theorem proving with z3

<!-- derived-from ./semantics.md -->
<!-- derived-from ./claims.md#b-theorem-proving-with-z3 -->
<!-- derived-from ../formal-verification.md#proofs-for-any-number-of-rows -->
<!-- derived-from ../engine-agreement.md#int-columns-and-now -->

This document describes main at `91524792`.

It states what `verify --prove` decides, under which assumptions, and on what evidence. Code
is cited as in [claims.md](../theory/claims.md#how-to-read-a-row): `core` is
`crates/rdra-ish-core/src/lifecycle`, `cli` is `crates/rdra-ish-cli`. Notation D1 to D25 is
that of [semantics.md](../theory/semantics.md); this document adds only the SMT-side symbols of
[Notation](#notation). Its claims are rows B1 to B18 of the
[claims inventory](../theory/claims.md#b-theorem-proving-with-z3), which gives each row's evidence
classes, tests and open gap; this document gives the definitions and the proofs.

The modeler-facing description, with examples of every line the proof prints, is
[formal-verification.md, "Proofs for any number of rows"](../formal-verification.md#proofs-for-any-number-of-rows).
It also records the design's measurements.

### Contents

- [Results at a glance](#results-at-a-glance)
- [Notation](#notation)
- [The encoding](#the-encoding)
- [Faithfulness of the encoding](#faithfulness-of-the-encoding)
- [Induction and Houdini](#induction-and-houdini)
- [The soundness theorem](#the-soundness-theorem)
- [Candidates and the trust base](#candidates-and-the-trust-base)
- [Verdicts other than proved](#verdicts-other-than-proved)
- [Budgets, solver count, and determinism](#budgets-solver-count-and-determinism)
- [The certificate cache](#the-certificate-cache)
- [Interaction with BFS](#interaction-with-bfs)
- [Reproductions](#reproductions)
- [Threats to validity](#threats-to-validity)
- [Related work](#related-work)

### Results at a glance

The main result is Theorem 1. Assume that z3 answers `unsat` only for unsatisfiable queries,
that the prover takes each answer for the check that asked it, that the build returns
`T_β(M, σ)` at the run's bound (claim [I1](../theory/claims.md#0-the-build)), and that the encoding
is faithful in the sense of [Faithfulness](#faithfulness-of-the-encoding). Then `proved: R`
implies that `R` holds in every reachable state of `T_{β'}(M, σ)` for every bound `β'` in the
proof's scope `Sc(M, β)` (Definition 3). That scope is every bound at which the model builds
without error: every slot count per entity, and every Int top `m'` from the largest Int
literal the model states up to the greatest at which it builds. That greatest top is 255; 254
with a nullable Int column (`DomainTooLarge`); or less where a state would pass its packed
width (`StateTooWide`). `σ` is the run's scope (D24), not the whole model. Every line that
claims a proof prints this scope ([Definition 3](#the-soundness-theorem)).

What carries the theorem, and how strongly each part is supported:

| Part | Support |
|---|---|
| z3 soundness for `unsat` | assumed |
| The printed text means the `Expr` it was built from (H2) | typed builder and its unit tests; assumed beyond them |
| Each answer is read for the check that asked it (H4) | assumed for z3's output protocol; the reader fails closed on any other line (`Z3::exec`, `Solver::check_sat`); no test drives the reader |
| The kept set is inductive relative to the encoding (Proposition 4) | proof from the code (Houdini loop, step pruning) |
| Step pruning (`breaks`) is sound (Lemma F5) | proof from the code |
| The encoding over-approximates `T_{β'}` (Proposition 2, lemmas F1 to F4) | proof sketch from reading the code; the differential tests `proof_equivalence` (with the skips claim [B1](../theory/claims.md#b-theorem-proving-with-z3) counts); the reference interpreter's proof check at `m + 1` and `m + 3` (`core/tests/reference/proofs.rs`); the corpus cases `z3_guarded_present_past_the_top` and `z3_guarded_null_beside_present` for the two choice constructs of [widening](#what-is-folded-what-is-widened-and-what-is-unsupported) |
| The definition is uniform in the bound (Lemma U₀) | immediate from D1 to D12, which read `β` at a few named places ([semantics.md](../theory/semantics.md#remarks-on-the-definitions)) |
| The build returns `T_β(M, σ)` at the run's bound (claim [I1](../theory/claims.md#0-the-build)) | not argued as a whole; the corpus headers, and on a subset of the language the reference interpreter (`core/tests/reference/`) |
| The build reads the bound where the definition does (Lemma U) | proof sketch from reading every use of `int_max` and of slot counts in `core/build`. CI checks that the sites the reading covers are still the sites the code has (`scripts/check-int-bound-sites.py`), not the reading itself |
| The preamble is satisfiable | a consequence of F1, F2 and `I ≠ ∅`; checked at run time by a canary on `unsat` only ([the preamble](#faithfulness-of-the-encoding)) |
| Houdini keeps the largest inductive subset | false in general (Proposition 5); the module doc of `prove/mod.rs` says so; only soundness depends on the kept set |

### Notation

<!-- constrained-by ./semantics.md#notation -->

The following symbols are new here. Everything else is D1 to D25.

- `β = (n, m)` is the run's bound (D1). The prover receives `T`, the system
  `TransitionSystem::build(M, β, σ)` returns. `T` is `T_β(M, σ)` (D13, D24) when claim I1
  holds at `β`. `T_{β'}` always means the system D13 defines at `β'`, never a build output.
- `Enc` is the encoding of `T` in SMT-LIB 2 built by `core/prove/encode.rs::Enc`. It
  consists of:
  - `Decl`: `Enc::declarations`, the sorts, the pre-state functions, and the domain facts;
  - `Init`: `Enc::initial`, the initial-state facts;
  - `Step_a`: `Enc::step(a)`, for each action `a` of `T`, the step relation, with its
    parameters `p0 … p_{r-1}` and fresh constants `c0 …` as free constants;
  - `⟦φ⟧^F`: `Enc::holds(φ, F)`, a state formula closed over its binders, read in frame
    `F ∈ {Pre, Post}`; `Viol(φ, F)`: `Enc::violation(φ, F)`, the assertion that some rows
    break it.
- A *candidate* is a formula `ψ` the prover may keep (`core/prove/mod.rs::Candidate`): a rule
  of `T` (D16), a built-in lemma, a mined lemma, or a clause from a CTI.
- `reads(ψ)` is the set of variables `ψ` reads: `TransitionSystem::pred_reads` of its body plus
  the existence variable of each binder. `writes(a)` is `TransitionSystem::action_writes(a)`.
- A set `K` of candidates is *inductive relative to `Enc`* when (initiation)
  `Decl ∧ Init ∧ Viol(ψ, Pre)` is unsatisfiable for each `ψ ∈ K`, and (consecution)
  `Decl ∧ Step_a ∧ ⋀_{χ∈K} ⟦χ⟧^Pre ∧ Viol(ψ, Post)` is unsatisfiable for each action `a` and
  each `ψ ∈ K`.
- For a state `s` of some `T_{β'}`, `𝔄(s)` is the structure of
  [Faithfulness](#faithfulness-of-the-encoding) that reads `s` as an interpretation of the
  signature of `Enc`.

### The encoding

<!-- derived-from ../formal-verification.md#proofs-for-any-number-of-rows -->

#### Sorts and the term builder

`core/prove/smt.rs` builds SMT-LIB terms as data. An `Expr` carries its sort (`Sort::Bool`,
`Sort::Int`, or `Sort::Ent(e)`), each constructor checks the sorts it takes, and one printer
writes every parenthesis. A sort mismatch panics in a debug build. In a release build z3
rejects the text, `Z3::exec` turns the `(error ...)` line into a `SolverError`, and the run
fails without a verdict (`cli/src/prove.rs::run_proof` maps it to `z3 failed`). So a printer
bug that produces ill-sorted text cannot read as `proved`; it can only stop the run. A printer
bug that produces well-sorted text meaning something other than the `Expr` is not caught this
way; that is hypothesis H2 of Theorem 1.

`core/prove/solver.rs::Z3` keeps one `z3 -in -smt2` process per solver for the whole proof and
reads its answers up to an echoed marker. `Solver::check_sat` sets `:rlimit` for one check and
resets it to 0 after; `sat`, `unsat`, and `unknown`/`timeout` are the three answers (`Sat`), and
anything else is a `SolverError`.

#### The answer reader

That an answer is taken for the check that asked it is hypothesis H4 of Theorem 1.
`Z3::exec` writes a batch of commands followed by `(echo "@@rdra-ish-done@@")` (`MARK`), then
reads stdout line by line: it returns the lines before the first line equal to `MARK`,
returns a `SolverError` at the first line that starts with `(error`, and returns one when
stdout closes. `Solver::check_sat` sends `(set-option :rlimit k) (check-sat)
(set-option :rlimit 0)` as one batch and reads only the first line `exec` returns.

So H4 holds when z3 processes a batch's commands in order, prints the marker once after
them, prints nothing for `set-option`, `push`, `pop`, `assert`, `declare-*`, and `define-fun`
(`:print-success` is off by default, and the prover never sets it), and prints no line equal
to the marker. z3's stderr is discarded (`Stdio::null`).

Two facts of the code keep a misread from turning into `proved`. A first line other than the
four answers is a `SolverError`. Every caller propagates a `SolverError` with `?` up to
`cli/src/prove.rs::run_proof`, which fails the run with `z3 failed`. So a session is never
used after an error, and `exec`'s early return at an `(error` line, which leaves the rest of
the batch unread, cannot shift later answers by one batch. What the code cannot see is a
well-formed answer that belongs to another check, which needs z3 to break the protocol
above. No unit test drives `exec` or `check_sat` (the tests of `solver.rs` parse `get-value`
output only); every proof test exercises them against z3 5.1.0.

#### How rows of unbounded count are represented

Each entity `E` becomes an uninterpreted sort `E<i>` (`Command::DeclareSort`). The sort's
elements are the rows; nothing bounds how many there are, and a model of `Enc` may have any
finite or infinite number of them. Two named elements exist in every sort:

- `nil<i>`, the value of an unlinked optional key; it is never a row (`Decl` asserts its
  existence is `absent`, so no quantifier over present rows reaches it);
- `zero<i>`, the slot a non-optional key holds at its base value (slot 0 in `T`), distinct
  from `nil<i>`.

Every row variable `v` of `E` (D2) becomes a unary function `v<k> : E<i> → Int` holding the
domain index (`Value`) of `v`, or `E<i> → E<j>` when `v` is a key into `E<j>`
(`Enc::func`, `Enc::var_sort`). The successor reads `v<k>_n`. `now` is an `Int` constant, `now`
and `now_n`. Marks (`Run`, `Raised`) and driven propositions are row functions like columns.

#### Domain facts (`Decl`)

`Enc::declarations` asserts, for every row `x`:

- a finite-domain variable lies in `0 … size-1`, with `size = Domain::size` (Enum, Bool,
  nullable variants, existence, marks; none of these sizes depends on `β`);
- an Int-typed variable (`Domain::is_numeric`: Int, Money, Decimal, Date, DateTime, nullable
  or not) is `≥ 0`, with no top (`Enc::in_domain`);
- a non-optional key is never `nil`;
- `now ≥ 0`; `nil<i>` is absent.

The facts are asserted for the pre-state only. The post-state functions are defined
(`define-fun`) from pre-state terms, literals, and fresh constants that are themselves
constrained to the domain, so they stay in the domain by construction.

#### Initial states (`Init`)

`Enc::initial` asserts, for every variable and every row, the start value of D4: the base
value (D3) for a created entity; for a pre-existing entity, `present` for existence and, per
`Start`, the base value, any value (`Start::Any`), or a key that is not `nil` together with
injectivity of the key function over non-`nil` rows (`Start::Identity`). The `nil` row is
pinned to the base value of every variable in every case (`ite(x = nil, base, start)`), which
Definition 1's reading of `⋆` satisfies. `now = 0`. A base value the encoding cannot state (a
key at a fixed slot other than 0) makes `Init` `Unsupported`; then every candidate falls as
`Unknown` and nothing is proved.

#### Steps (`Step_a`)

`Enc::step` builds one step relation per action, in the order of `explore/moves.rs::
TransitionSystem::each_successor`:

1. **Parameters.** Each parameter is a fresh constant other than `nil`. `New` asserts that the
   row is absent: *any* absent row, where `T` takes the lowest absent slot (D7). `Existing`,
   `Parent`, and `Triggered` assert `present`; `Earlier` asserts nothing (its run-mark guard
   selects it, as in `T`).
2. **Effects, in effect order.** `Create` sets existence to `present`; `Assign` of a literal
   or of a parameter's row sets the variable on that row; a `Transition` whose current value
   has exactly one target sets it at once, conditioned on the current value. These are the
   same three in-order updates `each_successor` makes.
3. **Deletes**, after every other effect: the deleted row's columns return to base and its
   existence becomes `deleted`; keys and marks are kept (D8, D10), as in `each_successor`.
4. **Forks**, after deletes: a `Transition` with several targets for some current value and
   every `Choose` become a fresh constant constrained to the listed values (`Enc::one_of`;
   for a choice of every number, `c ≥ from` or one of the listed values below `from`; see
   [widening](#what-is-folded-what-is-widened-and-what-is-unsupported)), applied last, in
   effect order, and skipped on a deleted row, as `each_successor` drops choices on deleted
   rows.
5. **Guards.** Every guard of `a` is asserted on the pre-state, with `At::Next` reading the
   successor, except `GuardWhy::Clock` (the `now < m` guard of `Tick`), which is dropped.
   Quantifiers and `now` inside a guard read the pre-state in both `Enc` and `expr.rs::eval`
   (`eval` reads `env.next` only for an `At::Next` slot).

`Tick` sets `now_n = now + 1`. Every variable no effect writes keeps its pre-state function
(`cur` returns `v<k>(x)`), so the post-state differs from the pre-state only on variables in
`writes(a)`; this is Lemma F5.

#### Predicates

`Enc::pred` follows `Pred` node by node. `Forall`/`Exists` over `E` quantify over rows with
existence `present`, as `expr.rs::quantify` ranges over present slots. `Count{E, φ, op, k}` is
built from `at_least(k)`: `k` pairwise distinct witnesses, each present and satisfying `φ`. That
is exact, in either polarity, so `≥ k`, `> k`, `≤ k`, `< k`, `== k`, `!= k` are exact as long as
every `at_least(j)` they need has `j ≤ COUNT_WITNESSES = 8`; past that the formula is
`Unsupported`. So `≥ 8` and `< 8` are stated, `> 8`, `≤ 8`, `== 8`, and `!= 8` are not
(`.at_most(7, ..)` is the largest `at_most`, `.exactly(7, ..)` the largest `exactly`).

`Enc::compare` mirrors `expr.rs::compare` (D5) case by case:

| Case | `expr.rs::compare` | `Enc::compare` |
|---|---|---|
| key against a row (`Term::Index`) | linked iff the key names the slot; only `==`, `!=` | the key function equals the row; only `==`, `!=`; a non-key, or a key into another entity, against a row is `Unsupported` |
| either side numeric | `Domain::number` (nullable Int: index 0 is null, `i` is `i-1`); null `==` only null, never ordered | the pair (is null, number), same truth table |
| keys and literals | raw indices: an optional key's 0 is null, a non-optional key's 0 is slot 0 | `nil` and `zero`; any other fixed slot, a key against a key of another shape, and an ordering of rows are `Unsupported` |
| other domain indices | raw index order | raw index order |

On every case both implement, the two agree by inspection. `Enc::compare` is a second
implementation of `expr.rs::compare`, tied to it only by that inspection and by the
differential tests. The cases `Enc::compare` rejects only make a formula `Unsupported`, which
the next section turns into a widening or an `out of scope`.

#### What is folded, what is widened, and what is unsupported

Three different mechanisms change what a predicate or step says, and they have different
consequences.

**Folding, in the builder.** `core/build/preds/atoms.rs::lists_whole_domain` replaces an `in`
set that lists every value of an Enum, Bool (or their nullable forms), nullable-or-present,
or existence variable by `true` (and `not in` by `false`). These domains do not depend on
`β`, so the fold is exact in `T_{β'}` for every `β'` and in `Enc`. An Int, nullable Int, or
key domain is never folded, because its values are as many as `β` says. The corpus cases
`z3_int_listing_whole_range`, `z3_int_listing_guard`, and `z3_int_listing_past_the_top` pin
this under `proof_equivalence`'s wider-Int axis: a rule, or a guard, that lists every Int value
up to the top leaves its rule true at `0..5` and `unproved` by z3, and the listing rule fails at
`0..7`.

**Widening, in the encoder.** Three constructs are encoded with more behaviors than `T_β` has:

- a `New` parameter takes any absent row;
- `Tick` has no guard, and Int-typed variables and `now` have no top;
- a `Choose` of every number of an Int-typed variable is encoded as `c ≥ from` (any number)
  or one of the listed values below `from`.

The builder records a choice of every number in a form that does not depend on `β`:
`Effect::Choose { numbers: Some(Numbers { from, .. }) }`, beside `values`, its expansion at
`β`, which the explorer, TLC and the generated code read. `Enc::one_of` reads `numbers` and,
of the list, only the values below `from`, which are the same at every bound. It encodes the
choice as `c ≥ from ∨ ⋁ { c = t : t listed, value < from }`; a `Choose` without `numbers`
becomes `⋁ { c = t : t listed }`. The Kani harness
`encode.rs::proofs::one_of_admits_what_the_step_forks_on` proves that this term holds of a
value `c` exactly when `each_successor` forks on `c` (within the domain) or `c ≥ from` (past
its top), for every non-key choice of the shape the builder makes, up to a domain of 4
values, `from` up to 3 and any non-empty set of listed values. It evaluates the term with an evaluator
of that fragment of `smt.rs`, so it does not cover z3's reading of the printed text (H2).

The builder fills `numbers` for `sets(.., col == present)` on a nullable Int-typed column
(`from = 1`, the first non-null index; guarded by `.when` or not; `build/sets.rs::sets_effects`,
`guarded_set`) and for a copy (`from = 0`, `copy_effects`). On a non-nullable Int column,
`(Domain::Int, Present)` is `EffectRhs::None`, no write, so only nullable Int columns have a
`present` choice. A value below `from` is listed
when a `.when`-guarded `sets(.., col == null)` joins the `Choose` of a `.when`-guarded
`present` write of the same nullable Int (`guarded_set`): null, index 0, beside `from = 1`.
The test `prove_cli::a_guarded_null_beside_a_guarded_present_is_a_step_of_the_proof` and the
corpus case `z3_guarded_null_beside_present` pin that the encoding has the step that writes
that null.

A `Choose` without `numbers` holds only values that are the same at every bound: a
`.when`-guarded literal `sets` (which `guarded_set` adds to a `Choose` whose `numbers` stays
`None` unless a `present` write of the same slot fills it), and the choices of
`build/transitions.rs::chosen_columns` and `creation_choices`, which range over a variation
column only (`ChoosesNotVariation`; the build gives a variation column the domain
`Domain::Enum`, never a numeric one).

When the `present` write carries `.when(C)`, `build/transitions.rs::arm_guards` states the
choice of numbers as one arm, `next.n != null ∧ C` (`next.n != v` for `v = from - 1`), which
names no value of `β`, and `Enc::step` asserts that guard as built. `arm_guards` leaves out of
the guard every listed value the numbers cover with the same conditions (`in_numbers`). A
listed value with conditions of its own keeps its arm; it is a literal the model writes, the
same `Value` at every bound. The test
`prove_cli::a_guarded_present_write_reaches_past_the_int_range` and the corpus case
`z3_guarded_present_past_the_top` pin that the encoded step writes numbers past the run's top.

**`Unsupported`.** A construct the encoder cannot state is handled by where it occurs:

- in a guard: the guard is dropped (`loose`, printed as `note: not encoded …`); dropping a
  conjunct of a step's guard only adds steps;
- in a write: the written value is a fresh constant constrained only to the domain
  (`Enc::havoc`); this includes the old and the true new value, so it only adds steps;
- in a candidate: a rule is `out of scope` and is not tried; a lemma is silently dropped.

No rule is ever approximated: `Unsupported` in a rule's body or binders yields
`Verdict::OutOfScope`, never a weaker formula (B7).

#### Uniformity in the bound

<!-- constrained-by ./semantics.md#remarks-on-the-definitions -->

The encoder reads only `T`, the build's output at `β`. Theorem 1 is about the systems
`T_{β'}(M, σ)` that D13 defines at other bounds `β'`. So it needs two things. First, `T` is
`T_β(M, σ)`: this is claim I1 at `β`. Second, `T_{β'}` differs from `T_β` only in ways the
encoding already covers.

**Lemma U₀ (uniformity of the definition).** Let `β' = (n', m')` satisfy D1, with every Int
literal of the model at most `m'`. Then `T_{β'}(M, σ)` has the same entities, variables, and
actions as `T_β(M, σ)`, with the same parameters, guards, and effects, and its rules have the
same binders and bodies. The exceptions are the places where D1 to D12 read the bound:

- the domains of D2: number columns have `[m'+1]` (or `[m'+1]⊥`), keys `[n'_P]` (plus `⊥`);
- D4's Identity starts and D7's slots, through `n'`;
- D8's `present` written to a nullable number column, a choice over `[m'+1]`, together with
  the `.when` bodies that filter that choice;
- D12's `Tick` guard, `now < m'`.

*Proof.* Read D1 to D12. ∎

Lemma U₀ is about the definition. Lemma U below is about the build: it says that the build
reads the bound only where the definition does. That makes it evidence for the uniformity
half of claim I1. Each of its items is a case of Lemma U₀, stated in the build's vocabulary.

**Lemma U (uniformity of the build).** Let `β' = (n', m')` be such that
`TransitionSystem::build(M, β', σ)` reports no error, and let every Int literal the model
states be at most `m'`. Then the system that call returns has the same entities, variables,
and actions as `T`, with the same parameters, guards, and effects, and its rules have the
same binders and bodies, except that:

1. each `Domain::Int`/`NullableInt` has `m'+1` (or `m'+2`) values, and each key domain has
   `n'_P` (`+1`) values;
2. each `Choose` the builder fills with every value of an Int-typed domain lists the values at
   `m'` instead of `m`;
3. the `Tick` guard compares `now` with `m'`.

A rule's premises and text can also differ: `build/preds/entity_rules.rs::push_unique` drops its
premise at one slot, and `build/preds/cross_rules.rs::count_premise` puts the slot count in its
text. The prover reads only a rule's binders and body, so these do not matter here.

*Proof sketch.* `int_max` is read while building `T` at five places: the `Tick` guard
(`build/vars.rs::tick`); the literal range checks (`build/preds/atoms.rs::eq_atom`,
`::int_term`, `build/sets.rs::out_of_bounds`, all raising `StateDiag::IntLiteralOutOfBounds`,
which is `Severity::Error`); `Domain::of_number`; `Domain::size` (in `lists_whole_domain`,
which refuses Int domains, and in the `Choose` lists of `build/sets.rs` and
`build/transitions.rs`); and the width check, `Domain::column_values` and `Domain::int_values`
called from `build/vars.rs::Builder::column_var`, which only raises `DomainTooLarge` and
changes no part of `T`.

`Domain::of_number` gives a literal `k` the value `Value(k)` (or `Value(k+1)` on a nullable
column) for `k ≤ min(m, m')`, and returns `None` past a byte; a domain past 256 values is
`DomainTooLarge`, a build error, so no value wraps. (`Domain::of_number` and `Domain::size`
are in `core/types.rs`; the other three places are in `core/build`.) Its callers are the writes of
`build/sets.rs`, the `@default` values of `build/vars.rs`, and
`atoms.rs::lists_whole_domain`, which reaches it only for finite domains, where it returns
`None`. A rule's literal is not a `Value`: `atoms.rs::eq_atom` builds `n == k` as
`Term::Int(k)`, compared as a number. `Domain::of_variant` narrows the same way, and a
nullable Enum past a byte is `DomainTooLarge` too (corpus case `width_nullable_enum_256`); that
case does not depend on `β`.

A `Choose` list is read a second time: `arm_guards` turns each listed value of a `Choose` with
conditions into a guard disjunct, except the values `numbers` covers with the same
conditions, which it states as the one arm `next.n != null` ([widening](#what-is-folded-what-is-widened-and-what-is-unsupported)).
Of the five places in `core/build` that push a `Choose`, only `build/sets.rs::guarded_set`
attaches conditions to a list that `Domain::size` fills, and that `Choose` carries `numbers`.
So the guard reads the list at `β` only for literal values, and it is the same at `β` and
`β'`.

Slot counts are read in `build/vars.rs` (the slots themselves and the `IdentityStartSlots`
error), and in `build/preds/entity_rules.rs` and `build/preds/cross_rules.rs` for vacuity
premises and the `CountAboveBound` diagnostic, neither of which enters a rule's body or an
action. The variables `T` holds (D2) are chosen by what is read or written, not by `β`. So,
with no error at `β'`, the system is the same up to items 1 to 3. ∎

**The sites the proof sketch reads.** The list below names every item of the crates that
mentions `int_max`, every caller of `Domain::of_number`, and every call of `Domain::size` in
`core/build`, as `token path::item`. CI's `check` job runs
`scripts/check-int-bound-sites.py`, which finds these sites in the code (outside comments,
literals, and test code) and fails, naming the site, when one appears that the list does not
hold or a listed one disappears; the script's header gives the exact convention. A failure
means the proof sketch above must be read again against the new site, and the list updated.
The check holds the list, not the lemma: it does not see a read through a helper with another
name, a second read inside a listed item, a derived read such as `arm_guards` reading a
`Choose` list, or slot counts. Lemma U stays a proof sketch.

```int-bound-sites
# Read while building T, as the proof sketch says.
int_max   crates/rdra-ish-core/src/lifecycle/build/vars.rs::Builder::tick
# The width check: a domain past 256 values is DomainTooLarge and gets no var.
int_max   crates/rdra-ish-core/src/lifecycle/types.rs::Domain::column_values
int_max   crates/rdra-ish-core/src/lifecycle/types.rs::Domain::int_values
int_max   crates/rdra-ish-core/src/lifecycle/build/preds/atoms.rs::Builder::eq_atom
int_max   crates/rdra-ish-core/src/lifecycle/build/preds/atoms.rs::Builder::int_term
int_max   crates/rdra-ish-core/src/lifecycle/build/sets.rs::Builder::out_of_bounds
int_max   crates/rdra-ish-core/src/lifecycle/types.rs::Domain::of_number
int_max   crates/rdra-ish-core/src/lifecycle/types.rs::Domain::size
of_number crates/rdra-ish-core/src/lifecycle/types.rs::Domain::of_number
# Callers of Domain::of_number: the writes, the @default values, and lists_whole_domain.
of_number crates/rdra-ish-core/src/lifecycle/build/sets.rs::Builder::effect_value
of_number crates/rdra-ish-core/src/lifecycle/build/vars.rs::Builder::column_var
of_number crates/rdra-ish-core/src/lifecycle/build/preds/atoms.rs::Builder::lists_whole_domain
# Calls of Domain::size in core/build: lists_whole_domain and the Choose lists.
size      crates/rdra-ish-core/src/lifecycle/build/preds/atoms.rs::Builder::lists_whole_domain
size      crates/rdra-ish-core/src/lifecycle/build/sets.rs::Builder::sets_effects
size      crates/rdra-ish-core/src/lifecycle/build/sets.rs::Builder::copy_effects
size      crates/rdra-ish-core/src/lifecycle/build/transitions.rs::Builder::chosen_columns
size      crates/rdra-ish-core/src/lifecycle/build/transitions.rs::Builder::creation_choices
# The bound itself: its field, constructors, and where the CLI sets it.
int_max   crates/rdra-ish-core/src/lifecycle/types.rs::Bounds
int_max   crates/rdra-ish-core/src/lifecycle/types.rs::Bounds::uniform
int_max   crates/rdra-ish-core/src/lifecycle/types.rs::Bounds::at
int_max   crates/rdra-ish-cli/src/export.rs::checked_bounds
int_max   crates/rdra-ish-cli/src/verify/escalate.rs::grown
# Outside TransitionSystem::build: they read the system the build returned, or print the bound.
# The claim a proof prints: the range of bounds at which the model builds, found by building.
int_max   crates/rdra-ish-cli/src/prove.rs::int_range
int_max   crates/rdra-ish-core/src/lifecycle/reduce/split.rs::TransitionSystem::monotone_vars
int_max   crates/rdra-ish-core/src/lifecycle/explore/bfs.rs::Bfs::growth
int_max   crates/rdra-ish-core/src/lifecycle/states.rs::Within::fmt
int_max   crates/rdra-ish-core/src/lifecycle/states.rs::BoundsLine::fmt
int_max   crates/rdra-ish-core/src/lifecycle/view.rs::axis
int_max   crates/rdra-ish-core/src/lint/conditions.rs::lint_int_comparisons
int_max   crates/rdra-ish-emit/src/tla/module.rs::render
int_max   crates/rdra-ish-emit/src/codegen/conformance/mod.rs::Traces
int_max   crates/rdra-ish-emit/src/codegen/conformance/mod.rs::traces
int_max   crates/rdra-ish-emit/src/codegen/conformance/coverage.rs::Traces::coverage
```

**Which items `Enc` covers.** (1) `Enc` states Int-typed domains as `≥ 0` and key domains as
sorts of any size. (2) A choice of every number is encoded from `numbers` as `c ≥ from`, with
the listed values below `from` beside it, which contains the full list at every `m'` (the
values below `from` are the same at every bound); a choice without `numbers` holds values
that are the same at `m` and `m'` (literals the model writes, and variation values). (3)
`Enc` drops the `Tick` guard. A literal the model writes is the same `Value` at `m` and `m'`,
as the proof sketch says.

Corollary 6 uses only `β' = β`, which needs no lemma. Changing only the instance counts
(`m' = m`, any `n'` at which the build has no error) touches no `Choose` list and no Int
domain.

The code never checks Lemma U for the bounds a proof covers. For escalation only,
`cli/src/verify/escalate.rs::same_system` compares a rebuild's entity ids, its number of
variables, and its rule names and sites; it does not compare the variables themselves, the
guards, or the effects.

### Faithfulness of the encoding

<!-- constrained-by #the-encoding -->

**Definition 1 (the structure of a state).** For a state `s` of `T_{β'}`, `𝔄(s)` interprets
`E<i>` as the slots `[n'_E]` plus one extra element `⋆_E`; `nil<i>` as `⋆_E`; `zero<i>` as
slot 0; `v<k>(i)` as the index `s(v)[i]` for a slot `i`, and as the base value of `v` on `⋆_E`;
a key's value as the parent slot it names, or `⋆_P` for null; `now` as `s(now)`. For a step
`s →_{a,b} s'`, `𝔄(s, b, s')` extends `𝔄(s)` with `p_j ↦ b_j`, the post-state functions read
from `s'`, and each fresh constant set to the value the step chose (any domain value when its
condition is false).

**Proposition 2 (B1, over-approximation).** Assume claim I1 at `β`, so that `Enc` encodes
`T_β(M, σ)`. Then for every `β' ∈ Sc(M, β)` (Definition 3, the hypotheses of Lemma U):

- (F1) `𝔄(s) ⊨ Decl` for every state `s` of `T_{β'}`;
- (F2) `𝔄(s) ⊨ Init` for every `s ∈ I_{β'}`;
- (F3) if `s →_{a,b} s'` in `T_{β'}`, then `𝔄(s, b, s') ⊨ Step_a`;
- (F4) for every predicate `φ` that encodes and every binding, `𝔄` satisfies `φ`'s encoding iff
  `expr.rs::eval` returns true on `s` (in frame `Pre`) or on `s'` (in frame `Post`).

*Proof sketch.* (F1) Each fact of `Decl` holds of `𝔄(s)`: indices lie in their (β-independent)
finite domains, Int-typed values are `≥ 0`, a non-optional key names a slot, `⋆` is absent.

(F2) D4 clause by clause: created entities at base; pre-existing slots present; `Start::Any`
any value; `Start::Identity` maps slot `i` to parent slot `i`, which is injective and not `⋆`.

(F3) Under I1 at `β` and Lemma U₀, the action of `T_{β'}` has the guards and effects `Enc`
encoded, up to items 1 to 3 of Lemma U, which restate Lemma U₀ and are covered. The parameter
constraints of `Enc` are implied by those of `T` (a lowest absent slot is absent). The
post-state functions equal `s'` because `Enc::step` and `each_successor` apply the same
updates in the same order ([Steps](#steps-step_a)). A choice's value is one `Enc` allows:
`each_successor` forks over the `Choose`'s `values`, which `one_of` contains by item 2 of
Lemma U, listed values below `from` included. A havocked write is satisfied by the value `T`
wrote. Guards hold by (F4).

(F4) By induction on `φ`, using the agreement table of [Predicates](#predicates) for `Cmp`,
and the facts that `Forall`, `Exists`, and `Count` range over present rows on both sides and
`⋆` is never present. ∎

What this rests on, beyond the sketch:

- **Reading, not mechanical checking.** One Kani harness covers `core/prove/`: `Enc::one_of`
  against the native step's choice, above. No shadow twin covers it. `Enc::compare` and `expr.rs::compare` are two implementations tied by inspection and by the
  differential tests below.
- **Differential tests.** `cli/tests/proof_equivalence.rs::no_proved_rule_has_a_bounded_counterexample`
  proves every sample and checks that BFS finds no violation of a proved rule at 1, 2, and 3
  instances per entity and at `int_max + 2`; `::no_proved_rule_has_a_bounded_counterexample_in_the_corpus`
  does the same on every engine-corpus case (both release jobs' tests, run in
  `engine-equivalence`). These test (F1) to (F4) only through their consequence, and only at
  those bounds. The corpus cases `z3_int_and_now_boundary` and `z3_now_past_the_top` pin the
  clock widening from both sides; `z3_guarded_present_past_the_top` and
  `z3_guarded_null_beside_present` pin the two choice constructs. Two limits weaken this
  evidence: a cone whose BFS does not finish under the test's state cap is skipped, and
  without z3 the whole test is skipped unless `RDRA_REQUIRE_Z3=1`; claim
  [B1](../theory/claims.md#b-theorem-proving-with-z3) gives the skip count the test pins.
- **The reference interpreter.** It checks the same consequence without going through
  `TransitionSystem::build`: `core/tests/reference/proofs.rs` checks the rules z3 proves at
  `m` against the reference at `m + 1` and `m + 3`, on two fixed models
  (`a_rule_proved_at_one_int_range_holds_at_wider_ones`, and
  `a_guarded_null_beside_a_guarded_present_is_no_proof` for the null listed below `from`) and
  on 200 random models of its subset on main
  (`rules_proved_on_random_models_hold_at_wider_int_ranges`).
- **The preamble.** (F1), (F2), and `I_{β} ≠ ∅` imply that `Decl ∧ Init` is satisfiable. If
  an encoder bug made it unsatisfiable, every initiation check would be `unsat` and every
  consecution check of a set holding initially could be too, so every rule would be `proved`.
  Every proof checks it first with a canary candidate `false`, which must fall at initiation
  (`premise`), and proves no rule when it survives: each rule is then `Unproved::Premise`
  (`core/tests/prove.rs::a_contradictory_premise_proves_no_rule`). The canary survives only on
  `unsat`; a premise z3 answers `unknown` on is not caught. A spot check with
  `examples/prove_transcript.rs` on `order.rdra`, `check_met.rdra`, `one_to_one_shared.rdra`
  (formal-verification samples) and `patent-ops` finds `Decl ∧ Init` `sat` under z3 5.1.0 with
  MBQI on ([Reproductions](#reproductions)); that is a spot check, not a guarantee.

### Induction and Houdini

<!-- derived-from ../formal-verification.md#proofs-for-any-number-of-rows -->

#### Relative induction

A state rule `R` is rarely inductive alone: a step can break it from a state where it holds but
which is not reachable. The standard remedy is to prove a set `K ∋ R` inductive together
(Definition in [Notation](#notation)); each member then holds in every reachable state, and the
other members act as lemmas. `prove` searches for such a set among the candidates with Houdini
(Flanagan and Leino, 2001): start with every candidate, drop each one some initiation or
consecution check breaks, and repeat until a pass over every check drops nothing. The search is
1-induction (Sheeran, Singh, and Stålmarck, 2000, with `k = 1`) over a set that only shrinks.

#### The rounds

`core/prove/mod.rs::prove_round` runs one Houdini search:

1. **Candidates.** Every selected rule not in `refuted`, the built-in lemmas (`lemmas`), the
   mined lemmas with their wait stage (`mine::Mined::wait`: 0, 1 for the fine ones and the
   counts, 2 for ∀∃), and the CTI clauses of earlier rounds. A formula that does not encode is
   left out (a rule becomes `out of scope`).
2. **Activation literals.** Each candidate `k` is declared once per solver as
   `act_k : Bool` with `act_k ⇒ ⟦ψ_k⟧^Pre`. A consecution check asserts `act_k` for the members
   of the current set. The literals of a wait stage are declared only when the stage joins,
   so a proof that never needs a stage sends z3 nothing of it.
3. **Initiation**, once per candidate, before the stage it belongs to joins: `Init ∧
   Viol(ψ, Pre)`; `sat` is `Fall::Initial`, `unknown` is `Fall::GaveUp(None)`. When `Init`
   does not encode, every candidate of the stage is `Fall::Unknown`.
4. **Consecution**, by passes over the actions. For action `a`, the hit set is every member `k`
   with `reads(ψ_k) ∩ writes(a) ≠ ∅` (`breaks`). Under one scope (`Step_a` and the members'
   literals) each hit candidate is checked: `sat` is `Fall::Step(a)`, `unknown` is
   `Fall::GaveUp(Some(a))`. A candidate that falls stays asserted for the rest of that action's
   checks, so the answers do not depend on the order of the checks. Passes repeat until one
   removes nothing.
5. **Waiting stages.** When a rule fell after initiation, the next stage with a member that
   holds initially joins; the rules that fell after initiation return to the set; step 4 runs
   again. Lemmas that fell do not return.
6. **CTIs**, for each rule that fell: on solver 0, after `(reset)`, with MBQI on, the kept set
   asserted directly (`assume`), and the rule's pre-state formula.

`prove_offered` then runs up to `STRENGTHEN_ROUNDS = 2` more complete rounds when the CTI stage
produced new clauses ([CTI clauses](#cti-clauses)); the result is that of the last round.

#### MBQI toggling

Steps 3 to 5 run with `smt.mbqi false`. Without model-based quantifier instantiation (Ge and
de Moura, 2009), z3 still proves `unsat` by E-matching. But in practice it answers `unknown`
on almost every query it would find a model for, because `Decl` contains quantifiers; the
candidate then falls as `GaveUp`, exactly as with `sat`. MBQI is a completeness device:
turning it off cannot turn a satisfiable query into `unsat`, so it changes no `proved`. The
measured effect on every sample on one solver was 150 s to 68 s of proof with the same
`Proof` (code comment in `prove_round`). The CTI stage needs a model, so it turns MBQI back on.

#### Step pruning

**Lemma F5 (B6).** If `reads(ψ) ∩ writes(a) = ∅`, then `Decl ∧ Step_a ⊨ ⟦ψ⟧^Pre ⇔ ⟦ψ⟧^Post`.

*Proof.* `Enc::step` gives a variable a new post-state function only through `set`, called for
`Create` (existence), `Assign`, single-target `Transition`, forks of `Transition` and
`Choose` (the variable written), and `Delete` (existence and the entity's columns); `now_n`
differs from `now` only under `Tick`. `core/cone.rs::effect_writes` lists, for the same
effects, existence; the variable; existence, columns, and keys; and `now`. So every variable
whose post-state function differs from its pre-state function is in `writes(a)`. `⟦ψ⟧^F` reads
the functions of `reads(ψ)` and of the existence of its binders and quantified entities (all in
`reads(ψ)`: the quantified entities' existence by `pred_reads`, the binders' by `prove_round`,
which adds them to each candidate's reads), `now` if `ψ` reads it, and row constants. All of
these agree in the two frames. ∎

So a member that `a` does not hit cannot be broken by `a`, and skipping its check loses nothing.

#### The kept set is inductive

**Proposition 4 (B5).** Assume z3's `unsat` answers are correct and each is taken for the
check that asked it (H1, H4). The set `K` a round keeps (the candidates with no fall and a
joined stage, `Set::contains`) is inductive relative to `Enc`.

*Proof.* Every member passed initiation: wait-0 candidates before the first pass, later stages
when they joined, and the rules that returned had passed it before they fell. The loop of step 4
exits only after a full pass over every action in which no member fell, and step 5 either adds
nothing (the proof ends) or is followed by another such loop. In that last pass the set is `K`
throughout, each action's scope asserts exactly the literals of `K`, and every hit member got
`unsat`. A non-hit member is preserved by Lemma F5. A model of `Decl ∧ Step_a ∧
⋀_{χ∈K}⟦χ⟧^Pre ∧ Viol(ψ, Post)` expands to a model of the checked query by setting `act_χ` true
for `χ ∈ K` and false otherwise, so that query being `unsat` makes the consecution condition
hold. ∎

`unknown` never helps a candidate: it is a fall (B12). A `SolverError` aborts the whole proof.

#### Maximality

The prover keeps an inductive set, which need not be the largest one. The module doc of
`prove/mod.rs` and formal-verification.md both say so.

**Proposition 5.** If every z3 answer were exact (no `unknown`), the first stage computes the
largest inductive subset of the wait-0 candidates (classical Houdini: a candidate is removed only
when it is in no inductive subset of the current set). The final `K` need not be the largest
inductive subset of all candidates offered, for four reasons:

1. `unknown` counts as a fall, and with MBQI off in practice almost every satisfiable check
   returns `unknown`, because `Decl` contains quantifiers, so a candidate can fall on a query
   z3 gave up on although it was unsatisfiable. How often that happens is not known: the
   `unknown` counts in formal-verification.md, "Budgets", do not separate satisfiable queries
   from undecided ones, so they are no evidence either way.
2. A lemma that fell before a stage joined never returns, though the joined lemmas might have
   made it inductive; only rules return (step 5).
3. A stage joins only when a rule fell after initiation; a stage that would let more lemmas
   survive does not join when every rule is already proved.
4. The CTI rounds restart Houdini with more candidates, and the result is the last round's.

None of these touches soundness, which rests on Proposition 4 alone. They affect which rules are
proved and which `lemma:` lines print.

### The soundness theorem

<!-- constrained-by #faithfulness-of-the-encoding -->
<!-- constrained-by #the-kept-set-is-inductive -->

**Definition 3 (scope of a proof).** For a run at `β = (n, m)` under scope `σ`, `Sc(M, β)` is
the set of bounds `β' = (n', m')` such that `TransitionSystem::build(M, β', σ)` reports no
error and every Int literal of the model is at most `m'`. (The second condition is implied
by the first, since a literal past the range is `IntLiteralOutOfBounds`, an error.)

By D1, `1 ≤ n'_E ≤ 255` and `1 ≤ m' ≤ 255` (`cli/src/export.rs::checked_bounds` rejects 0).
A model with a nullable Int column builds at no `m' = 255` (`DomainTooLarge`), so for it
`m' ≤ 254`. `β ∈ Sc(M, β)` whenever the run builds without error. The build errors that
depend on `m'` each hold on one side of the run's `m`: `IntLiteralOutOfBounds` below the
largest literal; `DomainTooLarge` above 254 with a nullable Int; and `StateTooWide` above the
`m'` at which a packed state passes its width (`build/mod.rs::too_wide`; the bits grow with
`m'` and with `n'`).

So at the run's slot counts the `m'` of `Sc(M, β)` form one range `L … M`, and
`cli/src/prove.rs::Claim::of` prints it, found by building: `for any number of rows and every
--int-range-max from L to M, in the whole model`, or `in the selected scope: <flags>`, on the
header, every `proved:` and `lemma:` line, the `OK:` clause, and the `--deny-unproved` error.
`Verdict::Proved`'s doc, formal-verification.md, and the `--prove` help text
(`cli/src/cli.rs`) cite that statement. `int_range` guesses `L` from the literals a build at 0 reports past the range and `M`
as 255 or 254, confirms each guess by a build on each side, and bisects only when a guess
fails. A model whose system has no Int-valued variable prints no range. The claim also says
"for any number of rows", every `n'` at which the build reports no error. The range is found
at the run's slot counts only; at a larger `n'` `StateTooWide` can hold at an `m'` inside it.
Such a pair `(n', m')` is no bound of `Sc(M, β)`: no system `T_{β'}` exists there (D1), so the
printed claim covers it vacuously.

**Theorem 1 (B2).** Assume

- (H1) z3 answers `unsat` only for unsatisfiable queries;
- (H2) the text `smt.rs` prints means the `Expr` it was built from;
- (H3) Proposition 2 holds for every `β' ∈ Sc(M, β)`;
- (H4) the prover takes each z3 answer for the check that asked it
  ([the answer reader](#the-answer-reader): `Z3::exec` reads a batch's output up to its
  echoed marker, and `Solver::check_sat` reads the first line of it as the answer);
- (H5) claim I1 holds at `β`: the build returns `T_β(M, σ)`, so `Enc` encodes it
  (Proposition 2 assumes H5).

If `verify --prove` prints `proved: R` (`Verdict::Proved`), then for every
`β' ∈ Sc(M, β)`, `T_{β'}(M, σ) ⊨ R` in the sense of D16.

The theorem is conditional on H3, and H3 is a proof sketch, not a checked fact.

*Proof.* `Verdict::Proved` is given to a rule exactly when it has no fall in the round whose
`Proof` is returned, that is, when it belongs to the kept set `K` of that round, or to a
certificate the reuse round accepted ([The certificate cache](#the-certificate-cache)); in both
cases `K` is inductive relative to `Enc` by Proposition 4 or Proposition 7. Fix `β' ∈ Sc(M, β)`.
By induction on the length of a finite trace `s_0 → … → s_j` of `T_{β'}` we show
`𝔄(s_j) ⊨ ⋀_{χ∈K} ⟦χ⟧^Pre`:

- *Base.* `s_0 ∈ I_{β'}`, so `𝔄(s_0) ⊨ Decl ∧ Init` by F1 and F2. For `χ ∈ K`,
  `Decl ∧ Init ∧ Viol(χ, Pre)` is unsatisfiable (initiation, with H1, H2, and H4), so
  `𝔄(s_0) ⊨ ⟦χ⟧^Pre`.
- *Step.* `s_j →_{a,b} s_{j+1}`. By F1 and F3, `𝔄(s_j, b, s_{j+1}) ⊨ Decl ∧ Step_a`, and by the
  hypothesis it satisfies `⋀_K ⟦χ⟧^Pre`. By consecution (with H1, H2, and H4) and Lemma F5 it
  satisfies `⟦χ⟧^Post` for every `χ ∈ K`. The post-state functions of that structure are those
  of `𝔄(s_{j+1})`, so `𝔄(s_{j+1}) ⊨ ⋀_K ⟦χ⟧^Pre`.

`R ∈ K` with `all_rows = false`, so `⟦R⟧^Pre` says that every tuple of present rows satisfies
the body; by F4 that is D16 on `s_j`. ∎

**Remarks on the hypotheses.**

- **Scope `σ`.** The theorem is about `T(M, σ)` (D24). Under `--buc`, `--without-usecase`,
  `--without-buc`, or `--actor`, the encoded system lacks the left-out actions, and `proved`
  says nothing about the whole model. The `scope:` header line names `σ`, and the `proof (...)`
  header, every `proved:` and `lemma:` line, the counts, and the `OK:` line say `in the
  selected scope` (`prove_cli::a_proof_under_a_scope_says_so_on_every_proved_line`).
- **What the printed claim means.** Inside `Enc`, Int-typed variables range over all
  non-negative integers. No `T_{β'}` with a top above 255 exists (D1), so the printed range
  stops at the greatest `m'` the model builds at. Negative values are outside both.
- **Instance counts.** `Enc` has no count, so the theorem holds for every `n'` at which the
  build has no error. Counts above 255 are not systems `T` defines.
- **Satisfiability of premises.** If `Decl ∧ Init` were unsatisfiable, the conclusion would
  still follow from the hypotheses, because H3 includes F1 and F2. The risk is that H3 is
  false, and that would show through the differential tests and, for the premise itself,
  through the canary ([the preamble](#faithfulness-of-the-encoding)).

**Corollary 6 (a cone decided by proof, B16 second part).** `BfsCone::decided_by_proof`
(`cli/src/verify/bfs.rs`) turns a safety cone that stopped at the memory limit with no
violation into `ok` when the cone has no `after` assertion and every invariant of the cone (by
name) is in `ProofRun::proved`. `β ∈ Sc(M, β)` whenever the run builds without error
(Definition 3), so by Theorem 1, under its hypotheses (H5 among them: claim I1 at `β`), each of
those invariants holds on `T_β(M, σ)`. A wrong `proved` makes such an `ok` wrong too, and
nothing at run time catches it. A cone stopped at `--max-states` is not changed, which is a
choice, not a limit of the theorem. Neither is a cone with a violation that did not lift,
which stays a tool error whatever the proof says.

### Candidates and the trust base

Proposition 4 holds for any candidate set, so no candidate source can make `proved` unsound.
A candidate's source matters only for which rules get proved. The *trust base* of `proved` is
H1 to H5 (H5 being claim I1 of the build, not of the prover); it contains no candidate, no
mining result, no CTI, and no user lemma.

#### Built-in lemmas

`core/prove/mod.rs::lemmas` offers, per entity, "an absent row holds base values" (needed
because `Enc` does not pin absent rows, while `T` creates rows at base), "no row of an entity no
action deletes is deleted", and, per key, "a present row's key never names an absent parent
row". All are wait 0.

#### Mined lemmas (B9)

`core/prove/mine.rs::mine` explores, with BFS, each entity cone and each selected rule's cone of
`T`, and of the one-instance twin `mining_twin` when it builds as `T` does, each up to
`STATE_CAP = 50,000` states, and turns what every reached state shows into candidates:

| Shape | Wait |
|---|---|
| a present row is one of the reached patterns of its cone's row variables (at most 64), and each column's values and value pairs | 0 |
| a child row with its parent: whole, per parent column | 0 |
| two present child rows that name one parent row: one of their reached pairs of patterns, or none such (`Seen::siblings`) | 0 |
| one child column with one parent column, or with two when its pairs do not imply it | 1 |
| count lemmas: a parent row with a value a count guard's step writes has at most / at least the guard's `k` children by that key (all, or those the guard pins), `k ≤ 7` for at most (`COUNT_MAX = 7`) and `k ≤ 8` for at least | 1 |
| ∀∃: while a present row of `E` holds `a`, some present row of `F` holds `b`, no key between them; from the run's bounds only | 2 |

A mined lemma is a guess from a finite instance, in the manner of invisible invariants (Pnueli,
Ruah, and Zuck, 2001) and of the data-driven inference of DistAI and DuoAI: it may be false at
other bounds, or depend on a capped run. Houdini keeps it only if it is inductive with the rest,
so a capped or wrong mining costs proofs, not soundness. The order of exploration is fixed
(`in_parallel` gathers results in cone order), so the candidates do not depend on threads.

#### CTI clauses

When a rule falls, `core/prove/strengthen.rs` reads the CTI model's pre-state over the step's
parameters and the rule's binders (`diagram`): the present rows, each once; the literals that pin
their columns and marks; their keys to one another or to null; and that two rows of one entity
differ. If there are at most `LITERAL_CAP = 40` literals, the clause `¬(l_1 ∧ … ∧ l_r)` over
those rows is checked for initiation and for consecution relative to the kept set plus the rule
(`inductive`, MBQI on, the lemma rlimit). If it passes, literals are dropped one at a time in a
fixed order while it still passes, and unused binders are trimmed (`generalize`, `trim`). This is
IC3's inductive generalization of a counterexample to induction (Bradley, 2011; Eén,
Mishchenko, and Brayton, 2011), without IC3's frames or its recursion into predecessors. A
surviving clause is a plain candidate of the next round. Its relative induction used the
rule as a hypothesis, so it proves nothing by itself; Houdini decides it with the rest.

#### User lemmas (`.lemma()`)

A rule marked `.lemma()` (`core/prove/mod.rs::lemma_invariants`) is a selected rule like any
other: `Origin::Rule`, wait 0, the rule rlimit (20,000,000). It prints `proved: lemma …` or
`unproved: lemma …` and is left out of the rule counts (`cli/src/prove.rs::run_proof`). Because
it is a rule:

- it adds nothing to the trust base: it is kept only if inductive with the rest (B11);
- when it falls after initiation, it opens the waiting stages like any rule;
- a certificate is reused only if it keeps every rule, user lemmas included (`reusable`,
  `holds_every_rule`).

A lemma, unlike other rules, may read marks: `E.in_run(UC)` reads the run mark of D10 and
`E.raised(Ev)` the latch of D11 (language-reference.md, "Marks in a lemma"). The encoding
reads them as row functions like any column. Reading a mark adds nothing to the trust base
either.

`verify` seeds no BFS cone from a lemma (`cli/src/verify/mod.rs` filters lemma targets) and never
escalates one (`run_proof` removes lemmas from `ProofRun::unproved`). But a lemma whose reads lie
inside another selected rule's cone is checked by that cone. Then a false lemma is `refuted` and
fails the run with a counterexample ([Reproductions](#reproductions)). When BFS does not
reach the violation, Houdini drops the false lemma (`unproved: lemma …: not inductive`), and
the rule it was meant to help is decided without it.
`prove_cli::a_false_lemma_is_refuted_where_bfs_reaches_it_and_dropped_where_not` covers both
paths.

### Verdicts other than proved

<!-- constrained-by ./semantics.md#verdicts -->

- **`unproved: R: not inductive` (B17).** The CTI stage asserts `Step_a`, every kept candidate,
  `⟦R⟧^Pre`, and `Viol(R, Post)`, and prints the model (`read_cti`). The pre-state satisfies `R`
  and every kept candidate, rules and lemmas alike, and the line says so ("breaks it from a
  state where it, every proved rule, and every kept lemma hold"). The pre-state is a model of
  `Enc`, not necessarily the image of a reachable state of any `T_{β'}`: a CTI is a
  counterexample to induction, not a counterexample. It may also have rows or Int values
  beyond `β`.
- **`unproved: R: z3 gave up on A` (B12).** One of: `Fall::Unknown`, when the initial states
  do not encode (no CTI search is made); `Fall::GaveUp(None)`, when the initiation check asked
  again is not `sat` (an `unsat` there prints this too, as "the initial states"); or
  `Fall::Step` or `Fall::GaveUp(Some)`, when no action's CTI check with MBQI on is `sat`.
  Nothing is claimed.
- **`unproved: R: an initial state violates it`.** `Init ∧ Viol(R, Pre)` is satisfiable. The
  model may have more rows or other start values than `I_β`, so this is a violation of some
  `T_{β'}` only if the model corresponds to an initial state of one. `Unproved::Initial`'s doc
  says the bounded engines report it with its trace when the violation lies within their
  bounds.
- **`out of scope: R: why`.** `R` does not encode. No claim.
- **``unproved: R: the canary `false` survived initiation …``.** The premise `Decl ∧ Init` is
  `unsat` (`premise`), so every rule is `Unproved::Premise` and none is proved; this is a bug
  of the builder or the encoder, never a property of the model.

A counterexample (D23) comes from BFS or TLC, never from z3. The two are joined per rule: BFS's
violation makes the rule `refuted`, and escalation looks for one past `β`
([Interaction with BFS](#interaction-with-bfs)).

### Budgets, solver count, and determinism

**Values.** `ProveOptions::default` sets `rlimit = 20,000,000` for a rule (user lemmas included)
and `lemma_rlimit = 2,000,000` for built-in, mined, and CTI lemmas; `strengthen::generalize`
checks a clause at the lemma limit. formal-verification.md and the doc of
`ProveOptions::lemma_rlimit` say that a `.lemma()` rule gets the rule limit.

**Measured headroom** (formal-verification.md, "Budgets", on z3 5.1.0): on every sample on 2
solvers, and on the formal-verification samples, insurance-claim-ops, and oidc-auth on 1 and 3,
a check that keeps its candidate spends at most 161,000 units for a rule (1/124 of its limit) and
116,000 for a lemma (1/17); on 4 solvers a lemma check spent 144,000 (1/14). No such check runs
out. `cli/tests/proof_equivalence.rs` meters every check and fails when a keeping check spends
more than a tenth of its limit, when a check runs out (`canceled`), or when an answering check
spends nine tenths (`only_a_check_that_ran_out_fails_the_headroom_check`,
`the_headroom_check_fails_when_a_check_runs_out` (release jobs; marks as in
[claims.md](../theory/claims.md#how-to-read-a-row))).

**Determinism (B13, B14).** An rlimit counts z3's internal work, not time, so the answer of a
check does not depend on load. It does depend on:

1. the z3 version: `VERIFIED_Z3 = ["5.1.0"]` (pinned by `flake.nix` and `flake.lock`), and
   `unverified_z3_note` prints a note under any other version, or an unreadable one;
2. the query text, fixed by the encoding;
3. the solver's history. Each solver is its own process, and `violated_each` deals checks to
   solver `h` by index (`skip(hand).step_by(hands)`), so each solver receives a sequence of
   commands that depends only on the encoding and on the number of solvers, not on thread timing.

So for one z3 version and one solver count, a proof is a deterministic function of the encoding,
assuming z3 is deterministic on equal input (its default seed is fixed). The solver count is
`RDRA_JOBS`, else `min(cores, 8)` (`crates/rdra-ish-core/src/parallel.rs::jobs`), so it differs
between machines. Independence from it is empirical:
`a_proof_is_the_same_on_any_number_of_solvers` (release jobs) compares 1 against 3 solvers on a
subset of the samples, and 2 solvers on the formal-verification samples. Proof digests were
also compared by hand on 40 roots, on 1 and 4 solvers and on 1, 2, and 3. formal-verification.md
records a lemma check in food-permit-ops that z3 gives up on by itself on 1 to 6 solvers and
that runs out of its limit on 3. Both are `unknown`, so the verdict is the same, but the case
shows that history changes what a check spends.

### The certificate cache

`cli/src/prove.rs` keeps, per model and rule selection, a file under the user's cache directory
holding the z3 version line, the digest of the round (`core/prove/mod.rs::digest`: a tag, the
rdra-ish version, the options, `Decl`, `Init`, every `Step_a`, and each candidate's origin,
rlimit, wait, and formulas), and the indices of the set the last proof kept. The file carries its
own checksum; a damaged one is ignored. `RDRA_ISH_NO_PROOF_CACHE` turns it off.

**Proposition 7 (B15).** A reused certificate makes a rule `proved` only if the set it names is
inductive relative to the current `Enc`.

*Proof.* `reusable` accepts the offered set `K` only if every index is a current candidate,
every rule candidate is in `K`, every candidate of `K` passes initiation, and, for each action,
every member it hits gets `unsat` under `Step_a` and the literals of exactly `K`. That is the
last pass of Proposition 4 for `K`. ∎

The proof does not use the digest. The digest is what makes reuse *output-preserving*: equal
digests mean the earlier run sent the same rounds, so with a deterministic z3 it kept the same
set, and the output is byte-identical
(`a_certificate_is_reused_only_when_it_rechecks_and_changes_no_proof` (release jobs),
`a_cached_certificate_changes_nothing_a_run_prints`). The digest tag (`rdra-ish proof
certificate 3`) must be bumped by hand when the order of checks or how a fall is decided
changes, since the hashed text does not show them. A certificate never carries CTI clauses, so
a proof that needed one misses the cache every time. A failed reuse costs the round, and the
solvers are reset before Houdini runs as usual
(`a_certificate_whose_round_fails_changes_no_proof`).

### Interaction with BFS

<!-- constrained-by ./semantics.md#verdicts -->

**Refutation.** `cli/src/verify/refuted.rs::refuted` collects every invariant a safety cone of
the same run violated, matched by name and site, and passes them to `prove` as `refuted`; they
are not candidates and print `refuted: R (BFS counterexample at k instances)`. `k` is per
entity when the counts differ, under `--instances E=k` or because the trace creates more rows
of an entity the cone does not count than the bound, as `Order=1 Payment=2`
(`cli/src/state_findings.rs::Instances`), and of several cones that violate `R` the one whose
largest count is least is printed.

**Proposition 8 (B16, first part).** Removing candidates never makes a `proved` unsound
(Proposition 4 holds for any candidate set). If (a) the BFS violation is a reachable state of
some `T_{β'}(M, σ)` with `β' ∈ Sc(M, β)` (at `β`, or at the larger instance counts its trace
takes) and (b) every z3 answer is exact, removing `R` changes no other rule's verdict.

*Proof sketch.* By (a) and Theorem 1, `R` is in no inductive set, so exact Houdini drops it in
any run that holds it. Its fall can open a waiting stage; when every other rule was already
kept, the stage only adds candidates, and exact Houdini over a larger start set keeps a superset
of the earlier kept set, so those rules stay proved. When another rule fell, the stage opens
either way. ∎

Neither hypothesis is guaranteed. `refuted.rs::refuted` counts only a violation the cone
vouches for or one `TransitionSystem::recheck_on` found again on a refined cone (D23), so (a)
rests on that recheck and on claim I1. Without (b), the order and content of checks differ
with and without `R`, and an `unknown` can land differently. The test
`a_refuted_rule_left_out_changes_no_other_verdict` (release jobs) checks the conclusion on the
formal-verification samples that have a rule BFS violates (`bounded_verdicts`) and on the
engine-corpus cases whose header expects `z3=refuted`; it skips a model with nothing refuted,
and it asserts that no check of either proof ran out of its limit, so the verdicts it compares
rest on no `canceled` answer. It cannot rule out an `unknown` z3 gives by itself. No `proved`
is wrong in either case.

**Escalation.** For a rule `unproved` by z3 whose own safety cone ran to completion without
violating it, or with only violations a recheck showed spurious
(`cli/src/verify/escalate.rs::passed_at`), the cone is explored again at `n+1` and `n+2`
instances per entity (`grown`). Each run is held to ten times the store of the first
(`GROWTH`) and to what the memory pool has left, when the run has a memory pool (with none,
neither cap applies). The rebuilt system must pass `same_system`. A violation found there is
printed as an error with its trace and fails the run. Its status is that of any BFS
counterexample (D23): one the cone cannot vouch for is decided again on a refined cone
(`recheck_on`), so it is a behavior of `T_{β'}` under claim I1, and `k` names the instances
that run counts, each entity's own. Escalation adds bounded evidence to an unproved rule; it
never changes a `proved`.

**Scheduling.** The proof starts after the safety cones and runs beside the property and
whole-model passes, so it uses the refuted set of the safety pass only.

**CTI against counterexample.** The CTI of an unproved rule and a BFS counterexample of the same
rule are different objects: the first is a state of `Enc` and need not be reachable, the second
is a trace of `T`. By design, the tools do not match one against the other.

### Reproductions

Each model below is short enough to quote in full, and each is in the repository as a test or
a corpus case, which its heading names. The outputs quoted are those of the release build of
main at `91524792` with z3 5.1.0; each run takes under a second.

**A proof under a scope** (`prove_cli::a_proof_under_a_scope_says_so_on_every_proved_line`,
which runs a `Door` model of the same shape and checks the same lines)**.**

```rdra
entity Order "o" {
  id: Int @pk
  status: Enum(open, closed) @default(open)
}
usecase Place "p"
usecase Close "c"
creates(Place, Order)
updates(Close, Order)
transitions(Close, Order.status, open -> closed)
forbidden(Order, status == closed)
```

`verify --prove --without-usecase Close` prints `scope: --without-usecase Close (1 use case
assumed not to run: Close)`, `proved: Forbidden_Order_0 (for any number of rows, in the
selected scope: --without-usecase Close)`, the same clause on every `lemma:` line, and `OK: no
errors in the selected scope, within 2 instances per entity, Int 0..5; 1 of 1 rules proved for
any number of rows, in the selected scope: --without-usecase Close`. Without the scope the same
rule is `refuted`: `Close` reaches `closed`. The proof is right about `T(M, σ)`, and only
about it.

**A build error a scope must not defer** (`prove_cli::a_deferred_error_never_reaches_a_proved_rule`,
`core/tests/select.rs::an_error_in_a_raiser_the_plain_cone_leaves_out_blocks`)**.** Approve's
guard reads Audit's arithmetic `@derived` column, which the build cannot evaluate (a
`DerivedColumnRead` error), so the guard is built as `false`; Approve raises the event that
starts ShipIt, which ships the shipment the rule forbids.

```rdra
entity Order "注文" { id: Int @pk  status: Enum(pending, closed) @default(pending) }
entity Shipment "出荷" { id: Int @pk  status: Enum(waiting, shipped) @default(waiting) }
entity Audit "監査記録" { id: Int @pk  count: Int @default("0")  score: Int @derived(count * 2) }
relate(Shipment, Order, N:1)
relate(Audit, Shipment, N:1)
usecase Place "注文確定"
usecase Prepare "出荷準備"
usecase Record "監査記録"
usecase Approve "承認"
usecase ShipIt "出荷"
event EvApproved "承認済"
creates(Place, Order)
creates(Prepare, Shipment)
creates(Record, Audit)
sets(Approve, Audit.count == 1)
condition ZeroScore "点数がゼロ"
holds(ZeroScore, Audit.score == 0)
applies_to(ZeroScore, Approve)
raises(Approve, EvApproved)
triggers(EvApproved, ShipIt)
transitions(ShipIt, Shipment.status, waiting -> shipped)
forbidden(Order, Shipment, Order.status == pending, Shipment.status == shipped).along(Order, Shipment)
```

The rule's plain cone (`target_cone`) reads Order and Shipment only: it runs ShipIt without
the mark, which only Approve raises, so no plain cone of the selection reads Audit. The prover
encodes the whole system, where Approve never runs. The deferral of build errors under
`--entity` reads the exact cone, which keeps Approve, so `verify --prove --entity Order` fails
with `error[DerivedColumnRead_Audit_score]` and prints no proof. The same model with the
equivalent `Audit.count == 0` prints `refuted: MultiForbidden_Order_0`.

**The top Int value of a nullable column** (the engine-corpus case `width_nullable_int_top`,
`check_cli::a_nullable_int_column_takes_no_range_past_254`)**.**

```rdra
entity Item "i" {
  id: Int @pk
  n: Int @null
}
usecase Add "a"
usecase Top "t"
creates(Add, Item)
updates(Top, Item)
sets(Top, Item.n == 255)
forbidden(Item, n == 255)
```

`verify --prove --int-range-max 255 --instance-count 1` prints `error[DomainTooLarge_Item_n]:
Item: n takes 257 values, null and the Int range, more than the 256 a state holds; give
--int-range-max 254 or less`, and no engine checks the rule. A `present` write reaches the
same error once the column's domain is nullable Int; a numeric rule forces that domain:

```rdra
entity Item "i" {
  id: Int @pk
  n: Int @null @default(0)
}
usecase Add "a"
usecase Fill "f"
creates(Add, Item)
updates(Fill, Item)
sets(Fill, Item.n == present)
forbidden(Item, n == null)
forbidden(Item, n < 0)
```

At `--int-range-max 5 --instance-count 1` both rules are `proved (for any number of rows and
every --int-range-max from 1 to 254, in the whole model)` and the run is `ok`. At 255 the run
prints `error[DomainTooLarge_Item_n]`. The corpus case pins the literal write; the `present`
path is in no test, and is covered because its `DomainTooLarge` error depends on the column
alone.

**A `.when`-guarded `present` write past the run's top** (the engine-corpus case
`z3_guarded_present_past_the_top`, `prove_cli::a_guarded_present_write_reaches_past_the_int_range`)**.**

```rdra
entity Item "i" {
  id: Int @pk
  open: Bool @default(true)
  n: Int @null
}
usecase Add "a"
usecase Fill "f"
creates(Add, Item)
updates(Fill, Item)
condition IsOpen "o"
holds(IsOpen, Item.open == true)
sets(Fill, Item.n == present).when(IsOpen)
forbidden(Item, n > 5)
```

At `--int-range-max 5 --instance-count 1` the rule holds in `T_β`, and z3 prints `unproved:
Forbidden_Item_0: not inductive; Fill(Item#1) breaks it …`: the encoded `Fill` writes any
number. At `--int-range-max 7` BFS prints `refuted: Forbidden_Item_0 (BFS counterexample at 1
instance)`, witnessed by `n=6`. A guard that listed the values at `β` would let the encoding
keep a lemma bounding `n` by 5 and print `proved` at 5 for a rule false at 7.

**A `.when`-guarded `null` beside a `.when`-guarded `present`** (the engine-corpus case
`z3_guarded_null_beside_present`, `prove_cli::a_guarded_null_beside_a_guarded_present_is_a_step_of_the_proof`,
`core/tests/reference/proofs.rs::a_guarded_null_beside_a_guarded_present_is_no_proof`)**.**

```rdra
entity Item "商品" { id: Int @pk  k: Int @null  n: Int @null }
usecase Add "追加"
usecase Bump "増加"
usecase Fill "記入"
creates(Add, Item)
sets(Add, Item.n == 1)
updates(Bump, Item)
sets(Bump, Item.k == present)
updates(Fill, Item)
condition Small "小さい"
holds(Small, Item.k <= 5)
condition Big "大きい"
holds(Big, Item.k > 5)
sets(Fill, Item.n == present).when(Small)
sets(Fill, Item.n == null).when(Big)
forbidden(Item, n == null)
```

The two `sets` lines join one `Choose` of every number with null listed beside it. At
`--int-range-max 5 --instance-count 1` no `k` passes 5, so no step writes null and the rule
holds in `T_β`; z3 prints `unproved: Forbidden_Item_0: not inductive; Fill(Item#1) breaks it
…`, since `one_of` keeps the listed null. At `--int-range-max 6` BFS prints `refuted:
Forbidden_Item_0 (BFS counterexample at 1 instance)`, witnessed by `k=6, n=null`. An
encoding of the choice as `c ≥ from` alone would have no step that writes null and would
print `proved`.

**A false `.lemma()`** (`prove_cli::a_false_lemma_is_refuted_where_bfs_reaches_it_and_dropped_where_not`)**.**
The engine-corpus case `z3_lemma.rdra` with its lemma replaced by the false
`forbidden(Door, Door.state == ajar).lemma()`:

- `verify --prove` prints `cones: 1 for 2 invariant(s) and 0 after-assert(s)`, the cone line
  `Cone_MultiForbidden_Light_0 [Forbidden_Door_0, MultiForbidden_Light_0]: counterexample`,
  `refuted: lemma Forbidden_Door_0 (BFS counterexample at 2 instances)`, the violation, and
  `result: counterexample`. The rule itself is still `proved`.
- `verify --prove --max-states 2`, where BFS does not reach the violation, prints `unproved:
  lemma Forbidden_Door_0: not inductive; Crack(Door#1) breaks it …` and `proved:
  MultiForbidden_Light_0`: Houdini drops the false lemma.

**Preamble satisfiability (spot check of [the preamble](#faithfulness-of-the-encoding)).**
`examples/prove_transcript.rs` writes the SMT-LIB each proof sends for a root. Its first two
batches are the canary's premise check: `Decl` and `Init` after a reset with MBQI off, then
the `(check-sat)` under its rlimit. For
`samples/formal-verification/{order,check_met,one_to_one_shared}.rdra` and
`samples/patent-ops`, the premise check with `smt.mbqi true` in place of `false` is `sat` on all four.

### Threats to validity

- **Faithfulness is argued, not checked.** Proposition 2 is a reading of `encode.rs` against
  `explore/moves.rs` and `expr.rs`. The differential tests check its consequence at 1 to 3
  instances and `int_max + 2` on the samples and the corpus, with the skips claim
  [B1](../theory/claims.md#b-theorem-proving-with-z3) counts. The reference interpreter's proof check covers its
  subset of the language at `m + 1` and `m + 3`. A mismatch that only matters at larger
  bounds, in a large cone, or on a construct neither uses goes unseen. A construct reaches the
  differential tests only once some sample or corpus case has it.
- **Uniformity is argued, not checked.** Lemma U rests on reading every use of `int_max` and
  of slot counts in `core/build`, and such a reading can miss a read; a derived read such as
  `arm_guards` reading a `Choose` list a second time is easy to miss. CI checks the list of
  sites the reading covers against the code ([the sites](#uniformity-in-the-bound) under
  Lemma U), so a new direct read of `int_max` or call of `Domain::of_number` or `Domain::size`
  in `core/build` fails a check; a derived read does not. A builder change that reads `β` into
  a rule or a guard would break Theorem 1 at other bounds without any test failing, unless
  `proof_equivalence`'s wider-Int axis happens to cover it, or the reference interpreter's
  proof check reaches it on its subset. A run-time check that compares the system at `β` with
  the one at a larger `m` would catch this class, and `same_system` does not do that.
- **z3.** H1 is an assumption. The queries are outside a decidable fragment (Int-valued
  functions over uninterpreted sorts, quantifier alternation in ∀∃ lemmas and guards), so
  `unknown` is expected; it costs proofs only. A z3 bug that answers `unsat` wrongly would yield
  a wrong `proved`; `proof_equivalence` would see it only if BFS reaches the violation.
- **Determinism across machines** depends on the solver count matching on every machine or on
  B14 holding, which is empirical.
- **Scope.** A proof under a partial-run flag is a proof of a smaller system, and every
  `proved:` line says so. Under `--entity` the encoded system keeps every build error the
  scope defers; an error is deferred only when it involves no entity of any selected target's
  exact cone, so it alters no action that cone keeps and the rule's verdict is the same with or
  without it.
- **The printer and the parser.** H2 rests on `smt.rs`'s typed builder and its unit tests. H4
  rests on z3 keeping the output protocol of [the answer reader](#the-answer-reader); the
  reader fails closed on any line that is not an answer, but no test drives it, and an answer
  that belongs to another check would go unseen. The model reader (`Sexp`, `read_cti`,
  `strengthen::diagram`) affects only CTIs and candidate clauses, never `proved`.
- **A stopped cone decided by proof.** Corollary 6 turns a cone stopped at the memory limit
  into `ok` on the strength of `proved` alone; a wrong `proved` there gives a wrong `ok` that
  no run-time check catches.

### Related work

- **Houdini.** C. Flanagan and K. R. M. Leino, "Houdini, an Annotation Assistant for ESC/Java",
  FME 2001. The prover's candidate loop is Houdini over candidates of fixed shapes, mined from
  reachable states, and from CTIs, with staged candidates added on failure; S. K. Lahiri and
  S. Qadeer, "Complexity and Algorithms for Monomial and Clausal Predicate Abstraction",
  CADE 2009, study the greatest-fixpoint property that Proposition 5 shows the staging gives up.
- **k-induction.** M. Sheeran, S. Singh, and G. Stålmarck, "Checking Safety Properties Using
  Induction and a SAT-Solver", FMCAD 2000. The prover uses `k = 1` with strengthening lemmas
  instead of a larger `k`.
- **IC3/PDR.** A. R. Bradley, "SAT-Based Model Checking without Unrolling", VMCAI 2011; N. Eén,
  A. Mishchenko, and R. Brayton, "Efficient Implementation of Property Directed Reachability",
  FMCAD 2011. `strengthen.rs` borrows the CTI diagram and the literal-dropping generalization,
  without frames or predecessor recursion.
- **Invariants of parameterized systems.** A. Pnueli, S. Ruah, and L. Zuck, "Automatic
  Deductive Verification with Invisible Invariants", TACAS 2001, generalize invariants from a
  small instance and check them for all sizes, as mining from BFS at the run's bounds and at one
  instance does. O. Padon, K. L. McMillan, A. Panda, M. Sagiv, and S. Shoham, "Ivy: Safety
  Verification by Interactive Generalization", PLDI 2016, keep queries in EPR so they are
  decidable; this encoding does not (Int columns, ∀∃). A. Karbyshev, N. Bjørner, S. Itzhaky,
  N. Rinetzky, and S. Shoham, "Property-Directed Inference of Universal Invariants or Proving
  Their Absence", CAV 2015 (UPDR), and A. Goel and K. Sakallah, "On Symmetry and Quantification:
  A New Approach to Verify Distributed Protocols", NFM 2021 (IC3PO), lift IC3 to quantified
  invariants over unbounded sorts. J. Yao, R. Tao, R. Gu, J. Nieh, S. Jana, and G. Ryan,
  "DistAI: Data-Driven Automated Invariant Learning for Distributed Protocols", OSDI 2021,
  enumerates ∀ candidates pruned by reachable states and keeps the inductive ones. J. Yao,
  R. Tao, R. Gu, and J. Nieh, "DuoAI: Fast, Automated Inference of Inductive Invariants for
  Verifying Distributed Protocols", OSDI 2022, extends this to ∀∃ candidates; the prover's ∀∃
  lemmas follow that pattern.
- **The solver.** L. de Moura and N. Bjørner, "Z3: An Efficient SMT Solver", TACAS 2008. Y. Ge
  and L. de Moura, "Complete Instantiation for Quantified Formulas in Satisfiability Modulo
  Theories", CAV 2009, describe MBQI, which the Houdini rounds turn off and the CTI stage turns
  on.


# Chapter 6 (cont.) The prover as an evaluated heuristic

*Source: `docs/paper/eval/prove.md`.*

## `--prove` as an evaluated heuristic (roadmap T16)

**Decision.** The prover's claim "for any number of rows" rests on Houdini over mined lemma
candidates and counterexample-to-induction clauses ([z3.md](../theory/z3.md), B1–B18). It has no
decidable fragment and no cutoff theorem. Roadmap T16 offered two options: identify such a
fragment, or present the prover as an evaluated heuristic. We take the second. The papers make no
completeness claim. Soundness is the claim B2 of the inventory: a `proved` rule holds at every bound
in the printed range, relative to z3 (H1, H2, H4). Paper 1 does not use `--prove`. The technical
report gives this evaluation.

**Method.** `scripts/paper/prove-eval.py` runs `verify ROOT --prove` at `RDRA_JOBS=1` on every
root of `samples.txt` (oidc-auth at 1 instance). It records each state rule's verdict:
`proved`; `unproved` with its reason; `refuted` (BFS found a counterexample, so the rule is not
tried); `escalated` (unproved, then BFS at one and two more instances). The same run on the 91 cases of
the engine corpus, at their header flags, gives [data/prove-corpus.tsv](eval/data/prove-corpus.tsv). z3
5.4.1 from nixpkgs; the flake pins 5.1.0, and the determinism claim B13 is stated for that version.
Main at `9e4a123`.

### Samples

Every state rule that BFS does not refute is proved, on every sample, in at most 73 s per root:

| Root | proved | unproved | refuted | escalated | out of scope | Wall (s) |
|---|--:|--:|--:|--:|--:|--:|
| samples/ec-site | 1 | 0 | 0 | 0 | 0 | 0.16 |
| samples/clinic-ops | 13 | 0 | 0 | 0 | 0 | 40.02 |
| samples/patent-ops | 13 | 0 | 0 | 0 | 0 | 6.8 |
| samples/food-permit-ops | 14 | 0 | 0 | 0 | 0 | 41.1 |
| samples/insurance-claim-ops | 23 | 0 | 0 | 0 | 0 | 73.19 |
| samples/oidc-auth --instance-count 1 | 16 | 0 | 0 | 0 | 0 | 22.44 |
| samples/personal-info | 2 | 0 | 0 | 0 | 0 | 0.08 |
| samples/incremental-order/step-7-business-rules/src | 2 | 0 | 0 | 0 | 0 | 0.05 |
| samples/formal-verification/check_met.rdra | 2 | 0 | 0 | 0 | 0 | 0.03 |
| samples/formal-verification/check_unmet.rdra | 0 | 0 | 1 | 0 | 0 | 0.08 |
| samples/formal-verification/cross_order_payment.rdra | 1 | 0 | 0 | 0 | 0 | 0.05 |
| samples/formal-verification/cross_order_payment_split.rdra | 0 | 0 | 1 | 0 | 0 | 0.1 |
| samples/formal-verification/int_stock.rdra | 1 | 0 | 0 | 0 | 0 | 0.04 |
| samples/formal-verification/min_card_met.rdra | 1 | 0 | 0 | 0 | 0 | 0.04 |
| samples/formal-verification/min_card_unmet.rdra | 0 | 0 | 1 | 0 | 0 | 0.1 |
| samples/formal-verification/none_any_child.rdra | 1 | 0 | 0 | 0 | 0 | 0.04 |
| samples/formal-verification/none_any_child_bypassed.rdra | 0 | 0 | 1 | 0 | 0 | 0.12 |
| samples/formal-verification/now_coupon.rdra | 0 | 0 | 1 | 0 | 0 | 0.09 |
| samples/formal-verification/one_to_one_shared.rdra | 0 | 0 | 1 | 0 | 0 | 0.1 |
| samples/formal-verification/order.rdra | 2 | 0 | 0 | 0 | 0 | 0.04 |
| samples/formal-verification/quantifier_none.rdra | 0 | 0 | 1 | 0 | 0 | 0.1 |
| samples/formal-verification/quantifier_guard.rdra | 1 | 0 | 0 | 0 | 0 | 0.06 |
| samples/formal-verification/split_run_order.rdra | 2 | 0 | 0 | 0 | 0 | 0.06 |
| samples/formal-verification/untriggered_sets.rdra | 0 | 0 | 1 | 0 | 0 | 0.09 |
| samples/formal-verification/unwitnessed_state.rdra | 1 | 0 | 0 | 0 | 0 | 0.09 |
| samples/formal-verification/unique_double_booking.rdra | 0 | 0 | 1 | 0 | 0 | 0.08 |
| samples/formal-verification/unique_booking_guarded.rdra | 1 | 0 | 0 | 0 | 0 | 0.04 |
| samples/formal-verification/counting_rules.rdra | 2 | 0 | 0 | 0 | 0 | 0.1 |
| samples/formal-verification-fail/order.rdra | 0 | 0 | 1 | 0 | 0 | 0.09 |
| **Total** | **99** | **0** | **10** | **0** | **0** | |

| Unproved: reason | Rules |
|---|--:|

The 79 rules of the five domain models are all proved for any number of rows. On these models the
heuristic is complete: no rule that holds at the bound is left unproved. The models were written
alongside the prover, so this is not evidence about models in general.

### Corpus

The corpus includes cases written to defeat the prover (`z3_*`) and cases whose rule fails only
with more rows. Over its 91 cases, 118 rule verdicts:

| Verdict | Rules |
|---|--:|
| proved | 53 |
| refuted by BFS at the header's bounds | 39 |
| unproved: not inductive | 12 |
| unproved: an initial state of the wider encoding violates it | 1 |

Each unproved rule is escalated: BFS runs its cone at one and two more instances.

| Case | Rule | Escalation |
|---|---|---|
| presence_per_entity_slots.rdra | Unique_Booking_tag | counterexample at Booking=2 Room=4 instances (BFS on its cone; ok at Booking=1 Room=3) |
| presence_per_entity_slots.rdra | Unique_Member_tag | counterexample at 2 instances (BFS on its cone; ok at 1) |
| safety_at_most_bound.rdra | Quantifier_Member_0 | ok at 3 instances, counterexample at 4 instances (BFS on its cone; ok at 2) |
| safety_exactly_along.rdra | Quantifier_Member_1 | counterexample at 3 instances (BFS on its cone; ok at 2) |
| safety_exactly_along.rdra | Quantifier_Member_2 | counterexample at 3 instances (BFS on its cone; ok at 2) |
| safety_max_card_range.rdra | MaxCard_Copy_Shelf | counterexample at 3 instances (BFS on its cone; ok at 2) |
| z3_guarded_null_beside_present.rdra | Forbidden_Item_0 | ok at 2 instances, not decided at 3 instances (BFS on its cone; ok at 1) |
| z3_guarded_present_past_the_top.rdra | Forbidden_Item_0 | ok at 2 instances, not decided at 3 instances (BFS on its cone; ok at 1) |
| z3_int_and_now_boundary.rdra | Forbidden_Coupon_2 | ok at 3 instances, ok at 4 instances (BFS on its cone; ok at 2) |
| z3_int_listing_guard.rdra | Forbidden_Item_0 | ok at 3 instances, ok at 4 instances (BFS on its cone; ok at 2) |
| z3_int_listing_whole_range.rdra | Forbidden_Item_0 | ok at 3 instances, ok at 4 instances (BFS on its cone; ok at 2) |
| z3_needs_run_mark.rdra | MultiForbidden_Light_0 | ok at 3 instances, not decided at 4 instances (BFS on its cone; ok at 2) |
| z3_two_ok_three_fails.rdra | Forbidden_Order_0 | counterexample at 3 instances (BFS on its cone; ok at 2) |

- **Seven rules fail at a larger bound** than the header's. Five of them are `ok` at the `verify`
  default of 2 instances and fail at 3 (four rules) or 4 (one rule): `Quantifier_Member_0`,
  `Quantifier_Member_1`, `Quantifier_Member_2`, `MaxCard_Copy_Shelf` and `Forbidden_Order_0` of
  `z3_two_ok_three_fails`. For these the prover's `unproved` is the right answer, and escalation
  turns it into a counterexample. These are cardinality and counting rules (`at_most`,
  `exactly`, maximum cardinality), which need more rows than the bound they count to.
- **Six rules hold at every bound tried and stay unproved.** The encoding widens the system (any
  absent row may be created, and Int terms are unbounded), and Houdini's candidate shapes do not hold
  an inductive strengthening. Three of the six are `not decided` at the larger bound.

### What this says about "any number of rows"

- On the domain samples, `proved` covers every rule that BFS passes, so the bounded verdict extends
  to any number of rows there.
- On adversarial inputs, 13 of 66 rules that BFS passes at the header's bounds (20%) are not proved.
  For 7 of them the prover is right and the bounded verdict was wrong (more rows break the rule). For
  6 the prover is incomplete.
- The bounded checker's default of 2 instances is not enough for counting rules: the seven above fail
  first at 3 or 4 instances. [minimal.md](eval/minimal.md) measured the corpus's *expected*
  counterexamples at their header bounds. This table adds counterexamples that no header expects,
  found by escalation. Together they say that 2 instances find the known counterexamples of most
  rule kinds, while counting rules need the prover or escalation.

**Baselines not run.** Ivy, mypyvy and DistAI take first-order specifications, and the models would
have to be encoded by hand. A comparison with them is left to the technical report's future work.


# Chapter 7 Agreement with TLC

*Source: `docs/theory/tlc-agreement.md`.*

## Agreement with TLC

<!-- derived-from ./semantics.md -->
<!-- derived-from ./claims.md#d-agreement-with-tlc -->
<!-- derived-from ../engine-agreement.md -->
<!-- derived-from ../formal-verification.md#mapping -->

This document describes main at `91524792`.

It states what assurance exists that the native verifier and TLC, reading the TLA+ module
rdra-ish prints, give the same verdicts, and what kind of assurance each piece is. It owns
rows T1 to T16 of the [claims inventory](../theory/claims.md#d-agreement-with-tlc) and uses the
notation of [semantics.md](../theory/semantics.md) (D1 to D25) without redefining it. Paths follow
the convention of the [claims inventory](../theory/claims.md#how-to-read-a-row): `core` is
`crates/rdra-ish-core/src/lifecycle`, `cli` is `crates/rdra-ish-cli`, `emit` is
`crates/rdra-ish-emit/src`.

**The central point [read].** Agreement with TLC is evidence, not a theorem. No part of the TLA+
translation is proved. What exists is (1) an argument, written here, that each construct of
the printed module denotes the system the build returned, which is `T_β(M)` (D13) when claim
[I1](../theory/claims.md#0-the-build) holds, (2) runtime agreement rules that turn a detected
difference into a tool error, (3) differential tests that run both engines on fixed inputs,
(4) a few mechanised checks of leaf functions, none of which touches the printer, and (5) CI
jobs, which no repository rule requires to pass
([README, Known limitations](../theory/README.md#known-limitations)). Each statement below is tagged:

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

### Terms

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
[I1](../theory/claims.md#0-the-build)), the cone selection (`core/cone.rs`), the
property-cone ladder (`cli/src/property_cone.rs`, `after_plain`, `after_wf`), the reduction's
quotient tables, and `TlaNames`.

### What agreement can show

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

### 1. The translation

<!-- derived-from ../formal-verification.md#mapping -->

#### The claim (Proposition T1)

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

#### Per-construct argument

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
([cones.md](../theory/cones.md#safety-cones)). Tested by the corpus cases `fair_triggered_handler`,
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

#### Status of T1

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

### 2. The agreement rules

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

### 3. Differential evidence

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

#### The corpus

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

#### The sample comparison

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

#### `verify --with-tlc` on the sample roots

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
gap is claims row [T5](../theory/claims.md#d-agreement-with-tlc).

#### Random models

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
[A8](../theory/claims.md#a-cones-of-influence-and-reductions)): of the 522 pull-request seeds, one binds a
fold's `Triggered` param on a stub and none a rise's; of the 45 main seeds, none. A second
generator, `reduction-diff.py --stubs`, draws cones that reduce with a folded S action's
`Triggered` param on a stub and a rise's `Parent`, `Existing` or `Triggered` param on one,
nested reductions among them. Both workflows run 95 of its seeds without TLC, and the
`shadow` job runs 30 of them with S up to `H`. The runs fail when fewer seeds than a floor
reach each shape. No TLC run sees these models.

#### The reduction, step by step

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
  [Lemma 6](../theory/cones.md#the-reduction-by-an-independent-sub-lifecycle)). Every initial state of
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

#### Batch JVMs

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

#### Mutation run

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

#### What each source can and cannot detect

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

### 4. Mechanised evidence

<!-- derived-from ../engine-agreement.md#kani-harnesses -->

**Kani (T13) [read].** There are 23 `#[kani::proof]` harnesses, all in `core`: two in
`expr.rs` (the comparison of D5), three in `types.rs` (the value encoding of the domains), four in `explore/store.rs`, three in `explore/segments.rs`,
three in `explore/packing.rs`, one in `explore/arena.rs`, two in `explore/table.rs`, one in
`explore/symmetry/mod.rs`, one in `prove/encode.rs` (the z3 encoding of a choice), three in
`cone.rs` (the read set of a predicate). Bounded model checking (CBMC, as Kani uses it) proves each
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

### 5. Process gates

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

**No job gates a merge** ([README, Known limitations](../theory/README.md#known-limitations)). Main
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

### Residual risk

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
   model's ([cones.md](../theory/cones.md#safety-cones)), would be a fault of this kind. The 215 hand-written `expect` lines see these on the 89 traps they name.

   For the builder, the reference interpreter (`core/tests/reference/`) is a second check
   [test]. It reads a subset of the language from the `SemanticModel` without
   `TransitionSystem::build`, explores it by brute force on plain values (`i64` integers,
   Enum values by name, no packing, cone, reduction or symmetry), and compares the initial
   states, reachable states, edges, rule names and violated rules with the native run of the
   whole model. Its subset of the language is the one claim
   [I1](../theory/claims.md#0-the-build) lists. It runs on the corpus cases and formal-verification samples in the subset at
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
   ([claims.md, I1](../theory/claims.md#0-the-build) lists what `RefModel::read` refuses; among it
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
   [claims.md](../theory/claims.md#c-the-native-verifier), row N16, with the reason each holds or the
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

### Threats to validity

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

### Related work

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


# Chapter 7 (cont.) TLC baseline

*Source: `docs/paper/eval/tlc-baseline.md`.*

## Baseline: TLC beside the native checker (roadmap #594, T8)

This file records what GitHub Actions logs say about TLC and the native BFS checker run on
the same inputs. TLC cannot run in the container this file was written in, so every number
comes from CI logs. The log lines are in [`data/tlc-ci-lines.txt`](eval/data/tlc-ci-lines.txt);
`scripts/paper/tlc-baseline.py` turns them into the tables below, and
`python3 -I scripts/paper/tlc-baseline.py --check docs/paper/eval/tlc-baseline.md` checks that
the tables here are its output.

### Sources

| | Run A (used first) | Run B (repeat) |
|---|---|---|
| Workflow run | [46114833478](https://github.com/manji-0/rdra-ish-dsl/actions/runs/46114833478) | [46086823346](https://github.com/manji-0/rdra-ish-dsl/actions/runs/46086823346) |
| Event, branch | `workflow_dispatch`, main | `workflow_dispatch`, main |
| Commit | `1d69695d7ff38b5665cebb217ecdc7f58091e4d3` (merge of #564) | `9857092ae33cd9c1dc4b1812a28cdba9b79a720e` (merge of #544) |
| Started | 2026-11-28 06:03 UTC | 2026-11-28 00:14 UTC |
| Run conclusion | failure: only the `check` job's step "no test a pull request runs takes over 10 s" failed | success |
| `formal-verify` job | 140886222125, success | 140798265053, success |
| `engine-equivalence` job | 140886222137, success | not used |
| Runner | `ubuntu-latest` (GitHub-hosted) | `ubuntu-latest` (GitHub-hosted) |
| TLC | 1.8.0 on JDK 25, from the flake's devShell (`tlaplus-1.8.0-jdk25`) | same |
| Binary | `target/ci-checked/rdra-ish` (`--profile ci-checked`) | same |

Run A is the newest main run whose `formal-verify` and `engine-equivalence` jobs succeeded.
Its overall conclusion is `failure` because of the test-time gate of the `check` job, which
runs no TLC. Run B is the run the task named; it is used only as a repeat of the
`formal-verify` numbers. Commit `1d69695` is an ancestor of `9e4a123`, the tree this file was
written on.

### What the logs contain, and what they do not

The per-cone numbers this baseline was meant to hold (native states, TLC distinct and
generated states, TLC and native time per cone) are **not in the CI logs**:

- `scripts/verify-samples.sh` captures each `verify` run's output in a temporary file and
  prints one line per run: verdict against the `// expect` header, root, flags, wall time and
  peak RSS (`measure` and the `printf` at the end of the loop). It prints the output itself
  only on a mismatch. So the `Cone_<check> ...: ok (<n> states)` lines of the native checker
  and the `tlc: Cone_<check> ...: ok (<n> states, <t> s, -Xmx..., <w> workers)` lines of TLC
  never reach the log.
- `reduction-diff.py run` prints one line per seed (`same` and the seed's wall time) and a
  JSON summary.
- `engine_equivalence` and `engine_corpus` are `cargo test` binaries; a passing test prints
  only `... ok`. `a_bfs_and_tlc_reach_the_same_states_and_verdicts` asserts that TLC's
  distinct-state count and graph equal BFS's on every compared cone, but prints neither.

So the tables below hold what the logs do give: per `verify --with-tlc` run, the agreement
with the header and the wall time and peak RSS of the native checker and TLC together; per
random model, agreement; per test binary, pass counts. There is no TLC-only or native-only
time for the same run, so no time ratio between the engines comes from these logs.

Where a state count would come from, if recorded:

- TLC's `tlc:` line prints TLC's **distinct** states (`tlc_output.rs::distinct_states`,
  `verify/tlc.rs`). TLC's generated-state count is parsed nowhere and printed nowhere by
  `verify`.
- The native line prints the states BFS stored for the cone (orbit representatives under
  `--symmetry always`; the samples run with the default, `never`).

To fill the per-cone table, one of these would be needed: a CI step (or a `verify-samples.sh`
option) that prints the `Cone_`/`tlc:` lines of each TLC run; the outputs uploaded as an
artifact; or a machine where TLC runs, running `verify --with-tlc` on the eight runs of
Table 1 at their header flags. This file does not make that change.

### Tables

`Agrees with header` means `verify-samples.sh` printed `ok`: the exit status, the
`tlc_result:` line and the set of error names match the `// expect` header. A disagreement
between the engines prints `error[EngineDisagreement]` and makes both result lines
`tool_error`, which no header expects, so `ok` also means the engines did not disagree on any
check both decided.

<!-- tlc-baseline.py: begin -->

Table 1. `verify --with-tlc` runs of `verify-samples.sh` (native checker and TLC in one process tree; wall time and peak RSS cover both).

| Root | Flags beyond the root | Agrees with header | Wall s, run 46114833478 (`1d69695`) | Peak RSS MB | Wall s, run 46086823346 (`9857092`) | Peak RSS MB |
|---|---|---|---:|---:|---:|---:|
| samples/clinic-ops | (header default) | yes | 24.79 | 699 | 25.46 | 720 |
| samples/patent-ops | (header default) | yes | 23.84 | 651 | 21.88 | 644 |
| samples/food-permit-ops | (header default) | yes | 22.44 | 814 | 20.86 | 812 |
| samples/insurance-claim-ops | (header default) | yes | 105.98 | 1170 | 104.06 | 1176 |
| samples/oidc-auth | `--instance-count 1 --prove --deny-undecided` | yes | 49.36 | 500 | 46.07 | 499 |
| samples/oidc-auth | `--deny-undecided --entity BackchannelRequest --entity DeviceAuthorization --entity LogoutNotice --entity SigningKey` | yes | 1.25 | 179 | 1.22 | 180 |
| samples/formal-verification/partial_scope.rdra | `--entity Order` | yes | 0.29 | 93 | 0.24 | 93 |
| samples/formal-verification/counting_rules.rdra | `--instances Loan=4` | yes | 1.48 | 121 | 1.41 | 121 |
| **Total** | | | **229.43** |  | **221.20** | |

Table 2. All runs of the step.

| CI run | Roots | `verify` runs with TLC | `verify` runs native only | `check` runs | Mismatches | Summary line |
|---|---:|---:|---:|---:|---:|---|
| 46114833478 (`1d69695`) | 40 | 8 | 35 | 40 | 0 | `verify-samples: 40 roots, 83 runs, 0 mismatches` |
| 46086823346 (`9857092`) | 40 | 8 | 35 | 40 | 0 | `verify-samples: 40 roots, 83 runs, 0 mismatches` |

Table 3. `reduction-diff.py run --with-tlc` on the 45 fixed main seeds.

| CI run | Seeds compared | Seeds that reduce a cone | Nested | At 3 instances | `same` | Diverged | Disagreed (`EngineDisagreement`) | Sum of seed wall s | Longest seed s |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 46114833478 (`1d69695`) | 45 | 33 | 12 | 10 | 45 | 0 | 0 | 650.5 | 67.5 (seed 38) |
| 46086823346 (`9857092`) | 45 | 33 | n/a | 10 | 45 | 0 | 0 | 619.3 | 63.9 (seed 38) |

Table 4. TLC test binaries of the `engine-equivalence` job.

| CI run | Test binary | Passed | Failed | Ignored | Wall s |
|---|---|---:|---:|---:|---:|
| 46114833478 (`1d69695`) | `engine_equivalence` | 49 | 0 | 1 | 699.75 |
| 46114833478 (`1d69695`) | `engine_corpus` | 8 | 0 | 0 | 318.48 |

<!-- tlc-baseline.py: end -->

The roots in Table 1 are the five of `scripts/tlc-roots.txt` (the roots with a
`VERIFY_FALLBACK` cone in `engine_equivalence`) and the two flagged header lines elsewhere
(`partial_scope.rdra --entity Order`, `counting_rules.rdra --instances Loan=4`). The other 35
`verify` runs of the step run native only (label `(native)` in the log). Between the two runs,
wall times of the runs over one second differ by at most 9% (patent-ops 23.84 s against
21.88 s), and the totals by 3.7%.

#### Reading the numbers

- **Agreement.** In run A, 8 of 8 `verify --with-tlc` runs on samples, 45 of 45 random models
  (each run reduced and unreduced, each with TLC), the 49 tests of `engine_equivalence` that
  ran, and the 8 tests of `engine_corpus` passed. No log line reports a disagreement between
  the engines. Run B gives the same counts for the steps it was read for.
- **Cones.** The number of cones TLC checked in these runs is not in the logs. The tests that
  compare state counts (`a_bfs_and_tlc_reach_the_same_states_and_verdicts`, every cone of the
  40 sample roots at 1 and 2 instances with the exceptions that
  [tlc-agreement.md, "The sample comparison"](../theory/tlc-agreement.md#the-sample-comparison)
  lists) pass without printing them.
- **Time.** The 229 s of Table 1 is the native checker and TLC together. The only side-by-side
  wall times in the repository are older and from another machine (claims rows T7 and T8 of
  [evidence/measurements.md](evidence/measurements.md): TLC 1.7.4, macOS, before v3.0.0).
  They are not a baseline at this commit. The `shadow` job's native-only run of the same
  samples (job 140886222162 of run A) is not usable as the native column: its binary compares
  every fast path with its reference twin, and it took longer than the TLC run on some roots
  (food-permit-ops 47.02 s native-with-twins against 22.44 s with TLC).

### Symmetry sets in TLC's runs

TLC ran **without** symmetry sets. The printer emits no `SYMMETRY` line in any `.cfg`
(`crates/rdra-ish-emit/src/tla/` contains no `SYMMETRY`; the only `Permutations` there is in
the list of TLC standard-module names the printer must not reuse, `names.rs`). The reasons, from
[instance-symmetry.md, "TLC and engine agreement"](../design/instance-symmetry.md#tlc-and-engine-agreement):

- Slots are integers (`<E>_Ids == 1..InstanceCount`), not model values, and a create takes
  the lowest absent slot through `FirstAbsent` (`module.rs`: `\A j \in DOMAIN ex : j < i =>
  ex[j] # "absent"`). No permutation of all slots preserves that order, and TLC does not check
  that a spec is symmetric under the set it is given, so a `SYMMETRY` set would make TLC
  unsound on the `FirstAbsent` asymmetry of #212.
- TLC's documentation warns that liveness checking under symmetry can miss violations.

The native checker's symmetry (`--symmetry always`) permutes only the slots that are present
in a state (`G_s` fixes absent slots), which keeps `FirstAbsent` sound. TLC's runs therefore
explore each module without symmetry reduction, and serve as an independent check of the
native symmetric run rather than a copy of it. A "TLC with symmetry sets" column of T8 cannot
be produced for these specs without changing how slots are modelled.

### Which system each engine checks

- **Safety cones.** `verify --with-tlc` prints each safety cone's module from the cone as
  `verify` checks it (`verify/jobs.rs::cone_jobs`, `ts.reduced_cone_in`): reduced by its
  independent sub-lifecycle (#74) when that applies, with the quotient's `Rep_`/`Succ_`/`Rise`
  tables computed in Rust and printed into the module. TLC explores that module by its own
  BFS. So on a reduced cone TLC and the native checker explore the same reduced system, and
  TLC's agreement says nothing independent about the reduction itself; the step-by-step check
  of the reduction against its unreduced cone (`tlc_refinement_holds_on_every_reduced_cone_of_the_corpus`)
  runs on the corpus only ([tlc-agreement.md, "The reduction, step by step"](../theory/tlc-agreement.md#the-reduction-step-by-step)).
- **Property cones.** `Cone_<P>` is the reduced cone (or the cone, when nothing reduces);
  `Cone_<P>_Full` and `Cone_<P>_WF` are printed from the unreduced cone. Runs are paired by the
  system they decide (`RunOn`). The native `WF_vars(Next)` run on the reduced cone
  (`decide_liveness_reduced`) has no TLC counterpart and meets TLC's `_WF` run only in the final
  verdict ([tlc-agreement.md, §2](../theory/tlc-agreement.md#2-the-agreement-rules)).
- **Symmetry.** None of the runs in these logs used `--symmetry always`, so a native state
  count and a TLC count of the same cone, if printed, would count the same system. Under
  `--symmetry always` the native count would be of orbits and TLC's of states.
- **Coverage gaps.** The 13 `VERIFY_FALLBACK` cones of oidc-auth meet no TLC run at 2
  instances; of the other four roots' 12 fallback cones, only the 5 cones of invariants or
  `after` assertions get a TLC run here (tlc-agreement.md, "`verify --with-tlc` on the sample
  roots").

### Encoding effort

rdra-ish prints the TLA+ itself; the user writes none. Lines (`wc -l`) at `9e4a123`:

| File | Lines | Role |
|---|---:|---|
| `crates/rdra-ish-emit/src/tla/mod.rs` | 316 | entry points, print modes |
| `crates/rdra-ish-emit/src/tla/module.rs` | 1,278 | module and cfg printer |
| `crates/rdra-ish-emit/src/tla/names.rs` | 571 | TLA+ identifiers, reserved names |
| `crates/rdra-ish-emit/src/tla/pred.rs` | 233 | predicates |
| `crates/rdra-ish-emit/src/tla/refine.rs` | 234 | refinement module (corpus check) |
| `crates/rdra-ish-emit/src/tla/tests.rs` | 1,464 | tests |
| **Printer, without tests** | **2,632** | |
| `crates/rdra-ish-cli/src/verify/tlc.rs` | 910 | launching and scheduling TLC |
| `crates/rdra-ish-cli/src/verify/tlc_tool.rs` | 141 | finding TLC |
| `crates/rdra-ish-cli/src/tlc_output.rs` | 1,116 | parsing TLC's output and traces |
| `crates/rdra-ish-cli/src/tlc_replay.rs` | 131 | replaying TLC traces through BFS |
| `crates/rdra-ish-cli/src/tlc_env.rs` | 200 | heap and environment |
| `crates/rdra-ish-cli/src/tlc_batch.rs` | 377 | batch JVM for the tests |

The CLI files may hold inline tests; their counts are whole files.

### Local run with TLC: per-cone states, verdicts and times

The CI logs above hold no per-cone numbers. This section adds them from a local run. TLC 1.7.4
comes from nixpkgs (`nix shell nixpkgs#tlaplus`; the flake's 1.8.0 build needs GitHub access that
this machine lacks). docs/theory lists 1.7.4 as a supported version. `scripts/paper/tlc-local.py`
runs `verify SPEC --with-tlc --memory 12GiB` and `verify SPEC` without TLC, both at the default
`RDRA_JOBS` on the 4-core machine. It pairs every native cone line with TLC's line for the same
cone. Data: [data/tlc-local.tsv](eval/data/tlc-local.tsv). Main at `9e4a123`.

**Agreement.** Both engines decided 50 cones and whole-model property runs. On all 50 they report
the same verdict and the same number of states. TLC also ran 36 entity-cone state models
(412,613 states in all), which check deadlocks and witnesses, and found what the native checker
found. The one difference in the `result` lines is expected. On insurance-claim-ops the native
checker reports the sample's expected deadlock (`Deadlock_ClaimCase_status_assessed`), while
`tlc_result` is `ok`. TLC's deadlock check is the other reading of D25 (a state with no successor),
and claim T6 records it as one-sided.

**What the state counts compare.** Every cone that TLC checks is the module that `verify` prints
for it: a reduced cone is printed reduced, with the quotient tables, and an orbit run is not
printed. So equal counts confirm that the two engines read the printed system alike. They do not
test the reduction, whose independent tests are the reduced-against-unreduced comparisons
([ablation.md](eval/ablation.md); `reduction-diff.py` in CI). Without the reduction, the native
checker does not finish four of the domain models in 300 s. TLC would explore at least the same
unreduced cones, since `engine_equivalence` asserts equal graphs on unreduced cones.

**Time.** TLC's time is the sum of its per-cone times, as `verify` prints them. Each includes a JVM
start of about 0.7 s. `verify` runs TLC jobs beside the native checker, so the wall time of
`verify --with-tlc` is less than the summed TLC time.

| Model | Cones compared | States (both engines) | TLC time, summed (s) | `verify --with-tlc` wall (s) | `verify` wall, native only (s) |
|---|--:|--:|--:|--:|--:|
| ec-site | 1 | 82 | 0.7 | 2.2 | 0.01 |
| personal-info | 2 | 26 | 1.6 | 2.1 | 0.01 |
| incremental-order | 1 | 9 | 0.7 | 2.0 | 0.01 |
| patent-ops | 9 | 1,684,427 | 55.5 | 25.0 | 1.6 |
| clinic-ops | 11 | 135,583 | 33.9 | 23.6 | 4.9 |
| food-permit-ops | 8 | 47,200 | 23.9 | 23.0 | 7.2 |
| insurance-claim-ops | 10 | 2,116,505 | 211.6 | 88.8 | 16.0 |
| oidc-auth (1 instance) | 8 | 111,376 | 19.5 | 38.2 | 3.2 |

On the domain models, `verify` with TLC beside it takes 3 to 16 times the wall time of the native
run alone. On patent-ops, TLC spends 55.5 s on cones that the native checker decides, with
everything else, in 1.6 s. The native run's wall time also covers the entity cones, the
sub-lifecycle runs and the whole-model property runs, so per-cone native time is lower still. A
per-state comparison would need per-cone native times, which `verify` does not print. The native
checker stores states exactly and runs several cones at once, each on several threads (`RDRA_JOBS`
defaults to the core count). TLC runs with fingerprints and 1 worker per cone (2 for the
whole-model runs), as its lines print.

**TLC with symmetry sets.** Not measured, and not sound here. The printed modules use ordered integer
slots (`1..InstanceCount`) and create in the lowest absent slot (`FirstAbsent`). A `SYMMETRY`
declaration over those slots would merge states that the creation step tells apart. The corpus
case `presence_first_absent_asymmetry` shows a verdict that changes, and TLC's documentation
warns that liveness checking under symmetry can miss violations.

**Alloy, Apalache.** Not run. An encoding of these models in Alloy 6 or Apalache would have to be
written by hand. The TLA+ printer alone is 2,632 lines of Rust, which bounds the effort from below.

#### Table

| Model | Cone | Native | Native states | TLC | TLC states | TLC time (s) |
|---|---|---|--:|---|--:|--:|
| samples/ec-site | Cone_Unique_Payment_Order | ok | 82 | ok | 82 | 0.7 |
| samples/ec-site | StateModel_Order | - | - | state model | 82 | - |
| samples/personal-info | Cone_Unique_PersonalInfo_UserAccount | ok | 1 | ok | 1 | 0.8 |
| samples/personal-info | Cone_Unique_PasswordCredential_UserAccount | ok | 25 | ok | 25 | 0.8 |
| samples/personal-info | StateModel_UserAccount | - | - | state model | 16 | - |
| samples/incremental-order/step-7-business-rules/src | Cone_Forbidden_Store_0 | ok | 9 | ok | 9 | 0.7 |
| samples/incremental-order/step-7-business-rules/src | StateModel_Store | - | - | state model | 9 | - |
| samples/patent-ops | Cone_Forbidden_DraftingOrder_0 | ok | 745 | ok | 745 | 1.1 |
| samples/patent-ops | Cone_Forbidden_PatentApplication_1 | ok | 47593 | ok | 47593 | 5.2 |
| samples/patent-ops | Cone_Forbidden_InventionDisclosure_2 | ok | 931 | ok | 931 | 1.1 |
| samples/patent-ops | Cone_Forbidden_OfficeAction_3 | ok | 1341707 | ok | 1341707 | 22.8 |
| samples/patent-ops | Cone_Forbidden_Patent_4 | ok | 202569 | ok | 202569 | 10.1 |
| samples/patent-ops | Cone_Inv_InventorReward_3 | ok | 87 | ok | 87 | 1.0 |
| samples/patent-ops | Cone_Unique_PatentApplication_DraftingOrder | ok | 761 | ok | 761 | 1.2 |
| samples/patent-ops | Cone_Unique_Patent_PatentApplication | ok | 15017 | ok | 15017 | 2.3 |
| samples/patent-ops | Cone_GrantEnds | ok | 75017 | ok | 75017 | 10.7 |
| samples/patent-ops | StateModel_DraftingOrder | - | - | state model | 745 | - |
| samples/patent-ops | StateModel_InventionDisclosure | - | - | state model | 931 | - |
| samples/patent-ops | StateModel_InventorReward | - | - | state model | 87 | - |
| samples/clinic-ops | Cone_Forbidden_ClaimStatement_0 | ok | 223 | ok | 223 | 1.3 |
| samples/clinic-ops | Cone_Inv_Appointment_0 | ok | 15374 | ok | 15374 | 4.7 |
| samples/clinic-ops | Cone_Inv_Appointment_1 | ok | 15374 | ok | 15374 | 4.0 |
| samples/clinic-ops | Cone_Inv_Encounter_2 | ok | 85154 | ok | 85154 | 9.8 |
| samples/clinic-ops | Cone_Inv_ClinicalOrder_3 | ok | 250 | ok | 250 | 1.8 |
| samples/clinic-ops | Cone_Inv_Prescription_4 | ok | 450 | ok | 450 | 2.0 |
| samples/clinic-ops | Cone_Inv_ClaimStatement_5 | ok | 223 | ok | 223 | 1.3 |
| samples/clinic-ops | Cone_Unique_PatientProfile_PatientAccount | ok | 3 | ok | 3 | 1.3 |
| samples/clinic-ops | Cone_Unique_Encounter_Appointment | ok | 18499 | ok | 18499 | 4.8 |
| samples/clinic-ops | Cone_Unique_AccountBalance_PatientAccount | ok | 1 | ok | 1 | 1.1 |
| samples/clinic-ops | Cone_Unique_FollowUpMessage_CarePlan | ok | 32 | ok | 32 | 1.8 |
| samples/clinic-ops | StateModel_CarePlan | - | - | state model | 381 | - |
| samples/clinic-ops | StateModel_ClaimStatement | - | - | state model | 315 | - |
| samples/clinic-ops | StateModel_ClinicalOrder | - | - | state model | 250 | - |
| samples/clinic-ops | StateModel_Encounter | - | - | state model | 85154 | - |
| samples/clinic-ops | StateModel_IntakePacket | - | - | state model | 245 | - |
| samples/clinic-ops | StateModel_PaymentTransaction | - | - | state model | 78752 | - |
| samples/clinic-ops | StateModel_Prescription | - | - | state model | 1690 | - |
| samples/food-permit-ops | Cone_Forbidden_Disposition_0 | ok | 174 | ok | 174 | 1.7 |
| samples/food-permit-ops | Cone_Forbidden_BusinessPermit_4 | ok | 243 | ok | 243 | 1.5 |
| samples/food-permit-ops | Cone_Inv_Disposition_0 | ok | 23034 | ok | 23034 | 4.8 |
| samples/food-permit-ops | Cone_Inv_PermitApplication_1 | ok | 5885 | ok | 5885 | 3.8 |
| samples/food-permit-ops | Cone_Inv_PermitApplication_2 | ok | 6125 | ok | 6125 | 3.3 |
| samples/food-permit-ops | Cone_MultiForbidden_BusinessPermit_1 | ok | 613 | ok | 613 | 1.1 |
| samples/food-permit-ops | Cone_AfterAssert_SuspendBusiness_0 | ok | 10883 | ok | 10883 | 5.6 |
| samples/food-permit-ops | Cone_HygieneManagerEventuallyAppointed | ok | 243 | ok | 243 | 2.1 |
| samples/food-permit-ops | StateModel_BusinessNotification | - | - | state model | 43 | - |
| samples/food-permit-ops | StateModel_ChangeNotice | - | - | state model | 3032 | - |
| samples/food-permit-ops | StateModel_Consultation | - | - | state model | 157 | - |
| samples/food-permit-ops | StateModel_Inspection | - | - | state model | 23601 | - |
| samples/food-permit-ops | StateModel_MonitoringPlan | - | - | state model | 7 | - |
| samples/food-permit-ops | StateModel_SiteVisit | - | - | state model | 55 | - |
| samples/food-permit-ops | StateModel_SuccessionNotice | - | - | state model | 1640 | - |
| samples/insurance-claim-ops | Cone_Forbidden_Complaint_2 | ok | 483 | ok | 483 | 1.5 |
| samples/insurance-claim-ops | Cone_Forbidden_ClaimCase_3 | ok | 63823 | ok | 63823 | 8.9 |
| samples/insurance-claim-ops | Cone_Forbidden_Claim_4 | ok | 633787 | ok | 633787 | 55.6 |
| samples/insurance-claim-ops | Cone_Forbidden_RepairEstimate_5 | ok | 441 | ok | 441 | 1.6 |
| samples/insurance-claim-ops | Cone_MultiForbidden_JibaisekiSurvey_2 | ok | 65 | ok | 65 | 1.5 |
| samples/insurance-claim-ops | Cone_MultiForbidden_Payment_3 | ok | 783427 | ok | 783427 | 59.2 |
| samples/insurance-claim-ops | Cone_MultiForbidden_RepairEstimate_4 | ok | 65 | ok | 65 | 1.3 |
| samples/insurance-claim-ops | Cone_MultiForbidden_Subrogation_7 | ok | 255 | ok | 255 | 2.8 |
| samples/insurance-claim-ops | Cone_ClaimEventuallyDecided | ok | 633787 | ok | 633787 | 76.4 |
| samples/insurance-claim-ops | Cone_SubrogationEnds | ok | 372 | ok | 372 | 2.8 |
| samples/insurance-claim-ops | StateModel_Complaint | - | - | state model | 39205 | - |
| samples/insurance-claim-ops | StateModel_JibaisekiSurvey | - | - | state model | 189 | - |
| samples/insurance-claim-ops | StateModel_RepairEstimate | - | - | state model | 441 | - |
| samples/insurance-claim-ops | StateModel_Subrogation | - | - | state model | 372 | - |
| samples/oidc-auth --instance-count 1 | Cone_Forbidden_AuthorizationRequest_0 | ok | 8 | ok | 8 | 0.9 |
| samples/oidc-auth --instance-count 1 | Cone_Forbidden_Interaction_1 | ok | 27013 | ok | 27013 | 3.7 |
| samples/oidc-auth --instance-count 1 | Cone_Forbidden_BackchannelRequest_3 | ok | 35 | ok | 35 | 0.9 |
| samples/oidc-auth --instance-count 1 | Cone_MultiForbidden_LogoutNotice_4 | ok | 10 | ok | 10 | 0.9 |
| samples/oidc-auth --instance-count 1 | Cone_Quantifier_Grant_0 | ok | 30277 | ok | 30277 | 4.6 |
| samples/oidc-auth --instance-count 1 | Cone_Quantifier_DeviceAuthorization_1 | ok | 7 | ok | 7 | 0.7 |
| samples/oidc-auth --instance-count 1 | Cone_Unique_Grant_AuthorizationRequest | ok | 27013 | ok | 27013 | 3.9 |
| samples/oidc-auth --instance-count 1 | Cone_Unique_AuthorizationCode_Grant | ok | 27013 | ok | 27013 | 3.9 |
| samples/oidc-auth --instance-count 1 | StateModel_AccessToken | - | - | state model | 30277 | - |
| samples/oidc-auth --instance-count 1 | StateModel_AuthorizationCode | - | - | state model | 30277 | - |
| samples/oidc-auth --instance-count 1 | StateModel_AuthorizationRequest | - | - | state model | 27013 | - |
| samples/oidc-auth --instance-count 1 | StateModel_BackchannelRequest | - | - | state model | 35 | - |
| samples/oidc-auth --instance-count 1 | StateModel_Client | - | - | state model | 5 | - |
| samples/oidc-auth --instance-count 1 | StateModel_Consent | - | - | state model | 27013 | - |
| samples/oidc-auth --instance-count 1 | StateModel_DeviceAuthorization | - | - | state model | 15 | - |
| samples/oidc-auth --instance-count 1 | StateModel_Grant | - | - | state model | 30277 | - |
| samples/oidc-auth --instance-count 1 | StateModel_LogoutNotice | - | - | state model | 14 | - |
| samples/oidc-auth --instance-count 1 | StateModel_OpSession | - | - | state model | 7 | - |
| samples/oidc-auth --instance-count 1 | StateModel_RefreshToken | - | - | state model | 30277 | - |
| samples/oidc-auth --instance-count 1 | StateModel_SigningKey | - | - | state model | 4 | - |


# Chapter 8 Overview, residual risks and known limitations

*Source: `docs/theory/README.md`.*

## The theory of rdra-ish's verification

This directory states what rdra-ish's verification tools decide, under which assumptions,
and on what evidence, for a reader trained in formal methods. Each document defines its
terms before it uses them, states its guarantees as propositions, and says for each one
whether it rests on a proof, a mechanical check, a differential test, a run-time check, or
an assumption.

This document describes main at `91524792`.

It is the entry point: what the verifier guarantees, engine by engine; what those guarantees
rest on; what every run checks; the residual risks; the known limitations; and a map of the
documents. Each statement points at the row of the [claims inventory](../theory/claims.md) that
states it with its evidence and tests. Code is cited by path and symbol, with the
abbreviations of the [claims inventory](../theory/claims.md#how-to-read-a-row): `core/` is
`crates/rdra-ish-core/src/lifecycle/`, `cli/` is `crates/rdra-ish-cli/`, and `emit/` is
`crates/rdra-ish-emit/src/`.

### What the verifier guarantees

A model `M` at bounds `β` denotes a transition system `T_β(M)`, defined from the model's
text in [semantics.md](../theory/semantics.md) (D1 to D13; D24 under a scope). `check` and `verify`
decide its rules and properties and print one verdict per check: `ok`, `counterexample`,
`not decided`, or, from the prover, `proved`; a fault of the tool itself is `tool_error`
([semantics.md, D23](../theory/semantics.md#d23)). Every engine reads the one system the build
returns, so every guarantee below is about that system, and is about `T_β(M)` exactly when
the build implements the definition (claim [I1](../theory/claims.md#0-the-build)).

#### The model and its system

- **The build implements the definition** for every model, bound and scope at which it
  reports no error (I1). The build refuses what it cannot encode faithfully: a column domain
  of more than 256 values (`DomainTooLarge`), a state past the bits a packed state addresses
  (`StateTooWide`), and an action that writes one `(var, param)` twice
  (`ConflictingEffects`). Evidence: per-construct readings, the hand-written corpus headers,
  and a reference interpreter that rebuilds the system from the `SemanticModel` without the
  builder on a subset of the language.
- **Readings a modeler should know.** Each is a decision, stated in the language reference
  and pinned by a test, under which every verdict is right for the model as written:
  - null in a comparison is two-valued, so `x != 3` holds on a null `x`; a lint flags
    `not` over an ordering of a nullable column (I2);
  - verification executes no referential action: `.on_delete(cascade)`, `set_null` and
    `set_default` delete the one row and leave the children, with a warning (I3);
  - a liveness goal is read on a slot whatever its existence, so deleting a row can meet a
    goal its base values satisfy; `.present()` asks for a present row (N9);
  - an `after` assertion does not check a step that changes nothing, as in TLA+ (N5, N18);
  - one raise runs a handler any number of times unless `.counted(k)` says otherwise
    ([semantics.md, D11](../theory/semantics.md#d11)).

#### Native BFS

The native verifier explores the graph of a cone, a reduced cone, or a quotient by
symmetry, and decides each check on that one graph ([native.md](../theory/native.md); claims
N1 to N18).

- A complete run stores exactly the reachable states, each with a shortest-path parent
  (N1); the packed store reads back every state it stores (N2); the memory charge is
  deterministic and never overflows (N3).
- Any number of threads computes the one-thread run: the same states, numbering, edges,
  findings and stop point, except for a stop by the wall-clock budget (N4, N17).
- Safety: a state rule, `always`, or `after` assertion is violated iff a stored state or move
  violates it, reported with a shortest trace (N5); `stable`, `unchanged`, `precedes`
  (N6, N7); `can_reach` on a complete graph (N8); deadlocks as D25 defines them (N14).
- Liveness under weak fairness by fair strongly connected components (N9); on a stopped
  graph only a counterexample decides (N10); up to symmetry, the same verdict as the plain
  run (N12).
- A cap or the budget gives `not decided`, never `ok` (N15, A24). A vacuity warning never
  changes a verdict (N18).
- Every printed counterexample replays through the step relation of its run before it
  prints; one that does not is a tool error, never a counterexample (N11, N13, N16, A16).

#### Cones, the reduction, and symmetry

The verifier decides most checks on a cone of influence, often reduced by an independent
sub-lifecycle and explored up to slot permutations ([cones.md](../theory/cones.md); claims A1 to
A28).

- An exact cone decides what the whole system decides, and a trace lifts (A1). A cone that
  frees slots or leaves a raised mark out over-approximates: its `ok` is sound, and its
  counterexample counts only once a refined or exact cone vouches for it (A2, A6, A28).
  Entity cones and `can_reach` cones are exact (A2, A7).
- A property cone decides under its own fairness by the three-run protocol (A4, A5).
- The sub-lifecycle reduction is a divergence-blind stuttering bisimulation, relative to the
  stability of its partition, which every build certifies, and to Lemma 6, which is proved
  from per-action facts about the code (A8 to A15, A22). The proof is step by step for a flat
  reduction and a sketch for a nested one or one run up to `H`.
  `_WF` runs on a reduced cone are sound relative to the same hypotheses (A15).
- Instance symmetry is a bisimulation under the group the scan computes; leaving a class out
  costs states, never a verdict (A18 to A23).
- Bound escalation turns a counterexample at `n+1` or `n+2` instances into a failure only
  when it is a trace of the larger system, and prints the instances per entity (A25).
- A covered cone's `ok` is the covering cone's (A27). The memory limit bounds the BFS stores,
  not the process (A26).

#### Proofs with z3

`--prove` encodes the system for any number of rows and runs Houdini with mined lemmas and
CTI clauses ([z3.md](../theory/z3.md); claims B1 to B18).

- The encoding over-approximates the system at every bound in the range the model builds
  at (B1). A `proved` rule holds for any number of rows and every `--int-range-max` in the
  printed range, under the printed scope (B2). The prover proves nothing from an
  unsatisfiable premise (B2).
- Initiation and consecution are checked on every candidate; `unknown` never becomes
  `proved`; mined lemmas, CTI clauses and `.lemma()` rules add nothing to the trust base
  (B4, B5, B9 to B12, B17).
- A cached certificate is used only after it re-checks (B15). The verdicts do not depend on
  the number of solvers (B14).
- A rule BFS refutes is not tried, and refuting it changes no other verdict; a proof can
  decide a cone stopped at the memory limit (B16). `proved` covers state rules only (B18).

#### Agreement with TLC

`verify --with-tlc` prints each cone as a TLA+ module and runs TLC beside BFS
([tlc-agreement.md](../theory/tlc-agreement.md); claims T1 to T16).

- The printed module denotes the cone's system (T1). Every check and run both engines decide
  gets the same verdict, every TLC trace replays through BFS's step relation, and a
  disagreement ends the run as `tool_error` (T2, T3, T4, T6).
- The evidence that the two readings agree: the sample comparison of states, edges with
  their params, and findings (T5); the engine corpus with hand-written verdicts (T7);
  random models reduced against unreduced (T9); symmetric against plain runs (T10); batch
  JVMs against processes (T11); the scheduling knobs (T12); Kani harnesses (T13); shadow
  twins (T14); mutation runs (T15); the Int range (T16).

### Trust base and assumptions

<a id="trust-base"></a>

The guarantees above hold given these components and assumptions, none of which the project
verifies:

- **z3** answers `unsat` only for unsatisfiable queries (H1), the SMT text the prover prints
  means the `Expr` it was built from (H2), and the prover takes each answer for the check that
  asked it (H4); z3 5.1.0 is deterministic under a fixed resource limit (B13, B15). Every (b)
  row assumes H1, H2 and H4.
- **TLC** (1.7.4, 1.8.0) and the JVM are correct. Every (d) row assumes it; CI runs the
  devShell's 1.8.0 pre-release on JDK 25.
- **The build** returns `T_β(M)` (I1). It is shared by every engine, together with the
  parser, `build_model`, the choice of a cone, the staging of a property cone's runs, the
  quotient tables of a reduced cone, and `TlaNames`.
- **Paper arguments** that are sketches, not proofs: the nested case of the reduction (A8),
  the per-construct argument for the TLA+ printer (T1), the theorems marked `P-sketch` in the
  [summary by evidence class](../theory/claims.md#summary-by-evidence-class).
- **Lean** (4.36.0, its kernel and the three standard axioms) for the Lean proofs that the
  M evidence of A8, A9, A10, A11, A12, A15, A18, A20, A21, A23 and N9 cites ([proofs/lean](../../proofs/lean/README.md)).
- **Kani** and the Rust compiler. CI's verification jobs build with overflow checks (profile
  `ci-checked`), so a fixed-width integer that wraps panics there; release builds wrap
  silently.

### What every run checks

<a id="run-time-checks"></a>

Every run, in every build:

- replays every printed counterexample through the step relation of the system it is about,
  and reports one that does not replay as a tool error (N16); the lifts' own look-ups are
  faults too (A16, N13);
- refuses, at build time, a column domain past a byte (`DomainTooLarge`), a state past the
  packed width (`StateTooWide`), and an action that writes one slot twice
  (`ConflictingEffects`) (I1, N2, T1);
- panics on an edge number past `u32` instead of wrapping it (`core/explore/edges.rs::edge_number`,
  and the orbit self-edge index of `core/explore/bfs.rs::Bfs::take`, N1);
- certifies the stability of the reduction's partition and falls back to the sweeps'
  partition when the certificate fails (`partition/reference.rs::stable`, A8);
- refuses to prove anything from an unsatisfiable premise (`prove/mod.rs::premise`, B2);
- uses a cached certificate only after it re-checks (B15);
- with TLC, replays every TLC trace through BFS's step relation (T2).

The internal invariants left as `expect`s and `panic!`s on a lift's way stop the run and
print no wrong trace; they are listed, with the reason each holds, in
[claims.md, "Panics left on a lift's way"](../theory/claims.md#panics-left-on-a-lifts-way).

### Residual risks

<a id="conclusion"></a>

<!-- derived-from ./tlc-agreement.md#residual-risk -->

Two residual risks outweigh the rest. They are the first two items of
[tlc-agreement.md, "Residual risk"](../theory/tlc-agreement.md#residual-risk), and the other theme
documents arrive at them from their own side.

1. **Common-mode faults.** The builder (I1), the choice of a cone, the staging of a property
   cone's runs, the quotient tables of a reduced cone, and the names that map native states
   to TLA+ ones (`TlaNames`) are read by both engines, so a fault in any of them moves the
   native verifier and TLC alike, and agreement with TLC cannot show it. A fault of the
   prover's encoding that no sample or corpus case exercises is not common-mode, but no
   comparison of a proof with BFS meets it either. The reference interpreter checks the
   builder without reading its output, on a subset of the language, and checks z3's
   `proved` at `m + 1` and `m + 3` (I1, B1). Outside that subset, and for the cone choice,
   the property-cone ladder, the quotient tables, and `TlaNames`, only hand-written
   expectations, such as the engine corpus's `expect` lines, see such a fault, and only on
   the models they name (T7).
2. **No run-time check of an `ok` or a `proved` as such.** A counterexample is checked
   before it prints (N16); an `ok` and a `proved` are not. The causes of a wrong one that
   are known are refused or checked in every run (see
   [What every run checks](#what-every-run-checks)). A fault of another kind that gives a
   wrong `ok` or `proved` passes unnoticed in the run. The evidence against it is offline:
   the corpus, the reference interpreter, and the differential tests, which CI's
   verification jobs run with overflow checks. A wrong `proved` can also make a cone stopped
   at the memory limit print `ok` (B16).

### Known limitations

<a id="open-problems"></a>

<!-- derived-from ./claims.md -->

Each item is open at this commit. There is one bullet per row of the
[claims inventory](../theory/claims.md) whose Open gap is not empty, led by that row's ID, and it
states every part of that gap; a test in `cli/tests/export_gate.rs` fails when a row with a
gap has no bullet here, or a bullet names a row without one. The list is what has been found,
not a bound on what exists.

**The build and the language**

- **I1.** Outside the reference interpreter's subset, or past its 20,000 states, and for the
  verdicts of properties, only hand-written expectations such as the corpus headers check
  model to `T`, on the models they name. The reference shares the parser, `build_model`, the
  rule names, and which Int columns are numbers. semantics.md takes which models are
  well-formed from the code's list of error diagnostics.
- **I2.** No sample or corpus model draws the `negated-order-admits-null` warning.
- **I3.** Verification executes no referential action, while the SQL export writes
  `ON DELETE CASCADE` / `SET NULL` / `SET DEFAULT`, so a rule over the children can be `ok`
  in the model and broken against that database; executing them is the owner's call.

**Cones, the reduction, and symmetry**

- **A1.** A release build does not check that a step changes only the variables `Wr(a)`
  names; debug and `shadow` builds do. No Kani harness proves it.
- **A2.** That `Trace::free_triggers` lists every step that runs an action reading a mark
  outside the cone is a reading of the code, with no test of its own; that the exact cone's
  counterexample is vouched for is a `debug_assert!` only.
- **A5.** Parts 1 and 3 rest on A8 and A15: on a nested reduction or one run up to `H` on
  their sketched part, and on a flat one on the facts of `Lemma6Hyp` read from the code (A8).
- **A6.** Rests on A8: its nested case and the run up to `H` are sketched, and its flat case rests on the facts of
  `Lemma6Hyp` read from the code.
- **A8.** Lemma 6 is proved from the sixteen fields of `Lemma6Hyp`, and eight of them
  (`kept_rd`, `folded_og`, `folded_names`, `other_names`, `eff_names`, `exact`, `rise_eff`,
  `eff_s`), with the half of `cover` that says what an unfolded guard names, are read from
  the code with no run-time check; that the code's actions
  are the slot syntax over which `local_`, `shape` and `new_writes` are proved is read too.
  The Lean model has one bit per fold, no node fold, the cone's parameter positions and `X`'s
  initial states as projections; cones.md argues those parts on paper. The nested case (the
  induction over the depth, the outer representative and class moves read on the inner run)
  and a run of `S` up to `H` are sketched. Lemma 6 is checked step by step only on the cones the tests explore unreduced,
  with code the check shares. TLC checks the reduction step by step on the corpus's reduced
  cones only, one way (`C ⇒ R`), without fairness. Of the locality Lemma 5 and 6 assume
  (`Local`), Kani proves only that a predicate reads its `pred_reads`, for shapes of depth 1;
  that a step writes only `Wr(a)` and writes a function of `Rd(a) ∪ Wr(a)` has no harness.
- **A10.** Rests on A8 (its nested case and the run up to `H` are sketched; its flat case rests on the facts read
  from the code). That the guards give `Pre` at each step is read from the code, and the
  write-graph model of `monotone_vars` is not joined to the action model of Lemma 5.
- **A11.** The fold equivalence is read from `fold_of` and `holds_fast`, not derived in Lean
  (`Lemma6Hyp.exact`), and is tested on the models and bounds of the fold tests only; node
  folds and bit groups are argued on paper, outside the Lean model; the nested case and the
  run up to `H` are sketched (A8).
- **A13.** No test makes `reduced_with` reject a reduction past the TLA+ cell budget; that
  rejection is read from the code only.
- **A14.** No test targets the cone of its own that a dropped check gets.
- **A15.** Relative to A8's hypotheses (A8 lists the facts of `Lemma6Hyp` read from the
  code). The nested case and the run up to `H` (Definition 5's cases 2 and 3) and case 4's reading of the one
  unobserved class as the partition by input bytes are sketched.
- **A16.** No test reaches most of the lift's look-up faults (a class with no initial S state,
  an S path up to `H` that starts elsewhere, a rise or a target with no block, an S action
  that is not the cone's, `Length`). The lifted start is not checked against the unreduced
  cone's initial states. The panics left on a lift's way stop the run.
- **A18.** No Kani harness proves the canonical form invariant under the group; it rests on an
  exhaustive unit test and the `Canonical` twin. The `g6.rs` test has no `always` or
  `unchanged`. The scan does not check the builder's two facts, and the extended
  sub-lifecycle's `Earlier { step: 0 }` parameters carry no run-mark conjunct. The Lean model
  has action guards only, so (G6) outside guards is a paper step, and `a18_holds_iff` takes the
  invariant's invariance as a hypothesis. The model reads effects in list order, where
  `each_successor` applies the deletes and then the choices last; it has no owner
  `Transition`, no data domain sizes and no `null` data values, and covers neither the reduced
  cone nor the extended sub-lifecycle (A22). That the model is `Group::apply`, `breaks.rs::scan`
  and `each_successor` is read from the code, and so is `WF`. The Lean model's `G6` forbids
  every guard read of a `New` row's owner, where the scan forbids it only when the parent's
  class moves; the remaining case is a paper step. The least BFS depth of each class is a paper
  step only.
- **A20.** Whether each symmetry rule is needed is open: eleven reasons have no
  witness of their own: the seven of `Reduced` and four of the six of `Observed`; the snapshot
  shows only that dropping one changes which entities a cone's group leaves out. That `Hyp`
  holds when a class is marked as not moving is read off its definition, with no Lean theorem.
- **A21.** Deadlocks past the branch cap rest on the entity-cone findings of A23, which have no
  theorem. That `Canon::canonical` returns a member of the orbit as a function of the state
  past the cap is read from the code.
- **A23.** The entity-cone findings up to symmetry have no theorem: that the deadlock criterion
  reads the same on a (state, slot) pair and its image is not derived. The orbit run's search
  is proved by its acceptance criterion, not by its code: Tarjan's components, N10's cut,
  `is_real` as `t ≠ r`, the permutation stored per edge, the bit sets, the choice of binding
  and start, and the lasso the code builds are read from the code. (LiveEqui), that the trigger
  and goal read the same on a state and its image, is argued on paper only. The orbit search on
  a reduced cone and A22's run up to `H` are not modelled.
- **A24.** That a counterexample a cap-stopped safety cone finds is real has no test.
- **A26.** The memory limit does not bound the process.
- **A27.** Covered cones have no test of their own.

**Proofs with z3**

- **B1.** `Enc::compare` re-implements `expr.rs::compare` and is tied to it by reading and
  differential tests only; under `core/prove/` only `Enc::one_of` has a Kani harness, and no
  code has a twin.
  `proof_equivalence` skips 130 comparisons on the samples. The header of
  `z3_guarded_null_beside_present` says BFS breaks its rule at `--int-range-max 6`, which no
  test runs.
- **B2.** Nothing checks at run time that a bound in a proof's printed `--int-range-max` range
  builds the same system; Lemma U is a reading of the code, checked by the reference
  interpreter on its subset.
- **B6.** Step pruning has no test; that a step changes only `Wr(a)` is checked in debug and
  `shadow` builds only.
- **B7.** No test checks that an unstateable rule is `out of scope`, that a guard is dropped or
  a write havocked, or the `note:` line.
- **B9.** No test plants a false or capped mined lemma.
- **B11.** The sketch covers only its central case, a false `.lemma()` being dropped.
- **B12.** H4 is assumed: no test drives `Z3::exec` or `Solver::check_sat`.
- **B13.** No test compares runs on two machines.
- **B14.** Independence from the solver count is empirical.
- **B16.** The checks z3 gives up on by itself are counted, not asserted. A wrong `proved`
  turns a cone stopped at the memory limit into a wrong `ok`, and no run-time check sees it
  (residual risk 2).
- **B18.** No test asserts that `proved` covers state rules only.

**The native verifier**

- **N2.** No Kani harness inserts into the state table and looks a row up.
- **N4.** The loom model, test jitter, and probe harness of native-bfs-parallel.md are not
  built, and no workflow runs Miri.
- **N5.** No test checks an `always` trace against an expected one.
- **N8.** TLC cannot state `can_reach`. The naive oracle shares the step relation and the
  evaluator, and does not cover entity cones up to symmetry. That the reported state is the
  first in BFS order rests on the proof alone.
- **N9.** The Lean proof states the acceptance criterion, not `avoid`'s code: Tarjan's
  components, the bit sets, the choice of binding and start, and N10's cut are read from the
  code.
- **N10.** No Lean theorem covers a stopped graph: the cut at `expanded` is read from the code.
- **N11.** The lasso checks share code with the search they certify (`moves` and
  `Property::fair_instance`; `holds_bound` and `triggers`), so a fault there passes both.
- **N13.** The second half of `a_path_that_does_not_replay_is_a_fault` never runs.
- **N14.** No test has a value whose only way out is a delete. TLC's deadlock is another
  reading (T6).
- **N16.** No test drives `trace_violations` with a stored path that fails to replay. The
  internal invariants of [Panics left on a lift's way](../theory/claims.md#panics-left-on-a-lifts-way)
  are unchecked; among them, that `Extended::rebuild` reproduces `Extended::of` is not
  checked, and that every restriction that keeps `Tick` keeps `now` is not argued.
- **N17.** As N4: the output's independence from `RDRA_JOBS` has the same open items.

**Agreement with TLC**

- **T1.** The printed reduced step has no per-construct argument, and that no two params of
  one action that can bind one slot write one variable is assumed, not checked.
- **T5.** The 25 `VERIFY_FALLBACK` cones meet TLC at 1 instance only, and oidc-auth's browser
  flow at 2 instances rests on the native checker alone; whether an entity cone of the other
  roots shares its variables with an invariant's cone, and so meets TLC there, is open.
- **T6.** TLC's deadlock check is one-sided.
- **T7.** Each corpus header checks only the model it names.
- **T8.** A wrong `ok` is not checked at run time (residual risk 2).
- **T9.** The random-model comparison is no required check (Process, below).
- **T11.** Batch JVMs are compared with processes only on cones of at most 5,000 states, and
  in no required check.
- **T13.** No Kani harness covers the canonical form, the TLA+ printer or the comparison.
- **T14.** A call above a shadow twin's cap is not compared, and a twin checks two
  implementations, not a specification.
- **T15.** No TLC-side mutation run has completed, so it has no kill rate; the mutation runs
  start only by hand and are no required check.

**Process**

- None of the CI checks is required: main has no branch protection and the repository no
  ruleset (`gh api`), so merging on green checks is a convention (T9, T11, T15;
  [tlc-agreement.md, "Process gates"](../theory/tlc-agreement.md#5-process-gates)).
- The proposal to restate items 2, 3 and 7 of the release gate to match what runs
  ([native-temporal-verification.md, "Migration", "The release gate"](../design/native-temporal-verification.md#migration))
  awaits the owner's decision.

### Documents

| Document | Contents |
|---|---|
| [Semantics and notation](../theory/semantics.md) | The transition system `T_β(M)` a model denotes, defined from the model's text, the meaning of every rule and property on it, fairness, scopes, and the verdict vocabulary. Definitions D1 to D25. |
| [Claims inventory](../theory/claims.md) | One row per guarantee the tools make or the docs claim, with its evidence classes, code, tests, and open gap: I1 to I3 (the build), A1 to A28, B1 to B18, N1 to N18, T1 to T16, and the summary by evidence class. |
| [(a) Cones of influence and reductions](../theory/cones.md) | The cone of a check and its closure; when a cone's reachable set is the projection of the model's and when it is a superset; the reduction by an independent sub-lifecycle and its quotient; `_WF` runs on reduced cones; instance symmetry (the group, orbits, entity cones, `DeletedRowOwner`); bound escalation to N+1 and N+2 instances; memory caps and the meaning of `not decided`. Claims A1 to A28. |
| [(b) Theorem proving with z3](../theory/z3.md) | The SMT encoding (`prove/smt.rs`, `prove/encode.rs`), what is folded and how Int terms are bounded; induction with Houdini; the lemma candidate shapes; what `proved` covers for any number of rows and what `.lemma()` adds to the trust base; rlimit determinism; the certificate cache; how proof and BFS refutation interact. Claims B1 to B18. |
| [(c) The native verifier](../theory/native.md) | BFS and the packed state store; parallel exploration and why it computes the one-thread result; safety, liveness under fairness, the temporal operators, `can_reach`; lifting a counterexample from a reduced or symmetric run to the model. Claims N1 to N18. |
| [(d) Agreement with TLC](../theory/tlc-agreement.md) | The TLA+ translation, the agreement rules, the engine corpus, the sample and random-model comparisons, shadow twins, Kani harnesses, the mutation run, and known disagreements. Claims T1 to T16. |

Each theme document takes its rows of the [claims inventory](../theory/claims.md) as its table of
contents: it states each of its claims as a proposition in the notation of
[semantics.md](../theory/semantics.md), gives the proof or proof sketch or names the check that
stands for one, states the assumptions, and lists threats to validity and related work.

#### Reading order

1. [semantics.md](../theory/semantics.md), whole. Everything else is stated in its notation.
2. [claims.md](../theory/claims.md), to see what is claimed and how well each claim is supported.
3. (c), then (a). The native verifier decides a check on one explored graph; the cone
   document says why a graph of a cone, a reduced cone, or a quotient by symmetry decides
   what the model's graph would.
4. (b), which is independent of (a) and (c) except for the interaction with BFS
   refutation and bound escalation.
5. (d), last, since it is the evidence for claims the others make.

#### Relation to the existing notes

<!-- derived-from ../formal-verification.md -->
<!-- derived-from ../engine-agreement.md -->
<!-- derived-from ../state-derivation.md -->

The notes below are written for modelers and contributors, and record the design decisions
and measurements behind each feature. This directory does not replace them. Where a theme
document and a note disagree, the theme document states the code's behavior; the note is
corrected in its own pull request.

| Note | Audience and role | Theme |
|---|---|---|
| [formal-verification.md](../formal-verification.md) | Modelers: what `check`, `verify`, `--with-tlc`, and `--prove` print and decide. | all |
| [engine-agreement.md](../engine-agreement.md) | Contributors: the agreement rules and the tests that hold the engines to them. | (d) |
| [state-derivation.md](../state-derivation.md) | Modelers and contributors: the transition system, bounds, cones, and findings. | semantics, (a) |
| [design/cone-soundness.md](../design/cone-soundness.md) | The soundness arguments for cones, the sub-lifecycle reduction, and `_WF` on a reduced cone. | (a) |
| [design/instance-symmetry.md](../design/instance-symmetry.md) | Design and measurements of the quotient by slot permutations. | (a), (c) |
| [design/native-temporal-verification.md](../design/native-temporal-verification.md) | Design of the native temporal checker and its migration from TLC. | (c), (d) |
| [design/native-bfs-parallel.md](../design/native-bfs-parallel.md) | Design and measurements of parallel BFS and its shared table. | (c) |
| [design/counted-events.md](../design/counted-events.md) | Counted triggers: semantics, cones, TLA+, z3. | semantics, (a), (b) |
| [design/counting-rules.md](../design/counting-rules.md) | Counted quantifiers and per-entity instance counts. | semantics, (a), (b) |
| [design/event-order.md](../design/event-order.md) | Raised marks, and the rounds of the reduction (monotone inputs, the size guard, the node fold). | semantics, (a) |
| [design/tlc-scheduling.md](../design/tlc-scheduling.md) | How TLC runs are scheduled under `verify --with-tlc`. | (d) |


# Chapter 8 (cont.) The claims inventory

*Source: `docs/theory/claims.md`.*

## Claims inventory

<!-- derived-from ./semantics.md -->

This document describes main at `91524792`.

The table lists every guarantee the verification tools make or the notes in `docs/` claim,
one per row. It is the skeleton of the theme documents: each row names the theme that owns
it, and that document states the claim as a proposition and gives its proof, or says what
stands in for one. Each row also says what is open about it today, if anything. The
[index](../theory/README.md) collects those open gaps under
[Known limitations](../theory/README.md#known-limitations).

### How to read a row

- **ID** is `I`, `A`, `B`, `N`, or `T` and a number, for themes (0) the build, (a) cones,
  (b) z3, (c) native, and (d) TLC. `D1` to `D25` are not rows: they are the definitions of
  [semantics.md](../theory/semantics.md).
- **Claim** is stated in the notation of [semantics.md](../theory/semantics.md). In themes (a) to (d),
  `T` is the system `TransitionSystem::build` returns. It is `T_β(M)` (D13), or `T_β(M, σ)`
  (D24) under a scope, exactly when claim I1 holds; the rows of (a) to (d) are about the
  built system and assume I1 only where they say so. `C` is a cone `T|W` (D15), `R` the cone
  reduced by its independent sub-lifecycle, `Q` the quotient of a run by slot permutations.
  `⊨` is "the verdict is `ok`", `⊭` "the verdict is `counterexample`".
- **Where** is the note and section that states the claim.
- **Classes** lists the evidence classes of the row, the strongest first. **Evidence** says
  what each one is. The classes:
  - **P** paper proof. `P-full` is an argument a reader can check step by step; `P-sketch`
    names the facts it rests on without deriving them.
  - **M** mechanically checked: a Kani harness (bounded model checking of a Rust function
    on every input up to a bound), a shadow twin (a reference implementation that a
    `--features shadow` build runs beside the fast one, panicking on any difference, on the
    inputs the CI runs give it), or a check that a debug or `shadow` build makes on every
    step, or a Lean 4 proof (`proofs/lean/`, checked by `scripts/check-lean-proofs.sh` and
    CI's `lean` job). A twin checks two implementations against each other, not against a
    specification. A Lean proof checks a theorem's argument from its hypotheses, stated over
    abstract transition systems or over models of the actions and their slots whose read and
    write sets are computed from the syntax of guards and effects; it does not read the Rust
    code, so it does not show that the code meets the hypotheses or that the code's actions
    are the modelled syntax.
  - **D** differentially tested: the engine corpus, the sample comparison, the random-model
    sweep, the reference interpreter, or the mutation run. Each names its test.
  - **R** runtime-checked: every user run checks the claim on its own output and reports a
    tool error, or refuses the model, when it fails. This is evidence that a wrong answer of
    that kind is not printed, not that the answer is computed correctly.
  - **A** assumed: rests on a component the project does not verify (z3, TLC, the JVM, the
    TLA+ translation's faithfulness, rlimit determinism).
  - **none written**: the claim is true by construction of the code, as far as the code
    shows, but no note argues it.
  - **none**: no evidence found.
- **Code** and **Tests** cite `crate/src/path::symbol`; `core` is
  `crates/rdra-ish-core/src/lifecycle`, `cli` is `crates/rdra-ish-cli`, `emit` is
  `crates/rdra-ish-emit/src`. Code is cited by path and symbol, never by line.
- **Where a test runs.** A cited test with no mark runs in CI's debug `test` step
  (`scripts/diff-test.sh --all` under the `pr` profile of `.config/nextest.toml`) on every
  pull request whose files are not all prose, and on main. The marks, which the theme
  documents use too:
  - **(main only)**: on the slow list of the `pr` profile, and run by no release job, so
    only the `slow tests` step on main runs it.
  - **(release jobs)**: left out of the debug step, on every event by
    `scripts/diff-test.sh`'s `release_only` list or on a pull request by the slow list, but
    run in release by
    `formal-verify`, `engine-equivalence` or `shadow` (`.github/workflows/ci.yml`). Those
    jobs run on every pull request that can change a verdict (the `changes` job's `verify`
    output: `scripts/verify-relevant.sh`, unless the label `no-verify` is set) and on main.
    They run the `engine_equivalence`, `proof_equivalence` and `engine_corpus` binaries
    whole through `cargo test`, which reads no nextest filter, and `formal-verify` runs the
    core tests whose names contain `symmetry::witnesses` or `symmetry::snapshot` and the
    reduced-domain fold tests; so a slow-listed test of those is marked so, not main only.
    A test whose name `release_only` matches runs only there wherever it is cited, marked
    or not: the `bfs_and_tlc_*`, `tlc_alone_*` and `tlc_refinement_*` tests (d) and A8 cite,
    and the reduced-domain
    fold tests the A8 and A11 rows cite, carry no mark.
  - **(with TLC)**, **(with z3)**: the test returns early without the tool on `PATH`, unless
    `RDRA_REQUIRE_TLC=1` or `RDRA_REQUIRE_Z3=1` makes that a failure, as every CI job that
    runs it sets. The corpus test drops a missing engine's verdicts instead. Every test of
    (b) needs z3 and every TLC comparison of (d) needs TLC; the mark is left out there.
  - **(shadow builds only)**: `#[cfg(feature = "shadow")]`; CI's `shadow` job runs it.
  - **(unix only)**: `#[cfg(unix)]`; CI runs on Linux. **(ignored)**: `#[ignore]`, run by hand.
- **Open gap** states, in the present tense, what the row's guarantee or evidence does not
  cover today. An empty cell means none is known. This list is what has been found, not a
  bound on what exists.

### (0) The build

<!-- constrained-by ./semantics.md#the-transition-system -->

[semantics.md](../theory/semantics.md) defines `T_β(M)` from the model's text (D1 to D13, D24). Every
engine reads the one system the build returns, so a departure of the build from the definition
moves every verdict at once, and agreement between the engines cannot show it (the
common-mode faults of the [index's residual risks](../theory/README.md#residual-risks)). This theme
has three rows and no theme document: I1 is the build as a whole, and I2 and I3 are two
readings of the language that the build gives on purpose and that a modeler may not expect.

| ID | Claim | Where | Classes | Evidence | Code | Tests | Open gap |
|---|---|---|---|---|---|---|---|
| I1 | The build implements the definition: for every model `M`, bound `β` (D1), and scope `σ` (D24) at which the build reports no error, `TransitionSystem::build(M, β, σ)` returns `T_β(M, σ)`: the same variables and domains (D2), base values (D3), initial states (D4), and steps `s →_{(a,p)} s'` labelled by use case (D6 to D12), and rules with the same truth value in every state (D16). | [semantics.md](../theory/semantics.md#d13), D13 and the **Implemented by** notes of D1 to D24; state-derivation.md, "The transition system" | none written, D, R | none written as a whole: per-construct readings only, namely z3.md's Lemma U for every use of the bound in `core/build` (with `scripts/check-int-bound-sites.py` holding its list of sites), tlc-agreement.md's argument per construct (which reads the builder's invariants), and the **Implemented by** notes, which are pointers, not readings. R: the build refuses a model it cannot encode faithfully: a column domain of more than 256 values is the build error `DomainTooLarge`, a state past the bits a packed state addresses is `StateTooWide`, and an action that writes one `(var, param)` twice is `ConflictingEffects` (N2, T1). D: the 89 hand-written corpus headers of T7, the only check of model to `T` in CI outside the reference interpreter's subset, on the models they name (`core/tests/oracle.rs` shares the build and does not count). D: a reference interpreter (`core/tests/reference/`) reads the `SemanticModel` without the builder, explores it by brute force on plain values, and compares initial states, reachable states, edges labelled by action (the steps of a split use case and the `UC_by_Ev` variants apart), rule names, and the violated set with the native run of the whole model, at `Scope::ALL`, on every system of at most 20,000 states (`MOST_STATES`; `compare` skips a larger one); it also checks z3's `proved` at `m` against the reference at `m+1` and `m+3`. Its subset, as `interp.rs` states it: entities with Enum, Bool, Int-like and nullable String columns, `@null`, `@default`, `@check`, pre-existing entities without owners, use cases that write one entity and do not both create and delete it, `sets` (literal, `null`, `present`, `.when`), `transitions` with choices and `.when`, `applies_to` on the written entity, and the four single-entity rule forms. It shares the parser, `build_model`, rule naming, and D2's reading of which Int columns are numbers. Outside it is everything `RefModel::read` and its helpers in `interp.rs` refuse with a reason, read from the code; it is more than the file's "Out (refused)" doc comment names, and this is the one list of it. Model lists (`refuse_lists`): `drives`, `chooses`, links, relinks and unlinks, quantifier rules, multi-entity `forbidden` rules and invariants, `after` assertions, counted triggers (`.counted(k)`), `.apart()` writes, foreign keys, and junctions. Entities and columns (`read`, `column`): published entities (exposes, shares), `@unique`, `@derived`, `@default(now())`, `@on_update`, `@version`, variation columns, a `@check` on a column with no variable (a String, or an Int-like column nothing reads as a number, without `@null`), and a `@default` that is no value of its column. Relations (`writes`): `relate`, `triggers`, `invokes`, `compensates`, an `applies_to` on anything but a use case or the system, a create, update or delete that is not from a use case to an entity, a use case that writes two entities (joins, transaction groups) or that both creates and deletes, and a guard on an unknown use case. Comparisons (`cmp`, `compared`, `body`): a read of a column with no variable, an ordered comparison of a column that is not Int, an Int literal against a column that is not Int, a value of another type or an Enum name the column lacks, two columns of different types or two read only as null or present, `now` (and with it the clock), `in` or `not in` without a value list or a value list under any other operator, a column the entity lacks (which a model that loads probably cannot reach; `col_index`), a `holds` body with `.has`, `.none`, `.along` or `.role` or one that reads another entity, and run or raised marks. Writes (`action`): copies; `present` on a column that is neither a nullable Int nor read only as null or present; a `sets` value that is no value of the column at the bound; `sets` on a deleted row; an unguarded `sets` beside another `sets` of the same column; and `transitions` on a created or deleted row, on a column that is not an Enum, with an arm outside the column's values, or beside a `sets` of the same column. Temporal properties are not refused but not evaluated (the doc comment: a property that compares with `now` adds the clock, and the harness skips that model). Scopes are outside it too, since it runs at `Scope::ALL`. Two things are accepted without a refusal: `@default(null)` on any column, nullable or not (`column` reads it as null; the builder's reading of it is not compared), and a `raises` relation, which is sound since the builder makes a mark only for a tracked `triggers` edge, which is refused. | `core/build/` (`TransitionSystem::build`); `core/explore/moves.rs::initial_states`, `::each_successor` | `cli/tests/engine_corpus.rs::bfs_and_tlc_and_z3_agree_on_the_adversarial_corpus` (release jobs); `core/tests/reference/mod.rs::the_builder_agrees_with_the_reference_on_the_corpus` (every corpus case and formal-verification sample in the subset, at three bounds; at least 90 systems), `::the_builder_agrees_with_the_reference_on_random_models` (seeds 0..300), `::the_builder_agrees_with_the_reference_on_many_random_models` (300..3000, main only), `::a_value_past_a_byte_is_not_wrapped`; `core/tests/reference/proofs.rs::a_rule_proved_at_one_int_range_holds_at_wider_ones`, `::a_guarded_null_beside_a_guarded_present_is_no_proof`, `::rules_proved_on_random_models_hold_at_wider_int_ranges` (200 seeds, main only); these three with z3 | Outside the reference interpreter's subset (the list in the Evidence cell) or past its 20,000 states, and for the verdicts of properties, only hand-written expectations such as the corpus headers check model to `T`, and only on the models they name. The reference shares the parser, `build_model`, the rule names, and which Int columns are numbers. semantics.md takes which models are well-formed from the code's list of error diagnostics. |
| I2 | Null in comparisons is read two-valued (D5): `x != 3` holds on a null `x`, and `not (x < 3)` holds on `⊥` while `x >= 3` does not; a logical `@derived` reads the comparison three-valued (`passes`). The truth table in the language reference is the evaluator's, and the lint warning `negated-order-admits-null` flags `not` over an ordering with a nullable operand in a `@check` or a `property`, unless the expression names that column's null. Each verdict is right for the model as written. | language-reference.md § Null; semantics.md D5 | D | D: a test computes the truth table through the builder and the evaluator and compares it with the page; the lint has unit tests | `core/expr.rs::compare`; `core/build/preds/mod.rs::columns_cmp`; `core/build/preds/checks.rs::passes`; `crates/rdra-ish-core/src/lint/nulls.rs` | `core/tests/null_table.rs::the_null_truth_table_in_the_reference_is_the_evaluators`; `crates/rdra-ish-core/src/lint/null_tests.rs` (`a_negated_order_on_a_nullable_column_is_a_warning`, `a_negated_order_that_means_what_it_says_is_not_flagged`) | No sample or corpus model draws the warning. |
| I3 | Verification executes no referential action. D9 guards a delete only for a `restrict` / `no_action` / unset key; for `.on_delete(cascade)`, `set_null` and `set_default` the delete removes the one row and the children, with their keys, stay. The build says so: `DeleteLeavesChildren` (cascade) and `DeleteLeavesChildKeys` (`set_null`, `set_default`) are warnings, drawn for no `@soft_delete` parent, and the language reference states it. | language-reference.md § Deleting a parent; semantics.md D9 | D | D (one corpus case and one unit test) | `core/build/preds/entity_rules.rs::cascade_deletes`; `core/diag.rs`; `emit/sql.rs::foreign_key_clause`; `emit/codegen/mod.rs::obligations` | corpus `safety_delete_set_null`; `core/tests/effects.rs::a_delete_leaves_the_keys_of_a_set_null_or_set_default_key_and_says_so` | The SQL export writes `ON DELETE CASCADE` / `SET NULL` / `SET DEFAULT` (`emit/sql.rs::foreign_key_clause`), so a rule over the children can be `ok` in the model and broken against that database. Executing the actions is the owner's call. |

### (a) Cones of influence and reductions

| ID | Claim | Where | Classes | Evidence | Code | Tests | Open gap |
|---|---|---|---|---|---|---|---|
| A1 | Safety cone exactness: for an invariant or `after` cone `C` with no freed slot and no left-out mark, `Reach(C) = π_W(Reach(T))`. Hence `C ⊨ R ⇔ T ⊨ R`, and a trace of `C` lifts to one of `T`. | state-derivation.md, "Exploration by cone of influence" (Exact); formal-verification.md, "What `verify` checks" | P-full, M, D | P-full ([cones.md](../theory/cones.md#projection-and-lifting), Theorem A1; exact = no freed entity, no left-out mark; a dropped copy guard is harmless). Lemma 2 reads `Wr(a)` (`cone.rs::action_writes`) as every variable a step of `a` can change. M: a debug or `shadow` build keeps per action the state bytes `Wr(a)` holds (`Plan::writes`) and holds every step to them (`check_writes`), panicking with the action and the variable; a `shadow` build counts it as the `writes` twin, so CI's shadow job checks every step of the samples and the corpus. Kani (T13: `an_atom_reads_only_its_read_set`, `a_connective_reads_only_its_read_set`, `a_quantifier_reads_only_its_read_set`) proves the read half Lemma 2 takes of a guard: two states that agree on `pred_reads` give a predicate the same value under `eval`, for atoms, `Not`, `And` and `Or` of at most one part, and one quantifier, on a system of 11 state bytes. D: `assert_cones_exact` asserts equality on every cone that frees no slot and leaves no mark out, `⊆` on a cone that frees slots and on a check's cone that leaves a mark out, and that no entity's cone that frees no slot leaves a mark out | `core/cone.rs::TransitionSystem::cone`, `::target_cone`, `::action_writes`; `core/restrict.rs::TransitionSystem::restrict_to`; `core/explore/plan.rs::check_writes` | `core/tests/cone.rs::cone_exploration_equals_the_projection_of_the_whole_system` (the samples, through `assert_cones_exact`), `::a_parent_existence_joins_the_cone_without_its_columns`, `::a_cone_that_leaves_a_mark_out_keeps_the_whole_systems_verdict_and_view`, `::an_after_assert_step_follows_its_params_into_a_cone`, `::a_step_that_changes_a_var_its_action_does_not_write_is_caught`; four single-model tests in `core/tests/{along_guards,joins,copies}.rs` that run `assert_cones_exact` | A release build does not check that a step changes only the variables `Wr(a)` names. No Kani harness proves it either: one over `each_successor` did not finish in minutes. |
| A2 | Over-approximating cones: when an action of `C` creates an entity `C` does not read (`cone_frees_slots`), or `C` leaves a raised mark out, `Reach(C) ⊇ π_W(Reach(T))`, so `C ⊨ R ⇒ T ⊨ R`. A counterexample of `C` may need more slots than `β` (freed slots) or may be a run the event order forbids at every bound (left-out mark), so it counts only once a cone vouches for its trace: `recheck_on` adds to the cone the raisers of each event the trace ran without (`Trace::free_triggers`) and the existence of each entity it creates past the bound (`Trace::over_bound`) (`refine`), runs it again until the trace is one the cone vouches for (`Trace::vouched`), and falls back to `exact_target_cone` when nothing is left to add; `check`, `verify`, `refuted.rs` and `escalate.rs` use it (`recheck_cones`). An entity's cone is its exact cone whenever its own cone would leave a mark out (`cone.rs::target_cone`), so every cone that feeds `RunsSoFar` and the state-model findings (`NoWitness`, `Deadlock`, `NeverRuns`, arms never taken, values never chosen) is exact for marks. | state-derivation.md, "Exploration by cone of influence" (Exact, Raised marks); formal-verification.md, "Known verification gaps" | P-full, D | P-full for the `ok` of universal checks (Lemma 2 and the stutter argument of Theorem A2); the existential and branching findings of an entity cone do not need it, since an entity's cone is exact for marks; no lifting statement for a left-out mark, whose counterexample is decided again instead; freed slots lift at a larger bound (A28), and a counterexample counts only once its trace is vouched for (no free trigger; with slots, no `over_bound`), which lifts it step for step ([cones.md](../theory/cones.md#projection-and-lifting), Corollary A28′); D | `core/cone.rs::TransitionSystem::cone_frees_slots`, `::target_cone`; `core/restrict.rs::restrict_to` (havoc); `core/targets.rs::TransitionSystem::recheck_on`, `::refine`; `core/trace.rs::over_bound`, `Trace::free_triggers` | `core/tests/cone.rs::cone_exploration_equals_the_projection_of_the_whole_system` (through `assert_cones_exact`, which checks the inclusion on cones that free slots or leave a mark out), `::an_entity_the_view_does_not_read_never_runs_out_of_slots`; `core/tests/trace.rs::a_candidate_counterexample_refines_its_cone_with_the_events_raisers`; corpus `cone_entity_mark_left_out` (a deadlock and a missing witness that a left-out mark would hide) | That `Trace::free_triggers` lists every step that runs an action reading a mark outside the cone is a reading of the code, with no test of its own. That the exact cone's counterexample is vouched for is a `debug_assert!` only (`targets.rs::TransitionSystem::recheck_on`, `cone.rs::TransitionSystem::exact_target_cone`), unchecked in a release build. |
| A3 | A counter of a counted trigger is never left out of a cone: every cone that holds a raiser holds the handler. | counted-events.md, "Cones"; state-derivation.md, "Exploration by cone of influence" | P-full, D | P-full (Proposition A3: `build/events.rs::count_up` and `::count_down` build `Transition`s on the counter, whose source `plain_reads` reads); D (the cited test checks the split of an entity's cone, which keeps the counter, its raiser and its handler in `T`; that the cone holds the handler is the reading above) | `core/cone.rs::TransitionSystem::is_counted`, `::cone`, `::plain_reads`; `core/build/events.rs::count_up`, `::count_down`, `::edge` | `core/tests/reduce.rs::a_counted_mark_keeps_its_raiser_and_handler_out_of_the_sub_lifecycle` | |
| A4 | Property cone: for a property `P` with cone `C = property_cone(P)`, `C ⊨ (Fair_P ⇒ P) ⇒ T ⊨ (Fair_P ⇒ P)`, and the converse when `C` frees no slot and leaves no mark out (`property_cone.rs::inexact` is `None`; otherwise a counterexample of `C` is `Undecided`, A6) (D20, D21 under `Plain`). | cone-soundness.md, "A property's cone decides under its own fairness"; formal-verification.md, "Temporal properties on a cone" | P-full, D | P-full for the unreduced cone `C` (Theorem A4: `ok` needs only the seed; counterexamples need the non-fair actions exact); the verdict `verify` prints is read on the reduced cone, see A5; D (`cli/tests/engine_equivalence.rs::a_property_cone_decides_what_the_whole_model_decides` (release jobs), corpus) | `core/cone.rs::TransitionSystem::property_cone`, `::property_seed`; `cli/src/property_cone.rs` | `core/tests/cone.rs::a_property_cone_keeps_its_fair_actions_whole`, `::a_property_cone_keeps_the_owner_its_along_reads`; corpus rules 1 to 3 | |
| A5 | Three-run protocol: `Cone_P ⊨` under `Plain` decides `ok`; `Cone_P_WF ⊭` under `WfNext` decides `counterexample`; `Cone_P_WF ⊨` decides `ok` only when the cone keeps every written variable; otherwise `RdraSpec_P` on `T` decides, or the verdict is `not decided`. | formal-verification.md, "Temporal properties on a cone"; cone-soundness.md (same section) | P-full, P-sketch, D | P-full for part 2, for `whole` implies exact, and for parts 1 and 3 on a flat reduction, relative to the hypotheses of Theorems A8 and A15 (Theorem A5: the `Plain` run and the first `WfNext` run read the reduced cone (`PropertyCone::bundle`, `verify/native.rs::native_property`)); P-sketch for parts 1 and 3 on a nested reduction or one run up to `H`, where A8 and A15 are sketched; D | `core/cone.rs::TransitionSystem::cone_keeps_every_write`; `cli/src/property_cone.rs` (`after_plain`, `after_wf`, `PropertyCone::bundle`); `cli/src/verify/native.rs::native_property`, `Plain::of` | `cli/src/property_cone.rs::tests::the_wf_run_decides_when_the_cone_is_the_whole_model`, `::a_plain_ok_decides_and_a_counterexample_asks_for_the_wf_run` | Parts 1 and 3 rest on A8 and A15: on a nested reduction or one run up to `H` on their sketched nested case, and on a flat one on the fields of `Lemma6Hyp` read from the code (A8). |
| A6 | On a property cone that frees slots or leaves a raised mark out only `ok` counts; a counterexample is decided again on `T` (`Why::FreesSlots`, `cone_leaves_marks`, `Why::LeavesMarks`). | formal-verification.md, "Temporal properties on a cone", "Order and stability properties" | P-full, P-sketch, D | P-full for `stable`, `unchanged`, `precedes`, read on the unreduced cone, and for `always`, `eventually`, `leads_to` on a flat reduction, relative to Theorem A8's hypotheses (Proposition A6: their `ok` is read on the reduced cone and rests on Theorem A8); P-sketch for those three on a nested reduction or one run up to `H`; D | `cli/src/property_cone.rs` (`Why::FreesSlots`, `Why::LeavesMarks`); `core/explore/properties.rs::TransitionSystem::decide_property` (its `reduced_cone` arm for `Ltl::Always` runs only from tests: the one caller in the CLI, `verify/bfs.rs::explored_properties`, passes only `stable`, `unchanged` and `precedes` (`LtlKind::is_explored`), and `verify` decides an `always` on its reduced property cone through `verify/native.rs::Answer::of`) | `core/tests/explored.rs::a_cone_that_frees_slots_decides_a_violation_on_the_whole_model`; `cli/src/property_cone.rs::tests::always_needs_one_run_and_freed_slots_keep_only_an_ok` | Rests on A8: on a nested reduction or one run up to `H` on its sketched part, and on a flat one on the fields of `Lemma6Hyp` read from the code (A8). |
| A7 | A `can_reach` cone is exact: it keeps every writer, counts every created slot, and is never reduced. | formal-verification.md, "Recoverability on a cone" | P-full, D | P-full (Theorem A7: `keep_writers` makes the cone exact); D | `core/cone.rs::reach_seed`, `::keep_writers`; `core/reduce/mod.rs::drop_unreadable_checks` | `core/tests/reach.rs` | |
| A8 | Sub-lifecycle reduction: the relation `(t, s) ~ (t, rep(class(s)))` is a divergence-blind stuttering bisimulation that preserves `T`-side variables and `obs`, so `R` and `C` agree on every next-free CTL* formula over them: invariants, witnesses, `Runs_*`, arms, `after` assertions, deadlocks, and `Cone_P` under `Plain`. A deadlock is kept because `StateDiag::Deadlock` reads "no path changes the column or deletes the row" (D25, `findings.rs::can_leave`), an existential reachability over non-silent edges, not "no step at all". | cone-soundness.md, "The reduction by an independent sub-lifecycle"; event-order.md, "Why it decides what the cone decides"; formal-verification.md, "Reduction by an independent sub-lifecycle" | P-full, P-sketch, R, M, D | P-full for a flat reduction (no inner reduction of `X`, and `S` not run up to `H`), relative to Lemma 5 (a)(b), the sixteen fields of `Lemma6Hyp`, (B1)–(B3) with a reachable representative per block, and the rise table ([cones.md, Theorem A8](../theory/cones.md#the-reduction-by-an-independent-sub-lifecycle): Lemma 5, Lemma 6 and the theorem are proved step by step from these, each step naming its Lean theorem, and a table there says how the code meets each field of `Lemma6Hyp`: R for `lemma5`, `reads`, `fold_sa` (`split_faults`) and `bind` (`StubCheck`); M for `local_`, `shape`, `new_writes` over the slot model of A18; read from the code for the rest); the parts of the code the Lean model leaves out (groups of folds sharing a bit, node folds, the renumbering of `cone_params`, a dropped key, the initial states) have their own arguments there. P-sketch for a nested reduction or one run up to `H`: up to `H`, `X_n` holds `X`'s states up to `H`'s canonical form and the certificates read the orbit graph (A22, a sketch), and the induction over the depth that puts `x(t, s)` in the inner run up to its canonical form, and the outer representative and class moves the code reads on the inner run, are named and not assembled. Lemma 6's fold equivalence is tested (A11), and so is that every `S_a` step of the cone is a silent step of the extended sub-lifecycle `X`: a parameter on a stub (a parent in `T` of an owner variable in `S`, absent in every state of `X`) whose role needs a present row (`Existing`, `Parent`, `Triggered`) binds to any slot in `X` (`bind_stub_params`), for a fold's `T` parameters and a rise's, so every `T_a` step that changes the inputs has its rise; an unfolded `S_a` action has no parameter on a stub ([cones.md, Lemma 6](../theory/cones.md#the-reduction-by-an-independent-sub-lifecycle)). The whole lemma, rises included, and the relation `~` are checked step by step on every reduced cone whose unreduced cone completes, on the corpus, the samples at their header flags and the random models (`reduce/lemma6.rs::check`, whose class level recomputes the blocks without the reduction's tables). R: every build checks the split against Lemma 5 (a)–(e) and the side rule of Definition 3 before the reduction uses it, from the actions' reads and writes and not from `split_at`'s own sets (`reduce/lemma5.rs::split_faults`, so the Lean proof's hypotheses Lemma 5 (a)(b) hold of every split used); every build also checks the built `X` (`extended.rs::StubCheck`): no parameter of any of its actions (folded and unfolded `S_a` actions, rises, flips) on a stub needs a present row, `bind_stub_params` binds no parameter of an unfolded `S_a` action, nor a folded one's outside its fold, and `restrict_to` keeps every fold's key (`split_with` leaves unfolded an action whose fold's key it would drop); on a fault the cone is decided unreduced, with a warning (a debug or `shadow` build panics). Every build also certifies (B1)–(B3) on the returned partition (`partition/reference.rs::stable`: it refines the labels, one sweep with every block a splitter splits nothing, and the rise signatures are equal per block) and falls back to the sweeps' partition, with a warning, when the certificate fails; a Lean proof over a model of that check shows that a partition it accepts on a complete run of a flat `X` is `Stable` with each block's first state as representative, and that a stable partition passes it (`proofs/lean/RdraIsh/Sweep.lean::stable_of_cert`, `cert_of_stable`), and a Boolean checker over the same lists is proved to imply the certificate (`check_sound`); the partition each flat run of `X` of the samples, the domain samples and the corpus is given, its representatives and the run's shape (a silent edge keeps the inputs, a rise keeps `S`) are checked in CI's `formal-verify` job by `scripts/check-lean-partitions.sh`, which dumps each run and runs a checker proved sound in Lean (`proofs/lean/RdraIsh/PartitionCheck.lean`, `RdraIsh.PCert.stable_of_check`, `RdraIsh.PCert.extended_of_check`), the dumped graph being `X`'s reachable graph read from the dump code. Every build also checks, on each run of `X`, three hypotheses the Lean proof takes (`reduce/hypotheses.rs`): the rise table and `X`'s side of the stages (`rise_faults`: the table holds exactly the rises of each class's representative under the input bytes they end at, each one stage of `stages`; that the class's other states rise alike is (B3), which `partition/reference.rs::stable` certifies), the engines' layout of the input bytes (`layout_faults`), and the inner split of a nested reduction (`inner_faults`: every rise in its `T_a`, every `obs` variable and input of `X` on its `T` side or in its `obs`, so the inner partition's (B1) fixes the inner representative's `obs` and input bytes and, with Lemma 5 (b) of the inner split, which rises it takes and the input bytes they end at, though not the block a rise enters); on a fault the cone is decided unreduced, with a warning (a debug or `shadow` build panics). The run these checks read is `X`'s own when `X` has no inner reduction and is not run up to `H`, which is where they are Lean's `RiseTable` literally; otherwise they read the stored graph, the inner run or the orbit graph, and the step to `X` stays argued. An unobserved `S` has no rise table. That a `T_a` step of `C` has rises through every stage it lists is not certified: a missing stage panics in `moves_of_into` in every build. M: the twins cones.md lists under Theorem A8, and a Lean proof (`proofs/lean/`, [README](../../proofs/lean/README.md)) from Lemma 5 (a)(b), Lemma 6, the shape of `X` (a silent edge keeps the inputs, a rise keeps `S`) and (B1)–(B3) with a reachable representative in each block, stated as hypotheses: `~` is a divergence-blind branching bisimulation between `C` and `R` in which a `T_a` step shows its label (an action with its binding) and an `S_a` step or a class move is silent, and it preserves `T_v` and `obs` (`ReductionLabels.lean::a8_branching`); a class move lifts to `S_a` steps whose actions the silent edges of `X` stand for (`class_move_lifts`, from Lemma 6 with the binding relation `stands`); consequence 1 (`Reduction.lean::a8_holds_iff`, `a8_reachable_obs_iff`; `holds_of_reduced` from the `C ⇒ X` half alone), consequence 2 (`a8_edges_iff`, `a8_runs_iff`, `a8_after_iff`), consequence 3 (`a8_stuck_iff`, `a8_deadlock_free_iff`, D25's paths) and consequence 4 for `□(trig ⇒ ◇goal)` under `Fair_P` over infinite behaviours (`Plain.lean::a8_plain_iff`, both ways); the same with the code's `T` move, `recanon` folding a rise table over the stages of Lemma 6 (`Stages.lean::a8_staged`); and the nested case by induction on depth, a stable partition of the inner run pulling back along each level's inner map, which A8 itself gives for the inner cone (`Nested.lean::a8_nested`, `a8_innerMap`, `Tower.stable`). `ExampleRise.lean` discharges every hypothesis on a cone with a block of two states, a silent exit through it and a rise. It does not derive Lemma 6, the stage list or the rise table from the code, and in the nested case it takes the outer representative among the reachable states of `X` and reads the outer class moves and rises on `X`, where the code takes an inner representative and reads them on the inner run. M: Lemma 5 (a)(b) is derived in Lean (`proofs/lean/RdraIsh/Split.lean`, `split_computed`) over an action-level model of the cone system, from Definition 3 computed as a least fixpoint and the locality of `Rd` and `Wr`, stated as a hypothesis (`Local`); Kani proves the read half of `Local` (1) for a predicate only (T13, the `cone.rs` harnesses: states that agree on `pred_reads` give a predicate the same value, for atoms, `Not`, `And` and `Or` of at most one part, and one quantifier, on a system of 11 state bytes); the folds play no part in it ([cones.md, Lemma 5](../theory/cones.md#the-reduction-by-an-independent-sub-lifecycle)); the code's sets are checked against that `Lemma5` on every top-level split of the samples, the domain samples and the corpus by `scripts/check-lean-certificates.sh`, which dumps each split with its actions' reads and writes and runs a checker proved sound in Lean (`proofs/lean/RdraIsh/Lemma5Check.lean`, `RdraIsh.Cert.Data.check_sound`), in CI's formal-verify job. M: Lemma 6 is derived in Lean over the same model, for an `X` built as Definition 4 builds it (`Restrict.lean`: roles as guard conjuncts on the existences, each parameter's side, the conjuncts `restrict_to` keeps, `fold_param`'s key, `bind_stub_params`, the bits and their flips, `stages`), from `Lemma6Hyp`, among whose fields are `Local`, Lemma 5 (a)–(c), what the folded and other conjuncts and the effects name, that `Fold::holds` computes the bit exactly, and that a rise's effect on the inputs reads `S_v ∪ inputs`. Both `S_a` halves hold with the binding relation of "Parameters" (`s_fwd`, `s_back`) and the rise half through the stages (`rise_fwd`), so `Lemma6By`, `Lemma6Staged` and `Lemma6` are theorems (`lemma6_by`, `lemma6_staged`, `lemma6`), and `a8_actions` gives A8 with `recanon` from `Lemma6Hyp`, (B1)–(B3) and the rise table alone, for the cone without the steps of actions that write nothing (which stutter; `coneW_reach_iff`, and `a8_actions_holds_iff` for invariants on the whole cone). The unfolded table's stub and dropped cells, for every role that reads its row, are derived from Lemma 5 (a)(c) (`unfolded_side`). `Lemma6Example.lean` discharges `Lemma6Hyp` on a model with a folded action whose free `T` parameter is on a stub, an unfolded one, and a rise with a parameter on a stub, and with `bind_stub_params` off the `C ⇒ X` half fails there (`stub_bug`); D | `core/reduce/mod.rs::TransitionSystem::reduced`, `::reduced_with`, `Reduction::recanon`; `core/reduce/split.rs::split_at`; `core/reduce/lemma5.rs::split_faults`; `core/reduce/hypotheses.rs::rise_faults`, `::layout_faults`, `::inner_faults`; `core/reduce/partition.rs::quotient_blocks`, `Classes::of`; `core/reduce/partition/reference.rs::stable`; `core/reduce/extended.rs::bind_stub_params`, `::stub_param`, `StubCheck`; `core/restrict.rs::restrict_to_mapped`; `core/reduce/table.rs::TransitionSystem::class_table`, `emit/tla/refine.rs::print_refinement` (the TLC refinement check) | `core/reduce/partition.rs::the_stability_certificate_rejects_a_partition_that_is_not_stable`; `core/reduce/hypotheses.rs::tests::a_planted_stage_or_rise_table_fails_its_check`, `::a_planted_fault_makes_a_reduced_cone_decide_unreduced` (each planted fault in `of_graph` and `Classes::of` makes a reduced corpus cone decide unreduced); `core/tests/reduce.rs::every_quotient_passes_the_checks_of_its_hypotheses` (every cone of the samples, the domain samples and the corpus at `Bounds::STATES`); `core/tests/reduce.rs::reduced_cones_decide_what_the_cones_decide`, `::reduced_domain_cones_decide_what_the_cones_decide`, `::a_silent_cycle_and_a_stuck_state_give_the_same_deadlocks_reduced`, `::a_triggered_param_on_a_stub_binds_in_the_extended_sub_lifecycle` (main only) (through the helper `fold_bits_hold_where_their_actions_run`, which the fold tests of A11 call too: every `S_a` move of the cone is a silent step of `X` to the same state, on the domain samples, the case fold and this test's model, where every reduced state's path also lifts to a real path of the cone), `::a_rise_with_a_param_on_a_stub_is_a_rise_of_the_extended_sub_lifecycle` (the reduced cone decides what the cone decides, and its traces lift: `assert_reduction_exact`), `::stub_and_dropped_rows_bind_where_the_cone_binds_them` (main only), `::input_rows_bind_where_the_cone_binds_them` (main only), `::the_reduction_models_bind_where_their_cones_bind` (main only) (the parameter matrix of Lemma 6 cell by cell, through the helper `lemma6_steps_hold`: both `S` halves and every `T` step's rises, on the cones of models that fill the cells they name), `::a_param_left_on_a_stub_makes_the_cone_decide_unreduced` (with `bind_stub_params` dropped, the check fires on the two corpus cases below and the cone is decided unreduced, with the cone's verdicts), `::no_extended_sub_lifecycle_leaves_a_param_on_a_stub` (the check passes on every top-level `X` of the samples, the domain samples and the corpus), `::a_fold_whose_key_the_restriction_drops_leaves_its_action_unfolded` (the fold whose key `restrict_to` drops stays unfolded and the reduced cone decides what the cone decides; with the unfolding dropped the check names the key and the cone is decided unreduced, with the cone's verdicts), `::a_rise_guard_on_a_dropped_new_row_goes_with_the_row` (a rise's guard that names a dropped created row goes with it, and `assert_reduction_exact` holds); `core/reduce/extended.rs::tests::a_stub_param_no_fold_or_rise_stands_for_is_left_and_reported` (a rise counted as an unfolded `S_a` action keeps its stub param's role, and the check reports it); `core/tests/reduce_steps.rs::reduced_cones_step_as_their_cones_step` (every corpus case and sample root), `::the_stub_cases_step_as_their_cones_step` (with `bind_stub_params` dropped in the `X` it builds, the check fails on both stub cases), `::a_removed_class_move_fails_the_class_level`; corpus `cone_triggered_param_on_a_stub`, `cone_rise_param_on_a_stub`, `cone_fold_key_dropped`, `cone_rise_names_a_dropped_new_row`; `cli/tests/engine_corpus.rs::tlc_refinement_holds_on_every_reduced_cone_of_the_corpus` (TLC maps each reached state of the unreduced cone to the class of its key and holds each of its steps to a step of the printed reduced module or a stutter, on the 21 reduced corpus cones, two of them nested: the `C ⇒ R` half of the correspondence and Lemma 6's first conjunct on reached states, [tlc-agreement.md](../theory/tlc-agreement.md#the-reduction-step-by-step)), `::tlc_refinement_catches_class_moves_left_out_that_tlc_on_the_reduced_cone_misses` (five class moves emptied: TLC on the reduced cone alone gives the same verdicts, the check fails), `::tlc_refinement_catches_a_param_left_on_a_stub` (shadow builds only: with the stub params left unbound and the stub check off, the check fails on both stub cases at the step `X` drops); `scripts/reduction-diff.py` (`run`, and `lemma6`, the step check on the PR seeds); `cli/tests/engine_equivalence.rs` on reduced cones | The fields of `Lemma6Hyp` that no run checks are read from the code: `kept_rd` (`restrict_to`), `cover` (what an unfolded guard names; what it reads is checked), `folded_og`, `folded_names`, `other_names`, `eff_names` (`fold_of`), `exact` (`Fold::holds`, step (a); `Twin::FoldHolds` compares two implementations only), `rise_eff` (step (c)) and `eff_s`; and that the code's actions are the slot syntax over which `local_`, `shape` and `new_writes` are proved. The Lean model of Lemma 6 has one bit per fold, no node fold, the cone's parameter positions in `X` and `X`'s initial states as the projections of the cone's; cones.md argues those parts on paper. The nested case, and a run of `S` up to `H` (A22), are sketched: in the nested case the induction over the depth that puts `x(t, s)` in the inner run up to its canonical form, and the outer representative and class moves the code reads on the inner run, are named and not assembled. Lemma 6 is checked on every reduced cone the tests explore unreduced; the cones never explored unreduced (past 200,000 states, 25 reduced cones of the samples among them, and every user run) rest on the proof and its readings, and the check shares `Reduction::key` and `Extended::build` with the code. The TLC refinement check runs on the corpus's reduced cones only, in the `C ⇒ R` direction, on safety steps: TLC does not evaluate `ENABLED` through the class mapping, so no fairness is checked through it. No Kani harness proves the split of an action's guards between `plain_reads`, `mark_reads` and `copy_reads`, or `Local` (2) and (3), that a step writes only `Wr(a)` and that what it writes is a function of `Rd(a) ∪ Wr(a)`. |
| A9 | The worklist partition equals the partition by repeated sweeps. | `core/reduce/partition/reference.rs` (module doc) | M, R, D | M (twin `Twin::Partition`, skipped above 2,500,000 states, absent in release builds); R (every build certifies the returned partition and falls back to the sweeps' on failure, A8; the certificate gives (B1)–(B3) by `proofs/lean/RdraIsh/Sweep.lean::stable_of_cert`, and that the sweeps' fallback meets the certificate when they stop is read from the code; the partition used on each flat run of the samples, the domain samples and the corpus, whichever it is, passes a checker proved sound in Lean, `proofs/lean/RdraIsh/PartitionCheck.lean::stable_of_check`, in CI's `formal-verify` job); D | `core/reduce/partition.rs::quotient_blocks`; `core/reduce/partition/reference.rs::quotient_blocks` | `core/reduce/partition.rs::the_worklist_partition_is_the_partition_of_repeated_sweeps` | |
| A10 | Monotone inputs: an `S` action may read `T` variables that only rise; the quotient of `S` extended with its inputs is stuttering-stable under silent steps and strongly stable under rises, so A8 still holds. `monotone_vars` counts a column cyclic when a delete resets it, and counts owners cyclic though a delete keeps them; it lists the values a non-creating write starts from through the checked `narrow::id8`, so a column of 256 values that a use case sets up and down is not monotone. | cone-soundness.md (point 5); event-order.md, "The extended sub-lifecycle and its quotient" | P-full, P-sketch, R, M, D | P-full for a flat reduction, relative to Theorem A8's hypotheses and, for the finitely many changes of a monotone variable, the pre-state facts `Pre` the guards give ([cones.md, A10](../theory/cones.md#the-reduction-by-an-independent-sub-lifecycle): Lemma 5 (c)(d) proved step by step; A8's proof reads the inputs only as input bytes and does not use their monotonicity; a step from `Pre` moves a cell along an edge of `monotone_vars`' graph, and an acyclic graph bounds the changes); the three listed properties of `monotone_vars` by reading; P-sketch for a nested reduction or one run up to `H`, as A8; R: every build checks Lemma 5 (c)(d) on the split before use, that every `T_v` variable an `S_a` action reads is an input or read by its fold and every input is monotone by `monotone_vars` (`reduce/lemma5.rs::split_faults`; the cone is decided unreduced, with a warning, on a fault); M (assert in `moves_of_into` under `shadow`; Lean proof of Lemma 5 (c)(d), `proofs/lean/RdraIsh/Split.lean::lemma5_of_closed`, given that an existence of a monotone variable is monotone and that the folds cover the `T_v` reads of `S_a` actions outside `raw_reads`; and a Lean model of the write graph `monotone_vars` builds and of `each_successor`, over which every step from a pre-state that meets `Pre` moves a non-cyclic variable's cells along that graph, so a variable it marks monotone changes finitely often along every behaviour, and an existence that only creates and deletes write is monotone, `proofs/lean/RdraIsh/Monotone.lean::succ_covers`, `monotone_finite_changes`, `existence_monotone`); D | `core/reduce/split.rs::monotone_vars`; `core/narrow.rs`; `core/reduce/extended.rs::Extended`; `core/reduce/mod.rs::Reduction::rises_from` | `core/tests/reduce.rs::a_sub_lifecycle_that_reads_inputs_of_t_is_reduced_and_decides_the_same` (main only), `::a_steady_action_never_changes_the_inputs`, `::a_256_value_column_set_both_ways_is_not_monotone`, `::every_split_passes_its_check`; `core/reduce/lemma5.rs::a_planted_split_fails_the_clause_it_breaks` | Rests on A8: on a nested reduction or one run up to `H` on its sketched part, and on a flat one on the fields of `Lemma6Hyp` read from the code (A8). That the guards give `Pre` at each step is read from the code, and the write-graph model of `Monotone.lean` is not joined to the action model, so `FoldData.ex_mono` is a hypothesis of Lemma 5 (d) there (the run-time check of (d) does not need it). |
| A11 | Folds (param fold, node fold): a folded guard is a function of its bit, and a count is never node-folded, so A8 holds with folds. | cone-soundness.md (folds); counting-rules.md, "Cone and reduction" | P-full, P-sketch, M, D | P-full for a flat reduction, relative to a reading of `fold_of` and `Fold::holds` (Lemma 6's step (a) derives the fold equivalence, `Lemma6Hyp.exact`, from them, and Lemma 6's proof, step by step and in Lean over the action model with one bit per fold (`proofs/lean/RdraIsh/Lemma6.lean`, A8), uses it with what `fold_of` lets the folded and other conjuncts and the effects name; groups of folds sharing a bit and node folds, outside the Lean model, are argued on paper in cones.md); P-sketch for a nested reduction or one run up to `H`, as A8; that a count is never node-folded is read from the code: `fold.rs::strip_links` returns `None` on `Pred::Count`, so `as_node` makes no node of it); M (twin `Twin::FoldHolds`); D against the cone's own step relation, in CI's formal-verify step with release optimisations and overflow checks (profile `ci-checked`): `core/tests/reduce.rs::reduced_domain_cones_hold_each_fold_bit_where_its_action_runs` (the bit holds wherever the action runs) and `::reduced_domain_cones_step_by_a_true_fold_bit_only_where_the_cone_steps` (a true bit adds no step the cone lacks), both on the 71 folds of the domain samples, and `::case_fold_bits_hold_exactly_where_their_actions_run_past_slot_0` (both halves past slot 0 on the case fold); planted bits fail each; no random model has a count | `core/reduce/fold.rs::as_node`, `::strip_links`, `::fold_initial`; `core/reduce/extended.rs::Extended::build` | `core/tests/reduce.rs::a_quantifier_over_t_folds_into_a_bit_keyed_by_the_linked_row`, `::a_count_over_t_in_a_guard_reduces_exactly` (main only), and the three fold tests named in Evidence | The fold equivalence is read from `fold_of` and `holds_fast`, not derived from them in Lean (`Lemma6Hyp.exact`), and is tested on these models and bounds only; node folds and bit groups are outside the Lean model and argued on paper. On a nested reduction or one run up to `H`, A8, and with it A11, is sketched (A8). |
| A12 | A counter is never an input of `S`: `split_at` puts every counted mark in `T`, and `monotone_vars` marks counters cyclic. | counted-events.md, "The reduction" | P-full, M, D | P-full by reading (`split_at` seeds `T` with every counter; `monotone_vars` marks it cyclic); M (Lean, `proofs/lean/RdraIsh/Split.lean::no_counter_input`: every input is monotone, so no counter is one, given that `monotone_vars` marks every counter non-monotone); D | `core/reduce/split.rs::split_at`, `::monotone_vars` | `core/tests/reduce.rs::a_counted_mark_is_not_monotone_without_its_handler` | |
| A13 | A reduction whose quotient is the identity, or whose TLA+ tables pass 530,000 cells, is not used, so the native checker and TLC check the same cones. | formal-verification.md, "Reduction by an independent sub-lifecycle"; event-order.md, "The size guard" | P-full, D | P-full by construction (one `Reduction` feeds both engines); D for the identity quotient and the cell count | `core/reduce/mod.rs::TLA_CELL_BUDGET`, `Reduction::tla_cells`, `Reduction::shrinks`; `core/reduce/partition.rs::Classes::of_graph` | `core/tests/reduce.rs::an_identity_quotient_leaves_the_cone_unreduced`, `::a_reduced_cone_counts_the_cells_its_tla_tables_print` | No test makes `reduced_with` reject a reduction past the cell budget; that rejection is read from the code only. |
| A14 | A check that reads `S` outside `obs`, an `after` assertion on an `S` action, or a property fair on an `S` action is decided on a cone of its own. | formal-verification.md, "Reduction by an independent sub-lifecycle" | P-full, R | P-full by reading (a target's own check reads only `T ∪ obs`, so only passengers are dropped; `Covered::covers` requires the check in the covering module); R: every build checks Lemma 5e on the split before use, that every seed's read of `S` is in `obs` and every kept action is in `T_a` (`reduce/lemma5.rs::split_faults`; the cone is decided unreduced, with a warning, on a fault) | `core/reduce/mod.rs::drop_unreadable_checks`; `core/reduce/lemma5.rs::split_faults` | for Lemma 5e: `core/reduce/lemma5.rs::a_planted_split_fails_the_clause_it_breaks`, `core/tests/reduce.rs::every_split_passes_its_check` | No test targets the drop itself. |
| A15 | `_WF` on a reduced cone: if the `WfNext` run on `R`, with every class that can loop taken as a stop, finds no counterexample on a complete graph, then `Cone_P_WF` on `C` is `ok`. A counterexample on `R` is a candidate that prints only after it lifts, replays on `C`, fails `P`, and passes the fairness check; otherwise `Cone_P_Full` and `Cone_P_WF` run on `C`. | cone-soundness.md, "`WF_vars(Next)` on a reduced cone"; formal-verification.md, "WF_vars(Next) on a reduced cone" | P-full, P-sketch, M, R, D | P-full for a flat reduction with `obs` not empty, relative to A8's hypotheses, finitely many reachable states of `X` and `R`, and case 1 of Definition 5 for every block with a silent cycle (Theorem A15, with A8's hypotheses as cones.md's Theorem A8 lists them and says how the code meets each); P-sketch for a nested reduction or one run up to `H` (the proof's (ii) and Definition 5's cases 2 and 3, "the same argument one level down") and for case 4 (`obs = ∅`, where the code's one unobserved class is read as the partition by input bytes); M: a Lean proof (`proofs/lean/RdraIsh/Fairness.lean`, `a15`) that Lemma 5 (a)(b), Lemma 6 both ways, the shape of `X` and (B1)–(B3), stated as hypotheses as in A8, with finitely many reachable states of `X` and of `R` and every block with a cycle of silent edges between different states marked as able to loop (Definition 5, case 1), make the run's verdict sound: if no lasso of `R` that satisfies `Fair_P` and stops only where no move leaves or the class can loop violates the property, no behaviour of `C` under `Fair_P ∧ WF_vars(Next)` does. It proves the three cases of the paper proof, the divergence of case (i) by pigeonhole over `X`, and the lasso of a fair behaviour on a finite system (`lasso_of_behaviour`); `ExampleFair.eventually_flag` discharges every hypothesis on a cone with a silent cycle. It covers the flat case only (not Definition 5's cases 2 and 3), and the argument (iii) is proved: a cell whose steps follow an acyclic write graph changes finitely often along every behaviour (`proofs/lean/RdraIsh/Monotone.lean::finite_changes_of_behaviour`), and when every cell of `S` does so along the silent edges no block has a silent cycle, so `a15` holds with no class marked (`no_silentCycle`, `a15_monotone`; `ExampleMono.eventually_flag` discharges it); for case 4 the code's one unobserved class is not (B1) on the input bytes, and its reading as the partition by input bytes is argued, and `a15_search` takes the verdict of the run's search on `R` itself, which N9's Lean proof (`Search.lean`) shows decides that; R: every build checks that hypothesis on each explored run of `X` (`reduce/hypotheses.rs::loop_faults`: the silent edges between different states inside the unmarked classes peel to nothing), whether `divergence` set the marks or `s_cycles` left every class unmarked, and decides the cone unreduced on a fault, with a warning (a debug or `shadow` build panics); this is `hloop` literally on a flat `X` not run up to `H`, and the same check on the stored graph otherwise, where Definition 5's cases 2 and 3 stay argued; an unobserved `S` is not explored in a release build, so there `s_cycles` is checked only in a test build, on a plain run of `X` under the test's own caps, and a run past them is skipped (`partition.rs::unobserved_loops`); R also: lift and replay; D | `core/reduce/partition.rs::divergence`; `core/reduce/hypotheses.rs::loop_faults`; `core/reduce/mod.rs::Reduction::can_loop`; `core/explore/liveness.rs::ComponentRun::decide_liveness_reduced`; `core/reduce/lift.rs::lift_cycle`, `::lift_stop` | `core/reduce/partition.rs::an_unobserved_sub_lifecycle_can_loop_unless_every_var_only_rises`; `core/reduce/hypotheses.rs::tests::a_planted_loop_mark_fails_its_check`, `::a_planted_fault_makes_a_reduced_cone_decide_unreduced` (`divergence_back_edges`, `s_cycles`); `core/tests/reduce.rs::every_quotient_passes_the_checks_of_its_hypotheses`; corpus `cone_reduced_class_loops`, `cone_reduced_inner_class_loops`, `cone_reduced_loop_fair_enabled`, `cone_reduced_loop_unreached`, `cone_reduced_no_loop_capped`; `cli/tests/engine_corpus.rs::a_reduced_wf_next_run_decides_what_the_unreduced_one_decides` | On a flat reduction, relative to A8's hypotheses, of which the fields of `Lemma6Hyp` read from the code are listed under A8. The nested case and the run up to `H` (Definition 5's cases 2 and 3) and case 4's reading of the one unobserved class as the partition by input bytes are sketched. |
| A16 | A trace of `R` lifts to a path of `C` that ends in a state breaking the same check; a lift that fails is a tool error, and the lifted trace replays before it prints (N16). | formal-verification.md, "Reduction by an independent sub-lifecycle" | R, D | R: `lift` checks every lifted `T` and `S` step against the unreduced cone's moves in every build and returns `ReplayError`, and the trace replays before it prints. The lift's own look-ups are faults too: `NotInitial` for a start with no class or no initial S state and for an S path up to `H` that starts elsewhere; `NoMove` for a rise or a class move's target it cannot find, for an S action that is not the cone's, and for a class move with no S path to its target (`reduce/partition.rs::Succ::shortest_path` returns `None`); `Length` for a mismatched group of the inner lift. The `expect`s and `panic!`s left are listed in [Panics left on a lift's way](#panics-left-on-a-lifts-way); D | `core/reduce/lift.rs::lift`; `core/reduce/partition.rs::Succ::shortest_path` | `core/tests/reduce.rs::a_violation_behind_the_sub_lifecycle_lifts_to_a_longer_real_trace`, `::a_lifted_step_that_is_no_move_of_the_cone_is_a_replay_error` (the `is_move` check of a lifted step), `::a_lift_from_no_initial_state_is_a_replay_error` (a start with no class), `::a_class_move_with_no_s_path_is_a_replay_error` (a forged move back to the class just left) | No test reaches `lift`'s other look-up faults: a class with no initial S state, an S path up to `H` that starts elsewhere, a rise or a target with no block, an S action that is not the cone's, and `Length`. The lifted start is built from the run's initial state and an initial S state of its class, and is not checked against the unreduced cone's initial states (`explore/graph.rs::ComponentRun::lifted_path` replays only the reduced path before it lifts). The panics of [Panics left on a lift's way](#panics-left-on-a-lifts-way) stop the run. |
| A17 | The quotient cache is transparent: a cached quotient equals a fresh one (the group is part of the key). | `core/reduce/cache.rs` (module doc) | M, D | M (twin `Twin::CacheHit`); D | `core/reduce/cache.rs::QuotientCache` | `core/reduce/cache.rs::an_equal_sub_lifecycle_takes_the_stored_quotient`, `::a_sub_lifecycle_that_differs_in_one_input_computes_its_own`; `core/tests/reduce.rs::reduced_cones_through_a_shared_cache_are_fresh_ones` | |
| A18 | Instance symmetry: under the group `G_s` that permutes the non-absent slots of each class and relabels owners, state equivalence is a bisimulation; exploring canonical representatives reaches exactly the orbits of `Reach(C)`, so every verdict equals the plain run's. | instance-symmetry.md, "The group", "Soundness per check"; formal-verification.md, "Instance symmetry" | P-full, M, D | P-full given (G1)–(G6) of Theorem A18 in the form `breaks.rs::scan` checks them ((G1) with no row id in an `In`, (G2) with no data column written from the slot of a class that moves), `WF` (a term reads, and an effect writes, a row of its own entity) and the two facts of the builder it names (`Built`: a `New` parameter is created by its action, an `Earlier` one carries a run-mark conjunct), on the states whose absent rows hold base values, which hold every image of a reachable state. The proof in [cones.md](../theory/cones.md#instance-symmetry) follows the Lean proof step by step and names the theorem of each step; its step on least BFS depth, (G6) outside guards, and a guard's read of a `New` row's owner whose parent's class does not move (which the Lean model's `G6` excludes, while the scan allows it) are paper only. `Group::apply` does not relabel an owner on an absent row, so (G6) needs that no read a verdict depends on reads an absent row's owner. For action guards the scan enforces it: it leaves the parent's class out when a guard reads, before the step, the key of the row the step creates (`Rule::AbsentOwner`). For invariants, `after` assertions, properties and `can_reach` the evaluators' presence gates enforce it: an invariant binds and quantifies present slots; an `after` assertion reads the state after its step, where each parameter's row is present or deleted; a property's trigger is `present ∧ link`, its other reads are gated by it or by presence after the step, and its goal is read after a trigger, where a row is present or deleted, since a slot never becomes absent again; `can_reach` gates its link and goal by presence. D for both: the witness test and `g6.rs`. M (Kani `apply_of_a_composition_is_the_composition_of_applies`: `apply` is a group action, on a 2-slot parent and a 2-slot child only; twins `Twin::Canonical` with `G_s` up to 1,296 elements, `Twin::OrbitRun` and `Twin::ReducedOrbitRun` under 200,000 states; Lean `proofs/lean/RdraIsh/Symmetry.lean::a18_dbsb`: `≈` is a bisimulation given that steps commute with every `g ∈ G_s` and keep `g` in `G_t`; Lean `proofs/lean/RdraIsh/SymActions.lean::step_comm` derives that commutation on the states whose absent rows hold base values, over a slot-level model of the cone system in which (G1)–(G4) and (G6) are conditions on the syntax of guards and effects as `breaks.rs::scan` states them, (G5) is the semantics of a delete, and the builder's two facts are hypotheses; `symOn` is the `SymOn` that A23 assumes, and `a18_holds_iff` gives an invariant `G_s` preserves the plain verdict on the canonical quotient; `proofs/lean/RdraIsh/SymActSearch.lean::orbit_verdict` gives the orbit search the plain liveness verdict from the same hypotheses; `proofs/lean/RdraIsh/SymActModel.lean::local_` reads the model as the action model of Lemmas 5 and 6 and derives their hypothesis `Local` from its syntax; `SymActionsExample.lean::off_W_counterexample` is a step from a state outside that set, under every hypothesis, that does not commute); D (corpus rule 8) | `core/explore/symmetry/mod.rs::Group::apply`, `::compose`, `Rule`; `core/explore/symmetry/breaks.rs::scan`; `core/explore/symmetry/canon.rs::Canon::canonical`; `core/explore/orbit.rs::OrbitRun`; `core/lasso.rs::triggers`; `core/expr.rs::find_violation`; `core/reach.rs::reach_failure` | `core/explore/symmetry/tests.rs::one_class_of_three_slots_with_a_three_valued_column`, `::a_parent_and_an_owner_child` (main only); `core/explore/symmetry/witnesses.rs::absent_owner_has_a_witness`; `core/explore/symmetry/g6.rs::reads_outside_guards_meet_no_absent_owner` (shops and orders at 2 instances, reading the order's owner through a cross `forbidden` with `.along`, an `at_most` invariant, two `after` assertions, and `leads_to`, `eventually`, `can_reach`, `stable` and `precedes` with `.along`; each gated read is the same on every reachable state of the plain run and its canonical form, the `eventually` and `leads_to` verdicts and the safety findings of the plain run and the run up to symmetry agree, and a goal given a read of an absent row's key by hand differs only on rows never created, which no verdict reads); corpus `sym_*` cases | No Kani harness proves `canonical(apply(g, s)) = canonical(s)`; that invariance rests on the exhaustive unit test and the `Canonical` twin. The `g6.rs` test has no `always` or `unchanged`. The scan does not check the builder's two facts; `restrict_to` keeps both, and the extended sub-lifecycle's `Earlier { step: 0 }` parameters carry no run-mark conjunct. The Lean model has action guards only: invariants, `after` assertions and properties are not in it, so (G6) outside guards is not derived there, and `a18_holds_iff` takes the invariance of the invariant under `G_s` as a hypothesis. The Lean model reads an action's effects in list order, where `each_successor` applies the deletes and then the choices last. It has no owner `Transition`, which the scan leaves out with `OwnerChoice`, and it covers neither the reduced cone nor the extended sub-lifecycle (A22). That the model's `act`, hypotheses and effects are `Group::apply`, `breaks.rs::scan` and `each_successor` is read from the code; the model checks no data domain sizes and has no `null` data values. `WF` is read from the code. The Lean model's `G6` forbids every guard read of a `New` row's owner, where the scan forbids it only when the parent's class moves; the remaining case is argued on paper. Each class is reached at its least depth by a paper step only. |
| A19 | A deleted row keeps its keys in every engine, so the group relabels a deleted row's key with its parent (`DeletedRowOwner`). | instance-symmetry.md, "The group" | P-full, D | P-full (the case (G5) removes); D (corpus, one planted bug) | `core/explore/moves.rs` (delete effect); `core/prove/encode.rs`; `emit/tla/module.rs` | corpus `sym_deleted_row_owner` | |
| A20 | The symmetry scan never refuses a model: leaving a class out of `G_s` gives a subgroup, which keeps soundness and loses only reduction. So a rule that leaves a class out without need costs states, never a verdict; soundness needs only that no needed rule is missing. | instance-symmetry.md, "The group" | P-full, M, D | P-full given A18: the group of the classes the scan keeps is closed under the group operations, preserved by its own elements and kept along steps, which are the hypotheses of `Sym.sub` ([cones.md](../theory/cones.md#instance-symmetry), "Leaving a class out"), and over the slot model (G1)–(G4) of `Hyp` hold of a class that does not move and `G6` does not mention the group, so A18's hypotheses hold of the smaller group; M (Lean `proofs/lean/RdraIsh/Symmetry.lean::Sym.sub`: a family of subgroups of `G_s` closed under the group operations, preserved by its own elements and kept along steps is again a symmetry, the commutation carrying over; it takes the closure facts as hypotheses, which the paper step shows of the group the scan keeps); D: witnesses in `witnesses.rs` for 8 of the 11 `Rule` variants, the seven scan rules `OrderedId`, `IdAgainstLiteral`, `OwnerLiteral`, `OwnerChoice`, `UnlinkedCreate`, `AbsentOwner`, `MixedIds`, and `PinnedStart`, whose witness shows it guards the traces' replay (without it the run stores the canonical form of an initial state, which need not be one, and the lifted counterexample fails the replay with `NotInitial`); `JointTooLarge` is a cost cap, shown so by `joint_too_large_is_a_cost_cap` (release jobs); `Observed:obs` and `Observed:input` in `engine_corpus.rs` under `shadow`; the six `Reduced` reasons in `KEPT` (`Reduced:S` follows from `S var`, which `reduce/mod.rs::Reduction::fixed_why` asserts) and the other 4 `Observed` reasons only add an entity to what `Group::leaving_out` leaves out, which gives a subgroup. A missing rule is caught by the always-on `up_to_h` assertion, the `reduced_orbit_run` twin, the bisimulation checks of `assert_up_to_g_t`, and the twins of A22 | `core/explore/symmetry/breaks.rs`; `core/explore/symmetry/witnesses.rs`; `core/explore/symmetry/snapshot.rs::KEPT`; `core/reduce/mod.rs::fixed_why` | `core/explore/symmetry/witnesses.rs::*_has_a_witness` (among them `pinned_start_has_a_witness`); `core/explore/symmetry/snapshot.rs::the_group_rules_of_every_cone_are_the_snapshot` (release jobs) | Whether each rule is needed is open: eleven reasons have no witness of their own, the seven of `Reduced` and four of the six of `Observed` (`KEPT` lists ten, since `Reduced:S` follows from `S var`), and the snapshot shows only that dropping one changes which entities a cone's group leaves out. That `Hyp` holds when a class is marked as not moving is read off its definition; no Lean theorem states it. |
| A21 | Past `BRANCH_CAP` (4,096) a state is stored as some member of its orbit; verdicts stay sound and the printed count becomes an upper bound (`about`). | instance-symmetry.md | P-full, P-sketch, M, D | P-full given A18 for checks invariant under `G_s` (soundness needs only a member of the orbit); P-full for liveness given A23's hypotheses, since A23 takes any member of the orbit chosen as a function of the state (Canon), which `Canon::canonical` is past the cap; P-sketch for deadlocks, which are entity-cone findings and rest on A23's sketch for them; `stable`, `unchanged` and `precedes` never run up to symmetry; M for the invariants (Lean `proofs/lean/RdraIsh/Symmetry.lean::quotient_holds_iff`: a run that stores `π(s)(s)` for any `π(s) ∈ G_s` gives the plain run's verdict of every invariant whose value is the same on `s` and `g(s)`, from A18's commutation as a hypothesis, which `proofs/lean/RdraIsh/SymActions.lean::a18_holds_iff` derives over the slot-level model of A18; for liveness, `proofs/lean/RdraIsh/SymActSearch.lean::orbit_verdict` gives the orbit search the plain verdict over that model, for any `can` in the orbit); D | `core/explore/symmetry/mod.rs::BRANCH_CAP` | `core/explore/symmetry/tests.rs::past_the_branch_cap_a_state_is_cut_and_stays_in_its_orbit`; `core/explore/symmetry/witnesses.rs::past_the_branch_cap_the_verdict_stands` (release jobs) | Deadlocks past the cap rest on the entity-cone findings of A23, which have no theorem. That `Canon::canonical` returns a member of the orbit as a function of the state past the cap, keeping the first branch, is read from the code. |
| A22 | Symmetry composes with the reduction: `S` up to a subgroup `H` that fixes `obs` and the inputs gives classes that are unions of `H`-orbits; `T` up to `G_T` on a reduced cone; a silent orbit self-edge marks its class as one that can loop. | instance-symmetry.md, the section on composing symmetry with the sub-lifecycle reduction | P-sketch, R, M, D | P-sketch; R (an `assert!` that `H` moves no read variable); M (twins `Twin::SUpToH`, `Twin::ReducedOrbitRun`); D | `core/reduce/partition.rs::Classes::up_to_h`, `::divergence`; `core/reduce/mod.rs::Reduction::fixed_why` | `core/tests/reduce.rs::reduced_cones_up_to_h_decide_what_the_cones_decide`, `::a_reduced_cone_up_to_g_t_stores_fewer_states_and_lifts_its_trace`; `cli/tests/engine_corpus.rs::each_h_reason_a_case_shows_has_a_witness` (shadow builds only) | |
| A23 | Liveness and entity-cone findings up to symmetry decide what the plain run decides. `stable`, `unchanged` and `precedes` never run up to symmetry (`core/explore/properties.rs::TransitionSystem::decide_property` runs them on the plain cone), and an `always` runs up to symmetry only on a cone the reduction leaves whole (`cli/src/verify/jobs.rs::PropertyJob::symmetric`). | instance-symmetry.md, "Phases" (phase 4); formal-verification.md, "Instance symmetry" | P-full, P-sketch, M, D | P-full for liveness relative to (Sym) (A18's commutation on a set `W` that holds the initial states and is closed under steps and every `G_s`), (SymLive), (LiveEqui), (Canon), (Finite) (finitely many reachable states, every `G_s` finite), a finite list of the fair instances enabled at each state, and a complete graph ([cones.md](../theory/cones.md#instance-symmetry), Theorem A23 and Theorem A23 for the search: step by step after the Lean proofs, naming the theorem of each step; (SymLive) and (Canon) read from the code, (LiveEqui) argued on paper from (G1), which the scan applies to properties, and the presence gates of (G6)); P-sketch for the entity-cone findings, where the step that the deadlock criterion reads the same on a (state, slot) pair and its image is not derived; M: a Lean proof (`proofs/lean/RdraIsh/SymmetryLiveness.lean`, `a23`) that steps commuting with every `g ∈ G_s` and keeping `g` in `G_t` (on every state, or with `a23_on` on a set that holds the initial states and is closed under steps and every `G_s`), (G3), fair instances and bindings moved by the group, a trigger and stop rule the group preserves, a goal it preserves on the states at or after a trigger, finitely many reachable states and every `G_s` finite make the verdict on the annotated quotient the plain one: a liveness property holds for every binding under `Fair_P` and the stop rule iff no annotated lasso of the canonical quotient that is thread-fair and stutters forever only where the rule allows violates it for any binding. It proves that the group is constant on a quotient cycle, that some power `d > 0` of the cycle's composite permutation acts as the identity and `d` copies close the concrete lasso (`lasso_closure`), and that thread fairness on a cycle is the rule per thread piece (`threadFair_iff_pieces`); `can` may be any member of the orbit, so it holds past `BRANCH_CAP`. `ExampleSymLive` discharges every hypothesis on two interchangeable instances whose quotient cycle closes after two copies; `ExampleReachOnly` shows that commutation on reachable states alone does not suffice. `SearchSym.lean` proves the analogue of N9 for the orbit search, given also a list of the fair instances enabled at each state: the search on (stored state, binding) pairs accepts iff some binding has a fair violating behaviour (`oAccepts_iff`), iff N9's search on the plain system accepts for some binding (`oAccepts_iff_accepts`), so its verdict is the quotient side of `a23` (`not_oAccepts_iff_lassos`); with the commutation only on a set `W` as above (`SymOn`) the same holds of the search on the system restricted to `W`, whose stored states lie in `W` (`liveHolds_iff_not_oAccepts_on`, `oAccepts_on_iff_accepts`, `not_oAccepts_on_iff_lassos`), and `proofs/lean/RdraIsh/SymActSearch.lean::orbit_verdict` composes it with A18 from (G1)–(G6), so no commutation outside `W` is assumed; a thread edge inside a component can be walked back (`TEdge.reverse`), and the code's union-find over the nodes of enabled instances is the criterion over pieces (`threadsCovered_iff`). It covers liveness, not the entity-cone findings; M (twins `Twin::OrbitLiveness`, which checks the quotient only, and `Twin::OrbitStateModels`); D (corpus rule 8) | `core/explore/orbit/liveness.rs::OrbitRun::decide_liveness`; `core/explore/orbit/findings.rs::state_model_findings` | `core/tests/reduce.rs::an_entity_cone_up_to_symmetry_decides_what_its_plain_run_left_open` (main only) | The orbit search is proved by its acceptance criterion, not its code: Tarjan's components, N10's cut, `is_real` as the criterion's `t ≠ r`, the permutation stored per edge as `π(t)`, the bit sets, the choice of binding and start, and the lasso the code builds are read from the code. (LiveEqui) on the code's properties is argued on paper only: the Lean model holds no properties, and `orbit_verdict` takes it as a hypothesis. The orbit search on a reduced cone and A22's run of `S` up to `H` are not modelled. The entity-cone findings (`state_model_findings`) have no theorem: that the deadlock criterion reads the same on a (state, slot) pair and its image is not derived. |
| A24 | A run stopped by a state cap or the memory limit never yields `ok`; a counterexample found before the stop is a counterexample of the full graph, so a stopped cone reports it and leaves only its other checks `not decided`; a liveness lasso on a capped graph uses only expanded states and is real. | formal-verification.md, "Memory, state caps, and threads"; engine-agreement.md, rule 7 | P-sketch, D, R | P-sketch (Proposition A24; part 3 relies on theme (c) for the parallel BFS order); D for properties: `a_capped_graph_decides_nothing_the_full_graph_contradicts` (release jobs) explores `always`, `eventually` and `leads_to` under state caps; for safety cones, `cli/tests/prove_cli.rs::a_proof_leaves_an_after_assert_and_a_state_cap_undecided` (with z3) shows a cone stopped by the memory limit and one stopped by a state cap print `not decided`, not `ok`, though z3 proves their rules; R: a counterexample a stopped run prints replays through the step relation before it prints (N16), and a liveness lasso also passes the fairness check (N11) | `cli/src/verify/bfs.rs::BfsCone::verdict`; `core/explore/liveness.rs::ComponentRun::expanded`, `::decide_liveness`; `core/reduce/partition.rs::Classes::of` | `cli/tests/engine_corpus.rs::a_capped_graph_decides_nothing_the_full_graph_contradicts` (release jobs); `cli/tests/prove_cli.rs::a_proof_leaves_an_after_assert_and_a_state_cap_undecided` (with z3) | That a counterexample a cap-stopped safety cone finds is real rests on part 1's reading, with no test (`verify_cli.rs::a_safety_cone_stops_once_every_check_on_it_has_failed` (main only) checks one from a run that stopped early once every check failed, not at a cap, against TLC's whole run). |
| A25 | Bound escalation: for a rule BFS passed at `β` and the prover left unproved, BFS runs on its cone at `n+1` and `n+2` slots per entity; a counterexample there is a reachable violation of `T_{β'}` for the larger `β'` and fails the run. A counterexample the cone cannot vouch for (free trigger or past the bound) is decided again on a cone refined up to the exact cone that counts every slot (`recheck_on`, A2). The run prints the instances of the cone that decided it, per entity (`state_findings.rs::Instances`): one number when every entity has it, else each entity's own, as `Loan=4 Member=3`, so a per-entity bound is never stated as the largest; an uncounted entity's created rows raise its own count only, and `refuted.rs` names the rows of an uncounted entity the trace creates. | formal-verification.md, "Proofs for any number of rows" (escalation) | P-full, D | P-full given A1, A8 and Corollary A28′ ([cones.md](../theory/cones.md#bound-escalation), Proposition A25: the printed counterexample is vouched for with slots, so it is a trace of `T_{β+k}`) for an exact cone; a cone with a left-out mark has its counterexample decided again (A2); D: one unit test of the bound arithmetic and two CLI tests. The engine corpus reads a per-entity count too: `cli/tests/engine_corpus.rs::Observed` keeps no count for a line such as `ok at Loan=3 Member=2 instances`, and only a count every entity of the cone has is held to a case's `bfs=` verdicts | `cli/src/verify/escalate.rs::escalate`, `::passed_at`, `::grown`, `::same_system`; `cli/src/verify/refuted.rs::refuted`; `cli/src/state_findings.rs::Instances` | `cli/src/verify/escalate.rs::tests::grown_bounds_add_to_every_count_and_stop_at_255`; `cli/tests/prove_cli.rs::escalation_reports_no_counterexample_past_the_bound_it_prints`, `::escalation_names_each_entitys_instances_under_per_entity_bounds` (both with z3) | |
| A26 | The memory limit bounds the BFS state stores (and TLC heaps), not the process; liveness, `can_reach`, the partition, lifting, and z3 run outside it. | formal-verification.md, "Memory, state caps, and threads" | none | none (a statement of scope; measured runs exceed the limit; an escalated run is bounded by min(pool left, 10 × peak) of BFS store only) | `cli/src/verify/mod.rs`; `core/explore/mod.rs::Caps` | none | The process's memory is not bounded. |
| A27 | Covered safety cones: `ok (checked in Cone_X)` for a cone whose checks, variables, kept actions, and step shapes are contained in a passing cone's: `Reach(C_small) ⊆ π(Reach(C_large))`, so the large cone's `ok` is the small one's. | cones.md, Proposition A27; D23 | P-full | P-full relative to the covering cone's `ok` (Proposition A27: Theorem A2 on the covering cone, whose `ok` rests on A8 when it is reduced; the `steps` field of `ConeSystem` holds `(guards.len(), params.len())`, which for nested cones means equal sets) | `cli/src/verify/tlc.rs::Covered::covers`; `cli/src/verify/jobs.rs::ConeSystem`; `cli/src/verify/bfs.rs` (`BfsCone::Covered`) | none specific | No test of its own. |
| A28 | Freed slots: a trace of a cone with no left-out mark that binds `c_E` new rows of each freed entity `E` lifts to `T_{β'}` with `n'_E = max(n_E, c_E)`; with every `c_E ≤ n_E` (`over_bound` empty) it is a trace of `T_β`. | cones.md, Proposition A28; state-derivation.md, "Exploration by cone of influence" | P-full, D | P-full ([cones.md](../theory/cones.md#projection-and-lifting), Proposition A28); D: the tests check that a cone that frees slots never runs out of them and that a trace past a freed cone's count is marked | `core/trace.rs::over_bound`; `core/cone.rs::cone_frees_slots` | `core/tests/cone.rs::an_entity_the_view_does_not_read_never_runs_out_of_slots`; `core/tests/trace.rs::a_trace_that_takes_more_slots_than_a_freed_cone_counts_is_marked` | |

### (b) Theorem proving with z3

The theme document is [z3.md](../theory/z3.md), which states each row below as a proposition. Every
row in this section assumes **z3 answers `unsat` only for unsatisfiable queries** (H1 of
[z3.md](../theory/z3.md#the-soundness-theorem), Theorem 1) **that the SMT text the prover prints means the `Expr` it was built from** (H2, assumed beyond
`prove/smt.rs`'s unit tests; [z3.md](../theory/z3.md#sorts-and-the-term-builder)), **and that the
prover takes each answer for the check that asked it** (H4: `Z3::exec` reads a batch's output up to an echoed marker and
`Solver::check_sat` reads its first line as the answer; see
[z3.md](../theory/z3.md#the-answer-reader)). Under
`core/prove/` only `Enc::one_of` has a Kani harness (B1), and no code has a shadow twin. `prove/encode.rs::Enc::compare`
re-implements `expr.rs::compare`, whose Kani harnesses do not reach the SMT mirror; only
reading and differential tests tie the two. Below, `Enc_n` is the z3 encoding of `T` with any
number of rows per entity, and `Enc` the encoding for every `n` at once (an uninterpreted sort
per entity).

| ID | Claim | Where | Classes | Evidence | Code | Tests | Open gap |
|---|---|---|---|---|---|---|---|
| B1 | Over-approximation: for every `n ≥ 1` and every `m` the model builds at (every literal of a rule ≤ `m`), every state of `Reach(T_{(n,m)})` and every step between two of them is a model of `Enc`. The encoding widens in three ways only: a `New` parameter takes any absent row, Int columns and `now` range over all non-negative integers (`Tick` has no guard), and a choice of every Int value from `k` becomes `c ≥ k`, with the values the choice lists below `k` kept beside it. | formal-verification.md, "Proofs for any number of rows"; `core/prove/encode.rs` (module doc) | P-sketch, M, D, A | P-sketch on `Sc(M, β)` ([z3.md](../theory/z3.md#faithfulness-of-the-encoding), Proposition 2 and Lemma U); holds for every `n` at the run's `m`; the encoding reads the build at `β`, so B1 about `T_{β'}` of D13 also assumes I1 at `β`. Its parts, read from the code: `Enc::step` and `each_successor` both apply creates, assigns and single-target transitions in effect order, then deletes, then forks, skipping forks on deleted rows; the builder records a choice of every number as such (`Effect::Choose` with `numbers`, set in `build/sets.rs`, including when `Builder::guarded_set` joins every `.when`-guarded `sets` line of one column into one choice), `build/transitions.rs::arm_guards` keeps those values one arm, so the guard is the same at every `m'`, and `Enc::one_of` encodes the choice as `c ≥ from ∨ ⋁ { c = t : t listed, value < from }`, so a `.when`-guarded `null` beside a `.when`-guarded `present` of one nullable Int is a step; post-states stay in their domains by construction; a nullable Int column at `m = 255` is the build error `DomainTooLarge`. M: the Kani harness `encode.rs::proofs::one_of_admits_what_the_step_forks_on` proves that the term `Enc::one_of` writes for a choice holds of a value `c` exactly when `each_successor` forks on `c` (`c` below the domain's size) or the choice is of every number from `from` and `c ≥ from` (`c` past it), for every non-key choice of the shape `build/sets.rs` builds: a domain of up to 4 values, `from` up to 3, any non-empty set of listed values beside the numbers, every `c` of a byte; the harness evaluates the term with an evaluator of that fragment of `prove/smt.rs`, not with z3. D: the reference interpreter checks `proved` at `m + 1` and `m + 3` on its subset (I1); `proof_equivalence` skips some comparisons (Open gap); without z3 the test is skipped unless `RDRA_REQUIRE_Z3=1`, which CI sets. A (z3) | `core/prove/encode.rs::Enc::initial`, `::step`, `::declarations`, `::in_domain`, `::one_of`, `::compare`; `core/build/sets.rs::Builder::guarded_set`; `core/build/transitions.rs::arm_guards` | `core/prove/encode.rs::proofs::one_of_admits_what_the_step_forks_on` (`cargo kani -p rdra-ish-core`); `cli/tests/proof_equivalence.rs::no_proved_rule_has_a_bounded_counterexample`, `::no_proved_rule_has_a_bounded_counterexample_in_the_corpus` (release jobs; 1 to 3 instances and `int_max + 2`); corpus `z3_int_and_now_boundary`, `z3_now_past_the_top`, `z3_guarded_present_past_the_top`, `z3_guarded_null_beside_present` (z3 `unproved`); `core/tests/reference/proofs.rs::a_rule_proved_at_one_int_range_holds_at_wider_ones`, `::a_guarded_null_beside_a_guarded_present_is_no_proof` (it fails when z3 proves a rule the reference breaks at `m + 1` or `m + 3`; that the reference breaks this rule holds by a reading of `interp.rs`, not by an assertion); `cli/tests/prove_cli.rs::a_guarded_null_beside_a_guarded_present_is_a_step_of_the_proof` | `Enc::compare` re-implements `expr.rs::compare` and is tied to it by reading and differential tests only. `proof_equivalence` skips 130 comparisons on the samples, the cones whose BFS passes `STATE_CAP` (300,000), and 0 on the corpus; `report_skipped` asserts the number, so a comparison that starts to be skipped fails the test. The header of `z3_guarded_null_beside_present` says BFS breaks its rule at `--int-range-max 6`, which no test runs. |
| B2 | `proved: R` ⇒ for every `n ≥ 1` and every `m` in the printed range, `T_{(n,m)} ⊨ R` (D16), under the printed scope. Follows from B1, B4, B5 and z3's soundness. Every proved line prints the claim `Claim::of` computes: "for any number of rows and every --int-range-max from L to M" (the least and greatest range at which the model builds, `int_range`) and the scope; the `--prove` help text states the same claim. Under `verify --buc`, `--without-usecase`, `--without-buc`, `--actor` or `--entity` the prover encodes the scoped system, and every proved line names the scope. Under `--entity` a build error is deferred only when no selected target's exact cone reads its entities (`select.rs::select`, `exact_target_cone(t, true)`, which keeps every action that writes into the cone; an altered action binds every entity it writes, `build/mod.rs::diag`, so an action outside that cone changes nothing the rule reads), so no deferred error reaches the encoded system of a rule it bears on. `Unproved::Initial` says the violation may lie past the run's bounds, where the bounded engines do not see it. The prover proves nothing from an unsatisfiable premise: it checks a canary candidate `false` before the rounds (`prove/mod.rs::premise`), and a premise with no state keeps the canary, proves no rule, and says why (`Unproved::Premise`). | formal-verification.md, "Proofs for any number of rows"; `cli/src/prove.rs::ProofTally::clause` | P-sketch, R, D, A | P-sketch conditional on B1 on `Sc(M, β)` ([z3.md](../theory/z3.md#the-soundness-theorem), Theorem 1) and on I1 at `β`; R (the premise check, every run); D (as B1, and the tests named here); A (z3's soundness H1, the printer H2, the answer reader H4) | `core/prove/mod.rs::prove_round`, `::premise`, `Unproved`, `Verdict`; `cli/src/prove.rs::run_proof`, `Claim::of`, `::int_range`; `cli/src/cli.rs`; `core/select.rs::select` | as B1; `cli/tests/prove_cli.rs::a_proved_line_names_the_bounds_the_proof_covers` (at 254 with a nullable Int column and 255 without; no line says "any Int value"), `::a_proof_under_a_scope_says_so_on_every_proved_line` (`--without-usecase` and `--entity`), `::a_deferred_error_never_reaches_a_proved_rule`; `core/tests/select.rs::an_error_in_a_raiser_the_plain_cone_leaves_out_blocks`; `core/tests/prove.rs::a_contradictory_premise_proves_no_rule` (an `Init` that makes the `nil` row present, which `Decl` excludes) | Nothing checks at run time that a bound `β'` in the printed range builds the same system: Lemma U of z3.md is a reading of every use of `int_max` in `core/build`, and the reference interpreter checks it on its subset (I1). |
| B3 | A `in`/`not in` set that lists every value is folded to `true`/`false` only for finite domains (Enum, Bool and their nullable forms, null-or-present, existence); Int, nullable Int and owner domains never fold, so no rule's meaning depends on `m` through folding. | state-derivation.md, "Exploration by cone of influence" (Cone) | P-full, D | P-full by reading ([z3.md](../theory/z3.md#what-is-folded-what-is-widened-and-what-is-unsupported), "Folding": `lists_whole_domain` returns `false` on `Int`, `NullableInt` and `Instance` domains, and folds only domains that do not depend on `β`); D | `core/build/preds/atoms.rs::lists_whole_domain` | corpus `z3_int_listing_whole_range`, `z3_int_listing_guard`, `z3_int_listing_past_the_top` under `proof_equivalence`'s wider-Int axis | |
| B4 | Initiation: every candidate the kept set holds is checked against `Enc::initial`, which encodes every row of every entity (created rows absent at base; pre-existing rows present under `Start::Base`, `Any`, `Identity`). | `core/prove/mod.rs` (module doc) | P-full, D | P-full from the code ([z3.md](../theory/z3.md#the-kept-set-is-inductive), Proposition 4); D | `core/prove/mod.rs::initiation`; `core/prove/encode.rs::Enc::initial` | `cli/tests/proof_equivalence.rs::no_proved_rule_has_a_bounded_counterexample`, `::no_proved_rule_has_a_bounded_counterexample_in_the_corpus` (release jobs; a consequence only: no proved rule has a bounded violation) | |
| B5 | Consecution: Houdini runs to a fixpoint; the loop exits only after a full pass over every step under the final set with no candidate falling, so the kept set is inductive relative to `Enc`. An `unknown` answer counts as a fall. With exact answers the first stage keeps the largest inductive subset of the wait-0 candidates, but the final kept set need not be the largest inductive subset of all candidates offered ([z3.md](../theory/z3.md#maximality), Proposition 5); only soundness depends on the kept set. | formal-verification.md, "Proofs for any number of rows"; `core/prove/mod.rs` | P-full, D | P-full from the code ([z3.md](../theory/z3.md#the-kept-set-is-inductive), Proposition 4); D | `core/prove/mod.rs::prove_round`, `::activate`, `::violated_each`, `::assume` | `cli/tests/proof_equivalence.rs::no_proved_rule_has_a_bounded_counterexample`, `::no_proved_rule_has_a_bounded_counterexample_in_the_corpus` (release jobs; a consequence only: no proved rule has a bounded violation) | |
| B6 | Step pruning: a step is checked only against candidates whose reads meet its writes. Sound if `action_writes` lists every variable the encoded step changes. | `core/prove/mod.rs` | P-full | P-full from the code ([z3.md](../theory/z3.md#step-pruning), Lemma F5) | `core/prove/mod.rs` (`breaks`); `core/cone.rs::TransitionSystem::pred_reads`, `::action_writes` | none | No test. That a step changes only `Wr(a)` is checked on the native step relation in debug and `shadow` builds only (A1). |
| B7 | A guard or write the encoding cannot state is dropped or havocked (printed as `note:`), which only adds steps; a rule it cannot state is `out of scope`, never approximated. | formal-verification.md, "Proofs for any number of rows"; `core/prove/encode.rs::Unsupported` | P-sketch, D | P-sketch; D for one case only (the unit test checks that `Enc::compare` gives `Unsupported` for a key against a nullable number) | `core/prove/encode.rs::Enc::step`, `::havoc`; `core/prove/mod.rs::prove_round` | `core/prove/encode.rs::tests::a_key_against_a_nullable_number_is_unsupported` | No test checks that such a rule is `out of scope`, that such a guard is dropped or a write havocked, or the `note:` line. |
| B8 | Counts: `Count{E, φ, ≥, k}` unrolls to `k` distinct present witnesses and is exact for `at_least(k)` with `k ≤ 8`; a count needing more is out of scope (`.at_most(7, ..)` is the largest `at_most`). | formal-verification.md, "Proofs for any number of rows"; counting-rules.md, "z3" | P-full, D | P-full ([z3.md](../theory/z3.md#predicates): `k` pairwise distinct present witnesses satisfying `φ` state `≥ k` exactly, and their negation `< k`; the other comparisons are combinations of these); D | `core/prove/encode.rs::Enc::at_least`, `::pred`, `COUNT_WITNESSES` | corpus `z3_needs_count`; `core/prove/encode.rs::tests::counts_and_negative_numbers_print_as_before` | |
| B9 | Mined lemmas (patterns, values, value pairs, child/parent tuples, sibling pairs, count lemmas, ∀∃, at most 50,000 states mined) are candidates like any other: a wrong or capped mining costs proofs, not soundness. | formal-verification.md, "Proofs for any number of rows"; `core/prove/mine.rs` (module doc) | P-full, D | P-full ([z3.md](../theory/z3.md#mined-lemmas-b9): a mined lemma is a candidate, and Proposition 4 of B4 and B5 holds for any candidate set); D (that mined lemmas prove rules, and the ∀∃ stage's order) | `core/prove/mine.rs::mine`, `::candidates`, `::exists`, `::counts`; `core/prove/mod.rs::lemmas` | `cli/tests/prove_cli.rs::an_unreachable_cti_is_ruled_out_by_a_mined_lemma`, `::a_forall_exists_lemma_joins_only_past_the_fine_ones` | No test plants a false or capped mined lemma, so the soundness half rests on B4 and B5. |
| B10 | IC3-style clauses from a CTI: a clause of at most 40 facts, generalized while it holds initially and is inductive relative to the hypotheses, then re-checked by Houdini, so it adds no trust. | formal-verification.md, "Proofs for any number of rows" | P-full, D | P-full ([z3.md](../theory/z3.md#cti-clauses): a surviving clause is a plain candidate of the next round, so Proposition 4 covers it); D | `core/prove/strengthen.rs::generalize`, `LITERAL_CAP`; `core/prove/mod.rs::STRENGTHEN_ROUNDS` | `cli/tests/prove_cli.rs::a_lemma_from_the_cti_proves_the_rule_and_reads_as_model_text`; corpus `z3_cti_true_rule` | |
| B11 | A `.lemma()` rule adds nothing to the trust base: it is a rule candidate in the same Houdini set, kept only if the whole set is inductive. It gets no BFS cone of its own and is never escalated; a cone of another selected rule that reads its variables checks it, and a violation there is `refuted` and fails the run, as `check` fails it. | language-reference.md, "Cross-entity constraints" (`.lemma()`), "Marks in a lemma"; formal-verification.md, "Proofs for any number of rows" | P-sketch, D, none written | P-sketch ([z3.md](../theory/z3.md#user-lemmas-lemma)); D for true lemmas; D for a false `.lemma()` dropped and one refuted by a shared cone; D that a lemma gets no TLC cone of its own; none written for "never escalated", which holds by construction (`run_proof` leaves lemmas out of `ProofRun::unproved`, the set `escalate` reads) | `core/prove/mod.rs::lemma_invariants`; `cli/src/prove.rs::Marks::of`; `cli/src/verify/mod.rs` (target filter) | `cli/tests/prove_cli.rs::a_lemma_the_model_marks_is_reported_apart_from_the_rules`, `::a_false_lemma_is_refuted_where_bfs_reaches_it_and_dropped_where_not`, `::verify_runs_no_tlc_cone_for_a_lemma` (with TLC), `::insurance_claim_ops_is_proved_without_its_lemma` (main only; it shows the sample needs no user lemma, not how a `.lemma()` is treated); corpus `z3_lemma`, `z3_lemma_run_mark` | The sketch covers only its central case, a false `.lemma()` being dropped. |
| B12 | `unknown` never becomes `proved`: a check that gives up makes the candidate fall, and a CTI that z3 cannot exhibit leaves `Unproved::Unknown`. z3 errors are tool errors, never verdicts. | `core/prove/mod.rs` | P-full, D | P-full from the code, given H4 ([z3.md](../theory/z3.md#the-kept-set-is-inductive), Proposition 4: `unknown` never keeps a candidate); D (headroom tests, which check the limits' margin, not that an `unknown` drops a candidate; `a_certificate_whose_round_fails_changes_no_proof` checks that an `unknown` refuses a certificate) | `core/prove/mod.rs::prove_round` (`Fall::GaveUp`, `Fall::Unknown`); `core/prove/solver.rs` (`Z3::exec`, `Solver::check_sat`) | `cli/tests/proof_equivalence.rs::only_a_check_that_ran_out_fails_the_headroom_check`, `::a_certificate_whose_round_fails_changes_no_proof` | H4 is assumed: no test drives `Z3::exec` or `Solver::check_sat` ([z3.md](../theory/z3.md#the-answer-reader)). |
| B13 | rlimit determinism: with the resource limit (20,000,000 per rule, a `.lemma()` rule included, 2,000,000 per lemma the prover adds) set per check, z3 5.1.0 gives the same answers on the same number of solvers (`RDRA_JOBS`) on every machine and run (across solver counts, B14). Any other version prints a note. | formal-verification.md, "Proofs for any number of rows" (budgets); cli-reference.md | A, D | A (z3 determinism); D (headroom tests, which check the limits' margin; the note) | `core/prove/mod.rs::ProveOptions::default`, `VERIFIED_Z3`, `unverified_z3_note` | `cli/tests/proof_equivalence.rs::the_headroom_check_fails_when_a_check_runs_out` (release jobs), `::no_proved_rule_has_a_bounded_counterexample` (release jobs; the headroom of the default limits on every sample); `core/prove/mod.rs::tests::a_verified_z3_prints_no_note`, `::any_other_z3_version_prints_a_note`, `::an_unreadable_z3_version_prints_a_note`; `cli/tests/prove_cli.rs::an_unverified_z3_version_is_a_note` (unix only) | No test compares runs on two machines. |
| B14 | The verdicts do not depend on the number of solvers (`RDRA_JOBS`). | formal-verification.md, "Proofs for any number of rows" | D, A | D (1 against 3 solvers on a subset of samples, and 2 on the formal-verification samples; offline, the same digest on 40 roots at 1, 2 and 3 solvers and at 1 and 4, measurements recorded in the descriptions of pull requests #445 and #434, which no test repeats); A | `core/prove/mod.rs::violated_each` | `cli/tests/proof_equivalence.rs::a_proof_is_the_same_on_any_number_of_solvers` (release jobs) | Independence from the solver count is empirical. |
| B15 | Certificate cache: a cached certificate is used only after initiation and one consecution round re-check with every answer `unsat` and every rule in the kept set, so a stale or forged certificate cannot make a rule `proved`; with an equal digest the output is byte-identical. | formal-verification.md, "Proofs for any number of rows"; cli-reference.md | P-full, A | P-full (soundness, [z3.md](../theory/z3.md#the-certificate-cache), Proposition 7); A (sameness: z3 determinism and a hand-bumped digest tag) | `core/prove/mod.rs::digest`, `::reusable`; `cli/src/prove.rs` (`certificate::read`) | `cli/tests/proof_equivalence.rs::a_certificate_is_reused_only_when_it_rechecks_and_changes_no_proof` (release jobs), `::a_certificate_whose_round_fails_changes_no_proof`; `cli/tests/prove_cli.rs::a_cached_certificate_changes_nothing_a_run_prints` | |
| B16 | Interaction with BFS: a rule a safety cone of the same run violates is `refuted` and not tried; refuting it changes no other verdict. Only a violation the cone vouches for, or that the refined or exact cone found, refutes a rule (`BfsCone::violations`, A2). A proof can turn a cone stopped at the memory limit into `ok` when every rule in it is proved (not for `after` assertions or `--max-states`). | formal-verification.md, "Proofs for any number of rows" | P-sketch, D | P-sketch under exact z3 answers and a real refutation ([z3.md](../theory/z3.md#interaction-with-bfs), Proposition 8, Corollary 6); the second part rests on B2 at the run's bound; D: the differential test asserts that no check of either proof ran out of its limit (`canceled`) | `cli/src/verify/refuted.rs::refuted`; `cli/src/verify/bfs.rs::decide_by_proof`, `::BfsCone::decided_by_proof`; `core/prove/mod.rs::prove` | `cli/tests/proof_equivalence.rs::a_refuted_rule_left_out_changes_no_other_verdict` (release jobs; the samples and corpus cases with a refuted rule); `cli/tests/prove_cli.rs::a_rule_bfs_refutes_is_not_tried_and_the_others_are`, `::a_proof_decides_a_cone_that_stopped_at_the_memory_limit`, `::a_proof_leaves_an_after_assert_and_a_state_cap_undecided` | The checks z3 gives up on by itself (Houdini rounds without MBQI) are counted, not asserted, so "every z3 answer is exact" is checked only for the limit; no `proved` becomes wrong either way. A wrong `proved` (a fault of B1) would turn a cone stopped at the memory limit into a wrong `ok`, and no run-time check sees it. |
| B17 | A CTI is a step from a state where the rule, every proved rule, and every kept lemma hold (not necessarily reachable); the CTI line says so. | formal-verification.md, "Proofs for any number of rows" | none written, D | none written (by construction); D (one test) | `core/prove/mod.rs::prove_round` (CTI stage), `read_cti`, `::assume` | `cli/tests/prove_cli.rs::a_rule_that_breaks_at_two_rows_is_unproved_with_its_step` | |
| B18 | Scope of `proved`: state rules only; not `after` assertions, temporal properties, deadlocks, or witnesses. | formal-verification.md, "Proofs for any number of rows" | none written | none written (by construction: `run_proof` takes only `Target::Invariant`) | `cli/src/prove.rs::run_proof` | `cli/tests/prove_cli.rs::a_proof_leaves_an_after_assert_and_a_state_cap_undecided` comes closest (an `after` assertion its cone checks stays `not decided` though z3 proves the cone's rules) | No test asserts it directly. |

### (c) The native verifier

The native verifier decides each check on one explored graph `G` (a complete or stopped BFS
run of a cone, reduced cone, or quotient). In this section `G` is complete unless a row says
otherwise, and `T` is the system `G` explores.

| ID | Claim | Where | Classes | Evidence | Code | Tests | Open gap |
|---|---|---|---|---|---|---|---|
| N1 | BFS reachability: a complete run stores exactly `Reach(T)`, each state with a parent edge on a shortest path from `I`. Every edge number goes through `core/explore/edges.rs::edge_number`, which panics past the last `u32` below `INITIAL` instead of wrapping, except the orbit self-edge index of `Bfs::take`, whose own `u32::try_from(..).expect` panics past `u32::MAX` and does not exclude `INITIAL`. | native-temporal-verification.md, "One graph, three kinds of question"; [native.md](../theory/native.md#explicit-state-exploration) | P-sketch, M, D | P-sketch (native.md; that `moves_into` enumerates D13's steps rests on D and twin `Twin::EachEnabled`, which in debug and `shadow` builds compares `explore/moves.rs::each_planned` with `explore/moves/reference.rs::enabled_bindings`); D | `core/explore/bfs.rs::Bfs::go`, `::explore`, `::admit_packed`; `core/explore/moves.rs::initial_states`; `core/explore/edges.rs::edge_number` | `cli/tests/engine_equivalence.rs::a_bfs_and_tlc_reach_the_same_states_and_verdicts` (release jobs; with TLC); `core/explore/edges.rs::an_edge_number_past_u32_panics` | |
| N2 | Packed store: every column domain has at most 256 values (the build rejects a larger one as `DomainTooLarge`, `build/vars.rs`, so no value past a byte wraps), and every state fits the bits a packed state addresses (the build rejects a wider one as `StateTooWide`, `build/mod.rs::too_wide`; bit offsets are `u32`). Then pack then unpack is the identity on states whose values lie in their domains, the word path equals the cell path, arena and segment pushes read back what they stored, and the table's tag match and growth rule are as stated. | engine-agreement.md, "Kani harnesses" | M, R, D | M (Kani `a_number_reads_back_and_never_wraps` and `a_variant_reads_back_and_never_wraps`: every value of an Int, Enum or Bool domain, with or without null, that `Domain::admits` admits reads back and no two share one, at every `int_max` and every declared count, and past the gate a number or a declared value has no value rather than a wrapped one; `an_owner_value_names_its_slot`; that the build gates every column with `admits` rests on the R tests below; `pack_then_unpack_is_the_state`, `a_patch_is_the_pack`, `a_cell_across_a_word_reads_back` (4 or 11 cells, widths up to 8 bits), `a_push_takes_the_bytes_bytes_with_said_and_reads_back`, `locate_numbers_rows_in_order`, `rows_take_what_bytes_with_said_and_never_move`, `segments_hold_every_row_and_no_more_than_a_doubling_vec`, `lanes_are_exactly_the_equal_bytes`, `growth_keeps_room_for_one_more`; twin `Twin::Packed`); R (the two build errors); D (past bit 65,535, and the domain errors under `check` and `verify` and the TLA+ export) | `core/explore/{packing,arena,segments,table}.rs`; `core/explore/packing.rs::offsets`; `core/types.rs::Domain::of_number`, `Domain::of_variant`, `Domain::of_slot`, `Domain::column_values`, `Domain::admits`; `core/build/vars.rs::column_var`; `core/build/mod.rs::too_wide` | the Kani harnesses; `core/explore/packing.rs::tests::a_state_past_bit_65535_packs_patches_and_reads_back`; `core/build/mod.rs::a_state_past_the_packed_width_is_a_build_error` (`too_wide` one bit past a width, and a model past bit 65,535 that builds without `StateTooWide`); `cli/tests/verify_cli.rs::a_state_past_bit_65535_keeps_its_verdicts` (main only; two models at 255 instances, each held to the verdict the test writes in); `cli/tests/check_cli.rs::an_enum_column_takes_no_more_values_than_a_state_holds` (a nullable Enum of 256 values and a non-null Enum of 257 are `DomainTooLarge` errors that stop `verify` before any engine runs, while 255 and 256 still build and break the rule; the TLA+ export refuses the same columns); corpus `width_nullable_int_top`, `width_nullable_enum_256`; `core/tests/reference/mod.rs::a_value_past_a_byte_is_not_wrapped` | No harness inserts into the table and looks a row up. |
| N3 | The memory charge is deterministic, monotone, and never overflows; it counts the stores only. | formal-verification.md, "Memory, state caps, and threads" | M, D | M (Kani `store_bytes_never_overflows`; `store_bytes_only_grows`, which proves each term of `store_bytes` monotone, not `store_bytes` itself, whose growth follows as a saturating sum of products of non-negative terms; `store_bytes_is_at_most_the_model_before`, without the lists of segments, which can make the model count more at a power of two; `a_block_rounds_up_to_16`); D | `core/explore/store.rs` | `core/explore/tests.rs::store_bytes_models_the_store_a_run_counts` (main only) | |
| N4 | Parallel exploration computes the one-thread run: the same states, numbering, parents, edges, findings, and stop point for any number of threads. A wall-clock budget stop is the exception, by design. | native-bfs-parallel.md, "Why the run is the one-thread run"; formal-verification.md, "Memory, state caps, and threads" | P-sketch, M, D | P-sketch (induction over moves, [native.md](../theory/native.md#parallel-exploration), every step read in the code, `moves_of_into` included; the pool beside other runs: the first run's stop marks it, and every rerun of the safety and property passes goes alone under an explicit room, `Caps::alone`, as `targets.rs::TransitionSystem::run_cones` does); M (twins `Twin::SameAs`, `Twin::DenseBinding`, `Twin::Seen`, `Twin::SeenGrowth` on every `shadow` run, on the path a release build takes; `Twin::Hit`, `Twin::Recent` only under `RDRA_ISH_FORCE_PROBES`, on code release builds do not run; CI's shadow job checks per run that the release path's twins compare and the probe twins do not); D | `core/explore/bfs.rs::Bfs::take`, `::resolve`, `::admit_packed`; `cli/src/verify/bfs.rs::bfs_cones`; `crates/rdra-ish-core/src/memory.rs::MemoryPool` | `core/explore/tests.rs::a_run_is_the_same_on_any_number_of_threads` (main only; 1, 2, 3, 4, 7, 17 threads), `::a_run_up_to_symmetry_is_the_same_on_any_number_of_threads`, `::a_run_stops_at_the_memory_limit_at_the_same_state_on_any_number_of_threads` (main only); `cli/tests/memory_cli.rs::safety_and_property_passes_stop_where_they_stop_one_at_a_time` (main only) | The loom model, test jitter, and "probe finds every key" harness that native-bfs-parallel.md describes are not built, and no workflow runs Miri. |
| N5 | Safety: on `G`, a state rule (D16) or `always(x)` (D19) is violated iff some stored state violates it, and an `after` assertion (D17) iff some move of its step from a stored state violates it, except moves with `s' = s`, which are not checked (D17, as in TLA+: `[][(\A p : UC(p) => post')]_vars`; `Bfs::take` continues before `check_after` on `Probe::Loop`); the reported trace is a shortest path. | native-temporal-verification.md, "One graph, three kinds of question"; [native.md](../theory/native.md#safety) | P-full, D | P-full (given N1); D for state rules and `after`; for `always`, verdicts only (`cli/tests/verify_cli.rs` and the corpus check `always` verdicts; `cli/tests/engine_corpus.rs::a_capped_graph_decides_nothing_the_full_graph_contradicts` (release jobs) decides each corpus `always` through it and compares capped graphs with the full one) | `core/explore/properties.rs::ComponentRun::unsafe_state`; `core/expr.rs::TransitionSystem::find_violation`; `core/explore/bfs.rs::Bfs::take` | `core/tests/trace.rs::a_violated_rule_reports_its_shortest_trace_once` (a state rule that three patterns break is reported once, with the shortest trace, Place then Pay); `core/tests/vacuous.rs::an_assert_after_a_step_that_only_stutters_holds_vacuously` (an `after` assertion whose step only stutters is not checked, at one and two instances, and one whose step moves is, and fails) | No test checks an `always` trace from `ComponentRun::decide_property` against an expected one. |
| N6 | `stable`, `unchanged`: violated iff some edge of `G` breaks the step condition of D19; the first breaking edge in BFS order ends a shortest trace. | language-reference.md, "Order and stability"; formal-verification.md, "Order and stability properties" | P-full, D | P-full (given N1; [native.md](../theory/native.md#step-forms-and-order-forms)); D (TLC on the same cone) | `core/lasso.rs::TransitionSystem::step_breaks`; `core/explore/properties.rs::ComponentRun::breaking_step` | `core/tests/explored.rs::stable_fails_at_the_first_step_that_leaves_it`, `::unchanged_fails_at_a_step_that_moves_a_column_under_when`, `::a_step_that_deletes_the_row_breaks_nothing` | |
| N7 | `precedes(p, q)`: violated iff `G` has a path from an initial state through states where `trig ∧ p` never holds to a state where `trig ∧ q` holds (D19). | language-reference.md, "Order and stability" | P-full, D | P-full (given N1; native.md); D (TLC checks the latch) | `core/explore/properties.rs::ComponentRun::unlatched_state` | `core/tests/explored.rs::precedes_reads_the_latch_of_every_earlier_state`, `::precedes_along_reads_only_linked_rows` | |
| N8 | `can_reach(g)`: decided per binding by a backward pass from `pres ∧ g` through `pres` states (D22); decided only on a complete graph, which an `assert!` checks in every build. | formal-verification.md, "Recoverability on a cone" | P-full, D | P-full (given N1; [native.md](../theory/native.md#recoverability)); D (unit tests; a naive oracle, `core/tests/oracle.rs` (main only), which explores the whole system on its own (its own map from `initial_states` and `moves`) and decides `can_reach` by a forward search from every state, on the corpus cases and formal-verification samples that build without an error and have at most 3,000 states (`MOST_STATES`), at 1 and 2 instances; it agrees with `reach_failure` and with the cone `verify` decides on, and fails unless it compared at least 100 `can_reach` bindings, 10 of them failing; it compares verdicts, and on the whole system that the reported state fails) | `core/reach.rs::TransitionSystem::reach_failure`; `core/targets.rs::TransitionSystem::verdict` | `core/tests/reach.rs::a_cycle_with_no_way_out_fails_where_no_deadlock_does`, `::the_cone_decides_what_the_whole_system_decides`, `::a_cone_stopped_at_the_state_cap_leaves_the_property_undecided`; `core/tests/oracle.rs::can_reach_and_deadlocks_agree_with_a_naive_oracle` (main only) | TLC cannot state `can_reach`. The oracle shares the step relation and the evaluator. That the reported state is the first in BFS order rests on the proof alone. The entity cones up to symmetry are not covered. |
| N9 | Liveness by fair components: under fairness `F` (D20), `eventually`/`leads_to` fails for binding `b` iff a state of `G` with `trig_b ∧ cause_b` and `¬goal_b` reaches, through `¬goal_b` states, either a stop (`Plain`: no fair instance enabled; `WfNext`: no move to another state) or a strongly connected set of `¬goal_b` states of size ≥ 2 in which every fair instance enabled at all of its states is taken inside it. This is the fair-SCC check (Lichtenstein–Pnueli, Emerson–Lei) for weak fairness. A step with `s' = s` neither enables nor takes an instance (D20, as `⟨A⟩_vars` reads it). A goal is read on a slot whatever its existence (D21), so deleting a row meets a goal its base values satisfy, in both engines; `.present()` on `eventually` / `leads_to` conjoins every bound row's presence with the trigger and the goal, lowered by the build into the goal predicate (`build/preds/temporal.rs::goal`), so every engine reads one predicate and the cone is unchanged; a deleted slot is never reused, so a delete before the goal leaves it unmet for good. Any other form takes no `.present()` (`PresentOnNonGoalProperty`), and the build hints `GoalMetByDelete` where a plain goal holds on the base values of a binder some use case deletes. | native-temporal-verification.md, "Liveness by fair components"; [native.md](../theory/native.md#the-check); language-reference.md | P-full, M, D | P-full ((⇐) given N1; (⇒) given N1 and D20's clause that a step with `s' = s` takes no instance, assumption τ of [native.md](../theory/native.md#the-fairness-model); the proof names the theorem of `Search.lean` that proves each step); M: a Lean proof (`proofs/lean/RdraIsh/Search.lean`, `accepts_iff`, `liveHolds_iff_not_accepts`) that, on a system whose reachable states lie in a list, the check as `avoid` computes it (components as mutual reachability by `E_N`, fair components, stops under the stop rule `True`, `¬ Moves` or a looping class) accepts iff some behaviour satisfies `Fair_P`, stays forever only where the stop rule holds, and violates the property, with τ read as D20's taken; `proofs/lean/RdraIsh/SearchSym.lean::oAccepts_on_iff_accepts`: the orbit search accepts iff this check accepts on the plain system for some binding; D (TLC per run); D (the on-demand mutation run, T15) | `core/explore/liveness.rs::ComponentRun::decide_liveness`, `::liveness_search`, `::fair_steps`, `::avoid`; `core/explore/graph.rs::Adjacency::components`; `core/lasso.rs::TransitionSystem::fails_at`; `core/build/preds/temporal.rs::goal`; `crates/rdra-ish-core/src/analysis/property.rs` | `core/tests/liveness.rs::a_stop_is_fair_only_where_nothing_fair_can_run`, `::a_cycle_that_passes_a_disabled_state_is_fair`, `::an_instance_whose_step_leaves_the_cycle_is_still_enabled`, `::a_self_loop_is_neither_a_step_nor_enabling`, `::a_deleted_row_still_owes_its_goal`, `::present_keeps_a_delete_from_meeting_the_goal`, `::present_adds_nothing_to_the_property_cone`; corpus `presence_goal_present_modifier` (plain `ok`, `.present()` counterexample on both engines), `presence_goal_present_both_ok` | The Lean proof states the acceptance criterion on a complete graph, not `avoid`'s code: Tarjan's components, the bit sets, the choice of binding and start, and N10's cut at `expanded` are read from the code ([proofs/lean/README.md](../../proofs/lean/README.md#the-liveness-searches)). |
| N10 | On a stopped graph, a liveness counterexample uses only expanded states and is a counterexample of the full graph; no counterexample gives `not decided`. | native-temporal-verification.md; engine-agreement.md, rule 7; native.md | P-full, D | P-full (given N9's (⇐) direction, which does not need assumption τ, applied to a fair component of the recorded edges, whose states are all expanded and which need not be a maximal component of `T`'s graph: [native.md](../theory/native.md#the-check)); D (every cap) | `core/explore/liveness.rs::ComponentRun::expanded`, `::avoid` | `core/tests/liveness.rs::a_capped_graph_decides_only_a_counterexample`; `cli/tests/engine_corpus.rs::a_capped_graph_decides_nothing_the_full_graph_contradicts` (release jobs) | No Lean theorem covers it: `Search.lean` takes the graph complete, and the cut at `expanded` (`avoid`'s `open` and the `s < expanded` test) is read from the code. |
| N11 | Self-certifying counterexamples: every native liveness lasso replays through the step relation, fails the formula, and passes a fairness check that reads the moves, not the search's data, before it prints; otherwise `error[LassoFault]`. | formal-verification.md, "What `verify` checks"; native-temporal-verification.md, "Counterexamples" | R | R | `core/lasso.rs::TransitionSystem::replay_lasso`, `::explain_property`, `::unfair_loop`; `cli/src/verify/native.rs` | `core/tests/liveness.rs::the_fairness_check_rejects_an_unfair_lasso`; `core/tests/lasso.rs::a_back_edge_no_move_takes_does_not_replay`, `::always_fails_where_its_condition_is_false`, `::always_reads_an_instance_only_while_it_is_present` (the last two explain a hand-built lasso with `explain_property`) | The checks share code with the search they certify: `unfair_loop` and `avoid` both use `moves` and `Property::fair_instance`, and `explain_property` and `liveness_search` both use `holds_bound` and `triggers`, so a fault there passes both. |
| N12 | Liveness up to instance symmetry decides what the plain run decides: the search runs on pairs of a stored state and a binding carried along each edge's permutation, and a lasso closes within the order of the loop's permutation (at most 10,000 copies, else `LassoFault::Unclosed`). | instance-symmetry.md, "Phases" (phase 4) | M, D | M (twin `Twin::OrbitLiveness`, which compares with the plain run's `avoid`, not with an independent fair-SCC check); D (rule 8) | `core/explore/orbit/liveness.rs::OrbitRun::decide_liveness` | engine-agreement.md, rule 8 | |
| N13 | A counterexample of a symmetric run lifts to concrete slots; a lift that fails is a tool error, and the lifted trace replays before it prints (N16). | formal-verification.md, "Instance symmetry" | R | R: a symmetric path replays through the step relation before it prints (`core/explore/orbit.rs::replays`), and a failure is a `LassoFault` or `TraceFault`, a tool error; the private `core/explore/orbit.rs::lift_path` returns `NoMove` for a stored edge that is no move of its source, and the lasso lift `core/explore/orbit/liveness.rs::Product::lift` returns `NoMove` for a stored edge that is not the move at its place or whose permutation does not take that move to its target, reported as `LassoFault::Replay` | `core/explore/orbit.rs::lift` (a free function), `::lift_path`, `OrbitRun::lift_path`, `::replays`; `core/explore/orbit/liveness.rs::Product::lift` | `core/tests/reduce.rs::a_reduced_cone_up_to_g_t_stores_fewer_states_and_lifts_its_trace`; `core/explore/tests.rs::a_path_that_does_not_replay_is_a_fault` (`orbit::replays` returns `NoMove` for a step to a state its action does not reach); `core/explore/symmetry/witnesses.rs::pinned_start_has_a_witness` (with `PinnedStart` dropped, the lifted counterexample starts at no initial state, and `run_orbits_annotating`'s replay makes it a `LiftFault` with `LassoFault::Replay(NotInitial)`); `core/explore/orbit/liveness.rs::an_edge_its_permutation_does_not_explain_is_a_replay_error` | The second half of `a_path_that_does_not_replay_is_a_fault`, which would build a `LiftFault` with `faulted`, runs only when the run finds a state-rule violation, and its model, `SHOP`, has no rule, so it never runs. |
| N14 | Deadlock: a reachable state where a present slot holds an open value of a lifecycle column (D25: not named by `@end`, and some `transitions` arm leads from it to another value; a value whose only way out is a deletion, or that no arm leaves, is an end state and never a deadlock) and no path that keeps the slot present changes the column or deletes the slot; reported at the first such state in BFS order. | state-derivation.md, "Deadlocks"; [semantics.md, D25](../theory/semantics.md#d25) | P-full, D | P-full (given N1; [native.md](../theory/native.md#deadlocks)); D (TLC alone, one-sided, T6; the naive oracle of `core/tests/oracle.rs` (main only, N8), against the whole system, `state_model_findings`, and the deadlocks `check` prints on every entity whose cone frees no slots, on the systems of N8, and fails unless it compared at least 50 deadlock values, 5 of them stuck; it compares stuck against not stuck, not the state reported, and picks the open values with the checker's own filter (`exits`, `has_way_on`)) | `core/findings.rs::TransitionSystem::deadlocks`, `::can_leave`, `::leave_columns` | `core/tests/deadlock.rs::every_deadlock_trace_ends_at_its_state`, `::an_end_value_is_no_deadlock`, `::a_loop_that_always_runs_does_not_hide_a_stuck_instance`; `core/tests/oracle.rs::can_reach_and_deadlocks_agree_with_a_naive_oracle` (main only) | No test has a value whose only way out is a delete. TLC's deadlock (no successor) is another reading (T6). |
| N15 | Caps and the budget give `not decided`, never `ok`; the run prints `result: not decided`, exits 0 with a warning, and exits 1 under `--deny-undecided`. A stopped native run is not decided and is compared with nothing (`Answer::outcome` maps `Stopped` to `None`); the explored-form message names the time budget; the `OK:` line after `result: not decided` stays, as compatibility.md documents. | formal-verification.md, "Temporal properties on a cone", "Memory, state caps, and threads" | P-sketch, D | P-sketch ([native.md](../theory/native.md#memory-accounting-and-the-stop-rule), N15: every reader returns `Undecided` on a stopped graph, and `cli/src/verify/end.rs` maps it to `result: not decided`); D | `cli/src/verify/end.rs::End::result`, `::finish`, `::capped`; `cli/src/verify/native.rs::Answer::outcome` | `cli/tests/verify_cli.rs::a_property_the_cone_cannot_decide_runs_on_the_whole_model_under_the_budget`, `::a_property_run_stopped_at_the_cap_is_no_disagreement_with_tlc`; `cli/tests/memory_cli.rs::verify_leaves_a_cone_past_its_bfs_share_undecided` (main only) | |
| N16 | Every printed counterexample replays through the step relation (formal-verification.md: "Every native counterexample replays … One that does not is a tool bug"). Every lifted and stored trace replays in every build, and a failure is a tool error (`LiftFault`, `LassoFault`, `TraceFault`), never a counterexample. | formal-verification.md, "What `verify` checks"; [native.md](../theory/native.md#counterexamples-what-is-checked-before-printing) | R | R: every printed trace, liveness or safety, plain, reduced or symmetric, replays through the step relation of its run before it prints: state rules and `after` assertions, and `always`, `stable`, `unchanged`, `precedes`, `can_reach`, deadlocks and witnesses through `ComponentRun::lifted_path`, which calls `core/explore/orbit.rs::replays` and then lifts (`core/reduce/lift.rs::lift`, A16; `core/explore/orbit.rs::lift_path`, N13). A reduced violation whose lifted last state does not violate is `LiftFault` carrying `LassoFault::Satisfies`, printed as `error[LassoFault]`, counted with the tool errors, with no trace. `decide_property` checks in every build that its path breaks the property at its last state, and on a reduced system reads an `always` again on the lifted last state (`PropertyVerdict::Fault`). The `expect`s and `panic!`s left are listed in [Panics left on a lift's way](#panics-left-on-a-lifts-way) with the reason each holds or the assumption it rests on | `core/trace.rs`; `core/explore/graph.rs::ComponentRun::lifted_path`; `core/explore/orbit.rs::replays`; `core/reduce/lift.rs::lift`; `core/explore/mod.rs::ComponentRun::trace_violations`; `core/explore/properties.rs::ComponentRun::decide_property` | as N11; `core/explore/tests.rs::a_path_that_does_not_replay_is_a_fault` (`orbit::replays` only), `::a_violation_that_does_not_lift_is_a_lift_fault` (the `LiftFault` that `faulted` makes through `lifted(None, ..)`: its name, sites and severity, and no trace); `cli/src/verify/bfs.rs::a_violation_that_does_not_lift_makes_the_cone_a_tool_error`; `core/tests/reduce.rs::a_class_move_with_no_s_path_is_a_replay_error`; `cli/src/verify/end.rs::a_trace_fault_is_a_tool_error`; the tests of A16 and N13 | No test drives `ComponentRun::trace_violations` with a stored path that fails to replay. The panics left stop the command; none can print a trace that does not replay. |
| N17 | The output does not depend on `RDRA_JOBS` (cones at once and threads per cone). | formal-verification.md, "Memory, state caps, and threads" | D | D (as N4; `cli/tests/memory_cli.rs::cones_side_by_side_stop_where_they_stop_one_at_a_time` (main only)) | `cli/src/verify/mod.rs`; `core/explore/bfs.rs` | `cli/tests/memory_cli.rs::a_stopped_entity_cone_leaves_its_room_on_any_number_of_threads` (main only), `::safety_and_property_passes_stop_where_they_stop_one_at_a_time` (main only) | As N4. |
| N18 | Vacuity is reported only for a rule or `after` assertion its complete cone run does not violate, when no stored state meets a premise; it never changes a verdict (D18). An `after` premise counts only steps with `s' ≠ s`, so an assertion whose last step only ever stutters warns `Vacuous_AfterAssert_*` (`UnmetPremise::OnlyStutters`, `Ends::moved`). | state-derivation.md, "Rules that hold vacuously"; [native.md](../theory/native.md#vacuity) | P-sketch, D | P-sketch ([native.md](../theory/native.md#vacuity): on a plain run from N1 and N2); the step to representatives on reduced and symmetric runs is theme (a); D (one test) | `core/targets.rs::TransitionSystem::vacuity`, `::unmet_of_rule`, `::unmet_of_assert` | `core/tests/vacuous.rs::an_assert_after_a_step_that_only_stutters_holds_vacuously` | |

#### Panics left on a lift's way

A16, N13 and N16 make every failure of a lift's checks a tool error. The non-test code of
`reduce/lift.rs`, `reduce/partition.rs::Succ` and
`explore/{orbit, orbit/liveness, orbit/findings, liveness, properties, graph, mod}.rs` keeps
the `expect`s, `panic!`s and `unreachable!`s below, each a panic in a release build. A
failure of any of them stops the command; none can print a trace that does not replay. They
are unchecked internal invariants, each with the reason it holds or the assumption it rests
on.

*In the lifts.*

- `reduce/lift.rs::lift_stop`'s `reduction()`, a precondition of its caller
  `decide_liveness_reduced`.
- A lifted lasso's state list is nonempty, since it starts with the lifted start.
- The cycle search of `lift_stop`'s own walk graph: a strongly connected component of two or
  more states has a cycle through its entry, and every state the search reaches has a
  parent.
- `explore/orbit.rs::run_orbits_annotating`'s `steps.last()` for an `after` violation:
  `explore/orbit.rs::lift` appends the violating step to the stored path, a reduced system
  keeps an `after` assertion only when the action of each of its steps is a T action
  (`reduce/mod.rs::drop_unreadable_checks`), and `reduce/lift.rs::lift` lifts a T step as
  exactly that step or returns an error, so the lifted path is not empty whatever path is
  lifted.
- The deadlock trace of `explore/orbit/findings.rs::deadlocks` (`trace_mut()`), which holds
  because `findings.rs::deadlock` always builds a `StateDiag::Deadlock`, which has a trace.
- `explore/mod.rs::faulted`'s `unreachable!`, whose callers (`run_orbits_annotating`,
  `lifted`, `trace_violations`) pass it only the diagnostic of a run's state-rule or `after`
  violation, or a `LiftFault` it made.

*In the searches before the lift.*

- In `explore/orbit/liveness.rs`: `Product::new`'s permutations, a precondition of
  `OrbitRun::decide_liveness` (a run from `run_annotated_orbits`); the look-ups of the pair
  search (`lasso` finds each BFS parent edge among its source's edges and each `toward` step
  among the pair's real edges; `thread_walk` finds a covering edge in a component the search
  marked fair; `walk_in` joins two pairs of one strongly connected component); and
  `threads_covered`'s look-up of an enabled instance, which asks only for the instance of a
  real edge of the state or for an entry of the state's enabled list, a list built from
  exactly those instances.
- In `explore/liveness.rs`, their counterparts on a plain run: `edge_between` finds the edge
  a `toward` step came from, since `toward` follows the predecessors of the adjacency built
  from the run's own edges; and `fair_loop` calls `walk_in` only inside a component `avoid`
  marked as a fair cycle (two or more states, expanded whole, and every instance enabled in
  all its states taken by one of its edges), so each instance not yet covered is disabled at
  one of its states or taken by one of its edges, the strongly connected component reaches
  it, and neither `walk_in`'s `panic!` nor `fair_loop`'s `expect` that the walk is not empty
  fires.

*Preconditions of the caller.*

- `liveness_search` (under `decide_liveness` and `decide_liveness_reduced`) and
  `Product::search` (under `OrbitRun::decide_liveness`) panic on a form that is not
  `eventually` or `leads_to`; `cli/src/verify/native.rs` reaches them only for a property
  whose `LtlKind::is_liveness` holds (`Answer::of`, `native_property`'s branch up to
  symmetry, and `decide_liveness_reduced` through `PropertyJob::wf`, which
  `cli/src/verify/jobs.rs` sets for liveness cones only).
- `ComponentRun::decide_property` returns `None` for exactly those two forms, and its callers
  never pass them: `Answer::of` sends them to `decide_liveness`, and
  `TransitionSystem::decide_property` runs, from `cli/src/verify/bfs.rs::explored_properties`,
  only for the forms `LtlKind::is_explored` names.
- `TransitionSystem::decide_property` finds the property in its cone's system:
  `cone.rs::property_seed` holds what the property reads, both arms build the system through
  `restrict.rs::restrict_to` (`reduced_cone` by `cone_system`), which keeps a property whose
  reads it holds, and the reduction, whose seed is that one, observes those reads and so
  `drop_unreadable_checks` keeps it (an `always` has no fairness).
- `decide_liveness_reduced`'s `reduction()`, called only when `cone.is_reduced()`;
  `OrbitRun::perms` in `explore/orbit/findings.rs`, since both callers of
  `targets.rs::read_orbits` pass a run of `run_annotated_orbits`; and
  `explore/mod.rs::bfs_on`'s group, which it reads only when asked to annotate, and only
  `run_orbits_annotating` asks, with a group.

*Outside those files, on a lift's way.*

- `reduce/lift.rs::lift` rebuilds the reduction's extended sub-lifecycle and explores it
  again. `reduce/extended.rs::Extended::rebuild` calls `Extended::build`, which keeps
  `expect`s (`"in S ∪ inputs"`), as do its helpers `fold_param` (`"the group's first"`) and
  `fold_linked` (`unreachable!("a linked fold")`, `"a linked fold's guard holds its node"`), and the
  code of `restrict.rs` that `build` reaches (`drop_params`, `renumber_by`, `Remap::e`,
  `Remap::v`). `bind_stub_params` keeps none. Then `sub.run` and `run_orbits_fixing` explore
  the extended sub-lifecycle under the inner reduction, which reaches
  `reduce/mod.rs::TransitionSystem::moves_of_into`'s
  `expect("a reduced state holds a representative")` and its `panic!` on a T step's rise,
  and `explore/moves.rs::TransitionSystem::initial_states`'s
  `expect("an initial S state has a class")`, which `lift` also reaches directly through
  `sub.initial_states()`. None of these reads the lifted path: each is about the cone, its
  split, its folds and the inner reduction, and each held when the reduction was built,
  since `reduce/mod.rs::TransitionSystem::reduced_with` ran `Extended::of`, the same `build`,
  on the cone, and `reduce/partition.rs::Classes::of_graph` read the classes off a complete
  run of that sub-lifecycle under that inner reduction. So they hold in the lift if
  `rebuild` reproduces that build. `rebuild` passes the folds group by group where `of`
  passed the split's folds in their own order, and runs on the cone after
  `drop_unreadable_checks` dropped some of its checks; the first fold of each group is the
  same in either order, but two linked folds of one action can add their params in the other
  order, and that no premise of these `expect`s changes is not checked.
- `explore/orbit.rs::lift_path` calls `explore/symmetry/canon.rs::Canon::canonical`, whose
  `split` keeps `expect("as many places as rows")`, which holds by counting: a tie of `k`
  rows has `k` places, and a placement (an injection, `explore/symmetry/canon.rs::injections`) gives `j` of
  them to the tie's `j` named rows, which leaves `k − j` places for its `k − j` other rows.
- `lasso.rs::TransitionSystem::replay_lasso`, `::explain_property` and `::unfair_loop` take
  `states.last()` of a replay, which holds its start since
  `trace.rs::TransitionSystem::replay` returns `ReplayError::Empty` for a path with no state;
  `explain_property` also indexes the back edge's target, which `replay_lasso` looked up
  with `get`.
- Two more hold whatever path is lifted: `trace.rs::TransitionSystem::base_state`'s `expect`
  (reached from `initial_states`), since it asks `state_from` for every var's base and so
  always gets a state, and `explore/moves.rs::TransitionSystem::each_successor`'s
  `expect("tick without a clock")`, since the build makes the `Tick` action only when it
  declared `now` (`build/vars.rs::Builder::tick`); that every restriction that keeps
  `Tick` keeps `now` is not argued.

Beyond these the list does not enumerate the step relation, the BFS, the state store and
the group code that the runs of `sub` go through, as every run does; their release panics
are those of N1 to N4 ([native.md](../theory/native.md)), not the lift's. Left out as bookkeeping:
conversions of a count to `u32` or `usize`, the look-up of an entry inserted just before
(`"just added"`, `"just read"`), and likewise `Succ::shortest_path`'s `parent[&at]` (every
state its search queues, other than `from`, has its parent recorded before it is queued;
the twin of `lift_stop`'s `"the walk's parent"`) and `Product::new`'s `key_index[..]` (each
key it indexes was inserted when the key was made), indexing into the tables a run, a
reduction or a layout built (`blocks`, `ext.acts`, a run's states, `comp`) by indices from
the same run, the `assert!`s that a caller passes caps that keep the edges or does not read
a released run, and the `shadow`-only and `debug_assert!` checks. The lift also indexes by
the path's own states and actions (`reduce/mod.rs::Reduction::key`,
`reduce/mod.rs::Reduction::write` and `reduce/mod.rs::Reduction::stages` on a step's
successor, `ext.acts` by an action of the inner lift): those are in range for a path of the
system's own state layout and action ids, which every caller passes (stored states and
steps of the run), not for any sequence of bytes.

### (d) Agreement with TLC

Every row in this section rests on an assumption no test can remove: **TLC (1.7.4, 1.8.0) is
correct**. The TLA+ printer (`emit/tla/mod.rs::print`, `emit/tla/module.rs`,
`emit/tla/pred.rs`) prints from the same `TransitionSystem` value BFS explores, so agreement
with TLC tests BFS against a second reading of `T`. It cannot test the step from the model
to `T` (claim I1), which only the hand-written corpus headers check, and the reference
interpreter on its subset. [tlc-agreement.md](../theory/tlc-agreement.md) states each row as a
proposition and tags its evidence; its CI evidence is about the devShell's TLC, which
`flake.nix` pins to the 1.8.0 pre-release on JDK 25.

| ID | Claim | Where | Classes | Evidence | Code | Tests | Open gap |
|---|---|---|---|---|---|---|---|
| T1 | The TLA+ module printed for a cone denotes the same transition system as the cone: same initial states, same steps, same invariants (formal-verification.md, "Mapping"). Stated as Proposition T1 in tlc-agreement.md. | formal-verification.md, "Mapping"; tlc-agreement.md, "The claim (Proposition T1)" | P-sketch, D, A | P-sketch (per construct, tlc-agreement.md; not mechanised). It rests on builder invariants: only pre-existing entities start off base; an action writes each `(var, param)` at most once, which the build enforces (`Builder::finish` fails any action that does otherwise with `ConflictingEffects`, `Action::slots_written_twice`), since natively a `Transition` chooses only when the current source has two or more targets while the printer turns the whole effect into `Post::ChooseIn` when any source has two; and no two params of one action that can bind one slot write the same variable. D: states, initial states, and edges `(from, to, action, params)` compared with `tlc -dump dot` on sample cones under 300,000 states; the comparison prints the cone with one `Next` disjunct per binding (`emit/tla/mod.rs::print_bound`, `TlaNames::bound_action`), and a reduced cone's class moves, which print no params, compare by action alone; temporal formulas and fairness by verdict only. A (TLC) | `emit/tla/mod.rs::print`, `::print_state_models`, `::print_bound`; `emit/tla/module.rs::init`, `::action`, `::effects`, `::checks`, `::fairness`, `::reduction`, `::bits`, `::rise_expr`; `emit/tla/pred.rs::Ctx::pred`; `emit/tla/names.rs::TlaNames`, `::decoder`, `::bound_action`; `core/types.rs::Action::slots_written_twice` | `cli/tests/engine_equivalence.rs::a_bfs_and_tlc_reach_the_same_states_and_verdicts`, `::tla_names_cover_every_slot`, `::bfs_and_tlc_agree_on_the_binding_of_each_edge` (two edges that differ only in params), the 36 `bfs_and_tlc_*` tests of `engine_equivalence.rs`; `emit/tla/tests.rs::a_bound_module_names_each_binding_of_an_action`; `cli/tests/engine_corpus.rs::tlc_refinement_holds_on_every_reduced_cone_of_the_corpus` (each step of the unreduced cone a step of the printed reduced module or a stutter, on the 21 reduced corpus cones); corpus `safety_created_set_beside_transitions` (a guarded `sets` on a created row's lifecycle column beside a two-arm `transitions` line: the build reports `error[ConflictingEffects_Order_Place_status]` and no engine runs) | The printed reduced step (`module.rs::bits` and `::rise_expr`) has no per-construct argument in tlc-agreement.md; it rests on the comparison of reduced sample cones and, on the corpus, the TLC refinement check. That no two params of one action that can bind one slot write one variable is assumed: neither `ConflictingEffects` nor any other check enforces it. |
| T2 | Per check: on `verify --with-tlc`, every check both engines decide gets the same verdict; `not decided` and a TLC `tool_error` are no disagreement. A disagreement prints `error[EngineDisagreement]` and the run ends `tool_error`. Every TLC trace of every job replays through BFS's step relation, whatever the native verdict, and a job with one that does not replay prints `error[EngineDisagreement]` and ends `tool_error`. TLC's `Invariant X is violated by the initial state:` form is parsed as X's violation. | engine-agreement.md, "Agreement rules" (rule 1); formal-verification.md, "Samples" | R, D | R; D | `cli/src/verify/compare.rs::disagreements`, `::unreplayed`; `cli/src/verify/state_models.rs::state_model_disagreements`; `cli/src/verify/native.rs::Answer::outcome` | `cli/tests/engine_corpus.rs::bfs_and_tlc_and_z3_agree_on_the_adversarial_corpus`; `cli/tests/verify_cli.rs::a_tlc_counterexample_that_does_not_replay_fails_the_run_as_a_tool_bug` (unix only), `::a_property_run_stopped_at_the_cap_is_no_disagreement_with_tlc`; `scripts/verify-samples.sh --with-tlc` | |
| T3 | Per run: for each of `Cone_P`, `Cone_P_Full`, `Cone_P_WF`, `RdraSpec_P` that both engines decided, the verdicts match. The runs are paired by the system each decides (`verify/jobs.rs::RunOn`: property, graph, fairness); the native `decide_liveness_reduced` run on the reduced cone is printed native-only, has no TLC run on its system, and meets TLC's `_WF` module only in the final verdict. | engine-agreement.md, rule 2; native-temporal-verification.md, "How we know it is right" | R, D | R; D (corpus rule 2 fails a native `Cone_` line without TLC's line when the header gives its checks a TLC verdict) | `cli/src/verify/compare.rs::disagreements` (`PropertyCone` arm); `cli/src/verify/jobs.rs::RunOn` | `cli/tests/engine_corpus.rs::check_case` (rule 2), `::rule_2`, `::rule_2_fails_a_tlc_line_the_header_expects_and_the_run_lacks`; `cli/tests/verify_cli.rs::a_wf_run_on_the_reduced_cone_is_named_native_only`; `cli/tests/engine_equivalence.rs::a_property_cone_decides_what_the_whole_model_decides` (release jobs; 23 `TEMPORAL_CASES`: 7 sample files, 16 inline fixtures; its native `_WF` run is always on the unreduced cone; its TLC runs pass `-lncheck final`, which `verify --with-tlc` does not, [tlc-agreement.md](../theory/tlc-agreement.md#batch-jvms)) | |
| T4 | `stable`, `unchanged`, `precedes` get their own TLC run on `Cone_P`, and on `RdraSpec_P` after a counterexample there when the cone frees slots or leaves a mark out (`property_cone::inexact`), in both engines. | engine-agreement.md, rule 1; formal-verification.md, "Order and stability properties" | D | D | `cli/src/verify/jobs.rs::explored_jobs`; `cli/src/verify/tlc.rs::run_job` (`Explored` arm); `cli/src/verify/compare.rs::disagreements` (`Explored` arm) | corpus `safety_stable_*`, `safety_precedes_latch` | |
| T5 | Sample comparison: on every cone of every sample, at 1 and 2 instances, reduced and unreduced (the unreduced cone only when its BFS completes under 300,000 states, the reduced one likewise; a cone whose unreduced BFS passes the cap is compared reduced only, listed in `VERIFY_REDUCED_ONLY`: 30 cones), BFS and TLC reach the same states and edges `(from, to, action, params)` (a reduced cone's class moves, which print no params, by action), the same state-model findings, and the same violated invariants and `after` assertions, each TLC trace replaying through BFS's step relation. Temporal properties are not compared here. These TLC runs pass `-lncheck final`, which `verify --with-tlc` does not; the `after` assertions they check are box-action properties, which the flag is assumed not to affect ([tlc-agreement.md](../theory/tlc-agreement.md#batch-jvms)). 25 cones (`VERIFY_FALLBACK`) are compared at 1 instance only. | engine-agreement.md, "The sample comparison" | D | D | `cli/tests/engine_equivalence.rs` (`VERIFY_FALLBACK`, `VERIFY_REDUCED_ONLY`) | `cli/tests/engine_equivalence.rs::a_bfs_and_tlc_reach_the_same_states_and_verdicts`, the 34 `bfs_and_tlc_agree_on_*` of `engine_equivalence.rs`, `::a_corrupted_tlc_trace_does_not_replay` | The 25 `VERIFY_FALLBACK` cones, the largest reduced ones among them, meet TLC at 1 instance only in this comparison. For the 13 of oidc-auth (eight browser-flow entity views and five invariants), `verify --with-tlc` in CI also runs at 1 instance only: it runs that root's headers (`samples/oidc-auth/buc/buc_ciba.rdra`), whose 2-instance run selects none of them, since under `--entity` a run keeps an invariant only when one of its binders is selected (`core/select.rs::TransitionSystem::select`, `target_entities`), and that line's run checks five other invariants (`Forbidden_BackchannelRequest_3` to `_5`, `MultiForbidden_LogoutNotice_4` and `Quantifier_DeviceAuthorization_1`). So oidc-auth's browser flow at 2 instances rests on the native checker alone. Of the other 12, by the code, `verify --with-tlc` runs TLC at 2 instances only on the 5 invariant and `after` cones: an entity cone gets no TLC safety job (`verify/jobs.rs::cone_jobs`), and its state models run under TLC only within 100,000 states (`verify/bfs.rs::state_model_cones`); whether an entity cone shares its variables with an invariant's cone, and so meets TLC there, is open ([tlc-agreement.md](../theory/tlc-agreement.md#verify---with-tlc-on-the-sample-roots)). |
| T6 | State models with TLC alone: on entity cones BFS explored to the end up to 100,000 states, TLC names the same states without a witness, and every deadlock TLC reports is one BFS reports (one-sided). TLC's deadlock is "no successor"; the native one is D25, and the corpus records a case where the two differ. | formal-verification.md, "State models with TLC alone"; engine-agreement.md, rule 1 | D | D | `cli/src/verify/bfs.rs::STATE_MODEL_TLC_STATES`; `cli/src/verify/state_models.rs::state_model_disagreements` | `cli/tests/engine_corpus.rs` cases (among them `cone_reduced_loop_unreached`); `cli/tests/engine_equivalence.rs::a_tlc_alone_finds_the_deadlocks_and_missing_witnesses_bfs_finds`, `::tlc_alone_misses_a_deadlock_while_its_cone_cycles` | One-sided: a native deadlock TLC does not report is not a disagreement. |
| T7 | Corpus: each of 91 cases (218 `expect` lines) states its verdict per engine from hand reasoning, and every engine meets it at the case's bounds. No case has a `known` disagreement. The corpus's `bfs=` runs at 1, 3 and 4 instances run TLC beside BFS under rules 1 to 3. The header test compares the case files with `cli/tests/engine_corpus/CASES` both ways, so a deleted or renamed case fails it. | engine-agreement.md, "The corpus", "Known disagreements" | D | D (the headers are the only check of model → `T` outside the reference interpreter's subset, I1) | `cli/tests/engine_corpus.rs::check_case` | `cli/tests/engine_corpus.rs::bfs_and_tlc_and_z3_agree_on_the_adversarial_corpus`, `::every_corpus_header_is_well_formed` | Each header checks only the model it names. |
| T8 | Every native counterexample replays through the step relation, fails its check, and (liveness) passes the fairness check before it prints; otherwise `error[LassoFault]`. A wrong `ok` has no such guard. | engine-agreement.md, rule 3; native-temporal-verification.md, "How we know it is right" | R | R (every printed trace, N16; the lifts' look-ups fault instead of panicking, A16, N13; the panics left are listed in [Panics left on a lift's way](#panics-left-on-a-lifts-way)) | `core/explore/liveness.rs::ComponentRun::decide_liveness`; `core/trace.rs` (replay) | corpus rule 3 | A wrong `ok` is not checked at run time. |
| T9 | Random models: reduced and unreduced `verify --deny-undecided` give the same exit code, findings, per-check verdicts, `result:` line, and state-model summary; under `--with-tlc` TLC disagreements surface as `EngineDisagreement`. `--stubs` draws models whose reduction binds a param on a stub (A8): a folded `S` action's `Triggered` param, a rise's `Parent`, `Existing` or `Triggered` param, and nested reductions. | engine-agreement.md, "The reduction on random models" | D | D (pull request: 522 seeds without TLC; main: 45 seeds with TLC; both: 95 `--stubs` seeds without TLC, and 30 of them in the `shadow` job with S up to `H`; the seed lists count 522, 45, 95 and 30; the `--stubs` runs fail below a floor of seeds that reach each stub shape (`--min-stub-fold`, `--min-stub-rise`) and S up to `H` (`--min-up-to-h`), counted from `RDRA_ISH_REDUCTION_SHAPE_STATS`; the counts of seeds that reduce, "284 reduced" and "33 reduced", are stated only in a `ci.yml` comment, which no test checks) | `scripts/reduction-diff.py` | `scripts/reduction-diff.py run` in `formal-verify` (pull request: `--seeds @scripts/reduction-diff-pr-seeds.txt`, no TLC; main: 45 seeds, `--with-tlc`; both: `--stubs`, and in `shadow` `--stubs --symmetry always` under `RDRA_ISH_FORCE_S_UP_TO_H`); `scripts/reduction-diff.py selfcheck` checks only the comparator, on saved outputs | Process gate ([README, Known limitations](../theory/README.md#known-limitations)). |
| T10 | Instance symmetry leaves verdicts unchanged: with `RDRA_ISH_FORCE_ORBITS=1`, the symmetric run gives the plain run's verdict on every check the plain run decides (a symmetric `not decided` there fails rule 8), at 2 instances and at each case's `bfs=` bounds. TLC never runs with `SYMMETRY`. | engine-agreement.md, rule 8; instance-symmetry.md, "TLC and engine agreement" | D | D (native against native) | `core/explore/orbit.rs`; `cli/tests/engine_corpus.rs::rule_8` | rule-8 loop in `cli/tests/engine_corpus.rs::check_case`; `cli/tests/engine_corpus.rs::rule_8_fails_a_check_symmetry_leaves_undecided` | |
| T11 | Batch JVMs give the results of separate TLC processes. Bears on the tests only: `verify --with-tlc` uses no batch JVM. | engine-agreement.md, "Batch JVMs" | D | D ([tlc-agreement.md, "Batch JVMs"](../theory/tlc-agreement.md#batch-jvms): on main, CI's `tlc-batch-both` job runs `engine_equivalence` with every TLC job routed to a batch JVM, the cones of at most 5,000 states, the state-model runs and a property cone's runs, again as a process, failing on any difference of exit, log, parsed run, invariant scan or graph; larger cones and whole-model runs are processes and never compared; every such run passes `-lncheck final`, which `verify --with-tlc` does not; on pull requests CI's `RDRA_REQUIRE_TLC_BATCH=1` requires that some job ran in a batch JVM and that none was lost for a reason other than the time limit (`assert_batch_ran`); by hand, the `#[ignore]`d replay) | `cli/src/tlc_batch.rs`; `nix/tlc-batch/RdraTlcBatch.java` | `cli/tests/engine_equivalence.rs` under `RDRA_TLC_BATCH=both` (`tlc_run` calls `assert_same_run`; CI's `tlc-batch-both`, main only); `recorded_tlc_runs_are_the_same_in_a_batch_jvm` (ignored). Fallback only, with a fake TLC that never answers, so no batch result is compared: `a_job_past_the_time_limit_runs_as_a_process_and_is_no_fault`, `a_job_that_exits_the_batch_jvm_runs_as_a_process`, `a_tlc_the_batch_driver_cannot_call_runs_as_a_process` (release jobs) | Cones past 5,000 states and whole-model runs are never compared batched against a process. Process gate ([README, Known limitations](../theory/README.md#known-limitations)). |
| T12 | TLC scheduling knobs change when and how TLC runs, never a verdict (except `RDRA_TLC_HEAP=states`, which can move a run between decided and `not decided`). | tlc-scheduling.md, "Knobs", "Memory after BFS" | P-sketch, D | P-sketch (prose); D (the scheduler's unit tests, which run no TLC, and `memory_cli`'s test with a fake TLC) | `cli/src/verify/schedule.rs::target`, `::pick`; `cli/src/verify/tlc.rs::run_job`, `::run_sized` | `cli/src/verify/schedule.rs::what_runs_at_once_never_passes_the_budget_unless_it_runs_alone`; `cli/tests/memory_cli.rs::the_tlc_knobs_fix_the_workers_and_the_runs_at_once` (unix only; a fake TLC) | |
| T13 | Kani harnesses: 23 bounded proofs of leaf functions (comparison, the value encoding of the domains, store and segment arithmetic, packing, arena, table lanes and growth, the symmetry group action, the z3 encoding of a choice, the read set of a predicate). None concerns TLC, the printer, the decoder, or the comparison; the two `expr.rs` harnesses fix the native side of the null rules that `emit/tla/pred.rs::Ctx::numeric` must mirror, tied to it by tests only. | engine-agreement.md, "Kani harnesses" | M | M | `core/expr.rs`, `core/types.rs`, `core/explore/{store,segments,arena,packing,table}.rs`, `core/explore/symmetry/mod.rs`, `core/prove/encode.rs`, `core/cone.rs` (`#[cfg(kani)]`) | `cargo kani -p rdra-ish-core` | No harness covers the canonical form (A18), the printer or the comparison. |
| T14 | Shadow twins: each of 26 fast paths (`shadow.rs::TWINS`) equals its reference implementation on every call the CI runs make, below each twin's cap. | `crates/rdra-ish-core/src/shadow.rs` (module doc); CLAUDE.md, "A speed-up change" | M | M (runtime twin; CI checks each twin compared at least one call; a `shadow` build prints `shadow: compared/skipped/stopped` whenever a twin lets a call through, counting a call over the cap (`skipped`) apart from one where the run stopped (`stopped`); the corpus harness fails a case on any `skipped` call but for the runs `PAST_A_TWIN_CAP` names (one: `cone_reduced_class_loops` unreduced at 3 instances), and CI's shadow job fails on a skip in a run and twin it does not expect) | `crates/rdra-ish-core/src/shadow.rs::TWINS` | CI job `shadow`; `cli/tests/engine_corpus.rs::bfs_and_tlc_and_z3_agree_on_the_adversarial_corpus` under `--features shadow` (its helper `check_case` fails a case on a skip that `PAST_A_TWIN_CAP` does not name) | A call above a twin's cap (200,000 or 2,500,000 states; `Twin::Canonical` only for groups of at most 1,296 elements) is not compared. A twin checks two implementations, not a specification. |
| T15 | Mutation runs: `cargo mutants` over `property_cone.rs`, `verify/native.rs`, `cone.rs`, `explore/liveness.rs`, killed by the corpus with TLC and z3 off PATH, which measures the corpus headers; and over the TLA+ printer (`emit/tla/{mod,module,names,pred}.rs`), `verify/compare.rs`, `tlc_output.rs` and `tlc_replay.rs` (`.cargo/mutants-tlc.toml`, run by `mutants.yml` with `-f config=tlc`), killed by the corpus with TLC and z3, the unit tests of the TLC reader, the comparison and the TLC driver, and the `verify_cli` tests that run TLC, which measures agreement with TLC. | engine-agreement.md, "Mutation run" | D | D (started by hand, `mutants.yml`; the runs and kill rates are recorded in [tlc-agreement.md, "Mutation run"](../theory/tlc-agreement.md#mutation-run)); the planted-bug tables of engine-agreement.md are records of single runs | `.cargo/mutants.toml`, `.cargo/mutants-tlc.toml`; `.github/workflows/mutants.yml` | `cli/tests/engine_corpus.rs` | No TLC-side run has completed, so it has no kill rate. The mutation runs start only by hand (`workflow_dispatch`). Process gate ([README, Known limitations](../theory/README.md#known-limitations)). |
| T16 | BFS and TLC range Int-like columns over the same `[m+1]`. | engine-agreement.md, "Int columns and `now`"; tlc-agreement.md, "Per-construct argument" | P-sketch, D | P-sketch (one `Bounds` value feeds `Domain::Int` and `IntRange`; out-of-range literals are build errors; `Tick`'s guard is an ordinary guard); D (the corpus cases below run BFS and TLC under rules 1 to 3 on Int columns and `now` at the top of the range, at `--int-range-max` 5 and 6; the sample comparison of T5 compares every state, so every Int column's values) | `core/types.rs::Domain::Int`; `emit/tla/module.rs` | corpus `z3_int_and_now_boundary`, `z3_now_past_the_top` (`--int-range-max 6`); `cli/tests/engine_equivalence.rs::bfs_and_tlc_agree_on_a_date_sets`, `::bfs_and_tlc_agree_on_two_actions_of_one_name` (its `tick_use_case` fixture), `::a_bfs_and_tlc_reach_the_same_states_and_verdicts` (release jobs) | |

### Summary by evidence class

83 rows: 3 in (0), 28 in (a), 18 in (b), 18 in (c), 16 in (d). A row counts once per class its
Classes cell lists, so the columns add up to more than 83; a row whose parts carry different
classes (I1, A5, A6, A8, A10, A11, A15, A21, A23, B11) counts in each. Every (b) row also assumes z3 (H1), the printer
(H2) and the answer reader (H4), and every (d) row assumes TLC; the **A** column counts only
rows whose own evidence names an assumption. A test in `cli/tests/export_gate.rs` recounts
this table from the Classes cells.

| Class | (0) | (a) | (b) | (c) | (d) | Rows |
|---|---|---|---|---|---|---|
| P-full | 0 | 22 | 9 | 7 | 0 | 38 |
| P-sketch | 0 | 10 | 5 | 4 | 3 | 22 |
| M | 0 | 13 | 1 | 6 | 2 | 22 |
| D | 3 | 25 | 15 | 15 | 13 | 71 |
| R | 1 | 8 | 1 | 4 | 3 | 17 |
| A | 0 | 0 | 5 | 0 | 1 | 6 |
| none written (true by construction, no argument in a note) | 1 | 0 | 3 | 0 | 0 | 4 |
| none | 0 | 1 | 0 | 0 | 0 | 1 |

The one paper proof in the existing notes that a reader can check step by step is A15 (`_WF`
on a reduced cone, cone-soundness.md). The nine (b) rows marked `P-full` (B3, B4, B5, B6, B8,
B9, B10, B12, B15) rest on proofs in [z3.md](../theory/z3.md), not in a note. [native.md](../theory/native.md)
proves N5 to N10 and N14 (`P-full`), each given N1 and N2's preconditions (enforced by the
build errors of N2); N1, N4, N15 and N18 are `P-sketch`.
[tlc-agreement.md](../theory/tlc-agreement.md) gives T1 and T16 a per-construct `P-sketch`; no (d) row
is `P-full`. The row with no evidence at all is A26 (a scope statement). A14 (dropped checks
go to their own cone) has the argument of [cones.md](../theory/cones.md), the run-time check of the
Lemma 5e it rests on, and no test of the drop itself; A27 has the argument and no test of its
own. B6 (step pruning) is proved from the code in [z3.md](../theory/z3.md#step-pruning) and has
no test.


# Chapter 9 The Lean development

*Source: `proofs/lean/README.md`.*

## Lean proofs of the reduction and symmetry theorems

<!-- derived-from ../../docs/theory/cones.md#the-reduction-by-an-independent-sub-lifecycle -->
<!-- derived-from ../../docs/theory/cones.md#instance-symmetry -->
<!-- derived-from ../../docs/theory/cones.md#wf_varsnext-on-a-reduced-cone -->
<!-- derived-from ../../docs/theory/semantics.md#temporal-properties -->
<!-- derived-from ../../docs/theory/semantics.md#verdicts -->
<!-- derived-from ../../docs/theory/native.md#the-check -->

This directory holds a Lean 4 project. It proves parts of Theorems A8, A15, A18, A20, A21 and A23 of
[cones.md](../theory/cones.md) from their hypotheses, Lemma 5 from Definition 3,
Lemma 6 from Definition 4, and Theorem A18's commutation from (G1)–(G6). The theorems of A8 and the
others are stated over abstract transition systems; Lemmas 5 and 6 are stated over a model of
the cone by its actions
([Definition 3 and Lemma 5 over actions](#definition-3-and-lemma-5-over-actions),
[Definition 4 and Lemma 6 over actions](#definition-4-and-lemma-6-over-actions)), and A18's
commutation over a model of the cone by its slots
([Theorem A18 over slots](#theorem-a18-over-slots)). The action model of Lemmas 5 and 6 is
read off that slot model, and `Local`, a hypothesis of both lemmas, is derived from its syntax
([One model of the actions](#one-model-of-the-actions)). N9 of
[native.md](../theory/native.md#the-check), the liveness search by fair components,
and the same search on the pairs of a run up to symmetry are stated over abstract
transition systems too ([The liveness searches](#the-liveness-searches)). None of them reads
the Rust code. Two executables check what the code computes
([Checking the code's sets with Lean](#checking-the-codes-sets-with-lean)): `lemma5-check`
checks the sets of each split against Lemma 5, and `partition-check` checks each partition
of a flat run of `X` against (B1)–(B3) and the run against the shape of `X`.

### Building

```sh
scripts/check-lean-proofs.sh
```

The script runs `lake build`, then `#print axioms` on each theorem that
[CheckAxioms.lean](../../proofs/lean/CheckAxioms.lean) lists. It fails on any of these:

- a `sorry`, `admit` or `axiom` in the sources;
- a listed theorem that depends on `sorryAx`;
- a listed theorem that depends on any axiom outside Lean's standard three (`propext`,
  `Classical.choice`, `Quot.sound`).

`lake build` alone passes with a `sorry`, because Lean only warns about one. A clean build
takes about 2 seconds.

CI's `lean` job runs the script on every pull request that touches this directory, the
script or `ci.yml`, and on every run on main. The job caches `~/.elan`.

`scripts/check-lean-refs.py` runs in CI's `check` job on every pull request, prose-only
ones included, and takes under a second. It checks three things:

- every Rust name this README, the Lean doc comments and the Lean-citing lines of
  claims.md and cones.md cite (`split_at`, `Reduction::stages`, `partition.rs::divergence`)
  names a definition in `crates/`;
- every name in the first column of a table below is declared in the file of its `File`
  column;
- every such name is listed in CheckAxioms.lean.

Its docstring gives the exact rule.

The toolchain is pinned in [lean-toolchain](../../proofs/lean/lean-toolchain) (`leanprover/lean4:v4.36.0`).
elan installs it on the first build. `nix develop` provides elan; `nix shell nixpkgs#elan`
does too.

**No Mathlib.** The project imports only Lean's core, and the executable `lemma5-check` also
imports `Lean.Data.Json`, which ships with Lean. The proofs need five things:

- reachability as an inductive predicate;
- paths through a set of states;
- existentials;
- a group action given by its laws;
- finiteness, for A15's and A23's pigeonhole steps, given as a `List` that holds every
  reachable state (and, for A23, every element of each `G_s`), with core's list membership
  lemmas (`List.mem_cons`, `List.mem_flatMap`, `List.mem_map`).

All five are a few lines each. Mathlib would add its compiled cache, a download of
thousands of files, to every CI run that misses the cache. Pinning it would also tie the
toolchain to Mathlib's releases. The group laws are fields of `Sym`, so
`Equiv.Perm`, `Subgroup` and `MulAction` are not needed.

### What each theorem proves

| Lean (`RdraIsh.`) | File | Claim | Statement |
|---|---|---|---|
| `holds_of_stutterSim` | `Stutter.lean` | A8, point 1, the inclusion | A stuttering simulation from `C` to `X` that preserves the observation and covers `C`'s initial states carries every invariant over the observation from `X` to `C`. |
| `holds_iff_of_dbsb` | `Stutter.lean` | A8, consequence 1 (Theorem A) | A divergence-blind stuttering bisimulation that preserves the observation and relates the initial states both ways gives an invariant over the observation the same verdict on both sides. |
| `reachable_obs_eq_of_dbsb` | `Stutter.lean` | A8, consequence 1 | Under the same hypotheses the two sides reach the same observations. |
| `a8_dbsb` | `Reduction.lean` | A8 (Theorem B) | From Lemma 5 (a)(b), Lemma 6 (both halves), the shape of `X` (a silent edge keeps the input bytes, a rise keeps `S`) and (B1)–(B3) with a representative per block: `(t, s) ~ (t, B(x(t, s)))` is a divergence-blind stuttering bisimulation between `C` and the reduced cone `R` built by `reduced`, and it preserves `(t, obs s)`. |
| `a8_holds_iff` | `Reduction.lean` | A8, consequence 1 | An invariant over `T_v ∪ obs` holds on `C` iff it holds on `R`. |
| `holds_of_reduced` | `Reduction.lean` | A8, point 1, the inclusion | An invariant that holds on `R` holds on `C`. The proof uses only the `C ⇒ X` half of Lemma 6 (`Lemma6Fwd`), which is the half that the stub-param bug (P67) broke. |
| `a18_dbsb` | `Symmetry.lean` | A18 | `s ≈ g(s)` for `g ∈ G_s` is a bisimulation of a system with itself. The hypotheses are the conclusions of A18's first sentence. |
| `Sym.sub` | `Symmetry.lean` | A20 | A family of subgroups of `G_s` is again a symmetry when it is closed under the group operations, preserved by its own elements, and kept along steps. |
| `quotient_covers`, `quotient_sound` | `Symmetry.lean` | A18, its last sentence | The run that stores `can(t) = π(t)(t)` for every successor `t`, with `π(t) ∈ G_t`, reaches an image of every reachable state, and only images of reachable states. |
| `quotient_holds_iff` | `Symmetry.lean` | A21, for invariants | An invariant whose value is the same on `s` and `g(s)` for every `g ∈ G_s` gets the plain run's verdict on that quotient. Any member of the orbit serves, as A21 needs. |
| `LTS.wfNext_iff` | `Behaviour.lean` | D20 | `WF_vars(Next)` holds on a behaviour iff it stays forever only at states that no step leaves. |
| `recurrent_segment` | `Behaviour.lean` | A15, the pigeonhole | A sequence over the states of a list has, after any index, a segment `[n₀, m)` with `ρ m = ρ n₀` in which every later step of the sequence occurs. |
| `LTS.lasso_of_behaviour` | `Behaviour.lean` | A15, native.md's lassos | On a system whose reachable states lie in a list, a behaviour that satisfies `Fair_P`, stays forever only where `stop` holds and violates `□(trig ⇒ ◇goal)` folds into a lasso (a prefix and a cycle) with the same four properties. |
| `image_step`, `takes_fwd`, `enabled_back`, `moves_back` | `Fairness.lean` | A15, the image | The image `(t, s) ↦ (t, B(x(t, s)))` of a step of `C` is a step of `R` or a stutter; a fair instance taken in `C` is taken by the image step; one enabled at the image is enabled in `C`; a move of `R` from the image lifts to a move of `C`. |
| `silentCycle_of_divergence` | `Fairness.lean` | A15, case (i) | A behaviour of `C` that keeps its image at `(t, c)` and still changes state infinitely often gives a cycle of silent edges of `X` between different states of `c`. |
| `a15_image`, `a15_behaviour`, `a15` | `Fairness.lean` | A15 | Under A8's hypotheses (Lemma 6 both ways), finitely many reachable states of `X` and `R`, and `hloop` (every block with such a silent cycle can loop): if no lasso of `R` that satisfies `Fair_P` and stays forever only where no move leaves or the class can loop violates the property, `C` satisfies it under `Fair_P ∧ WF_vars(Next)`. `a15_image` is the map of a counterexample of `C` to one of `R`. |
| `ExampleFair.silent_cycle`, `ExampleFair.eventually_flag` | `FairnessExample.lean` | none | A cone whose one block holds a silent cycle (a toggle) and whose fair `T_a` action sets a flag: every hypothesis of `a15` is discharged and the run's verdict proved, so `eventually flag` holds under `WfNext` although `C` can toggle forever. |
| `Example.flag_after_s`, `ExampleSym.sorted_quotient` | `Example.lean` | none | Concrete systems that discharge every hypothesis of `a8_holds_iff` and of `quotient_holds_iff`, so the hypotheses are jointly satisfiable. The A8 instance is small: its partition is the identity, it has no rises and its input bytes are constant. `ExampleRise` is the A8 instance that exercises (B2), (B3) and the rises. |
| `BranchSim.stutterSim`, `BranchBisim.dbsb` | `Branching.lean` | A8, the labels | A divergence-blind branching bisimulation (a visible step is matched by silent steps through states related to the source, then one step with the same label; only a silent step may stutter) is a divergence-blind stuttering bisimulation once the labels are forgotten. |
| `edges_of_branchSim`, `edges_iff_of_branchBisim` | `Branching.lean` | A8, consequence 2 | Under such a bisimulation that keeps the observation and relates the initial states both ways, the two sides have the same reachable edges by source observation, visible label and target observation (`Edges`). |
| `path_of_stutterSim`, `canLeave_of_stutterSim`, `stuck_iff_of_dbsb`, `canLeave_all_iff_of_dbsb` | `Branching.lean` | A8, consequence 3 | For an irreflexive change relation `ch`, a reachable state from which no path takes an edge that changes the observation by `ch` (`Stuck`, D25's *stuck*) exists on one side iff on the other, and "no reachable state is stuck" has the same verdict on both. |
| `Lemma6By.toLemma6`, `Lemma6.toBy` | `ReductionLabels.lean` | A8, Lemma 6 "Parameters" | `Lemma6By stands` is Lemma 6 with the relation "the silent edge's action stands for the cone's `S_a` action, with its binding" on both halves; with `stands` always true it is `Lemma6`. |
| `class_move_lifts` | `ReductionLabels.lean` | A8 point 3, A16's statement | From a state related to `(t, c)`, a class move of `R` into `c'` lifts to a path of `C` through states related to `(t, c)` and one more step into a state related to `(t, c')`, every step an `S_a` step whose action a silent edge of `X` between the two projections stands for. |
| `branch_forward`, `branch_backward`, `t_step_back`, `a8_branching` | `ReductionLabels.lean` | A8, with labels | `~` is a divergence-blind branching bisimulation between `C` and `R` in which a `T_a` step shows its label (an action with its binding) and an `S_a` step or class move is silent, and it preserves `T_v` and `obs`. A `T_a` step of `R` is the same `T_a` step of `C` from every related state, with no silent prefix. |
| `a8_reachable_obs_iff` | `ReductionLabels.lean` | A8, consequence 1 | `C` and `R` reach the same valuations of `T_v ∪ obs` (witnesses, vacuity). |
| `a8_edges_iff`, `a8_runs_iff`, `a8_after_iff` | `ReductionLabels.lean` | A8, consequence 2 | `C` and `R` have the same reachable `T_a` edges by label and by the values of `T_v ∪ obs` before and after: a label runs on one iff on the other (`NeverRuns`, arms taken), and an `after` relation between the values before and after holds of every edge of a label on one iff on the other. |
| `a8_stuck_iff`, `a8_deadlock_free_iff` | `ReductionLabels.lean` | A8, consequence 3 | For an irreflexive change relation on `T_v ∪ obs`, `C` has a reachable stuck state with given values iff `R` has one; a silent cycle with no way out is stuck on both. |
| `a8_plain_sound`, `a8_plain_complete`, `a8_plain_iff` | `Plain.lean` | A8, consequence 4 | A liveness property `□(trig ⇒ ◇goal)` at one binding (D21: `eventually`, `leads_to`) over `T_v ∪ obs`, whose fair instances are sets of `T_a` labels, holds on `C` under `Fair_P` iff it holds on `R` under `Fair_P`. One way maps a fair violating behaviour of `C` to its image; the other lifts a fair violating behaviour of `R` to one of `C`, segment by segment. |
| `seg_exists` | `Plain.lean` | A8, consequence 4 | Behind each step of a behaviour of `R` from the image of a reachable state of `C` there is a finite segment of `C` from that state, inside the image, ending at a state whose image is the step's target, whose last step takes every fair instance the step of `R` takes. |
| `RiseThrough.riseStar`, `Lemma6Staged.toLemma6` | `Stages.lean` | A8 point 5, `Reduction::stages` | Lemma 6 with one rise per stage of `stages`, as the code states it, implies Lemma 6 with any sequence of rises. |
| `recanon_of_through`, `staged_step_iff`, `branchBisim_congr`, `a8_staged`, `a8_staged_holds_iff` | `Stages.lean` | A8 point 5, `Reduction::recanon` | Folding a rise table, filled from each reachable representative's rises (`RiseTable`), over the stages of a chain from a reachable state ends at the block of the chain's end. So `reducedStaged`, the reduced cone whose `T` move is the code's `recanon`, has the steps of `reduced` at every related state, and A8 holds for it. |
| `InnerMap.reach`, `InnerMap.reach_back`, `InnerMap.id`, `InnerMap.trans`, `InnerMap.ofIso` | `Nested.lean` | A8, nested | An inner map from `X` to the inner reduced run maps reachable states onto reachable states, silent edges to silent edges or stutters, rises to the same rises, and lifts back both ways. The identity is one (depth zero), maps compose (one more level), and an isomorphism that keeps visibility and observation is one (`X`'s states `S × I` read as an inner cone's `T' × S'`). |
| `a8_innerMap` | `Nested.lean` | A8, nested | The reduction of an inner cone whose rises are among its `T_a` actions is an inner map from it to its reduced cone, with the observation `T_v ∪ obs` of the inner cone. |
| `stableV_pullback`, `Tower.stable` | `Nested.lean` | A8, nested | A partition that satisfies (B1)–(B3) on the inner run pulls back along an inner map to one that satisfies them on `X`. A `Tower` is indexed by its partition, the innermost one read through every level, and by induction on the depth that partition is stable on `X`. |
| `stable_of_stableV`, `stableV_of_stable`, `a8_nested`, `a8_nested_holds_iff` | `Nested.lean` | A8, nested | For the partition of a tower and any representative that is a reachable state of each block (`exists_rep` gives one), A8 with labels and its invariant form hold for `C` and the reduced cone built on that partition. |
| `tower_of_inner` | `Nested.lean` | A8, nested | One level assembled: an `X` isomorphic to an inner cone that satisfies A8's hypotheses, whose rises are among its `T_a` actions and whose `T_v ∪ obs` determine `X`'s `obs` and input bytes, with a partition stable on the inner reduced cone, is a tower of depth two whose partition is that partition read through the inner reduction and the isomorphism. |
| `liveHolds_congr`, `a8_staged_edges_iff`, `a8_staged_stuck_iff`, `a8_staged_plain_iff` | `Plain.lean` | A8, consequences 2 to 4 with `recanon` | Consequences 2, 3 and 4 hold for `reducedStaged` too: two systems with the same initial states and the same steps at every state related to a state of `C` have the same verdicts under `Fair_P`. |
| `ExampleRise.merged_block`, `rise_changes_input`, `exit_through_block` | `ExampleRise.lean` | none | The instance's partition merges two reachable states `a`, `b` of `X` joined by a silent edge; a reachable `T_a` step changes the input byte and is matched by a rise; the silent exit from `a` crosses its own block through `b`, since `a` has no silent edge into the block of `c`. |
| `ExampleRise.bisim`, `c_after_flag`, `edges`, `stuck`, `flag_leads_to_c`, `bisim_staged`, `nested_flat` | `ExampleRise.lean` | none | Every hypothesis of `a8_branching`, `a8_holds_iff`, `a8_edges_iff`, `a8_stuck_iff`, `a8_plain_iff`, `a8_staged` (with a rise table and stages) and `a8_nested` (depth zero) is discharged on that instance, with an `S_a` action that reads the input. |
| `ExampleRise.inner_map` | `ExampleRise.lean` | none | `a8_innerMap` applies to the same cone, its `T_a` step taken as a rise, and the inner map it gives sends two different reachable states to one: `InnerMap` holds of a map that merges states. No instance of a tower of depth one or more is built. |
| `takes_act`, `enabled_act` | `SymmetryLiveness.lean` | A23, fairness transfer | For `g ∈ G_s`, the step `g(s) → g(s')` takes `g(f)` iff `s → s'` takes `f` (D20 with τ), and `g(f)` is enabled at `g(s)` iff `f` is enabled at `s`. |
| `ARun.lift`, `project`, `Framed.fair_iff`, `Framed.stops_iff`, `Framed.violates_iff` | `SymmetryLiveness.lean` | A23, N12 | An annotated run of the quotient (stored states `r n`, concrete successors `t n`, permutations `p n ∈ G_{t n}`, `r (n+1) = p n (t n)`) lifts through the accumulated frame `Θ (n+1) = p n · Θ n` to a behaviour of `M`, and a behaviour projects to the canonical run. Through a frame, thread fairness, the stops and the violation for the moved binding are `Fair_P`, `StopsOnly` and `Violates` of the behaviour. |
| `a23_lift`, `a23_project`, `a23_iff` | `SymmetryLiveness.lean` | A23, on runs | A property holds for every binding under `Fair_P` and a stop rule the group preserves iff no annotated run of the canonical quotient that is thread-fair and stops only where the rule allows violates it for any binding. |
| `project_lasso`, `a23` | `SymmetryLiveness.lean` | A23 | With the reachable states in a list and every `G_s` finite, the same with annotated lassos on the quotient's side: the projection of a concrete lasso is an annotated lasso. |
| `ARun.mem_const`, `order_exists`, `lasso_closure` | `SymmetryLiveness.lean` | A23, N12, lasso closure | On a quotient cycle every stored state has the same group, which holds every permutation of the cycle; an element of a finite `G_s` has a power `d > 0` that acts as the identity on states, instances and bindings at once; `d` copies of the cycle, for such a power of its composite, close the lift. |
| `threadFair_iff_pieces` | `SymmetryLiveness.lean` | A23, thread pieces | On an annotated lasso a thread is periodic, and it is fair iff within one period it takes its instance or meets a stored state where the instance is not enabled. |
| `restrict_liveHolds_iff`, `a23_on` | `SymmetryLiveness.lean` | A18, A23, reachable states | With commutation only on a set `W` that holds the initial states and is closed under steps and every `G_s` (`SymOn`), `M` restricted to `W` has a `Sym` with `M`'s verdicts, and `a23` holds for it. |
| `ExampleSymLive.plain_ok`, `ExampleSymLive.quotient_ok`, `ExampleSymLive.one_copy_open`, `ExampleSymLive.two_copies_close` | `SymmetryLivenessExample.lean` | none | Two interchangeable instances, each with a bit its fair action toggles, and the swap: every hypothesis of `a23` is discharged, the plain verdict of `eventually` is proved and `a23` gives the quotient's. The quotient cycle `(F,F) → (F,T) → (F,F)` has the swap as its composite; its lift does not close after one copy and does after two. |
| `ExampleReachOnly.reach_only_counterexample` | `SymmetryLivenessExample.lean` | none | A system whose steps commute with `G_s` on its reachable states, with a canonical form in the orbit and an invariant the group preserves, where the invariant holds on the plain run and fails on the canonical quotient, which stores an unreachable image. So commutation on reachable states is not enough. |
| `LTS.accepts_iff`, `LTS.accepts_iff_lasso`, `LTS.liveHolds_iff_not_accepts` | `Search.lean` | N9 | On a system whose reachable states lie in a list, N9's search (`Accepts`: a reachable trigger state in `N` reaches, by edges of `E_N`, a stop or a state of a fair component) accepts iff some behaviour, and iff some lasso, satisfies `Fair_P`, stays forever only where the stop rule holds, and violates `□(trig ⇒ ◇goal)`. So the property holds (D21) iff the search does not accept. |
| `LTS.lasso_of_accepts`, `LTS.FairComp.cycle`, `LTS.lasso_of_cycle` | `Search.lean` | N9, (⇐) | A fair component holds a closed walk of `E_N` that takes every edge between two of its states; the prefix, the path to the end and that walk, or a stutter at a stop, fold into a lasso with the four properties. |
| `LTS.accepts_of_behaviour` | `Search.lean` | N9, (⇒) | A fair violating behaviour reaches a segment it repeats: a stop if the segment is one state, else a fair component, since an instance enabled on the whole component is enabled forever and so taken by a step inside it. |
| `LTS.liveHolds_plain_iff`, `LTS.liveHolds_wfNext_iff`, `LTS.isStop_true_iff`, `LTS.isStop_moves_iff` | `Search.lean` | N9, the stop rules | Under `Plain` the stop rule is `True`; under `WfNext` it is `¬ Moves`, and a state with no move has no enabled instance, so the code's test of `¬ Moves` alone is the stop. |
| `a15_search` | `Search.lean` | A15 | `a15` with the run's verdict read as "N9's search on `R` under `LoopingStop` does not accept"; `ReducedRunOk` follows from it by N9 on `R`. |
| `TEdge.reverse`, `Conn.star` | `SearchSym.lean` | A23, thread pieces | Inside a strongly connected component of the pairs, a thread edge can be walked back: the edge and a path of pairs back close a walk whose composite permutation lies in one finite `G_s` and has a power that acts as the identity. So a piece of the thread graph (connected with the direction forgotten) is strongly connected. |
| `threadsCovered_iff`, `fairPComp_iff_code` | `SearchSym.lean` | A23, the code's pieces | The code's criterion, union-find over the nodes of enabled instances with a node marked when an edge of the component takes its instance or joins it to a node whose instance is not enabled, holds iff every piece of the full thread graph holds a covered node. |
| `FairPComp.cover_one`, `FairPComp.cover`, `run_of_parts`, `behaviour_of_oAccepts` | `SearchSym.lean` | A23, the search (⇐) | In an accepted component every thread has a walk to a covered node; one such walk per instance enabled at the end pair, each from where the previous ones left it, is a loop whose every round covers every thread. The stored path, the path of pairs and that loop round and round (or a stutter at a stop pair) are an annotated run that `a23_lift` carries to a fair violating behaviour of `M`. |
| `oAccepts_of_lasso`, `oAccepts_of_behaviour` | `SearchSym.lean` | A23, the search (⇒) | A fair violating lasso of `M`, seen through frames kept on stutters, is a sequence of pairs that repeats; it ends at a stop pair or in a component with a cycle, and a piece with no covered node would hold a thread of the lasso that is always enabled and never taken. |
| `oAccepts_iff`, `liveHolds_iff_not_oAccepts`, `oAccepts_iff_accepts`, `not_oAccepts_iff_lassos` | `SearchSym.lean` | A23, N9 | With `a23`'s hypotheses and a list of the fair instances enabled at each state, the orbit search accepts iff some binding has a fair violating behaviour of `M`, iff N9's search on `M` accepts for some binding; it does not accept iff no thread-fair violating annotated lasso of the canonical quotient exists, the quotient side of `a23`. |
| `quotient_restrict_W`, `restrict_rEdge_iff`, `restrict_lists` | `SearchSym.lean` | A23, N9, the closed set `W` | With commutation on a closed set `W` (`SymOn`), the stored states of the canonical quotient of `M` restricted to `W` lie in `W`, where the restriction has `M`'s real edges; the lists of reachable states and enabled instances given on `M` serve the restriction. |
| `liveHolds_iff_not_oAccepts_on`, `oAccepts_on_iff`, `oAccepts_on_iff_accepts`, `not_oAccepts_on_iff_lassos`, `LiveEqui.withStop`, `liveHolds_wfNext_iff_not_oAccepts_on` | `SearchSym.lean` | A23, N9 | The theorems above with `SymOn` in place of `Sym`: the orbit search on `M` restricted to `W` does not accept iff the property holds on `M` for every binding, iff N9's search on `M` accepts for no binding, iff no thread-fair violating annotated lasso of the restricted quotient exists. Under `WF_vars(Next)` the search's stop is `¬ Moves` of the restriction. |
| `ExampleFair.search_ok`, `ExampleFair.eventually_flag_search`, `ExampleSymLive.orbit_ok`, `ExampleSymLive.plain_search_ok` | `SearchExample.lean` | none | On the A15 and A23 examples the searches do not accept, and `a15_search` gives `C`'s verdict from the search on `R`. |

### What the hypotheses are

The proofs assume the following hypotheses and do not derive them:

- **Lemma 5 (a)(b)** (`Split`). An `S_a` step keeps `T`. A `T_a` step keeps `S` and reads
  `S` only through `obs`. `Reduction.lean` takes it as a hypothesis; over the action model,
  `split_computed` derives it from `Local`
  ([below](#definition-3-and-lemma-5-over-actions)).
- **Lemma 6** (`Lemma6Fwd`, `Lemma6`, `Lemma6By`, `Lemma6Staged`). Every initial state
  projects to an initial state of `X`. From a reachable `(t, s)`, an `S_a` step of `C` is a
  silent edge of `X` from `x(t, s) = (s, inp t)`, and the converse holds; `Lemma6By stands`
  also asks that the edge's action stands for the step's action with its binding. A `T_a`
  step is matched by a sequence of rises from `x(t, s)` to `x(t', s)`; `Lemma6Staged` asks
  for one rise per stage of `stages`. `x(t, s) ∈ Reach(X)` is not assumed: `x_reach`
  derives it. Over the action model, `lemma6_by`, `lemma6_staged` and `lemma6` derive all
  four from `Lemma6Hyp` ([below](#definition-4-and-lemma-6-over-actions)).
  Every build certifies the side of `Lemma6Staged` that `X` holds
  (`reduce/hypotheses.rs::rise_faults`, `::layout_faults`). Each rise of a block's
  representative sets the raw inputs or flips one bit slot, and `stages` lists it as exactly
  one stage; by (B3) so does every rise of the run of `X`. The engines read the input bytes
  with the raw width and length `X` lays out. That a `T_a` step of `C` has rises through every
  stage is not certified: a missing stage is a `recanon` miss in `moves_of_into`, which
  panics in every build.
- **The rise table** (`RiseTable`, for `a8_staged`). Every rise of a reachable
  representative is entered under its block and the input bytes it ends at. Every build
  certifies it on each run of `X` (`rise_faults`). The table holds exactly the rises of each
  block's first state, the code's representative, under the input bytes they end at and with
  the block they enter. This is `RiseTable` literally when `X` has no inner reduction and is
  not run up to `H`. Otherwise the check reads the inner run or the orbit graph.
- **The nested case** (`InnerMap`, `Tower`, for `a8_nested`). Each level's inner run is
  related to the level above by an inner map, which `a8_innerMap` derives from that level's
  own A8 hypotheses. The seeds `reduced_in` gets in `Classes::of` are `obs` and the rises.
  So the inner split keeps every rise in its `T_a`, and puts each `obs` variable and input of
  `X` on its `T` side or in its own `obs`. Every build certifies both
  (`reduce/hypotheses.rs::inner_faults`). This enters as the choice of observation and
  visibility of the map, and the reindexing of `S × I` as `T' × S'` as an `InnerMap.ofIso`.
- **Definition 4's shape of `X`** (`Extended`). A silent edge keeps the input bytes, and a
  rise keeps `S`. `Ext.extended` proves it of the `X` of `Restrict.lean`.
- **(B1)–(B3)** (`Stable`), on the reachable states of `X`, with
  `rep(B x) ∈ Reach(X) ∩ B x`. `Sweep.stable_of_cert` derives it from the run-time
  certificate, `partition/reference.rs::stable`, on a complete run of a flat `X`
  ([below](#the-partition-certificate-and-monotone-variables)).
- **A18's commutation** (`Sym`). This covers:
  - every `g ∈ G_s` maps a step `s → t` to the step `g(s) → g(t)`, and `g ∈ G_t`;
  - `G_{g(s)} = G_s`;
  - (G3).

  `Sym` assumes it on every state. `SymOn` assumes it on a set `W` that holds the initial
  states and is closed under steps and every `G_s`, which the quotient's stored states need;
  reachable states alone do not suffice (`ExampleReachOnly.reach_only_counterexample`).
  `SymActions.lean` derives a `SymOn` from (G1)–(G6), with `W` the states whose absent rows
  hold base values ([below](#theorem-a18-over-slots)).
- **A23's extra hypotheses** (`SymLive`, `LiveEqui`, `a23`).
  - `G` acts on fair instances and on bindings, with the composition and inverse laws, and
    moves a label of an instance to a label of the moved instance.
  - The trigger and the stop rule read the same on `s` and `g(s)` for `g ∈ G_s`, with the
    binding moved by `g`. For `WF_vars(Next)`'s stop rule, `¬ Moves`, `stop_moves_equi`
    proves it. The goal reads the same on the states `after b`, a set that holds where the
    trigger of `b` holds and that every step keeps: the states after a trigger, where (G6)
    gives it.
  - The canonical form is `can s = π(s)(s)` with `π(s) ∈ G_s`, any member of the orbit.
  - For `a23`, the reachable states lie in a list and each `G_s` in a list (`FiniteGroups`).
- **A15's extra hypotheses** (`a15`).
  - The reachable states of `X` and of `R` lie in lists (`LX`, `LR`). The model's
    bounds make both finite. A complete `R` graph is what the theorem's "`R`'s graph is
    complete" asks; when `obs = ∅` the code does not explore `X`, and `LX` is only a
    hypothesis of the proof.
  - `hloop`: every block whose silent edges between different states contain a cycle from
    a reachable state can loop (`SilentCycle`, Definition 5, case 1). Every build certifies
    it on each explored run of `X` (`reduce/hypotheses.rs::loop_faults`), whether
    `partition.rs::divergence` set the marks or `s_cycles` left every class unmarked. The
    check peels the silent edges inside the unmarked classes. It is `hloop` literally on a
    flat `X` not run up to `H`, and the same check on the stored graph otherwise. When `obs = ∅` a release build
    does not explore `X`, so there `s_cycles` is checked only in tests, on a plain run of `X`
    under the test's own caps; a run past them is skipped (`partition.rs::unobserved_loops`).
    When `s_cycles` is false, `Mono.no_silentCycle` derives `hloop` with no class marked
    from the monotone cells of `S` ([below](#the-partition-certificate-and-monotone-variables)).
- **The searches** (`Accepts`, `OAccepts`; [below](#the-liveness-searches)).
  - The searched graph is the system's step relation on its reachable states: a complete
    run (N1), so `En`, `Moves` and `Tk` are those of `M`.
  - The reachable states lie in a list (`L`, `LM`). The orbit search also needs every `G_s`
    finite and, at each state, the fair instances enabled there in a list (`hΦ`); the code
    keys a finite set of instances (`Product::keys`).
  - The canonical form is a `Canon`: `can s = π(s)(s)` with `π(s) ∈ G_s`.
  - The orbit search's theorems take the commutation on every state (`Sym`); their `_on`
    forms (`oAccepts_on_iff`, `not_oAccepts_on_iff_lassos`) take it on a closed set `W` only
    (`SymOn`), as `a23_on` does for `a23`.
- **The fairness model** (`Behaviour.lean`). A fair instance is a set of `T_a` labels (fair
  actions are `T_a` actions, Lemma 5 (e), assumed by giving instances on `Ta` only). A step
  takes an instance only when it changes the state (assumption τ of native.md). Behaviours
  may stutter.

### What is not covered

- **The code.** No theorem here reads `Extended::build`, `restrict_to`, the folds,
  `quotient_blocks`, `Group::apply` or `Canon::canonical`.
  - Lemma 6 is proved over the action model, whose `X` is built as `Extended::build` builds
    it. The fields of `Lemma6Hyp` are read from the code, and the model leaves out what
    [below](#what-the-lemma-6-model-leaves-out) lists. Reading the builder itself would need
    a deep embedding of it.
  - (G1)–(G6) are stated over a slot-level model of the code's syntax
    ([below](#theorem-a18-over-slots)). That the model's `act`, hypotheses and effects are
    `Group::apply`, `breaks::scan` and `each_successor`, and that its read and write sets
    are those of `action_reads` and `effect_writes`, is read from the code, not derived.
  - (B1)–(B3) are certified at run time on each partition (claim A8, R).
    `Sweep.stable_of_cert` proves that a partition that passes the certificate is `Stable`,
    over a model of the check ([below](#the-partition-certificate-and-monotone-variables)).
    That the run the check reads is `X`'s reachable graph (`Sweep.Complete`), that the
    depth-first marking of `partition/reference.rs::stuttering_blocks` computes
    `Sweep.Marked`, and that the fallback partition of `partition/reference.rs::quotient_blocks`, used without the check,
    meets `Sweep.Cert` when the sweeps stop are read from the code. On the flat runs of the
    samples, the domain samples and the corpus, `partition-check` checks the partition the
    code uses, whichever of the two it is, and the shape of the run
    ([below](#the-partitions-and-the-shape-of-x)).
- **A8, the rest of the theorem.** The proofs leave out these parts:
  - The rises carry no `stands`. Consequences 2 to 4 read only the `T_a` labels of `C` and
    `R`, and `R`'s `T` move reads a rise only through the input bytes it ends at.
  - What `stages` returns. `Lemma6Staged` takes the stage list as a parameter. Over the
    action model `Ext.stages` is `Reduction::stages`, and `lemma6_staged` proves that the
    rises of that `X` follow it. The run-time certificate shows that each rise of `X`
    is one stage of `Reduction::stages`.
  - The outer representative in the nested case. The code takes it among the inner run's
    states, so its `S` bytes are an inner representative's, which need not be a reachable
    state of `X`; `a8_nested` takes a reachable member of the block (`exists_rep`). `R`
    reads the representative only through `obs`, its input bytes and its rises. The inner
    split's certificate puts the `obs` variables and inputs on the inner `T` side or in the
    inner `obs`, and Lemma 5 (b) of the inner split keeps the rises' guards and effects there.
    The inner partition's (B1) then fixes on the inner class the representative's `obs` and
    input bytes, which rises it takes, and the input bytes they end at. All three are
    certified at run time. The block a rise enters is read on the inner run (the next item).
    That some reachable member of `X` has the representative's inner `T` part is A8 for the
    inner cone. No theorem assembles these steps. Checking the representative against a
    concrete reachable member would need that member, which comes only from lifting a path to
    each representative through the inner reduction, or from a plain run of `X`.
  - The outer reduced cone's moves in the nested case. `a8_nested` builds `R` with the class
    moves and rises of `X` under the pulled-back partition. The code reads them on the inner
    run. `InnerMap.path_fwd`, `path_back`, `rise_fwd` and `rise_back` carry the moves between
    the two, but no theorem states that the two `R`s have the same steps.
  - Consequences 2 to 4 in the nested case are not restated. The `R` of `a8_nested` is
    `reduced` on a partition that `stable_of_stableV` makes `Stable`, so `a8_edges_iff`,
    `a8_stuck_iff` and `a8_plain_iff` apply to it as they stand.
  - Consequence 4 is stated for `□(trig ⇒ ◇goal)` at one binding, the form of D21's
    `eventually` and `leads_to`, over behaviours. N9's search decides it on a finite system
    (`liveHolds_iff_not_accepts`). `always` is an invariant (consequence 1).
  - The folds. Their effect is inside the step relations that Lemma 6 relates. Lemma 5
    (c)–(e) are derived over the action model with `FoldData`
    ([below](#definition-3-and-lemma-5-over-actions)). Lemma 6's model reads a param fold's
    bit and takes `Fold::holds` computing it exactly as the hypothesis `Lemma6Hyp.exact`;
    node folds and groups of folds that share a bit are not modelled. Lemma 5 (e) enters
    `a15` and `a8_plain_iff` as fair instances given on `T_a` labels.
- **A15, the rest of the theorem.**
  - The run's search is the abstract one of
    [The liveness searches](#the-liveness-searches): `a15_search` takes its verdict on `R`,
    and what that section leaves out of the code is left out here. The lift and replay of a
    candidate are run-time evidence, and the converse (a counterexample of the run is one of
    `C`) is not claimed.
  - The nested case. Definition 5's cases 2 (an inner class that can loop) and 3 (an orbit
    self-edge of `S` up to `H`), and the proof's (ii), need the inner reduction, which
    Reduction.lean does not model.
  - Case 4 (`obs = ∅`, `s_cycles` over `monotone_vars`). The proof's (iii) is proved:
    `Mono.finite_changes_of_behaviour` (a cell whose steps follow an acyclic write graph
    changes finitely often), and `Mono.no_silentCycle` and `Mono.a15_monotone` (when every
    cell of `S` is monotone along the silent edges, `hloop` holds with no class marked).
    `Mono.Sys.succ_covers` shows that the steps of `each_successor` follow the graph
    `monotone_vars` builds, under the pre-state facts `Mono.Sys.Pre`, which the guards are
    read to give ([below](#the-partition-certificate-and-monotone-variables)). The code's
    unobserved `S` is one class for all input bytes, which is not (B1); `a15_monotone` holds
    for any partition, and the reading of the code's `R` as the reduced cone of the
    partition by input bytes, whose classes are a function of `T`, is not proved.
- **A23, the rest of the theorem.**
  - The run's search is the abstract one of
    [The liveness searches](#the-liveness-searches): `not_oAccepts_iff_lassos` and
    `not_oAccepts_on_iff_lassos` make its verdict `a23`'s quotient side, and what that
    section leaves out of the code is left out here.
  - The entity-cone findings up to symmetry (`state_model_findings`) and deadlocks are not
    stated.
- **A18's BFS depth.** The claim that each class is reached at its least depth is not stated.

The sources hold no `sorry`, no `admit` and no `axiom`, and no statement is left unproved.

### The liveness searches

<!-- derived-from ../../docs/theory/native.md#the-check -->

`Search.lean` states the check of N9 and proves that it decides `□(trig ⇒ ◇goal)` at one
binding. `SearchSym.lean` does the same for the search a run up to symmetry makes on pairs of
a stored state and a binding, and relates it to N9's search on `M` and to `a23`. Each
definition is the predicate the code computes; the table names where.

The definitions take the system from the symmetry. With the commutation only on a closed set
`W` (`SymOn`), the search is stated on `M` restricted to `W`, with `SymOn.toSym`. That is the
graph the code stores for `M`: every stored state lies in `W` (`quotient_restrict_W`), and
from a state of `W` the restriction has `M`'s real edges, enabled instances and taken
instances (`restrict_rEdge_iff`, `restrict_enabled_iff`, `restrict_takes_iff`). The theorems
whose names end in `_on` compare its verdict with `M`'s, and `SymActSearch.lean` composes
them with A18 from (G1)–(G6).

| Lean (`RdraIsh.`) | File | Code (`crates/rdra-ish-core/src/lifecycle/explore/`) |
|---|---|---|
| `LTS.Enabled`, `LTS.Moves` | `Behaviour.lean` | `liveness.rs::ComponentRun::fair_steps`: an instance is enabled where an edge to another state takes it |
| `LTS.NEdge`, `LTS.SameComp` | `Search.lean` | `liveness.rs::ComponentRun::avoid`: the goal-free states, the edges between two of them, and the components of that graph |
| `LTS.FairComp` | `Search.lean` | `liveness.rs::ComponentRun::avoid`: a component of two or more states in which every instance enabled at all of its states is taken by an edge inside it |
| `LTS.IsStop` | `Search.lean` | `liveness.rs::ComponentRun::avoid`, `liveness.rs::StopRule`: the stops under `Plain`, `WfNext` and `Looping` |
| `LTS.Accepts` | `Search.lean` | `liveness.rs::ComponentRun::avoid` (the backward walk from the stops and fair components) and `liveness.rs::ComponentRun::liveness_search` (the start) |
| `REdge`, `pn`, `tn` | `SearchSym.lean` | `orbit/liveness.rs::OrbitRun::is_real`, `orbit/liveness.rs::Product::target`, `orbit/liveness.rs::Product::moved` |
| `PN`, `PEdge`, `SameP` | `SearchSym.lean` | `orbit/liveness.rs::Product::search`: the goal-free pairs, the real edges between two of them, and the components of that graph |
| `Candidate` | `SearchSym.lean` | `orbit/liveness.rs::Product::search`: a component of two or more pairs, or one pair with a real edge to itself |
| `En`, `TEdgeE`, `CoveredE`, `ThreadsCovered` | `SearchSym.lean` | `orbit/liveness.rs::Product::enabled`, `orbit/liveness.rs::Product::threads_covered` |
| `PStop`, `OAccepts` | `SearchSym.lean` | `orbit/liveness.rs::Product::search`: the stop pairs, the backward walk and the start |

In `orbit/liveness.rs::Product::threads_covered`, the mark of a node into which an edge comes
from a pair where no enabled instance moves to it is the third case of `CoveredE`; the move of
an instance is a bijection, so this is the pair where the instance's preimage is not enabled.

Not modelled:

- Tarjan's algorithm (`graph.rs::Adjacency::components`): a component is mutual
  reachability.
- N10, the cut at `expanded` of a capped graph (`open`, `s < expanded`): the graph is complete.
- The choice of binding (the first triggered state in BFS order) and of start: the
  verdicts quantify over all.
- The bit sets and sorted lists that hold `always`, `taken` and the enabled instances.
- The permutation of an edge. `pn` and `tn` move a pair and an instance by `π(t)` of the
  edge's concrete successor; the code reads the permutation it stored for the edge
  (`perms.id(k)`), which the annotated run sets to `π(t)` (`RunOf`).
- A real edge. `REdge` asks `t ≠ r`; `is_real` asks that the stored target differ from `r` or
  that the edge be an orbit self-edge. No theorem here shows that the two agree; cones.md
  lists their agreement among the facts read from the code.
- The counterexample the code builds (`walk`, `fair_loop`, the lasso lift and its
  `MOST_COPIES`): the proofs build their own lasso, and the code's certifies itself at run
  time (N11).
- `imap`'s case for an action the reduction skips, which keeps the instance: the orbit
  search on a reduced cone is outside the model, and its counterexample is decided again on
  the plain runs (`cli/src/verify/native.rs::liveness_up_to_symmetry`).

### Definition 3 and Lemma 5 over actions

`Actions.lean`, `Split.lean` and `SplitExample.lean` go one level below the abstract systems
above. They model the cone system by its actions and prove Lemma 5 from Definition 3. With
them, the hypothesis `Split` of `a8_dbsb` (Lemma 5 (a)(b)) is a theorem, `split_computed`,
whose one hypothesis is `Local` (below). Lemma 5 (c)–(e) need `FoldData` as well. For the
model read off the slot syntax of A18, `Local` is a theorem too
([One model of the actions](#one-model-of-the-actions)).

The model (`Act.Model`):

- a finite set of variables (`vars` lists them all), each with the existence variable of its
  entity (`own`, the code's `own_existence`) and its `existences`;
- states as valuations `V → Val`;
- actions with their parameters (an entity and a `Role`: `existing`, `parent`,
  `triggered`, `new`, `earlier`), guard conjuncts, a read set `rd`, a write set `wr` and an
  effect;
- a step that binds each parameter to a slot and each choice to a value (`Binding`), when
  every guard conjunct holds.

`rd` is the read set `split_with` uses (`action_reads`, mark reads included, with
`copy_reads`), and `wr` is `action_writes`. Lemma 5 does not use the parameters and roles.
They are there for Lemma 6.

| Lean (`RdraIsh.`) | File | Claim | Statement |
|---|---|---|---|
| `Act.mem_closed`, `Act.mem_least` | `Split.lean` | Definition 3 | `Mem`, the inductive predicate of Definition 3's conditions for one anchor, is closed under them and lies inside every closed pair: it is the least fixpoint. |
| `Act.closed_of_fixed`, `Act.computed_fixed`, `Act.computed_iff` | `Split.lean` | Definition 3 | One pass applies every condition at once. A pair that a pass leaves unchanged is closed. Passes from the anchor and the counters reach such a pair within one more pass than there are variables and actions, and that pair is `Mem`. |
| `Act.mem_eq_of_sound` | `Split.lean` | Definition 3 | A closed pair whose members all follow from the conditions is `Mem`, whatever order the conditions were applied in. `split_at`'s in-place loop is one such order: each insert applies one condition, and it stops on a pass that adds nothing. |
| `Act.lemma5_of_closed`, `Act.lemma5_computed` | `Split.lean` | Lemma 5 (a)–(e), A10 | Every closed pair, with `S_v`, `S_a`, `obs` and `inputs` as Definition 3 defines them, satisfies `Lemma5`. So does the computed split of every anchor, and with it the split `split_with` picks. |
| `Act.no_counter_input` | `Split.lean` | A12 | Under `Lemma5`, no counter is an input when `mono` is false on counters. |
| `Act.lemma5ab_of_closed` | `Split.lean` | Lemma 5 (a)(b) | Every closed pair, with Definition 3's `obs`, satisfies `Lemma5ab`. This needs nothing of the folds. |
| `Act.split_of_lemma5`, `Act.split_computed` | `Split.lean` | A8, the hypothesis `Split` | Under `Local`, sets that satisfy `Lemma5ab` make the cone system over `T_v × S_v` valuations (`Act.cone`) satisfy `Split`: an `S_a` step keeps `T`, and a `T_a` step keeps `S` and reads `S` only through `obs`. `split_computed` applies it to the computed split. |
| `Act.cone_reach_iff` | `Split.lean` | none | `Act.cone` is the action-level system with its states split: a pair is reachable in `cone` iff its valuation is reachable in `lts`. |
| `Act.Local.idle` | `Actions.lean` | none | Under `Local`, an action that writes nothing stutters. |
| `ExampleSplit.sets`, `ExampleSplit.split`, `ExampleSplit.lemma5`, `ExampleSplit.input_and_obs` | `SplitExample.lean` | none | A three-variable cone with a kept action, a monotone input and an observed `S` variable. It evaluates the computed split and discharges `Local` and `FoldData`, so the hypotheses are jointly satisfiable, and `Split` and `Lemma5` are derived there. It has no entities (`own` and `existences` are empty), no fold, no action that writes nothing, and no parameters. |

`Lemma5` is a predicate on any sets: `T_v`, `T_a`, `obs`, `inputs` and the folded reads. It
is the contract a run-time certificate of the sets `split_with` returns must check. Its fields
are (a), (b) (writes and reads), (c), (d), (e) (seeds and kept actions), `obs ⊆ S_v` and
`inputs ⊆ T_v`. `S_v` is the complement of `T_v`, and `S_a` the writers outside `T_a`. `rd` is
the reads with the copy reads, as `split_with` takes them. A certificate that checks the code's
`s` and `s_actions` must also check that they are these two sets. `Split` needs only the
`Lemma5ab` part: (a), (b) and `obs ⊆ S_v`. `split_of_lemma5` takes those and `Local`, and not
leastness. So a split that passes them meets `Split`, however it was computed. Leastness only
keeps `T` small.

#### What these hypotheses are

- **`Local`**, the locality of the code's `rd` and `wr`:
  - every guard conjunct reads only variables of `rd a ∪ wr a`;
  - a conjunct's value depends only on the variables it reads;
  - the effect leaves every variable outside `wr a` unchanged;
  - the effect's value on `wr a` depends only on `rd a ∪ wr a`.

  The footprint is `rd ∪ wr` and not `rd` alone. The code does not count two things as reads:
  taking a `new` slot (`action_reads`), which looks at the existence the action writes, and
  the slots of a written variable that the binding leaves alone. This is the statement that
  the locality of expression evaluation must give for each action. `SymAct.Sys.local_`
  proves it for the model read off the slot syntax.
- **`FoldData`**, for the folds and `raw_reads`, given the split:
  - `raw_rd`: `raw_reads` lies inside `rd`;
  - `cover`: every `T_v` read of an `S_a` action is a raw read, an existence of a raw `T_v`
    read, or a read its fold covers. A `Triggered` parameter's existence is the middle case:
    `raw_reads` leaves it out, and `existences` of its mark brings it back;
  - `ex_mono`: an existence of a monotone variable is monotone. In `monotone_vars` only
    creates and deletes write an existence, from absent to present to deleted. With inputs
    off, `mono` is constant `false` and the condition is empty.
- **`mono`** is a parameter. Lemma 5 (d) reads it only through `ex_mono` and the closure.
  `Monotone.lean` models the write graph `monotone_vars` computes (`Mono.Sys.Monotone`),
  separately from `Act.Model`. Over it, an existence that only creates and deletes write is
  monotone (`Mono.Sys.existence_monotone`), which is the fact `ex_mono` asks of the code;
  the two models are not joined, so `ex_mono` stays a hypothesis here.
- **The labels.** `Act.cone` labels a step `.inl` when its action is in `T_a`, and `.inr`
  otherwise. So the `.inr` side holds the `S_a` actions and the actions that write nothing
  and are not kept, which neither set contains. Those stutter (`Local.idle`). Lemma 6 counts
  them out: `coneW` is `cone` without their steps, `X` holds no such action (`restrict_to`
  drops an action that writes nothing in `S_v ∪ inputs`), and `coneW_reach_iff` shows that
  `coneW` and `cone` reach the same states.
- **The pass.** `computed` applies the conditions simultaneously, while `split_at` updates
  `T` in place. `mem_eq_of_sound` shows that both give `Mem`, provided every insert of the
  loop applies one condition. That much is read from the code.

The folds stay abstract in this model. Lemma 6's model below adds them, with the cone that
`restrict_to` builds.

### Checking the code's sets with Lean

<!-- constrained-by #definition-3-and-lemma-5-over-actions -->

```sh
scripts/check-lean-certificates.sh
```

`Lemma5` is a predicate on any sets, so the sets the code computes can be checked against it
directly. The script does that for every top-level split of the samples, the domain samples
and the engine corpus cases that build a model, at `Bounds::STATES` and `Bounds::VERIFY`,
with the inputs of `S` on and off (the walk of `every_split_passes_its_check`):

1. The ignored Rust test `dump_lemma5_certificates` writes one certificate per split, one
   JSON object per line, equal certificates once. A certificate holds the sets
   `split_faults` reads, by `VarId` and `ActId` index: each action's reads (`action_reads`
   with `copy_reads`) and writes (`action_writes`), from the same `read_write_sets` that
   `split_faults` calls; the variables `monotone_vars` marks; the split `split_with` returns
   (`T_v`, `S_v`, `T_a`, `S_a`, `obs`, `inputs`, and each action's `Fold::reads`); and the
   seeds' reads and kept actions.
2. `lake build` checks the soundness proof below and builds the executable `lemma5-check`
   (`Lemma5CheckMain.lean`). It reads the file with `Lean.Json` and runs
   `Cert.Data.check`, a Boolean function over lists of numbers
   (`RdraIsh/Lemma5Cert.lean`), on each certificate. It fails unless it accepts all of them.
3. Eleven faults are planted in copies of the dump, one for each part of `check`, such as an
   `S_a` write moved into `T_v`, an `obs` read dropped, or a non-monotone input. Each must be
   rejected, and the rejection must name its part.

The dump has about 500 distinct certificates, about half of them with an `S` side, at most
34 variables and 46 actions each, about 200 KB in all. Once the Rust test binary and the
Lean project are built, this part takes about 3 seconds, and `lemma5-check` itself takes
0.03 s of CPU. A cold run adds the build of rdra-ish-core's test binary. The script then runs
`scripts/check-lean-partitions.sh`
([below](#the-partitions-and-the-shape-of-x)). CI's `formal-verify` job runs both, with the
`ci-checked` test binary it has already built.

| Lean (`RdraIsh.`) | File | Claim | Statement |
|---|---|---|---|
| `Cert.Data.check_sound`, `Cert.Data.check_lemma5` | `Lemma5Check.lean` | Lemma 5 (a)–(e) | A certificate that `check` accepts satisfies `Lemma5` over `Fin nv` and `Fin na`, for every model whose reads and writes are the certificate's (`Fits`). The guards, effects and initial states of the model are unconstrained. `check_lemma5` is the same for the certificate's own `model`. |
| `Cert.Data.fits_model` | `Lemma5Check.lean` | none | The certificate's `model` (its reads and writes, no guards, effects that change nothing) fits it, so `check_sound` is not vacuous. |
| `Cert.Data.sides_of_check`, `Cert.Data.sa_iff` | `Lemma5Check.lean` | Lemma 5, the split | An accepted certificate's `S_v` is the complement of its `T_v`, and its `S_a` is `Sa`, the writers outside `T_a`. |
| `Cert.Data.split_of_check` | `Lemma5Check.lean` | A8, the hypothesis `Split` | Under `Local`, an accepted certificate's split makes `Act.cone` satisfy `Split`. |
| `Cert.mem_toFin` | `Lemma5Check.lean` | none | The ids of a list below `n`, as `Fin n`, are the list's members. |

`check` is ordinary compiled code that the kernel never evaluates on the dump. The theorem
is about the function `lemma5-check` runs: `Lemma5CheckMain.lean` imports
`Lemma5Cert.lean`, and `Lemma5Check.lean` proves `check_sound` of the `check` defined there.

An accepted dump does not establish these:

- **`Fits` of the code.** That a certificate's reads and writes are the ones the cone system
  has is read from the dump code: it calls `read_write_sets`, as `split_faults` does. That
  they bound what each action looks at is `Local`, which nothing checks.
- **Nested splits.** A nested split, chosen in `Classes::of`, is not dumped.
- **The folds and `monotone_vars`.** `folded` is taken to be `Fold::reads`, and `mono` to be
  `monotone_vars`. `FoldData` and the acyclicity behind `monotone_vars` are not checked.
- **Three clauses of `split_faults` that `Lemma5` does not state.** The first is the side rule
  of Definition 3 (`Clause::Sides`). The second is that every fold is of an `S_a` action. The
  third is that a `T_v` variable an `S_a` action reads outside its fold (`raw_reads`) is an
  input.
- **The tools.** The Lean compiler, `Lean.Json` and the Rust dump are trusted.

#### The partitions and the shape of `X`

<!-- constrained-by #the-partition-certificate-and-monotone-variables -->

```sh
scripts/check-lean-partitions.sh
```

The script checks every partition `Classes::of_graph` computes on a flat run of `X` (no
inner reduction, not up to `H`) while the cones of the samples, the domain samples and the
engine corpus cases that build a model are reduced, at `Bounds::STATES` and
`Bounds::VERIFY`. That is the walk of `every_quotient_passes_the_checks_of_its_hypotheses`,
with each run of `X` capped at `RDRA_LEAN_RUNS_CAP` states (200,000 by default).

1. The ignored Rust test `dump_partition_certificates` turns on a record that only a test
   build compiles (`partition/dump.rs`), called from `Classes::of_graph` once the partition
   is certified (or replaced by the sweeps' partition). It writes one record per distinct
   run, in lines of numbers:
   - each state's `obs` value, its `S` value (every variable of `X` outside the inputs) and
     its input bytes, each value numbered as it first shows;
   - each state's class, numbered as `of_graph` numbers the classes;
   - each class's representative (`Classes::reps`);
   - the initial states, the silent edges and the rises (`split_edges`).

   It counts the runs it leaves out, by reason: a run of an `X` under its own inner
   reduction, whose states are inner classes; a run up to `H`, whose states are orbits; a
   partition that is the identity, which `of_graph` turns down before it numbers classes; a
   run the cap stopped, which is never partitioned; and an unobserved `S`, which is never
   explored.
2. `lake build` checks the soundness proof below and builds the executable
   `partition-check` (`PartitionCheckMain.lean`). It runs `PCert.Data.check`
   (`RdraIsh/PartitionCert.lean`) on each record, over arrays, and fails unless it accepts
   all of them. `check` computes a witness and tests it with `validate`. The witness code is
   not trusted, because the theorems hold for any witness `validate` accepts. The parts are:
   - **well-formed**: each class number and each state number in range;
   - **rep**: each class's representative lies in the class;
   - **b1**: each state has its representative's `obs` value and input bytes;
   - **b2**: each silent edge between two classes enters an exit listed for its source's
     class, and each state reaches each exit of its class through the class, by a successor
     of lower rank for each state and exit;
   - **b3**: the classes each state enters by a rise, each listed class witnessed by a rise
     and each rise by a listed class, are its representative's;
   - **silent keeps inputs** and **rise keeps S**: the shape of `X`;
   - **reach**: every state has an edge from a state of lower rank, or is initial.
3. Eight faults are planted in copies of the dump, one per part. Each must be rejected, and
   the rejection must name its part:
   - a state given an `obs` value of its own (b1);
   - two classes with the same `obs` and input bytes merged where the merge splits (b2);
   - a rise added into a class the rest of its class does not enter (b3);
   - a representative moved into another class (rep);
   - a silent edge sent to a state with other input bytes;
   - a rise sent to a state of the same class with another `S`;
   - the initial states dropped (reach);
   - a class number out of range (well-formed).

The walk partitions 182 flat runs, 60 of them distinct, 344,409 states in all, the largest
85,154. It leaves out 78 nested runs, 24 identity partitions, 36 runs stopped at the cap and
6 unobserved sub-lifecycles; no run is up to `H`. The dump is about 27 MB, most of it
silent edges. `partition-check` takes 1.0 s of CPU on it. In a `ci-checked` build the dump
takes about 7 s, and the walk peaks at about 2.9 GB of memory; a debug build is much slower
(about 170 s), so `RDRA_LEAN_RUNS_CAP=50000` suits a quick local run.

| Lean (`RdraIsh.`) | File | Claim | Statement |
|---|---|---|---|
| `PCert.cert_of_validate` | `PartitionCheck.lean` | A8, (B1)–(B3) | For any witness that `validate` accepts, the record's partition passes `Sweep.Cert` on the run `Data.run`, whose labels are the `obs` value and the input bytes. |
| `PCert.marked_of_exit`, `PCert.b2_of_check` | `PartitionCheck.lean` | A8, (B2) | A state whose class lists the splitter as an exit is `Sweep.Marked`, by induction on the rank of its successor. The marking "the class lists the splitter" is closed and all or none on each class, so `Sweep.b2_of_marking` gives the certificate's (B2). |
| `PCert.enters_rep` | `PartitionCheck.lean` | A8, (B3) | Each state enters by a rise exactly the classes its representative enters. |
| `PCert.reach_of_check`, `PCert.complete_of_check` | `PartitionCheck.lean` | A8, `Sweep.Complete` | Given `Graph` and `Faithful`, every numbered state of an accepted record is reachable, and the record's run is `Sweep.Complete`. |
| `PCert.stable_of_check` | `PartitionCheck.lean` | A8, (B1)–(B3) | Given `Graph` and `Faithful`, an accepted record's partition is `Stable`, with the dumped representatives. |
| `PCert.extended_of_check`, `PCert.reach_onReach` | `PartitionCheck.lean` | A8, the shape of `X` | Given `Graph` and `Faithful`, the steps of `X` from its reachable states (`onReach`, which has the reachable states of `X`) are `Extended`: a silent edge keeps the input bytes, a rise keeps `S`. |
| `PCert.graph_onReach`, `PCert.onReach_of_check` | `PartitionCheck.lean` | A8, (B1)–(B3) and the shape of `X` | An accepted record's run is a run of `onReach X` too, so `onReach X` has both a `Stable` partition, with the dumped representatives, and `Extended`, the two hypotheses of A8 the record bears on, for one system. Lemma 6 for `onReach X` is not derived here. |
| `PCert.wf_of` | `PartitionCheck.lean` | none | The bounds the well-formed part checks. |

An accepted dump does not establish these:

- **`PCert.Graph`.** That the numbering is a bijection onto a set that holds every
  reachable state of `X`, that the silent edges and the rises are exactly those of `X` from
  reachable states, and that the listed initial states are initial, are read from the dump
  code and the explorer: the dump writes the run's states in the order the run holds them,
  and `split_edges` of the run's graph. That every numbered state is reachable is checked
  (reach).
- **`PCert.Faithful`.** That equal numbers in a record mean equal `obs` values, `S` values
  and input bytes is read from the dump code, which numbers each projection as it first
  shows.
- **The runs left out.** Nested runs, runs up to `H`, runs past a cap, and every run outside
  this walk, user runs among them, are not checked. The shape is checked on the reachable
  part of `X` only: a step from a state no run reaches is not seen.
- **The tools.** The Lean compiler, the Rust dump and the line parser of
  `PartitionCheckMain.lean` are trusted. `check` is ordinary compiled code that the kernel
  never evaluates on the dump. `PartitionCheckMain.lean`
  imports `PartitionCert.lean`, and `PartitionCheck.lean` proves the theorems of the `check`
  defined there.

### Definition 4 and Lemma 6 over actions

<!-- constrained-by #definition-3-and-lemma-5-over-actions -->

`Restrict.lean`, `Lemma6.lean` and `Lemma6Example.lean` build the extended sub-lifecycle `X`
of Definition 4 from the action model and a split, and prove Lemma 6 for it. With them the
hypotheses `Lemma6Fwd`, `Lemma6`, `Lemma6By`, `Lemma6Staged` and `Extended` of the theorems
above are theorems whose one hypothesis is `Lemma6Hyp`, and `a8_actions` is Theorem A8 with
`recanon` from `Lemma6Hyp`, (B1)–(B3) and the rise table alone.

The model (`Ext`, `Ext.ext`):

- **Roles.** `Rows.cand` is `candidate_slots`: `existing`, `parent` and `triggered` bind a
  present row, `new` the lowest absent one, `earlier` any slot. A role is a guard conjunct
  on its entity's existence (`roleConj`), and an action's guard is its role conjuncts
  followed by its other conjuncts `og`.
- **Sides.** A parameter's entity is on `S`, an input, a stub, or dropped, by where its
  existence lies (`side`).
- **The input bytes.** The raw inputs and one bit per key (`Inp`, `inp`, `bitOf`).
- **`X`'s roles** (`xrole`). A parameter keeps its role on `S` and on an input. A fold's key
  off `S` gets `Earlier { step: 0 }` (`fold_param`), and so does a stub parameter that needs
  a present row, of a rise or a folded action (`bind_stub_params`; the flag `bindStubs`
  turns it off). A parameter on a dropped entity, and a `new` one on a stub, is dropped. A
  stub's existence reads `dflt` in `X`.
- **`X`'s guard** (`xguard`). The roles `X` gives, the conjuncts `kept` names, and the bit at
  the key's slot.
- **`X`'s edges.** A silent edge is an `S_a` action under `xguard`. A rise is a `T_a` action
  that writes an input, under `xguard`, which writes the raw inputs, or the free flip of one
  bit (`reduce/extended.rs::flips`). `Ext.stages` is `Reduction::stages`: the raw inputs,
  then each bit that changes, in the order of `keys`.
- **The cone** (`coneW`). The cone system without the steps of actions that write nothing
  and are not kept ([the labels](#what-these-hypotheses-are)).

| Lean (`RdraIsh.`) | File | Claim | Statement |
|---|---|---|---|
| `Act.mem_roleConjs` | `Restrict.lean` | none | The role conjunct of each position of a parameter list is among the list's role conjuncts. |
| `Act.mem_roleConjs_inv`, `Act.roleConj_local` | `Lemma6.lean` | none | Every role conjunct of a parameter list is that of one of its positions, and each depends only on what it reads. |
| `Act.Ext.unfolded_side` | `Lemma6.lean` | Lemma 6, "Parameters", the unfolded table | Every parameter of an unfolded `S_a` action whose role reads its row is on `S` or an input: the row's existence is in `Rd ∪ Wr`, so in `S_v ∪ inputs` by Lemma 5 (a)(c). This derives the table's stub and dropped cells. |
| `Act.Ext.xrole_cand`, `Act.Ext.xguard_of_guard`, `Act.Ext.eff_x` | `Lemma6.lean` | Lemma 6, (P1)–(P4) | The role `X` gives a parameter admits the slot the cone binds it to; a guard of the cone at `(t, s)` is the guard of `X` at `x(t, s)` for the same binding; an `S_a` action writes the same `S_v` values at both. |
| `Act.Ext.s_fwd`, `Act.Ext.s_back` | `Lemma6.lean` | Lemma 6, the `S_a` half | An `S_a` step of the cone is a silent edge of `X` by the same action and binding. A silent edge of `X` from `x(t, s)` is an `S_a` step of the cone by the same action, at a binding equal to the edge's outside the fold's free `T` parameters (`stands`), whose slots come from step (a). |
| `Act.Ext.flips_end`, `Act.Ext.rise_fwd` | `Lemma6.lean` | Lemma 6, step (c) | A `T_a` step of the cone is matched by one rise of `X` per stage of `stages`: the restricted action for the raw inputs when they change, then one flip per bit that changes. |
| `Act.Ext.extended`, `Act.Ext.lemma6_by`, `Act.Ext.lemma6_staged`, `Act.Ext.lemma6`, `Act.Ext.lemma6_fwd` | `Lemma6.lean` | Definition 4, Lemma 6 | `X` has the shape `Extended` asks for, and `Lemma6By stands`, `Lemma6Staged stages`, `Lemma6` and `Lemma6Fwd` hold of `coneW` and `X`. |
| `Act.Ext.reads_of_lemma5` | `Lemma6.lean` | Lemma 5 (c) | `Lemma6Hyp.reads` is `Lemma5.c`, with the inputs and the folds' reads as its sets. |
| `Act.split_coneW`, `Act.coneW_reach_iff` | `Lemma6.lean` | none | `coneW` satisfies `Split` under `Local` and Lemma 5 (a)(b), and reaches the states `cone` reaches. |
| `Act.a8_actions`, `Act.a8_actions_holds_iff` | `Lemma6.lean` | A8 | Under `Lemma6Hyp`, (B1)–(B3) of a partition of `X` and a complete rise table, A8's relation is a divergence-blind branching bisimulation between `coneW` and the reduced cone whose `T` move is `recanon` over `stages`, and it preserves `T_v` and `obs`. An invariant over `T_v ∪ obs` holds on `cone` iff it holds on that reduced cone. |
| `ExampleLemma6.hyp`, `lemma6`, `rise_from_init`, `fold_step`, `mark_step` | `Lemma6Example.lean` | none | A five-variable cone with a folded `S_a` action (its key on an input, a free `T` parameter on a stub), an unfolded `S_a` action with a kept conjunct, a rise with a parameter on a stub and a bit writer. `Lemma6Hyp` is discharged, and reachable steps of each kind are matched in `X`. It has no parameter on a dropped entity and no `new` or `earlier` parameter. |
| `ExampleLemma6.stub_bug` | `Lemma6Example.lean` | Lemma 6, (P3) | With `bind_stub_params` off, the rise's stub parameter keeps its `Parent` role, which `X`'s absent stub never meets, so the `C ⇒ X` half of Lemma 6 fails on that cone. |

#### What `Lemma6Hyp` assumes

Each field is a fact about one action, read from the code:

- `local_`, `lemma5`, `reads`: `Local`, and Lemma 5 (a)(b) and (c), with a fold's reads
  those of `Fold::reads`. `reduce/lemma5.rs::split_faults` certifies Lemma 5 at run time.
  `lemma5ab_of_closed` derives (a)(b) from Definition 3, and `lemma5_of_closed` derives (c)
  given `FoldData` (`reads_of_lemma5`).
- `shape`: an action's guard is its role conjuncts followed by `og`.
- `new_writes`: a `new` parameter comes with its `Create`, which writes its entity's
  existence (`build/actions.rs`).
- `kept_rd`: a conjunct `X` keeps reads only `S_v ∪ inputs` (`restrict_to`).
- `cover`: every conjunct in `og` of an `S_a` action is kept or folded.
- `rise_eff`: step (c), a `T_a` action's value on an input depends only on `S_v ∪ inputs`.
- `fold_sa`, `folded_og`: a fold belongs to an `S_a` action, and its folded guards are
  conjuncts of it.
- `exact`: step (a), `Fold::holds` computes the bit exactly. The bit at slot `c` holds at
  `t` iff some binding with the key at `c` puts each `T` parameter whose role needs a
  present row on one and makes every folded guard true. It is stated for every slot number,
  where the code has bits only for the key entity's slots.
- `folded_names`: the folded guards name only `T` parameters and the key (`fold_of` lets
  them name at most one `S` parameter, which is then the key).
- `other_names`, `eff_names`: no other conjunct and no effect names a `T` parameter but the
  key (`fold_of`'s link guards, and the one `T` parameter an effect may reference).
- `eff_s`: a folded action's effect on its writes reads only `S_v ∪ inputs`. For an unfolded
  action, `Local` and Lemma 5 (c) give it.
- `bind`: `bind_stub_params` runs.

#### What the Lemma 6 model leaves out

- **Initial states.** `X`'s initial states are the projections of the cone's. That
  `restrict_to`'s initial states are those projections (Lemma 2) and that every bit starts
  where the cone starts (`fold_initial`) is not derived.
- **Folds.** The model has one bit per folded action. A group of folds sharing a bit (the
  disjunction of `push_bits`, `Signature::Param`) and node folds (`FoldKey::Linked`, which
  add a parameter) are not modelled.
- **Parameter positions.** A binding of `X` keeps the cone's positions, so step (b) (the
  renumbering of `Extended::cone_params`) is not modelled. Nothing states that `X`'s kept
  conjuncts and a rise's effects ignore the slots of the parameters `X` drops. A silent
  edge's binding is the cone's outside the fold's free `T` parameters, either way. `X` reads
  a fold's bit at the key's position whatever the key's side, so the rule that a fold whose
  key the restriction drops stays unfolded (`Fold::key_kept`, `Fault::KeyDropped`) plays no
  part.
- **The rest.** The nested case is not modelled, and nothing reads `StubCheck`, which checks
  at run time that the built `X` is the one `bind_stub_params` is meant to build.
- **Consequences 2 to 4.** These are stated for `coneW`. `a8_actions_holds_iff` reads
  invariants on `cone` itself.

### Theorem A18 over slots

<!-- derived-from ../../docs/theory/cones.md#instance-symmetry -->

`SymActions.lean` models the cone system by its slots and derives Theorem A18's commutation
from (G1)–(G6). The model (`SymAct.Sys`):

- entities in classes, each class with its slot count and whether the scan lets it move;
- a state gives each slot an existence (absent, present, deleted), each data column and each
  owner a value per slot, and the clock. An owner holds a parent slot or `null`;
- a permutation per class that fixes the slots past the class's count. `G_s` is the
  permutations that fix every absent slot and every slot of a class that does not move.
  `act` is `Group::apply`: a slot's variables move with it, and an owner on a row that is
  not absent is relabelled by its parent's permutation;
- terms, predicates and effects after `lifecycle/types.rs`. A term reads at a parameter, at
  a parameter's slot in the successor, or at a quantifier's slot. Quantifiers range over the
  present slots of the pre-state. Effects are read pointwise: a location takes the last
  write in the list;
- a step binds each parameter by its role (`Existing`, `Parent`, `Triggered` a present slot,
  `New` the lowest absent slot, `Earlier` any slot), and each choice effect picks a value.

`BaseAbsent`, the `W` of Theorem A18, is the set of states whose absent rows hold base values.

| Lean (`RdraIsh.`) | File | Claim | Statement |
|---|---|---|---|
| `SymAct.Sys.act_mul`, `SymAct.Sys.mem_act` | `SymActions.lean` | Definition 6 | `act` composes, and `G_{g(s)} = G_s` for `g ∈ G_s`. |
| `SymAct.Sys.eval_mv` | `SymActions.lean` | A18, predicates | Under (G1), (G6) and well-typedness, a predicate has the same value at `(s, t, p)` and `(g(s), g(t), g(p))` when the rows it may read an owner of are not absent. |
| `SymAct.Sys.post_comm` | `SymActions.lean` | A18, effects | On `W`, the successor of `g(s)` under `g(p)` is `g` of the successor of `s` under `p`, by (G2) and (G4). |
| `SymAct.Sys.step_comm`, `SymAct.Sys.mem_step` | `SymActions.lean` | A18 | A step from a state of `W` commutes with every `g ∈ G_s`, and a step keeps `g ∈ G_t` on every state. |
| `SymAct.Sys.W_step`, `SymAct.Sys.W_act`, `SymAct.Sys.init_act` | `SymActions.lean` | A18, (G3) | `W` holds the initial states and is closed under steps and every `G_s`; (G3) closes the initial set under every `G_s`. |
| `SymAct.Sys.symOn` | `SymActions.lean` | A18, A23 | These give the `SymOn` of `a23_on`, with `W` the base-absent states. |
| `SymOn.quotient_reach_iff`, `SymAct.Sys.a18_holds_iff` | `SymActions.lean` | A18, A21 | The canonical quotient of a system reaches what the quotient of its restriction to `W` reaches; so an invariant that `G_s` preserves gets the plain run's verdict on the canonical quotient of the cone system. |
| `SymAct.Sys.orbit_verdict`, `SymAct.Sys.orbit_accepts_iff`, `SymAct.Sys.orbit_verdict_wfNext` | `SymActSearch.lean` | A18, A23, N9 | Under `WF`, `Built` and `Hyp`, with the `SymOn` of `symOn`: the property holds on the cone system for every binding under `Fair_P` and a stop rule iff the orbit search does not accept, iff N9's search on the cone system accepts for no binding; under `WF_vars(Next)` too. No commutation outside `W` is assumed. |
| `ExampleOffW.off_W_counterexample` | `SymActionsExample.lean` | none | Shops and orders, a creation that does not write the order's optional owner: every hypothesis of `step_comm` holds, and a step from a state outside `W`, with the absent order's owner on shop 0, has no image under the swap of the shops. |

#### What these hypotheses are

- **`WF`**: a term reads, and an effect writes, a column or an owner on a row of its own
  entity, and every parameter it names exists.
- **`Built`**, what the builder gives and the scan does not check. A `New` parameter is
  created by its action, and an `Earlier` parameter carries a run-mark conjunct `mark = k`
  whose base is not `k`. The `Earlier { step: 0 }` parameters of the extended sub-lifecycle
  carry no such conjunct; they bind entities `H` leaves out.
- **`Hyp`**, (G1)–(G4) and (G6) as `breaks::scan` states them. (G1): a term naming rows of a
  class that moves is compared only by `=` or `≠` with a term naming rows of the same class
  or with `null`, and appears in no `In`. (G2): an owner is written from a parameter of its
  parent's class or from `null`, and an owner choice ranges over every row. A data column is
  not written from the slot of a class that moves. (G3): an identity start couples its child
  and parent classes, and a pre-existing row's owner starting at its base names no row. (G4):
  a creation writes every owner of the created row whose base names a row. (G6): no guard
  reads an owner on a `New` parameter in the pre-state. (G1)–(G4) also hold when the class
  they name does not move, which is how the scan's leaving a class out enters them. (G6) does
  not mention the group: the scan forbids the read only when the parent's class moves, so a
  model whose parent class the scan leaves out is outside `Hyp`, and cones.md argues that case
  on paper (Theorem A18, step 4).
- **(G5)** is the semantics: a delete resets the row's columns (not its run marks) and keeps
  its owners.
- **Guards only.** Invariants, `after` assertions and properties are not in the model, so
  (G6) outside guards is not derived; `a18_holds_iff` takes the invariance of its invariant
  under `G_s` as a hypothesis. For the same reason `orbit_verdict` takes `LiveEqui`'s trigger
  and goal parts, the fair instances and bindings with their actions (`SymLive`), the
  canonical form (`Canon`), and lists of the reachable states, of each `G_s` and of the
  enabled instances as hypotheses.

Outside `W` the commutation can fail. Take an absent row whose optional owner names a row,
created by a step that does not write that owner: the step from `g(s)` keeps the value and
`g(t)` relabels it (`ExampleOffW.off_W_counterexample`).

The model differs from the code in these ways:

- Effects are read in list order. `each_successor` applies the deletes after the other
  effects and the choices last, and it drops a choice on a deleted row.
- A `Transition` is a choice among its arms from the pre-state value. Owner `Transition`s
  are not in the syntax; the scan leaves their class out (`OwnerChoice`).
- The domain sizes of data columns are not checked, and `null` data values are not
  modelled.
- The roles carry no data but `Earlier`'s step, which `roleOk` ignores. `Parent` and
  `Triggered` bind a present slot, and what else they ask is in the guard.
- The reduced cone (`G_T`) and the run of `S` up to `H` (A22) are not modelled.

#### One model of the actions

<!-- constrained-by #definition-3-and-lemma-5-over-actions -->

`SymAct.Sys` uses the `Param`, `Role` and `Binding` of `Actions.lean`, and a role binds what
`Rows.cand` says, the semantics Lemma 6 reads (`roleOk`, through `rows`). `SymActModel.lean`
reads an `Act.Model` off a `SymAct.Sys` (`Sys.model`), given lists of its entities, columns,
owners and actions:

- **Variables** (`Var`): an existence per entity, a data column, an owner, the clock. A value
  (`Val`) holds every slot of a variable. `enc` reads a state as a valuation, and `dec` reads
  it back.
- **`rd`**, after `action_reads` with `copy_reads`: the existences of the `Existing`,
  `Parent` and `Triggered` parameters, what every guard predicate reads (`pred_reads`: a
  term's variable, whether it reads at a parameter, in the successor or at a quantifier's
  slot, and the existence of each quantified entity), a `Transition`'s variable and a
  `Tick`'s clock.
- **`wr`**, after `effect_writes`: a create writes the existence; a delete the existence and
  every column and owner of its entity; a write or a choice its variable; a tick the clock.
- **Guards**: the role conjuncts `roleConjs` with `rowsV`, then `og`: one slot bound per
  parameter (`boundConjs`), the choices' picks, and each guard predicate evaluated against the
  successor. A bound names only its own position, so a fold can hold the bounds of its free
  parameters among its folded guards, as `Lemma6Hyp.other_names` needs.
- **Effect**: a variable of `wr` takes its value in `post`, and every other keeps its own.

| Lean (`RdraIsh.`) | File | Claim | Statement |
|---|---|---|---|
| `SymAct.Sys.model`, `SymAct.Sys.rd`, `SymAct.Sys.wr`, `SymAct.Sys.og`, `SymAct.Sys.eff` | `SymActModel.lean` | Definitions 3 and 4 | The action model of a slot system, with its read and write sets computed from the syntax. |
| `SymAct.Sys.eval_congr`, `SymAct.Sys.post_pt` | `SymActModel.lean` | none | A predicate's value depends only on the variables `predRd` names, in the pre-state and in the successor; a variable's value after `post` depends only on its value before. |
| `SymAct.Sys.post_frame`, `SymAct.Sys.eff_enc` | `SymActModel.lean` | none | A variable outside `wr` keeps its value, so on a state read as a valuation the model's effect is `post`. |
| `SymAct.Sys.local_` | `SymActModel.lean` | Lemmas 5 and 6, `Local` | Under `Built`, the model meets `Local`. `Built` gives the one case the syntax alone does not: a `New` parameter's role conjunct reads its existence, which its create writes. |
| `SymAct.Sys.guard_enc`, `SymAct.Sys.step_enc`, `SymAct.Sys.reach_enc` | `SymActModel.lean` | none | The model's guard on `enc s` is the step condition of `S.lts`, its steps from `enc s` are those of `S.lts` from `s`, and its reachable valuations are the reachable states read as valuations. |
| `SymAct.Sys.split_computed`, `SymAct.Sys.shape`, `SymAct.Sys.new_writes` | `SymActModel.lean` | Lemma 5 (a)(b), Lemma 6 | The computed split of any anchor meets `Split` under `Built` alone; `Lemma6Hyp.shape` and `Lemma6Hyp.new_writes` hold of the model. |
| `ExampleOffW.model_local` | `SymActionsExample.lean` | none | The system of `off_W_counterexample`, which meets every hypothesis of `step_comm`, meets `Local` as an action model. |

So one model carries the hypotheses of A18 ((G1)–(G6), `WF`, `Built`) and the locality that
Lemmas 5 and 6 assume. What stays outside:

- A `Triggered` parameter's existence counts as read. The code counts it only when the
  parameter's mark guard is found (`triggered_params`). A larger `rd` keeps `Local` and can
  only grow `T`.
- The fields of `Lemma6Hyp` besides `local_`, `shape` and `new_writes` concern the split, the
  folds and `X` (`Ext`), which the slot model does not build. They stay hypotheses, and so do
  `FoldData` and `mono` for Lemma 5 (c)–(e).
- `Local` is proved for the slot model as it stands, with the differences from
  `each_successor` that [Theorem A18 over slots](#theorem-a18-over-slots) lists.

### The partition certificate and monotone variables

<!-- constrained-by #what-is-not-covered -->

`Sweep.lean` and `SweepExample.lean` model the check that `Classes::of_graph` runs on each
partition before the reduction uses it, `partition/reference.rs::stable`, and prove that it
gives `Stable`. `Monotone.lean` and `MonotoneExample.lean` model the write graph of
`reduce/split.rs::TransitionSystem::monotone_vars` and prove what the reduction uses of it:
the proof's (iii) in Theorem A15, and case 4 of Definition 5 with no class marked.

The run (`Sweep.ExplRun`) is what the check reads: `n` states by number, the silent edges
and the rises as pairs of numbers (`reduce/partition.rs::split_edges`), and each state's
label (`Partition::initial`). `Sweep.Complete` says it is `X`'s reachable graph and that a
label fixes `obs` and the input bytes. The check (`Sweep.Cert`) has three parts: a block lies
in one label (B1); for every splitter, the states the sweep of `partition/reference.rs::stuttering_blocks` marks
(`Sweep.Marked`: outside the splitter with a silent edge into it, or a silent predecessor
in its own block of a marked state, self-loops dropped) are every state of a block or none
(B2); every state enters by a rise the blocks its block's first state enters (B3).
`Sweep.check` is a Boolean checker over the same lists, for a checker of a dumped partition.

| Lean (`RdraIsh.`) | File | Claim | Statement |
|---|---|---|---|
| `Sweep.marked_iff` | `Sweep.lean` | A8, (B2) | On a complete run, a reachable state is marked for a splitter iff it leaves its block into the splitter by a silent path through its block and one more silent edge (`SilentExit`). |
| `Sweep.stable_of_cert` | `Sweep.lean` | A8, (B1)–(B3) | On a complete run, a partition that passes the certificate is `Stable`, with each block's first state as its representative, the one `of_graph` takes. |
| `Sweep.cert_of_stable` | `Sweep.lean` | A8, (B1)–(B3) | On a complete run whose labels are exactly the values of `obs` and the input bytes, a `Stable` partition passes the certificate. |
| `Sweep.b2_of_marking` | `Sweep.lean` | A8, (B2) | Any marking per splitter that lies inside `Marked`, is closed under its two rules and is all or none on each block gives the certificate's (B2). A checker's own marking needs only those three facts. |
| `Sweep.marked_of_marks`, `Sweep.check_sound`, `Sweep.stable_of_check` | `Sweep.lean` | A8, (B1)–(B3) | The marking by rounds lies inside `Marked`. A partition that `check` accepts passes the certificate, so on a complete run it is `Stable`. |
| `ExampleSweep.accepts`, `accepted_cert`, `rejects`, `not_cert` | `SweepExample.lean` | none | On three states with one silent edge, `check` accepts the identity partition and rejects a block that one state leaves and the other does not, which fails the certificate too. |
| `Mono.chain_of_change`, `Mono.finite_changes` | `Monotone.lean` | A15, (iii) | A sequence whose steps keep a value or follow an edge of a graph goes, across a change, from one value to another by a chain of edges. If the graph is acyclic and the values lie in a list, it changes value finitely often. |
| `Mono.finite_changes_of_behaviour`, `Mono.settles` | `Monotone.lean` | A15, (iii) | A cell of a state whose steps from reachable states follow an acyclic graph changes finitely often along every behaviour; if finitely many such cells fix the state, every behaviour is constant from some index on. |
| `Mono.no_silentCycle`, `Mono.a15_monotone` | `Monotone.lean` | A15, Definition 5 case 4 | If the cells of `S` fix it and each follows an acyclic graph along the silent edges, no block of any partition holds a silent cycle, so `a15` holds with `canLoop := fun _ => False`: `s_cycles` false. |
| `Mono.Sys.succ_covers`, `Mono.Sys.monotone_finite_changes` | `Monotone.lean` | A10, A15 | A successor of `each_successor` from a pre-state that meets `Pre` keeps each cell of a variable that is not cyclic or moves it along an edge of the graph `monotone_vars` builds. So a variable it marks monotone changes finitely often, slot by slot, along every behaviour of the system. |
| `Mono.Sys.existence_monotone` | `Monotone.lean` | Lemma 5 (d) | An existence that only creates and deletes write, and that is not cyclic, is monotone. |
| `ExampleMono.no_cycle`, `ExampleMono.eventually_flag` | `MonotoneExample.lean` | none | A cone whose `S` variable only rises: every hypothesis of `a15_monotone` is discharged and `eventually flag` holds under `Fair_P ∧ WF_vars(Next)` from a run with no class marked. |
| `ExampleMonoSys.level_monotone`, `ExampleMonoSys.ex_monotone`, `ExampleMonoSys.steps_covered` | `MonotoneExample.lean` | none | A system with a create and a transition `0 → 1 → 2`: its column and its existence are `Monotone`, and a create step and a raise step meet `Pre`, so `succ_covers` applies to them. |

#### What these hypotheses are

- **`Sweep.Complete`**: the run is `X`'s reachable graph, numbered, with its silent edges
  and rises exactly, and a label fixes `obs` and the input bytes. That is the flat case, a
  run of `X` with no inner reduction and not up to `H`; otherwise the check reads the stored
  graph.
- **The marking.** `Sweep.Marked` is the least set closed under the sweep's two rules. That
  the depth-first marking of `partition/reference.rs::stuttering_blocks` computes it, and that `partition/reference.rs::same_partition` of
  the sweep's result holds iff the sweep split nothing (a split adds a block, and a sweep
  that splits nothing never changes the blocks its splitters see), are read from the code.
  `stable` renumbers the blocks by first appearance, which is injective.
- **`Mono.Sys`, `Mono.Sys.Succ`**: the effects (`Effect`) and the edges `monotone_vars`
  adds for each, and `each_successor`: the creates, assignments, single-arm transitions and
  ticks in order, then the deletes, then the open choices, a choice of a deleted row dropped.
  `Acyclic` is the graph with no cycle but self-loops; that the depth-first search of
  `reduce/split.rs::acyclic` decides it is read from the code.
- **`Mono.Sys.Pre`**, of the pre-state of every reachable step, from the guards and roles:
  a created row is absent and every cell an assignment or a choice of the creating action
  writes on it is at base (their `froms` are the base value only; the create's `New`
  parameter binds only an absent slot, `ParamRole::binds_exists`, and `SymActions.lean`'s
  `W`, the states whose absent rows hold base values, is the same invariant), a deleted row is
  present (the delete's `Existing` parameter binds only a present slot), each cell an
  assignment or a choice writes holds a value of its domain (`froms` elsewhere are the whole
  domain), a slot an assignment names lies among its entity's slots, and the clock ticks
  only below `int_max` (the `Tick` action's guard). With `Pre`, every case of
  `monotone_vars` is sound for the steps; no case computes an edge set the proof cannot use.
- **The cells of `S`** (`Mono.no_silentCycle`): `S` is fixed by the cells of the variables
  of `X` that are not inputs, which is what `partition.rs::s_cycles` reads, through
  `monotone_vars` of the extended system. On an explored `X`, `of_graph` leaves every class
  unmarked when `s_cycles` is false, and `a15_monotone` applies as stated. For `obs = ∅`
  the code's one class covers every input byte, which is not (B1).
- **What `check` does not do.** `Sweep.check` checks no part of `Extended`, takes each
  block's first state as its representative rather than reading one, and runs over lists.
  `PCert.Data.check` ([above](#the-partitions-and-the-shape-of-x)) runs over arrays, reads
  the dumped representatives, checks `Extended` on the reachable part of `X`, and reaches
  (B2) through `Sweep.b2_of_marking`. That either accepts every partition that passes
  `Cert` is not proved.


# Chapter 10 Evaluation

*Source: `docs/paper/eval/README.md`.*

## Evaluation (roadmap T4, T6, T7, T8, T10, T11)

Every table here is the output of a script on a data file in [data/](eval/data). The commands are in
[artifact.md](artifact.md). Main at `9e4a123`. Machine: a 4-core x86_64 Linux VM, 15 GiB of
memory, kernel 6.18. Build: `cargo build --release -p rdra-ish-cli --features perf`. Each run is one
`verify` process at `RDRA_JOBS=1` (except the determinism table), with `--memory 12GiB` and a
timeout. CPU is the process's user CPU and memory its peak RSS, both from `wait4`. States are the
sum over the run's BFS runs (`perf-run` lines), and "largest run" is the largest of them.

| Page | Question | Data |
|---|---|---|
| [ablation.md](eval/ablation.md) | T6: what does each of cone, sub-lifecycle reduction and symmetry buy? | `data/ablation.tsv`, `data/whole.tsv` |
| [scaling.md](eval/scaling.md) | T7: states, CPU and memory against the instance count, with and without symmetry | `data/scaling.tsv` |
| [minimal.md](eval/minimal.md) | T4: the least instance count of each known counterexample | `data/minimal.tsv` |
| [determinism.md](eval/determinism.md) | T10: is the output the same across thread counts and runs? | `data/determinism.tsv` |
| [tlc-baseline.md](eval/tlc-baseline.md) | T8: agreement with TLC, from CI logs | `data/tlc-ci-lines.txt` |

### Threats that apply to every table

- **One machine, one run.** CPU times are from a single run on a VM. The verdict and state columns
  are deterministic (see [determinism.md](eval/determinism.md)); the CPU and memory columns are not.
  Instructions retired were not available on this VM.
- **Background load.** The whole-model runs (`paper_whole`) and the ablation ran at the same time on
  separate cores. Each is single-threaded, and user CPU is per process.
- **Models written by the authors** (T9). See Paper 1, threat T1.

### T11: the TLC-side mutation run

**Result: a kill rate of 80.4% on a systematic sample** (37 of 46 viable mutants; 95% Wilson
interval 66.8% to 89.3%). The sample is one mutant in 12 of the 569 that `.cargo/mutants-tlc.toml`
lists: cargo-mutants' round-robin shards 0/24 and 12/24, which together are shard 0/12. It ran
locally with cargo-mutants 27.1.0 (the CI version), TLC 1.7.4 and z3 5.4.1 from nixpkgs, and the CI
job's test command and timeouts (`--baseline skip --timeout 900`). Main at `9e4a123`. Of the 48
mutants: 37 caught, 9 missed, 2 unviable, 0 timeouts. Data:
[data/mutants-tlc-sample.txt](eval/data/mutants-tlc-sample.txt).

The nine survivors. Each marks code that no test in the mutant's test command (the corpus under
TLC and z3, the TLC reader's unit tests, five `verify_cli` tests) distinguishes from its mutant.
Whether each is an equivalent mutant or a missing test needs a look at each one.

| File | Mutant | Area |
|---|---|---|
| `tla/pred.rs:137` | delete the `CmpOp::Ne` arm of `Ctx::cmp` | printing of `!=` in predicates |
| `tla/mod.rs:133` | `==` to `!=` in `render_value` | printing of values |
| `tla/module.rs:917` | `==` to `!=` in `from_guarded` | printing of guarded effects |
| `tla/module.rs:974` | `bindings` returns `[[1]]` | the bindings table of the module |
| `tla/names.rs:100` | `+=` to `*=` in `Taken::claim` | name disambiguation |
| `tla/names.rs:301` | `rise_table` returns `Some("xyzzy")` | the name of the rise table |
| `tla/names.rs:439` | `slot_keys` returns `[]` | slot key names |
| `tlc_output.rs:387` | a match guard to `false` in `parse_block` | reading TLC's trace blocks |
| `tlc_output.rs:442` | `+=` to `*=` in `split_top` | splitting nested TLC values |

The full run did not complete in CI (below). At about 210 s per mutant, the 569 mutants need about
33 hours on one machine.

#### The attempt in CI

The `Mutants` workflow (`.github/workflows/mutants.yml`) with `config=tlc` mutates the TLA+ printer
(`crates/rdra-ish-emit/src/tla/`), the comparison (`verify/compare.rs`) and the TLC output reader
(`tlc_output.rs`, `tlc_replay.rs`). It tests each mutant with the engine corpus under TLC and z3.
No run of it had completed before this work: the one earlier run (46052978676 on branch
`mutants-tlc`, 2026-11-27) was cancelled. This work started run
[46128666288](https://github.com/manji-0/rdra-ish-dsl/actions/runs/46128666288) on main, with four
shards, on 2026-11-28.

**Result in CI: no kill rate.** All four shards were lost before they uploaded results ("The runner has
received a shutdown signal"), after 10, 26, about 58 and about 62 minutes. The summary job found 0
artifacts. What the logs show:

- Each shard lists about 143 mutants (shard 0: 143; shard 3: 142), so about 570 in all.
- Each mutant's test (the engine corpus under TLC and z3, the TLC reader's unit tests and five
  `verify_cli` tests) takes about 280 s. One shard would need about 11 hours, while the workflow
  allows 330 minutes per shard step. The whole run is about 45 runner-hours.
- Shard 0 tested two mutants before it was lost. Both were **missed**:
  `tlc_output.rs:90:58` (`||` to `&&` in `TlcRun::parse`) and `tlc_output.rs:184:52` (`&&` to `||` in
  `violation_kind`). Two survivors say nothing about a rate, but both are worth a test of their own.
- The `tlc` configuration runs without an address-space cap, because the JVM cannot start under one.
  A mutant that makes the checker allocate without bound would exhaust the runner, which matches
  the shutdowns. The logs do not show the cause.

**What completing T11 needs** (a workflow change and an owner's decision on the CI cost): about 24
shards or a narrower `re` per run; a per-mutant memory cap that the JVM tolerates (a cgroup limit
such as `systemd-run --scope -p MemoryMax=`, or `prlimit` on the `rdra-ish` process with
`RDRA_TLC_HEAP` set); and a shorter test command per mutant (the corpus cases that reach the
mutated file). Paper 2 reports the two earlier liveness-ladder runs (66.5% and 62.9%) and states
that no reduction-side kill rate exists.


# Chapter 10.1 Ablation

*Source: `docs/paper/eval/ablation.md`.*

## Ablation: cone, sub-lifecycle reduction, symmetry (roadmap T6)

**Question.** What does each reduction buy, in states, CPU and memory, and does any of them change
a verdict?

**Configurations.** Every check runs on a cone of influence in all four configurations. The
configurations switch the other two reductions:

| Configuration | Sub-lifecycle reduction | Instance symmetry |
|---|---|---|
| `cone` | off (`RDRA_ISH_NO_REDUCE=1`) | off (`--symmetry never`) |
| `cone+reduce` | on (the default) | off (the default) |
| `cone+sym` | off | on (`--symmetry always`) |
| `cone+reduce+sym` | on | on |

A fifth column, *no cone*, is BFS on the whole model with no cone, no reduction and no symmetry
(`crates/rdra-ish-core/examples/paper_whole.rs`, under a state cap). Data:
[data/ablation.tsv](eval/data/ablation.tsv) and [data/whole.tsv](eval/data/whole.tsv). Bounds: the
`verify` default of 2 instances per entity and Int 0..5, except oidc-auth at 1 instance (at 2 it
needs more than this machine's memory, about 19.5 GB at the default limit, per the CLI reference).
Timeout 300 s per run, `--memory 12GiB`, `RDRA_JOBS=1`.

### Results

**Whole model, no cone.** On every domain model BFS on the whole model passes 10 million states
without finishing (patent-ops in 77 s, food-permit-ops in 86 s, insurance-claim-ops in 102 s,
oidc-auth at 1 instance in 98 s). clinic-ops did not reach the cap within 400 s. ec-site passes 20
million states in 83 s, while all of its cones together hold 711 states. Only the three smallest
models finish whole (personal-info 209 states, incremental-order 36, api-contract 1).

**Cone and sub-lifecycle reduction.**

| Model | `cone`: result, CPU, peak RSS | `cone+reduce`: result, CPU, peak RSS | Largest run with reduction |
|---|---|---|--:|
| patent-ops | timeout at 300 s, 5.3 GB | ok, 4.0 s, 163 MB | 1,341,707 |
| clinic-ops | timeout at 300 s, 7.4 GB | ok, 10.6 s, 549 MB | 2,238,170 |
| food-permit-ops | timeout at 300 s, 7.4 GB | ok, 20.0 s, 745 MB | 4,846,569 |
| insurance-claim-ops | timeout at 300 s, 5.4 GB | counterexample (expected), 42.3 s, 805 MB | 4,580,087 |
| oidc-auth (1 instance) | ok, 0.6 s, 30 MB | ok, 4.8 s, 304 MB | 1,028,160 |
| four small samples | ok, under 0.01 s | ok, same states | 532 at most |

- On the four larger domain models, `verify` finishes only with the reduction. Without it, each
  run is stopped by the 300 s timeout after 5.3 to 7.6 GB. This table cannot say how long an
  unreduced run would take. The states column of a stopped run counts only the BFS runs that
  finished before the timeout. That is why food-permit-ops shows 43 states: its first large cone
  did not finish.
- insurance-claim-ops reports `Deadlock_ClaimCase_status_assessed`, the counterexample its sample
  header expects (`// expect verify: fail Deadlock_ClaimCase_status_assessed`).
- **The reduction can cost.** On oidc-auth at one instance, every cone is small (30,277 states at
  most), but the extended sub-lifecycle `X` that the reduction explores first has 1,028,160 states.
  `X` leaves the inputs free, so it can be larger than the cone it is meant to shrink. The
  reduction gives up afterwards, and the run costs 4.8 s instead of 0.6 s. This is the case that
  an explore-budget heuristic for `X` would address.
- On the four small samples the reduction does nothing: no cone has an independent sub-lifecycle
  worth splitting.

**Symmetry.** At 2 instances, `--symmetry always` changes the states by 0 to 16% and the CPU by
less than the noise:

| Model | States, `cone+reduce` | States, `cone+reduce+sym` | Fewer |
|---|--:|--:|--:|
| ec-site | 711 | 665 | 6.5% |
| personal-info | 118 | 103 | 12.7% |
| incremental-order | 19 | 16 | 15.8% |
| patent-ops | 2,017,009 | 1,942,129 | 3.7% |
| clinic-ops | 5,617,011 | 5,495,861 | 2.2% |
| food-permit-ops | 7,009,703 | 6,998,263 | 0.2% |
| insurance-claim-ops | 13,499,587 | 13,444,059 | 0.4% |
| oidc-auth (1 instance) | 1,345,035 | 1,345,035 | 0% |

At two slots per entity the largest orbit has two members, and the tool applies the group only to
some cones: the safety cones (on a reduced cone, up to the group of its `T` rows), `always` on
cones the reduction leaves whole, and `eventually` and `leads_to`. With the reduction off, symmetry
does not rescue the timed-out runs either: `cone+sym` stops at the same point as `cone`. The
symmetry factor grows with the instance count, which [scaling.md](eval/scaling.md) measures. At one
instance, `G_s` is trivial.

**Verdicts.** No configuration changes a verdict. Every run that finished printed the same `result:`
line as every other finished run of its model. The same failing checks were reported, the
expected deadlock on insurance-claim-ops and nothing elsewhere. Each finished domain-model run
matches its sample's `// expect` header. The *Same verdicts* column below compares each run with
the `cone` run, and shows `-` where that run timed out.

### Threats

- One run per cell, on a 4-core VM. The CPU and RSS columns would move between runs; the states
  and verdict columns would not ([determinism.md](eval/determinism.md)).
- The timeout hides how far the unreduced runs are from finishing. The whole-model column shows
  the scale: more than 10 million states on every domain model.
- The `perf` build adds counters to every BFS run. It changes no verdict and no state count.

### Full table

| Model | Configuration | Result | States (all runs) | Largest run | Peak RSS (MB) | User CPU (s) | Same verdicts |
|---|---|---|---:|---:|---:|---:|---|
| samples/ec-site | cone | ok | 711 | 532 | 15 | 0.01 | yes |
| samples/ec-site | cone+reduce | ok | 711 | 532 | 15 | 0.01 | yes |
| samples/ec-site | cone+sym | ok | 665 | 532 | 15 | 0.0 | yes |
| samples/ec-site | cone+reduce+sym | ok | 665 | 532 | 15 | 0.01 | yes |
| samples/personal-info | cone | ok | 118 | 45 | 15 | 0.01 | yes |
| samples/personal-info | cone+reduce | ok | 118 | 45 | 15 | 0.01 | yes |
| samples/personal-info | cone+sym | ok | 103 | 45 | 15 | 0.01 | yes |
| samples/personal-info | cone+reduce+sym | ok | 103 | 45 | 15 | 0.01 | yes |
| samples/api-contract | cone | ok | 1 | 1 | 15 | 0.0 | yes |
| samples/api-contract | cone+reduce | ok | 1 | 1 | 15 | 0.0 | yes |
| samples/api-contract | cone+sym | ok | 1 | 1 | 15 | 0.0 | yes |
| samples/api-contract | cone+reduce+sym | ok | 1 | 1 | 15 | 0.0 | yes |
| samples/incremental-order/step-7-business-rules/src | cone | ok | 19 | 9 | 15 | 0.0 | yes |
| samples/incremental-order/step-7-business-rules/src | cone+reduce | ok | 19 | 9 | 15 | 0.0 | yes |
| samples/incremental-order/step-7-business-rules/src | cone+sym | ok | 16 | 9 | 15 | 0.0 | yes |
| samples/incremental-order/step-7-business-rules/src | cone+reduce+sym | ok | 16 | 9 | 15 | 0.0 | yes |
| samples/patent-ops | cone | timeout | 329,978 | 323,071 | 5255 | 296.32 | - |
| samples/patent-ops | cone+reduce | ok | 2,017,009 | 1,341,707 | 163 | 3.96 | - |
| samples/patent-ops | cone+sym | timeout | 329,978 | 323,071 | 5575 | 295.61 | - |
| samples/patent-ops | cone+reduce+sym | ok | 1,942,129 | 1,341,707 | 163 | 3.65 | - |
| samples/clinic-ops | cone | timeout | 120,873,184 | 105,791,435 | 7435 | 294.5 | - |
| samples/clinic-ops | cone+reduce | ok | 5,617,011 | 2,238,170 | 549 | 10.61 | - |
| samples/clinic-ops | cone+sym | timeout | 120,873,184 | 105,791,435 | 7578 | 294.58 | - |
| samples/clinic-ops | cone+reduce+sym | ok | 5,495,861 | 2,238,170 | 549 | 12.31 | - |
| samples/food-permit-ops | cone | timeout | 43 | 43 | 7372 | 295.59 | - |
| samples/food-permit-ops | cone+reduce | ok | 7,009,703 | 4,846,569 | 745 | 19.97 | - |
| samples/food-permit-ops | cone+sym | timeout | 43 | 43 | 7372 | 295.64 | - |
| samples/food-permit-ops | cone+reduce+sym | ok | 6,998,263 | 4,846,569 | 745 | 20.02 | - |
| samples/insurance-claim-ops | cone | timeout | 93,277,095 | 93,277,095 | 5443 | 295.99 | - |
| samples/insurance-claim-ops | cone+reduce | counterexample | 13,499,587 | 4,580,087 | 805 | 42.26 | - |
| samples/insurance-claim-ops | cone+sym | timeout | 93,277,095 | 93,277,095 | 5355 | 295.65 | - |
| samples/insurance-claim-ops | cone+reduce+sym | counterexample | 13,444,059 | 4,580,087 | 805 | 42.3 | - |
| samples/oidc-auth --instance-count 1 | cone | ok | 316,826 | 30,277 | 30 | 0.63 | yes |
| samples/oidc-auth --instance-count 1 | cone+reduce | ok | 1,345,035 | 1,028,160 | 304 | 4.82 | yes |
| samples/oidc-auth --instance-count 1 | cone+sym | ok | 316,826 | 30,277 | 30 | 0.63 | yes |
| samples/oidc-auth --instance-count 1 | cone+reduce+sym | ok | 1,345,035 | 1,028,160 | 304 | 4.86 | yes |


# Chapter 10.2 Scaling

*Source: `docs/paper/eval/scaling.md`.*

## Scaling in the instance count, plain and symmetric (roadmap T7)

**Question.** How do states, CPU and memory grow with the number of instances per entity, and what
does instance symmetry (`--symmetry always`) save as the count grows?

**Method.** `scripts/paper/eval.py scaling` runs `verify` at `--instance-count` 1 to 4, with
`--symmetry never` and `always`, on ec-site, personal-info, patent-ops and clinic-ops, with the
reduction on (the default). Each run has a 300 s timeout and `--memory 12GiB`, and a series stops
at its first timeout. A second measurement runs patent-ops at 3 instances under `--memory 4GiB` and
a 900 s timeout, both ways. The tool reruns a cone up to symmetry when its plain run stops at the
memory limit, so a lower limit is the setting where symmetry is meant to help. Data:
[data/scaling.tsv](eval/data/scaling.tsv), [data/scaling-memory.tsv](eval/data/scaling-memory.tsv).

### Results

**Growth.** States grow by one to three orders of magnitude per added instance:

| Model | n=1 | n=2 | n=3 | n=4 |
|---|--:|--:|--:|--:|
| ec-site | 44 | 711 | 26,094 | 1,459,159 (3.2 s) |
| personal-info | 27 | 118 | 659 | 4,878 |
| patent-ops | 1,204 | 2,017,009 (3.6 s) | timeout at 300 s, 6.0 GB | — |
| clinic-ops | 1,063 | 5,617,011 (11.7 s) | killed at 192 s, 15.6 GB | timeout |

(States summed over every BFS run of `verify`, symmetry off.)

- clinic-ops at 3 instances was killed by the kernel's out-of-memory killer after 192 s, with a peak
  RSS of 15.6 GB on a 15 GiB machine, although `--memory 12GiB` was set. `dmesg` records the two
  kills (symmetry off and on). This is the known gap of claim A26: the memory limit bounds the BFS
  stores, not the process.
- patent-ops at 3 instances does not finish in 300 s (12 GiB), nor in 900 s under 4 GiB: about 86
  million states in total, symmetry off and on alike.

**Symmetry.** Symmetry never shrinks the largest run of ec-site, patent-ops or clinic-ops at any
count:

| Model | n | Largest run, plain | Largest run, `always` | All runs, plain | All runs, `always` |
|---|--:|--:|--:|--:|--:|
| ec-site | 4 | 1,411,966 | 1,411,966 | 1,459,159 | 1,435,913 |
| personal-info | 3 | 273 | 273 | 659 | 453 |
| personal-info | 4 | 2,713 | 1,589 | 4,878 | 2,200 |
| patent-ops | 2 | 1,341,707 | 1,341,707 | 2,017,009 | 1,942,129 |
| patent-ops | 3, 4 GiB | — | — | 85,913,758 (timeout 900 s) | 85,913,758 (timeout 900 s) |
| clinic-ops | 2 | 2,238,170 | 2,238,170 | 5,617,011 | 5,495,861 |

- The savings are in the small cones: 1.6 to 55% of all states, up to 2.2× on personal-info at 4
  instances. The largest run of each domain model is one that the group leaves alone. Either its
  classes are fixed by the scan (on a reduced cone, the group acts only on the `T` rows, and the
  sub-lifecycle's rows are fixed), or it is the extended sub-lifecycle `X`, which runs plain unless
  its run stops at the memory limit.
- Under 4 GiB and 900 s, patent-ops at 3 instances gives the same totals both ways, so neither run
  reached a cone where the orbit rerun applies before the timeout.
- The only symmetry factors on record with both sides complete are 3.0×, 5.2× and 5.5× on three
  reduced cones of patent-ops at 3 instances, up to the group of their `T` rows (S7 in
  [../evidence/measurements.md](evidence/measurements.md); 29,680,875 states against 5,413,500
  orbits on the largest). This evaluation does not reproduce them: no end-to-end run here reached
  those cones within its budget. A per-cone measurement at the pinned commit is needed before the
  paper quotes a symmetry factor.

**Reading.** On these models, the reduction and the cone carry the gains, and symmetry saves little
end to end. This is one reason Paper 1 leads with the sub-lifecycle quotient
([contribution.md](contribution.md)). Symmetry's case is soundness, not speed: Theorem 5 lets
liveness be checked on the quotient at all, while TLC's symmetry sets would not be sound here.

### Threats

- One run per cell; four models; timeouts of 300 s and 900 s. A longer budget would show where the
  unfinished runs end.
- The machine's 15 GiB bounds what can finish. Runs that need more memory were killed, not stopped.

### Tables

#### Instance counts 1 to 4 (`--memory 12GiB`, 300 s)

| Model | Run | Result | States (all runs) | Largest run | Peak RSS (MB) | User CPU (s) |
|---|---|---|---:|---:|---:|---:|
| samples/ec-site | n=1 sym=never | ok | 44 | 18 | 15 | 0.01 |
| samples/ec-site | n=2 sym=never | ok | 711 | 532 | 15 | 0.01 |
| samples/ec-site | n=3 sym=never | ok | 26,094 | 23,670 | 17 | 0.04 |
| samples/ec-site | n=4 sym=never | ok | 1,459,159 | 1,411,966 | 184 | 3.22 |
| samples/ec-site | n=1 sym=always | ok | 44 | 18 | 15 | 0.0 |
| samples/ec-site | n=2 sym=always | ok | 665 | 532 | 15 | 0.0 |
| samples/ec-site | n=3 sym=always | ok | 25,014 | 23,670 | 17 | 0.04 |
| samples/ec-site | n=4 sym=always | ok | 1,435,913 | 1,411,966 | 184 | 3.21 |
| samples/personal-info | n=1 sym=never | ok | 27 | 7 | 15 | 0.01 |
| samples/personal-info | n=2 sym=never | ok | 118 | 45 | 15 | 0.01 |
| samples/personal-info | n=3 sym=never | ok | 659 | 273 | 15 | 0.0 |
| samples/personal-info | n=4 sym=never | ok | 4,878 | 2,713 | 16 | 0.01 |
| samples/personal-info | n=1 sym=always | ok | 27 | 7 | 15 | 0.01 |
| samples/personal-info | n=2 sym=always | ok | 103 | 45 | 15 | 0.01 |
| samples/personal-info | n=3 sym=always | ok | 453 | 273 | 15 | 0.0 |
| samples/personal-info | n=4 sym=always | ok | 2,200 | 1,589 | 16 | 0.01 |
| samples/patent-ops | n=1 sym=never | ok | 1,204 | 547 | 20 | 0.05 |
| samples/patent-ops | n=2 sym=never | ok | 2,017,009 | 1,341,707 | 163 | 3.55 |
| samples/patent-ops | n=3 sym=never | timeout | 5,282,872 | 5,167,635 | 5974 | 295.83 |
| samples/patent-ops | n=1 sym=always | ok | 1,204 | 547 | 20 | 0.04 |
| samples/patent-ops | n=2 sym=always | ok | 1,942,129 | 1,341,707 | 163 | 3.51 |
| samples/patent-ops | n=3 sym=always | timeout | 5,282,872 | 5,167,635 | 5608 | 295.25 |
| samples/clinic-ops | n=1 sym=never | ok | 1,063 | 112 | 22 | 0.06 |
| samples/clinic-ops | n=2 sym=never | ok | 5,617,011 | 2,238,170 | 549 | 11.74 |
| samples/clinic-ops | n=3 sym=never | ? | 61,849,224 | 61,849,224 | 15573 | 192.45 |
| samples/clinic-ops | n=4 sym=never | timeout | 0 | 0 | 13975 | 290.93 |
| samples/clinic-ops | n=1 sym=always | ok | 1,063 | 112 | 21 | 0.07 |
| samples/clinic-ops | n=2 sym=always | ok | 5,495,861 | 2,238,170 | 547 | 10.75 |
| samples/clinic-ops | n=3 sym=always | ? | 61,849,224 | 61,849,224 | 15574 | 198.34 |
| samples/clinic-ops | n=4 sym=always | timeout | 0 | 0 | 14170 | 291.27 |

#### patent-ops at 3 instances under `--memory 4GiB`, 900 s

| Model | Run | Result | States (all runs) | Largest run | Peak RSS (MB) | User CPU (s) |
|---|---|---|---:|---:|---:|---:|
| samples/patent-ops | n=3 sym=never memory=4GiB | timeout | 85,913,758 | 68,021,867 | 4768 | 883.96 |
| samples/patent-ops | n=3 sym=always memory=4GiB | timeout | 85,913,758 | 68,021,867 | 4768 | 884.65 |


# Chapter 10.3 Least instance counts

*Source: `docs/paper/eval/minimal.md`.*

## Least instance count of each known counterexample (roadmap T4)

**Question.** At how many instances per entity does each known counterexample first appear?
This supports the bounded-verification argument (`verify` defaults to 2 instances) and the split
between BFS and `--prove`.

**Method.** `scripts/paper/eval.py minimal` takes every case of the engine corpus
(`crates/rdra-ish-cli/tests/engine_corpus/`, 91 cases) whose header expects a native
counterexample, and every `// expect verify ...: fail` header of `samples/formal-verification`
and `samples/formal-verification-fail`. It runs `verify` at the header's flags with every
`--instance-count` and `--instances` removed, at `--instance-count` 1, 2 and 3. For each
expected failing check it records the least count whose run reports `error[<check>]`, and the
length of the printed counterexample at that count. Data: [data/minimal.tsv](eval/data/minimal.tsv).
Main at `9e4a123`.

**Result.** 132 expected counterexamples:

| Least uniform instance count | Counterexamples |
|---|--:|
| 1 | 106 |
| 2 | 24 |
| 3 | 0 |
| not found at 1 to 3 | 2 |

- No expected counterexample needs three uniform instances where two do not suffice. This covers only
  the counterexamples that the headers expect at their own bounds. The prover's escalation finds five
  more corpus rules that are `ok` at 2 instances and fail at 3 or 4, all of them counting rules
  ([prove.md](eval/prove.md)).
- The two not found are expected. `safety_count_per_entity` / `Quantifier_Member_0` needs the
  header's per-entity count `--instances Loan=4`, which a uniform count of 3 does not reach.
  `cap_memory_limit` / `NoShip` is a case about the memory cap: its header sets
  `--memory 2KiB`, and the script passes `--memory 12GiB` after the header's flags, so the run
  does not stop where the header expects it to.
- The 24 cases that need two instances are those the corpus wrote to need two rows: uniqueness
  (`Unique_*`), counting quantifiers, foreign keys that move (`presence_along_key_moves`), the
  asymmetry of lowest-slot creation (`presence_first_absent_asymmetry`), the symmetry cases
  (`sym_*`), and seven deadlock findings.
- Of the 84 counterexamples with a printed trace length, the mean length is 3.3 steps and the
  longest is 9. At their least count, those at one instance average 3.1 steps (67 traces), and
  those at two instances 4.1 steps (17 traces). Deadlock and witness findings print no step count.

**Reading.** On these models, two instances find every expected counterexample that a uniform bound
finds. Counting rules are the exception: [prove.md](eval/prove.md) shows five that need 3 or 4. This is evidence about a corpus written to exercise the checker, not a small-scope theorem.
The corpus cases are adversarial by design, so they over-represent the two-row cases. The domain
samples have one expected counterexample (`insurance-claim-ops`,
`Deadlock_ClaimCase_status_assessed` at 2 instances), outside this table.

### Table

| Case | Check | Least n | Steps at least n |
|---|---|---:|---:|
| cap_memory_limit.rdra | NoShip | >3 | - |
| cone_entity_mark_left_out.rdra | Deadlock_Order_status_placed | 1 | - |
| cone_entity_mark_left_out.rdra | Witness_Order_status_shipped | 1 | - |
| cone_entity_mark_left_out.rdra | Witness_Notice_status_received | 1 | - |
| cone_entity_mark_left_out.rdra | Witness_Notice_status_read | 1 | - |
| cone_entity_mark_left_out.rdra | Witness_Payment_status_paid | 1 | - |
| cone_entity_mark_left_out.rdra | Deadlock_Payment_status_unpaid | 1 | - |
| cone_fold_key_dropped.rdra | Forbidden_Order_0 | 1 | 9 |
| cone_handler_mark_left_out.rdra | ShipsUnfair | 1 | 3 |
| cone_handler_mark_left_out.rdra | ShipsFair | 1 | 3 |
| cone_handler_mark_left_out.rdra | Deadlock_Order_status_paid | 1 | - |
| cone_independent_loop.rdra | PaidShipsUnfair | 1 | 5 |
| cone_receipt_frees_slots.rdra | PendingPays | 1 | 2 |
| cone_reduced_class_loops.rdra | JobStarts | 1 | 3 |
| cone_reduced_class_loops.rdra | JobStartsUnfair | 1 | 3 |
| cone_reduced_class_loops.rdra | JobWaits | 1 | 5 |
| cone_reduced_inner_class_loops.rdra | JobStarts | 1 | 3 |
| cone_reduced_loop_unreached.rdra | Deadlock_Gate_phase_pa | 1 | - |
| cone_reduced_loop_unreached.rdra | Deadlock_Job_status_waiting | 1 | - |
| cone_rise_names_a_dropped_new_row.rdra | Forbidden_Order_0 | 1 | 8 |
| cone_rise_names_a_dropped_new_row.rdra | Deadlock_Order_status_paid | 2 | - |
| cone_rise_param_on_a_stub.rdra | Forbidden_Order_0 | 1 | 9 |
| cone_rise_param_on_a_stub.rdra | Deadlock_Order_status_paid | 2 | - |
| cone_triggered_param_on_a_stub.rdra | Forbidden_Order_0 | 1 | 7 |
| cone_triggered_param_on_a_stub.rdra | Deadlock_Order_status_paid | 2 | - |
| fair_counted_exposes_lost_run.rdra | OrderComplete | 1 | 3 |
| fair_counted_exposes_lost_run.rdra | Deadlock_Order_delivered_partial | 1 | - |
| fair_counted_exposes_lost_run.rdra | Witness_Order_delivered_complete | 1 | - |
| fair_creating_use_case.rdra | PendingPays | 1 | 2 |
| fair_enabled_disabled_forever.rdra | PaidShips | 1 | 4 |
| fair_same_edge_two_bindings.rdra | LoopsForever | 1 | 3 |
| fair_selfloop_not_enabling.rdra | PaidShips | 1 | 2 |
| fair_split_use_case.rdra | EndsUnfairSplit | 1 | 5 |
| fair_stop_vs_wf_next.rdra | ShippedReopens | 1 | 3 |
| fair_triggered_handler.rdra | PostedUnfair | 1 | 6 |
| presence_along_key_moves.rdra | CapturedPaid | 2 | 9 |
| presence_along_key_moves.rdra | Deadlock_Order_status_open | 2 | - |
| presence_deleted_row_owes_goal.rdra | PendingPays | 1 | 2 |
| presence_first_absent_asymmetry.rdra | RushFinishes | 2 | 7 |
| presence_first_absent_asymmetry.rdra | Deadlock_Order_status_open | 2 | - |
| presence_goal_present_modifier.rdra | HeldReleasedPresent | 1 | 3 |
| presence_per_entity_slots.rdra | Unique_Loan_tag | 2 | 3 |
| presence_per_entity_slots.rdra | Unique_Room_tag | 2 | 0 |
| safety_at_least_guard.rdra | Quantifier_Claim_0 | 2 | 5 |
| safety_at_least_guard.rdra | Deadlock_Claim_status_open | 1 | - |
| safety_at_least_guard.rdra | Deadlock_Order_status_open | 1 | - |
| safety_can_reach_frees_slots.rdra | OrderCanPay | 1 | 2 |
| safety_count_guard_order.rdra | Deadlock_Shelf_status_open | 2 | - |
| safety_count_per_entity.rdra | Quantifier_Member_0 | >3 | - |
| safety_count_per_entity.rdra | Quantifier_Shelf_1 | 2 | 3 |
| safety_counted_once_per_raise.rdra | Forbidden_Crate_1 | 1 | 4 |
| safety_created_set_beside_transitions.rdra | ConflictingEffects_Order_Place_status | 1 | - |
| safety_delete_cascade.rdra | Forbidden_OrderLine_0 | 1 | 3 |
| safety_delete_set_null.rdra | Forbidden_OrderLine_0 | 1 | 4 |
| safety_edge_free_cone.rdra | Forbidden_Ticket_0 | 1 | 2 |
| safety_edge_free_cone.rdra | Witness_Order_status_refunded | 1 | - |
| safety_event_overflow.rdra | EventOverflow_Inbox_EvNoted_Read | 1 | 4 |
| safety_exactly_along.rdra | Deadlock_Member_status_pending | 1 | - |
| safety_null_columns_cross.rdra | MultiForbidden_Line_1 | 1 | 2 |
| safety_null_columns_cross.rdra | MultiForbidden_Line_2 | 1 | 4 |
| safety_null_columns_eq.rdra | Forbidden_Item_1 | 1 | 3 |
| safety_null_columns_eq.rdra | ABNeverMet | 1 | 3 |
| safety_null_columns_ne.rdra | Forbidden_Item_0 | 1 | 1 |
| safety_null_columns_ne.rdra | Required_Item_0 | 1 | 1 |
| safety_null_derived_in.rdra | Forbidden_Item_2 | 1 | 1 |
| safety_null_derived_in.rdra | Forbidden_Item_5 | 1 | 1 |
| safety_null_derived_in.rdra | Forbidden_Box_7 | 1 | 2 |
| safety_null_enum_bool_columns.rdra | Forbidden_Item_0 | 1 | 1 |
| safety_null_enum_bool_columns.rdra | Forbidden_Item_1 | 1 | 1 |
| safety_null_enum_bool_columns.rdra | Forbidden_Item_3 | 1 | 3 |
| safety_null_enum_bool_columns.rdra | Forbidden_Item_4 | 1 | 2 |
| safety_null_enum_bool_literals.rdra | Forbidden_Item_0 | 1 | 1 |
| safety_null_enum_bool_literals.rdra | Forbidden_Item_3 | 1 | 1 |
| safety_null_enum_bool_literals.rdra | Required_Item_3 | 1 | 1 |
| safety_precedes_latch.rdra | ShippedAfterAudit | 1 | 4 |
| safety_stable_across_delete.rdra | PendingStays | 1 | 2 |
| safety_unique_sets.rdra | Unique_Badge_color | 2 | 2 |
| safety_unique_sets.rdra | Unique_Enrollment_course_id_student_id | 2 | 2 |
| safety_vacuity.rdra | AlwaysDone | 1 | 1 |
| sym_deleted_row_owner.rdra | Quantifier_Mall_0 | 2 | 8 |
| sym_identity_pair.rdra | MultiForbidden_Account_0 | 1 | 1 |
| sym_lift_compose.rdra | Quantifier_Queue_0 | 2 | 6 |
| sym_master_ties.rdra | MultiForbidden_Visit_0 | 1 | 3 |
| sym_master_ties.rdra | Deadlock_Visit_status_booked | 1 | - |
| sym_relink_loop.rdra | TicketCloses | 2 | 5 |
| sym_relink_loop.rdra | TicketClosesFair | 2 | 5 |
| sym_self_owned.rdra | Deadlock_Task_status_open | 1 | - |
| width_nullable_enum_256.rdra | DomainTooLarge_Row_s | 1 | - |
| width_nullable_int_top.rdra | DomainTooLarge_Item_n | 1 | - |
| z3_cti_true_rule.rdra | Deadlock_Door_state_closed | 1 | - |
| z3_cti_true_rule.rdra | Deadlock_Light_state_on | 1 | - |
| z3_inductive.rdra | Forbidden_Order_1 | 1 | 4 |
| z3_int_and_now_boundary.rdra | Forbidden_Item_1 | 1 | 2 |
| z3_int_default_out_of_range.rdra | IntLiteralOutOfBounds_Item_7 | 1 | - |
| z3_int_listing_guard.rdra | Deadlock_Item_status_open | 1 | - |
| z3_int_listing_guard.rdra | Witness_Item_status_flagged | 1 | - |
| z3_int_listing_past_the_top.rdra | Forbidden_Item_0 | 1 | 2 |
| z3_lemma.rdra | Deadlock_Door_state_closed | 1 | - |
| z3_lemma.rdra | Deadlock_Light_state_on | 1 | - |
| z3_lemma_run_mark.rdra | Deadlock_Light_state_on | 1 | - |
| z3_lemma_run_mark.rdra | Deadlock_Sign_state_dark | 1 | - |
| z3_needs_exists.rdra | Deadlock_Order_status_open | 2 | - |
| z3_needs_exists_global.rdra | Deadlock_Gate_state_shut | 1 | - |
| z3_needs_exists_global.rdra | Deadlock_Guard_state_on | 1 | - |
| z3_needs_run_mark.rdra | Deadlock_Light_state_on | 1 | - |
| z3_needs_run_mark.rdra | Deadlock_Sign_state_dark | 1 | - |
| z3_now_past_the_top.rdra | Forbidden_Coupon_0 | 1 | 7 |
| z3_two_ok_three_fails.rdra | Deadlock_Order_status_c | 1 | - |
| z3_two_ok_three_fails.rdra | Witness_Order_status_d | 1 | - |
| after_assert_unmet.rdra | AfterAssert_DeliverOrder_0 | 1 | 4 |
| can_reach_unmet.rdra | OrderCanFinish | 1 | 3 |
| check_unmet.rdra | Check_CartLine_qty_0 | 1 | 2 |
| cross_order_payment_split.rdra | MultiForbidden_Order_0 | 1 | 3 |
| deadlock.rdra | Deadlock_Order_status_paid | 1 | - |
| min_card_unmet.rdra | MinCard_OrderLine_Order | 1 | 1 |
| none_any_child_bypassed.rdra | Unique_Invoice_Order | 2 | 4 |
| now_coupon.rdra | Forbidden_Coupon_0 | 1 | 2 |
| one_to_one_shared.rdra | Unique_Shipment_Order | 2 | 3 |
| order_stability.rdra | CancelledStaysCancelled | 1 | 3 |
| order_stability.rdra | AmountKeptOnceDecided | 1 | 4 |
| order_stability.rdra | ShippedAfterInvoiced | 1 | 4 |
| partial_scope.rdra | DerivedColumnRead_Audit_score | 1 | - |
| quantifier_none.rdra | Quantifier_Cert_0 | 1 | 2 |
| temporal_along.rdra | CapturedPaidAnyOrder | 2 | 1 |
| temporal_fair.rdra | GrantEndsUnfair | 2 | 5 |
| temporal_unmet.rdra | NeverShipped | 1 | 3 |
| temporal_unmet.rdra | PendingLeadsToDelivered | 1 | 3 |
| unique_double_booking.rdra | Unique_Booking_room_id_slot | 2 | 2 |
| untriggered_sets.rdra | Forbidden_Order_0 | 1 | 3 |
| unwitnessed_state.rdra | Witness_Order_status_refunded | 1 | - |
| unwitnessed_state.rdra | Deadlock_Order_status_shipped | 1 | - |
| order.rdra | Inv_Order_0 | 1 | 2 |


# Chapter 10.4 Determinism

*Source: `docs/paper/eval/determinism.md`.*

## Determinism across thread counts and runs (roadmap T10)

**Question.** Does `verify` print the same output whatever the number of threads, and from one run
to the next? Claims N4 and N17 of [docs/theory/claims.md](../theory/claims.md) state that any
number of threads computes the one-thread run: the same states, numbering, edges, findings and stop
point, except for a stop by the wall-clock budget. TLC's multi-worker mode does not promise this:
its state numbering and the counterexample it reports can change with the number of workers
(the issue cites tlaplus issue 883).

**Method.** `scripts/paper/eval.py determinism` runs `verify` on four models at `RDRA_JOBS` 1, 2 and
4, twice each, and compares the whole standard output of every run byte for byte with the first
run's. The output holds every verdict, every state count per cone, and every printed trace.
Data: [data/determinism.tsv](eval/data/determinism.tsv).

**Result.** All 24 runs print the same output as the first run of their model.

| Model | Wall time at 1 job (s) | at 2 jobs | at 4 jobs | Same output |
|---|--:|--:|--:|---|
| patent-ops | 4.0 | 3.5 | 3.5 | 6 of 6 |
| clinic-ops | 11.8 | 6.4 | 6.0 | 6 of 6 |
| food-permit-ops | 20.3 | 10.3 | 9.7 | 6 of 6 |
| oidc-auth (1 instance) | 4.6 | 4.0 | 3.8 | 6 of 6 |

(Means of the two runs.)

**Reading.** `RDRA_JOBS` runs cones and properties in parallel, and the BFS of one cone in parallel.
The output does not depend on it. The single-reader design costs speed-up: from 2 to 4 jobs the
wall time barely changes on these models. One large cone dominates each run, and the parallel BFS
of [native-bfs-parallel.md](../design/native-bfs-parallel.md) keeps the one-thread order. This
table does not separate the two sources of parallelism. The machine has 4 cores, so 4 jobs is its
limit.

**Threats.** Two repeats per setting, on one machine. A nondeterminism that shows only under heavy
contention or on other hardware would not show here. The N4 proof and the open items of N4 and N17
(no loom model, no Miri run) are in the claims inventory.

### Table

| Model | RDRA_JOBS | Repeat | Wall (s) | User CPU (s) | Same stdout as first run |
|---|---:|---:|---:|---:|---|
| samples/patent-ops | 1 | 0 | 4.02 | 3.87 | yes |
| samples/patent-ops | 1 | 1 | 3.98 | 3.85 | yes |
| samples/patent-ops | 2 | 0 | 3.4 | 7.39 | yes |
| samples/patent-ops | 2 | 1 | 3.6 | 7.62 | yes |
| samples/patent-ops | 4 | 0 | 3.58 | 9.06 | yes |
| samples/patent-ops | 4 | 1 | 3.4 | 9.15 | yes |
| samples/clinic-ops | 1 | 0 | 11.98 | 11.44 | yes |
| samples/clinic-ops | 1 | 1 | 11.56 | 11.07 | yes |
| samples/clinic-ops | 2 | 0 | 6.43 | 14.33 | yes |
| samples/clinic-ops | 2 | 1 | 6.43 | 14.73 | yes |
| samples/clinic-ops | 4 | 0 | 6.62 | 15.06 | yes |
| samples/clinic-ops | 4 | 1 | 5.45 | 13.99 | yes |
| samples/food-permit-ops | 1 | 0 | 19.53 | 18.89 | yes |
| samples/food-permit-ops | 1 | 1 | 21.09 | 20.3 | yes |
| samples/food-permit-ops | 2 | 0 | 9.88 | 23.78 | yes |
| samples/food-permit-ops | 2 | 1 | 10.77 | 25.86 | yes |
| samples/food-permit-ops | 4 | 0 | 9.83 | 30.67 | yes |
| samples/food-permit-ops | 4 | 1 | 9.55 | 29.58 | yes |
| samples/oidc-auth --instance-count 1 | 1 | 0 | 4.9 | 4.68 | yes |
| samples/oidc-auth --instance-count 1 | 1 | 1 | 4.36 | 4.21 | yes |
| samples/oidc-auth --instance-count 1 | 2 | 0 | 4.01 | 6.12 | yes |
| samples/oidc-auth --instance-count 1 | 2 | 1 | 4.02 | 6.22 | yes |
| samples/oidc-auth --instance-count 1 | 4 | 0 | 3.76 | 8.0 | yes |
| samples/oidc-auth --instance-count 1 | 4 | 1 | 3.87 | 8.21 | yes |


# Chapter 11 Extensions

*Source: `docs/paper/extensions.md`.*

## Phase 4 extensions and the external-models item (roadmap T9, T15, T17, T18)

The issue marks T15–T18 optional. This page records what was done for each and what remains. It
describes main at `9e4a123`.

### T15: certificates for `proved` and for `ok`

**Residual risk 2** of [docs/theory/README.md](../theory/README.md#conclusion): an `ok` and a
`proved` are not checked at run time.

#### `proved`: every z3 answer re-checked by a second solver (done, offline)

A `proved` rests on `unsat` answers of z3: initiation and consecution of the inductive set Houdini
keeps (z3.md, Theorem 1, under H1: z3 answers `unsat` only for unsatisfiable queries). The example
`crates/rdra-ish-core/examples/prove_transcript.rs` records every SMT-LIB command z3 receives in one
prover run. [scripts/paper/smt-crosscheck.py](../../scripts/paper/smt-crosscheck.py) replays that
transcript through z3 and through cvc5. For cvc5 it removes z3's own options (`:rlimit`, `:smt.mbqi`)
and sets the logic after each `(reset)`. It then compares the `check-sat` answers in order. The
transcript is the certificate. It is self-contained SMT-LIB, so any SMT solver can check it,
without rdra-ish.

| Root | Queries | unsat (z3) and unsat (cvc5) | unknown, both | sat (z3), unknown (cvc5) | unsat (z3), sat (cvc5) | Rules proved |
|---|--:|--:|--:|--:|--:|--:|
| patent-ops | 1,116 | 1,111 | 4 | 1 | 0 | 13 |
| clinic-ops | 4,083 | 3,994 | 88 | 1 | 0 | 13 |
| food-permit-ops | 3,842 | 3,783 | 58 | 1 | 0 | 14 |
| insurance-claim-ops | 3,372 | 3,313 | 58 | 1 | 0 | 23 |
| oidc-auth | 13.8 MB transcript; the replay did not finish in 590 s | | | | | 16 |

Data: [eval/data/smt-crosscheck.txt](eval/data/smt-crosscheck.txt). z3 5.4.1 and cvc5 1.4.2 from
nixpkgs.

- On four domain models, cvc5 answers `unsat` to every one of the 12,201 queries that z3 answered
  `unsat`. So the 63 `proved` rules of these models no longer rest on z3 alone: H1 is needed for one
  of two independent solvers, not for z3 in particular.
- The cross-check covers H1 only. It does not cover H2, that the SMT text means the encoding of the
  system: both solvers read the same text. It also does not cover B1, the faithfulness of the
  encoding.
- It runs offline, as a script. Making it a run-time option (`--prove --cross-check cvc5`) would put
  the second solver on every `proved`. A smaller certificate would hold only the final round:
  `Decl`, `Init`, every `Step_a`, and the kept set `K`, with one initiation and one consecution query
  per member of `K`. Its size is about one Houdini round instead of the whole search.

#### BFS `ok`: a reachable-set certificate (investigated, not built)

The certificate would be the set `R` of states a run stored, with a checker of three facts: every
initial state is in `R`; every successor of a state of `R` is in `R`; every state of `R` satisfies the
checked rules. Then `Reach ⊆ R`, so the rules hold. For a reduced cone the facts are about `R` of the
reduced system, and the partition and split certificates (already checked on every run) carry it
to the cone.

| Model (2 instances) | States in all BFS runs | Packed size of all runs | Largest run | Bits per state (largest) |
|---|--:|--:|--:|--:|
| patent-ops | 2,017,009 | 17.6 MB | 1,341,707 | 76 |
| clinic-ops | 5,617,011 | 45.5 MB | 2,238,170 | 68 |
| food-permit-ops | 7,009,703 | 79.1 MB | 4,846,569 | 106 |
| insurance-claim-ops | 13,499,587 | 150.6 MB | 4,580,087 | 94 |

(From the `perf-run` lines of a `perf` build: states times `bits_packed`.)

- **Size.** 18 to 151 MB, before compression, for the domain models at the default bound. That is
  small enough to write.
- **Checking cost.** One successor computation per state: about the cost of one BFS pass, without
  the frontier, the parents or the reductions' bookkeeping. It parallelizes trivially.
- **What it would and would not catch.** A checker that recomputes successors with the same step
  relation shares the builder, so it catches faults in the search, the store, the reductions' tables
  and the canonical form, but not in the builder (residual risk 1). A checker with its own step
  function, for example the reference interpreter of `core/tests/reference/` on its subset of the
  language, would also catch builder faults.
- **Liveness.** An `ok` for `eventually` or `leads_to` needs, beyond `R`, a certificate that no fair
  lasso violates the property: a ranking over the goal-free part of the graph, or the component
  decomposition with the fair-instance bits per component, re-checked by a small program. This is
  open.

**Status.** The `proved` cross-check is done for four of five domain models. The `ok` certificate is
designed and sized here, not built.

### T17: the requirements-to-code chain

`check --code` generates again every file named in a project's `[code]` tables and compares it with
what is on disk (`fresh`). It then runs the compiler on a probe (`build`), and runs a conformance
test that replays the model's traces against the service (`run`) ([cli-reference.md, `check
--code`](../cli-reference.md#check---code)). codegen is experimental, so the claim is scoped to
what the tests show.

**Case study.** The repository's tests carry a complete chain on one model
(`crates/rdra-ish-cli/tests/fixtures/codegen/conformance.rdra`, an order with payment, shipment and
cancellation). `codegen --code` generates the Rust domain code and the trace file. A reference
service crate implements the use cases. Each test then plants one kind of drift between the model
and the code, and `check --code` must report it as a named diagnostic. We ran both test binaries
with `RDRA_REQUIRE_CODEGEN_TOOLS=1`, so that a missing toolchain fails instead of skipping: 26 of 26
pass (`cargo test -p rdra-ish-cli --test codegen_code_build --test codegen_code_check`, 57 s including
the build).

| Level | Planted drift | Reported as |
|---|---|---|
| `fresh` | the generated domain file edited by hand | edited file, by its text |
| `fresh` | the trace file edited | edited traces, by their hash |
| `fresh` | the model changed after generation | model changed |
| `fresh` | other generator options in `rdra.toml` | generator changed |
| `fresh` | a generated file deleted | missing file, with what writes it |
| `build` | use case `Ship` not implemented | `CodeMissingUseCase_Ship`, mapped from rustc E0277 to the model line of `usecase Ship` |
| `build` | `Ship`'s implementation empty | `CodeMissingUseCase_Ship` (E0046) |
| `build` | `commit_cancel` missing | `CodeIncomplete_Cancel` (E0046) |
| `build` | `Pay` takes a bare integer for `OrderId` | `CodeSignature_Pay` (E0053) |
| `run` | the service skips the core of `Pay` | `CodeConformance`: the store differs from the model after `Pay` |
| `run` | the service drops a row | `CodeConformance`: a `Shipment` row the model has and the service lacks |
| `run` | the service accepts `Cancel` where the model's guard refuses it | `CodeConformance`: the model refuses `Cancel 101` here, and the service ran it |
| `run` | the service's preparation step writes unconditionally | `CodeConformance`: prepared before `Place` changed a row it writes |
| `run` | the service ignores the trace's ids | the self-check of the conformance run fails |

**Scope of the claim.** Every planted drift of these kinds is detected on this one model. The
conformance level replays traces generated from the model's own state space up to a cap
(`conformance_states`), so it covers behaviours the traces reach, not all behaviours. A capped suite
warns, and `--deny-incomplete` fails it. codegen and `check --code` are experimental. The case study
ran first on Rust. The same planted omissions and faults for TypeScript
(`codegen_code_build_ts`, 7 tests) and Go (`codegen_code_check_go`, 10 tests, including a probe
compiled for every sample) also pass: 17 of 17 with `tsc` and `go` required, 175 s. Paper 1 and Paper 2 make no claim about
the code chain. A paper on it would need models and services from outside the project (T9).

### T18: agent feedback study (protocol only)

Kept separate from the papers, as the issue asks. A protocol for a later short paper:

- **Question.** Do coding agents write correct lifecycle models more often when they see the
  verifier's output (shortest counterexamples, `not decided` notes) than when they see only parse
  and type errors?
- **Tasks.** The five domain models' PRDs (`samples/*/prd.md`) with the model removed. Each task asks
  for the model and its rules.
- **Arms.** (A) agent with `rdra-ish check` only; (B) agent with `check` and `verify`, whose shortest
  counterexamples it may read; (C) B plus the `rdra-ish-verify` skill of `skills/`.
- **Measures.** Rules that hold in the reference model and fail in the agent's (found by running
  the reference model's `verify` on the agent's model, and the converse), the number of iterations,
  and tokens.
- **Threats.** Contamination: the samples are public. Use held-out PRDs written for the study.

Not run.

### T9: models not written by the authors

All nine sample models and the 91 corpus cases were written by the project's authors. The five
domain models cite public sources for their business rules (statutes, official guidance, the
OpenID specifications), but the modelling choices are the authors'. Paper 1 states this as threat
T1.

What would close it:

1. **Outside modellers.** Ask modellers outside the project to write models from the same PRDs
   (`samples/*/prd.md`), and run the evaluation on their models. This needs contributors, who
   cannot be recruited from this repository.
2. **Published case studies.** Translate published lifecycle case studies from the artifact-centric
   literature (for example the BPM benchmarks VERIFAS was evaluated on) into rdra-ish. The
   translation is still done by the authors, but the requirements are someone else's.

Neither was done. T9 remains open, and the papers say so.


# Chapter A Defect record

*Source: `docs/paper/evidence/defects.md`.*

## Defects found in rdra-ish, by how they were found

This record lists the defects of the verifier that the project history reports, and
classifies each by the activity that found it. It is roadmap item T3 of issue #594. It
describes the repository at commit `9e4a123` (2026-11-28).

The table is generated from [`defects.tsv`](evidence/defects.tsv);
`python3 -I scripts/paper/defect-summary.py` prints the summary tables of this document from
it, and `--table` prints the full table.

### Sources

- `CHANGELOG.md`: every "Fixed" section from v0.2.0 to Unreleased, and the "Erratum" of
  v3.0.0.
- `git log` of the whole history (2,124 commits), subjects and bodies, including merge
  messages.
- The table of open problems P1 to P67 in `docs/theory/README.md` as it stood before commit
  `b1b4222d`, which replaced it with the current index (`git show b1b4222d^:docs/theory/README.md`).
  Its "Numbering" paragraph says where each block of items came from.
- `docs/theory/*.md`, `docs/design/*.md` (in particular `event-order.md`, "Landing plan"),
  `docs/engine-agreement.md` and `proofs/lean/README.md`.
- The GitHub issues and pull requests that these sources cite, read through the REST API,
  for the sentence that says how a defect was found.

### Method

**What counts.** A row is a *defect* (`D`) when a source says the tool did one of these:
printed a wrong `ok` or `proved`, missed a finding, printed a counterexample or finding the
model does not have, printed a wrong trace or instance count, crashed, ended a run as
`tool_error` without a fault of the model, skipped a cross-check silently, or gave output
that depended on the thread count. A row is a *doc/claim error* (`C`) when a source says a
document or definition stated a guarantee the code did not give, with no defect in the
code. Each row cites the sentence it rests on.

The scope is the verification pipeline: the builder of the transition system, cones,
the sub-lifecycle reduction, instance symmetry, BFS and the liveness search, the z3 prover,
the TLA+ printer, and the `verify` / `check` driver that runs them and compares them with
TLC. These are not counted:

- defects of `codegen`, `check --code`, the language server, diagrams, `list`, `states`
  display, `fmt`, and the SQL and DBML exports;
- changes of semantics the owner decided as design, even when a source calls the old
  reading surprising (for example #78, the guards of a split use case; #112 and #30, the
  event order; #199 and #212, deletes);
- missing run-time checks and evidence gaps with no defect behind them (P3, P7, P14, P19,
  P26, P35, P42), and defects of the test harnesses (P13, P15, P17, P25, P44);
- performance problems and incompleteness of the prover (for example #101, #231);
- vacuity and `info` findings, which no verdict reads (for example 086b4352, 0552fabb,
  2cb3e297);
- items closed as "not a bug" (P34) or as a duplicate (P51 is counted under P22);
- one-line or vague July 2026 fixes whose effect the commit does not state
  (ba5c088c, 4f259f1d, 6839cab8, 72a6f782, f1d25984, d826eed4, 39203571, 2186c485, 870fb46d).

**Discovery class.** The classes are those of the roadmap:

| Class | Meaning |
|---|---|
| 1 | Lean proof attempt or mechanization (`proofs/lean/`) |
| 2 | Paper proof: found while writing the theory documents (#464 to #468: the semantics and claims inventory, and the theme documents on cones, z3, the native verifier and TLC agreement, which state each guarantee as a proposition with its proof or evidence) — subclass `write-up` — or by a review or audit of those documents against the code — subclass `review/audit` |
| 3 | Reference interpreter (`core/tests/reference/`) |
| 4 | TLC differential: `engine_equivalence`, `verify --with-tlc`, and the engine corpus with its hand-written headers |
| 5 | Mutation run |
| 6 | Run-time self-certification (counterexample replay, partition certificate) |
| 7 | Shadow twin |
| 8 | Kani, Miri, or overflow-checked CI |
| 9 | Random-model `reduction-diff` |
| 10 | Unit or regression test, user report, code review (people or models), use of the tool on a sample model (`dogfooding`), found while fixing another issue, a lint |
| unstated | No source says how the defect was found |

A class is assigned only from a sentence in a source. The P-table's "Numbering" paragraph
places P1 to P21 in the survey for the claims inventory (#464), P22 to P24 in the
TLC-agreement document (#465), P30 and later in the cones document (#467), P40 and later in
the z3 document (#468), and P50 and later in the native-verifier document (#466); the pull
requests that fixed P32, P43 and P54 name the review of #467, #468 and #466 that found them,
and P66, P67 and #541 name the fourth, fifth and third theory audits. P60 comes from the
owner's review. Every other row cites its own source.

**Columns.** `ref` is the commit, issue (`#n` after a hash) or P-number, with the pull
request that fixed it. `version` is the first release whose CHANGELOG section holds the fix;
"(branch)" marks a defect found before its code merged. `date` is the date of the fixing
commit, or the release date when the source is a CHANGELOG entry alone. `shown` says whether
a run of the tool showed the wrong output: `yes`, `no` (a reading of the code only),
`test-only` (only on a forged input), or `unstated`.

**Effect.** `wrong ok` (includes a wrong `proved` and a missed finding), `wrong
counterexample` (includes a spurious finding and an understated instance count), `wrong
verdict` (the source states a wrong reading of the model with no direction, or both
directions), `crash` (a panic), `tool_error`, `missed check` (a comparison skipped with no
report), `other` (a wrong count, a merged report, output that depends on the thread count,
a lost verdict), `unstated`, and `claim` for the C rows.

### Findings

- 74 defects and 15 doc/claim errors are recorded. 30 defects are a wrong verdict that a run
  showed on some model (`wrong ok`, `wrong counterexample` or `wrong verdict` with `shown`
  yes).
- No defect is credited to a Lean proof (class 1), the reference interpreter (3), a
  mutation run (5), a run-time self-check (6) or a shadow twin (7). The history records
  these only as checks that a known or planted defect fails: the reference interpreter
  "Fails on P40/P54 and P43 before their fixes" (fc505d39), and each shadow twin, Kani
  harness and corpus rule is shown against a planted bug (`docs/engine-agreement.md`, "What
  the corpus catches"). Most of them are recent: shadow twins, Kani harnesses and the
  mutation runs date from 2026-11-24, the reference interpreter and the Lean proofs from
  2026-11-28. Kani is credited with one defect (f3df52b3, D45), which no reachable state
  exercises.
- The stub-parameter bug P67 (D69) was found by the fifth theory audit (PR #547: "Fixes
  audit 5's C1"), fixed at 10:25 on 2026-11-28. Its Lean counterpart,
  `ExampleLemma6.stub_bug` in `proofs/lean/RdraIsh/Lemma6Example.lean`, was committed at
  15:12 the same day (377bd06d): the Lean proof restates the defect after the fact and did
  not find it. The same holds for the reduction-diff `--stubs` generator (d2f01460) and the
  TLC refinement check (43cefaee), which were shown to catch it on the pre-fix build.
- Class 2 finds the defects that agreement between engines cannot see. Of its 13
  wrong-verdict defects, P40, P54 (D51, D52), P43, P66 (D59, D68), P4 (D60) and P67 (D69)
  printed a wrong `ok` or `proved` on which BFS and TLC agreed, or which no comparison with
  BFS reached. The common-mode defects in class 4 were found by the engine corpus's
  hand-written headers, not by agreement: #180 (D42), where "every engine reads the model
  the same wrong way".
- The differential tests in classes 4 and 9 find disagreements in the TLA+ printer and the
  driver: 3 of the 7 class 4 defects and 1 of the 2 class 9 defects are in TLA+ emit, and
  the class 4 defects of the event-order landing (D71 to D74) were found before merge.
- Of the 52 defects fixed in a release up to v3.0.0, 23 have no stated discovery, 19 were
  found by review, by use of the tool on a sample model, or while fixing another issue
  (class 10), 7 by TLC differential tests or the engine corpus (class 4), 2 by
  `reduction-diff` (class 9) and 1 by Kani (class 8). All 19 class 2 defects are fixed after
  v3.0.0 (Unreleased), as the theory documents date from 2026-11-28.

### Summary

Printed by `python3 -I scripts/paper/defect-summary.py`.

Rows: 89: 74 defects (D) and 15 doc/claim errors (C).
Defects with a wrong verdict shown on a model: 30.

**Defects by discovery class and effect**

| Discovery class | wrong ok | wrong counterexample | wrong verdict | crash | tool_error | missed check | other | unstated | Total |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 1 Lean proof |  |  |  |  |  |  |  |  | 0 |
| 2 Paper proof (theory write-up or its audit) | 9 | 3 | 1 | 1 | 2 | 2 | 1 |  | 19 |
| 3 Reference interpreter |  |  |  |  |  |  |  |  | 0 |
| 4 TLC differential / engine corpus | 1 | 2 |  | 1 |  |  |  | 3 | 7 |
| 5 Mutation run |  |  |  |  |  |  |  |  | 0 |
| 6 Run-time self-certification |  |  |  |  |  |  |  |  | 0 |
| 7 Shadow twin |  |  |  |  |  |  |  |  | 0 |
| 8 Kani / Miri / overflow checks |  |  |  |  |  |  | 1 |  | 1 |
| 9 Random-model reduction-diff |  |  |  |  | 1 |  | 1 |  | 2 |
| 10 Test, user report, review, other | 7 | 3 | 6 | 1 | 3 |  | 1 |  | 21 |
| Unstated | 9 | 4 | 5 | 1 | 3 |  | 2 |  | 24 |
| **Total** | 26 | 12 | 12 | 4 | 9 | 2 | 6 | 3 | 74 |

**Wrong-verdict defects (wrong ok, wrong counterexample, wrong verdict) by discovery class and whether a run showed them**

| Discovery class | shown: yes | shown: no | shown: unstated | Total |
|---|--:|--:|--:|--:|
| 2 Paper proof (theory write-up or its audit) | 9 | 4 |  | 13 |
| 4 TLC differential / engine corpus | 3 |  |  | 3 |
| 10 Test, user report, review, other | 9 | 1 | 6 | 16 |
| Unstated | 9 |  | 9 | 18 |
| **Total** | 30 | 5 | 15 | 50 |

**Defects by component and effect**

| Component | wrong ok | wrong counterexample | wrong verdict | crash | tool_error | missed check | other | unstated | Total |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| builder | 7 | 5 | 9 |  |  |  | 2 |  | 23 |
| cone | 4 | 2 |  |  |  |  |  |  | 6 |
| reduction | 2 | 1 | 1 | 4 |  |  |  | 1 | 9 |
| symmetry | 1 |  |  |  |  |  |  |  | 1 |
| BFS/liveness | 1 |  | 1 |  |  |  | 1 |  | 3 |
| prover/z3 | 4 |  |  |  | 1 |  |  |  | 5 |
| TLA+ emit | 3 | 2 | 1 |  | 6 |  |  | 2 | 14 |
| other | 4 | 2 |  |  | 2 | 2 | 3 |  | 13 |
| **Total** | 26 | 12 | 12 | 4 | 9 | 2 | 6 | 3 | 74 |

**Defects by component and discovery class**

| Component | 2 | 4 | 8 | 9 | 10 | unstated | Total |
|---|--:|--:|--:|--:|--:|--:|--:|
| builder | 2 | 1 |  |  | 13 | 7 | 23 |
| cone | 2 |  |  |  | 1 | 3 | 6 |
| reduction | 3 | 2 |  |  | 2 | 2 | 9 |
| symmetry | 1 |  |  |  |  |  | 1 |
| BFS/liveness | 2 |  | 1 |  |  |  | 3 |
| prover/z3 | 3 |  |  |  | 1 | 1 | 5 |
| TLA+ emit | 1 | 3 |  | 1 | 2 | 7 | 14 |
| other | 5 | 1 |  | 1 | 2 | 4 | 13 |
| **Total** | 19 | 7 | 1 | 2 | 21 | 24 | 74 |

**Subclasses (all rows)**

| Discovery class | Subclass | Defects | Claims |
|---|---|--:|--:|
| 2 Paper proof (theory write-up or its audit) | review/audit | 7 | 1 |
| 2 Paper proof (theory write-up or its audit) | write-up | 12 | 12 |
| 4 TLC differential / engine corpus | --with-tlc cross-check | 1 |  |
| 4 TLC differential / engine corpus | engine corpus header | 1 |  |
| 4 TLC differential / engine corpus | engine_equivalence | 5 |  |
| 8 Kani / Miri / overflow checks | Kani | 1 |  |
| 9 Random-model reduction-diff | reduction-diff | 2 |  |
| 10 Test, user report, review, other | doc sync | 1 |  |
| 10 Test, user report, review, other | dogfooding | 7 |  |
| 10 Test, user report, review, other | lint | 1 |  |
| 10 Test, user report, review, other | review | 9 | 1 |
| 10 Test, user report, review, other | while fixing | 3 |  |

**Doc/claim errors by discovery class**

| Discovery class | Claims |
|---|--:|
| 2 Paper proof (theory write-up or its audit) | 13 |
| 10 Test, user report, review, other | 1 |
| Unstated | 1 |
| **Total** | 15 |

### Full table

Printed by `python3 -I scripts/paper/defect-summary.py --table`. Quotations from Japanese issues are translated and marked so.

| ID | Ref | Version | Date | Component | Effect | Shown | Class | Description | Evidence |
|---|---|---|---|---|---|---|---|---|---|
| D01 | 9ae35cac | v0.2.0 | 2026-07-19 | TLA+ emit | wrong ok | unstated | unstated | The TLA+ export baked the `now` safety condition into the Assign/TickNow actions, so the check of it was vacuous. | commit 9ae35cac "harden TLA export against vacuous now"; CHANGELOG v0.2.0: "`now` Safety is not baked into `Assign`/`TickNow`" |
| D02 | b5530c12 | v0.2.0 | 2026-07-20 | TLA+ emit | wrong counterexample | yes | unstated | TLC reported terminal lifecycle states as deadlocks. | commit b5530c12: "Export CHECK_DEADLOCK FALSE so cancelled/delivered terminals do not fail verify on order samples." |
| D03 | e70eb323 | v0.2.0 | 2026-07-20 | TLA+ emit | tool_error | unstated | unstated | A multi-instance module used `1..InstanceCount` without extending Integers. | commit e70eb323: "Multi-entity exports use 1..InstanceCount and must import Integers even without Int axes." |
| D04 | 48731055 | v0.2.0 | 2026-07-20 | TLA+ emit | wrong verdict | unstated | unstated | `after.assert` was exported as primed assignments inside the actions instead of as a property. | commit 48731055: "Stop injecting primed assignments into SpecActions; export independent [][action => post] properties" |
| D05 | ad626a68 (#12) | v0.3.0 | 2026-10-27 | TLA+ emit | wrong ok | yes | unstated | The TLA+ export dropped the `sets` effects of an entity with transitions, so `verify` printed TLC: OK for a model with a reachable forbidden state. | issue #12: "`rdra-ish verify` prints `TLC: OK` and exits 0 for a model in which a `forbidden` state is reachable" |
| D06 | 02773ff8 (#12) | v0.3.0 | 2026-10-27 | TLA+ emit | wrong ok | unstated | unstated | TLA+ Init took only the first creation state, so a violation reachable only from a later one was missed. | commit 02773ff8: "a violation reachable only from a later one is no longer missed" |
| D07 | 0213c1f8 | v0.3.0 | 2026-10-28 | cone | wrong counterexample | yes | unstated | Creating an entity outside the target's cone used up its only slot, so on personal-info every card and address use case after the first never fired. | commit 0213c1f8: "on personal-info every card and address use case after the first never fired in `states`" |
| D08 | 66ae90cb | v0.3.0 | 2026-10-28 | cone | wrong ok | unstated | unstated | A predicate that read an owner variable put only the child in its component, so the restriction dropped the owner variable and every invariant that read it. | commit 66ae90cb: "restrict dropped the owner var (it needs both ends) and with it any invariant that read the owner alone" |
| D09 | CHANGELOG v0.3.0 | v0.3.0 | 2026-10-30 | other | wrong ok | unstated | unstated | `check` dropped the verdicts of the constraints that an entity without state columns binds. | CHANGELOG v0.3.0 Fixed: "`check` dropped the verdicts of the constraints that an entity without state columns binds" |
| D10 | CHANGELOG v0.3.0 | v0.3.0 | 2026-10-30 | TLA+ emit | tool_error | yes | unstated | `export --kind tla` failed on two samples: an undefined name in TypeOK, and an init fallback. | CHANGELOG v0.3.0 Fixed: "`export --kind tla` works on `samples/api-contract` ... and on `samples/clinic-ops` (it ended in `TLA_FATAL:init_pending_fallback`)" |
| D11 | 8435d700 | v0.3.0 | 2026-10-28 | builder | wrong verdict | unstated | unstated | A use case that creates a parent took an already-linked child from its parent through an `.optional()` key. | CHANGELOG v0.3.0 Fixed: "no longer takes an already-linked child from its parent when the key is `.optional()`" |
| D12 | 718c98ac | v0.4.0 | 2026-10-31 | cone | wrong ok | yes | unstated | An `after` assertion's cone dropped what the use case writes, so the step was a stutter and the assertion passed unread; the TLA+ cone also named a removed parameter (TLC semantic error). | commit 718c98ac: "`after(IssueCert).assert(CertificateOrder.status == executed)` passed even though IssueCert can run while the order is still requested" |
| D13 | CHANGELOG v0.4.0 (#46) | v0.4.0 | 2026-11-01 | builder | wrong verdict | unstated | unstated | A property atom `Entity.qty == limit` over two Int columns compared `qty` with no value and was false in every state. | CHANGELOG v0.4.0 Fixed: "the atom compared `qty` with no value at all and was false in every state (#46)" |
| D14 | CHANGELOG v0.4.0 | v0.4.0 | 2026-11-01 | builder | wrong ok | unstated | unstated | A one-entity `invariant(...).when(...).none(Related ...)` was dropped and never checked. | CHANGELOG v0.4.0 Fixed: "`invariant(Order).when(status == paid).none(Ticket.status == pending)` is checked ... it was dropped" |
| D15 | bbaee8c5 (#52) | v1.0.0 | 2026-11-01 | builder | wrong ok | yes | 10 (dogfooding) | Several `transitions` of one use case from one state kept only the first arm; `check` passed and called the other targets unreachable. | issue #52 (translated): "only the first transition is explored ... `check` is OK"; "occurred in patent-ops' `DecideFiling`" |
| D16 | 0940db4c (#63) | v1.0.0 | 2026-11-01 | builder | wrong verdict | yes | unstated | A `.has`/`.none` quantifier read the row the use case creates (or updates) instead of the quantified rows; TLC stopped on an undefined primed variable. | commit 0940db4c: "BFS let the use case run whatever the existing rows were ... TLC stopped with `Parcel_state is undefined`" |
| D17 | 6662a888 (#64) | v1.0.0 | 2026-11-01 | other | wrong ok | yes | 4 (engine_equivalence) | TLC with -continue reports one violated check per state, so `verify` never printed a rule that fails only where another rule of the cone fails too. | issue #64 (translated): "BFS and TLC reports disagree ... in the engine_equivalence cone comparison"; commit 6662a888: "TLC was the one missing a rule" |
| D18 | 69a3fb18 (#67) | v1.0.0 | 2026-11-02 | TLA+ emit | tool_error | yes | 10 (review) | A guard on a creating use case read a primed variable before the UNCHANGED that binds it, so every run holding the column ended tool_error. | CHANGELOG v1.0.0: "`verify` decides a model whose condition on a creating use case reads a column ... (#67 review)" |
| D19 | 918466f3 | v1.0.0 | 2026-11-02 | other | wrong counterexample | unstated | unstated | A TLC report that the emitter's TypeOK was violated was shown as a counterexample of the model. | commit 918466f3: "TLC reporting it violated is a bug in the generated TLA+, not a counterexample" |
| D20 | a3b2f553 | v1.0.0 | 2026-11-02 | other | other | unstated | unstated | A whole-model property run stopped after TLC had printed its verdict was reported `not decided`. | commit a3b2f553: "the property was still listed as not decided" |
| D21 | de6b7cc8 (#75) | v1.0.0 | 2026-11-02 | builder | wrong ok | unstated | 10 (dogfooding) | Two mandatory keys from one child to one parent shared one parent parameter, so violations that need the keys on two rows were missed. | issue #75 (translated): "`check` and `verify` miss rule violations there (a soundness hole)"; listed as left over from #73 (food-permit-ops) |
| D22 | CHANGELOG v1.0.0 (#75) | v1.0.0 | 2026-11-02 | builder | wrong verdict | unstated | 10 (dogfooding) | A `holds` comparison or copy source on a parent the use case binds ignored `.role` and read the bound parameter. | CHANGELOG v1.0.0: "Before, it read the bound parameter whatever `.role` named (#75)" |
| D23 | a7074152 (#73) | v1.0.0 | 2026-11-02 | builder | wrong verdict | yes | 10 (dogfooding) | An action relinked a parent parameter it only reads: `RegisterPermit` linked the application it read as renewing the permit it created. | CHANGELOG v1.0.0: "An action never relinks a parent parameter, a row it only reads (#73)"; commit a7074152: "Two binding fixes the second key exposed" (the `renews` key of food-permit-ops) |
| D24 | a7074152 (#73) | v1.0.0 | 2026-11-02 | builder | wrong counterexample | unstated | 10 (dogfooding) | A `holds` guard comparing a column of the parent of an `.optional()` key created the child unlinked, so the guard never held. | commit a7074152: "instead of creating it unlinked so the guard never holds"; "Two binding fixes the second key exposed" |
| D25 | 83151c25 (#76) | v1.0.0 | 2026-11-03 | builder | wrong ok | unstated | 10 (dogfooding) | A use case that writes a parent put it on every key of a child, so violations that need the keys apart were missed. | CHANGELOG v1.0.0: "`verify` missed every violation that needs the two apart"; issue #76 is "the rest of #75" |
| D26 | 07be83c2 (#81) | v1.0.0 | 2026-11-03 | builder | wrong counterexample | yes | 10 (review) | `holds(...).none(...).along(...)` bound every path entity as a witness, so `verify` reported a false deadlock. | issue #81 (translated): "Found in the pre-1.0 self-review (Sonnet)"; "`Deadlock_Member_status_active` ... should be able to withdraw" |
| D27 | 49682092 (#83) | v1.0.0 | 2026-11-03 | builder | wrong counterexample | yes | 10 (review) | An `after` assertion on a use case split into transaction groups was read after every step, giving a false violation. | issue #83 (translated): "Found in the pre-1.0 self-review (Fable)" |
| D28 | 80300305 (#84) | v1.0.0 | 2026-11-03 | builder | wrong ok | unstated | 10 (review) | A `@check` read a comparison over null as passing while every other rule form fails it. | issue #84 (translated): "Found in the pre-1.0 self-review (Fable)"; commit 80300305: "The same `qty >= 1` held as a `@check` and failed as an `invariant(..).then`" |
| D29 | 07f56d0e (#90) | v1.0.0 | 2026-11-03 | builder | wrong verdict | unstated | 10 (while fixing) | An invariant's `.has`/`.none` ignored `.along` and counted every related row in the system. | issue #90 (translated): "Found while working on #81"; CHANGELOG v1.0.0: "counted every loan in the system" |
| D30 | c3ea59df | v1.2.0 | 2026-11-04 | TLA+ emit | tool_error | yes | 9 (reduction-diff) | A reduced cone's `Succ_` table as a chain of `@@` overflowed the JVM stack, so `verify` reported tool_error on a cone 1.1 decided ok. | commit c3ea59df: "(review B of #74, a random model minimized; seed 2849 at 2829 classes and seed 2563 ...)" |
| D31 | 9b60c203 | v1.2.0 | 2026-11-04 | reduction | wrong ok | yes | unstated | The reduction related actions by name, so a sub-lifecycle action named like a target step lost its class moves (`ok` where the cone finds the violation) or panicked. | commit 9b60c203: "verify said ok where the cone finds the violation), or its params were read with the step's param list (an index panic in observe)" |
| D32 | 6bbace14 | v1.2.0 | 2026-11-04 | builder | other | yes | unstated | A generated action name equal to a use case id merged two actions in the findings. | commit 6bbace14: "printed `whether Go_1 can run is not decided` while both ran" |
| D33 | 87677745 (#103) | v1.2.0 | 2026-11-05 | builder | other | yes | 10 (review) | Two generated rule names collided, and `verify` listed one name as both ok and a counterexample. | CHANGELOG v1.2.0: "`verify` listed one name as `ok` and as a counterexample"; issue #103 is a search for such collisions |
| D34 | 2125833c (#103) | v1.2.0 | 2026-11-05 | TLA+ emit | tool_error | yes | 10 (review) | A use case named `Succ_...` or `Rep_...` defined a reduced cone's table name twice and SANY failed the cone. | CHANGELOG v1.2.0: "defined the name twice, and SANY failed the cone with `tool_error`" |
| D35 | 373db625, 8effed83 (#103) | v1.2.0 | 2026-11-05 | other | tool_error | yes | 10 (review) | The spec files `verify` writes overwrote each other (`P_WF` beside `P`; names equal ignoring case), giving tool_error or `not decided` on a violated property. | CHANGELOG v1.2.0: "(`tool_error`, or `not decided` on a violated `P`)" |
| D36 | 207ff92a (#110) | v1.2.0 | 2026-11-05 | builder | wrong counterexample | yes | unstated | The columns a `sets(..).when(Cond)` guard reads were not state variables, so the guard never held and `check` reported never-runs, a missing witness and a deadlock. | commit 207ff92a: "check reported the use case as never running, its target state as unwitnessed, and the source state as a deadlock (#110)" |
| D37 | 5d9be069 | v2.0.0 | 2026-11-06 | other | wrong ok | yes | unstated | A handler variant that only capped cones kept got no finding, so `--deny-undecided` passed on a question no cone answered. | commit 5d9be069: "`--deny-undecided` passed on a question no cone answered" |
| D38 | 22ea13c2 (#121) | v2.2.0 | 2026-11-16 | reduction | crash | yes | 10 (dogfooding) | The BFS of a reduced cone panicked (index out of bounds) on a class move whose T params the extended sub-lifecycle dropped. | PR #121: "It also fixes an engine bug that the new model exposed"; CHANGELOG v2.2.0 Fixed |
| D39 | 17992524 | v3.0.0 | 2026-11-21 | prover/z3 | tool_error | yes | unstated | A key compared with a nullable Int gave an ill-sorted SMT term, and an empty choice printed `(or )`; z3 rejected both. | commit 17992524: "aborted the proof with a z3 error ... printed `(or )`, which z3 rejects" |
| D40 | c56f35ff (#164) | v3.0.0 | 2026-11-22 | cone | wrong ok | yes | 10 (review) | Covering turned a violation into `ok (checked in ...)`: a covering cone could take a step as a stutter or drop a mark's guard. | commit c56f35ff: "A review found two models where `covering` turned a violation into `ok (checked in ...)`" |
| D41 | ad6a90f0 (#177) | v3.0.0 | 2026-11-23 | other | other | yes | 9 (reduction-diff) | A rule broken in several cones was counted once per cone, so the failure line depended on how cones overlap. | commit ad6a90f0: "the hooked reduction-diff run diverged on seeds 7, 8, 46, and 459 on that count alone" |
| D42 | 7aa6977d (#180, #185) | v3.0.0 | 2026-11-23 | builder | wrong counterexample | yes | 4 (engine corpus header) | An Int `@default` outside the range silently started at 0 in every engine, so every engine reported a violation the model does not have. | issue #180: "Every engine reads the model the same wrong way, so `verify --with-tlc` reports no disagreement"; model from the adversarial corpus (commit 8090d239 records it as `// known`) |
| D43 | 32bd88ec (#246, #258) | v3.0.0 | 2026-11-24 | other | wrong ok | yes | 10 (doc sync) | A `can_reach` whose cone stopped at a cap was counted nowhere: the run printed OK and passed `--deny-undecided`. | issue #246: "Found while syncing the docs with main (#245)" |
| D44 | 1d0bf9d5 (#261, #267) | v3.0.0 | 2026-11-24 | TLA+ emit | wrong counterexample | yes | 4 (--with-tlc cross-check) | The state-model `Deadlock_` invariant left out event marks a writing step reads, so TLC (and a hand run of the export) reported a deadlock that does not exist. | issue #261: "A bug the `--with-tlc` state-model cross-check of #201 found" |
| D45 | f3df52b3 (#279) | v3.0.0 | 2026-11-24 | BFS/liveness | other | no | 8 (Kani) | `store_bytes` could overflow its u128 sum near `width == usize::MAX`: a debug build panicked and a release build wrapped to a small count. | commit f3df52b3: "Kani found it (#275)"; "No state is that wide" |
| D46 | 7b9c8cad (#355, #360) | v3.0.0 | 2026-11-25 | builder | wrong verdict | yes | unstated | `==` between two nullable columns read null == null as true in every engine and z3, against the language reference and the SQL export. | issue #355: "the BFS engine, TLC and z3 all read `null == null` as true ... So `verify --prove` can prove a rule that the generated SQL CHECK rejects" |
| D47 | 3aa1ef06 (#359, #367) | v3.0.0 | 2026-11-25 | builder | wrong verdict | yes | 10 (while fixing) | An Enum or Bool column marked `@null` had no null in the lifecycle and started at its first value. | issue #359: "Found while fixing #355" |
| D48 | 70ec425a (#374, #381) | v3.0.0 | 2026-11-26 | builder | wrong verdict | yes | 10 (while fixing) | `@derived(qty in (...))` over a nullable Int was false, not null, while `qty` was null. | issue #374: "Found while fixing #359 (PR #367)" |
| D49 | d7c798ea (#425, #429) | Unreleased | 2026-11-27 | prover/z3 | wrong ok | yes | 10 (review) | The build read a set listing every Int value up to the range's top as the whole domain, so `--prove` proved a rule BFS refutes at a wider range. | issue #425: "Found by the z3 encoding audit (2026-11-27)"; CHANGELOG Unreleased Fixed |
| D50 | P21 (#469) | Unreleased | 2026-11-28 | other | tool_error | yes | 2 (write-up) | A native property run stopped at a cap was recorded as ok in the per-run TLC comparison, giving a false EngineDisagreement. | P-table P21 (README at b1b4222d^): "reproduced before the fix with `temporal_unmet.rdra` at `--max-states 3`"; P1-P21 "come from the survey for the claims inventory" |
| D51 | P40 (#470) | Unreleased | 2026-11-28 | builder | wrong ok | yes | 2 (write-up) | A nullable Int at `--int-range-max 255` wrapped its top value to null; a rule the model breaks printed `ok` and `proved`. | P-table P40: "`sets(Top, Item.n == 255)` with `forbidden(Item, n == 255)` printed `proved:` and `ok`"; PR #470 cites docs/theory/z3.md "Reproductions" (#468) |
| D52 | P54 (#470) | Unreleased | 2026-11-28 | builder | wrong ok | yes | 2 (review/audit) | An Enum of 256 (nullable) or 257 values wrapped its last value onto null or the first value; a broken rule printed `ok` at the default bounds. | P-table P54: "Reproduced at one instance by the review of native.md"; PR #470: "P54 (from the #466 review), at the default bounds" |
| D53 | P12 (#471) | Unreleased | 2026-11-28 | other | missed check | yes | 2 (write-up) | A TLC trace that did not replay through BFS's step relation silently skipped the comparison. | P-table P12: "a non-replaying trace dropped the comparison" (reproduced "with a `tlc` wrapper that corrupts one value") |
| D54 | P1 (#477, #509) | Unreleased | 2026-11-28 | cone | wrong counterexample | yes | 2 (write-up) | A cone that left a raised mark out was treated as exact: its counterexample was reported, a rule marked refuted, and escalation printed a counterexample at k instances. | P-table P1: "Consequence: a spurious `counterexample` and a rule never proved; not a wrong `ok`"; PR #477: "each reproduced on main first" |
| D55 | P11 (#477, #509, #532) | Unreleased | 2026-11-28 | other | wrong counterexample | no | 2 (write-up) | Escalation and refutation printed fewer instances than a counterexample needs (freed entities excluded; a maximum under per-entity counts). | P-table P11: "Escalation could understate the instances a counterexample needs" |
| D56 | P32 (#477) | Unreleased | 2026-11-28 | cone | wrong ok | yes | 2 (review/audit) | An entity cone that left a raised mark out could hide a deadlock, a missing witness or a never-runs finding. | P-table P32: "a wrong \"no finding\", that is, a wrong `ok`"; PR #477: "P32 (#467 review, item 2) ... On the reviewer's model, main reported no `Witness_Order_status_shipped`, no `Deadlock_Order_status_open`" |
| D57 | P50 (#479, #510) | Unreleased | 2026-11-28 | BFS/liveness | wrong ok | yes | 2 (write-up) | Packed-state bit offsets were u16 and saturated at bit 65,535, so wide states were stored wrongly and distinct successors merged. | P-table P50: "Reproduced on a 34-Int-column entity at 255 instances (69,870 bits): a spurious `counterexample` and a wrong `ok`"; PR #466 (native.md) lists P50 |
| D58 | P55 (#479) | Unreleased | 2026-11-28 | BFS/liveness | wrong verdict | no | 2 (review/audit) | The first-edge index was a bare `as u32` cast and could wrap past 2^32 edges, attributing edges to wrong sources. | P-table P55: "Not reproduced"; commit 02e5b48f "native.md review fixes; P54-P56" |
| D59 | P43 (#481) | Unreleased | 2026-11-28 | prover/z3 | wrong ok | yes | 2 (review/audit) | A `.when`-guarded `present` write to a nullable Int kept the run's top value in the encoding, so `forbidden(Item, n > 5)` was proved at range 5 and refuted at 7. | PR #481: "P43 is F1 of the #468 review: a wrong `proved` at ordinary bounds" |
| D60 | P4 (#483, #514, #523) | Unreleased | 2026-11-28 | prover/z3 | wrong ok | yes | 2 (write-up) | A proof under a scope flag proved the scoped system without saying so, and under `--entity` a deferred build error made a rule the model breaks print `proved:`. | P-table P4: "Reproduced: with `--without-usecase Close`, a rule the whole model violates printed `proved:`"; PR #523: "It was reproduced first, and it is a real bug" |
| D61 | P53 (#484) | Unreleased | 2026-11-28 | other | other | yes | 2 (write-up) | A safety cone rerun after a memory stop was not marked, so the printed state count depended on the thread count. | P-table P53: "`RDRA_JOBS=8` printed `(15150 states)` where 1 printed `(65793 states)`" |
| D62 | P24 (#485) | Unreleased | 2026-11-28 | TLA+ emit | tool_error | yes | 2 (write-up) | The TLA+ printer and the native step read a multi-target transition beside another write of the same slot differently. | P-table P24: "built that shape (reproduced: `EngineDisagreement` on main)" |
| D63 | P52 (#487) | Unreleased | 2026-11-28 | reduction | wrong counterexample | no | 2 (write-up) | A reduced safety trace whose lifted last state no longer violated the check was printed as a counterexample. | P-table P52: "kept the reduced message and printed the lifted trace with no fault" |
| D64 | P30 (#488) | Unreleased | 2026-11-28 | symmetry | wrong ok | no | 2 (write-up) | The symmetry scan did not check that no guard reads the owner key of an absent row, which the group does not relabel. | P-table P30: "No model was known to reach it" |
| D65 | P23 (#503) | Unreleased | 2026-11-28 | other | missed check | no | 2 (write-up) | The per-run comparison paired the native WF run on the reduced cone with TLC's `_WF` run on the unreduced cone. | P-table P23: "so Proposition 0 of tlc-agreement.md (both engines decide the same `T'`) failed on that path" |
| D66 | P65 (#513) | Unreleased | 2026-11-28 | reduction | wrong verdict | no | 10 (lint) | The monotonicity test listed a 256-value column's start values as an empty range, so a column set up and down read as monotone. | PR #513: "One cast was wrong" (found by denying clippy::cast_possible_truncation); P-table P65: "no wrong verdict was seen" |
| D67 | #541 | Unreleased | 2026-11-28 | reduction | crash | test-only | 2 (review/audit) | The search for the S path behind a class move ended in a `panic!` when no path existed. | PR #541: "Fixes W1 of the third theory audit"; "On main the test panics at `partition.rs:797`" (forged move) |
| D68 | P66 (#544) | Unreleased | 2026-11-28 | prover/z3 | wrong ok | yes | 2 (review/audit) | A `.when`-guarded `null` beside a `.when`-guarded `present` on one nullable Int was no step of the encoding, so a rule BFS and TLC refute in three steps printed `proved`. | P-table P66: "Found by reading in the fourth audit; reproduced at `d44128aa`" |
| D69 | P67 (#547) | Unreleased | 2026-11-28 | reduction | wrong ok | yes | 2 (review/audit) | A parameter on a stub kept a role no stub row meets in the extended sub-lifecycle, so folded S steps and rises were lost: a wrong `ok` on a reduced cone, and a panic in `recanon`. | PR #547: "Fixes audit 5's C1: default `verify` gave a wrong `ok`"; "`Forbidden_Order_0: ok (35 states)`. `RDRA_ISH_NO_REDUCE=1` finds a 7-step counterexample" |
| D70 | 59418550 (#569) | Unreleased | 2026-11-28 | reduction | crash | yes | unstated | `verify` panicked while building the extended sub-lifecycle: a fold keyed by a dropped row, and a rise guard naming a dropped created row. | commit 59418550: "`verify` panicked while building the extended sub-lifecycle, in two ways"; PR #569: no sample or corpus cone had the pattern |
| D71 | event-order landing (#112) | v2.0.0 (branch) | 2026-11-06 | reduction | unstated | yes | 4 (engine_equivalence) | (Before merge.) A fold keyed by a `New` parameter, whose slot has no identity before the step. | docs/design/event-order.md, landing plan: "`engine_equivalence` ... Its first full run found two defects: a fold keyed by a `New` parameter" |
| D72 | event-order landing (#112) | v2.0.0 (branch) | 2026-11-06 | reduction | crash | yes | 4 (engine_equivalence) | (Before merge.) A fold bit started false where the cone's initial state already raised it, leaving the initial S state without a class. | docs/design/event-order.md: "left the initial S state without a class and made `verify` of clinic-ops panic" |
| D73 | event-order landing (#112) | v2.0.0 (branch) | 2026-11-06 | TLA+ emit | unstated | yes | 4 (engine_equivalence) | (Before merge.) The class conjunct read a primed input before the UNCHANGED that binds it. | docs/design/event-order.md: "Two TLA+ defects came with them: the class conjunct read a primed input before the `UNCHANGED` that binds it" |
| D74 | event-order landing (#112) | v2.0.0 (branch) | 2026-11-06 | TLA+ emit | unstated | yes | 4 (engine_equivalence) | (Before merge.) The decoder took an input's value from the representative instead of the module's own variable. | docs/design/event-order.md: "the decoder took an input's value from the representative instead of the module's own var" |
| C01 | 09ba5d6f | v1.2.0 | 2026-11-05 | TLA+ emit | claim | unstated | unstated | The note `export --kind tla` prints said TLC finds every deadlock; the exported `.cfg` sets CHECK_DEADLOCK FALSE. | commit 09ba5d6f: "the export note no longer says TLC finds every deadlock" |
| C02 | P2 (#514, #525) | Unreleased | 2026-11-28 | prover/z3 | claim | yes | 2 (write-up) | What `proved` quantifies over was stated three ways; "any Int value" was false given P43. | P-table P2: "with the P43 construct \"any Int value\" was false, not merely loose" |
| C03 | P5 (#490, #525) | Unreleased | 2026-11-28 | prover/z3 | claim | unstated | 2 (write-up) | The proof notes overstated the CTI premise, the lemma limit, maximality, and the count range. | P-table P5: "The proof notes overstated details" |
| C04 | P6 (#475) | Unreleased | 2026-11-28 | BFS/liveness | claim | yes | 2 (write-up) | The language reference stated `eventually` as `<>expr`, but a goal is read on a deleted row at its base values. | P-table P6: "reproduced: with `pending` the default, `leads_to(status == paid, status == pending)` is `ok` on both engines" |
| C05 | P8 (#475) | Unreleased | 2026-11-28 | symmetry | claim | unstated | 2 (write-up) | The notes said a Kani harness proves the canonical form invariant under the group; no such harness existed. | P-table P8: "The Kani claims for symmetry were overstated" |
| C06 | P9 (#475) | Unreleased | 2026-11-28 | symmetry | claim | unstated | 2 (write-up) | Design notes kept stale statements on which symmetric readers are sound and on what a delete resets. | P-table P9: "Stale design statements" |
| C07 | P10 (#477) | Unreleased | 2026-11-28 | other | claim | unstated | 2 (write-up) | formal-verification.md said a stopped cone leaves its invariants not decided; the code reports a counterexample found before the stop. | P-table P10: "not a bug; prose fixed in #477" |
| C08 | P16 (#486, #497, #508) | v3.0.0 | 2026-11-28 | other | claim | unstated | 2 (write-up) | Gates the notes described were not in force; release-gate item 2 of v3.0.0 held on a narrower check than written. | P-table P16; CHANGELOG v3.0.0 Erratum (2026-11-28): "So item 2 held on that narrower check, not as written" |
| C09 | P20 (#518, #522, #529, #531, #541) | Unreleased | 2026-11-28 | BFS/liveness | claim | unstated | 2 (write-up) | "Every native counterexample replays" did not hold for safety traces: plain traces printed as stored, reduced lifts checked by debug_assert only. | P-table P20: "\"Every native counterexample replays\" did not hold for safety traces" |
| C10 | P22, P51 (#475, #494) | Unreleased | 2026-11-28 | BFS/liveness | claim | yes | 2 (write-up) | Definition D17 omitted that neither engine checks an `after` assertion on a step that changes nothing. | P-table P22; P51: "a use case whose every step is a self-loop and whose assertion fails after it gave `ok` on both engines" |
| C11 | P31 (#475, #516, #521) | Unreleased | 2026-11-28 | reduction | claim | unstated | 2 (write-up) | The reduction's soundness argument used, without stating it, the step that ties the extended sub-lifecycle to the cone (Lemma 6). | P-table P31: "The reduction's proof omitted the step that ties the extended sub-lifecycle to the cone" |
| C12 | P33 (#512, #516) | Unreleased | 2026-11-28 | symmetry | claim | unstated | 2 (write-up) | Claim A20 said every rule that leaves a class out has a witness; `PinnedStart` and ten reasons had none. | P-table P33: "Group rules were kept without a witness" |
| C13 | P41 (#490) | Unreleased | 2026-11-28 | prover/z3 | claim | yes | 2 (write-up) | The notes said a `.lemma()` rule is never checked by BFS; a false one printed `refuted` and failed the run. | P-table P41: "a false one printed `refuted: lemma Forbidden_Door_0`" |
| C14 | P56 (#475) | Unreleased | 2026-11-28 | BFS/liveness | claim | unstated | 2 (review/audit) | Definition D20 let a stuttering self-loop take a fair instance, unlike the code and WF_vars. | P-table P56; commit 02e5b48f "native.md review fixes; P54-P56" |
| C15 | P60 (#493) | Unreleased | 2026-11-28 | builder | claim | unstated | 10 (review) | semantics.md defined the transition system as what the build constructs, so a builder fault was part of the specification. | P-table P60; numbering: "P25 and P60 and later come from the owner's review" |

### Threats to validity

**Discovery is often not recorded.** 24 of the 74 defects (32%) have no source that says
how they were found, among them every defect of v0.2.0 to v0.4.0 (D01 to D14). Most of these
fixes were committed directly to `main`, before pull requests carried descriptions, and
their commit messages state the defect and the fix only. The counts of the other classes
are lower bounds.

**The record favours the period it was written in.** The theory documents and their audits
(2026-11-28) number, reproduce and cite every item, so class 2 is recorded in full. Earlier
defects appear only when a commit message or a CHANGELOG entry names them, and defects
fixed inside a feature branch before merge are recorded only where a design note lists them
(D71 to D74). A defect found and fixed in one commit with a neutral message is missing.

**Class 2 mixes two activities.** Writing a theory document checks each claim against the
code, and many items were found by that reading rather than by a failed proof step; the
P-table does not say which. The audits and reviews (subclass `review/audit`) read the code
against the documents' propositions. A reader who counts only failed proof steps should
treat class 2 as an upper bound for "paper proof", and the subclass column lets the two be
split. Code reviews outside the theory documents (#67, #81, #83, #84, #425, c56f35ff) are
class 10, and several of them were made by language models ("self-review (Sonnet)",
"(Fable)").

**Classes follow the first finder named.** Several defects were found by one activity and
then reproduced or pinned by another: P40 and P54 are failed by the reference interpreter
and the engine corpus after the fact, P67 by Lean, reduction-diff `--stubs` and the TLC
refinement check. Only the first named finder is counted, so the later checks get no credit
for defects they would have caught.

**Inclusion is a judgement.** The line between a defect and a design change follows the
sources' own words. #355, #359 and #374 are counted because the issues say the engines
contradicted the language reference; #78 and #112 are not, because the issues ask the owner
to choose a semantics. The scope excludes code generation, the language server and the
exports, so the record says nothing about defects there. Effects are as the sources state
them; where a source shows a wrong reading without its direction, the row says `wrong
verdict`, and `shown` is `unstated` where the source neither reproduces nor rules out the
wrong output.

**Severity is not weighted.** A wrong `ok` on a model no user writes and a crash on a sample
count one each. The `shown` column separates the defects reproduced on some model from
those found by reading alone (P30, P55, P65, P11, P23, P52, and Kani's D45).


# Chapter B Earlier measurements

*Source: `docs/paper/evidence/measurements.md`.*

## Existing measurements and whether the paper can cite them (roadmap T5)

<!-- derived-from ../../design/instance-symmetry.md -->
<!-- derived-from ../../design/native-bfs-parallel.md -->
<!-- derived-from ../../theory/cones.md -->
<!-- derived-from ../../theory/tlc-agreement.md -->
<!-- derived-from ../../../proofs/lean/README.md -->

This file collects the quantitative measurements that the repository already records, for
the evaluation of Paper 1 (issue #594). It was compiled from main at `9e4a123`
(2026-11-28). The sources are `docs/design/*.md`, `docs/theory/*.md`,
`docs/formal-verification.md`, `docs/engine-agreement.md`, `docs/cli-reference.md`,
`proofs/lean/README.md`, `CHANGELOG.md`, `.github/workflows/ci.yml` and
`git log --format='%h %ad %s%n%b' --date=short`. No number here was measured again for
this file. The only numbers this file adds are the Lean line and declaration counts in L1,
which come from `wc` and `grep` on the tree.

### How to read the tables

- **Number**: the figure as the source records it.
- **Setup**: the command, flags and bounds when the source gives them. "n=3" means
  `--instance-count 3`.
- **When**: the commit the source names, or the date of the change that recorded it.
- **Machine**: as the source states it. "not stated" means the source does not say.
  Several design notes use `/usr/bin/time -l`. That is the BSD/macOS form of the flag, so
  those runs were very likely on macOS. This file marks that as *inferred*.
- **Verdict**:
  - **citable** means that the setup is precise, a command can reproduce it, and the code
    path is still the current one. For a state, orbit or class count, which BFS fixes
    independently of thread count and machine, it also means "re-run once at the commit
    the paper pins". Later changes to the samples or the builder can move a count
    (for example #477 on insurance-claim-ops' entity cones, or P67 on the extended
    sub-lifecycle).
  - **re-measure** means at least one of these: the setup is missing, the code is older
    than a change that affects the number, the number is a wall time taken under
    uncontrolled load or on macOS only, the run was a single run, or the number comes
    from a prototype or probe that is not the shipped code.
  - **not for the paper** marks a number that is valid but outside Paper 1's claims. Each
    such row names the topic it belongs to.

The status of instance symmetry matters for every row in the first table. `verify
--symmetry` defaults to `never` on main. `always` is experimental, and the change of the
default is still open (`docs/design/instance-symmetry.md`, status of 2026-11-26;
`CHANGELOG.md`, Unreleased).

### 1. Instance symmetry (`G_s`, `G_T`, `H`)

| # | Number | What was measured | Setup | When | Machine | Verdict and reason |
|---|---|---|---|---|---|---|
| S1 | Plain runs stopped at 25.3M (`Inv_Appointment_0`, `_1`), 25.9M (`Unique_Encounter_Appointment`), 27.0M (`Inv_Encounter_2`), 27.9M (`Forbidden_ClinicalOrder_1`) states; insurance-claim-ops `Forbidden_ClaimCase_3` over 45M; patent-ops cones at 33M–41M | Where an unreduced `verify` stops at the memory limit (#212) | n=3, `--memory 8GiB` | `450558e2` (2026-11-24) | not stated | **re-measure.** A stop point is a function of the bytes stored per state and of the room each cone gets. Both changed after `450558e2`: a stopped entity cone now gives back its room (#361), and later store changes followed. These numbers show only that the cones do not fit. They are not sizes. |
| S2 | 240,016 orbits (`Inv_Appointment_0`, `_1`, `Unique_Encounter_Appointment`); 1,696,486 (`Inv_Encounter_2`); 682,640 (insurance-claim-ops `Forbidden_ClaimCase_3`) | Orbit counts of complete runs up to `G_s`. First counted by #212's prototype, then reproduced exactly by the shipped canonicalizer (phase 1 exit) | n=3, `--memory 8GiB`, `--symmetry always` | prototype at `450558e2`; phase 1 at #311 (`40e3783a`, 2026-11-25) | not stated | **citable.** The canonical form is exact (`docs/design/instance-symmetry.md`, "The canonical form"), so the count is a property of the model and the bounds. Re-run at the pinned commit, because insurance-claim-ops changed under #477. There is no exact plain count to set beside these numbers: the plain runs do not finish (S1). |
| S3 | `about 820M states` for `Inv_Encounter_2` | The orbit-size estimate that the cone line prints (sum of `\|G_r\|/\|Aut(r)\|`) | `verify samples/clinic-ops --instance-count 3 --memory 8GiB --symmetry always` | example in `docs/cli-reference.md` at `9e4a123` | not stated | **re-measure.** This is an estimate. It is an upper bound that also counts unreachable images (S18). It is not a plain count, and the example output in the reference may not come from a recorded run. |
| S4 | Sub-lifecycle S of clinic-ops `Forbidden_ClinicalOrder_1`: 61,849,224 states and 20 classes; the reduced cone has 5,050 states | Size of S under a plain run, and of the #74 quotient (#284) | n=3, `--memory 32GiB` | #284 comment, around 2026-11-24 | not stated | **citable** for the counts (the quotient is deterministic). Re-run at the pinned commit. |
| S5 | Over 41.1M orbits, incomplete | insurance-claim-ops `Forbidden_Claim_4`, unreduced cone up to `G_s` | n=3 | #212 | not stated | **re-measure.** This is a lower bound from a run that stopped. |
| S6 | 15–22× (22.5×, 21.3×, 12.4×) | #212 prototype's reduction on patent-ops' reduced cones, with S permuted too | n=3 | #212 | not stated | **re-measure (do not cite).** It uses a prototype group that permutes S. That is not the shipped `G_T`, which fixes S (`Rule::Reduced`). The shipped factors are S7. |
| S7 | 5.5× on `Forbidden_PatentApplication_1` (29,680,875 states / 5,413,500 orbits); 5.2× on `Unique_Patent_PatentApplication` (5,237,935 / 1,009,760); 3.0× on `Forbidden_DraftingOrder_0` (48,315 / 15,940) | Plain states over orbits up to `G_T` on #74-reduced cones (phase 3) | n=3 | phase 3 (#324, #328), 2026-11-25 | not stated | **citable.** Both sides are complete runs, so both counts are exact. This is the cleanest symmetry factor on record. Re-run at the pinned commit. |
| S8 | `Forbidden_Patent_4`: plain stopped at 40.8M; up to `G_T`, 31,926,220 orbits (about 260M states), `ok`, 98 s. `Forbidden_OfficeAction_3`: stopped at 67,108,864 orbits (about 360M states), not decided. `GrantEnds` reduced cone: 7,346,935 orbits (about 87M states) | Phase 3 results at n=3 under 8 GiB | `verify --instance-count 3 --memory 8GiB --symmetry always --entity <E>`, `RDRA_JOBS=4` | 2026-11-25 | not stated | **citable** for the orbit counts and verdicts. **re-measure** the 98 s (a wall time from one run with no load figure). The "about" figures are estimates (S18). `Forbidden_OfficeAction_3` is a documented negative result: `G_T` moves only `OfficeAction`, so the factor is at most 3! = 6. |
| S9 | `GrantEnds`: at n=2, 75,017 plain states against 30,313 orbits; at n=3, 7,346,935 orbits with 52.7M edges and no orbit self-edge, and the property decided `ok` (plain stops at 40.0M) | First liveness run on the annotated quotient (phase 4, #352) | n=2 and n=3, 8 GiB | 2026-11-25/26 | not stated | **citable.** This is the only recorded liveness property decided up to symmetry at n=3. It has no orbit self-edges, so it does not exercise the self-edge case of A23. Re-run at the pinned commit. |
| S10 | Before `Shared::new`: insurance-claim-ops `ClaimEventuallyDecided` plus `Cone_Forbidden_Claim_4` cost +16.2% under `always` (319.20G to 370.77G instructions) | Cost of running one system twice up to symmetry | `RDRA_JOBS=1`, instructions retired | phase 4 first unit | inferred macOS (`/usr/bin/time -l`) | **not for the paper.** The defect it shows was fixed. |
| S11 | On 43 sample pairs at their `// expect verify` flags: `never` +0.36% against main (627.70G to 629.98G instructions; at most +0.61%), `always` +0.45%; `always` against `never` +0.49% (patent-ops +0.88%, insurance-claim-ops +0.86%, clinic-ops −0.13%, oidc-auth −0.04%) | Overhead of the phase 4 code on models where symmetry does little | `RDRA_JOBS=1`, instructions retired | phase 4 first unit, 2026-11-25/26 | inferred macOS | **re-measure.** Instruction counts do not depend on load, so the method is the right one. The code has changed since: the partition certificate, run-time hypothesis checks, `AbsentOwner` (P30), and the stub fix (P67). The paper's "overhead when nothing permutes" figure should come from a run at the pinned commit with this method. |
| S12 | 628.64G (`never`) against 696.82G (`always`), +10.8%; patent-ops +53.7%, insurance-claim-ops +15.1%; largest peak RSS 1,195 MiB against 1,197 MiB; the same verdicts on all 43 pairs | `always` before step 1 (reuse of plain results) | `RDRA_JOBS=1`, instructions | `7e464c43` (2026-11-25) | inferred macOS; load past 18 for wall times, which the source therefore leaves out | **re-measure (superseded).** Step 1 removed most of the cost (S11). Only the statement "the same verdicts on all 43 pairs" carries over, and the shadow and rule 8 runs in CI check that continuously. |
| S13 | patent-ops n=3: `PatentApplication` `never` 341 s wall, 333 s user, 15.0 GB, against `always` 67 s, 96 s, 12.5 GB, 1,649G instructions. `Patent` `never` not decided at 111,509,504 states, 832 s, 19.1 GB, against `always` `ok` at 31,926,220 orbits, 98 s, 13.0 GB, 3,559G. `OfficeAction` `always` 262 s, 13.4 GB, 9,067G, not decided | Time and peak RSS, `never` against `always` | `--instance-count 3 --memory 8GiB --entity <E>`, `RDRA_JOBS=4`; `never` under `timeout`, so it has no instruction count | `7e464c43` (2026-11-25) | inferred macOS | **citable** for verdicts and counts. **re-measure** times and RSS: single runs, no load figure, and on macOS. The RSS figures exceed `--memory` because of #237 (A26: the limit bounds stores, not the process). |
| S14 | 1,159 ns per stored state for the orbit run against 742 ns for a plain run (1.6×) | Per-state cost of canonicalization, patent-ops safety pass | n=2, `RDRA_PERF=1 RDRA_JOBS=1` | `7e464c43` | inferred macOS | **re-measure.** A nanosecond timing from one run, and the step 1 fast paths 2(a) and 2(b) landed after it. |
| S15 | 75.4K instructions per successor for canonicalization against 2.5K for a plain successor | Canonicalizer cost on clinic-ops `Inv_Encounter_2` | `RDRA_JOBS=1`, instructions | #311 (2026-11-25) | not stated | **re-measure.** It predates the step 1 fast paths. The phase 1 exit target ("a fifth of the prototype") has no recorded result beside this number. |
| S16 | Prototype: 4–40 µs per state on one thread; 70–190 thousand successors per second on 8 threads. The plain run of S made 268M edges in 35 s | #212 §5 and #284 | clinic-ops cones, n=3 | 2026-11-24 | not stated | **re-measure (do not cite).** These are prototype numbers. |
| S17 | Runs of S that stop at the memory limit at n=3 took 35 s and 61 s | Cost of the plain attempt before running S up to `H` | n=3 (#284) | 2026-11-24 | not stated | **re-measure.** Wall times with no setup. |
| S18 | Estimate 253 for 243 states (food-permit-ops `Forbidden_BusinessPermit_4`); 7.3K for 6,125 (`Inv_PermitApplication_2`); 4 of the 41 reachable states of `presence_first_absent_asymmetry` have a swap that no run reaches | Error of the `about` estimate, and the `FirstAbsent` asymmetry | samples at their flags; corpus case at n=2 | 2026-11-25 | n/a (deterministic) | **citable.** "4 of 41" is a fact about the model and is the paper's motivating example for `G_s`. The estimate errors are exact counts. |
| S19 | insurance-claim-ops `MultiForbidden_Payment_3`: 779,267 orbits for 783,427 states | A cone where the group moves almost nothing | sample flags | 2026-11-25 | n/a | **citable** as a count. It is a useful "symmetry buys nothing here" data point. Re-run at the pinned commit. |
| S20 | clinic-ops entity cones, n=3: `always` leaves 3 of the 17 findings `never` leaves open. `Appointment` plain stopped at 100,133,268 states, while up to `G_s` it finishes in 14,346,108 orbits (about 3.9G states). `Encounter` 1,696,486, `PatientAccount` 3,885,148, `PaymentTransaction` 1,022,613 orbits; `EligibilityCheck` stops again at 25,681,245 orbits. Peak RSS 37.1 GB against 35.6 GB | Phase 4 second unit (#370), entity cones | n=3, `--memory 8GiB`, `RDRA_JOBS=8` | 2026-11-26 | not stated | **citable** for the counts and the open-finding counts. **re-measure** RSS (single run, process above the limit, #237). |
| S21 | 117 open findings fell to 17 when a stopped entity cone gives back its room; the run took 5,578 s where it took 1,707 s, and peaked at 35.6 GB where it peaked at 29.2 GB | Effect of #361, not of symmetry | clinic-ops, n=3, 8 GiB | 2026-11-25 | not stated | **not for the paper**, except as context for S20. These are wall times from single runs. |
| S22 | "16 to 108 times fewer stored states" | Expected gain of phase 1 | ratio of S1 stop points to S2 orbit counts | design estimate | n/a | **re-measure (do not cite).** The plain side is a stop point, not a size, so the ratio is only a lower bound on the factor, and the source labels it as expected. |
| S23 | 15 reduced cones of the corpus and samples run up to `G_T`; 11 rule reasons have no witness (7 `Reduced`, 4 `Observed`); 9 scan rules fire in no cone of the samples or the corpus | Coverage of the per-rule gate | `explore/symmetry/snapshot.rs` | 2026-11-25 to 11-28 | n/a | **citable** as evidence counts at `9e4a123`. These numbers belong in a threats-to-validity paragraph: the scan rules are never exercised by a real model. |
| S24 | 507 reachable states on which every gated read equals its value on the canonical form | Test of (G6) outside guards (`explore/symmetry/g6.rs`) | 2 instances | `4b46e5c1` (2026-11-28) | n/a | **citable** as a test size. It is one built model. |
| S25 | `JOINT_CAP` 5,040, `BRANCH_CAP` 4,096, twin bound `\|G_s\| ≤ 6^4 = 1,296`, about 135 KB canonicalizer scratch (computed) | Constants of the canonicalizer | code | `9e4a123` | n/a | **citable** as configuration, not as measurements. |

### 2. The sub-lifecycle reduction (#74), its certificates and checks

| # | Number | What was measured | Setup | When | Machine | Verdict and reason |
|---|---|---|---|---|---|---|
| R1 | clinic-ops `verify --deny-undecided`: about 40 s and 1.4 GB, against 2.5 min and 3.7 GB before. `Charge` cone: over 8,388,608 states to 38,533. `Prescription`: 4,266,635 to 1,690. patent-ops `OfficeAction`: 4,818,909 to 1,341,705 | First release of the reduction | n=2 (sample flags of the time) | v1.2.0 (2026-11-05) | not stated | **re-measure.** The samples, the event semantics (#112), the reduction and the store have all changed since. These are the historical headline numbers. The paper needs the same table at the pinned commit. |
| R2 | Of 89 sample targets whose cone `verify` reduced, 49 had a sub-lifecycle that quotients to itself | How often the reduction is a no-op (#101) | `lifecycle_measure` at `verify`'s bounds, 200,000 states | v1.2.0 | n/a | **re-measure.** This rate matters for the evaluation's "how often does it apply" question, and it is from old code. |
| R3 | patent-ops `verify --deny-undecided` about 30 s, 0.75 GB; `OfficeAction` 1,341,707 reduced (over 8,388,608 unreduced); `PatentApplication` 293,833 and `Patent` 202,569 reduced. insurance-claim-ops about 60 s; claim cones 754,247 reduced | Sample sizes after the reduction | sample flags | v1.2.0 | not stated | **re-measure.** Same reasons as R1. |
| R4 | clinic-ops `EligibilityCheck` 905,528 (nothing reduces); patent-ops `OfficeAction` 1,341,707 reduced (185 classes on the application's side); food-permit-ops `BusinessPermit` 4,648,771 reduced (444 classes); insurance-claim-ops 633,787 to 903,987 reduced; oidc-auth over 2,000,000 (nothing reduces) | Cones too large for TLC in `engine_equivalence`, and why | `docs/formal-verification.md`, the cones TLC skips | current text at `9e4a123` | n/a | **citable** for the reduced counts and class counts after a re-run. The unreduced sizes are not recorded ("over"). These cones are the evaluation's natural "reduction is necessary" and "reduction does not apply" cases. |
| R5 | food-permit-ops `BusinessPermit` view grew from 10 vars and 64 states to 33 vars and 5,874 states; a reduction of 1,314 states to 15 with a 61-state, 11-class sub-lifecycle; `Claim` view 836,427 reduced with S of 63,823 states and 443 classes; `Appointment` view halved (188,699 to 97,139) | Effect of the event-order design (#112) on cone sizes | mostly 1 instance, Int 0..5, cap 1,000,000, `lifecycle_measure` | design rounds, before v2.0.0 | 18 cores, 128 GB | **re-measure.** These are design-time numbers for a semantics that has since been revised. |
| R6 | 53 reductions (10 of them nested) checked step by step against their unreduced cone (Lemma 6 and the A8 relation); 25 more reduced sample cones pass 200,000 states unreduced and are not checked | Coverage of `reduce/lemma6.rs::check` (`core/tests/reduce_steps.rs`) | corpus at its flags, sample roots at their `// expect verify` flags | `900456b0`, `e4000b12` (2026-11-28) | CI (`formal-verify`, release) | **citable** as a coverage count. It is also a limitation to report: the largest reductions are exactly the 25 that are not checked. |
| R7 | Random models: 522 PR seeds compared, 284 of them reduce a cone; 45 main seeds with TLC, 33 reduce. The Lemma 6 sweep over the 522 PR seeds: 361 reduce a cone, about 2 min locally. TLC seeds take 7–9 min on the runner | `scripts/reduction-diff.py` sweeps | `run` and `lemma6`, seeds in `scripts/reduction-diff-pr-seeds.txt` and in `ci.yml` | 2026-11-25 (comment in `ci.yml`) | GitHub Actions (ubuntu) | **citable** for the counts. The generator and the seeds are fixed, and CI reruns them on every PR (floors 500/200). `tlc-agreement.md` notes that the counts themselves are a single recorded run. The 284 and 361 differ because they come from different runs and checks; the paper should re-count both at the pinned commit. |
| R8 | The partition certificate takes 25–35% of the partition's time, at most 0.45 s CPU (food-permit-ops) in a 3 s `verify` | Cost of the run-time stability certificate (P7) | samples, release build | `447f20a7` (2026-11-28) | not stated | **re-measure.** This is the only overhead figure for a run-time certificate, and it has no command, no per-sample table and no machine. The paper's "cost of certification" claim needs a controlled measurement: instructions at `RDRA_JOBS=1` with the certificate on and off. |
| R9 | `TLA_CELL_BUDGET` 530,000 cells. 2,428 classes print 72,840 cells and run in a 1 GB TLC heap; the `ClaimCase` view's 4,368 classes print 104,832 cells at a 912 MB peak; a random model's 24,000 classes print 326,400 cells and run out of heap. food-permit-ops `AfterAssert_SuspendBusiness_0` at n=3: 28,928 classes, 520,704 cells, a 7.6 MB module, 7,167,444 states, which TLC checks in 98 s at `-Xmx4g` and runs out of heap on at `-Xmx1g` and `-Xmx2g` | Size of the reduced cone's TLA+ tables | TLC heap as stated | #284, #309 (2026-11-24/25) | not stated | **citable** for the cell and class counts. **re-measure** TLC times (TLC version and machine not stated). This is relevant if the paper reports that TLC checks the same reduced cone. |
| R10 | Escalation memory factor `GROWTH = 10`: one instance more multiplied the store by 1.1–6.4, two more by 1.4–10.1 | Store growth with the bound, on the corpus's escalated cases | doc comment of `cli/src/verify/escalate.rs::GROWTH` | before 2026-11-28 | n/a | **re-measure.** It has no command, and it concerns the prover's bound escalation (A25), not the reductions. |

### 3. Cones of influence and property cones

| # | Number | What was measured | Setup | When | Machine | Verdict and reason |
|---|---|---|---|---|---|---|
| C1 | patent-ops `GrantEnds` on a 4,105-state cone, while its whole model explored 14 million states in 11 minutes without finishing; food-permit-ops `HygieneManagerEventuallyAppointed` on a 257-state cone, while the whole model found 6.9 million states in 300 s without finishing | Property cones against the whole model (TLC whole-model runs) | sample flags of the time | v1.0/v1.1 (2026-11-03/04) | not stated | **re-measure.** The models and the cone construction changed. This is the motivating "cone versus whole model" figure; the paper should take it again with the native whole-model run at the pinned commit. |
| C2 | Property cones: `GrantEnds` reduced 75,017 states and 321,548 edges, against 553,643 states and 3,447,460 edges unreduced; insurance-claim-ops `ClaimEventuallyDecided` reduced 633,787 states and 3,192,700 edges; `SubrogationEnds` reduced 372 states; the unreduced insurance cones exceed 3,000,000 states and 13,800,000 edges. Self-loops are up to 29% of edges (1,007,716 of 3,447,460) | Graph sizes of property cones, reduced and unreduced | throwaway probe like `lifecycle_measure`, BFS, adjacency and Tarjan, cap 3,000,000 | design of native temporal verification (before v3.0.0) | 18-core Apple silicon, 128 GB | **re-measure.** The probe is not the shipped checker. The reduced-against-unreduced ratio (7.4× in states on `GrantEnds`) is worth re-taking with `verify` at the pinned commit. |
| C3 | `Caps::VERIFY` raised from 2,097,152 to 8,388,608; clinic-ops `Prescription` 4,266,635 states, `Charge` 6,629,659, at 2 instances; `verify samples/clinic-ops` about 2.5 min and 3.5 GB | Largest cones before the reduction | n=2 | v1.0.0 era | not stated | **re-measure (historical).** |
| C4 | patent-ops `OfficeAction` cone 4,818,909 states at 2 instances; `verify` about 75 s, up from 4 s | Effect of restoring two guards | n=2 | v0.4.0 era | not stated | **not for the paper** (historical). |
| C5 | A run that stops at the memory limit was measured at up to 1.85 times the limit | Process memory against `--memory` (#237) | not stated | v2.3.0 (2026-11-18) | not stated | **re-measure.** If the paper describes the memory limit, it must say that the limit bounds stores, not the process (A26). |

### 4. Parallel BFS (#296)

All rows in this section come from `docs/design/native-bfs-parallel.md` or from the
CHANGELOG entries that quote it. Their stated machine is an "M5 Max with 18 cores and
128 GB", running macOS (it uses `sample` and `taskpolicy`). The 1-minute load was between
1.8 and 9. Each row is a median of three or of five runs. Parallel BFS is engineering, not
a claim of Paper 1. The paper needs at most one sentence on it: the output is the one-thread
output at every job count.

| # | Number | What was measured | Setup | When | Verdict and reason |
|---|---|---|---|---|---|
| P1 | food-permit-ops / clinic-ops / insurance-claim-ops at 1 job: 89.8G / 95.8G / 313.3G instructions; 5.02M / 5.63M / 9.87M states; 40.9M / 36.9M / 50.3M edges; 27 / 50 / 21 BFS runs | Totals | `verify samples/<name>` at each `// expect` default, release | `c51f2543` (2026-11-25) | **re-measure.** The counts are machine-independent, but they predate 3.1 and later changes. |
| P2 | At 8 jobs: food-permit-ops 1.94×, clinic-ops 2.17×, insurance-claim-ops 3.14× (wall time, median of three; 1-job time 4.15 s / 4.17 s / 12.83 s) | Speedup before 3.1 | `RDRA_JOBS` 1–18 | `c51f2543` | **not for the paper** (superseded by P5). |
| P3 | `Bfs::take` is 36.0% / 32.0% / 23.8% of a 1-job run; at 8 jobs the reader spends 89% of its samples in `take`, and the workers wait 81–83% | Profile | `sample` at 1 ms, `#[inline(never)]` build | `c51f2543` | **not for the paper** (macOS profiler, pre-3.1). |
| P4 | Hits are 80–88% of edges; 30–57% hit a state admitted inside the worker's window; 13–43% hit a state admitted by the same chunk | Look-ahead hit rates | `perf` build with a patch to `perf.rs` that is not for merge | `c51f2543` | **re-measure.** The patch is not in the tree, so the number cannot be reproduced. |
| P5 | Part 3 at 8 jobs: food-permit-ops 2.10 s to 1.81 s (33% of main's 1-job time; 30% at 6 jobs), clinic-ops 1.74 s to 1.58 s (28%), insurance-claim-ops 4.23 s to 3.52 s (24%); instructions +0.3% to +0.8%. The owner accepted 3.1 at 31% / 28% / 25–28% | Speedup of 3.1 | interleaved, median of three or five | 2026-11-26 | **re-measure** if cited: these are macOS wall times under load 3–9. |
| P6 | Probes on: 12% faster on food-permit-ops at 8 jobs, 4–5% slower on the other two; the shared table took 87 ms against hashbrown's 59 ms in a microbenchmark | Part 2, which release builds leave off | as stated | 2026-11-26 | **not for the paper.** |
| P7 | Follow-up (a): 1% fewer cycles at 1 job, equal at 8; follow-up (b): not taken (table of builds `a1`, `b0`–`b3`) | Table rebuild, prefetch | interleaved | 2026-11-26 | **not for the paper.** |
| P8 | Default QoS 4.15 s; `taskpolicy -c utility` 3.99 s; `taskpolicy -b` 16.0 s (3.9× slower, 1.4 GHz) | Apple efficiency cores | 1 job | `c51f2543` | **not for the paper** (macOS only). |
| P9 | clinic-ops `verify` from 73 s to 28 s on 4 cores | First parallel cones | v2.2.0 era | 2026-11-16 | **not for the paper** (historical). |
| P10 | stdout and stderr byte-identical at 1, 2, 4, 6, 8 and 18 jobs; memory stop points the same at 1, 3 and 8 jobs (7, 8 and 10 MiB) | Determinism | `memory_cli` tests (main only) and the design's runs | 2026-11-25 to 28 | **citable.** It is a tested property, not a timing, and `cli/tests/memory_cli.rs` reruns it. |

### 5. The z3 prover (`--prove`)

These rows are outside Paper 1's thesis. They are listed because the task asked for every
measurement and because a paper on the prover (a later paper) would need them.

| # | Number | What was measured | Setup | When | Machine | Verdict and reason |
|---|---|---|---|---|---|---|
| Z1 | 150 s to 68 s of proof over every sample on one solver (clinic-ops 66 s to 16 s, food-permit-ops 33 s to 12 s, oidc-auth 29 s to 22 s), the same `Proof` | MBQI off in the Houdini rounds (#421) | one solver | Unreleased (2026-11-2x) | not stated | **not for the paper**; re-measure for the prover paper. |
| Z2 | SMT-LIB text: clinic-ops 113 MB to 14 MB, insurance-claim-ops 108 MB to 12 MB, food-permit-ops 75 MB to 9 MB, patent-ops 18 MB to 3 MB; wall time 27–36% lower | Activation literals (#421) | `RDRA_JOBS=4`, medians of 3 | Unreleased | not stated | **not for the paper.** The byte counts are deterministic; the times are not. |
| Z3 | A rerun with the certificate cache takes 25–54% less wall time | Proof cache | `RDRA_JOBS=4`, medians of 3 | Unreleased | not stated | **not for the paper.** |
| Z4 | Within 2% after the two-column parent lemma (clinic-ops 6.56 s to 6.70 s, etc.); unpruned, the view cost food-permit-ops 16%; insurance-claim-ops proves all 22 rules without its `.lemma()` | #428 | `RDRA_JOBS=4`, cache off, medians of 3 | Unreleased | not stated | **not for the paper.** |
| Z5 | A check that keeps its candidate spends at most 161,000 rlimit units for a rule (1/124 of its limit) and 116,000 for a lemma (1/17); on 4 solvers a lemma check spent 144,000 (1/14) | Resource-limit headroom | z3 5.1.0, every sample on 2 solvers, some on 1 and 3 | `docs/theory/z3.md` at `91524792` | n/a (rlimit is load-independent) | **citable** for a prover paper. `proof_equivalence` enforces the tenth-of-limit bound on every run. |
| Z6 | Escalation on insurance-claim-ops with its lemma dropped stops at 3 instances after about 40 s at 340 MB; a refuted rule adds about 9 s instead of 24 s | Bound escalation, early refutation | `RDRA_JOBS=4` | Unreleased | not stated | **not for the paper.** |
| Z7 | z3 uses about 0.3–0.5 GiB per process | Memory outside `--memory` | not stated | v3.0.0 era | not stated | **re-measure** if cited. |
| Z8 | 20 Kani harnesses; the `one_of` harness takes about 90 s and 10 GB under CBMC | Bounded proofs of Rust functions | `cargo kani` | `4b58c0e1` (2026-11-28) | macOS arm64 | **citable** for the count. **re-measure** the time (macOS). |

### 6. Agreement with TLC, mutation runs and test counts

| # | Number | What was measured | Setup | When | Machine | Verdict and reason |
|---|---|---|---|---|---|---|
| T1 | Kill rate 66.5% (139 of 209 viable; 251 mutants, 137 caught, 2 timeouts, 70 missed, 42 unviable). Missed by file: `cone.rs` 27, `explore/liveness.rs` 26, `verify/native.rs` 12, `property_cone.rs` 5 | `cargo mutants`, liveness configuration, killed by `engine_corpus` with TLC and z3 off `PATH` | `mutants.yml`, `.cargo/mutants.toml`; Actions run 45212589802 | `b0f593ba` (run dated 2026-11-23 in the doc; commit dated 2026-11-24) | GitHub Actions | **citable** as a recorded run. It is a lower bound on the mutation score (equivalent mutants count as missed). It does not cover `reduce/` or `explore/symmetry/`, so it is no evidence for either reduction. |
| T2 | Kill rate 62.9% (151 of 240; 298 mutants, 147 caught, 4 timeouts, 89 missed, 58 unviable). Missed: 32, 38, 17, 2 | same configuration | Actions run 46034352562 | `6f2d29a1` (run dated 2026-11-27; commit dated 2026-11-28) | GitHub Actions | **citable** with the same limits as T1. The rate fell between the two runs while the mutant count grew; the paper should not present either run as a trend. |
| T3 | 571 TLC-side mutants; no run completed, so no kill rate | `.cargo/mutants-tlc.toml` | `cargo mutants --list` | `91524792` | n/a | **citable** as "not measured". |
| T4 | 40 sample roots, 89 corpus cases, 23 temporal cases, the 36 `bfs_and_tlc_*` tests of `engine_equivalence.rs`, and 45 random models with TLC | Inputs of the TLC agreement evidence | `docs/theory/tlc-agreement.md`, threats to validity | `91524792` | n/a | **citable** as counts at that commit. Re-count at the pinned commit. |
| T5 | 30 `VERIFY_REDUCED_ONLY` cones in the samples; 25 fallback cones at 2 instances in `engine_equivalence`; a cone over 300,000 states at 2 instances is not handed to TLC, and one over 2,000,000 states would be a TLC run of over an hour | What TLC does not see | `engine_equivalence` | `91524792` | not stated | **citable** for the counts. They bound the TLC evidence for the reductions: the refinement check runs on the corpus only. |
| T6 | 17 TLC runs of at most 1,680 table states, four at a time: about 14 s | Cost of the TLC refinement check of `R` against `C` | release jobs | `91524792` | "locally" | **re-measure** the time. The run count is citable. |
| T7 | Release gate of v3.0.0, native against TLC (wall time / peak RSS): patent-ops 1.0 s / 187 MB against 33.3 s / 795 MB; food-permit-ops 2.7 s / 681 MB against 24.0 s / 1.61 GB; insurance-claim-ops 5.9 s / 867 MB against 127.0 s / 2.10 GB; the five small samples under 0.01 s / 10 MB against 3.7–7.1 s / 122–152 MB | Native checker against `verify --backend tlc` | release build, default bounds; TLC 1.7.4 | v3.0.0 (2026-11-26); the TLC column is older | 18-core Apple silicon, 128 GB, one run each | **re-measure.** These are single runs on macOS. The TLC column used TLC 1.7.4 and the `--backend tlc` path, while the flake now pins TLC 1.8.0 on JDK 25. The two columns are not from the same commit, and the TLC runs include JVM start-up. |
| T8 | TLC alone on `Cone_ClaimEventuallyDecided` (633,787 distinct states, 3,131,413 generated): 75.5 s / 1.80 GB with `-workers 1`, 22.7 s / 2.33 GB with `-workers auto`; the native probe took 1.06 s | One property cone, TLC against BFS | 4 GB heap | before v3.0.0 | 18 cores, 128 GB | **re-measure.** The native side was a probe that decided nothing. That TLC finds the same 633,787 distinct states is a citable agreement datum. |
| T9 | Two TLC runs at once took 60 min and eight at once took 30; heaps of 8g and 16g gave the same time, 5g was 15% slower; 25–30 of 64 cores were busy with 2 × 32 workers; an example line shows `Cone_Big`: 12,787,852 states, 1412.0 s, `-Xmx16g`, 8 workers | TLC scheduling (#384, #309) | 64 cores, `--memory 96GiB` | 2026-11-26/27 | 64 cores | **not for the paper.** |
| T10 | The `--with-tlc` sample step was 17% slower in one run (6 min 7 s against 5 min 13 s); patent-ops 7.6 s against 7.3 s | TLC scheduling change | one run | 2026-11-27 | 18 cores | **not for the paper.** |

### 7. Lean

| # | Number | What was measured | Setup | When | Machine | Verdict and reason |
|---|---|---|---|---|---|---|
| L1 | 41 `.lean` files (37 under `RdraIsh/` and 4 at the top), 14,460 lines; 628 lines that begin a `theorem` declaration (lemmas declared another way are not counted); no `sorry`, `admit` or `axiom` in the sources (the only match of `sorry` is the word `sorryAx` in a comment of `CheckAxioms.lean`); Lean 4.36.0, no Mathlib | Size of the mechanization | `find proofs/lean -name '*.lean' \| xargs wc -l`; `grep -E '^\s*(private \|protected )?theorem '`; `scripts/check-lean-proofs.sh` checks the axioms | `9e4a123` (2026-11-28), counted for this file | n/a | **citable** for files, lines and the axiom check. The declaration count is a rough `grep`: re-count with Lean's environment before citing a theorem count. |
| L2 | A clean `lake build` takes about 2 s | Build time | `scripts/check-lean-proofs.sh` | README at `9e4a123` | not stated | **re-measure** (a timing with no machine). |
| L3 | `lemma5-check`: about 3 s for the Lean part, 0.03 s of CPU for the check | Lean-side certificate checker for Lemma 5 | README | `9e4a123` | not stated | **re-measure** if cited. |
| L4 | `partition-check` walks 182 flat runs (60 distinct, 344,409 states, the largest 85,154). It leaves out 78 nested runs, 24 identity partitions, 36 runs stopped at the cap and 6 unobserved sub-lifecycles, and no run is up to `H`. The dump is about 27 MB; the check takes 1.0 s CPU; the dump takes about 7 s in a `ci-checked` build (about 170 s in debug) and peaks at about 2.9 GB. 8 planted faults, one per certificate part, are each rejected | Lean-verified checker of the partition certificate on the samples' runs | `scripts/` walk with `RDRA_LEAN_RUNS_CAP` | README at `9e4a123` | not stated | **citable** for the coverage counts. **re-measure** the times. The left-out runs (nested, capped, up to `H`) are a limitation the paper must state: the Lean-checked certificate does not cover them. |
| L5 | Mechanized in Lean: A8 (flat case), A15 (flat case), A18 (on a slot model whose (G6) is stronger than the scan's), A20 (subgroup), A21 (invariants), A23 (annotated quotient), N9 (fair-component search). On paper or sketched only: nested reductions, S run up to `H`, the depth claim of A18 step 7, entity-cone findings (deadlocks) up to symmetry, and the reading of the Rust code as the model | Scope of the mechanization | `docs/theory/cones.md`, `docs/theory/README.md` | `9e4a123` | n/a | **citable** as the paper's statement of scope. It is not a measurement, but every "mechanized" claim in the paper must match it. |

### 8. Other topics (not for Paper 1)

These numbers are recorded in the design notes and the CHANGELOG. They concern code
generation, the conformance stream and the LSP check, which Paper 1 does not cover.

- `docs/design/code-check.md`: generation plus `codegen --check` in 10 ms (ec-site) and
  39 ms (clinic-ops); `cargo check` 0.4 s, `go vet` 0.2 s, `tsc` 0.12 s on the probes;
  clinic-ops traces reach 178 MB at the cap; a tree-sitter spike binary of 4.4 MB.
- `docs/design/conformance-stream.md` (at `f9925df7`, Apple silicon, 18 cores, load
  about 5): clinic-ops' joint BFS does not fit in 4 GiB; about 4.2×10^8 moves; 59,228,160
  states discovered by the stop; 707 s wall time; 5.7 GB peak RSS.
- `docs/design/lsp-lifecycle-check.md`: the check takes 29 ms / 64 ms / 0.04 s and 24 MB on
  clinic-ops, and similar on two other samples, under a 2 s and 512 MiB budget.
- `docs/design/verus.md`: `library.rdra` verifies in about 0.6 s.
- `CHANGELOG.md`: the conformance self-check's mutant pick went from 299 s and 487 s to
  0.36 s and 0.33 s; TypeScript traces of 602 MB now run; the plugin clone falls from 27 MB
  to 1.3 MB.
- `docs/design/event-order.md`: sample `verify` times of 18 s to 600 s under the event-order
  variants (18 cores, 128 GB, loads 1.7 to 5.0). These were design-round measurements of a
  semantics that has since been revised.

### What the evaluation must measure again

The paper's evaluation should come from one pinned commit, on one stated machine, with each
run alone. It should report instructions retired at `RDRA_JOBS=1` (which do not depend on
load, per `CLAUDE.md`) and state and orbit counts, and give wall times only as medians with
the load stated. Measuring on Linux would match the CI runner. The following figures have
no citable version yet:

1. **Reduction factors of the sub-lifecycle reduction at the current code**: per sample
   cone, unreduced states (where a run completes), reduced states, classes, and whether a
   nested reduction applied. This replaces R1–R3 and R5, and it should also re-take R2
   (how often the quotient is the identity).
2. **Symmetry factors with exact plain counts**: S7 is the only factor on record with both
   sides complete. Repeat it, and add the n=3 cones where both a plain run (at a larger
   `--memory`) and the orbit run finish. Do not use stop points (S1) or prototype factors
   (S6, S16, S22) as a denominator.
3. **Overhead when nothing permutes or nothing reduces**: instructions at `RDRA_JOBS=1`,
   `--symmetry never` against `always`, on every sample pair (S11's method at the pinned
   commit, which now includes the run-time checks added on 2026-11-28).
4. **Cost of the run-time certificates and of counterexample replay**: the partition
   stability certificate (R8), `loop_faults`, and the lift-and-replay of each printed
   trace, each as a share of the run in instructions.
5. **Liveness up to symmetry**: S9 is a single property with no orbit self-edge. Measure
   `eventually` and `leads_to` cones up to symmetry on every sample at n=3, and report how
   many lassos needed unrolling past one copy of the loop.
6. **Native against TLC on the same reduced cones**: verdict agreement and state counts at
   the pinned commit, with the TLC version the flake pins (1.8.0) and the JVM start-up
   reported separately (T7, T8).
7. **Cone against whole model for properties** (C1, C2), with the native whole-model run.
8. **Peak RSS against `--memory`**, if the paper reports memory at all (S13, S20, C5).
9. **Mutation runs over `reduce/` and `explore/symmetry/`**: no kill rate exists for either
   reduction (T1–T3 mutate other files). If the paper uses mutation scores as evidence for
   the reductions, a run over those directories is needed.
10. **Lean build and checker times** (L2–L4), on the evaluation machine, if the paper
    states them.


# Chapter C Related work

*Source: `docs/paper/related-work.md`.*

## Related work and the delta of Paper 1 (roadmap T14)

<!-- derived-from ../theory/cones.md#related-work -->
<!-- derived-from ../theory/cones.md#instance-symmetry -->
<!-- derived-from ../theory/cones.md#the-reduction-by-an-independent-sub-lifecycle -->
<!-- derived-from ../theory/native.md#related-work -->
<!-- derived-from ../theory/z3.md#related-work -->
<!-- derived-from ../theory/tlc-agreement.md#related-work -->
<!-- derived-from ../design/instance-symmetry.md#tlc-and-engine-agreement -->

This note places Paper 1 (issue #594) against prior work. Paper 1 is about two reductions
for fair-liveness model checking of data-carrying lifecycle models:

- a state-dependent symmetry group, and
- a quotient of an independent sub-lifecycle.

Both come with run-time certificates and a sound fallback, and both are mechanized in Lean
in part. The note describes main at `9e4a123` (2026-11-28). References are in
[refs.bib](refs.bib). Each bibliographic entry was checked against the publisher or DBLP
where a search found it. An entry that could not be checked has `note = {unverified}`. A
table cell that rests on a claim this note could not check is marked "(unverified)".

### What rdra-ish does, stated for comparison

The comparison uses the following facts about rdra-ish. Each cites its claim in
[docs/theory](../theory/README.md).

- **The group depends on the state (A18).** In a state `s`, `G_s` permutes the slots of an
  entity class that are not absent in `s`. Absent slots stay fixed, because creation takes
  the lowest absent slot (`FirstAbsent`). A permutation of all slots would not commute with
  that creation step: in the corpus case `presence_first_absent_asymmetry`, every reachable
  state in which only the first order exists has a full-slot swap that no run reaches. An owner (foreign-key) value on a
  present or deleted row is relabelled by the parent class's permutation, and a deleted row
  keeps its keys (G5). A syntactic scan leaves out a class when the model breaks one of the
  hypotheses (G1)–(G6), and leaving a class out loses only reduction (A20). The
  equivalence `s ≈ g(s)` with `g ∈ G_s` is a bisimulation on the states `W` whose absent
  rows hold base values. This is proved in Lean over a slot model of guards and effects
  (`SymActions.lean`). The Lean form of (G6) is stronger than the scan's.
- **Fairness transfers to the annotated quotient (A23).** Each stored edge keeps the
  permutation that took its concrete successor to the stored state. Bindings and fair
  instances move along these permutations ("threads"). An orbit self-edge counts as a real
  move. The fair-cycle search on (stored state, binding) pairs then decides `eventually`
  and `leads_to` under per-instance weak fairness exactly as on the concrete graph. This is
  proved in Lean (`SymmetryLiveness.lean`, `SearchSym.lean`). `stable`, `unchanged`,
  `precedes` and `can_reach` run without symmetry. So does `always` on a reduced cone.
- **The independent sub-lifecycle quotient (A8, A15).** A cone is split into a part `T`,
  kept concrete, and a sub-lifecycle `S` that `T` observes only through `obs` and that
  reads `T` only through input variables. The quotient merges the `S` states of an extended
  system `X` by divergence-blind stuttering bisimulation. That gives a reduced cone `R`
  that is stuttering equivalent to the cone `C` (A8). Divergence blindness would be unsound
  for `WF_vars(Next)`. A15 restores soundness with a per-class "can loop" mark. Both are
  proved in Lean for the flat case (`Reduction.lean::a8_dbsb`, `Fairness.lean::a15`).
  Nested reductions and runs of `S` up to the subgroup `H` are argued on paper.
- **Run-time certificates and fallback.** Every build checks that the partition is stable
  under (B1)–(B3) before it uses the partition (`reference::stable`). A partition that
  fails is a tool bug: the run warns and uses the sweeps' partition. A Lean-verified
  checker (`PCert.stable_of_check`) accepts the dumped certificates of the samples' flat
  runs. Every build also checks the loop marks of A15 on each explored `X` (`loop_faults`).
  A fault makes the cone run unreduced, with a warning. Every printed counterexample is
  lifted through each permutation and each class move, then replayed on the unreduced
  model, and a trace that does not replay is a tool error. A reduced liveness candidate
  that does not replay sends the property to the unreduced cone (`Spurious`). Reference
  twins (`shadow` builds, in CI only) compare each fast path with a plain run under a size
  cap.
- **Scope.** Every result is at a bound (instances per entity, Int range). Instance
  symmetry is experimental and off by default (`--symmetry never`).

### The table

The column headings are abbreviated as follows.

- **(1) State-dependent group**: the reduction's group, or its equivalence, depends on the
  current state, not only on the program.
- **(2) Fairness/liveness**: the reduction is shown to preserve liveness under a fairness
  assumption.
- **(3) Relational data/FKs**: the reduction handles values that name other objects
  (foreign keys, references, relations) and relabels them consistently.
- **(4) Traces lifted**: a counterexample found on the reduced model is turned into a
  concrete path of the original model.
- **(5) Mechanized**: a soundness proof is checked in a proof assistant.
- **(6) Certificate/fallback**: the tool checks a run's reduction at run time, or falls back
  to a sound unreduced run when a check fails.

"Partial" means the property holds for a subset of the setting, or only with an extra
assumption, which the cell names. "n/a" means the column does not apply to that kind of
technique.

| Work | (1) State-dependent group | (2) Fairness/liveness | (3) Relational data/FKs | (4) Traces lifted | (5) Mechanized | (6) Certificate/fallback |
|---|---|---|---|---|---|---|
| **rdra-ish (Paper 1)** | yes: `G_s` fixes absent slots (A18) | yes for `eventually`/`leads_to` under per-instance WF (A23) and for `WF_vars(Next)` on reduced cones (A15); other property kinds run unreduced | yes: owners relabelled with the parent class; deleted rows keep keys (G5) | yes: lifted and replayed on the unreduced model; a non-replaying trace is a tool error | partial: Lean for A18 (slot model, stronger G6), A23, A8 and A15 (flat case); nested cases and the code-to-model reading are on paper | yes: partition certificate and loop-mark check in every build, with fallback; replay of every trace; twins in CI only |
| Ip–Dill, Murφ scalarsets [ipdill96] | no: one group, fixed by the scalarset types | no: invariants and deadlock (unverified for later Murφ versions) | partial: scalarset values in variables and array indices are permuted consistently; no allocation order | partial: correspondence of quotient and concrete paths is shown; how Murφ prints traces (unverified) | no | no: the type rules guarantee symmetry statically; no run-time check |
| Emerson–Sistla [emersonsistla96, emersonsistla97] | no: a fixed automorphism group of process indices | yes: CTL\* with symmetric formulas (1996), fairness through annotated quotient structures (1997) | no: process indices, not relational data | yes: annotated quotient edges carry permutations | no | no |
| Surveys and successors: Clarke et al. [clarke96symmetry, clarke98symmetry], Sistla–Godefroid [sistlagodefroid04], virtual symmetry [emerson00virtual], survey [miller06survey] | partial: partial, near, rough and virtual symmetry handle asymmetric systems, and guarded annotated quotients track added edges; none that we found fixes unallocated slots per state | yes: CTL\*-X and fairness under symmetry are covered | no | yes (annotated and guarded quotients) | no | no |
| Heap and process symmetry for dynamic software: dSPIN [iosif04], Bogor | partial: canonical heap order is computed per state, over objects allocated so far | (unverified) | yes: references between heap objects are relabelled by the canonical order | (unverified) | no | no |
| TLC symmetry sets [tlaplus-contrib, lamport-symliveness] | no: permutations of model values in named sets | no: TLC's documentation says liveness checking under symmetry can miss violations or report nonexistent ones | partial: model values are permuted wherever they occur in the state; slots must be model values, not ordered integers | partial: for liveness the trace "may not be a correct counterexample" (Lamport); safety traces (unverified) | no | no: TLC does not check that a declared set is a symmetry set |
| Alloy / Kodkod symmetry breaking [torlak07kodkod, shlyakhter07] | no: static lex-leader predicates over atoms of a bounded universe | no: bounded traces only; fairness is a user formula (unverified for Alloy 6's interaction with symmetry breaking) | yes: relations over atoms | yes, trivially: each instance found is concrete | no (unverified) | no |
| Event-B / ProB symmetry (permutation flooding, nauty, hash markers) [leuschel07flooding, turner07symmetry, spermann08nauty, turner10refinement] | no: deferred sets permuted globally | partial: ProB's LTL checker is reported usable with symmetry [plagge10z] (unverified which modes and with which fairness) | yes: B states are sets and relations over deferred-set elements; nauty canonicalizes the relational graph | (unverified) | partial: a refinement-based correctness proof in B for safety [turner10refinement]; whether it was machine-checked (unverified) | no: the hash mode is approximate and can merge non-symmetric states |
| SPIN partial-order reduction (contrast) [holzmannpeled94, peled93, godefroid96] | n/a: the ample set is chosen per state, but no group is involved | yes: stutter-invariant LTL (LTL-X); weak fairness option (compatibility with POR unverified) | no | yes: the reduced graph is a subgraph, so its paths are concrete | partial: ample-set POR verified in Isabelle/HOL for the CAVA checker [brunner18por], not SPIN's code | no |
| Artifact-centric / data-centric processes [deutsch09icdt, hariri13pods, calvanese13pods, belardinelli14jair] | partial: finite abstractions up to isomorphism of data values that keeps constants (genericity), not a slot group (unverified per paper) | partial: LTL-FO or first-order μ-calculus over unbounded data; fairness not a focus (unverified) | yes: the state is a relational database; keys and foreign keys in several models | partial (unverified) | no | no |
| VERIFAS [li17verifas] | partial: symbolic search over isomorphism types of artifact tuples (unverified detail) | partial: LTL-FO properties; fairness (unverified) | yes: Hierarchical Artifact Systems with a database schema (keys and foreign keys, unverified detail) | partial (unverified) | no | no |
| Parameterized: cutoffs [emersonkahlon00, emersonnamjoshi95] | no: full symmetry of identical processes is assumed | yes: indexed CTL\*\\X, with fairness in later work (unverified for the cited results) | no | n/a: a result at the cutoff transfers to every size | no | no |
| Parameterized: invariant inference (Ivy [padon16ivy, padon18liveness], mypyvy [wilcox24mypyvy], DistAI [yao21distai], IC3PO [goel21ic3po]) | no: IC3PO uses the symmetry of sorts to generalize clauses, a fixed symmetry | partial: Ivy reduces liveness to safety in first-order logic [padon18liveness]; DistAI and IC3PO prove safety only | yes: first-order relations over unbounded sorts | n/a for proofs; bounded counterexamples are concrete | no for the meta-theory; each proof is checked by an SMT solver | yes for safety: the inductive invariant is a certificate any SMT solver re-checks |
| Stuttering bisimulation, divergence-blind [browne88, denicola95, groote90, groote17tocl, manolios99] | n/a | partial: divergence-sensitive variants preserve CTL\*-X; divergence-blind does not preserve liveness under divergence; fairness is not addressed | no | partial: quotient tools do not, in general, lift traces (unverified) | partial: ACL2 used per system to prove a stuttering bisimulation [manolios99]; the algorithms are proved on paper (unverified) | no |
| Cone of influence [kurshan94, clarke99book, clarke94abstraction, clarke00cegar] | n/a | partial: exact for properties over the kept variables; fairness constraints must be kept in the cone | no: works on variables, without slot allocation or event marks | yes: by projection; over-approximations need a check (CEGAR) | no (unverified) | partial: CEGAR checks each abstract counterexample on the concrete model |
| Apalache [konnov19apalache] | no: no symmetry reduction (unverified) | partial: liveness by a liveness-to-safety reduction in recent versions (unverified detail) | yes: TLA+ sets and functions encoded in SMT | yes: SMT models give concrete bounded traces | no | partial: inductive-invariant checking gives an unbounded safety proof when the user supplies the invariant |

### The delta, row by row

**Ip–Dill (Murφ scalarsets).** Ip and Dill [ipdill96] introduce the scalarset type. The
type's syntactic restrictions (no ordering, no constants, only `=` and `≠`) guarantee that
the whole state space is symmetric under permutations of the type's values. Murφ then
stores one canonical state per orbit. rdra-ish's scan rules (G1) and (G2) are the same kind
of syntactic guarantee for row ids and owners. rdra-ish adds three things:

- **Slot allocation order.** rdra-ish slots are not unordered values: creation takes the
  lowest absent slot. Under full scalarset symmetry, that step would not commute. `G_s`
  restores commutation by fixing the absent slots of each state, and the bisimulation is
  proved over the base-absent states `W` only (A18).
- **Foreign keys.** The relabelling of owners follows the parent class's permutation,
  including on deleted rows (G5). Coupled classes (an identity start) move together.
- **Liveness.** A23 covers `eventually` and `leads_to` under per-instance weak fairness.
  The scalarset paper treats safety.

SymmSpin [bosnacki02symmspin] brings scalarsets to Promela and SPIN. The same deltas apply
to it.

**Emerson–Sistla.** Emerson and Sistla [emersonsistla96] quotient by a group of process
permutations and model check symmetric CTL\* on the quotient. Their 1997 paper
[emersonsistla97] treats fairness with annotated quotient structures. That paper is the
direct ancestor of A23: rdra-ish's edges carry permutations, and fairness is read along
threads, the way annotated edges let fairness be checked on the quotient. The delta:

- The group is per state. `G_s = G_{g(s)}` holds for `g ∈ G_s`, but `G_t` grows when a step
  creates a row. So the "symmetry of the structure" is not one group, and A23's hypotheses
  (Sym) state commutation on `W` with a family of groups.
- Fairness is per action instance, and an instance is keyed by its parameter slots. The
  instance therefore moves with the edge permutation too. The theorem proves that a
  component admits a fair cycle exactly when every piece of a thread is taken or disabled.
- A23 is proved in Lean, with a recorded counterexample showing that commutation on
  reachable states alone does not suffice (commit `d99c1133`).

**Surveys and successors.** The surveys [clarke96symmetry, clarke98symmetry,
miller06survey] and the later work on asymmetric systems are the closest prior work for
claim (1):

- partial symmetry and guarded annotated quotients, which add transitions to make a model
  symmetric and track the added edges [sistlagodefroid04];
- near, rough and virtual symmetry, under which a structure is bisimilar to its quotient
  even though it is not fully symmetric [emerson00virtual].

"Dynamic symmetry reduction" [emersonwahl05] is a different idea despite its name. It
computes orbit representatives during symbolic fixpoint iterations for a fixed group.

We did not find a construction in which the group of a state fixes the slots not yet
allocated, so that allocation order is respected without adding transitions. A reader may
still see `G_s` as an instance of virtual symmetry, or of the state symmetries of these
surveys. The paper should compare `≈` with virtual symmetry explicitly, not claim novelty
of "state-dependent symmetry" in general (see "Risky cells" below). The concrete deltas
that survive that comparison:

- the orbit relation is a bisimulation without added transitions, because absent slots are
  fixed;
- relational keys are relabelled;
- A23 is mechanized.

**Heap and process symmetry for dynamic software (dSPIN, Bogor).** This row was added
because it is the nearest prior work for allocation and references. Iosif [iosif04]
defines heap and process symmetry reductions for programs that allocate objects
dynamically. A state's heap is put in a canonical order, which relabels references
consistently. This is the closest prior treatment of "foreign keys relabelled" and of a
state space where objects come into existence over time. The differences:

- rdra-ish slots are a bounded array with lowest-slot allocation, and allocation order is
  observable (`FirstAbsent`). `G_s` keeps that order exact. A heap canonicalization makes
  addresses unobservable instead.
- rdra-ish proves fair-liveness preservation (A23) and mechanizes it. We could not confirm
  what the dSPIN work proves for liveness.
- rdra-ish lifts and replays every trace.

**TLC symmetry sets.** TLC [yu99tlc] is the explicit-state checker for TLA+ [lamport02],
the language rdra-ish exports to. TLC permutes the model values of the sets a configuration names, and
it does not check that the specification is symmetric. TLC's own documentation and the
TLA+ project say that liveness checking under symmetry can miss violations, can report
violations that do not exist, and can print traces that are not correct counterexamples
[tlaplus-contrib, lamport-symliveness]. rdra-ish's TLA+ export emits no `SYMMETRY`. Its
slots are integers that `FirstAbsent` orders, so a symmetry set over them would be unsound
on the very asymmetry `G_s` handles (`docs/design/instance-symmetry.md`, "TLC and engine
agreement"). The delta:

- rdra-ish derives the group from the model and checks the hypotheses syntactically. Out
  of a class that breaks one, it makes a fixed class, losing only reduction (A20).
- rdra-ish gives a liveness procedure on the quotient with a proof (A23).
- rdra-ish replays every printed trace.

TLC therefore acts as an independent second opinion on the unreduced or #74-reduced cone,
not as a symmetric checker to compare against.

**Alloy / Kodkod.** Kodkod [torlak07kodkod] detects symmetries of a bounded relational
problem and adds lex-leader symmetry-breaking predicates, following Shlyakhter
[shlyakhter07]. This is the closest prior work for symmetry over relational data with
foreign keys. The setting differs:

- Kodkod's symmetries are global over a universe of atoms, and they break symmetry in a SAT
  encoding of bounded instances or traces, not in an explicit-state fixpoint.
- No fairness transfer is needed or offered.
- Each found instance is concrete, so there is nothing to lift.

rdra-ish's delta is the fair-liveness setting and the per-state group. The common ground is
the relational treatment of keys.

**Event-B / ProB.** ProB offers symmetry reduction over B's deferred sets: by permutation
flooding [leuschel07flooding], by canonical forms computed with nauty on a graph of the
state [spermann08nauty], and by approximate hash markers [turner07symmetry]. Turner,
Butler and Leuschel give a refinement-based correctness proof of the reduced model-checking
algorithm for safety [turner10refinement]. Whether that proof was machine-checked is
unverified. The graph canonical form handles relational states in general, which is more
general than rdra-ish's owner relabelling. The delta of rdra-ish:

- allocation-respecting groups;
- a proved fair-liveness transfer;
- a Lean proof of the bisimulation from syntactic hypotheses on guards and effects;
- run-time replay of traces.

ProB's hash mode is unsound by design (it may merge non-symmetric states). rdra-ish has no
such mode: past `BRANCH_CAP` it stores a non-canonical member of the class, which costs
states, not soundness (A21).

**SPIN partial-order reduction (contrast).** Partial-order reduction [peled93, godefroid96,
holzmannpeled94] prunes interleavings of independent transitions with a per-state ample set,
and preserves stutter-invariant LTL. The sub-lifecycle reduction targets the same blow-up (a
`T` interleaved with an independent `S`). It quotients `S`'s states instead of pruning
interleavings, which is why its correctness is a stuttering bisimulation (A8) and not an
ample-set condition. The reduced graph of POR is a subgraph, so its traces need no lifting.
rdra-ish's quotient needs a lift through class moves (A16), and it replays the result. On
mechanization, Brunner and Lammich [brunner18por] verified ample-set POR in Isabelle/HOL,
down to executable code. That goes further than rdra-ish's Lean proofs, which are about a
model of the Rust code, not about the code. This contrast should be stated plainly in the
paper.

**Artifact-centric and data-centric process verification.** Deutsch, Hull, Patrizi and
Vianu [deutsch09icdt], Bagheri Hariri et al. [hariri13pods], Calvanese, De Giacomo and
Montali [calvanese13pods], and Belardinelli, Lomuscio and Patrizi [belardinelli14jair]
verify processes over relational data. Their data is unbounded, and their decidability
results come from restrictions (guardedness, run-boundedness, state-boundedness) and from
finite abstractions up to isomorphism of data. This is the application domain closest to
rdra-ish's entity lifecycles. The differences:

- They obtain unbounded results by restricting the model. rdra-ish checks a bounded number
  of rows per entity and gives no unbounded guarantee ("Bounded results" in
  `docs/theory/cones.md`).
- Their reductions are abstractions argued on paper. rdra-ish's are symmetry and quotient
  reductions with run-time certificates, trace replay and partial Lean proofs.
- Fairness is central to rdra-ish (per-instance WF, `WF_vars(Next)`). In this line of work,
  as far as we could check, it is not a focus (unverified).

**VERIFAS.** VERIFAS [li17verifas] is a practical verifier for LTL-FO properties of
Hierarchical Artifact Systems with unbounded data. It is the closest tool in the data-centric
line. The delta matches the previous row:

- VERIFAS handles unbounded data within the HAS restrictions; rdra-ish is bounded.
- rdra-ish models lifecycles with deletion, events and per-instance weak fairness, and
  makes reductions with certificates.

Any comparison on benchmarks would need a translation between the two models. We did not
attempt one.

**Parameterized verification: cutoffs.** Cutoff results [emersonnamjoshi95,
emersonkahlon00] show that checking a fixed small number of identical processes decides all
sizes, for restricted communication (token rings, conjunctive or disjunctive guards).
rdra-ish has no cutoff theorem. Its `escalate` reruns a rule at one and two more instances
(A25), which "adds two bounded results; it is not an induction" (`docs/theory/cones.md`).
A cutoff for rdra-ish models with keys is open. The paper should not suggest that the
bounded results generalize.

**Parameterized verification: invariant inference (Ivy, mypyvy, DistAI, IC3PO).** These
tools prove safety for all sizes by finding a quantified inductive invariant:

- Ivy [padon16ivy] uses decidable EPR checks and user guidance, and handles liveness by a
  reduction to safety [padon18liveness];
- mypyvy [wilcox24mypyvy] is a research platform for such algorithms, among them the
  separator-based inference of invariants with quantifier alternations
  [koenig20separators];
- DistAI [yao21distai] enumerates candidates pruned by reachable states;
- IC3PO [goel21ic3po] uses the symmetry of sorts to boost clauses into quantified
  invariants.

Their certificates are stronger than rdra-ish's for safety: an inductive invariant is an
unbounded proof that a solver re-checks. rdra-ish's own z3 prover follows this line for
state rules (`docs/theory/z3.md`) but is outside Paper 1. For the reductions, the delta is
that rdra-ish decides fair liveness at a bound with no invariant, by explicit search on
reduced graphs. IC3PO's symmetry is a fixed symmetry used to generalize clauses, not to
quotient a state space.

**Stuttering bisimulation, divergence-blind.** The quotient of A8 uses the classical theory:

- Browne, Clarke and Grumberg [browne88] show that stuttering equivalence preserves CTL\*
  without next;
- De Nicola and Vaandrager [denicola95] separate the divergence-blind and
  divergence-sensitive variants;
- Groote and Vaandrager [groote90] give the partition refinement that rdra-ish's
  `quotient_blocks` follows (now improved to O(m log n) [groote17tocl]);
- Manolios, Namjoshi and Sumners [manolios99] prove a stuttering bisimulation in ACL2 for
  one system and model check its finite quotient.

rdra-ish differs in three ways:

- it quotients one component `S` of a product and keeps `T` concrete, which is closer to
  compositional minimization than to minimizing the whole graph;
- it uses the divergence-blind variant, which is cheaper and coarser, and repairs its
  unsoundness for `WF_vars(Next)` with the "can loop" marks of A15, checked at run time by
  `loop_faults`;
- it certifies each partition at run time and checks the certificates of the samples with
  a Lean-verified checker.

We found no prior treatment of weak fairness on a divergence-blind quotient of one
component. That search was not exhaustive.

**Cone of influence.** Cone-of-influence reduction [kurshan94, clarke99book] keeps the
variables a property depends on. Over-approximating cones are existential abstractions
[clarke94abstraction], whose counterexamples need a check against the concrete system
[clarke00cegar]. Theorem A1 is that construction for guarded actions with parameters, where
the dependency must also cover slot allocation and event marks. A28 and `recheck_on` play
the role of the concrete check. The delta is in the data model (freed slots, marks) and in
fairness: `WF_vars(A)` restricts to a cone when `A` and what it reads are in the cone, while
`WF_vars(Next)` does not (A4, A5). Cones are background in Paper 1, not a contribution.

**Apalache.** Apalache [konnov19apalache] checks TLA+ symbolically with SMT: bounded model
checking and inductive-invariant checking. It does not reduce by symmetry or quotient, as
far as we could check (unverified). It is the natural second engine for rdra-ish's TLA+
export, besides TLC. Its inductive-invariant mode is a certificate in the sense of column
(6), for safety. It is not part of rdra-ish's agreement evidence today.

**Certifying model checkers.** Namjoshi [namjoshi01certifying] has a model checker emit a
deductive proof that a simple checker can verify independently, on success as well as on
failure. rdra-ish's certificates are narrower. They certify the reduction (a stable
partition, loop marks) and each counterexample (by replay), not the verdict `ok` as a
whole. On the unreduced graph, an `ok` rests on the search itself (N9 in
`docs/theory/native.md`). The paper should use the word "certificate" in that narrower
sense and cite this work for the broader one.

### Five strongest deltas

Each delta below is stated against the closest prior work. Each has a "but" that the paper
must state with it.

1. **Against Emerson–Sistla 1997 and TLC symmetry: fair liveness on a symmetry quotient
   whose group depends on the state, mechanized.** A23 decides `eventually` and `leads_to`
   under per-instance weak fairness on an annotated quotient. Each thread follows the edge
   permutations, and fair instances keyed by parameter slots move with them. The proof is
   in Lean. Emerson–Sistla handle fairness for a fixed group on paper. TLC documents that
   its symmetry is unsafe for liveness. *But*: A23 covers `eventually`/`leads_to` only, the
   entity-cone deadlocks up to symmetry are only sketched, and the code is linked to the
   Lean model by reading.
2. **Against Ip–Dill, Murφ and ProB: a symmetry that respects lowest-slot allocation and
   relabels foreign keys, with a syntactic scan that can only lose reduction.** `G_s` fixes
   absent slots, so the orbit relation is a bisimulation on the base-absent states without
   adding transitions. Owners follow the parent's permutation, including on deleted rows.
   A20 shows that leaving out more classes is always sound. *But*: no scan rule fires on
   any sample or corpus model, so the rules are exercised only by hand-built witnesses, and
   11 reasons of `G_T` and `H` have no witness.
3. **Against the stuttering-bisimulation literature: a divergence-blind quotient of one
   component that stays sound for `WF_vars(Next)`.** A15 adds "can loop" marks per class,
   with a Lean proof of the flat case and a run-time check of the marks in every build. A
   failed check falls back to the unreduced cone. *But*: the nested case and the runs of
   `S` up to `H` are on paper.
4. **Against all symmetry and quotient tools surveyed: run-time certificates plus a sound
   fallback.** Every build certifies partition stability (B1)–(B3) before use and checks
   the loop marks. Every printed trace is lifted and replayed. A failure gives a warning
   and an unreduced run, or a tool error, never a verdict. A Lean-verified checker accepts
   the samples' flat partitions. No row of the table combines run-time checking of the
   reduction with a fallback. *But*: the Lean checker covers 182 flat runs and leaves out
   nested, capped and `H` runs. The twins that compare fast paths with plain runs run in CI
   (`shadow`), not in release builds. The certificate cost is measured once and roughly
   (25–35% of the partition's time, `measurements.md` R8).
5. **Against artifact-centric verification and VERIFAS: fair-liveness checking of bounded
   data-carrying lifecycles with both reductions composed.** A cone is reduced by the
   sub-lifecycle quotient. Its `T` part is then explored up to `G_T`, `S` itself may be run
   up to `H`, and traces lift through both, in reverse order. *But*: the results are
   bounded, and those systems decide unbounded data under restrictions. The paper must
   present this as a different point in the design space, not as a superset.

### Risky cells

- **(1) for the surveys row, and the novelty of "state-dependent" in general.** Virtual
  symmetry [emerson00virtual], guarded annotated quotients [sistlagodefroid04], state
  symmetries in the surveys, and per-state heap canonicalization [iosif04] all relax global
  symmetry. A reviewer may argue that `G_s` is a special case of one of them. Before
  claiming novelty, the paper should state precisely why `≈` (the orbit relation of the
  family `G_s` on `W`) is or is not a virtual symmetry of the structure, and compare it
  with dSPIN's heap symmetry.
- **(2) for Ip–Dill and for SPIN POR with weak fairness.** We did not confirm what liveness
  Murφ supports under symmetry, or whether SPIN's POR is documented as compatible with its
  weak-fairness option.
- **(2), (4) and (6) for Event-B/ProB.** The claim that ProB's LTL checker works "without
  restriction" under symmetry comes from a secondary source [plagge10z]. Which symmetry
  modes and which fairness notions it covers is unverified. So is whether the 2010
  correctness proof [turner10refinement] was machine-checked.
- **(1) and (3) for the artifact-centric rows and VERIFAS.** "Up to isomorphism types" and
  "keys and foreign keys in HAS" are from memory of those papers, not re-read. Check them
  against the papers before submission.
- **(5) for stuttering bisimulation and cone of influence.** Mechanized proofs of
  partition-refinement algorithms or of cone-of-influence soundness may exist (for example
  in Isabelle's Archive of Formal Proofs). We did not find one, but the search was short.
  "No" may be wrong.
- **(5) for rdra-ish itself.** "Mechanized" must be qualified each time it is used. The
  Lean proofs are about a model of the code, their (G6) is stronger than the scan's, and
  the nested reductions and `H` are not modelled (`docs/theory/README.md`, open items for
  A8, A15, A18 and A23).
- **Apalache (1) and (2).** "No symmetry" and "liveness by liveness-to-safety" come from a
  search result describing a 2025 tool paper. Check them in the Apalache manual.
- **TLC (4) for safety.** Lamport's statement about wrong traces concerns liveness. Whether
  safety traces under symmetry are always concrete paths is unverified (TLC issue #816
  reports odd error transitions with symmetry sets, which we did not read in full).
