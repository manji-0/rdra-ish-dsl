//! BFS over one system's flat states. Guards, invariants, and after-asserts are checked on
//! the fly; per-entity view statistics are gathered from the edges as they are found. The run
//! keeps every edge and, per state, the edge that first reached it.

mod bfs;
mod graph;
mod moves;
mod scan;
mod views;
mod violations;

pub use graph::LabelledEdges;

use std::collections::{BTreeSet, HashSet};

use crate::fasthash::FastMap;

use super::diag::Diag;
use super::expr::State;
use super::types::ActId;
use super::TransitionSystem;

/// Where exploration stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caps {
    /// Distinct view patterns per entity (`--max-patterns`).
    pub patterns: usize,
    /// Joint states per run (`--max-states`).
    pub states: usize,
}

impl Caps {
    pub const DEFAULT_PATTERNS: usize = 256;
    /// Joint states per target cone that `check`, `states`, and the generators explore by
    /// default.
    pub const DEFAULT_STATES: usize = 65_536;

    pub const fn new(patterns: usize, states: usize) -> Self {
        Caps { patterns, states }
    }
}

pub struct ComponentRun {
    /// The system explored; `states` and the views index into it.
    pub system: TransitionSystem,
    /// Reached states in discovery order.
    pub states: Vec<State>,
    /// False when a cap stopped the BFS.
    pub complete: bool,
    pub diags: Vec<Diag>,
    pub(crate) views: Vec<EntityView>,
    /// Parallel to `states`: the index in `edges` of the edge that first reached the state;
    /// `None` for an initial state.
    parents: Vec<Option<u32>>,
    /// Every move from an expanded state, self-loops and repeats included, in discovery order.
    edges: Vec<Edge>,
    /// `(action, params)` of each distinct binding an edge names.
    bindings: Vec<(ActId, Box<[u8]>)>,
}

/// One move of the run: `states[from]` steps to `states[to]` under one `(action, params)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Edge {
    pub from: u32,
    pub to: u32,
    binding: u32,
}

/// One step: an action, its bound slots, and the successor. BFS enumerates this, and nothing
/// else evaluates guards and effects.
pub(crate) struct Move {
    pub act: ActId,
    pub params: Box<[u8]>,
    pub next: State,
}

/// Moves in the order found, in flat buffers: an edge of a large run costs no allocation.
#[derive(Default)]
pub(crate) struct Moves {
    acts: Vec<ActId>,
    /// The params of every move, end to end.
    params: Vec<u8>,
    /// Where each move's params end.
    param_ends: Vec<usize>,
    /// The successor of every move, `width` bytes each.
    next: Vec<u8>,
    width: usize,
}

impl Moves {
    pub fn len(&self) -> usize {
        self.acts.len()
    }

    pub fn push(&mut self, act: ActId, params: &[u8], next: &State) {
        debug_assert!(self.acts.is_empty() || self.width == next.0.len());
        self.width = next.0.len();
        self.acts.push(act);
        self.params.extend_from_slice(params);
        self.param_ends.push(self.params.len());
        self.next.extend_from_slice(&next.0);
    }

    /// Move `i`: its action, its params, and its successor's bytes.
    pub fn get(&self, i: usize) -> (ActId, &[u8], &[u8]) {
        let from = i.checked_sub(1).map_or(0, |i| self.param_ends[i]);
        (
            self.acts[i],
            &self.params[from..self.param_ends[i]],
            &self.next[i * self.width..(i + 1) * self.width],
        )
    }

    pub fn into_moves(self) -> Vec<Move> {
        (0..self.len())
            .map(|i| {
                let (act, params, next) = self.get(i);
                Move {
                    act,
                    params: params.into(),
                    next: State(next.into()),
                }
            })
            .collect()
    }
}

/// What both engines produce for one cone: the reached states in discovery order, initial
/// ones first, and the moves between them. Findings read only this.
pub struct Graph<'a> {
    pub states: &'a [State],
    /// False when a cap stopped the exploration, so a state it did not reach may be reachable.
    pub complete: bool,
    edges: &'a [Edge],
    bindings: &'a [(ActId, Box<[u8]>)],
}

/// Where a violation was seen.
enum Reach {
    /// `states[at]` violates `invariants()[inv]`.
    State { at: u32, inv: usize },
    /// The step from `states[from]` to `to` fails `after_asserts()[after]`.
    Step {
        from: u32,
        act: ActId,
        params: Box<[u8]>,
        to: State,
        after: usize,
    },
}

/// What the edges said about one entity's view patterns, indexed by pattern id.
#[derive(Default)]
pub(crate) struct EntityView {
    pub patterns: Vec<Box<[u8]>>,
    index: FastMap<Box<[u8]>, usize>,
    pub initial: Vec<bool>,
    /// Some enabled action moves a slot off this pattern.
    pub changes: Vec<bool>,
    /// Some enabled action deletes a slot in this pattern.
    pub deletable: Vec<bool>,
    /// Actions that create or delete a slot in this pattern.
    touch: Vec<BTreeSet<ActId>>,
    /// (from pattern, action, to pattern) for every move between patterns.
    moves: BTreeSet<(usize, ActId, usize)>,
    pub truncated: bool,
}

/// What `moves` checks for one action, scanned over a set of states one check at a time.
pub(crate) enum Scan {
    /// These params find no slot in any state: `New` no absent one, the others no present one.
    Unbound(Vec<usize>),
    /// Each param finds a slot in some state, but no state binds them all.
    Apart,
    /// These guards are false under every binding of every state; post-guards are read on
    /// every successor of the binding, whether or not the pre-guards hold.
    NeverHeld(Vec<usize>),
    /// Some state binds every param and every guard holds under some binding, but not all of
    /// them in one state.
    EachHeld,
    /// In some state every pre-guard holds and every param but these `New` ones finds a slot:
    /// the instance bound leaves them none there.
    Crowded(Vec<usize>),
}

/// Interns each `(action, params)` an edge names, so an edge stores one index.
#[derive(Default)]
struct Bindings {
    list: Vec<(ActId, Box<[u8]>)>,
    /// Per action: its params to their index in `list`.
    index: Vec<FastMap<Box<[u8]>, u32>>,
}

struct Bfs<'t> {
    ts: &'t TransitionSystem,
    caps: Caps,
    states: Vec<State>,
    seen: FastMap<State, u32>,
    /// Scratch for the pattern keys `observe` reads, so an edge allocates none.
    keys: (Vec<u8>, Vec<u8>),
    complete: bool,
    diags: Vec<Diag>,
    views: Vec<EntityView>,
    /// Invariants and asserts with a reported violation: each reports its first one only.
    violated: HashSet<String>,
    /// Per reported violation: its index in `diags` and where it was seen.
    violations: Vec<(usize, Reach)>,
    parents: Vec<Option<u32>>,
    edges: Vec<Edge>,
    bindings: Bindings,
}

impl TransitionSystem {
    /// Explores this system.
    pub fn run(self, caps: Caps) -> ComponentRun {
        self.run_on(caps, crate::parallel::jobs())
    }

    /// `run` on up to `threads` threads. The run does not depend on the number: the states,
    /// the edges, and the findings come out in the order one thread finds them.
    fn run_on(self, caps: Caps, threads: usize) -> ComponentRun {
        let mut bfs = Bfs::new(&self, caps);
        bfs.go(threads);
        let Bfs {
            states,
            complete,
            diags,
            views,
            parents,
            edges,
            bindings,
            violations,
            ..
        } = bfs;
        let mut run = ComponentRun {
            system: self,
            states,
            complete,
            diags,
            views,
            parents,
            edges,
            bindings: bindings.list,
        };
        for (at, reach) in &violations {
            let (trace, last) = run.trace_of(reach);
            // A reduced run holds representatives: the message describes the lifted state.
            if run.system.reduction().is_some() {
                let rebuilt = match reach {
                    Reach::State { inv, .. } => run.system.invariant_violation(*inv, &last),
                    Reach::Step {
                        after, act, params, ..
                    } => run.system.after_violation(*after, *act, params, &last),
                };
                if let Some(d) = rebuilt {
                    run.diags[*at].diag = d.diag;
                }
            }
            if let Some(t) = run.diags[*at].diag.trace_mut() {
                *t = trace;
            }
        }
        run
    }
}

#[cfg(test)]
mod tests;
