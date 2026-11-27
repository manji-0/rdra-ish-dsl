//! What a run's state store takes in bytes, counted from the lengths and capacities of its
//! collections rather than read from the process, so a run stops at the same state on any
//! machine and with any number of worker threads.

use crate::lifecycle::narrow::id32;
use std::mem::size_of;

use crate::lifecycle::types::ActId;

use super::segments::SHIFT;
use super::{Bfs, ComponentRun, Layers};

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

/// How a run keeps its moves (`edges::Steps`), beside the bytes of its states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    /// Bytes in one binding id: 1 while the run has at most 256 bindings, 2 up to 65,536,
    /// else 4.
    pub binding: usize,
    /// The run keeps its edge list: every run but one that stops once every check fails.
    pub graph: bool,
}

/// The bytes the store of a run of `shape` takes once it holds `states` states of `width`
/// bytes and `edges` edges, with `seen`. With `b` bytes in a binding id, a run that keeps its
/// edge list takes `(w + 4 + 4 + 4) * room(n) + 3 * list(n) + 5 * buckets(n) + (4 + b) *
/// room(e) + 2 * list(e)`, and one that keeps none `(w + 4 + b + 4) * room(n) + 3 * list(n)
/// + 5 * buckets(n)`. `room(x)` is what a buffer of segments holds room for
/// (`segments::Rows`): the least power of two of at least `x` (4 at the least) up to
/// `2 * SEG`, and whole segments of `SEG` beyond; `list(x)` is the list of those whole
/// segments, 24 bytes a slot, doubling from four, and nothing up to `2 * SEG`; `buckets(n)`
/// is the least power of two of at least `n * 8 / 7`. Each state is `w` bytes in `states`,
/// packed: each cell in the bits its domain needs, `ceil(log2(size))`, the sum rounded up to
/// whole bytes (#296 item 2.3);
/// with an edge list it has a 4-byte parent edge and the 4-byte index of its first edge, and
/// an edge is a 4-byte target and a binding id; without one its parent is a 4-byte source and
/// a binding id. `seen` holds a 4-byte index and a control byte per bucket, and the 4 bytes
/// per state more stand for the bindings and the layers. It models
/// `Bfs::bytes_with_one_more` at a run's end and is never under what that counts
/// (`store_bytes_models_the_store_a_run_counts`). It is computed from the counts alone, so it
/// is the same on every machine and thread count. Saturates at `u64::MAX`.
pub fn store_bytes(states: u64, width: usize, edges: u64, shape: Shape) -> u64 {
    store_bytes_in::<SHIFT>(true, states, width, edges, shape)
}

/// `store_bytes` with segments of `2^S` rows, and with the lists of segments when `lists`:
/// what the harnesses read at the segment of a build, where Kani's own build has segments of
/// 4, and without the lists, which a doubling `Vec` had no counterpart of. Segments are
/// counted by shifts, not division, which keeps the harnesses fast.
fn store_bytes_in<const S: u32>(
    lists: bool,
    states: u64,
    width: usize,
    edges: u64,
    shape: Shape,
) -> u64 {
    let (n, e) = (u128::from(states), u128::from(edges));
    let (per_state, per_edge, edge_buffers) = coefficients(shape);
    let list = |x: u128| if lists { list::<S>(x) } else { 0 };
    // Each term fits in u128; the product with a width near `usize::MAX` and the sums need not.
    let total = (width as u128 + per_state + 4)
        .saturating_mul(room::<S>(n))
        .saturating_add(3 * list(n))
        .saturating_add(5 * buckets(n))
        .saturating_add(per_edge.saturating_mul(room::<S>(e)))
        .saturating_add(edge_buffers * list(e));
    u64::try_from(total).unwrap_or(u64::MAX)
}

/// The bytes per state besides the state, per edge, and the buffers of edges a run of
/// `shape` keeps.
fn coefficients(shape: Shape) -> (u128, u128, u128) {
    let b = shape.binding as u128;
    match shape.graph {
        true => (4 + 4, 4 + b, 2),
        false => (4 + b, 0, 0),
    }
}

/// The rows a buffer of segments of `2^S` holds room for once it holds `x`: the least power
/// of two of at least `x` (4 at the least) up to `2^(S+1)`, then whole segments.
fn room<const S: u32>(x: u128) -> u128 {
    let doubled = 2 << S;
    match x <= doubled {
        true => x.max(4).next_power_of_two(),
        false => doubled + (whole::<S>(x) << S),
    }
}

/// The bytes of the list of the whole segments that `x` rows take: 24 a slot, doubling from
/// four, and none up to `2^(S+1)` rows.
fn list<const S: u32>(x: u128) -> u128 {
    match x > 2 << S {
        true => 24 * whole::<S>(x).max(4).next_power_of_two(),
        false => 0,
    }
}

/// The whole segments of `2^S` rows past the doubling ones that `x > 2^(S+1)` rows take,
/// counted by shifts, not division, which keeps the harnesses fast.
fn whole<const S: u32>(x: u128) -> u128 {
    (x - (2 << S) + (1 << S) - 1) >> S
}

/// The buckets `seen` holds for `n` states: the least power of two of at least `n * 8 / 7`.
fn buckets(n: u128) -> u128 {
    (n * 8).div_ceil(7).max(4).next_power_of_two()
}

/// What a run held when the memory limit stopped it, to say what finishing would take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    /// The store with the state that did not fit (`Bfs::bytes_with_one_more`).
    pub bytes: u64,
    /// Bytes in one state as the store keeps it, packed into its cells' bits.
    pub width: usize,
    /// How the run keeps its moves.
    pub shape: Shape,
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
        let edges = u64::try_from(edges).unwrap_or(u64::MAX);
        store_bytes(states, self.width, edges, self.shape)
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
                (decl.domain.size(self.ts) as u64).checked_pow(id32(self.ts.slots_of(&decl.kind)))
            })
            .try_fold(1u64, |bound, size| bound.checked_mul(size?));
        Held {
            bytes,
            width: self.states.packed_width(),
            shape: Shape {
                binding: self.steps.binding_width(),
                graph: self.steps.has_edges(),
            },
            edges: self.steps.moves(),
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
            + self.steps.bytes_with(more)
            + vec_bytes::<u32>(&self.orbit_self, 0)
            + self.perms.as_ref().map_or(0, |p| p.bytes())
            + self.bindings.bytes
            + self.layers.bytes()
    }

    /// `bytes_with_one_more` with the states, parents and edges as a run kept them before
    /// #296, in doubling `Vec`s: what the shadow build holds the store, its lists of segments
    /// left out, to.
    #[cfg(feature = "shadow")]
    pub(super) fn bytes_before_with_one_more(&self) -> u64 {
        let states = (self.states.len() + 1).max(4).next_power_of_two() * self.states.width();
        states as u64
            + self.steps.bytes_before_with(1)
            + vec_bytes::<u32>(&self.orbit_self, 0)
            + self.perms.as_ref().map_or(0, |p| p.bytes())
            + self.bindings.bytes
            + self.layers.bytes()
            + self.seen.bytes_with(1)
    }

    /// The lists of segments in `bytes_with_one_more`: 24 bytes a slot, past `2 * SEG` rows
    /// of a buffer, which a doubling `Vec` had no counterpart of.
    #[cfg(feature = "shadow")]
    pub(super) fn lists_with_one_more(&self) -> u64 {
        self.states.list_bytes_with(1) + self.steps.list_bytes_with(1)
    }
}

impl ComponentRun {
    /// What the result keeps, in the bytes `Caps::memory` counts.
    pub fn kept_bytes(&self) -> u64 {
        self.kept
    }

    /// Bytes in one state as the store keeps it, packed into its cells' bits.
    pub fn state_width(&self) -> usize {
        self.states.packed_width()
    }

    /// How the run keeps its moves.
    pub fn shape(&self) -> Shape {
        Shape {
            binding: self.steps.binding_width(),
            graph: self.steps.has_edges(),
        }
    }
}

/// Kani harnesses (`cargo kani -p rdra-ish-core`).
#[cfg(kani)]
mod proofs {
    use super::*;

    /// A shape a run can have: a binding id of 1, 2 or 4 bytes, with or without edges.
    fn any_shape() -> Shape {
        let binding: usize = kani::any();
        kani::assume(binding == 1 || binding == 2 || binding == 4);
        Shape {
            binding,
            graph: kani::any(),
        }
    }

    /// `store_bytes` neither overflows nor panics on any input.
    #[kani::proof]
    fn store_bytes_never_overflows() {
        store_bytes(kani::any(), kani::any(), kani::any(), any_shape());
    }

    /// Each term of `store_bytes` only grows: more rows never take less room, a longer list or
    /// more buckets, at the build's segments and at Kani's, and a wider binding id or an edge
    /// list never takes fewer bytes per state, per edge or buffers of edges. `store_bytes`
    /// adds and multiplies these terms, all at least 0, saturating, so a store of more states,
    /// wider states, more edges, wider binding ids, or with an edge list where the other has
    /// none counts at least as many bytes, and past `u64` the count stays at `u64::MAX`.
    /// (A harness of `store_bytes` itself took past 30 minutes on segments.)
    #[kani::proof]
    fn store_bytes_only_grows() {
        let (x, y): (u128, u128) = (kani::any(), kani::any());
        kani::assume(x <= y && y <= u128::from(u64::MAX));
        assert!(room::<2>(x) <= room::<2>(y) && room::<14>(x) <= room::<14>(y));
        assert!(list::<2>(x) <= list::<2>(y) && list::<14>(x) <= list::<14>(y));
        assert!(buckets(x) <= buckets(y));
        let (s, t) = (any_shape(), any_shape());
        kani::assume(s.binding <= t.binding && (!s.graph || t.graph));
        let ((a, b, c), (d, e, f)) = (coefficients(s), coefficients(t));
        assert!(a <= d && b <= e && c <= f);
    }

    /// Without the lists of segments, the model of #296 never counts more than the one before
    /// it, which charged an 8-byte parent per state and a 12-byte edge in doubling `Vec`s: the
    /// rows of a cone that the old model said fits still do. The lists, 24 bytes a slot past
    /// `2 * SEG` rows, can make it count more at a power of two (the owner accepted this).
    #[kani::proof]
    fn store_bytes_is_at_most_the_model_before() {
        let (n, w, e): (u64, usize, u64) = (kani::any(), kani::any(), kani::any());
        let before = |n: u64, w: usize, e: u64| {
            let p2 = |x: u128| x.max(4).next_power_of_two();
            let n = u128::from(n);
            let total = (w as u128 + 8 + 4)
                .saturating_mul(p2(n))
                .saturating_add(5 * p2((n * 8).div_ceil(7)))
                .saturating_add(12 * p2(u128::from(e)));
            u64::try_from(total).unwrap_or(u64::MAX)
        };
        assert!(store_bytes_in::<14>(false, n, w, e, any_shape()) <= before(n, w, e));
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
