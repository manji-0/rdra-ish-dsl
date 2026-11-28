# Papers from docs/theory (issue #594)

This directory turns [docs/theory](../theory/README.md) into two papers, a technical report and an
artifact, following the roadmap of issue #594. It describes main at `9e4a123`. The theory
documents stay the source of truth for every guarantee. This directory selects from them, restates
the core over a calculus, and adds the evaluation.

| File | Contents |
|---|---|
| [paper1.md](./paper1.md) | **Paper 1 draft** (method paper, TACAS): the sub-lifecycle quotient and instance symmetry for fair liveness, with certificates and fallback |
| [paper2.md](./paper2.md) | **Paper 2 draft** (experience/methodology, ICSE SEIP or ISSTA): per-claim evidence classes, and what each class found |
| [tech-report.md](./tech-report.md), [tech-report-full.md](./tech-report-full.md) | The technical report: its own front matter and summary, and the full report assembled from the theory documents by `scripts/paper/build-tech-report.py` |
| [contribution.md](./contribution.md) | T1–T2: thesis, venue, contribution statement, the motivating examples |
| [calculus.md](./calculus.md) | T12–T13: the calculus LC, Theorems 1–6 restated on it, and the decision on the code link |
| [related-work.md](./related-work.md), [refs.bib](./refs.bib) | T14: the delta table against prior work, and the bibliography |
| [evidence/defects.md](./evidence/defects.md) | T3: 74 defects and 15 doc/claim errors by discovery class |
| [evidence/measurements.md](./evidence/measurements.md) | T5: earlier measurements, each marked citable or to re-measure |
| [eval/](./eval/README.md) | T4, T6–T8, T10, T11: the evaluation, with data and the commands that regenerate it |
| [artifact.md](./artifact.md) | T19: how to rebuild and regenerate every table |
| [extensions.md](./extensions.md) | T9, T15, T17, T18 |

Scripts: [scripts/paper/eval.py](../../scripts/paper/eval.py) (ablation, scaling, least instance
count, determinism, tables), [scripts/paper/defect-summary.py](../../scripts/paper/defect-summary.py),
[scripts/paper/tlc-baseline.py](../../scripts/paper/tlc-baseline.py), and the example
`crates/rdra-ish-core/examples/paper_whole.rs` (BFS on the whole model).

## Roadmap status

| Item | Status | Where |
|---|---|---|
| T1 thesis and venue | Done: the sub-lifecycle quotient leads, symmetry second; TACAS, then CAV | [contribution.md](./contribution.md#t1-the-thesis-and-the-venue) |
| T2 contribution statement and example | Done: two corpus cases that a divergence-blind quotient and full-slot symmetry get wrong | [contribution.md](./contribution.md#the-example-that-existing-tools-get-wrong) |
| T3 bug-finding record | Done: 74 defects, 30 wrong verdicts shown; Lean, reference interpreter, mutation, certificates and twins found none first | [evidence/defects.md](./evidence/defects.md) |
| T4 minimal-instance data | Done: 106 of 132 expected counterexamples at n=1, 24 at n=2; escalation adds 5 counting rules that fail first at 3 or 4 | [eval/minimal.md](./eval/minimal.md), [eval/prove.md](./eval/prove.md) |
| T5 reuse of measurements | Done: about 80 earlier figures, most to re-measure | [evidence/measurements.md](./evidence/measurements.md) |
| T6 ablation | Done: with the reduction, four domain models finish in 4–42 s; without it, they time out at 300 s after 5–8 GB. The reduction costs on oidc-auth. | [eval/ablation.md](./eval/ablation.md) |
| T7 scaling | Done: symmetry saves 1.6–55% of states end to end and never shrinks a domain model's largest run; the S7 per-cone factors are not reproduced end to end. | [eval/scaling.md](./eval/scaling.md) |
| T8 baselines | Done for TLC: a local run with TLC 1.7.4 on eight models gives the same verdict and state count on all 50 cones both engines decided, with per-cone TLC times; CI agreement (TLC 1.8.0) as well. TLC with symmetry sets is unsound on this encoding, so there is no such column. Alloy and Apalache encodings are not written (stated as a limit). | [eval/tlc-baseline.md](./eval/tlc-baseline.md) |
| T9 external models | Open, and needs people outside the project: all models are the authors'. The five domain models cite public sources for their rules. The page names two routes (outside modellers on the same PRDs; translated published case studies). | [extensions.md](./extensions.md#t9-models-not-written-by-the-authors) |
| T10 determinism | Done: 24 of 24 runs at 1, 2 and 4 threads print byte-identical output. | [eval/determinism.md](./eval/determinism.md) |
| T11 TLC-side mutation run | Done on a systematic 1-in-12 sample (48 of 569 mutants), locally with TLC: kill rate 80.4% (37/46 viable; 95% CI 66.8–89.3%), 9 survivors listed as test gaps. The full run in CI lost its runners; completing all 569 needs about 33 machine-hours. | [eval/README.md](./eval/README.md#t11-the-tlc-side-mutation-run) |
| T12 core calculus | Done: LC, with Theorems 1–6 stated on it | [calculus.md](./calculus.md) |
| T13 code link | Decided: the claim is narrowed to LC plus a validated translation (certificates, replay, shared Lean syntax, differential tests); a deep embedding of the builder is future work | [calculus.md](./calculus.md#4-the-link-to-the-code-t13) |
| T14 related-work delta | Done, with the cells that need checking before submission listed | [related-work.md](./related-work.md#risky-cells) |
| T15 certificates for `ok` | `proved`: done offline. Every SMT query behind the 63 `proved` rules of four domain models is re-checked by cvc5: all 12,201 `unsat` answers confirmed (`scripts/paper/smt-crosscheck.py`). BFS `ok`: a reachable-set certificate designed and sized (18–151 MB), not built. | [extensions.md](./extensions.md#t15-certificates-for-proved-and-for-ok) |
| T16 z3 characterization | Done as an evaluated heuristic (no fragment or cutoff theorem): every rule BFS passes on the samples is proved (79 on the domain models); on the corpus 13 of 66 are unproved, 7 of them because more rows break the rule. Ivy/DistAI baselines not run. | [eval/prove.md](./eval/prove.md) |
| T17 requirements-to-code chain | Done as a case study on one model (Rust): 14 kinds of planted model/code drift, each reported by `check --code` at `fresh`, `build` or `run`; 26 of 26 Rust tests and 17 of 17 TypeScript and Go tests pass. One model only, and codegen is experimental. | [extensions.md](./extensions.md#t17-the-requirements-to-code-chain) |
| T18 agent feedback study | Protocol written; not run (kept separate, as the issue asks). | [extensions.md](./extensions.md#t18-agent-feedback-study-protocol-only) |
| T19 artifact | Done: pinned toolchain (Nix flake) and scripts for every table | [artifact.md](./artifact.md) |
| T20 drafts | Paper 1 and Paper 2 drafted; the technical report written (front matter, summary of results) and assembled from the theory documents | this directory |

## Before submission

- Check every cell that [related-work.md](./related-work.md#risky-cells) lists as risky, and every
  `refs.bib` entry marked `unverified`.
- Rerun the evaluation on a quiet machine at the submission commit. Report instructions retired
  where the machine has counters.
- Turn `paper1.md` into the LNCS template (`pandoc docs/paper/paper1.md -o paper1.tex` is a start)
  and cut to 16 pages: the proofs of Section 4 and 5 go to the appendix and the report.
