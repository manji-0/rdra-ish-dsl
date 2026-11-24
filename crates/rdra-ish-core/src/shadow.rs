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
//! | a `QuotientCache` hit (`reduce/cache.rs`, #252) | `Classes::of` itself, through a cache that holds nothing |
//! | the reduced step's class moves (`moves_of_into`, #252) | `reduce/reference.rs`: `moves_from` |
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
}

const TWINS: [(Twin, &str); 7] = [
    (Twin::EachEnabled, "each_enabled"),
    (Twin::Seen, "seen"),
    (Twin::Source, "source"),
    (Twin::UnchangedRow, "unchanged_row"),
    (Twin::Partition, "quotient_blocks"),
    (Twin::CacheHit, "cache_hit"),
    (Twin::ClassMoves, "class_moves"),
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
