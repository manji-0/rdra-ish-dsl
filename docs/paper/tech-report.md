---
title: "rdra-ish: Semantics, Reductions, Proofs and Evidence of a Requirements-Model Checker"
subtitle: "Technical report accompanying Paper 1 and Paper 2 (issue #594, roadmap T20)"
---

<!-- This file is the report's own text: front matter, introduction, summary of results, and the
chapter guide. scripts/paper/build-tech-report.py assembles the full report as
docs/paper/tech-report-full.md: this file, then each chapter's source document at the build's
commit, with links rewritten. The theory documents stay the source of truth. Do not edit the
assembled file. -->

# Abstract

rdra-ish is a textual requirements language in the RDRA tradition, with a checker for the lifecycle
rules and temporal properties of a model. This report collects in one place the semantics of the
language, the soundness arguments for every reduction the checker uses, the prover for state rules
over any number of rows, the agreement of the native checker with TLC, the inventory of claims with
the evidence behind each, the Lean development, and the full evaluation. Paper 1 (sound reductions
for fair liveness) and Paper 2 (what each kind of evidence catches) cite it for proofs and tables
that do not fit in them.

# 1 Introduction

A model `M` at a bound `β` (slots per entity, Int range) denotes a finite transition system
`T_β(M)`. `rdra-ish verify` decides the model's state rules, step assertions, temporal properties
under per-instance weak fairness, reachability goals and deadlocks on that system. It answers `ok`,
`counterexample`, `not decided` or `tool_error`. `verify --prove` tries to prove state rules for every
bound in a range with z3. `verify --with-tlc` runs TLC on the same printed systems and fails on any
disagreement.

Explicit-state checking at useful bounds needs three reductions, and the report proves each:

1. a **cone of influence** per check (Chapter 2);
2. the **sub-lifecycle quotient**, a divergence-blind stuttering bisimulation of one part of a cone,
   with a per-block mark that keeps it sound for `WF_vars(Next)` (Chapter 3);
3. **instance symmetry** under a state-dependent group that fixes absent slots and relabels foreign
   keys, with fairness carried along each edge's permutation (Chapter 4).

The report states each guarantee as a numbered claim (I1–I3, A1–A28, B1–B18, N1–N18, T1–T16) with
the kind of evidence behind it and its open gap (Chapter 8). It is explicit about what is proved,
what is mechanized, what is tested, what is checked at run time, and what is assumed.

## 1.1 Summary of results

| Result | Where | Evidence |
|---|---|---|
| Exact cones decide what the whole system decides; inexact cones over-approximate | Ch. 2, A1, A2 | Paper proof; tests against the projection of the whole system |
| The reduced cone is a divergence-blind stuttering bisimulation of the cone | Ch. 3, A8 | Paper proof; Lean for the flat case (`a8_dbsb`, `a8_branching`); run-time certificates of the split and the partition |
| Liveness under `WF_vars(Next)` on the reduced cone with the *can loop* mark | Ch. 3, A15 | Paper proof; Lean for the flat case (`a15`); run-time check of the marks |
| The orbit relation of `G_s` is a bisimulation on states whose absent rows hold base values | Ch. 4, A18 | Paper proof; Lean over a slot model (`symOn`, `a18_dbsb`) |
| Fair liveness on the annotated quotient | Ch. 4, A23 | Paper proof; Lean (`a23`, `oAccepts_on_iff`) |
| The fair-component search decides liveness | Ch. 5, N9 | Paper proof; Lean (`accepts_iff`) |
| Any number of threads computes the one-thread run | Ch. 5, N4 | Paper proof; 24 of 24 runs byte-identical (Ch. 10) |
| `proved` holds for every bound in the printed range | Ch. 6, B2 | Paper proof relative to z3; every `unsat` behind 63 `proved` rules re-checked by cvc5 (Ch. 10) |
| The native checker and TLC agree | Ch. 7, T2–T16 | 50 of 50 cones with equal verdicts and state counts; CI on every pull request |

## 1.2 The calculus LC

Chapter 1 opens with the calculus LC of [calculus.md](./calculus.md): entities, slots, owners and
guarded actions with per-instance weak fairness. Paper 1 states its theorems over LC. The rest of
Chapter 1 defines the full language (D1–D25), which extends LC with event latches and counters,
split use cases and their run marks, copies between columns, the clock, and scopes.

## 1.3 How the report is assembled

The chapters are the theory documents of the repository at the report's commit:

| Chapter | Source |
|---|---|
| 1. The calculus LC and the semantics | [calculus.md](./calculus.md), [semantics.md](../theory/semantics.md) |
| 2–4. Cones, the sub-lifecycle reduction, instance symmetry | [cones.md](../theory/cones.md) |
| 5. The native checker | [native.md](../theory/native.md) |
| 6. Proofs for any number of rows | [z3.md](../theory/z3.md), with the evaluation of [eval/prove.md](./eval/prove.md): the claim "any number of rows" is an evaluated heuristic, not a decision procedure (roadmap T16) |
| 7. Agreement with TLC | [tlc-agreement.md](../theory/tlc-agreement.md), with [eval/tlc-baseline.md](./eval/tlc-baseline.md) |
| 8. The claims inventory | [claims.md](../theory/claims.md), and the overview with residual risks and known limitations, [README.md](../theory/README.md) |
| 9. The Lean development | [proofs/lean/README.md](../../proofs/lean/README.md) |
| 10. Evaluation | [eval/](./eval/README.md): ablation, scaling, least instance counts, determinism, TLC, the prover, the mutation runs |
| 11. Extensions | [extensions.md](./extensions.md): certificates for `proved` and `ok`, the code chain, the agent study, external models |
| A. Defect record | [evidence/defects.md](./evidence/defects.md) |
| B. Earlier measurements | [evidence/measurements.md](./evidence/measurements.md) |
| C. Related work | [related-work.md](./related-work.md) |

`python3 -I scripts/paper/build-tech-report.py` writes `docs/paper/tech-report-full.md`, and
`pandoc docs/paper/tech-report-full.md -o tech-report.tex` gives LaTeX.

## 1.4 What the report does not claim

The report carries the known limitations of the theory overview at its commit, row by row, and its
two residual risks. The first is common-mode faults in the parts every engine shares (the builder,
the cone choice, the quotient tables, the TLA+ names). The second is that `ok` itself is not checked
at run time. Two kinds of step are argued on paper only, and the report marks them where they occur:
the nested case of the reduction and the run of a sub-lifecycle up to a subgroup.
