# Lean proofs of the reduction and symmetry theorems

<!-- derived-from ../../docs/theory/cones.md#the-reduction-by-an-independent-sub-lifecycle -->
<!-- derived-from ../../docs/theory/cones.md#instance-symmetry -->
<!-- derived-from ../../docs/theory/cones.md#wf_varsnext-on-a-reduced-cone -->
<!-- derived-from ../../docs/theory/semantics.md#temporal-properties -->
<!-- derived-from ../../docs/theory/semantics.md#verdicts -->

This directory holds a Lean 4 project. It proves parts of Theorems A8, A15, A18, A20, A21 and A23 of
[cones.md](../../docs/theory/cones.md) from their hypotheses, Lemma 5 from Definition 3,
Lemma 6 from Definition 4, and Theorem A18's commutation from (G1)–(G6). The theorems of A8 and the
others are stated over abstract transition systems; Lemmas 5 and 6 are stated over a model of
the cone by its actions
([Definition 3 and Lemma 5 over actions](#definition-3-and-lemma-5-over-actions),
[Definition 4 and Lemma 6 over actions](#definition-4-and-lemma-6-over-actions)), and A18's
commutation over a model of the cone by its slots
([Theorem A18 over slots](#theorem-a18-over-slots)). None of them reads the Rust code.

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

`scripts/check-lean-refs.py` runs in CI's `check` job on every pull request, prose-only
ones included, and takes under a second. It checks three things:

- every Rust name this README, the Lean doc comments and the Lean-citing lines of
  claims.md and cones.md cite (`split_at`, `Reduction::stages`, `partition.rs::divergence`)
  names a definition in `crates/`;
- every name in the first column of a table below is declared in the file of its `File`
  column;
- every such name is listed in CheckAxioms.lean.

Its docstring gives the exact rule.

The toolchain is pinned in [lean-toolchain](./lean-toolchain) (`leanprover/lean4:v4.36.0`).
elan installs it on the first build. `nix develop` provides elan; `nix shell nixpkgs#elan`
does too.

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
- **The rise table** (`RiseTable`, for `a8_staged`). Every rise of a reachable
  representative is entered under its block and the input bytes it ends at.
- **The nested case** (`InnerMap`, `Tower`, for `a8_nested`). Each level's inner run is
  related to the level above by an inner map, which `a8_innerMap` derives from that level's
  own A8 hypotheses. That the inner split puts `obs` and the input bytes on its `T` side and
  keeps the rises (the seeds `reduced_in` gets in `Classes::of`) enters as the choice of
  observation and visibility of the map, and the reindexing of `S × I` as `T' × S'` as an
  `InnerMap.ofIso`.
- **Definition 4's shape of `X`** (`Extended`). A silent edge keeps the input bytes, and a
  rise keeps `S`. `Ext.extended` proves it of the `X` of `Restrict.lean`.
- **(B1)–(B3)** (`Stable`), on the reachable states of `X`, with
  `rep(B x) ∈ Reach(X) ∩ B x`.
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
    a reachable state can loop (`SilentCycle`, Definition 5, case 1). The code's
    `partition.rs::divergence` and, for `obs = ∅`, `s_cycles` must make it true; cones.md
    argues both.
- **The fairness model** (`Behaviour.lean`). A fair instance is a set of `T_a` labels (fair
  actions are `T_a` actions, Lemma 5 (e), assumed by giving instances on `Ta` only). A step
  takes an instance only when it changes the state (assumption τ of native.md). Behaviours
  may stutter.

## What is not covered

- **The code.** No theorem here reads `Extended::build`, `restrict_to`, the folds,
  `quotient_blocks`, `Group::apply` or `Canon::canonical`.
  - Lemma 6 is proved over the action model, whose `X` is built as `Extended::build` builds
    it. The fields of `Lemma6Hyp` are read from the code, and the model leaves out what
    [below](#what-the-lemma-6-model-leaves-out) lists. Reading the builder itself would need
    a deep embedding of it.
  - (G1)–(G6) are stated over a slot-level model of the code's syntax
    ([below](#theorem-a18-over-slots)). That the model's `act`, hypotheses and effects are
    `Group::apply`, `breaks::scan` and `each_successor` is read from the code, not derived.
  - (B1)–(B3) are certified at run time on each partition (claim A8, R). That certificate
    is the nearest link from the code to `Stable`, but it is not `Stable` itself. The code
    checks that one sweep with every block as a splitter splits nothing, while `Stable.b2`
    is the path form of (B2). cones.md argues the step between the two, and Lean does not
    check it.
- **A8, the rest of the theorem.** The proofs leave out these parts:
  - The rises carry no `stands`. Consequences 2 to 4 read only the `T_a` labels of `C` and
    `R`, and `R`'s `T` move reads a rise only through the input bytes it ends at.
  - What `stages` returns. `Lemma6Staged` takes the stage list as a parameter. Over the
    action model `Ext.stages` is `Reduction::stages`, and `lemma6_staged` proves that the
    rises of that `X` follow it.
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
  - The folds. Their effect is inside the step relations that Lemma 6 relates. Lemma 5
    (c)–(e) are derived over the action model with `FoldData`
    ([below](#definition-3-and-lemma-5-over-actions)). Lemma 6's model reads a param fold's
    bit and takes `Fold::holds` computing it exactly as the hypothesis `Lemma6Hyp.exact`;
    node folds and groups of folds that share a bit are not modelled. Lemma 5 (e) enters
    `a15` and `a8_plain_iff` as fair instances given on `T_a` labels.
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

## Definition 3 and Lemma 5 over actions

`Actions.lean`, `Split.lean` and `SplitExample.lean` go one level below the abstract systems
above. They model the cone system by its actions and prove Lemma 5 from Definition 3. With
them, the hypothesis `Split` of `a8_dbsb` (Lemma 5 (a)(b)) is a theorem, `split_computed`,
whose one hypothesis is `Local` (below). Lemma 5 (c)–(e) need `FoldData` as well.

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

### What these hypotheses are

- **`Local`**, the locality of the code's `rd` and `wr`:
  - every guard conjunct reads only variables of `rd a ∪ wr a`;
  - a conjunct's value depends only on the variables it reads;
  - the effect leaves every variable outside `wr a` unchanged;
  - the effect's value on `wr a` depends only on `rd a ∪ wr a`.

  The footprint is `rd ∪ wr` and not `rd` alone. The code does not count two things as reads:
  taking a `new` slot (`action_reads`), which looks at the existence the action writes, and
  the slots of a written variable that the binding leaves alone. This is the statement that
  the locality of expression evaluation must give for each action.
- **`FoldData`**, for the folds and `raw_reads`, given the split:
  - `raw_rd`: `raw_reads` lies inside `rd`;
  - `cover`: every `T_v` read of an `S_a` action is a raw read, an existence of a raw `T_v`
    read, or a read its fold covers. A `Triggered` parameter's existence is the middle case:
    `raw_reads` leaves it out, and `existences` of its mark brings it back;
  - `ex_mono`: an existence of a monotone variable is monotone. In `monotone_vars` only
    creates and deletes write an existence, from absent to present to deleted. With inputs
    off, `mono` is constant `false` and the condition is empty.
- **`mono`** is a parameter. Lemma 5 (d) reads it only through `ex_mono` and the closure.
  The write-edge acyclicity that `monotone_vars` computes is not modelled.
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

## Definition 4 and Lemma 6 over actions

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

### What `Lemma6Hyp` assumes

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

### What the Lemma 6 model leaves out

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

## Theorem A18 over slots

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
| `ExampleOffW.off_W_counterexample` | `SymActionsExample.lean` | none | Shops and orders, a creation that does not write the order's optional owner: every hypothesis of `step_comm` holds, and a step from a state outside `W`, with the absent order's owner on shop 0, has no image under the swap of the shops. |

### What these hypotheses are

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
  reads an owner on a `New` parameter in the pre-state. Each condition also holds when the
  class it names does not move, which is how the scan's leaving a class out enters.
- **(G5)** is the semantics: a delete resets the row's columns (not its run marks) and keeps
  its owners.
- **Guards only.** Invariants, `after` assertions and properties are not in the model, so
  (G6) outside guards is not derived; `a18_holds_iff` takes the invariance of its invariant
  under `G_s` as a hypothesis.

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
- The roles carry no data. `Parent` and `Triggered` bind a present slot, and what else they
  ask is in the guard.
- The reduced cone (`G_T`) and the run of `S` up to `H` (A22) are not modelled.

The slot model and the action model of `Actions.lean` are separate. `Act.Model` keeps guards
and effects opaque behind read and write sets, and `SymAct.Sys` gives them a syntax. Its
`Param` and `Role` restate `Act.Param` and `Act.Role` without `Role.earlier`'s step. A
common model would read `Act.Model`'s guard and effect from `SymAct`'s syntax and share one
`Role` with the semantics Lemma 6 gives it.
