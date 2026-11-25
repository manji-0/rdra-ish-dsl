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
            Pred::Count {
                entity,
                body,
                op,
                n,
            } => {
                let count = self.count(*entity, body, state, env);
                ordered(count.cmp(&usize::from(*n)), *op)
            }
        }
    }

    /// The present slots of `entity` for which `body` holds, with the slot as the next
    /// binder.
    fn count(&self, entity: EntId, body: &Pred, state: &StateSlice, env: &mut Env) -> usize {
        let mut count = 0;
        for slot in 0..self.slots(entity) {
            if !state.is_present(self, entity, slot) {
                continue;
            }
            env.bound.push(slot as u8);
            count += usize::from(self.eval(body, state, env));
            env.bound.pop();
        }
        count
    }

    fn quantify(
        &self,
        entity: EntId,
        body: &Pred,
        state: &StateSlice,
        env: &mut Env,
        all: bool,
    ) -> bool {
        for slot in 0..self.slots(entity) {
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
                .find(|&k| read[k] && usize::from(env.bound[k]) + 1 < self.slots(binders[k]))
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
        for slot in 0..self.slots(first) {
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
///
/// This is term equality, which `col == null`, a copy between columns, and a unique set read.
/// A rule's `==` / `!=` between two columns is not one `Cmp`: the builder adds a present
/// guard on each nullable operand (`columns_cmp` in `build/preds`), so two null columns are
/// not equal there, as the language reference (§ Null) and SQL read them (#355).
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

/// Kani harnesses (`cargo kani -p rdra-ish-core`): the comparison of two operands, for every
/// value of both.
#[cfg(kani)]
mod proofs {
    use super::*;

    const OPS: [CmpOp; 6] = [
        CmpOp::Eq,
        CmpOp::Ne,
        CmpOp::Lt,
        CmpOp::Le,
        CmpOp::Gt,
        CmpOp::Ge,
    ];

    fn any_op() -> CmpOp {
        let k: usize = kani::any();
        kani::assume(k < OPS.len());
        OPS[k]
    }

    /// The operator that holds exactly when the operands are swapped.
    fn mirrored(op: CmpOp) -> CmpOp {
        match op {
            CmpOp::Eq => CmpOp::Eq,
            CmpOp::Ne => CmpOp::Ne,
            CmpOp::Lt => CmpOp::Gt,
            CmpOp::Le => CmpOp::Ge,
            CmpOp::Gt => CmpOp::Lt,
            CmpOp::Ge => CmpOp::Le,
        }
    }

    /// Any operand: an integer literal, a slot index, a literal value, or a value read from a
    /// var of any domain.
    fn any_operand(domains: &[Domain; 8]) -> Operand<'_> {
        match kani::any::<u8>() % 4 {
            0 => Operand::Number(kani::any()),
            1 => Operand::Slot(kani::any()),
            2 => Operand::Value(Value(kani::any()), None),
            _ => {
                let k: usize = kani::any();
                kani::assume(k < domains.len());
                Operand::Value(Value(kani::any()), Some(&domains[k]))
            }
        }
    }

    /// One of each domain. `compare` never reads an `Enum`'s names, so this one has none.
    fn domains() -> [Domain; 8] {
        [
            Domain::Existence,
            Domain::Enum(Vec::new()),
            Domain::Bool,
            Domain::Nullable,
            Domain::Int,
            Domain::NullableInt,
            Domain::Instance {
                parent: EntId::at(0),
                optional: false,
            },
            Domain::Instance {
                parent: EntId::at(0),
                optional: true,
            },
        ]
    }

    /// Two numbers compare as `i64` orders them; `null` equals only `null`, differs from every
    /// number, and is never ordered. This is term equality: `col == null` holds on a null
    /// column. Two null columns in a rule are not equal, by the guard the builder adds (#355).
    #[kani::proof]
    fn numbers_compare_as_i64_and_null_is_never_ordered() {
        let (a, b): (Option<i64>, Option<i64>) = (kani::any(), kani::any());
        let op = any_op();
        let want = match (a, b) {
            (Some(a), Some(b)) => match op {
                CmpOp::Eq => a == b,
                CmpOp::Ne => a != b,
                CmpOp::Lt => a < b,
                CmpOp::Le => a <= b,
                CmpOp::Gt => a > b,
                CmpOp::Ge => a >= b,
            },
            (None, None) => matches!(op, CmpOp::Eq),
            _ => matches!(op, CmpOp::Ne),
        };
        assert_eq!(compare_numbers(a, op, b), want);
    }

    /// For any two operands, of any kinds and domains: `compare` does not panic, `!=` is the
    /// negation of `==`, and swapping the operands mirrors the operator.
    #[kani::proof]
    fn compare_negates_and_mirrors() {
        let domains = domains();
        let lhs = any_operand(&domains);
        let rhs = any_operand(&domains);
        let op = any_op();
        assert_eq!(
            compare(&lhs, CmpOp::Ne, &rhs),
            !compare(&lhs, CmpOp::Eq, &rhs)
        );
        assert_eq!(compare(&lhs, op, &rhs), compare(&rhs, mirrored(op), &lhs));
    }
}
