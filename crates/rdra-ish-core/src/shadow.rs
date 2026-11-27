//! The `shadow` feature (#275): each fast function that has a reference twin also runs the
//! twin and panics, naming the call, when the two differ. A run of a `shadow` build over the
//! samples and the corpus holds every fast path to its reference on every call it makes.
//!
//! The twins live in files of their own, so a diff shows whether a change touched one. Each
//! twin's module compiles under one of three gates:
//!
//! - `feature = "shadow"` alone, for most twins.
//! - `any(debug_assertions, feature = "shadow")`: a debug build checks every call too.
//!   `explore/moves/reference.rs` and `reduce/fold/reference.rs`.
//! - `any(test, feature = "shadow")`: a test build checks or calls the twin as well.
//!   `reduce/partition/reference.rs`, `reduce/partition/up_to_h.rs` and
//!   `explore/symmetry/reference.rs`.
//!
//! | Fast path | Twin |
//! |---|---|
//! | `each_planned` (`explore/moves.rs`, #253) | `explore/moves/reference.rs`: `bindings` filtered by `pre_guards_hold`, both in `explore/moves.rs` |
//! | `Seen` (`explore/arena.rs`, #240) | `explore/arena/reference.rs`: an ordered map of the state bytes |
//! | `observe`'s source pattern, looked up once per state (`explore/bfs.rs`, #251) | `explore/bfs/reference.rs`: the lookup on every edge |
//! | `observe`'s skip of a row whose columns are unchanged (#251) | `explore/bfs/reference.rs`: what the edge would intern |
//! | `quotient_blocks` (`reduce/partition.rs`, #252) | `reduce/partition/reference.rs`: the sweeps, on quotients of up to `SHADOW_CAP` states |
//! | a `QuotientCache` hit (`reduce/cache.rs`, #252), a declined entry's included (#289) | `Classes::of` itself, through a cache that holds nothing |
//! | a run of S another quotient key ran (`QuotientCache::run_of_s`, #296) | a fresh run of the sub-lifecycle |
//! | the reduced step's class moves (`moves_of_into`, #252) | `reduce/reference.rs`: `moves_from` |
//! | `Canon::canonical`, the canonical form under instance symmetry (`explore/symmetry/canon.rs`, #212) | `explore/symmetry/reference.rs`: the least layout over every element of `G_s`, when it has at most 6^4 |
//! | `OrbitRun`, a run up to instance symmetry (`explore/orbit.rs`, #212) | `explore/orbit/reference.rs`: the plain run of the same cone, up to `SHADOW_CAP` states |
//! | `OrbitRun` on a #74-reduced cone, up to `G_T` (#212 phase 3) | the same twin, counted as `reduced_orbit_run` |
//! | `OrbitRun::decide_liveness`, liveness on pairs of a stored state and a binding (#212 phase 4) | `explore/orbit/reference.rs`: the plain run's `avoid` per binding, up to `SHADOW_CAP` states, counted as `orbit_liveness` |
//! | `OrbitRun::state_model_findings`, an entity cone's findings up to symmetry (#370) | `explore/orbit/reference.rs`: `state_model_findings` on the plain run, up to `SHADOW_CAP` states, counted as `orbit_state_models` |
//! | a fold's input bit without allocations (`Fold::holds`, `reduce/fold.rs`, #296) | `reduce/fold/reference.rs`: the bit with its `Vec`s |
//! | edges without their source, binding ids in 1 to 4 bytes, parents (`explore/edges.rs`, #296 2.1) | `explore/edges/reference.rs`: the 12-byte `Edge` list and the parent edges, as a run kept them before |
//! | an after-assert step's ends recorded on a run without an edge list (`StepEnds`, #296 2.1) | `ComponentRun::step_ends`'s scan of that 12-byte list |
//! | `Classes::of` on a run of S up to `H` (`reduce/partition.rs`, #212 phase 2) | `reduce/partition/up_to_h.rs`: the classes of the plain run of S, up to `SHADOW_CAP` states |
//! | states packed into their cells' bits (`States`, `explore/arena.rs`, #296 2.3) | the states unpacked, beside the packed rows, compared on every read; `Seen`'s twin keys the unpacked bytes |
//! | a move the expanding thread found to repeat an earlier move of its chunk (`Probe::SameAs`, `explore/probe.rs`, #296 3.1) | `Seen`'s twin: the state it names is the one the earlier move reached |
//! | a binding's id looked up by its number (`Bindings::id_numbered`, `explore/bfs.rs`, #296 3.1) | the map from params to ids `Bindings::id` keeps |
//! | a worker's probe of the run's shared table (`Probe::Hit`, `explore/probe.rs`, #296 3.1) | `Seen`'s twin: the state the hit names is the one the key is |
//! | the shared table's growth (`table::Writer`, `explore/table.rs`, #296 3.1) | a hashbrown table filed alike: the two capacities are equal after every insert |
//! | the reader's look-up of a `Miss` among the states admitted since its chunk was handed out (`Recent`, `explore/bfs.rs`, #296 3.1) | `Seen::find_hashed` on the run's whole table |
//! | a graph's adjacency, its two directions built side by side (`Graph::adjacency`, `explore/graph.rs`, #296 3.1) | `Adjacency::new`: one direction after the other |
//! | the states that can leave each column and slot, side by side (`leave_columns`, `findings.rs`, #296 3.1) | `can_leave` on each column and slot in turn |
//! | every step of an action changes only the vars `action_writes` names, which the cones read as `Wr(a)` (P35 of docs/theory/README.md) | the bytes the step changed, against `Plan::writes`; not a fast path but the over-approximation Lemma 2 of docs/theory/cones.md rests on, also checked by a debug build |
//!
//! A `shadow` build also runs the checks `shadow_assert!` makes, as a debug build does: each
//! reader of a run's check results reads only checks the run evaluated (#251), and a steady
//! action's step leaves the inputs of a reduced cone as they were (`moves_of_into`).
//!
//! A twin lets a call pass unchecked in two cases (`Skip`): the input is past the twin's own
//! cap (a group of more than 6^4 elements, a plain run past `SHADOW_CAP` states), counted as
//! `skipped`; or the fast path's run, or the twin's under the same limits, stopped, which
//! leaves nothing to compare, counted as `stopped`. When the process exits it prints how many
//! calls each twin compared, skipped and stopped on, with `RDRA_ISH_SHADOW_STATS` set or
//! whenever a call passed unchecked, so a run that compared nothing, or let a call through,
//! shows as such. The corpus harness fails a case on any `skipped` call, since every corpus
//! case is within every cap, and the CI's shadow job fails on a skip its runs do not expect.

use std::sync::atomic::{AtomicU64, Ordering};

/// A reference twin, for the counts.
#[derive(Clone, Copy)]
// A test build skips no partition and recomputes no cache hit.
#[cfg_attr(test, allow(dead_code))]
pub(crate) enum Twin {
    EachEnabled,
    Seen,
    Source,
    UnchangedRow,
    Partition,
    CacheHit,
    ClassMoves,
    Canonical,
    OrbitRun,
    ReducedOrbitRun,
    FoldHolds,
    SharedRun,
    SUpToH,
    Edges,
    StepEnds,
    OrbitLiveness,
    Packed,
    OrbitStateModels,
    SameAs,
    DenseBinding,
    Hit,
    SeenGrowth,
    Recent,
    SideBySideAdjacency,
    LeaveColumns,
    Writes,
}

const TWINS: [(Twin, &str); 26] = [
    (Twin::EachEnabled, "each_enabled"),
    (Twin::Seen, "seen"),
    (Twin::Source, "source"),
    (Twin::UnchangedRow, "unchanged_row"),
    (Twin::Partition, "quotient_blocks"),
    (Twin::CacheHit, "cache_hit"),
    (Twin::ClassMoves, "class_moves"),
    (Twin::Canonical, "canonical"),
    (Twin::OrbitRun, "orbit_run"),
    (Twin::ReducedOrbitRun, "reduced_orbit_run"),
    (Twin::FoldHolds, "fold_holds"),
    (Twin::SharedRun, "shared_run"),
    (Twin::SUpToH, "s_up_to_h"),
    (Twin::Edges, "edges"),
    (Twin::StepEnds, "step_ends"),
    (Twin::OrbitLiveness, "orbit_liveness"),
    (Twin::Packed, "packed"),
    (Twin::OrbitStateModels, "orbit_state_models"),
    (Twin::SameAs, "same_as"),
    (Twin::DenseBinding, "dense_binding"),
    (Twin::Hit, "hit"),
    (Twin::SeenGrowth, "seen_growth"),
    (Twin::Recent, "recent"),
    (Twin::SideBySideAdjacency, "side_by_side_adjacency"),
    (Twin::LeaveColumns, "leave_columns"),
    (Twin::Writes, "writes"),
];

static COMPARED: [AtomicU64; TWINS.len()] = [const { AtomicU64::new(0) }; TWINS.len()];
static SKIPPED: [AtomicU64; TWINS.len()] = [const { AtomicU64::new(0) }; TWINS.len()];
static STOPPED: [AtomicU64; TWINS.len()] = [const { AtomicU64::new(0) }; TWINS.len()];

/// Why a twin let a call pass unchecked.
#[derive(Clone, Copy)]
#[cfg_attr(test, allow(dead_code))]
pub(crate) enum Skip {
    /// The input is past the twin's own cap: a group of more than 6^4 elements, a plain run
    /// or a quotient past `SHADOW_CAP` states. The fast path's result stands unchecked.
    OverCap,
    /// The fast path's run, or the twin's run under the same caps and memory pool, stopped at
    /// a limit the run sets, so there is no complete result to compare.
    Stopped,
}

/// Notes that `twin` compared one call.
pub(crate) fn compared(twin: Twin) {
    COMPARED[twin as usize].fetch_add(1, Ordering::Relaxed);
}

/// Notes that `twin` let one call pass unchecked, and why.
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn skipped(twin: Twin, why: Skip) {
    let counts = match why {
        Skip::OverCap => &SKIPPED,
        Skip::Stopped => &STOPPED,
    };
    counts[twin as usize].fetch_add(1, Ordering::Relaxed);
}

/// Prints the counts to stderr, as `name=compared/skipped/stopped` per twin (`skipped` over
/// the twin's cap, `stopped` at a limit of the run), when a twin let a call pass unchecked,
/// and when `RDRA_ISH_SHADOW_STATS` is set and a twin saw a call; a command that explores
/// nothing, such as `files`, prints nothing. So a skip is never silent: the corpus harness
/// fails a case on a call skipped over a cap, and the CI's shadow job on a skip its runs do
/// not expect.
pub fn report() {
    let seen = |counts: &[AtomicU64]| counts.iter().any(|c| c.load(Ordering::Relaxed) > 0);
    let asked = std::env::var_os("RDRA_ISH_SHADOW_STATS").is_some();
    let unchecked = seen(&SKIPPED) || seen(&STOPPED);
    if !unchecked && !(asked && seen(&COMPARED)) {
        return;
    }
    let counts: Vec<String> = TWINS
        .iter()
        .map(|(twin, name)| {
            let at = *twin as usize;
            format!(
                "{name}={}/{}/{}",
                COMPARED[at].load(Ordering::Relaxed),
                SKIPPED[at].load(Ordering::Relaxed),
                STOPPED[at].load(Ordering::Relaxed)
            )
        })
        .collect();
    eprintln!("shadow: compared/skipped/stopped {}", counts.join(" "));
}
