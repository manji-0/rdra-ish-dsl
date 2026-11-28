# Technical report: outline and sources (roadmap T20)

The technical report (arXiv) holds what does not fit in Paper 1 or Paper 2. It is assembled from
the theory documents, which stay the source of truth: the report cites them at a commit and does
not fork them. This outline gives the chapters and, for each, the source and what must change for
an outside reader. It describes main at `9e4a123`.

| Chapter | Source | Changes for the report |
|---|---|---|
| 1. The language and its semantics | [semantics.md](../theory/semantics.md) D1–D25 | Add the LC calculus of [calculus.md](./calculus.md) as Section 1.1, and state D2–D13 as an extension of LC (latches, counters, run marks, copies, clock). Move the *Implemented by* notes to a table at the end of the chapter. |
| 2. Cones of influence | [cones.md](../theory/cones.md), "The cone of a seed" to "`can_reach` cones" (A1–A7, A28) | Restate Definition 1 over LC with marks and copies. Keep the proofs of Lemmas 1–4 and Theorems A1, A2, A4, A5, A7 whole. |
| 3. The sub-lifecycle reduction | cones.md, "The reduction by an independent sub-lifecycle" and "`WF_vars(Next)` on a reduced cone" (A8–A16, A22) | Full proofs of Lemma 5, Lemma 6 (all ten steps), Theorem A8 and Theorem A15; the nested case as a sketch, marked so; the folds as an optimization with their hypotheses. |
| 4. Instance symmetry | cones.md, "Instance symmetry" (A18–A23) | Full proofs of A18 and A23, the witnesses table of A20, the canonical form and its cap (A21). |
| 5. The native checker | [native.md](../theory/native.md) (N1–N18) | The packed store and its memory accounting, parallel BFS and why it computes the one-thread result (N4), the fair-component search (N9, N10), counterexample replay. |
| 6. Proofs for any number of rows | [z3.md](../theory/z3.md) (B1–B18) | The SMT encoding, Houdini with mined lemmas, Lemma U, the certificate cache. State plainly that "any number of rows" is an evaluated heuristic, not a decision procedure (roadmap T16). |
| 7. Agreement with TLC | [tlc-agreement.md](../theory/tlc-agreement.md) (T1–T16) | The TLA+ translation, the agreement rules, the corpus, the random-model comparison, the mutation runs with their kill rates (T11), residual risk. |
| 8. The claims inventory | [claims.md](../theory/claims.md) | Reproduce the 83 rows with evidence classes and gaps, at the report's commit. |
| 9. The Lean development | [proofs/lean/README.md](../../proofs/lean/README.md) | The theorem map, the hypotheses, and what is not covered. |
| 10. Evaluation, full tables | [eval/](./eval/README.md) | Every table of `scripts/paper/eval.py`, per cone where the paper gives totals. |
| Appendix | [evidence/defects.md](./evidence/defects.md), [evidence/measurements.md](./evidence/measurements.md) | The defect record and the table of earlier measurements with their verdicts. |

**What the report does not claim.** It carries the Known limitations of
[docs/theory/README.md](../theory/README.md#known-limitations) at its commit, row by row, and the two
residual risks.

**Status.** Outline only. The chapters are the theory documents as they stand. The edits in the
last column are the remaining work.
