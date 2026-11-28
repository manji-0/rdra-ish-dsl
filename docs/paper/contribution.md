# Paper 1: thesis, venue and contributions (roadmap T1, T2)

This note records the decisions of roadmap items T1 and T2 of issue #594. It describes main at
`9e4a123`.

## T1: the thesis and the venue

**Thesis.** *Two reductions make explicit-state checking of fair liveness feasible on
data-carrying lifecycle models. Each one is sound for per-instance weak fairness, each one is
checked against its hypotheses on every run, and each one falls back to a sound alternative (the
unreduced cone, a fallback partition, or a smaller group) when a check fails.* The two reductions are:

1. the quotient of an independent sub-lifecycle by divergence-blind stuttering bisimulation,
   with a *can loop* mark per class that keeps the quotient sound for `WF(Next)`; and
2. instance symmetry under a state-dependent group `G_s` that fixes absent slots and relabels
   foreign keys, with fairness carried along each edge's permutation.

**Which candidate leads.** The issue offered two candidates: symmetry with fairness, or the
sub-lifecycle quotient with the certificate fallback. The paper leads with the **sub-lifecycle
quotient**. Symmetry is the second contribution. Three facts decide the order:

- The quotient is on by default, and the evaluation shows its effect on the domain samples. On
  patent-ops at 2 instances, `verify` finishes in about 4 s with the reduction and does not
  finish in 300 s without it ([eval/ablation.md](./eval/ablation.md)).
- Instance symmetry is experimental and off by default (`--symmetry never`). The only symmetry
  factor on record with exact counts on both sides is 3.0× to 5.5× on patent-ops
  ([evidence/measurements.md](./evidence/measurements.md), S7). A paper that leads with symmetry
  would need the default changed and a larger evaluation first.
- Both reductions have Lean proofs for their flat cases (A8, A15, A18, A23). The quotient also has
  a Lean-verified checker for its run-time certificate (`Sweep.stable_of_cert`,
  `Lemma5Check.check_sound`), which makes the "certificate and fallback" contribution concrete.

**Venue.** TACAS (tool and theory paper, regular research track, artifact evaluation). The
second choice is CAV. Reasons:

- TACAS takes a combination of theory, an implementation and an evaluation, and runs artifact
  evaluation. The Nix flake and `scripts/paper/` make an artifact feasible (T19).
- The material fits 16 pages LNCS only when the proofs go to an appendix and the technical
  report. The full Lean development backs the appendix.
- An FM-class venue would value the Lean development more. It would also ask for the code link
  that the paper does not claim (see [calculus.md, Section 4](./calculus.md#4-the-link-to-the-code-t13)).

**What the paper does not claim.** These follow the issue's "Claims to avoid" list and the gaps
in [docs/theory/README.md](../theory/README.md#known-limitations):

- that the tool is verified: the theorems are about the calculus LC, and the code link is
  validated, not proved;
- anything about `--prove` and "any number of rows" (that is the technical report's material,
  T16);
- any productivity claim about requirements modeling.

## T2: the contribution statement

> Business requirements describe entities whose rows are created, linked by foreign keys,
> moved through lifecycles, and deleted. Checking that every paid order is eventually shipped,
> under the assumption that each order's shipping step is not starved forever, is a fair-liveness
> question over a product of many such lifecycles. Explicit-state checking of these models
> suffers from two kinds of blow-up. The first comes from sub-lifecycles that a property
> observes only through a few values. The second comes from interchangeable rows. The standard
> answers lose soundness here. A quotient by divergence-blind stuttering bisimulation is unsound
> for weak fairness of the next-state relation. TLC's symmetry sets are documented as unsound
> for liveness. Symmetry over all slots does not commute with creation in the lowest free slot.
>
> We present two reductions for this setting and prove them sound for fair liveness. (1) The
> *sub-lifecycle quotient* splits a cone of influence into a part `T` that the property reads and
> a sub-lifecycle `S` that `T` observes through a set of variables `obs`. It replaces `S` by the
> blocks of a stable partition of an extended system, and marks the blocks that can loop
> silently, so that the quotient stays sound under `WF(Next)`. (2) *Instance symmetry* uses a
> state-dependent group `G_s` that permutes only the non-absent slots and relabels foreign keys.
> A quotient edge carries the permutation that produced it, and fair instances are tracked along
> these permutations, so the search for fair cycles decides `eventually` and `leads_to` exactly.
> The flat cases of both theorems are mechanized in Lean 4, with two steps of the symmetry proofs
> argued on paper. Neither reduction trusts the code
> that computes it. Every run checks the split, the partition, the loop marks and the symmetry
> hypotheses. A failed check makes it decide the cone unreduced, use a fallback partition, or leave
> a class out of the group. Every printed counterexample is lifted to the unreduced cone and
> replayed on the built system before it prints.
>
> We implemented both reductions in rdra-ish, an explicit-state checker for a requirements DSL.
> We evaluate them on nine sample models and a corpus of 91 adversarial cases, reporting an
> ablation per reduction, scaling in the number of instances, and agreement with TLC.

**Contributions**, as the introduction will list them:

1. A sub-lifecycle quotient for fair liveness: a split condition (S1–S5), a stability condition
   (B1–B3), and the *can loop* mark. We prove the quotient a divergence-blind stuttering
   bisimulation that keeps `Fair_P` liveness (Theorem 2), and sound for `WF(Next)` with the mark
   (Theorem 3).
2. A state-dependent symmetry group for models with creation and foreign keys, with syntactic
   hypotheses (G1–G6). We show that it is a bisimulation on the states whose absent rows hold base
   values (Theorem 4), and that fair liveness transfers to an annotated quotient (Theorem 5).
3. Certificates and fallback: executable checks of every hypothesis that depends on computed
   data, a Lean model of the partition check and Lean-verified offline checkers, and replay of
   every printed counterexample on the unreduced cone.
4. An implementation and an evaluation: an ablation per reduction, scaling, determinism across
   thread counts, and agreement with TLC on the same printed systems.

## The example that existing tools get wrong

Two cases of the engine corpus (`crates/rdra-ish-cli/tests/engine_corpus/`) are the paper's
running examples. CI checks both with the native checker and with TLC, without reduction or
symmetry on TLC's side.

**A loop that a divergence-blind quotient hides** (`cone_reduced_class_loops.rdra`). A job waits
until its gate is open, and the gate opens only after its key is unlocked. While the gate is
shut, its latch can be set and cleared forever (`Unlatch`, `Relatch`). The property
`eventually(Job.status == running)` holds under `Fair_P ∧ WF(Next)` only if the latch loop
cannot run forever. It can: on the unreduced cone the latch toggles while the key stays locked,
and that behaviour satisfies `WF(Next)`. The job's cone is reduced with the gate as its
sub-lifecycle. The divergence-blind quotient merges the latch states of a shut gate into one
block, and that block has a move to `open` and no loop. A `WF(Next)` search on the quotient
alone answers `ok`. That answer is wrong. With the *can loop* mark on the block, the search may
stop in the block and reports the counterexample that TLC reports on the unreduced module.

**Slots that are not symmetric** (`presence_first_absent_asymmetry.rdra`). A gate opens only
once some order is done, a rush order needs the gate open, and `Finish` skips rush orders. So
the first order created is never a rush order. Creation takes the lowest absent slot, so the first
order is always `Order#1`, and only `Order#2` can be a rush order that never finishes. A checker
that decides one binding and assumes the others alike answers `ok` for `eventually(Order.status == done).fair(Finish)`. The correct verdict is a
counterexample. In the run up to `G_s`, a permutation never moves an absent slot, so the
asymmetry of creation is kept, and the counterexample is found and lifted. Of the 41 reachable
states, every one in which only `Order#1` exists has a full-slot swap that no run reaches, so a
group that moves absent slots is no symmetry of this system.

**What existing tools do with them.**

- **TLC** checks both correctly without symmetry. With a symmetry set over the order slots,
  TLC's documentation warns that liveness checking can miss violations. The slots are also
  ordered by creation, so they are not interchangeable model values.
- **Murφ scalarsets and Emerson–Sistla symmetry** take a fixed group of process indices. The
  second example has no such group: the group that commutes with its steps depends on which
  slots are absent.
- **A quotient by divergence-blind stuttering bisimulation** (the Groote–Vaandrager algorithm
  over the sub-lifecycle) gets the first example wrong under `WF(Next)`, for the reason above.
  The divergence-sensitive variant is sound, but it does not merge the latch states. The *can
  loop* mark keeps the coarser partition and adds back only the stops that it hides.
