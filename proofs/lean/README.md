# Lean proofs of the reduction and symmetry theorems

<!-- derived-from ../../docs/theory/cones.md#the-reduction-by-an-independent-sub-lifecycle -->
<!-- derived-from ../../docs/theory/cones.md#instance-symmetry -->
<!-- derived-from ../../docs/theory/cones.md#wf_varsnext-on-a-reduced-cone -->
<!-- derived-from ../../docs/theory/semantics.md#temporal-properties -->
<!-- derived-from ../../docs/theory/semantics.md#verdicts -->
<!-- derived-from ../../docs/theory/native.md#the-check -->

This directory holds a Lean 4 project. It proves parts of Theorems A8, A15, A18, A20, A21 and A23 of
[cones.md](../../docs/theory/cones.md) from their hypotheses, Lemma 5 from Definition 3,
Lemma 6 from Definition 4, and Theorem A18's commutation from (G1)–(G6). The theorems of A8 and the
others are stated over abstract transition systems; Lemmas 5 and 6 are stated over a model of
the cone by its actions
([Definition 3 and Lemma 5 over actions](#definition-3-and-lemma-5-over-actions),
[Definition 4 and Lemma 6 over actions](#definition-4-and-lemma-6-over-actions)), and A18's
commutation over a model of the cone by its slots
([Theorem A18 over slots](#theorem-a18-over-slots)). The action model of Lemmas 5 and 6 is
read off that slot model, and `Local`, a hypothesis of both lemmas, is derived from its syntax
([One model of the actions](#one-model-of-the-actions)), and so are Lemma 6's other
hypotheses but Lemma 5 (a)(b), the split's inputs and step (a)
([Lemma 6's hypotheses from the syntax](#lemma-6s-hypotheses-from-the-syntax)). N9 of
[native.md](../../docs/theory/native.md#the-check), the liveness search by fair components,
and the same search on the pairs of a run up to symmetry are stated over abstract
transition systems too ([The liveness searches](#the-liveness-searches)). None of them reads
the Rust code. Two executables check what the code computes
([Checking the code's sets with Lean](#checking-the-codes-sets-with-lean)): `lemma5-check`
checks the sets of each split against Lemma 5, and `partition-check` checks each partition
of a flat run of `X` against (B1)–(B3) and the run against the shape of `X`.

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

## What is not covered

- **The code.** No theorem here reads `Extended::build`, `restrict_to`, the folds,
  `quotient_blocks`, `Group::apply` or `Canon::canonical`.
  - Lemma 6 is proved over the action model, whose `X` is built as `Extended::build` builds
    it. The fields of `Lemma6Hyp` are read from the code, and the model leaves out what
    [below](#what-the-lemma-6-model-leaves-out) lists. Over the slot model, `fold_of` and
    the conjuncts `restrict_to` keeps are modelled on the syntax, and every field but
    Lemma 5 (a)(b) and step (a) is derived ([below](#lemma-6s-hypotheses-from-the-syntax));
    that those models are the code's is read from it. Reading the builder itself would need
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
    `Sweep.Marked` are read from the code. The fallback partition of
    `partition/reference.rs::quotient_blocks` passes the same check, and a second reading of
    (B2), `partition/reference.rs::marking_splits_nothing`, which marks by the rules of
    `Sweep.Marked` alone and checks the hypotheses of `Sweep.b2_of_marking`; a fallback that
    fails leaves the cone unreduced. On the flat runs of the
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
    bit and takes `Fold::holds` computing it exactly as the hypothesis `Lemma6Hyp.exact`.
    Over the slot model `exact_holdsB` proves it for the bit `holdsB` that the fold's
    conditions define, so what is read from the code is that `holds_fast` computes
    `holdsB`. Node folds and groups of folds that share a bit are not modelled. Lemma 5 (e) enters
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

## The liveness searches

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
  reachability. Each run checks the plain search's components, fair flags, stops and
  backward walk against the criterion instead (`liveness/certify.rs::ComponentRun::search_faults`,
  N9's certificate), an argument on paper only.
- N10, the cut at `expanded` of a capped graph (`open`, `s < expanded`): the graph is complete.
  The certificate recomputes the cut from `expanded`.
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

## Definition 3 and Lemma 5 over actions

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

### What these hypotheses are

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

## Checking the code's sets with Lean

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

### The partitions and the shape of `X`

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

Each field is a fact about one action. Every build checks the fields marked (R) on the built
`X` for `FoldKey::Param` folds, from the actions' syntax (`reduce/lemma6hyp.rs::lemma6_faults`, which names
each field it checks in its module doc), and decides the cone unreduced on a fault; for node
folds and groups of folds sharing a bit, and for the fields not marked, the fact is read from
the code:

- `local_`, `lemma5`, `reads`: `Local`, and Lemma 5 (a)(b) and (c), with a fold's reads
  those of `Fold::reads`. `reduce/lemma5.rs::split_faults` certifies Lemma 5 at run time.
  `lemma5ab_of_closed` derives (a)(b) from Definition 3, and `lemma5_of_closed` derives (c)
  given `FoldData` (`reads_of_lemma5`).
- `shape`: an action's guard is its role conjuncts followed by `og`.
- `new_writes`: a `new` parameter comes with its `Create`, which writes its entity's
  existence (`build/actions.rs`).
- `kept_rd` (R, on `X`'s own guards): a conjunct `X` keeps reads only `S_v ∪ inputs`
  (`restrict_to`).
- `cover` (R; the tie to `X` is a count of its guards): every conjunct in `og` of an `S_a`
  action is kept or folded.
- `rise_eff` (R): step (c), a `T_a` action's value on an input depends only on
  `S_v ∪ inputs`.
- `fold_sa`, `folded_og` (R): a fold belongs to an `S_a` action, and its folded guards are
  conjuncts of it.
- `exact`: step (a), `Fold::holds` computes the bit exactly. The bit at slot `c` holds at
  `t` iff some binding with the key at `c` puts each `T` parameter whose role needs a
  present row on one and makes every folded guard true. It is stated for every slot number,
  where the code has bits only for the key entity's slots.
- `folded_names` (R, with `FoldSpec.no_next`): the folded guards name only `T` parameters
  and the key (`fold_of` lets them name at most one `S` parameter, which is then the key).
- `other_names`, `eff_names` (R): no other conjunct and no effect names a `T` parameter but
  the key (`fold_of`'s link guards, and the one `T` parameter an effect may reference).
- `eff_s` (R): a folded action's effect on its writes reads only `S_v ∪ inputs`. For an
  unfolded action, `Local` and Lemma 5 (c) give it.
- `bind`: `bind_stub_params` runs.

Over the slot model of [Theorem A18 over slots](#theorem-a18-over-slots), every field but
`lemma5` and `exact` is derived from the syntax, and `exact` from a bit the syntax defines
([Lemma 6's hypotheses from the syntax](#lemma-6s-hypotheses-from-the-syntax)).

### What the Lemma 6 model leaves out

- **Initial states.** `X`'s initial states are the projections of the cone's. That
  `restrict_to`'s initial states are those projections (Lemma 2) and that every bit starts
  where the cone starts (`fold_initial`) is not derived; every build checks it on the built
  `X` (`reduce/lemma6hyp.rs::lemma6_faults`).
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
  at run time that the built `X` is the one `bind_stub_params` is meant to build, or
  `reduce/lemma6hyp.rs::lemma6_faults`.
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
| `SymAct.Sys.orbit_verdict`, `SymAct.Sys.orbit_accepts_iff`, `SymAct.Sys.orbit_verdict_wfNext` | `SymActSearch.lean` | A18, A23, N9 | Under `WF`, `Built` and `Hyp`, with the `SymOn` of `symOn`: the property holds on the cone system for every binding under `Fair_P` and a stop rule iff the orbit search does not accept, iff N9's search on the cone system accepts for no binding; under `WF_vars(Next)` too. No commutation outside `W` is assumed. |
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

### One model of the actions

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
- `Lemma6Syntax.lean` builds the split's `X` (`Ext`) from the slot model and derives the
  other fields of `Lemma6Hyp` but `lemma5` and `exact`
  ([Lemma 6's hypotheses from the syntax](#lemma-6s-hypotheses-from-the-syntax)).
  `FoldData` and `mono` for Lemma 5 (c)–(e) stay hypotheses.
- `Local` is proved for the slot model as it stands, with the differences from
  `each_successor` that [Theorem A18 over slots](#theorem-a18-over-slots) lists.

## Lemma 6's hypotheses from the syntax

<!-- constrained-by #definition-4-and-lemma-6-over-actions -->
<!-- constrained-by #one-model-of-the-actions -->

`Lemma6Syntax.lean` builds the data `Ext` of Definition 4 from a slot system `SymAct.Sys`, its
lists and a split (`SymAct.Sys.ext`), and derives the fields of `Lemma6Hyp` from the syntax
(`SymAct.Sys.lemma6Hyp`). The model:

- **What the syntax names** (`predPs`, `predNext`, `effPs`): the parameters a predicate's
  terms name (`named_params`), whether a predicate reads the successor (`Pred::reads_next`),
  and the parameters an effect names, its row and the row a column or an owner is set from
  (the touched parameters of `fold_of`). A predicate depends on the binding only at the
  parameters it names, and not on the successor when it reads none. The successor depends on
  the binding only at the parameters the effects name, and on the choices.
- **`fold_of`** (`foldKey`). An action with a `T` parameter folds when every guard that names
  one reads no successor, and either reads `T_v` alone (a folded guard) or is a link
  `v[k] = Index(p)`, with `v` in `S_v`, `k` not a `T` parameter and `p` one (`linkOf`). The
  key is the first `T` parameter an effect or a link names, else the `S` parameter the folded
  guards name, else the first `New` or `Existing` parameter. No effect or link names a `T`
  parameter but the key, no folded guard names an `S` parameter but the key, and the key is
  not `New` (`foldOk`). These are the code's tests: a touched or linked parameter is a `T`
  one and an `S` key is not, so "each one is the key" is "at most one of each, and not a
  link and an `S` key together".
- **The folds the split keeps** (`keyOf`): an `S_a` action's fold that `fold_initial` gives
  a start (`Cut.sel`, left open, as initial states are not modelled) and whose key
  `restrict_to` keeps (`Fold::key_kept`, `keepsP`).
- **A fold's conjuncts** (`foldedC`): the slot bounds of the `T` parameters and of the key,
  and the folded guards. The key's bound makes the bit false past the key's slots.
- **The kept conjuncts** (`keptOg`): the conjuncts of `og` that read only `S_v ∪ inputs` and
  name no parameter the restriction drops. `keepsP` is `keeps_param` with havoc off: a `New`
  parameter is kept when its existence is, any other when its entity's existence is or the
  entity is a stub, the parent of a kept owner (`stubE`). The picks conjunct stands for the
  choice effects and names what the effects name.
- **The raw reads** (`rawRd`, `raw_reads`): an unfolded action's reads, and a folded action's
  reads outside its `T` parameters' existences and its folded guards.

`SymAct.Sys.Inputs` states what the split and its inputs give besides Lemma 5 (a)(b):

- `col_side`: a column of an entity whose existence is in `T_v` is in `T_v`. This is the side
  rule of Definition 3: `split_faults` checks it, and `Closed` gives it (`inputs_of_closed`).
- `inputs_col`: the existence of a column's entity is an input when the column is.
  `split_with` adds the existences of every input.
- `raw`: the raw reads of an `S_a` action are in `S_v ∪ inputs`. `split_with` takes the raw
  reads of `T_v` as inputs, and `split_faults` checks them. Lemma 5 (c) with the folds' reads
  follows (`rd_split`).

`split_faults` checks `col_side` and `raw` at run time; the certificate that `lemma5-check`
reads holds neither, and `inputs_col` holds by the construction of the inputs alone.

| Lean (`RdraIsh.`) | File | Claim | Statement |
|---|---|---|---|
| `SymAct.predPs`, `SymAct.predNext`, `SymAct.effPs`, `SymAct.Sys.eval_bind`, `SymAct.Sys.eval_succ`, `SymAct.Sys.post_bind`, `SymAct.Sys.picks_bind` | `Lemma6Syntax.lean` | none | What a predicate and an effect name, and whether a predicate reads the successor. A predicate's value depends on the binding only at what it names, and not on the successor when it reads none; the successor and the picks depend on the binding only at what the effects name and on the choices. |
| `SymAct.linkOf`, `SymAct.linkOf_spec`, `SymAct.Sys.foldKey`, `SymAct.Sys.foldOk`, `SymAct.Sys.FoldSpec`, `SymAct.Sys.foldKey_spec` | `Lemma6Syntax.lean` | Lemma 6, "Parameters" | `fold_of` on the syntax. A fold it finds has no guard naming a `T` parameter that reads the successor, no folded guard naming an `S` parameter but the key, every other guard naming a `T` parameter a link of the key, no effect naming a `T` parameter but the key, and a key that is not `New`. |
| `SymAct.Cut`, `SymAct.Sys.stubE`, `SymAct.Sys.keepsP`, `SymAct.Sys.keyOf`, `SymAct.Sys.foldedC`, `SymAct.Sys.keptOg`, `SymAct.Sys.rawRd`, `SymAct.Sys.ext` | `Lemma6Syntax.lean` | Definition 4 | The data of `Extended::build` from the syntax and a split: the stubs, the parameters `restrict_to` keeps, the folds the split keeps, each fold's conjuncts, the kept conjuncts and the raw reads. |
| `SymAct.Sys.Inputs`, `SymAct.Sys.inputs_of_closed` | `Lemma6Syntax.lean` | Lemma 5 (c), Definition 3 | The side rule for columns and the code's inputs. A split closed under Definition 3 has the side rule. |
| `SymAct.Sys.unfolded_keeps`, `SymAct.Sys.rd_split`, `SymAct.Sys.og_cover`, `SymAct.Sys.keptOg_rd`, `SymAct.Sys.foldedC_og`, `SymAct.Sys.foldedC_names`, `SymAct.Sys.og_names`, `SymAct.Sys.eff_bind`, `SymAct.Sys.eff_side`, `SymAct.Sys.eff_inputs` | `Lemma6Syntax.lean` | Lemma 6, "Parameters" | An unfolded `S_a` action drops no parameter: a role that reads its row reads it raw, a `New` row is created in `S_v`, and an `Earlier` row's mark (`Built.earlier_mark`) puts its entity in `S_v` or the inputs. Then `reads`, `cover`, `kept_rd`, `folded_og`, `folded_names`, `other_names`, `eff_names`, `eff_s` and `rise_eff` of `Lemma6Hyp` hold of `ext`. |
| `SymAct.Sys.lemma6Hyp`, `SymAct.Sys.lemma6_of_syntax`, `SymAct.Sys.a8_of_syntax` | `Lemma6Syntax.lean` | Lemma 6, A8 | Under `Built`, Lemma 5 (a)(b), `Inputs` and step (a), `Lemma6Hyp` holds of `ext`. So do Lemma 6 with the code's stages and Theorem A8 with `recanon`, given (B1)–(B3) and the rise table. |
| `Act.Ext.Exact`, `SymAct.Sys.holdsB`, `SymAct.Sys.exact_holdsB` | `Lemma6Syntax.lean` | Lemma 6, step (a) | Step (a) for the bit `holdsB`, one per action and slot below a bound `B` on every class's slots: some binding with the key at the slot meets the fold's conditions at the `T_v` valuation. |
| `ExampleLemma6Syntax.hyp`, `lemma6`, `note_key`, `mark_unfolded`, `open_shop` | `Lemma6SyntaxExample.lean` | none | Desks, shops and orders: a folded `S_a` action keyed by an `S` parameter with a free `T` parameter on a stub, an unfolded `S_a` action whose read makes the desks an input, and a rise with a `Parent` parameter on the stub, which `X` rebinds. `Lemma6Hyp` and Lemma 6 follow from `lemma6Hyp` and `exact_holdsB`, with only `Built`, Lemma 5 (a)(b) and `Inputs` checked on the instance. |

What stays a hypothesis: `lemma5`, Lemma 5 (a)(b), which Definition 3 gives
(`lemma5ab_of_closed`) and `split_faults` checks; `Inputs`; and step (a), which
`exact_holdsB` proves for `holdsB`, so what is read from the code is that `Fold::holds` is
`holdsB`. `Built` is what the builder gives. `bind` holds as `ext` runs `bind_stub_params`,
`keyOf` keeps only a fold whose key `restrict_to` keeps, and `StubCheck` is not read. `WF` is
not used.

What the model of the folds leaves out:

- An action with no `T` parameter does not fold: node folds (`node_fold_of`) are not
  modelled, nor groups of folds that share a bit.
- The slot model has no copy guards (`copy_target`), which `fold_of` refuses to fold.
- Which folds `fold_initial` keeps is left open (`Cut.sel`).
- A `Triggered` parameter's existence counts as read, and as a raw read of an unfolded
  action.

## The partition certificate and monotone variables

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

### What these hypotheses are

- **`Sweep.Complete`**: the run is `X`'s reachable graph, numbered, with its silent edges
  and rises exactly, and a label fixes `obs` and the input bytes. That is the flat case, a
  run of `X` with no inner reduction and not up to `H`; otherwise the check reads the stored
  graph.
- **The marking.** `Sweep.Marked` is the least set closed under the sweep's two rules. That
  the depth-first marking of `partition/reference.rs::stuttering_blocks` computes it, and that `partition/reference.rs::same_partition` of
  the sweep's result holds iff the sweep split nothing (a split adds a block, and a sweep
  that splits nothing never changes the blocks its splitters see), are read from the code.
  The fallback partition is also checked by `partition/reference.rs::marking_splits_nothing`,
  which needs neither reading: it meets the hypotheses of `Sweep.b2_of_marking` (a marking
  inside `Marked`, closed under its rules, all or none of each block).
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
