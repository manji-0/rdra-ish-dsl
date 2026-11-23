//! What a run's state store takes in bytes, counted from the lengths and capacities of its
//! collections rather than read from the process, so a run stops at the same state on any
//! machine and with any number of worker threads.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::mem::size_of;

use crate::lifecycle::expr::State;
use crate::lifecycle::types::ActId;

use super::{Bfs, Edge};

/// A heap block for `n` bytes: the allocator rounds up to 16.
fn block(n: usize) -> u64 {
    n.div_ceil(16) as u64 * 16
}

/// A `Vec`'s buffer once `more` items are pushed: it doubles when full.
fn vec_bytes<T>(v: &Vec<T>, more: usize) -> u64 {
    let cap = if v.len() + more <= v.capacity() {
        v.capacity()
    } else {
        (v.capacity() * 2).max(v.len() + more).max(4)
    };
    (cap * size_of::<T>()) as u64
}

/// A `HashMap`'s table once `more` entries are inserted: one control byte and one entry per
/// bucket, at 7/8 load, doubling when full.
fn map_bytes<K, V, S: BuildHasher>(m: &HashMap<K, V, S>, more: usize) -> u64 {
    let buckets = |cap: usize| match cap {
        0 => 0,
        c if c < 8 => (c + 1).next_power_of_two(),
        c => (c * 8 / 7).next_power_of_two(),
    };
    let mut n = buckets(m.capacity());
    if m.len() + more > m.capacity() {
        n = (n * 2).max(4);
    }
    (n * (size_of::<(K, V)>() + 1)) as u64
}

/// One binding: its entry in `Bindings::list` and its key in `Bindings::index`.
pub(super) fn binding_bytes(params: &[u8]) -> u64 {
    2 * block(params.len())
        + size_of::<(ActId, Box<[u8]>)>() as u64
        + size_of::<(Box<[u8]>, u32)>() as u64
}

impl Bfs<'_> {
    /// The store once one more state is admitted: each state twice (in `states` and as a key
    /// of `seen`), its parent, every edge, and the bindings.
    pub(super) fn bytes_with_one_more(&self) -> u64 {
        let width = self.states.first().map_or(0, |s| s.0.len());
        self.kept_with(1) + map_bytes(&self.seen, 1) + (self.states.len() as u64 + 1) * block(width)
    }

    /// What the run's result keeps once the run ends and `seen` is dropped.
    pub(super) fn kept(&self) -> u64 {
        self.kept_with(0)
    }

    fn kept_with(&self, more: usize) -> u64 {
        let width = self.states.first().map_or(0, |s| s.0.len());
        vec_bytes::<State>(&self.states, more)
            + (self.states.len() + more) as u64 * block(width)
            + vec_bytes::<Option<u32>>(&self.parents, more)
            + vec_bytes::<Edge>(&self.edges, 0)
            + self.bindings.bytes
    }
}
