//! How a BFS run grew, layer by layer, and what that says about the states it would reach.

/// The states a run first reached at each depth. BFS reads its states in discovery order, so
/// while it expands the states of one depth it finds those of the next: every layer up to
/// `reading` is whole, and the one after it is still filling.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layers {
    /// New states per depth, the initial states at 0.
    pub new: Vec<u64>,
    /// The depth whose states are being expanded.
    pub reading: usize,
    /// How many states of depth `reading` were expanded in full.
    pub expanded: u64,
    /// The index of the first state of depth `reading`.
    start: u64,
}

/// Whole layers past the peak before the layers say how many states remain: with one, the
/// sample cones' estimates came out as low as a quarter of their counts.
const FALLEN: usize = 2;

impl Layers {
    /// Counts a new state: an initial one at depth 0, else one past the depth being read.
    pub(super) fn admit(&mut self, initial: bool) {
        let depth = if initial { 0 } else { self.reading + 1 };
        if self.new.len() <= depth {
            self.new.resize(depth + 1, 0);
        }
        self.new[depth] += 1;
    }

    /// Notes that the run expands `states[at]` next.
    pub(super) fn read(&mut self, at: usize) {
        let at = at as u64;
        while at >= self.start + self.new[self.reading] {
            self.start += self.new[self.reading];
            self.reading += 1;
        }
        self.expanded = at - self.start;
    }

    /// What the store spends on this record: one count per depth.
    pub(super) fn bytes(&self) -> u64 {
        (self.new.capacity() * std::mem::size_of::<u64>()) as u64
    }

    /// The states reached.
    pub fn states(&self) -> u64 {
        self.new.iter().sum()
    }

    /// The states expanded in full: their moves are all found.
    pub fn expanded_states(&self) -> u64 {
        self.start + self.expanded
    }

    /// The whole layers: those up to the one being expanded.
    pub fn whole(&self) -> &[u64] {
        &self.new[..self.new.len().min(self.reading + 1)]
    }

    /// What the layers say about the states a stopped run would reach. The frontier of the
    /// sample cones rises to a peak and falls about as it rose, so once two whole layers lie
    /// past the largest one the falling side is taken to hold what the rising side did.
    /// While the last whole layer is the largest yet, nothing says where the peak is.
    pub fn estimate(&self) -> Estimate {
        let whole = self.whole();
        // The first of the largest: `max_by_key` keeps the last it meets.
        let Some((peak, &most)) = whole.iter().enumerate().rev().max_by_key(|&(_, n)| *n) else {
            return Estimate::Rising {
                depth: 0,
                last: 0,
                before: 0,
            };
        };
        let depth = whole.len() - 1;
        if peak == depth {
            return Estimate::Rising {
                depth,
                last: most,
                before: depth.checked_sub(1).map_or(0, |d| whole[d]),
            };
        }
        let rising: u64 = whole[..peak].iter().sum();
        let states = rising.saturating_mul(2).saturating_add(most);
        if depth - peak < FALLEN {
            return Estimate::Unsettled { peak, depth };
        }
        if states <= self.states() {
            return Estimate::Overrun { peak, depth };
        }
        Estimate::Falling {
            states,
            peak,
            depth,
        }
    }
}

/// What a stopped run's layers say about the states it would reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Estimate {
    /// The largest whole layer is `peak`, and the whole layers after it, up to `depth`, are
    /// smaller: about `states` in all, the falling side mirroring the rising one. On 73 sample
    /// cones stopped at a tenth to nine tenths of their states, the 61 such estimates came out
    /// at 0.51 to 1.37 of the count, and at 0.74 or more but once.
    Falling {
        states: u64,
        peak: usize,
        depth: usize,
    },
    /// The frontier peaked at `peak`, and `depth` is fewer than two whole layers past it.
    Unsettled { peak: usize, depth: usize },
    /// The frontier peaked at `peak`, and by `depth` the run holds as many states as the
    /// mirror gives: its fall is longer than its rise, and says nothing of where it ends.
    Overrun { peak: usize, depth: usize },
    /// The last whole layer, `depth`, is the largest yet, with `last` states against `before`
    /// at the depth before it.
    Rising {
        depth: usize,
        last: u64,
        before: u64,
    },
}

impl Estimate {
    /// The states to size the store for, to finish: the estimate and a quarter more. With that
    /// margin, the memory it names covered the store of all but one of the 61 sample estimates.
    pub fn states_to_cover(&self) -> Option<u64> {
        match *self {
            Estimate::Falling { states, .. } => Some(states.saturating_add(states / 4)),
            Estimate::Unsettled { .. } | Estimate::Overrun { .. } | Estimate::Rising { .. } => None,
        }
    }
}
