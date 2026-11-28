# Baseline: TLC beside the native checker (roadmap #594, T8)

This file records what GitHub Actions logs say about TLC and the native BFS checker run on
the same inputs. TLC cannot run in the container this file was written in, so every number
comes from CI logs. The log lines are in [`data/tlc-ci-lines.txt`](./data/tlc-ci-lines.txt);
`scripts/paper/tlc-baseline.py` turns them into the tables below, and
`python3 -I scripts/paper/tlc-baseline.py --check docs/paper/eval/tlc-baseline.md` checks that
the tables here are its output.

## Sources

| | Run A (used first) | Run B (repeat) |
|---|---|---|
| Workflow run | [46114833478](https://github.com/manji-0/rdra-ish-dsl/actions/runs/46114833478) | [46086823346](https://github.com/manji-0/rdra-ish-dsl/actions/runs/46086823346) |
| Event, branch | `workflow_dispatch`, main | `workflow_dispatch`, main |
| Commit | `1d69695d7ff38b5665cebb217ecdc7f58091e4d3` (merge of #564) | `9857092ae33cd9c1dc4b1812a28cdba9b79a720e` (merge of #544) |
| Started | 2026-11-28 06:03 UTC | 2026-11-28 00:14 UTC |
| Run conclusion | failure: only the `check` job's step "no test a pull request runs takes over 10 s" failed | success |
| `formal-verify` job | 140886222125, success | 140798265053, success |
| `engine-equivalence` job | 140886222137, success | not used |
| Runner | `ubuntu-latest` (GitHub-hosted) | `ubuntu-latest` (GitHub-hosted) |
| TLC | 1.8.0 on JDK 25, from the flake's devShell (`tlaplus-1.8.0-jdk25`) | same |
| Binary | `target/ci-checked/rdra-ish` (`--profile ci-checked`) | same |

Run A is the newest main run whose `formal-verify` and `engine-equivalence` jobs succeeded.
Its overall conclusion is `failure` because of the test-time gate of the `check` job, which
runs no TLC. Run B is the run the task named; it is used only as a repeat of the
`formal-verify` numbers. Commit `1d69695` is an ancestor of `9e4a123`, the tree this file was
written on.

## What the logs contain, and what they do not

The per-cone numbers this baseline was meant to hold (native states, TLC distinct and
generated states, TLC and native time per cone) are **not in the CI logs**:

- `scripts/verify-samples.sh` captures each `verify` run's output in a temporary file and
  prints one line per run: verdict against the `// expect` header, root, flags, wall time and
  peak RSS (`measure` and the `printf` at the end of the loop). It prints the output itself
  only on a mismatch. So the `Cone_<check> ...: ok (<n> states)` lines of the native checker
  and the `tlc: Cone_<check> ...: ok (<n> states, <t> s, -Xmx..., <w> workers)` lines of TLC
  never reach the log.
- `reduction-diff.py run` prints one line per seed (`same` and the seed's wall time) and a
  JSON summary.
- `engine_equivalence` and `engine_corpus` are `cargo test` binaries; a passing test prints
  only `... ok`. `a_bfs_and_tlc_reach_the_same_states_and_verdicts` asserts that TLC's
  distinct-state count and graph equal BFS's on every compared cone, but prints neither.

So the tables below hold what the logs do give: per `verify --with-tlc` run, the agreement
with the header and the wall time and peak RSS of the native checker and TLC together; per
random model, agreement; per test binary, pass counts. There is no TLC-only or native-only
time for the same run, so no time ratio between the engines comes from these logs.

Where a state count would come from, if recorded:

- TLC's `tlc:` line prints TLC's **distinct** states (`tlc_output.rs::distinct_states`,
  `verify/tlc.rs`). TLC's generated-state count is parsed nowhere and printed nowhere by
  `verify`.
- The native line prints the states BFS stored for the cone (orbit representatives under
  `--symmetry always`; the samples run with the default, `never`).

To fill the per-cone table, one of these would be needed: a CI step (or a `verify-samples.sh`
option) that prints the `Cone_`/`tlc:` lines of each TLC run; the outputs uploaded as an
artifact; or a machine where TLC runs, running `verify --with-tlc` on the eight runs of
Table 1 at their header flags. This file does not make that change.

## Tables

`Agrees with header` means `verify-samples.sh` printed `ok`: the exit status, the
`tlc_result:` line and the set of error names match the `// expect` header. A disagreement
between the engines prints `error[EngineDisagreement]` and makes both result lines
`tool_error`, which no header expects, so `ok` also means the engines did not disagree on any
check both decided.

<!-- tlc-baseline.py: begin -->

Table 1. `verify --with-tlc` runs of `verify-samples.sh` (native checker and TLC in one process tree; wall time and peak RSS cover both).

| Root | Flags beyond the root | Agrees with header | Wall s, run 46114833478 (`1d69695`) | Peak RSS MB | Wall s, run 46086823346 (`9857092`) | Peak RSS MB |
|---|---|---|---:|---:|---:|---:|
| samples/clinic-ops | (header default) | yes | 24.79 | 699 | 25.46 | 720 |
| samples/patent-ops | (header default) | yes | 23.84 | 651 | 21.88 | 644 |
| samples/food-permit-ops | (header default) | yes | 22.44 | 814 | 20.86 | 812 |
| samples/insurance-claim-ops | (header default) | yes | 105.98 | 1170 | 104.06 | 1176 |
| samples/oidc-auth | `--instance-count 1 --prove --deny-undecided` | yes | 49.36 | 500 | 46.07 | 499 |
| samples/oidc-auth | `--deny-undecided --entity BackchannelRequest --entity DeviceAuthorization --entity LogoutNotice --entity SigningKey` | yes | 1.25 | 179 | 1.22 | 180 |
| samples/formal-verification/partial_scope.rdra | `--entity Order` | yes | 0.29 | 93 | 0.24 | 93 |
| samples/formal-verification/counting_rules.rdra | `--instances Loan=4` | yes | 1.48 | 121 | 1.41 | 121 |
| **Total** | | | **229.43** |  | **221.20** | |

Table 2. All runs of the step.

| CI run | Roots | `verify` runs with TLC | `verify` runs native only | `check` runs | Mismatches | Summary line |
|---|---:|---:|---:|---:|---:|---|
| 46114833478 (`1d69695`) | 40 | 8 | 35 | 40 | 0 | `verify-samples: 40 roots, 83 runs, 0 mismatches` |
| 46086823346 (`9857092`) | 40 | 8 | 35 | 40 | 0 | `verify-samples: 40 roots, 83 runs, 0 mismatches` |

Table 3. `reduction-diff.py run --with-tlc` on the 45 fixed main seeds.

| CI run | Seeds compared | Seeds that reduce a cone | Nested | At 3 instances | `same` | Diverged | Disagreed (`EngineDisagreement`) | Sum of seed wall s | Longest seed s |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 46114833478 (`1d69695`) | 45 | 33 | 12 | 10 | 45 | 0 | 0 | 650.5 | 67.5 (seed 38) |
| 46086823346 (`9857092`) | 45 | 33 | n/a | 10 | 45 | 0 | 0 | 619.3 | 63.9 (seed 38) |

Table 4. TLC test binaries of the `engine-equivalence` job.

| CI run | Test binary | Passed | Failed | Ignored | Wall s |
|---|---|---:|---:|---:|---:|
| 46114833478 (`1d69695`) | `engine_equivalence` | 49 | 0 | 1 | 699.75 |
| 46114833478 (`1d69695`) | `engine_corpus` | 8 | 0 | 0 | 318.48 |

<!-- tlc-baseline.py: end -->

The roots in Table 1 are the five of `scripts/tlc-roots.txt` (the roots with a
`VERIFY_FALLBACK` cone in `engine_equivalence`) and the two flagged header lines elsewhere
(`partial_scope.rdra --entity Order`, `counting_rules.rdra --instances Loan=4`). The other 35
`verify` runs of the step run native only (label `(native)` in the log). Between the two runs,
wall times of the runs over one second differ by at most 9% (patent-ops 23.84 s against
21.88 s), and the totals by 3.7%.

### Reading the numbers

- **Agreement.** In run A, 8 of 8 `verify --with-tlc` runs on samples, 45 of 45 random models
  (each run reduced and unreduced, each with TLC), the 49 tests of `engine_equivalence` that
  ran, and the 8 tests of `engine_corpus` passed. No log line reports a disagreement between
  the engines. Run B gives the same counts for the steps it was read for.
- **Cones.** The number of cones TLC checked in these runs is not in the logs. The tests that
  compare state counts (`a_bfs_and_tlc_reach_the_same_states_and_verdicts`, every cone of the
  40 sample roots at 1 and 2 instances with the exceptions that
  [tlc-agreement.md, "The sample comparison"](../../theory/tlc-agreement.md#the-sample-comparison)
  lists) pass without printing them.
- **Time.** The 229 s of Table 1 is the native checker and TLC together. The only side-by-side
  wall times in the repository are older and from another machine (claims rows T7 and T8 of
  [evidence/measurements.md](../evidence/measurements.md): TLC 1.7.4, macOS, before v3.0.0).
  They are not a baseline at this commit. The `shadow` job's native-only run of the same
  samples (job 140886222162 of run A) is not usable as the native column: its binary compares
  every fast path with its reference twin, and it took longer than the TLC run on some roots
  (food-permit-ops 47.02 s native-with-twins against 22.44 s with TLC).

## Symmetry sets in TLC's runs

TLC ran **without** symmetry sets. The printer emits no `SYMMETRY` line in any `.cfg`
(`crates/rdra-ish-emit/src/tla/` contains no `SYMMETRY`; the only `Permutations` there is in
the list of TLC standard-module names the printer must not reuse, `names.rs`). The reasons, from
[instance-symmetry.md, "TLC and engine agreement"](../../design/instance-symmetry.md#tlc-and-engine-agreement):

- Slots are integers (`<E>_Ids == 1..InstanceCount`), not model values, and a create takes
  the lowest absent slot through `FirstAbsent` (`module.rs`: `\A j \in DOMAIN ex : j < i =>
  ex[j] # "absent"`). No permutation of all slots preserves that order, and TLC does not check
  that a spec is symmetric under the set it is given, so a `SYMMETRY` set would make TLC
  unsound on the `FirstAbsent` asymmetry of #212.
- TLC's documentation warns that liveness checking under symmetry can miss violations.

The native checker's symmetry (`--symmetry always`) permutes only the slots that are present
in a state (`G_s` fixes absent slots), which keeps `FirstAbsent` sound. TLC's runs therefore
explore each module without symmetry reduction, and serve as an independent check of the
native symmetric run rather than a copy of it. A "TLC with symmetry sets" column of T8 cannot
be produced for these specs without changing how slots are modelled.

## Which system each engine checks

- **Safety cones.** `verify --with-tlc` prints each safety cone's module from the cone as
  `verify` checks it (`verify/jobs.rs::cone_jobs`, `ts.reduced_cone_in`): reduced by its
  independent sub-lifecycle (#74) when that applies, with the quotient's `Rep_`/`Succ_`/`Rise`
  tables computed in Rust and printed into the module. TLC explores that module by its own
  BFS. So on a reduced cone TLC and the native checker explore the same reduced system, and
  TLC's agreement says nothing independent about the reduction itself; the step-by-step check
  of the reduction against its unreduced cone (`tlc_refinement_holds_on_every_reduced_cone_of_the_corpus`)
  runs on the corpus only ([tlc-agreement.md, "The reduction, step by step"](../../theory/tlc-agreement.md#the-reduction-step-by-step)).
- **Property cones.** `Cone_<P>` is the reduced cone (or the cone, when nothing reduces);
  `Cone_<P>_Full` and `Cone_<P>_WF` are printed from the unreduced cone. Runs are paired by the
  system they decide (`RunOn`). The native `WF_vars(Next)` run on the reduced cone
  (`decide_liveness_reduced`) has no TLC counterpart and meets TLC's `_WF` run only in the final
  verdict ([tlc-agreement.md, §2](../../theory/tlc-agreement.md#2-the-agreement-rules)).
- **Symmetry.** None of the runs in these logs used `--symmetry always`, so a native state
  count and a TLC count of the same cone, if printed, would count the same system. Under
  `--symmetry always` the native count would be of orbits and TLC's of states.
- **Coverage gaps.** The 13 `VERIFY_FALLBACK` cones of oidc-auth meet no TLC run at 2
  instances; of the other four roots' 12 fallback cones, only the 5 cones of invariants or
  `after` assertions get a TLC run here (tlc-agreement.md, "`verify --with-tlc` on the sample
  roots").

## Encoding effort

rdra-ish prints the TLA+ itself; the user writes none. Lines (`wc -l`) at `9e4a123`:

| File | Lines | Role |
|---|---:|---|
| `crates/rdra-ish-emit/src/tla/mod.rs` | 316 | entry points, print modes |
| `crates/rdra-ish-emit/src/tla/module.rs` | 1,278 | module and cfg printer |
| `crates/rdra-ish-emit/src/tla/names.rs` | 571 | TLA+ identifiers, reserved names |
| `crates/rdra-ish-emit/src/tla/pred.rs` | 233 | predicates |
| `crates/rdra-ish-emit/src/tla/refine.rs` | 234 | refinement module (corpus check) |
| `crates/rdra-ish-emit/src/tla/tests.rs` | 1,464 | tests |
| **Printer, without tests** | **2,632** | |
| `crates/rdra-ish-cli/src/verify/tlc.rs` | 910 | launching and scheduling TLC |
| `crates/rdra-ish-cli/src/verify/tlc_tool.rs` | 141 | finding TLC |
| `crates/rdra-ish-cli/src/tlc_output.rs` | 1,116 | parsing TLC's output and traces |
| `crates/rdra-ish-cli/src/tlc_replay.rs` | 131 | replaying TLC traces through BFS |
| `crates/rdra-ish-cli/src/tlc_env.rs` | 200 | heap and environment |
| `crates/rdra-ish-cli/src/tlc_batch.rs` | 377 | batch JVM for the tests |

The CLI files may hold inline tests; their counts are whole files.
