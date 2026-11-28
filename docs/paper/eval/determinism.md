# Determinism across thread counts and runs (roadmap T10)

**Question.** Does `verify` print the same output whatever the number of threads, and from one run
to the next? Claims N4 and N17 of [docs/theory/claims.md](../../theory/claims.md) state that any
number of threads computes the one-thread run: the same states, numbering, edges, findings and stop
point, except for a stop by the wall-clock budget. TLC's multi-worker mode does not promise this:
its state numbering and the counterexample it reports can change with the number of workers
(the issue cites tlaplus issue 883).

**Method.** `scripts/paper/eval.py determinism` runs `verify` on four models at `RDRA_JOBS` 1, 2 and
4, twice each, and compares the whole standard output of every run byte for byte with the first
run's. The output holds every verdict, every state count per cone, and every printed trace.
Data: [data/determinism.tsv](./data/determinism.tsv).

**Result.** All 24 runs print the same output as the first run of their model.

| Model | Wall time at 1 job (s) | at 2 jobs | at 4 jobs | Same output |
|---|--:|--:|--:|---|
| patent-ops | 4.0 | 3.5 | 3.5 | 6 of 6 |
| clinic-ops | 11.8 | 6.4 | 6.0 | 6 of 6 |
| food-permit-ops | 20.3 | 10.3 | 9.7 | 6 of 6 |
| oidc-auth (1 instance) | 4.6 | 4.0 | 3.8 | 6 of 6 |

(Means of the two runs.)

**Reading.** `RDRA_JOBS` runs cones and properties in parallel, and the BFS of one cone in parallel.
The output does not depend on it. The single-reader design costs speed-up: from 2 to 4 jobs the
wall time barely changes on these models. One large cone dominates each run, and the parallel BFS
of [native-bfs-parallel.md](../../design/native-bfs-parallel.md) keeps the one-thread order. This
table does not separate the two sources of parallelism. The machine has 4 cores, so 4 jobs is its
limit.

**Threats.** Two repeats per setting, on one machine. A nondeterminism that shows only under heavy
contention or on other hardware would not show here. The N4 proof and the open items of N4 and N17
(no loom model, no Miri run) are in the claims inventory.

## Table

| Model | RDRA_JOBS | Repeat | Wall (s) | User CPU (s) | Same stdout as first run |
|---|---:|---:|---:|---:|---|
| samples/patent-ops | 1 | 0 | 4.02 | 3.87 | yes |
| samples/patent-ops | 1 | 1 | 3.98 | 3.85 | yes |
| samples/patent-ops | 2 | 0 | 3.4 | 7.39 | yes |
| samples/patent-ops | 2 | 1 | 3.6 | 7.62 | yes |
| samples/patent-ops | 4 | 0 | 3.58 | 9.06 | yes |
| samples/patent-ops | 4 | 1 | 3.4 | 9.15 | yes |
| samples/clinic-ops | 1 | 0 | 11.98 | 11.44 | yes |
| samples/clinic-ops | 1 | 1 | 11.56 | 11.07 | yes |
| samples/clinic-ops | 2 | 0 | 6.43 | 14.33 | yes |
| samples/clinic-ops | 2 | 1 | 6.43 | 14.73 | yes |
| samples/clinic-ops | 4 | 0 | 6.62 | 15.06 | yes |
| samples/clinic-ops | 4 | 1 | 5.45 | 13.99 | yes |
| samples/food-permit-ops | 1 | 0 | 19.53 | 18.89 | yes |
| samples/food-permit-ops | 1 | 1 | 21.09 | 20.3 | yes |
| samples/food-permit-ops | 2 | 0 | 9.88 | 23.78 | yes |
| samples/food-permit-ops | 2 | 1 | 10.77 | 25.86 | yes |
| samples/food-permit-ops | 4 | 0 | 9.83 | 30.67 | yes |
| samples/food-permit-ops | 4 | 1 | 9.55 | 29.58 | yes |
| samples/oidc-auth --instance-count 1 | 1 | 0 | 4.9 | 4.68 | yes |
| samples/oidc-auth --instance-count 1 | 1 | 1 | 4.36 | 4.21 | yes |
| samples/oidc-auth --instance-count 1 | 2 | 0 | 4.01 | 6.12 | yes |
| samples/oidc-auth --instance-count 1 | 2 | 1 | 4.02 | 6.22 | yes |
| samples/oidc-auth --instance-count 1 | 4 | 0 | 3.76 | 8.0 | yes |
| samples/oidc-auth --instance-count 1 | 4 | 1 | 3.87 | 8.21 | yes |
