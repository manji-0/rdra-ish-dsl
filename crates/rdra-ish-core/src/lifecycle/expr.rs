//! The one evaluator of `Pred` over a `State`. The explorer uses it for guards, invariants,
//! and after-asserts; a printer walks the same tree.

use super::types::{At, CmpOp, Domain, EntId, Pred, Term, Value, VarId};
use super::TransitionSystem;

/// One flat byte per slot. Slot `(var, inst)` lives at `offset(var) + inst`; `now` has one slot.
/// It reads as a `StateSlice`, the form a run's store (`States`) lends its states in.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct State(pub(crate) Box<[u8]>);

/// A state's bytes wherever they are kept, as `str` is to `String`: in a `State`, or in the one
/// buffer `States` holds a run's states in.
#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct StateSlice(pub(crate) [u8]);

impl StateSlice {
    pub(crate) fn new(bytes: &[u8]) -> &StateSlice {
        // SAFETY: `StateSlice` is `repr(transparent)` over `[u8]`, so the two share a layout.
        unsafe { &*(bytes as *const [u8] as *const StateSlice) }
    }

    fn new_mut(bytes: &mut [u8]) -> &mut StateSlice {
        // SAFETY: as in `new`.
        unsafe { &mut *(bytes as *mut [u8] as *mut StateSlice) }
    }

    pub fn get(&self, ts: &TransitionSystem, var: VarId, slot: usize) -> Value {
        Value(self.0[ts.offset(var) + slot])
    }

    pub(crate) fn set(&mut self, ts: &TransitionSystem, var: VarId, slot: usize, value: Value) {
        self.0[ts.offset(var) + slot] = value.0;
    }

    pub fn is_present(&self, ts: &TransitionSystem, entity: EntId, slot: usize) -> bool {
        self.get(ts, ts.entity(entity).exists, slot) == Value::PRESENT
    }
}

impl std::ops::Deref for State {
    type Target = StateSlice;

    fn deref(&self) -> &StateSlice {
        StateSlice::new(&self.0)
    }
}

impl std::ops::DerefMut for State {
    fn deref_mut(&mut self) -> &mut StateSlice {
        StateSlice::new_mut(&mut self.0)
    }
}

impl std::borrow::Borrow<StateSlice> for State {
    fn borrow(&self) -> &StateSlice {
        self
    }
}

impl ToOwned for StateSlice {
    type Owned = State;

    fn to_owned(&self) -> State {
        State(self.0.into())
    }
}

impl PartialEq<StateSlice> for State {
    fn eq(&self, other: &StateSlice) -> bool {
        **self == *other
    }
}

impl PartialEq<State> for StateSlice {
    fn eq(&self, other: &State) -> bool {
        *self == **other
    }
}

/// Bindings a predicate reads: action params and quantifier binders, as slot indices.
pub(crate) struct Env<'a> {
    pub params: &'a [u8],
    pub bound: Vec<u8>,
    /// The successor `At::Next` reads.
    pub next: Option<&'a StateSlice>,
}

impl<'a> Env<'a> {
    pub fn new(params: &'a [u8]) -> Self {
        Env {
            params,
            bound: Vec::new(),
            next: None,
        }
    }

    fn slot(&self, at: At) -> usize {
        usize::from(match at {
            At::Param(p) | At::Next(p) => self.params[usize::from(p)],
            At::Bound(b) => self.bound[usize::from(b)],
        })
    }
}

enum Operand<'d> {
    Value(Value, Option<&'d Domain>),
    Number(i64),
    /// The slot index of a bound entity.
    Slot(u8),
}

impl TransitionSystem {
    pub(crate) fn eval(&self, pred: &Pred, state: &StateSlice, env: &mut Env) -> bool {
        match pred {
            Pred::True => true,
            Pred::False => false,
            Pred::Cmp(lhs, op, rhs) => {
                let lhs = self.operand(lhs, state, env);
                let rhs = self.operand(rhs, state, env);
                compare(&lhs, *op, &rhs)
            }
            Pred::In(lhs, values) => {
                let lhs = self.operand(lhs, state, env);
                values
                    .iter()
                    .any(|v| compare(&lhs, CmpOp::Eq, &self.operand(v, state, env)))
            }
            Pred::Not(inner) => !self.eval(inner, state, env),
            Pred::And(parts) => parts.iter().all(|p| self.eval(p, state, env)),
            Pred::Or(parts) => parts.iter().any(|p| self.eval(p, state, env)),
            Pred::Forall { entity, body } => self.quantify(*entity, body, state, env, true),
            Pred::Exists { entity, body } => self.quantify(*entity, body, state, env, false),
        }
    }

    fn quantify(
        &self,
        entity: EntId,
        body: &Pred,
        state: &StateSlice,
        env: &mut Env,
        all: bool,
    ) -> bool {
        for slot in 0..self.bounds.slots() {
            if !state.is_present(self, entity, slot) {
                continue;
            }
            env.bound.push(slot as u8);
            let holds = self.eval(body, state, env);
            env.bound.pop();
            if holds != all {
                return !all;
            }
        }
        all
    }

    /// Which of a rule's binders a premise asks rows of: those `pred` reads, or every one
    /// when it reads none.
    pub(crate) fn premise_rows(binders: usize, pred: &Pred) -> Vec<bool> {
        let mut read = vec![false; binders];
        for term in pred.terms() {
            let (Term::Slot {
                at: At::Bound(b), ..
            }
            | Term::Index(At::Bound(b))) = term
            else {
                continue;
            };
            if let Some(read) = read.get_mut(usize::from(*b)) {
                *read = true;
            }
        }
        if !read.contains(&true) {
            read.fill(true);
        }
        read
    }

    /// Whether `state` meets a premise: `pred` holds for some present rows of the binders it
    /// asks rows of (`premise_rows`). A binder it does not read needs no row, so a condition
    /// on one entity of a rule over two is met by a row of that entity alone.
    pub(crate) fn meets(&self, binders: &[EntId], pred: &Pred, state: &StateSlice) -> bool {
        let read = Self::premise_rows(binders.len(), pred);
        let slots = self.bounds.slots();
        let mut env = Env::new(&[]);
        env.bound = vec![0; binders.len()];
        loop {
            let present = (0..binders.len())
                .all(|k| !read[k] || state.is_present(self, binders[k], usize::from(env.bound[k])));
            if present && self.eval(pred, state, &mut env) {
                return true;
            }
            // The next rows of the binders the premise reads; the others stay at slot 0.
            let Some(k) = (0..binders.len())
                .rev()
                .find(|&k| read[k] && usize::from(env.bound[k]) + 1 < slots)
            else {
                return false;
            };
            env.bound[k] += 1;
            for later in (k + 1..binders.len()).filter(|&l| read[l]) {
                env.bound[later] = 0;
            }
        }
    }

    /// The first binding of `binders` (present slots, index order) under which `body` fails.
    pub(crate) fn find_violation(
        &self,
        binders: &[EntId],
        body: &Pred,
        state: &StateSlice,
        params: &[u8],
    ) -> Option<Vec<u8>> {
        let mut env = Env::new(params);
        self.search(binders, body, state, &mut env)
    }

    fn search(
        &self,
        binders: &[EntId],
        body: &Pred,
        state: &StateSlice,
        env: &mut Env,
    ) -> Option<Vec<u8>> {
        let Some((&first, rest)) = binders.split_first() else {
            return (!self.eval(body, state, env)).then(|| env.bound.clone());
        };
        for slot in 0..self.bounds.slots() {
            if !state.is_present(self, first, slot) {
                continue;
            }
            env.bound.push(slot as u8);
            let found = self.search(rest, body, state, env);
            env.bound.pop();
            if found.is_some() {
                return found;
            }
        }
        None
    }

    /// Whether the slot `term` names under `binding` holds the same value in `a` and `b`; a
    /// term that names no slot never changes.
    pub(crate) fn term_unchanged(
        &self,
        term: &Term,
        a: &StateSlice,
        b: &StateSlice,
        binding: &[u8],
    ) -> bool {
        match term {
            Term::Slot {
                var,
                at: At::Bound(k),
            } => {
                let slot = usize::from(binding[usize::from(*k)]);
                a.get(self, *var, slot) == b.get(self, *var, slot)
            }
            Term::Now => self
                .now
                .is_none_or(|now| a.get(self, now, 0) == b.get(self, now, 0)),
            _ => true,
        }
    }

    fn operand(&self, term: &Term, state: &StateSlice, env: &Env) -> Operand<'_> {
        match term {
            Term::Slot { var, at } => {
                let read = match at {
                    At::Next(_) => env.next.expect("a successor guard without a successor"),
                    _ => state,
                };
                Operand::Value(
                    read.get(self, *var, env.slot(*at)),
                    Some(&self.var(*var).domain),
                )
            }
            Term::Lit(value) => Operand::Value(*value, None),
            Term::Int(n) => Operand::Number(*n),
            Term::Index(at) => Operand::Slot(env.slot(*at) as u8),
            Term::Now => {
                let now = self.now.expect("predicate reads now without a clock");
                Operand::Number(i64::from(state.get(self, now, 0).0))
            }
        }
    }
}

/// Numeric when either side is an integer or an Int-typed slot; `null` is equal only to
/// `null` and is never ordered. Otherwise values compare as domain indices.
fn compare(lhs: &Operand, op: CmpOp, rhs: &Operand) -> bool {
    // An owner against a slot index: an unlinked optional owner names no slot.
    if let (Operand::Slot(i), Operand::Value(v, Some(d)))
    | (Operand::Value(v, Some(d)), Operand::Slot(i)) = (lhs, rhs)
    {
        let linked = d.slot(*v) == Some(*i);
        return match op {
            CmpOp::Eq => linked,
            CmpOp::Ne => !linked,
            _ => false,
        };
    }
    fn numeric_domain<'d>(o: &Operand<'d>) -> Option<&'d Domain> {
        match o {
            Operand::Value(_, d) => d.filter(|d| d.is_numeric()),
            Operand::Number(_) | Operand::Slot(_) => None,
        }
    }
    let domain = numeric_domain(lhs).or_else(|| numeric_domain(rhs));
    let is_number = |o: &Operand| matches!(o, Operand::Number(_));
    if domain.is_some() || is_number(lhs) || is_number(rhs) {
        // A slot reads through its own domain; a literal through the other operand's.
        let num = |o: &Operand| match o {
            Operand::Number(n) => Some(*n),
            Operand::Slot(s) => Some(i64::from(*s)),
            Operand::Value(v, d) => match d.filter(|d| d.is_numeric()).or(domain) {
                Some(d) => d.number(*v),
                None => Some(i64::from(v.0)),
            },
        };
        return compare_numbers(num(lhs), op, num(rhs));
    }
    let index = |o: &Operand| match o {
        Operand::Value(v, _) => v.0,
        Operand::Slot(s) => *s,
        Operand::Number(_) => unreachable!("numbers take the numeric path"),
    };
    ordered(index(lhs).cmp(&index(rhs)), op)
}

fn compare_numbers(lhs: Option<i64>, op: CmpOp, rhs: Option<i64>) -> bool {
    match (lhs, rhs) {
        (Some(a), Some(b)) => ordered(a.cmp(&b), op),
        _ => match op {
            CmpOp::Eq => lhs == rhs,
            CmpOp::Ne => lhs != rhs,
            _ => false,
        },
    }
}

fn ordered(ord: std::cmp::Ordering, op: CmpOp) -> bool {
    use std::cmp::Ordering::*;
    match op {
        CmpOp::Eq => ord == Equal,
        CmpOp::Ne => ord != Equal,
        CmpOp::Lt => ord == Less,
        CmpOp::Le => ord != Greater,
        CmpOp::Gt => ord == Greater,
        CmpOp::Ge => ord != Less,
    }
}
