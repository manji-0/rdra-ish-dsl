//! Constraints, after-asserts, and properties as predicates over the system's vars.

mod atoms;
mod checks;
mod cross_atoms;
mod cross_rules;
mod display;
mod entity_rules;
mod guards;
mod temporal;

use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{At, CmpOp, Pred, Site, Term, VarId};
use crate::model::{CmpOpModel, CrossCmpRhs, CrossEntityCondition, EntityKey, FkId};

/// Binds each entity of a predicate to a slot reference.
type Binding = Vec<(EntityKey, At)>;

fn at_of(binding: &Binding, ek: EntityKey) -> Option<At> {
    binding.iter().find(|(k, _)| *k == ek).map(|(_, at)| *at)
}

/// The entities a body atom reads, left operand first.
pub(super) fn cond_entities(cond: &CrossEntityCondition) -> Vec<EntityKey> {
    match cond {
        CrossEntityCondition::Equals { column, .. } => vec![column.entity],
        CrossEntityCondition::Comparison(p) => match &p.rhs {
            CrossCmpRhs::Column(rc) => vec![p.lhs.entity, rc.entity],
            _ => vec![p.lhs.entity],
        },
    }
}

fn collect_vars(pred: &Pred, out: &mut Vec<VarId>) {
    let term = |t: &Term, out: &mut Vec<VarId>| {
        if let Term::Slot { var, .. } = t {
            out.push(*var);
        }
    };
    match pred {
        Pred::True | Pred::False => {}
        Pred::Cmp(a, _, b) => {
            term(a, out);
            term(b, out);
        }
        Pred::In(a, vs) => {
            term(a, out);
            vs.iter().for_each(|v| term(v, out));
        }
        Pred::Not(p) => collect_vars(p, out),
        Pred::And(ps) | Pred::Or(ps) => ps.iter().for_each(|p| collect_vars(p, out)),
        Pred::Forall { body, .. } | Pred::Exists { body, .. } => collect_vars(body, out),
    }
}

pub(super) fn implies(a: Pred, b: Pred) -> Pred {
    match a {
        Pred::True => b,
        a => Pred::Or(vec![Pred::Not(Box::new(a)), b]),
    }
}

fn cmp_op(op: CmpOpModel) -> CmpOp {
    match op {
        CmpOpModel::Eq => CmpOp::Eq,
        CmpOpModel::Ne => CmpOp::Ne,
        CmpOpModel::Lt => CmpOp::Lt,
        CmpOpModel::Le => CmpOp::Le,
        CmpOpModel::Gt => CmpOp::Gt,
        CmpOpModel::Ge => CmpOp::Ge,
        CmpOpModel::In | CmpOpModel::NotIn => {
            unreachable!("`in` / `not in` is read by `member_atom`")
        }
    }
}

/// `t in (v, ..)` when each equality is `t == v` over one and the same slot.
fn one_slot_set(eqs: &[Pred]) -> Option<Pred> {
    let mut slot = None;
    let mut values = Vec::new();
    for eq in eqs {
        let Pred::Cmp(t @ Term::Slot { .. }, CmpOp::Eq, v @ (Term::Lit(_) | Term::Int(_))) = eq
        else {
            return None;
        };
        if slot.get_or_insert(t) != &t {
            return None;
        }
        values.push(v.clone());
    }
    Some(Pred::In(slot?.clone(), values))
}

/// `AmbiguousOwnerLink` for a join of `a` and `b` that `statement` makes, which `candidates`
/// could follow: the rewrite names the first.
pub(super) fn ambiguous_owner_link(
    model: &crate::model::SemanticModel,
    statement: &str,
    a: EntityKey,
    b: EntityKey,
    candidates: &[FkId],
    site: &Site,
) -> StateDiag {
    let roles: Vec<&str> = candidates
        .iter()
        .map(|id| model.key_role(model.foreign_key(*id)))
        .collect();
    StateDiag::AmbiguousOwnerLink {
        statement: statement.to_string(),
        a: model.entities[a].id.clone(),
        b: model.entities[b].id.clone(),
        roles: roles.join(", "),
        rewrite: format!("{statement}.role({})", roles[0]),
        site: site.clone(),
    }
}
