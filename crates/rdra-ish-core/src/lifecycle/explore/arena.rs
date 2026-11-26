//! A run's states, end to end in segments (`segments::Rows`): each state is kept once, packed
//! into the bits its cells' domains need (`packing`, #296 2.3), with no heap block or pointer
//! of its own, and stays where it was pushed. A reader gets a state unpacked, by value
//! (`Unpacked`), never a borrow of the store.

use std::hash::Hasher;
use std::ops::{Deref, Range};
use std::sync::Arc;

use hashbrown::HashTable;

use crate::fasthash::FastHasher;
use crate::lifecycle::expr::{State, StateSlice};

use super::packing::Packing;
use super::segments::{locate, Rows};
use super::table::{Table, Writer};

#[cfg(feature = "shadow")]
mod reference;

/// States of `width` bytes each, numbered in the order pushed, kept packed.
#[derive(Clone, Debug)]
pub struct States {
    packing: Arc<Packing>,
    rows: Rows<u8>,
    /// Under the `shadow` feature: the states unpacked, which every read is compared with.
    #[cfg(feature = "shadow")]
    twin: Rows<u8>,
}

impl Default for States {
    fn default() -> Self {
        States::new(0)
    }
}

impl PartialEq for States {
    fn eq(&self, other: &Self) -> bool {
        self.width() == other.width() && self.iter().eq(other.iter())
    }
}

impl Eq for States {}

/// The bytes of a state an `Unpacked` holds in place; every sample's state is narrower.
const INLINE: usize = 96;

/// A state of a `States`, unpacked: up to `INLINE` bytes in place, more on the heap.
#[derive(Clone)]
pub enum Unpacked {
    Inline(u8, [u8; INLINE]),
    Heap(Box<[u8]>),
}

impl Unpacked {
    fn zeroed(width: usize) -> Unpacked {
        match u8::try_from(width) {
            Ok(w) if width <= INLINE => Unpacked::Inline(w, [0; INLINE]),
            _ => Unpacked::Heap(vec![0; width].into_boxed_slice()),
        }
    }

    /// The state, as an owned `State`.
    pub fn into_state(self) -> State {
        (*self).to_owned()
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        match self {
            Unpacked::Inline(w, b) => &mut b[..usize::from(*w)],
            Unpacked::Heap(b) => b,
        }
    }
}

impl Deref for Unpacked {
    type Target = StateSlice;

    #[inline]
    fn deref(&self) -> &StateSlice {
        StateSlice::new(match self {
            Unpacked::Inline(w, b) => &b[..usize::from(*w)],
            Unpacked::Heap(b) => b,
        })
    }
}

impl std::borrow::Borrow<StateSlice> for Unpacked {
    fn borrow(&self) -> &StateSlice {
        self
    }
}

impl AsRef<StateSlice> for Unpacked {
    fn as_ref(&self) -> &StateSlice {
        self
    }
}

impl PartialEq for Unpacked {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl Eq for Unpacked {}

impl PartialEq<StateSlice> for Unpacked {
    fn eq(&self, other: &StateSlice) -> bool {
        **self == *other
    }
}

impl PartialEq<State> for Unpacked {
    fn eq(&self, other: &State) -> bool {
        **self == **other
    }
}

impl PartialEq<Unpacked> for State {
    fn eq(&self, other: &Unpacked) -> bool {
        **self == **other
    }
}

impl std::fmt::Debug for Unpacked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (**self).fmt(f)
    }
}

impl From<Unpacked> for State {
    fn from(s: Unpacked) -> State {
        (*s).to_owned()
    }
}

/// Runs `f` on a buffer of `n` zero bytes, on the stack when it fits `INLINE`.
#[inline]
fn with_scratch<R>(n: usize, f: impl FnOnce(&mut [u8]) -> R) -> R {
    if n <= INLINE {
        f(&mut [0u8; INLINE][..n])
    } else {
        f(&mut vec![0u8; n])
    }
}

impl States {
    /// States of `width` bytes, each byte kept whole.
    pub fn new(width: usize) -> Self {
        States::packed(Arc::new(Packing::new(std::iter::repeat_n(256, width))))
    }

    /// States packed as `packing` says.
    pub(crate) fn packed(packing: Arc<Packing>) -> Self {
        States {
            rows: Rows::new(packing.bytes()),
            #[cfg(feature = "shadow")]
            twin: Rows::new(packing.width()),
            packing,
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.len() == 0
    }

    /// Bytes in one state, unpacked.
    pub fn width(&self) -> usize {
        self.packing.width()
    }

    /// Bytes in one state as the store keeps it.
    pub(crate) fn packed_width(&self) -> usize {
        self.packing.bytes()
    }

    /// State `i`, unpacked. Panics when there is none.
    #[inline]
    pub fn at(&self, i: usize) -> Unpacked {
        let mut out = Unpacked::zeroed(self.width());
        self.packing.unpack(self.rows.row(i), out.bytes_mut());
        #[cfg(feature = "shadow")]
        {
            let want = self.twin.row(i);
            let got: &[u8] = &out.0;
            assert!(
                got == want,
                "shadow: States::at({i}): unpacked {got:?}, reference {want:?}"
            );
            crate::shadow::compared(crate::shadow::Twin::Packed);
        }
        out
    }

    /// State `i` unpacked into `out`, which is `width()` long.
    #[inline]
    pub(crate) fn read_into(&self, i: usize, out: &mut [u8]) {
        self.packing.unpack(self.rows.row(i), out);
        #[cfg(feature = "shadow")]
        {
            let want = self.twin.row(i);
            assert!(
                *out == *want,
                "shadow: States::read_into({i}): unpacked {out:?}, reference {want:?}"
            );
            crate::shadow::compared(crate::shadow::Twin::Packed);
        }
    }

    /// Cell `c` of state `i`, read from its packed bits alone.
    #[inline]
    pub(crate) fn cell(&self, i: usize, c: usize) -> u8 {
        let got = self.packing.cell(self.rows.row(i), c);
        #[cfg(feature = "shadow")]
        {
            let want = self.twin.row(i)[c];
            assert!(
                got == want,
                "shadow: States::cell({i}, {c}): packed {got}, reference {want}"
            );
            crate::shadow::compared(crate::shadow::Twin::Packed);
        }
        got
    }

    /// `s` packed as the store keeps it, into `out`.
    #[inline]
    pub(crate) fn pack(&self, s: &StateSlice, out: &mut Vec<u8>) {
        out.resize(self.packing.bytes(), 0);
        self.packing.pack(&s.0, out);
    }

    /// `pack` into `out`, which is `packed_width()` long.
    #[inline]
    pub(crate) fn pack_into(&self, s: &StateSlice, out: &mut [u8]) {
        self.packing.pack(&s.0, out);
    }

    /// `to` packed as the store keeps it, into `out`, which is `packed_width()` long, given
    /// `from`, which is state `i`: only the cells where the two differ are packed. Under the
    /// `shadow` feature the whole pack is compared with it.
    #[inline]
    pub(crate) fn patch(&self, i: usize, from: &StateSlice, to: &StateSlice, out: &mut [u8]) {
        self.packing.patch(&from.0, self.rows.row(i), &to.0, out);
        #[cfg(feature = "shadow")]
        {
            let mut want = vec![0u8; self.packing.bytes()];
            self.packing.pack(&to.0, &mut want);
            assert!(
                *out == want,
                "shadow: States::patch({i}): patched {out:?}, packed {want:?}"
            );
            crate::shadow::compared(crate::shadow::Twin::Packed);
        }
    }

    /// State `i`'s packed bytes.
    pub(super) fn packed_row(&self, i: usize) -> &[u8] {
        self.rows.row(i)
    }

    pub fn get(&self, i: usize) -> Option<Unpacked> {
        (i < self.len()).then(|| self.at(i))
    }

    pub fn first(&self) -> Option<Unpacked> {
        self.get(0)
    }

    pub fn last(&self) -> Option<Unpacked> {
        self.len().checked_sub(1).map(|i| self.at(i))
    }

    pub fn iter(&self) -> Iter<'_> {
        Iter {
            states: self,
            at: 0..self.len(),
        }
    }

    /// A copy of the states in `range`, packed alike.
    pub(crate) fn slice(&self, range: Range<usize>) -> States {
        let mut out = States::packed(self.packing.clone());
        for i in range {
            out.rows.push(self.rows.row(i));
            #[cfg(feature = "shadow")]
            out.twin.push(self.twin.row(i));
        }
        out
    }

    /// Adds `s` at index `len()`, packed, in the last segment, or in a new one when that is
    /// full. No state already pushed moves.
    pub(crate) fn push(&mut self, s: &StateSlice) {
        with_scratch(self.packing.bytes(), |packed| {
            self.packing.pack(&s.0, packed);
            self.rows.push(packed);
        });
        #[cfg(feature = "shadow")]
        self.twin.push(&s.0);
    }

    /// `push(s)`, given `packed`, what `pack` made of `s`.
    pub(crate) fn push_packed(&mut self, packed: &[u8], s: &StateSlice) {
        debug_assert_eq!(packed.len(), self.packing.bytes());
        self.rows.push(packed);
        #[cfg(feature = "shadow")]
        self.twin.push(&s.0);
        #[cfg(not(feature = "shadow"))]
        let _ = s;
    }

    /// The states pushed so far, for threads that read them while this one pushes more
    /// (`Published`). `segments` caches the segments' addresses from one call to the next.
    pub(super) fn publish(&self, segments: &mut Option<Arc<[Segment]>>) -> Published {
        let count = self.rows.segments_used();
        if segments.as_ref().is_none_or(|s| s.len() != count) {
            *segments = Some(self.rows.segment_ptrs().into_iter().map(Segment).collect());
        }
        Published {
            segments: segments.clone().expect("just set"),
            len: self.len(),
            packing: self.packing.clone(),
        }
    }

    /// The states the buffer holds room for.
    #[cfg(kani)]
    pub(crate) fn capacity(&self) -> usize {
        self.rows.capacity()
    }

    /// The bytes the buffer takes once `more` states are pushed.
    pub(crate) fn bytes_with(&self, more: usize) -> u64 {
        self.rows.bytes_with(more)
    }

    /// The bytes of `bytes_with` that the list of segments takes.
    #[cfg(feature = "shadow")]
    pub(crate) fn list_bytes_with(&self, more: usize) -> u64 {
        self.rows.parts_with(more).1
    }
}

/// The states of a `States` in order, unpacked.
#[derive(Clone)]
pub struct Iter<'a> {
    states: &'a States,
    at: Range<usize>,
}

impl Iterator for Iter<'_> {
    type Item = Unpacked;

    fn next(&mut self) -> Option<Unpacked> {
        self.at.next().map(|i| self.states.at(i))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.at.size_hint()
    }
}

impl DoubleEndedIterator for Iter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.at.next_back().map(|i| self.states.at(i))
    }
}

impl ExactSizeIterator for Iter<'_> {}

impl<'a> IntoIterator for &'a States {
    type Item = Unpacked;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

impl FromIterator<State> for States {
    /// The states in order, each byte kept whole; they must all have the first one's width.
    fn from_iter<I: IntoIterator<Item = State>>(states: I) -> Self {
        let mut states = states.into_iter().peekable();
        let width = states.peek().map_or(0, |s| s.0.len());
        let mut out = States::new(width);
        for s in states {
            out.push(&s);
        }
        out
    }
}

/// The address of a segment of a `States`' rows.
#[derive(Clone, Copy)]
pub(super) struct Segment(*const u8);

// SAFETY: a `Segment` is an address that `Published::row` reads through, under the contract
// stated there; sending or sharing the address itself reads nothing.
unsafe impl Send for Segment {}
unsafe impl Sync for Segment {}

/// The packed rows of a `States` below `len`, which a worker reads while the reader pushes more
/// (#296 item 3.1): the addresses of the store's segments, taken when the rows were published.
/// A segment is allocated whole and never moves or grows (`segments::Rows`), and a row, once
/// pushed, is never written again, so the rows below `len` are settled memory. The reader
/// hands a `Published` to a worker with a chunk, through the chunk queue's lock, after it
/// pushed those rows: that hand-off orders the rows' bytes before the worker's reads.
#[derive(Clone)]
pub(super) struct Published {
    segments: Arc<[Segment]>,
    len: usize,
    packing: Arc<Packing>,
}

impl Published {
    /// How many states were published: rows `0..len()`.
    pub(super) fn len(&self) -> usize {
        self.len
    }

    /// Packed row `i`, if it was pushed before this was published.
    ///
    /// # Safety
    ///
    /// The `States` this was published from must be alive, and must not have been dropped or
    /// replaced since: only then are its segments where `segments` says. `Bfs::explore`
    /// publishes its run's store and keeps the run alive, its store in place, until every
    /// worker of the run has stopped (`std::thread::scope`).
    #[inline]
    pub(super) unsafe fn row(&self, i: usize) -> Option<&[u8]> {
        if i >= self.len {
            return None;
        }
        let (k, at) = locate(i);
        let base = self.segments[k].0;
        let width = self.packing.bytes();
        // SAFETY: row `i < len` lies in segment `k`, which was allocated with room for its rows
        // of `width` bytes and holds row `i` at `at`; the caller keeps the segment alive, and
        // the row's bytes were written before this was published and are never written again.
        Some(unsafe { std::slice::from_raw_parts(base.add(at * width), width) })
    }
}

/// The hash `Seen` files a packed state under.
#[inline]
pub(super) fn hash(packed: &[u8]) -> u64 {
    let mut hasher = FastHasher::default();
    hasher.write(packed);
    hasher.finish()
}

/// The index of each state in a `States`, found by its packed bytes: the table holds the index
/// alone. Under the `shadow` feature it keeps its reference twin, keyed on the unpacked bytes,
/// beside the table, so a packing that makes two states alike shows.
pub(super) struct Seen {
    table: Index,
    #[cfg(feature = "shadow")]
    twin: reference::Seen,
    /// Under the `shadow` feature: a hashbrown table filed as `insert_unique` grows it, which
    /// every insert holds the table's growth to (a shared table's, or one grown in store
    /// order), so `bytes_with` charges what hashbrown's growth gives.
    #[cfg(feature = "shadow")]
    hashbrown: HashTable<u32>,
}

/// `Seen::find_hashed` in a shared table, kept out of line so that the look-up of a run that
/// does not probe, a release build's, stays as it was.
#[cold]
#[inline(never)]
fn find_shared(table: &Writer, states: &States, packed: &[u8], hash: u64) -> Option<u32> {
    table.find(hash, |i| states.packed_row(i as usize) == packed)
}

/// A hashbrown table that holds indices `0..len`, filed in that order under `hash_of`, with
/// room for one more: what `insert_unique` grows a full table of `len` to, with the capacity
/// hashbrown's own growth gives (`with_capacity(len + 1)`; the `seen_growth` twin holds the
/// two equal). hashbrown's growth walks the old buckets and hashes each index again, which
/// reads the store's rows in no order; filing them in store order reads them as they lie,
/// which took a rebuild of synthetic rows from about 125 to about 70 cycles a state in a
/// microbenchmark, at the same instructions (#296 item 3.1, follow-up (a)).
#[cold]
#[inline(never)]
fn grown_in_store_order(len: usize, hash_of: impl Fn(u32) -> u64) -> HashTable<u32> {
    let mut table = HashTable::with_capacity(len + 1);
    for i in 0..u32::try_from(len).expect("states fit in u32") {
        table.insert_unique(hash_of(i), i, |&k| hash_of(k));
    }
    table
}

/// `Seen::insert_hashed` in a shared table, out of line as `find_shared` is.
#[cold]
#[inline(never)]
fn insert_shared(table: &mut Writer, states: &States, at: u32, hash_of_at: u64) {
    table.insert(hash_of_at, at, |i| hash(states.packed_row(i as usize)));
}

/// The table itself. A run whose workers probe it (#296 item 3.1) keeps the table they can
/// read while the reader inserts (`table::Writer`); any other run keeps hashbrown's, whose
/// look-up is faster: in a microbenchmark of 4.6 million states the shared table took 1.5
/// times as long to find a state (`docs/design/native-bfs-parallel.md`, "Part 2: the shared
/// table").
enum Index {
    Local(HashTable<u32>),
    Shared(Writer),
}

impl Seen {
    /// An empty table, which workers can probe when `shared`.
    pub fn new(shared: bool) -> Self {
        Seen {
            table: match shared {
                true => Index::Shared(Writer::default()),
                false => Index::Local(HashTable::new()),
            },
            #[cfg(feature = "shadow")]
            twin: Default::default(),
            #[cfg(feature = "shadow")]
            hashbrown: HashTable::new(),
        }
    }

    /// The index of `s` in `states`, if it is there, given `packed`, what `States::pack` made
    /// of `s`, and its `hash`.
    #[inline]
    pub fn find_hashed(
        &self,
        states: &States,
        packed: &[u8],
        hash: u64,
        s: &StateSlice,
    ) -> Option<u32> {
        let found = match &self.table {
            Index::Local(table) => table
                .find(hash, |&i| states.packed_row(i as usize) == packed)
                .copied(),
            Index::Shared(table) => find_shared(table, states, packed, hash),
        };
        #[cfg(not(feature = "shadow"))]
        let _ = s;
        #[cfg(feature = "shadow")]
        {
            let want = self.twin.find(s);
            assert!(
                found == want,
                "shadow: Seen::find({:?}): table {found:?}, reference {want:?}",
                &s.0
            );
            crate::shadow::compared(crate::shadow::Twin::Seen);
        }
        found
    }

    /// The index the reference twin holds for `s`: what a `Probe::SameAs` or a `Probe::Hit`
    /// must name.
    #[cfg(feature = "shadow")]
    pub fn twin_find(&self, s: &StateSlice) -> Option<u32> {
        self.twin.find(s)
    }

    /// The table as it is now, for a worker to probe while the reader goes on (`Published`),
    /// when the run keeps a shared one.
    pub fn current(&self) -> Option<Arc<Table>> {
        match &self.table {
            Index::Local(_) => None,
            Index::Shared(table) => Some(table.current()),
        }
    }

    /// Notes that state `at` of `states` is not yet in the table and now is, given the hash of
    /// its packed bytes.
    pub fn insert_hashed(&mut self, states: &States, at: u32, hash_of_at: u64) {
        debug_assert_eq!(hash_of_at, hash(states.packed_row(at as usize)));
        let hash_of = |i: u32| hash(states.packed_row(i as usize));
        match &mut self.table {
            Index::Local(table) => {
                if table.len() == table.capacity() {
                    *table = grown_in_store_order(table.len(), hash_of);
                }
                table.insert_unique(hash_of_at, at, |&i| hash_of(i));
            }
            Index::Shared(table) => insert_shared(table, states, at, hash_of_at),
        }
        #[cfg(feature = "shadow")]
        {
            assert!(
                self.twin.insert(&states.at(at as usize), at),
                "shadow: Seen::insert({at}): the reference holds {:?} already",
                &states.at(at as usize).0
            );
            self.hashbrown
                .insert_unique(hash_of_at, at, |&i| hash_of(i));
            let (_, capacity) = self.len_and_capacity();
            assert!(
                capacity == self.hashbrown.capacity(),
                "shadow: Seen::insert({at}): capacity {capacity}, hashbrown's {}",
                self.hashbrown.capacity()
            );
            crate::shadow::compared(crate::shadow::Twin::SeenGrowth);
        }
    }

    /// How many indices the table holds, and how many it holds room for.
    fn len_and_capacity(&self) -> (usize, usize) {
        match &self.table {
            Index::Local(table) => (table.len(), table.capacity()),
            Index::Shared(table) => (table.len(), table.capacity()),
        }
    }

    /// The bytes the table takes once `more` indices are inserted: one control byte and one
    /// index per bucket, at 7/8 load, doubling when full, as hashbrown's table grows and the
    /// shared table grows with it.
    pub fn bytes_with(&self, more: usize) -> u64 {
        let buckets = |cap: usize| match cap {
            0 => 0,
            c if c < 8 => (c + 1).next_power_of_two(),
            c => (c * 8 / 7).next_power_of_two(),
        };
        let (len, capacity) = self.len_and_capacity();
        let mut n = buckets(capacity);
        if len + more > capacity {
            n = (n * 2).max(4);
        }
        (n * (size_of::<u32>() + 1)) as u64
    }
}

/// Kani harnesses (`cargo kani -p rdra-ish-core`).
#[cfg(kani)]
mod proofs {
    use super::*;

    const PUSHES: usize = 9;

    /// For states of one byte and every run of up to 9 pushes, past the doubling segments (4
    /// and 4 states under Kani) into a fixed one: each state reads back as pushed and stays at
    /// the address it was pushed to, no index leaves the buffer, and `bytes_with(1)` before a
    /// push is `bytes_with(0)` after it, which is what the store's buffers hold and what the
    /// memory limit charges. (At widths up to 2 CBMC ran 20 minutes and 4.5 GB; the unit
    /// tests of `segments` push wider rows at the build's own segment size.)
    #[kani::proof]
    #[kani::unwind(11)]
    fn a_push_takes_the_bytes_bytes_with_said_and_reads_back() {
        let pushes: usize = kani::any();
        kani::assume(pushes <= PUSHES);
        let probe: usize = kani::any();
        kani::assume(probe < pushes);
        let mut states = States::new(1);
        let mut probed = (0u8, std::ptr::null());
        for i in 0..pushes {
            let s: u8 = kani::any();
            let said = states.bytes_with(1);
            states.push(StateSlice::new(&[s]));
            assert_eq!(states.bytes_with(0), said);
            assert_eq!(states.len(), i + 1);
            assert!(states.len() <= states.capacity());
            assert_eq!(states.rows.allocated(), said);
            if i == probe {
                probed = (s, states.packed_row(i).as_ptr());
            }
        }
        assert_eq!(states.at(probe).0, [probed.0]);
        assert_eq!(states.packed_row(probe).as_ptr(), probed.1);
        assert!(states.get(pushes).is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A worker reads the rows a `Published` names while the store goes on pushing, past the
    /// segments it was published with: each row reads back as pushed. Under Miri this checks
    /// the hand-off of #296 item 3.1 (`Published::row`): no read races a write.
    #[test]
    fn published_rows_read_back_while_the_store_grows() {
        let pushes: usize = if cfg!(miri) { 600 } else { 50_000 };
        let row = |i: usize| [(i % 251) as u8, (i / 251 % 256) as u8];
        let mut states = States::new(2);
        let mut segments = None;
        let (tx, rx) = std::sync::mpsc::channel::<Published>();
        std::thread::scope(|s| {
            s.spawn(move || {
                for published in rx {
                    let len = published.len();
                    for i in (0..len).rev().step_by(7).chain([0]) {
                        // SAFETY: the store outlives the scope, and it is only pushed to.
                        let got = unsafe { published.row(i) };
                        assert_eq!(got, Some(&row(i)[..]), "row {i} of {len}");
                    }
                    // SAFETY: as above.
                    assert!(unsafe { published.row(len) }.is_none());
                }
            });
            for i in 0..pushes {
                states.push(StateSlice::new(&row(i)));
                if i % 37 == 0 {
                    tx.send(states.publish(&mut segments)).expect("the reader");
                }
            }
            drop(tx);
        });
    }
}
