# Existing measurements and whether the paper can cite them (roadmap T5)

<!-- derived-from ../../design/instance-symmetry.md -->
<!-- derived-from ../../design/native-bfs-parallel.md -->
<!-- derived-from ../../theory/cones.md -->
<!-- derived-from ../../theory/tlc-agreement.md -->
<!-- derived-from ../../../proofs/lean/README.md -->

This file collects the quantitative measurements that the repository already records, for
the evaluation of Paper 1 (issue #594). It was compiled from main at `9e4a123`
(2026-11-28). The sources are `docs/design/*.md`, `docs/theory/*.md`,
`docs/formal-verification.md`, `docs/engine-agreement.md`, `docs/cli-reference.md`,
`proofs/lean/README.md`, `CHANGELOG.md`, `.github/workflows/ci.yml` and
`git log --format='%h %ad %s%n%b' --date=short`. No number here was measured again for
this file. The only numbers this file adds are the Lean line and declaration counts in L1,
which come from `wc` and `grep` on the tree.

## How to read the tables

- **Number**: the figure as the source records it.
- **Setup**: the command, flags and bounds when the source gives them. "n=3" means
  `--instance-count 3`.
- **When**: the commit the source names, or the date of the change that recorded it.
- **Machine**: as the source states it. "not stated" means the source does not say.
  Several design notes use `/usr/bin/time -l`. That is the BSD/macOS form of the flag, so
  those runs were very likely on macOS. This file marks that as *inferred*.
- **Verdict**:
  - **citable** means that the setup is precise, a command can reproduce it, and the code
    path is still the current one. For a state, orbit or class count, which BFS fixes
    independently of thread count and machine, it also means "re-run once at the commit
    the paper pins". Later changes to the samples or the builder can move a count
    (for example #477 on insurance-claim-ops' entity cones, or P67 on the extended
    sub-lifecycle).
  - **re-measure** means at least one of these: the setup is missing, the code is older
    than a change that affects the number, the number is a wall time taken under
    uncontrolled load or on macOS only, the run was a single run, or the number comes
    from a prototype or probe that is not the shipped code.
  - **not for the paper** marks a number that is valid but outside Paper 1's claims. Each
    such row names the topic it belongs to.

The status of instance symmetry matters for every row in the first table. `verify
--symmetry` defaults to `never` on main. `always` is experimental, and the change of the
default is still open (`docs/design/instance-symmetry.md`, status of 2026-11-26;
`CHANGELOG.md`, Unreleased).

## 1. Instance symmetry (`G_s`, `G_T`, `H`)

| # | Number | What was measured | Setup | When | Machine | Verdict and reason |
|---|---|---|---|---|---|---|
| S1 | Plain runs stopped at 25.3M (`Inv_Appointment_0`, `_1`), 25.9M (`Unique_Encounter_Appointment`), 27.0M (`Inv_Encounter_2`), 27.9M (`Forbidden_ClinicalOrder_1`) states; insurance-claim-ops `Forbidden_ClaimCase_3` over 45M; patent-ops cones at 33M–41M | Where an unreduced `verify` stops at the memory limit (#212) | n=3, `--memory 8GiB` | `450558e2` (2026-11-24) | not stated | **re-measure.** A stop point is a function of the bytes stored per state and of the room each cone gets. Both changed after `450558e2`: a stopped entity cone now gives back its room (#361), and later store changes followed. These numbers show only that the cones do not fit. They are not sizes. |
| S2 | 240,016 orbits (`Inv_Appointment_0`, `_1`, `Unique_Encounter_Appointment`); 1,696,486 (`Inv_Encounter_2`); 682,640 (insurance-claim-ops `Forbidden_ClaimCase_3`) | Orbit counts of complete runs up to `G_s`. First counted by #212's prototype, then reproduced exactly by the shipped canonicalizer (phase 1 exit) | n=3, `--memory 8GiB`, `--symmetry always` | prototype at `450558e2`; phase 1 at #311 (`40e3783a`, 2026-11-25) | not stated | **citable.** The canonical form is exact (`docs/design/instance-symmetry.md`, "The canonical form"), so the count is a property of the model and the bounds. Re-run at the pinned commit, because insurance-claim-ops changed under #477. There is no exact plain count to set beside these numbers: the plain runs do not finish (S1). |
| S3 | `about 820M states` for `Inv_Encounter_2` | The orbit-size estimate that the cone line prints (sum of `\|G_r\|/\|Aut(r)\|`) | `verify samples/clinic-ops --instance-count 3 --memory 8GiB --symmetry always` | example in `docs/cli-reference.md` at `9e4a123` | not stated | **re-measure.** This is an estimate. It is an upper bound that also counts unreachable images (S18). It is not a plain count, and the example output in the reference may not come from a recorded run. |
| S4 | Sub-lifecycle S of clinic-ops `Forbidden_ClinicalOrder_1`: 61,849,224 states and 20 classes; the reduced cone has 5,050 states | Size of S under a plain run, and of the #74 quotient (#284) | n=3, `--memory 32GiB` | #284 comment, around 2026-11-24 | not stated | **citable** for the counts (the quotient is deterministic). Re-run at the pinned commit. |
| S5 | Over 41.1M orbits, incomplete | insurance-claim-ops `Forbidden_Claim_4`, unreduced cone up to `G_s` | n=3 | #212 | not stated | **re-measure.** This is a lower bound from a run that stopped. |
| S6 | 15–22× (22.5×, 21.3×, 12.4×) | #212 prototype's reduction on patent-ops' reduced cones, with S permuted too | n=3 | #212 | not stated | **re-measure (do not cite).** It uses a prototype group that permutes S. That is not the shipped `G_T`, which fixes S (`Rule::Reduced`). The shipped factors are S7. |
| S7 | 5.5× on `Forbidden_PatentApplication_1` (29,680,875 states / 5,413,500 orbits); 5.2× on `Unique_Patent_PatentApplication` (5,237,935 / 1,009,760); 3.0× on `Forbidden_DraftingOrder_0` (48,315 / 15,940) | Plain states over orbits up to `G_T` on #74-reduced cones (phase 3) | n=3 | phase 3 (#324, #328), 2026-11-25 | not stated | **citable.** Both sides are complete runs, so both counts are exact. This is the cleanest symmetry factor on record. Re-run at the pinned commit. |
| S8 | `Forbidden_Patent_4`: plain stopped at 40.8M; up to `G_T`, 31,926,220 orbits (about 260M states), `ok`, 98 s. `Forbidden_OfficeAction_3`: stopped at 67,108,864 orbits (about 360M states), not decided. `GrantEnds` reduced cone: 7,346,935 orbits (about 87M states) | Phase 3 results at n=3 under 8 GiB | `verify --instance-count 3 --memory 8GiB --symmetry always --entity <E>`, `RDRA_JOBS=4` | 2026-11-25 | not stated | **citable** for the orbit counts and verdicts. **re-measure** the 98 s (a wall time from one run with no load figure). The "about" figures are estimates (S18). `Forbidden_OfficeAction_3` is a documented negative result: `G_T` moves only `OfficeAction`, so the factor is at most 3! = 6. |
| S9 | `GrantEnds`: at n=2, 75,017 plain states against 30,313 orbits; at n=3, 7,346,935 orbits with 52.7M edges and no orbit self-edge, and the property decided `ok` (plain stops at 40.0M) | First liveness run on the annotated quotient (phase 4, #352) | n=2 and n=3, 8 GiB | 2026-11-25/26 | not stated | **citable.** This is the only recorded liveness property decided up to symmetry at n=3. It has no orbit self-edges, so it does not exercise the self-edge case of A23. Re-run at the pinned commit. |
| S10 | Before `Shared::new`: insurance-claim-ops `ClaimEventuallyDecided` plus `Cone_Forbidden_Claim_4` cost +16.2% under `always` (319.20G to 370.77G instructions) | Cost of running one system twice up to symmetry | `RDRA_JOBS=1`, instructions retired | phase 4 first unit | inferred macOS (`/usr/bin/time -l`) | **not for the paper.** The defect it shows was fixed. |
| S11 | On 43 sample pairs at their `// expect verify` flags: `never` +0.36% against main (627.70G to 629.98G instructions; at most +0.61%), `always` +0.45%; `always` against `never` +0.49% (patent-ops +0.88%, insurance-claim-ops +0.86%, clinic-ops −0.13%, oidc-auth −0.04%) | Overhead of the phase 4 code on models where symmetry does little | `RDRA_JOBS=1`, instructions retired | phase 4 first unit, 2026-11-25/26 | inferred macOS | **re-measure.** Instruction counts do not depend on load, so the method is the right one. The code has changed since: the partition certificate, run-time hypothesis checks, `AbsentOwner` (P30), and the stub fix (P67). The paper's "overhead when nothing permutes" figure should come from a run at the pinned commit with this method. |
| S12 | 628.64G (`never`) against 696.82G (`always`), +10.8%; patent-ops +53.7%, insurance-claim-ops +15.1%; largest peak RSS 1,195 MiB against 1,197 MiB; the same verdicts on all 43 pairs | `always` before step 1 (reuse of plain results) | `RDRA_JOBS=1`, instructions | `7e464c43` (2026-11-25) | inferred macOS; load past 18 for wall times, which the source therefore leaves out | **re-measure (superseded).** Step 1 removed most of the cost (S11). Only the statement "the same verdicts on all 43 pairs" carries over, and the shadow and rule 8 runs in CI check that continuously. |
| S13 | patent-ops n=3: `PatentApplication` `never` 341 s wall, 333 s user, 15.0 GB, against `always` 67 s, 96 s, 12.5 GB, 1,649G instructions. `Patent` `never` not decided at 111,509,504 states, 832 s, 19.1 GB, against `always` `ok` at 31,926,220 orbits, 98 s, 13.0 GB, 3,559G. `OfficeAction` `always` 262 s, 13.4 GB, 9,067G, not decided | Time and peak RSS, `never` against `always` | `--instance-count 3 --memory 8GiB --entity <E>`, `RDRA_JOBS=4`; `never` under `timeout`, so it has no instruction count | `7e464c43` (2026-11-25) | inferred macOS | **citable** for verdicts and counts. **re-measure** times and RSS: single runs, no load figure, and on macOS. The RSS figures exceed `--memory` because of #237 (A26: the limit bounds stores, not the process). |
| S14 | 1,159 ns per stored state for the orbit run against 742 ns for a plain run (1.6×) | Per-state cost of canonicalization, patent-ops safety pass | n=2, `RDRA_PERF=1 RDRA_JOBS=1` | `7e464c43` | inferred macOS | **re-measure.** A nanosecond timing from one run, and the step 1 fast paths 2(a) and 2(b) landed after it. |
| S15 | 75.4K instructions per successor for canonicalization against 2.5K for a plain successor | Canonicalizer cost on clinic-ops `Inv_Encounter_2` | `RDRA_JOBS=1`, instructions | #311 (2026-11-25) | not stated | **re-measure.** It predates the step 1 fast paths. The phase 1 exit target ("a fifth of the prototype") has no recorded result beside this number. |
| S16 | Prototype: 4–40 µs per state on one thread; 70–190 thousand successors per second on 8 threads. The plain run of S made 268M edges in 35 s | #212 §5 and #284 | clinic-ops cones, n=3 | 2026-11-24 | not stated | **re-measure (do not cite).** These are prototype numbers. |
| S17 | Runs of S that stop at the memory limit at n=3 took 35 s and 61 s | Cost of the plain attempt before running S up to `H` | n=3 (#284) | 2026-11-24 | not stated | **re-measure.** Wall times with no setup. |
| S18 | Estimate 253 for 243 states (food-permit-ops `Forbidden_BusinessPermit_4`); 7.3K for 6,125 (`Inv_PermitApplication_2`); 4 of the 41 reachable states of `presence_first_absent_asymmetry` have a swap that no run reaches | Error of the `about` estimate, and the `FirstAbsent` asymmetry | samples at their flags; corpus case at n=2 | 2026-11-25 | n/a (deterministic) | **citable.** "4 of 41" is a fact about the model and is the paper's motivating example for `G_s`. The estimate errors are exact counts. |
| S19 | insurance-claim-ops `MultiForbidden_Payment_3`: 779,267 orbits for 783,427 states | A cone where the group moves almost nothing | sample flags | 2026-11-25 | n/a | **citable** as a count. It is a useful "symmetry buys nothing here" data point. Re-run at the pinned commit. |
| S20 | clinic-ops entity cones, n=3: `always` leaves 3 of the 17 findings `never` leaves open. `Appointment` plain stopped at 100,133,268 states, while up to `G_s` it finishes in 14,346,108 orbits (about 3.9G states). `Encounter` 1,696,486, `PatientAccount` 3,885,148, `PaymentTransaction` 1,022,613 orbits; `EligibilityCheck` stops again at 25,681,245 orbits. Peak RSS 37.1 GB against 35.6 GB | Phase 4 second unit (#370), entity cones | n=3, `--memory 8GiB`, `RDRA_JOBS=8` | 2026-11-26 | not stated | **citable** for the counts and the open-finding counts. **re-measure** RSS (single run, process above the limit, #237). |
| S21 | 117 open findings fell to 17 when a stopped entity cone gives back its room; the run took 5,578 s where it took 1,707 s, and peaked at 35.6 GB where it peaked at 29.2 GB | Effect of #361, not of symmetry | clinic-ops, n=3, 8 GiB | 2026-11-25 | not stated | **not for the paper**, except as context for S20. These are wall times from single runs. |
| S22 | "16 to 108 times fewer stored states" | Expected gain of phase 1 | ratio of S1 stop points to S2 orbit counts | design estimate | n/a | **re-measure (do not cite).** The plain side is a stop point, not a size, so the ratio is only a lower bound on the factor, and the source labels it as expected. |
| S23 | 15 reduced cones of the corpus and samples run up to `G_T`; 11 rule reasons have no witness (7 `Reduced`, 4 `Observed`); 9 scan rules fire in no cone of the samples or the corpus | Coverage of the per-rule gate | `explore/symmetry/snapshot.rs` | 2026-11-25 to 11-28 | n/a | **citable** as evidence counts at `9e4a123`. These numbers belong in a threats-to-validity paragraph: the scan rules are never exercised by a real model. |
| S24 | 507 reachable states on which every gated read equals its value on the canonical form | Test of (G6) outside guards (`explore/symmetry/g6.rs`) | 2 instances | `4b46e5c1` (2026-11-28) | n/a | **citable** as a test size. It is one built model. |
| S25 | `JOINT_CAP` 5,040, `BRANCH_CAP` 4,096, twin bound `\|G_s\| ≤ 6^4 = 1,296`, about 135 KB canonicalizer scratch (computed) | Constants of the canonicalizer | code | `9e4a123` | n/a | **citable** as configuration, not as measurements. |

## 2. The sub-lifecycle reduction (#74), its certificates and checks

| # | Number | What was measured | Setup | When | Machine | Verdict and reason |
|---|---|---|---|---|---|---|
| R1 | clinic-ops `verify --deny-undecided`: about 40 s and 1.4 GB, against 2.5 min and 3.7 GB before. `Charge` cone: over 8,388,608 states to 38,533. `Prescription`: 4,266,635 to 1,690. patent-ops `OfficeAction`: 4,818,909 to 1,341,705 | First release of the reduction | n=2 (sample flags of the time) | v1.2.0 (2026-11-05) | not stated | **re-measure.** The samples, the event semantics (#112), the reduction and the store have all changed since. These are the historical headline numbers. The paper needs the same table at the pinned commit. |
| R2 | Of 89 sample targets whose cone `verify` reduced, 49 had a sub-lifecycle that quotients to itself | How often the reduction is a no-op (#101) | `lifecycle_measure` at `verify`'s bounds, 200,000 states | v1.2.0 | n/a | **re-measure.** This rate matters for the evaluation's "how often does it apply" question, and it is from old code. |
| R3 | patent-ops `verify --deny-undecided` about 30 s, 0.75 GB; `OfficeAction` 1,341,707 reduced (over 8,388,608 unreduced); `PatentApplication` 293,833 and `Patent` 202,569 reduced. insurance-claim-ops about 60 s; claim cones 754,247 reduced | Sample sizes after the reduction | sample flags | v1.2.0 | not stated | **re-measure.** Same reasons as R1. |
| R4 | clinic-ops `EligibilityCheck` 905,528 (nothing reduces); patent-ops `OfficeAction` 1,341,707 reduced (185 classes on the application's side); food-permit-ops `BusinessPermit` 4,648,771 reduced (444 classes); insurance-claim-ops 633,787 to 903,987 reduced; oidc-auth over 2,000,000 (nothing reduces) | Cones too large for TLC in `engine_equivalence`, and why | `docs/formal-verification.md`, the cones TLC skips | current text at `9e4a123` | n/a | **citable** for the reduced counts and class counts after a re-run. The unreduced sizes are not recorded ("over"). These cones are the evaluation's natural "reduction is necessary" and "reduction does not apply" cases. |
| R5 | food-permit-ops `BusinessPermit` view grew from 10 vars and 64 states to 33 vars and 5,874 states; a reduction of 1,314 states to 15 with a 61-state, 11-class sub-lifecycle; `Claim` view 836,427 reduced with S of 63,823 states and 443 classes; `Appointment` view halved (188,699 to 97,139) | Effect of the event-order design (#112) on cone sizes | mostly 1 instance, Int 0..5, cap 1,000,000, `lifecycle_measure` | design rounds, before v2.0.0 | 18 cores, 128 GB | **re-measure.** These are design-time numbers for a semantics that has since been revised. |
| R6 | 53 reductions (10 of them nested) checked step by step against their unreduced cone (Lemma 6 and the A8 relation); 25 more reduced sample cones pass 200,000 states unreduced and are not checked | Coverage of `reduce/lemma6.rs::check` (`core/tests/reduce_steps.rs`) | corpus at its flags, sample roots at their `// expect verify` flags | `900456b0`, `e4000b12` (2026-11-28) | CI (`formal-verify`, release) | **citable** as a coverage count. It is also a limitation to report: the largest reductions are exactly the 25 that are not checked. |
| R7 | Random models: 522 PR seeds compared, 284 of them reduce a cone; 45 main seeds with TLC, 33 reduce. The Lemma 6 sweep over the 522 PR seeds: 361 reduce a cone, about 2 min locally. TLC seeds take 7–9 min on the runner | `scripts/reduction-diff.py` sweeps | `run` and `lemma6`, seeds in `scripts/reduction-diff-pr-seeds.txt` and in `ci.yml` | 2026-11-25 (comment in `ci.yml`) | GitHub Actions (ubuntu) | **citable** for the counts. The generator and the seeds are fixed, and CI reruns them on every PR (floors 500/200). `tlc-agreement.md` notes that the counts themselves are a single recorded run. The 284 and 361 differ because they come from different runs and checks; the paper should re-count both at the pinned commit. |
| R8 | The partition certificate takes 25–35% of the partition's time, at most 0.45 s CPU (food-permit-ops) in a 3 s `verify` | Cost of the run-time stability certificate (P7) | samples, release build | `447f20a7` (2026-11-28) | not stated | **re-measure.** This is the only overhead figure for a run-time certificate, and it has no command, no per-sample table and no machine. The paper's "cost of certification" claim needs a controlled measurement: instructions at `RDRA_JOBS=1` with the certificate on and off. |
| R9 | `TLA_CELL_BUDGET` 530,000 cells. 2,428 classes print 72,840 cells and run in a 1 GB TLC heap; the `ClaimCase` view's 4,368 classes print 104,832 cells at a 912 MB peak; a random model's 24,000 classes print 326,400 cells and run out of heap. food-permit-ops `AfterAssert_SuspendBusiness_0` at n=3: 28,928 classes, 520,704 cells, a 7.6 MB module, 7,167,444 states, which TLC checks in 98 s at `-Xmx4g` and runs out of heap on at `-Xmx1g` and `-Xmx2g` | Size of the reduced cone's TLA+ tables | TLC heap as stated | #284, #309 (2026-11-24/25) | not stated | **citable** for the cell and class counts. **re-measure** TLC times (TLC version and machine not stated). This is relevant if the paper reports that TLC checks the same reduced cone. |
| R10 | Escalation memory factor `GROWTH = 10`: one instance more multiplied the store by 1.1–6.4, two more by 1.4–10.1 | Store growth with the bound, on the corpus's escalated cases | doc comment of `cli/src/verify/escalate.rs::GROWTH` | before 2026-11-28 | n/a | **re-measure.** It has no command, and it concerns the prover's bound escalation (A25), not the reductions. |

## 3. Cones of influence and property cones

| # | Number | What was measured | Setup | When | Machine | Verdict and reason |
|---|---|---|---|---|---|---|
| C1 | patent-ops `GrantEnds` on a 4,105-state cone, while its whole model explored 14 million states in 11 minutes without finishing; food-permit-ops `HygieneManagerEventuallyAppointed` on a 257-state cone, while the whole model found 6.9 million states in 300 s without finishing | Property cones against the whole model (TLC whole-model runs) | sample flags of the time | v1.0/v1.1 (2026-11-03/04) | not stated | **re-measure.** The models and the cone construction changed. This is the motivating "cone versus whole model" figure; the paper should take it again with the native whole-model run at the pinned commit. |
| C2 | Property cones: `GrantEnds` reduced 75,017 states and 321,548 edges, against 553,643 states and 3,447,460 edges unreduced; insurance-claim-ops `ClaimEventuallyDecided` reduced 633,787 states and 3,192,700 edges; `SubrogationEnds` reduced 372 states; the unreduced insurance cones exceed 3,000,000 states and 13,800,000 edges. Self-loops are up to 29% of edges (1,007,716 of 3,447,460) | Graph sizes of property cones, reduced and unreduced | throwaway probe like `lifecycle_measure`, BFS, adjacency and Tarjan, cap 3,000,000 | design of native temporal verification (before v3.0.0) | 18-core Apple silicon, 128 GB | **re-measure.** The probe is not the shipped checker. The reduced-against-unreduced ratio (7.4× in states on `GrantEnds`) is worth re-taking with `verify` at the pinned commit. |
| C3 | `Caps::VERIFY` raised from 2,097,152 to 8,388,608; clinic-ops `Prescription` 4,266,635 states, `Charge` 6,629,659, at 2 instances; `verify samples/clinic-ops` about 2.5 min and 3.5 GB | Largest cones before the reduction | n=2 | v1.0.0 era | not stated | **re-measure (historical).** |
| C4 | patent-ops `OfficeAction` cone 4,818,909 states at 2 instances; `verify` about 75 s, up from 4 s | Effect of restoring two guards | n=2 | v0.4.0 era | not stated | **not for the paper** (historical). |
| C5 | A run that stops at the memory limit was measured at up to 1.85 times the limit | Process memory against `--memory` (#237) | not stated | v2.3.0 (2026-11-18) | not stated | **re-measure.** If the paper describes the memory limit, it must say that the limit bounds stores, not the process (A26). |

## 4. Parallel BFS (#296)

All rows in this section come from `docs/design/native-bfs-parallel.md` or from the
CHANGELOG entries that quote it. Their stated machine is an "M5 Max with 18 cores and
128 GB", running macOS (it uses `sample` and `taskpolicy`). The 1-minute load was between
1.8 and 9. Each row is a median of three or of five runs. Parallel BFS is engineering, not
a claim of Paper 1. The paper needs at most one sentence on it: the output is the one-thread
output at every job count.

| # | Number | What was measured | Setup | When | Verdict and reason |
|---|---|---|---|---|---|
| P1 | food-permit-ops / clinic-ops / insurance-claim-ops at 1 job: 89.8G / 95.8G / 313.3G instructions; 5.02M / 5.63M / 9.87M states; 40.9M / 36.9M / 50.3M edges; 27 / 50 / 21 BFS runs | Totals | `verify samples/<name>` at each `// expect` default, release | `c51f2543` (2026-11-25) | **re-measure.** The counts are machine-independent, but they predate 3.1 and later changes. |
| P2 | At 8 jobs: food-permit-ops 1.94×, clinic-ops 2.17×, insurance-claim-ops 3.14× (wall time, median of three; 1-job time 4.15 s / 4.17 s / 12.83 s) | Speedup before 3.1 | `RDRA_JOBS` 1–18 | `c51f2543` | **not for the paper** (superseded by P5). |
| P3 | `Bfs::take` is 36.0% / 32.0% / 23.8% of a 1-job run; at 8 jobs the reader spends 89% of its samples in `take`, and the workers wait 81–83% | Profile | `sample` at 1 ms, `#[inline(never)]` build | `c51f2543` | **not for the paper** (macOS profiler, pre-3.1). |
| P4 | Hits are 80–88% of edges; 30–57% hit a state admitted inside the worker's window; 13–43% hit a state admitted by the same chunk | Look-ahead hit rates | `perf` build with a patch to `perf.rs` that is not for merge | `c51f2543` | **re-measure.** The patch is not in the tree, so the number cannot be reproduced. |
| P5 | Part 3 at 8 jobs: food-permit-ops 2.10 s to 1.81 s (33% of main's 1-job time; 30% at 6 jobs), clinic-ops 1.74 s to 1.58 s (28%), insurance-claim-ops 4.23 s to 3.52 s (24%); instructions +0.3% to +0.8%. The owner accepted 3.1 at 31% / 28% / 25–28% | Speedup of 3.1 | interleaved, median of three or five | 2026-11-26 | **re-measure** if cited: these are macOS wall times under load 3–9. |
| P6 | Probes on: 12% faster on food-permit-ops at 8 jobs, 4–5% slower on the other two; the shared table took 87 ms against hashbrown's 59 ms in a microbenchmark | Part 2, which release builds leave off | as stated | 2026-11-26 | **not for the paper.** |
| P7 | Follow-up (a): 1% fewer cycles at 1 job, equal at 8; follow-up (b): not taken (table of builds `a1`, `b0`–`b3`) | Table rebuild, prefetch | interleaved | 2026-11-26 | **not for the paper.** |
| P8 | Default QoS 4.15 s; `taskpolicy -c utility` 3.99 s; `taskpolicy -b` 16.0 s (3.9× slower, 1.4 GHz) | Apple efficiency cores | 1 job | `c51f2543` | **not for the paper** (macOS only). |
| P9 | clinic-ops `verify` from 73 s to 28 s on 4 cores | First parallel cones | v2.2.0 era | 2026-11-16 | **not for the paper** (historical). |
| P10 | stdout and stderr byte-identical at 1, 2, 4, 6, 8 and 18 jobs; memory stop points the same at 1, 3 and 8 jobs (7, 8 and 10 MiB) | Determinism | `memory_cli` tests (main only) and the design's runs | 2026-11-25 to 28 | **citable.** It is a tested property, not a timing, and `cli/tests/memory_cli.rs` reruns it. |

## 5. The z3 prover (`--prove`)

These rows are outside Paper 1's thesis. They are listed because the task asked for every
measurement and because a paper on the prover (a later paper) would need them.

| # | Number | What was measured | Setup | When | Machine | Verdict and reason |
|---|---|---|---|---|---|---|
| Z1 | 150 s to 68 s of proof over every sample on one solver (clinic-ops 66 s to 16 s, food-permit-ops 33 s to 12 s, oidc-auth 29 s to 22 s), the same `Proof` | MBQI off in the Houdini rounds (#421) | one solver | Unreleased (2026-11-2x) | not stated | **not for the paper**; re-measure for the prover paper. |
| Z2 | SMT-LIB text: clinic-ops 113 MB to 14 MB, insurance-claim-ops 108 MB to 12 MB, food-permit-ops 75 MB to 9 MB, patent-ops 18 MB to 3 MB; wall time 27–36% lower | Activation literals (#421) | `RDRA_JOBS=4`, medians of 3 | Unreleased | not stated | **not for the paper.** The byte counts are deterministic; the times are not. |
| Z3 | A rerun with the certificate cache takes 25–54% less wall time | Proof cache | `RDRA_JOBS=4`, medians of 3 | Unreleased | not stated | **not for the paper.** |
| Z4 | Within 2% after the two-column parent lemma (clinic-ops 6.56 s to 6.70 s, etc.); unpruned, the view cost food-permit-ops 16%; insurance-claim-ops proves all 22 rules without its `.lemma()` | #428 | `RDRA_JOBS=4`, cache off, medians of 3 | Unreleased | not stated | **not for the paper.** |
| Z5 | A check that keeps its candidate spends at most 161,000 rlimit units for a rule (1/124 of its limit) and 116,000 for a lemma (1/17); on 4 solvers a lemma check spent 144,000 (1/14) | Resource-limit headroom | z3 5.1.0, every sample on 2 solvers, some on 1 and 3 | `docs/theory/z3.md` at `91524792` | n/a (rlimit is load-independent) | **citable** for a prover paper. `proof_equivalence` enforces the tenth-of-limit bound on every run. |
| Z6 | Escalation on insurance-claim-ops with its lemma dropped stops at 3 instances after about 40 s at 340 MB; a refuted rule adds about 9 s instead of 24 s | Bound escalation, early refutation | `RDRA_JOBS=4` | Unreleased | not stated | **not for the paper.** |
| Z7 | z3 uses about 0.3–0.5 GiB per process | Memory outside `--memory` | not stated | v3.0.0 era | not stated | **re-measure** if cited. |
| Z8 | 20 Kani harnesses; the `one_of` harness takes about 90 s and 10 GB under CBMC | Bounded proofs of Rust functions | `cargo kani` | `4b58c0e1` (2026-11-28) | macOS arm64 | **citable** for the count. **re-measure** the time (macOS). |

## 6. Agreement with TLC, mutation runs and test counts

| # | Number | What was measured | Setup | When | Machine | Verdict and reason |
|---|---|---|---|---|---|---|
| T1 | Kill rate 66.5% (139 of 209 viable; 251 mutants, 137 caught, 2 timeouts, 70 missed, 42 unviable). Missed by file: `cone.rs` 27, `explore/liveness.rs` 26, `verify/native.rs` 12, `property_cone.rs` 5 | `cargo mutants`, liveness configuration, killed by `engine_corpus` with TLC and z3 off `PATH` | `mutants.yml`, `.cargo/mutants.toml`; Actions run 45212589802 | `b0f593ba` (run dated 2026-11-23 in the doc; commit dated 2026-11-24) | GitHub Actions | **citable** as a recorded run. It is a lower bound on the mutation score (equivalent mutants count as missed). It does not cover `reduce/` or `explore/symmetry/`, so it is no evidence for either reduction. |
| T2 | Kill rate 62.9% (151 of 240; 298 mutants, 147 caught, 4 timeouts, 89 missed, 58 unviable). Missed: 32, 38, 17, 2 | same configuration | Actions run 46034352562 | `6f2d29a1` (run dated 2026-11-27; commit dated 2026-11-28) | GitHub Actions | **citable** with the same limits as T1. The rate fell between the two runs while the mutant count grew; the paper should not present either run as a trend. |
| T3 | 571 TLC-side mutants; no run completed, so no kill rate | `.cargo/mutants-tlc.toml` | `cargo mutants --list` | `91524792` | n/a | **citable** as "not measured". |
| T4 | 40 sample roots, 89 corpus cases, 23 temporal cases, the 36 `bfs_and_tlc_*` tests of `engine_equivalence.rs`, and 45 random models with TLC | Inputs of the TLC agreement evidence | `docs/theory/tlc-agreement.md`, threats to validity | `91524792` | n/a | **citable** as counts at that commit. Re-count at the pinned commit. |
| T5 | 30 `VERIFY_REDUCED_ONLY` cones in the samples; 25 fallback cones at 2 instances in `engine_equivalence`; a cone over 300,000 states at 2 instances is not handed to TLC, and one over 2,000,000 states would be a TLC run of over an hour | What TLC does not see | `engine_equivalence` | `91524792` | not stated | **citable** for the counts. They bound the TLC evidence for the reductions: the refinement check runs on the corpus only. |
| T6 | 17 TLC runs of at most 1,680 table states, four at a time: about 14 s | Cost of the TLC refinement check of `R` against `C` | release jobs | `91524792` | "locally" | **re-measure** the time. The run count is citable. |
| T7 | Release gate of v3.0.0, native against TLC (wall time / peak RSS): patent-ops 1.0 s / 187 MB against 33.3 s / 795 MB; food-permit-ops 2.7 s / 681 MB against 24.0 s / 1.61 GB; insurance-claim-ops 5.9 s / 867 MB against 127.0 s / 2.10 GB; the five small samples under 0.01 s / 10 MB against 3.7–7.1 s / 122–152 MB | Native checker against `verify --backend tlc` | release build, default bounds; TLC 1.7.4 | v3.0.0 (2026-11-26); the TLC column is older | 18-core Apple silicon, 128 GB, one run each | **re-measure.** These are single runs on macOS. The TLC column used TLC 1.7.4 and the `--backend tlc` path, while the flake now pins TLC 1.8.0 on JDK 25. The two columns are not from the same commit, and the TLC runs include JVM start-up. |
| T8 | TLC alone on `Cone_ClaimEventuallyDecided` (633,787 distinct states, 3,131,413 generated): 75.5 s / 1.80 GB with `-workers 1`, 22.7 s / 2.33 GB with `-workers auto`; the native probe took 1.06 s | One property cone, TLC against BFS | 4 GB heap | before v3.0.0 | 18 cores, 128 GB | **re-measure.** The native side was a probe that decided nothing. That TLC finds the same 633,787 distinct states is a citable agreement datum. |
| T9 | Two TLC runs at once took 60 min and eight at once took 30; heaps of 8g and 16g gave the same time, 5g was 15% slower; 25–30 of 64 cores were busy with 2 × 32 workers; an example line shows `Cone_Big`: 12,787,852 states, 1412.0 s, `-Xmx16g`, 8 workers | TLC scheduling (#384, #309) | 64 cores, `--memory 96GiB` | 2026-11-26/27 | 64 cores | **not for the paper.** |
| T10 | The `--with-tlc` sample step was 17% slower in one run (6 min 7 s against 5 min 13 s); patent-ops 7.6 s against 7.3 s | TLC scheduling change | one run | 2026-11-27 | 18 cores | **not for the paper.** |

## 7. Lean

| # | Number | What was measured | Setup | When | Machine | Verdict and reason |
|---|---|---|---|---|---|---|
| L1 | 41 `.lean` files (37 under `RdraIsh/` and 4 at the top), 14,460 lines; 628 lines that begin a `theorem` declaration (lemmas declared another way are not counted); no `sorry`, `admit` or `axiom` in the sources (the only match of `sorry` is the word `sorryAx` in a comment of `CheckAxioms.lean`); Lean 4.36.0, no Mathlib | Size of the mechanization | `find proofs/lean -name '*.lean' \| xargs wc -l`; `grep -E '^\s*(private \|protected )?theorem '`; `scripts/check-lean-proofs.sh` checks the axioms | `9e4a123` (2026-11-28), counted for this file | n/a | **citable** for files, lines and the axiom check. The declaration count is a rough `grep`: re-count with Lean's environment before citing a theorem count. |
| L2 | A clean `lake build` takes about 2 s | Build time | `scripts/check-lean-proofs.sh` | README at `9e4a123` | not stated | **re-measure** (a timing with no machine). |
| L3 | `lemma5-check`: about 3 s for the Lean part, 0.03 s of CPU for the check | Lean-side certificate checker for Lemma 5 | README | `9e4a123` | not stated | **re-measure** if cited. |
| L4 | `partition-check` walks 182 flat runs (60 distinct, 344,409 states, the largest 85,154). It leaves out 78 nested runs, 24 identity partitions, 36 runs stopped at the cap and 6 unobserved sub-lifecycles, and no run is up to `H`. The dump is about 27 MB; the check takes 1.0 s CPU; the dump takes about 7 s in a `ci-checked` build (about 170 s in debug) and peaks at about 2.9 GB. 8 planted faults, one per certificate part, are each rejected | Lean-verified checker of the partition certificate on the samples' runs | `scripts/` walk with `RDRA_LEAN_RUNS_CAP` | README at `9e4a123` | not stated | **citable** for the coverage counts. **re-measure** the times. The left-out runs (nested, capped, up to `H`) are a limitation the paper must state: the Lean-checked certificate does not cover them. |
| L5 | Mechanized in Lean: A8 (flat case), A15 (flat case), A18 (on a slot model whose (G6) is stronger than the scan's), A20 (subgroup), A21 (invariants), A23 (annotated quotient), N9 (fair-component search). On paper or sketched only: nested reductions, S run up to `H`, the depth claim of A18 step 7, entity-cone findings (deadlocks) up to symmetry, and the reading of the Rust code as the model | Scope of the mechanization | `docs/theory/cones.md`, `docs/theory/README.md` | `9e4a123` | n/a | **citable** as the paper's statement of scope. It is not a measurement, but every "mechanized" claim in the paper must match it. |

## 8. Other topics (not for Paper 1)

These numbers are recorded in the design notes and the CHANGELOG. They concern code
generation, the conformance stream and the LSP check, which Paper 1 does not cover.

- `docs/design/code-check.md`: generation plus `codegen --check` in 10 ms (ec-site) and
  39 ms (clinic-ops); `cargo check` 0.4 s, `go vet` 0.2 s, `tsc` 0.12 s on the probes;
  clinic-ops traces reach 178 MB at the cap; a tree-sitter spike binary of 4.4 MB.
- `docs/design/conformance-stream.md` (at `f9925df7`, Apple silicon, 18 cores, load
  about 5): clinic-ops' joint BFS does not fit in 4 GiB; about 4.2×10^8 moves; 59,228,160
  states discovered by the stop; 707 s wall time; 5.7 GB peak RSS.
- `docs/design/lsp-lifecycle-check.md`: the check takes 29 ms / 64 ms / 0.04 s and 24 MB on
  clinic-ops, and similar on two other samples, under a 2 s and 512 MiB budget.
- `docs/design/verus.md`: `library.rdra` verifies in about 0.6 s.
- `CHANGELOG.md`: the conformance self-check's mutant pick went from 299 s and 487 s to
  0.36 s and 0.33 s; TypeScript traces of 602 MB now run; the plugin clone falls from 27 MB
  to 1.3 MB.
- `docs/design/event-order.md`: sample `verify` times of 18 s to 600 s under the event-order
  variants (18 cores, 128 GB, loads 1.7 to 5.0). These were design-round measurements of a
  semantics that has since been revised.

## What the evaluation must measure again

The paper's evaluation should come from one pinned commit, on one stated machine, with each
run alone. It should report instructions retired at `RDRA_JOBS=1` (which do not depend on
load, per `CLAUDE.md`) and state and orbit counts, and give wall times only as medians with
the load stated. Measuring on Linux would match the CI runner. The following figures have
no citable version yet:

1. **Reduction factors of the sub-lifecycle reduction at the current code**: per sample
   cone, unreduced states (where a run completes), reduced states, classes, and whether a
   nested reduction applied. This replaces R1–R3 and R5, and it should also re-take R2
   (how often the quotient is the identity).
2. **Symmetry factors with exact plain counts**: S7 is the only factor on record with both
   sides complete. Repeat it, and add the n=3 cones where both a plain run (at a larger
   `--memory`) and the orbit run finish. Do not use stop points (S1) or prototype factors
   (S6, S16, S22) as a denominator.
3. **Overhead when nothing permutes or nothing reduces**: instructions at `RDRA_JOBS=1`,
   `--symmetry never` against `always`, on every sample pair (S11's method at the pinned
   commit, which now includes the run-time checks added on 2026-11-28).
4. **Cost of the run-time certificates and of counterexample replay**: the partition
   stability certificate (R8), `loop_faults`, and the lift-and-replay of each printed
   trace, each as a share of the run in instructions.
5. **Liveness up to symmetry**: S9 is a single property with no orbit self-edge. Measure
   `eventually` and `leads_to` cones up to symmetry on every sample at n=3, and report how
   many lassos needed unrolling past one copy of the loop.
6. **Native against TLC on the same reduced cones**: verdict agreement and state counts at
   the pinned commit, with the TLC version the flake pins (1.8.0) and the JVM start-up
   reported separately (T7, T8).
7. **Cone against whole model for properties** (C1, C2), with the native whole-model run.
8. **Peak RSS against `--memory`**, if the paper reports memory at all (S13, S20, C5).
9. **Mutation runs over `reduce/` and `explore/symmetry/`**: no kill rate exists for either
   reduction (T1–T3 mutate other files). If the paper uses mutation scores as evidence for
   the reductions, a run over those directories is needed.
10. **Lean build and checker times** (L2–L4), on the evaluation machine, if the paper
    states them.
