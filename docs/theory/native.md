# The native verifier

<!-- constrained-by ./semantics.md -->
<!-- derived-from ./claims.md#c-the-native-verifier -->

This document states how rdra-ish's native verifier decides a check on a graph that its
breadth-first search (BFS) explored, and on what evidence each step rests. It owns rows N1
to N18 of the [claims inventory](./claims.md#c-the-native-verifier) and uses the notation
of [semantics.md](./semantics.md) (D1 to D23) without redefining it.

It describes the code at commit `fb6928c74ccc5b479dea07d92cc1e39c34ddd218` (origin/main,
2026-11-28). The other theory documents pin `b83b9544`; `git diff --stat b83b9544 fb6928c7`
over `crates/rdra-ish-core/src/lifecycle`, `crates/rdra-ish-core/src/{memory,shadow,fasthash}.rs`,
`crates/rdra-ish-cli/src/verify` and `crates/rdra-ish-emit/src/tla` is empty, so the
verifier is the same at both. Code is cited as in [semantics.md](./semantics.md): paths are
relative to `crates/rdra-ish-core/src/lifecycle/` unless they start with `cli/`
(`crates/rdra-ish-cli/`) or name the crate root (`core-root/`, for
`crates/rdra-ish-core/src/`).

Two reproductions in this document ran a release build of the CLI from a sibling worktree
on `64232981`, the parent of `fb6928c7` with the same `crates/`. That worktree's
`crates/rdra-ish-core/src` and `crates/rdra-ish-cli/src` are byte-identical to the pinned
commit's (`diff -rq`), its `crates/rdra-ish-emit/src` differs only under `codegen/`, and no
core or CLI source file there is newer than the binary. The models and commands are in
[Reproductions](#reproductions); each run takes under a second.

---

## Scope and terms

<!-- derived-from ../design/native-temporal-verification.md#one-graph-three-kinds-of-question -->

The native verifier is the BFS of `explore/` together with the readers that decide checks
on the graph it stores. It decides every check `verify`, `check` and `states` print except
`proved` and the TLC side of `--with-tlc`. Which system it explores (a cone, a reduced
cone, a quotient by slot permutations) is theme (a); this document takes the explored
system `T` as given and asks what the readers decide about `T`.

Terms used below:

- A **run** is one BFS of a system `T` (`ComponentRun`, built by
  `TransitionSystem::bfs_on` in `explore/mod.rs`). It stores a **graph** `G = (S_G, E_G)`:
  the admitted states numbered `0, 1, …` in admission order, and one **edge**
  `(from, to, binding)` per move read, self-loops included.
- `idx(s)` is the number of state `s`; **BFS order** is the order of `idx`.
- A run is **complete** when no cap stopped it (`ComponentRun::complete`), else
  **stopped**. Stops: the state cap, the memory limit, the time budget, the pattern cap
  (views only), and, for a safety cone run with `Caps::until_all_violated`, the point where
  every check it evaluates has failed.
- `expanded(G)` is `|S_G|` on a complete graph and, on a stopped one, the source of the last
  recorded edge (`ComponentRun::expanded`). States below it had every move recorded.
- A **shortest trace** to a state or step is a finite trace (D14) of least length among
  those that end there.

The questions the readers ask fall into four kinds, as in the design note: a reachable
state (`always`, state rules, `precedes`), a reachable step (`after`, `stable`,
`unchanged`), a reachable fair end (`eventually`, `leads_to`), and a backward closure
(`can_reach`, deadlocks).

## Explicit-state exploration

<!-- derived-from ../design/native-temporal-verification.md#one-graph-three-kinds-of-question -->

**N1 (BFS).** If a run of `T` is complete, then `S_G = Reach(T)`, every step `s → s'` of
`T` with `s ∈ Reach(T)` is an edge of `G` (one per binding and fork that produces it), and
for each non-initial state the parent edge (`ComponentRun::parent`) lies on a shortest trace to it.

*Algorithm.* `Bfs::go` admits the initial states (`TransitionSystem::initial_states`, D4)
in the order that function lists them. `Bfs::explore` then reads the states in admission
order; for each it computes the moves (`TransitionSystem::moves_into`, D6 to D8) and, in
move order, resolves each successor against the store (`Bfs::resolve`): a known state gets
its number, an unknown one is admitted with the current state and the move's binding as its
parent (`Bfs::admit_packed`). Every move becomes an edge (`Bfs::push`).

*Proof sketch.* Admission order is the queue order of textbook BFS, so the depth of the
admitted states is non-decreasing in `idx`, and the first edge that reaches a state comes
from a state of least depth among its predecessors; the parent chain therefore has the
state's depth as its length. Every reachable state is admitted because each admitted state
is expanded before a complete run ends and `moves_into` lists every successor. The one
step this sketch does not derive is that `moves_into` enumerates exactly the successors of
D13 (every admissible binding of D7, every fork of D8); that is evidenced differentially
(TLC on the same cone, `cli/tests/engine_equivalence.rs::a_bfs_and_tlc_reach_the_same_states_and_verdicts`)
and by the twin `Twin::EachEnabled` (`explore/moves/reference.rs`), which compares the
planned enumeration with a filter over all bindings.

## The state store and its encoding

<!-- derived-from ../design/native-bfs-parallel.md#the-shared-table -->

A state of `T` is a byte per cell (`expr.rs::State`), one cell per slot of each variable of
D2. The store keeps it packed: each cell in `ceil(log2 |Domain|)` bits, cells end to end,
the total rounded up to bytes (`explore/packing.rs::Packing::of`). States are found by
their packed bytes in a hash table (`explore/arena.rs::Seen`) under the FxHash-style
multiply-rotate hash of `core-root/fasthash.rs` with a fixed seed (`arena.rs::hash`).

**N2 (Store).** On states whose cells hold values inside their domains, packing is
injective and unpacking inverts it; the patch path (`Packing::patch`, which writes only the
cells a move changed into the source's packed row) equals packing the successor; the
arena, segment and table read back what they stored.

*Evidence.* Kani proves the round trip, the patch path, and the arena and table operations
on small widths (4 or 11 cells, widths up to 8 bits; the harnesses are listed in row N2).
The table lookup compares whole packed rows (`Seen::find_hashed`), so a hash collision costs
time and never merges two states; the hash's quality does not bear on correctness. The
seed is fixed and no table is iterated into the output (`fasthash.rs` states the rule), so
the hash does not bear on determinism either. `Packing::pack` panics when a value does not
fit its bits, in release builds too.

**Finding P50 (guarantee, reproduced).** N2 holds only while a packed state has at most
65,535 bits. `packing.rs::offsets` stores each cell's bit offset as a `u16` and saturates at
`u16::MAX`; `pack_into` and `unpack_into` count offsets as they go and are unaffected, but
`Packing::patch` (the path every plain run's successor takes, `Probes::read`) and
`Packing::cell` (read by deadlocks, `findings.rs::can_leave`) use the saturated offsets, so
every cell past bit 65,535 is written to and read from the same place. Distinct successors
then pack alike and are merged, and stored rows unpack to states `T` does not have. No
check guards the width. The Kani harnesses stop at 11 cells, far below the bound.

Reproduction (a model in which every row is created with `c0 … c33 = 1`, 34 Int columns,
at `--instance-count 255 --int-range-max 255`, a cone state of 255 × (34 × 8 + 2) = 69,870
bits):

- `forbidden(Row, Row.c33 == 0, …)` that reads every column: `verify` prints
  `counterexample (256 states)` whose trace shows `Row#1` created with `c32=0, c33=0`, a
  state the model cannot reach. At 8 instances the same model is `ok`.
- Two creating use cases that differ only in `c32` (1 against 2), with
  `forbidden(Row, Row.c32 == 2, …)` reading every column: at 8 instances `verify` prints
  `counterexample (511 states)`, the right verdict; at 255 instances it prints
  `ok (256 states)` and `result: ok`, in either declaration order of the two use cases.
  This is a wrong `ok`.

The bound is reachable only at large bounds: a cone state of more than 8,191 packed bytes,
about 8,192 cells of 8 bits (for example 255 slots of an entity with 32 or more Int
columns at `m = 255`). No sample comes near it, but D1 admits it and nothing rejects it.

## Memory accounting and the stop rule

<!-- derived-from ../formal-verification.md#memory-state-caps-and-threads -->
<!-- derived-from ../design/native-bfs-parallel.md#the-memory-pool -->

**N3 (Charge).** Before each admission `Bfs::admit_packed` computes the bytes the store
would hold with one more state (`Bfs::bytes_with_one_more`) from the lengths and capacities
of the run's own collections, and asks the run's `core-root/memory.rs::Charge` to cover
them; a refusal stops the run before the state is stored. The count is
`kept_with(1) + Seen::bytes_with(1)` (`explore/store.rs`): the states, the steps (parents
and edges), the orbit self-edges and permutations, the bindings' bytes, and the layers,
each from its length and capacity, plus the table. It is a function of the
sequence of admissions and edges alone (`explore/store.rs` models it as `store_bytes`), so
for a fixed sequence it is the same on every machine and thread count. Kani proves
`store_bytes` never overflows, only grows, and is at most the model before #296; the unit
test `store_bytes_models_the_store_a_run_counts` holds the model above what a run counts.
The charge covers the state store only: the passes over a finished graph (liveness,
`can_reach`, deadlocks, the partition) and the workers' scratch are outside it, as
formal-verification.md says.

**N15 (Stops give `not decided`).** Every reader below returns `Undecided` (or
`NotDecided`, or leaves the finding open) on a stopped graph unless it found a violation in
the part explored, and the CLI maps that to `not decided` (`cli/src/verify/end.rs::End::result`,
cited from row N15 and not re-read here).
A stopped run's counterexamples are sound for safety (they are paths of `G`, hence traces
of `T`) and for liveness by N10. The time budget (`Caps::deadline`, read every 256
admissions) is the one stop whose position depends on the machine.

## Parallel exploration

<!-- derived-from ../design/native-bfs-parallel.md#why-the-run-is-the-one-thread-run -->
<!-- derived-from ../design/native-bfs-parallel.md#holding-it-correct -->

`Bfs::explore` hands chunks of 128 consecutive unread states (`CHUNK`) to worker threads,
which expand them (`bfs.rs::expand`): compute moves, canonical forms under symmetry, packed
successors and their hashes, a `Probe` per move, and a dense binding number. The caller,
the **reader**, takes the expanded chunks back in hand-out order (`Workers::next`) and does
everything that changes the run: numbering, admission, edges, bindings, violations,
charges, and stops (`Bfs::take`, `Bfs::resolve`).

**N4 (Same run on any thread count).** Let `R_k` be the run of `T` with `k` workers and
`R_0` the run with none (`Threads::Fixed(0)`). If no time budget stops either and each runs
alone in its memory pool, `R_k` and `R_0` have the same states, numbering, parents, edges,
bindings, diagnostics and stop point.

*Proof.* Induction on the moves in reading order `m_1, m_2, …` (by state, then by move
within the state). Let `Σ_j` be the reader's state after `m_j`. Claim: `Σ_j` is the same in
`R_k` and `R_0` for every `j`.

1. *The moves of a state, and their order, are a function of the state.* On an unreduced
   system `moves_into` goes through `each_raw_move`, which iterates the actions in index
   order and, per action, the bindings `each_enabled` yields and the successors
   `each_successor` yields; it reads the state, the system and its own scratch only. On a
   reduced system the moves come from `moves_of_into` in `reduce/`, which was not read for
   this step: that it too reads nothing but the state and the system is the missing part.
   Canonicalization (`Canon`) and packing carry no state between calls. A worker and the reader call the same functions,
   so the move list read at `m_j` is the same, and so is each move's `Probe::Loop` or
   `Probe::OrbitSelf` flag, packed successor, hash and binding number. Chunk boundaries
   differ between `R_k` and `R_0` (the reader expands what no worker holds), and nothing
   below depends on them.
2. *The table only grows and never renumbers.* `Bfs` never removes a state; `Seen` inserts
   each state once under the number it was admitted with.
3. *Each resolution returns what `R_0`'s lookup returns.* For a `Probe::Miss` the reader
   looks the successor up itself, in its own table, which by the hypothesis is `R_0`'s at
   `m_j`. For `Probe::SameAs(i)` the successor equals that of an earlier move `i` of the
   same chunk, which was a `Miss` (only misses enter the chunk's dedup map) and which the
   reader resolved at some `m_{j'}`, `j' < j`; the state found or admitted there is in the
   table at `m_j` with that number, so `R_0`'s lookup returns it. A `Probe::Hit(i)` occurs
   only when workers probe the shared table (`Probing::On` or `Forced`), never in a release
   build (`Probing::of_build`); a hit names a row the reader published before the chunk was
   handed out and compares the bytes, so by (2) it is `R_0`'s answer as well. A `Miss`
   under probing is then looked up only among the states admitted after publication
   (`Recent`), which is correct exactly when the shared table's lookup has no false
   negatives; that property is the concurrency argument of native-bfs-parallel.md and is
   checked only by the twins named below.
4. *Every other update reads only `Σ`.* Binding ids are assigned in first-seen order on the
   reader (`Bindings::id`, `id_numbered`); violations keep the first per name
   (`Bfs::violation`); the stops are evaluated on the reader at the admission that causes
   them, and the charge requested is a function of `Σ` (N3).
5. *Work past a stop is discarded.* Chunks expanded after the stop are dropped unread.

Hence `Σ_j` agrees for all `j`, and with it everything the readers below compute. ∎

*Steps that rest on something other than this proof.*

- *The pool.* "Alone" matters: `Charge::cover` succeeds or fails against what the other
  runs of the command hold (`MemoryPool::charge`), and with `RDRA_JOBS` cones run side by
  side. Two mechanisms restore the one-job result. In the entity pass,
  `targets.rs::TransitionSystem::run_cones` defines the result as that of the cones run one
  at a time in cone order, each with the room the earlier results leave: a run that was
  crowded (`memory::Watched::crowded`, a refusal within the pool's room) or whose peak
  exceeds that room runs again under `pool.with_room(room)`, an explicit room computed from
  the results before it. The quotients are reduced one at a time in target order
  (`cli/src/verify/jobs.rs`), so what the quotient cache holds is the same on any job count.
  native-bfs-parallel.md, "The memory pool", argues the same for #260's rerun. The CLI's
  safety and property passes (`cli/src/verify/bfs.rs::bfs_cones`, `Native::crowded` in
  `cli/src/verify/native.rs`) rerun a crowded run against the command's pool itself, not an
  explicit room. That the pool holds the same bytes at that rerun whatever the job count is
  the one step not shown (open problem P53). Evidence:
  `cli/tests/memory_cli.rs::cones_side_by_side_stop_where_they_stop_one_at_a_time` and
  `::a_stopped_entity_cone_leaves_its_room_on_any_number_of_threads`.
- *The table under probing* (step 3) holds only on the probe paths, which release builds
  do not take.

*Evidence by twin (P18, corrected).*

| Twin | Fast path | Taken in a release build | Twin runs |
|---|---|---|---|
| `Twin::SameAs` | `Probe::SameAs` (chunk dedup) | yes | every `shadow` run |
| `Twin::DenseBinding` | `Bindings::id_numbered` | yes | every `shadow` run |
| `Twin::Seen` | `Seen` lookups | yes | every `shadow` run |
| `Twin::Hit` | `Probe::Hit` | no (`Probing::Off`) | `shadow` runs under `RDRA_ISH_FORCE_PROBES` (one CI run, clinic-ops) |
| `Twin::Recent` | the reader's `Recent` lookup | no | same |
| `Twin::SeenGrowth` | the shared table's growth (`table::Writer`) | no | same |

The unit test `explore/tests.rs::a_run_is_the_same_on_any_number_of_threads` compares 1, 2,
3, 4, 7 and 17 workers with `R_0`, on a complete and a capped run, with probing on at 4
and 17 workers (`probes_at`), not under `shadow`. The loom model, the test-only jitter and a
Miri CI job named in native-bfs-parallel.md, "Holding it correct", do not exist; the table
and arena tests are Miri-aware (`cfg(miri)` in `explore/table.rs`, `explore/arena.rs`) but
no workflow runs Miri.

**N17 (Output independent of `RDRA_JOBS`).** Follows from N4 for each run, from the
in-order collection of `in_parallel` results, and, for runs that met the memory limit, from
the reruns above, up to P53's missing step. Evidence: as N4.

## Safety

<!-- derived-from ../design/native-temporal-verification.md#one-graph-three-kinds-of-question -->

**N5 (State rules and `always`).** On a complete `G`, a state rule `R` (D16) is violated
iff some `s ∈ S_G` violates it, and `always(x)` (D19) is violated iff some `s ∈ S_G` and
binding `b` have `trig_b(s) ∧ ¬x_b(s)`. The reported trace is a shortest trace to a
violating state.

*Code.* State rules are evaluated at admission (`Bfs::check_invariants`, before the state
is stored), on the unpacked successor; the first violation per rule name is kept, and its
trace is the parent path (`ComponentRun::trace_of`). `always` is evaluated after the run by
`properties.rs::ComponentRun::unsafe_state`: the first state in BFS order where some binding
fails, and its parent path.

*Proof.* Iff: by N1, `S_G = Reach(T)`. Shortest: admission order is BFS order, so the first
violating state admitted has least depth, and its parent path has that length (N1). For
`always`, `ComponentRun::decide_property` re-evaluates the path's states with
`TransitionSystem::property_failure` and `debug_assert`s that the failure is at the last
state; that holds because every earlier state of the path has a smaller `idx` and so
breaks no binding. ∎ On a stopped graph a violation found is a path of `G`, hence a trace
of `T`; no violation gives `Undecided`.

**`after` assertions (D17).** `Bfs::check_after` evaluates each `after(UC)` on every move of
the last step of `UC` read from an expanded state, on the move's concrete successor, before
the successor is resolved. The trace is the parent path to the move's source and the move
itself (`Reach::Step`); the first violating move in reading order has a source of least
depth, so the trace is shortest.

**Finding P51 (prose, reproduced).** `Bfs::take` skips `check_after` on a move whose
successor equals its source (`Probe::Loop`), so an `after` assertion is never evaluated on a
step with `s' = s`. D17 quantifies over every step of the last step of `UC`, and D13 says a
step with `s' = s` is possible. TLC agrees with the code, not with D17: the TLA+ printer
emits `[][… => …]_vars` (`crates/rdra-ish-emit/src/tla/module.rs`), whose subscript exempts stuttering steps.
Reproduction: `Touch` sets `Order.status := open` on a row created `open`, and
`after(Touch).assert(Order.flag == true)` with `flag` false and never written. Every `Touch`
step is a self-loop; `verify` prints `ok (3 states)`. The control, `Touch` setting `closed`,
prints `counterexample (7 states)`, so the action is in the cone and the self-loop is what
hides the step. The engines agree; what differs is D17's wording (or the modeler's
expectation that an assertion after a use case holds whenever the use case can run).

### Step forms and order forms

<!-- derived-from ../design/native-temporal-verification.md#one-graph-three-kinds-of-question -->

**N6 (`stable`, `unchanged`).** On a complete `G` the property is violated iff some edge
`(s, s')` with `s ≠ s'` and some binding `b` have `step_breaks(p, b, s, s')`
(`lasso.rs::TransitionSystem::step_breaks`, the condition of D19). The reported trace is a
shortest trace ending with a breaking step.

*Proof.* Iff: by N1 every step from a reachable state is an edge; steps with `s' = s` break
neither form by D19, and the code skips them (`breaking_step` requires `e.from != e.to`).
Shortest: `breaking_step` takes the first breaking edge in edge order; edges are stored in
the order of their sources' `idx`, so its source has least depth among the sources of
breaking edges, and the trace (parent path plus the edge) has depth + 1 steps. ∎

**N7 (`precedes(p, q)`).** On a complete `G` the property is violated iff for some binding
`b` there is a path of `G` from an initial state, through states where `trig_b ∧ p_b` is
false, to a state where `trig_b ∧ q_b` holds (and, being on the path, `trig_b ∧ p_b` is
false there too).

*Proof.* This restates D19 on `G` by N1. `ComponentRun::unlatched_state` runs a
multi-source BFS per binding from the initial states where `trig ∧ p` fails, through such
states, and stops at the first state where `trig ∧ q` holds; the shortest path over all
bindings is reported (the first binding on a tie). A BFS on the subgraph finds a shortest
path in it, and a violation trace must lie in it, so the trace is shortest. ∎ The
`debug_assert` in `decide_property` holds because a binding that failed earlier on the
chosen path would have a shorter path of its own.

## Recoverability

<!-- derived-from ../formal-verification.md#recoverability-on-a-cone -->

**N8 (`can_reach(g)`).** On a complete `G`, `can_reach(g)` (D22) fails for binding `b` iff
some `s ∈ S_G` with `trig_b(s)` is not in `Can_b`, the least set containing the states
where `pres_b ∧ g_b` holds and closed under predecessors that satisfy `pres_b`.

*Proof.* `Can_b` is the least fixpoint of `X = (pres ∧ g) ∪ (pres ∩ pre(X))`, the CTL
`E[pres U (pres ∧ g)]`, and on `G = Reach(T)` with every step an edge (N1) it is exactly the
set of D22. `reach.rs::TransitionSystem::reach_failure` marks the goal states and closes
backwards through `Adjacency::mark_backward` with `within = pres`; self-loops are dropped
from the adjacency, which does not change reachability. The first trigger state not marked,
in BFS order over all bindings, ends the reported trace, which is therefore shortest. ∎

A stopped graph must not be read: an unexpanded state has no out-edges and would look
stuck. The guard is the match arm `Target::Reach(i) if !cones[at].run.complete` in
`targets.rs::TransitionSystem::answer_targets`, which records the property as
undecided; `reach_failure` and `TransitionSystem::verdict` only `debug_assert!` completeness.
This confirms open problem P19. TLC cannot state `can_reach`, so the evidence is unit tests
alone (row N8).

## Deadlocks

<!-- derived-from ../state-derivation.md#deadlocks -->

**N14 (Deadlock).** On a complete entity-cone graph, value `v` of a lifecycle column `c` of
`E` (not `@end`, with some exit arm: `findings.rs::exits`, `has_way_on`) is reported stuck
iff some `s ∈ S_G` and slot `k` have `E[k]` present, `c[k] = v`, and no path from `s`
through states where `E[k]` is present reaches an edge that changes `c[k]` or makes `E[k]`
non-present. The state reported is the least such `s` in BFS order (over all slots).

*Proof.* `findings.rs::TransitionSystem::can_leave` marks the sources of leaving edges
(source present, and target not present or with another value) and closes backwards
through present predecessors; this is `E[present U leave]` per slot, as in N8. A state not
marked has no such path, which is the definition. The reported state is the minimum over
`(idx, k)`. ∎ On a stopped graph every candidate value is `Progress::NotDecided`. Both
`can_leave` and the trace read packed cells through `Packing::cell`, so P50 applies.
TLC's deadlock check is one-sided (row T6); this check has no external oracle (P19).

## Vacuity

**N18 (Vacuity).** `targets.rs::TransitionSystem::vacuity` reports a state rule or `after`
assertion as vacuous only on a complete run of its cone, only when the run reports no
violation of it, and only when no stored state meets one of its premises
(`unmet_of_rule`, `unmet_of_assert`). It is a warning and changes no verdict (D18). On a
complete plain run "no stored state" is "no reachable state" by N1. On a reduced run or a
run up to symmetry the stored states are representatives; that a premise met by some
member of a class is met by its representative is theme (a). The `after` premise is read
from the step's ends (`ComponentRun::step_ends`), which count self-loop moves as runs of the use
case, consistent with P51.

Properties have a check of the same kind, `targets.rs::TransitionSystem::property_vacuity`,
on a complete run of the property's cone: it warns when no stored state, under some binding
of the binders to present rows, meets the `.along` link together with the form's own
condition (the trigger of a `leads_to`, what a `stable` keeps, the `.when` of an
`unchanged`, what a `precedes` asks for first). It is read off the same run that decides
the property and changes no verdict.

## Liveness

<!-- derived-from ../design/native-temporal-verification.md#liveness-by-fair-components -->

### The fairness model

The fairness conditions are those of D20: `Fair_P`, weak fairness per fair instance
`(a, κ)`, where `κ` is the slots of `a`'s non-`New` parameters
(`types.rs::Property::fair_instance`), and `WF_vars(Next)`. A run decides a liveness form
under `Plain` (`Fair_P` alone) or `WfNext` (`Fair_P ∧ WF_vars(Next)`)
(`lasso.rs::Fairness`). There is no strong fairness anywhere in the language, the
printer, or the checker. On `G` define, for a state `s`:

- `En(s)`: the fair instances that label an edge from `s` to a state `≠ s`
  (`liveness.rs::FairSteps::enabled`); `Moves(s)`: some edge leaves `s` to a state `≠ s`
  (`FairSteps::moves_on`).
- For a set `C` of states, `Tk(C)`: the fair instances that label an edge `(u, v)` with
  `u ≠ v` and `u, v ∈ C`.

By N1 and the definition of enabledness in D20 (some binding extending `κ` has a successor
`≠ s`), `En(s)` on a complete `G` is the set of fair instances enabled at `s` in `T`, and a
step `s → s'` with `s ≠ s'` takes every fair instance that labels an edge from `s` to `s'`.
A step that changes nothing neither enables nor takes, as in TLA+'s `⟨A⟩_vars`.

### The check

Fix an `eventually(x)` or `leads_to(x, y)` with `cause`, `goal` as in D21, a binding `b`,
and a fairness `F`. Let `N_b = {s ∈ S_G : ¬goal_b(s)}` and `E_N` the edges of `G` with both
ends in `N_b` and different ends. A **stop** is `s ∈ N_b` with `En(s) = ∅` (`Plain`) or
`¬Moves(s)` (`WfNext`). A strongly connected component `C` of `(N_b, E_N)` is **fair** when
`|C| ≥ 2` and every fair instance enabled at every state of `C` is in `Tk(C)`. The **ends**
are the stops and the states of fair components.

**N9 (Liveness by fair components).** On a complete `G`, the property fails for `b` under
`F` iff some `s ∈ S_G` with `trig_b(s) ∧ cause_b(s) ∧ ¬goal_b(s)` reaches an end by a path
of `E_N` (of length zero included).

*Proof.* (⇐) Let `s` reach an end `e` inside `N_b`. Take the parent path to `s` (N1), then
the path to `e`. If `e` is a stop, stutter at `e` forever: under `Plain` no fair instance is
enabled at `e`, so `Fair_P` holds; under `WfNext` no step to another state exists either,
so `WF_vars(Next)` holds. If `e` is in a fair component `C`, follow a closed walk of `E_N`
inside `C` that visits every state of `C` and, for each fair instance enabled at every state
of `C`, an edge of `C` labelled by it (it exists by fairness of `C` and strong connectivity),
and repeat it forever. An instance not enabled everywhere in `C` is disabled infinitely
often, so weak fairness does not constrain it; the others are taken infinitely often. The
walk changes state infinitely often (`|C| ≥ 2`), so `WF_vars(Next)` holds. The behavior
reaches `s`, where the trigger and cause hold, and from there stays in `N_b`, so the goal
never holds at or after `s`; it violates D21.

(⇒) Let `σ` be a behavior satisfying `F` and `i` a position with `trig_b ∧ cause_b` at
`σ_i` and no goal at any `j ≥ i`. Then `σ_i ∈ S_G` (it is reachable) and every `σ_j`,
`j ≥ i`, is in `N_b`. Two cases, since `S_G` is finite. (a) From some point `σ` stutters at
a state `e` forever. Under `Plain`, a fair instance enabled at `e` would be enabled forever
and never taken (stuttering takes nothing), against `Fair_P`, so `En(e) = ∅`; under
`WfNext`, a step from `e` to another state would violate `WF_vars(Next)`, so `¬Moves(e)`.
So `e` is a stop. (b) `σ` changes state infinitely often. Let `I` be the states visited
infinitely often and the non-stuttering steps taken infinitely often; `I` is strongly
connected within `(N_b, E_N)`, has at least two states, and lies in one component `C`. A
fair instance enabled at every state of `C` is enabled at every state of `σ` from some
point on, so `Fair_P` makes `σ` take it infinitely often, on a step inside `I ⊆ C`; so it is
in `Tk(C)`, and `C` is fair. In both cases `σ_i` reaches the end by the non-stuttering steps
of `σ`, which are edges of `E_N`. ∎

*Code.* `liveness.rs::ComponentRun::liveness_search` runs `ComponentRun::avoid` per binding:
the components by iterative Tarjan (`graph.rs::Adjacency::components`), the `always` and
`taken` bit sets per component, the stops, and a backward BFS from the ends over the
predecessors in `E_N` (`toward`). The binding whose failing trigger state comes first in BFS
order is chosen. For weak fairness a single pass over the maximal components suffices,
because enlarging a cycle can only add states where an instance is disabled and edges where
it is taken; strong fairness would need the recursive decomposition of Lichtenstein–Pnueli
and Emerson–Lei, which is why the design note keeps it out of scope.

**N10 (Stopped graph).** On a stopped `G`, a state at or after `expanded(G)` is never a
stop, and a component that holds one is never fair (`avoid`'s `open` and the `s < expanded`
test). Then every end found has its out-edges fully recorded, so `En`, `Moves` and `Tk` are
exact there, and the (⇐) direction of N9 goes through unchanged on the recorded edges: a
counterexample found is one of `T`. The (⇒) direction does not, so no counterexample gives
`Undecided`. The cut at `expanded(G)` is conservative: a stop can land at the first move of
a state, after states with no moves, and those are treated as unexpanded too.

**N11 (Self-certification).** Before a native lasso prints, `decide_liveness` replays it
through the step relation (`lasso.rs::TransitionSystem::replay_lasso`: the first state must
be initial, each step and the back edge must be a move of the named action), re-evaluates
the formula on the infinite behavior it stands for (`explain_property`, `fails_at`), and
checks fairness from the moves at the lasso's states rather than from the search's data
(`unfair_loop`: the instances enabled at every loop state minus those some loop step takes
must be empty, and under `WfNext` a stutter must have no move). A failure is
`LassoFault`, printed as a tool error, never as a finding. This is runtime evidence (R): a
wrong counterexample cannot print, a wrong `ok` can. The fairness check is independent of
`avoid` in code, but both read the same `moves` and the same `fair_instance`, so a fault in
those would pass both.

**The lasso.** `ComponentRun::walk` builds the prefix (parent path), the bridge (the
`toward` pointers), and for a component a loop (`fair_loop`) that repeatedly walks inside
the component to the nearest state where an uncovered instance is disabled or the nearest
edge that takes one, and then back to the end it started from. The loop is in `N_b` and
covers each fair instance, so it is the lasso of the (⇐) proof.

**Where `ok` comes from.** `Answer::of` maps `PropertyVerdict` to the line;
`cli/src/verify/native.rs::native_property_after` orders the runs (`Plain` on the reduced
cone, then `WfNext` on the reduced graph with looping classes, then the unreduced cone, then
`WfNext` there). The two implications of D21 (an `ok` under `Plain` is one under `WfNext`;
a counterexample under `WfNext` is one under `Plain`) and the cone's own fairness are theme
(a).

### The goal on absent rows

`fails_at` and `avoid` evaluate the goal on a slot whatever its existence, so a behavior
that deletes the row meets `eventually(E.c == v)` when `v` is `c`'s base value, while
`can_reach` keeps the slot present along the path. Read on the code here, this confirms
open problem P6; TLC reads the printed formula the same way.

## Counterexamples: what is checked before printing

<!-- derived-from ../design/native-temporal-verification.md#counterexamples -->
<!-- derived-from ../formal-verification.md#what-verify-checks -->

**N16** claims that every printed counterexample replays. The table records, per check and
per kind of run, what the code does to a trace before it prints. *Replayed* means matched
step by step against `moves` from an initial state. *Re-evaluated* means the check is
evaluated again on the trace's last state or step. *Plain* is an unreduced run without
symmetry; *reduced* a run of a cone reduced by its independent sub-lifecycle (#74), whose
trace is lifted by `reduce/lift.rs::TransitionSystem::lift`; *symmetric* a run up to slot
permutations, lifted by `explore/orbit.rs::lift`.

| Check | Plain | Reduced | Symmetric |
|---|---|---|---|
| State rule, `after` | nothing: the trace is the stored path, printed as stored (`trace_violations`) | lifted; re-evaluated on the lifted last state, and if that no longer violates, the reduced message is kept without any fault (`ComponentRun::trace_violations`, `rebuilt` is `None`); a T step is checked against the cone's moves only by `debug_assert!`, an S step of a folded action is checked (`expect`), one taken by `in_cone_params` is not | replayed on the system the run explored (`orbit.rs::replays`, panics on failure), then lifted by `lift` when that system is reduced, without a further replay; re-evaluated on the lifted last state (`expect`, panics) |
| `always` | re-evaluated on the stored path (`property_failure`, then `debug_assert!` on the position) | re-evaluated on the reduced path, then lifted with no re-evaluation (`decide_property`, `trace_along`) | lifted, replayed (panics on failure) and re-evaluated on the lifted path (`OrbitRun::decide_always`, `expect`) |
| `stable`, `unchanged`, `precedes` | as `always`, plain | never reduced (`decide_property` restricts without reduction) | not run up to symmetry |
| `eventually`, `leads_to` | replayed, formula re-evaluated, fairness re-checked (N11) | the reduced lasso is replayed on the reduced system and never printed; what prints is either the `WfNext` lasso lifted to the unreduced cone and replayed there (`decide_liveness_reduced`, `Spurious` otherwise), or the lasso of the rerun on the unreduced cone (`Cone_P_Full`); if that rerun stops, the property is `not decided` | replayed, re-evaluated and re-checked after lifting (`orbit/liveness.rs`, `LassoFault` on failure, `LassoFault::Unclosed` past 10,000 copies) |
| `can_reach` | nothing: the parent path to the stuck state | never reduced | not run up to symmetry |
| Deadlock | nothing beyond the stored path | entity cones are not reduced by #74 | lifted and replayed (`orbit/findings.rs`, panics on failure) |

Two consequences.

- *Unchecked traces are where P50 becomes visible.* A plain safety trace is never replayed,
  so the corrupted states of the P50 reproduction printed as a counterexample with no
  `LassoFault`. A replay of every safety trace would have turned that wrong verdict into a
  tool error, though not the wrong `ok`.
- *P20, restated.* Liveness lassos that print are replayed on the system the verdict is
  about in every case; the gaps are on the safety side: plain traces are not replayed at
  all, reduced traces are lifted with `debug_assert!` checks and can keep a reduced message
  silently, and symmetric traces are replayed but fail by panic rather than `LassoFault`.
  A reduced liveness counterexample whose unreduced rerun stops becomes `not decided`
  rather than printing the lifted lasso.

### The lift's mechanics

The soundness of each reduction is theme (a) (`cones.md`); this section states
only what the lift computes.

- **Reduced, finite path** (`lift`). The S part of the start becomes an initial S state of
  its class; each T step is kept and the S rises it causes are inserted; each class move
  becomes a shortest S path (`Succ::shortest_path`) from the current concrete S state into
  the target class. The S path is lifted again under an inner reduction, and, when S was
  explored up to its group `H`, first lifted to concrete S states (`OrbitRun::lift_path`).
- **Reduced, loop** (`lift_cycle`). The prefix and `c` copies of the loop are lifted for
  `c = 4, 8, …, 4,096` (`MAX_COPIES`), until the concrete state at the end of one copy
  repeats at the end of an earlier one; the lasso closes there. The lift of a copy is a
  function of the concrete state it starts from, so the copies close within as many copies
  as there are concrete states in the loop's start class; past 4,096 it gives
  `Spurious::Unclosed` and the CLI falls back to the unreduced cone.
- **Reduced, stop** (`lift_stop`). From the lifted last state, a breadth-first walk of at
  most 2^18 states (`MAX_WALK`) through S steps that keep T and `obs` finds a state with no
  move or a cycle; none gives `Spurious::NoLoop`.
- **Symmetric** (`orbit.rs::lift_path`). Each stored edge is recomputed from its source as
  the first move of its action and parameters whose successor `t` canonicalizes to the
  stored target; with `σ` the permutation that takes `t` there, the concrete step is the
  move read through the accumulated `τ`, and `τ := τ ∘ σ⁻¹`. A liveness loop on
  (state, binding) pairs (**N12**) closes after at most the order of the loop's composed
  permutation, capped at 10,000 copies (`orbit/liveness.rs::MOST_COPIES`). **N13**: the lifted
  trace replays or the process panics.

The `Twin::OrbitLiveness` twin compares the orbit search with the plain run's `avoid`
(`ComponentRun::liveness_reference`), so it tests the quotient against N9's implementation,
not N9 itself; it skips runs past 200,000 states (`orbit/reference.rs::SHADOW_CAP`), as do
`Twin::OrbitRun` and `Twin::OrbitStateModels`. This confirms the twin parts of P9 and P17.

## Features that add no case to the verifier

<!-- derived-from ../design/counted-events.md#the-transition-system -->
<!-- derived-from ../design/counting-rules.md#native-checker -->
<!-- derived-from ../design/event-order.md#checks-and-findings -->

Raised marks, counted triggers, and counted quantifiers reach the verifier as ordinary
variables, guards and effects, so every statement above applies to them without a new
case. A raised latch or counter is a `VarKind::Raised` variable whose raise and handling
are `Effect::Transition` arms, with the overflow rule an ordinary `Invariant`
(counted-events.md, "The transition system"). A counted quantifier is evaluated by `eval`
over every slot of its entity (counting-rules.md, "Native checker"). Deadlocks read view
columns only, so a mark is never a stuck value; `.fair(UC)` on a triggered use case makes
each of its variants a fair action whose enabledness reads its mark (event-order.md,
"Checks and findings").

## Related work

- **Explicit-state model checking.** The store, BFS and per-state checks follow SPIN
  (Holzmann, "The model checker SPIN", IEEE TSE 23(5), 1997) and TLC (Yu, Manolios,
  Lamport, "Model checking TLA+ specifications", CHARME 1999). Like TLC and unlike SPIN's
  default, exploration is breadth-first, which gives shortest safety traces; unlike TLC, which stores 64-bit fingerprints and accepts a small probability of merging
  two states, and unlike SPIN's bitstate mode, rdra-ish stores each state exactly and
  compares full packed rows, so a hash collision costs time, not soundness (P50 is a fault
  of the encoding, not of hashing).
- **Fair cycle detection.** Deciding a liveness property under weak fairness by strongly
  connected components of the graph restricted to the goal's complement goes back to
  Lichtenstein and Pnueli ("Checking that finite state concurrent programs satisfy their
  linear specification", POPL 1985) and Emerson and Lei ("Modalities for model checking:
  branching time logic strikes back", Science of Computer Programming 8, 1987), whose fair-
  SCC computation for Fair CTL is the shape of N9 (fair components, then a backward
  closure). On-the-fly alternatives are nested depth-first search (Courcoubetis, Vardi,
  Wolper, Yannakakis, "Memory-efficient algorithms for the verification of temporal
  properties", FMSD 1, 1992) and SCC-based emptiness checks (Couvreur, "On-the-fly
  verification of linear temporal logic", FM 1999). rdra-ish is not on-the-fly: it decides
  liveness after the graph is built, which needs the whole graph in memory but lets the
  same graph answer every property of the cone and gives deterministic lassos.
- **Parallel BFS model checking.** Barnat, Brim and Chaloupka ("Parallel breadth-first
  search LTL model-checking", ASE 2003) parallelize level-synchronized BFS across machines,
  partitioning states by owner. rdra-ish parallelizes only expansion: one reader commits
  every state in sequential order, which gives up scaling of the commit step in exchange
  for the exact one-thread run (N4).
- **TLA+ fairness.** `WF_vars(A)` and stuttering steps are Lamport's ("The temporal logic of
  actions", ACM TOPLAS 16(3), 1994; *Specifying Systems*, 2002). D20's per-instance weak
  fairness is the conjunction of `WF_vars` over instances that the TLA+ printer emits; the
  exemption of stuttering steps from action properties (`[][A]_vars`) is the source of
  P51.

## Threats to validity

- **The step relation is trusted.** Every proof above assumes `moves_into` enumerates the
  steps of D13 (N1). The evidence is differential (TLC, which reads the same
  `TransitionSystem`, so a fault in the build of `T` would pass both) and one twin.
- **Bounded mechanical evidence.** The Kani harnesses cover widths far below where P50
  begins; the shadow twins skip runs above 200,000 or 2,500,000 states (P17) and the probe
  twins run in one CI job (P18).
- **Readers share code with the checks they certify.** `unfair_loop` and `avoid` both use
  `moves` and `Property::fair_instance`; `explain_property` and `liveness_search` both use
  `holds_bound` and `triggers`. Self-certification catches a fault in the search, not in
  these.
- **Wide states.** P50 is a wrong `ok` reachable within the documented bounds.
- **Determinism of memory stops beside other runs** rests on the reruns of N4; one step is
  not shown for the CLI's passes (P53).
- **Reproductions** used a binary built in another worktree (see the first paragraph); the
  issues that take P50 and P51 up should reproduce them on a clean build.
- **Not re-read here:** `cli/src/verify/end.rs` (N15), `reduce/`'s `moves_of_into` (N4,
  step 1), and the orbit readers' internals beyond their certification steps.
- **P10 confirmed.** `cli/src/verify/bfs.rs::BfsCone::verdict` returns `counterexample`
  whenever the run found a violation, stopped or not, and `None` (`not decided`) only for a
  stopped run with none; formal-verification.md's "leaves its invariants `not decided`"
  does not describe that.

## Open problems this document records

<!-- derived-from #the-state-store-and-its-encoding -->
<!-- derived-from #safety -->
<!-- derived-from #counterexamples-what-is-checked-before-printing -->
<!-- derived-from #parallel-exploration -->

New: P50 (packing past 65,535 bits; wrong `ok`, reproduced), P51 (`after` skips steps with
`s' = s`; reproduced; the engines agree), P52 (a reduced safety trace whose lifted last
state no longer violates keeps the reduced message silently), P53 (one step of memory-stop
determinism for the CLI's reruns is not shown). Confirmed on the code: P6, P10, P17 (orbit
twins), P19, P20 (restated as the table above). Corrected: P18 (`SameAs` and `DenseBinding` are release
paths whose twins run in every `shadow` run). The [index](./README.md#open-problems) holds
the entries.

## Reproductions

The two runs behind P50 and P51, with the binary described in the first paragraph.

**P51.** The model:

```
entity Order "注文" {
  id: Int @pk
  status: Enum(open, closed) @default(open)
  flag: Bool @default(false)
}
usecase Create "作成"
usecase Touch "触る"
buc B "B"
contains(B, Create)
contains(B, Touch)
creates(Create, Order)
updates(Touch, Order)
sets(Touch, Order.status == open)
after(Touch).assert(Order.flag == true)
```

`rdra-ish verify` prints `Cone_AfterAssert_Touch_0 [AfterAssert_Touch_0]: ok (3 states)`.
With `sets(Touch, Order.status == closed)` it prints `counterexample (7 states)`.

**P50.** One entity `Row` with `id: Int @pk` and 34 columns `c0` to `c33`, each
`Int @default(0)`; every flag below is given to `verify`.

1. One use case `Create` that creates a `Row` and `sets` every column to 1, and
   `forbidden(Row, Row.c33 == 0, Row.c0 != 2, …, Row.c32 != 2)`. With
   `--instance-count 255 --int-range-max 255 --memory 1GiB`: `counterexample (256 states)`,
   a two-step trace whose created rows show `c32=0, c33=0`. With `--instance-count 8`: `ok`.
2. Two use cases `CreateA` and `CreateB` that create a `Row`; `CreateA` sets every column to
   1, `CreateB` sets `c32` to 2 and every other column to 1; and
   `forbidden(Row, Row.c32 == 2, Row.c0 != 3, …)` over every other column. At
   `--instance-count 8`: `counterexample (511 states)`. At `--instance-count 255`:
   `ok (256 states)`, `result: ok`, with the two `usecase` declarations in either order.
