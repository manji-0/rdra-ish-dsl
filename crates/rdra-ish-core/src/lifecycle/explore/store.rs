//! What a run's state store takes in bytes, counted from the lengths and capacities of its
//! collections rather than read from the process, so a run stops at the same state on any
//! machine and with any number of worker threads.

use std::mem::size_of;

use crate::lifecycle::types::ActId;

use super::{Bfs, ComponentRun, Edge, Layers};

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

/// One binding: its entry in `Bindings::list` and its key in `Bindings::index`.
pub(super) fn binding_bytes(params: &[u8]) -> u64 {
    2 * block(params.len())
        + size_of::<(ActId, Box<[u8]>)>() as u64
        + size_of::<(Box<[u8]>, u32)>() as u64
}

/// The bytes the store of a run takes once it holds `states` states of `width` bytes and
/// `edges` edges, with `seen`: `(w + 8 + 4) * p2(n) + 5 * buckets(n) + 12 * p2(e)`, where `p2(x)`
/// is the least power of two of at least `x` (4 at the least) and `buckets(n)` the least power
/// of two of at least `n * 8 / 7`. Each state is `w` bytes in `states` and an 8-byte parent,
/// `seen` holds a 4-byte index and a control byte per bucket, and an edge is 12 bytes; the 4
/// bytes per state more stand for the bindings and the layers. It models
/// `Bfs::bytes_with_one_more` at a run's end; on the sample cones of a thousand states and
/// more, at `check` and `verify` bounds, it stood 0.9% to 5.2% over what that counts and never
/// under. Saturates at `u64::MAX`.
pub fn store_bytes(states: u64, width: usize, edges: u64) -> u64 {
    let p2 = |x: u128| x.max(4).next_power_of_two();
    let n = u128::from(states);
    // Each term fits in u128; the product with a width near `usize::MAX` and the sums need not.
    let total = (width as u128 + 8 + 4)
        .saturating_mul(p2(n))
        .saturating_add(5 * p2((n * 8).div_ceil(7)))
        .saturating_add(12 * p2(u128::from(edges)));
    u64::try_from(total).unwrap_or(u64::MAX)
}

/// What a run held when the memory limit stopped it, to say what finishing would take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    /// The store with the state that did not fit (`Bfs::bytes_with_one_more`).
    pub bytes: u64,
    /// Bytes in one state.
    pub width: usize,
    /// Moves found so far.
    pub edges: u64,
    /// The most states the system can have: the product of its variables' domains. `None`
    /// when that passes `u64`.
    pub bound: Option<u64>,
    /// The states first reached at each depth.
    pub layers: Layers,
}

impl Held {
    /// The store each state took, the one that did not fit counted, to the nearest byte.
    pub fn bytes_per_state(&self, states: usize) -> u64 {
        let n = states as u64 + 1;
        (self.bytes + n / 2) / n
    }

    /// What the store takes once the run holds `states` states, at the moves per expanded
    /// state seen so far. Early layers have fewer moves per state than later ones, so this
    /// is low on that part.
    pub fn bytes_for(&self, states: u64) -> u64 {
        let expanded = self.layers.expanded_states().max(1);
        let edges = (u128::from(self.edges) * u128::from(states)).div_ceil(u128::from(expanded));
        store_bytes(states, self.width, u64::try_from(edges).unwrap_or(u64::MAX))
    }
}

impl Bfs<'_> {
    /// What the run holds now, `bytes` being what its store would take with one more state.
    pub(super) fn held(&self, bytes: u64) -> Held {
        let bound = self
            .ts
            .var_ids()
            .map(|v| {
                let decl = self.ts.var(v);
                (decl.domain.size(self.ts) as u64).checked_pow(self.ts.slots_of(&decl.kind) as u32)
            })
            .try_fold(1u64, |bound, size| bound.checked_mul(size?));
        Held {
            bytes,
            width: self.ts.width,
            edges: self.edges.len() as u64,
            bound,
            layers: self.layers.clone(),
        }
    }

    /// The store once one more state is admitted: each state once, in `states`, its index in
    /// `seen`, its parent, every edge, and the bindings.
    pub(super) fn bytes_with_one_more(&self) -> u64 {
        self.kept_with(1) + self.seen.bytes_with(1)
    }

    /// What the run's result keeps once the run ends and `seen` is dropped.
    pub(super) fn kept(&self) -> u64 {
        self.kept_with(0)
    }

    fn kept_with(&self, more: usize) -> u64 {
        self.states.bytes_with(more)
            + vec_bytes::<Option<u32>>(&self.parents, more)
            + vec_bytes::<Edge>(&self.edges, 0)
            + vec_bytes::<u32>(&self.orbit_self, 0)
            + self.bindings.bytes
            + self.layers.bytes()
    }
}

impl ComponentRun {
    /// What the result keeps, in the bytes `Caps::memory` counts.
    pub fn kept_bytes(&self) -> u64 {
        self.kept
    }

    /// Bytes in one state.
    pub fn state_width(&self) -> usize {
        self.states.width()
    }
}

/// Kani harnesses (`cargo kani -p rdra-ish-core`).
#[cfg(kani)]
mod proofs {
    use super::*;

    /// `vec_bytes` is the growth of a real `Vec` of edges: what it says before a push is what
    /// the buffer takes after it, for every run of up to 17 pushes.
    #[kani::proof]
    #[kani::unwind(19)]
    fn vec_bytes_is_what_a_vec_of_edges_grows_to() {
        let pushes: usize = kani::any();
        kani::assume(pushes <= 17);
        let mut v: Vec<Edge> = Vec::new();
        for _ in 0..pushes {
            let said = vec_bytes(&v, 1);
            v.push(Edge {
                from: 0,
                to: 0,
                binding: 0,
            });
            assert_eq!((v.capacity() * size_of::<Edge>()) as u64, said);
            assert_eq!(vec_bytes(&v, 0), said);
        }
    }

    /// `store_bytes` neither overflows nor panics on any input.
    #[kani::proof]
    fn store_bytes_never_overflows() {
        store_bytes(kani::any(), kani::any(), kani::any());
    }

    /// A store of more states, wider states, or more edges never counts as fewer bytes: past
    /// `u64` the count stays at `u64::MAX` and does not wrap to a small one.
    #[kani::proof]
    fn store_bytes_only_grows() {
        let (n, m): (u64, u64) = (kani::any(), kani::any());
        let (w, v): (usize, usize) = (kani::any(), kani::any());
        let (e, f): (u64, u64) = (kani::any(), kani::any());
        kani::assume(n <= m && w <= v && e <= f);
        assert!(store_bytes(n, w, e) <= store_bytes(m, v, f));
    }

    /// A heap block holds what was asked, in whole 16-byte units, and wastes under 16 bytes.
    #[kani::proof]
    fn a_block_rounds_up_to_16() {
        let n: usize = kani::any();
        kani::assume(n <= usize::MAX - 16);
        let b = block(n);
        assert!(b % 16 == 0 && b >= n as u64 && b - (n as u64) < 16);
    }
}
