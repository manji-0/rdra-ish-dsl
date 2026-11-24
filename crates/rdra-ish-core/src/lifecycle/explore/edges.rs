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

use super::segments::Rows;
use super::Edge;

#[cfg(feature = "shadow")]
mod reference;

/// Items in fixed-size segments (`segments::Rows`, #296 2.2): the first doubles from four
/// items up to a segment, and every later one is allocated whole, so growing copies no full
/// buffer, and `bytes_with` says before a push what the buffer takes after it.
#[derive(Clone, Debug)]
pub(crate) struct Buf<T>(Rows<T>);

impl<T: Copy> Default for Buf<T> {
    fn default() -> Self {
        Buf(Rows::new(1))
    }
}

impl<T: Copy> Buf<T> {
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn push(&mut self, x: T) {
        self.0.push(&[x]);
    }

    pub(crate) fn get(&self, i: usize) -> T {
        self.0.row(i)[0]
    }

    /// The first index whose item `pred` rejects, the items being those it accepts first.
    pub(crate) fn partition_point(&self, pred: impl Fn(T) -> bool) -> usize {
        let (mut lo, mut hi) = (0, self.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match pred(self.get(mid)) {
                true => lo = mid + 1,
                false => hi = mid,
            }
        }
        lo
    }

    /// The bytes the buffer takes once `more` items are pushed: its segments, and with
    /// `list` the list of them, which the harnesses leave out to compare the segments with
    /// the one `Vec` each buffer was before #296.
    pub(crate) fn bytes(&self, more: usize, list: bool) -> u64 {
        let (items, segments) = self.0.parts_with(more);
        items + if list { segments } else { 0 }
    }

    /// The items as `W`, a segment at a time, in the room these had (`Rows::map`).
    fn widen<W: Copy + From<T>>(self) -> Buf<W> {
        Buf(self.0.map(W::from))
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
                    *self = Ids::Two(std::mem::take(b).widen());
                    self.push(id);
                }
            },
            Ids::Two(b) => match u16::try_from(id) {
                Ok(id) => b.push(id),
                Err(_) => {
                    *self = Ids::Four(std::mem::take(b).widen());
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

    pub(crate) fn bytes(&self, more: usize, list: bool) -> u64 {
        match self {
            Ids::One(b) => b.bytes(more, list),
            Ids::Two(b) => b.bytes(more, list),
            Ids::Four(b) => b.bytes(more, list),
        }
    }
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

    /// The state edge `k` leaves: the last state read whose first edge is at or before it.
    fn source_of(&self, k: usize) -> u32 {
        (self.first.partition_point(|f| f as usize <= k) - 1) as u32
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

    fn bytes(&self, list: bool) -> u64 {
        self.first.bytes(0, list) + self.to.bytes(0, list) + self.binding.bytes(0, list)
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
        self.bytes(more, true)
    }

    /// `bytes_with`, with the lists of segments when `list`.
    fn bytes(&self, more: usize, list: bool) -> u64 {
        let parents = match &self.parents {
            Parents::Edge(parents) => parents.bytes(more, list),
            Parents::Step { from, binding } => from.bytes(more, list) + binding.bytes(more, list),
        };
        parents + self.edges.as_ref().map_or(0, |e| e.bytes(list))
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
}

impl Iterator for EdgeIter<'_> {
    type Item = Edge;

    fn next(&mut self) -> Option<Edge> {
        let k = self.at.next()?;
        let first = &self.edges.list.first;
        while (self.from as usize + 1) < first.len() && first.get(self.from as usize + 1) as usize <= k
        {
            self.from += 1;
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

    /// `Buf::partition_point` is the slice's: for every ascending run of up to 9 items across
    /// segments of 4, and every bound, the index of the first item past the bound.
    #[kani::proof]
    #[kani::unwind(11)]
    fn a_buf_partition_point_is_the_slices() {
        let pushes: usize = kani::any();
        kani::assume(pushes <= 9);
        let (mut b, mut v): (Buf<u32>, Vec<u32>) = (Buf::default(), Vec::new());
        let mut last = 0u32;
        for _ in 0..pushes {
            let step: u32 = kani::any();
            kani::assume(step <= 2);
            last += step;
            b.push(last);
            v.push(last);
        }
        let bound: u32 = kani::any();
        assert_eq!(
            b.partition_point(|x| x <= bound),
            v.partition_point(|&x| x <= bound)
        );
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
            // The segments, their lists left out, against the one `Vec` each buffer was.
            assert!(steps.bytes(1, false) <= vec_bytes(&parents, 1) + vec_bytes(&edges, 0));
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
