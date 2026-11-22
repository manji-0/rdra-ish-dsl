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
use crate::lifecycle::types::{At, CmpOp, Pred, Premise, Site, Term, VarId};
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
    out.extend(pred.terms().filter_map(|t| match t {
        Term::Slot { var, .. } => Some(*var),
        _ => None,
    }));
}

/// The premise that `parts` hold together, written `text`.
pub(super) fn premise(parts: Vec<Pred>, text: String) -> Premise {
    let mut parts: Vec<Pred> = parts.into_iter().filter(|p| *p != Pred::True).collect();
    let pred = match parts.len() {
        0 => Pred::True,
        1 => parts.remove(0),
        _ => Pred::And(parts),
    };
    Premise { pred, text }
}

/// The premises of a `forbidden` whose rows `link` ties and whose conditions are `conds`,
/// written `texts`: the linked rows, and, when there are several conditions, each on its own.
/// That the conditions never hold together is what the rule claims, so their conjunction is
/// no premise, and neither is the one condition of a rule that has no other. A condition of
/// several that no row ever meets alone makes the rule hold whatever the others say.
pub(super) fn forbidden_premises(link: &Pred, conds: &[Pred], texts: &[String]) -> Vec<Premise> {
    let mut premises = vec![premise(vec![link.clone()], String::new())];
    if conds.len() > 1 {
        premises.extend(
            conds
                .iter()
                .zip(texts)
                .map(|(cond, text)| premise(vec![cond.clone()], text.clone())),
        );
    }
    premises
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
