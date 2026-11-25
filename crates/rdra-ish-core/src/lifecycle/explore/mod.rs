//! BFS over one system's flat states. Guards, and the invariants and after-asserts the caller
//! reads (`Checks`), are checked on the fly; per-entity view statistics are gathered from the
//! edges as they are found. The run keeps every edge and, per state, the edge that first
//! reached it.

mod arena;
mod bfs;
mod edges;
mod graph;
mod liveness;
mod moves;
mod orbit;
mod packing;
#[cfg(feature = "perf")]
mod perf;
#[cfg(feature = "perf")]
pub use perf::evals_line as perf_evals_line;
mod plan;
mod probe;
mod properties;
mod reach;
mod scan;
mod segments;
mod store;
pub(crate) mod symmetry;
mod views;
mod violations;

pub use arena::{States, Unpacked};
pub use edges::{AnEdge, EdgeIter, Edges, Ends, Listed};
pub use graph::{Adjacency, LabelledEdges};
pub use liveness::{LassoFault, ReducedVerdict, Spurious};
pub use orbit::{LabelledEdge, OrbitRun};
pub(crate) use plan::Plans;
pub use properties::PropertyVerdict;
pub use reach::{Estimate, Layers};
pub use store::{store_bytes, Held, Shape};
pub use symmetry::{LeftOut, Rule as SymmetryRule, Symmetry};

use std::collections::BTreeSet;

use std::time::Instant;

use crate::fasthash::FastMap;
use crate::memory::{Charge, MemoryPool};

use super::diag::Diag;
use super::expr::{State, StateSlice};
use super::types::{ActId, EntId};
use super::TransitionSystem;

/// The invariants and after-asserts a run evaluates: the ones its caller reads. A check the
/// run does not evaluate reports no violation, so a caller leaves out only checks it never
/// reads. `TransitionSystem::run` evaluates all of them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Checks {
    /// Every invariant and after-assert the system holds.
    #[default]
    All,
    /// The invariants and after-asserts with one of these names. Checks that share a name
    /// share one report (the first violation of any of them), so a name stands for all of
    /// them.
    Named(BTreeSet<String>),
}

impl Checks {
    /// No check: for a caller that reads only the graph.
    pub fn none() -> Self {
        Checks::Named(BTreeSet::new())
    }

    /// The run evaluates the checks named `name`.
    pub fn reads(&self, name: &str) -> bool {
        match self {
            Checks::All => true,
            Checks::Named(names) => names.contains(name),
        }
    }
}

/// Where exploration stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caps {
    /// Distinct view patterns per entity (`--max-patterns`), in a run that gathers them
    /// (`views`).
    pub patterns: usize,
    /// Joint states per run (`--max-states`).
    pub states: usize,
    /// The pool every run's state store is charged to (`--memory`); `None` charges nothing.
    pub memory: Option<&'static MemoryPool>,
    /// When the run stops, with `StateDiag::TimeBudgetReached` (`verify --temporal-budget`);
    /// `None` runs until a cap or the end.
    pub deadline: Option<Instant>,
    /// Stop once every invariant and after-assert the run evaluates has a counterexample
    /// (`RunEnd::ChecksFailed`). Only a run that decides nothing but those checks sets it: an
    /// entity target, a property, or a `can_reach` reads the whole graph. The stop reads
    /// every check the run evaluates, so a run that sets it evaluates `Checks::All`, as its
    /// callers do: with fewer, it would stop at another state. Since nothing reads the graph
    /// of such a run, it keeps no edge list (`ComponentRun::edges` panics on it): each
    /// state's parent holds the move that reached it, and the run records as it goes what
    /// the vacuity of an after-assert reads (`ComponentRun::step_ends`).
    pub stop_when_all_violated: bool,
    /// Gather each entity's view patterns from the edges (`EntityView`), which only
    /// `entity_view` reads: `check`, `states` and the emitters, on an entity's cone. A run
    /// that gathers none has no pattern to count, so `patterns` does not stop it (#251).
    pub views: bool,
    /// The sub-lifecycle of a reduced cone whose plain run stops at the memory limit runs
    /// again up to `H`, the instance symmetry that keeps what the quotient reads (#212 phase
    /// 2; `verify --symmetry always`), where `Classes::of` reduces nothing
    /// otherwise.
    pub s_up_to_h: bool,
    /// Test-only: such a sub-lifecycle runs up to `H` at once, with no plain run first, so
    /// that the samples at small bounds reach the path and its twin. Only a test build or a
    /// `shadow` build can set it (`forcing_s_up_to_h`, `RDRA_ISH_FORCE_S_UP_TO_H`).
    pub(crate) force_s_up_to_h: bool,
    /// An entity cone of `verify`'s entity pass whose plain run stops at the memory limit
    /// runs again up to instance symmetry, its edges' permutations kept, and its state-model
    /// findings are read on that run when it completes (#212 phase 4; `verify --symmetry
    /// always`).
    pub entity_orbits: bool,
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
            stop_when_all_violated: false,
            views: true,
            s_up_to_h: false,
            force_s_up_to_h: false,
            entity_orbits: false,
        }
    }

    /// These caps, with `states` as the state cap.
    pub const fn with_states(self, states: usize) -> Self {
        Caps { states, ..self }
    }

    /// These caps, with a run of a sub-lifecycle that stops at the memory limit run again up
    /// to `H` (`s_up_to_h`).
    pub const fn with_s_up_to_h(self, on: bool) -> Self {
        Caps {
            s_up_to_h: on,
            ..self
        }
    }

    /// These caps, with an entity cone that stops at the memory limit run again up to
    /// instance symmetry (`entity_orbits`).
    pub const fn with_entity_orbits(self, on: bool) -> Self {
        Caps {
            entity_orbits: on,
            ..self
        }
    }

    /// Test-only: these caps, with a sub-lifecycle run up to `H` at once (`force_s_up_to_h`).
    /// A `shadow` build also reads `RDRA_ISH_FORCE_S_UP_TO_H` for it; a release build has
    /// neither.
    #[cfg(any(test, feature = "shadow"))]
    pub const fn forcing_s_up_to_h(self) -> Self {
        Caps {
            s_up_to_h: true,
            force_s_up_to_h: true,
            ..self
        }
    }

    /// These caps, for a run whose caller reads no entity view: it gathers no pattern, so the
    /// pattern cap does not stop it.
    pub const fn without_views(self) -> Self {
        Caps {
            views: false,
            ..self
        }
    }

    /// These caps, with a run stopped once every invariant and after-assert has failed.
    pub const fn until_all_violated(self) -> Self {
        Caps {
            stop_when_all_violated: true,
            ..self
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
    pub states: States,
    /// False when the BFS stopped before it reached every reachable state: at a cap, or
    /// once every check failed (`checks_failed`).
    pub complete: bool,
    /// `Caps::stop_when_all_violated` stopped the run once every invariant and after-assert
    /// had a counterexample. The run is not complete, so nothing that needs the whole graph
    /// (vacuity, a covering cone's `ok`) reads it as one; its checks are decided.
    pub checks_failed: bool,
    pub diags: Vec<Diag>,
    /// The checks the run evaluated: `diags` holds no violation of any other.
    checks: Checks,
    /// Per entity, the patterns the edges showed; empty patterns when the run gathered none
    /// (`Caps::views`, `gathered_views`).
    pub(crate) views: Vec<EntityView>,
    gathered_views: bool,
    /// Per state the move that first reached it, and, unless the run stopped once every
    /// check failed (`Caps::stop_when_all_violated`), every move from an expanded state,
    /// self-loops and repeats included, in discovery order.
    steps: edges::Steps,
    /// On a run that keeps no edge list: what the after-assert steps' moves reached.
    ends: Option<edges::StepEnds>,
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
    /// The run kept only its system, diagnostics and end once its reader was done
    /// (`release`): a reader of its graph panics.
    released: bool,
}

/// What a reader of state predicates and action labels reads of a run, plain or up to
/// symmetry (`OrbitRun`): the stored states, whether the run reached all of them, its
/// diagnostics, and what the edges an action labels reach. Each such reader is sound on
/// a run up to symmetry, since a state predicate that reads every row has one value on a
/// whole class, and an action labels an edge of the stored graph exactly when it labels one
/// of the plain graph.
pub trait StoredRun {
    fn system(&self) -> &TransitionSystem;
    fn stored(&self) -> &States;
    /// The run reached every state, or every class.
    fn whole(&self) -> bool;
    fn diagnostics(&self) -> &[Diag];
    fn evaluated(&self) -> &Checks;
    /// What the edges `act` labels reach: whether there is one, and whether the stored target
    /// of one has a row of every entity in `binders` (`ComponentRun::step_ends`).
    fn step_ends(&self, act: ActId, binders: &[EntId]) -> Ends;
}

impl StoredRun for ComponentRun {
    fn system(&self) -> &TransitionSystem {
        &self.system
    }
    fn stored(&self) -> &States {
        &self.states
    }
    fn whole(&self) -> bool {
        self.complete
    }
    fn diagnostics(&self) -> &[Diag] {
        &self.diags
    }
    fn evaluated(&self) -> &Checks {
        &self.checks
    }
    fn step_ends(&self, act: ActId, binders: &[EntId]) -> Ends {
        ComponentRun::step_ends(self, act, binders)
    }
}

impl StoredRun for OrbitRun {
    fn system(&self) -> &TransitionSystem {
        OrbitRun::system(self)
    }
    fn stored(&self) -> &States {
        self.states()
    }
    fn whole(&self) -> bool {
        self.complete()
    }
    fn diagnostics(&self) -> &[Diag] {
        self.diags()
    }
    fn evaluated(&self) -> &Checks {
        self.checks()
    }
    fn step_ends(&self, act: ActId, binders: &[EntId]) -> Ends {
        OrbitRun::step_ends(self, act, binders)
    }
}

/// How a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunEnd {
    /// Every reachable state was explored.
    Complete,
    /// A cap, the memory limit, or the time budget stopped the run.
    Capped,
    /// Every invariant and after-assert had a counterexample, so the run stopped
    /// (`Caps::stop_when_all_violated`): each check is decided, the graph is not whole.
    ChecksFailed,
}

impl ComponentRun {
    /// Charges what the run keeps to `pool` instead of the pool it ran in, for a result held
    /// past its cone (`QuotientCache`); false, leaving the charge where it was, when `pool`
    /// has less left.
    pub(crate) fn charge_to(&mut self, pool: &'static crate::memory::MemoryPool) -> bool {
        match Charge::hold(pool, self.kept) {
            Some(charge) => {
                self._charge = Some(charge);
                true
            }
            None => false,
        }
    }

    /// Drops the states, the edges and the views of a stopped run once what it decides was
    /// read, and gives what they held back to `Caps::memory`; the system, the diagnostics and
    /// the end stay. The entity pass of `verify` releases a stopped entity cone after
    /// `entity_findings` (`TransitionSystem::explore_targets_reduced_in`), so that the cones
    /// after it run in the room it held. A reader of the graph of a released run panics.
    pub(crate) fn release(&mut self) {
        assert!(!self.complete, "only a stopped run is released");
        self.states = States::new(self.states.width());
        self.steps = edges::Steps::new(false);
        self.ends = None;
        self.bindings = Vec::new();
        self.views = Vec::new();
        self.gathered_views = false;
        self.layers = Layers::default();
        self.kept = 0;
        self._charge = None;
        self.released = true;
    }

    /// Panics on a released run (`release`): only `entity_findings` reads a stopped entity
    /// cone of the entity pass, before the release.
    pub(crate) fn assert_held(&self) {
        assert!(
            !self.released,
            "a stopped entity cone's run is released once entity_findings read it: nothing else may read its graph"
        );
    }

    /// Whether `other` is the same run of the same graph: the states, parents, edges,
    /// bindings and findings in the same order, and the same end. The system is not compared.
    #[cfg(any(test, feature = "shadow"))]
    pub(crate) fn same_graph(&self, other: &ComponentRun) -> bool {
        let parents = |run: &ComponentRun| -> Vec<Option<Edge>> {
            (0..run.states.len() as u32)
                .map(|k| run.parent(k))
                .collect()
        };
        self.states == other.states
            && parents(self) == parents(other)
            && self.has_edges() == other.has_edges()
            && self.moves() == other.moves()
            && (!self.has_edges() || self.edges() == other.edges())
            && self.bindings == other.bindings
            && self.diags == other.diags
            && self.complete == other.complete
            && self.checks_failed == other.checks_failed
    }

    /// The invariants and after-asserts the run evaluated; `diags` says nothing of the rest.
    pub fn checks(&self) -> &Checks {
        &self.checks
    }

    /// Whether the run gathered the entities' view patterns (`Caps::views`): a reader of
    /// `views` reads only a run that did.
    pub fn gathered_views(&self) -> bool {
        self.gathered_views
    }

    /// How the run ended.
    pub fn end(&self) -> RunEnd {
        match (self.complete, self.checks_failed) {
            (true, _) => RunEnd::Complete,
            (false, true) => RunEnd::ChecksFailed,
            (false, false) => RunEnd::Capped,
        }
    }
}

/// The worker threads a run computes moves on, beside the thread that reads them.
#[derive(Clone, Copy)]
enum Threads {
    /// This many, whatever other runs hold; none runs on the caller's thread alone.
    #[cfg_attr(not(test), allow(dead_code))]
    Fixed(usize),
    /// What the process-wide budget has left (`parallel::Helpers`), `bfs_threads()` less one in all.
    Shared,
}

/// One move of the run: `states[from]` steps to `states[to]` under one `(action, params)`.
/// A run stores it in fewer bytes (`Edges`) and yields it by value.
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
    /// Under symmetry: each successor's canonical form, `width` bytes each, and what
    /// canonicalizing it found (`canonicalize`).
    canon: Vec<u8>,
    found: Vec<symmetry::Canonical>,
    /// Under symmetry: per successor, the permutation that takes it to its canonical form,
    /// `Group::slots` bytes each, for a run that keeps its edges' permutations (#212 phase 4).
    perm: Vec<u8>,
    perm_width: usize,
    /// What the reduced step reuses from one state to the next.
    pub(crate) reduced: super::reduce::StepBuffers,
}

impl Moves {
    pub fn len(&self) -> usize {
        self.acts.len()
    }

    pub fn push(&mut self, act: ActId, params: &[u8], next: &StateSlice) {
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

    /// Canonicalizes the successors of the moves from `from` on, in a worker.
    /// A `shadow` build holds each form to `symmetry/reference.rs`.
    pub(crate) fn canonicalize(
        &mut self,
        ts: &TransitionSystem,
        group: &symmetry::Group,
        canon: &mut symmetry::Canon,
        from: usize,
    ) {
        let w = self.width;
        self.canon.resize(self.len() * w, 0);
        self.perm_width = group.slots;
        for i in from..self.len() {
            let (next, out) = (
                &self.next[i * w..(i + 1) * w],
                &mut self.canon[i * w..(i + 1) * w],
            );
            let found = canon.canonical(group, next, out);
            symmetry::check(ts, group, next, out, found);
            self.found.push(found);
            self.perm.extend_from_slice(canon.perm());
        }
    }

    /// The permutation that takes move `i`'s successor to its canonical form, after
    /// `canonicalize`.
    pub(crate) fn perm(&self, i: usize) -> &[u8] {
        &self.perm[i * self.perm_width..(i + 1) * self.perm_width]
    }

    /// Move `i`'s canonical successor and what canonicalizing it found, after
    /// `canonicalize`.
    pub(crate) fn canonical(&self, i: usize) -> (&[u8], symmetry::Canonical) {
        (
            &self.canon[i * self.width..(i + 1) * self.width],
            self.found[i],
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
    pub states: &'a States,
    /// False when a cap stopped the exploration, so a state it did not reach may be reachable.
    pub complete: bool,
    edges: GraphEdges<'a>,
    bindings: &'a [(ActId, Box<[u8]>)],
}

/// The moves of a `Graph`: a BFS run's list, or another engine's moves in its order.
#[derive(Clone, Copy)]
enum GraphEdges<'a> {
    Run(Edges<'a>),
    Labelled(&'a [Edge]),
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

/// The checks with a reported violation, as a bit per check name. Checks are numbered
/// invariants first, then after-asserts; checks that share a name share its bit, as one
/// violation reports for both.
struct Violated {
    /// Per check: the index of its name among the system's distinct check names.
    name_of: Vec<u32>,
    /// Per distinct name: a violation of it was reported.
    reported: Vec<bool>,
    /// Distinct names with no violation yet.
    open: usize,
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
    /// The invariants the run evaluates, by index, in order.
    invariants: Vec<usize>,
    /// The after-asserts the run evaluates, by index, in order.
    after: Vec<usize>,
    states: States,
    /// The index of each state in `states`.
    seen: arena::Seen,
    /// The state being looked up, packed as `states` keeps it, so a new one packs once.
    packed: Vec<u8>,
    /// Scratch for the pattern keys `observe` reads, so an edge allocates none.
    keys: (Vec<u8>, Vec<u8>),
    /// Per `(entity, slot)`: the state being expanded, and the slot's pattern id in it, so
    /// `observe` looks a source pattern up once per expanded state instead of once per edge.
    sources: Vec<(usize, usize)>,
    /// The slots `sources` holds per entity: the most any entity has.
    source_slots: usize,
    complete: bool,
    checks_failed: bool,
    diags: Vec<Diag>,
    views: Vec<EntityView>,
    /// Invariants and asserts with a reported violation: each reports its first one only.
    violated: Violated,
    /// Per reported violation: its index in `diags` and where it was seen.
    violations: Vec<(usize, Reach)>,
    steps: edges::Steps,
    /// On a run that keeps no edge list: what the after-assert steps' moves reached.
    ends: Option<edges::StepEnds>,
    bindings: Bindings,
    /// What the store is charged to `Caps::memory`, when the caps name a pool; the result
    /// keeps what it holds of it.
    charge: Option<Charge>,
    peak_admitted: u64,
    layers: Layers,
    /// The group a run up to symmetry stores canonical forms under (`run_orbits`); `None`
    /// for a plain run.
    group: Option<&'t symmetry::Group>,
    /// The reader thread's canonicalizer: for the initial states and the chunks it expands.
    canon: symmetry::Canon,
    /// Under symmetry: the index in `edges` of each orbit self-edge, a move into the class
    /// of its own source, which is stored as a loop but is a real step.
    orbit_self: Vec<u32>,
    orbits: symmetry::Orbits,
    /// Under symmetry, for a run whose readers follow a binding along its edges (#212 phase
    /// 4): each edge's permutation, from its concrete successor to the stored target.
    perms: Option<orbit::EdgePerms>,
}

/// What a run up to symmetry found beside its plain run's result.
pub(crate) struct OrbitParts {
    pub(crate) orbit_self: Vec<u32>,
    pub(crate) orbits: symmetry::Orbits,
    pub(crate) perms: Option<orbit::EdgePerms>,
}

impl TransitionSystem {
    /// Explores this system, evaluating every invariant and after-assert it holds.
    pub fn run(self, caps: Caps) -> ComponentRun {
        self.run_checking(caps, Checks::All)
    }

    /// Explores this system, evaluating only `checks`. The states, the edges, the views, and
    /// where a cap stops the run are those of `run`; `diags` holds the violations of `checks`
    /// alone.
    pub fn run_checking(self, caps: Caps, checks: Checks) -> ComponentRun {
        self.run_on(caps, checks, Threads::Shared)
    }

    /// `run_checking` with `threads` workers. The run does not depend on the number: the
    /// states, the edges, and the findings come out in the order one thread finds them.
    fn run_on(self, caps: Caps, checks: Checks, threads: Threads) -> ComponentRun {
        let (mut run, violations, _) = self.bfs_on(caps, checks, threads, None, false);
        run.trace_violations(&violations);
        run
    }

    /// The BFS of `run_on`, up to the symmetry of `group` when there is one, with where each
    /// violation was seen; the violations' traces are not filled in.
    fn bfs_on(
        self,
        caps: Caps,
        checks: Checks,
        threads: Threads,
        group: Option<&symmetry::Group>,
        annotate: bool,
    ) -> (ComponentRun, Vec<(usize, Reach)>, OrbitParts) {
        crate::shadow_assert!(
            !caps.stop_when_all_violated || checks == Checks::All,
            "a run that stops once every check fails evaluates every check"
        );
        #[cfg(feature = "perf")]
        let clock = perf::Clock::start();
        let mut bfs = Bfs::new(&self, caps, &checks, group);
        if annotate {
            let g = group.expect("only a run up to symmetry keeps edge permutations");
            bfs.perms = Some(orbit::EdgePerms::new(g));
        }
        bfs.go(threads);
        let kept = bfs.kept();
        let peak_admitted = bfs.peak_admitted;
        let peak = bfs.bytes_with_one_more().max(peak_admitted);
        if let Some(charge) = &mut bfs.charge {
            charge.shrink_to(kept);
        }
        if let Some(perms) = &mut bfs.perms {
            perms.truncate(bfs.steps.moves() as usize);
        }
        let Bfs {
            states,
            complete,
            checks_failed,
            diags,
            views,
            steps,
            ends,
            bindings,
            violations,
            charge,
            layers,
            orbit_self,
            orbits,
            perms,
            ..
        } = bfs;
        let run = ComponentRun {
            system: self,
            states,
            complete,
            checks_failed,
            diags,
            checks,
            views,
            gathered_views: caps.views,
            steps,
            ends,
            bindings: bindings.list,
            kept,
            peak_admitted,
            peak,
            layers,
            _charge: charge,
            released: false,
        };
        #[cfg(feature = "perf")]
        perf::line(&run, clock, caps.stop_when_all_violated);
        (
            run,
            violations,
            OrbitParts {
                orbit_self,
                orbits,
                perms,
            },
        )
    }
}

impl ComponentRun {
    /// Fills in the trace of each violation the run reported, at its index in `diags`.
    fn trace_violations(&mut self, violations: &[(usize, Reach)]) {
        let run = self;
        for (at, reach) in violations {
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
    }
}

#[cfg(test)]
mod plan_tests;
#[cfg(test)]
mod tests;
