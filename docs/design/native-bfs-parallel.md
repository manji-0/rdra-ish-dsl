# A native BFS that uses the cores

<!-- constrained-by ./instance-symmetry.md#the-canonical-form -->
<!-- constrained-by ./instance-symmetry.md#memory -->
<!-- constrained-by ../cli-reference.md#memory-limit -->

Status (2026-11-26): 3.1 is being built in parts.

- **Landed.** Part 1 of 3.1: workers pack, hash and deduplicate (#372,
  [3.1 as built](#31-as-built)). One BFS run takes `max(4, cores / 3)` threads, at most 12
  and never more than the cores (#371). Part 2 of 3.1: the shared table and the workers'
  probe, which a release build leaves off
  ([Part 2: the shared table](#part-2-the-shared-table)).
- **Open on #296.** Part 3 of 3.1, the serial findings tail of the largest cone
  ([Part 3: the findings tail](#part-3-the-findings-tail)), and whether the probes go on. 3.1's exit is not met: food-permit-ops at 8 jobs took 35% of
  its one-job time, against 30%. 3.2's shared helper pool and its sweep (`Helpers::take`
  still gives one run every free helper). 3.3.
- **Superseded.** The helper default `available_parallelism() - 1` of
  [Helpers by need and the jobs cap](#helpers-by-need-and-the-jobs-cap), by the owner's
  `max(4, cores / 3)` of 2026-11-26
  ([comment](https://github.com/manji-0/rdra-ish-dsl/issues/296#issuecomment-7166110226)). Both
  [open questions](#open-questions-for-the-owner) were answered on 2026-11-25
  ([comment](https://github.com/manji-0/rdra-ish-dsl/issues/296#issuecomment-7164434628)).

This is phase 3 of
[#296](https://github.com/manji-0/rdra-ish-dsl/issues/296), which the owner approved
([decision](https://github.com/manji-0/rdra-ish-dsl/issues/296#issuecomment-7134170599) item 1:
"phases 2 and 3 go ahead after phase 1"). It refines steps 2, 3 and 5 of
[#254](https://github.com/manji-0/rdra-ish-dsl/issues/254) (keep the BFS reader thread thin)
and the helper budget of [#163](https://github.com/manji-0/rdra-ish-dsl/issues/163). Every
number is marked **measured**, **computed** (arithmetic on measured numbers) or
**estimated**.

## Summary

<!-- derived-from #where-the-time-goes-on-main -->
<!-- derived-from #workers-probe-the-reader-commits -->
<!-- derived-from #why-the-run-is-the-one-thread-run -->

1. **Today the reader limits one cone to 1.9x.** On main (c51f2543), `Bfs::take` is 36% of a
   one-thread food-permit-ops run. At 8 jobs the reader thread spends 89% of its samples in
   `take`, and each of the 7 workers waits 81% to 83% of the time. food-permit-ops gains
   1.94x at 6 to 18 jobs (measured, median of three).
2. **#296's premise holds for hits and fails for timing.** Hits are 80% to 88% of edges
   (measured). But a worker sees a hit only if the target was admitted before the worker
   probed. A worker probes up to `helpers × AHEAD × CHUNK` states ahead of the reader. At 8
   jobs, 30% (food-permit-ops) to 57% (insurance-claim-ops) of all edges hit a state that
   was admitted inside that window. The diamonds of BFS are local: 13% to 43% of edges hit a
   state that a move of the same chunk admitted (measured).
3. **3.1 as designed here.** Workers pack, hash and probe each successor against a table
   that one thread writes and many threads read. They resolve repeats within their own
   chunk and evaluate the after-asserts and invariants. The reader probes only a worker's
   miss, assigns numbers, and pushes edges in order. Estimated reader share after 3.1: 15%,
   14% and 10% of the one-thread run (from 36%, 32% and 24%). Estimated gain for
   food-permit-ops: from 1.94x to 3.5x to 3.9x at 6 to 18 jobs.
4. **Exits (owner, 2026-11-25).** 3.1's exit is food-permit-ops at 8 jobs in at most 30%
   of its one-thread wall time, with the same output at every job count. #296's 4x (a
   quarter of the one-thread time) is the exit of 3.3. The model gives 3.9x for 3.1. The
   same model gives 2.16x for today's code against 1.94x measured, and corrected by that
   ratio it gives 3.5x: both meet 30% (3.3x). 3.3
   ([Beyond 3.1](#beyond-31-in-flight-dedup)) gives 4.2x to 4.7x. Outside BFS, 10% of the
   food-permit-ops run is serial (the findings and adjacency of the one large cone, start
   and exit), which bounds any design at about 10x. See [Decisions](#decisions).
5. **The output stays the one-thread output.** A worker's hit is final, because the table
   and the store only grow. A worker's miss is a hint, and the reader probes again. Every
   stop point is decided on the reader, in move order, from reader-owned lengths. The new
   table has the growth rule of today's `Seen`, so `Seen::bytes_with`, `store_bytes` and
   the `--memory` stop points do not change in 3.1.
6. **Prerequisite: 2.2 is required, 2.3 is assumed.** Workers read stored states while the
   reader appends. That needs rows that never move (2.2's segments). 2.3's packed rows make
   the comparisons cheap, but they are not required.

## Where the time goes on main

Measured on c51f2543 (main with phase 1 of #296 and #212 phase 1), a release build,
`verify samples/<name>` at the defaults of each `// expect` header. The machine was an
M5 Max with 18 cores and 128 GB, at a 1-minute load of 1.8 to 4.5. The method and the raw
files are under [Measurement method](#measurement-method).

### Totals and wall time

| | food-permit-ops | clinic-ops | insurance-claim-ops |
|---|---:|---:|---:|
| Instructions, 1 job (#296 at 5eb2fe0e) | 89.8 G (111.6 G) | 95.8 G (113.1 G) | 313.3 G (458.7 G) |
| Cycles, 1 job | 17.7 G | 17.8 G | 54.3 G |
| BFS runs | 27 | 50 | 21 (25) |
| States / edges | 5.02 M / 40.9 M | 5.63 M / 36.9 M | 9.87 M / 50.3 M (10.57 M / 53.8 M) |

Wall time in seconds, median of three runs, speedup over 1 job (measured; #296's single run
in brackets):

| Jobs | food-permit-ops | clinic-ops | insurance-claim-ops |
|---:|---:|---:|---:|
| 1 | 4.15 (5.58) | 4.17 (5.18) | 12.83 (18.70) |
| 2 | 2.65, 1.57x (3.78) | 1.97, 2.12x (2.34) | 7.15, 1.79x (10.19) |
| 4 | 2.17, 1.91x (3.29) | 1.64, 2.54x (2.13) | 4.44, 2.89x (5.78) |
| 6 | 2.13, 1.95x | 1.63, 2.56x | 3.99, 3.22x |
| 8 | 2.14, 1.94x (3.32) | 1.92, 2.17x (2.08) | 4.09, 3.14x (5.67) |
| 18 | 2.15, 1.93x (3.29) | 1.90, 2.19x (2.25) | 4.18, 3.07x (5.84) |

- Instructions grow by 1% to 2% from 1 to 18 jobs, and cycles by 12% to 18%: the helpers
  wait, they do not contend. The stdout and stderr of every run are byte-identical to the
  1-job run.
- clinic-ops is bimodal at 6 to 18 jobs (1.55 s to 2.00 s). `Helpers::take` gives every
  free helper to the first run that asks, and the run keeps them until it ends. Which of
  the two largest runs (2.24 M and 2.15 M states) asks first depends on timing. In one
  `perf` run at 8 jobs, the second one's thread CPU equalled its one-thread CPU: it
  expanded every chunk itself. That build's thread CPU is inflated above 1 job (see
  [Measurement method](#measurement-method)), so the wall times, not that line, are the
  evidence. This is #163's budget problem, and 3.2 addresses it.

### The reader today

From `sample` at 1 ms on a build with `#[inline(never)]` on the hot functions, 1 job. The
shares are of the whole run. The food-permit-ops profile is taken without the 180 samples
`sample` recorded while it attached at launch, 4.8% of the file, which hold only
`_dyld_start`. The other two profiles have no such samples.

| Part of `Bfs::take` | food-permit-ops | clinic-ops | insurance-claim-ops |
|---|---:|---:|---:|
| `Bfs::take`, all of it (#296) | **36.0%** (54.7%) | **32.0%** (47.0%) | **23.8%** (24.6%) |
| · `Seen::find`, hashing and compare included | 9.7% | 9.9% | 7.0% |
| · `Bindings::id` | 8.7% | 6.4% | 2.1% |
| · `take`'s own code: edge push, loop, layers | 3.9% | 3.9% | 2.2% |
| · rehash when `seen` grows | 3.7% | 3.2% | 2.4% |
| · copies (`s2`, `to_owned`) | 3.2% | 2.0% | 2.0% |
| · self-loop compare (`*s2 == s`) | 2.6% | 2.1% | 1.0% |
| · `Moves::get` | 1.4% | 1.4% | 0.7% |
| · `admit`: insert, push, charge | 2.3% | 2.5% | 1.7% |
| · `check_invariants`, `check_after` | 0.5% | 0.7% | 4.6% |
| `expand`, all threads | 51.0% | 45.1% | 64.2% |
| Outside BFS: findings, adjacency, start and exit | 10.2% | not split (several cones) | not split (several cones) |

The 10.2% outside BFS agrees with the `perf` build's own lines: the BFS runs' walls sum to
3.69 s of the 4.15 s run, which leaves 11.1% (measured).

At 8 jobs on food-permit-ops, the reader thread spends 88.8% of its samples in `take`. The 7
workers spend 17% to 18% in `expand` and 81% to 83% waiting (measured). With one cone at
98% of the BFS work, the bound is `1 / (take + outside BFS) = 1 / (0.360 + 0.102) = 2.16x`
(computed). The measured speedup is 1.94x.

### Hits, loops and stale misses

From the `perf-run` lines and a counting patch over each run's edges and parents (not for
merge). All rows are summed over the runs of each sample. Each column is a fraction of all
edges.

| | food-permit-ops | clinic-ops | insurance-claim-ops |
|---|---:|---:|---:|
| Hit rate `(edges - states) / edges` | 0.877 | 0.847 | 0.804 |
| Self-loops (no probe needed) | 0.320 | 0.065 | 0.075 |
| New states (fresh misses) | 0.123 | 0.153 | 0.196 |
| Probe hits | 0.557 | 0.782 | 0.728 |
| Stale: hit a state admitted in the same chunk | 0.136 | 0.432 | 0.331 |
| Stale at 2 jobs (window 512 states) | 0.235 | 0.514 | 0.481 |
| Stale at 4 jobs (window 1,536) | 0.274 | 0.539 | 0.534 |
| Stale at 8 jobs (window 3,584) | 0.299 | 0.564 | 0.572 |
| Stale at 18 jobs (window 8,704) | 0.308 | 0.591 | 0.605 |

A probe hit is *stale* when its target was first reached by a move from a state at or after
`chunk start - window`. Here `window = helpers × AHEAD × CHUNK = helpers × 512`. A worker
that expands the chunk as soon as it is handed out has not seen that state yet. This is the
worst case, and it is the usual case while the reader is the bottleneck, because the
workers then run as far ahead as `has_room` allows. The same-chunk row is the floor at any
job count. Checks: in every run, the fresh count equals the states minus the initial
states, and the stale counts rise with the window.

So the 80% to 88% hit rate of #296 is mostly not visible to a worker. A diamond
`s → a → ab`, `s → b → ab` has its two parents `a` and `b` next to each other in discovery
order. The second move into `ab` almost always comes within a chunk or two of the first.

### What changed since #296

- **Instructions fell by 15% to 32%**: 1.1 (`Fold::holds`), 1.2 (one run per equal cone:
  insurance-claim-ops has 21 BFS runs instead of 25), 1.4 (below), and the smaller changes
  merged since 5eb2fe0e (the allocation-free canonicalizer among them).
- **`observe` is gone from `verify`.** Item 1.4 gathers view patterns only where a caller
  reads them, and `verify` reads none: its caps are built `.without_views()`
  (`verify/mod.rs`), and the profiles show no `observe`. `observe` was 28.9%, 18.5% and
  8.5% of the three runs in #296's profile. That is why the reader share fell from 54.7%
  to 36.0% on food-permit-ops. In absolute terms, the rest of the reader stayed about the
  same: 32 G instructions now against 29 G then (computed from the shares). The one-cone
  bound rose from 1.8x to 2.2x, and food-permit-ops at 8 jobs went from 1.7x to 1.94x.
- **insurance-claim-ops' reader share barely moved** (24.6% to 23.8%), because 1.1 took
  instructions out of `expand`, not out of the reader.
- **Wall times are 20% to 31% lower at 1 job.** That is fewer instructions plus a quieter
  machine: load 2 to 4.5 now, against 6 to 9 for #296.

## Assumptions about phase 2

Phase 2 is being built in parallel: 2.1 compact edges, 2.2 fixed segments, and 2.3
bit-packed states at the store boundary in `Bfs::admit` and `Seen`. This design assumes the
following interface. A different one changes the parts named under
[If phase 2 lands differently](#if-phase-2-lands-differently).

- **A1 (2.2): rows never move.** `States` keeps its rows in segments of `2^k` rows that are
  never reallocated. The directory of segments is fixed-size or append-only, so a thread
  that holds a published index can read that row while the reader appends others.
- **A2 (2.3): packing is a pure function.** `pack(&system, &StateSlice, &mut [u8])` reads
  only the system. Any thread can call it, and under symmetry it is applied to the
  canonical form (#296's phase 2 dependency note).
- **A3 (2.3): `Seen` holds packed rows.** `Seen` hashes and compares packed rows, and its
  hash is over the packed bytes.
- **A4: the charge formula.** `Seen::bytes_with` stays "one control byte and one 4-byte
  index per bucket, at 7/8 load, doubling", as today. 3.1 keeps whatever formula phase 2
  leaves. It changes no formula.

## Workers probe, the reader commits

<!-- constrained-by #assumptions-about-phase-2 -->
<!-- derived-from #the-reader-today -->

This section is item 3.1.

### What moves to the workers

<!-- constrained-by #the-reader-today -->

Every value below depends on the expanded state and the move alone. A worker computes it
for each move in its chunk and stores it in `Moves` beside the successor.

| Per move | Today | After 3.1, worker | After 3.1, reader |
|---|---|---|---|
| Successor, canonical form (symmetry) | worker | worker | — |
| Packed form (2.3) | reader, in `admit` | worker | — |
| Hash | reader, 2 to 3 times per new state | worker, once | uses the given hash |
| Self-loop and orbit-self flags | reader (`*s2 == s`, `*key == *s`) | worker | reads the flag |
| Probe of the table | reader (`seen.find`, and again in `admit`) | worker: `Hit(i)`, `SameAs(k)` or `Miss` | re-probes a `Miss` only |
| Binding | reader, `Bindings::id` (hash of the params) | worker: dense key `act`, params in radix of the slot counts | `Vec<u32>` lookup by dense key; first sight assigns the next id |
| After-asserts | reader, `check_after` | worker: bit per evaluated assert | reports the first violation per name, in order |
| Invariants of the successor | reader, `check_invariants` in `admit` | worker, on a `Miss` only | used only when the state is admitted |
| View pattern keys (`check`, `states`) | reader, `observe` | worker: per written slot, unchanged flag and post key | interns keys, sets view bits |

`SameAs(k)` is the worker's own dedup. A worker keeps a small map of the packed successors
of its chunk that missed the table. A later move of the same chunk with the same successor
records the position `k` of the first one instead of `Miss`. This turns the same-chunk row
of the stale table (13% to 43% of edges) into an array lookup on the reader.

### The shared table

<!-- constrained-by #assumptions-about-phase-2 -->

`Seen` becomes a table that one thread writes (the reader) and many threads read (the
workers). hashbrown cannot be read during an insert, so the table is new code.

- **Layout.** It has hashbrown's shape: one control byte per bucket holding 7 bits of the
  hash, grouped eight to an `AtomicU64`, and an `AtomicU32` row index per bucket. Probing is
  triangular over groups. That is 5 bytes per bucket, the bytes `Seen::bytes_with` counts
  today.
- **Growth.** It grows at the points hashbrown grows: buckets of `(cap * 8 / 7)` rounded up
  to a power of two, doubling when full. So `Seen::bytes_with` stays the same function of
  the length, and `store_bytes` and its Kani harnesses do not change. A new Kani harness
  pins the growth rule to `bytes_with`, as `vec_bytes_is_what_a_vec_of_edges_grows_to` does
  for edges.
- **Insert (reader only).** The reader writes the packed row into its segment slot, stores
  the index (`Relaxed`), then stores the group word with the new control byte (`Release`).
  A bucket is written once and never changed. Nothing is ever removed.
- **Probe (any thread).** Load the group word (`Acquire`), match the 7-bit tag, load the
  index, compare the packed row in its segment. The `Release`/`Acquire` pair orders the
  row's bytes before the index that names them. The row is never written again, so the
  comparison reads settled memory.
- **Generations.** To grow, the reader builds the next table privately, rehashing every
  row, and then makes it current. A chunk is handed out with an `Arc` of the table that is
  current at that moment, so the existing chunk queue carries it and needs no new
  synchronization. A worker probes that generation. An old generation is frozen once it
  stops being current. It is freed when the last chunk that holds it is dropped, after at
  most `helpers × AHEAD` chunks.

### The reader after 3.1

<!-- constrained-by #what-moves-to-the-workers -->

```text
for each move i of the chunk, in order:
    binding = bindings.by_dense_key(rec.binding_key)        // assigns the next id on first sight
    if views: intern rec's pattern keys in order            // the pattern cap stops here
    if rec.self_loop: push loop edge; resolved[i] = from; continue
    report rec's violated after-asserts not yet reported    // first per name
    if rec.orbit_self: push loop edge, orbit_self; resolved[i] = from; stop check; continue
    to = match rec.probe:
        Hit(j)    => j
        SameAs(k) => resolved[k]
        Miss      => seen.find_hashed(rec.packed, rec.hash) or admit(rec)   // caps, charge, push, insert
    resolved[i] = to
    push edge (from, to, binding); stop check
```

`admit` loses its leading `seen.find`, because the caller has just probed. It still checks
the state cap, the deadline and `bytes_with_one_more` against the charge. It reads the
worker's invariant result instead of evaluating the invariants.

### Why the run is the one-thread run

<!-- constrained-by #the-shared-table -->
<!-- constrained-by #the-reader-after-31 -->

Call R1 the run with no workers (`Threads::Fixed(0)`). Its reader reads the moves
`m_1, m_2, ...` in order: by state, then by move within the state. At each move it updates
its state Σ, which holds the states, `seen`, parents, edges, bindings, layers, views, the
reported violations and the charge. The claim is that with workers, the reader's Σ after
move `m_k` equals R1's, for every `k`. Then states, numbers, parents, edges, traces
(`trace_of` reads only parents and edges), diagnostics and their order are equal, and so is
every stop point. The proof is by induction on `k`.

1. **Move-local values are equal.** The successor, canonical form, packed form, hash,
   flags, dense binding key, assert and invariant bits and pattern keys are pure functions
   of the system, the expanded state and the move. Workers compute them with the same
   functions the reader calls when it expands a chunk itself. `Canon` and `StepBuffers` are
   scratch and carry nothing between calls; the `shadow` twins of `symmetry` and `moves`
   hold this. This is today's argument for `expand` (#163), extended to more fields.
2. **The table only grows, and indices never change.** `Bfs` never removes a state.
   `States::push` appends at `len()`. `Seen::insert` is `insert_unique` today and a
   write-once bucket in the new table. The run ends rather than evicting anything. So
   after any reader step, the table maps exactly the admitted states, each to the index it
   was given.
3. **A worker's hit is final.** A worker probes chunk `c` before the reader reads chunk
   `c`, because the reader reads a chunk only after its worker has sent it back. So every
   state in the probed generation was admitted at some move `m_j` with `j < k`. By the
   induction hypothesis it has the index R1 gave it. By (2), it is still in the table at
   `m_k` with that index. R1's `seen.find` at `m_k` therefore returns the same `Some(i)`.
   It does not matter when the worker asked: the probe reads a prefix of R1's history,
   and a prefix never contradicts a later state.
4. **A worker's miss is only a hint.** On `Miss`, the reader probes Σ's own table, which is
   R1's table at `m_k`, so the answer is R1's. On `SameAs(k')` with `k' < k` in the same
   chunk, R1 at `m_k` would find the state admitted or found at `m_{k'}`, which is
   `resolved[k']`, an R1 value by the hypothesis.
5. **Every stop is the reader's.** The pattern cap (`intern`), the state cap, the deadline,
   the memory limit (`admit`) and the all-violated stop (`stop_if_all_failed`) are
   evaluated on the reader at the move that causes them. They read only Σ.
   `bytes_with_one_more` reads lengths and capacities of the states, parents, edges,
   `orbit_self`, the bindings' bytes, the layers, and `Seen::bytes_with`. All of these are
   reader-owned, and the table's growth rule depends only on its length. Workers charge
   nothing.
6. **Work past a stop is discarded.** Chunks expanded beyond the stop are dropped
   unread, as today. Workers write nothing that Σ reads: they read the table and the
   segments, and they fill their own `Moves`.

Against today's code: in `take`, the only per-edge read of shared order-dependent data is
`self.seen.find(&self.states, key)`, and its only use is `Some(to)`, the number of an
admitted state. `admit`'s own `seen.find` repeats it. Step 3 replaces that call with a
worker's `Hit(to)` or `SameAs`. Steps 2 and 4 show the replacement returns what the call
would have returned. `Bindings::id` assigns ids in first-seen order, `intern` assigns
pattern ids in first-seen order, and `violation` keeps the first violation per name. All
three stay on the reader and see the moves in R1's order. The deadline is wall-clock and
already differs between machines, as it does today.

### What 3.1 buys

<!-- derived-from #the-reader-today -->
<!-- derived-from #hits-loops-and-stale-misses -->

Estimated reader share of the one-thread run after 3.1, from the profile and the stale
table, at 8 jobs:

| | food-permit-ops | clinic-ops | insurance-claim-ops |
|---|---:|---:|---:|
| Reader today | 36.0% | 32.0% | 23.8% |
| Reader probes per edge, today (non-loop edges plus `admit`'s second probe) | 0.803 | 1.088 | 1.121 |
| Reader probes per edge after 3.1: fresh, plus stale outside the chunk | 0.285 | 0.285 | 0.437 |
| Same, without the worker's `SameAs` dedup | 0.422 | 0.717 | 0.768 |
| Reader after 3.1 | **15.4%** | **14.0%** | **9.6%** |
| Same, without `SameAs` | 16.6% | 16.9% | 11.1% |

How the estimate is made. The `seen.find` share is scaled by the probe ratio, less its
hashing (26% of it). `Bindings::id` falls to a tenth. The copies fall to a fifth. The
self-loop compare and the checks go to the workers. The rest stays on the reader: `take`'s
own code, the rehash on growth, `Moves::get`, and the insert, push and charge. This
ignores phase 2, which shrinks the compare, the hash and the copies on both sides and adds
the pack on the workers.

For food-permit-ops, whose one cone is 98% of the BFS work, the wall time model is
`T(n) = max(reader, BFS / n) + outside BFS`. It assumes the reader expands chunks itself
whenever it would otherwise wait (see [Efficiency cores](#efficiency-cores)). Without that,
moving work off the reader makes 2 jobs slower than today: one worker would carry 74% of
the run. Applied to today's shares at 8 jobs, with today's reader, the model gives 2.16x,
against 1.94x measured: it is 11% optimistic. After 3.1, with BFS at 89.8% of the run:

| Jobs | 2 | 4 | 6 | 8 | 18 |
|---|---:|---:|---:|---:|---:|
| Speedup, model (computed) | 1.8x | 3.1x | 3.9x | 3.9x | 3.9x |
| Same, less the model's 11% (estimated) | 1.6x | 2.8x | 3.5x | 3.5x | 3.5x |
| Today, measured | 1.57x | 1.91x | 1.95x | 1.94x | 1.93x |

Up to 4 jobs the expansion bounds the run. From 6 jobs on, the reader (15.4%) plus the
serial part outside BFS (10.2%) is the bound: 1 / 0.256 = 3.9x. The factors "a tenth" and
"a fifth" above are guesses of the same size as the gap to 4x, so the estimate is a range,
3.5x to 3.9x, not a verdict.

## Symmetry

<!-- constrained-by ./instance-symmetry.md#the-canonical-form -->
<!-- constrained-by ./instance-symmetry.md#memory -->

#212 phase 1 already canonicalizes in the workers (`Moves::canonicalize`). 3.1 continues
from there.

- The worker packs the canonical form (A2), hashes it, probes with it, and dedups `SameAs`
  on it. The orbit-self flag compares the canonical form with the stored source row, which
  the worker reads from its segment.
- `check_after` reads the concrete successor, as it does today. The invariants read the
  canonical form that would be stored, as `admit` does today.
- `orbits.admit(found)` stays on the reader. It sums per admitted state, and the order of
  admission is R1's.
- **Requirement:** the canonical form must stay a pure function of the state. `BRANCH_CAP`
  keeps the first branches in a fixed order, which is pure. An adaptive cap per thread, or
  a cache shared between calls, would break step 1 of the proof.
- #212 phase 2 (S up to H, in flight) runs inside the reduced step, so on the workers. It
  does not touch the reader. 3.1 does not touch `reduce/` or `symmetry/`.
- **Test gap.** No test runs a run up to symmetry on several threads: `run_orbits` always
  uses `Threads::Shared`. 3.1 adds one (see [Holding it correct](#holding-it-correct)).

## The memory pool

<!-- constrained-by ../cli-reference.md#memory-limit -->
<!-- constrained-by #the-shared-table -->

- **`store_bytes` and the stop points do not move in 3.1.** The table keeps
  `Seen::bytes_with`'s formula and growth points, so `bytes_with_one_more` is the same
  function of the run's lengths at 1 job and at 18. Phase 2 moves the stop points; 3.1
  does not.
- **Every charge stays on the reader.** `memory::watch` records crowding in a thread-local
  (`WATCH`), and `Charge::cover` runs on the run's own thread, inside `admit`. If a charge
  moved to a worker, `watch` would miss it, and the crowded rerun of #260 would not
  trigger. 3.1 keeps `admit` on the reader for this reason.
- **The crowded rerun is unchanged.** A run refused memory it would have had alone is run
  again alone, after `in_parallel` returns (`targets.rs`). Its stop point is the same at any
  thread count, so the rerun's result is too. 3.1 changes which runs are crowded only
  through timing, which is the case #260 already handles.
- **Uncharged, as today (#237):**
  - The old table generation alive after a growth: at most half the current table, for at
    most `helpers × AHEAD` chunks. Today hashbrown's resize also holds both tables while it
    rehashes, and that is not charged either.
  - The workers' `Moves`, which now carry about 20 bytes more per move: hash, probe
    result, flags, dense binding key, assert bits. At 17 helpers × 4 chunks × 128 states ×
    8.4 moves × about 90 bytes, that is about 6.6 MB (computed).
  - Each worker's `SameAs` map, at most one chunk of successors.

  Phase 4 decides whether to charge these.

## Helpers by need and the jobs cap

This section is item 3.2.

**Today.** `Helpers::take` hands every free helper to the first run that has two chunks
waiting, and the run holds them until it ends. On clinic-ops at 8 jobs, the 2.24 M-state
run held 7 helpers and used about one. The 2.15 M-state run beside it got none, and
whichever run asks first changes from run to run (bimodal wall time, measured). Before 3.1
this costs little, because no run can use more than one helper. After 3.1 a run can use 5
or 6, so the order of asking would decide the wall time.

**Design.**

1. **One pool of helper threads per pass, fed by every run.** `jobs() - 1` helpers are
   started in a scope around a pass's `in_parallel`. Every run hands its chunks to one
   shared queue. The queue serves runs round-robin, so a run gets helpers in proportion to
   its waiting chunks. A run keeps at most `AHEAD × helpers` chunks in flight, as
   `has_room` does today. A run's system moves into an `Arc`, so chunk tasks outlive the
   borrow that `Workers` holds today.
2. **A run never holds an idle helper.** At the end of a layer, a run with fewer than two
   chunks waiting expands on its own thread, as it does today. The helpers it used serve
   other runs in the meantime.
3. **Determinism is unaffected.** A run's result does not depend on which thread expanded
   its chunks; that is the argument of [Why the run is the one-thread run](#why-the-run-is-the-one-thread-run).

**The jobs cap.** `jobs()` is `min(cores, 8)` and sets two things. One is how many cones
run side by side, and each cone holds its own store. The other is how many threads expand
one cone's moves. Measured today: no gain past 4 to 6 jobs. After 3.1, the model says
food-permit-ops stops gaining at about 6. Proposal:

- Split the two settings. Cones side by side stay at `min(cores, 8)`, because each one
  holds a store and the pool already bounds them. Helpers become
  `available_parallelism() - 1`. `RDRA_JOBS` sets both, as today.
- Choose the final helper default from a sweep after 3.1: 4, 6, 8, 12 and 17 helpers on
  food-permit-ops, clinic-ops and insurance-claim-ops, here and on #315's base M5.

[Since #371 `jobs()` sets only the cones side by side. The threads of one run are
`bfs_threads()`, `max(4, cores / 3)` and at most 12. `RDRA_JOBS` still sets both.]

## Efficiency cores

<!-- constrained-by #helpers-by-need-and-the-jobs-cap -->

`sysctl` names the two tiers of this machine `hw.perflevel0` "Super" × 6 and
`hw.perflevel1` "Performance" × 12. The task calls them performance and efficiency cores.

**Measured:**

| food-permit-ops, 1 job | Wall | Cycles | Instructions | IPC | Average clock |
|---|---:|---:|---:|---:|---:|
| Default QoS | 4.15 s | 17.7 G | 89.8 G | 5.1 | 4.4 GHz |
| `taskpolicy -c utility` | 3.99 s | 17.5 G | 90.0 G | 5.1 | 4.5 GHz |
| `taskpolicy -b` (background) | 16.0 s | 21.9 G | 90.1 G | 4.1 | 1.4 GHz |

The instructions stay the same in all three. Background QoS confines the process to the
second tier at a low clock: 3.9x slower, with 20% lower IPC and a third of the clock.
Utility QoS stayed on the first tier. The second tier's speed at default QoS was not
measured. Neither class that `taskpolicy` sets puts a full-clock thread there: utility
stays on the fast tier, and background is clocked down. Several runs at once would fill
the fast tier, but this study ran one measurement at a time. 3.9x is therefore the worst
case.

**What the design does about it:**

- **The reader is the serial part, so it must run on the fastest tier.** macOS has no core
  affinity, and QoS is the only lever. Reader threads (the cone threads of `in_parallel`)
  stay at the default QoS. Helpers never go below it either: background QoS would cost 3.9x
  on exactly the work that 3.1 makes parallel.
- **Ordered reading makes the slowest chunk the head.** The reader reads chunks in order.
  A chunk on a slow core holds back everything after it once the reader catches up. Two
  mitigations, both safe because expanding a chunk is pure:
  1. Chunks are claimed, not pushed. While the reader would wait, it claims the oldest
     chunk no helper has claimed, the head first, and expands it itself. It keeps the
     result until it reaches that chunk. Today it expands only when nothing is in flight.
     This is also what keeps 3.1 from being slower than today at 2 jobs (see
     [What 3.1 buys](#what-31-buys)).
  2. Optionally, if the head chunk is claimed but not done after a time threshold, the
     reader expands it too and takes whichever copy finishes first. Both copies are equal.
     This costs CPU and is kept only if the sweep shows head stalls.
- **The lookahead (`AHEAD = 4` chunks per helper) absorbs speed differences.** It holds
  `4 × helpers × 128` states, so a chunk can take several times longer than the reader's
  pace before it becomes the head. A larger window would hide slow cores better, but the
  stale table shows it also costs probes: stale misses grow from 0.235 to 0.308 of edges
  between 512 and 8,704 states on food-permit-ops. Keep 4 and measure.
- **On this machine,** the reader plus 5 helpers fit on the 6 Super cores, and 3.1's model
  saturates at about 6 jobs. The second tier matters only for side-by-side cones and for
  3.3.
- **On a base M5 (#315: 4 + 6 cores),** the reader plus 3 helpers fill the fast tier. After
  3.1 the reader saturates when the helpers deliver `(BFS - reader) / reader = (89.8 -
  15.4) / 15.4 ≈ 4.8` fast-core equivalents (computed for food-permit-ops). So the 6 slower
  cores decide whether 3.1 reaches its bound there. #315 needs a method for the second
  tier's speed at default QoS first. Candidates: per-cluster residency and frequency from
  `powermetrics` during an 18-job run, or one 1-job run started while a known load holds
  the fast tier, if the one-at-a-time rule allows it.

## Holding it correct

<!-- constrained-by #why-the-run-is-the-one-thread-run -->

Phase 3 changes scheduling, so there is no `reference.rs` twin for it (CLAUDE.md, "A
speed-up change"). The parts that compute something new do get one.

1. **Output diff at 1, 3, 8 and 18 jobs.** `scripts/verify-samples.sh` with `RDRA_JOBS` at
   each count, against main's output at 1 job, at the `// expect` flags and without
   `--with-tlc`. Add the same with `--symmetry always` on the roots #212 covers. This study
   established that main itself is byte-identical across 1 to 18 jobs on the three samples
   measured.
2. **#163's any-thread tests, extended.** `a_run_is_the_same_on_any_number_of_threads`,
   `a_run_stops_at_the_memory_limit_at_the_same_state_on_any_number_of_threads`,
   `a_run_stops_once_every_check_has_failed` and
   `a_run_reports_the_checks_it_evaluates_and_nothing_else` move to thread counts
   `[1, 2, 3, 4, 7, 17]` and also compare `kept`, `peak` and `layers`. New:
   - The same comparison for `run_orbits` (`Threads::Fixed`) under a symmetry group.
   - A run whose table starts at 4 buckets, so it grows many times while chunks are in
     flight.
   - A test-only jitter (`cfg(test)`: a random sleep before a worker probes and before it
     sends a chunk), so `Hit`, stale `Miss`, `SameAs` and old-generation probes all occur.
     Debug counters assert that each occurred.
   - `memory_cli.rs`'s 1, 3 and 8 job comparisons, unchanged, and once with jitter.
3. **`shadow`, the table against `Seen`.** The hashbrown `Seen` moves to `reference.rs`
   beside its `BTreeMap` twin. Under `--features shadow`, every reader probe is compared
   with the twin. So is every worker `Hit(i)`: at read time, the twin must hold the same
   row at `i`. So is every `SameAs(k)`: the twin's answer for move `i` must equal
   `resolved[k]`. So is every dense binding id, against `Bindings::id`'s map. Each
   comparison is counted for `RDRA_ISH_SHADOW_STATS`. A planted bug is shown caught: a
   worker that reports `Hit` for a 7-bit tag match without comparing rows.
4. **loom for the publish protocol.** The protocol is the control word, the index and the
   segment row, plus the generation swap. Model: one writer inserting 3 keys with one
   growth, and two readers probing concurrently. Assert that every `Hit(i)` names the
   probed key's row, and that after the writer finishes, every key is found. The table's
   atomics go through a `cfg(loom)` shim. loom fits this; Kani does not reason about
   interleavings.
5. **Kani for the sequential parts.** The 8-lane tag match returns exactly the lanes whose
   byte equals the tag. A probe sequence finds every inserted key for tables up to 16
   buckets. The growth rule equals `Seen::bytes_with`, in the style of the existing store
   harnesses.
6. **Miri** on the table's unit tests and on a small any-thread run, for the `unsafe`
   segment reads.
7. **Speed gate, in instructions at 1 job:** no more than 2% above main on the five runs of
   #296. At 1 job 3.1 adds no work, only moves it. Wall time at 6, 8 and 18 jobs is
   measured on a quiet machine, as here.

## 3.1 as built

<!-- derived-from #workers-probe-the-reader-commits -->
<!-- constrained-by #assumptions-about-phase-2 -->

### The assumptions, checked on main

Checked on e653976b, main with phase 2 (#321, #326, #347) and #212 phase 4 (#370).

- **A1 holds for rows and fails for the directory.** `segments::Rows` allocates each segment
  whole and never moves a row. But its directory, `Rows::rest`, is a `Vec` of segment `Vec`s
  that doubles, and each segment's length is written on every push, so a worker cannot read
  the store while the reader pushes. Per [If phase 2 lands differently](#if-phase-2-lands-differently),
  part 2 adds its own fixed view of the segments' addresses for the workers. `Rows` and its
  charge (`parts_with` counts the directory's slots) do not change.
- **A2 holds.** `Packing::pack` and `Packing::patch` read only the packing, which every
  thread of a run shares in an `Arc`.
- **A3 holds.** `Seen` hashes and compares packed rows.
- **A4 holds.** `Seen::bytes_with` reads hashbrown's capacity. Part 2's table copies
  hashbrown's growth and pins it to hashbrown.

### Where the time goes after phase 2

Measured on e653976b's parent with the method of [Measurement method](#measurement-method):
`sample` at 1 ms of a build with `#[inline(never)]` on the hot functions, food-permit-ops,
1 job, at a 1-minute load of 13 to 18.

| Part of `Bfs::take`, share of the run | food-permit-ops |
|---|---:|
| `Bfs::take`, all of it | **49.5%** |
| · `States::patch` (2.3's pack of the successor) | 18.0% |
| · `Seen::find`, its hash and compare included | 11.4% |
| · `admit`: insert, rehash, push, charge | 4.3% |
| · `Bindings::id` | 3.6% |
| · `Packing::unpack` of the source state | 3.2% |
| · copies and the self-loop compare | 4.2% |

2.3 added the pack to the reader: `take` went from 36% to 49.5% of the run. At 8 jobs the
reader spends 90% of its samples in `take` and each worker waits 88% to 91%.

### Part 1: workers pack, hash and deduplicate

The expanding thread (`expand`, a worker's or the reader's own) computes per move, into
`Probes` (`explore/probe.rs`):

- `Loop` when the successor is the source, `OrbitSelf` when its canonical form is;
- else the key packed as the store keeps it (patched from the source's packed row, or the
  canonical form packed whole) and its hash;
- `SameAs(k)` when move `k` of the same chunk had the same key, else `Miss`;
- the binding's number: per action an offset and the params in the radix of their
  entities' slot counts (`Numbering`), up to 65,536 numbers.

The reader re-probes every `Miss` with the given hash, takes `SameAs(k)` from what it found
for `k`, reads the binding's id from an array by its number (the first sight goes through
`Bindings::id`, so ids and bytes are unchanged), and unpacks the source only when the run
gathers views. Chunks are claimed from a queue rather than sent: while the chunk the reader
reads next is not done, the reader expands the oldest chunk no worker has claimed. Without
that, 2 jobs were 11% slower than main (4.39 s against 3.95 s, measured once): the one worker
then carries the moved work.

The argument of [Why the run is the one-thread run](#why-the-run-is-the-one-thread-run) holds
as written: part 1 has no `Hit`, and `SameAs` is its step 4.

**Measured** (e653976b and the branch, median of three, interleaved, 1-minute load 4.5 to
9.9; a run waited for a load under 10; insurance-claim-ops ran at loads up to 35, so its 1-job
medians are rough):

| | food-permit-ops | clinic-ops | insurance-claim-ops |
|---|---:|---:|---:|
| Main, 1 job | 6.09 s | 5.73 s | 14.77 s |
| Main, 8 jobs | 3.55 s, 58% | 2.27 s, 40% | 6.24 s, 42% |
| Part 1, 1 job | 6.15 s | 5.65 s | 16.45 s |
| Part 1, 8 jobs | **2.16 s, 35%** | **1.72 s, 30%** | **4.37 s, 27%** |
| Instructions at 1 job, part 1 against main | +0.5% | -0.4% | -0.6% |

food-permit-ops at 2 / 4 / 6 jobs: main 3.50 / 3.45 / 3.39 s, part 1 3.29 / 2.22 / 2.14 s
(53% / 36% / 35% of its 1-job time).

After part 1 the reader is 22% to 23% of a 1-job food-permit-ops run (profile at 1 job),
of which `Seen::find` is 7% to 9.5%, `admit` 3.3% to 3.8% and the edge push 2.4% to 3.4%.
With 12% of the run outside BFS, that bounds 8 jobs at about 1 / (0.23 + 0.12) = 2.9x
(computed), as measured. The exit needs about 1 / 0.30 - 0.12 = 0.21 or less on the
reader, and part 2 (workers probe the table) is the lever: the reader then probes only what
the workers could not see.

### Part 2: the shared table

<!-- constrained-by #the-shared-table -->
<!-- constrained-by #why-the-run-is-the-one-thread-run -->

Part 2 builds what the workers need to probe the run's table, and it measures the probes.
A release build leaves the probes off: on two of the three samples they cost more than they
saved (see the measurements below). With the probes off, a run keeps hashbrown's table and
takes part 1's path. Part 3 decides whether to turn the probes on.

A run whose workers probe keeps, as `Seen`, the table of [The shared table](#the-shared-table)
(`explore/table.rs`). One thread writes it, and any thread probes it, without a lock and
without `unsafe`:

- **Layout.** Eight 7-bit tags share an `AtomicU64` control word. Each tag has an
  `AtomicU32` index beside it. Probing is triangular over groups.
- **Insert.** The writer stores the index (`Relaxed`), then the control word (`Release`).
- **Growth.** The writer builds the next generation privately and makes it current. A
  chunk holds the generation that was current when it was handed out.
- **Capacity.** The table grows where hashbrown grows. A unit test pins it to hashbrown,
  insert by insert, and a `shadow` twin (`seen_growth`) checks it on every insert of a run.
  So `Seen::bytes_with`, `store_bytes` and the `--memory` stop points do not change.
- **Proofs.** Two Kani harnesses cover the table. `lanes_are_exactly_the_equal_bytes`
  proves the tag match for every word and tag. `growth_keeps_room_for_one_more` proves the
  growth rule.
- **Speed.** The table finds a state more slowly than hashbrown. In a microbenchmark of 4.6
  million states, with half of the look-ups missing, it took 87 ms against hashbrown's 59 ms
  (**measured**, 1.5 times as long). Two variants did not close the gap: `Relaxed` loads on
  the writer's own thread, and each group's indexes beside its control word (93 ms). With
  the probes off, a release build that kept this table was 5% slower than main on
  food-permit-ops at 8 jobs (2.11 s against 2.01 s, median of five, 1-minute load 6) and
  3.5% slower on clinic-ops (1.76 s against 1.70 s). So a run that does not probe keeps
  hashbrown's table.

A chunk is handed out with a `Shared`, which holds the table and a `Published`
(`explore/arena.rs`). A `Published` holds the addresses of the store's segments and the
number of rows at hand-out. A segment never moves (A1), and a pushed row is never written
again. The chunk queue's lock orders the rows' bytes before the worker reads them.
`Bfs::go` runs the workers in a `std::thread::scope` that ends after `explore` returns, so
the store outlives every reader of it. Reading a row is the one `unsafe` read
(`Published::row`). A unit test reads published rows on another thread while the store
grows. Miri has not run on it: the toolchain has no Miri component.

The workers' probe works as follows:

- **The probe.** The expanding thread probes each `Miss` of part 1 against the table. It
  takes `Hit(i)` only when `i` is a published row and that row's bytes equal the key. So
  a hit is right whatever the memory ordering gave the probe: this is step 3 of
  [Why the run is the one-thread run](#why-the-run-is-the-one-thread-run). A `shadow` twin
  (`hit`) holds every hit to the reader's own look-up.
- **A miss.** A move that is still a `Miss` tells the reader that the successor is none
  of the states below the chunk's published count `P`. The reader looks it up only among
  the states admitted since `P`, in a small table (`Recent`) that stays in cache. When
  `Recent` does not hold every state from `P` on, the reader uses the run's table. This is
  step 4 with a smaller table. A `shadow` twin (`recent`) holds each look-up to the run's
  table.
- **The switch.** `Probing` turns the probes on: at 4 workers or more in the unit tests,
  and at any worker count in a `shadow` build under the test-only `RDRA_ISH_FORCE_PROBES`.
  One run of the CI's shadow job (clinic-ops) sets that variable. Two planted bugs were caught under it on
  clinic-ops at 4 jobs: a hit without the row compare (`shadow: Probe::Hit(157) at move
  118 of state 157: reference None`), and `Recent` letting go of 64 states it still had to
  hold (`shadow: Recent::find(..): None, the run's table Some(558)`).

**Measured, probes on** (a build of the branch with the probes on and the shared table, against
main 3b4860cf, which holds part 1; RDRA_JOBS set; median of three, interleaved). That build
also moved work off the reader: a worker copied its chunk's states out of the store itself,
dropped the chunks the reader had read, and spun briefly before it slept. With hashbrown's
table and the probes off, those three together measured within noise of main, so part 2
does not keep them. food-permit-ops at a
1-minute load of 3.5 to 4.2:

| Jobs | 1 | 2 | 4 | 6 | 8 | 12 | 17 |
|---|---:|---:|---:|---:|---:|---:|---:|
| Main | 5.81 s | 3.28 s | 2.11 s | 2.05 s | 2.11 s | 2.19 s | 2.17 s |
| Probes on | 5.90 s | 3.30 s | 2.19 s | **1.81 s** | **1.86 s** | 1.87 s | 1.89 s |

At 8 jobs, the probes take 12% off the run: it is 32% of main's 1-job time, short of the
≤30% exit. The 4-job run had 3 workers probing. Probing from 4 workers on (`PROBING`) gives
2.19 s at 4 jobs, against main's 2.17 s. 12 and 17 jobs gain nothing over 6 to 8 on either
branch.

At 8 jobs on all three samples, at a load of 5 to 7:

| | food-permit-ops | clinic-ops | insurance-claim-ops |
|---|---:|---:|---:|
| Main | 2.00 s | 1.74 s | 4.20 s |
| Probes on | **1.81 s, -9.5%** | 1.81 s, +4% | 4.39 s, +4.5% |

**Measured, as built** (the probes off, hashbrown's table; against main cfe781d4; 8 jobs,
median of seven, interleaved, at a 1-minute load of 5 to 9):

| | food-permit-ops | clinic-ops | insurance-claim-ops |
|---|---:|---:|---:|
| Main | 1.96 s | 2.02 s | 4.08 s |
| Part 2 | 1.93 s | 2.02 s | 4.17 s |

The runs of each sample overlap: food-permit-ops 1.95 to 2.09 s against 1.91 to 2.31 s,
clinic-ops 1.71 to 2.32 s against 1.65 to 2.22 s (both bimodal), insurance-claim-ops 3.93 to
4.43 s against 3.99 to 4.53 s. food-permit-ops at 1 job: 5.54 s against 5.48 s. Instructions
rise 0.1% to 0.3%. The shared table's look-up and insert sit out of line (`find_shared`,
`insert_shared`): inlined beside hashbrown's, a build that did not probe was 2.5% slower on
food-permit-ops at 8 jobs (2.09 s against 2.04 s, median of seven).

These costs were measured with the slower table, so part 3 should not read them as the cost
of the probes alone. The probes cost the CPU. On clinic-ops at 8 jobs, user time rose from
7.0 s (main) to 7.6 s with that build's probes off, and to 8.5 s with them on, while
instructions rose 5.7%. So the
probes stall: they miss the cache in a table of about 40 MB and in the store's rows. The
reader keeps up in none of these runs: of the chunks the biggest run of each sample reads,
95% to 100% were probed, and the reader almost never waited on a worker.

Two gates did not help. One probed only while the workers kept up with the reader, and the
reader seldom waited. The other probed only when a run held the whole helper budget, and
the big runs always did.

**Hypothesis, not verified:** this machine has 6 performance cores and 12 efficiency
cores. When several big cones overlap, as in clinic-ops and insurance-claim-ops, the
probes' extra CPU may push cone readers onto efficiency cores. In food-permit-ops one cone
runs alone at the end. Part 3 tests this before it designs around it: for example,
clinic-ops with the probes on at 6 jobs against 8, and whether the slowdown appears only
when cones overlap.

**Profile, probes on** (food-permit-ops, 8 jobs, `sample` at 1 ms): the reader spends
80% to 87% of its samples in `take`. Inside `take`, `admit` takes 22% of the reader thread,
of which the table insert is 10% to 16%. The edge push takes 14% to 19%, `Moves::get` 5%,
and `check_after` 2% to 4.5%. A second thread then runs `state_model_findings` for the big
cone for 0.5 to 0.6 s after its BFS ends (`read_entity` in `targets.rs`), on one thread.
That tail is a third of the 1.86 s 8-job run. Part 3 takes it off the critical path.

### Part 3: the findings tail

<!-- constrained-by #part-2-the-shared-table -->

Once the largest cone of food-permit-ops has finished its BFS, its thread computes the
state-model findings on its own (`state_model_findings`, from `read_entity` in
`targets.rs`). That work took 0.52 s of a 1.93 s run at 8 jobs (**measured**, `sample`):

- Most of it went to the deadlock pass's per-column, per-slot `can_leave` sweeps, about
  0.29 s.
- Building the graph's adjacency took about 0.17 s.

Both parts are independent work, so part 3 runs them side by side on the helpers that the
process has free (`parallel::Helpers`), which the BFS gave back when it ended:

- **Adjacency.** `Graph::adjacency` builds its two directions side by side.
- **Leave columns.** The deadlock pass computes the `can_leave` of every column and slot up
  front (`leave_columns`). It does so with `parallel::in_parallel`, which returns the results
  in item order.

The result does not depend on the thread count. In a `shadow` build each change has a twin
that checks it on every call: `side_by_side_adjacency` against `Adjacency::new`, and
`leave_columns` against `can_leave` on each column and slot in turn. Two planted bugs were
caught, on clinic-ops at 4 jobs:

- The predecessors built as successors: `shadow: Graph::adjacency of 242015 states: the
  directions built side by side differ`.
- The columns rotated by a slot: `shadow: leave_columns(EntId(1)): column VarId(3) at slot 0
  differs`.

After part 3 the tail is about 0.31 s, made up of three pieces:

- `leave_columns`' own share, about 0.15 s.
- The adjacency, about 0.09 s.
- The per-value scan for a stuck state, and `runs`.

**Measured** (part 3 against main cfe781d4, RDRA_JOBS set, interleaved, 1-minute load 2.4
to 4.5; 8 jobs median of five, else of three):

| food-permit-ops, jobs | 1 | 2 | 4 | 6 | 8 |
|---|---:|---:|---:|---:|---:|
| Main | 5.54 s | 3.15 s | 2.04 s | 1.93 s | 2.10 s |
| Part 3 | 5.58 s | **2.91 s** | **1.81 s** | **1.65 s** | **1.81 s** |
| Part 3, share of main's 1 job | | 53% | 33% | 30% | 33% |

| | clinic-ops | insurance-claim-ops |
|---|---:|---:|
| Main, 1 job | 5.67 s | 14.44 s |
| Main, 8 jobs | 1.74 s | 4.23 s |
| Part 3, 1 job | 5.75 s | 14.54 s |
| Part 3, 8 jobs | **1.58 s, 28%** | **3.52 s, 24%** |

Instructions rise 0.3% to 0.8%; one job takes the time it took. food-permit-ops at 8 jobs
stays at 33% of main's 1-job time, short of the ≤30% exit; at 6 jobs it reaches 30%. The
reader of the largest cone is now what the run waits on; items (a) and (b) of the owner's
follow-ups (the table rebuilt in store order, and the reader's look-ups prefetched) aim at
it.

With the probes of part 2 turned on as well, in a build made for this measurement only:

| | food-permit-ops | clinic-ops | insurance-claim-ops |
|---|---:|---:|---:|
| Part 3 | 1.80 s | 1.82 s | 3.72 s |
| Part 3, probes on | 1.71 s | 1.83 s | 3.92 s |

The probes still cost insurance-claim-ops 5%, so they stay off.

### Follow-up (a): the table grown in store order

<!-- constrained-by #part-2-the-shared-table -->

hashbrown 0.17's growth walks the old buckets and hashes each state again, which reads the
store's rows at random. When a run's own table (`Index::Local`) is full, `Seen` now builds
the next one as `with_capacity(len + 1)` and files the states `0..len` in store order
(`grown_in_store_order`), reading the rows as they lie. hashbrown's growth gives the same
capacity, and the `seen_growth` twin, now on every insert of every run, holds the two equal,
so `bytes_with` and the `--memory` stop points do not change. A planted capacity of
`2 * len + 1` was caught: `shadow: Seen::insert(7): capacity 28, hashbrown's 14`.

**Measured** (against part 3, food-permit-ops, RDRA_JOBS set, interleaved, 1-minute load
3.5 to 4.8): at 1 job 5.64 s against 5.69 s and 24.31 G cycles against 24.55 G (-1%), at
the same instructions; at 8 jobs 1.72 s against 1.72 s (median of five), below the noise. A
microbenchmark of synthetic rows had put the rebuild at about 70 cycles a state against about
125 (the owner's second opinion); the rebuilds are a small share of a run.

## Beyond 3.1: in-flight dedup

<!-- constrained-by #hits-loops-and-stale-misses -->
<!-- constrained-by #what-31-buys -->

This section describes a possible item 3.3, which is #254 step 5. After 3.1, the reader's
remaining probes are the fresh states and the stale misses from other chunks in flight:
0.285 to 0.437 per edge. Each is a hash probe with a given hash, about 2% to 2.5% of the
run. Two ways remove them:

- **A pending-claims table.** Each worker inserts every missed successor, tagged with its
  global move position, into a second concurrent set that keeps the least position per
  successor (a compare-and-swap loop). When the reader reaches move `m`, every earlier
  move's claim is in place, because chunks are read in order and only after their workers
  finish. So "an earlier move has the same successor" is decided exactly. The reader then
  takes that move's number from a ring buffer over the window. The table is cleared by
  generation as the window slides. It has many writers, so it needs a separate loom model.
- **Barrier windows.** Workers expand a window of chunks, partition the successors by hash
  into shards, and dedup each shard against the frozen table. The reader numbers the new
  states in move order and cuts at the first state that does not fit, and the shards insert
  the states up to the cut. There is no pipelining across the barrier.

Either takes the reader to about 9% on food-permit-ops (computed: 15.4% less the probes
and the rehash). Then the expansion bounds the run at 8 jobs: 4.7x by the model, 4.2x less
its 11%, and only if all 7 helpers run at full speed. 4x is 3.3's exit
([Decisions](#decisions)). Decide after 3.1 is measured, as
#254 says. Independently, the 10.2% outside BFS (the findings and `Adjacency::new` of the
one large cone, start and exit) bounds every design at about 10x on food-permit-ops. A
parallel adjacency build is the lever there, and it is outside #296's list.

## If phase 2 lands differently

<!-- constrained-by #assumptions-about-phase-2 -->

- **2.2 does not land, or its segment directory reallocates.** Workers cannot read stored
  rows while the reader appends. 3.1 would need a copy of each key in the table, which
  doubles the state bytes and changes `bytes_with`, or a fixed directory added by 3.1
  itself. 2.2 is the one hard prerequisite.
- **2.3 does not land.** Nothing in the argument changes. Hashes and compares are over the
  unpacked width, so the probe costs on both sides stay proportional to 40 to 66 bytes.
- **2.3 packs only for storage, and `Seen` compares unpacked rows.** The worker would have
  to unpack a stored row for each tag match. 3.1 should instead pack the successor and
  compare packed rows. That needs only that packing is injective, which 2.3's round-trip
  Kani harness proves.
- **Packing is not callable from a worker** (a codec built lazily on the reader, or one
  holding mutable state). 3.1 needs `&Codec: Sync` and a codec built before the first
  chunk is handed out.
- **2.3 changes `Seen::bytes_with`** (a stored tag, say). 3.1 copies whatever growth rule
  2.3 leaves. The stop points then move once, in phase 2, and not again in phase 3.
- **2.1 drops `from` from `Edge`.** The reader writes the compact edge in the same order.
  The argument does not change.

## Decisions

<!-- derived-from #what-31-buys -->
<!-- derived-from #beyond-31-in-flight-dedup -->

1. **The exits of 3.1 and 3.3** (owner, 2026-11-25, on
   [#320](https://github.com/manji-0/rdra-ish-dsl/pull/320)). #296's exit, food-permit-ops
   at 8 jobs in at most a quarter of its one-thread wall time, was estimated just out of
   reach for 3.1 alone (3.5x to 3.9x). The owner chose to split it:
   - **3.1's exit:** food-permit-ops wall time at 8 jobs at or under 30% of its one-thread
     wall time, with the same output at every job count.
   - **3.3's exit:** the same at or under a quarter (4x).

## Open questions for the owner

<!-- derived-from #helpers-by-need-and-the-jobs-cap -->
<!-- derived-from #the-memory-pool -->

1. **Helpers and cones split** (3.2). `RDRA_JOBS` keeps setting both. Should the default
   helper count become `available_parallelism() - 1`, with cones side by side staying at
   `min(cores, 8)`? This is a documented behavior of `RDRA_JOBS` in `cli-reference.md`.
2. **Uncharged transients.** The old table generation and the larger `Moves` are not
   charged, like today's hashbrown resize and #237's worker buffers. Leave them to phase
   4, or charge them now? Charging changes `store_bytes` and the stop points.

## Measurement method

- **Wall time, instructions and cycles.** `/usr/bin/time -l` on the plain release build of
  c51f2543. Three runs per point. Each run started only when the 1-minute load was under
  10 and the 5-minute load under 18. The actual 1-minute load before each run was 1.8 to
  4.5. One run at a time.
- **Per-run counts and thread CPU.** The `perf` build (`RDRA_PERF=1`), 1 job. Its
  thread-CPU numbers at 8 and 18 jobs are not used: the counting allocator contends across
  threads, giving 44 s of user time on insurance-claim-ops at 8 jobs against 15 s for the
  plain build.
- **Shares.** `sample` at 1 ms on the plain build with `#[inline(never)]` on the functions
  #296 names, at 1 job, and at 8 jobs on food-permit-ops per thread. Samples taken while
  `sample` attached at launch (`_dyld_start` alone) are left out.
- **Stale misses.** The same build with `perf` and a patch to `perf.rs`'s `RDRA_PERF=2`
  columns. For every non-loop edge, the patch takes the move that admitted its target, from
  `parents`, and compares that move's source with `chunk start - helpers × 512`. Not for
  merge.
- **Tiers.** A 1-job run under `taskpolicy -b` and one under `taskpolicy -c utility`.
- The binaries, raw outputs, scripts and patch are in session scratch (`p3/`, `raw/`,
  `bin/`), which does not survive a reboot. Every number in this document is reproduced
  above.
