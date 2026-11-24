//! What a run keeps of its moves (#255, #296 item 2.1): per state the move that first reached
//! it, and on a run whose graph is read every move, as a target and a binding id grouped by
//! the state they leave. An edge does not store its source: BFS reads the states in order, so
//! the moves of each state are one run of the list, found from the index of its first edge.
//! A binding id takes one byte while the run has at most 256 bindings, two up to 65,536, and
//! four beyond, so an edge is 5, 6 or 8 bytes where it was 12.
//!
//! A run that stops once every check fails (`Caps::stop_when_all_violated`) decides its
//! checks and nothing else, so it keeps no edge list: each state's parent holds the source
//! state and the binding of the move that reached it, which is what a trace reads, and the
//! one other reader of such a run's moves, the vacuity of an after-assert, reads what the run
//! recorded as it went (`StepEnds`). `edges()` on such a run panics: an empty list would read
//! as "no moves".

use std::mem::size_of;

use crate::lifecycle::expr::StateSlice;
use crate::lifecycle::types::{ActId, EntId, Pred};
use crate::lifecycle::TransitionSystem;

use super::Edge;

#[cfg(feature = "shadow")]
mod reference;

/// Items in a buffer that grows to twice its items when full, four at the least, and to no
/// more: it holds `capacity() * size_of::<T>()` bytes, which `bytes_with` says before a push.
#[derive(Clone, Debug)]
pub(crate) struct Buf<T>(Vec<T>);

impl<T> Default for Buf<T> {
    fn default() -> Self {
        Buf(Vec::new())
    }
}

impl<T: Copy> Buf<T> {
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn push(&mut self, x: T) {
        if self.0.len() == self.0.capacity() {
            let room = (self.0.len() * 2).max(4);
            self.0.reserve_exact(room - self.0.len());
        }
        self.0.push(x);
    }

    pub(crate) fn get(&self, i: usize) -> T {
        self.0[i]
    }

    pub(crate) fn as_slice(&self) -> &[T] {
        &self.0
    }

    /// The buffer with room for `cap` items and none in it.
    fn with_capacity(cap: usize) -> Self {
        Buf(Vec::with_capacity(cap))
    }

    fn capacity(&self) -> usize {
        self.0.capacity()
    }

    /// The bytes the buffer takes once `more` items are pushed.
    pub(crate) fn bytes_with(&self, more: usize) -> u64 {
        let cap = if self.0.len() + more <= self.0.capacity() {
            self.0.capacity()
        } else {
            (self.0.capacity() * 2).max(self.0.len() + more).max(4)
        };
        (cap * size_of::<T>()) as u64
    }
}

/// Binding ids, each in the fewest bytes the largest one so far needs: one, two or four. The
/// ids of a run are numbered from 0 as they are first met, so the width widens at the same
/// id on every machine and thread count, and a wider buffer keeps the narrower one's room.
#[derive(Clone, Debug)]
pub(crate) enum Ids {
    One(Buf<u8>),
    Two(Buf<u16>),
    Four(Buf<u32>),
}

impl Default for Ids {
    fn default() -> Self {
        Ids::One(Buf::default())
    }
}

impl Ids {
    /// Bytes in one id.
    pub(crate) fn width(&self) -> usize {
        match self {
            Ids::One(_) => 1,
            Ids::Two(_) => 2,
            Ids::Four(_) => 4,
        }
    }

    pub(crate) fn push(&mut self, id: u32) {
        match self {
            Ids::One(b) => match u8::try_from(id) {
                Ok(id) => b.push(id),
                Err(_) => {
                    *self = Ids::Two(widen(b));
                    self.push(id);
                }
            },
            Ids::Two(b) => match u16::try_from(id) {
                Ok(id) => b.push(id),
                Err(_) => {
                    *self = Ids::Four(widen(b));
                    self.push(id);
                }
            },
            Ids::Four(b) => b.push(id),
        }
    }

    pub(crate) fn get(&self, i: usize) -> u32 {
        match self {
            Ids::One(b) => b.get(i).into(),
            Ids::Two(b) => b.get(i).into(),
            Ids::Four(b) => b.get(i),
        }
    }

    pub(crate) fn bytes_with(&self, more: usize) -> u64 {
        match self {
            Ids::One(b) => b.bytes_with(more),
            Ids::Two(b) => b.bytes_with(more),
            Ids::Four(b) => b.bytes_with(more),
        }
    }
}

/// `narrow`'s ids in a wider buffer with the same room.
fn widen<N: Copy, W: Copy + From<N>>(narrow: &Buf<N>) -> Buf<W> {
    let mut wide = Buf::with_capacity(narrow.capacity());
    wide.0.extend(narrow.as_slice().iter().map(|&x| W::from(x)));
    wide
}

/// Every move of a run, grouped by the state it leaves, in the order BFS found them.
#[derive(Clone, Debug, Default)]
pub(crate) struct EdgeList {
    /// Per state read, in order: the index of its first edge.
    first: Buf<u32>,
    to: Buf<u32>,
    binding: Ids,
}

impl EdgeList {
    fn len(&self) -> usize {
        self.to.len()
    }

    /// The first edge of the state after `states[at]`; `usize::MAX` past the last state read.
    fn first_after(&self, at: u32) -> usize {
        let next = at as usize + 1;
        match next < self.first.len() {
            true => self.first.get(next) as usize,
            false => usize::MAX,
        }
    }

    /// The state edge `k` leaves: the last state read whose first edge is at or before it.
    fn source_of(&self, k: usize) -> u32 {
        let read = self.first.as_slice();
        (read.partition_point(|&f| f as usize <= k) - 1) as u32
    }

    fn get(&self, k: usize) -> Edge {
        Edge {
            from: self.source_of(k),
            to: self.to.get(k),
            binding: self.binding.get(k),
        }
    }

    /// The edges of `states[at]`: none when it was not read.
    fn range_of(&self, at: u32) -> std::ops::Range<usize> {
        let at = at as usize;
        match at < self.first.len() {
            false => 0..0,
            true => {
                let end = match at + 1 < self.first.len() {
                    true => self.first.get(at + 1) as usize,
                    false => self.len(),
                };
                self.first.get(at) as usize..end
            }
        }
    }

    fn bytes_with(&self) -> u64 {
        self.first.bytes_with(0) + self.to.bytes_with(0) + self.binding.bytes_with(0)
    }
}

/// The parent of each state: the move that first reached it.
#[derive(Clone, Debug)]
enum Parents {
    /// On a run that keeps its edge list: the index of that edge, `INITIAL` for an initial
    /// state.
    Edge(Buf<u32>),
    /// On a run that keeps none: the state the move left, `INITIAL` for an initial state, and
    /// its binding.
    Step { from: Buf<u32>, binding: Ids },
}

/// The parent of an initial state.
const INITIAL: u32 = u32::MAX;

/// What a run keeps of its moves. Under the `shadow` feature it keeps the 12-byte edge list
/// and the parent edges as they were before #296 beside it, and holds every read to them.
#[derive(Clone, Debug)]
pub(crate) struct Steps {
    /// `None` on a run that keeps no edge list.
    edges: Option<EdgeList>,
    parents: Parents,
    /// Moves read, self-loops and repeats included: the edges a run with a list holds.
    moves: u64,
    #[cfg(feature = "shadow")]
    twin: reference::Steps,
}

impl Steps {
    /// The moves of a run that keeps its edge list when `graph`, else its parents alone.
    pub(crate) fn new(graph: bool) -> Self {
        Steps {
            edges: graph.then(EdgeList::default),
            parents: match graph {
                true => Parents::Edge(Buf::default()),
                false => Parents::Step {
                    from: Buf::default(),
                    binding: Ids::default(),
                },
            },
            moves: 0,
            #[cfg(feature = "shadow")]
            twin: reference::Steps::default(),
        }
    }

    /// Whether the run keeps its edge list.
    pub(crate) fn has_edges(&self) -> bool {
        self.edges.is_some()
    }

    /// Moves read, self-loops and repeats included.
    pub(crate) fn moves(&self) -> u64 {
        self.moves
    }

    /// Notes that the moves of `states[at]`, the next state in order, are read next.
    pub(crate) fn read(&mut self, at: usize) {
        if let Some(edges) = &mut self.edges {
            debug_assert_eq!(edges.first.len(), at, "states are read in order");
            edges.first.push(self.moves as u32);
        }
    }

    /// Records a move of the state being read, to `states[to]` under `binding`.
    pub(crate) fn push(&mut self, from: u32, to: u32, binding: u32) {
        if let Some(edges) = &mut self.edges {
            debug_assert_eq!(edges.source_of(edges.len()), from, "the state being read");
            edges.to.push(to);
            edges.binding.push(binding);
        }
        self.moves += 1;
        #[cfg(feature = "shadow")]
        self.twin.edges.push(Edge { from, to, binding });
        #[cfg(not(feature = "shadow"))]
        let _ = from;
    }

    /// Notes the parent of the next state admitted: `None` for an initial state, else the
    /// source and binding of the move about to be pushed, which reaches it.
    pub(crate) fn admit(&mut self, parent: Option<(u32, u32)>) {
        let edge = u32::try_from(self.moves).expect("edges fit in u32");
        match &mut self.parents {
            Parents::Edge(parents) => parents.push(parent.map_or(INITIAL, |_| edge)),
            Parents::Step { from, binding } => {
                let (f, b) = parent.unwrap_or((INITIAL, 0));
                from.push(f);
                binding.push(b);
            }
        }
        #[cfg(feature = "shadow")]
        self.twin.parents.push(parent.map(|_| edge));
    }

    /// The move that first reached `states[at]`; `None` for an initial state.
    pub(crate) fn parent(&self, at: u32) -> Option<Edge> {
        let i = at as usize;
        let found = match &self.parents {
            Parents::Edge(parents) => match parents.get(i) {
                INITIAL => None,
                k => self.edges.as_ref().map(|edges| edges.get(k as usize)),
            },
            Parents::Step { from, binding } => match from.get(i) {
                INITIAL => None,
                f => Some(Edge {
                    from: f,
                    to: at,
                    binding: binding.get(i),
                }),
            },
        };
        #[cfg(feature = "shadow")]
        self.twin.parent(at, found);
        found
    }

    /// The edge list; panics on a run that keeps none, since nothing that reads one may read
    /// such a run.
    pub(crate) fn edges(&self) -> Edges<'_> {
        let list = self.edges.as_ref().expect(
            "a run that stops once every check fails keeps no edge list: nothing may read its graph",
        );
        Edges {
            list,
            #[cfg(feature = "shadow")]
            twin: &self.twin.edges,
        }
    }

    /// Bytes in one binding id.
    pub(crate) fn binding_width(&self) -> usize {
        match (&self.edges, &self.parents) {
            (Some(edges), _) => edges.binding.width(),
            (None, Parents::Step { binding, .. }) => binding.width(),
            (None, Parents::Edge(_)) => unreachable!("a run without edges keeps its steps"),
        }
    }

    /// The bytes the parents and the edges take once `more` states are admitted.
    pub(crate) fn bytes_with(&self, more: usize) -> u64 {
        let parents = match &self.parents {
            Parents::Edge(parents) => parents.bytes_with(more),
            Parents::Step { from, binding } => from.bytes_with(more) + binding.bytes_with(more),
        };
        parents + self.edges.as_ref().map_or(0, EdgeList::bytes_with)
    }

    /// The 12-byte edge list the shadow build keeps beside the run's own, on any run.
    #[cfg(feature = "shadow")]
    pub(crate) fn twin_edges(&self) -> &[Edge] {
        &self.twin.edges
    }

    /// The bytes the parents and the edges took before #296, at the same lengths: the
    /// shadow build holds what a run is charged to never more than that, so a run that
    /// completed under a memory limit before still does.
    #[cfg(feature = "shadow")]
    pub(crate) fn bytes_before_with(&self, more: usize) -> u64 {
        self.twin.bytes_with(more)
    }
}

/// A run's edge list, read in order or by index. Each read yields an `Edge` by value; under
/// the `shadow` feature each is compared with the 12-byte list kept beside it.
#[derive(Clone, Copy)]
pub struct Edges<'a> {
    list: &'a EdgeList,
    #[cfg(feature = "shadow")]
    twin: &'a [Edge],
}

impl<'a> Edges<'a> {
    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.len() == 0
    }

    /// Edge `k`, in the order BFS found them.
    pub fn get(&self, k: usize) -> Edge {
        let e = self.list.get(k);
        self.check(k, e);
        e
    }

    pub fn last(&self) -> Option<Edge> {
        self.len().checked_sub(1).map(|k| self.get(k))
    }

    /// Every edge in the order BFS found them.
    pub fn iter(&self) -> EdgeIter<'a> {
        self.within(0..self.list.len(), 0)
    }

    /// The moves from `states[at]`, in the order BFS found them: none when the run did not
    /// read it.
    pub fn from_state(&self, at: u32) -> EdgeIter<'a> {
        self.within(self.list.range_of(at), at)
    }

    /// The edges `at`, whose first leaves `states[from]`.
    fn within(&self, at: std::ops::Range<usize>, from: u32) -> EdgeIter<'a> {
        EdgeIter {
            edges: *self,
            at,
            from,
            next: self.list.first_after(from),
        }
    }

    #[cfg_attr(not(feature = "shadow"), allow(unused_variables))]
    fn check(&self, k: usize, e: Edge) {
        #[cfg(feature = "shadow")]
        reference::check_edge(self.twin, k, e);
    }
}

impl PartialEq for Edges<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl std::fmt::Debug for Edges<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// `for e in run.edges()` yields each edge as a `Listed`, whose fields read as the `Edge`'s and
/// which `ComponentRun::step` takes by value, as it takes an `&Edge`.
impl<'a> IntoIterator for Edges<'a> {
    type Item = Listed;
    type IntoIter = std::iter::Map<EdgeIter<'a>, fn(Edge) -> Listed>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter().map(Listed)
    }
}

/// An edge of a run's list, as a `for` loop over `Edges` yields it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Listed(pub Edge);

impl std::ops::Deref for Listed {
    type Target = Edge;

    fn deref(&self) -> &Edge {
        &self.0
    }
}

/// What `ComponentRun::step` reads the binding of: an `&Edge`, or a `Listed` by value.
pub trait AnEdge {
    fn edge(&self) -> &Edge;
}

impl AnEdge for &Edge {
    fn edge(&self) -> &Edge {
        self
    }
}

impl AnEdge for Listed {
    fn edge(&self) -> &Edge {
        &self.0
    }
}

/// Edges of a run in order, their source tracked as the walk passes each state's first edge.
#[derive(Clone)]
pub struct EdgeIter<'a> {
    edges: Edges<'a>,
    at: std::ops::Range<usize>,
    /// The source of `at.start`, or a state at or before it.
    from: u32,
    /// The first edge of the state after `from`: where the source changes.
    next: usize,
}

impl Iterator for EdgeIter<'_> {
    type Item = Edge;

    #[inline]
    fn next(&mut self) -> Option<Edge> {
        let k = self.at.next()?;
        while k >= self.next {
            self.from += 1;
            self.next = self.edges.list.first_after(self.from);
        }
        let e = Edge {
            from: self.from,
            to: self.edges.list.to.get(k),
            binding: self.edges.list.binding.get(k),
        };
        self.edges.check(k, e);
        Some(e)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.at.size_hint()
    }

    /// The edges a state at a time, each state's targets and binding ids read as slices:
    /// what `map`, `filter`, `for_each` and the folds over a whole graph call.
    fn fold<B, F: FnMut(B, Edge) -> B>(self, init: B, mut f: F) -> B {
        let EdgeIter {
            edges,
            at,
            mut from,
            mut next,
        } = self;
        let mut acc = init;
        let mut k = at.start;
        while k < at.end {
            while k >= next {
                from += 1;
                next = edges.list.first_after(from);
            }
            let run = k..next.min(at.end);
            acc = match &edges.list.binding {
                Ids::One(b) => fold_state(edges, b.as_slice(), from, run.clone(), acc, &mut f),
                Ids::Two(b) => fold_state(edges, b.as_slice(), from, run.clone(), acc, &mut f),
                Ids::Four(b) => fold_state(edges, b.as_slice(), from, run.clone(), acc, &mut f),
            };
            k = run.end;
        }
        acc
    }
}

/// `f` over the edges `at`, all of which leave `states[from]`.
fn fold_state<T: Copy + Into<u32>, B>(
    edges: Edges<'_>,
    ids: &[T],
    from: u32,
    at: std::ops::Range<usize>,
    mut acc: B,
    f: &mut impl FnMut(B, Edge) -> B,
) -> B {
    let to = &edges.list.to.as_slice()[at.clone()];
    for ((k, &to), &b) in at.clone().zip(to).zip(&ids[at]) {
        let e = Edge {
            from,
            to,
            binding: b.into(),
        };
        edges.check(k, e);
        acc = f(acc, e);
    }
    acc
}

impl ExactSizeIterator for EdgeIter<'_> {}

/// Per after-assert step of a run that keeps no edge list: whether a move of its action was
/// read, and whether the target of one has rows of every entity the step leaves unbound. It
/// is what `ComponentRun::step_ends` scans the edge list for on a run that keeps one.
#[derive(Clone, Debug, Default)]
pub(crate) struct StepEnds {
    /// Per action: the steps it takes, as indices into `steps`.
    by_act: Vec<Vec<usize>>,
    /// Per distinct `(action, binders)` of the system's after-assert steps: the step, and
    /// what the run saw of it.
    steps: Vec<(ActId, Vec<EntId>, Ends)>,
}

/// What a run saw of the moves of one after-assert step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ends {
    /// A move of the step's action was read.
    pub ran: bool,
    /// The target of one has a row of every entity the step leaves unbound.
    pub with_rows: bool,
}

impl StepEnds {
    pub(crate) fn new(ts: &TransitionSystem) -> Self {
        let mut out = StepEnds {
            by_act: vec![Vec::new(); ts.actions.len()],
            steps: Vec::new(),
        };
        for step in ts.after.iter().flat_map(|a| a.steps.iter()) {
            let known = (out.steps.iter()).any(|(a, b, _)| *a == step.act && *b == step.binders);
            if !known {
                out.by_act[step.act.index()].push(out.steps.len());
                out.steps
                    .push((step.act, step.binders.clone(), Ends::default()));
            }
        }
        out
    }

    /// Notes a move of `act` into `to`.
    pub(crate) fn read(&mut self, ts: &TransitionSystem, act: ActId, to: &StateSlice) {
        for &k in &self.by_act[act.index()] {
            let (_, binders, ends) = &mut self.steps[k];
            ends.ran = true;
            if !ends.with_rows && ts.meets(binders, &Pred::True, to) {
                ends.with_rows = true;
            }
        }
    }

    /// What the run saw of the step of `act` that leaves `binders` unbound.
    pub(crate) fn of(&self, act: ActId, binders: &[EntId]) -> Ends {
        (self.steps.iter())
            .find(|(a, b, _)| *a == act && b == binders)
            .map(|(_, _, ends)| *ends)
            .expect("an after-assert step of the run's system")
    }
}

/// Kani harnesses (`cargo kani -p rdra-ish-core`).
#[cfg(kani)]
mod proofs {
    use super::*;

    /// `Buf::bytes_with` is what the buffer grows to: what it says before a push is what the
    /// buffer takes after it, and the buffer holds room for at most twice its items (four at
    /// the least), for every run of up to 17 pushes.
    #[kani::proof]
    #[kani::unwind(19)]
    fn a_buf_takes_what_bytes_with_said() {
        let pushes: usize = kani::any();
        kani::assume(pushes <= 17);
        let mut b: Buf<u32> = Buf::default();
        for i in 0..pushes {
            let said = b.bytes_with(1);
            b.push(i as u32);
            assert_eq!((b.capacity() * size_of::<u32>()) as u64, said);
            assert_eq!(b.bytes_with(0), said);
            assert!(b.capacity() <= (2 * b.len()).max(4));
        }
    }

    /// A `Vec`'s buffer once `more` items are pushed: what a run was charged before #296.
    fn vec_bytes<T>(v: &Vec<T>, more: usize) -> u64 {
        let cap = if v.len() + more <= v.capacity() {
            v.capacity()
        } else {
            (v.capacity() * 2).max(v.len() + more).max(4)
        };
        (cap * size_of::<T>()) as u64
    }

    /// For every run of up to 6 steps, each admitting a state (reached by a move pushed with
    /// it, or initial before any state is read), reading the next state, or pushing a move to
    /// any state with any binding id, with an edge list or without one: every edge and parent
    /// reads back as the 12-byte `Edge` the run kept before #296, and the bytes `Steps` takes
    /// with one state more are never more than the 8-byte parents and 12-byte edges of the
    /// `Vec`s it replaces at the same lengths, so a run that fit a memory limit still does.
    #[kani::proof]
    #[kani::unwind(8)]
    fn steps_read_back_and_take_no_more_than_before() {
        let graph: bool = kani::any();
        let mut steps = Steps::new(graph);
        let mut parents: Vec<Option<u32>> = Vec::new();
        let mut edges: Vec<Edge> = Vec::new();
        let (mut states, mut read) = (0u32, 0u32);
        for _ in 0..6 {
            let op: u8 = kani::any();
            if op == 0 {
                let parent = (read > 0).then(|| (read - 1, kani::any::<u32>()));
                steps.admit(parent);
                parents.push(parent.map(|_| edges.len() as u32));
                if let Some((from, binding)) = parent {
                    steps.push(from, states, binding);
                    edges.push(Edge {
                        from,
                        to: states,
                        binding,
                    });
                }
                states += 1;
            } else if op == 1 && read < states {
                steps.read(read as usize);
                read += 1;
            } else if read > 0 {
                let (to, binding): (u32, u32) = (kani::any(), kani::any());
                kani::assume(to < states);
                steps.push(read - 1, to, binding);
                edges.push(Edge {
                    from: read - 1,
                    to,
                    binding,
                });
            }
            assert!(steps.bytes_with(1) <= vec_bytes(&parents, 1) + vec_bytes(&edges, 0));
        }
        assert_eq!(steps.moves(), edges.len() as u64);
        for at in 0..states {
            assert_eq!(
                steps.parent(at),
                parents[at as usize].map(|k| edges[k as usize])
            );
        }
        if graph {
            let list = steps.edges();
            for (k, e) in edges.iter().enumerate() {
                assert_eq!(list.get(k), *e);
            }
        }
    }
}
