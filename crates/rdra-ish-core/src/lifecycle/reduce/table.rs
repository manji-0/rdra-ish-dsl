//! The class of every state of the extended sub-lifecycle, for a check outside the reducer
//! that each state of the unreduced cone maps to a reached state and each step to a class
//! move or a stutter (the TLC refinement check of docs/theory/tlc-agreement.md).
use std::collections::HashMap;

use super::extended::Extended;
use super::partition::partition;
use super::*;
use crate::lifecycle::explore::ComponentRun;

/// The most states of the extended sub-lifecycle `class_table` lists: the TLA+ module that
/// prints them holds one tuple per state.
pub const CLASS_TABLE_CAP: usize = 200_000;

/// Each state of an extended sub-lifecycle, as a key (`Reduction::key`), with its class.
pub type ClassTable = Vec<(Box<[u8]>, u32)>;

/// Why `class_table` lists no classes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoClassTable {
    /// The cone is not reduced.
    NotReduced,
    /// The extended sub-lifecycle, or a nested one, was explored up to `H` (#212 phase 2):
    /// its run stores one form per orbit, so its states are not the cone's keys.
    UpToH,
    /// The extended sub-lifecycle has more states than `CLASS_TABLE_CAP`, nested ones
    /// expanded.
    TooLarge(usize),
    /// The extended sub-lifecycle did not rebuild as it was built
    /// (`Extended::rebuild_checked`).
    Rebuilt,
}

impl std::fmt::Display for NoClassTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoClassTable::NotReduced => f.write_str("the cone is not reduced"),
            NoClassTable::UpToH => f.write_str(
                "the sub-lifecycle was explored up to symmetry, so its states are stored forms",
            ),
            NoClassTable::TooLarge(n) => write!(
                f,
                "the sub-lifecycle has more than {CLASS_TABLE_CAP} states ({n} at least)"
            ),
            NoClassTable::Rebuilt => {
                f.write_str("the sub-lifecycle did not rebuild as the reduction built it")
            }
        }
    }
}

impl Reduction {
    /// Each class with the block of `run`'s partition its representative is in.
    pub(super) fn rep_blocks<'a>(
        &'a self,
        run: &'a ComponentRun,
        blocks: &'a [u32],
    ) -> impl Iterator<Item = (u32, u32)> + 'a {
        run.states
            .iter()
            .enumerate()
            .filter_map(move |(i, s)| self.class_of.get(&s.0).map(|c| (*c, blocks[i])))
    }

    /// The bytes after the vars of a key (`key`): the stubs, which no action writes.
    pub fn tail_len(&self) -> usize {
        self.tail.len()
    }
}

impl TransitionSystem {
    /// Every state of the extended sub-lifecycle of this reduced cone that its run reaches,
    /// as a key (`Reduction::key`: S and the inputs, the stubs, the bits), with its class.
    /// A key of a state of the cone that is not listed is a state the extended
    /// sub-lifecycle does not reach, which Lemma 6 of docs/theory/cones.md rules out.
    ///
    /// The run and its partition are the ones `lift` recomputes. A nested reduction is
    /// expanded: the run of the extended sub-lifecycle holds each state with its inner S
    /// part replaced by the inner class's representative, so each such state stands for
    /// every state whose inner S part is in that inner class, which the inner reduction's
    /// own table lists. A run up to `H` is refused: its stored forms are not states of the
    /// cone.
    pub fn class_table(&self) -> Result<ClassTable, NoClassTable> {
        let r = self.reduction().ok_or(NoClassTable::NotReduced)?;
        if r.up_to_h {
            return Err(NoClassTable::UpToH);
        }
        let ext = Extended::rebuild_checked(self, r).ok_or(NoClassTable::Rebuilt)?;
        let mut sub = ext.system.clone();
        sub.reduction = r.inner.clone();
        let run = sub.clone().run(Caps::new(usize::MAX, CLASS_TABLE_CAP));
        if !run.complete {
            return Err(NoClassTable::TooLarge(run.states.len()));
        }
        // An unobserved S is one class (`Classes::unobserved`), which no partition made.
        let class_of_state: Vec<u32> = match r.obs.is_empty() {
            true => vec![0; run.states.len()],
            false => {
                let blocks = partition(&ext, &run.system, &run).blocks;
                let class_of_block: HashMap<u32, u32> =
                    r.rep_blocks(&run, &blocks).map(|(c, b)| (b, c)).collect();
                blocks
                    .iter()
                    .map(|b| {
                        *class_of_block
                            .get(b)
                            .expect("every block of the run holds a representative")
                    })
                    .collect()
            }
        };
        let Some(inner) = r.inner.as_deref() else {
            return Ok(run
                .states
                .iter()
                .zip(class_of_state)
                .map(|(s, c)| (Box::<[u8]>::from(&s.0[..]), c))
                .collect());
        };
        // The inner table, by inner class: every state of the inner extended sub-lifecycle
        // the inner class holds. A class keeps the inputs, so each shares the inputs of the
        // state that stands for it, and only its inner S part is written.
        let mut by_inner: HashMap<u32, Vec<Box<[u8]>>> = HashMap::new();
        for (key, c) in sub.class_table()? {
            by_inner.entry(c).or_default().push(key);
        }
        let mut out = Vec::new();
        for (s, c) in run.states.iter().zip(class_of_state) {
            let state = s.into_state();
            let k = inner
                .class(&sub, &state)
                .expect("a state of the run holds an inner representative");
            for key in by_inner.get(&k).map_or(&[][..], Vec::as_slice) {
                let mut x = state.clone();
                inner.write(&mut x, key);
                out.push((Box::<[u8]>::from(&x.0[..]), c));
                if out.len() > CLASS_TABLE_CAP {
                    return Err(NoClassTable::TooLarge(out.len()));
                }
            }
        }
        Ok(out)
    }
}
