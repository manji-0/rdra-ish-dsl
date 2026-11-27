# Cone soundness: why a cone, a reduced cone, and its `_WF` run decide what the model decides

<!-- constrained-by ../state-derivation.md#exploration-by-cone-of-influence -->

Status (2026-11-26): describes the code on `main`. The text moved here from
[formal-verification.md](../formal-verification.md) when that guide was cut down for the
next major release. Only its cross-references changed, and the definition of the classes
that can loop stayed in the guide, which keeps what a modeler needs and links here for the
arguments. The reduction landed in #74 and gained monotone inputs and folds in #112; the
`_WF` run on a reduced cone landed with the native checker
([native temporal verification](./native-temporal-verification.md)).

## A property's cone decides under its own fairness

A property's cone (`TransitionSystem::property_cone`) is the smallest closed var set that
holds what the formula reads, the owner vars of its `.along(...)` link, and, for each
action of each use case its `.fair` lists, everything the action reads and writes: its
guards with the `applies_to` conditions and the
quantifier reads inside them, the existence of each instance it binds, the source of each
transition, the clock when it ticks, the columns it assigns, transitions, or chooses, and the
existence of each entity it creates or deletes. Closed means what it means for an invariant's
cone: every action that writes a var of the cone reads only vars of the cone, with two
exceptions. A copy guard (`sets(UC, Child.col == Parent.col)`) whose target column is
outside the cone is dropped with the column, and a creation of an entity outside the cone
is dropped with its slot (the case `TransitionSystem::cone_frees_slots` detects and
`property_cone.rs` reports as `Why::FreesSlots`; see the guide's
[Temporal properties on a cone](../formal-verification.md#temporal-properties-on-a-cone)). The cone decides the property's
verdict under the property's own fairness because, for a fair action `A`, whether `A(p)` is
enabled in the whole model is a function of the cone's vars, every step of `A(p)` that
changes any var changes a var of the cone, and so `WF_vars(A(p))` on the cone's `vars` is
`WF_vars(A(p))` on the whole model's; every other action either writes into the cone, and
then reads only the cone apart from those two exceptions, or leaves the cone unchanged,
which is a stuttering step of the cone's `[Next]_vars`. Every behavior of the whole model
that satisfies `Fair_P` therefore projects onto a behavior of the cone that satisfies
`Fair_P`. A behavior of the cone lifts to one of the whole model on three facts:

- A dropped copy guard can hold in the whole model whatever its source holds: `copyable`
  accepts a copy only between columns that hold the same values, with a nullable source
  only into a nullable target, and rejects any other copy with `SetsCopyMismatch`, so the
  target can take every source value. The step lifts with the target set to the source's
  value. A copy from an optional parent keeps its `Owner` guard and its `Parent` param in
  the cone, so the cone still needs a linked parent where the whole model does.
- A cone step that both a fair `A(p)` and a non-fair `B` explain lifts as `A(p)`, so the
  lifted behavior takes the fair step wherever `WF_vars(A(p))` needs one.
- An `always` counterexample the first run finds without `WF_vars(Next)` extends to a
  behavior that satisfies the whole model's `WF_vars(Next)`: `WF_vars(Next)` and `Fair_P`
  are weak fairness on actions of `Next`, which is machine closed, so every finite prefix
  extends to a behavior that satisfies both, and an `always` is violated by the prefix
  alone.

The lift needs the cone to count the slots of every entity its actions create; a cone that
frees slots (`cone_frees_slots`) is the one where it does not.

What does not restrict to a cone is the `WF_vars(Next)` that `Spec` adds for any `eventually`
or `leads_to`: a step outside the cone satisfies it, so the cone with `WF_vars(Next)` of its
own would rule out runs the whole model allows (another entity looping forever while the
property's instance stands still), and the cone without it admits a stutter the whole model's
`WF_vars(Next)` rules out. `verify` therefore gives such a property up to two runs on the
cone, and the whole model only when neither decides. The native checker runs each one on
the graph BFS explores: a behavior that avoids the goal forever either stops at a state
where the fairness lets it stop or cycles through a strongly connected set of goal-free
states that the fairness admits, and the property fails when a triggered state reaches
such an end. The two runs on the unreduced cone read one graph. Each counterexample
replays through the step relation, fails the property, and passes the fairness check
before it prints; one that does not is a tool bug and prints `error[LassoFault]`. The
guide lists the runs.

## The reduction by an independent sub-lifecycle

A cone often holds a second lifecycle that its targets never write and read only in part: the
claim's cone in insurance-claim-ops holds the case's settlement, the charge's cone in
clinic-ops holds the appointment and the encounter. Every state of that lifecycle interleaves
with every state of the target's, and an exact cone cannot be smaller than that product. Since
#74 `verify` explores and prints each cone reduced by such a sub-lifecycle
(`TransitionSystem::reduced`), and the reduced cone decides what the cone decides. `check`
and `states` explore the cone unreduced.

The cone's vars split into a target side T and the rest S:

- T holds one var the targets read (every var they read is tried as the anchor, and the
  split with the largest S wins), the writes of the actions the targets keep (the fair use
  cases of a property, the steps of an `after` assertion), and, by closure, every write of
  an action that writes or reads a var of T. An entity's vars share a side. So every action
  that writes S reads only S: S is a transition system of its own, which the rest of the
  cone never writes and reads only through `obs`, the S vars that T's actions and the
  targets' checks read (guards, transition sources, copy sources, the formula of a property).
  A check that reads S outside `obs`, an `after` assertion on an S action, and a property
  whose `.fair` names an S action are dropped from the reduced cone and decided on their own
  cones, where what they read is T or `obs`.
- S is explored on its own, as a cone with the same reduction applied to it, so a reduction
  nests, and its reachable states are quotiented by divergence-blind stuttering equivalence
  with respect to `obs`: the coarsest partition in which two states agree on `obs`, and
  whenever a step of S takes one of them out of its class into a class `c`, a path of S steps
  takes the other into `c`. The reduced cone keeps one representative S state per class in
  its state, runs T's actions on it unchanged, and replaces S's actions by class moves: from
  a class, by the last action of a path that leaves it, into the class the path enters. In
  TLA+ the sub-lifecycle is one variable `S_class`, a `Rep_<var>` table per observed var that
  the var's name reads, and a `Succ_<A>` table per S action (the snapshot
  `a_reduced_cone_prints_its_sub_lifecycle_as_classes` in `crates/rdra-ish-emit/src/tla/tests.rs`).
  Each table is one tuple literal with a cell per class, which TLC evaluates cell by cell
  whatever the class count; a chain of `@@` entries nests once per entry, and a sub-lifecycle
  of a thousand classes overflowed the JVM's stack. An unobserved S (`obs` empty) is one
  class with no moves. When every state of S is a class of its own and S was not reduced
  inside, the quotient is the identity and would reduce nothing, so the cone stays unreduced
  (#101): it keeps all its checks, its module has no `S_class`, and a property's
  counterexample needs no `_Full` run. A nested reduction still counts: an S whose own
  reduction shrank it is reduced, even when its classes are its states. An identity inner
  reduction does not count (`Reduction::shrinks`), and a split whose S is wholly in `obs` is
  not explored, since its quotient is the identity whatever the inputs do.
- Since #112 an S action may read T vars that only rise, its *inputs*. A var is monotone
  when the cone's writes, taken as edges between its values, form no cycle
  (`TransitionSystem::monotone_vars`): a create and a delete move an existence forward, a
  delete resets the entity's columns (which makes them cyclic; the row keeps its owners, but
  the rule counts them cyclic too, which only gives up a reduction), an assignment or
  a choice starts from the base value in the creating action and from any value elsewhere, a
  transition is its arms, and the clock ticks up. A raised mark, an existence, an owner key,
  and a status whose arms all point forward are monotone. The rule is checked on the cone,
  so a model that adds a backward arm loses the reduction and nothing else. The counter of a
  [counted trigger](../state-derivation.md#counted-triggers) is never monotone, whatever arms
  the cone holds, and the split puts every counter in T, with its raisers and its handler,
  which write it: a counted edge is never reduced across. Every action that reads a counter
  also writes it, so a counter in S could only be internal to S, which the argument below
  allows; T is the simpler rule, and the one #220 asked for
  ([design](./counted-events.md#the-reduction)). The quotient is
  then computed on S *extended* with its inputs (`restrict_to` of S and the inputs, without
  the havoc of a `Triggered` parameter): the S actions are silent, and every T action that
  writes an input is a *rise*, kept with the guards it has on S and the inputs. The
  partition is stuttering-stable under the silent steps and strongly stable under the
  rises, so two states of a class rise into the same class. The reduced cone looks a rise up
  in a table, per class and new input bytes (`Reduction::rises`), whenever a T step changes
  the inputs, and the lift replays the same rise on the extended run.
- A read of T through a handler's `Triggered` row, or through a quantifier over a T entity
  linked to an S row, is *folded* into one Boolean per slot of its key, so the extended
  system sees a bit where it would see every input's history. The param fold
  (`FoldKey::Param`) folds the guards an S action has on T params that only key its row:
  the bit at slot `c` holds when the guards hold for some binding of the other T params,
  each on a present row. The node fold (`FoldKey::Linked`) folds a guard of an action with
  no T param that reads T only through a quantifier whose body is T atoms beside the link
  `owner[p] = row`: the bit is keyed by the linked row, and the guard reads `owner[p] = q ∧
  bit[q]` where the quantifier stood. The variants of one handler whose folds agree share a
  bit. A bit starts where the cone's initial states put it, or stays a raw input when they
  disagree. In the extended system each bit has a free rise and a free fall, so the quotient
  stays correct even when a fold is not monotone.
- The tables a reduced cone hands TLC are bounded. `Reduction::tla_cells` counts one cell per
  class for each observed var and each S action, plus one per rise, and a reduction past
  `TLA_CELL_BUDGET` (530,000 cells) is not used: the split without inputs is tried, and the
  cone stays unreduced when that fails too. The native checker and TLC reduce under the same
  budget, so they check the same cones. Measured with TLC's 1 GB cone heap, a cone of 2,428
  classes prints 72,840 cells and runs, the insurance-claim-ops `ClaimCase` view's 4,368
  classes print 104,832 at a peak of 912 MB, and a random model's 24,000 classes printed
  326,400 and ran out of heap. The budget was 200,000 until #284 raised it to take
  food-permit-ops' `AfterAssert_SuspendBusiness_0` cone at 3 instances: 28,928 classes,
  520,704 cells (a 7.6 MB module), 7,167,444 states, which TLC checks in 98 s at `-Xmx4g`
  and runs out of heap on at `-Xmx1g` and `-Xmx2g`. Under `--with-tlc` a cone's heap
  follows its module, 512 MiB per MiB of module text (#309), so that cone gets `-Xmx4g`
  at the default `--memory`; a module that needs more than TLC's share of the memory limit
  is a tool error with the hint to raise `--memory` or set `RDRA_TLC_HEAP`.

With inputs, the module prints each bit group as an operator `In_<g>(c)` over the T vars, the
`Rise` table (one cell per class, `:>` and `@@` from `TLC`, so such a module `EXTENDS TLC`),
and, in every action that writes an input, the conjunct `S_class' = IF inputs' = inputs THEN
S_class ELSE Rise[S_class][inputs']`, staged as one `LET` per input group after the
`UNCHANGED` that binds the primed inputs. The decoder reads an input from the module's own
var, not from the representative.

**Proof obligation.** Let C be the cone and R the reduced cone. The relation
`(t, s) ~ (t, rep(class(s)))` between their states is a divergence-blind stuttering
bisimulation that preserves T and `obs`:

1. Related states agree on T and on `obs`: a class is a block of the partition, whose first
   refinement is by `obs`.
2. A T step of C from `(t, s)` is a T step of R from `(t, rep)` and back: a T action reads T
   and `obs` only (`obs` is what T's actions read of S) and writes T only, so it is enabled
   in both and leads to related states.
3. An S step of C from `(t, s)` stays in `s`'s class, which is a stutter step of R, or enters
   a class `c'`; then a path of S steps takes `rep` into `c'` (the partition is stable), and
   its last step is a class move of R into `c'`, whose target is related to the step's. A
   class move of R from `(t, rep)` into `c'` is matched by that path from `s`, every state
   of which but the last is related to `(t, rep)`.
4. An initial state `(t0, s0)` of C is related to the initial state `(t0, rep(class(s0)))`
   of R, and R has no other initial state.
5. With inputs, the class of `s` is read at the inputs of `t`. A T step that leaves the
   inputs alone is matched as in 2. A T step that changes them is a rise of the extended
   system from `(s, inp(t))`; strong stability under the rises gives the representative a
   rise into the same class, which `Rise` takes. A fold is sound by the same argument with
   the bit in place of the vars it folds, since the folded guard is a function of them and
   the key. The monotone rule is not used here: it bounds the input histories, which keeps
   the extended system finite and small. The `_WF` run below does use it: its case 3 and
   the shortcut for an `S` with no cycle (`s_cycles`) rest on `monotone_vars`.

Points 2, 3 and 5 also use a step they do not state: every reachable state of C has an `S`
part, inputs and bits that form a reachable state of the extended system, and a silent edge
of the extended system from there is a step of C. The theory notes call it Lemma 6 of the
cones theme; it is argued there, not proved, and folds rest on `Fold::holds` matching the
folded guard, which only the `fold_holds` twin compares with a second implementation (P31 of
[the theory's open problems](../theory/README.md#open-problems)).

A divergence-blind stuttering bisimulation preserves every formula of CTL\* without the next
operator, and the reduced cone's findings are such formulas over T and `obs`: a state is
reached (a witness, a violated invariant), an edge is taken
(a use case that runs, an arm taken, a violated `after` assertion), and from every reached
state with a present slot some path takes an edge that moves it (no deadlock). A property
`Fair_P => (...)` over T and `obs` whose fair actions are T actions is a stuttering-invariant
formula over T and `obs` under a fairness that reads T and `obs`, so the `Cone_<P>` run, whose
`Spec` admits stuttering, decides on R what it decides on C. What the bisimulation does not
preserve is `WF_vars(Next)`: a class that loops inside itself forever never stutters in C under
`WF_vars(Next)` while R stands still. So the `_WF` run reads R with every class that can loop
marked ([WF_vars(Next) on a reduced cone](#wf_varsnext-on-a-reduced-cone)), and when that does
not decide, a counterexample of the `Cone_<P>` run on R is repeated on the unreduced cone as
`Cone_<P>_Full`, whose lasso the printed trace reads (`verify` reports that run's outcome beside
the first's; an `ok` there is a tool bug, reported as such), and the `_WF` run follows there.

A trace of R is lifted to a path of C before it is printed (`TransitionSystem::lift`): a T step
is itself, and a class move becomes the path of S steps that realizes it from the concrete S
state the lifted path holds, which exists by 3, recursively through nested reductions. The
violation is read again on the lifted last state, so the message is built from a real state,
and a printed counterexample can be longer than the reduced one and name a use case of S on a
step. The tests in `crates/rdra-ish-core/src/lifecycle/tests/reduce.rs` compare the reduced and
unreduced exploration of every sample cone (the states on T and `obs`, the views, the verdicts,
and that every lifted path is a path of the cone), and `engine_equivalence` runs TLC on each
reduced cone against its BFS and checks that each check the reduced cone keeps has the cone's
verdict. `scripts/reduction-diff.py` compares whole `verify` runs, reduced and unreduced, on
random models (see [The reduction on random models](../engine-agreement.md#the-reduction-on-random-models)).

## `WF_vars(Next)` on a reduced cone

The classes that *can loop* (cases 1 to 3) are defined in
[the guide](../formal-verification.md#wf_varsnext-on-a-reduced-cone).

The bisimulation above is blind to divergence, so the reduced cone R cannot answer the
`Cone_<P>_WF` run as it is: a behavior of the cone C that steps inside one class of S
forever meets `WF_vars(Next)`, and its image in R stands still at a state that may have a
move. The native checker runs `Cone_<P>_WF` on R anyway, with every class that *can loop*
taken as a place where a behavior may stand still forever. This section proves that an `ok`
of that run is the `ok` of `Cone_<P>_WF` on C. A counterexample of that run is only a
candidate, and the last paragraph says how it is checked.

The mark costs one strongly connected component pass over the in-class silent edges of `X`,
which `Quotient::of` already holds. It reads nothing outside the sub-lifecycle.

**The run.** On R, under `Fair_P` and the property's goal as before, a state `r = (t, c)` of
`N` may stop when no fair instance is enabled at `r` and either no move leaves `r` or `c`
can loop. A component of `N` admits a fair cycle by the rule it has today. Every stop of
this run is a stop of the `Cone_<P>` run on R, so the run lies between that run and the
blind `WF_vars(Next)` run on R.

**Claim.** Suppose C has a behavior σ that satisfies `Fair_P` and `WF_vars(Next)`, reaches
a state where the trigger holds, and then never meets the goal. Then the run on R finds a
counterexample. So when it finds none on a complete graph, `Cone_<P>_WF` on C is `ok`.

**Proof.** Map each state `(t, s)` of σ to `(t, rep(class(s)))`. By points 2, 3, and 5 of
the proof obligation, each step of σ is a step of R or stays at the same state of R, so σ
maps to a path of R from an initial state. Related states agree on T and `obs`. The trigger,
the goal, and the enabledness of every fair instance read only T and `obs`, since the fair
actions are T actions. A fair instance takes a step of σ exactly when its image takes a step
of R, because a T step is the same step on both sides. Now split σ by what its image does
after the trigger.

- *The image moves infinitely often.* From some point the image visits only the states and
  edges of one strongly connected component `K` of `N` in R. Each fair instance is either
  taken infinitely often by σ, and so labels an edge inside `K`, or disabled infinitely
  often, and so disabled at some state of `K`. `K` has an edge, so `K` admits a fair cycle.
- *The image moves finitely often and σ changes the state infinitely often.* From some
  point σ holds T at `t`, and every step that changes the state is an S step inside one
  class `c`. If infinitely many of them move between states of `X`, those states
  are finitely many, so they contain a cycle of in-class silent edges, which is case 1. If
  from some point none does, σ steps inside one inner class forever, and the same argument
  one level down gives case 2. With `obs` empty, an infinite path of S steps changes some S
  var infinitely often, so that var is not monotone, which is case 3. So `c` can loop. Every
  fair instance is disabled at `(t, s)` from that point on, since `Fair_P` holds and none
  takes a step. Its enabledness is the same at `(t, rep(c))`. So `(t, rep(c))` may stop.
- *σ stands still at `(t, s)` forever.* `WF_vars(Next)` then says that no move leaves
  `(t, s)` in C. A T move of R from `(t, rep(c))` reads only T and `obs`, so it would leave
  `(t, s)` too. A class move from `rep(c)` into `c'` would, by stability, be matched by a
  path of S steps from `s` into `c'`, which has a first step. So no move leaves
  `(t, rep(c))`, and it may stop.

In each case the image of the triggered state reaches a fair end of the run through goal-free
states, so the run finds a counterexample. ∎

The claim does not use that the mark is tight. Marking more classes only adds stops, which
keeps the claim and adds candidates. What `Cone_<P>_WF` on C decides is unchanged: its
counterexample is the whole model's, and its `ok` is the whole model's only when the cone
holds every var an action writes (`cone_keeps_every_write`). Otherwise the property goes to
the whole model, as it does after the unreduced run.

**Candidates.** A stop at a class that can loop need not exist in C. The class may hold
states that can loop and states that cannot, and C may reach it at `t` only in the second
kind (`cone_reduced_loop_unreached` in the corpus). The looping states may also be ones that
C does not reach at `t` at all, since `X` takes its rises without their T guards and folds
bits that rise and fall freely. So a counterexample of the run on R is
lifted to C and certified there before anything uses it. The prefix lifts with `lift`. A
loop of R lifts by lifting the prefix and the loop repeated, until the concrete state at the
end of a copy is one at the end of an earlier copy. That takes at most as many copies as the
class has concrete states; `lift.rs::lift_cycle` gives up after `MAX_COPIES` (4,096)
copies, and a loop it gives up on is dropped as a lasso that fails a check is. A stop lifts to the concrete state `x` that the lifted prefix
ends at, followed by a walk from `x` of S steps of C that keep T and `obs` as they are at
`x`, to a state with no move or around a cycle. The lifted lasso must replay through the
moves of C, fail the property there, and pass the fairness check under `WF_vars(Next)`. A
lasso that fails a check, or a walk that finds neither, is dropped unprinted, and `verify`
runs `Cone_<P>_Full` and `Cone_<P>_WF` on C as it did before.
