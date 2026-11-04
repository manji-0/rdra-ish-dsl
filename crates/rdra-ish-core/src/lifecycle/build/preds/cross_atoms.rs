//! Atoms over several entities: a cross-entity comparison and the owner or key link that ties its rows.

use super::display::cross_rhs_display;
use super::{ambiguous_owner_link, at_of, cmp_op, Binding};
use crate::lifecycle::build::Builder;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{At, CmpOp, Pred, Site, Term, VarId, VarKind};
use crate::lifecycle::EntId;
use crate::model::{CrossCmpRhs, CrossEntityCondition, EntityKey, FkId, Join};

impl Builder<'_> {
    pub(super) fn cross_atom(
        &mut self,
        cond: &CrossEntityCondition,
        binding: &Binding,
        site: &Site,
    ) -> Pred {
        match cond {
            CrossEntityCondition::Equals { column, value } => {
                let Some(at) = at_of(binding, column.entity) else {
                    return Pred::False;
                };
                self.eq_atom(column.entity, at, &column.column, value, site)
            }
            CrossEntityCondition::Comparison(prop) => {
                let what = format!(
                    "{}.{} {} {}",
                    self.model.entities[prop.lhs.entity].id,
                    prop.lhs.column,
                    prop.op.as_str(),
                    cross_rhs_display(self.model, &prop.rhs)
                );
                let Some(lat) = at_of(binding, prop.lhs.entity) else {
                    return Pred::False;
                };
                if let CrossCmpRhs::Values(values) = &prop.rhs {
                    let (ek, column) = (prop.lhs.entity, &prop.lhs.column);
                    return self.member_atom(ek, lat, column, prop.op, values, site);
                }
                let op = cmp_op(prop.op);
                let ordered = !matches!(op, CmpOp::Eq | CmpOp::Ne);
                match &prop.rhs {
                    CrossCmpRhs::Values(_) => unreachable!("a set is read above"),
                    CrossCmpRhs::Value(v) => {
                        self.value_atom(prop.lhs.entity, lat, &prop.lhs.column, op, v, site)
                    }
                    CrossCmpRhs::Column(rc) => {
                        let Some(rat) = at_of(binding, rc.entity) else {
                            return Pred::False;
                        };
                        let l = self.read_slot(prop.lhs.entity, &prop.lhs.column, lat, site);
                        let r = self.read_slot(rc.entity, &rc.column, rat, site);
                        match (l, r) {
                            (Some((lt, ld)), Some((rt, rd)))
                                if (ld.is_numeric() && rd.is_numeric())
                                    || (!ordered && ld == rd) =>
                            {
                                Pred::Cmp(lt, op, rt)
                            }
                            _ => {
                                self.diag(
                                    vec![prop.lhs.entity, rc.entity],
                                    StateDiag::UnsupportedComparison {
                                        comparison: what,
                                        reason: "operands are not both Int-typed or of one domain"
                                            .to_string(),
                                        site: site.clone(),
                                    },
                                );
                                Pred::False
                            }
                        }
                    }
                    CrossCmpRhs::IntLit(n) => {
                        match self.numeric_term(prop.lhs.entity, &prop.lhs.column, lat, site, &what)
                        {
                            Some(l) => {
                                let r = self.int_term(*n, prop.lhs.entity, site);
                                Pred::Cmp(l, op, r)
                            }
                            None => Pred::False,
                        }
                    }
                    CrossCmpRhs::Now => {
                        match self.numeric_term(prop.lhs.entity, &prop.lhs.column, lat, site, &what)
                        {
                            Some(l) => Pred::Cmp(l, op, Term::Now),
                            None => Pred::False,
                        }
                    }
                }
            }
        }
    }

    pub(super) fn cross_all(
        &mut self,
        conds: &[CrossEntityCondition],
        binding: &Binding,
        site: &Site,
    ) -> Pred {
        Pred::And(
            conds
                .iter()
                .map(|c| self.cross_atom(c, binding, site))
                .collect(),
        )
    }

    /// `owner[child] == parent` for the key [`SemanticModel::join_key`] picks between the two
    /// bound entities, `None` when no key joins them. Several keys and no `.by` naming one is
    /// `AmbiguousOwnerLink` on `statement`, and the two are left unlinked.
    ///
    /// [`SemanticModel::join_key`]: crate::model::SemanticModel::join_key
    pub(super) fn owner_link(
        &mut self,
        a: (EntityKey, At),
        b: (EntityKey, At),
        by: &[String],
        statement: &str,
        site: &Site,
    ) -> Option<Pred> {
        match self.model.join_key(a.0, b.0, by) {
            Join::Unrelated => None,
            Join::Key(fk) => self.key_link(fk, a, b),
            Join::Ambiguous(candidates) => {
                let diag = ambiguous_owner_link(self.model, statement, a.0, b.0, &candidates, site);
                self.diag(vec![a.0], diag);
                None
            }
        }
    }

    /// `owner[child] == parent` for key `fk`, which joins the two bound entities.
    pub(super) fn key_link(
        &self,
        fk: FkId,
        a: (EntityKey, At),
        b: (EntityKey, At),
    ) -> Option<Pred> {
        let &owner = self.owners.iter().find(|(var, ..)| {
            matches!(self.ts.vars[var.index()].kind, VarKind::Owner { fk: id, .. } if id == fk)
        })?;
        Self::link_pred(self.ent_of[&a.0], self.ent_of[&b.0], owner, a.1, b.1)
    }

    fn link_pred(
        ea: EntId,
        eb: EntId,
        (var, child, parent): (VarId, EntId, EntId),
        at_a: At,
        at_b: At,
    ) -> Option<Pred> {
        let (c, p) = if (child, parent) == (ea, eb) {
            (at_a, at_b)
        } else if (child, parent) == (eb, ea) {
            (at_b, at_a)
        } else {
            return None;
        };
        Some(Pred::Cmp(
            Term::Slot { var, at: c },
            CmpOp::Eq,
            Term::Index(p),
        ))
    }
}
