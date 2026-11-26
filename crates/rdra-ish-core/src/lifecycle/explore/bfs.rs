//! The BFS itself: interning bindings, admitting states, and checking each move.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::ops::Range;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
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

use hashbrown::HashTable;

use super::arena::{Seen, Segment};
use super::edges::{StepEnds, Steps};
use super::probe::{Dedup, Numbering, Probe, Probes, Shared};
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
/// The fewest workers that probe the run's table for each move where probing is on
/// (`Probing::On`).
const PROBING: usize = 4;

/// Whether the workers of a run probe the run's table for each move (`Probe::Hit`), which
/// takes the look-up off the reader. A probe or its absence changes no state, number or edge
/// of the run. Off in a release build, whose run keeps hashbrown's table: on food-permit-ops
/// at 8 jobs the probes took 12% off the run, but they cost clinic-ops and insurance-claim-ops
/// 4% to 5%, and the table they need finds a state more slowly than hashbrown's, so they wait
/// for #296 item 3.1 part 3 (docs/design/native-bfs-parallel.md, "Part 2: the shared table").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Probing {
    Off,
    /// With `PROBING` workers or more: a unit test that asks for it (`tests::probing`). With
    /// up to three workers the expansion, not the reader, is what the run waits on, and a
    /// probe there only adds to it. (food-permit-ops at 4 jobs: 2.26 s with 3 workers
    /// probing, 2.19 s without, main 2.17 s.)
    #[cfg_attr(not(test), allow(dead_code))]
    On,
    /// With any worker: a `shadow` build under the test-only `RDRA_ISH_FORCE_PROBES`, so that
    /// the CI's shadow job holds every hit and every look-up of `Recent` to its twin on its
    /// 4-core runner.
    Forced,
}

#[cfg(test)]
thread_local! {
    /// Whether a run that a unit test starts on this thread probes (`Probing::On`).
    pub(super) static PROBING_IN_TEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl Probing {
    pub(super) fn of_build() -> Self {
        #[cfg(test)]
        if PROBING_IN_TEST.get() {
            return Probing::On;
        }
        if cfg!(feature = "shadow") && std::env::var_os("RDRA_ISH_FORCE_PROBES").is_some() {
            Probing::Forced
        } else {
            Probing::Off
        }
    }
}

/// The moves from a run of states, in state order, where each state's moves end, and what
/// the expanding thread read off each move (`Probes`).
struct Expanded {
    moves: Moves,
    ends: Vec<usize>,
    probes: Probes,
}

/// What a thread that expands chunks reuses from one chunk to the next.
#[derive(Default)]
struct Scratch {
    canon: Canon,
    dedup: Dedup,
}

/// What expanding a chunk of a run reads besides the chunk: the same on every thread.
#[derive(Clone, Copy)]
struct Expander<'a> {
    ts: &'a TransitionSystem,
    /// The group a run up to symmetry canonicalizes successors under.
    group: Option<&'a Group>,
    numbering: &'a Numbering,
}

/// The moves from a run of states, under symmetry each successor's canonical form beside it,
/// and each move's `Probe` and binding number: the expanding thread canonicalizes, packs,
/// hashes and numbers, so the reader only looks these up.
fn expand(
    Expander {
        ts,
        group,
        numbering,
    }: Expander,
    states: &States,
    range: Range<usize>,
    scratch: &mut Scratch,
    shared: Option<Shared>,
) -> Expanded {
    let mut moves = Moves::default();
    let mut probes = Probes::new(states.packed_width(), group.is_some());
    let Some(mut next) = states.get(range.start).map(Unpacked::into_state) else {
        return Expanded {
            moves,
            ends: Vec::new(),
            probes,
        };
    };
    scratch.dedup.start(shared);
    // The state being expanded, unpacked from the store.
    let mut at = next.clone();
    let mut ends = Vec::with_capacity(range.len());
    for i in range {
        let from = moves.len();
        states.read_into(i, &mut at.0);
        ts.moves_into(&at, &mut next, &mut moves);
        if let Some(group) = group {
            moves.canonicalize(ts, group, &mut scratch.canon, from);
        }
        probes.read(states, i, &at, &moves, from, &mut scratch.dedup);
        probes.number_bindings(numbering, &moves, from);
        ends.push(moves.len());
    }
    Expanded {
        moves,
        ends,
        probes,
    }
}

/// Workers that expand chunks of states while the caller reads the chunks before them. A
/// state's moves depend on that state alone, and the caller reads the chunks in the order it
/// handed them out, so the run is the one a single thread makes. A chunk is claimed, not
/// pushed: a worker takes the oldest one no thread has taken, and so does the caller while
/// the chunk it reads next is not done, rather than wait (#296 item 3.1).
struct Workers<'scope, 'env> {
    scope: &'scope Scope<'scope, 'env>,
    expander: Expander<'env>,
    /// How many workers the run starts; `None` until `Threads::Shared` has taken some.
    threads: Option<usize>,
    /// The helpers a `Threads::Shared` run holds, given back when the run ends.
    held: Option<Helpers>,
    probing: Probing,
    /// The chunks handed out and not yet claimed; `None` until the first one is.
    queue: Option<Arc<Queue>>,
    /// Per chunk handed out and not yet read, in order: its result once it is expanded.
    waiting: VecDeque<Option<std::thread::Result<Expanded>>>,
    /// The chunks before `waiting[0]`: how many were handed out and read.
    read: usize,
    done: Receiver<(usize, std::thread::Result<Expanded>)>,
    done_tx: Sender<(usize, std::thread::Result<Expanded>)>,
}

/// A chunk handed out: its number in hand-out order, its states, and what its successors are
/// probed against when the run probes (`Workers::probes`).
type Chunk = (usize, States, Option<Shared>);

/// Chunks handed out and not yet claimed, each with its number in hand-out order, and
/// whether the run has ended, which tells the workers to stop.
#[derive(Default)]
struct Queue {
    chunks: Mutex<(VecDeque<Chunk>, bool)>,
    ready: Condvar,
}

impl Queue {
    /// The oldest chunk no thread has claimed, waiting for one when `wait`; `None` once the
    /// run has ended, or when none is there and not `wait`.
    fn claim(&self, wait: bool) -> Option<Chunk> {
        let mut chunks = self.chunks.lock().expect("the chunk queue");
        loop {
            if chunks.1 {
                return None;
            }
            if let Some(chunk) = chunks.0.pop_front() {
                return Some(chunk);
            }
            if !wait {
                return None;
            }
            chunks = self.ready.wait(chunks).expect("the chunk queue");
        }
    }

    fn push(&self, chunk: Chunk) {
        self.chunks
            .lock()
            .expect("the chunk queue")
            .0
            .push_back(chunk);
        self.ready.notify_one();
    }

    fn close(&self) {
        if let Ok(mut chunks) = self.chunks.lock() {
            chunks.1 = true;
            chunks.0.clear();
        }
        self.ready.notify_all();
    }
}

impl Drop for Workers<'_, '_> {
    fn drop(&mut self) {
        if let Some(queue) = &self.queue {
            queue.close();
        }
    }
}

impl<'scope, 'env> Workers<'scope, 'env> {
    fn new(
        scope: &'scope Scope<'scope, 'env>,
        expander: Expander<'env>,
        threads: Threads,
        probing: Probing,
    ) -> Self {
        let (done_tx, done) = mpsc::channel();
        Workers {
            scope,
            expander,
            threads: match threads {
                Threads::Fixed(n) => Some(n),
                Threads::Shared => None,
            },
            held: None,
            probing,
            queue: None,
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

    /// Whether a worker probes the run's table for each move (`Probing`); when not, the reader
    /// looks each move up itself.
    fn probes(&self) -> bool {
        let fewest = match self.probing {
            Probing::Off => return false,
            Probing::On => PROBING,
            Probing::Forced => 1,
        };
        self.threads.is_some_and(|n| n >= fewest)
    }

    /// Hands out the chunk `states`, whose successors are probed against `shared` when there
    /// is one (`probes`).
    fn hand_out(&mut self, states: States, shared: Option<Shared>) {
        let queue = self.queue.get_or_insert_with(|| {
            let queue = Arc::new(Queue::default());
            for _ in 0..self
                .threads
                .expect("a chunk is handed out once there are workers")
            {
                let (queue, done) = (queue.clone(), self.done_tx.clone());
                let expander = self.expander;
                self.scope.spawn(move || {
                    let mut scratch = Scratch::default();
                    while let Some((number, states, shared)) = queue.claim(true) {
                        let expanded = catch_unwind(AssertUnwindSafe(|| {
                            let all = 0..states.len();
                            expand(expander, &states, all, &mut scratch, shared)
                        }));
                        if done.send((number, expanded)).is_err() {
                            break;
                        }
                    }
                });
            }
            queue
        });
        queue.push((self.read + self.waiting.len(), states, shared));
        self.waiting.push_back(None);
    }

    /// The oldest chunk handed out and not yet read; a worker's panic resumes here. While it
    /// is not done, the caller expands the chunks no worker has claimed yet, with `scratch`.
    fn next(&mut self, scratch: &mut Scratch) -> Expanded {
        while self.waiting[0].is_none() {
            let (number, expanded) = match self.done.try_recv() {
                Ok(done) => done,
                Err(_) => match self.queue.as_ref().and_then(|q| q.claim(false)) {
                    Some((number, states, shared)) => {
                        let all = 0..states.len();
                        let expanded = expand(self.expander, &states, all, scratch, shared);
                        (number, Ok(expanded))
                    }
                    None => self.done.recv().expect("a worker sends its chunk"),
                },
            };
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

    /// `id`, looked up by the binding's `number` (`Numbering`) where it has one: an array read
    /// in place of hashing the params. The first sight of a binding goes through `id`, so the
    /// ids and the bytes counted are the same. A `shadow` build holds every lookup to `id`'s
    /// map.
    #[inline]
    fn id_numbered(&mut self, ids: &mut [u32], number: u32, act: ActId, params: &[u8]) -> u32 {
        let Some(slot) = ids.get_mut(number as usize) else {
            return self.id(act, params);
        };
        if *slot == u32::MAX {
            *slot = self.id(act, params);
            return *slot;
        }
        let id = *slot;
        #[cfg(feature = "shadow")]
        {
            let want = self.index[act.index()].get(params).copied();
            assert!(
                want == Some(id),
                "shadow: Bindings::id_numbered({act:?}, {params:?}): {id}, the map {want:?}"
            );
            crate::shadow::compared(crate::shadow::Twin::DenseBinding);
        }
        id
    }
}

/// What the reader keeps while it reads a run's chunks.
struct Reading {
    /// Per move of the chunk being read: the state it reached, `u32::MAX` while unread or for
    /// a loop. A `Probe::SameAs(k)` reads it at `k`.
    resolved: Vec<u32>,
    /// Per binding number (`Numbering`): the binding's id, `u32::MAX` until it is first seen.
    ids: Vec<u32>,
    recent: Recent,
    /// Whether a chunk may be probed by its expanding thread (`Workers::probes`), so that
    /// `recent` must note each state admitted; once true, true for the rest of the run.
    record: bool,
}

/// The states admitted since the chunk being read was handed out, by the hash of their packed
/// bytes. The thread that expanded the chunk probed the run's table as it was at hand-out,
/// which holds every state below the chunk's `Probes::published`, and a `Miss` says the
/// successor is none of them; so the reader looks a `Miss` up among the states from there on
/// alone, a small table that stays in cache, rather than in the run's table. A `shadow` build
/// holds every such look-up to the run's table.
struct Recent {
    table: HashTable<(u32, u64)>,
    /// The states the table holds, in admission order, with their hashes.
    order: VecDeque<(u32, u64)>,
    /// The table holds every state from this one on.
    floor: u32,
}

impl Recent {
    /// Holding nothing, the states below `floor` being in the run's table.
    fn new(floor: usize) -> Self {
        Recent {
            table: HashTable::new(),
            order: VecDeque::new(),
            floor: u32::try_from(floor).expect("states fit in u32"),
        }
    }

    /// Holds nothing again, the states below `floor` being in the run's table, when no chunk
    /// handed out before now is still to be read.
    fn reset(&mut self, floor: usize) {
        *self = Recent::new(floor);
    }

    /// Notes state `at`, just admitted, whose packed bytes hash to `hash`.
    fn admit(&mut self, at: u32, hash: u64) {
        self.table.insert_unique(hash, (at, hash), |&(_, h)| h);
        self.order.push_back((at, hash));
    }

    /// Passes over state `at`, just admitted, while no chunk is probed: the table, empty, holds
    /// every state from the next one on.
    fn skip(&mut self, at: u32) {
        debug_assert!(
            self.order.is_empty(),
            "the states are noted from the first probe on"
        );
        self.floor = at + 1;
    }

    /// Lets go of the states below `published`: a chunk handed out after them was probed
    /// against a table that held them. Returns whether the table still holds every state from
    /// `published` on.
    fn from(&mut self, published: usize) -> bool {
        while let Some(&(at, hash)) = self.order.front() {
            if at as usize >= published {
                break;
            }
            self.order.pop_front();
            if let Ok(entry) = self.table.find_entry(hash, |&(i, _)| i == at) {
                entry.remove();
            }
            self.floor = at + 1;
        }
        self.floor as usize <= published
    }
}

impl<'t> Bfs<'t> {
    pub(super) fn new(
        ts: &'t TransitionSystem,
        caps: Caps,
        checks: &Checks,
        group: Option<&'t Group>,
    ) -> Self {
        let probing = Probing::of_build();
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
            seen: Seen::new(probing != Probing::Off),
            probing,
            packed: Vec::new(),
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
            perms: None,
        }
    }

    /// The state `key` is, which the thread that expanded its move did not find: packed
    /// `packed`, which hashes to `hash`. With `recent`, which holds every state the expanding
    /// thread could not see, only those are looked at.
    #[inline]
    fn find_missed(
        &self,
        packed: &[u8],
        hash: u64,
        key: &StateSlice,
        recent: Option<&Recent>,
    ) -> Option<u32> {
        let Some(recent) = recent else {
            return self.seen.find_hashed(&self.states, packed, hash, key);
        };
        let found = recent
            .table
            .find(hash, |&(i, _)| self.states.packed_row(i as usize) == packed)
            .map(|&(i, _)| i);
        #[cfg(feature = "shadow")]
        {
            let want = self.seen.find_hashed(&self.states, packed, hash, key);
            assert!(
                found == want,
                "shadow: Recent::find({:?}): {found:?}, the run's table {want:?}",
                &key.0
            );
            crate::shadow::compared(crate::shadow::Twin::Recent);
        }
        #[cfg(not(feature = "shadow"))]
        let _ = key;
        found
    }

    /// What a chunk handed out now probes its successors against: the run's table and the
    /// states admitted so far. `segments` caches the store's segment addresses.
    fn shared(&self, segments: &mut Option<Arc<[Segment]>>) -> Shared {
        Shared {
            table: self
                .seen
                .current()
                .expect("a run that probes shares its table"),
            rows: self.states.publish(segments),
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
        let numbering = Numbering::of(ts);
        let expander = Expander {
            ts,
            group: self.group,
            numbering: &numbering,
        };
        let probing = self.probing;
        std::thread::scope(|scope| {
            self.explore(Workers::new(scope, expander, threads, probing));
        });
    }

    /// Expands the states in discovery order and reads their moves in that order, until none
    /// is left or a cap stops the run.
    fn explore(&mut self, mut workers: Workers) {
        // The state being read, unpacked from the store when views are gathered.
        let Some(mut cur) = self.states.first().map(Unpacked::into_state) else {
            return;
        };
        let mut reading = Reading {
            resolved: Vec::new(),
            ids: vec![u32::MAX; workers.expander.numbering.len()],
            recent: Recent::new(self.states.len()),
            record: false,
        };
        let mut scratch = Scratch::default();
        // The addresses of the store's segments, kept while no segment is added.
        let mut segments = None;
        // States before `next` are read; states before `sent` are read or handed out.
        let (mut next, mut sent) = (0, 0);
        loop {
            while self.states.len() - sent >= 2 * CHUNK && workers.has_room() {
                let shared = workers.probes().then(|| self.shared(&mut segments));
                workers.hand_out(self.states.slice(sent..sent + CHUNK), shared);
                sent += CHUNK;
            }
            let expanded = if next < sent {
                workers.next(&mut scratch)
            } else if sent < self.states.len() {
                let end = self.states.len().min(sent + CHUNK);
                // The reader probes the moves it expands as it reads them, not twice.
                let range = sent..end;
                let expanded = expand(workers.expander, &self.states, range, &mut scratch, None);
                sent = end;
                expanded
            } else {
                return;
            };
            reading.resolved.clear();
            reading.resolved.resize(expanded.moves.len(), u32::MAX);
            reading.record = workers.probes();
            let recent = match expanded.probes.published() {
                // No thread probed the chunk; no chunk handed out before it is still unread.
                0 => {
                    reading.recent.reset(self.states.len());
                    false
                }
                published => reading.recent.from(published),
            };
            let mut from = 0;
            for &end in &expanded.ends {
                if !self.take(next, &expanded, from..end, &mut cur, &mut reading, recent) {
                    return;
                }
                from = end;
                next += 1;
            }
        }
    }

    /// Reads `moves[range]` of `chunk`, the moves from `states[at]`; false when a cap stops
    /// the run, or the last open check fails under `Caps::stop_when_all_violated`. `s` is a
    /// state of the system, overwritten with `states[at]` when the run gathers views. With
    /// `recent`, `reading.recent` holds every state the chunk's expanding thread could not see.
    fn take(
        &mut self,
        at: usize,
        chunk: &Expanded,
        range: Range<usize>,
        s: &mut State,
        reading: &mut Reading,
        recent: bool,
    ) -> bool {
        let (moves, probes) = (&chunk.moves, &chunk.probes);
        self.layers.read(at);
        self.steps.read(at);
        let from = at as u32;
        let views = self.caps.views;
        if views {
            self.states.read_into(at, &mut s.0);
        }
        let s: &State = s;
        for i in range {
            let (act, params, next) = moves.get(i);
            let s2 = StateSlice::new(next);
            if views && !self.observe(at, act, params, s, s2) {
                return false;
            }
            let number = probes.number(i);
            let binding = self
                .bindings
                .id_numbered(&mut reading.ids, number, act, params);
            let probe = probes.probe(i);
            if probe == Probe::Loop {
                // A concrete self-loop: its permutation is the identity.
                if let Some(perms) = &mut self.perms {
                    perms.push_identity();
                }
                self.push(from, from, act, binding, s2);
                continue;
            }
            self.check_after(from, act, params, s2);
            // Under symmetry the run stores the successor's canonical form; a move into the
            // class of its own source is an orbit self-edge, a real step stored as a loop.
            // An annotated run keeps the permutation beside the edge pushed below; a plain run
            // reads nothing more per move (`Bfs::perms`). A run a cap stops here keeps one more
            // permutation than edges, which `bfs_on` drops.
            let (key, found) = match self.group {
                Some(_) => {
                    if let Some(perms) = &mut self.perms {
                        perms.push(moves.perm(i));
                    }
                    moves.canonical(i)
                }
                None => (next, Canonical::PLAIN),
            };
            let key = StateSlice::new(key);
            if probe == Probe::OrbitSelf {
                let edge = u32::try_from(self.steps.moves()).expect("edges fit in u32");
                // The key is the source: the stored target of the loop.
                self.push(from, from, act, binding, key);
                self.orbit_self.push(edge);
                // The step's after-asserts were read on its concrete successor above.
                if self.stop_if_all_failed() {
                    return false;
                }
                continue;
            }
            // The successor packed as the store keeps it, by the thread that expanded it: a
            // plain run's patched from the source's packed row, a canonical form packed whole.
            let (packed, hash) = probes.packed(i);
            let to = match probe {
                Probe::Hit(to) => {
                    #[cfg(feature = "shadow")]
                    {
                        let want = self.seen.twin_find(key);
                        assert!(
                            want == Some(to),
                            "shadow: Probe::Hit({to}) at move {i} of state {at}: reference {want:?}"
                        );
                        crate::shadow::compared(crate::shadow::Twin::Hit);
                    }
                    to
                }
                Probe::SameAs(k) => {
                    let to = reading.resolved[k as usize];
                    #[cfg(feature = "shadow")]
                    {
                        let want = self.seen.twin_find(key);
                        assert!(
                            want == Some(to),
                            "shadow: Probe::SameAs({k}) at move {i} of state {at}: the earlier \
                             move reached {to}, reference {want:?}"
                        );
                        crate::shadow::compared(crate::shadow::Twin::SameAs);
                    }
                    to
                }
                _ => match self.find_missed(packed, hash, key, recent.then_some(&reading.recent)) {
                    Some(to) => to,
                    None => {
                        let to = self.states.len() as u32;
                        if !self.admit_packed(key, packed, hash, Some((from, binding)), found) {
                            return false;
                        }
                        match reading.record {
                            true => reading.recent.admit(to, hash),
                            false => reading.recent.skip(to),
                        }
                        to
                    }
                },
            };
            reading.resolved[i] = to;
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
        let mut packed = std::mem::take(&mut self.packed);
        self.states.pack(s, &mut packed);
        let hash = super::arena::hash(&packed);
        let admitted = (self.seen.find_hashed(&self.states, &packed, hash, s)).is_some()
            || self.admit_packed(s, &packed, hash, parent, found);
        self.packed = packed;
        admitted
    }

    /// `admit`, of a state `seen` does not hold, given `packed`, what `States::pack` made of
    /// it, and its `hash`.
    fn admit_packed(
        &mut self,
        s: &StateSlice,
        packed: &[u8],
        hash: u64,
        parent: Option<(u32, u32)>,
        found: Canonical,
    ) -> bool {
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
        self.states.push_packed(packed, s);
        self.seen
            .insert_hashed(&self.states, self.states.len() as u32 - 1, hash);
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

#[cfg(test)]
mod tests;
