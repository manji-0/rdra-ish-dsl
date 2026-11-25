//! The reference twin of `Steps` (#255, #296 item 2.1): every move as a 12-byte
//! `Edge { from, to, binding }` and each state's parent as the index of its edge, as a run
//! kept them before. Under the `shadow` feature every run keeps one beside its `Steps`, a run
//! without an edge list included, and holds each edge and parent it reads to it. It is not
//! counted in `bytes_with`, so the memory limit stops a run at the same state with it or
//! without it; `bytes_with` here is what the run was charged for its moves before.

use std::mem::size_of;

use crate::lifecycle::explore::Edge;

#[derive(Clone, Debug, Default)]
pub(super) struct Steps {
    pub(super) edges: Vec<Edge>,
    pub(super) parents: Vec<Option<u32>>,
}

impl Steps {
    /// Holds the parent `Steps` read for `states[at]` to the edge this twin kept.
    pub(super) fn parent(&self, at: u32, found: Option<Edge>) {
        let want = self.parents[at as usize].map(|k| self.edges[k as usize]);
        assert!(
            found == want,
            "shadow: Steps::parent({at}): {found:?}, reference {want:?}"
        );
        crate::shadow::compared(crate::shadow::Twin::Edges);
    }

    /// The bytes the parents and the edges took before #296 once `more` states are admitted:
    /// the two `Vec`s, doubling when full.
    pub(super) fn bytes_with(&self, more: usize) -> u64 {
        vec_bytes(&self.parents, more) + vec_bytes(&self.edges, 0)
    }
}

/// Holds edge `k` as `Edges` read it to the one this twin kept.
pub(super) fn check_edge(twin: &[Edge], k: usize, e: Edge) {
    assert!(
        twin.get(k) == Some(&e),
        "shadow: Edges::get({k}): {e:?}, reference {:?}",
        twin.get(k)
    );
    crate::shadow::compared(crate::shadow::Twin::Edges);
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
