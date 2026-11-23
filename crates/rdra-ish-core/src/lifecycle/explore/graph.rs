//! The graph both engines hand to findings, and the run's edge, path and trace readers.

use crate::lifecycle::expr::State;
use crate::lifecycle::reduce::Step;
use crate::lifecycle::trace::Trace;
use crate::lifecycle::types::ActId;
use crate::lifecycle::TransitionSystem;

use super::{ComponentRun, Edge, Graph, Reach};

impl Graph<'_> {
    /// Every move as `(from, to, action)`, self-loops and repeats included.
    pub fn edges(&self) -> impl Iterator<Item = (u32, u32, ActId)> + '_ {
        self.edges
            .iter()
            .map(|e| (e.from, e.to, self.bindings[e.binding as usize].0))
    }

    /// The distinct successors and predecessors of every state, self-loops left out, for the
    /// passes that walk the graph rather than replay a run (`can_reach`, deadlock).
    pub fn adjacency(&self) -> Adjacency {
        Adjacency::new(self.states.len(), || {
            self.edges.iter().map(|e| (e.from, e.to))
        })
    }
}

/// A graph's moves by state, in both directions, self-loops and repeats left out. Compressed:
/// 4 bytes per move for each direction.
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

    /// The states one move from `at` reaches, in ascending order.
    pub fn succs(&self, at: u32) -> &[u32] {
        self.succs.of(at)
    }

    /// The states one move from which `at` is reached, in ascending order.
    pub fn preds(&self, at: u32) -> &[u32] {
        self.preds.of(at)
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
        for root in 0..n as u32 {
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
struct Csr {
    start: Vec<u32>,
    next: Vec<u32>,
}

impl Csr {
    /// From `n` states and the `(from, to)` pairs `pairs` yields, each time it is called.
    fn new<I: Iterator<Item = (u32, u32)>>(n: usize, pairs: impl Fn() -> I) -> Self {
        let mut start = vec![0u32; n + 1];
        for (from, _) in pairs() {
            start[from as usize + 1] += 1;
        }
        for i in 0..n {
            start[i + 1] += start[i];
        }
        let mut fill = start.clone();
        let mut next = vec![0u32; start[n] as usize];
        for (from, to) in pairs() {
            next[fill[from as usize] as usize] = to;
            fill[from as usize] += 1;
        }
        // Sort each row and drop repeats, closing the gaps in place.
        let mut kept = 0usize;
        let mut from = 0usize;
        for at in 0..n {
            let row = from..start[at + 1] as usize;
            from = row.end;
            next[row.clone()].sort_unstable();
            start[at] = kept as u32;
            let mut last = None;
            for i in row {
                if last != Some(next[i]) {
                    last = Some(next[i]);
                    next[kept] = next[i];
                    kept += 1;
                }
            }
        }
        start[n] = kept as u32;
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

    pub fn graph<'a>(&'a self, states: &'a [State], complete: bool) -> Graph<'a> {
        Graph {
            states,
            complete,
            edges: &self.edges,
            bindings: &self.bindings,
        }
    }
}

impl ComponentRun {
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    pub fn graph(&self) -> Graph<'_> {
        Graph {
            states: &self.states,
            complete: self.complete,
            edges: &self.edges,
            bindings: &self.bindings,
        }
    }

    /// The action and its params on an edge.
    pub fn step(&self, edge: &Edge) -> (ActId, &[u8]) {
        let (act, params) = &self.bindings[edge.binding as usize];
        (*act, params)
    }

    /// The edge that first reached `states[at]`; `None` for an initial state.
    pub fn parent(&self, at: u32) -> Option<&Edge> {
        self.parents[at as usize].map(|e| &self.edges[e as usize])
    }

    /// The BFS path from an initial state to `states[at]`, first edge first.
    pub fn path_to(&self, mut at: u32) -> Vec<Edge> {
        let mut path = Vec::new();
        while let Some(edge) = self.parent(at) {
            path.push(*edge);
            at = edge.from;
        }
        path.reverse();
        path
    }

    /// The BFS path to `states[at]` in model terms: a shortest one, since BFS reaches states
    /// in depth order. On a reduced system the path is lifted (`TransitionSystem::lift`).
    pub fn trace_to(&self, at: u32) -> Trace {
        self.lifted_trace(at, None).0
    }

    /// The trace of a violation: the path to the violating state, or for an after-assert the
    /// path to the step's source and the step itself. The step need not be on the BFS path to
    /// its target, which another path may reach first. Also the trace's last state.
    pub(super) fn trace_of(&self, reach: &Reach) -> (Trace, State) {
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
    fn lifted_trace(&self, at: u32, extra: Option<Step>) -> (Trace, State) {
        let path = self.path_to(at);
        let first = path.first().map_or(at, |e| e.from);
        self.lifted_path(first, &path, extra)
    }

    /// `path`, a walk of edges from `states[first]`, lifted and in model terms.
    pub(crate) fn trace_along(&self, first: u32, path: &[Edge]) -> Trace {
        self.lifted_path(first, path, None).0
    }

    fn lifted_path(&self, first: u32, path: &[Edge], extra: Option<Step>) -> (Trace, State) {
        let mut steps: Vec<Step> = path
            .iter()
            .map(|e| {
                let (act, params) = self.step(e);
                (act, params.into(), self.states[e.to as usize].clone())
            })
            .collect();
        steps.extend(extra);
        let ts = &self.system;
        let (start, steps) = ts.lifted(&self.states[first as usize], &steps);
        (
            ts.trace_from(&start, &steps),
            steps.last().map_or(start, |s| s.2.clone()),
        )
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
