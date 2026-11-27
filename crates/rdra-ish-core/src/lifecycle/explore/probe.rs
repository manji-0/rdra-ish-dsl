//! What the thread that expands a chunk reads off each move besides the move itself (#296 item
//! 3.1, `docs/design/native-bfs-parallel.md`): whether the move is a self-loop, its successor
//! packed as the store keeps it, the hash of that, whether an earlier move of the same chunk
//! reached the same successor, and the number of the move's binding (`Numbering`). Each of
//! these is a function of the expanded state and the move alone, so a worker computes it and
//! the reader, which reads the moves in order, only looks it up. The reader still decides every
//! number a state gets: a `Miss` here is a hint it checks against its own table.

use std::sync::Arc;

use hashbrown::HashTable;

use crate::lifecycle::expr::StateSlice;

use crate::lifecycle::types::ActId;
use crate::lifecycle::TransitionSystem;

use super::arena::{hash, Published, States};
use super::table::Table;
use super::Moves;

/// What the reader does with one move, as the expanding thread found it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Probe {
    /// The successor is the source: a self-loop, which the reader stores without a lookup.
    Loop,
    /// Under symmetry: the successor's canonical form is the source, an orbit self-edge.
    OrbitSelf,
    /// The successor is state `i`, admitted before the chunk was handed out: found in the
    /// run's table (`Shared`), its row compared.
    Hit(u32),
    /// The successor is the one move `k` of the same chunk reached, an earlier move: the
    /// reader takes the state it found for `k`.
    SameAs(u32),
    /// No earlier move of the chunk reached it: the reader looks it up.
    Miss,
}

/// Per move of a chunk, in move order: its `Probe`, and for a `SameAs` or a `Miss` the
/// successor packed as the store keeps it (its canonical form under symmetry) and that
/// packing's hash. A loop's packed bytes and hash are zero.
#[derive(Default)]
pub(super) struct Probes {
    probe: Vec<Probe>,
    hash: Vec<u64>,
    packed: Vec<u8>,
    /// Per move: its binding's number, `Numbering::NONE` for none.
    number: Vec<u32>,
    width: usize,
    /// The run is up to symmetry: a move's key is its successor's canonical form.
    canonical: bool,
    /// The states the moves were probed against: those below this number (`Shared`), none
    /// when it is 0. A `Miss` is none of them.
    published: usize,
}

/// The moves of one chunk that a later move may repeat, by the hash of their packed
/// successor: scratch for the thread that expands it, emptied per chunk. With the run's table
/// as it was when the chunk was handed out (`shared`).
#[derive(Default)]
pub(super) struct Dedup {
    moves: HashTable<u32>,
    shared: Option<Shared>,
}

/// What a worker probes a successor against: the run's table and the rows it names, as the
/// reader published them with the chunk. A probe accepts an index only below the published
/// length and only when the row there is the successor, so a `Hit` is a state the run holds
/// with that number, whatever the table's buckets said while the reader wrote them.
#[derive(Clone)]
pub(super) struct Shared {
    pub(super) table: Arc<Table>,
    pub(super) rows: Published,
}

impl Probes {
    /// No moves yet, of successors packed into `width` bytes; under symmetry (`canonical`)
    /// each move is keyed by its successor's canonical form.
    pub(super) fn new(width: usize, canonical: bool) -> Self {
        Probes {
            width,
            canonical,
            ..Probes::default()
        }
    }

    /// The states the moves were probed against: those below this.
    pub(super) fn published(&self) -> usize {
        self.published
    }

    pub(super) fn probe(&self, i: usize) -> Probe {
        self.probe[i]
    }

    /// Move `i`'s binding's number (`Numbering`), `Numbering::NONE` for none.
    pub(super) fn number(&self, i: usize) -> u32 {
        self.number[i]
    }

    /// Numbers the bindings of the moves of `moves` from `from` on.
    pub(super) fn number_bindings(&mut self, numbering: &Numbering, moves: &Moves, from: usize) {
        for i in from..moves.len() {
            let (act, params, _) = moves.get(i);
            self.number.push(numbering.number(act, params));
        }
    }

    /// Move `i`'s packed successor and its hash.
    pub(super) fn packed(&self, i: usize) -> (&[u8], u64) {
        (
            &self.packed[i * self.width..(i + 1) * self.width],
            self.hash[i],
        )
    }

    /// Reads the moves of `moves` from `from` on, which all leave `states[at]`, unpacked in
    /// `source`; under symmetry their canonical forms are computed. `dedup` holds the earlier
    /// moves of the chunk.
    pub(super) fn read(
        &mut self,
        states: &States,
        at: usize,
        source: &StateSlice,
        moves: &Moves,
        from: usize,
        dedup: &mut Dedup,
    ) {
        self.published = dedup.shared.as_ref().map_or(0, |s| s.rows.len());
        let w = self.width;
        for i in from..moves.len() {
            debug_assert_eq!(self.probe.len(), i);
            let (_, _, next) = moves.get(i);
            let start = self.packed.len();
            self.packed.resize(start + w, 0);
            if next == &source.0[..] {
                self.probe.push(Probe::Loop);
                self.hash.push(0);
                continue;
            }
            let out = &mut self.packed[start..];
            if self.canonical {
                let (key, _) = moves.canonical(i);
                if key == &source.0[..] {
                    self.probe.push(Probe::OrbitSelf);
                    self.hash.push(0);
                    continue;
                }
                states.pack_into(StateSlice::new(key), out);
            } else {
                // A plain run's key is the successor itself: patch the source's packed row with
                // the cells the move changed.
                states.patch(at, source, StateSlice::new(next), out);
            }
            let h = hash(out);
            self.hash.push(h);
            let packed = &self.packed;
            let row = |k: u32| &packed[k as usize * w..(k as usize + 1) * w];
            let mine = &packed[start..];
            let hit = dedup.shared.as_ref().and_then(|shared| {
                shared.table.find(h, |k| {
                    // SAFETY: `Bfs::explore` published these rows from its run's store, which
                    // outlives every chunk it hands out (`Published::row`).
                    let row = unsafe { shared.rows.row(k as usize) };
                    row == Some(mine)
                })
            });
            if let Some(k) = hit {
                self.probe.push(Probe::Hit(k));
                continue;
            }
            let probe = match dedup.moves.find(h, |&k| row(k) == mine) {
                Some(&k) => Probe::SameAs(k),
                None => {
                    let hashes = &self.hash;
                    let at = u32::try_from(i).expect("a chunk's moves fit in u32");
                    dedup.moves.insert_unique(h, at, |&k| hashes[k as usize]);
                    Probe::Miss
                }
            };
            self.probe.push(probe);
        }
    }
}

impl Dedup {
    /// Empty, for a new chunk, which probes `shared` when there is one.
    pub(super) fn start(&mut self, shared: Option<Shared>) {
        self.moves.clear();
        self.shared = shared;
    }
}

/// Each binding `(act, params)` of a system numbered densely: per action an offset, and the
/// params read as digits in the radix of their entities' slot counts. A system with more than
/// `MOST` bindings numbers none.
pub(super) struct Numbering {
    /// Per action: the number of its first binding, and the slot count of each param.
    actions: Vec<(u32, Box<[u8]>)>,
    total: usize,
}

impl Numbering {
    /// The most bindings numbered: the reader keeps a 4-byte id per number.
    const MOST: usize = 1 << 16;

    /// The number of a binding that has none. It is past `MOST`, so an array of one entry per
    /// number (`Bindings::id_numbered`) has no entry for it.
    pub(super) const NONE: u32 = u32::MAX;

    pub(super) fn of(ts: &TransitionSystem) -> Self {
        let mut total = 0usize;
        let mut actions = Vec::with_capacity(ts.actions.len());
        for action in &ts.actions {
            let radix: Box<[u8]> = action
                .params
                .iter()
                .map(|p| ts.entity(p.entity).slots.get())
                .collect();
            let count = radix
                .iter()
                .try_fold(1usize, |n, &r| n.checked_mul(usize::from(r)));
            match count
                .and_then(|c| total.checked_add(c))
                .filter(|&t| t <= Self::MOST)
            {
                Some(t) => {
                    actions.push((total as u32, radix));
                    total = t;
                }
                None => {
                    return Numbering {
                        actions: Vec::new(),
                        total: 0,
                    }
                }
            }
        }
        Numbering { actions, total }
    }

    /// How many numbers there are.
    pub(super) fn len(&self) -> usize {
        self.total
    }

    /// The number of `(act, params)`, `NONE` when the system has too many bindings, or a
    /// param is past its radix, which no move of the system makes.
    #[inline]
    pub(super) fn number(&self, act: ActId, params: &[u8]) -> u32 {
        let Some((first, radix)) = self.actions.get(act.index()) else {
            return Self::NONE;
        };
        if radix.len() != params.len() {
            return Self::NONE;
        }
        let mut at = 0u32;
        for (&p, &r) in params.iter().zip(radix.iter()) {
            if p >= r {
                return Self::NONE;
            }
            at = at * u32::from(r) + u32::from(p);
        }
        first + at
    }
}
