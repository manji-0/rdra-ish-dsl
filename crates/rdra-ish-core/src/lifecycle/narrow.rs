//! The two narrowings of the lifecycle layer, checked. `clippy::cast_possible_truncation` is
//! denied here (claims N1 and N2 of docs/theory/claims.md: a fixed-width integer that wraps
//! gives a wrong verdict); an `as` that narrows goes through one of these, which panic where an
//! `as` would wrap, or carries an `#[allow]` whose comment proves its bound.

/// A number of a state, an edge, a component, a block, or a row of a run's table, as a `u32`.
/// A run admits at most `MOST_STATES = u32::MAX` states (`explore/bfs.rs::admit_packed`) and
/// numbers its edges with `edge_number`, which stops below `u32::MAX` (`explore/edges.rs`);
/// every other count numbered here is at most one of those.
#[inline]
#[track_caller]
pub(crate) fn id32(n: usize) -> u32 {
    u32::try_from(n).expect("a state, edge, or block number is below u32::MAX")
}

/// A slot, a parameter, a binder, or a value, as a `u8`. An entity has at most 255 slots
/// (`Bounds::instances` is a `NonZeroU8`), an action or a rule binds at most 255, and a domain
/// holds at most 256 values, `0..=255` (`DomainTooLarge`).
#[inline]
#[track_caller]
pub(crate) fn id8(n: usize) -> u8 {
    u8::try_from(n).expect("a slot, parameter, binder, or value index is at most 255")
}
