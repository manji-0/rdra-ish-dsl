//! The `perf` feature (#256, #296): where a run's time and memory go, printed to stderr.
//!
//! A build with `--features perf` and `RDRA_PERF=1` in the environment prints
//!
//! - a `perf-pass` line when each pass of `verify` ends (`pass`): its wall and process CPU
//!   time, the allocations it made, and the live heap and the memory pool at their peaks so
//!   far;
//! - a `perf-run` line when each BFS run ends (`lifecycle::explore`): the pass and label it ran
//!   under, its states, edges, width, the bits its declared domains need, what its store keeps,
//!   and its thread CPU time. `RDRA_PERF=2` adds what takes a pass over the run's states or
//!   edges: the bits the reached values need, the bytes a state changes from its parent, the
//!   distinct `(from, to)` pairs, and hashes of the states, edges and bindings, which tell
//!   two runs of one graph apart from two runs of equal size.
//!
//! The counting allocator (`Counting`) is the CLI's global allocator in such a build.
//! `scripts/perf-tables.py` runs the samples and prints the tables of #296 from the lines.
//!
//! Without the feature every function here is an empty one the compiler drops, so a release
//! build prints and counts nothing.
//!
//! A `perf` build also reads these switches (`stats`), each a stderr line for a developer:
//!
//! | Variable | Line |
//! |---|---|
//! | `RDRA_ISH_G_T_STATS` | `g-t:` per reduced cone run up to its group `G_T`: per entity, whether it moves, and why it stays fixed |
//! | `RDRA_ISH_S_UP_TO_H_STATS` | `s-up-to-h:` per sub-lifecycle run up to `H`: where the plain run stopped, the orbits, the entities left out and those that move, then the classes |
//! | `RDRA_ISH_DIVERGENCE_STATS` | no line of its own: the `mixed` and `sensitive` counts of a reduction's divergence marks, which the `RDRA_ISH_WF_REDUCED_STATS` line prints |

/// Whether the developer switch `var` is set. Only a `perf` build reads it.
pub(crate) fn stats(var: &str) -> bool {
    cfg!(feature = "perf") && std::env::var_os(var).is_some()
}

#[cfg(feature = "perf")]
mod on;
#[cfg(feature = "perf")]
pub use on::*;

#[cfg(not(feature = "perf"))]
mod off {
    /// Ends the pass before and starts `name` (`verify`'s passes).
    #[inline(always)]
    pub fn pass(_name: &'static str) {}

    /// Labels the runs this thread starts until the guard drops.
    #[inline(always)]
    pub fn label(_name: &'static str) -> Label {
        Label
    }

    /// Restores the label before it when dropped.
    pub struct Label;
}
#[cfg(not(feature = "perf"))]
pub use off::*;
