# Instance symmetry: exploring cones up to a permutation of rows

<!-- derived-from ./event-order.md#tlc-on-the-view-cones-and-instance-symmetry -->
<!-- constrained-by ../engine-agreement.md#agreement-rules -->
<!-- constrained-by ../formal-verification.md#reduction-by-an-independent-sub-lifecycle -->

Status: design, not built. It answers
[#212](https://github.com/manji-0/rdra-ish-dsl/issues/212) after its measurement
([comment](https://github.com/manji-0/rdra-ish-dsl/issues/212#issuecomment-7130227315)) and
[#284](https://github.com/manji-0/rdra-ish-dsl/issues/284)'s analysis of why the #74 reduction
is lost at 3 instances
([comment](https://github.com/manji-0/rdra-ish-dsl/issues/284#issuecomment-7132476872)).
The owner answered its open questions on 2026-11-24; [Decisions](#decisions) records them.

At `--instance-count 3`, many cones of the domain samples do not fit in 8 GiB. Most of
their states differ only in which slot holds which row. This document plans a BFS that
stores one state per class of states that a permutation of rows maps onto each other,
reports the same verdicts, and prints counterexamples that replay on the model.

## Why

`docs/design/event-order.md` rejected symmetry at 2 instances: the factor there is at most
2^k and #74 does far better where it applies. #212 measured 3 instances on `origin/main` at
`450558e2` (labels as in the issue: **measured** is counted, **estimated** is extrapolated).

| Group | Cone | `verify` today at n=3 | Orbits at n=3 |
|---|---|---|---|
| A: #74 finds nothing to fold, even at n=2 | clinic-ops `Inv_Appointment_0`, `_1` | stopped at 25.3M states (8 GiB) | 240,016, complete (measured) |
| A | clinic-ops `Unique_Encounter_Appointment` | stopped at 25.9M | 240,016, complete (measured) |
| A | clinic-ops `Inv_Encounter_2` | stopped at 27.0M | 1,696,486, complete (measured) |
| A | insurance-claim-ops `Forbidden_ClaimCase_3` | not reached; over 45M | 682,640, complete (measured) |
| B: #74 reduced at n=2, lost at n=3 | clinic-ops `Forbidden_ClinicalOrder_1` | stopped at 27.9M | #284: S has 61,849,224 states and 20 classes; the reduced cone has 5,050 states (measured with `--memory 32GiB`) |
| B | insurance-claim-ops `Forbidden_Claim_4` | not reached | over 41.1M orbits of the unreduced cone (measured) |
| reduced at n=3 | patent-ops `Forbidden_OfficeAction_3` and 3 more | stopped at 33–41M | 15–22× on its complete reduced cones (measured), counted with the S side permuted too; with S fixed, not known |

Three facts shape the design.

- Group A has no other lever. Symmetry decides five cone lines that nothing decides today.
- Group B's clinic-ops cone is lost because S, the sub-lifecycle #74 quotients, does not fit:
  S is the group-A cone `Inv_Appointment_0` (#284). So the run of S has to go up to symmetry,
  not only whole cones.
- The prototype canonicalizer was the bottleneck: 4 to 40 µs per state on one thread, and
  70 to 190 thousand successors per second on 8 threads on the 11-entity clinic-ops cones
  (measured, #212 §5). The plain run of the same S made 268M edges in 35 s (#284). The cost
  of the canonical form decides where it runs and what phase 1 must show.

## Goals and non-goals

<!-- derived-from #why -->

Goals:

- Decide the group-A cones at n=3 under `--memory 8GiB` with the verdicts and violated-check
  sets an unreduced run gives with enough memory.
- Print counterexamples that name concrete slots, replay through the step relation, and pass
  every check a counterexample passes today ([Agreement rules](../engine-agreement.md#agreement-rules),
  rule 3).
- Run S up to symmetry inside #74, so the group-B cones whose S is a group-A cone are
  reduced again.
- A cone `verify` decides today prints what it prints today. Only a cone that stops at the
  memory limit is run again up to symmetry ([Decisions](#decisions)).

Non-goals:

- Symmetry at n≤2 as a default. The factor there is 2–8 and #74 wins where it applies.
- TLC `SYMMETRY` sets (see [TLC and engine agreement](#tlc-and-engine-agreement)).
- A cheaper `--prove`. z3 reads no state space.
- `check` and `states`. They draw and list concrete states; they stay unreduced.

## The group

<!-- derived-from #why -->

**Classes.** The group permutes the slots of each entity. An entity's vars (existence,
columns, propositions, owners, run marks, raised marks) move with its slots. Entities form
*classes* that permute together:

- An owner var that starts at its own slot (`Start::Identity`, a pre-existing 1:1 child or a
  child of a `1..*` key) couples the child's entity and the parent's into one class. The
  builder chose the identity as one start out of all its permutations
  (`build/vars.rs`, `owner_vars`), so the starts are closed only under the coupled
  permutation. The two entities have the same slot count, or the build already fails with
  `IdentityStartSlots`.
- Any other owner couples nothing. Its value on a *present* child row is relabelled by the
  parent's permutation. On an absent or deleted row it holds the base value, which names no
  row and is not relabelled. #212's harness relabelled it at first, which was a bug.
- `now` belongs to no class and stays.

**The group of a state fixes absent slots.** For a state `s`, `G_s` is the product, over
classes, of the permutations of the slots that are not absent in `s` (for a class of two
entities, not absent in either). Absent slots stay where they are. The successor function
is the one BFS has today, `FirstAbsent` included.

Why this is sound. Let `s ~ s'` when `s' = g(s)` for some `g` in `G_s`. Then `G_s = G_s'`,
so `~` is an equivalence. It is a bisimulation: if `s -> t` under an action and params, then
`g(s) -> g(t)` under the same action and the params mapped by `g`. A create takes the lowest
absent slot `k`; `g` fixes every absent slot of `s`, so `k` is also the lowest absent slot of
`g(s)`, and `g` fixes it. A guard reads row ids only by `=` and `≠` against owners or other
row ids (`Term::Index`; the builder emits nothing else: `build/links.rs`, `build/sets.rs`,
`build/events.rs`, `build/preds/`), and both are kept by a bijection. `Forall`, `Exists`
and `Count` range over all present rows. Since `t` has no absent slot `s` lacks, `g` is also
in `G_t`, so `t ~ g(t)`. Exploring one representative per `~`-class therefore reaches
exactly the classes of the reachable states.

A representative need not be reachable. In `presence_first_absent_asymmetry` at n=2,
"Order#1 done, Order#2 rush" is reachable, and its swap "Order#1 rush, Order#2 done" is in
its class but no run reaches it: the first order is never a rush order. #212 counted 4 of
the 41 reachable states whose swap no run reaches, and any of those swaps can be the stored
representative. That does not touch
soundness, since every check in phase 1 has the same value on every member of a class. It
does mean nothing may print a stored state as it is; output goes through
[Counterexamples](#counterexamples).

`G_s` against #212's group. #212 permuted every slot, absent ones too. On the reachable
states of a created entity the absent slots are a suffix (creation takes the lowest absent
slot and a deleted slot is never reused), and every absent row is at base and named by no
present row (a pre-existing child of a created parent is optional and starts unlinked, or
the build fails with `PreExistingChildOfCreatedParent`). So two reachable states in one
orbit of #212's group are also one `~`-class, and #212's orbit counts are the counts this
design must reproduce. Phase 1 checks that on the group-A cones.

**What breaks the group, and what happens then.** A scan over the cone system decides, per
class, whether it joins. It reads every guard, effect, invariant, after-assert, property,
premise, and `can_reach` goal. A class is left out (its slots fixed, its children still
permuted among themselves) when any of these touches one of its entities:

1. `Term::Index`, or an owner var of its domain, under `<`, `≤`, `>`, `≥`, or in an `In`
   list. `compare` (`expr.rs`) sends those down the numeric path and orders row ids.
2. `Term::Index` or such an owner compared with `Term::Lit` or `Term::Int`, other than an
   optional owner against `null`.
3. `Rhs::Lit` assigning an owner a value other than `null`.
4. `Effect::Choose` or `Effect::Transition` on an owner var whose values are not the whole
   domain.
5. A start that pins a slot: `Start::Identity` on a pair of unequal slot counts (an error
   today; checked anyway), or any start the scan cannot classify.

The scan never refuses a model. Leaving a class out keeps every run sound and only loses
reduction. A run up to symmetry prints one note per cone that leaves a class out, naming the
entity and the rule. Today the builder emits none of the five (its `Choose` and `Transition` effects write
columns and marks, and it writes an owner only from a param's slot or `null`), so no sample
and no corpus model loses a class; the scan guards against a later builder change. Because the DSL cannot
name a slot, the trap #212 called `sym_index_named` cannot be a corpus case. It is a unit
test on a hand-built `TransitionSystem` with a `Term::Index` under `<`.

One shape the scan cannot see: a guard that reads the owner of a deleted row. Such a row
holds the base value 0, which a non-optional owner reads as slot 0, and the relabelling
skips it. The `Earlier` role binds rows by run mark, not by existence, so this needs a split
use case that deletes a row in one step and reads its key in a later one. The bisimulation
check of the [shadow twin](#verifying-the-reduction) catches it, and a corpus case
(`sym_deleted_row_owner`) pins it.

**Entities of a #74 reduction.** In a reduced system the state holds a class representative
for the S side. Permuting an S entity's slots does not map a representative to a
representative, and the class tables key on the inputs (T vars S reads) and the fold bits
slot by slot. So on a reduced system the group leaves out every S entity, every T entity
that holds an input var or a fold-bit key, and every T entity that is the parent of an S
entity. [Composition with #74](#composition-with-74) uses this rule.

## The canonical form

<!-- constrained-by #the-group -->

**Definition.** Lay the state out class by class, parents before children (cycles of owner
references, such as an entity that owns itself, form one block), each class row by row in
slot order. The canonical form of `s` is the least such key over `G_s`, where an owner value
in a child's row is written as the parent row's position in the parent's canonical order.
It is exact: two states have the same canonical form if and only if they are in one class.
Exactness keeps the stored-state count a property of the model, not of the
canonicalizer, which the [shadow twin](#verifying-the-reduction) relies on.

**Algorithm.** For each block in order:

1. Give each movable slot a signature: its row's bytes, with owner values replaced by the
   parent's canonical position (parents are already placed). Sort the movable slots by
   signature. With no ties, that order is the block's part of the least key.
2. Tied slots that no present child row names are automorphisms: any order gives the same
   key. Take them in slot order and do not branch.
3. Branch over the other ties, carrying each branch to the later blocks that read those
   slots, and drop branches that agree on every slot a later block reads. This is #212's
   prototype with its 7× deduplication, preceded by steps 1 and 2. Ordering ties by their
   children's signatures instead would be cheaper but need not give the least key, so it
   may only prune branches it proves equal, never pick one.
4. Past `BRANCH_CAP` branches (4,096 in the prototype, never reached in #212), keep the
   first branch. The result is still a member of the class, so the run stays sound, but two
   members may then be stored twice. The cone keeps its verdict (owner decision,
   [Decisions](#decisions)): the verdict is sound, and only the stored count may then depend
   on the canonicalizer. The run counts the states stored this way and reports them in a
   note under the cone line, `note: N states stored without their canonical form (branch
   cap); the orbit count is an upper bound`, and in the `RDRA_ISH_SHADOW_STATS` line.

**Fast path.** A move writes one to three rows of a state that is already canonical. When
the written rows tie with no other row and no owner value moves, re-sorting those rows alone
gives the canonical form. The full canonicalizer runs otherwise. The shadow twin compares
the two on every call.

**Where it runs.** In the workers, not in `Bfs::admit`. `expand` (`explore/bfs.rs`) already
computes each chunk's moves in parallel and the reader takes them in order, so the run does
not depend on the thread count. Each move in `Moves` gains its canonical bytes beside its
concrete successor. The reader then, in `take`:

1. runs `observe` and `check_after` on the concrete triple (representative, params,
   successor), as today: the view patterns and the after-asserts read the slots the params
   name in that successor;
2. treats a concrete self-loop (successor equal to the state) as today;
3. looks the *canonical* bytes up in `seen`, and admits them when new. `check_invariants`
   runs on the canonical state, which is equivalent because every invariant reads all rows.

Initial states are canonicalized and deduplicated before they are admitted. The initial set
is closed under `G_s` (pre-existing `Start::Any` columns take every value, `Start::Identity`
pairs move together, created entities start absent), so each stored initial state is an
initial state.

**Orbit self-edges.** A move can change the state and land in its own class: a child
relinked from one parent to an identical one. It is stored with `to == from`. Liveness,
`breaking_step`, `Adjacency`, and the divergence pass of #74 (`divergence` in
`reduce/partition.rs` skips `x == y`) read `to == from` as a stutter, but this one is a real
step: `s -> h(s) -> h²(s) -> …` returns to `s` after at most the order of `h` steps, so the
concrete system has a cycle there. From phase 1 the run records the index of every orbit
self-edge in a list charged to the memory pool. Phase 2's divergence pass reads it (a silent
orbit self-edge makes its class loop), and phase 4 replaces it with the edge's permutation,
which is the identity exactly on concrete self-loops.

**FirstAbsent.** Nothing special: `G_s` never moves an absent slot, so the successor function
stays as it is and the lowest absent slot of a representative is the lowest absent slot of
every member.

## Soundness per check

<!-- constrained-by #the-group -->
<!-- constrained-by #the-canonical-form -->

A reader of a run is sound on the reduced graph when it reads in one of two ways, and not
otherwise. The property's name does not decide it.

| How the reader reads the run | Readers | On a reduced run |
|---|---|---|
| One state, quantified over every binding | `check_invariants` (every `InvKind`, `Unique`, `MinCard`, `MaxCard`, `DanglingKey`, `Overflow` included); `unsafe_state` (`always`); `meets` in the vacuity and unmet-premise checks; `growth`; the state and layer counts | sound as is |
| The concrete (state, params, successor) triple inside `take` | `check_after`; `observe` (the entity views' patterns, `initial`, `changes`, `deletable`, `moves`) | sound as is |
| Which actions label some edge, and state predicates of their targets | `runs` (`NeverRuns`), `unmet_of_assert` (`meets` on the targets of an action's edges) | sound as is: an action labels an edge of the reduced graph exactly when it labels one of the full graph, and `meets` reads all rows |
| Action labels, graph structure, and state labels the group keeps, on a run of S up to `H` | `split_edges`, `quotient_blocks`, the class moves and rises, `Succ` and `shortest_path` in `lift`; `divergence` with the orbit self-edges | sound: `H` fixes `obs` and the inputs ([Composition with #74](#composition-with-74)); `divergence` needs the orbit self-edges, since it reads `to == from` as a stutter |
| Two stored states compared slot by slot across an edge | `can_leave` and `deadlocks` (`Deadlock_*`), the `moves` closure of `guarded_arms`, `step_breaks` (`stable`, `unchanged`) | not sound: the edge's target is stored permuted |
| One binding followed along a path | `reach_failure` (`can_reach`), `unlatched_state` (`precedes`), `fair_steps` and the fair components (`eventually`, `leads_to`, `.fair`), the per-slot deadlock pass | not sound: the binding's slot moves with each edge's permutation |

**Phase 1 covers the first three rows**: the invariant and after-assert cones (the
`Cone_` jobs of `cone_jobs`) and `always` properties, on cones #74 leaves unreduced. That is
every group-A line. Entity cones (their state-model findings read slot by slot), `stable`,
`unchanged`, `precedes`, `can_reach`, and every liveness property run unreduced as today.

To keep a later reader from reading a reduced run by mistake, a symmetric run is a type of
its own (`OrbitRun`), not a `ComponentRun` with a flag. It exposes the diagnostics, the
lifted traces, the counts, the stored states for state predicates, and a label-only edge view
(source, target, action, params from the source, and whether the edge is an orbit self-edge)
that the readers of the fourth row use. It has no `graph()` or `adjacency()`, so a reader of
the last two rows does not compile against it.

**Later phases** make the last two rows sound with an *annotated quotient*: each edge keeps
the permutation `σ` that took its concrete successor to the stored one. A slot-wise
comparison across an edge maps the target's slots back through `σ`. A per-binding analysis
runs on pairs (stored state, binding), and an edge moves the binding by `σ`; the pairs
reachable are at most the stored states times the bindings, and in practice far fewer than
the concrete states. Fairness is per binding already (`keys_fair_instance`), so the fair
step of an instance is the fair step of its tracked binding. A cycle of the annotated graph
whose permutations compose to `π` is a concrete path from `c` to `π(c)`; the concrete lasso
repeats it until `π` returns to the identity, at most the order of `π` times (6 for one
class of 3 slots). `presence_first_absent_asymmetry` must then still report
`RushFinishes` on `Order#2` only.

## Counterexamples

<!-- constrained-by #the-canonical-form -->

The stored path to a violation runs through representatives. The concrete trace is rebuilt
forward, so nothing per state is stored:

1. Start at `c_0 = r_0`, a stored initial state, which is an initial state. Let `τ_0` be the
   identity.
2. For each stored edge `r_i -> r_{i+1}` with action `a` and params `p`, recompute the moves
   of `a` under `p` from `r_i`, take the first whose successor `t` canonicalizes to
   `r_{i+1}`, and keep the permutation `σ_i` with `σ_i(t) = r_{i+1}`. The concrete step is
   `a` under `τ_i(p)` from `c_i` into `c_{i+1} = τ_i(t)`, and `τ_{i+1} = τ_i ∘ σ_i⁻¹`.
3. An after-assert's last step (`Reach::Step`) is the concrete successor `take` saw, mapped
   by `τ`.

Each `σ_i` fixes the absent slots of `t`, so by induction every `τ_i` fixes the absent slots
of `r_i`: a create in the trace takes the lowest absent slot of `c_i`, and the trace replays
through `replay` (`trace.rs`) under `FirstAbsent`. Step 2 needs only a deterministic
representative, so a state cut at `BRANCH_CAP` lifts too.

The diagnostic is rebuilt on the lifted last state, at the place `run_on` already rebuilds it
for a #74-reduced run (`invariant_violation`, `after_violation`). `find_violation` then names
the binding the concrete state violates, in concrete slots.

Order with #74: the symmetry was applied last, to the system BFS explored, so it is lifted
first. The lifted path, still holding class representatives on the S side, then goes to
`lift` (`reduce/lift.rs`), which lifts the class moves as today. Inside it, an S that was run
up to symmetry lifts its own path the same way before its inner reduction does.

The counterexample is a shortest one: the bisimulation keeps distances, so BFS over classes
reaches each class at the least depth of its members. It may name other slots, and for a
check with several violating bindings another binding, than an unreduced run.
[Agreement rule 1](../engine-agreement.md#agreement-rules) compares verdicts and the set of
violated checks, not slots, so this is no disagreement.

## Composition with #74

<!-- constrained-by #the-group -->
<!-- constrained-by ../formal-verification.md#reduction-by-an-independent-sub-lifecycle -->

#74's classes are not unions of orbits under the whole group, because the quotient keys on
`obs` and the inputs slot by slot: two states one swap apart may differ in what T observes.
The design therefore runs S up to a subgroup that keeps every label.

**S up to `H`.** `Classes::of` (`reduce/partition.rs`) runs the extended sub-lifecycle and
partitions its graph. Run it up to `H`, the group of [The group](#the-group) on the extended
system with these classes left out as well: every entity that holds an `obs` var, an input
var, or a fold bit, every stub of a T parent, and every entity whose rows a T owner can name.
Every `h` in `H` then fixes `obs` and the inputs, so the `~`-classes of `H` are a
label-preserving bisimulation, finer than the coarsest stuttering partition #74 computes.
Every class of #74 is then a union of `H`-classes, and `quotient_blocks` on the reduced graph
of S yields today's classes.

What changes in `reduce/`:

- `Classes::of`: the run of S is an `OrbitRun` under `H`. A class's representative is the
  first stored state of its block, which may be an unreachable S state. It agrees with every
  member of its class on `obs` and the inputs, which is all the reduced cone reads.
- `init`: an initial S state is canonicalized under `H` before its position is looked up.
- Class moves: an edge's params are those of a concrete move from a stored state of the
  class, so they stay valid. Rises: keyed by the inputs of the target, which `H` keeps.
- `divergence`: a silent orbit self-edge marks its class as one that can loop, since it
  stands for a concrete cycle inside the class ([The canonical form](#the-canonical-form)).
  Without this, `loops` would miss the cycle and a reduced cone's `_WF` verdict could be a
  wrong `ok`. The `Classes::of` twin compares `loops` too.
- `lift`: it reruns S with `Caps::new(usize::MAX, usize::MAX)` and no memory charge. Run
  unreduced at n=3 that is 62M states, so `lift` must run S up to `H` as `Classes::of` did,
  find the path on that graph, and lift it with [Counterexamples](#counterexamples) before
  the inner lift.
- `QuotientCache`: the group is part of the key, so a cached quotient from a plain run is
  never served to a symmetric one, or the reverse.

The classes are the same sets, but their numbering follows discovery order and the
representatives are other states. The printed TLA+ module (`Rep_` tables) and the lifted
traces therefore change. Applied at n≤2, this would move outputs that are stable today,
which #284 forbids. That is why S runs up to `H` only when the plain run of S stops at the
memory limit ([Decisions](#decisions)), which no sample's S does at n≤2.

Expected gain (estimated). In clinic-ops `Forbidden_ClinicalOrder_1`, `obs` is
`Encounter._exists`, so `H` leaves out `Encounter`. Each orbit of #212's group splits into at
most 3! = 6 classes of `H`, so S has at most 6 × 240,016 ≈ 1.44M stored states, against
61.8M today, provided `Encounter` is the only class `H` leaves out (not checked). The partition then yields #284's 20 classes and a reduced cone of 5,050 states.
The two other clinic-ops group-B lines (`Inv_Prescription_4`,
`Unique_FollowUpMessage_CarePlan`) are probably the same shape; phase 2 measures them. For
insurance-claim-ops `Forbidden_Claim_4`, `H` leaves out the case's side that holds the fold
bits; the size of S under `H` is not known. Its classes at n=3 would very likely pass
today's `TLA_CELL_BUDGET`; #284 decided to raise the budget for the native checker and TLC
alike, which is done there and not here.

**T up to `G_T` on a reduced cone.** patent-ops' four undecided cones are reduced cones. On
them the group of [The group](#the-group) acts on T entities only, with the exclusions of
its last paragraph, and leaves the class representative alone. #212's 15–22× on patent-ops
was counted with the S side permuted too, so the gain here is not known until phase 3
measures it.

**Order.** Build S's quotient up to `H`, reduce the cone, then explore the reduced cone up to
`G_T`. Lift in the reverse order.

## TLC and engine agreement

<!-- constrained-by ../engine-agreement.md#agreement-rules -->

**No `SYMMETRY` in TLC's configuration.** TLC permutes model values of a set the
configuration names and does not check that the spec is symmetric under them. The printed
specs model slots as integers (`<E>_Ids == 1..InstanceCount`), and a create takes
`FirstAbsent`, which orders them (`j < i`, `crates/rdra-ish-emit/src/tla/module.rs`); no
permutation of all slots keeps that, and slots are not model values either; a `SYMMETRY` set would make TLC unsound on exactly the
`FirstAbsent` asymmetry #212 found. TLC's documentation also warns that liveness checking
under symmetry can miss violations. TLC keeps exploring the cone it is given, unreduced or
#74-reduced, by its own algorithm. That makes it an independent second opinion for the
native symmetric run, not a copy of it.

**Comparable cones.** `cone_jobs` prints the same module with or without symmetry, apart
from the class numbering and `Rep_` bytes of a cone whose S ran up to `H`. Rule 1 compares
the verdict and the set of violated checks; rule 3 requires the lifted counterexample to
replay. Neither reads a state count. On a cone TLC cannot finish at n=3, TLC gives no
verdict, which rule 1 already treats as no disagreement. There the second opinions are the
in-process unreduced twin up to its cap ([Verifying the reduction](#verifying-the-reduction)),
z3 under `--prove` for state rules (rules 4 and 5), and the verdicts the corpus headers state.

**Changes to `docs/engine-agreement.md`**, made in phase 1:

- A rule 8 under "The native checker against itself": on every corpus case at 1, 2, and 3
  instances, a run with `--symmetry always` gives the same verdict and the same violated checks as
  a run without it, and every counterexample it prints replays. The harness runs each case
  both ways at each `bfs=` bound; the header grammar needs no new field.
- A `sym_` family in the corpus, with the cases listed below.
- The planted bugs below, in "What the corpus catches".
- No TLC rule changes.

## Verifying the reduction

<!-- constrained-by #soundness-per-check -->
<!-- constrained-by ../engine-agreement.md#kani-harnesses -->

The rule of [A speed-up change](../../CLAUDE.md#a-speed-up-change) (a fast path keeps its
replaced function as a reference twin under the `shadow` feature) is necessary here
but not enough: the reduction changes what a run prints (counts, slots in traces), so no twin
can show an identical output. What each check holds instead:

**Reference twins under `shadow`.**

| Fast path | Twin | Compares |
|---|---|---|
| `canonical` | `symmetry/reference.rs`: the least key over every element of `G_s`, enumerated, when `G_s` has at most 6^4 elements | the canonical bytes, on every call |
| the fast path for a re-sorted row | the full canonicalizer | the canonical bytes, on every call |
| an `OrbitRun` of a cone | the plain run of the same cone, when it stays under `SHADOW_CAP` states | (1) the set of canonical forms of the plain run's states equals the stored states; (2) for every plain state `s`, the canonical forms of the successors of `s` equal those of the successors of its representative, which checks the bisimulation itself; (3) the verdicts and violated-check names are equal; (4) in later phases, the view pattern sets and the per-binding verdicts are equal |
| `Classes::of` up to `H` | `Classes::of` on the plain run of S, under `SHADOW_CAP` | the partitions as sets of concrete states, the class moves and rises mapped through them, and `loops` |

Each twin counts its comparisons, so `RDRA_ISH_SHADOW_STATS=1` shows that it ran. Check (2) is
the one that catches a wrong group. A closure check on the reached set cannot, since the set
is not closed under `G_s` (see [The group](#the-group)).

**Exhaustive small scope.** A unit test enumerates every state of small layouts (one class of
3 slots with a 3-valued column; a parent and an owner child; a `Start::Identity` pair; a
self-owning entity) and every element of `G_s`, and checks that `canonical(g(s)) =
canonical(s)`, that `canonical(s)` is some `g(s)`, and that `canonical` is idempotent. It
runs in `cargo test` in seconds.

**Kani.** One harness, following [Kani harnesses](../engine-agreement.md#kani-harnesses):
`apply(g ∘ h) = apply(g) ∘ apply(h)` and `canonical(apply(g, s)) = canonical(s)` for symbolic
`s` and `g`, at one class of 3 slots and one child class. It names the file it proves in
`scripts/kani-relevant.sh`. A planted bug must fail it before it lands.

**Corpus cases (`sym_` family).**

| Case | Trap |
|---|---|
| `sym_owner_coupled` | A rule that holds only if a child's owner is relabelled with its parent; a canonicalizer that permutes children alone merges states that differ |
| `sym_identity_pair` | A `Start::Identity` pair; permuting child and parent apart reaches starts the model does not have |
| `sym_deleted_row_owner` | A deleted row holds base owner 0; relabelling it makes a deleted row look linked to another parent |
| `sym_master_ties` | Identical pre-existing rows named by children; the tie path must branch on named rows and not on others |
| `sym_self_owned` | An entity that owns itself (a tree); one block, enumerated jointly |
| `sym_first_absent` | A safety violation whose representative differs from the state BFS reaches first; the lifted trace must create at the lowest absent slots to replay |
| `sym_per_entity_bounds` | `--instances` with different counts per entity |
| `presence_first_absent_asymmetry` | (existing) runs unreduced until phase 4, then must still report `Order#2` only |

**Planted bugs**, each reverted, each caught by the named check:

| Planted bug | Caught by |
|---|---|
| Owners relabelled on deleted rows too | `sym_deleted_row_owner`, twin (2) |
| `Start::Identity` pair not coupled | `sym_identity_pair`, twin (1) |
| Absent slots permuted (#212's group) | the replay of `sym_first_absent`'s trace (rule 3) |
| Ties broken in slot order with no branching | the canonicalizer twin, the exhaustive test |
| `obs` entity left in `H` | the `Classes::of` twin, then the reduced cone's verdicts |
| Orbit self-edges dropped from `divergence` | the `Classes::of` twin (`loops`), `cone_reduced_class_loops` run with `--symmetry always` |
| Trace lift applying only the last `σ` | replay (rule 3), `sym_owner_coupled` |
| Scan rule 1 removed, with a hand-built `<` on `Term::Index` | the unit test that stands for `sym_index_named` |

## Memory

<!-- constrained-by #the-canonical-form -->

- Stored states are canonical states of the same width, so `store_bytes` and the charge per
  state do not change. A symmetric run that meets the memory limit stops at the same byte
  count as a plain one, with more classes behind each state.
- No permutation is stored per state; [Counterexamples](#counterexamples) recomputes it.
- `Moves` holds each successor twice, concrete and canonical, for the chunks in flight. It is
  transient and, like today's `Moves`, not charged.
- A worker's canonicalizer scratch is a few kilobytes, plus the branches: at most
  `BRANCH_CAP` × classes × slots bytes (about 135 KB at 11 classes of 3 slots). Permutations
  are generated, not tabled.
- The group of a system is a few bytes per var.
- The `Held` estimates ("finishing would take …") count stored states, so under symmetry they
  speak of classes, and the message says so.
- Later phases add the edge permutation: one byte per tracked class per edge (5! = 120 fits),
  so `Edge` grows from 12 bytes. `store_bytes` gains the per-edge width, and
  `store_bytes_only_grows` and `vec_bytes_is_what_a_vec_of_edges_grows_to` are updated. The
  alternative, recomputing `σ` for every edge on demand, costs a second canonicalization of
  every successor per analysis pass; phase 4 measures both before choosing.

## Interface

<!-- constrained-by #tlc-and-engine-agreement -->

- `verify --symmetry <never|retry|always>`, and `symmetry = "..."` under `[check]` in
  `rdra.toml`. `retry` runs a cone up to symmetry only after its plain run stopped at the
  memory limit ([Decisions](#decisions)); `always` runs every cone the current phase covers
  up to symmetry, for rule 8 and the tests; `never` is today's `verify`. The default is
  `never` until phase 5 makes it `retry`.
- A cone line run up to symmetry shows the stored count as orbits and an estimate of the
  plain state count, `(240,016 orbits, about 62M states)`, and a retried cone says why:
  `retried up to instance symmetry after the memory limit at 25,266,745 states`. The
  estimate is described under [Decisions](#decisions).
- The notes under a cone line: a class the scan left out ([The group](#the-group)), and states
  stored without their canonical form ([The canonical form](#the-canonical-form)).
- `check`, `states`, `export`, and `--prove` do not read the option.

## Phases

<!-- derived-from #soundness-per-check -->
<!-- derived-from #composition-with-74 -->

Each phase lands when its exit check passes. Phases 1 to 4 change no output under the
default `--symmetry never`; phase 5 changes only cones that stop at the memory limit.

**Phase 1: safety cones #74 leaves unreduced.** The group, the scan, the canonicalizer with
its fast path, canonicalization in the workers, `OrbitRun` with its orbit self-edge list,
the trace lift, the orbit-size estimate, `--symmetry always|retry|never`, the
twins, the exhaustive test, the Kani harness, the `sym_` cases, and rule 8. Exit:

- the four group-A cones give #212's counts exactly: 240,016 (`Inv_Appointment_0`, `_1`),
  240,016 (`Unique_Encounter_Appointment`), 1,696,486 (`Inv_Encounter_2`), 682,640
  (`Forbidden_ClaimCase_3`), and their five lines are decided at n=3 under `--memory 8GiB`;
- with `--symmetry always`, the samples at their `// expect` flags and the corpus at n=1, 2, 3 give
  the verdicts they give without it;
- the canonicalizer's cost per successor on `Inv_Encounter_2`, in instructions at
  `RDRA_JOBS=1`, is reported beside #212's prototype; the target is a fifth of it;
- a `shadow` run of the samples and the corpus shows every new twin's count above zero;
- the orbit-size estimate is reported against the exact plain counts of #212's complete
  n=3 cones (§3b, 29,680,875 states on patent-ops `Forbidden_PatentApplication_1` the
  largest).

Expected: five undecided lines of clinic-ops and insurance-claim-ops decided (measured on
the prototype), 16 to 108 times fewer stored states.

**Phase 2: S up to `H`.** `Classes::of`, `init`, `lift`, and `QuotientCache` as in
[Composition with #74](#composition-with-74), with the `Classes::of` twin. As built, S runs
up to `H` under `--symmetry retry` and `always` once its plain run stopped at the memory
limit with the pool to itself (a run another run crowded out is no reason); `never` runs S
as before. No sample's S stops at n≤2, so a test-only switch runs S up to `H` at once:
`Caps::forcing_s_up_to_h` in tests, and `RDRA_ISH_FORCE_S_UP_TO_H` in a `shadow` build,
which CI's twin step sets. A release build has neither. First unit: the
size of S under `H` for the three clinic-ops group-B cones and insurance-claim-ops
`Forbidden_Claim_4`, reported before the code lands. Exit: clinic-ops
`Forbidden_ClinicalOrder_1` reduced and decided at n=3 under 8 GiB with #284's 20 classes; a
lifted counterexample on a seeded violation of that cone replays.

**Phase 3: T up to `G_T` on reduced cones.** First unit: the factor on patent-ops' four
undecided cones with the S side fixed. Exit: those cones decided at n=3, or the measured
factor recorded as why not.

**Phase 4: the annotated quotient.** Edge permutations, the slot-wise and per-binding
readers of [Soundness per check](#soundness-per-check), lasso unrolling, and entity cones.
Exit: rule 8 holds for every check kind, `presence_first_absent_asymmetry` reports
`Order#2` only with `--symmetry always`, and `verify --with-tlc` reports no disagreement on the
samples at their `// expect` flags.

**Phase 5: retry by default.** `--symmetry` defaults to `retry`. It needs only phase 1's
exit, so it may land before phases 2 to 4; each of those then widens which cones a retry
covers. Exit: `verify-samples.sh` at the `// expect` flags prints what it printed before on
every root, and the group-A lines are decided at n=3 under `--memory 8GiB` with no option.

## Decisions

<!-- derived-from #phases -->

The owner decided these on 2026-11-24.

- **The default is a retry.** A cone that `verify` decides today keeps today's output. A
  cone that stops at the memory limit is run again up to symmetry; nothing else is.
  - When: after the plain attempts `verify` already makes. `bfs_cones`
    (`crates/rdra-ish-cli/src/verify/bfs.rs`) reruns a cone alone when it met the limit
    beside others (`crowded`); the retry follows only if that lone rerun also stops at the
    limit, so a cone the lone rerun decides keeps today's output.
  - Memory: the retry runs alone, as the lone rerun does, after the plain run's result and
    its `Charge` are dropped, so it has the whole `--memory` pool, minus what the quotient
    cache holds, as today.
  - Time: safety cones have no time budget today, and the retry gets none either. Its cost
    is the plain attempts' time plus the orbit run; for scale, the runs of S that #284 saw
    stop at the limit at n=3 took 35 and 61 s. A property
    run keeps its `--temporal-budget` deadline, and a retry in a later phase runs within
    what is left of it, not a fresh one.
  - S: the same rule inside `Classes::of`. A run of S that stops at the memory limit is run
    again up to `H` instead of returning `Classified::Nothing`.
  - The cone line says so, as [Interface](#interface) shows.
- **A cut canonical form keeps the verdict.** When `BRANCH_CAP` cuts, the cone's verdict
  stands; it is sound, and only the stored count may then depend on the canonicalizer. The
  count of such states goes to a note under the cone line and to the shadow stats
  ([The canonical form](#the-canonical-form)).
- **The cone line shows orbits and an estimate of the plain count**, as `(240,016 orbits,
  about 62M states)`.
  - How: the size of a stored state's orbit is `|G_r| / |Aut(r)|`. `|G_r|` is the product,
    over classes, of the factorial of the movable slot count. `|Aut(r)|`, the elements of
    `G_r` that leave `r` as it is, is counted once per stored state at `admit`: the factorials
    of the tie groups no child names, times the branches that reach the least key, counted
    with their multiplicity rather than deduplicated. The sizes are summed in a `u128`, and
    the line prints two significant figures.
  - Cost: one more pass of the canonicalizer per *stored* state, where the run already makes
    one per successor; at the 11 edges per state of clinic-ops' S (#284), about a tenth more
    canonicalization work.
  - Label: `about`, and the docs call it an estimate. It is high, not exact: some members of
    an orbit are states no run reaches (the `FirstAbsent` asymmetry of [The group](#the-group)),
    and a state the branch cap cut counts with `|G_r|` alone. Phase 1 reports its error.
- **`TLA_CELL_BUDGET` is decided in #284**: it is raised for the native checker and TLC
  alike, in a change of its own. It is not part of this plan.
