//! The one evaluator of `Pred` over a `State`. The explorer uses it for guards, invariants,
//! and after-asserts; a printer walks the same tree.

use super::types::{At, CmpOp, Domain, EntId, Pred, Term, Value, VarId};
use super::TransitionSystem;

/// One flat byte per slot. Slot `(var, inst)` lives at `offset(var) + inst`; `now` has one slot.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct State(pub(crate) Box<[u8]>);

impl State {
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

/// Bindings a predicate reads: action params and quantifier binders, as slot indices.
pub(crate) struct Env<'a> {
    pub params: &'a [u8],
    pub bound: Vec<u8>,
}

impl<'a> Env<'a> {
    pub fn new(params: &'a [u8]) -> Self {
        Env {
            params,
            bound: Vec::new(),
        }
    }

    fn slot(&self, at: At) -> usize {
        usize::from(match at {
            At::Param(p) => self.params[usize::from(p)],
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
    pub(crate) fn eval(&self, pred: &Pred, state: &State, env: &mut Env) -> bool {
        match pred {
            Pred::True => true,
            Pred::False => false,
            Pred::Cmp(lhs, op, rhs) => {
                let lhs = self.operand(lhs, state, env);
                let rhs = self.operand(rhs, state, env);
                compare(&lhs, *op, &rhs)
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
        state: &State,
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

    /// The first binding of `binders` (present slots, index order) under which `body` fails.
    pub(crate) fn find_violation(
        &self,
        binders: &[EntId],
        body: &Pred,
        state: &State,
        params: &[u8],
    ) -> Option<Vec<u8>> {
        let mut env = Env::new(params);
        self.search(binders, body, state, &mut env)
    }

    fn search(
        &self,
        binders: &[EntId],
        body: &Pred,
        state: &State,
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

    fn operand(&self, term: &Term, state: &State, env: &Env) -> Operand<'_> {
        match term {
            Term::Slot { var, at } => Operand::Value(
                state.get(self, *var, env.slot(*at)),
                Some(&self.var(*var).domain),
            ),
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
        let num = |o: &Operand| match (o, domain) {
            (Operand::Number(n), _) => Some(*n),
            (Operand::Slot(s), _) => Some(i64::from(*s)),
            (Operand::Value(v, _), Some(d)) => d.number(*v),
            (Operand::Value(v, _), None) => Some(i64::from(v.0)),
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
