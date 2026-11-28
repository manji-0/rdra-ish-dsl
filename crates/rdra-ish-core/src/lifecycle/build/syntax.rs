//! The run-time certificate that a built system is in the slot syntax the Lean proofs take
//! (claims A8 and A18 of docs/theory/claims.md): `proofs/lean/RdraIsh/SymActions.lean`'s
//! `WF` and `Built`, which `SymActModel.lean::local_`, `::shape` and `::new_writes` and
//! `SymActions.lean::step_comm` assume, read field by field off each action. Every build
//! checks it at its end (`Builder::finish`), and a fault stops the build as a tool error: no
//! engine runs on a system the proofs do not cover.
//!
//! - `WF.guards` (`pwf`): every term of a guard reads a column or an owner on a row of its
//!   own entity: the var of `Term::Slot { var, at }` belongs to the entity `at` names (a
//!   param's, or a quantifier's, outermost first in the code and innermost first in Lean's
//!   `refEnt`), and `Term::Index(at)` names a param or a quantifier there is. An existence
//!   var or the clock in a slot term is outside the syntax: Lean reads an existence only
//!   through roles and quantifiers, and the clock through `Term::Now`.
//! - `WF.effs` (`ewf`): every effect writes a column or an owner on a row of its own
//!   entity, and names params the action has.
//! - `Built.new_create`: a `New` param has its `Create`.
//! - `Built.earlier_mark`: an `Earlier` param carries a conjunct `mark = k` on its own row,
//!   `Pred::Cmp(Slot { mark, Param(p) }, Eq, Lit(k))`, whose base is not `k`.
//! - The roles bind what Lean's `Rows.cand` says (`ParamRole::binds_exists`): a present row
//!   for `Existing`, `Parent` and `Triggered`, an absent one for `New`, any for `Earlier`.
//!   That `candidate_slots` applies that filter before any guard, and takes the lowest
//!   absent slot for `New`, is the code of `explore/moves.rs` and is read.
//!
//! The same pass checks the aliasing assumption of Proposition T1 (`shared_slot_writes`):
//! no two params of one action that can bind one slot write one var there with values that
//! can differ. An action that has such a pair is refused with `ConflictingEffects`, as one
//! param writing a slot twice is.

use std::fmt;

use crate::lifecycle::narrow::id8;
use crate::lifecycle::types::{
    Action, At, CmpOp, Effect, EntId, Param, ParamRole, Pred, Rhs, Term, Value, VarId, VarKind,
};
use crate::lifecycle::TransitionSystem;

/// The field of `WF` or `Built` an action fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SyntaxClause {
    /// `WF.guards`: a guard's term reads a var of another entity's row, or names no row.
    GuardWf,
    /// `WF.effs`: an effect writes a var of another entity's row, or names no param.
    EffectWf,
    /// `Built.new_create`: a `New` param without its `Create`.
    NewCreate,
    /// `Built.earlier_mark`: an `Earlier` param without its run-mark conjunct.
    EarlierMark,
    /// A role binds other than `Rows.cand`.
    Role,
}

/// One fault: the clause and what breaks it, the action named.
#[derive(Debug)]
pub(crate) struct SyntaxFault {
    pub(crate) clause: SyntaxClause,
    pub(crate) what: String,
}

impl fmt::Display for SyntaxFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let clause = match self.clause {
            SyntaxClause::GuardWf => "WF.guards",
            SyntaxClause::EffectWf => "WF.effs",
            SyntaxClause::NewCreate => "Built.new_create",
            SyntaxClause::EarlierMark => "Built.earlier_mark",
            SyntaxClause::Role => "Rows.cand",
        };
        write!(f, "{clause}: {}", self.what)
    }
}

/// What `Rows.cand` lets a role bind, as `binds_exists` states it: transcribed from
/// `proofs/lean/RdraIsh/Actions.lean`, not read off `binds_exists`.
fn lean_binds(role: ParamRole) -> Option<Value> {
    match role {
        ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Triggered => {
            Some(Value::PRESENT)
        }
        ParamRole::New => Some(Value::ABSENT),
        ParamRole::Earlier { .. } => None,
    }
}

impl TransitionSystem {
    /// The entity whose row a var is a column or an owner of (`cent`, `child`); `None` for
    /// an existence and the clock, which no slot term or effect of the syntax names.
    fn row_entity(&self, v: VarId) -> Option<EntId> {
        match self.var(v).kind {
            VarKind::Column { entity, .. }
            | VarKind::Prop { entity, .. }
            | VarKind::Run { entity, .. }
            | VarKind::Raised { entity, .. } => Some(entity),
            VarKind::Owner { child, .. } => Some(child),
            VarKind::Exists(_) | VarKind::Now => None,
        }
    }

    /// Every fault of the built actions against `WF` and `Built`; empty when every action is
    /// in the syntax.
    pub(crate) fn slot_syntax_faults(&self) -> Vec<SyntaxFault> {
        let mut out = Vec::new();
        for a in &self.actions {
            let mut fault = |clause, what: String| {
                out.push(SyntaxFault {
                    clause,
                    what: format!("{}: {what}", a.name),
                })
            };
            for g in &a.guards {
                let mut ctx = Vec::new();
                if let Some(what) = self.pred_wf(&a.params, &mut ctx, &g.pred) {
                    fault(SyntaxClause::GuardWf, what);
                }
            }
            for eff in &a.effects {
                if let Some(what) = self.effect_wf(&a.params, eff) {
                    fault(SyntaxClause::EffectWf, what);
                }
            }
            for (p, param) in a.params.iter().enumerate() {
                let p = id8(p);
                if param.role.binds_exists() != lean_binds(param.role) {
                    fault(SyntaxClause::Role, format!("param {p} ({:?})", param.role));
                }
                match param.role {
                    ParamRole::New if !a.effects.contains(&Effect::Create { param: p }) => {
                        fault(SyntaxClause::NewCreate, format!("param {p} is not created"));
                    }
                    ParamRole::Earlier { .. } if !self.has_run_mark(a, p, param.entity) => {
                        fault(
                            SyntaxClause::EarlierMark,
                            format!("param {p} has no run mark"),
                        );
                    }
                    _ => {}
                }
            }
        }
        out
    }

    /// `Built.earlier_mark` for param `p` of `a`: a guard `mark = k` on its row, `k` not
    /// the mark's base.
    fn has_run_mark(&self, a: &Action, p: u8, entity: EntId) -> bool {
        a.guards.iter().any(|g| match &g.pred {
            Pred::Cmp(
                Term::Slot {
                    var,
                    at: At::Param(q),
                },
                CmpOp::Eq,
                Term::Lit(k),
            ) => *q == p && self.row_entity(*var) == Some(entity) && self.var(*var).base != *k,
            _ => false,
        })
    }

    /// The entity of the row `at` names, in an action with `params` under the quantifiers
    /// `ctx` (outermost first, as `At::Bound` counts them).
    fn at_entity(params: &[Param], ctx: &[EntId], at: At) -> Option<EntId> {
        match at {
            At::Param(p) | At::Next(p) => params.get(usize::from(p)).map(|q| q.entity),
            At::Bound(b) => ctx.get(usize::from(b)).copied(),
        }
    }

    /// `twf`: why a term is not well typed, if it is not.
    fn term_wf(&self, params: &[Param], ctx: &[EntId], t: &Term) -> Option<String> {
        match *t {
            Term::Slot { var, at } => {
                let row = Self::at_entity(params, ctx, at);
                let own = self.row_entity(var);
                (row.is_none() || own.is_none() || row != own)
                    .then(|| format!("{} read at {at:?}", self.var_name(var)))
            }
            Term::Index(at) => Self::at_entity(params, ctx, at)
                .is_none()
                .then(|| format!("the index of {at:?}")),
            Term::Lit(_) | Term::Int(_) | Term::Now => None,
        }
    }

    /// `pwf`: why a predicate is not well typed, if it is not.
    fn pred_wf(&self, params: &[Param], ctx: &mut Vec<EntId>, pred: &Pred) -> Option<String> {
        match pred {
            Pred::True | Pred::False => None,
            Pred::Cmp(x, _, y) => self
                .term_wf(params, ctx, x)
                .or_else(|| self.term_wf(params, ctx, y)),
            Pred::In(x, ys) => std::iter::once(x)
                .chain(ys)
                .find_map(|t| self.term_wf(params, ctx, t)),
            Pred::Not(p) => self.pred_wf(params, ctx, p),
            Pred::And(ps) | Pred::Or(ps) => ps.iter().find_map(|p| self.pred_wf(params, ctx, p)),
            Pred::Forall { entity, body }
            | Pred::Exists { entity, body }
            | Pred::Count { entity, body, .. } => {
                ctx.push(*entity);
                let out = self.pred_wf(params, ctx, body);
                ctx.pop();
                out
            }
        }
    }

    /// `ewf`: why an effect is not well typed, if it is not.
    fn effect_wf(&self, params: &[Param], eff: &Effect) -> Option<String> {
        let named = |p: u8| params.get(usize::from(p)).map(|q| q.entity);
        match eff {
            Effect::Create { param } | Effect::Delete { param } => named(*param)
                .is_none()
                .then(|| format!("{eff:?} names no param")),
            Effect::Assign {
                var, param, rhs, ..
            } => {
                let rhs_ok = match rhs {
                    Rhs::Param(q) => named(*q).is_some(),
                    Rhs::Lit(_) => true,
                };
                (named(*param).is_none() || named(*param) != self.row_entity(*var) || !rhs_ok)
                    .then(|| format!("{} written at param {param}", self.var_name(*var)))
            }
            Effect::Transition { var, param, .. } | Effect::Choose { var, param, .. } => {
                (named(*param).is_none() || named(*param) != self.row_entity(*var))
                    .then(|| format!("{} written at param {param}", self.var_name(*var)))
            }
            Effect::Tick => None,
        }
    }

    /// The vars two params of `a` that can bind one slot both write with values that can
    /// differ, with the two params: Proposition T1 assumes there are none. Two writes of one
    /// literal (`Assign` of `Rhs::Lit`) agree in either order, in the native step and in the
    /// printer's `EXCEPT`, as a raise sets its mark to `true` on every row of the entity the
    /// step binds; every other pair of writes may differ. Two params of one entity can bind
    /// one slot unless their roles bind disjoint rows (`binds_exists`: present against
    /// absent), a guard `Index(p) != Index(q)` holds them apart, they carry conjuncts `m = j`
    /// and `m = k` on one var with `j != k`, or one is `New` and the other carries a conjunct
    /// `m = k` with `k` not `m`'s base (an absent row holds base values on every reachable
    /// state).
    pub(crate) fn shared_slot_writes(&self, a: &Action) -> Vec<(VarId, u8, u8)> {
        let mut out = Vec::new();
        let writes = |p: u8| -> Vec<(VarId, Option<Value>)> {
            (a.effects.iter())
                .filter(|e| effect_param(e) == Some(p))
                .flat_map(|e| {
                    let how = written_literal(e);
                    self.effect_writes(a, e).into_iter().map(move |v| (v, how))
                })
                .collect()
        };
        for (i, pi) in a.params.iter().enumerate() {
            for (j, pj) in a.params.iter().enumerate().skip(i + 1) {
                let (p, q) = (id8(i), id8(j));
                if pi.entity != pj.entity || !self.can_share_slot(a, (p, pi), (q, pj)) {
                    continue;
                }
                let theirs = writes(q);
                for (v, how) in writes(p) {
                    let differ = (theirs.iter())
                        .any(|(w, other)| *w == v && (how.is_none() || how != *other));
                    if differ && !out.contains(&(v, p, q)) {
                        out.push((v, p, q));
                    }
                }
            }
        }
        out
    }

    /// Whether params `p` and `q` of `a`, of one entity, can bind one slot (see
    /// `shared_slot_writes`).
    fn can_share_slot(&self, a: &Action, (p, pp): (u8, &Param), (q, pq): (u8, &Param)) -> bool {
        if let (Some(x), Some(y)) = (pp.role.binds_exists(), pq.role.binds_exists()) {
            if x != y {
                return false;
            }
        }
        let index = |r: u8| Term::Index(At::Param(r));
        let apart = a.guards.iter().any(|g| match &g.pred {
            Pred::Cmp(x, CmpOp::Ne, y) => {
                (*x == index(p) && *y == index(q)) || (*x == index(q) && *y == index(p))
            }
            _ => false,
        });
        if apart {
            return false;
        }
        let marks = |r: u8| -> Vec<(VarId, Value)> {
            (a.guards.iter())
                .filter_map(|g| match &g.pred {
                    Pred::Cmp(
                        Term::Slot {
                            var,
                            at: At::Param(s),
                        },
                        CmpOp::Eq,
                        Term::Lit(k),
                    ) if *s == r => Some((*var, *k)),
                    _ => None,
                })
                .collect()
        };
        let (mp, mq) = (marks(p), marks(q));
        let differ = mp
            .iter()
            .any(|(v, k)| mq.iter().any(|(w, j)| v == w && k != j));
        let off_base = |m: &[(VarId, Value)]| m.iter().any(|(v, k)| self.var(*v).base != *k);
        let new_apart = (pp.role == ParamRole::New && off_base(&mq))
            || (pq.role == ParamRole::New && off_base(&mp));
        !differ && !new_apart
    }
}

/// The literal an effect writes, the one kind of write two params can share: `None` for
/// any other effect.
fn written_literal(e: &Effect) -> Option<Value> {
    match e {
        Effect::Assign {
            rhs: Rhs::Lit(v), ..
        } => Some(*v),
        _ => None,
    }
}

/// The param an effect writes the row of; `None` for `Tick`.
fn effect_param(e: &Effect) -> Option<u8> {
    match e {
        Effect::Create { param }
        | Effect::Delete { param }
        | Effect::Assign { param, .. }
        | Effect::Transition { param, .. }
        | Effect::Choose { param, .. } => Some(*param),
        Effect::Tick => None,
    }
}

/// Faults the tests plant in a build (`witnesses::dropping`), before its checks run:
/// `slot_syntax` drops the first `Create`; `shared_slot` gives two present params of one
/// entity a write of two values of one column.
#[cfg(test)]
impl super::Builder<'_> {
    pub(super) fn plant_syntax_fault(&mut self) {
        use crate::lifecycle::explore::symmetry::dropped;
        use crate::lifecycle::types::{ModelRef, Site};
        if dropped("slot_syntax") {
            for a in &mut self.ts.actions {
                let create = a
                    .effects
                    .iter()
                    .position(|e| matches!(e, Effect::Create { .. }));
                if let Some(i) = create {
                    a.effects.remove(i);
                    return;
                }
            }
        }
        if dropped("shared_slot") {
            let ts = &self.ts;
            let present = |p: &Param| p.role.binds_exists() == Some(Value::PRESENT);
            let pair = ts.actions.iter().enumerate().find_map(|(i, a)| {
                let (p, q) = (0..a.params.len()).find_map(|p| {
                    (p + 1..a.params.len())
                        .find(|&q| {
                            a.params[p].entity == a.params[q].entity
                                && present(&a.params[p])
                                && present(&a.params[q])
                        })
                        .map(|q| (p, q))
                })?;
                let column = *ts.entity(a.params[p].entity).columns.first()?;
                Some((i, id8(p), id8(q), column))
            });
            if let Some((i, p, q, var)) = pair {
                let base = self.ts.var(var).base;
                for (param, v) in [(p, base), (q, Value(base.0.wrapping_add(1)))] {
                    self.ts.actions[i].effects.push(Effect::Assign {
                        var,
                        param,
                        rhs: Rhs::Lit(v),
                        site: Site::bare(ModelRef::Synthesized),
                    });
                }
            }
        }
    }
}
