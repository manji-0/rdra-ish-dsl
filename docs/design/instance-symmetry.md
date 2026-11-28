# Instance symmetry: exploring cones up to a permutation of rows

<!-- derived-from ./event-order.md#tlc-on-the-view-cones-and-instance-symmetry -->
<!-- constrained-by ../engine-agreement.md#agreement-rules -->
<!-- constrained-by ../formal-verification.md#reduction-by-an-independent-sub-lifecycle -->

Status (2026-11-26): built in part. `verify --symmetry never|always` ships, and `never` is the
default.

- **Landed.** The design (#292, #340). [Phases](#phases) 1 (#311), 2 (#319) and 3 (#324,
  #328); a deleted row keeps its keys (#317). Step 1 of
  [Always by default](#always-by-default), which removed `retry` (#344). The per-rule
  witnesses and the snapshot (#350, #353). Two units of phase 4: `eventually` and
  `leads_to` (#352), and entity cones (#370), after a stopped entity cone gives back its
  room (#361).
- **Open.** The rest of phase 4: `stable`, `unchanged`, `precedes`, `can_reach`, and an
  `always` property on a reduced cone still run plain. The flip of the default to `always`,
  a `### Breaking` entry whose release is the owner's call. `PinnedStart` has a witness
  since #512 (claim A20 of [the claims inventory](../theory/claims.md)). #212 and #284 stay open.
- **Superseded.** The `retry` default of [Decisions](#decisions), by
  [Always by default](#always-by-default) (2026-11-25). The cone line with an orbit count,
  by `(about <m> states)`.

The note answers
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
| reduced at n=3 | patent-ops `Forbidden_OfficeAction_3` and 3 more | stopped at 33–41M | 15–22× on its complete reduced cones (measured), counted with the S side permuted too; with S fixed, 3.0–5.5× (phase 3, measured) |

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
- Any other owner couples nothing. Its value on a present or deleted child row is
  relabelled by the parent's permutation: a deleted row keeps the key it had (the owner's
  decision after phase 1; see below). On an absent row it holds the base value, which names
  no row and is not relabelled.
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

These are the first five of the ten rules the code has (`symmetry/mod.rs::Rule`; claim A20 of
[the claims inventory](../theory/claims.md)). The later phases added
five more:

6. `UnlinkedCreate`: a creation leaves an owner naming the class's rows at its base value,
   which is slot 0.
7. `MixedIds`: a row id is compared with, or written into, a value that is not a row id of
   the same class.
8. `JointTooLarge`: a block of classes that name each other's rows has more arrangements
   than the canonicalizer enumerates.
9. `Observed`: on a run of a sub-lifecycle up to `H` (phase 2), an entity that holds `obs`,
   an input or a fold bit, a stub of a `T` parent, or whose rows a `T` key can name.
10. `Reduced`: on a #74-reduced cone up to `G_T` (phase 3), an `S` entity, an entity that
    holds an `S` var or an input or whose rows such a key names, an entity a bit is kept per
    row of, and an entity an `S` action binds (`Reduction::fixed_why`).

The scan never refuses a model. Leaving a class out keeps every run sound and only loses
reduction. A run up to symmetry prints one note per cone that leaves a class out, naming the
entity and the rule. Today the builder emits none of the five (its `Choose` and `Transition` effects write
columns and marks, and it writes an owner only from a param's slot or `null`), so no sample
and no corpus model loses a class; the scan guards against a later builder change. Because the DSL cannot
name a slot, the trap #212 called `sym_index_named` cannot be a corpus case. It is a unit
test on a hand-built `TransitionSystem` with a `Term::Index` under `<`.

A guard can read the owner of a deleted row: the `Earlier` role binds rows by run mark, not
by existence, so a split use case can delete a row in one step and read its key in a later
one. Phase 1 found that a delete which reset the key to its base value made that read name
slot 0 of a parent class the group moves, so no group that moves the parent was a
bisimulation. The owner decided that a deleted row keeps its keys, in the native checker,
z3, and the TLA+ export alike; the group then relabels a deleted row's key with its parent,
and the corpus case `sym_deleted_row_owner` pins it.

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
| Two stored states compared slot by slot across an edge | `can_leave` and `deadlocks` (`Deadlock_*`), the `moves` closure of `guarded_arms`, `step_breaks` (`stable`, `unchanged`) | not sound as written: the edge's target is stored permuted. Since #370 an entity cone up to symmetry reads its findings with readers of its own (`OrbitRun::state_model_findings`: deadlocks and `can_leave` on (stored state, slot) pairs carried along each edge's permutation). `step_breaks` has none: `stable` and `unchanged` run plain |
| One binding followed along a path | `reach_failure` (`can_reach`), `unlatched_state` (`precedes`), `fair_steps` and the fair components (`eventually`, `leads_to`, `.fair`), the per-slot deadlock pass | not sound as written: the binding's slot moves with each edge's permutation. Since #352 `eventually` and `leads_to` run up to symmetry on (stored state, binding) pairs (`OrbitRun::decide_liveness`), and since #370 the per-slot deadlock pass does (above). `reach_failure` and `unlatched_state` have none: `can_reach` and `precedes` run plain |

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

As built (phase 3), `Reduction::fixed_why` leaves out every S entity, every entity that holds an
S var or an input and every parent such a key names, the entity each bit is kept per row of,
and every entity a param of an S action binds: a class move's params are stored with its
class, so they cannot follow a permutation of T rows. Everything else in T moves. The run
is an `OrbitRun` on the reduced system; its successors are the reduced step relation's (the
T actions, the rises, and the class moves), canonicalized after the step, so a canonical
form keeps its S part and with it its class. A counterexample is lifted through the
symmetry first, replayed on the reduced system, then lifted through #74 by `lift`, and the
diagnostic is rebuilt on that last concrete state. The factor, measured at n=3 against the
plain counts of #212's complete cones: 5.5× on `Forbidden_PatentApplication_1` (29,680,875
states, 5,413,500 orbits), 5.2× on `Unique_Patent_PatentApplication` (5,237,935 and
1,009,760), and 3.0× on `Forbidden_DraftingOrder_0` (48,315 and 15,940), against #212's
22.5×, 21.3× and 12.4× with S permuted.

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

**Kani.** One harness, following [Kani harnesses](../engine-agreement.md#kani-harnesses),
`apply_of_a_composition_is_the_composition_of_applies` in `symmetry/mod.rs`: `apply(g ∘ h) =
apply(g) ∘ apply(h)` for symbolic `s`, `g` and `h`, on a 2-slot parent and a 2-slot child
with a key to it. It never calls `canonical`. The plan also named `canonical(apply(g, s)) =
canonical(s)` at one class of 3 slots; that harness was not built (claim A18 of
[the claims inventory](../theory/claims.md)). The invariance is checked by
the exhaustive unit test above, on every state of its layouts and every element of `G_s`,
and on every call by the `canonical` twin while `G_s` has at most 6^4 elements; neither is a
proof for symbolic states past those layouts.

**Corpus cases (`sym_` family).**

| Case | Trap |
|---|---|
| `sym_owner_coupled` | A rule that holds only if a child's owner is relabelled with its parent; a canonicalizer that permutes children alone merges states that differ |
| `sym_identity_pair` | A `Start::Identity` pair; permuting child and parent apart reaches starts the model does not have |
| `sym_deleted_row_owner` | A deleted row keeps its key and a later step reads it; a group that does not relabel it flags the wrong parent |
| `sym_master_ties` | Identical pre-existing rows named by children; the tie path must branch on named rows and not on others |
| `sym_self_owned` | An entity that owns itself (a tree); one block, enumerated jointly |
| `sym_first_absent` | A safety violation whose representative differs from the state BFS reaches first; the lifted trace must create at the lowest absent slots to replay |
| `sym_per_entity_bounds` | `--instances` with different counts per entity |
| `presence_first_absent_asymmetry` | (existing) runs unreduced until phase 4, then must still report `Order#2` only |

**Planted bugs**, each reverted, each caught by the named check:

| Planted bug | Caught by |
|---|---|
| Owners not relabelled on deleted rows | the exhaustive test, the `canonical` twin, rule 8 on `sym_deleted_row_owner` |
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

- `verify --symmetry <never|always>`, and `symmetry = "..."` under `[verify]` in
  `rdra.toml`. `always` runs up to symmetry each cone the current phase covers that no plain
  run decides ([Always by default](#always-by-default), 1); `never` is today's `verify`. The
  default is `never` until the flip makes it `always`. `retry` was removed in step 1.
- A cone line run up to symmetry keeps `never`'s shape and prints an estimate of the plain
  state count, `(about 62M states)`, described under [Decisions](#decisions).
  `RDRA_ISH_SYMMETRY_STATS` prints the orbits stored, the estimate, the states the branch cap
  cut, and the classes the scan left out ([The group](#the-group)).
- The note under a cone line: states stored without their canonical form
  ([The canonical form](#the-canonical-form)).
- `check`, `states`, `export`, and `--prove` do not read the option.

## Phases

<!-- derived-from #soundness-per-check -->
<!-- derived-from #composition-with-74 -->

Each phase lands when its exit check passes. Phases 1 to 4 change no output under the
default `--symmetry never`; phase 5 changes only cones that stop at the memory limit.

**Phase 1: safety cones #74 leaves unreduced.** The group, the scan, the canonicalizer with
its fast path, canonicalization in the workers, `OrbitRun` with its orbit self-edge list,
the trace lift, the orbit-size estimate, `--symmetry always|retry|never` (`retry` removed
in step 1 of [Always by default](#always-by-default)), the
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
up to `H` under `--symmetry always` (and `retry` until step 1) once its plain run stopped at the memory
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

As built, a reduced safety cone runs up to `G_T` under `--symmetry always`, and, until step 1
removed it, under `retry` once its lone plain run stopped at the memory limit, as a whole cone does; an
`always` property on a reduced cone still runs plain. The table below is `always`'s. Under
`retry`, `Forbidden_Patent_4` has not been seen to get that far at n=3: a run of
`verify --entity Patent --symmetry retry` at `RDRA_JOBS=4` was stopped after 2,400 s, still
in the plain attempts that come before the retry, where `always` takes 98 s. The `shadow` build counts the
`OrbitRun` twin on a reduced cone apart, as `reduced_orbit_run`. At n=3 under 8 GiB
(`verify --instance-count 3 --memory 8GiB --symmetry always --entity <E>`, `RDRA_JOBS=4`):

| Line | Plain | Up to `G_T` | Result |
|---|---|---|---|
| `Forbidden_Patent_4` | stopped at 40.8M | 31,926,220 orbits, about 260M states; `G_T` moves `DraftingOrder`, `Patent`, `PatentApplication` | `ok`, 98 s |
| `Inv_Patent_0` | stopped at 38.1M | checked in `Cone_Forbidden_Patent_4` | `ok` |
| `Forbidden_OfficeAction_3` | stopped at 33.6M | stopped at 67,108,864 orbits, about 360M states | not decided |
| `GrantEnds` | stopped at 40.0M | its reduced cone completes at 7,346,935 orbits, about 87M states (measured outside `verify`) | not run up to `G_T`: a `leads_to` with `.fair` |

`Forbidden_OfficeAction_3` is out of phase 3's reach: its S is `DraftingOrder`, `Employee`,
`InventionDisclosure`, `PatentApplication` and `PatentFirm`, so `G_T` moves `OfficeAction`
alone and the factor is at most 3! = 6; the run stored 67M orbits, about 5.4× (estimated)
fewer than the states they stand for, and still stopped. `GrantEnds` reads fair components
per binding, which no run up to symmetry may read before phase 4.

**Phase 4: the annotated quotient.** Edge permutations, the slot-wise and per-binding
readers of [Soundness per check](#soundness-per-check), lasso unrolling, and entity cones.
Exit: rule 8 holds for every check kind, `presence_first_absent_asymmetry` reports
`Order#2` only with `--symmetry always`, and `verify --with-tlc` reports no disagreement on the
samples at their `// expect` flags.

As built, first unit: `eventually` and `leads_to`, `.fair` included. Measured first, on
patent-ops `GrantEnds` (a reduced cone; `G_T` moves `DraftingOrder`, `Patent` and
`PatentApplication`): at n=2 `never` stores 75,017 states and the run up to `G_T` 30,313
orbits; at n=3 under 8 GiB `never` stops at 40.0M and the run up to `G_T` completes at
7,346,935 orbits, about 87M states, with 52.7M edges and no orbit self-edge.

- **Edge permutations.** `run_annotated_orbits` keeps, per edge, an index into a table of
  the distinct permutations its edges name, charged to the memory pool. Only a liveness
  reader asks for them, so every other run up to symmetry stops where it did.
- **The reader.** `OrbitRun::decide_liveness` runs `liveness.rs`'s search on (stored state,
  binding) pairs. An orbit self-edge is a real move: it enables, is taken, and is a loop of
  its pair when it keeps the binding, so a component of one pair can be a fair cycle.
- **Fair instances.** The sentence above that fairness "is per binding already" is looser
  than the code: an instance is keyed by its action's param slots (`keys_fair_instance`),
  which need not be the binding's, and an edge moves them by its permutation too (a class
  move keeps its params, as `concrete` does). So fairness is read on threads, (pair,
  instance) nodes. The threads over one component fall apart into pieces that each stand
  for one concrete instance over one concrete component, so the component admits a fair
  cycle exactly when it has a cycle, is expanded, and every piece takes its instance on some
  edge or meets a pair where it is not enabled.
- **Lasso.** The loop from the end's pair covers every instance enabled there; it is
  repeated until the concrete states close it, and the concrete lasso must replay, fail the
  property and pass `unfair_loop`, as a plain one must.
- **In `verify`.** Under `always`, such a property runs up to symmetry when no plain run
  decides it (`PropertyJob::symmetric`): the `Plain` run, and on an unreduced cone the
  `WfNext` run on the same graph. On a reduced cone a `Plain` counterexample asks for the
  `WF_vars(Next)` run on the reduced graph or for the unreduced cone, which this unit does
  not run up to symmetry: the property is decided as under `never`, and a plain `Plain` run
  that finds no counterexample is a tool error.
- **Twin.** `orbit_liveness` holds the pairs to a plain run under `SHADOW_CAP`: each edge's
  permutation takes its move to its target, and every plain (state, binding) fails the goal
  and can avoid it exactly when its pair does. The plain side is `liveness.rs`'s own
  `avoid`, so the twin checks the quotient, not the fair-component rule the two share
  (claim N9 of the [theory](../theory/claims.md)).
- **Result.** `GrantEnds` is decided at n=3 under 8 GiB: `ok`.
- **A system a safety cone shares stays plain.** An `eventually` or `leads_to` whose
  reduced cone is a safety cone's system does not run up to symmetry (`Shared::new`), so one
  plain run decides both, as before this unit; which ones is read off the jobs before any
  run. Without it, insurance-claim-ops' `ClaimEventuallyDecided` and
  `Cone_Forbidden_Claim_4` ran up to symmetry apart, for about as many orbits as the 633,787
  plain states, and the pair cost 16.2% more under `always` (319.20G to 370.77G instructions
  at `RDRA_JOBS=1`). On the samples it keeps `ClaimEventuallyDecided` alone plain;
  `GrantEnds` shares no system, at n=2 or n=3, and stays on the orbit path.
- **Cost.** On the 43 pairs `never` prints what `main` prints, and `always` changes four
  pairs' lines to `about <m> states`, no verdict. Instructions at `RDRA_JOBS=1`, against
  `main`: `never` 627.70G to 629.98G (+0.36%; at most +0.61%, clinic-ops), `always` 630.20G
  to 633.04G (+0.45%). Against this build's own `never`, `always` is +0.49% in all,
  insurance-claim-ops +0.86% and patent-ops +0.88%, which run cones up to symmetry, and
  clinic-ops −0.13% and oidc-auth −0.04%, which run none: within step 1's exit.

As built, second unit: entity cones. Measured first, on `clinic-ops` at n=3 under 8 GiB
(#284): the 117 findings left open were cones starved of room, not cones past the limit
(the owner's decision of [Decisions](#decisions), a stopped entity cone gives back its room,
came first, in its own change). The cones still stopped after it are what this unit runs up
to symmetry.

- **When.** Under `always`, an entity cone of `verify`'s entity pass whose plain run stops
  at the memory limit runs again up to symmetry (`run_annotated_orbits`) in the room the
  released plain run held, when that run left a finding open. A run that completes decides
  the cone's findings; one that stops leaves the plain run's.
- **The readers.** `OrbitRun::state_model_findings` reads the witnesses, the runs and the
  never-runs on stored states and action labels, as on a plain run. A `.when` arm compares
  the bound slot across an edge, so it reads the edge's concrete successor, the stored
  target taken back through the edge's permutation (`guarded_arms_by`); on an orbit
  self-edge that is not the source. The deadlock pass runs on (stored state, slot) pairs: a
  move keeps its source's slots, and the edge's permutation `σ` takes slot `j` of the
  concrete successor to slot `σ(j)` of the target, so `can_leave` and the walk to the held
  states follow the instance through `σ`. The held states are scanned each at the slot it
  keeps the instance on (`scan_guards_pinned`).
- **Traces.** A deadlock's trace is the lifted path to its pair, replayed; the lift's last
  permutation `τ` gives the slot the concrete instance holds, `τ(k)`, which the deadlock
  names. It can name another instance than `never`, as a counterexample can.
- **Twin.** `orbit_state_models` holds the findings to a plain run under `SHADOW_CAP`: the
  same witnesses, runs and arms; deadlocks at the same values; and each deadlock on a
  concrete state the plain run reaches, its instance stuck there, with the deadlock the plain
  pass gives that instance, but for the trace. A `shadow` build under
  `RDRA_ISH_FORCE_ORBITS` runs it on every entity cone of `--symmetry always`.
  `sym_arm_reorders` is a corpus case whose every guarded move swaps the stored rows.
- **Result.** On `clinic-ops` at n=3 under 8 GiB (`RDRA_JOBS=8`), `always` leaves 3 of the
  17 findings `never` leaves open. `Appointment` (stopped at 100,133,268 states) finishes in
  14,346,108 orbits for about 3.9G states, `Encounter` in 1,696,486, `PatientAccount` in
  3,885,148 and `PaymentTransaction` in 1,022,613; the rerun up to `H` of phase 2 decides
  `CarePlan`, as it does without this unit; `EligibilityCheck` stops again at 25,681,245
  orbits. Seven stopped cones (`AccountBalance` at 83,486,263 states among them) left nothing
  open and do not run again. The run peaks at 37.1 GB RSS, where `never` peaks at 35.6 GB.

**Phase 5: always by default.** Replaced on 2026-11-25: the owner chose `always`, made
invisible, over a `retry` default. [Always by default](#always-by-default) holds the plan and
its measurements.

## Decisions

<!-- derived-from #phases -->

The owner decided these on 2026-11-24.

- **The default is a retry.** Superseded on 2026-11-25 by
  [Always by default](#always-by-default); kept as the record of what `retry` does. A cone
  that `verify` decides today keeps today's output. A cone that stops at the memory limit is
  run again up to symmetry; nothing else is.
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
  about 62M states)`. Superseded on 2026-11-25: the line prints `(about 62M states)`, and
  the orbit count moves to `RDRA_ISH_SYMMETRY_STATS`. The estimate below stands.
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

Decided on 2026-11-25 for [Always by default](#always-by-default). The owner delegated the
three choices of the first draft to the coordinator's recommendations; the reuse of plain
results was proposed with the measurements of item 1 and accepted by the coordinator.

- **`never` stays and `retry` goes.** `never` is the escape hatch, still exercised in CI after
  the flip. `retry` is removed in step 1, since `--symmetry` is in no release.
- **The default output describes the user's model.**
  - Cone lines keep `never`'s shape and print a state count, exact where a plain run
    decided the cone and `about <m>` where the orbit run did.
  - Orbit counts and the left-out notes move to `RDRA_ISH_SYMMETRY_STATS`.
  - A counterexample may name other rows than `never`'s only if it replays on the
    unreduced model and is the same in every run and at every `RDRA_JOBS`.
  - A plain result that an earlier pass decided on a complete run is kept. That choice
    follows from the pass order and the model, never from which run finishes first, and
    the output is the same at `RDRA_JOBS=1`, 4 and 8.
- **The order and its gates.** Step 1, then phase 4, then the flip. The gates: the per-pair
  exit of step 1, per-rule witnesses before the flip, the flip as a `### Breaking` entry,
  and any residual-cost choice back to the owner.

The owner decided this on 2026-11-25, before the entity cones of phase 4 (#284).

- **A stopped entity cone gives back its room.** The rule was that under a memory limit the
  cones run one at a time in cone order, each in the room the results before it leave
  ([Memory limit](../cli-reference.md#memory-limit)). Measured on `clinic-ops` at n=3 under
  8 GiB, the 117 findings `verify` left open were not cones too large for the limit, but
  cones with no room: `AccountBalance`, first in cone order, stopped at 83,486,263 states and
  held 7.9 GB until the pass ended, and the 17 cones after it got no room and stored no
  state, `InsurancePolicy` (about 1 KB alone) and `Prescription` (about 400 MB) among them.
  Now `verify` reads a stopped entity cone's findings (witnesses, runs, arms taken) when it
  stops and releases its run, which leaves no result, so the cones after it run in the room
  it held. Only an undecided finding may change, to decided: the 43 sample pairs under
  `never` print what they printed, the output does not depend on `RDRA_JOBS`, and a reader
  of a released run's graph panics, so nothing but `entity_findings` reads a stopped entity
  cone. `check` and `states` keep every run, since they read a stopped cone's views. On
  `clinic-ops` at n=3 under 8 GiB, 17 findings stay open where 117 did. The cones that had
  no room now explore, so the run takes 5,578 s where it took 1,707 s and peaks at 35.6 GB
  RSS where it peaked at 29.2 GB (the limit bounds the stores, not the process: #237,
  #296).

## Always by default

<!-- derived-from #phases -->
<!-- constrained-by #soundness-per-check -->
<!-- constrained-by #composition-with-74 -->

The owner decided on 2026-11-25 (#212): no `retry` default. Instance symmetry is on by
default and users should not have to think about it: the verdicts of `never`, counterexamples
as concrete paths of the user's model, no noticeable cost on a model with nothing to permute,
and no need for `--symmetry`. This section measures what `always` costs and prints today and
says how each difference goes away or why it may stay. Phase 4 and the default change follow
in small PRs once this lands.

### Measured

On `main` at `7e464c43` (phases 1 to 3), each run alone.

**The 43 sample pairs at their `// expect verify` flags**, `RDRA_JOBS=1`, instructions retired
(`/usr/bin/time -l`; wall times are left out, since other jobs pushed the load past 18):

| | `never` | `always` |
|---|---|---|
| Instructions, all pairs | 628.64G | 696.82G (+10.8%) |
| Largest peak RSS | 1,195 MiB | 1,197 MiB |
| Verdicts and exit codes | | the same on all 43 |

| Pair | `never` | `always` | Change |
|---|---|---|---|
| patent-ops | 36.76G | 56.50G | +53.7% |
| insurance-claim-ops | 316.94G | 364.71G | +15.1% |
| oidc-auth `--instance-count 1 --prove` | 62.53G | 63.05G | +0.8% |
| food-permit-ops | 102.57G | 102.84G | +0.3% |
| clinic-ops | 107.43G | 107.31G | −0.1% |
| the other 38 | 0.04–0.64G each | | −10% to +5% of under 0.7G |

**patent-ops at n=3**, `--instance-count 3 --memory 8GiB --entity <E>`, `RDRA_JOBS=4`. The
`never` runs ran under `timeout`, which hid their instruction counts; user CPU stands in:

| Scope | `never` | `always` |
|---|---|---|
| `DraftingOrder` | `ok`, 48,315 states; 0.07 s | `ok`, 15,940 orbits; 0.05 s |
| `PatentApplication` | `Forbidden_PatentApplication_1` `ok`, 29,680,875 states; 341 s wall, 333 s user, 15.0 GB peak | `ok`, 5,413,500 orbits; 67 s wall, 96 s user, 12.5 GB, 1,649G instructions |
| `Patent` | `Forbidden_Patent_4` not decided at 111,509,504 states; `Inv_Patent_0` `ok` at 40,637,685; 832 s wall, 826 s user, 19.1 GB | `Forbidden_Patent_4` `ok`, 31,926,220 orbits, `Inv_Patent_0` with it; 98 s wall, 184 s user, 13.0 GB, 3,559G |
| `OfficeAction` | not rerun (#212: stopped at 33.6M) | not decided, 67,108,864 orbits; 262 s wall, 472 s user, 13.4 GB, 9,067G |

`never` decides `Inv_Patent_0` on today's `main` by itself, so phase 3's new line at n=3 is
`Forbidden_Patent_4` alone.

### Where `always` costs more or prints otherwise

1. **Runs it repeats.** Under `always`, a job that runs up to symmetry shares no run with
   the entity, safety and property passes (#306). So the safety pass runs up to symmetry a
   cone the entity pass has already explored plainly and to the end. This is most of
   patent-ops' +53.7%; the other pairs are measured again in step 1. On patent-ops
   (`RDRA_PERF=1 RDRA_JOBS=1`, n=2):
   - the entity pass explores the same reduced cones plainly either way, 1,861,636 states;
   - the safety pass grows from 3 runs of 63,371 states (41 ms) to 6 runs of 808,320
     stored states (938 ms);
   - per stored state the orbit run costs 1,159 ns, against 742 ns for a plain run of the
     same cone: 1.6×, not the 30× that the per-successor cost of canonicalization suggests.

   It also moves output: a counterexample cone counts what it stored before it stopped.
   `formal-verification-fail/order.rdra` `Inv_Order_0` reads `(3 orbits, about 3 states)`
   where `never` reads `(7 states)`, off the entity pass's run.

   *Removed by:* a cone that an earlier pass decided on a complete plain run keeps that
   result, its line and its count, as under `never`. A cone runs up to symmetry only where
   no plain run decided it. The choice follows from the pass order and the model alone.
   - Which cones an entity cone's run can decide is fixed before any run: a safety or
     property cone whose system is that of an entity cone (`Shared::wanted`).
   - The entity pass ends before the safety pass starts.
   - A run that met the memory limit in a crowded pool runs again alone before its answer
     counts, as under `never`.

   So `RDRA_JOBS` cannot move the choice. The 43 pairs must print the same at
   `RDRA_JOBS=1`, 4 and 8. A test-only switch, as for S up to `H`,
   runs every symmetric cone up to symmetry, so that the twins and rule 8 still reach the
   orbit run on the samples.
2. **Canonicalization on every successor.** It costs 75.4K instructions per successor on
   `Inv_Encounter_2`, against 2.5K for a plain successor (#311). Nothing short-circuits a
   group that moves nothing: `Canon::canonical` sorts every block of every successor, and
   `Group::is_trivial` exists only in tests. Some cones pay for nothing: insurance-claim-ops
   `MultiForbidden_Payment_3` stores 779,267 orbits for 783,427 states.
   *Removed by:*
   - (a) a cone whose group moves nothing runs the plain path, `run` and not
     `run_orbits`, so its cost and output are `never`'s;
   - (b) a state in which no class has two rows that may move skips the sort and is stored
     as it is;
   - (c) a cheaper canonical form for the common case of one block and no joint classes,
     built only if the exit below needs it.
3. **The cone line counts orbits.** 78 lines in 36 of the 43 pairs read
   `(<n> orbits, about <m> states)` where `never` reads `(<n> states)`. In 19 of them the two
   counts are equal, because nothing moved.
   *Removed by:* 1 and 2(a), which print `never`'s line wherever a plain run decided the cone
   or the group moves nothing. Where the orbit run decides, the line keeps `never`'s shape
   and prints `(about <m> states)`. The orbit count moves to the diagnostic level (4).

   The exact count cannot be recovered from the orbit sizes at insert. The size of an orbit
   is `|G_r| / |Aut(r)|`, and an orbit can hold states no run reaches: the `FirstAbsent`
   asymmetry of [The group](#the-group), 4 of 41 states in
   `presence_first_absent_asymmetry`. A state the branch cap cut also counts as `|G_r|`.
   The measured estimate runs high: 253 for 243 states on food-permit-ops
   `Forbidden_BusinessPermit_4`, and 7.3K for 6,125 on `Inv_PermitApplication_2`. So `about`
   stays on exactly those lines.
4. **Notes on what the group leaves out.** 80 `note: instance symmetry leaves <entity> out`
   lines in 7 pairs: every S entity of a reduced cone, and each class the scan leaves out.
   They explain lost reduction, not a verdict.
   *Removed by:* the diagnostic level, which is the environment variable
   `RDRA_ISH_SYMMETRY_STATS`. It prints, per run up to symmetry, the orbits, the estimate,
   the states the branch cap cut, and the classes left out with their rules.
   `RDRA_ISH_G_T_STATS` keeps the per-entity reasons of `G_T`. The branch-cap note stays
   under the line, because it says the count is an upper bound. No sample hits it.
5. **Where `--memory` and `--max-states` stop a run.** An orbit run stops at the memory limit
   at another count than a plain run: `Forbidden_OfficeAction_3` at 67,108,864 orbits, and
   `never`'s `Forbidden_Patent_4` at 111,509,504 states. Under `--max-states` the cap counts
   stored states, which are orbits.
   *Acceptable:* a run up to symmetry stores one state per orbit of the states a plain run
   would store, so it reaches either limit no earlier in the exploration. The compatibility
   gate in [Order](#order) settles what this means for a release.
6. **S up to `H` only at the memory limit.** Kept. Under the default, the sub-lifecycle of a
   reduced cone runs plainly first, and runs up to `H` only when that run stops at the
   memory limit with the pool to itself. A cone whose S fits keeps its classes, its `Rep_`
   tables and its lifted traces (#284). The price is the plain attempt before `H`: 35 to 61 s
   on the S runs #284 saw stop at n=3, paid only where S does not fit.
7. **RSS above `--memory` (#237).** At n=3 under `--memory 8GiB` the process peaks at
   12.5–13.4 GB with `always` and 15.0–19.1 GB with `never`. `always` is not worse; the gap
   between the limit and the process is #237's, and the default change does not wait for it.
8. **Counterexamples.** They are lifted through every permutation, and through #74 on a
   reduced cone, and replayed with the rows each step names, so they are concrete paths of
   the user's model. They may name other rows, or another violating binding, than `never`'s;
   agreement rule 1 compares verdicts and violated checks. No counterexample text differed
   on the 43 pairs.
   *Acceptable, under a condition:* such a counterexample replays on the unreduced model and
   is the same in every run and at every `RDRA_JOBS`. Step 1 adds a test that holds it to
   both.
9. **`--with-tlc`.** TLC still explores the plain module and prints its state counts beside
   the native line, and the two are compared on verdicts only. *Acceptable.*
10. **Checks that stay plain until phase 4.** Entity cones, `stable`, `unchanged`,
    `precedes`, `can_reach`, and an `always` property on a reduced cone run as under
    `never`, so they print `never`'s lines. The liveness properties run up to symmetry since
    phase 4's first unit, but on a reduced cone a counterexample still sends the property to
    the plain runs. [Since #370 an entity cone whose plain run stops at the memory limit
    with a finding left open runs again up to symmetry.]

### What a user still sees differ

Once step 1 lands, a cone that a plain run decided, or whose group moves nothing, prints
exactly what `never` prints. A cone that the orbit run decides differs in three ways:

- its count reads `about <m> states`, rounded to two figures;
- its counterexample can name other rows, the same ones in every run;
- it decides more under the same `--memory` and `--max-states`.

Nothing else in the default output names symmetry.

### `--symmetry`

The default becomes `always`. `never` stays as an escape hatch, kept out of the help's first
screen and still run in CI after the flip by engine-agreement rule 8 and the twins. It
serves three readers:

- rule 8 and the twins, which compare a run up to symmetry with a plain one;
- a user who suspects the symmetry and wants to rerun plainly;
- docs and tests that quote exact plain state counts, such as the cone sizes in sample
  READMEs.

`retry` goes in step 1, not at the flip: `--symmetry` is in no release tag (`v2.3.0` does not
hold `40e3783a`), so its removal needs no deprecation. `always` covers what `retry` did.
`symmetry` under `[verify]` in `rdra.toml` keeps `never` and `always`.

### Order

1. **Step 1.** Under `--symmetry always`, with `never` byte-identical to `main`:
   - the reuse of plain results (1);
   - 2(a) and 2(b), and 2(c) if the exit needs it;
   - the line and the diagnostic level (3, 4);
   - the counterexample test (8);
   - the removal of `retry`, with the CLI reference, the `rdra.toml` docs, and an
     `Unreleased` CHANGELOG entry.

   **Exit, per pair and not in aggregate,** since an aggregate hides patent-ops:
   - instructions at `RDRA_JOBS=1` within 0.5% of `never` on every pair where no cone runs
     up to symmetry;
   - within 2% on every other pair;
   - output identical at `RDRA_JOBS=1`, 4 and 8.

   A cone that the orbit run decides may keep a residual cost only up to its stored states
   times the per-state cost measured in 1, and only where `never` would store more. If 2(a)
   to 2(c) leave a pair above 2%, the choice between accepting that residual and an
   internal per-cone choice goes back to the owner. That choice would be derived from the
   model and the flags before any run, not from a runtime retry.
2. **Phase 4**, the annotated quotient, as [Phases](#phases) has it.
3. **The flip** to `always`. Two gates come first:
   - **Per-rule witnesses** ([The per-rule gate](#the-per-rule-gate)). With `always` as the
     default, no single rule may regress silently.
   - **Compatibility.** [Compatibility](../compatibility.md#bounds) calls a result that
     changes within the same printed bounds a breaking change.
     - Under `always`, a cone that stops at `--max-states` under `never` can decide, since
       the cap counts stored states.
     - With `--deny-undecided`, a cone that stops at `--memory` can then pass where it
       failed, which changes the exit status.
     - A verdict never moves between `ok` and `counterexample`.

     The flip therefore goes under `### Breaking`, naming those two moves and `never` as
     the way back. Which release carries it is the owner's call.

### The per-rule gate
<!-- constrained-by #the-group -->
<!-- constrained-by #composition-with-74 -->

Decided on 2026-11-25. Every rule that leaves a class out of the group, and every reason
`G_T` or `H` keeps an entity in place for, needs one of these:

- (a) A corpus witness: a model in the corpus or the samples on which dropping that rule
  alone changes a verdict or fires a twin, as `sym_deleted_row_owner` does for the deleted
  row's key.
- (b) A hand-built witness, for a rule the builder cannot emit today and for a `G_T` or `H`
  reason that can stand alone. Dropping the rule alone makes the quotient fail the
  bisimulation check, check 2 of the `orbit_run` twin.
- (c) For a reason another reason implies, the implication checked by an always-on
  assertion or a test, or the two reasons merged.
- (d) `JointTooLarge` is exempt, as a cost cap. A test shows that past `BRANCH_CAP` a state
  is stored without its canonical form and the verdict stands.
- (e) For a rule the owner keeps with none of these, the snapshot of the group rules. Per
  cone of the samples and the corpus, it records which entities `G_T` keeps in place and
  why, which entities `H` keeps in place and why, and which scan rules fire.

An always-on check that panics, or that reports a tool error (`LiftFault`), when the rule is
dropped also counts as a witness, since that failure is loud. A reason with no witness, even hand-built, stays in the code and goes
to the owner as possibly over-conservative.

A test-only switch drops one rule at a time: `RDRA_ISH_DROP_RULE=<rule>` in a `shadow`
build, and a per-thread hook in unit tests (`dropped`). It names a scan rule by its variant
(`OrderedId`), a `G_T` reason as `Reduced:<reason>`, and an `H` reason as
`Observed:<reason>`.

| Rule or reason | Witness |
|---|---|
| `OrderedId`, `IdAgainstLiteral`, `MixedIds`, `OwnerLiteral`, `OwnerChoice`, `UnlinkedCreate`, `AbsentOwner` | (b): `explore/symmetry/witnesses.rs`, one test each |
| `PinnedStart` | an always-on assertion: `witnesses.rs::pinned_start_has_a_witness` (claim A20, #512) |
| `JointTooLarge` | (d): `joint_too_large_is_a_cost_cap`, `past_the_branch_cap_the_verdict_stands` |
| `G_T` "S" | (c): implied by "S var", asserted in `Reduction::fixed_why` |
| `H` "obs", "input" | the assertion in `up_to_h` (`reduce/partition.rs`) that `H` moves nothing the quotient reads; `each_h_reason_a_case_shows_has_a_witness` in engine_corpus |
| `G_T` "S var", "input", "named by S", "named by an input", "bit key", "S action param" | (e): see below |
| `H` "named by obs", "named by an input", "stub", "named by T" | (e): see below |

No model the builder emits today needs a scan rule. A run of every corpus case and sample
forced up to symmetry shows none of them, as breaks.rs says, so the witnesses of (b) are
built models with one hand-made change each.

**Kept without a witness of their own.** No witness of kinds (a) to (d) was found for the
eleven rules and reasons below. The owner decided on 2026-11-25 that they keep their place.
The snapshot (e) is their witness for this gate: `the_group_rules_of_every_cone_are_the_snapshot`
in `explore/symmetry/snapshot.rs`.

- **What it records.** It reduces every cone that an unscoped `verify` runs, as `verify`
  reduces it, and runs none of them. The cones are the safety cones, the entity cones, and
  the cones of the properties that `verify` does not decide on an explored graph. Each model runs at the
  bounds of its `// expect verify` headers, or at its corpus `// flags:` line. A corpus case
  whose header names BFS verdicts per bound also runs at `--instance-count 3`. Per cone and
  entity, a line records the slots, whether the group moves the entity, the scan rules, and
  the `G_T` reasons. On a reduced cone, the line also records the same for `H` on the
  sub-lifecycle that S would run up to `H`. That is the innermost nested one, which
  `Classes::of` explores unreduced. `Extended::fixed_why` gives `H`'s reasons, and
  `Extended::fixed` is that map's set.
- **How it bites.** A dropped reason drops out of its lines, even where another reason
  keeps the entity in place. So a changed or dropped rule changes the snapshot, which an
  unchanged verdict would not show. The test drops every rule and reason in turn. It fails
  if the snapshot stays the same for a rule that some line names. It also fails if the
  snapshot changes for a rule that no line names. The snapshot's header counts the lines
  that name each rule and reason.
- **`PinnedStart` fires in no cone** of the samples or the corpus, so the snapshot cannot see
  it dropped. The header says so. Its witness is a test of its own (below).
- **The runs of S.** No run of S in the test has a memory limit. A plain run of S stops at
  2,000,000 states, or at `--max-states`. Past that cap, S runs up to `H`, as `--symmetry
  always` does at the memory limit, and the cone line says so. Two sub-lifecycles pass the
  cap; a run with no cap passed 40 GB before it was stopped. clinic-ops' Charge reduces up
  to `H`. One oidc-auth safety cone passes the cap up to `H` too, so it stays unreduced. That cone may be outside
  what the oidc-auth headers run, since the test builds every model with `Scope::ALL`, a
  superset of the cones of a header line scoped by `--entity`.
- **Where it runs.** The test takes about 3 s in release and about a minute in debug. So
  `scripts/diff-test.sh` skips it, and the formal-verify job runs it in release, in the
  step "Each group rule has a witness".

Why no witness was found:

- `PinnedStart` has one since #512 (claim A20). Only the start depends on it, and the
  transitions do not, so dropping it leaves the quotient a bisimulation with the same
  verdicts, as dropping it on `sym_identity_pair` showed. What breaks is the trace: the run
  stores the canonical form of each initial state, and where that form is no initial state
  the counterexample lifted from it starts nowhere the model starts. `orbit.rs::replays`
  checks every lifted trace in every build and panics (`NotInitial`), an always-on assertion.
  `witnesses.rs::pinned_start_has_a_witness` builds that case: pre-existing orders under
  the shop in slot 0 (a start set by hand, since no built model pins a slot) and shops whose
  tier is free at the start.
- The `G_T` reasons other than "S". On the 15 reduced cones the corpus and the samples run up
  to `G_T`, every fixed entity has at least two reasons, with one exception:
  insurance-claim-ops' Claim, fixed for "S action param" alone. Dropping that reason there,
  at one instance per entity and two Claims, runs Claim's cones up to `G_T`, and the
  `reduced_orbit_run` twin compares both runs and passes. One hand-built variant of the gate
  model, a T parent of the job, came out as part of S.
- The `H` reasons other than "obs" and "input": dropping one alone on the corpus fires
  nothing.
