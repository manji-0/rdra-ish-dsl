---
title: "What Each Kind of Evidence Catches: Stating a Model Checker's Guarantees Claim by Claim"
subtitle: "Draft of Paper 2 (issue #594, roadmap T20). Target: ICSE SEIP or ISSTA (experience / methodology), 10 pages plus references."
---

<!-- Source of truth for the draft. Numbers come from docs/paper/evidence/defects.tsv through
scripts/paper/defect-summary.py, and from docs/theory/claims.md at 9e4a123. -->

# Abstract

A model checker's users read its `ok` as "the model satisfies the property". Behind that word sit a
model builder, a cone-of-influence choice, two state-space reductions, a liveness search, a
translation to TLA+ and a z3 encoding. Each has its own argument for being right, and some have
none. We report on rdra-ish, a requirements-model checker, whose documentation states each
guarantee as one of 83 claims. Each claim is tagged with the kind of evidence behind it: a paper
proof, a mechanized proof, a differential test, a run-time check, or an assumption. Each claim also
records an open gap, and a test keeps the user-facing list of known limitations in sync with
those gaps. We then ask what each kind of evidence actually found. In the project's history, 74
defects of the verification pipeline are recorded, and 30 of them printed a wrong verdict on some
model. Writing and auditing the paper proofs found 19 defects. 13 of these were wrong verdicts. 6 of
those were a wrong `ok` or `proved` that the native checker and TLC agreed on, or that no
comparison with BFS reached.
Differential testing against TLC found 7 defects, mostly in the TLA+ printer. The mechanized
proofs, the reference interpreter, mutation runs, run-time certificates and reference twins found
none that was not already known. They were added late, and the history shows them catching planted
defects. We argue that per-claim evidence is worth its cost because writing the claims finds
common-mode defects that engine agreement cannot see. The mechanical evidence classes then keep
those defects from coming back.

# 1 Introduction

The verification literature has well-developed ways to make a model checker trustworthy:
verified implementations, certifying checkers that emit proofs, differential testing against a
second engine, and mutation analysis of the test suite. A project usually adopts some of them, and
its users cannot tell which guarantee rests on which. This paper reports one project's attempt to
say, claim by claim, what each guarantee rests on, and then to measure what each kind of evidence
caught.

**Setting.** rdra-ish checks requirements models: entities with lifecycles, foreign keys, use cases
and temporal properties. Its native explicit-state checker decides each check on a cone of
influence, optionally reduced by a sub-lifecycle quotient and by instance symmetry. It can run TLC
on the same printed system, and it can prove state rules for any number of rows with z3 (Houdini
over mined lemmas). Paper 1 presents the reductions.

**The claims inventory.** `docs/theory/claims.md` has 83 rows: I1–I3 (the builder), A1–A28 (cones
and reductions), B1–B18 (the prover), N1–N18 (the native checker) and T1–T16 (agreement with TLC).
Each row states a guarantee as a proposition over the semantics, and lists its evidence classes,
the code that implements it, the tests that check it, and its *open gap*. A test
(`cli/tests/export_gate.rs`) fails when a row with an open gap has no bullet in the README's
"Known limitations", or a bullet names a row with no gap.

**Contributions.**

1. A format for stating a verifier's guarantees with per-claim evidence classes and open gaps, kept
   in sync with user-facing documentation by a test (Section 2).
2. A record of 74 defects and 15 documentation errors, classified by how each was found, by
   component, and by effect (Section 3).
3. An analysis of what each evidence class caught, and why agreement between engines misses
   common-mode defects that writing down proofs finds (Section 4).
4. The costs of the approach and lessons for other tool builders (Section 5).

# 2 Claims with evidence classes

**Row format.** Each claim row has:

- *Statement*: a proposition over `T_β(M)`, the transition system that a model denotes at a
  bound, defined in a semantics document with no reference to code (definitions D1–D25).
- *Evidence*: one or more of P-full (a paper proof with every step), P-sketch (a named step not
  derived), M (mechanized: a Lean theorem, a Kani harness, a Lean-verified checker), D
  (differential: two implementations or engines compared), R (a run-time check in every build), A
  (an assumption: z3 is sound, TLC is correct).
- *Code and tests*: the functions that implement the claim and the tests that check it, by name.
- *Open gap*: what the evidence does not cover. An empty gap is a claim to be checked.

**Residual risks.** The README states the two risks that outweigh the rest. *Common-mode faults*:
the builder, the cone choice, the quotient tables and the TLA+ names are read by every engine, so
agreement between engines cannot show a fault in them. *No run-time check of `ok` or `proved`*: a
counterexample is replayed before it prints, but an `ok` is not checked.

**The summary by evidence class** counts claims per class and lists the P-sketch theorems. These
are the paper arguments that the trust base includes.

# 3 The defect record

**Method.** We read `CHANGELOG.md`, the full git history (2,124 commits), the issue and pull
request descriptions they cite, and the theory documents' former table of open problems (P1–P67,
removed in a later commit and read from history). A row is a defect in the verification
pipeline: a wrong verdict, a wrong trace, a crash, a tool error, or a missed check. Documentation
errors about guarantees are separate rows. For each, we record the component, the effect, whether a
run showed it on a model, and the discovery class that the source names. If the source does not
say how the defect was found, the class is *unstated*. We did not guess. Codegen, the language
server, the diagram and SQL export, owner decisions on semantics, and performance problems are out
of scope ([evidence/defects.md](./evidence/defects.md) lists the exclusions).

**Discovery classes.** (1) Lean proof attempt; (2) paper proof: writing a theory document or
auditing one; (3) reference interpreter; (4) TLC differential or the engine corpus's hand-written
headers; (5) mutation run; (6) run-time self-certification; (7) shadow (reference) twin; (8) Kani,
Miri or overflow-checked CI; (9) random-model reduction diff; (10) test, user report, review, use of
the tool on a sample model, or found while fixing another defect.

**Results.** Generated by `scripts/paper/defect-summary.py` from `evidence/defects.tsv`.

| Discovery class | Defects | Wrong verdicts | Wrong verdicts shown by a run |
|---|--:|--:|--:|
| 1 Lean proof | 0 | 0 | 0 |
| 2 Paper proof (write-up or audit) | 19 | 13 | 9 |
| 3 Reference interpreter | 0 | 0 | 0 |
| 4 TLC differential / engine corpus | 7 | 3 | 3 |
| 5 Mutation run | 0 | 0 | 0 |
| 6 Run-time self-certification | 0 | 0 | 0 |
| 7 Shadow twin | 0 | 0 | 0 |
| 8 Kani / overflow checks | 1 | 0 | 0 |
| 9 Random-model reduction diff | 2 | 0 | 0 |
| 10 Test, review, use, other | 21 | 16 | 9 |
| Unstated | 24 | 18 | 9 |
| **Total** | **74** | **50** | **30** |

By component, the builder has the most defects (23), then the TLA+ printer (14), the "other"
driver code (13), the reduction (9), the cone (6), the prover (5), the BFS and liveness search (3)
and symmetry (1).

# 4 What each class caught

**Paper proofs find common-mode defects.** Of the 13 wrong-verdict defects found by writing or
auditing proofs, 9 were shown by a run. In 6 of them (P40, P54, P43, P66, P4, P67), the wrong `ok`
or `proved` was one that BFS and TLC agreed on, or one that no comparison with BFS reached. Examples:

- *A stub parameter never bound in the reduction's extended system* (P67). Default `verify`
  printed `Forbidden_Order_0: ok (35 states)`, while the unreduced run finds a 7-step counterexample.
  No comparison in CI reached it. The fifth audit of
  the reduction's soundness argument found the defect: the `C ⇒ X` half of Lemma 6 did not hold for
  parameters on entities that the extended system holds only as stubs.
- *A guarded `null` beside a guarded `present` in the z3 encoding* (P66). BFS and TLC refute the
  rule in 3 steps. `--prove` printed `proved`.
- *Packed bit offsets that saturated at bit 65,535* (P50). This gave a wrong `ok` on a 34-column
  entity at 255 instances. It was found while writing the native checker's theory document.

Each was found when a proof step needed a fact that the code did not provide.

**Differential testing finds disagreements, not shared errors.** The 7 class-4 defects are in the
TLA+ printer and the driver. For example, TLC with `-continue` reports one violated check per
state, so `verify` missed a rule that fails only where another rule fails too. The one common-mode
defect in class 4 (an out-of-range Int default silently read as 0 by every engine) was caught by a
*hand-written* corpus header, not by agreement.

**Mechanical evidence was added after the defects it targets.** The Lean development, the reference
interpreter, the shadow twins, the Kani harnesses and the mutation runs all date from the last weeks
of the record. The history shows each one failing on a planted defect or on the pre-fix build of a
known one: the reference interpreter "fails on P40/P54 and P43 before their fixes", and the Lean
example `stub_bug` restates P67 after its fix. They found no new defect. This is not evidence that
they are useless. Their role is regression: they turn a fact that a proof needed into a check that
runs on every commit or every run. For P67 that check is the binding check of the extended system
(`StubCheck`), which every build now runs.

**Unstated discoveries.** A third of the defects (24) have no recorded discovery method. Most are
early fixes committed straight to main. The record is more complete after the theory documents
were written, which inflates class 2 relative to early classes (Section 6).

# 5 Costs and lessons

- **Cost.** The theory documents are about 7,100 lines and the Lean development about 14,000
  lines, written over the project's last weeks. The claims inventory needs updating whenever a
  guarantee changes. A test enforces only the link between the inventory's gaps and the README.
- **Lesson 1: write the claim before the check.** The defects that engine agreement cannot see
  came out when a claim had to be stated with its hypotheses. The mechanical check came afterwards.
- **Lesson 2: name the open gap.** Gaps such as "no run-time check of `ok`" and "the code link is
  read" keep the paper and the README from overclaiming. The gate test makes a missing gap visible.
- **Lesson 3: certify hypotheses, not results.** Checking `ok` itself is costly. Checking the
  hypotheses that the soundness proof needs (a split, a partition, loop marks) is cheap, and a
  failure has a sound fallback.
- **Lesson 4: record how a defect was found.** A third of this project's record cannot say so, which
  limits the analysis above.

# 6 Threats to validity

- *Record completeness.* The unstated third and the late arrival of the theory documents bias the
  counts towards class 2. The analysis is of one project.
- *Classification.* Class 2 merges reading code while writing a proof with a proof step failing.
  The TSV's subclass column separates write-up (12) from review or audit (7).
- *First finder only.* A defect that a later check would also have caught is credited once.
- *Mutation runs.* The recorded kill rates (66.5% and 62.9%) cover the native liveness ladder, not
  the reductions. A TLC-side run (roadmap T11) lost all four of its CI runners before reporting.
  It tested two mutants, and both survived. About 45 runner-hours are needed to complete it.

# 7 Related work

Verified model checkers (CAVA, verified LTL checkers in Isabelle) prove the implementation, which
removes common-mode builder faults at a high cost. Certifying model checkers (Namjoshi) emit
proofs of the result. Differential testing of model checkers and SMT solvers finds engine
disagreements. Mutation analysis measures test adequacy. Assurance cases (GSN) structure arguments
for safety properties of systems. A claims inventory is a lightweight assurance case for the tool
itself, with machine-checked links to the documentation. We know of no study that reports, for one
verifier, what each kind of evidence found.

# 8 Conclusion

Stating each guarantee with its evidence and its gap found the defects that matter most: wrong
`ok` verdicts that every engine agreed on. Mechanical evidence keeps them fixed. Tool builders who
can afford one of the two first should start with the claims.
