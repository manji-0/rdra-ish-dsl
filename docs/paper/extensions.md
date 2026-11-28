# Phase 4 extensions and the external-models item (roadmap T9, T15, T17, T18)

The issue marks T15–T18 optional. This page records what was done for each and what remains. It
describes main at `9e4a123`.

## T15: certificates for `proved` and for `ok`

**Residual risk 2** of [docs/theory/README.md](../theory/README.md#conclusion): an `ok` and a
`proved` are not checked at run time.

### `proved`: every z3 answer re-checked by a second solver (done, offline)

A `proved` rests on `unsat` answers of z3: initiation and consecution of the inductive set Houdini
keeps (z3.md, Theorem 1, under H1: z3 answers `unsat` only for unsatisfiable queries). The example
`crates/rdra-ish-core/examples/prove_transcript.rs` records every SMT-LIB command z3 receives in one
prover run. [scripts/paper/smt-crosscheck.py](../../scripts/paper/smt-crosscheck.py) replays that
transcript through z3 and through cvc5. For cvc5 it removes z3's own options (`:rlimit`, `:smt.mbqi`)
and sets the logic after each `(reset)`. It then compares the `check-sat` answers in order. The
transcript is the certificate. It is self-contained SMT-LIB, so any SMT solver can check it,
without rdra-ish.

| Root | Queries | unsat (z3) and unsat (cvc5) | unknown, both | sat (z3), unknown (cvc5) | unsat (z3), sat (cvc5) | Rules proved |
|---|--:|--:|--:|--:|--:|--:|
| patent-ops | 1,116 | 1,111 | 4 | 1 | 0 | 13 |
| clinic-ops | 4,083 | 3,994 | 88 | 1 | 0 | 13 |
| food-permit-ops | 3,842 | 3,783 | 58 | 1 | 0 | 14 |
| insurance-claim-ops | 3,372 | 3,313 | 58 | 1 | 0 | 23 |
| oidc-auth | 13.8 MB transcript; the replay did not finish in 590 s | | | | | 16 |

Data: [eval/data/smt-crosscheck.txt](./eval/data/smt-crosscheck.txt). z3 5.4.1 and cvc5 1.4.2 from
nixpkgs.

- On four domain models, cvc5 answers `unsat` to every one of the 12,201 queries that z3 answered
  `unsat`. So the 63 `proved` rules of these models no longer rest on z3 alone: H1 is needed for one
  of two independent solvers, not for z3 in particular.
- The cross-check covers H1 only. It does not cover H2, that the SMT text means the encoding of the
  system: both solvers read the same text. It also does not cover B1, the faithfulness of the
  encoding.
- It runs offline, as a script. Making it a run-time option (`--prove --cross-check cvc5`) would put
  the second solver on every `proved`. A smaller certificate would hold only the final round:
  `Decl`, `Init`, every `Step_a`, and the kept set `K`, with one initiation and one consecution query
  per member of `K`. Its size is about one Houdini round instead of the whole search.

### BFS `ok`: a reachable-set certificate (built for unreduced cones)

The certificate is the set `R` of states a complete run stored.
`crates/rdra-ish-core/src/lifecycle/certificate.rs` writes it (`ReachCertificate::write`: a magic
line, the width, the count, then the raw states), reads it back, and checks three facts with the
step relation alone (`TransitionSystem::check_reach_certificate`): every initial state is in `R`;
every successor of every state of `R` is in `R`; every invariant holds in every state of `R`. Then
`Reach ⊆ R`, so the invariants hold. The checker uses none of the search's code: no frontier, no
parents, no packed store, no table, no threads. It refuses a reduced cone (`CertFault::Reduced`),
whose class moves are not a cone's steps, and a run that stopped (`CertFault::Incomplete`).

`crates/rdra-ish-core/examples/paper_certify.rs` certifies every distinct safety cone of a sample,
unreduced. It writes each certificate to a file, reads it back, checks it, and then plants two
faults in the largest certificate, which the check must reject. Data:
[eval/data/certificates.tsv](./eval/data/certificates.tsv).

| Run | Cones certified | `ok` | Largest certificate | Its check |
|---|--:|--:|---|--:|
| five domain models, 1 instance | 51 | 51 | 30,277 states, 113,434 moves, 1.5 MB (oidc-auth) | 57 ms (run: 23 ms) |
| patent-ops, 2 instances | 11 | 10 | 1,494,339 states, 9,285,628 moves, 66 MB | 3.8 s (run: 1.6 s) |
| `formal-verification-fail/order.rdra`, 2 instances | 1 | 0 | 7 states | rejected: `Violated { invariant: "Inv_Order_0" }`, as the sample expects |

- Every certificate of a cone whose rules hold checks `ok`. The certificate of the failing sample is
  rejected for the invariant that `verify` reports violated.
- patent-ops' `Forbidden_OfficeAction_3` cone has more than 20 million states unreduced, so its run
  stops at the example's cap and it gets no certificate. `verify` decides that cone reduced
  (1,341,707 states). A certificate of a reduced cone would be the reduced run's states plus the
  split and partition certificates that every build already checks, read by a checker of the class
  moves. That checker is not built.
- **Planted faults.** On every sample run, dropping one state from the largest certificate fails
  with `SuccessorMissing` (patent-ops at 2 instances: a successor of state 1,494,178 by
  `DecideFiling`). Adding a state with one changed byte fails too, since its successors are not
  certified.
- **Cost.** The check takes 2 to 3 times the run of the same cone. Raw certificates are 66 MB for
  1.5 million states. Packing them as the store does (46 bytes to 76 bits per state on patent-ops) would make them
  about 5 times smaller.
- **Limits.** The checker computes successors with the same step relation as the run, so it does
  not catch a fault in the builder (residual risk 1). It catches faults in the search, the store,
  the parallel exploration and the caps. It covers state rules, not `after` assertions or
  properties. It is an example, not a flag of `verify`: wiring it in as `verify --certify DIR` is
  the remaining step.

## T17: the requirements-to-code chain

`check --code` generates again every file named in a project's `[code]` tables and compares it with
what is on disk (`fresh`). It then runs the compiler on a probe (`build`), and runs a conformance
test that replays the model's traces against the service (`run`) ([cli-reference.md, `check
--code`](../cli-reference.md#check---code)). codegen is experimental, so the claim is scoped to
what the tests show.

**Case study.** The repository's tests carry a complete chain on one model
(`crates/rdra-ish-cli/tests/fixtures/codegen/conformance.rdra`, an order with payment, shipment and
cancellation). `codegen --code` generates the Rust domain code and the trace file. A reference
service crate implements the use cases. Each test then plants one kind of drift between the model
and the code, and `check --code` must report it as a named diagnostic. We ran both test binaries
with `RDRA_REQUIRE_CODEGEN_TOOLS=1`, so that a missing toolchain fails instead of skipping: 26 of 26
pass (`cargo test -p rdra-ish-cli --test codegen_code_build --test codegen_code_check`, 57 s including
the build).

| Level | Planted drift | Reported as |
|---|---|---|
| `fresh` | the generated domain file edited by hand | edited file, by its text |
| `fresh` | the trace file edited | edited traces, by their hash |
| `fresh` | the model changed after generation | model changed |
| `fresh` | other generator options in `rdra.toml` | generator changed |
| `fresh` | a generated file deleted | missing file, with what writes it |
| `build` | use case `Ship` not implemented | `CodeMissingUseCase_Ship`, mapped from rustc E0277 to the model line of `usecase Ship` |
| `build` | `Ship`'s implementation empty | `CodeMissingUseCase_Ship` (E0046) |
| `build` | `commit_cancel` missing | `CodeIncomplete_Cancel` (E0046) |
| `build` | `Pay` takes a bare integer for `OrderId` | `CodeSignature_Pay` (E0053) |
| `run` | the service skips the core of `Pay` | `CodeConformance`: the store differs from the model after `Pay` |
| `run` | the service drops a row | `CodeConformance`: a `Shipment` row the model has and the service lacks |
| `run` | the service accepts `Cancel` where the model's guard refuses it | `CodeConformance`: the model refuses `Cancel 101` here, and the service ran it |
| `run` | the service's preparation step writes unconditionally | `CodeConformance`: prepared before `Place` changed a row it writes |
| `run` | the service ignores the trace's ids | the self-check of the conformance run fails |

**Scope of the claim.** Every planted drift of these kinds is detected on this one model. The
conformance level replays traces generated from the model's own state space up to a cap
(`conformance_states`), so it covers behaviours the traces reach, not all behaviours. A capped suite
warns, and `--deny-incomplete` fails it. codegen and `check --code` are experimental. The case study
ran first on Rust. The same planted omissions and faults for TypeScript
(`codegen_code_build_ts`, 7 tests) and Go (`codegen_code_check_go`, 10 tests, including a probe
compiled for every sample) also pass: 17 of 17 with `tsc` and `go` required, 175 s. Paper 1 and Paper 2 make no claim about
the code chain. A paper on it would need models and services from outside the project (T9).

## T18: agent feedback study (protocol only)

Kept separate from the papers, as the issue asks. A protocol for a later short paper:

- **Question.** Do coding agents write correct lifecycle models more often when they see the
  verifier's output (shortest counterexamples, `not decided` notes) than when they see only parse
  and type errors?
- **Tasks.** The five domain models' PRDs (`samples/*/prd.md`) with the model removed. Each task asks
  for the model and its rules.
- **Arms.** (A) agent with `rdra-ish check` only; (B) agent with `check` and `verify`, whose shortest
  counterexamples it may read; (C) B plus the `rdra-ish-verify` skill of `skills/`.
- **Measures.** Rules that hold in the reference model and fail in the agent's (found by running
  the reference model's `verify` on the agent's model, and the converse), the number of iterations,
  and tokens.
- **Threats.** Contamination: the samples are public. Use held-out PRDs written for the study.

Not run.

## T9: models not written by the authors

All nine sample models and the 91 corpus cases were written by the project's authors. The five
domain models cite public sources for their business rules (statutes, official guidance, the
OpenID specifications), but the modelling choices are the authors'. Paper 1 states this as threat
T1.

What would close it:

1. **Outside modellers.** Ask modellers outside the project to write models from the same PRDs
   (`samples/*/prd.md`), and run the evaluation on their models. This needs contributors, who
   cannot be recruited from this repository.
2. **Published case studies.** Translate published lifecycle case studies from the artifact-centric
   literature (for example the BPM benchmarks VERIFAS was evaluated on) into rdra-ish. The
   translation is still done by the authors, but the requirements are someone else's.

Neither was done. T9 remains open, and the papers say so.
