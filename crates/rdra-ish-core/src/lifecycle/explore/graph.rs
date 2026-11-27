//! The graph both engines hand to findings, and the run's edge, path and trace readers.

use crate::lifecycle::expr::State;
use crate::lifecycle::narrow::id32;
use crate::lifecycle::reduce::Step;
use crate::lifecycle::trace::{ReplayError, Trace};
use crate::lifecycle::types::ActId;
use crate::lifecycle::TransitionSystem;

use super::{AnEdge, ComponentRun, Edge, Edges, Ends, Graph, GraphEdges, Reach, States};
use crate::lifecycle::types::{EntId, Pred};

impl<'a> GraphEdges<'a> {
    fn iter(self) -> GraphEdgeIter<'a> {
        match self {
            GraphEdges::Run(edges) => GraphEdgeIter::Run(edges.iter()),
            GraphEdges::Labelled(edges) => GraphEdgeIter::Labelled(edges.iter()),
        }
    }
}

/// A graph's edges, from a run's list or a labelled list, each read on its own path so that
/// a fold over the run's list takes `EdgeIter::fold`.
enum GraphEdgeIter<'a> {
    Run(super::EdgeIter<'a>),
    Labelled(std::slice::Iter<'a, Edge>),
}

impl Iterator for GraphEdgeIter<'_> {
    type Item = Edge;

    fn next(&mut self) -> Option<Edge> {
        match self {
            GraphEdgeIter::Run(it) => it.next(),
            GraphEdgeIter::Labelled(it) => it.next().copied(),
        }
    }

    fn fold<B, F: FnMut(B, Edge) -> B>(self, init: B, f: F) -> B {
        match self {
            GraphEdgeIter::Run(it) => it.fold(init, f),
            GraphEdgeIter::Labelled(it) => it.copied().fold(init, f),
        }
    }
}

impl Graph<'_> {
    /// Every move as `(from, to, action)`, self-loops and repeats included.
    pub fn edges(&self) -> impl Iterator<Item = (u32, u32, ActId)> + '_ {
        self.edges
            .iter()
            .map(|e| (e.from, e.to, self.bindings[e.binding as usize].0))
    }

    /// Every move as `(from, to, action, params)`, each param's slot from 0, self-loops and
    /// repeats included. A graph of another engine's moves names no params.
    pub fn bound_edges(&self) -> impl Iterator<Item = (u32, u32, ActId, &[u8])> + '_ {
        self.edges.iter().map(|e| {
            let (act, params) = &self.bindings[e.binding as usize];
            (e.from, e.to, *act, &params[..])
        })
    }

    /// The distinct successors and predecessors of every state, self-loops left out, for the
    /// passes that walk the graph rather than replay a run (`can_reach`, deadlock).
    ///
    /// The two directions are built side by side when the process has a helper to spare
    /// (`parallel::Helpers`): on the largest cone of food-permit-ops this runs after the BFS
    /// on its own, on the run's critical path (#296 item 3.1 part 3). A `shadow` build holds
    /// the result to the one built a direction after the other.
    pub fn adjacency(&self) -> Adjacency {
        let moves = || self.edges.iter().map(|e| (e.from, e.to));
        let helpers = crate::parallel::Helpers::take();
        let adjacency = Adjacency::side_by_side(self.states.len(), moves, helpers.count() > 0);
        drop(helpers);
        #[cfg(feature = "shadow")]
        {
            assert!(
                adjacency == Adjacency::new(self.states.len(), moves),
                "shadow: Graph::adjacency of {} states: the directions built side by side differ",
                self.states.len()
            );
            crate::shadow::compared(crate::shadow::Twin::SideBySideAdjacency);
        }
        adjacency
    }
}

/// A graph's moves by state, in both directions, self-loops and repeats left out. Compressed:
/// 4 bytes per move for each direction.
#[cfg_attr(feature = "shadow", derive(PartialEq))]
pub struct Adjacency {
    succs: Csr,
    preds: Csr,
}

impl Adjacency {
    /// From `n` states and the `(from, to)` moves `moves` yields, each time it is called.
    pub(crate) fn new<I: Iterator<Item = (u32, u32)>>(n: usize, moves: impl Fn() -> I) -> Self {
        let moves = || moves().filter(|(from, to)| from != to);
        Adjacency {
            succs: Csr::new(n, moves),
            preds: Csr::new(n, || moves().map(|(from, to)| (to, from))),
        }
    }

    /// `new`, the predecessors built on a thread of their own beside the successors when
    /// `split`.
    fn side_by_side<I: Iterator<Item = (u32, u32)>>(
        n: usize,
        moves: impl Fn() -> I + Sync,
        split: bool,
    ) -> Self {
        if !split {
            return Adjacency::new(n, moves);
        }
        let moves = || moves().filter(|(from, to)| from != to);
        std::thread::scope(|scope| {
            let preds = scope.spawn(|| Csr::new(n, || moves().map(|(from, to)| (to, from))));
            let succs = Csr::new(n, moves);
            Adjacency {
                succs,
                preds: preds
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
            }
        })
    }

    /// The states one move from `at` reaches, in ascending order.
    pub fn succs(&self, at: u32) -> &[u32] {
        self.succs.of(at)
    }

    /// The states one move from which `at` is reached, in ascending order.
    pub fn preds(&self, at: u32) -> &[u32] {
        self.preds.of(at)
    }

    /// `at` and every state a path from it through states `within` holds reaches, in the
    /// order a breadth-first search finds them, `at` first.
    pub(crate) fn forward_from(&self, at: u32, within: impl Fn(u32) -> bool) -> Vec<u32> {
        let mut seen = vec![false; self.succs.start.len() - 1];
        let mut order = vec![at];
        seen[at as usize] = true;
        let mut next = 0;
        while let Some(&s) = order.get(next) {
            next += 1;
            for &to in self.succs(s) {
                if within(to) && !seen[to as usize] {
                    seen[to as usize] = true;
                    order.push(to);
                }
            }
        }
        order
    }

    /// Marks every state from which a path through states `within` holds reaches a marked
    /// state. `starts` are the states `marked` holds on entry.
    pub(crate) fn mark_backward(
        &self,
        marked: &mut [bool],
        mut starts: Vec<u32>,
        within: impl Fn(u32) -> bool,
    ) {
        while let Some(at) = starts.pop() {
            for &p in self.preds(at) {
                if !marked[p as usize] && within(p) {
                    marked[p as usize] = true;
                    starts.push(p);
                }
            }
        }
    }

    /// The strongly connected components of the forward graph by iterative Tarjan: each
    /// state's component, numbered in the order the components close (a component comes
    /// before every component that reaches it), and the number of components.
    pub fn components(&self) -> (Vec<u32>, usize) {
        const UNSEEN: u32 = u32::MAX;
        let n = self.succs.start.len() - 1;
        let mut index = vec![UNSEEN; n];
        let mut low = vec![0u32; n];
        let mut comp = vec![UNSEEN; n];
        let mut stack: Vec<u32> = Vec::new();
        let mut call: Vec<(u32, usize)> = Vec::new();
        let (mut counter, mut count) = (0u32, 0u32);
        for root in 0..id32(n) {
            if index[root as usize] != UNSEEN {
                continue;
            }
            call.push((root, 0));
            while let Some(&(at, next)) = call.last() {
                let a = at as usize;
                if next == 0 {
                    index[a] = counter;
                    low[a] = counter;
                    counter += 1;
                    stack.push(at);
                }
                if let Some(&to) = self.succs.of(at).get(next) {
                    call.last_mut().expect("just read").1 += 1;
                    let t = to as usize;
                    if index[t] == UNSEEN {
                        call.push((to, 0));
                    } else if comp[t] == UNSEEN {
                        low[a] = low[a].min(index[t]);
                    }
                    continue;
                }
                call.pop();
                if let Some(&(parent, _)) = call.last() {
                    low[parent as usize] = low[parent as usize].min(low[a]);
                }
                if low[a] == index[a] {
                    while let Some(member) = stack.pop() {
                        comp[member as usize] = count;
                        if member == at {
                            break;
                        }
                    }
                    count += 1;
                }
            }
        }
        (comp, count as usize)
    }
}

/// One direction of the moves: `next[start[at]..start[at + 1]]` are the targets of `at`.
#[cfg_attr(feature = "shadow", derive(PartialEq))]
struct Csr {
    start: Vec<u32>,
    next: Vec<u32>,
}

impl Csr {
    /// From `n` states and the `(from, to)` pairs `pairs` yields, each time it is called.
    fn new<I: Iterator<Item = (u32, u32)>>(n: usize, pairs: impl Fn() -> I) -> Self {
        let mut start = vec![0u32; n + 1];
        // `for_each`, not `for`: a run's edges fold a state at a time (`EdgeIter::fold`).
        pairs().for_each(|(from, _)| start[from as usize + 1] += 1);
        for i in 0..n {
            start[i + 1] += start[i];
        }
        let mut fill = start.clone();
        let mut next = vec![0u32; start[n] as usize];
        pairs().for_each(|(from, to)| {
            next[fill[from as usize] as usize] = to;
            fill[from as usize] += 1;
        });
        // Sort each row and drop repeats, closing the gaps in place.
        let mut kept = 0usize;
        let mut from = 0usize;
        for at in 0..n {
            let row = from..start[at + 1] as usize;
            from = row.end;
            next[row.clone()].sort_unstable();
            start[at] = id32(kept);
            let mut last = None;
            for i in row {
                if last != Some(next[i]) {
                    last = Some(next[i]);
                    next[kept] = next[i];
                    kept += 1;
                }
            }
        }
        start[n] = id32(kept);
        next.truncate(kept);
        Csr { start, next }
    }

    fn of(&self, at: u32) -> &[u32] {
        &self.next[self.start[at as usize] as usize..self.start[at as usize + 1] as usize]
    }
}

/// Moves another engine found, each naming its action but not its params, so that its graph
/// gets the findings BFS gets.
pub struct LabelledEdges {
    edges: Vec<Edge>,
    bindings: Vec<(ActId, Box<[u8]>)>,
}

impl LabelledEdges {
    /// `(from, to, action)` moves of `ts`, between indices of the states the graph will have.
    pub fn new(ts: &TransitionSystem, moves: impl IntoIterator<Item = (u32, u32, ActId)>) -> Self {
        LabelledEdges {
            edges: moves
                .into_iter()
                .map(|(from, to, act)| Edge {
                    from,
                    to,
                    binding: act.0.into(),
                })
                .collect(),
            bindings: ts.action_ids().map(|a| (a, Box::from([]))).collect(),
        }
    }

    pub fn graph<'a>(&'a self, states: &'a States, complete: bool) -> Graph<'a> {
        Graph {
            states,
            complete,
            edges: GraphEdges::Labelled(&self.edges),
            bindings: &self.bindings,
        }
    }
}

impl ComponentRun {
    /// Every move from an expanded state, self-loops and repeats included, in discovery
    /// order. Panics on a run that stopped once every check failed
    /// (`Caps::stop_when_all_violated`), which keeps no edge list: nothing may read its graph.
    pub fn edges(&self) -> Edges<'_> {
        self.assert_held();
        self.steps.edges()
    }

    /// Whether the run keeps its edge list: every run but one that stops once every check
    /// fails.
    pub fn has_edges(&self) -> bool {
        self.steps.has_edges()
    }

    /// Moves read, self-loops and repeats included: the length of the edge list on a run
    /// that keeps one.
    pub fn moves(&self) -> u64 {
        self.steps.moves()
    }

    /// The graph findings read; panics on a run without an edge list, as `edges` does.
    pub fn graph(&self) -> Graph<'_> {
        Graph {
            states: &self.states,
            complete: self.complete,
            edges: GraphEdges::Run(self.edges()),
            bindings: &self.bindings,
        }
    }

    /// The action and its params on an edge: an `&Edge`, or a `Listed` as a `for` loop over
    /// `edges()` yields it.
    pub fn step(&self, edge: impl AnEdge) -> (ActId, &[u8]) {
        let (act, params) = &self.bindings[edge.edge().binding as usize];
        (*act, params)
    }

    /// The edge that first reached `states[at]`; `None` for an initial state.
    pub fn parent(&self, at: u32) -> Option<Edge> {
        self.steps.parent(at)
    }

    /// The BFS path from an initial state to `states[at]`, first edge first.
    pub fn path_to(&self, mut at: u32) -> Vec<Edge> {
        self.assert_held();
        let mut path = Vec::new();
        while let Some(edge) = self.parent(at) {
            path.push(edge);
            at = edge.from;
        }
        path.reverse();
        path
    }

    /// What the run's moves of `act` reached: whether one was read, whether one changes the
    /// state, and whether the target of one that does has a row of every entity in
    /// `binders`, the entities an after-assert step of `act` leaves unbound. A self-loop is a
    /// stuttering step, which D17 does not check. A run with an edge list scans it; a run without one recorded it
    /// as it went, for every after-assert step of its system.
    pub fn step_ends(&self, act: ActId, binders: &[EntId]) -> Ends {
        self.step_ends_with(act, binders, &[])
    }

    /// `step_ends` on a run that stores `orbit_self`, the indices of its orbit self-edges
    /// (ascending): a loop among them is a real step, any other loop a stuttering one.
    pub(crate) fn step_ends_with(&self, act: ActId, binders: &[EntId], orbit_self: &[u32]) -> Ends {
        let scan = |edges: &mut dyn Iterator<Item = Edge>| {
            let mut ends = Ends::default();
            for (k, e) in edges.enumerate() {
                if self.step(&e).0 != act {
                    continue;
                }
                ends.ran = true;
                let moved = e.from != e.to || orbit_self.binary_search(&id32(k)).is_ok();
                if !moved {
                    continue;
                }
                ends.moved = true;
                if self
                    .system
                    .meets(binders, &Pred::True, &self.states.at(e.to as usize))
                {
                    ends.with_rows = true;
                    break;
                }
            }
            ends
        };
        match &self.ends {
            None => scan(&mut self.edges().iter()),
            Some(recorded) => {
                let ends = recorded.of(act, binders);
                #[cfg(feature = "shadow")]
                {
                    let want = scan(&mut self.steps.twin_edges().iter().copied());
                    assert!(
                        ends == want,
                        "shadow: ComponentRun::step_ends({act:?}, {binders:?}): recorded {ends:?}, reference {want:?}"
                    );
                    crate::shadow::compared(crate::shadow::Twin::StepEnds);
                }
                ends
            }
        }
    }

    /// The BFS path to `states[at]` in model terms: a shortest one, since BFS reaches states
    /// in depth order. On a reduced system the path is lifted (`TransitionSystem::lift`).
    pub fn trace_to(&self, at: u32) -> Result<Trace, ReplayError> {
        Ok(self.lifted_trace(at, None)?.0)
    }

    /// The trace of a violation: the path to the violating state, or for an after-assert the
    /// path to the step's source and the step itself. The step need not be on the BFS path to
    /// its target, which another path may reach first. Also the trace's last state.
    pub(super) fn trace_of(&self, reach: &Reach) -> Result<(Trace, State), ReplayError> {
        match reach {
            Reach::State { at, .. } => self.lifted_trace(*at, None),
            Reach::Step {
                from,
                act,
                params,
                to,
                ..
            } => self.lifted_trace(*from, Some((*act, params.clone(), to.clone()))),
        }
    }

    /// The path to `states[at]`, then `extra`, lifted and in model terms, with its last state.
    fn lifted_trace(&self, at: u32, extra: Option<Step>) -> Result<(Trace, State), ReplayError> {
        let path = self.path_to(at);
        let first = path.first().map_or(at, |e| e.from);
        self.lifted_path(first, &path, extra)
    }

    /// `path`, a walk of edges from `states[first]`, lifted and in model terms.
    pub(crate) fn trace_along(&self, first: u32, path: &[Edge]) -> Result<Trace, ReplayError> {
        Ok(self.lifted_path(first, path, None)?.0)
    }

    /// Before it lifts, the path replays through the run's own step relation from one of its
    /// initial states (agreement rule 3, P20); one that does not, or whose lift fails, is the
    /// `ReplayError`, a tool bug the caller reports as a fault.
    fn lifted_path(
        &self,
        first: u32,
        path: &[Edge],
        extra: Option<Step>,
    ) -> Result<(Trace, State), ReplayError> {
        let mut steps: Vec<Step> = path
            .iter()
            .map(|e| {
                let (act, params) = self.step(e);
                (
                    act,
                    params.into(),
                    self.states.at(e.to as usize).into_state(),
                )
            })
            .collect();
        steps.extend(extra);
        let ts = &self.system;
        let first = self.states.at(first as usize).into_state();
        super::orbit::replays(ts, &first, &steps)?;
        let (start, steps) = ts.lifted(&first, &steps)?;
        Ok((
            ts.trace_from(&start, &steps),
            steps.last().map_or(start, |s| s.2.clone()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::Adjacency;

    fn adjacency(n: usize, moves: &[(u32, u32)]) -> Adjacency {
        Adjacency::new(n, || moves.iter().copied())
    }

    #[test]
    fn both_directions_drop_self_loops_and_repeats() {
        let a = adjacency(3, &[(0, 2), (0, 1), (0, 2), (1, 1), (2, 0)]);
        assert_eq!(a.succs(0), [1, 2]);
        assert_eq!(a.succs(1), [] as [u32; 0]);
        assert_eq!(a.preds(0), [2]);
        assert_eq!(a.preds(2), [0]);
        assert_eq!(a.preds(1), [0]);
    }

    #[test]
    fn components_group_cycles_and_close_sinks_first() {
        // 0 <-> 1 -> 2 <-> 3 -> 4, and 5 on its own with a self-loop.
        let a = adjacency(6, &[(0, 1), (1, 0), (1, 2), (2, 3), (3, 2), (3, 4), (5, 5)]);
        let (comp, count) = a.components();
        assert_eq!(count, 4);
        assert_eq!(comp[0], comp[1]);
        assert_eq!(comp[2], comp[3]);
        let mut distinct = vec![comp[0], comp[2], comp[4], comp[5]];
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), 4);
        assert!(comp[4] < comp[2] && comp[2] < comp[0]);
    }

    #[test]
    fn a_long_chain_does_not_recurse() {
        let n = 200_000u32;
        let moves: Vec<_> = (0..n - 1).map(|i| (i, i + 1)).collect();
        let (_, count) = adjacency(n as usize, &moves).components();
        assert_eq!(count, n as usize);
    }
}
