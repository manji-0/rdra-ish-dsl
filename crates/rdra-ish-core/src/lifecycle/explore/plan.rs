//! What the move generator reads of each action's guards, worked out once per system (#253):
//! which guards read the pre-state and which the successor, and which parts of the pre-state
//! guards test one row and so become byte tests on a param's candidate slots.

use std::sync::OnceLock;

use crate::lifecycle::expr::{Env, StateSlice};
use crate::lifecycle::types::{ActId, Action, At, Pred, Term, VarId};
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
    /// The state bytes a step of the action may change, those of the vars of
    /// `action_writes`, as a mask over the state read `LANE` bytes to a word: in word `i`,
    /// the byte for state byte `LANE * i + j` is `0x00` when a var of `action_writes` holds
    /// that byte and `0xFF` when none does, and the bytes past the state's end are `0x00`.
    /// Every step is held to it (`TransitionSystem::check_writes`).
    pub writes: Box<[Word]>,
    /// The mask of the state's last `LANE` bytes, in the same form, when the state is at least
    /// `LANE` bytes wide and its width is no multiple of `LANE`: those bytes are read as one
    /// word, so a step's partial last word is checked without a copy.
    last: Word,
}

/// The word `Plan::writes` reads the state in.
pub(crate) type Word = u128;
/// The bytes of a `Word`.
const LANE: usize = std::mem::size_of::<Word>();

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

/// Bytes `LANE * i..LANE * (i + 1)` of a state, or of its zero-padded tail, as
/// `Plan::writes` reads them.
fn word(bytes: &[u8]) -> Word {
    let mut w = [0u8; LANE];
    w[..bytes.len()].copy_from_slice(bytes);
    Word::from_ne_bytes(w)
}

impl Plan {
    /// Whether a step of the action may change state byte `cell`: a var of `action_writes`
    /// holds it.
    pub fn writes(&self, cell: usize) -> bool {
        self.writes[cell / LANE].to_ne_bytes()[cell % LANE] == 0
    }

    /// Whether a step from `s` to `next` changes a byte no var of `action_writes` holds.
    #[inline]
    pub fn changes_unwritten(&self, s: &StateSlice, next: &StateSlice) -> bool {
        let (a, a_tail) = s.0.as_chunks::<LANE>();
        let (b, b_tail) = next.0.as_chunks::<LANE>();
        let mut changed: Word = 0;
        for ((x, y), keep) in a.iter().zip(b).zip(self.writes.iter()) {
            changed |= (Word::from_ne_bytes(*x) ^ Word::from_ne_bytes(*y)) & keep;
        }
        if !a_tail.is_empty() {
            changed |= match (s.0.last_chunk::<LANE>(), next.0.last_chunk::<LANE>()) {
                (Some(x), Some(y)) => {
                    (Word::from_ne_bytes(*x) ^ Word::from_ne_bytes(*y)) & self.last
                }
                _ => (word(a_tail) ^ word(b_tail)) & self.writes[0],
            };
        }
        changed != 0
    }

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
        let written = self.written_cells(action);
        Plan {
            rows: rows.into_iter().map(Vec::into_boxed_slice).collect(),
            #[cfg(feature = "perf")]
            pre_kinds: pre.iter().map(super::perf::Kind::of).collect(),
            pre: pre.into(),
            post: post.into(),
            writes: written.chunks(LANE).map(word).collect(),
            last: (written.last_chunk::<LANE>()).map_or(0, |w| Word::from_ne_bytes(*w)),
        }
    }

    /// Per state byte, `0x00` when a var of `action_writes(action)` holds it and `0xFF`
    /// when none does (`Plan::writes`). A test that plants a writer `action_writes` misses
    /// (`dropped("action_writes")`) leaves the first var of each action's set out.
    fn written_cells(&self, action: &Action) -> Vec<u8> {
        let mut cells = vec![0xFFu8; self.width];
        let planted = usize::from(crate::lifecycle::explore::symmetry::dropped(
            "action_writes",
        ));
        for var in self.action_writes(action).iter().skip(planted) {
            let at = self.offset(*var);
            let n = self.slots_of(&self.var(*var).kind);
            cells[at..at + n].fill(0);
        }
        cells
    }

    /// Checks that a step of `act` from `s` to `next` changes only bytes of vars
    /// `action_writes` names (claims A1, B6 of docs/theory/claims.md): the cone's closure
    /// (Lemma 2 of docs/theory/cones.md) and the prover's step pruning (Lemma F5 of
    /// docs/theory/z3.md) read `Wr(a)` as every var a step of `a` can change, so a step that
    /// changes another var would make a cone drop a writer it needs. Every step a run takes
    /// comes through here (`each_raw_move`); one that changes another var stops the run as a
    /// tool error, naming the action and the var (a debug or `shadow` build panics with
    /// `shadow:`).
    #[inline]
    pub(crate) fn check_writes(&self, plan: &Plan, act: ActId, s: &StateSlice, next: &StateSlice) {
        if plan.changes_unwritten(s, next) {
            self.writes_fault(plan, act, s, next);
        }
        #[cfg(feature = "shadow")]
        crate::shadow::compared(crate::shadow::Twin::Writes);
    }

    #[cold]
    #[inline(never)]
    fn writes_fault(&self, plan: &Plan, act: ActId, s: &StateSlice, next: &StateSlice) -> ! {
        let cell = (s.0.iter().zip(next.0.iter()).enumerate())
            .position(|(cell, (a, b))| a != b && !plan.writes(cell))
            .expect("a changed byte outside the mask");
        let var = (self.var_ids())
            .filter(|v| self.offset(*v) <= cell)
            .max_by_key(|v| self.offset(*v))
            .expect("a var holds every byte");
        let fault = format!(
            "step of {} changes {}, which action_writes leaves out",
            self.actions[act.index()].name,
            self.var_name(var)
        );
        crate::shadow_assert!(false, "shadow: {fault}");
        panic!("tool error: a {fault} (a tool bug, not a finding about the model)");
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
    // A quantifier, or a count, reads every row of its entity.
    let quantifier = |p: &Pred| {
        matches!(
            p,
            Pred::Forall { .. } | Pred::Exists { .. } | Pred::Count { .. }
        )
    };
    let mut read = None;
    let one = !pred.nodes().any(quantifier) && pred.terms().all(|t| term_on_row(t, &mut read));
    read.filter(|_| one)
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
