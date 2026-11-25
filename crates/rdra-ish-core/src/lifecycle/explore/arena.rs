//! A run's states, end to end in segments (`segments::Rows`): each state is kept once, with
//! no heap block or pointer of its own, and stays where it was pushed.

use std::hash::Hasher;
use std::ops::{Index, Range};

use hashbrown::HashTable;

use crate::fasthash::FastHasher;
use crate::lifecycle::expr::{State, StateSlice};

use super::segments::Rows;

#[cfg(feature = "shadow")]
mod reference;

/// States of `width` bytes each, numbered in the order pushed.
#[derive(Clone, Debug)]
pub struct States {
    rows: Rows<u8>,
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

impl States {
    pub fn new(width: usize) -> Self {
        States {
            rows: Rows::new(width),
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.len() == 0
    }

    /// Bytes in one state.
    pub fn width(&self) -> usize {
        self.rows.width()
    }

    pub fn get(&self, i: usize) -> Option<&StateSlice> {
        (i < self.len()).then(|| &self[i])
    }

    pub fn first(&self) -> Option<&StateSlice> {
        self.get(0)
    }

    pub fn last(&self) -> Option<&StateSlice> {
        self.len().checked_sub(1).map(|i| &self[i])
    }

    pub fn iter(&self) -> Iter<'_> {
        Iter {
            states: self,
            at: 0..self.len(),
        }
    }

    /// A copy of the states in `range`.
    pub(crate) fn slice(&self, range: Range<usize>) -> States {
        let mut rows = Rows::new(self.width());
        for i in range {
            rows.push(self.rows.row(i));
        }
        States { rows }
    }

    /// Adds `s` at index `len()`, in the last segment, or in a new one when that is full. No
    /// state already pushed moves.
    pub(crate) fn push(&mut self, s: &StateSlice) {
        self.rows.push(&s.0);
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

impl Index<usize> for States {
    type Output = StateSlice;

    fn index(&self, i: usize) -> &StateSlice {
        StateSlice::new(self.rows.row(i))
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
                probed = (s, states[i].0.as_ptr());
            }
        }
        assert_eq!(states[probe].0, [probed.0]);
        assert_eq!(states[probe].0.as_ptr(), probed.1);
        assert!(states.get(pushes).is_none());
    }
}
