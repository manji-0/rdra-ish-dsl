# Scaling in the instance count, plain and symmetric (roadmap T7)

**Question.** How do states, CPU and memory grow with the number of instances per entity, and what
does instance symmetry (`--symmetry always`) save as the count grows?

**Method.** `scripts/paper/eval.py scaling` runs `verify` at `--instance-count` 1 to 4, with
`--symmetry never` and `always`, on ec-site, personal-info, patent-ops and clinic-ops, with the
reduction on (the default). Each run has a 300 s timeout and `--memory 12GiB`, and a series stops
at its first timeout. A second measurement runs patent-ops at 3 instances under `--memory 4GiB` and
a 900 s timeout, both ways. The tool reruns a cone up to symmetry when its plain run stops at the
memory limit, so a lower limit is the setting where symmetry is meant to help. Data:
[data/scaling.tsv](./data/scaling.tsv), [data/scaling-memory.tsv](./data/scaling-memory.tsv).

## Results

**Growth.** States grow by one to three orders of magnitude per added instance:

| Model | n=1 | n=2 | n=3 | n=4 |
|---|--:|--:|--:|--:|
| ec-site | 44 | 711 | 26,094 | 1,459,159 (3.2 s) |
| personal-info | 27 | 118 | 659 | 4,878 |
| patent-ops | 1,204 | 2,017,009 (3.6 s) | timeout at 300 s, 6.0 GB | — |
| clinic-ops | 1,063 | 5,617,011 (11.7 s) | killed at 192 s, 15.6 GB | timeout |

(States summed over every BFS run of `verify`, symmetry off.)

- clinic-ops at 3 instances was killed by the kernel's out-of-memory killer after 192 s, with a peak
  RSS of 15.6 GB on a 15 GiB machine, although `--memory 12GiB` was set. `dmesg` records the two
  kills (symmetry off and on). This is the known gap of claim A26: the memory limit bounds the BFS
  stores, not the process.
- patent-ops at 3 instances does not finish in 300 s (12 GiB), nor in 900 s under 4 GiB: about 86
  million states in total, symmetry off and on alike.

**Symmetry.** Symmetry never shrinks the largest run of ec-site, patent-ops or clinic-ops at any
count:

| Model | n | Largest run, plain | Largest run, `always` | All runs, plain | All runs, `always` |
|---|--:|--:|--:|--:|--:|
| ec-site | 4 | 1,411,966 | 1,411,966 | 1,459,159 | 1,435,913 |
| personal-info | 3 | 273 | 273 | 659 | 453 |
| personal-info | 4 | 2,713 | 1,589 | 4,878 | 2,200 |
| patent-ops | 2 | 1,341,707 | 1,341,707 | 2,017,009 | 1,942,129 |
| patent-ops | 3, 4 GiB | — | — | 85,913,758 (timeout 900 s) | 85,913,758 (timeout 900 s) |
| clinic-ops | 2 | 2,238,170 | 2,238,170 | 5,617,011 | 5,495,861 |

- The savings are in the small cones: 1.6 to 55% of all states, up to 2.2× on personal-info at 4
  instances. The largest run of each domain model is one that the group leaves alone. Either its
  classes are fixed by the scan (on a reduced cone, the group acts only on the `T` rows, and the
  sub-lifecycle's rows are fixed), or it is the extended sub-lifecycle `X`, which runs plain unless
  its run stops at the memory limit.
- Under 4 GiB and 900 s, patent-ops at 3 instances gives the same totals both ways, so neither run
  reached a cone where the orbit rerun applies before the timeout.
- The only symmetry factors on record with both sides complete are 3.0×, 5.2× and 5.5× on three
  reduced cones of patent-ops at 3 instances, up to the group of their `T` rows (S7 in
  [../evidence/measurements.md](../evidence/measurements.md); 29,680,875 states against 5,413,500
  orbits on the largest). This evaluation does not reproduce them: no end-to-end run here reached
  those cones within its budget. A per-cone measurement at the pinned commit is needed before the
  paper quotes a symmetry factor.

**Reading.** On these models, the reduction and the cone carry the gains, and symmetry saves little
end to end. This is one reason Paper 1 leads with the sub-lifecycle quotient
([contribution.md](../contribution.md)). Symmetry's case is soundness, not speed: Theorem 5 lets
liveness be checked on the quotient at all, while TLC's symmetry sets would not be sound here.

## Threats

- One run per cell; four models; timeouts of 300 s and 900 s. A longer budget would show where the
  unfinished runs end.
- The machine's 15 GiB bounds what can finish. Runs that need more memory were killed, not stopped.

## Tables

### Instance counts 1 to 4 (`--memory 12GiB`, 300 s)

| Model | Run | Result | States (all runs) | Largest run | Peak RSS (MB) | User CPU (s) |
|---|---|---|---:|---:|---:|---:|
| samples/ec-site | n=1 sym=never | ok | 44 | 18 | 15 | 0.01 |
| samples/ec-site | n=2 sym=never | ok | 711 | 532 | 15 | 0.01 |
| samples/ec-site | n=3 sym=never | ok | 26,094 | 23,670 | 17 | 0.04 |
| samples/ec-site | n=4 sym=never | ok | 1,459,159 | 1,411,966 | 184 | 3.22 |
| samples/ec-site | n=1 sym=always | ok | 44 | 18 | 15 | 0.0 |
| samples/ec-site | n=2 sym=always | ok | 665 | 532 | 15 | 0.0 |
| samples/ec-site | n=3 sym=always | ok | 25,014 | 23,670 | 17 | 0.04 |
| samples/ec-site | n=4 sym=always | ok | 1,435,913 | 1,411,966 | 184 | 3.21 |
| samples/personal-info | n=1 sym=never | ok | 27 | 7 | 15 | 0.01 |
| samples/personal-info | n=2 sym=never | ok | 118 | 45 | 15 | 0.01 |
| samples/personal-info | n=3 sym=never | ok | 659 | 273 | 15 | 0.0 |
| samples/personal-info | n=4 sym=never | ok | 4,878 | 2,713 | 16 | 0.01 |
| samples/personal-info | n=1 sym=always | ok | 27 | 7 | 15 | 0.01 |
| samples/personal-info | n=2 sym=always | ok | 103 | 45 | 15 | 0.01 |
| samples/personal-info | n=3 sym=always | ok | 453 | 273 | 15 | 0.0 |
| samples/personal-info | n=4 sym=always | ok | 2,200 | 1,589 | 16 | 0.01 |
| samples/patent-ops | n=1 sym=never | ok | 1,204 | 547 | 20 | 0.05 |
| samples/patent-ops | n=2 sym=never | ok | 2,017,009 | 1,341,707 | 163 | 3.55 |
| samples/patent-ops | n=3 sym=never | timeout | 5,282,872 | 5,167,635 | 5974 | 295.83 |
| samples/patent-ops | n=1 sym=always | ok | 1,204 | 547 | 20 | 0.04 |
| samples/patent-ops | n=2 sym=always | ok | 1,942,129 | 1,341,707 | 163 | 3.51 |
| samples/patent-ops | n=3 sym=always | timeout | 5,282,872 | 5,167,635 | 5608 | 295.25 |
| samples/clinic-ops | n=1 sym=never | ok | 1,063 | 112 | 22 | 0.06 |
| samples/clinic-ops | n=2 sym=never | ok | 5,617,011 | 2,238,170 | 549 | 11.74 |
| samples/clinic-ops | n=3 sym=never | ? | 61,849,224 | 61,849,224 | 15573 | 192.45 |
| samples/clinic-ops | n=4 sym=never | timeout | 0 | 0 | 13975 | 290.93 |
| samples/clinic-ops | n=1 sym=always | ok | 1,063 | 112 | 21 | 0.07 |
| samples/clinic-ops | n=2 sym=always | ok | 5,495,861 | 2,238,170 | 547 | 10.75 |
| samples/clinic-ops | n=3 sym=always | ? | 61,849,224 | 61,849,224 | 15574 | 198.34 |
| samples/clinic-ops | n=4 sym=always | timeout | 0 | 0 | 14170 | 291.27 |

### patent-ops at 3 instances under `--memory 4GiB`, 900 s

| Model | Run | Result | States (all runs) | Largest run | Peak RSS (MB) | User CPU (s) |
|---|---|---|---:|---:|---:|---:|
| samples/patent-ops | n=3 sym=never memory=4GiB | timeout | 85,913,758 | 68,021,867 | 4768 | 883.96 |
| samples/patent-ops | n=3 sym=always memory=4GiB | timeout | 85,913,758 | 68,021,867 | 4768 | 884.65 |
