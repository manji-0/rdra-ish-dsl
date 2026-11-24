//! A run's states, end to end in one buffer: each state is kept once, with no heap block or
//! pointer of its own.

use std::hash::Hasher;
use std::ops::{Index, Range};

use hashbrown::HashTable;

use crate::fasthash::FastHasher;
use crate::lifecycle::expr::{State, StateSlice};

#[cfg(feature = "shadow")]
mod reference;

/// States of `width` bytes each, numbered in the order pushed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct States {
    bytes: Vec<u8>,
    width: usize,
    len: usize,
}

impl States {
    pub fn new(width: usize) -> Self {
        States {
            bytes: Vec::new(),
            width,
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Bytes in one state.
    pub fn width(&self) -> usize {
        self.width
    }

    pub fn get(&self, i: usize) -> Option<&StateSlice> {
        (i < self.len).then(|| &self[i])
    }

    pub fn first(&self) -> Option<&StateSlice> {
        self.get(0)
    }

    pub fn last(&self) -> Option<&StateSlice> {
        self.len.checked_sub(1).map(|i| &self[i])
    }

    pub fn iter(&self) -> Iter<'_> {
        Iter {
            states: self,
            at: 0..self.len,
        }
    }

    /// A copy of the states in `range`.
    pub(crate) fn slice(&self, range: Range<usize>) -> States {
        States {
            bytes: self.bytes[range.start * self.width..range.end * self.width].to_vec(),
            width: self.width,
            len: range.len(),
        }
    }

    /// Adds `s` at index `len()`. The buffer grows to twice its states when full, four at the
    /// least, and to no more: what it holds is `width * capacity()` bytes.
    pub(crate) fn push(&mut self, s: &StateSlice) {
        debug_assert_eq!(s.0.len(), self.width);
        if self.len == self.capacity() {
            let states = (self.len * 2).max(4);
            self.bytes
                .reserve_exact(states * self.width - self.bytes.len());
        }
        self.bytes.extend_from_slice(&s.0);
        self.len += 1;
    }

    /// The states the buffer holds room for.
    pub(crate) fn capacity(&self) -> usize {
        match self.width {
            0 => self.len.max(4).next_power_of_two(),
            w => self.bytes.capacity() / w,
        }
    }

    /// The bytes the buffer takes once `more` states are pushed.
    pub(crate) fn bytes_with(&self, more: usize) -> u64 {
        let cap = if self.len + more <= self.capacity() {
            self.capacity()
        } else {
            (self.capacity() * 2).max(self.len + more).max(4)
        };
        (cap * self.width) as u64
    }
}

impl Index<usize> for States {
    type Output = StateSlice;

    fn index(&self, i: usize) -> &StateSlice {
        assert!(i < self.len, "state {i} of {}", self.len);
        StateSlice::new(&self.bytes[i * self.width..(i + 1) * self.width])
    }
}

/// The states of a `States` in order.
#[derive(Clone)]
pub struct Iter<'a> {
    states: &'a States,
    at: Range<usize>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a StateSlice;

    fn next(&mut self) -> Option<&'a StateSlice> {
        self.at.next().map(|i| &self.states[i])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.at.size_hint()
    }
}

impl DoubleEndedIterator for Iter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.at.next_back().map(|i| &self.states[i])
    }
}

impl ExactSizeIterator for Iter<'_> {}

impl<'a> IntoIterator for &'a States {
    type Item = &'a StateSlice;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

impl FromIterator<State> for States {
    /// The states in order; they must all have the first one's width.
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

fn hash(s: &StateSlice) -> u64 {
    let mut hasher = FastHasher::default();
    hasher.write(&s.0);
    hasher.finish()
}

/// The index of each state in a `States`, found by its bytes: the table holds the index alone.
/// Under the `shadow` feature it keeps its reference twin beside the table.
#[derive(Default)]
pub(super) struct Seen {
    table: HashTable<u32>,
    #[cfg(feature = "shadow")]
    twin: reference::Seen,
}

impl Seen {
    /// The index of `s` in `states`, if it is there.
    pub fn find(&self, states: &States, s: &StateSlice) -> Option<u32> {
        let found = self
            .table
            .find(hash(s), |&i| states[i as usize] == *s)
            .copied();
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

    /// Notes that `states[at]` is not yet in the table and now is.
    pub fn insert(&mut self, states: &States, at: u32) {
        self.table
            .insert_unique(hash(&states[at as usize]), at, |&i| {
                hash(&states[i as usize])
            });
        #[cfg(feature = "shadow")]
        assert!(
            self.twin.insert(&states[at as usize], at),
            "shadow: Seen::insert({at}): the reference holds {:?} already",
            &states[at as usize].0
        );
    }

    /// The bytes the table takes once `more` indices are inserted: one control byte and one
    /// index per bucket, at 7/8 load, doubling when full.
    pub fn bytes_with(&self, more: usize) -> u64 {
        let buckets = |cap: usize| match cap {
            0 => 0,
            c if c < 8 => (c + 1).next_power_of_two(),
            c => (c * 8 / 7).next_power_of_two(),
        };
        let mut n = buckets(self.table.capacity());
        if self.table.len() + more > self.table.capacity() {
            n = (n * 2).max(4);
        }
        (n * (size_of::<u32>() + 1)) as u64
    }
}
