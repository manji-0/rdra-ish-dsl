# Conformance stream: cases generated on the fly instead of a traces file

<!-- derived-from ../codegen.md#the-conformance-port-harness-and-traces -->
<!-- constrained-by ../codegen.md#what-a-conformance-run-shows -->
<!-- constrained-by ./code-check.md#the-self-check-by-mutant-traces -->
<!-- constrained-by ../compatibility.md#what-is-not-covered -->

Status: design study, not implemented. Measured against `f9925df7` on main.

## Summary

**Replace the traces file with a stream that rdra-ish generates from its BFS, over a socket
to the generated harness. The harness compares locally and writes back a stream of
results. The stream removes the file and the state cap. It does not make clinic-ops
complete: at check's bounds the joint BFS of clinic-ops does not fit in 4 GiB, and the
part that fits already holds about 4×10^8 moves and as many refused calls, which is days
of service time at database speed.**

The recommended design:

- **Transport.** rdra-ish listens on a loopback TCP port and passes its address in
  `RDRA_ISH_CONFORMANCE` to the run command it starts, as it passes `RDRA_ISH_TRACES` today.
  The harness connects. A containerised service uses `--conformance-connect HOST:PORT`, and
  then rdra-ish connects to it. The test runner keeps stdio, so `cargo test`, `go test -v`
  and `node` need no change of behaviour.
- **Wire format.** Newline-terminated text records in the grammar the traces file uses
  today, with a case number on each `T` and two new records: `D`, the change in the
  expected state, and `E`, the end of a case. Every harness already parses this grammar by
  hand (the Rust domain code has no dependencies, so it cannot use JSON). Text costs about
  5 times the bytes of a binary format. On loopback that does not matter, because the
  service takes 100 to 1,000 times longer per call than parsing the call does.
- **Cases.** A case is one walk: seed an initial state, then follow moves, as a trace does
  today. Walks amortise `seed`. Walks are generated from the finished (or stopped) BFS, so
  the stream is the same on every run whatever the service's speed. The generator then
  runs at the same time as the service and its shards, and the BFS, a few minutes at most,
  comes before them. [Ordering the cases](#ordering-the-cases) weighs this against walks
  generated during the BFS.
- **Comparison in the harness.** rdra-ish sends the expected state as a delta. The harness
  applies the delta and compares the store with the result. It reports one `F` record per
  failure, with the store's lines, and an `A <case>` acknowledgement every 256 cases or
  100 ms. The oracle stays with rdra-ish, and the comparator is generated code that
  `check --code=fresh` holds to the model.
- **Bounded memory and backpressure.** Each connection allows at most a window of cases
  that it has not acknowledged, by default 1,024 cases or 4 MiB. When the window is full,
  the generator stops. The BFS store and the generator's bookkeeping are charged to the
  `[check]` memory pool (default 4 GiB).
- **Completeness.** With no state cap, a BFS that stops at the memory limit or the time
  budget is incomplete. Incompleteness is an error by default. `--allow-incomplete` turns
  it back into the warning it is today.
- **Refusals per expanded state.** A state's moves are all known once it is expanded. The
  refusals of every state in the fully expanded prefix of the BFS are therefore sent even
  when the BFS stops. Today a capped run tries none. On clinic-ops at the 65,536 cap, that
  adds 341,548 refused calls.

[What the numbers say](#what-the-numbers-say) gives the measurements.
[What this does not fix](#what-this-does-not-fix-clinic-ops) explains why clinic-ops needs
a further, separate change (conformance per cone, under one more assumption) before the
default can be "complete or fail" for it.

## What exists today

`codegen --conformance <FILE>` runs a BFS of the whole system at `Bounds::STATES` (one row
per entity, Int `0..=5`), capped at `--conformance-states` (default 65,536). It writes:

1. Every state that a trace names, as `S`/`A` lines.
2. Trace-shaped walks (`T`, `R`, `G`, `C`, `X`) that take every move of a use case that
   has a function.
3. One trace per interleaving pair (`I`).
4. Up to 16 traces seeded at a reached state.

The harness (`conform` / `Conform`) reads the whole file, makes a fresh port per trace,
and compares after each row. `check --code=fresh` compares the file by hash.
`check --code=run` runs the language's test once on the real traces, then once per mutant
with `RDRA_ISH_TRACES` pointing at that mutant.

When the BFS stops at the cap:

- No refusal on an existing row is tried, because `refusals` returns nothing unless
  `run.complete` holds.
- The moves out of states that were never expanded are not run.
- The run still exits 0, with `warning[CodeCoverageIncomplete]`.

On clinic-ops this leaves 22 of the 90 use cases of the port never run. The file is
236,971,795 bytes and holds 181,111 traces.

## What the numbers say

Measured on an Apple Silicon machine (18 cores, load about 5) with a release build of
`f9925df7`. A measuring function replaced `traces()` for the run and was not committed.
It ran the same BFS as `traces()`, at check's bounds (one row per entity, Int `0..=5`).
The uncapped run had no state cap and was charged to a 4 GiB pool, as `check` is. The
function then computed what the stream would send, without building the text. Byte
figures are **measured text bytes** of the current grammar unless a row says
**estimate**. "Expanded" is the fully expanded prefix of the BFS: every layer before the
one it was reading, plus the states of that layer it had expanded.

| | ec-site | clinic-ops, 65,536 cap | clinic-ops, 1,000,000 cap | clinic-ops, uncapped |
|---|---|---|---|---|
| BFS | complete | stopped at the cap | stopped at the cap | stopped at the 4 GiB memory limit |
| States reached (expanded) | 54 (54) | 65,536 (21,494) | 1,000,000 (400,336) | 59,228,160 (21,026,949) |
| Moves of implemented use cases | 189 | 308,918 | 5,982,203 | about 4.2×10^8 (423,238,346 BFS edges) |
| Moves left out (tick, no function) | 0 | 0 | 16 | |
| Moves per expanded state | 3.5 | 14.4 | 14.9 | 20.1 |
| Refused calls on existing rows, today | 180 | 0 (none tried when capped) | 0 | 0 |
| Refused calls of the expanded states, stream | 180 | 341,548 | 8,348,201 | about 4.4×10^8 (**estimate**, 21 per state) |
| of which `NotEnabled` / `Rejected` / any kind | 162 / 0 / 18 | 298,306 / 8,730 / 34,512 | 7,680,003 / 84,897 / 583,301 | |
| Refusals untried (over 64 bindings) | 0 | 0 | 0 | |
| Use cases of the port refused at least once | 6 of 11 | 57 of 90 | 65 of 90 | |
| Use cases never run, today | 0 of 11 | 22 of 90 | | |
| BFS time | 0.17 ms | 0.69 s | 23 s for the whole command | about 2 min to the limit, then 10 min of growth estimate (below) |
| Peak RSS of the command | 13.6 MB | 313 MB | 904 MB | 5.7 GB |
| Walks (one seed each) | 59 | 180,874 | |
| Calls in walks (of which prefix replays) | 394 (205) | 1,615,988 (1,307,070) | |
| Call, text bytes, mean | 20.5 | 41.6 | |
| Expected state, text bytes, mean (rows) | 358 (8.1) | 1,054 (15.8) | |
| Expected-state delta per move, text bytes, mean (lines) | 101 (1.41) | 135 (1.52) | |
| Seed bytes if every move were its own seeded case | 69,969 | 307,410,291 | |

The peak RSS covers the whole `codegen` command. That includes the `check` it runs first
and the measuring function's own sets, which were not charged to the pool. In the
uncapped run, RSS went 1.4 GB past the 4 GiB pool, and the measurement does not say which
part of that was what. The BFS's own store at the 65,536 cap was 4.3 MB (`peak`), about
66 bytes per state. The 1,000,000 cap was run only to count refusals at a larger prefix.
The refused-call rows count each refusal of each expanded state once, which is what the
stream would send. A refusal is 50 to 54 text bytes on clinic-ops.

**Uncapped clinic-ops does not fit.** At check's bounds, with no state cap, the joint
BFS stopped at the 4 GiB memory limit (`MemoryCapReached`):

- It had discovered 59,228,160 states and expanded 21,026,949 of them, through depth 14
  and 6.8 million of the 26.5 million states at depth 15.
- The store was 4,295,171,360 bytes, so `peak` held the limit exactly.
- The layers grew by about 2.8 times per depth up to the last whole layer: 3.1 million,
  9.4 million, 26.5 million. Nothing in them suggests a peak.
- The whole command took 707 s of wall time. The store reached the limit within about
  2 minutes: an earlier run of the same command was in the cap's growth estimate
  (`Bfs::growth`), at 5.5 GB RSS, 2 min 14 s after it started. The remaining 10 minutes
  went to that single-threaded estimate. A stream generator should skip the estimate, or
  bound it.
- At the 65,536 and 1,000,000 caps, all but 0 and 16 of the BFS edges are moves of
  implemented use cases. The 423 million edges of the uncapped run are therefore about
  that many moves. Its refusals were not computed; the table's figure scales the 21 per
  state of the 1,000,000 cap.

So the whole reachable space at one row per entity is at least an order of magnitude past
4 GiB. Extrapolating the layers (an **estimate**), it holds well over 10^9 states.

**Bytes per case on the wire.** With the delta encoding, a move costs a call plus its
delta: about 120 text bytes on ec-site and 175 on clinic-ops. A refusal (`X <kind>
<call>`) costs 32 bytes on ec-site and 50 to 54 on clinic-ops. A seed costs the
state's rows twice (`R` and the expected `A`). A binary encoding would carry, per move:

- a sequence varint, 1 to 3 bytes;
- the action, 1 byte;
- the ids as varints, 1 or 2 bytes each;
- timestamps as varints from the run's epoch, 2 to 4 bytes;
- per changed row, the entity, the slot and one byte per tracked column.

That comes to about 25 to 35 bytes per move on clinic-ops (**estimate**), about 5 times
less than text. On loopback, 175 bytes is under 0.2 µs of copying. A service call that
reaches a database costs 50 to 500 µs (an **assumption**, not measured here). Text is
therefore not the bottleneck, and its
grammar already has a parser in each of the three harnesses.

**Seeds.** Per-edge cases (seed, then one call) would send 307 MB of seed rows on the
capped clinic-ops graph and call `seed` 308,918 times, against 180,874 walks. On that
graph the walks are poor, because most of the 65,536 states are unexpanded dead ends: a
walk replays 4.2 prefix calls per new move. On a graph whose states are all expanded, the
greedy walk takes many moves per seed. On ec-site, 59 walks cover 189 moves, with 205
prefix calls. Per-edge cases are also impossible for a state with a run mark or a raise,
because `seed` stores rows only. Walks stay.

## Design

### Roles and transport

rdra-ish holds the model, the BFS and the oracle. The probe's conformance test is the
"test server": it owns a port (`new`), consumes cases and writes results. rdra-ish starts
it with the run command the table names, as it does today.

| Mode | Who listens | When |
|---|---|---|
| default | rdra-ish, on `127.0.0.1:0` | the run command runs on this machine |
| `--conformance-connect HOST:PORT` | the harness (`RDRA_ISH_CONFORMANCE_LISTEN=ADDR` makes the generated test listen instead of connect) | the service runs in a container or on another host; rdra-ish does not start it |

Why not stdio: the run commands are test runners. `cargo test` captures the test's
stdout per thread, `go test -v` prefixes and interleaves output, and a custom `run`
command can be anything. A socket address in an environment variable has none of these
problems, and it is the mechanism `RDRA_ISH_TRACES` already uses. A Unix socket would
also work. TCP is chosen because all three standard libraries have it and a container
can reach it.

The connection opens with a handshake:

```
> # rdra-ish conformance stream 1
> H model=<model hash> bounds=1,0..5 harness=<hash of the generated harness> shard=0/1
< H ok harness=<hash the harness was generated with>
```

The harness compares the model hash and its own generated-code hash with what it was
generated with, and refuses a stream from another model. That replaces the format-line
check of today's file.

### Records and sequence ids

Cases go from rdra-ish to the harness:

```
T 17 init=0              case 17 starts: a fresh port, the seed of initial state 0
R Patient 1 status=registered ...
G                        store the seed, compare with the expected state
A Patient 1 status=registered ...   (the expected state after G, in full)
C Register 1 ...         a call the model runs
D + Appointment 101 status=booked   the expected state changes: lines added (+) and removed (-)
X NotEnabled Cancel 101  a call the model refuses, with its kind; the state stays
I Pay 1 | Cancel 1       interleaving: prepare the first, run the second, commit the first (refused)
D ...
E 17                     case 17 ends
```

- **Sequence ids.** The number on `T`/`E` is a wire sequence id. It counts up from 0 on
  each connection, and the harness cannot tell anything from it. Inside rdra-ish, each
  wire id maps to a stable **case number** (the case's place in the deterministic stream)
  or to a mutant. Case numbers therefore do not shift with `--mutants`,
  `--mutant-seed` or the shard count, and a mutant looks like any other case on the wire.
  Results name a wire id and the 0-based index of a record in the case. A result never
  needs a line number of a file.
- **Expected states as deltas.** `want` lives in the harness for one case only. `A` sets
  it in full after the seed. Each `D` after a `C` or an `I` changes it. A `C` with no `D`
  expects the state not to change. This drops the `S` table, which would otherwise have
  to be held by the harness for the whole run (millions of states) or sent in full each
  time (8 times the bytes of the delta on clinic-ops).

Results go from the harness to rdra-ish:

```
A 1024                   every case up to 1024 sent on this connection is done
F 17 5 kind=state        case 17 failed at record 5
| model only: Appointment 101 status=booked
| store only: Appointment 101 status=cancelled
U 18 3 Cancel            case 18 stopped at record 3: the call reached the unimplemented port
N 19 2 Cancel            case 19, record 2: a refused call reached the unimplemented port (no refusal, case goes on)
B 20 reached=42 calls=310 refusals=55 interleavings=2   end-of-run counts (with the final A)
```

The harness processes cases in order on one connection. A single acknowledgement
watermark (`A n`) is therefore enough, and rdra-ish needs no per-case reply. A failure
does not stop the harness: it finishes the case's records, skipping the rest of the
failed case, and goes on to the next `T`. rdra-ish decides when to stop sending (see
[Failure reporting](#failure-reporting)).

### Who compares

| | The harness compares (chosen) | rdra-ish compares |
|---|---|---|
| Bandwidth up | the expected delta, about 135 bytes per move | nothing |
| Bandwidth down | `A` every 256 cases, plus failures | every snapshot's abstract lines, about 1,054 bytes per move on clinic-ops |
| Where the oracle is | rdra-ish: it generates every expected state from the BFS | rdra-ish |
| Who could cheat | a harness that acknowledges without running. The mutants catch it, because mutant cases look exactly like real ones | a harness that sends back the expected lines. The mutants catch it too |
| What the comparator is | generated code, held to the model by `check --code=fresh` | rdra-ish |
| Failure report | the harness sends both sides of the difference | rdra-ish has both sides already |

The abstraction from `StoreSnapshot` to lines must run in the harness in both designs,
because only the service's language can read its snapshot type. Sending the expected
delta costs the uplink about 135 bytes per move. Sending snapshots back would cost the
downlink about 1,054 bytes per move and make rdra-ish's receive path the hot loop. The
trust is the same either way: what makes a harness honest is the self-check, not where
the comparison runs. The chosen design keeps the comparator that exists, and adds one
thing: the failing record carries both sides, so rdra-ish can report it without a file.

### Ordering the cases

The generator reads the store of the BFS once the BFS has completed or stopped.

1. **Expanded states.** The BFS reads chunks in the order it handed them out, and the
   result is the one a single thread produces (`bfs.rs`, `Workers`). State numbers are
   therefore the same on every run. The generator uses only the fully expanded prefix:
   every state below `sum(layers.new[..reading]) + layers.expanded`. Every move out of
   those states is known.
2. **Walks.** The walk is today's greedy walk with two restrictions. It steps only into
   expanded states, and it takes at most 4,096 steps before a new case starts. It starts
   at the lowest-numbered expanded state that has an untaken move. It follows the shortest
   call path to that state from the call-graph parents (the parents `traces()` computes
   from the calls alone, not `run.parent`, which can be a tick or a move with no function).
   It then takes untaken moves, preferring one into a state that has more. A state's taken
   moves are a bitset of its out-edges, charged to the pool, at 1 bit per move.
3. **Refusals.** `X` records for a state go into the first walk that arrives at it, as
   today. They are computed from that state's own moves, so they are sound for every
   expanded state, whether or not the BFS completed. Today's code computes them only when
   `run.complete` holds, so the first implementation step adds a test that the refusals
   of a capped run's prefix equal those of the complete run's same states.
4. **Interleavings and seeded cases.** These come after the walks. An interleaving
   needs both its state and the state after the second call expanded. The `(function,
   use case)` pairs already tried are a set of at most functions × use cases entries.
5. **Absent rows.** The `NotFound` calls go in case 0, as today.

Because the BFS is deterministic and the generator starts after it, the case stream is
deterministic for a model, its bounds and the rdra-ish version. Case `n` is the same on
every run. That is what makes sharding, resumption and replay possible without a file.

**Why not generate walks while the BFS runs.** The owner asked that the BFS and the
service work in parallel. A walk generated during the BFS can step only into states
expanded so far. Which states those are depends on how far the BFS got while the
generator waited on the service, so case `n` would differ from run to run. A
deterministic rule, such as "generate layer `d`'s walks once layer `d + 1` is expanded",
cuts every walk at the frontier. That is the dead-end graph the 65,536 cap measures:
0.59 walks per move and 4.2 prefix replays per move, against 0.31 and 1.1 on ec-site's
whole graph. Generating after the BFS costs its time once: 0.17 ms on ec-site and about
2 minutes on clinic-ops at the memory limit, against hours of service time. The service
still runs at the same time as the generator and across shards. A `--conformance-early`
mode could trade determinism for a start within seconds on very large models. This note
does not propose it.

### Backpressure and parallelism

After the BFS, two parts run as a pipeline:

- the generator thread, which writes records into a bounded channel;
- one writer per connection, which sends while the window allows it, and one reader per
  connection for the results.

Each connection has a window of cases that it has not acknowledged, by default 1,024
cases or 4 MiB, whichever comes first. TCP's own flow control bounds the socket buffers.
When every window is full, the generator blocks on the channel. The latency of one case
therefore never serialises the run: up to a window of cases is in flight on every
connection, and the service's speed sets the pace.

To report a failure, rdra-ish keeps per unacknowledged case only what regenerates it: its
start state and the edge index of each step, a few bytes per step. It does not keep the
text.

### Sharding

`--conformance-shards N` runs N connections. Each comes from a separate start of the run
command with `RDRA_ISH_CONFORMANCE_SHARD=k/N`, so `new` can pick a database schema or a
container per shard.

- **Inside one rdra-ish process.** Cases go to the connection with the most free window.
  That choice does not change any case's outcome: under H1 and H3 a case depends only on
  its own records, because every case starts with a fresh seed. rdra-ish logs which shard
  ran which case, so a failure names its shard.
- **Across machines.** `--conformance-shard k/N` makes this process send only the cases
  whose case number is `k` mod `N`. Each process runs the whole BFS, which is cheap next
  to the service. The partition is deterministic because the case numbers are. The union
  of the N shards is the whole stream, and a CI matrix can run them as separate jobs.

Partitioning by case number rather than by a hash of the frontier state keeps neighbouring
walks (which share prefixes) apart. Balance matters more than locality here, because a
case's cost is the service's, not the generator's.

### Completeness, memory and a time budget

- **No state cap.** `--conformance-states` and `conformance_states` go away. The BFS is
  bounded only by the `[check]` memory limit (`--memory`, default 4 GiB) and by
  `--conformance-budget DURATION` (default none).
- **What is charged.** The BFS store, the taken-move bitsets, the window bookkeeping and
  the tried-interleavings set are all charged to the pool. Today's `traces()` holds per
  edge a `Call` with its argument strings, plus `seen` and `moved` sets of heap vectors,
  outside the pool. The stream generator must not.
- **Incomplete is an error.** A run is incomplete when any of these happen:
  - the BFS stopped (at the memory limit or the budget);
  - the budget stopped the sending;
  - a refusal stayed untried (more than 64 bindings);
  - a use case of the port was never run;
  - a call reached the unimplemented port.

  `check --code=run` then exits 1 with `CodeCoverageIncomplete` or `CodeUnimplemented`.
  The run prints how far it got: states expanded, moves run, refusals tried, and the
  checkpoint to resume from. `--allow-incomplete` makes these warnings again.
  `--deny-incomplete` stays as an accepted no-op for one minor release.

  This is the owner's requirement, and it is right for ec-site-sized models. For
  clinic-ops it means `check --code=run` fails by default until
  [conformance per cone](#what-this-does-not-fix-clinic-ops) exists. That is the honest
  result: today's exit 0 on clinic-ops claims a suite that never runs 22 use cases. The
  sample's `rdra.toml`, if it gets a `[code]` table, should set
  `allow_incomplete = true` with a comment until then.

### Resuming

A checkpoint is a small file (`--conformance-checkpoint FILE`, written on stop and every
minute). It holds:

- the model hash, the bounds and the rdra-ish version;
- the harness hash and the shard spec;
- the low watermark: every case below it is done;
- the done cases above the watermark, at most one window per connection.

`--resume FILE` refuses a checkpoint whose hashes differ. Otherwise it runs the BFS again
(deterministic, and seconds next to the service) and skips the done cases. Coverage
counts the resumed cases as run, and the statement says `resumed from case <n>`.

### Failure reporting

On the first `F`, rdra-ish:

1. Stops handing out new cases, lets the in-flight ones finish (`--keep-going N` collects
   up to N failures instead), and closes the connections.
2. Rebuilds the failing case from its stored steps and prints it, from the seed to the
   failing record. The `CodeConformance` message prints the model's and the store's lines,
   as it does today.
3. Prints the **shortest path** from an initial state to the state before the failing
   record (from the BFS parents), and **runs it again as one case**. This splits two
   outcomes:
   - The short case fails too. The report is the short case, the smallest reproduction.
   - The short case passes. Then the service answers differently in the same model
     state reached by another history, which breaks H1. The report says so and names
     both paths.
4. Prints how to replay it without a file: `check --code=run --conformance-case 17`. The
   case number is stable for the same model, bounds and version.

`--dump FILE` writes every record sent, in order, and the results, as text. The probe's
test still accepts `RDRA_ISH_TRACES=<dump>` and runs a dump offline, which keeps a
debugger session simple. A dump is for debugging, not for `fresh`.

### The self-check in stream form

Today the self-check runs the test command again once per mutant. In the stream, mutants
are cases in the same stream:

- A mutant is a real case cut after one record, with that record changed as today
  (`refused-call-runs`, `call-refused`, `wrong-state`, `absent-row`, `guard-removed`).
  It gets the next wire id on its connection and no case number. Nothing on the wire marks it as a mutant.
- rdra-ish expects an `F` for it at the changed record. A mutant that gets an `A` past its
  id without an `F` there is `CodeMutantSurvived`. An `F` at another record counts as
  failed elsewhere, as today. A `U` before the changed record counts as left unchecked by
  the unimplemented port.
- Selection is deterministic in `--mutant-seed`. The candidates per kind come from the
  first 4,096 cases. A `guard-removed` mutant comes from the first case that rejects with
  its condition, wherever that case falls. Mutants go out right after their source case,
  so a harness that does not run the cases fails within the first window.
- `--mutants N` keeps its meaning. The default goes from one of each kind to four of each,
  because a mutant now costs one case, not a second start of the test process.

This is stronger than today on one point. A custom `run` command that ignores its input
cannot pass, because the stream needs a live connection that answers. And a harness that
acknowledges without comparing fails on the mutants it cannot tell apart.

### The unimplemented port

`U` stops a case, as a stopped trace does today. `N` counts a refused call that reached
the stand-in. Coverage counts both by use case, and either makes the run incomplete
(above).

### What changes in the generated harnesses

All three keep `ConformancePort`, `seed`, `snapshot`, the record parsers, `conf_compare`
and the unimplemented-port handling. Each gains a stream driver and loses the `S`-table
and file reading:

- **Rust.** `pub fn conform_stream<P: ConformancePort>(new_port: impl FnMut() -> P, addr:
  &str) -> Result<ConformanceReport, String>`, over `std::net::TcpStream` with a
  `BufReader` and a `BufWriter` that flushes on `A`. It stays one thread, with one port at
  a time, as today. A shard is a separate process, so the service never needs `Send`. The
  probe's test reads `RDRA_ISH_CONFORMANCE`, or else `RDRA_ISH_TRACES` (a dump), and calls
  `conform_stream` or `conform`. No new dependency.
- **TypeScript.** `export async function conformStream(newPort, addr)` over
  `node:net` with `node:readline`, the same way the file is read today, through
  `process.getBuiltinModule`. Writes use `socket.write`, and the driver awaits `drain`
  when the write returns false. Node 22.18 stays the floor.
- **Go.** `func ConformStream(newPort func() ConformancePort, addr string)
  (ConformanceReport, error)` over `net.Dial` and a `bufio.Reader` (not `Scanner`, whose
  default token limit is 64 KiB), with a `bufio.Writer`.
- **The model adapter.** It runs the same stream. Its second pass, which makes every call
  through `prepare` and `commit`, becomes a second connection with
  `RDRA_ISH_CONFORMANCE_PASS=prepared`, so its `ran:` line is unchanged. This is to be
  confirmed against `codegen/adapter/*.rs`, which this note did not read in detail.

The harness's `ConformanceReport` keeps its counts. The `B` record carries them back
instead of the test's printed `unimplemented:` line, so a `run` command that hides the
test's output no longer gives `unimplemented: not reported`.

### `codegen --check`, `check --code=fresh` and `rdra.toml`

- There is no traces file, so there is no hash to compare. `fresh` compares the domain
  code (which holds the harness) and the probe by text, as it does now.
- The handshake's harness hash catches a harness built from stale code at `run` time,
  even when `fresh` was skipped.
- `codegen --conformance-harness` (no value) prints the port and the harness into the
  code. `codegen --conformance-dump FILE` writes the stream as a dump, for offline use.
  `--conformance <FILE>`, which today does both, stays for one minor release as the
  deprecated spelling of the two together (see [Migration](#migration-and-compatibility)).
  `--conformance-states` goes away.
- `[code.<lang>]`:
  - `conformance = true` replaces `traces = "<file>"`.
  - `conformance_states` goes away.
  - The new keys are `conformance_shards`, `conformance_budget`, `conformance_connect`
    and `allow_incomplete`.

### Migration and compatibility

`codegen` and `check --code` are experimental ([What is not
covered](../compatibility.md#what-is-not-covered)), so this may land in a minor release.
To be kind to the projects that use it:

1. **First release.** The stream lands beside the file. `traces = ...` keeps working and
   prints a deprecation warning that names `conformance = true`. `--conformance <FILE>`
   prints a deprecation warning that names `--conformance-harness --conformance-dump
   <FILE>`, and does what they do. The dump uses the new grammar, which is today's grammar
   plus the wire ids, `D` and `E`, and drops
   the `S` table. The traces format line moves to version 6. A harness generated before
   then refuses a version-6 file with the message it already has ("generate the code and
   the traces again").
2. **Next minor release.** `traces`, `conformance_states` and `--conformance-states` fail
   to load, with a message that names the rewrite.

The record grammar keeps `T R G C X I`. The probe's test keeps reading `RDRA_ISH_TRACES`
for dumps, so a project's debugging script still works.

### What happens to H1 to H5

- **H1, the service answers from the tracked columns and the call alone.** The failure
  report now tests H1 on every failure: the shortest-path rerun separates a wrong answer
  from a history-dependent one. Seeded cases stay at 16.
- **H2, every reachable call is tried.** H2 is what the cap broke, and it is now either
  true or an error: the run fails unless every move and every refusal of every reachable
  state was sent. It gets stronger in one more way. Refusals of expanded states are tried
  even in an incomplete run.
- **H3, `seed` and `snapshot` reach the real store.** This assumption carries more
  weight. An uncapped run calls `seed` once per walk, which on a large model is millions
  of times, often on shards with their own schemas. A `seed` that clears only some tables,
  or a shard that shares a table with another, lets state leak between cases. The harness
  already compares the store after `G` with the full expected state (`A`). That check
  stays mandatory, and the statement says how many seeds it covered. A leak that changes
  untracked columns only is still invisible, as today.
- **H4, every write goes through the port, one call one transaction.** Unchanged.
- **H5, the rules read tracked columns only.** Unchanged.

The statement's `assumes:` line gains nothing for the default mode. Symmetry and cones
add one assumption each (below).

### Symmetry, opt-in

`--conformance-symmetry` would run the BFS up to instance symmetry
([instance-symmetry.md](./instance-symmetry.md)) and send one walk per orbit.

- **At check's default bounds it does nothing.** There is one slot per entity, so no
  permutation of slots exists. It only pays at `--instance-count 2` or more, which the
  file made impractical and the stream makes possible.
- **Extra assumption, H6s.** The service treats rows of one entity alike whatever their
  ids: no branch on an id's value, no order by id that the model does not have. The
  entity-banded ids of today's traces help to break H6s visibly. A service that reads
  another entity's id finds no row.
- **Reporting.** A failure in a symmetric case is reported on the canonical
  representative. Its replay is the representative, not the permuted original.

## What this does not fix: clinic-ops

The stream removes the cap and the file. It does not make the joint system of clinic-ops
small:

- **Memory.** The BFS alone passes 4 GiB at 59 million states, about 66 bytes a state,
  with the frontier still growing about 2.8 times per depth.
- **Service time.** The 21 million states that fit have about 4.2×10^8 moves and about
  4.4×10^8 refused calls (**estimate** from 21 per state), before walk prefixes. At an
  assumed 100 µs per call, which is a database round trip, 8.6×10^8 calls take about
  24 hours on one shard, and 3 hours on 8. The whole space is more than 10 times that.
  Sharding divides the time, but not by enough.

With the stream, clinic-ops at the default 4 GiB gets the following:

- the moves out of 21 million expanded states, against 21,494 today;
- every refusal of those states, against none today;
- every use case the first 14 depths reach;
- a `CodeCoverageIncomplete` error, unless `--allow-incomplete` is given.

That is much more coverage than today's suite, but it is still not the whole space. The
measuring function did not count how many of the 22 never-run use cases these depths
reach.

**Conformance per cone (follow-up, not part of the stream).** `check` passes clinic-ops
because it explores cones, not the joint system ([cones.md](../theory/cones.md)). A
conformance run per cone would walk each cone's lifecycle. It would seed the rows of
entities outside the cone at one fixed state that the model reaches, and compare only the
cone's rows. It needs one more assumption:

- **H6c.** A call's result and writes depend only on the rows of the cone it belongs to.

H6c is H1 narrowed to the rows a use case reads. It is what makes the model's cone
argument hold for the service. This is the change that would let `check --code=run` be
complete on clinic-ops at the default memory. It needs its own design note: the cone
argument is about rules, and a service can break locality in ways the model cannot. The
stream is the transport it would use.

## Rejected alternatives

- **One request and response per case.** This serialises the run on round-trip latency,
  about 50 µs on loopback and more in a container, per case.
- **JSON lines.** Rust's generated code has no dependencies, so it would need a hand-written
  JSON parser. JSON would also be about 2 times the bytes of the current text (an
  **estimate**, from keys and quotes on every value), and it adds
  nothing the grammar lacks.
- **Length-prefixed binary.** It is about 5 times smaller than text (estimate above), but
  each harness would need a new parser, and dumps would not be readable. It can be added
  later as a second encoding chosen in the handshake, if profiling ever shows that the
  wire matters.
- **Stdio as the default transport.** The test runners own it. See
  [Roles and transport](#roles-and-transport).
- **Per-edge seeded cases.** They need 1.7 times the seeds of walks even on the
  dead-end-heavy capped graph, they cannot express states with run marks or raises, and
  on clinic-ops they would send 307 MB of seed rows.
- **Keeping the file, compressed.** This keeps the cap, because the file must be held to
  hash it and the BFS must finish before the write. It fixes neither one-sidedness nor
  memory.

## Implementation plan

Each step ends in a state that CI checks. The estimates are working days for one agent.

1. **Generator without text (1.5 days).** `conformance::stream` yields records from a
   pool-charged BFS. It restricts walks to the fully expanded prefix, caps them at 4,096
   steps, and sends refusals per expanded state. `traces()` becomes a consumer that writes
   the dump. Two tests come with it:
   - on every complete fixture, the dump holds the same multiset of `C`, `X` and `I`
     records as today's file;
   - the refusals of a capped run's prefix equal those of the complete run's same states.

   The old writer stays as `reference.rs` under `shadow` for one release.
2. **Wire protocol in rdra-ish (2 days).** The listener, the handshake, the windows, the
   `A`/`F`/`U`/`N`/`B` parser, the checkpoint, and `--dump`. It is tested against an
   in-process fake harness that applies the model, plus one that cheats (acknowledges
   everything), which the mutants must catch.
3. **Rust harness (2 days).** `conform_stream`, the probe's test, the model adapter's
   second pass, and the `codegen_conformance` and `codegen_code_check` tests on ec-site
   and the fixtures. `check --code=run` switches for `conformance = true`.
4. **Mutants in stream (1 day).** In-stream mutants, `CodeMutantSurvived` from results,
   and the default of four per kind.
5. **Sharding, budget and resume (1.5 days).** `--conformance-shards`,
   `--conformance-shard k/N`, `--conformance-budget` and `--resume`, with a test that
   shows that the union of shards is the stream.
6. **TypeScript harness (1.5 days)**, then **Go harness (1.5 days)**. The same tests run
   under `codegen_code_build_ts` and `codegen_code_check_go`.
7. **Docs and deprecation (1 day).** `codegen.md`, `cli-reference.md`, the `[code]` keys
   and the deprecation warning, then the removal one minor release later.

The total is about 12 working days. Rust works end to end after step 4 (about 6.5 days).
Conformance per cone is not included. It needs its own note first.

## Open questions

- Should `allow_incomplete` default to `true` while `codegen` is experimental, so that
  clinic-ops-sized projects do not start failing on upgrade? This note follows the
  owner's requirement: error by default.
- Is a time budget enough, or does the owner want a move budget (`--conformance-moves N`),
  which is reproducible where a time budget is not?
- Should a failing case's shortest-path rerun also try the 16 seeded variants of its
  pre-state, so that H1 is tested with more than one history?
