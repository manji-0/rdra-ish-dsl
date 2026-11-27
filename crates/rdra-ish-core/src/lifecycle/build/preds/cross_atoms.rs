//! Atoms over several entities: a cross-entity comparison and the owner or key link that ties its rows.

use super::display::cross_rhs_display;
use super::{ambiguous_owner_link, at_of, cmp_op, columns_cmp, Binding};
use crate::lifecycle::build::Builder;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{At, CmpOp, Pred, Site, Term, VarId, VarKind};
use crate::lifecycle::types::{Domain, Value};
use crate::lifecycle::EntId;
use crate::model::{CrossCmpRhs, CrossEntityCondition, EntityKey, FkId, Join, RowMark};

impl Builder<'_> {
    /// `E.in_run(UC)`, `E.in_run(UC, k)`, or `E.raised(Ev)` on the row at `at`: its run mark
    /// is not `none` or is `k`, or its latch is set. A mark the lifecycle has no var for is
    /// `NoSuchMark`, except `in_run` of a use case the scope leaves out, whose mark stays
    /// `none`.
    fn mark_atom(&mut self, ek: EntityKey, mark: RowMark, at: At, site: &Site) -> Pred {
        let model = self.model;
        let e = self.ent_of[&ek];
        let entity = &model.entities[ek].id;
        let reason = match mark {
            RowMark::InRun { usecase, step } => {
                let uc = &model.use_cases[usecase].id;
                if let Some(&var) = self.run_marks.get(&(usecase, e)) {
                    let slot = Term::Slot { var, at };
                    let Some(k) = step else {
                        return Pred::Cmp(slot, CmpOp::Ne, Term::Lit(Value(0)));
                    };
                    let Domain::Enum(names) = &self.ts.vars[var.index()].domain else {
                        unreachable!("a run mark is an Enum");
                    };
                    if let Some(v) = self.ts.vars[var.index()].domain.of_name(&k.to_string()) {
                        return Pred::Cmp(slot, CmpOp::Eq, Term::Lit(v));
                    }
                    let last = names.len() - 1;
                    let range = match last {
                        1 => "1".to_string(),
                        n => format!("1 to {n}"),
                    };
                    format!("{uc} has {} steps, so k can only be {range}", names.len())
                } else if !self.uc_actions.contains_key(&usecase)
                    && self.active_ucs.contains(&usecase)
                {
                    // The scope leaves the use case out: no run starts, and the mark stays
                    // `none`.
                    return Pred::False;
                } else if self.run_marks.keys().any(|(u, _)| *u == usecase) {
                    format!(
                        "no step of {uc} before the last writes {entity}, so {entity} has no run mark for it"
                    )
                } else {
                    format!(
                        "{uc} is not split into transaction groups, so no row has a run mark for it"
                    )
                }
            }
            RowMark::Raised { event } => {
                let ev = &model.events[event].id;
                let latch = self.ts.vars.iter().position(|v| {
                    matches!(&v.kind, VarKind::Raised { entity: x, event: y, handler: None }
                        if *x == e && y == ev)
                });
                if let Some(i) = latch {
                    let var = VarId::at(i);
                    return Pred::Cmp(Term::Slot { var, at }, CmpOp::Eq, Term::Lit(Value::TRUE));
                }
                format!(
                    "{entity} keeps no latch for {ev}: no use case raises it on {entity} for an uncounted `triggers({ev}, ..)` to read"
                )
            }
        };
        let mark = mark.display(model, ek);
        let site = site.clone();
        self.diag(vec![ek], StateDiag::NoSuchMark { mark, reason, site });
        Pred::False
    }

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
            CrossEntityCondition::Mark { entity, mark } => {
                let Some(at) = at_of(binding, *entity) else {
                    return Pred::False;
                };
                self.mark_atom(*entity, *mark, at, site)
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
                            (Some(l), Some(r))
                                if (l.1.is_numeric() && r.1.is_numeric())
                                    || (!ordered && l.1.compares_with(&r.1)) =>
                            {
                                columns_cmp(l, op, r)
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
