# Ablation: cone, sub-lifecycle reduction, symmetry (roadmap T6)

**Question.** What does each reduction buy, in states, CPU and memory, and does any of them change
a verdict?

**Configurations.** Every check runs on a cone of influence in all four configurations. The
configurations switch the other two reductions:

| Configuration | Sub-lifecycle reduction | Instance symmetry |
|---|---|---|
| `cone` | off (`RDRA_ISH_NO_REDUCE=1`) | off (`--symmetry never`) |
| `cone+reduce` | on (the default) | off (the default) |
| `cone+sym` | off | on (`--symmetry always`) |
| `cone+reduce+sym` | on | on |

A fifth column, *no cone*, is BFS on the whole model with no cone, no reduction and no symmetry
(`crates/rdra-ish-core/examples/paper_whole.rs`, under a state cap). Data:
[data/ablation.tsv](./data/ablation.tsv) and [data/whole.tsv](./data/whole.tsv). Bounds: the
`verify` default of 2 instances per entity and Int 0..5, except oidc-auth at 1 instance (at 2 it
needs more than this machine's memory, about 19.5 GB at the default limit, per the CLI reference).
Timeout 300 s per run, `--memory 12GiB`, `RDRA_JOBS=1`.

## Results

**Whole model, no cone.** On every domain model BFS on the whole model passes 10 million states
without finishing (patent-ops in 77 s, food-permit-ops in 86 s, insurance-claim-ops in 102 s,
oidc-auth at 1 instance in 98 s). clinic-ops did not reach the cap within 400 s. ec-site passes 20
million states in 83 s, while all of its cones together hold 711 states. Only the three smallest
models finish whole (personal-info 209 states, incremental-order 36, api-contract 1).

**Cone and sub-lifecycle reduction.**

| Model | `cone`: result, CPU, peak RSS | `cone+reduce`: result, CPU, peak RSS | Largest run with reduction |
|---|---|---|--:|
| patent-ops | timeout at 300 s, 5.3 GB | ok, 4.0 s, 163 MB | 1,341,707 |
| clinic-ops | timeout at 300 s, 7.4 GB | ok, 10.6 s, 549 MB | 2,238,170 |
| food-permit-ops | timeout at 300 s, 7.4 GB | ok, 20.0 s, 745 MB | 4,846,569 |
| insurance-claim-ops | timeout at 300 s, 5.4 GB | counterexample (expected), 42.3 s, 805 MB | 4,580,087 |
| oidc-auth (1 instance) | ok, 0.6 s, 30 MB | ok, 4.8 s, 304 MB | 1,028,160 |
| four small samples | ok, under 0.01 s | ok, same states | 532 at most |

- On the four larger domain models, `verify` finishes only with the reduction. Without it, each
  run is stopped by the 300 s timeout after 5.3 to 7.6 GB. This table cannot say how long an
  unreduced run would take. The states column of a stopped run counts only the BFS runs that
  finished before the timeout. That is why food-permit-ops shows 43 states: its first large cone
  did not finish.
- insurance-claim-ops reports `Deadlock_ClaimCase_status_assessed`, the counterexample its sample
  header expects (`// expect verify: fail Deadlock_ClaimCase_status_assessed`).
- **The reduction can cost.** On oidc-auth at one instance, every cone is small (30,277 states at
  most), but the extended sub-lifecycle `X` that the reduction explores first has 1,028,160 states.
  `X` leaves the inputs free, so it can be larger than the cone it is meant to shrink. The
  reduction gives up afterwards, and the run costs 4.8 s instead of 0.6 s. This is the case that
  an explore-budget heuristic for `X` would address.
- On the four small samples the reduction does nothing: no cone has an independent sub-lifecycle
  worth splitting.

**Symmetry.** At 2 instances, `--symmetry always` changes the states by 0 to 16% and the CPU by
less than the noise:

| Model | States, `cone+reduce` | States, `cone+reduce+sym` | Fewer |
|---|--:|--:|--:|
| ec-site | 711 | 665 | 6.5% |
| personal-info | 118 | 103 | 12.7% |
| incremental-order | 19 | 16 | 15.8% |
| patent-ops | 2,017,009 | 1,942,129 | 3.7% |
| clinic-ops | 5,617,011 | 5,495,861 | 2.2% |
| food-permit-ops | 7,009,703 | 6,998,263 | 0.2% |
| insurance-claim-ops | 13,499,587 | 13,444,059 | 0.4% |
| oidc-auth (1 instance) | 1,345,035 | 1,345,035 | 0% |

At two slots per entity the largest orbit has two members, and the tool applies the group only to
some cones: the safety cones (on a reduced cone, up to the group of its `T` rows), `always` on
cones the reduction leaves whole, and `eventually` and `leads_to`. With the reduction off, symmetry
does not rescue the timed-out runs either: `cone+sym` stops at the same point as `cone`. The
symmetry factor grows with the instance count, which [scaling.md](./scaling.md) measures. At one
instance, `G_s` is trivial.

**Verdicts.** No configuration changes a verdict. Every run that finished printed the same `result:`
line as every other finished run of its model. The same failing checks were reported, the
expected deadlock on insurance-claim-ops and nothing elsewhere. Each finished domain-model run
matches its sample's `// expect` header. The *Same verdicts* column below compares each run with
the `cone` run, and shows `-` where that run timed out.

## Threats

- One run per cell, on a 4-core VM. The CPU and RSS columns would move between runs; the states
  and verdict columns would not ([determinism.md](./determinism.md)).
- The timeout hides how far the unreduced runs are from finishing. The whole-model column shows
  the scale: more than 10 million states on every domain model.
- The `perf` build adds counters to every BFS run. It changes no verdict and no state count.

## Full table

| Model | Configuration | Result | States (all runs) | Largest run | Peak RSS (MB) | User CPU (s) | Same verdicts |
|---|---|---|---:|---:|---:|---:|---|
| samples/ec-site | cone | ok | 711 | 532 | 15 | 0.01 | yes |
| samples/ec-site | cone+reduce | ok | 711 | 532 | 15 | 0.01 | yes |
| samples/ec-site | cone+sym | ok | 665 | 532 | 15 | 0.0 | yes |
| samples/ec-site | cone+reduce+sym | ok | 665 | 532 | 15 | 0.01 | yes |
| samples/personal-info | cone | ok | 118 | 45 | 15 | 0.01 | yes |
| samples/personal-info | cone+reduce | ok | 118 | 45 | 15 | 0.01 | yes |
| samples/personal-info | cone+sym | ok | 103 | 45 | 15 | 0.01 | yes |
| samples/personal-info | cone+reduce+sym | ok | 103 | 45 | 15 | 0.01 | yes |
| samples/api-contract | cone | ok | 1 | 1 | 15 | 0.0 | yes |
| samples/api-contract | cone+reduce | ok | 1 | 1 | 15 | 0.0 | yes |
| samples/api-contract | cone+sym | ok | 1 | 1 | 15 | 0.0 | yes |
| samples/api-contract | cone+reduce+sym | ok | 1 | 1 | 15 | 0.0 | yes |
| samples/incremental-order/step-7-business-rules/src | cone | ok | 19 | 9 | 15 | 0.0 | yes |
| samples/incremental-order/step-7-business-rules/src | cone+reduce | ok | 19 | 9 | 15 | 0.0 | yes |
| samples/incremental-order/step-7-business-rules/src | cone+sym | ok | 16 | 9 | 15 | 0.0 | yes |
| samples/incremental-order/step-7-business-rules/src | cone+reduce+sym | ok | 16 | 9 | 15 | 0.0 | yes |
| samples/patent-ops | cone | timeout | 329,978 | 323,071 | 5255 | 296.32 | - |
| samples/patent-ops | cone+reduce | ok | 2,017,009 | 1,341,707 | 163 | 3.96 | - |
| samples/patent-ops | cone+sym | timeout | 329,978 | 323,071 | 5575 | 295.61 | - |
| samples/patent-ops | cone+reduce+sym | ok | 1,942,129 | 1,341,707 | 163 | 3.65 | - |
| samples/clinic-ops | cone | timeout | 120,873,184 | 105,791,435 | 7435 | 294.5 | - |
| samples/clinic-ops | cone+reduce | ok | 5,617,011 | 2,238,170 | 549 | 10.61 | - |
| samples/clinic-ops | cone+sym | timeout | 120,873,184 | 105,791,435 | 7578 | 294.58 | - |
| samples/clinic-ops | cone+reduce+sym | ok | 5,495,861 | 2,238,170 | 549 | 12.31 | - |
| samples/food-permit-ops | cone | timeout | 43 | 43 | 7372 | 295.59 | - |
| samples/food-permit-ops | cone+reduce | ok | 7,009,703 | 4,846,569 | 745 | 19.97 | - |
| samples/food-permit-ops | cone+sym | timeout | 43 | 43 | 7372 | 295.64 | - |
| samples/food-permit-ops | cone+reduce+sym | ok | 6,998,263 | 4,846,569 | 745 | 20.02 | - |
| samples/insurance-claim-ops | cone | timeout | 93,277,095 | 93,277,095 | 5443 | 295.99 | - |
| samples/insurance-claim-ops | cone+reduce | counterexample | 13,499,587 | 4,580,087 | 805 | 42.26 | - |
| samples/insurance-claim-ops | cone+sym | timeout | 93,277,095 | 93,277,095 | 5355 | 295.65 | - |
| samples/insurance-claim-ops | cone+reduce+sym | counterexample | 13,444,059 | 4,580,087 | 805 | 42.3 | - |
| samples/oidc-auth --instance-count 1 | cone | ok | 316,826 | 30,277 | 30 | 0.63 | yes |
| samples/oidc-auth --instance-count 1 | cone+reduce | ok | 1,345,035 | 1,028,160 | 304 | 4.82 | yes |
| samples/oidc-auth --instance-count 1 | cone+sym | ok | 316,826 | 30,277 | 30 | 0.63 | yes |
| samples/oidc-auth --instance-count 1 | cone+reduce+sym | ok | 1,345,035 | 1,028,160 | 304 | 4.86 | yes |
