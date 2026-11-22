//! The BFS itself: interning bindings, admitting states, and checking each move.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::ops::Range;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::Scope;

use crate::fasthash::FastMap;
use crate::lifecycle::diag::{CapGrowth, Diag, StateDiag, VarSpread};
use crate::lifecycle::expr::State;
use crate::lifecycle::types::{ActId, Domain, EntId, Value, VarKind};
use crate::lifecycle::view::pattern_bound;
use crate::lifecycle::TransitionSystem;

use super::{Bfs, Bindings, Caps, Edge, EntityView, Moves, Reach};

/// States one worker expands at a time. Fewer than two chunks of waiting states are
/// expanded on the caller's thread, so a small run starts no worker.
const CHUNK: usize = 128;
/// Chunks handed out ahead of the one being read, per worker: what the workers hold in
/// memory at once.
const AHEAD: usize = 4;

/// The moves from a run of states, in state order, and where each state's moves end.
type Expanded = (Moves, Vec<usize>);

fn expand(ts: &TransitionSystem, states: &[State]) -> Expanded {
    let mut moves = Moves::default();
    let Some(mut scratch) = states.first().cloned() else {
        return (moves, Vec::new());
    };
    let ends = states
        .iter()
        .map(|s| {
            ts.moves_into(s, &mut scratch, &mut moves);
            moves.len()
        })
        .collect();
    (moves, ends)
}

/// Workers that expand chunks of states while the caller reads the chunks before them. A
/// state's moves depend on that state alone, and the caller reads the chunks in the order it
/// handed them out, so the run is the one a single thread makes.
struct Workers<'scope, 'env> {
    scope: &'scope Scope<'scope, 'env>,
    ts: &'env TransitionSystem,
    threads: usize,
    /// `None` until the first chunk is handed out.
    jobs: Option<Sender<Vec<State>>>,
    /// Per chunk handed out and not yet read, in order: its result once a worker sends it.
    waiting: VecDeque<Option<std::thread::Result<Expanded>>>,
    /// The chunks before `waiting[0]`: how many were handed out and read.
    read: usize,
    done: Receiver<(usize, std::thread::Result<Expanded>)>,
    done_tx: Sender<(usize, std::thread::Result<Expanded>)>,
}

impl<'scope, 'env> Workers<'scope, 'env> {
    fn new(scope: &'scope Scope<'scope, 'env>, ts: &'env TransitionSystem, threads: usize) -> Self {
        let (done_tx, done) = mpsc::channel();
        Workers {
            scope,
            ts,
            threads,
            jobs: None,
            waiting: VecDeque::new(),
            read: 0,
            done,
            done_tx,
        }
    }

    fn has_room(&self) -> bool {
        self.threads > 1 && self.waiting.len() < self.threads * AHEAD
    }

    fn hand_out(&mut self, states: Vec<State>) {
        let jobs = self.jobs.get_or_insert_with(|| {
            let (jobs, queue) = mpsc::channel::<Vec<State>>();
            let queue = Arc::new(Mutex::new((0usize, queue)));
            for _ in 0..self.threads {
                let (queue, done, ts) = (queue.clone(), self.done_tx.clone(), self.ts);
                self.scope.spawn(move || loop {
                    // The chunk and its number are taken together, so the numbers are the
                    // order the chunks were handed out in.
                    let (number, states) = {
                        let mut queue = queue.lock().expect("the chunk queue");
                        let Ok(states) = queue.1.recv() else { break };
                        queue.0 += 1;
                        (queue.0 - 1, states)
                    };
                    let expanded = catch_unwind(AssertUnwindSafe(|| expand(ts, &states)));
                    if done.send((number, expanded)).is_err() {
                        break;
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

impl Bindings {
    fn id(&mut self, act: ActId, params: &[u8]) -> u32 {
        if self.index.len() <= act.index() {
            self.index.resize_with(act.index() + 1, FastMap::default);
        }
        if let Some(id) = self.index[act.index()].get(params) {
            return *id;
        }
        let id = u32::try_from(self.list.len()).expect("bindings fit in u32");
        self.list.push((act, params.into()));
        self.index[act.index()].insert(params.into(), id);
        id
    }
}

impl<'t> Bfs<'t> {
    pub(super) fn new(ts: &'t TransitionSystem, caps: Caps) -> Self {
        Bfs {
            ts,
            caps,
            states: Vec::new(),
            seen: FastMap::default(),
            keys: Default::default(),
            complete: true,
            diags: Vec::new(),
            views: ts.entities.iter().map(|_| EntityView::default()).collect(),
            violated: HashSet::new(),
            violations: Vec::new(),
            parents: Vec::new(),
            edges: Vec::new(),
            bindings: Bindings::default(),
        }
    }

    /// Explores on up to `threads` threads; the run is the same on any number.
    pub(super) fn go(&mut self, threads: usize) {
        let ts = self.ts;
        for s in ts.initial_states() {
            for (i, _) in ts.entities.iter().enumerate() {
                let e = EntId::at(i);
                for slot in 0..ts.bounds.slots() {
                    if s.is_present(ts, e, slot) {
                        let key = ts.pattern_key(&s, e, slot);
                        let Some(id) = self.intern(e, &key) else {
                            return;
                        };
                        self.views[i].initial[id] = true;
                    }
                }
            }
            if !self.admit(s, None) {
                return;
            }
        }
        std::thread::scope(|scope| self.explore(Workers::new(scope, ts, threads)));
    }

    /// Expands the states in discovery order and reads their moves in that order, until none
    /// is left or a cap stops the run.
    fn explore(&mut self, mut workers: Workers) {
        // The successor being read, copied out of the worker's buffer.
        let Some(mut s2) = self.states.first().cloned() else {
            return;
        };
        // States before `next` are read; states before `sent` are read or handed out.
        let (mut next, mut sent) = (0, 0);
        loop {
            while workers.has_room() && self.states.len() - sent >= 2 * CHUNK {
                workers.hand_out(self.states[sent..sent + CHUNK].to_vec());
                sent += CHUNK;
            }
            let expanded = if next < sent {
                workers.next()
            } else if sent < self.states.len() {
                let end = self.states.len().min(sent + CHUNK);
                let expanded = expand(self.ts, &self.states[sent..end]);
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

    /// Reads `moves[range]`, the moves from `states[at]`; false when a cap stops the run.
    /// `s2` is a state of the system, overwritten with each successor in turn.
    fn take(&mut self, at: usize, moves: &Moves, range: Range<usize>, s2: &mut State) -> bool {
        let from = at as u32;
        let s = self.states[at].clone();
        for i in range {
            let (act, params, next) = moves.get(i);
            s2.0.copy_from_slice(next);
            if !self.observe(act, params, &s, s2) {
                return false;
            }
            let binding = self.bindings.id(act, params);
            let edge = self.edges.len() as u32;
            if *s2 == s {
                self.edges.push(Edge {
                    from,
                    to: from,
                    binding,
                });
                continue;
            }
            self.check_after(from, act, params, s2);
            let to = match self.seen.get(&*s2) {
                Some(&to) => to,
                None => {
                    let to = self.states.len() as u32;
                    if !self.admit(s2.clone(), Some(edge)) {
                        return false;
                    }
                    to
                }
            };
            self.edges.push(Edge { from, to, binding });
        }
        true
    }

    /// Adds a new state; false when the state cap stops the run. `parent` is the index in
    /// `edges` that the edge reaching it takes.
    fn admit(&mut self, s: State, parent: Option<u32>) -> bool {
        if self.seen.contains_key(&s) {
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
        self.check_invariants(&s, self.states.len() as u32);
        self.seen.insert(s.clone(), self.states.len() as u32);
        self.states.push(s);
        self.parents.push(parent);
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
                    domain: decl.domain.size(ts.bounds),
                    range,
                }
            })
            .collect();
        vars.sort_by(|a, b| b.reached.cmp(&a.reached).then(a.name.cmp(&b.name)));
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for edge in self.parents.iter().flatten() {
            let (act, _) = self.bindings.list[self.edges[*edge as usize].binding as usize];
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

    /// Records what one edge says about each written slot's pattern.
    fn observe(&mut self, act: ActId, params: &[u8], s: &State, s2: &State) -> bool {
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
            let pre_id = pre.then(|| {
                ts.pattern_key_into(s, e, slot, &mut before);
                self.views[e.index()]
                    .id(&before)
                    .expect("pre-state pattern interned")
            });
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
    fn violation(&mut self, name: &str, diag: Diag, reach: Reach) {
        if self.violated.insert(name.to_string()) {
            self.violations.push((self.diags.len(), reach));
            self.diags.push(diag);
        }
    }

    fn check_invariants(&mut self, s: &State, at: u32) {
        let ts = self.ts;
        for (i, inv) in ts.invariants.iter().enumerate() {
            if self.violated.contains(&inv.name) {
                continue;
            }
            if let Some(d) = ts.invariant_violation(i, s) {
                self.violation(&inv.name, d, Reach::State { at, inv: i });
            }
        }
    }

    fn check_after(&mut self, from: u32, act: ActId, params: &[u8], s2: &State) {
        let ts = self.ts;
        for (i, a) in ts.after.iter().enumerate() {
            if self.violated.contains(&a.name) {
                continue;
            }
            if let Some(d) = ts.after_violation(i, act, params, s2) {
                let reach = Reach::Step {
                    from,
                    act,
                    params: params.into(),
                    to: s2.clone(),
                    after: i,
                };
                self.violation(&a.name, d, reach);
            }
        }
    }
}
