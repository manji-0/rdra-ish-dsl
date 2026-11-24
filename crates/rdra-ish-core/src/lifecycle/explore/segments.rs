//! Fixed-size segments for what a run keeps (#296 item 2.2): its states, parents and edges.
//! A buffer that doubles holds room for up to as many items again as it has, and while it
//! grows the old buffer and the new one are both alive. A segmented buffer holds whole
//! segments of `SEG` rows: growing allocates one segment and copies nothing, and the room it
//! holds unused is under one segment. The first segment still doubles from four rows up to a
//! whole one, so a run of a few states takes the bytes it took before, and growing copies at
//! most one segment.

use std::mem::size_of;

/// Rows in a segment: 2^14, or 4 under Kani, so that a harness crosses segments.
#[cfg(not(kani))]
pub(crate) const SHIFT: u32 = 14;
#[cfg(kani)]
pub(crate) const SHIFT: u32 = 2;
pub(crate) const SEG: usize = 1 << SHIFT;

/// Rows of `width` items of `T` each: the first `SEG` in `first`, which doubles up to a whole
/// segment, the rest in `rest`, a whole segment each.
#[derive(Clone, Debug)]
pub(crate) struct Rows<T> {
    width: usize,
    first: Vec<T>,
    /// Rows `first` holds room for: kept apart, as a width of 0 has no capacity to read.
    first_rows: usize,
    rest: Vec<Vec<T>>,
    len: usize,
}

impl<T: Copy> Rows<T> {
    pub(crate) fn new(width: usize) -> Self {
        Rows {
            width,
            first: Vec::new(),
            first_rows: 0,
            rest: Vec::new(),
            len: 0,
        }
    }

    /// No rows, with room for `rows` of them in the first segment when they fit one.
    pub(crate) fn with_room(width: usize, rows: usize) -> Self {
        let rows = rows.min(SEG);
        Rows {
            width,
            first: Vec::with_capacity(rows * width),
            first_rows: rows,
            rest: Vec::new(),
            len: 0,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn width(&self) -> usize {
        self.width
    }

    /// The rows the buffer holds room for.
    pub(crate) fn capacity(&self) -> usize {
        self.first_rows + self.rest.len() * SEG
    }

    pub(crate) fn push(&mut self, row: &[T]) {
        debug_assert_eq!(row.len(), self.width);
        if self.len < SEG {
            if self.len == self.first_rows {
                let rows = (self.len * 2).max(4).min(SEG);
                self.first.reserve_exact((rows - self.len) * self.width);
                self.first_rows = rows;
            }
            self.first.extend_from_slice(row);
        } else {
            if self.len == self.capacity() {
                if self.rest.len() == self.rest.capacity() {
                    let slots = (self.rest.len() * 2).max(4);
                    self.rest.reserve_exact(slots - self.rest.len());
                }
                self.rest.push(Vec::with_capacity(SEG * self.width));
            }
            let last = self.rest.last_mut().expect("a segment with room");
            last.extend_from_slice(row);
        }
        self.len += 1;
    }

    pub(crate) fn row(&self, i: usize) -> &[T] {
        assert!(i < self.len, "row {i} of {}", self.len);
        let (seg, at) = match i < SEG {
            true => (&self.first, i),
            false => (&self.rest[(i >> SHIFT) - 1], i & (SEG - 1)),
        };
        &seg[at * self.width..(at + 1) * self.width]
    }

    /// The bytes the buffer takes once `more` rows are pushed: its segments, and once it has
    /// more than one, the list of the others, which doubles from four slots.
    pub(crate) fn bytes_with(&self, more: usize) -> u64 {
        let (items, list) = self.parts_with(more);
        items + list
    }

    /// The bytes of the segments and of their list once `more` rows are pushed.
    pub(crate) fn parts_with(&self, more: usize) -> (u64, u64) {
        let n = self.len + more;
        let (first, rest, slots) = match n <= self.capacity() {
            true => (self.first_rows, self.rest.len(), self.rest.capacity()),
            false => {
                let first = match n <= SEG {
                    true => (self.first_rows * 2).max(n).max(4).min(SEG),
                    false => SEG,
                };
                let rest = n.saturating_sub(SEG).div_ceil(SEG);
                let slots = match rest <= self.rest.capacity() {
                    true => self.rest.capacity(),
                    false => (self.rest.capacity() * 2).max(rest).max(4),
                };
                (first, rest, slots)
            }
        };
        let row = (self.width * size_of::<T>()) as u64;
        (
            (first + rest * SEG) as u64 * row,
            (slots * size_of::<Vec<T>>()) as u64,
        )
    }

    /// The bytes the buffer holds now, read from its `Vec`s: what `bytes_with(0)` says.
    #[cfg(any(kani, test))]
    pub(crate) fn allocated(&self) -> u64 {
        let items = self.first.capacity() + self.rest.iter().map(Vec::capacity).sum::<usize>();
        (items * size_of::<T>() + self.rest.capacity() * size_of::<Vec<T>>()) as u64
    }

    /// The rows as `U`, a segment at a time, so that the buffer and its copy are never both
    /// whole: each segment is dropped once it is copied. The copy keeps the room the buffer
    /// had.
    pub(crate) fn map<U: Copy>(self, f: impl Fn(T) -> U) -> Rows<U> {
        let segment = |s: Vec<T>, cap: usize| {
            let mut out = Vec::with_capacity(cap);
            out.extend(s.into_iter().map(&f));
            out
        };
        let mut rest = Vec::with_capacity(self.rest.capacity());
        let first = segment(self.first, self.first_rows * self.width);
        for s in self.rest {
            rest.push(segment(s, SEG * self.width));
        }
        Rows {
            width: self.width,
            first,
            first_rows: self.first_rows,
            rest,
            len: self.len,
        }
    }
}

/// Kani harnesses (`cargo kani -p rdra-ish-core`).
#[cfg(kani)]
mod proofs {
    use super::*;

    /// For every width up to 2 and every run of up to 14 pushes, from an empty buffer or one
    /// made with room for a few rows, across three segments of 4 rows: each row reads back as
    /// pushed, `bytes_with(1)` before a push is `bytes_with(0)` after it, which is what the
    /// buffer's `Vec`s hold, and the room it holds unused is under a segment past the first.
    #[kani::proof]
    #[kani::unwind(16)]
    fn rows_take_what_bytes_with_said_and_read_back() {
        let width: usize = kani::any();
        kani::assume(width <= 2);
        let pushes: usize = kani::any();
        kani::assume(pushes <= 14);
        let room: usize = kani::any();
        kani::assume(room <= 6);
        let mut rows: Rows<u8> = match kani::any() {
            true => Rows::new(width),
            false => Rows::with_room(width, room),
        };
        let probe: usize = kani::any();
        let mut probed = [0u8; 2];
        for i in 0..pushes {
            let r: [u8; 2] = kani::any();
            let said = rows.bytes_with(1);
            rows.push(&r[..width]);
            assert_eq!(rows.bytes_with(0), said);
            assert_eq!(rows.allocated(), said);
            assert!(rows.len() <= rows.capacity());
            if rows.len() > SEG {
                assert!(rows.capacity() - rows.len() < SEG);
            }
            if i == probe {
                probed = r;
            }
        }
        if probe < pushes {
            assert_eq!(rows.row(probe), &probed[..width]);
        }
    }

    /// `map` keeps every row and the room: it widens a buffer in place of rebuilding it.
    #[kani::proof]
    #[kani::unwind(12)]
    fn map_keeps_the_rows_and_the_room() {
        let pushes: usize = kani::any();
        kani::assume(pushes <= 10);
        let mut rows: Rows<u8> = Rows::new(1);
        for i in 0..pushes {
            rows.push(&[i as u8]);
        }
        let probe: usize = kani::any();
        kani::assume(probe < pushes);
        let capacity = rows.capacity();
        let wide = rows.map(u16::from);
        assert_eq!(wide.row(probe), &[probe as u16]);
        assert_eq!(wide.capacity(), capacity);
        assert_eq!(wide.bytes_with(0), wide.allocated());
    }
}
