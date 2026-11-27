# The native verifier

<!-- constrained-by ./semantics.md -->
<!-- derived-from ./claims.md#c-the-native-verifier -->

This document states how rdra-ish's native verifier decides a check on a graph that its
breadth-first search (BFS) explored, and on what evidence each step rests. It owns rows N1
to N18 of the [claims inventory](./claims.md#c-the-native-verifier) and uses the notation
of [semantics.md](./semantics.md) (D1 to D25) without redefining it. `T` is the system the
build returns; that it is `T_β(M)` (D13) is claim [I1](./claims.md#0-the-build), not a step
of any proof here.

It describes the code at commit `307ee4e84ffc466bd69ce998c047092f37c6ccc6` (origin/main,
2026-11-28, the merge of #511), the pin of the [index](./README.md). Its `crates/` are those
of `c97c88ef` (the merge of #525): #511 changes only `Cargo.toml`, adding the `ci-checked`
profile (Threats to validity, "Fixed-width integers"), and the CI workflow. The document was
first written against `fb6928c7` and was
brought up to date with `c97c88ef` after the final audit of docs/theory: the code that the
fixes of P19, P20, P50, P51, P52, P53, P54 and P55 changed (#470, #478, #479, #484, #487,
#494, #510, #518, #522) was re-read at `c97c88ef`, and for every other function, type and
constant this document names, its definition at `fb6928c7` was compared with the one at
`c97c88ef`. A definition that did not change, or changed only by a checked narrowing cast
(`id8`, `id32`, #513), reads as it did; one that changed otherwise was re-read or carries
`unread@pin`. `#525`, the last merge before the pin, changes only `cli/src/prove.rs`,
`cli/src/cli.rs` and prose, none of which this document cites. Code is cited as in
[semantics.md](./semantics.md): paths are relative to `crates/rdra-ish-core/src/lifecycle/`
unless they start with `cli/` (`crates/rdra-ish-cli/`) or name the crate root (`core-root/`,
for `crates/rdra-ish-core/src/`).

The reproductions of P50 and P51 ran a release build of the CLI from a sibling worktree
on `64232981`, the parent of `fb6928c7` with the same `crates/`. That worktree's
`crates/rdra-ish-core/src` and `crates/rdra-ish-cli/src` are byte-identical to the pinned
commit's (`diff -rq`), its `crates/rdra-ish-emit/src` differs only under `codegen/`, and no
core or CLI source file there is newer than the binary. The reproductions of P54 were run
by the independent review of this document, on a release binary of another sibling worktree
(possibly the same one) whose core and CLI `src` it checked against the same sources with
`diff -rq`; that binary no longer exists, so they were not re-run for the current text. These
runs describe the code before the fixes of P50 (#479), P51 (#494) and P54 (#470); the
models are now tests, named in [Reproductions](#reproductions), with the commands; each run
takes under a second. How the
document changed after it was first written is in [Errata](#errata). `unread@pin` marks a statement whose cited code was not read at the pinned commit ([claims inventory](./claims.md#how-to-read-a-row)).

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
planned enumeration with a filter over all bindings. Two further premises are not part of
this sketch: that the built `T` is `T_β(M)` of D13 (claim I1; problem P54, fixed in #470,
was a case where the build departed from it by encoding a literal as another value), and that
the edge list reads back the edges it recorded (problem P55, fixed in #479, was a case where it
did not past 2^32 edges).

## The state store and its encoding

<!-- derived-from ../design/native-bfs-parallel.md#the-shared-table -->

A state of `T` is a byte per cell (`expr.rs::State`), one cell per slot of each variable of
D2. The store keeps it packed: each cell in `ceil(log2 |Domain|)` bits, cells end to end,
the total rounded up to bytes (`explore/packing.rs::Packing::of`). States are found by
their packed bytes in a hash table (`explore/arena.rs::Seen`) under the FxHash-style
multiply-rotate hash of `core-root/fasthash.rs` with a fixed seed (`arena.rs::hash`).

**N2 (Store).** Suppose (i) every domain has at most 256 values, so that each value of
`T_β(M)` is a distinct byte and fits its cell (`packing.rs::bits_for` caps a cell at 8
bits), and (ii) every cell starts at a bit offset that `Packing` holds exactly
(`packing.rs::offsets`, a `u32` per cell since #479). Then on states whose cells hold values
inside their domains, packing is injective and unpacking inverts it; the patch path
(`Packing::patch`, which writes only the cells a move changed into the source's packed row)
equals packing the successor, and `Packing::cell` reads the cell back; the arena, segment and
table read back what they stored.

Both preconditions are enforced by the build at the pin. (i): a column whose domain has more
than 256 values is the build error `DomainTooLarge` and gets no variable
(`build/vars.rs`, #470), and `types.rs::Domain::of_number` returns no value past a byte
instead of wrapping. (ii): `build/mod.rs` sums the packed bits of the whole system in a `u64`
(`explore/packing.rs::packed_bits`) and stops with the build error `StateTooWide` past
`MOST_PACKED_BITS = u32::MAX` (#510), so every offset of every cone, a restriction of that
system, fits the `u32`; `offsets` also panics on overflow of its checked add, which the build
error keeps out of reach (#479). Before these fixes neither held: (i) failed for a nullable
Int column at `m = 255` and for an Enum column with 256 values and `@null` or with 257 or more
values (P54), and (ii), then an offset of at most 65,535 in a `u16`, failed for some cone
states wider than 65,536 bits (P50). Every claim below that reads a stored state ("by N1",
"by N2") holds under both.

*Evidence.* Kani proves the round trip, the patch path, and the arena and table operations
on small widths (4 or 11 cells, widths up to 8 bits; the harnesses are listed in row N2).
The table lookup compares whole packed rows (`Seen::find_hashed`), so a hash collision costs
time and never merges two states; the hash's quality does not bear on correctness. The
seed is fixed and no table is iterated into the output (`fasthash.rs` states the rule for
`FastMap`), so the hash does not bear on determinism either. `Packing::pack` panics when a
value does not fit its bits, in release builds too; that check could not catch P54, whose
values were already wrapped to a byte before they reached the store.

**Finding P50 (guarantee, reproduced; fixed in #479).** *As read at `fb6928c7`, before the
fix:* `packing.rs::offsets` stored each cell's bit offset as a `u16` and saturated at
`u16::MAX`. `pack_into` and `unpack_into` count offsets as they go and were unaffected, but
`Packing::patch` and `Packing::cell` used the saturated offsets, so every cell of non-zero
width whose true offset exceeded 65,535 was written to and read from the bits at 65,535.
That was the exact failure condition: a total width above 65,536 bits was necessary but not
sufficient (cells of zero width past the bound did no harm). On a plain run every successor
takes the patch path (`probe.rs::Probes::read`), so distinct successors packed alike and were
merged, and stored rows unpacked to states `T` does not have. On a run up to symmetry the
successor is the canonical form packed whole (`States::pack_into`), so only `Packing::cell`
was affected there: the deadlock readers (`findings.rs::can_leave`, `orbit/findings.rs`). No
check guarded the width in a release build. The Kani harnesses stop at 11 cells, far below the
bound. The shadow twins would have seen it: `Twin::Packed` compares `States::patch` with a
whole pack and `States::cell` with the unpacked row (`explore/arena.rs`) on every read, and
`Twin::Seen` keys its reference table on unpacked states (`core-root/shadow.rs` gives neither
a cap); but `shadow` builds run only in tests and CI, and none of their inputs reached that
width.

The merged states also reached the warnings. When the review re-ran the second reproduction
below, the run also printed `warning[Vacuous_Forbidden_Row_0] … holds vacuously`: no stored
state met the premise because the state that would have was merged away. That was a false
claim about `Reach(T)`, and N18's "by N1" failed with N2's precondition (ii).

Reproduction, before #479 (a model in which every row is created with `c0 … c33 = 1`, 34 Int columns,
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

P50 needed large bounds: a cell of non-zero width past bit 65,535, about 8,192 cells of 8
bits (for example 255 slots of an entity with 32 or more Int columns at `m = 255`). No
sample comes near it, but D1 admits it and nothing rejected it. P54 below needed far less.

*Fixed in #479 and #510.* `Packing` keeps offsets as `u32`, summed with a checked add that
panics past `u32::MAX` bits; below the old limit the offsets are the same numbers, so no
narrower state changes. Both reproductions give the verdicts of 8 instances at 255
(`cli/tests/verify_cli.rs::a_state_past_bit_65535_keeps_its_verdicts`, on the fixtures
`cli/tests/fixtures/wide_state/`), and
`packing.rs::tests::a_state_past_bit_65535_packs_patches_and_reads_back` checks the patch,
the unpack and `Packing::cell` on either side of the first cell past bit 65,535. #479 also
stops a run, as at the state cap, before it numbers a state `u32::MAX`, the marker of an
unresolved move and of an unnumbered component (`Bfs::admit_packed`, `MOST_STATES`). #510
stops the build first past `u32::MAX` bits: `StateTooWide`, an error of the same family as
`DomainTooLarge`, so the checked add is never reached from a model
(`build/mod.rs::width_tests::a_state_past_the_packed_width_is_a_build_error`).

**Finding P54 (guarantee, reproduced by the review; fixed in #470).** *As read at `fb6928c7`,
before the fix:* a value outside a byte was truncated when the build encoded it, so the built
`T` departed from `T_β(M)` of D13 (D2, D8): claim I1 failed. `types.rs::Domain::of_number`
returned `Value(n as u8 + 1)` for a `NullableInt` column; `Domain::size` gives that domain
`m + 2` values, 257 at `m = 255`, and for `n = 255` the sum wrapped to 0, which is
`Value::NULL`. The workspace `Cargo.toml` sets no `overflow-checks` for the release profile
(it has only `[profile.mutants]`), so a release build wrapped silently; a debug build
panicked. The callers that wrapped were the writes of `sets` (`build/sets.rs`) and the
`@default` values (`build/vars.rs`); a predicate's Int literal is compared as a number
(`build/preds/atoms.rs::eq_atom`), and the one call in `atoms.rs` (`lists_whole_domain`) is
reached only for finite domains ([z3.md](./z3.md), item 5 of Lemma U, reads the same). This
nullable-Int case is problem P40 of theme (b), found independently. `Domain::of_variant`
returned `Value((i + shift) as u8)`, an `as` cast that truncated in every build: in an Enum
with `@null` and 256 values, value `v255` became `Value(0)`, the null; in an Enum without
`@null` and 257 or more values, `v256` became `v0`. No check limited the number of values of
an Enum, and the CLI accepts `--int-range-max 255` (`cli/src/export.rs::checked_bounds`), as
D1 does. Because the fault was in `T`, every engine reading `T` inherited it, a replay of the
trace against `T` passed, and the store's checks (N2) did not see it.

*Fixed in #470.* A column whose domain has more than 256 values is the build error
`DomainTooLarge` (`build/vars.rs`) and gets no variable, so no value past a byte exists;
`Domain::of_number` returns `None` past a byte (`u8::try_from`), and `Domain::of_variant` goes
through `Value::at`, which panics rather than wrap a value past a byte. Tests: the engine-corpus cases `width_nullable_int_top` and
`width_nullable_enum_256`, `cli/tests/check_cli.rs::an_enum_column_takes_no_more_values_than_a_state_holds`,
and the reference interpreter's `core/tests/reference/mod.rs::a_value_past_a_byte_is_not_wrapped`
(P26, #492), which decodes each native state through `Domain`'s documented encoding rather
than the builder's tables.

Reproduced by the review at one instance (two states), before #470:

- `n: Int @null`, `sets(Create, Row.n == 255)`, `forbidden(Row, Row.n >= 200)`,
  `--instance-count 1 --int-range-max 255`: `ok (2 states)`, `result: ok`. A wrong `ok`.
  Controls: setting 254 gives `counterexample`; a non-null `n: Int @default(0)` gives
  `counterexample`. `forbidden(Row, Row.n == null)` gives a spurious `counterexample`
  whose trace shows `n=null`.
- `s: Enum(v0, …, v255) @null`, `sets(Create, Row.s == v255)`,
  `forbidden(Row, Row.s != null)` at the default bounds: `ok`. A wrong `ok`. Control: `v254`
  gives `counterexample`. `forbidden(Row, Row.s == v255)` gives `counterexample` with a
  trace that shows `s=null`.

The non-null Enum with 257 values was read, not run. The TLA+ printer renders values from
`T` (`emit/tla/mod.rs::render_value`), so a wrapped value printed as null (`-1` or `"<null>"`)
and TLC checked the same wrong system; TLC was not run on these models.

**Finding P55 (guarantee, read; fixed in #479).** *As read at `fb6928c7`, before the fix:*
`explore/edges.rs::Steps::read` recorded the first edge of each state read as
`edges.first.push(self.moves as u32)`, a bare cast, while `Steps::admit` and the orbit
self-edge in `Bfs::take` used `u32::try_from(…).expect`. Once a run had recorded 2^32 edges, a
state read after that point with no admission following it (late layers whose moves reach
only known states) got a wrapped index in `EdgeList::first`. `EdgeList::source_of` and the
per-state ranges then attributed edges to the wrong sources, and liveness, `can_reach` and
deadlocks read a graph that is not `G`. Each edge takes at least 4 bytes for its target and
more for its binding, so the edges alone exceed `verify`'s default memory limit of 16 GiB,
which stops the run first; a larger `--memory` reached it. Not reproduced. *Fixed in #479:* every edge number (`first`, the edge
being pushed, a parent edge) goes through `edges.rs::edge_number`, which panics past the
last `u32` below the `INITIAL` marker instead of wrapping
(`edges.rs::tests::an_edge_number_past_u32_panics`).

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
cited from row N15; unread@pin).
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

1. *The moves of a state, and their order, are a function of the state.*
   `explore/moves.rs::TransitionSystem::moves_into` always calls
   `reduce/mod.rs::TransitionSystem::moves_of_into`. That function goes through
   `each_raw_move`, which iterates the actions in index order and, per action, the bindings
   `each_enabled` yields and the successors `each_successor` yields; without a reduction
   that is all it does. With one, it skips the actions `Reduction::skips` names (a lookup in
   the reduction's `skip` table), rewrites the S part of a T step's successor through
   `Reduction::recanon` and `Reduction::write`, and appends the class moves
   `r.moves[class]`, where the class comes from `Reduction::class_into`. These read the
   state `s`, the system's immutable fields (the `class_of`, `rises`, `reps` and `moves`
   tables) and buffers they clear or overwrite first (`out.reduced`, `scratch`); `reduce/`
   has no interior mutability outside the quotient cache (`reduce/cache.rs`), which this path
   does not touch. The one lazily filled field on the path, `explore/plan.rs::plans`, is a
   `OnceLock` of a function of the actions alone. Canonicalization (`Canon`) and packing
   carry no state between calls. A worker and the reader call the same functions,
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
  side. Two mechanisms restore the one-job result, and they use two different tests, here
  called *crowded in the pool* and *met the limit*. In the entity pass,
  `targets.rs::TransitionSystem::run_cones` defines the result as that of the cones run one
  at a time in cone order, each with the room the earlier results leave: a run that was
  crowded in the pool (`core-root/memory.rs::Watched::crowded`, set when a charge is refused
  although it fits the pool's room alone) or whose peak exceeds that room runs again under
  `pool.with_room(room)`, an explicit room computed from the results before it. The quotients
  are reduced one at a time in target order (`cli/src/verify/jobs.rs`), so what the quotient
  cache holds is the same on any job count. native-bfs-parallel.md, "The memory pool",
  argues the same for #260's rerun. The CLI's safety and property passes
  (`cli/src/verify/bfs.rs::bfs_cones`, the `crowded` flag of `Native` in
  `cli/src/verify/native.rs`) use the other test: `cli/src/verify/bfs.rs::crowded` is
  `met_memory_limit`, true for any `MemoryCapReached` diagnostic, crowded in the pool or
  not. Until #484 such a run was rerun against the command's pool itself, not an explicit
  room, and a safety cone with partner properties whose first, complete-to-the-end run met
  the limit beside others fell back to its early-stopping run inside the parallel phase
  without being marked: one thread printed `counterexample (65793 states)` where eight
  printed `(15150 states)` (problem P53, reproduced). Since #484 the first run's memory
  stop marks the cone, and every rerun of both passes goes alone under `Caps::alone`, a child
  pool with the room the command's pool has left (`MemoryPool::whole`), which refuses by its
  own room whatever the parent holds. The reruns come after every run of the pass has ended,
  and the results a pass keeps (`BfsCone`, `Plain`, `Native`, `Diag`, the entity pass's
  `Bfs`) hold no charge, the quotient cache being dropped at the end of the entity pass; so
  the room left is the whole room, which a debug build asserts at each rerun. A rerun's stop
  point is then a function of the run alone (N4), the same as at one job, where every run of
  a pass already had the pool to itself; a run that met no limit beside others meets none
  alone, with at least as much room. Evidence:
  `cli/tests/memory_cli.rs::safety_and_property_passes_stop_where_they_stop_one_at_a_time`
  (1, 3, 8 jobs at 7, 8, 10 MiB; fails on main before #484),
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
| `Twin::Recent` | the reader's `Recent` lookup | no | `shadow` runs under `RDRA_ISH_FORCE_PROBES` |
| `Twin::SeenGrowth` | the growth of `Seen`'s index, after every insert: the shared table's (`table::Writer`) under probing, the plain one's otherwise | yes | every `shadow` run (P18, fixed in #473) |

The unit test `explore/tests.rs::a_run_is_the_same_on_any_number_of_threads` compares 1, 2,
3, 4, 7 and 17 workers with `R_0`, on a complete and a capped run, with probing on at 4
and 17 workers (`probes_at`), not under `shadow`. The loom model, the test-only jitter and a
Miri CI job named in native-bfs-parallel.md, "Holding it correct", do not exist; the table
and arena tests are Miri-aware (`cfg(miri)` in `explore/table.rs`, `explore/arena.rs`) but
no workflow runs Miri.

**N17 (Output independent of `RDRA_JOBS`).** Follows from N4 for each run, from the
in-order collection of `in_parallel` results, and, for runs that met the memory limit, from
the reruns above (P53 fixed in #484). Evidence: as N4.

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

**Finding P51 (guarantee, reproduced; a duplicate of P22, resolved in #475 and #494).**
`Bfs::take` skips `check_after` on a move whose successor equals its source (`Probe::Loop`),
so an `after` assertion is never evaluated on a step with `s' = s`. D17 as first written
quantified over every step of the last step of `UC`, and D13 says a step with `s' = s` is
possible. TLC agrees with the code: the TLA+ printer emits `[][… => …]_vars`
(`crates/rdra-ish-emit/src/tla/module.rs`), whose subscript exempts stuttering steps.
Reproduction: `Touch` sets `Order.status := open` on a row created `open`, and
`after(Touch).assert(Order.flag == true)` with `flag` false and never written. Every `Touch`
step is a self-loop; `verify` prints `ok (3 states)`. The control, `Touch` setting `closed`,
prints `counterexample (7 states)`, so the action is in the cone and the self-loop is what
hides the step. The engines agree; what differed was D17's wording (or the modeler's
expectation that an assertion after a use case holds whenever the use case can run).
Resolved by the owner's call: D17 and the language reference take TLA+'s reading (#475), and
since #494 the vacuity check reports this model (`Vacuous_AfterAssert_Touch_0`, test
`tests/vacuous.rs::an_assert_after_a_step_that_only_stutters_holds_vacuously`). The `ok`
the code prints for it is the one D17 now licenses.

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
undecided; `reach_failure` and `TransitionSystem::verdict` also `assert!` completeness in
every build (they only `debug_assert!`ed it before P19 was fixed in #478). TLC cannot state
`can_reach`; besides the unit tests, a naive oracle (`core/tests/oracle.rs`) with its own
exploration and a forward search from every state agrees with the checker on the corpus
and the samples (row N8).

## Deadlocks

<!-- derived-from ../state-derivation.md#deadlocks -->

**N14 (Deadlock).** The finding is the one [semantics.md, D25](./semantics.md#d25)
defines. On a complete entity-cone graph, an open value `v` of a lifecycle column `c` of `E`
(not `@end`, and with some `transitions` arm from `v` to another value: `findings.rs::exits`
and `has_way_on`, which `findings.rs::deadlocks` filters on; a value whose only way out is a
delete is an end state and never reported) is reported stuck iff some `s ∈ S_G` and slot `k`
have `E[k]` present, `c[k] = v`, and no path from `s` through states where `E[k]` is present
reaches an edge that changes `c[k]` or makes `E[k]` non-present. The state reported is the
least such `s` in BFS order (over all slots).

*Proof.* `findings.rs::TransitionSystem::can_leave` marks the sources of leaving edges
(source present, and target not present or with another value) and closes backwards
through present predecessors; this is `E[present U leave]` per slot, as in N8. A state not
marked has no such path, which is the definition. The reported state is the minimum over
`(idx, k)`. ∎ On a stopped graph every candidate value is `Progress::NotDecided`. Both
`can_leave` and the trace read packed cells through `Packing::cell`, so P50 applied here
until #479.
TLC's deadlock check is one-sided (row T6); the naive oracle of `core/tests/oracle.rs`
checks this one against a forward search on its own exploration (P19, fixed in #478).

## Vacuity

**N18 (Vacuity).** `targets.rs::TransitionSystem::vacuity` reports a state rule or `after`
assertion as vacuous only on a complete run of its cone, only when the run reports no
violation of it, and only when no stored state meets one of its premises
(`unmet_of_rule`, `unmet_of_assert`). It is a warning and changes no verdict (D18). On a
complete plain run "no stored state" is "no reachable state" by N1 and N2, so only under
N2's preconditions, which hold since #470, #479 and #510: before, P50 printed a
false vacuity warning, and P54 could have too, since the state that met the premise was
merged away or never built. On a reduced run or a
run up to symmetry the stored states are representatives; that a premise met by some
member of a class is met by its representative is theme (a). The `after` premise is read
from the step's ends (`ComponentRun::step_ends`), filled by `Bfs::push` on every move.
Until #494 a self-loop (`Probe::Loop`) counted as a run of the use case while the
assertion is never evaluated on it (P51), so the two readers treated the step in opposite
ways. Now `Ends::moved` records whether a move of the step changes the state (an orbit
self-edge does), only such moves count, and a step that only stutters is its own vacuity
reason (`UnmetPremise::OnlyStutters`).

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

**Assumption τ (a step that changes nothing takes nothing).** D20 requires `s' ≠ s` for
enabledness but not for *taken*: it says a fair instance is taken "by a step whose action is
`a` and whose binding extends `κ`", and D13 allows such a step with `s' = s`. The code and the
TLA+ output read *taken* as requiring a change of state: `fair_steps` keeps only edges with
`from ≠ to`, and the printer's `WF_vars` takes `⟨A⟩_vars` steps only. This document adopts
that reading as assumption τ, which D20 states since #475 (problem P56, fixed there). A second reading
goes with it: D14's behaviors are sequences of states without action labels, so the code
counts a step `s → s'` as taking every fair instance that labels some edge from `s` to `s'`
(`En`, `Tk` above; `lasso.rs::unfair_loop` likewise), not only the instance of the action a
behavior "used". This is the TLA+ reading (`⟨A⟩_vars` is a predicate on the pair of states)
and is closer to it than D20's "a step whose action is `a`". Under D20 read
literally, a behavior that stutters forever at a state where an instance `(a, κ)` has both a
self-loop and a move to another state takes `(a, κ)` infinitely often on the self-loop and
satisfies `Fair_P`, and the code does not count that state as a stop.

### The check

Fix an `eventually(x)` or `leads_to(x, y)` with `cause`, `goal` as in D21, a binding `b`,
and a fairness `F`. Let `N_b = {s ∈ S_G : ¬goal_b(s)}` and `E_N` the edges of `G` with both
ends in `N_b` and different ends. A **stop** is `s ∈ N_b` with `En(s) = ∅` (`Plain`) or
`¬Moves(s)` (`WfNext`). A strongly connected component `C` of `(N_b, E_N)` is **fair** when
`|C| ≥ 2` and every fair instance enabled at every state of `C` is in `Tk(C)`. The **ends**
are the stops and the states of fair components.

**N9 (Liveness by fair components).** On a complete `G`, if some `s ∈ S_G` with
`trig_b(s) ∧ cause_b(s) ∧ ¬goal_b(s)` reaches an end by a path of `E_N` (of length zero
included), the property fails for `b` under `F` (⇐). Under assumption τ the converse holds
too (⇒), so the property fails iff such an `s` exists.

*Proof.* (⇐) This direction does not use τ: it needs only that a step to another state
takes the instances that label it, which D20 states. Let `s` reach an end `e` inside `N_b`. Take the parent path to `s` (N1), then
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

(⇒) This direction uses τ twice, marked below. Let `σ` be a behavior satisfying `F` and
`i` a position with `trig_b ∧ cause_b` at
`σ_i` and no goal at any `j ≥ i`. Then `σ_i ∈ S_G` (it is reachable) and every `σ_j`,
`j ≥ i`, is in `N_b`. Two cases, since `S_G` is finite. (a) From some point `σ` stutters at
a state `e` forever. Under `Plain`, a fair instance enabled at `e` would be enabled forever
and never taken (stuttering takes nothing: τ), against `Fair_P`, so `En(e) = ∅`; under
`WfNext`, a step from `e` to another state would violate `WF_vars(Next)`, so `¬Moves(e)`.
So `e` is a stop. (b) `σ` changes state infinitely often. Let `I` be the states visited
infinitely often and the non-stuttering steps taken infinitely often; `I` is strongly
connected within `(N_b, E_N)`, has at least two states, and lies in one component `C`. A
fair instance enabled at every state of `C` is enabled at every state of `σ` from some
point on, so `Fair_P` makes `σ` take it infinitely often, on a step inside `I ⊆ C`, and by
τ on a step that changes the state, an edge of `E_N`; so it is in `Tk(C)`, and `C` is fair.
(Without τ the instance could be taken only on self-loops, which `Tk` does not count.) In
both cases `σ_i` reaches the end by the non-stuttering steps of `σ`, which are edges of
`E_N`. ∎

Without τ, (⇒) fails in exactly the way the note on assumption τ describes, and the
consequence would be an `ok` that D20 and D21 as first written did not license (P56, fixed in
#475 by stating τ in D20); the counterexamples of
(⇐), and with them N10 and N11, do not depend on τ.

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
`can_reach` keeps the slot present along the path. Read on the code here, this confirmed
problem P6; TLC reads the printed formula the same way. #475 decided P6 as documented:
language-reference.md and D21 now state the reading. A `.present()` goal (P64) has the
presence of every bound row in its predicate, conjoined by the build
(`build/preds/temporal.rs::Builder::goal`), so the same evaluation never meets it on a
deleted row.

## Counterexamples: what is checked before printing

<!-- derived-from ../design/native-temporal-verification.md#counterexamples -->
<!-- derived-from ../formal-verification.md#what-verify-checks -->

**N16** claims that every printed counterexample replays. The table records, per check and
per kind of run, what the code at the pin does to a trace before it prints; it was rebuilt
row by row from the code after #518 and #522 (P20). *Replayed* means matched step by step,
action, parameters and successor, against `moves` from an initial state
(`explore/orbit.rs::replays`, which calls `trace.rs::TransitionSystem::replay`, or
`lasso.rs::TransitionSystem::replay_lasso` for a lasso). *Re-evaluated* means the check is
evaluated again on the trace's last state or step. *Plain* is an unreduced run without
symmetry; *reduced* a run of a cone reduced by its independent sub-lifecycle (#74), whose
trace is lifted by `reduce/lift.rs::TransitionSystem::lift`, which checks every lifted step
against the unreduced cone's moves in every build and returns a `ReplayError` on the first
that is not one; *symmetric* a run up to slot permutations, lifted by
`explore/orbit.rs::lift_path` (safety, `always`, deadlocks) or the lasso lift of
`explore/orbit/liveness.rs`. The shared path for stored traces is
`explore/graph.rs::ComponentRun::lifted_path` (behind `trace_to`, `trace_of` and
`trace_along`): it replays the stored path on the system the run explored, then lifts it. A
failure is a fault, a tool error that the run counts as such and prints without a trace:
`LiftFault` for a state rule or `after` assertion, `LassoFault` for a property, `TraceFault`
for a deadlock or `can_reach`.

| Check | Plain | Reduced | Symmetric |
|---|---|---|---|
| State rule, `after` | replayed (`explore/mod.rs::ComponentRun::trace_violations`, `trace_of`; `LiftFault` carrying `LassoFault::Replay` on failure, since #518); not re-evaluated: the violation was evaluated at admission on the same states | replayed on the reduced system, then lifted step by step (`LiftFault` on failure, since #522); re-evaluated on the lifted last state, and a lifted state that no longer violates is `LiftFault` carrying `LassoFault::Satisfies` (#487, P52) | lifted (`lift_path`), replayed on the system the run explored, lifted through the reduction when that system is reduced (`explore/orbit.rs`, `LiftFault` on either failure, since #522); re-evaluated on the lifted last state (`LassoFault::Satisfies` as in the reduced column) |
| `always` | re-evaluated on the stored path (`explore/properties.rs::ComponentRun::decide_property`: `property_failure`, which `expect`s a failure, then `debug_assert_eq!` on its position), then replayed (`trace_along`; `PropertyVerdict::Fault` with `LassoFault::Replay` on failure, since #522) | re-evaluated on the reduced path as in the plain column, then replayed on the reduced system and lifted step by step (`trace_along`); not re-evaluated on the lifted path | only on a cone the reduction leaves whole (`cli/verify/native.rs::native_property`): lifted, replayed (`LassoFault::Replay`) and re-evaluated on the lifted path (`OrbitRun::decide_always`, `LassoFault::Satisfies`) |
| `stable`, `unchanged`, `precedes` | as `always`, plain | never reduced (`decide_property` restricts without reduction) | not run up to symmetry |
| `eventually`, `leads_to` | replayed, formula re-evaluated, fairness re-checked (N11; `LassoFault` on failure) | the reduced lasso is never printed; what prints is either the `WfNext` lasso lifted to the unreduced cone and replayed, re-evaluated and fairness-checked there (`decide_liveness_reduced`; `Spurious` otherwise, `Spurious::Fails` for a lift that fails its step checks since #522), or the lasso of the rerun on the unreduced cone (`Cone_P_Full`); if that rerun stops, the property is `not decided` | only on a cone the reduction leaves whole: replayed, re-evaluated and fairness-checked after lifting (`orbit/liveness.rs`, `LassoFault` on failure, `LassoFault::Unclosed` past 10,000 copies) |
| `can_reach` | replayed (`targets.rs::TransitionSystem::verdict`, `trace_to`; `TraceFault` on failure, since #522); the finding is not decided again on the trace | never reduced (`explore_cone` keeps a `can_reach` cone whole) | not run up to symmetry |
| Deadlock | replayed (`targets.rs::read_entity`, `trace_to`; `TraceFault` in the deadlock's place on failure, since #522); not decided again on the trace | `verify` reduces entity cones (`explore_targets_reduced_in`), `check` does not: replayed on the reduced system and lifted step by step (`TraceFault`); the finding is decided on the reduced graph (cones.md, Theorem A8, consequence 3) | lifted (`lift_path`), replayed, and lifted through the reduction when the cone is reduced (`orbit/findings.rs::trace_to_slot`, `TraceFault` on failure) |

Three consequences.

- *P50 was visible only in a trace that was not replayed.* At `fb6928c7` a plain safety
  trace was printed as stored, so the corrupted states of the P50 reproduction printed as a
  counterexample with no fault. The replay of #518 would have turned that wrong verdict into a
  tool error, though not the wrong `ok`. A replay cannot catch P54 at all: it replays against
  `T`, which held the wrapped value.
- *P20, fixed.* Every printed trace now replays on the system the run explored and, when that
  system is reduced or a quotient, lifts with every step checked against the unreduced cone
  (#518 for state rules and `after`; #522 for the rest). Before, plain safety traces were not
  replayed, reduced traces were lifted with `debug_assert!` checks only, and symmetric traces
  were replayed but failed by panic. A reduced liveness counterexample whose unreduced rerun
  stops still becomes `not decided` rather than printing the lifted lasso.
- *What is still unchecked or panics.* A reduced `always` is not re-evaluated on its lifted
  path. A plain `always`'s position is checked by `debug_assert_eq!` only, and
  `decide_property` `expect`s that the stored path breaks the property. Deadlock and
  `can_reach` traces replay, but the finding is not decided again on them. The lifted start of
  a reduced trace is not checked against the unreduced cone's initial states. And the lifts
  hold invariants of their own tables by release `expect`s and `assert!`s:
  `reduce/lift.rs::lift` (that the start's class holds a representative and an initial S
  state, that a T step's rise and a class move's target exist, one inner group per rise and
  per S step, `assert_eq!` that a rise lifts as itself, the indexing of `block_of_class`, and
  up to `H` a `debug_assert!` that the concrete S path starts at the stored initial state),
  `explore/orbit.rs::lift_path` (that a stored edge is a move of its source), and the lasso
  lift of `explore/orbit/liveness.rs` (that an edge is the move at its place and its
  permutation takes the move to the stored target). A failure there stops the command; none
  can print a trace that does not replay.

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
  move read through the accumulated `τ`, and `τ := τ ∘ σ⁻¹`. The lasso lift of
  `orbit/liveness.rs` takes instead the move at the edge's place among its source's moves and
  the permutation the annotated run stored for the edge, and `assert!`s that they match the
  edge. A liveness loop on
  (state, binding) pairs (**N12**) closes after at most the order of the loop's composed
  permutation, capped at 10,000 copies (`orbit/liveness.rs::MOST_COPIES`). **N13**: a
  lifted trace that does not replay is a tool error, never a printed counterexample (P20,
  fixed in #518 and #522; before, it panicked). The lift itself still `expect`s that each
  stored edge is a move of its source, a release panic if the quotient's tables are wrong
  (the third consequence above).

The `Twin::OrbitLiveness` twin compares the orbit search with the plain run's `avoid`
(`ComponentRun::liveness_reference`), so it tests the quotient against N9's implementation,
not N9 itself; it skips runs past 200,000 states (`orbit/reference.rs::SHADOW_CAP`), as do
`Twin::OrbitRun` and `Twin::OrbitStateModels`. This confirmed the twin parts of P9 and P17:
since #475 the twin's description says it checks the quotient only (P9), and since #473 a
`shadow` build reports each skipped call (P17).

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
  compares full packed rows, so a hash collision costs time, not soundness (P50 and P54
  are faults of the encoding, not of hashing).
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
- **Parallel and distributed state-space search.** Stern and Dill ("Parallelizing the Murφ
  verifier", CAV 1997, LNCS 1254) distribute Murφ's reachability over a distributed-memory
  machine or a network of workstations by message passing.
  Barnat, Brim and Chaloupka ("Parallel breadth-first search LTL model-checking", ASE 2003)
  detect accepting cycles of an LTL product with a distributed, level-synchronized BFS over
  an MPI cluster, through back-level edges; their subject is the cycle check, not only the
  parallel BFS. Holzmann and Bošnački ("The design of a multicore extension of the SPIN
  model checker", IEEE TSE 33(10), 2007) extend SPIN to shared-memory multicores. Laarman,
  van de Pol and Weber ("Boosting multi-core reachability performance with shared hash
  tables", FMCAD 2010) replace static partitioning with one lockless shared hash table that
  all workers probe; rdra-ish's shared table (`explore/table.rs`: control bytes and indices
  in `AtomicU64` and `AtomicU32` arrays, written only by the reader) is of that kind, but
  release builds turn probing off (N4, step 3). rdra-ish parallelizes only expansion: one reader
  commits every state in sequential order, which gives up scaling of the commit step in
  exchange for the exact one-thread run (N4), shortest traces included. TLC's workers take
  states from a shared queue without such a reader. An issue reports that with several
  workers the diameter TLC prints varies between runs, a collaborator explains it by a state
  being discovered first along a longer path, and a maintainer answers that a deterministic
  diameter would need synchronization that limits scaling
  ([tlaplus/tlaplus#883](https://github.com/tlaplus/tlaplus/issues/883)). That TLC's traces
  with several workers can then also differ between runs and not be shortest is our
  reading, not a statement from TLC's documentation.
- **Symmetric counterexamples.** Ip and Dill ("Better verification through symmetry",
  FMSD 9(1/2), 1996) and Emerson and Sistla ("Symmetry and model checking", FMSD 9(1/2),
  1996) explore the quotient by a symmetry group and relate paths of the quotient to paths
  of the original system, which is what `orbit.rs::lift_path` does when it carries the
  accumulated permutation along a stored path (N12, N13). rdra-ish then replays the lifted
  trace on `T` rather than relying on that correspondence alone.
- **TLA+ fairness.** `WF_vars(A)` and stuttering steps are Lamport's ("The temporal logic of
  actions", ACM TOPLAS 16(3), 1994; *Specifying Systems*, 2002). D20's per-instance weak
  fairness is the conjunction of `WF_vars` over instances that the TLA+ printer emits; the
  exemption of stuttering steps from action properties (`[][A]_vars`) is the source of
  P51, and the same exemption in `WF_vars(A)`, which only `⟨A⟩_vars` steps satisfy, is the
  reading assumption τ adopts (P56).

## Threats to validity

- **The step relation is trusted.** Every proof above assumes `moves_into` enumerates the
  steps of the built `T` (N1) and that the built `T` is `T_β(M)` of D13 (claim I1). The evidence is differential (TLC, which reads
  the same `TransitionSystem`, so a fault in the build of `T` would pass both) and one twin.
  P54 was such a fault in the build, fixed in #470: a literal wrapped to another value of its
  domain. Since #492 a reference interpreter (`core/tests/reference/`, P26, partly closed)
  checks I1 without reading the build's output, on a subset of the language: it explores the
  model on its own and compares initial states, reachable states, labelled edges and broken
  rules with the native run of the whole model, on the engine corpus and on random models.
- **Bounded mechanical evidence.** The Kani harnesses cover widths far below where P50
  began and no domain of more than 256 values (P54); the fixes of both are pinned by the
  tests named in their findings, not by Kani. `Twin::Packed` and `Twin::Seen` would detect a
  wrong offset but no `shadow` input is that wide; other shadow twins skip runs above 200,000
  or 2,500,000 states, and since #473 say so (P17), and the probe twins run in one CI job,
  on code release builds do not run (P18, corrected and partly fixed in #473).
- **Readers share code with the checks they certify.** `unfair_loop` and `avoid` both use
  `moves` and `Property::fair_instance`; `explain_property` and `liveness_search` both use
  `holds_bound` and `triggers`. Self-certification catches a fault in the search, not in
  these.
- **Fixed-width integers.** Three places truncated or saturated silently in a release build,
  and each gave a wrong verdict within the documented bounds: the value encoding (P54, a
  wrong `ok` at one instance with a nullable Int at `m = 255` or a nullable Enum of 256
  values; fixed in #470), the bit offsets of the packing (P50, a wrong `ok` past 65,535 bits;
  fixed in #479, with `StateTooWide` in #510), and the edge index (P55, read only, past 2^32
  edges; fixed in #479). #479 also audited the narrowing casts of the explore store (state
  numbers, edge numbers, slot counts), and #513 replaced the narrowing `as` casts of the
  lifecycle code with the checked `id8` and `id32` (`narrow.rs`), which panic rather than
  wrap. Since #511 the CI jobs that verify the samples, the corpus and engine agreement build
  with the `ci-checked` profile of the workspace `Cargo.toml` (release with
  `overflow-checks`), so an arithmetic overflow panics there; the `release` profile, which
  users run, still wraps silently.
- **Fairness wording.** N9's (⇒) direction, hence every liveness `ok`, rests on assumption
  τ, which D20 states since #475 (P56).
- **Determinism of memory stops beside other runs** rests on the reruns of N4; the CLI's
  passes rerun alone with an explicit room since #484 (P53).
- **Reproductions** used binaries built in other worktrees, and those of P54 were run by
  the review rather than for this document (see the first paragraph). Each is now a test on
  a clean build ([Reproductions](#reproductions)).
- **Read at the pin for the N16 table and the lift's mechanics:** `reduce/lift.rs::lift`
  (every lifted step checked by `is_move`, the remaining `expect`s), `::lift_cycle` and
  `::lift_stop` (a failed lift is `Spurious::Fails`), `explore/graph.rs::ComponentRun::lifted_path`,
  `explore/mod.rs::ComponentRun::trace_violations`, `explore/properties.rs::ComponentRun::decide_property`,
  `explore/orbit.rs::lift_path`, `::replays` and `OrbitRun::decide_always`, the safety lift of
  `explore/orbit.rs`, `explore/orbit/findings.rs::lift_to` and `trace_to_slot`, the lasso lift
  of `explore/orbit/liveness.rs`, `explore/liveness.rs::ComponentRun::decide_liveness` and
  `decide_liveness_reduced`, and the `TraceFault` arms of `targets.rs`. For N18,
  `targets.rs::vacuity`, `unmet_of_rule` and `unmet_of_assert` (which reads `step_ends`)
  and `edges.rs::StepEnds::read` (which records whether a move changes the state,
  `Ends::moved`, since #494); for N14, the choice of the least `(idx, k)` per column value
  and the `has_way_on` filter in `findings.rs`.
- **Not re-read here (unread@pin):** `cli/src/verify/end.rs` (N15); `reduce/lift.rs::lift_stop`
  beyond `MAX_WALK`, `Spurious::NoLoop` and its error path; the `Twin::EachEnabled` path;
  `fasthash.rs` beyond the rule quoted in N2; `cli/src/verify/native.rs::native_property_after`
  beyond the order of its runs and what it records for the comparison; and the condition
  under which `verify` runs a property up to symmetry (`Shared::up_to_symmetry`), taken from
  `native_property`'s doc comment.
- **P10, prose.** `cli/src/verify/bfs.rs::BfsCone::verdict` returns `counterexample`
  whenever the run found a violation of the whole system, stopped or not, and `None`
  (`not decided`) for a stopped run with none; formal-verification.md said a stopped cone
  "leaves its invariants `not decided`" until #477 corrected it.

## Open problems this document records

<!-- derived-from #the-state-store-and-its-encoding -->
<!-- derived-from #safety -->
<!-- derived-from #counterexamples-what-is-checked-before-printing -->
<!-- derived-from #parallel-exploration -->
<!-- derived-from #the-fairness-model -->

New, with their status at the pin; none is open:

- P50 (packing past 65,535 bits; wrong `ok`, reproduced): fixed in #479, with the build
  error `StateTooWide` in #510 ([The state store](#the-state-store-and-its-encoding)).
- P51 (`after` skips steps with `s' = s`; reproduced; the engines agree): a duplicate of P22,
  resolved by D17's TLA+ reading (#475) and the vacuity warning of #494 ([Safety](#safety)).
- P52 (a reduced safety trace whose lifted last state no longer violates kept the reduced
  message silently): fixed in #487 for state rules and `after` assertions; the rest went
  under P20.
- P53 (one step of memory-stop determinism for the CLI's reruns was not shown; reproduced):
  fixed in #484 ([Parallel exploration](#parallel-exploration)).
- P54 (a value past a byte wrapped when the build encoded it; wrong `ok` at one instance,
  reproduced by the review; its nullable-Int case is P40): fixed in #470.
- P55 (the edge index wrapped past 2^32 edges; read): fixed in #479.
- P56 (D20 did not say that a step with `s' = s` takes nothing, which N9's (⇒) direction
  needs; the engines agree): fixed in #475, which states τ in D20 ([The fairness
  model](#the-fairness-model)).

Read on the code here, with their status at the pin: P6 (decided as documented, #475); P10
(prose, corrected in #477); P17, the orbit twins' caps (fixed in #473: a skip is reported);
P19 (fixed in #478: a naive oracle, and `assert!` on completeness in every build); P20
(fixed in #518 and #522: every printed trace replays, the table of
[Counterexamples](#counterexamples-what-is-checked-before-printing); the release `expect`s
and `assert!`s it lists remain). Corrected: P18 (`SameAs` and `DenseBinding` are release paths
whose twins run in every `shadow` run; `Twin::SeenGrowth` runs on the release path since
#473; the loom model and a Miri job still do not exist). The [index](./README.md#open-problems)
holds the entries.

## Reproductions

The runs behind P50, P51 and P54, with the binaries described in the first paragraph, at
`fb6928c7` before the fixes. Each is now a test or corpus case in the repository, named in
its heading; the outputs quoted are those of the runs before the fix.

**P51** (a duplicate of P22; in the repository since #494 as
`tests/vacuous.rs::an_assert_after_a_step_that_only_stutters_holds_vacuously`). The model:

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

`rdra-ish verify` printed `Cone_AfterAssert_Touch_0 [AfterAssert_Touch_0]: ok (3 states)`.
With `sets(Touch, Order.status == closed)` it printed `counterexample (7 states)`. Since #494
the vacuity check reports `AfterAssert_Touch_0` as vacuous, the step running only as one that
changes nothing, which the test asserts at one and two instances with the control.

**P50** (in the repository since #479: `crates/rdra-ish-cli/tests/fixtures/wide_state/{two_creates,one_create}.rdra`, run by `cli/tests/verify_cli.rs::a_state_past_bit_65535_keeps_its_verdicts`, which asserts the 8-instance verdicts at 255). One entity `Row` with `id: Int @pk` and 34 columns `c0` to `c33`, each
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

**P54** (run by the review; in the repository as the engine-corpus cases `width_nullable_int_top.rdra` and `width_nullable_enum_256.rdra` and `cli/tests/check_cli.rs::an_enum_column_takes_no_more_values_than_a_state_holds`, since #470, and `core/tests/reference/mod.rs::a_value_past_a_byte_is_not_wrapped`, since #492). An entity
`Row { id: Int @pk; n: Int @null }`, a use case `Create` that creates a `Row` with
`sets(Create, Row.n == 255)`, and `forbidden(Row, Row.n >= 200)`, under
`--instance-count 1 --int-range-max 255`: `ok (2 states)`, `result: ok`. With 254 in place
of 255, or with `n: Int @default(0)`, the same command prints `counterexample`. The Enum
case replaces `n` with `s: Enum(v0, v1, …, v255) @null` (256 declared values), sets
`Row.s == v255`, and forbids `Row.s != null`, at the default bounds: `ok`; with `v254`,
`counterexample`.

## Errata

Changes to statements of this document after it was first written, newest last.

- **2026-11-28, after an independent review** (commits `5cd4c35e`, `02e5b48f`, and
  `12baa870`). The review ran the P54 reproductions on its own release binary; the revisions
  that followed did not re-run them, because that binary no longer exists. The "Symmetric"
  bullet of [The lift's mechanics](#the-lifts-mechanics) was written in the first version
  (`6e1451a1`) from a reading of `explore/orbit.rs::lift_path` and `orbit/liveness.rs` up to
  their certification steps; the revisions after the review did not re-check it against their
  bodies.
- **2026-11-28, fixes recorded as they merged.** P50 and P55 fixed in #479 (commit
  `37b58561`); P54 and P40 fixed in #470 (commit `82ba631f`); P19 fixed in #478 (`26768ce4`);
  P53 fixed in #484 (`2f0bc045`, `9745f938`); P17 and P18 in #473 (`101ff45c`); P6, P22 and
  P51, P56 marked for #475 (`a92020fe`); P52 fixed in #487 for state rules and `after`
  (`a14a3fae`); the revision history moved into this section (`5fe8931f`); `unread@pin`
  markers and *repro pending in repo* (`3345c389`).
- **2026-11-28, after the owner's review of semantics.md (P60)** (commit `c55dfa60`). This document said that
  under P54 "`T` is not `T_β(M)`". semantics.md defined `T_β(M)` as the build's output, so
  under its own definitions that sentence was false. semantics.md now defines `T_β(M)` from
  the model's text. P54 and the threats to validity now say that the built `T` departs from
  D13, a failure of claim I1. No proof here changes: every proof is about the built `T`.
- **2026-11-28, later fixes** (commits `16a701b7` and `58b7f522`, #494: P51's vacuity; `1aeba6e0`,
  #495: a `.present()` goal in "The goal on absent rows"; `2a373175`, #510: `StateTooWide`;
  `79c4d186` and `9519199b`, #522: N13 and the N16 reading list say a failed replay is a fault).
- **2026-11-28, after the final audit of docs/theory** (findings 1, 2, 4, 16, 45 to 49;
  commit `065e53d7`). Re-pinned from `fb6928c7` to `307ee4e8`, whose crates are those of `c97c88ef`
  (commit `2e9ff3e6` named the content commit; a follow-up moved the pin from `c97c88ef` to
  `307ee4e8`, the index's pin, after #511), with the method of the
  first paragraph; the sentence that the verifier was the same at `b83b9544` and `fb6928c7`
  is gone, and semantics.md is cited for D1 to D25, N14 citing D25 with its `has_way_on`
  condition. N2's preconditions are enforced (`DomainTooLarge`, #470; `u32` offsets, #479;
  `StateTooWide`, #510), where the text said neither was. The findings P50, P54 and P55 and
  the warnings of N18 describe the code before their fixes in the past tense. The table of
  [Counterexamples](#counterexamples-what-is-checked-before-printing) was rebuilt row by row
  from the code after #518 and #522: every printed trace replays, the reduced lift checks each
  step, and a failure is a fault, where the old table gave "nothing", "`debug_assert!` only"
  and "panics"; it also said entity cones are not reduced, but `verify` reduces them. What
  remains unchecked, and the release `expect`s and `assert!`s of the lifts, are listed there.
  The "Symmetric" bullet of [The lift's mechanics](#the-lifts-mechanics) was re-read against
  `explore/orbit.rs::lift_path` and the lasso lift of `orbit/liveness.rs`, which takes the
  move at the edge's place rather than the first matching one. "All are open" became a status
  per problem; the Reproductions no longer say *repro pending in repo*, and name the P51
  test `an_assert_after_a_step_that_only_stutters_holds_vacuously` instead of a test called
  `P51`, which does not exist. The P26 reference interpreter is described as it exists (#492).
