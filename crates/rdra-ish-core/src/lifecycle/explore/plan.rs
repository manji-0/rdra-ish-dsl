//! What the move generator reads of each action's guards, worked out once per system (#253):
//! which guards read the pre-state and which the successor, and which parts of the pre-state
//! guards test one row and so become byte tests on a param's candidate slots.

use std::sync::OnceLock;

use crate::lifecycle::expr::{Env, StateSlice};
use crate::lifecycle::types::{Action, At, Pred, Term, VarId};
use crate::lifecycle::TransitionSystem;

/// Per action of a system, in action order.
pub(crate) struct Plan {
    /// Per param, the tests a slot must pass to be a candidate: the pre-state guard conjuncts
    /// that read that param's row and nothing else but literals.
    pub rows: Box<[Box<[RowTest]>]>,
    /// What is left of the pre-state guards once the row tests hold, in guard order: whole
    /// guards, or the conjuncts of a guard that the row tests do not decide.
    pub pre: Box<[Pred]>,
    /// The guards read on the pre-state and the successor together, as indices into
    /// `Action::guards`, in guard order.
    pub post: Box<[usize]>,
    /// Per `pre` item, what it reads (`perf::Kind`), for the evaluator counts of #300.
    #[cfg(feature = "perf")]
    pub pre_kinds: Box<[super::perf::Kind]>,
}

/// A conjunct that reads one var of one param's row: it holds on a slot exactly when the
/// byte the var holds there is in `values`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RowTest {
    pub var: VarId,
    /// `TransitionSystem::offset(var)`.
    pub offset: usize,
    /// Bit `b` is set when the conjunct holds on a slot whose byte is `b`.
    pub values: [u64; 4],
}

impl RowTest {
    pub fn admits(&self, byte: u8) -> bool {
        self.values[usize::from(byte >> 6)] >> (byte & 63) & 1 == 1
    }
}

/// A system's plans, built the first time the system generates moves. A clone starts empty,
/// so a system cloned and then edited (a mutant without a guard, an extended sub-lifecycle)
/// builds its own; `TransitionSystem::layout` empties it as well.
#[derive(Default)]
pub(crate) struct Plans(OnceLock<Box<[Plan]>>);

impl Clone for Plans {
    fn clone(&self) -> Self {
        Plans::default()
    }
}

/// The plans are a function of the system's actions and vars, so two systems equal in
/// those are equal whatever their cells hold.
impl PartialEq for Plans {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for Plans {}

impl std::fmt::Debug for Plans {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Plans")
    }
}

impl Plan {
    /// Whether `slot` passes the row tests of param `param`.
    pub fn row_holds(&self, param: usize, state: &StateSlice, slot: usize) -> bool {
        self.rows[param]
            .iter()
            .all(|t| t.admits(state.0[t.offset + slot]))
    }
}

impl TransitionSystem {
    /// The plan of every action, in action order.
    pub(crate) fn plans(&self) -> &[Plan] {
        self.plans
            .0
            .get_or_init(|| self.actions.iter().map(|a| self.plan_of(a)).collect())
    }

    fn plan_of(&self, action: &Action) -> Plan {
        let mut rows: Vec<Vec<RowTest>> = vec![Vec::new(); action.params.len()];
        let mut pre = Vec::new();
        let mut post = Vec::new();
        for (g, guard) in action.guards.iter().enumerate() {
            if guard.reads_successor() {
                post.push(g);
                continue;
            }
            let parts = conjuncts(&guard.pred);
            let mut left = Vec::new();
            for part in &parts {
                match one_row(part) {
                    Some((param, var)) => {
                        let test = self.row_test(action, part, param, var);
                        let tests = &mut rows[usize::from(param)];
                        match tests.iter_mut().find(|t| t.var == var) {
                            Some(t) => {
                                for (a, b) in t.values.iter_mut().zip(test.values) {
                                    *a &= b;
                                }
                            }
                            None => tests.push(test),
                        }
                    }
                    None => left.push(*part),
                }
            }
            if left.len() == parts.len() {
                pre.push(guard.pred.clone());
            } else {
                pre.extend(left.into_iter().cloned());
            }
        }
        for tests in &mut rows {
            tests.retain(|t| t.values != [u64::MAX; 4]);
        }
        Plan {
            rows: rows.into_iter().map(Vec::into_boxed_slice).collect(),
            #[cfg(feature = "perf")]
            pre_kinds: pre.iter().map(super::perf::Kind::of).collect(),
            pre: pre.into(),
            post: post.into(),
        }
    }

    /// The test of a conjunct `one_row` maps to `(param, var)`: `eval` itself, run on every
    /// byte the var could hold, so the test agrees with it by construction.
    pub(super) fn row_test(&self, action: &Action, part: &Pred, param: u8, var: VarId) -> RowTest {
        let offset = self.offset(var);
        let mut state = vec![0u8; self.width];
        let params = vec![0u8; action.params.len()];
        debug_assert!(usize::from(param) < params.len());
        let mut values = [0u64; 4];
        for byte in 0..=u8::MAX {
            state[offset] = byte;
            if self.eval(part, StateSlice::new(&state), &mut Env::new(&params)) {
                values[usize::from(byte >> 6)] |= 1 << (byte & 63);
            }
        }
        RowTest {
            var,
            offset,
            values,
        }
    }
}

/// The conjuncts of `pred`: the parts of nested `And`s, left to right, and `pred` itself when
/// it is no `And`. `pred` holds exactly when all of them do.
pub(super) fn conjuncts(pred: &Pred) -> Vec<&Pred> {
    match pred {
        Pred::And(parts) => parts.iter().flat_map(conjuncts).collect(),
        _ => vec![pred],
    }
}

/// The one param and var `pred` reads, when every slot it reads is that var on that param's
/// row in the pre-state and the rest are literals: a function of one byte of the state.
/// Anything else, a quantifier or count, a bound or successor slot, a slot index, `now`, or no slot at
/// all, is `None`, and `eval` decides it.
pub(super) fn one_row(pred: &Pred) -> Option<(u8, VarId)> {
    let mut read = None;
    if reads_one_row(pred, &mut read) {
        read
    } else {
        None
    }
}

fn reads_one_row(pred: &Pred, read: &mut Option<(u8, VarId)>) -> bool {
    match pred {
        Pred::True | Pred::False => true,
        Pred::Cmp(a, _, b) => term_on_row(a, read) && term_on_row(b, read),
        Pred::In(a, values) => term_on_row(a, read) && values.iter().all(|v| term_on_row(v, read)),
        Pred::Not(body) => reads_one_row(body, read),
        Pred::And(parts) | Pred::Or(parts) => parts.iter().all(|p| reads_one_row(p, read)),
        // A quantifier, or a count, reads every row of its entity.
        Pred::Forall { .. } | Pred::Exists { .. } | Pred::Count { .. } => false,
    }
}

fn term_on_row(term: &Term, read: &mut Option<(u8, VarId)>) -> bool {
    match *term {
        Term::Slot {
            var,
            at: At::Param(p),
        } => *read.get_or_insert((p, var)) == (p, var),
        Term::Slot {
            at: At::Bound(_) | At::Next(_),
            ..
        } => false,
        Term::Lit(_) | Term::Int(_) => true,
        Term::Index(_) | Term::Now => false,
    }
}
