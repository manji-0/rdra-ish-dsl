//! A run's states, end to end in one buffer: each state is kept once, with no heap block or
//! pointer of its own.

use std::hash::Hasher;
use std::ops::{Index, Range};

use hashbrown::HashTable;

use crate::fasthash::FastHasher;
use crate::lifecycle::expr::{State, StateSlice};

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
#[derive(Default)]
pub(super) struct Seen(HashTable<u32>);

impl Seen {
    /// The index of `s` in `states`, if it is there.
    pub fn find(&self, states: &States, s: &StateSlice) -> Option<u32> {
        self.0.find(hash(s), |&i| states[i as usize] == *s).copied()
    }

    /// Notes that `states[at]` is not yet in the table and now is.
    pub fn insert(&mut self, states: &States, at: u32) {
        self.0.insert_unique(hash(&states[at as usize]), at, |&i| {
            hash(&states[i as usize])
        });
    }

    /// The bytes the table takes once `more` indices are inserted: one control byte and one
    /// index per bucket, at 7/8 load, doubling when full.
    pub fn bytes_with(&self, more: usize) -> u64 {
        let buckets = |cap: usize| match cap {
            0 => 0,
            c if c < 8 => (c + 1).next_power_of_two(),
            c => (c * 8 / 7).next_power_of_two(),
        };
        let mut n = buckets(self.0.capacity());
        if self.0.len() + more > self.0.capacity() {
            n = (n * 2).max(4);
        }
        (n * (size_of::<u32>() + 1)) as u64
    }
}

/// Kani harnesses (`cargo kani -p rdra-ish-core`).
#[cfg(kani)]
mod proofs {
    use super::*;

    const WIDTH: usize = 2;
    const PUSHES: usize = 5;

    /// For every width up to 2 and every run of up to 5 pushes, from an empty store or from a
    /// `slice` of one: each state reads back as pushed, no index leaves the buffer (the
    /// `unsafe` cast included), the buffer never holds room for more than `capacity()` states,
    /// and `bytes_with(1)` before a push is `bytes_with(0)` after it, which is what the memory
    /// limit charges.
    #[kani::proof]
    #[kani::unwind(7)]
    fn a_push_takes_the_bytes_bytes_with_said_and_reads_back() {
        let width: usize = kani::any();
        kani::assume(width <= WIDTH);
        let pushes: usize = kani::any();
        kani::assume(pushes <= PUSHES);
        let probe: usize = kani::any();
        kani::assume(probe < pushes);
        let mut states = States::new(width);
        let mut probed = [0u8; WIDTH];
        // A slice in the middle: its buffer is as long as its states, not a power of two.
        let cut: usize = kani::any();
        kani::assume(cut <= pushes);
        let sliced: bool = kani::any();
        for i in 0..pushes {
            if sliced && i == cut {
                states = states.slice(0..i);
            }
            let s: [u8; WIDTH] = kani::any();
            let said = states.bytes_with(1);
            states.push(StateSlice::new(&s[..width]));
            assert_eq!(states.bytes_with(0), said);
            assert_eq!(states.len(), i + 1);
            assert!(states.len() <= states.capacity());
            assert_eq!(states.bytes.capacity(), states.capacity() * width);
            if i == probe {
                probed = s;
            }
        }
        assert_eq!(states[probe].0, probed[..width]);
        assert!(states.get(pushes).is_none());
    }
}
