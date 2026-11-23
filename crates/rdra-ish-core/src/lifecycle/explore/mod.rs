//! BFS over one system's flat states. Guards, invariants, and after-asserts are checked on
//! the fly; per-entity view statistics are gathered from the edges as they are found. The run
//! keeps every edge and, per state, the edge that first reached it.

mod bfs;
mod graph;
mod liveness;
mod moves;
mod properties;
mod reach;
mod scan;
mod store;
mod views;
mod violations;

pub use graph::{Adjacency, LabelledEdges};
pub use liveness::LassoFault;
pub use properties::PropertyVerdict;
pub use reach::{Estimate, Layers};
pub use store::{store_bytes, Held};

use std::collections::{BTreeSet, HashSet};

use std::time::Instant;

use crate::fasthash::FastMap;
use crate::memory::{Charge, MemoryPool};

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
    /// The pool every run's state store is charged to (`--memory`); `None` charges nothing.
    pub memory: Option<&'static MemoryPool>,
    /// When the run stops, with `StateDiag::TimeBudgetReached` (`verify --temporal-budget`);
    /// `None` runs until a cap or the end.
    pub deadline: Option<Instant>,
}

impl Caps {
    /// The default caps at `instances` per entity (`--instance-count`): 2^(10 + 8 * (n - 1))
    /// patterns and no state cap. Each added instance multiplies a cone's states by far more
    /// than the pattern cap grew by, so the pattern cap grows with it; `--memory` is what stops
    /// a run that outgrows the machine. A shift past `usize` saturates, and `0` counts as `1`.
    pub const fn for_instances(instances: u32) -> Self {
        let shift = 10 + 8 * (instances.saturating_sub(1) as u64);
        let patterns = match 1usize.checked_shl(if shift > u32::MAX as u64 {
            u32::MAX
        } else {
            shift as u32
        }) {
            Some(patterns) => patterns,
            None => usize::MAX,
        };
        Caps::new(patterns, usize::MAX)
    }

    pub const fn new(patterns: usize, states: usize) -> Self {
        Caps {
            patterns,
            states,
            memory: None,
            deadline: None,
        }
    }

    /// These caps, with every run stopped at `deadline`.
    pub const fn until(self, deadline: Instant) -> Self {
        Caps {
            deadline: Some(deadline),
            ..self
        }
    }

    /// These caps, with every run charged to `pool`.
    pub const fn charged_to(self, pool: &'static MemoryPool) -> Self {
        Caps {
            memory: Some(pool),
            ..self
        }
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
    /// What the run's result holds, in the bytes `Caps::memory` counts.
    pub(crate) kept: u64,
    /// The largest store the run asked `Caps::memory` to cover: the last admitted state's
    /// `bytes_with_one_more`, with `seen` in it.
    pub peak_admitted: u64,
    /// The store once every edge is counted, with `seen`: `peak_admitted` leaves the edges of
    /// the last expanded states out.
    pub peak: u64,
    /// The states first reached at each depth.
    pub layers: Layers,
    /// `kept`, charged to `Caps::memory` while the result is alive.
    _charge: Option<Charge>,
}

/// The worker threads a run computes moves on, beside the thread that reads them.
#[derive(Clone, Copy)]
enum Threads {
    /// This many, whatever other runs hold; none runs on the caller's thread alone.
    #[cfg_attr(not(test), allow(dead_code))]
    Fixed(usize),
    /// What the process-wide budget has left (`parallel::Helpers`), `jobs()` less one in all.
    Shared,
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
    /// What `list` and `index` hold, in bytes (`store::binding_bytes`).
    bytes: u64,
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
    /// What the store is charged to `Caps::memory`, when the caps name a pool; the result
    /// keeps what it holds of it.
    charge: Option<Charge>,
    peak_admitted: u64,
    layers: Layers,
}

impl TransitionSystem {
    /// Explores this system.
    pub fn run(self, caps: Caps) -> ComponentRun {
        self.run_on(caps, Threads::Shared)
    }

    /// `run` with `threads` workers. The run does not depend on the number: the states, the
    /// edges, and the findings come out in the order one thread finds them.
    fn run_on(self, caps: Caps, threads: Threads) -> ComponentRun {
        let mut bfs = Bfs::new(&self, caps);
        bfs.go(threads);
        let kept = bfs.kept();
        let peak_admitted = bfs.peak_admitted;
        let peak = bfs.bytes_with_one_more().max(peak_admitted);
        if let Some(charge) = &mut bfs.charge {
            charge.shrink_to(kept);
        }
        let Bfs {
            states,
            complete,
            diags,
            views,
            parents,
            edges,
            bindings,
            violations,
            charge,
            layers,
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
            kept,
            peak_admitted,
            peak,
            layers,
            _charge: charge,
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
