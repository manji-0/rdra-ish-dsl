//! The index of each state of a run by its packed bytes, in a table that one thread writes (the
//! reader, which admits the states) and any thread reads (the workers, which probe each
//! successor before the reader reaches it; #296 item 3.1, `docs/design/native-bfs-parallel.md`,
//! "The shared table"). hashbrown's table cannot be read while it is written, so this is its
//! layout without its writes in place:
//!
//! - a control byte per bucket, eight to an `AtomicU64` group: `EMPTY`, or the top 7 bits of
//!   the hash of the state the bucket holds, and an `AtomicU32` index per bucket;
//! - probing over groups, triangular, from the group the hash's low bits name;
//! - one writer, which fills a bucket once and never empties one, and nothing ever removed;
//! - growth into a new table, built aside and then made current (`Seen::insert`): a thread
//!   that holds an older one probes that, which no longer changes.
//!
//! A probe returns an index whose row the caller's `eq` accepted, so whatever a probe reads
//! from a bucket, it can only return a state that is in the run with the bytes asked for: the
//! caller compares the row. What the ordering of the atomics adds is only that a probe sees the
//! buckets written before the caller learned of them, which a worker never relies on
//! (`Published`).

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// The control byte of a bucket that holds nothing; a full one's top bit is clear.
const EMPTY: u8 = 0x80;
/// Buckets in a group, one control byte each in a word.
const LANES: usize = 8;
const HIGH: u64 = 0x8080_8080_8080_8080;
const LOW7: u64 = 0x7f7f_7f7f_7f7f_7f7f;
const ONES: u64 = 0x0101_0101_0101_0101;

/// The 7 bits of `hash` a full bucket's control byte holds: its top ones, as hashbrown's.
#[inline]
fn tag(hash: u64) -> u8 {
    (hash >> 57) as u8
}

/// The lanes of `word` whose byte is `byte`, as the top bit of each such byte. Exact, with
/// no carry between bytes: a byte of `x` is zero exactly when neither its low 7 bits plus
/// `0x7f` nor the byte itself reach the top bit.
#[inline]
fn lanes(word: u64, byte: u8) -> u64 {
    let x = word ^ (ONES * u64::from(byte));
    !(((x & LOW7) + LOW7) | x | LOW7)
}

/// The lanes of `word` that hold nothing.
#[inline]
fn empty(word: u64) -> u64 {
    word & HIGH
}

/// One generation of the table: a power of two of groups.
pub(super) struct Table {
    ctrl: Box<[AtomicU64]>,
    idx: Box<[AtomicU32]>,
    /// Groups less one.
    mask: usize,
}

impl Table {
    /// A table of at least `buckets` buckets, one group at the least, all empty.
    fn new(buckets: usize) -> Table {
        let groups = buckets.div_ceil(LANES).max(1).next_power_of_two();
        Table {
            ctrl: (0..groups)
                .map(|_| AtomicU64::new(u64::from_ne_bytes([EMPTY; LANES])))
                .collect(),
            idx: (0..groups * LANES).map(|_| AtomicU32::new(0)).collect(),
            mask: groups - 1,
        }
    }

    /// The groups a probe for `hash` visits, in order: each group once, since the groups are
    /// a power of two and the stride grows by one each step.
    #[inline]
    fn groups(&self, hash: u64) -> impl Iterator<Item = usize> + '_ {
        let mut g = (hash as usize) & self.mask;
        let mut stride = 0;
        std::iter::from_fn(move || {
            let at = g;
            stride += 1;
            g = (g + stride) & self.mask;
            Some(at)
        })
        .take(self.mask + 1)
    }

    /// The first index filed under `hash` that `eq` accepts; `None` once the probe meets a
    /// group with an empty bucket, or has visited every group. Any thread may call it.
    #[inline]
    pub(super) fn find(&self, hash: u64, eq: impl FnMut(u32) -> bool) -> Option<u32> {
        self.find_ordered(hash, eq, Ordering::Acquire)
    }

    /// `find` with the control words loaded as `order` says: `Relaxed` on the writer's own
    /// thread, which wrote every bucket itself.
    #[inline]
    fn find_ordered(
        &self,
        hash: u64,
        mut eq: impl FnMut(u32) -> bool,
        order: Ordering,
    ) -> Option<u32> {
        let t = tag(hash);
        for g in self.groups(hash) {
            let word = self.ctrl[g].load(order);
            let mut hit = lanes(word, t);
            while hit != 0 {
                let lane = (hit.trailing_zeros() / 8) as usize;
                let i = self.idx[g * LANES + lane].load(Ordering::Relaxed);
                if eq(i) {
                    return Some(i);
                }
                hit &= hit - 1;
            }
            if empty(word) != 0 {
                return None;
            }
        }
        None
    }

    /// Files `at` under `hash` in the first empty bucket its probe meets: the index first, then
    /// the control byte, so a thread that sees the byte (`Acquire`) sees the index. Only the
    /// table's one writer calls it, and the table has an empty bucket (`Seen` keeps it at most
    /// 7/8 full).
    fn insert(&self, hash: u64, at: u32) {
        for g in self.groups(hash) {
            let word = self.ctrl[g].load(Ordering::Relaxed);
            let free = empty(word);
            if free != 0 {
                let lane = (free.trailing_zeros() / 8) as usize;
                self.idx[g * LANES + lane].store(at, Ordering::Relaxed);
                let shift = lane * 8;
                let word = (word & !(0xff << shift)) | (u64::from(tag(hash)) << shift);
                self.ctrl[g].store(word, Ordering::Release);
                return;
            }
        }
        unreachable!("a table at most 7/8 full has an empty bucket");
    }
}

/// The buckets hashbrown holds room for `cap` items in: what `Seen::bytes_with` charges.
fn buckets_for(cap: usize) -> usize {
    match cap {
        c if c < 4 => 4,
        c if c < 8 => 8,
        c => (c * 8 / 7).next_power_of_two(),
    }
}

/// The items `buckets` buckets hold, hashbrown's rule: all but one up to 8, else 7/8.
fn capacity_of(buckets: usize) -> usize {
    match buckets {
        b if b <= 8 => b - 1,
        b => b / 8 * 7,
    }
}

/// The capacity after an insert into a table of `cap` items that holds `len`, as hashbrown's
/// `insert_unique` grows it: unchanged while there is room, else to the buckets of
/// `max(len + 1, cap + 1)` items.
fn grown(len: usize, cap: usize) -> usize {
    match len < cap {
        true => cap,
        false => capacity_of(buckets_for((len + 1).max(cap + 1))),
    }
}

/// The table of a run, by its one writer: the current generation, how many indices it holds,
/// and the capacity hashbrown would have, which `bytes_with` charges.
pub(super) struct Writer {
    table: Arc<Table>,
    len: usize,
    cap: usize,
}

impl Default for Writer {
    fn default() -> Self {
        Writer {
            table: Arc::new(Table::new(0)),
            len: 0,
            cap: 0,
        }
    }
}

impl Writer {
    /// The current generation, for a thread that probes while the writer goes on.
    pub(super) fn current(&self) -> Arc<Table> {
        self.table.clone()
    }

    /// `Table::find` on the writer's own thread.
    #[inline]
    pub(super) fn find(&self, hash: u64, eq: impl FnMut(u32) -> bool) -> Option<u32> {
        self.table.find_ordered(hash, eq, Ordering::Relaxed)
    }

    pub(super) fn len(&self) -> usize {
        self.len
    }

    pub(super) fn capacity(&self) -> usize {
        self.cap
    }

    /// Files index `at`, the next one, under `hash`. When the table is full it first builds the
    /// next generation, filing every index again under `hash_of(i)`, and makes it current.
    pub(super) fn insert(&mut self, hash: u64, at: u32, hash_of: impl Fn(u32) -> u64) {
        debug_assert_eq!(at as usize, self.len);
        let cap = grown(self.len, self.cap);
        if cap != self.cap {
            let next = Table::new(buckets_for(cap));
            for i in 0..self.len as u32 {
                next.insert(hash_of(i), i);
            }
            self.table = Arc::new(next);
            self.cap = cap;
        }
        self.table.insert(hash, at);
        self.len += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fasthash::FastHasher;
    use std::hash::Hasher;

    /// How many indexes a test files: few under Miri, which runs each step thousands of times
    /// slower, enough there to grow the table through several generations.
    const N: u32 = if cfg!(miri) { 400 } else { 100_000 };

    fn h(i: u32) -> u64 {
        let mut s = FastHasher::default();
        s.write_u32(i);
        s.finish()
    }

    /// The capacity grows where hashbrown's does, insert by insert, so `Seen::bytes_with`
    /// charges what it charged when `Seen` was a hashbrown table.
    #[test]
    #[cfg_attr(miri, ignore = "no unsafe code; slow under Miri")]
    fn the_table_grows_where_hashbrown_grows() {
        let mut ours = Writer::default();
        let mut theirs: hashbrown::HashTable<u32> = hashbrown::HashTable::new();
        assert_eq!(ours.capacity(), theirs.capacity());
        for i in 0..200_000u32 {
            ours.insert(h(i), i, h);
            theirs.insert_unique(h(i), i, |&k| h(k));
            assert_eq!(
                ours.capacity(),
                theirs.capacity(),
                "after {} inserts",
                i + 1
            );
        }
    }

    /// Every index filed is found under its hash, through every growth, and a hash never filed
    /// finds nothing; a generation made current earlier still finds what it held.
    #[test]
    fn every_index_filed_is_found() {
        let mut w = Writer::default();
        let mut old = Vec::new();
        for i in 0..N / 2 {
            w.insert(h(i), i, h);
            if i.is_power_of_two() {
                old.push((i, w.current()));
            }
        }
        for i in 0..N / 2 {
            assert_eq!(w.find(h(i), |k| k == i), Some(i));
        }
        for i in N / 2..N / 2 + N / 10 {
            assert_eq!(w.find(h(i), |k| k == i), None);
        }
        for (n, t) in old {
            for i in 0..=n {
                assert_eq!(t.find(h(i), |k| k == i), Some(i), "generation of {n}");
            }
        }
    }

    /// `lanes` is exact on every byte value in every lane, beside others of any value.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "no unsafe code; the Kani harness `lanes_are_exactly_the_equal_bytes` proves it"
    )]
    fn lanes_match_exactly_the_bytes_equal_to_the_tag() {
        for t in 0..=255u8 {
            for b in 0..=255u8 {
                for lane in 0..LANES {
                    let other = t.wrapping_add(1 + (b % 3));
                    let mut bytes = [other; LANES];
                    bytes[lane] = b;
                    let word = u64::from_le_bytes(bytes);
                    let want: u64 = bytes
                        .iter()
                        .enumerate()
                        .filter(|(_, &x)| x == t)
                        .map(|(k, _)| 0x80u64 << (8 * k))
                        .sum();
                    assert_eq!(lanes(word, t), want, "tag {t}, byte {b} in lane {lane}");
                }
            }
        }
    }

    /// Workers probe while the writer files and grows: every hit a reader returns names the
    /// index it asked for, and a reader that holds the current table once the writer is done
    /// finds every index.
    #[test]
    fn probes_beside_the_writer_return_only_what_they_ask() {
        use std::sync::atomic::AtomicUsize;
        use std::sync::Mutex;
        let n = N;
        let current = Mutex::new(Writer::default().current());
        let filed = AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..3 {
                s.spawn(|| {
                    let mut hits = 0;
                    while filed.load(Ordering::Acquire) < n as usize {
                        let t = current.lock().expect("current").clone();
                        let upto = filed.load(Ordering::Acquire) as u32;
                        for i in (0..upto.max(1)).step_by(97) {
                            if let Some(k) = t.find(h(i), |k| k == i) {
                                assert_eq!(k, i);
                                hits += 1;
                            }
                        }
                    }
                    hits
                });
            }
            let mut w = Writer::default();
            for i in 0..n {
                w.insert(h(i), i, h);
                filed.store(i as usize + 1, Ordering::Release);
                if i % (N / 100) == 0 {
                    *current.lock().expect("current") = w.current();
                }
            }
            *current.lock().expect("current") = w.current();
            let t = w.current();
            for i in 0..n {
                assert_eq!(t.find(h(i), |k| k == i), Some(i));
            }
        });
    }
}

/// Kani harnesses (`cargo kani -p rdra-ish-core`).
#[cfg(kani)]
mod proofs {
    use super::*;

    /// `lanes` returns exactly the top bit of each byte equal to the tag, for any word and tag.
    #[kani::proof]
    fn lanes_are_exactly_the_equal_bytes() {
        let (word, t): (u64, u8) = (kani::any(), kani::any());
        let got = lanes(word, t);
        for k in 0..LANES {
            let byte = (word >> (8 * k)) as u8;
            assert_eq!((got >> (8 * k)) as u8, if byte == t { 0x80 } else { 0 });
        }
    }

    /// The growth rule is hashbrown's as `Seen::bytes_with` reads it: from any capacity the
    /// rule reaches, an insert keeps it while there is room, and else moves to a capacity of
    /// at least one more item whose buckets are a power of two that `bytes_with` counts.
    #[kani::proof]
    fn growth_keeps_room_for_one_more() {
        let (len, k): (usize, u32) = (kani::any(), kani::any());
        kani::assume(k >= 2 && k <= 40);
        let cap = capacity_of(1 << k);
        kani::assume(len <= cap);
        let next = grown(len, cap);
        assert!(next > len);
        assert!(next == cap || buckets_for(next) == (1usize << k) * 2);
        assert!(next < buckets_for(next));
    }
}
