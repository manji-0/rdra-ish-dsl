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
