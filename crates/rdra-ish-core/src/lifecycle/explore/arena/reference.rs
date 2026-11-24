//! The reference twin of `Seen` (#240, #275): the index of each state in an ordered map of
//! its bytes, with no hash. Under the `shadow` feature every `Seen` keeps one beside its
//! table and holds each `find` and `insert` to it. It is not counted in `bytes_with`, so the
//! memory limit stops a run at the same state with it or without it.

use std::collections::BTreeMap;

use crate::lifecycle::expr::StateSlice;

#[derive(Default)]
pub(super) struct Seen(BTreeMap<Box<[u8]>, u32>);

impl Seen {
    /// The index of `s`, if it is there.
    pub(super) fn find(&self, s: &StateSlice) -> Option<u32> {
        self.0.get(&s.0).copied()
    }

    /// Notes `s` at index `at`; false when the map held `s` already.
    pub(super) fn insert(&mut self, s: &StateSlice, at: u32) -> bool {
        self.0.insert(s.0.into(), at).is_none()
    }
}
