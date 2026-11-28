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

## Local run with TLC: per-cone states, verdicts and times

The CI logs above hold no per-cone numbers. This section adds them from a local run. TLC 1.7.4
comes from nixpkgs (`nix shell nixpkgs#tlaplus`; the flake's 1.8.0 build needs GitHub access that
this machine lacks). docs/theory lists 1.7.4 as a supported version. `scripts/paper/tlc-local.py`
runs `verify SPEC --with-tlc --memory 12GiB` and `verify SPEC` without TLC, both at the default
`RDRA_JOBS` on the 4-core machine. It pairs every native cone line with TLC's line for the same
cone. Data: [data/tlc-local.tsv](./data/tlc-local.tsv). Main at `9e4a123`.

**Agreement.** Both engines decided 50 cones and whole-model property runs. On all 50 they report
the same verdict and the same number of states. TLC also ran 36 entity-cone state models
(412,613 states in all), which check deadlocks and witnesses, and found what the native checker
found. The one difference in the `result` lines is expected. On insurance-claim-ops the native
checker reports the sample's expected deadlock (`Deadlock_ClaimCase_status_assessed`), while
`tlc_result` is `ok`. TLC's deadlock check is the other reading of D25 (a state with no successor),
and claim T6 records it as one-sided.

**What the state counts compare.** Every cone that TLC checks is the module that `verify` prints
for it: a reduced cone is printed reduced, with the quotient tables, and an orbit run is not
printed. So equal counts confirm that the two engines read the printed system alike. They do not
test the reduction, whose independent tests are the reduced-against-unreduced comparisons
([ablation.md](./ablation.md); `reduction-diff.py` in CI). Without the reduction, the native
checker does not finish four of the domain models in 300 s. TLC would explore at least the same
unreduced cones, since `engine_equivalence` asserts equal graphs on unreduced cones.

**Time.** TLC's time is the sum of its per-cone times, as `verify` prints them. Each includes a JVM
start of about 0.7 s. `verify` runs TLC jobs beside the native checker, so the wall time of
`verify --with-tlc` is less than the summed TLC time.

| Model | Cones compared | States (both engines) | TLC time, summed (s) | `verify --with-tlc` wall (s) | `verify` wall, native only (s) |
|---|--:|--:|--:|--:|--:|
| ec-site | 1 | 82 | 0.7 | 2.2 | 0.01 |
| personal-info | 2 | 26 | 1.6 | 2.1 | 0.01 |
| incremental-order | 1 | 9 | 0.7 | 2.0 | 0.01 |
| patent-ops | 9 | 1,684,427 | 55.5 | 25.0 | 1.6 |
| clinic-ops | 11 | 135,583 | 33.9 | 23.6 | 4.9 |
| food-permit-ops | 8 | 47,200 | 23.9 | 23.0 | 7.2 |
| insurance-claim-ops | 10 | 2,116,505 | 211.6 | 88.8 | 16.0 |
| oidc-auth (1 instance) | 8 | 111,376 | 19.5 | 38.2 | 3.2 |

On the domain models, `verify` with TLC beside it takes 3 to 16 times the wall time of the native
run alone. On patent-ops, TLC spends 55.5 s on cones that the native checker decides, with
everything else, in 1.6 s. The native run's wall time also covers the entity cones, the
sub-lifecycle runs and the whole-model property runs, so per-cone native time is lower still. A
per-state comparison would need per-cone native times, which `verify` does not print. The native
checker stores states exactly and runs several cones at once, each on several threads (`RDRA_JOBS`
defaults to the core count). TLC runs with fingerprints and 1 worker per cone (2 for the
whole-model runs), as its lines print.

**TLC with symmetry sets.** Not measured, and not sound here. The printed modules use ordered integer
slots (`1..InstanceCount`) and create in the lowest absent slot (`FirstAbsent`). A `SYMMETRY`
declaration over those slots would merge states that the creation step tells apart. The corpus
case `presence_first_absent_asymmetry` shows a verdict that changes, and TLC's documentation
warns that liveness checking under symmetry can miss violations.

**Alloy, Apalache.** Not run. An encoding of these models in Alloy 6 or Apalache would have to be
written by hand. The TLA+ printer alone is 2,632 lines of Rust, which bounds the effort from below.

### Table

| Model | Cone | Native | Native states | TLC | TLC states | TLC time (s) |
|---|---|---|--:|---|--:|--:|
| samples/ec-site | Cone_Unique_Payment_Order | ok | 82 | ok | 82 | 0.7 |
| samples/ec-site | StateModel_Order | - | - | state model | 82 | - |
| samples/personal-info | Cone_Unique_PersonalInfo_UserAccount | ok | 1 | ok | 1 | 0.8 |
| samples/personal-info | Cone_Unique_PasswordCredential_UserAccount | ok | 25 | ok | 25 | 0.8 |
| samples/personal-info | StateModel_UserAccount | - | - | state model | 16 | - |
| samples/incremental-order/step-7-business-rules/src | Cone_Forbidden_Store_0 | ok | 9 | ok | 9 | 0.7 |
| samples/incremental-order/step-7-business-rules/src | StateModel_Store | - | - | state model | 9 | - |
| samples/patent-ops | Cone_Forbidden_DraftingOrder_0 | ok | 745 | ok | 745 | 1.1 |
| samples/patent-ops | Cone_Forbidden_PatentApplication_1 | ok | 47593 | ok | 47593 | 5.2 |
| samples/patent-ops | Cone_Forbidden_InventionDisclosure_2 | ok | 931 | ok | 931 | 1.1 |
| samples/patent-ops | Cone_Forbidden_OfficeAction_3 | ok | 1341707 | ok | 1341707 | 22.8 |
| samples/patent-ops | Cone_Forbidden_Patent_4 | ok | 202569 | ok | 202569 | 10.1 |
| samples/patent-ops | Cone_Inv_InventorReward_3 | ok | 87 | ok | 87 | 1.0 |
| samples/patent-ops | Cone_Unique_PatentApplication_DraftingOrder | ok | 761 | ok | 761 | 1.2 |
| samples/patent-ops | Cone_Unique_Patent_PatentApplication | ok | 15017 | ok | 15017 | 2.3 |
| samples/patent-ops | Cone_GrantEnds | ok | 75017 | ok | 75017 | 10.7 |
| samples/patent-ops | StateModel_DraftingOrder | - | - | state model | 745 | - |
| samples/patent-ops | StateModel_InventionDisclosure | - | - | state model | 931 | - |
| samples/patent-ops | StateModel_InventorReward | - | - | state model | 87 | - |
| samples/clinic-ops | Cone_Forbidden_ClaimStatement_0 | ok | 223 | ok | 223 | 1.3 |
| samples/clinic-ops | Cone_Inv_Appointment_0 | ok | 15374 | ok | 15374 | 4.7 |
| samples/clinic-ops | Cone_Inv_Appointment_1 | ok | 15374 | ok | 15374 | 4.0 |
| samples/clinic-ops | Cone_Inv_Encounter_2 | ok | 85154 | ok | 85154 | 9.8 |
| samples/clinic-ops | Cone_Inv_ClinicalOrder_3 | ok | 250 | ok | 250 | 1.8 |
| samples/clinic-ops | Cone_Inv_Prescription_4 | ok | 450 | ok | 450 | 2.0 |
| samples/clinic-ops | Cone_Inv_ClaimStatement_5 | ok | 223 | ok | 223 | 1.3 |
| samples/clinic-ops | Cone_Unique_PatientProfile_PatientAccount | ok | 3 | ok | 3 | 1.3 |
| samples/clinic-ops | Cone_Unique_Encounter_Appointment | ok | 18499 | ok | 18499 | 4.8 |
| samples/clinic-ops | Cone_Unique_AccountBalance_PatientAccount | ok | 1 | ok | 1 | 1.1 |
| samples/clinic-ops | Cone_Unique_FollowUpMessage_CarePlan | ok | 32 | ok | 32 | 1.8 |
| samples/clinic-ops | StateModel_CarePlan | - | - | state model | 381 | - |
| samples/clinic-ops | StateModel_ClaimStatement | - | - | state model | 315 | - |
| samples/clinic-ops | StateModel_ClinicalOrder | - | - | state model | 250 | - |
| samples/clinic-ops | StateModel_Encounter | - | - | state model | 85154 | - |
| samples/clinic-ops | StateModel_IntakePacket | - | - | state model | 245 | - |
| samples/clinic-ops | StateModel_PaymentTransaction | - | - | state model | 78752 | - |
| samples/clinic-ops | StateModel_Prescription | - | - | state model | 1690 | - |
| samples/food-permit-ops | Cone_Forbidden_Disposition_0 | ok | 174 | ok | 174 | 1.7 |
| samples/food-permit-ops | Cone_Forbidden_BusinessPermit_4 | ok | 243 | ok | 243 | 1.5 |
| samples/food-permit-ops | Cone_Inv_Disposition_0 | ok | 23034 | ok | 23034 | 4.8 |
| samples/food-permit-ops | Cone_Inv_PermitApplication_1 | ok | 5885 | ok | 5885 | 3.8 |
| samples/food-permit-ops | Cone_Inv_PermitApplication_2 | ok | 6125 | ok | 6125 | 3.3 |
| samples/food-permit-ops | Cone_MultiForbidden_BusinessPermit_1 | ok | 613 | ok | 613 | 1.1 |
| samples/food-permit-ops | Cone_AfterAssert_SuspendBusiness_0 | ok | 10883 | ok | 10883 | 5.6 |
| samples/food-permit-ops | Cone_HygieneManagerEventuallyAppointed | ok | 243 | ok | 243 | 2.1 |
| samples/food-permit-ops | StateModel_BusinessNotification | - | - | state model | 43 | - |
| samples/food-permit-ops | StateModel_ChangeNotice | - | - | state model | 3032 | - |
| samples/food-permit-ops | StateModel_Consultation | - | - | state model | 157 | - |
| samples/food-permit-ops | StateModel_Inspection | - | - | state model | 23601 | - |
| samples/food-permit-ops | StateModel_MonitoringPlan | - | - | state model | 7 | - |
| samples/food-permit-ops | StateModel_SiteVisit | - | - | state model | 55 | - |
| samples/food-permit-ops | StateModel_SuccessionNotice | - | - | state model | 1640 | - |
| samples/insurance-claim-ops | Cone_Forbidden_Complaint_2 | ok | 483 | ok | 483 | 1.5 |
| samples/insurance-claim-ops | Cone_Forbidden_ClaimCase_3 | ok | 63823 | ok | 63823 | 8.9 |
| samples/insurance-claim-ops | Cone_Forbidden_Claim_4 | ok | 633787 | ok | 633787 | 55.6 |
| samples/insurance-claim-ops | Cone_Forbidden_RepairEstimate_5 | ok | 441 | ok | 441 | 1.6 |
| samples/insurance-claim-ops | Cone_MultiForbidden_JibaisekiSurvey_2 | ok | 65 | ok | 65 | 1.5 |
| samples/insurance-claim-ops | Cone_MultiForbidden_Payment_3 | ok | 783427 | ok | 783427 | 59.2 |
| samples/insurance-claim-ops | Cone_MultiForbidden_RepairEstimate_4 | ok | 65 | ok | 65 | 1.3 |
| samples/insurance-claim-ops | Cone_MultiForbidden_Subrogation_7 | ok | 255 | ok | 255 | 2.8 |
| samples/insurance-claim-ops | Cone_ClaimEventuallyDecided | ok | 633787 | ok | 633787 | 76.4 |
| samples/insurance-claim-ops | Cone_SubrogationEnds | ok | 372 | ok | 372 | 2.8 |
| samples/insurance-claim-ops | StateModel_Complaint | - | - | state model | 39205 | - |
| samples/insurance-claim-ops | StateModel_JibaisekiSurvey | - | - | state model | 189 | - |
| samples/insurance-claim-ops | StateModel_RepairEstimate | - | - | state model | 441 | - |
| samples/insurance-claim-ops | StateModel_Subrogation | - | - | state model | 372 | - |
| samples/oidc-auth --instance-count 1 | Cone_Forbidden_AuthorizationRequest_0 | ok | 8 | ok | 8 | 0.9 |
| samples/oidc-auth --instance-count 1 | Cone_Forbidden_Interaction_1 | ok | 27013 | ok | 27013 | 3.7 |
| samples/oidc-auth --instance-count 1 | Cone_Forbidden_BackchannelRequest_3 | ok | 35 | ok | 35 | 0.9 |
| samples/oidc-auth --instance-count 1 | Cone_MultiForbidden_LogoutNotice_4 | ok | 10 | ok | 10 | 0.9 |
| samples/oidc-auth --instance-count 1 | Cone_Quantifier_Grant_0 | ok | 30277 | ok | 30277 | 4.6 |
| samples/oidc-auth --instance-count 1 | Cone_Quantifier_DeviceAuthorization_1 | ok | 7 | ok | 7 | 0.7 |
| samples/oidc-auth --instance-count 1 | Cone_Unique_Grant_AuthorizationRequest | ok | 27013 | ok | 27013 | 3.9 |
| samples/oidc-auth --instance-count 1 | Cone_Unique_AuthorizationCode_Grant | ok | 27013 | ok | 27013 | 3.9 |
| samples/oidc-auth --instance-count 1 | StateModel_AccessToken | - | - | state model | 30277 | - |
| samples/oidc-auth --instance-count 1 | StateModel_AuthorizationCode | - | - | state model | 30277 | - |
| samples/oidc-auth --instance-count 1 | StateModel_AuthorizationRequest | - | - | state model | 27013 | - |
| samples/oidc-auth --instance-count 1 | StateModel_BackchannelRequest | - | - | state model | 35 | - |
| samples/oidc-auth --instance-count 1 | StateModel_Client | - | - | state model | 5 | - |
| samples/oidc-auth --instance-count 1 | StateModel_Consent | - | - | state model | 27013 | - |
| samples/oidc-auth --instance-count 1 | StateModel_DeviceAuthorization | - | - | state model | 15 | - |
| samples/oidc-auth --instance-count 1 | StateModel_Grant | - | - | state model | 30277 | - |
| samples/oidc-auth --instance-count 1 | StateModel_LogoutNotice | - | - | state model | 14 | - |
| samples/oidc-auth --instance-count 1 | StateModel_OpSession | - | - | state model | 7 | - |
| samples/oidc-auth --instance-count 1 | StateModel_RefreshToken | - | - | state model | 30277 | - |
| samples/oidc-auth --instance-count 1 | StateModel_SigningKey | - | - | state model | 4 | - |
