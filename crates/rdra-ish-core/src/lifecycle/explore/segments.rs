//! Segments for what a run keeps (#296 item 2.2): its states, parents and edges. A `Vec`
//! that doubles copies its rows into a buffer twice the size, and while it does both are
//! alive. `Rows` never copies a row: it holds rows in segments of 4, 4, 8, 16, ... rows up to
//! `SEG`, then in segments of `SEG` rows each, and allocates a segment whole when the last
//! one is full. So a row stays where it was pushed for as long as the buffer lives (the
//! invariant phase 3, #320, builds on), growing allocates one segment and frees none, and up
//! to `2 * SEG` rows the buffer holds the room a doubling `Vec` holds.

use std::mem::size_of;

/// Rows in a segment past the doubling ones: 2^14, or 4 under Kani, so that a harness
/// reaches them.
#[cfg(not(kani))]
pub(crate) const SHIFT: u32 = 14;
#[cfg(kani)]
pub(crate) const SHIFT: u32 = 2;
pub(crate) const SEG: usize = 1 << SHIFT;
/// The doubling segments (4, 4, 8, ... `SEG` rows) hold the first `DOUBLED` rows.
const DOUBLED: usize = 2 * SEG;
/// How many doubling segments there are: `log2(DOUBLED) - 1`.
const HEAD: usize = SHIFT as usize;

/// The segment that holds row `i`, and the row's place in it.
pub(crate) fn locate(i: usize) -> (usize, usize) {
    if i < 4 {
        (0, i)
    } else if i < DOUBLED {
        let log = i.ilog2() as usize;
        (log - 1, i - (1 << log))
    } else {
        let past = i - DOUBLED;
        (HEAD + (past >> SHIFT), past & (SEG - 1))
    }
}

/// The rows segment `k` holds.
pub(crate) fn segment_rows(k: usize) -> usize {
    match k {
        0 => 4,
        k if k < HEAD => 1 << (k + 1),
        _ => SEG,
    }
}

/// The rows the first `k` segments hold.
fn rows_in(k: usize) -> usize {
    match k {
        0 => 0,
        k if k <= HEAD => 1 << (k + 1),
        k => DOUBLED + (k - HEAD) * SEG,
    }
}

/// Rows of `width` items of `T`. The doubling segments live in the buffer itself, the rest in
/// a list that doubles from four slots; only that list's slots, 24 bytes each, are heap
/// bytes a doubling `Vec` would not take.
#[derive(Clone, Debug)]
pub(crate) struct Rows<T> {
    width: usize,
    head: [Vec<T>; HEAD],
    rest: Vec<Vec<T>>,
    len: usize,
    /// The rows the segments so far hold room for: a push at `len == room` allocates one.
    room: usize,
}

impl<T: Copy> Rows<T> {
    pub(crate) fn new(width: usize) -> Self {
        Rows {
            width,
            head: std::array::from_fn(|_| Vec::new()),
            rest: Vec::new(),
            len: 0,
            room: 0,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// The segments that hold a row.
    pub(crate) fn segments_used(&self) -> usize {
        match self.len {
            0 => 0,
            n => locate(n - 1).0 + 1,
        }
    }

    /// The address of each segment that holds a row, in order: where the segment's rows stay
    /// for as long as the buffer lives.
    pub(crate) fn segment_ptrs(&self) -> Vec<*const T> {
        (0..self.segments_used())
            .map(|k| self.segment(k).as_ptr())
            .collect()
    }

    /// The segments allocated so far.
    #[cfg(kani)]
    fn segments(&self) -> usize {
        match self.len {
            0 => 0,
            n => locate(n - 1).0 + 1,
        }
    }

    /// The rows the buffer holds room for.
    #[cfg(kani)]
    pub(crate) fn capacity(&self) -> usize {
        rows_in(self.segments())
    }

    fn segment(&self, k: usize) -> &Vec<T> {
        match k < HEAD {
            true => &self.head[k],
            false => &self.rest[k - HEAD],
        }
    }

    /// The segment rows go to next, allocated when the last one is full.
    #[inline]
    fn last(&mut self) -> &mut Vec<T> {
        if self.len == self.room {
            self.grow();
        }
        match self.rest.last_mut() {
            Some(seg) => seg,
            None => &mut self.head[locate(self.len).0],
        }
    }

    /// Allocates the next segment whole.
    #[cold]
    fn grow(&mut self) {
        let k = locate(self.len).0;
        let fresh = Vec::with_capacity(segment_rows(k) * self.width);
        match k < HEAD {
            true => self.head[k] = fresh,
            false => {
                if self.rest.len() == self.rest.capacity() {
                    let slots = (self.rest.len() * 2).max(4);
                    self.rest.reserve_exact(slots - self.rest.len());
                }
                self.rest.push(fresh);
            }
        }
        self.room = rows_in(k + 1);
    }

    #[inline]
    pub(crate) fn push(&mut self, row: &[T]) {
        debug_assert_eq!(row.len(), self.width);
        let seg = self.last();
        // Within its capacity: the segment's rows never move. Workers read the rows of a
        // run's states through the segments' addresses (`arena::Published`), so this holds in
        // a release build too.
        assert!(seg.len() + row.len() <= seg.capacity(), "a segment grew");
        seg.extend_from_slice(row);
        self.len += 1;
    }

    /// `push` of a row of one item.
    #[inline]
    pub(crate) fn push_item(&mut self, x: T) {
        debug_assert_eq!(self.width, 1);
        let seg = self.last();
        debug_assert!(seg.len() < seg.capacity());
        seg.push(x);
        self.len += 1;
    }

    /// The segment that holds row `i` and the row's place in it, a fixed segment first: in a
    /// large buffer most rows are there.
    #[inline]
    fn place(&self, i: usize) -> (&Vec<T>, usize) {
        assert!(i < self.len, "row {i} of {}", self.len);
        match i.checked_sub(DOUBLED) {
            Some(past) => (&self.rest[past >> SHIFT], past & (SEG - 1)),
            None => {
                let (k, at) = locate(i);
                (&self.head[k], at)
            }
        }
    }

    #[inline]
    pub(crate) fn row(&self, i: usize) -> &[T] {
        let (seg, at) = self.place(i);
        &seg[at * self.width..(at + 1) * self.width]
    }

    /// Row `i` of a buffer of rows of one item.
    #[inline]
    pub(crate) fn item(&self, i: usize) -> T {
        debug_assert_eq!(self.width, 1);
        let (seg, at) = self.place(i);
        seg[at]
    }

    /// Rows `at` as one slice of their items: `at` must lie in one segment, which
    /// `segment_end` bounds.
    pub(crate) fn run(&self, at: std::ops::Range<usize>) -> &[T] {
        if at.is_empty() {
            return &[];
        }
        let (k, start) = locate(at.start);
        let end = start + at.len();
        debug_assert!(end <= segment_rows(k), "rows {at:?} span segments");
        &self.segment(k)[start * self.width..end * self.width]
    }

    /// The end of the segment that holds row `i`: the rows `i..segment_end(i)` are one slice.
    pub(crate) fn segment_end(i: usize) -> usize {
        let (k, at) = locate(i);
        i - at + segment_rows(k)
    }

    /// The bytes the buffer takes once `more` rows are pushed: its segments' items and the
    /// list of the segments past the doubling ones.
    pub(crate) fn bytes_with(&self, more: usize) -> u64 {
        let (items, list) = self.parts_with(more);
        items + list
    }

    /// `bytes_with` as its two parts: the segments' items, never more than a doubling `Vec`
    /// of the same rows holds, and the list's slots.
    pub(crate) fn parts_with(&self, more: usize) -> (u64, u64) {
        let n = self.len + more;
        let segments = match n {
            0 => 0,
            n => locate(n - 1).0 + 1,
        };
        let listed = segments.saturating_sub(HEAD);
        let slots = match listed <= self.rest.capacity() {
            true => self.rest.capacity(),
            false => (self.rest.capacity() * 2).max(listed).max(4),
        };
        let row = (self.width * size_of::<T>()) as u64;
        (
            rows_in(segments) as u64 * row,
            (slots * size_of::<Vec<T>>()) as u64,
        )
    }

    /// The heap bytes the buffer holds now, read from its `Vec`s: what `bytes_with(0)` says.
    #[cfg(any(kani, test))]
    pub(crate) fn allocated(&self) -> u64 {
        let items: usize = self.head.iter().chain(&self.rest).map(Vec::capacity).sum();
        (items * size_of::<T>() + self.rest.capacity() * size_of::<Vec<T>>()) as u64
    }

    /// The rows as `U`, a segment at a time, each segment dropped once it is copied, so the
    /// buffer and its copy are never both whole. The copy has the same segments.
    pub(crate) fn map<U: Copy>(self, f: impl Fn(T) -> U) -> Rows<U> {
        let segment = |s: Vec<T>| {
            let mut out = Vec::with_capacity(s.capacity());
            out.extend(s.into_iter().map(&f));
            out
        };
        let mut head: [Vec<U>; HEAD] = std::array::from_fn(|_| Vec::new());
        for (k, s) in self.head.into_iter().enumerate() {
            head[k] = segment(s);
        }
        let mut rest = Vec::with_capacity(self.rest.capacity());
        for s in self.rest {
            rest.push(segment(s));
        }
        Rows {
            width: self.width,
            head,
            rest,
            len: self.len,
            room: self.room,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row stays where it was pushed: past every doubling segment and into the fixed ones,
    /// its address and bytes are the ones it had when it was pushed.
    #[test]
    fn a_pushed_row_never_moves() {
        let n = DOUBLED + 3 * SEG + 5;
        let mut rows: Rows<u32> = Rows::new(2);
        let mut at = Vec::new();
        for i in 0..n as u32 {
            rows.push(&[i, !i]);
            at.push(rows.row(i as usize).as_ptr());
        }
        for (i, &p) in at.iter().enumerate() {
            assert_eq!(rows.row(i).as_ptr(), p, "row {i} moved");
            assert_eq!(rows.row(i), &[i as u32, !(i as u32)]);
        }
        assert_eq!(rows.bytes_with(0), rows.allocated());
    }

    /// `map` keeps every row and the room, past the doubling segments.
    #[test]
    fn map_keeps_the_rows_and_the_room() {
        let mut rows: Rows<u8> = Rows::new(1);
        for i in 0..(DOUBLED + SEG + 3) {
            rows.push(&[i as u8]);
        }
        let bytes = rows.bytes_with(0);
        let wide = rows.map(u16::from);
        for i in 0..wide.len() {
            assert_eq!(wide.row(i), &[i as u8 as u16]);
        }
        assert_eq!(wide.allocated(), wide.bytes_with(0));
        assert_eq!(wide.parts_with(0).0, 2 * (bytes - wide.parts_with(0).1));
    }

    /// Up to `DOUBLED` rows the segments hold what a doubling `Vec` holds; past it, the rows
    /// hold less, and the list of segments 24 bytes a slot.
    #[test]
    fn the_room_is_a_doubling_vecs_or_less() {
        let mut rows: Rows<u8> = Rows::new(1);
        for n in 1..=(DOUBLED + 4 * SEG) {
            rows.push(&[0]);
            let doubling = n.max(4).next_power_of_two() as u64;
            let (items, list) = rows.parts_with(0);
            assert!(items <= doubling, "{n}: {items} > {doubling}");
            if n <= DOUBLED {
                assert_eq!((items, list), (doubling, 0), "{n}");
            }
        }
    }
}

/// Kani harnesses (`cargo kani -p rdra-ish-core`).
#[cfg(kani)]
mod proofs {
    use super::*;

    /// `locate` numbers rows in order: every row up to 40 falls in one segment, at a place
    /// under that segment's rows, right after the row before it.
    #[kani::proof]
    fn locate_numbers_rows_in_order() {
        let i: usize = kani::any();
        kani::assume(i > 0 && i < 40);
        let (k, at) = locate(i);
        assert!(at < segment_rows(k));
        assert_eq!(rows_in(k) + at, i);
        let (k0, at0) = locate(i - 1);
        match at {
            0 => assert_eq!((k0 + 1, at0 + 1), (k, segment_rows(k0))),
            _ => assert_eq!((k0, at0 + 1), (k, at)),
        }
    }

    /// For rows of one byte and every run of up to 9 pushes, over the two doubling segments
    /// and into a fixed one: each row reads back as pushed and stays at the address it was
    /// pushed to, and `bytes_with(1)` before a push is `bytes_with(0)` after it and what the
    /// `Vec`s hold. (At widths up to 2, or 14 pushes, CBMC passed 8 GB.)
    #[kani::proof]
    #[kani::unwind(11)]
    fn rows_take_what_bytes_with_said_and_never_move() {
        let pushes: usize = kani::any();
        kani::assume(pushes <= 9);
        let probe: usize = kani::any();
        kani::assume(probe < pushes);
        let mut rows: Rows<u8> = Rows::new(1);
        let mut probed = (0u8, std::ptr::null());
        for i in 0..pushes {
            let r: u8 = kani::any();
            let said = rows.bytes_with(1);
            rows.push(&[r]);
            assert_eq!(rows.bytes_with(0), said);
            assert_eq!(rows.allocated(), said);
            if i == probe {
                probed = (r, rows.row(i).as_ptr());
            }
        }
        assert_eq!(rows.row(probe), &[probed.0]);
        assert_eq!(rows.row(probe).as_ptr(), probed.1);
    }

    /// The segments that hold `n` rows hold room for all of them, and never for more than a
    /// doubling `Vec` of `n` rows (4 at the least) holds; up to `DOUBLED` rows, for exactly
    /// as many. For every `n` up to 2^40. (A harness that pushes into the `Vec`s themselves
    /// passed 8 GB in CBMC; `a_pushed_row_never_moves` and `the_room_is_a_doubling_vecs_or_less`
    /// push them at the build's own segment size.)
    #[kani::proof]
    fn segments_hold_every_row_and_no_more_than_a_doubling_vec() {
        let n: usize = kani::any();
        kani::assume(n > 0 && n <= 1 << 40);
        let room = rows_in(locate(n - 1).0 + 1);
        let doubling = n.max(4).next_power_of_two();
        assert!(n <= room);
        assert!(room <= doubling);
        if n <= DOUBLED {
            assert_eq!(room, doubling);
        }
    }
}
