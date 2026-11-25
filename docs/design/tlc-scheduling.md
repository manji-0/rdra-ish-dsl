# TLC scheduling: runs at once, workers, and heap under `verify --with-tlc`

<!-- constrained-by ../cli-reference.md#memory-limit -->

Status (2026-11-26): a proposal for #384. Nothing here is on `main` yet. It builds on the
byte budget of #309 decision 3 (`verify/schedule.rs`, `Budget`) and leaves the state-model
runs, which #323 changes, as they are.

#384 measured `verify --with-tlc` on a 2-instance model with about 27 TLC cone runs on 64-
and 192-vCPU machines. Its finding: wall time follows how many TLC runs go at once and
whether the longest runs have enough workers. Most cores sat idle. This note says why the
code does that, what to change, and which constants are measured and which are guessed.
None of the many-core numbers can be reproduced on the 18-core machine this note was
written on.

## What the code does today

<!-- constrained-by ../cli-reference.md#with-tlc -->

- **When TLC starts.** `run_cones` starts in a thread before the entity pass, so every TLC
  cone run starts while BFS runs, with no BFS state count known. Whole-model runs, then the
  state models, follow once the cone runs end.
- **Memory.** `VerifySplit` gives the cone runs half of `--memory` (`cones`) for the whole
  run, also after BFS has ended. The whole-model runs get all of it (`wholes`).
- **Heap.** `TlcHeap::of`: 512 MiB per MiB of module text, in whole GiB, at least 1 GiB
  (2 GiB for a whole-model run), at most the share. A run that runs out of memory reruns once
  at twice the heap. `RDRA_TLC_HEAP` or a `-Xmx` in `JAVA_TOOL_OPTIONS` replaces the rule.
- **Runs at once.** A run starts when its `-Xmx` plus `JVM_OVERHEAD` fits beside the
  running ones (`Budget`), on up to `cores` threads.
- **Workers.** `tlc_workers(cores, sharing)`: the cores divided by the runs that can go at
  once (running and queued, no more than fit the share), at most 8, and 1 when the runs
  outnumber the cores. The count is fixed when the JVM starts.
- **Order.** The largest heap first, then `Job::weight`: a temporal property before a safety
  cone, then more variables.

The rows of #384, read through this code (64 cores, `--memory` 96 GiB, so the cone share is
48 GiB):

| `RDRA_TLC_HEAP` | Charge per run | Fit in 48 GiB | `-workers` | What #384 saw |
|---|---|---|---|---|
| `16g` | 16.25 GiB | 2 | `tlc_workers(64, 2)` = 8 | 2 runs, 8 workers, not finished in 75 min |
| unset | 1 GiB + 256 MiB (every module is 126 kB) | 38 | `tlc_workers(64, 27)` = 2 | 24 runs, 2 workers, the long ones not finished |

The question #384 leaves open, where the 8 comes from: it is the clamp in `tlc_workers`
(#309, "cap at 8"), not `rdra_ish_core::jobs()` (`min(cores, 8)`) and not the BFS thread
count of #371. At 361ccec the rule was `cone_workers`: `cores / at_once`, at least 2, no cap,
which gave 2 × 32.

## What the numbers support

Measured in #384:

1. Two runs at once took 60 minutes and eight at once took 30.
2. A heap of 8g and one of 16g gave the same time. 5g was 15% slower.
3. With two runs of 32 workers, 25 to 30 of 64 cores were busy.
4. With the heap unset, each run started with 2 workers and kept them to the end.
5. tmpfs in place of NVMe changed nothing.

Not settled by those rows, and read differently here:

- **Workers do matter for the longest runs.** 2 × 32 finished in 60 minutes, and 2 × 8 did
  not finish in 75. Rows 3 and 4 are two builds apart, and between them the flake's TLC
  moved from 1.7.4 to 1.8.0 and its JDK to 25, so the comparison is not clean. Still, 25 to
  30 busy cores across two runs means each run used 12 to 15. A cap of 8 slows a run that
  has the machine to itself.
- **The unset-heap rows are explained by workers as well as by heap.** The largest cones ran
  on 2 workers each. #309 measured 1.5 to 2.5 times the 8-worker time at 2 workers on
  mid-size cones. Each of the 24 JVMs also starts its own parallel GC threads (about 43 on 64
  cores). The scratch directory holds one state queue per run, so 24 runs at once fill more
  scratch than 2, whatever their heap. #384 infers that a 1 GiB heap spills TLC's
  fingerprints to disk. That may be true, but these rows do not show it.

So the first lever is that the longest runs start with enough workers. The heap comes
second.

## The rule

<!-- constrained-by #what-the-numbers-support -->

### A run's size

A job is **known** when BFS has finished a complete, plain run of the system the job's
module prints, and **unknown** until then. Only a complete plain run counts:

- not one stopped once every check failed (`Caps::until_all_violated`);
- not one stopped at the memory limit or `--max-states`;
- not one run up to symmetry, which counts orbits.

A safety cone's count is the count of its `Cone_` module. A property cone's count is that of
its plain (reduced) module. Its `_Full` and `_WF` follow-ups are larger, so the count is a
lower bound for them. The counts reach the scheduler at the end of each BFS pass: after the
entity pass (`Shared::decide_on`), after `bfs_cones`, and after `native_properties`. The
scheduler reads them when it picks a job. The `stable`, `unchanged`, and `precedes` jobs,
and the whole-model runs, are never counted.

### Workers

The **target** of a job:

- `--tlc-workers N` (or `RDRA_TLC_WORKERS`): `N`.
- Known, with `s` states: `clamp(ceil(s / 50_000), 1, W)`. Then a 48k-state cone gets 1
  worker, a 203k-state cone 5, and a cone of 400k states or more gets `W`.
- Unknown while BFS runs: `W`, as if it were large.
- Unknown once BFS has ended: the cores divided among the runs left, at most `W`, as before
  #384.

`W` is 8, the cap of #309. When memory, not the queue, bounds the runs at once, the cores
those runs leave idle go to them: a job whose target is `W` gets
`clamp(cores / fit, W, 4W)` instead, where `fit` is how many runs of its charge fit the
share. This gives back 361ccec's 2 × 32 under `RDRA_TLC_HEAP=16g` on 64 cores, and it is an
owner decision (below).

A run holds its workers of a **core budget** of `cores` while it runs, beside the bytes it
holds of the memory budget. `--tlc-jobs N` (or `RDRA_TLC_JOBS`) also bounds the number of
runs. A job starts with `min(target, free cores)` workers, and waits until
`free cores >= min(target, cores)` and its bytes fit, or until no run is going.

### Order

Each time a run can start, the job at the head of this order goes:

1. Known jobs whose target is `W`, the most states first.
2. Unknown jobs, by today's order (heap, then `Job::weight`).
3. The other known jobs, the most states first.

The head job waits for its cores. Nothing passes it, so no job starves. Because the largest
jobs go first, they meet the most free cores, and the runs left at the end are the ones that
started with the most workers. A JVM cannot take workers or heap once it has started.

Known before unknown conflicts with largest first: the largest cones are the ones whose BFS
finishes last. Ranking an unknown job as a `W`-worker job, between the large known ones and
the small ones, is a guess. Without a count, nothing better than `Job::weight` is available
before TLC starts.

### What the rule gives

Each row is a test of the pure function (`schedule::pick` with a simulated completion
sequence). Cores, memory and counts are the inputs, so no test needs the machine.

| Case | Today | Under the rule |
|---|---|---|
| 64 cores, 48 GiB share, heap 16g | 2 runs × 8 workers | 2 × 32. After BFS ends (below), 5 fit in 96 GiB, and a run that starts then gets 12 |
| 64 cores, 48 GiB, heap unset, 27 unknown jobs | 24 × 2 | 8 × 8 at first. A job BFS has counted takes its target as cores free, and small jobs run on 1 worker |
| 4 cores, 15 jobs of a few hundred states each (a sample in CI) | 4 × 1 | 1 × 4 until BFS has counted the jobs, a second at most, then 4 × 1 |
| 18 cores, heap set so 1 fits | 1 × 8 | 1 × 18 (`cores / fit`, at most `4W`) |

### Memory after BFS

When BFS, the native property runs and the proof have ended, the cone runs' budget grows
from half of `--memory` to all of it (`Pool::bfs_ended`), so more runs fit at once. A run's
heap, and its rerun at twice the heap, are still sized by the half, whenever the run starts.
Otherwise whether a module is `not run` over the share, or gets its rerun, would depend on
when BFS ended, and a verdict would depend on the schedule. On #384's model BFS takes about 13 minutes and TLC 30 to 60, so for most of the run
TLC would have 96 GiB rather than 48. BFS does not give everything back: it keeps the state
models and findings, and the process overshoots the limit (#237). How much to hand over is
an owner decision.

### Heap from states

When a job is known, its heap is
`max(text rule, ceil_GiB(1 GiB + s × 512 B))`, clamped to the share. It only ever raises
the text rule's heap. It never makes a run `not run` that runs today: the clamp is to the
share, never `OverShare`. 12.8M states give 8 GiB, which is #384's floor where 8g and 16g
were equal. 512 B per state and the 1 GiB floor are fitted to that one point, and #309 shows
that a safety cone needs far less: 1.34M states ran in 64 MiB at full speed. So the rule is
opt-in, as `RDRA_TLC_HEAP=states`, until a many-core run with per-run cost (below) measures
it.

## Knobs

| Knob | Effect |
|---|---|
| `--tlc-jobs <N>`, `RDRA_TLC_JOBS` | At most `N` TLC runs at once. The memory budget still applies |
| `--tlc-workers <N>`, `RDRA_TLC_WORKERS` | Every run gets `N` workers. A safety cone that does not pass still reruns on 1 worker |
| `RDRA_TLC_HEAP=<size>` | As today: every run's heap, no rerun at twice the heap |
| `RDRA_TLC_HEAP=states` | Heap from states, as above |

A flag beats its variable. The knobs change when and how TLC runs, never a verdict.

## Per-run cost on `tlc:` lines

<!-- constrained-by ../compatibility.md#the-command-line -->

Each `tlc:` run line of a cone, property or whole-model job ends with what its TLC runs
cost:

```
tlc: Cone_Big [Big]: ok (12787852 states, 1412.0 s, -Xmx16g, 8 workers)
```

- The seconds are the wall time of every TLC launch of the job added up: the rerun on one
  worker, the `_rest<n>` reruns, `_Full`, `_WF`, and the rerun at twice the heap.
- `-Xmx` is the heap of the last launch.
- The workers are those of the first launch.
- A job with no launch adds nothing: a covered cone (`checked in ...`), or one over the share.
- `StateModel_` lines do not change. #323 asserts that their output is the same in all three
  batch modes.

[compatibility.md](../compatibility.md#the-command-line) covers `bounds:`, `result:` and
`tlc_result:`, and lists run time as not covered. So the change breaks no promise. It does
break `verify_cli`'s check that a `tlc:` line equals its native line, which is changed to
compare up to the cost.

## A caller that runs TLC itself

Experimental, as `--prove` is. Neither the flags nor the JSON are covered by the
compatibility contract until the owner says so.

- **`verify --with-tlc --plan <FILE> -o <DIR>`** runs the native checker and prints what it
  prints. It writes the specs to `<DIR>` and the TLC launches it would make to `<FILE>`, and
  starts no TLC. Each entry of `launches` gives: `module`, `cfg`, `kind` (`cone`, `property`,
  `explored`, `whole`), the checks it answers, `covered_by` when it is not run, the BFS
  verdict and state count, and the `heap`, `workers` and `args` the schedule would give it.
  The file starts with `"version": 1`.
- **`verify --with-tlc --tlc-results <DIR>`** runs the native checker, then the same
  `run_job` ladder as `--with-tlc`, but reads each launch from
  `<DIR>/<cfg stem>.out` (stdout and stderr) and `<DIR>/<cfg stem>.status` (the exit code,
  which `TlcRun::outcome` needs) instead of starting TLC. It compares and prints the verdict
  as `--with-tlc` does.
- **Follow-ups.** The ladder decides which follow-up runs are needed only from what a run
  printed: the rerun on one worker, a `_rest<n>.cfg` written from the checks a run reported,
  `_Full`, `_WF`, the whole-model runs, and the state models. So the exchange goes in rounds.
  When a launch has no log, `--tlc-results` writes the cfgs it needs, lists them in
  `<DIR>/plan.json` in the `--plan` form, prints `tlc: <module>: waiting for <cfg>`, and
  exits 3. The caller runs what is listed and calls it again. Exit 0 or 1 means the verdict
  is complete.

`run_cfg` moves behind a closure (start TLC, or read a log), so both modes share the ladder
and the comparison, and only the launching moves out.

## How this fits #309, #310 and #323

- **#309 decision 3** (heap per module, byte budget) stays. The cone and whole-model runs go
  through a `Pool` that holds bytes the way `Budget` does and cores beside them.
  `Budget::new`, `hold`, `Hold::regrow` and `run_budgeted` keep their signatures. The
  state-model runs keep `run_budgeted` and `tlc_workers`, which #323 changes. Whole-model
  runs still go 2 at once unless `--tlc-jobs` says otherwise.
- **#323** runs small state models in batch JVMs, which it does not overlap with process runs
  because `tlc_workers` does not count the batch JVMs. With a core budget, a batch JVM holds
  one core like a one-worker run, and the two can overlap. That is a change for after #323,
  not part of this work. Nothing here conflicts with #323's code: this work does not touch
  `state_models.rs`, `tlc_batch.rs` or `tlc_tool.rs`.
- **#309 decision 5** (start a cone with the small-run JVM settings and restart it): a known
  job under `SMALL_RUN_STATES` (10,000) could start with C1 as a state model does, with no
  restart. That is left for after this work, since it is a change to how the JVM runs, not to
  scheduling.
- **#310** (the batch driver) gains a source of sizes: a known cone of a few thousand states
  could go to a batch JVM. That is also later.

## Constants

| Constant | Value | Source |
|---|---|---|
| `W`, the worker cap | 8 | #309: no gain past 4 to 8 workers on cones up to 1.34M states (18 cores) |
| `4W` | 32 | #384: 2 × 32 on 64 cores kept 25 to 30 cores busy. A guess for the top |
| States per worker | 50,000 | #309's local curve (48k gains nothing past 2 workers, 203k gains up to 4). A guess past 1.34M |
| Heap per state | 512 B over 1 GiB | Fitted to #384's 8g floor at about 12.8M states. A guess |
| Memory after BFS | all of `--memory` | A guess. See #237 |

## For the owner to decide

1. **The worker cap when memory bounds the runs.** Give the idle cores to the runs that fit
   (`cores / fit`, at most 32), or keep 8. Recommended: give them, since 2 × 8 was the
   slowest row of #384.
2. **Heap from states:** opt-in (`RDRA_TLC_HEAP=states`) in this release, or the default.
   Recommended: opt-in, then the default once a many-core run with per-run cost shows the
   floor.
3. **Memory after BFS:** grow the cone share to all of `--memory`, or to a fraction.
   Recommended: all of it, as the whole-model runs already have.
4. **Per-run cost on `tlc:` lines.** Recommended: add it and leave the lines uncovered, as
   [compatibility.md](../compatibility.md#what-is-not-covered) leaves run time.
5. **`--plan` and `--tlc-results`.** Recommended: experimental, the JSON at `"version": 1`
   with no promise, and exit 3 for "waiting for logs".
6. **Unknown jobs ranked as large.** On a small machine in CI this runs one unknown job alone
   for up to about a second before BFS has counted the rest. Recommended: accept it, and
   check the engine corpus's wall time on the PR.

## What to measure on a many-core machine

With the per-run cost in place, on the model of #384 (`--instance-count 2 --instances
IssuanceTarget=1 --deny-undecided --with-tlc`, `--memory` at 75% of RAM):

1. The default: heap unset, no knobs.
2. `RDRA_TLC_HEAP=8g`.
3. `RDRA_TLC_HEAP=states`.
4. `--tlc-jobs 8 --tlc-workers 8`.

Record the `tlc:` lines and the wall time of each. They settle the constants above, the heap
floor, and whether 32 workers beat 8 on the longest cone.
