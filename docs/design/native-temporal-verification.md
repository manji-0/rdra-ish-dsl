# Native temporal verification: deciding properties without TLC

<!-- constrained-by ../vision.md -->
<!-- constrained-by ../compatibility.md -->
<!-- derived-from ../formal-verification.md#temporal-properties-on-a-cone -->
<!-- derived-from ../formal-verification.md#reduction-by-an-independent-sub-lifecycle -->
<!-- derived-from ../state-derivation.md#exploration-by-cone-of-influence -->
<!-- derived-from ../language-reference.md#fairness-fairusecase- -->

Status (2026-11-26): in progress. The plan started after
[#165](https://github.com/manji-0/rdra-ish-dsl/issues/165),
[#166](https://github.com/manji-0/rdra-ish-dsl/issues/166), and
[#167](https://github.com/manji-0/rdra-ish-dsl/issues/167) landed.

- **Landed.** Phases 0 to 3 (#176, #177, #178, #179): `verify` decides every check natively
  by default, and TLC runs only under `--with-tlc`. The conservative divergence trial C,
  adopted (#227, [Decisions](#decisions)).
- **In review.** Phase 4 with #206, on the branch `native-temporal/phase-4`: the exact
  cone, steps 4 to 6, and the safety recheck. No native `RdraSpec_` line is left. The exit
  check (TLC, `reduction-diff.py`, the samples) is not run yet.
- **Open.** Phase 5, the lift of a reduced lasso. Phase 6, the release gate.
- **Superseded.** Nothing.

The owner decided three questions on 2026-11-23: `verify` defaults to the native checker
and `--with-tlc` adds TLC, each phase lands on `main` when it passes while the next release
waits until the native checker is stable, and `check` and `states` evaluate no `property`.
The same day the owner settled `--backend tlc`, the verdict line, verdicts that move off
`not decided`, and the release gate, and then the wait of the release gate, step 5, a trial
of a conservative divergence, and where the samples run with `--with-tlc`.
[Decisions](#decisions) records them.

`verify` needs a JVM and TLC today. TLC decides every invariant cone, every `after`
assertion, and every temporal property, while the explorer in `rdra-ish-core` reaches the
same states and keeps every edge. This document plans a checker inside `rdra-ish-core` that
decides the properties an rdra-ish model can state, so that TLC becomes a second opinion
that a run may ask for and no longer a tool that `verify` cannot run without. [Since phase 3
(#179) `verify` needs no JVM and no TLC; TLC runs only under `--with-tlc`.]

**Purpose.** `verify` gives the verdicts it gives today, at the same bounds, with no
external tool. It prints each counterexample in the form it prints now. TLC stays as the
oracle of the tests, and `verify --with-tlc` runs it beside the native checker and fails
when the two disagree.

## What exists today

[As of 2026-11-23, before phase 0. Phase 3 moved every check off TLC; see the status above.]

**The explorer keeps the graph a temporal check needs.** `ComponentRun` holds the reached
states, the edge that first reached each state, and every move as an `Edge` of three `u32`
(`from`, `to`, and the index of an `(action, params)` binding), self-loops and repeats
included (`crates/rdra-ish-core/src/lifecycle/explore/mod.rs:43-77`). `Graph` hands the
states and edges to the findings (`explore/mod.rs:140`). The deadlock finding already walks
that graph backwards (`can_leave`, `lifecycle/findings.rs:398`) over a compressed adjacency
(`Adjacency`, `findings.rs:114`).

**The explorer evaluates no temporal property.** `Property` holds the binders, an `Ltl` of
`Always`, `Eventually`, or `LeadsTo`, and the `.fair` use cases
(`lifecycle/types.rs:736-760`). Its comment says the explorer does not check it. `check`
prints a note and points at `verify`.

**The semantics of a property already exist in Rust, for one lasso.** `fails_at`
(`lifecycle/lasso.rs:149-179`) evaluates a property on the infinite behavior a lasso stands
for, per binding of the binders, the way the TLA+ printer spells it. `always x` is
`[](present => x)`, `eventually x` is `[](present => <>x)`, and `leads_to x y` is
`(present /\ x) ~> y`. Only the trigger reads `present`. `replay_lasso` matches a lasso from
another engine against the step relation, and `explain_property` builds the message
(`lasso.rs:57`, `lasso.rs:92`). Nothing in `lasso.rs` reads fairness, because TLC has
already applied it.

**TLC runs up to three times per property.** `PropertyCone`
(`crates/rdra-ish-cli/src/property_cone.rs:34`) and
[Temporal properties on a cone](../formal-verification.md#temporal-properties-on-a-cone)
define the runs:

| Run | System | Fairness | What it decides |
|---|---|---|---|
| `Cone_<P>` (`Plain`) | The property's cone, reduced by its sub-lifecycle | `Fair_P` only | An `ok` is the whole model's `ok`. A counterexample of an `always` is final. |
| `Cone_<P>_Full` | The unreduced cone | `Fair_P` only | Repeats a counterexample of the reduced cone, because a reduced lasso does not lift step by step. |
| `Cone_<P>_WF` (`WfNext`) | The unreduced cone | `Fair_P` and `WF_vars(Next)` | A counterexample is final. An `ok` is final only when the cone holds every var an action writes (`cone_keeps_every_write`, `lifecycle/cone.rs:418`). |
| `RdraSpec_<P>` | The whole model | `Fair_P` and `WF_vars(Next)` | Decides what the cone left open, under `--temporal-budget` and a cap of 2,097,152 states (`verify.rs:40`). |

A cone that frees an entity's slots (`cone_frees_slots`, `cone.rs:426`) reaches a superset,
so only its `ok` counts and a counterexample goes to the whole model.

**TLC also decides the safety cones.** `verify` prints one module per cone of an invariant
or `after` assertion and runs TLC on it. BFS runs on the entity cones only, for witnesses,
deadlocks, and use cases that never run
([What `verify` checks](../formal-verification.md#what-verify-checks)). `check` decides the
same invariants and assertions by BFS, at its own bounds.

**The engines are compared.** `engine_equivalence` compares BFS and TLC on the states, the
edges, the findings, and the safety verdicts of every sample cone.
`a_property_cone_decides_what_the_whole_model_decides`
(`crates/rdra-ish-cli/tests/engine_equivalence.rs:2732`) compares the cone verdict of each
property with a whole-model TLC run on five samples and twelve fixtures
(`TEMPORAL_CASES`, `engine_equivalence.rs:2474`).
`scripts/reduction-diff.py` compares reduced and unreduced `verify` runs on random models
that hold `leads_to` and `eventually` properties, some with `.fair`
(`scripts/reduction-diff.py:233-240`).

**What the samples cost today.** One run each on an 18-core Apple silicon machine with
128 GB, release build, TLC 1.7.4, `verify --backend tlc` at the default bounds:

| Sample | `verify` wall time | Peak RSS | Property cones (states) |
|---|---:|---:|---|
| `order.rdra` | 5.8 s | 142 MB | 31, 31, then the whole model (31) twice |
| `temporal_fair.rdra` | 7.1 s | 146 MB | 43, 43 |
| `temporal_unmet.rdra` | 7.0 s | 122 MB | 31, 31 |
| `split_run_order.rdra` | 3.9 s | 152 MB | 82 |
| `int_stock.rdra` | 3.7 s | 125 MB | 21 |
| patent-ops | 33.3 s | 795 MB | `GrantEnds` 75,017 |
| food-permit-ops | 24.0 s | 1.61 GB | `HygieneManagerEventuallyAppointed` 243 |
| insurance-claim-ops | 127.0 s | 2.10 GB | `ClaimEventuallyDecided` 633,787, `SubrogationEnds` 372 |

`check --instance-count 2` on `order.rdra` takes under 10 ms at 7.6 MB, so the seconds of
the five small samples are JVM starts, not exploration. The RSS is that of the `rdra-ish`
process and the largest child, as `/usr/bin/time -l` reports it.
[Measurements](#measurements) has the numbers for the native side.

## Goals and non-goals

Goals:

- **One checker for every property form.** After #165 to #167 the forms are `always`,
  `eventually`, `leads_to`, `stable`, `unchanged`, `precedes`, and `can_reach`. Each is
  decided on the explicit graph of its cone, by code in `rdra-ish-core`.
- **The verdicts TLC gives.** Per run and per property, at the same bounds, with the
  fairness `verify` defines today. Where the native checker decides a property that today
  ends as `not decided`, the verdict is the one the whole model has.
- **The same counterexample output.** A violation prints through `explain_property`, with
  the numbered steps, `then loops back to step K`, or `then nothing changes forever`.
- **No JVM for a default `verify`.** That includes the invariant and `after` cones, which
  move to the BFS that `check` already uses.
- **TLC stays the oracle.** The tests keep comparing the engines, and `--with-tlc` asks
  for the comparison in a run.
- **One command per kind of question.** `check` and `states` verify safety by state
  exploration: the rules, the `after` assertions, and the state-model findings. Every
  `property` form and every inductive proof is `verify`'s. The docs say so in one place
  each (see [Migration](#migration)).

Non-goals:

- **General LTL or CTL.** The checker has no formula language, no Büchi automaton, and no
  nested operators. A property is one of the seven forms, and its operands are state
  predicates. The parser accepts nothing else today, and this plan adds no syntax.
- **Strong fairness.** `.fair` stays weak fairness per instance. A step that is enabled and
  disabled forever is still not forced
  ([Approximations and limitations](../formal-verification.md#approximations-and-limitations)).
- **Verdicts beyond the bounds.** Every result holds at the instance count and Int range of
  the `bounds:` line. Proofs for any number of rows stay with `verify --prove`, which reads
  state rules only.
- **`next`, real time, and counting.** There is no next-step operator, the clock `now` stays
  a bounded counter, and a raised mark still counts no events. (Reopened for one opt-in form
  by #220: `triggers(Ev, UC).counted(k)` counts the raises of that edge, with a bound and an
  overflow rule, in [counted events](./counted-events.md). A latch edge is unchanged.)
- **Symbolic or partial-order search.** The checker reads the graph BFS built. It adds no
  BDD, no SAT encoding, and no on-the-fly product.
- **Removing the TLA+ export.** `export --kind tla` and its names stay covered by
  [compatibility](../compatibility.md#the-tla-export).

## What makes a native checker possible

The transition system is finite by construction, so a property is a question about a finite
graph:

- **Finite variables.** Every variable has a small domain: the three existence values, the
  Enum variants, `false` and `true`, `null` and `present`, an Int in `0..5`, a parent slot,
  a run mark, a raised mark, and `now` in `0..5`
  ([Variables](../state-derivation.md#variables)).
- **Finite instances.** Each entity has `--instance-count` slots, and a deleted slot is
  never reused.
- **Finite actions.** A use case is one action per transaction group, and its parameters
  range over the slots.
- **A cone is closed.** Every action that writes a var of a cone reads only vars of the
  cone, so a cone is a system of its own.

Three things are abstracted, and the native checker inherits each one unchanged. Int-like
columns and `now` stop at the top of the range. A raised mark is a latch, unless its edge
says `.counted(k)` ([counted events](./counted-events.md)). A cone that does
not hold every raiser of an event runs the handler unordered, so a finding there can be a
run the order prevents
([Known verification gaps](../formal-verification.md#known-verification-gaps)).

The explorer has no symmetry reduction, and the system is not symmetric in its slots: a
creation takes the lowest absent slot (`FirstAbsent`). The checker therefore evaluates every
binding of a property's binders and assumes no symmetry.

## The property fragment

Each property is quantified over every slot of each binder entity. Write `b` for one
binding, `present(b)` for "every bound slot is present", and `link(b)` for the owner
equalities of an `.along` path (#165). A property without `.along` has `link(b) = TRUE`.
The trigger of every form is `present(b) /\ link(b)`.

| Form | Meaning per binding | Class | Reads fairness |
|---|---|---|---|
| `always(x)` | `[](trigger => x)` | State safety | No |
| `precedes(p, q)` (#166) | `[](trigger /\ q => seen_p)`, with a latch `seen_p` per binding | State safety | No |
| `stable(p)` (#166) | On every step, `present /\ p` before and `present` after imply `p` after | Step safety | No |
| `unchanged(E.col, ..).when(p)` (#166) | On every step, `present /\ p` before and `present` after imply the columns keep their values | Step safety | No |
| `eventually(x)` | `[](trigger => <>x)` | Liveness | Yes |
| `leads_to(x, y)` | `(trigger /\ x) ~> y` | Liveness | Yes |
| `can_reach(p)` (#167) | From every reachable state where `trigger` holds, some path reaches a state where `p` holds | Branching | No |

`eventually(x)` is `leads_to(TRUE, x)`, so the checker has one liveness routine with a
trigger predicate and a goal predicate.

**Presence.** Only the trigger reads `present` and `link`. A goal holds or fails on the
values of its columns whatever the slot's existence, as `fails_at` evaluates it today. A
deleted row has base values, so a goal over base values can hold for a deleted row.

**Creation and deletion.** A binding whose slot is absent in every state triggers nothing.
A row created later triggers from the state that creates it. A row deleted after its
trigger still owes its goal, and the goal is then read on the base values.

**`.along` and moving keys.** The link is read where the trigger is read. A child that
moves to another parent after its trigger keeps the obligation it took on at the trigger.
The goal is read on the bound slots, not on the row the key names later.

**Stuttering.** A behavior may repeat its last state forever. A self-loop edge, where an
action runs and changes no var, is a stuttering step. It does not count as a step of any
action, and it does not make any action enabled.

**Fairness.** `verify` defines two assumptions, and the native checker implements both as
they are printed:

- **`Fair_P`.** For each action of each use case in `.fair`, and for each binding of the
  action's parameters other than `New` (`Existing`, `Parent`, `Earlier`, and `Triggered`),
  `WF_vars` of the action with its `New` parameters bound inside by `\E`
  (`crates/rdra-ish-emit/src/tla/module.rs`). A fair instance is therefore the pair of an
  action and its non-`New` slots. `Property::fair_instance` and
  `ParamRole::keys_fair_instance` state the rule once, and the printer and the checker both
  read it. A split use case has
  one fair instance per step and binding. The property is `Fair_P => (...)`, so a behavior
  that starves a fair instance is not a counterexample.
- **`WF_vars(Next)`.** The specification of an `eventually` or a `leads_to` adds weak
  fairness on the whole `Next`. A behavior that stops while some step can still change a
  var is not a counterexample. `always` and the other safety forms do not read it, because
  a finite prefix violates them and every prefix extends to a fair behavior.

Weak fairness of an instance `a` says that a behavior does not stay forever in states
where `a` is enabled without taking an `a` step. On a finite graph, "enabled" at a state
means that the state has an out-edge labelled `a` whose target differs from its source.

**Who evaluates a property.** `verify` alone, for all seven forms. `check` and `states`
evaluate no `property`, whatever its class. `always`, `stable`, `unchanged`, and
`precedes` are safety properties that BFS can decide, and they are still `verify`'s,
because the rule goes by the declaration and not by the class. `check` keeps printing its
note that the properties were not checked, and the note names `verify`. A rule that
`check` must enforce is written as a `forbidden`, an `invariant`, or an `after` assertion.

Out of the fragment: operators nested in an operand, a property that compares two moments
of one row other than through `stable`, `unchanged`, or `precedes`, strong fairness, a
bound on the number of steps, and any property over the run marks or raised marks, which no
operand can name.

## The algorithm

### One graph, three kinds of question

The checker takes a finished run (`ComponentRun`), a property, and a mode, and returns a
verdict with a witness. It reads the run's graph, the BFS parents for the prefix of a
counterexample, and each binding's parameters for the fair instances. It never explores. Each class of form asks a different question of the same graph:

| Class | Question | Cost per binding |
|---|---|---|
| State safety | Is a state that violates the predicate reachable? | One pass over the states |
| Step safety | Does an edge violate the predicate? | One pass over the edges |
| Liveness | Is a fair cycle or a fair stop reachable from a triggered state without passing the goal? | One strongly connected component pass |
| Branching | Does some triggered state have no path to a goal state? | One backward pass |

State and step safety already run inside BFS for invariants and `after` assertions
(`check_invariants`, `check_after` in `explore/bfs.rs`). The property forms use the same
two evaluators, and only a run that `verify` starts hands them the properties. `always(x)`
is an invariant over the present binders with the name of the property, and it gets the
shortest trace BFS gives every invariant. `stable`, `unchanged`, and `precedes` are the
evaluators of #166, and `can_reach` is the backward pass of #167. The liveness routine is
the new part.

### Liveness by fair components

Fix a property with trigger `t` and goal `g`, a binding `b`, and the graph `G` of its cone.
Let `N` be the subgraph of the states where `g(b)` is false, with the edges of `G` that
stay inside it and are not self-loops. A counterexample is a behavior that reaches a state
of `N` where `t(b)` holds and then stays in `N` forever under the fairness of the mode.

A behavior that stays in a finite graph forever does one of two things:

- **It stops.** It repeats one state `s` forever. That is fair when no fair instance is
  enabled at `s`. In the mode with `WF_vars(Next)`, `s` must also have no out-edge to
  another state at all.
- **It cycles.** The states and edges it visits infinitely often lie in one strongly
  connected component `C` of `N`. Weak fairness is easiest to satisfy by visiting all of
  `C`, since more states give more chances to be disabled and more edges give more chances
  to take the step. So `C` admits a fair cycle exactly when `C` has an edge and every fair
  instance `a` either labels an edge inside `C` or is disabled at some state of `C`. A
  cycle takes steps forever, so `WF_vars(Next)` holds on it.

Enabledness is read on the out-edges of the whole graph `G`, because an instance whose
only step leaves `N` is still enabled. "Takes the step" is read inside `C`, because a step
that leaves `C` ends the cycle. Weak fairness needs no recursion into smaller components.
Strong fairness would, and that is one reason it stays out of scope.

The routine per binding:

1. Mark the states of `N` and compute its strongly connected components (Tarjan, iterative,
   on a compressed adjacency).
2. Collect the *fair ends*: every state that may stop, and every state of a component that
   admits a fair cycle.
3. Walk backwards inside `N` from the fair ends. A state the walk reaches *can avoid the
   goal forever*.
4. The property fails for `b` when some state where `t(b)` holds can avoid the goal
   forever. Every state of `G` is reachable, since BFS found it.

Two edges between the same states under different bindings both stay in the graph, so a
step that a fair instance and a non-fair action both explain counts for the fair instance.
That is the reading the cone's proof uses today.

**Cost.** For `V` states, `E` edges, `B` bindings (`slots ^ binders`, 2 or 4 at the default
bounds), and `F` fair instances, the routine takes `O(B * (V + E))` time plus one pass that
sets, per state, the bits of the fair instances enabled there. The bits fit one `u64` per
state for up to 64 fair instances, and a wider set uses a bit vector per state. Memory on
top of the run is 4 bytes per edge for each adjacency direction and about 16 bytes per
state for the component pass. The run itself already holds 12 bytes per edge. At the
`verify` cap of 8,388,608 states and the 4.3 to 6.2 edges per state measured below, a cone
has 36 to 52 million edges. The run holds 0.4 to 0.6 GB of edges, and both adjacencies with
the component arrays add 0.4 to 0.55 GB.

**Incomplete graphs.** A graph that stopped at the cap has states whose moves were not all
recorded. A state with no recorded move does not identify them, since the cap can stop a
state partway through its moves. BFS expands states in order, so every state from the
source of the last recorded edge onward counts as unexpanded. Such a state is never a fair
end, and a component that holds one is skipped, so a
counterexample found on an incomplete graph is real. An `ok` needs a complete graph. A
capped graph with no counterexample is `not decided`, as a capped BFS cone is today.

### Modes and the cone

A liveness property keeps the run structure it has today, with the same reasons, because
the reasons are about cones and not about TLC:

1. **Plain on the reduced cone.** Fairness is `Fair_P`. No counterexample is the whole
   model's `ok`.
2. **Plain on the unreduced cone**, when the reduced cone has a counterexample and the
   lasso cannot be lifted (see [Counterexamples](#counterexamples)).
3. **With `WF_vars(Next)` on the unreduced cone.** A counterexample is the whole model's.
   An `ok` is final when the cone holds every written var.

Plain and `WF_vars(Next)` on one cone share the graph and the component pass. They differ
only in which states may stop, so the third run costs one more backward walk and no
exploration.

What stays open after step 3 is one precise case: the plain run found only stops, each at a
cone state `c` where some non-fair step of the cone is enabled. Such a stop is a real
counterexample exactly when the whole model can keep changing vars outside the cone forever
while the cone stays at `c`. Today the whole model answers that, and on patent-ops,
food-permit-ops, and insurance-claim-ops the whole model does not finish
([Known verification gaps](../formal-verification.md#known-verification-gaps)). A native
port of the whole-model run would not finish either: those models pass the BFS cap. The
plan therefore answers the question on a cone, in three steps of rising cost.

All three steps need an *exact* cone: one that frees no slots and leaves no raised mark
out. On any other cone a step can be enabled on the cone and disabled in the whole model,
because a slot ran out or an event was never raised, and the var that disables it is
monotone, so step 4 would call the outside harmless. This model shows it. `Pay` moves the
order and creates a receipt, `Reissue` creates receipts and writes nothing else, and the
property is `leads_to(Order.status == pending, Order.status == paid)` with no `.fair`.
`verify` prints `Cone_PendingPays_WF: ok` on the 13-state cone, and the whole model has a
counterexample (39 states): `Reissue` takes the last receipt slot and a pending order can
never pay. `Receipt`'s existence only rises, so step 4 on that cone would answer `ok`. A
cone that is not exact is therefore widened first, with the existence of each entity its
actions create and with each raised mark its handlers read, and closed again. Steps 4 to 6
read the widened cone.

4. **Nothing outside can run forever.** A var is monotone when the writes of the model,
   taken as edges between its values, form no cycle (`monotone_vars`,
   `lifecycle/reduce/split.rs:31`). When every var outside the cone that some action writes
   is monotone, each of them changes finitely often, so no behavior changes only outside
   vars forever. Then every fair behavior of the whole model either keeps moving the cone,
   which the run with `WF_vars(Next)` covers, or ends with no step enabled, which that run
   covers too. Its `ok` is the whole model's `ok`. The check is a table lookup.
   `cone_keeps_every_write` is the case with no outside write at all.
5. **Something independent runs forever.** Suppose a var set `W` is disjoint from the
   cone, closed, exact as the cone is, and no action writes both `W` and the cone. Then the two systems interleave freely. If the graph of `W` has a cycle
   that changes a var, the whole model can run that cycle forever at any cone state, and
   the plain counterexample stands. `verify` already explores every entity cone, so the
   candidates and their graphs exist.
6. **The widened cone.** Otherwise close the cone over the non-monotone written vars
   outside it and run the mode with `WF_vars(Next)` there. Everything outside the widened
   cone is monotone, so step 4's argument makes that run exact. The widened cone is at
   most the whole model, and it stops at the same cap. Past the cap the property is `not
   decided`, as today.

The widening that makes a cone exact also removes the freed-slots case of today, where a
counterexample goes to the whole model. The widened cone counts the slots, so its
counterexample is real and no whole-model run is needed. `can_reach` needs the same step,
because a superset graph can show a path to the goal that the bounded model does not have,
which would make its `ok` unsound.

Steps 4 to 6 are new reasoning, not a port, so they carry the heaviest proof duty in
[How we know it is right](#how-we-know-it-is-right). The fixtures that today end as
`Why::WfNext` or `Why::FreesSlots` each match one step. The table is a prediction read from
the fixtures' sources and comments (`engine_equivalence.rs:2528-2722`), not a run. The
whole-model verdict is the one the fixture's comment states, or the one `verify` printed
for `order.rdra`:

| Fixture and property | Outside the cone | Step | Whole model |
|---|---|---|---|
| `order.rdra`, both properties | `delivered_at`, `null` to `present` only | 4 | `ok` (run) |
| `guard_reads_other`, `PaidShipsHalfFair` | `Order.note`, `false` to `true` only | 4 | `ok` (comment) |
| `independent_loop`, `PaidShipsUnfair` | `Ticket.status` opens and closes forever | 5 | counterexample (comment) |
| `quantifier_chain`, `OrderShips` and `OrderShipsPlain` | `Clerk.state` toggles forever | 5 | counterexample (comment) |
| `copy_parent`, `IssueEventuallyPlain` | `Permit.kind`, a copy that may write any value | 6 | not stated, the test's whole-model run decides |
| `frees_slots`, `PaidShips` | `Receipt` slots | widening by existence | not stated, the test's whole-model run decides |

`copy_parent` shows the limit of step 4. `Issue` runs once per permit, so `Permit.kind`
changes once, but the static rule sees an assignment from any value to any value and calls
the var cyclic. Step 6 then widens the cone to the whole model, which is small there.

### Reduction

The reduced cone is a divergence-blind stuttering bisimulation of the cone over the target
side and `obs`
([Reduction by an independent sub-lifecycle](../formal-verification.md#reduction-by-an-independent-sub-lifecycle)).
It preserves formulas of CTL\* without the next operator, so these run on the reduced graph
as they are: the state and step safety forms, `can_reach`, and the plain liveness run,
whose fair actions are on the target side. The run with `WF_vars(Next)` does not survive
the reduction, because a class that loops inside itself looks like a stop. It stays on the
unreduced cone.

The native checker reads the reduced graph through the same `Graph` type. A class move is
an edge like any other, and it is never a fair instance.

### Counterexamples

A state safety violation is the BFS path to the state, as for an invariant. A step safety
violation is the BFS path to the source and the step. A `can_reach` violation is the BFS
path to a state with no path to the goal, with the set #167 defines.

A liveness counterexample is a lasso, built in three parts:

1. **Prefix.** Among the triggered states that can avoid the goal forever, take the one BFS
   found first, and the BFS path to it.
2. **Bridge.** A shortest path inside `N` from that state to the nearest fair end.
3. **Loop.** For a stop, nothing. For a component, one witness per fair instance: an edge
   inside the component with the instance's label, or a state where the instance is
   disabled. The loop visits the witnesses in the order of first discovery by shortest
   paths inside the component and returns to its first state.

The result goes through `replay_lasso` and `explain_property` unchanged, so the message,
the numbered steps, and the ending line are the ones TLC's lassos get today. Three things
improve. The lasso is the same on every run, where TLC's differs between runs. The head no
longer says `from TLC`. The `violated: Error: Temporal properties were violated.` line and
TLC's own trace appear only when TLC ran.

**A lasso certifies itself.** `fails_at` checks that the lasso violates the formula. A new
function beside it checks the fairness of the loop: for each fair instance, the loop takes
its step or passes a state where it is disabled, and in the mode with `WF_vars(Next)` a
stop has no step to another state. The check reads `moves` at each state of the lasso, not
the graph, so it does not share code with the search. A native counterexample that fails
either check is a tool bug and prints the engine-disagreement line that exists today.

**Lifting a reduced lasso.** The prefix and the bridge lift with `lift`
(`lifecycle/reduce/lift.rs:18`), as traces do. The loop lifts to a path that returns to
the same class and maybe to another concrete state of the sub-lifecycle. Lifting the loop
again from where the last copy ended, until a concrete state repeats, gives a lasso of the
unreduced cone without exploring it. The number of copies is at most the size of the
largest class. This replaces the second run for a property whose unreduced cone passes the
cap, which today has no printable counterexample. It is its own phase, since the first
phases can keep the second run.

## How we know it is right

**Counterexamples check themselves, so only `ok` needs an oracle.** Every native lasso
replays through `moves` and passes the formula check and the fairness check before it
prints. A wrong counterexample cannot reach the user as a verdict. A wrong `ok` can, so
every comparison below is about `ok`.

**Per run, against TLC.** `a_property_cone_decides_what_the_whole_model_decides` gains the
native verdict of each run it makes: plain reduced, plain unreduced, with `WF_vars(Next)`,
and whole model. The four must equal TLC's four on every case, not only the final verdict.
A final verdict can agree while two runs disagree in ways that cancel.

**The cone ladder, against the whole model.** For steps 4 to 6 and the widening by
existence, the oracle is TLC on the whole model, which the test already runs for every
case. Each fixture in the table above must be decided by its step with the whole model's
verdict, and the list of undecided properties in `TEMPORAL_CASES` becomes empty. New
fixtures cover what the table does not: the receipt model above, where a cone that frees
slots passes with `WF_vars(Next)` and the whole model fails, the same shape with a handler
whose raised mark the cone leaves out, an outside var that is monotone only because no
delete resets it, a candidate `W` that shares a writer with the cone, a candidate `W` that
drops a mark, and a widened cone that is the whole model.

**Random models.** `scripts/reduction-diff.py` gains a mode that runs each seed with
`verify --with-tlc` and records every check on which the two engines differ, beside what
it compares today between the reduced and the unreduced run: the exit code, the finding
names, the verdict of each check, and the state-model summary. The
generator already emits `leads_to` and `eventually` with and without `.fair`. It gains
`always`, the forms of #166 and #167, `.along` on a two-entity property, a fair use case
that creates, and a fair use case split over two APIs. A second mode, at 2 instances on
models small enough for a whole-model TLC run, compares every native verdict with the
whole model, so that steps 4 to 6 are tested where no fixture was written for them.

**Samples.** `scripts/verify-samples.sh` runs every root as its header says, and CI runs
every root once more with `--with-tlc`, which fails on a check the engines decide
differently.

**When the engines disagree.** In a test, the case fails, the model is copied out, and
`minimize` shrinks it, as `reduction-diff.py` does today. In a run with `--with-tlc`,
`verify` prints the engine-disagreement line with both verdicts, keeps the spec directory,
and exits 1. Neither verdict is reported as the result. A disagreement is a bug in the
tool, never a finding about the model. A property that the native checker decides and TLC
leaves `not decided` at its budget is not a disagreement, since TLC gave no verdict.

**What the tests do not show.** Agreement is tested at 1 to 3 instances on models TLC can
finish. A native verdict on a cone TLC cannot finish has no oracle, as a BFS finding on a
cone in `VERIFY_FALLBACK` has none at 2 instances today.

## Measurements

One run each, on the machine named above. The probe was a throwaway example that built
each property cone as `lifecycle_measure` does, ran BFS, built a forward adjacency without
self-loops, and ran an iterative Tarjan pass over the whole graph. It measures the cost of
the graph work, not a finished checker: it evaluates no predicate and no fairness. The
whole graph is an upper bound for the subgraph `N` of one binding. The reduced rows include
the time to explore and quotient the sub-lifecycle, which is why 372 states take 1.2 s.

| Property cone | States | Edges | Self-loops | BFS | Adjacency | Components |
|---|---:|---:|---:|---:|---:|---:|
| patent-ops `GrantEnds`, reduced | 75,017 | 321,548 | 23,456 | 42 ms | under 1 ms | under 1 ms |
| patent-ops `GrantEnds`, unreduced | 553,643 | 3,447,460 | 1,007,716 | 531 ms | 8 ms | 8 ms |
| food-permit-ops `HygieneManagerEventuallyAppointed` | 243 | 929 | 132 | under 1 ms | under 1 ms | under 1 ms |
| insurance-claim-ops `ClaimEventuallyDecided`, reduced | 633,787 | 3,192,700 | 327,328 | 1,049 ms | 8 ms | 8 ms |
| insurance-claim-ops `SubrogationEnds`, reduced | 372 | 2,525 | 0 | 1,209 ms | under 1 ms | under 1 ms |
| insurance-claim-ops, either property, unreduced | over 3,000,000 | over 13,800,000 | | over 5 s | | |

The probe ran all eight inputs in 13.6 s at a peak of 1.87 GB, most of it the two unreduced
insurance cones that stopped at the probe's cap of 3,000,000 states.

**TLC alone on the largest property cone.** `verify -o` wrote
`Cone_ClaimEventuallyDecided`, the reduced cone of 633,787 states, and `tlc` ran on that
module by itself with a 4 GB heap. TLC generated 3,131,413 states and found 633,787
distinct ones, the count BFS reaches:

| Engine | Wall time | Peak RSS |
|---|---:|---:|
| TLC, `-workers 1` | 75.5 s | 1.80 GB |
| TLC, `-workers auto` (18 cores) | 22.7 s | 2.33 GB |
| BFS on the same cone, sub-lifecycle included, plus the component pass | 1.06 s | not measured alone |

The property holds, so TLC's time is the search of the behavior graph and not the
construction of a lasso. The native row is the probe's, which decides nothing yet. The
comparison bounds what the routine can cost, not what a finished checker takes.

What the numbers say:

- **The graph work is not the cost.** The component pass takes 8 ms where BFS takes one
  second. A liveness check costs about what one more BFS finding costs.
- **Self-loops are common.** Up to 29% of the edges of a cone are self-loops (1,007,716 of
  3,447,460 on the unreduced `GrantEnds` cone). A checker that counted a self-loop as a
  step would report cycles that are stops, and one that counted it as enabled would force
  steps that do not exist. The rule in [The property fragment](#the-property-fragment) is
  load-bearing.
- **The unreduced insurance cones are out of reach.** They pass 3,000,000 states, and
  [the documentation](../formal-verification.md#approximations-and-limitations) records
  over 9,000,000. A counterexample of `ClaimEventuallyDecided` has no second run that
  finishes, with TLC or natively. That is the case for lifting the reduced lasso.
- **`GrantEnds` frees slots.** The patent-ops cone reports `cone_frees_slots`, so a
  counterexample there goes to the whole model today, which does not finish. The widening
  by existence is what would decide it.

**Every cone on BFS.** `RDRA_REDUCED_ONLY=1 lifecycle_measure -- 9000000 verify <sample>`
explores each entity view, invariant cone, `after` cone, and property cone reduced, as
`verify` would. The middle column is the sum of the per-cone times it prints for the
invariants, the `after` assertions, and the properties, with a shared cone counted once.
The first column counts checks, which share cones: `verify` runs 11 cones for the 13
invariants of patent-ops.

| Sample | Invariants and `after` assertions | BFS time of their cones and the property cones | Largest | Whole measurement, entity views included | Peak RSS |
|---|---:|---:|---:|---:|---:|
| clinic-ops | 13 | 0.1 s | 85,154 | 8.7 s | 0.88 GB |
| patent-ops | 13 | 1.2 s | 1,341,707 | 2.7 s | 0.37 GB |
| food-permit-ops | 16 | 0.1 s | 23,034 | 7.8 s | 1.22 GB |
| insurance-claim-ops | 24 | 10.5 s | 783,427 | 80.6 s | 3.42 GB |

`verify` already pays for the entity views, so the middle column is what phase 1 adds. The
insurance row leaves out `MultiForbidden_Claim_8`, the `.lemma()` rule: its cone passes
9,000,000 states after 56.5 s, and `verify` runs no cone for a lemma today. That one cone
is most of the 80.6 s and of the 3.42 GB. The native backend must keep skipping lemma
cones.

## Migration

The owner decided the shape on 2026-11-23. The native checker is the default of `verify`,
and `--with-tlc` adds TLC. Each phase, the change of default included, lands on `main` as
soon as its exit check passes. The next tagged release waits until the native checker is
stable.

**`main` is the early channel.** A user who wants the native checker before the release
installs from `main`, for example with `nix profile install github:manji-0/rdra-ish-dsl`,
which the README documents. The promises of [compatibility](../compatibility.md) cover
releases, not `main`: between two releases `main` may change a default, an output line, or
a verdict, and each such change is in the `Unreleased` section of the changelog. What a
user of `main` gets after each phase:

| After phase | `verify` on `main` |
|---|---|
| 0 | As released. Nothing a user sees changes. |
| 1 and 2 | As released. `verify` still needs TLC, and the native path runs only under a test hook. |
| 3 | Native by default, with no JVM. The verdicts are those of today's run structure, so a property the cone leaves open still ends as `not decided` where the whole model is too large. `--with-tlc` runs TLC beside it. The verdict line, the `// expect` grammar, and the sample headers are the new ones. |
| 4 | A property that ended as `not decided` gets a verdict on a cone. A run that passed with a `not decided` property can now fail with its counterexample. |
| 5 | A counterexample on a reduced cone whose unreduced cone passes the cap prints, lifted. Lassos on reduced cones may differ from those of phase 3. |
| 6 | The same tool, tagged as a release, with the contract of `compatibility.md` in force again. |

**Before the default changes.** Phases 1 and 2 build the native path while `verify` still
runs TLC. A test hook selects it, `RDRA_ISH_NATIVE`, which is not a setting, as
`RDRA_ISH_NO_REDUCE` is not one. No flag is added for a state that lasts two phases.

**Flags after the default changes.**

| Flag | Meaning |
|---|---|
| `verify` | The native checker decides every cone and every property. No JVM starts. |
| `verify --with-tlc` | The native checker decides, TLC runs the modules it runs today, and the run fails with the engine-disagreement line when a check has two verdicts. A missing `tlc` is exit 1, as today. |
| `--prove`, `--deny-unproved`, `--z3` | Unchanged. The proof reads the state rules and uses z3, with or without `--with-tlc`. |
| `--tlc-jar`, `RDRA_TLC_HEAP` | Apply only with `--with-tlc`. |
| `--temporal-budget` | Bounds the widened cone of step 6 in seconds, and TLC's whole-model run under `--with-tlc`. |
| `-o <dir>` | Still writes the modules and `cones.txt`, for a TLC run by hand. |
| `--backend tlc` | A deprecated spelling of `--with-tlc` that prints a warning, removed in the next major release. |

**Output.** A native run prints the lines `verify` prints today, with these differences.
The cone names stay `Cone_<name>` as labels, so a script that reads the lines keeps
working. The second and third runs keep their notes (`Cone_P_WF: ok`). Steps 4 to 6 add a
note that says why the cone decided (`nothing outside the cone runs forever`, `<Entity>
runs forever outside the cone`, `on the cone widened by N vars`). The `tlc_result:` line
prints only when TLC ran, which is under `--with-tlc`. Every run prints the verdict on a
`result:` line of the same form (see [Decisions](#decisions)).

**`compatibility.md` changes in the change that moves a covered item.** The page states
what the next release keeps, so it is edited where the behavior moves and not at the
release. The change of phase 3 edits the page, the changelog, and every sample header
together. The covered items that move:

- **The command line.** The default of `verify`, the new `--with-tlc`, and the fate of
  `--backend`. "`verify` without `tlc`" stops being an example of exit status 1 and
  becomes "`verify --with-tlc` without `tlc`".
- **The `tlc_result:` line.** Its form stays parseable, and the page says that it prints
  under `--with-tlc` only.
- **`// expect` headers.** A `verify` line without flags no longer expects a `tlc_result:`
  line. The grammar in
  [Sample expectations](../formal-verification.md#sample-expectations) changes with it, and
  all 32 roots are rewritten in the same change.
- **Meaning.** A property that ended as `not decided` (exit 0) can become a counterexample
  (exit 1) at the same bounds, once steps 4 to 6 land. Each change of phase 4 that moves a
  sample's verdict adds the property to the `### Breaking` entry and says how to see the
  case in a model.
- **Not covered, and changing.** The message wording, the state counts, and the lasso TLC
  would have picked.

These are breaks of covered items, so by the rule on that page the release that carries
them is a major release. That follows from the contract. It does not gate the work.

**The release gate: what "stable" means.** No release is tagged between the change of
default and the day all of these hold at one commit of `main`. The criteria are this
plan's proposal:

1. **Every phase has passed.** Phases 0 to 5 have passed their exit checks, or the owner
   has moved a phase out of the release in [Decisions](#decisions).
2. **The samples agree.** `verify --with-tlc` on every root of `samples.txt` reports no
   disagreement, and the native run alone matches every `// expect` header.
3. **Random models agree.** `reduction-diff.py` in its `--with-tlc` mode and in its
   whole-model mode finds no disagreement on seeds `0:2000` at 2 and at 3 instances, with
   no seed that only one side finished. A sweep of 500 seeds that no earlier run used
   finds none either, so the fixed seeds are not the only evidence.
4. **No wrong verdict is open.** Every disagreement found since the default changed is
   closed with a fixture in `engine_equivalence` that fails without its fix. No open issue
   reports a native verdict as wrong, whether a test or a user of `main` found it.
5. **No counterexample failed its own check.** No run of items 2 and 3 printed the tool-bug
   line for a lasso that does not replay or does not satisfy its fairness.
6. **Nothing got worse.** No sample check that `verify` decided with TLC is `not decided`
   natively. Each domain sample's native `verify` takes no more wall time and no more peak
   memory than the TLC run in [What exists today](#what-exists-today), measured on the
   same machine.
7. **A quiet period without a fix to a verdict.** No commit on `main` changed a native
   verdict on a sample, a fixture, or a sweep seed, for as long as the owner judges enough.
8. **The contract is written.** `compatibility.md`, the changelog, the docs, and the
   skills below describe the tool at that commit.

Item 7 is a waiting time and the others are checks. The owner ends the wait when they
judge it long enough; it has no fixed number of days.

**CI.** Before the default changes, the `formal-verify` job runs the samples as today and
once more under `RDRA_ISH_NATIVE`. After it, the step that checks every sample against its
header runs without a JVM, and a second step runs the samples with `--with-tlc`.
`engine_equivalence` keeps `RDRA_REQUIRE_TLC=1`, and the sweep runs its `--with-tlc` mode
on the fixed seeds. Once the release gate passes, the step with `--with-tlc` moves to `release.yml`
and stops running on `main`.

**Docs that change.** The documents must say in plain words who checks what:

- **`docs/formal-verification.md`.** The title loses "TLC". The page opens with the split:
  `check` and `states` verify safety by state exploration, `verify` checks every `property`
  natively and proves state rules with `--prove`, and TLC is `--with-tlc`. The "Temporal
  properties are checked only by `verify`" gap becomes that statement and stops being a
  gap. Every `verify ... --backend tlc` transcript is rerun.
- **`docs/state-derivation.md`.** The `property` row of
  [Checks](../state-derivation.md#checks) lists all seven forms and says that BFS in
  `check` and `states` does not evaluate them and that `verify` does.
- **`docs/language-reference.md`.**
  [Temporal path properties](../language-reference.md#temporal-path-properties) says once,
  for every form, that a `property` is checked by `verify` and that a rule for `check` is a
  `forbidden`, an `invariant`, or an `after` assertion.
- **`docs/cli-reference.md`.** The `verify` flags, the `check` note, and the exit statuses.
- **`docs/vision.md`.** A dated entry under Decisions for the three decisions, and
  "Verification is bounded, fails closed, and points at the model" says which command
  checks which declaration.
- **`docs/compatibility.md`.** As above, and one sentence that the page covers releases
  and that `main` between releases is the `Unreleased` section of the changelog.
- **`README.md`.** Beside the install from `main`, a line that `main` holds the native
  checker before its release and that its output may still change.
- **The `check` note.** `crates/rdra-ish-cli/src/lib.rs:117` says `use verify --backend
  tlc`. It says `use verify`.
- **`skills/rdra-ish-verify`.** It stops telling the reader to install TLC first and
  presents `--with-tlc` as the second opinion.

## Phases

Each phase ends in a state that a test proves. A phase starts after the one before it has
passed its exit check, and it lands on `main` when its own exit check passes. No phase has
`check` or `states` evaluate a `property`.

**Phase 0: shared graph helpers and the measurement.** Move `Adjacency` out of
`findings.rs` to a module the property checks share, with both directions and without
self-loops. Add the edge count, the self-loop count, and the component time to
`lifecycle_measure`. No behavior changes. Exit: `cargo test` passes, and `lifecycle_measure`
reproduces the table in [Measurements](#measurements) within the run-to-run spread.

**Phase 1: safety cones on BFS.** Under the test hook, `verify` decides each invariant
cone and each `after` cone by the explorer, at the bounds of `verify`, reduced as today.
`always` joins the invariants of a `verify` run. `stable`, `unchanged`, `precedes`, and
`can_reach` need only the dispatch, since #166 and #167 bring their evaluators. `verify`
keeps skipping the cone of a `.lemma()` rule. Exit: under the hook, `verify-samples.sh`
reports the exit status and error names that the TLC run reports on every root that has no
`eventually` or `leads_to`, `check` reports for every sample what it reported before, and
`engine_equivalence` is unchanged.

**Phase 2: the liveness routine.** Add the fair-component routine to `rdra-ish-core` with
the two modes, the lasso builder, and the fairness check of a lasso. It reads a finished
run and returns a verdict with a lasso. `verify` does not call it yet. Exit: in
`a_property_cone_decides_what_the_whole_model_decides`, the native verdict of each run
equals TLC's on every case, and every native lasso replays and passes both checks.

**Phase 3: the native checker becomes the default.** Wire the liveness routine into
`verify` with the run structure of today, the whole-model run included, on BFS under the
budget and the cap. Make the native path the default, add `--with-tlc`, remove the test
hook, and in the same change edit `compatibility.md`, the changelog, the sample headers,
and the docs listed under [Migration](#migration). Exit: `verify-samples.sh` passes with
no `tlc` on `PATH`, `verify --with-tlc` reports no disagreement on any root, and
`reduction-diff.py` in its `--with-tlc` mode finds none on seeds `0:500` at a 120 s
timeout. From this phase on, no release is tagged until the gate passes.

**Phase 4: decide on a cone what the whole model decided.** Add the widening that makes a
cone exact, by existence and by raised mark, then steps 4, 5, and 6, each in its own
change with its fixtures. Each change that moves a sample's verdict adds the property to
the `### Breaking` entry. The same widening by raised mark also serves the safety side
([#206](https://github.com/manji-0/rdra-ish-dsl/issues/206)): a counterexample whose trace
runs a handler without its event having been raised replays on the cone widened with the
left-out marks and their raisers. A replay that succeeds prints the counterexample without
the `note:` row; one that fails reruns that check alone on the widened cone, under the same
memory limit, and keeps the note if the rerun is undecided. Exit: the undecided list of
`TEMPORAL_CASES` is empty with every verdict equal to the whole model's, the whole-model
mode of `reduction-diff.py` finds no difference on seeds `0:500`, `order.rdra` prints no
`RdraSpec_` line, and a corpus case whose only cone counterexample runs a handler before its
raiser reports `ok`, beside a twin whose violation is real either way.

**Phase 5: lift a reduced lasso.** Replace the second run by the lift where the unreduced
cone passes the cap, then everywhere if the lifted lassos are no longer than the explored
ones on the samples. Exit: on `reduced_gate`, whose two counterexamples the test reads on
the second run today, the lifted lasso replays on the unreduced cone and passes both checks
with no exploration of that cone.

**Phase 6: the release gate.** No code. Run the eight checks of the release gate in
[Migration](#migration) at one commit and record the result in the changelog entry. Exit:
all eight hold, and the release is tagged at that commit.

## What #165 to #167 can do to make this cheaper

The three issues are in review. Each of these choices costs little there and saves a
rewrite here. The first one is not a choice any more.

- **#166 and #167: `verify` evaluates the new forms, and `check` does not.** Both issues
  say that `check` and `states` report their forms. The owner decided on 2026-11-23 that
  `check` and `states` evaluate no `property`. The evaluators stay where the issues put
  them, in the explorer, and only a run that `verify` starts receives the properties.
  `check` prints its note for these forms as for `always`. The TLC formula of `stable` and
  `unchanged` and the latch of `precedes` stay, because the tests compare them with TLC.
  `--with-tlc` started no TLC run for these forms until #201, which added one per
  property on its cone (`verify/jobs.rs`, `explored_jobs`).

- **#165: keep the link as its own field.** Store the `.along` link as one `Pred` on
  `Property`, beside the formula, and not inside the operands. The printer, `fails_at`, the
  cone, and the native checker then share one definition of the trigger, `present /\
  link`. A link folded into each operand would have to be recognized again to keep the goal
  free of it.
- **#165: say when the link is read.** The issue fixes the trigger. State that the goal
  does not read the link, so a key that moves after the trigger does not change the
  obligation. `fails_at` and the printer should have a test for a moved key.
- **#166: make the form's class a total function.** Replace or extend `Ltl` with one enum
  of the seven forms and a method that returns its class: state safety, step safety,
  liveness, or branching. `verify`, the cone, and the printer then dispatch with one
  `match`, and a new form fails to compile until each of them handles it.
- **#166: keep the latch in the property's cone only.** The latch of `precedes` is a var,
  so a step that only sets it changes the state. Give it to the cone of its property and
  to no other cone, as a raised mark belongs to the cones that read it. A latch in another
  property's `vars` would turn a stop of that property into a step and change which stops
  `WF_vars(Next)` allows. The latch only rises, so it is monotone and step 4 treats it as
  harmless outside a cone.
- **#166: check steps on the graph's edges.** If `stable` and `unchanged` are evaluated per
  edge with the binding in hand, as `check_after` is, the native `verify` path needs no
  second evaluator.
- **#167: share the backward walk.** `can_reach` needs the reverse adjacency that the
  deadlock finding builds. Putting it in the shared module of phase 0 gives the liveness
  routine the same structure.
- **#167: refuse a cone that frees slots.** On a superset graph, a path to the goal may use
  more instances than the bound, so an `ok` is unsound there, not only a violation. Until
  the widening by existence exists, `can_reach` on such a cone should be `not decided`.
- **#167: return a verdict type the other forms can use.** A result of "ok, counterexample
  with a witness, or not decided with a reason" that is not tied to TLC's `Outcome` lets
  every form report through one path, and gives the `verify` line for a natively decided
  property one format from the start.

## Risks

- **A wrong `ok` from the cone ladder.** Steps 4 to 6 rest on arguments about what runs
  outside a cone. A gap there is silent, and the first draft of this plan had one: it
  applied step 4 to a cone that frees slots, which the receipt model refutes. Mitigation: each step lands alone, behind the
  whole-model oracle on fixtures and random models, `--with-tlc` stays available in every
  run, and the release gate keeps a wrong verdict found on `main` out of a release.
- **Users of `main` meet verdicts that still move.** From phase 3 on, `main` gives native
  verdicts, and phases 4 and 5 change some of them. The owner chose that: `main` is the
  early channel. Mitigation: phase 3 keeps today's run structure, whose verdicts are
  TLC's, every later move is in the `Unreleased` changelog, and the README says what
  `main` is.
- **Semantic drift between the printer and the checker.** Two definitions of fairness can
  diverge when one changes. Mitigation: `engine_equivalence` compares per run in CI, and
  the fair instance is derived from `ParamRole` in one function that both use.
- **Memory at the cap.** A cone at 8,388,608 states holds its edges and two adjacencies.
  The estimate above is about 1 GB beside the states. Mitigation: phase 0 measures it on
  the largest sample cones before phase 2 starts, and the liveness routine can build the
  reverse adjacency only for the subgraph `N`.
- **Counterexample length.** A loop that visits one witness per fair instance can be longer
  than TLC's. Mitigation: phase 2 records the lasso lengths of both engines on the
  fixtures. A length that hurts reading gets a shorter cover, which changes no verdict.
- **A widened cone as large as the whole model.** Step 6 then decides nothing new and costs
  an exploration up to the cap. Mitigation: the run estimates the widened cone's var count
  first and skips to `not decided` with the `.fair` hint when the cone is the whole model.
  [Phase 4 runs step 6 in place of the whole-model run instead, under its budget and cap,
  so the whole model is explored at most once, as before; a run that stops gives the
  hint.]
- **Two engines to keep.** TLC stays in the tests, so the TLA+ printer, the decoder, and
  the replay stay maintained. That is the price of the oracle, and it is the price paid
  today.

## Decisions

The owner decided these questions on 2026-11-23:

- **`verify` defaults to the native checker, and `--with-tlc` adds TLC.** With the flag,
  TLC runs beside the native checker and the run fails when the two disagree. The plan had
  proposed an opt-in `--backend native` and a `--cross-check tlc`. Both are gone. The z3
  options (`--prove`, `--deny-unproved`, `--z3`) stay as they are.
- **Each phase lands on `main` when it passes, and no release ships until the native
  checker is stable.** The change of default is not tied to a major release. It lands with
  phase 3, with `compatibility.md` edited in the same change. `main` is the channel for a
  user who wants the unreleased native checker, and the promises of `compatibility.md`
  cover releases, not `main`. The next tagged release waits for the release gate in
  [Migration](#migration). The eight criteria there are this plan's proposal for what
  "stable" means, and the owner has not confirmed them.
- **`check` and `states` do safety verification by state exploration only.** Every
  `property` form, the forms of #166 and #167 included, and every inductive proof belong to
  `verify`, and the docs say so. This closes the question of whether `check` evaluates
  `always`, `eventually`, and `leads_to`, and it changes what #166 and #167 say about
  `check`.

The owner decided four more on 2026-11-23:

- **`--backend tlc` stays as a deprecated spelling of `--with-tlc`.** It prints a warning
  and runs TLC beside the native checker, so a script that passes it keeps getting what it
  asked for. The major release after the one that changes the default removes it.
- **Every run states its verdict on a neutral `result:` line.** The line has the form
  `tlc_result:` has today. Under `--with-tlc`, `tlc_result:` prints beside it. An
  `// expect verify:` header reads `ok` or `fail <Names>`, and `tlc=` stays for a line
  whose flags hold `--with-tlc`.
- **The native checker may decide what today is `not decided`.** A run that passes today
  with a `not decided` property can fail with its counterexample at the same bounds. Each
  change of phase 4 that moves a sample's verdict names the property in the `### Breaking`
  entry.
- **`result:` states the verdict of the whole run.** It counts the state-model findings
  (a deadlock, a use case that never runs) beside the invariants, the `after` assertions,
  and the properties, so `result: ok` means exit 0. Phase 3 had it count only the checks,
  which let `result: ok` stand beside a failing run. Decided on 2026-11-23, before any
  release carries the line.
- **The release gate keeps its eight criteria, with a shorter wait.** Item 7 waits fewer
  than 14 days.

The owner decided four more on 2026-11-23, closing the questions this plan left open:

- **The owner ends the wait of item 7.** It has no fixed number of days: the quiet period
  lasts until the owner judges it enough.
- **Step 5 stays for now.** Phase 4 measures how often step 5 and step 6 fire on the
  samples and the sweep, and the owner decides from those numbers whether step 5 stays.
- **Try a conservative divergence before deciding the reduction.** The run with
  `WF_vars(Next)` runs on the reduced cone with every class whose sub-lifecycle can loop
  taken as able to loop inside itself. Taking more classes as able to loop admits more
  fair behaviors, so an `ok` there is final. A counterexample there may not exist in the
  cone: it is lifted and must pass the replay, the formula, and the fairness checks, and
  one that fails them falls back to the unreduced cone as today. The partition and its
  proof stay as they are. The trial writes the argument that the `ok` is sound first, adds
  corpus cases whose only counterexample loops inside one class, and then measures how many
  of the samples' runs with `WF_vars(Next)` it decides. A divergence-sensitive partition,
  which splits classes by whether they loop, is considered only if the trial falls short.
- **How the trial's numbers choose between C, B, and A.** All numbers compare with the
  `main` commit the trial branched from. "Undecided for memory" counts the runs with
  `WF_vars(Next)` that are `not decided` because the unreduced cone hit the memory limit.
  - **Before any choice, C must be sound.** C is dropped if the argument fails or needs a
    condition that keeps it off the three large domain samples, if C says `ok` where TLC
    finds a counterexample on a sample or a sweep seed (a fixable bug is fixed with a
    fixture and the trial is measured again), or if native, TLC, and z3 disagree on the
    corpus cases the trial adds. A spurious counterexample is not a soundness failure: the
    lift and the checks reject it and the run falls back.
  - **C is chosen when all three hold.** On patent-ops, food-permit-ops, and
    insurance-claim-ops, C decides at least half of the runs undecided for memory. At most
    one in five of the runs C enters falls back on a spurious counterexample. No sample's
    wall time or peak memory grows by more than 10% over `main`.
  - **B is considered when C misses one of those** and most of what C leaves undecided is
    fallbacks, or when C's argument needs a condition that keeps it off the large samples;
    and a divergence-sensitive split would keep the reduction: at most twice today's
    classes, estimated from the classes that mix states that can loop inside the class
    with states that cannot, and a reduced cone that fits the memory limit.
  - **A is chosen otherwise:** when neither C nor B qualifies, when the three large samples
    have two or fewer runs undecided for memory in all, or when what stays undecided comes
    from the whole-model step that phase 4 replaces, in which case phase 4 goes first and
    the trial is measured again after it.
- **C is adopted (2026-11-24).** The trial (#227) held its argument with no extra
  condition; native and TLC agreed on every corpus case, every sample run, and every sweep
  seed C decided, at 2 and at 3 instances; 1 of 21 sweep runs C entered fell back on a
  spurious counterexample; and insurance-claim-ops' wall time and peak memory did not grow
  (medians 11.57 s and 2,055 MB against main's 12.27 s and 2,083 MB, 5 runs each). The
  committed samples have no run with `WF_vars(Next)` left undecided for memory, so by the
  letter of the criteria above the choice was A. The owner chose C on the variants without
  `.fair`, where C decided both of insurance-claim-ops' memory-undecided runs at 3.4 times
  less time and 6.5 times less memory, and because phase 4 will turn that saving into
  final verdicts. B is not needed: a divergence-sensitive split would add about 2% classes.
  Two gaps stay open: an `ok` on a cone with looping classes is untested at 3 instances,
  and the unobserved-sub-lifecycle clause is covered by a unit test only, since no
  property reaches it today.
- **The samples run with `--with-tlc` on `main` until the release gate passes, and then
  only at release.** While phases 4 and 5 and the divergence trial change native verdicts,
  that step is the only one that compares the samples' temporal verdicts with TLC. Once the
  gate passes, the step moves from the `main` CI to `release.yml`, where it gates every
  release. The step that checks the headers without a JVM stays on `main`, and so do the
  random-model sweep with `--with-tlc`, `engine_equivalence`, and the adversarial corpus.
