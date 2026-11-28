# Lean proofs of the reduction and symmetry theorems

<!-- derived-from ../../docs/theory/cones.md#the-reduction-by-an-independent-sub-lifecycle -->
<!-- derived-from ../../docs/theory/cones.md#instance-symmetry -->
<!-- derived-from ../../docs/theory/cones.md#wf_varsnext-on-a-reduced-cone -->
<!-- derived-from ../../docs/theory/semantics.md#temporal-properties -->
<!-- derived-from ../../docs/theory/semantics.md#verdicts -->

This directory holds a Lean 4 project. It proves parts of Theorems A8, A15, A18, A20, A21 and A23 of
[cones.md](../../docs/theory/cones.md) from their hypotheses. Every theorem is stated over
abstract transition systems. None of them reads the Rust code.

## Building

```sh
scripts/check-lean-proofs.sh
```

The script runs `lake build`, then `#print axioms` on each theorem that
[CheckAxioms.lean](./CheckAxioms.lean) lists. It fails on any of these:

- a `sorry`, `admit` or `axiom` in the sources;
- a listed theorem that depends on `sorryAx`;
- a listed theorem that depends on any axiom outside Lean's standard three (`propext`,
  `Classical.choice`, `Quot.sound`).

`lake build` alone passes with a `sorry`, because Lean only warns about one. A clean build
takes about 2 seconds.

CI's `lean` job runs the script on every pull request that touches this directory, the
script or `ci.yml`, and on every run on main. The job caches `~/.elan`.

The toolchain is pinned in [lean-toolchain](./lean-toolchain) (`leanprover/lean4:v4.36.0`).
elan installs it on the first build. Without elan, run `nix shell nixpkgs#elan` and then
the script.

**No Mathlib.** The project imports only Lean's core. The proofs need five things:

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

## What each theorem proves

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

## What the hypotheses are

The proofs assume the following hypotheses and do not derive them:

- **Lemma 5 (a)(b)** (`Split`). An `S_a` step keeps `T`. A `T_a` step keeps `S` and reads
  `S` only through `obs`.
- **Lemma 6** (`Lemma6Fwd`, `Lemma6`, `Lemma6By`, `Lemma6Staged`). Every initial state
  projects to an initial state of `X`. From a reachable `(t, s)`, an `S_a` step of `C` is a
  silent edge of `X` from `x(t, s) = (s, inp t)`, and the converse holds; `Lemma6By stands`
  also asks that the edge's action stands for the step's action with its binding. A `T_a`
  step is matched by a sequence of rises from `x(t, s)` to `x(t', s)`; `Lemma6Staged` asks
  for one rise per stage of `stages`. `x(t, s) ∈ Reach(X)` is not assumed: `x_reach`
  derives it.
- **The rise table** (`RiseTable`, for `a8_staged`). Every rise of a reachable
  representative is entered under its block and the input bytes it ends at.
- **The nested case** (`InnerMap`, `Tower`, for `a8_nested`). Each level's inner run is
  related to the level above by an inner map, which `a8_innerMap` derives from that level's
  own A8 hypotheses. That the inner split puts `obs` and the input bytes on its `T` side and
  keeps the rises (the seeds `reduced_in` gets in `Classes::of`) enters as the choice of
  observation and visibility of the map, and the reindexing of `S × I` as `T' × S'` as an
  `InnerMap.ofIso`.
- **Definition 4's shape of `X`** (`Extended`). A silent edge keeps the input bytes, and a
  rise keeps `S`.
- **(B1)–(B3)** (`Stable`), on the reachable states of `X`, with
  `rep(B x) ∈ Reach(X) ∩ B x`.
- **A18's commutation** (`Sym`). This covers:
  - every `g ∈ G_s` maps a step `s → t` to the step `g(s) → g(t)`, and `g ∈ G_t`;
  - `G_{g(s)} = G_s`;
  - (G3).

  cones.md derives the commutation from (G1)–(G6). The Lean statement assumes it on every
  state, where cones.md states it on reachable states. `SymOn` assumes it on a set `W` that
  holds the initial states and is closed under steps and every `G_s`, which the quotient's
  stored states need; reachable states alone do not suffice
  (`ExampleReachOnly.reach_only_counterexample`).
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
    a reachable state can loop (`SilentCycle`, Definition 5, case 1). The code's
    `divergence` and, for `obs = ∅`, `s_cycles` must make it true; cones.md argues both.
- **The fairness model** (`Behaviour.lean`). A fair instance is a set of `T_a` labels (fair
  actions are `T_a` actions, Lemma 5 (e), assumed by giving instances on `Ta` only). A step
  takes an instance only when it changes the state (assumption τ of native.md). Behaviours
  may stutter.

## What is not covered

- **The code.** No theorem here reads `Extended::build`, `restrict_to`, the folds,
  `quotient_blocks`, `Group::apply` or `Canon::canonical`.
  - The gap between Lemma 6 and the code is where the stub-param bug sat, and it stays
    open. Closing it would need a deep embedding of the transition-system builder.
  - The same holds for (G1)–(G6) and the symmetry scan.
  - (B1)–(B3) are certified at run time on each partition (claim A8, R). That certificate
    is the nearest link from the code to `Stable`, but it is not `Stable` itself. The code
    checks that one sweep with every block as a splitter splits nothing, while `Stable.b2`
    is the path form of (B2). cones.md argues the step between the two, and Lean does not
    check it.
- **A8, the rest of the theorem.** The proofs leave out these parts:
  - The rises carry no `stands`. Consequences 2 to 4 read only the `T_a` labels of `C` and
    `R`, and `R`'s `T` move reads a rise only through the input bytes it ends at.
  - What `stages` returns. `Lemma6Staged` takes the stage list as a parameter; that the
    rises of `X` follow `Reduction::stages` is part of the Lemma 6 gap.
  - The outer representative in the nested case. The code takes it among the inner run's
    states, so its `S` bytes are an inner representative's, which need not be a reachable
    state of `X`; `a8_nested` takes a reachable member of the block (`exists_rep`). `R`
    reads the representative only through `obs` and its rises, which (B1) and (B3) fix on
    the block, but that the inner representative's are those of a reachable member is not
    derived.
  - The outer reduced cone's moves in the nested case. `a8_nested` builds `R` with the class
    moves and rises of `X` under the pulled-back partition. The code reads them on the inner
    run. `InnerMap.path_fwd`, `path_back`, `rise_fwd` and `rise_back` carry the moves between
    the two, but no theorem states that the two `R`s have the same steps.
  - Consequences 2 to 4 in the nested case are not restated. The `R` of `a8_nested` is
    `reduced` on a partition that `stable_of_stableV` makes `Stable`, so `a8_edges_iff`,
    `a8_stuck_iff` and `a8_plain_iff` apply to it as they stand.
  - Consequence 4 is stated for `□(trig ⇒ ◇goal)` at one binding, the form of D21's
    `eventually` and `leads_to`, over behaviours. That the native search over lassos
    decides it is N9. `always` is an invariant (consequence 1).
  - Lemma 5 (c)–(e) and the folds. Their effect is inside the step relations that Lemma 6
    relates; Lemma 5 (e) enters as fair instances given on `T_a` labels.
- **A15, the rest of the theorem.**
  - The run's search. `a15` reads the run's verdict as "no lasso of `R` with the fairness
    and stops above violates the property". That the search over strongly connected
    components finds such a lasso when one exists is N9, which Lean does not check. The
    lift and replay of a candidate are run-time evidence, and the converse (a counterexample
    of the run is one of `C`) is not claimed.
  - The nested case. Definition 5's cases 2 (an inner class that can loop) and 3 (an orbit
    self-edge of `S` up to `H`), and the proof's (ii), need the inner reduction, which
    Reduction.lean does not model.
  - Case 4 (`obs = ∅`, `s_cycles` over `monotone_vars`) enters only through the hypothesis
    `hloop`. The proof's (iii), that a variable whose write graph is acyclic changes
    finitely often, is not formalized.
- **A23, the rest of the theorem.**
  - The run's search. `a23` reads the orbit run's verdict as "no thread-fair annotated lasso
    of the canonical quotient violates the property". `OrbitRun::decide_liveness` accepts a
    strongly connected component of goal-free pairs when every piece of its thread graph is
    taken or meets a disabled pair. Lean does not check that an accepted component holds a
    thread-fair violating annotated lasso, nor that every such lasso lies in an accepted
    component or reaches a stop (the analogue of N9). `threadFair_iff_pieces` covers one
    thread round one cycle, not the pieces over a component with several cycles.
  - The entity-cone findings up to symmetry (`state_model_findings`) and deadlocks are not
    stated.
- **A18's BFS depth.** The claim that each class is reached at its least depth is not stated.

The sources hold no `sorry`, no `admit` and no `axiom`, and no statement is left unproved.
