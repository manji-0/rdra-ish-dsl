//! The BFS itself: interning bindings, admitting states, and checking each move.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::ops::Range;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::Scope;
use std::time::Instant;

use crate::fasthash::FastMap;
use crate::lifecycle::diag::{CapGrowth, Diag, StateDiag, VarSpread};
use crate::lifecycle::expr::{State, StateSlice};
use crate::lifecycle::types::{ActId, Domain, EntId, Value, VarKind};
use crate::lifecycle::view::pattern_bound;
use crate::lifecycle::TransitionSystem;
use crate::memory::Charge;
use crate::parallel::Helpers;

use super::edges::{StepEnds, Steps};
use super::store::binding_bytes;
use super::symmetry::{Canon, Canonical, Group};
use super::{
    Bfs, Bindings, Caps, Checks, EntityView, Moves, Reach, States, Threads, Unpacked, Violated,
};

#[cfg(feature = "shadow")]
mod reference;

/// States one worker expands at a time. Fewer than two chunks of waiting states are
/// expanded on the caller's thread, so a small run starts no worker.
const CHUNK: usize = 128;
/// Chunks handed out ahead of the one being read, per worker: what the workers hold in
/// memory at once.
const AHEAD: usize = 4;

/// The moves from a run of states, in state order, and where each state's moves end.
type Expanded = (Moves, Vec<usize>);

/// The moves from a run of states, and under symmetry (`sym`) each successor's canonical
/// form beside it: a worker canonicalizes, so the reader only looks the forms up.
fn expand(
    ts: &TransitionSystem,
    states: &States,
    range: Range<usize>,
    mut sym: Option<(&Group, &mut Canon)>,
) -> Expanded {
    let mut moves = Moves::default();
    let Some(mut scratch) = states.get(range.start).map(Unpacked::into_state) else {
        return (moves, Vec::new());
    };
    // The state being expanded, unpacked from the store.
    let mut at = scratch.clone();
    let mut ends = Vec::with_capacity(range.len());
    for i in range {
        let from = moves.len();
        states.read_into(i, &mut at.0);
        ts.moves_into(&at, &mut scratch, &mut moves);
        if let Some((group, canon)) = &mut sym {
            moves.canonicalize(ts, group, canon, from);
        }
        ends.push(moves.len());
    }
    (moves, ends)
}

/// Workers that expand chunks of states while the caller reads the chunks before them. A
/// state's moves depend on that state alone, and the caller reads the chunks in the order it
/// handed them out, so the run is the one a single thread makes.
struct Workers<'scope, 'env> {
    scope: &'scope Scope<'scope, 'env>,
    ts: &'env TransitionSystem,
    /// The group a run up to symmetry canonicalizes successors under.
    group: Option<&'env Group>,
    /// How many workers the run starts; `None` until `Threads::Shared` has taken some.
    threads: Option<usize>,
    /// The helpers a `Threads::Shared` run holds, given back when the run ends.
    held: Option<Helpers>,
    /// `None` until the first chunk is handed out.
    jobs: Option<Sender<States>>,
    /// Per chunk handed out and not yet read, in order: its result once a worker sends it.
    waiting: VecDeque<Option<std::thread::Result<Expanded>>>,
    /// The chunks before `waiting[0]`: how many were handed out and read.
    read: usize,
    done: Receiver<(usize, std::thread::Result<Expanded>)>,
    done_tx: Sender<(usize, std::thread::Result<Expanded>)>,
}

impl<'scope, 'env> Workers<'scope, 'env> {
    fn new(
        scope: &'scope Scope<'scope, 'env>,
        ts: &'env TransitionSystem,
        group: Option<&'env Group>,
        threads: Threads,
    ) -> Self {
        let (done_tx, done) = mpsc::channel();
        Workers {
            scope,
            ts,
            group,
            threads: match threads {
                Threads::Fixed(n) => Some(n),
                Threads::Shared => None,
            },
            held: None,
            jobs: None,
            waiting: VecDeque::new(),
            read: 0,
            done,
            done_tx,
        }
    }

    /// Whether another chunk may be handed out. A shared run asks the budget for helpers
    /// here, when it first has chunks to hand out, and again each time while it has none.
    fn has_room(&mut self) -> bool {
        let threads = match self.threads {
            Some(threads) => threads,
            None => {
                let helpers = Helpers::take();
                let count = helpers.count();
                if count > 0 {
                    self.threads = Some(count);
                    self.held = Some(helpers);
                }
                count
            }
        };
        threads > 0 && self.waiting.len() < threads * AHEAD
    }

    fn hand_out(&mut self, states: States) {
        let jobs = self.jobs.get_or_insert_with(|| {
            let (jobs, queue) = mpsc::channel::<States>();
            let queue = Arc::new(Mutex::new((0usize, queue)));
            for _ in 0..self
                .threads
                .expect("a chunk is handed out once there are workers")
            {
                let (queue, done, ts) = (queue.clone(), self.done_tx.clone(), self.ts);
                let group = self.group;
                self.scope.spawn(move || {
                    let mut canon = Canon::default();
                    loop {
                        // The chunk and its number are taken together, so the numbers are the
                        // order the chunks were handed out in.
                        let (number, states) = {
                            let mut queue = queue.lock().expect("the chunk queue");
                            let Ok(states) = queue.1.recv() else { break };
                            queue.0 += 1;
                            (queue.0 - 1, states)
                        };
                        let sym = group.map(|g| (g, &mut canon));
                        let expanded = catch_unwind(AssertUnwindSafe(|| {
                            expand(ts, &states, 0..states.len(), sym)
                        }));
                        if done.send((number, expanded)).is_err() {
                            break;
                        }
                    }
                });
            }
            jobs
        });
        jobs.send(states).expect("a worker takes the chunk");
        self.waiting.push_back(None);
    }

    /// The oldest chunk handed out and not yet read; a worker's panic resumes here.
    fn next(&mut self) -> Expanded {
        while self.waiting[0].is_none() {
            let (number, expanded) = self.done.recv().expect("a worker sends its chunk");
            self.waiting[number - self.read] = Some(expanded);
        }
        self.read += 1;
        match self.waiting.pop_front().flatten().expect("a result") {
            Ok(expanded) => expanded,
            Err(panic) => resume_unwind(panic),
        }
    }
}

impl Violated {
    /// No violation yet; the names of `checks` are the open ones.
    fn new(ts: &TransitionSystem, checks: &Checks) -> Self {
        let mut names: HashMap<&str, u32> = HashMap::new();
        let all = ts.invariants.iter().map(|i| i.name.as_str());
        let name_of: Vec<u32> = all
            .chain(ts.after.iter().map(|a| a.name.as_str()))
            .map(|name| {
                let next = names.len() as u32;
                *names.entry(name).or_insert(next)
            })
            .collect();
        let open = names.keys().filter(|name| checks.reads(name)).count();
        Violated {
            name_of,
            reported: vec![false; names.len()],
            open,
        }
    }

    fn has(&self, check: usize) -> bool {
        self.reported[self.name_of[check] as usize]
    }

    /// Marks `check` violated; false when its name already was.
    fn insert(&mut self, check: usize) -> bool {
        let bit = &mut self.reported[self.name_of[check] as usize];
        let new = !*bit;
        *bit = true;
        self.open -= usize::from(new);
        new
    }

    /// The run evaluates a check, and every one it evaluates has a violation.
    fn all(&self, evaluated: bool) -> bool {
        self.open == 0 && evaluated
    }
}

impl Bindings {
    fn id(&mut self, act: ActId, params: &[u8]) -> u32 {
        if self.index.len() <= act.index() {
            self.index.resize_with(act.index() + 1, FastMap::default);
        }
        if let Some(id) = self.index[act.index()].get(params) {
            return *id;
        }
        let id = u32::try_from(self.list.len()).expect("bindings fit in u32");
        self.bytes += binding_bytes(params);
        self.list.push((act, params.into()));
        self.index[act.index()].insert(params.into(), id);
        id
    }
}

impl<'t> Bfs<'t> {
    pub(super) fn new(
        ts: &'t TransitionSystem,
        caps: Caps,
        checks: &Checks,
        group: Option<&'t Group>,
    ) -> Self {
        let invariants = (0..ts.invariants.len())
            .filter(|&i| checks.reads(&ts.invariants[i].name))
            .collect();
        let after = (0..ts.after.len())
            .filter(|&i| checks.reads(&ts.after[i].name))
            .collect();
        Bfs {
            ts,
            caps,
            invariants,
            after,
            states: States::packed(std::sync::Arc::new(super::packing::Packing::of(ts))),
            seen: Default::default(),
            keys: Default::default(),
            sources: vec![(usize::MAX, 0); ts.entities.len() * ts.most_slots()],
            source_slots: ts.most_slots(),
            complete: true,
            checks_failed: false,
            diags: Vec::new(),
            views: ts.entities.iter().map(|_| EntityView::default()).collect(),
            violated: Violated::new(ts, checks),
            violations: Vec::new(),
            // A run that stops once every check fails decides nothing that reads its graph.
            steps: Steps::new(!caps.stop_when_all_violated),
            ends: caps.stop_when_all_violated.then(|| StepEnds::new(ts)),
            bindings: Bindings::default(),
            charge: caps.memory.map(Charge::new),
            peak_admitted: 0,
            layers: Default::default(),
            group,
            canon: Canon::default(),
            orbit_self: Vec::new(),
            orbits: Default::default(),
        }
    }

    /// `s` as the run stores it: its canonical form under symmetry, else itself.
    fn stored(&mut self, s: &StateSlice) -> (State, Canonical) {
        let mut out = s.to_owned();
        let found = match self.group {
            Some(g) => {
                let found = self.canon.canonical(g, &s.0, &mut out.0);
                super::symmetry::check(self.ts, g, &s.0, &out.0, found);
                found
            }
            None => Canonical::PLAIN,
        };
        (out, found)
    }

    /// Explores with `threads` workers; the run is the same with any number.
    pub(super) fn go(&mut self, threads: Threads) {
        let ts = self.ts;
        let views = self.caps.views;
        for s in ts.initial_states() {
            for (i, _) in ts.entities.iter().enumerate().filter(|_| views) {
                let e = EntId::at(i);
                for slot in 0..ts.slots(e) {
                    if s.is_present(ts, e, slot) {
                        let key = ts.pattern_key(&s, e, slot);
                        let Some(id) = self.intern(e, &key) else {
                            return;
                        };
                        self.views[i].initial[id] = true;
                    }
                }
            }
            let (s, found) = self.stored(&s);
            if !self.admit(&s, None, found) || self.stop_if_all_failed() {
                return;
            }
        }
        let group = self.group;
        std::thread::scope(|scope| self.explore(Workers::new(scope, ts, group, threads)));
    }

    /// Expands the states in discovery order and reads their moves in that order, until none
    /// is left or a cap stops the run.
    fn explore(&mut self, mut workers: Workers) {
        // The successor being read, copied out of the worker's buffer.
        let Some(mut s2) = self.states.first().map(Unpacked::into_state) else {
            return;
        };
        // States before `next` are read; states before `sent` are read or handed out.
        let (mut next, mut sent) = (0, 0);
        loop {
            while self.states.len() - sent >= 2 * CHUNK && workers.has_room() {
                workers.hand_out(self.states.slice(sent..sent + CHUNK));
                sent += CHUNK;
            }
            let expanded = if next < sent {
                workers.next()
            } else if sent < self.states.len() {
                let end = self.states.len().min(sent + CHUNK);
                let sym = self.group.map(|g| (g, &mut self.canon));
                let expanded = expand(self.ts, &self.states, sent..end, sym);
                sent = end;
                expanded
            } else {
                return;
            };
            let (moves, ends) = expanded;
            let mut from = 0;
            for end in ends {
                if !self.take(next, &moves, from..end, &mut s2) {
                    return;
                }
                from = end;
                next += 1;
            }
        }
    }

    /// Reads `moves[range]`, the moves from `states[at]`; false when a cap stops the run, or
    /// the last open check fails under `Caps::stop_when_all_violated`.
    /// `s2` is a state of the system, overwritten with each successor in turn.
    fn take(&mut self, at: usize, moves: &Moves, range: Range<usize>, s2: &mut State) -> bool {
        self.layers.read(at);
        self.steps.read(at);
        let from = at as u32;
        let s = self.states.at(at).into_state();
        for i in range {
            let (act, params, next) = moves.get(i);
            s2.0.copy_from_slice(next);
            if self.caps.views && !self.observe(at, act, params, &s, s2) {
                return false;
            }
            let binding = self.bindings.id(act, params);
            if *s2 == s {
                self.push(from, from, act, binding, s2);
                continue;
            }
            self.check_after(from, act, params, s2);
            // Under symmetry the run stores the successor's canonical form; a move into the
            // class of its own source is an orbit self-edge, a real step stored as a loop.
            let (key, found) = match self.group {
                Some(_) => moves.canonical(i),
                None => (&s2.0[..], Canonical::PLAIN),
            };
            let key = StateSlice::new(key);
            if self.group.is_some() && *key == *s {
                let edge = u32::try_from(self.steps.moves()).expect("edges fit in u32");
                self.push(from, from, act, binding, &s);
                self.orbit_self.push(edge);
                // The step's after-asserts were read on its concrete successor above.
                if self.stop_if_all_failed() {
                    return false;
                }
                continue;
            }
            let to = match self.seen.find(&self.states, key) {
                Some(to) => to,
                None => {
                    let to = self.states.len() as u32;
                    if !self.admit(key, Some((from, binding)), found) {
                        return false;
                    }
                    to
                }
            };
            // The stored target: under symmetry the canonical form, which an after-assert
            // step's rows read as they read the concrete one.
            self.push(from, to, act, binding, key);
            // After the edge, so the trace of a violation at `to` reaches it.
            if self.stop_if_all_failed() {
                return false;
            }
        }
        true
    }

    /// Records the move from `states[from]` to `states[to]`, which is `stored`, under `act`
    /// and its `binding`.
    fn push(&mut self, from: u32, to: u32, act: ActId, binding: u32, stored: &StateSlice) {
        self.steps.push(from, to, binding);
        if let Some(ends) = &mut self.ends {
            ends.read(self.ts, act, stored);
        }
    }

    /// Ends the run when it stops once every check has failed and every one now has: the
    /// counterexamples are BFS's first, so the shortest, and any further state could only
    /// repeat a violation.
    fn stop_if_all_failed(&mut self) -> bool {
        let evaluated = !(self.invariants.is_empty() && self.after.is_empty());
        let stop = self.caps.stop_when_all_violated && self.violated.all(evaluated);
        if stop {
            self.complete = false;
            self.checks_failed = true;
        }
        stop
    }

    /// Adds a new state; false when the state cap or the memory limit stops the run.
    /// `parent` is the source and the binding of the move that reaches it, pushed next.
    fn admit(&mut self, s: &StateSlice, parent: Option<(u32, u32)>, found: Canonical) -> bool {
        if self.seen.find(&self.states, s).is_some() {
            return true;
        }
        if self.states.len() >= self.caps.states {
            self.complete = false;
            let entities = self.ts.entities.iter().map(|e| e.key).collect();
            self.diags.push(Diag {
                entities,
                diag: StateDiag::StateCapReached {
                    cap: self.caps.states,
                    growth: self.growth(),
                },
            });
            return false;
        }
        // Read the clock every 256 states, the first included, so a budget already spent
        // stops the run before it holds a state.
        if let Some(deadline) = self.caps.deadline {
            if self.states.len().is_multiple_of(256) && Instant::now() >= deadline {
                self.complete = false;
                let entities = self.ts.entities.iter().map(|e| e.key).collect();
                self.diags.push(Diag {
                    entities,
                    diag: StateDiag::TimeBudgetReached {
                        states: self.states.len(),
                    },
                });
                return false;
            }
        }
        let bytes = self.bytes_with_one_more();
        #[cfg(feature = "shadow")]
        {
            let before = self.bytes_before_with_one_more();
            let lists = self.lists_with_one_more();
            assert!(
                bytes - lists <= before,
                "shadow: Bfs::bytes_with_one_more at {} states: {bytes} B ({lists} B of lists of \
                 segments), {before} B before #296",
                self.states.len()
            );
        }
        self.peak_admitted = bytes;
        if let Some(charge) = &mut self.charge {
            if !charge.cover(bytes) {
                let pool = charge.pool();
                self.complete = false;
                let entities = self.ts.entities.iter().map(|e| e.key).collect();
                self.diags.push(Diag {
                    entities,
                    diag: StateDiag::MemoryCapReached {
                        limit: pool.limit(),
                        raise: pool.raise(),
                        states: self.states.len(),
                        growth: self.growth(),
                        held: self.held(bytes),
                    },
                });
                return false;
            }
        }
        self.check_invariants(s, self.states.len() as u32);
        self.orbits.admit(found);
        self.states.push(s);
        self.seen.insert(&self.states, self.states.len() as u32 - 1);
        self.steps.admit(parent);
        self.layers.admit(parent.is_none());
        true
    }

    /// What the reached states vary in: the distinct values of each variable, and the use
    /// case of the edge that first reached each state.
    fn growth(&self) -> CapGrowth {
        let ts = self.ts;
        let mut vars: Vec<VarSpread> = ts
            .var_ids()
            .map(|v| {
                let decl = ts.var(v);
                let slots = ts.slots_of(&decl.kind);
                let reached: BTreeSet<Value> = self
                    .states
                    .iter()
                    .flat_map(|s| (0..slots).map(move |slot| s.get(ts, v, slot)))
                    .collect();
                let (name, range) = match &decl.kind {
                    VarKind::Now => ("now".to_string(), Some("clock".to_string())),
                    VarKind::Column { entity, .. } | VarKind::Prop { entity, .. } => {
                        let range = matches!(decl.domain, Domain::Int | Domain::NullableInt)
                            .then(|| format!("Int 0..{}", ts.bounds.int_max));
                        let name = format!("{}.{}", ts.entity(*entity).id, ts.var_name(v));
                        (name, range)
                    }
                    VarKind::Exists(_)
                    | VarKind::Owner { .. }
                    | VarKind::Run { .. }
                    | VarKind::Raised { .. } => (ts.var_name(v), None),
                };
                VarSpread {
                    name,
                    reached: reached.len(),
                    domain: decl.domain.size(ts),
                    range,
                }
            })
            .collect();
        vars.sort_by(|a, b| b.reached.cmp(&a.reached).then(a.name.cmp(&b.name)));
        let mut counts: HashMap<&str, usize> = HashMap::new();
        let parents = (0..self.states.len() as u32).filter_map(|at| self.steps.parent(at));
        for edge in parents {
            let (act, _) = self.bindings.list[edge.binding as usize];
            *counts
                .entry(ts.actions[act.index()].name.as_str())
                .or_default() += 1;
        }
        let mut usecases: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(name, n)| (name.to_string(), n))
            .collect();
        usecases.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        CapGrowth { vars, usecases }
    }

    /// Interns a view pattern; `None` when the pattern cap stops the run.
    fn intern(&mut self, e: EntId, key: &[u8]) -> Option<usize> {
        let view = &mut self.views[e.index()];
        if let Some(id) = view.id(key) {
            return Some(id);
        }
        if view.patterns.len() >= self.caps.patterns {
            view.truncated = true;
            self.complete = false;
            let bound = pattern_bound(self.ts, e);
            self.diags.push(Diag {
                entities: vec![self.ts.entity(e).key],
                diag: StateDiag::PatternCapReached {
                    cap: self.caps.patterns,
                    bound,
                },
            });
            return None;
        }
        Some(view.intern(key))
    }

    /// The id of the pattern of `e`'s `slot` in `states[at]`, which is `s`: looked up on the
    /// first edge from `s` that writes the slot, and kept for the others.
    fn source(
        &mut self,
        at: usize,
        s: &StateSlice,
        e: EntId,
        slot: usize,
        key: &mut Vec<u8>,
    ) -> usize {
        let cached = &mut self.sources[e.index() * self.source_slots + slot];
        if cached.0 == at {
            #[cfg(feature = "shadow")]
            {
                let want = reference::source(self.ts, &self.views[e.index()], s, e, slot);
                assert!(
                    want == Some(cached.1),
                    "shadow: Bfs::source(state {at}, {}, slot {slot}): kept {}, reference {want:?}",
                    self.ts.entity(e).id,
                    cached.1
                );
                crate::shadow::compared(crate::shadow::Twin::Source);
            }
            return cached.1;
        }
        self.ts.pattern_key_into(s, e, slot, key);
        let id = self.views[e.index()]
            .id(key)
            .expect("pre-state pattern interned");
        *cached = (at, id);
        id
    }

    /// Records what one edge from `states[at]`, which is `s`, says about each written slot's
    /// pattern.
    fn observe(
        &mut self,
        at: usize,
        act: ActId,
        params: &[u8],
        s: &StateSlice,
        s2: &StateSlice,
    ) -> bool {
        let ts = self.ts;
        let action = &ts.actions[act.index()];
        let (mut before, mut after) = std::mem::take(&mut self.keys);
        let mut ok = true;
        for (p, param) in action.params.iter().enumerate() {
            let in_s = ts.reduction().is_some_and(|r| r.is_s_entity(param.entity));
            if !param.role.is_written() || in_s {
                continue;
            }
            let (e, slot) = (param.entity, usize::from(params[p]));
            let (pre, post) = (s.is_present(ts, e, slot), s2.is_present(ts, e, slot));
            // A row present on both sides with the same columns has the same pattern after
            // the move, already interned: the edge says nothing of it.
            let columns = &ts.entity(e).columns;
            if pre
                && post
                && columns
                    .iter()
                    .all(|v| s.get(ts, *v, slot) == s2.get(ts, *v, slot))
            {
                #[cfg(feature = "shadow")]
                {
                    let (pre_id, post_id) =
                        reference::row_ids(ts, &self.views[e.index()], s, s2, e, slot);
                    assert!(
                        pre_id.is_some() && pre_id == post_id,
                        "shadow: Bfs::observe({}, {}, slot {slot}) skipped a row the edge moves: pattern {pre_id:?} to {post_id:?}",
                        action.name,
                        ts.entity(e).id
                    );
                    crate::shadow::compared(crate::shadow::Twin::UnchangedRow);
                }
                continue;
            }
            let pre_id = pre.then(|| self.source(at, s, e, slot, &mut before));
            if post {
                ts.pattern_key_into(s2, e, slot, &mut after);
            }
            match (pre_id, post) {
                (None, true) => {
                    let Some(id) = self.intern(e, &after) else {
                        ok = false;
                        break;
                    };
                    let view = &mut self.views[e.index()];
                    view.initial[id] = true;
                    view.touch[id].insert(act);
                }
                (Some(id), false) => {
                    let view = &mut self.views[e.index()];
                    view.deletable[id] = true;
                    view.touch[id].insert(act);
                }
                (Some(id), true) => {
                    let Some(id2) = self.intern(e, &after) else {
                        ok = false;
                        break;
                    };
                    let view = &mut self.views[e.index()];
                    if id2 != id {
                        view.changes[id] = true;
                        view.moves.insert((id, act, id2));
                    }
                }
                (None, false) => {}
            }
        }
        self.keys = (before, after);
        ok
    }

    /// Reports the violation unless its rule already has one; the trace is filled in when the
    /// run ends.
    fn violation(&mut self, check: usize, diag: Diag, reach: Reach) {
        if self.violated.insert(check) {
            self.violations.push((self.diags.len(), reach));
            self.diags.push(diag);
        }
    }

    fn check_invariants(&mut self, s: &StateSlice, at: u32) {
        let ts = self.ts;
        for k in 0..self.invariants.len() {
            let i = self.invariants[k];
            if self.violated.has(i) {
                continue;
            }
            if let Some(d) = ts.invariant_violation(i, s) {
                self.violation(i, d, Reach::State { at, inv: i });
            }
        }
    }

    fn check_after(&mut self, from: u32, act: ActId, params: &[u8], s2: &StateSlice) {
        let ts = self.ts;
        let first = ts.invariants.len();
        for k in 0..self.after.len() {
            let i = self.after[k];
            if self.violated.has(first + i) {
                continue;
            }
            if let Some(d) = ts.after_violation(i, act, params, s2) {
                let reach = Reach::Step {
                    from,
                    act,
                    params: params.into(),
                    to: s2.to_owned(),
                    after: i,
                };
                self.violation(first + i, d, reach);
            }
        }
    }
}
