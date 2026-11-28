# Related work and the delta of Paper 1 (roadmap T14)

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
[refs.bib](./refs.bib). Each bibliographic entry was checked against the publisher or DBLP
where a search found it. An entry that could not be checked has `note = {unverified}`. A
table cell that rests on a claim this note could not check is marked "(unverified)".

## What rdra-ish does, stated for comparison

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

## The table

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

## The delta, row by row

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

## Five strongest deltas

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

## Risky cells

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
