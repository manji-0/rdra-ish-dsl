//! The `shadow` feature (#275): each fast function that has a reference twin also runs the
//! twin and panics, naming the call, when the two differ. A run of a `shadow` build over the
//! samples and the corpus holds every fast path to its reference on every call it makes.
//!
//! The twins live in files of their own, so a diff shows whether a change touched one:
//!
//! | Fast path | Twin |
//! |---|---|
//! | `each_planned` (`explore/moves.rs`, #253) | `explore/moves/reference.rs`: `bindings` filtered by `pre_guards_hold` |
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
//!
//! A `shadow` build also runs the checks `shadow_assert!` makes, as a debug build does: each
//! reader of a run's check results reads only checks the run evaluated (#251), and a steady
//! action's step leaves the inputs of a reduced cone as they were (`moves_of_into`).
//!
//! With `RDRA_ISH_SHADOW_STATS` set, the process prints how many calls each twin compared
//! and how many it skipped when it exits, so a run that compared nothing shows as such.

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
}

const TWINS: [(Twin, &str); 25] = [
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
];

static COMPARED: [AtomicU64; TWINS.len()] = [const { AtomicU64::new(0) }; TWINS.len()];
static SKIPPED: [AtomicU64; TWINS.len()] = [const { AtomicU64::new(0) }; TWINS.len()];

/// Notes that `twin` compared one call.
pub(crate) fn compared(twin: Twin) {
    COMPARED[twin as usize].fetch_add(1, Ordering::Relaxed);
}

/// Notes that `twin` let one call pass unchecked.
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn skipped(twin: Twin) {
    SKIPPED[twin as usize].fetch_add(1, Ordering::Relaxed);
}

/// Prints the counts to stderr when `RDRA_ISH_SHADOW_STATS` is set and a twin saw a call; a
/// command that explores nothing, such as `files`, prints nothing.
pub fn report() {
    let seen = |counts: &[AtomicU64]| counts.iter().any(|c| c.load(Ordering::Relaxed) > 0);
    if std::env::var_os("RDRA_ISH_SHADOW_STATS").is_none() || !(seen(&COMPARED) || seen(&SKIPPED)) {
        return;
    }
    let counts: Vec<String> = TWINS
        .iter()
        .map(|(twin, name)| {
            let at = *twin as usize;
            format!(
                "{name}={}/{}",
                COMPARED[at].load(Ordering::Relaxed),
                SKIPPED[at].load(Ordering::Relaxed)
            )
        })
        .collect();
    eprintln!("shadow: compared/skipped {}", counts.join(" "));
}
