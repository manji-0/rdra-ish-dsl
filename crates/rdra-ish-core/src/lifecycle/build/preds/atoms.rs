//! Column atoms: a slot read, an equality, a comparison, and a set membership over one entity.

use std::collections::BTreeSet;

use super::{cmp_op, one_slot_set};
use crate::lifecycle::build::naming::is_driven;
use crate::lifecycle::build::Builder;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{At, CmpOp, Domain, Pred, Site, Term, Value};
use crate::model::{CmpOpModel, CmpRhs, ComparisonProp, DerivedExpr, EffectValue, EntityKey};

impl<'m> Builder<'m> {
    // ── Atoms ────────────────────────────────────────────────────────────────

    pub(super) fn slot(&self, ek: EntityKey, column: &str, at: At) -> Option<(Term, Domain)> {
        let e = self.ent_of[&ek];
        let var = *self.col_var.get(&(e, column.to_string()))?;
        Some((
            Term::Slot { var, at },
            self.ts.vars[var.index()].domain.clone(),
        ))
    }

    /// The `@derived` expression of a column, if it has one.
    fn derived(&self, ek: EntityKey, column: &str) -> Option<&'m DerivedExpr> {
        let model = self.model;
        model.entities[ek]
            .columns
            .iter()
            .find(|c| c.name == column)?
            .derived_expr
            .as_ref()
    }

    /// `slot`, after reporting a read of an arithmetic `@derived` column, which has no var.
    pub(super) fn read_slot(
        &mut self,
        ek: EntityKey,
        column: &str,
        at: At,
        site: &Site,
    ) -> Option<(Term, Domain)> {
        if let Some(DerivedExpr::Arith(_)) = self.derived(ek, column) {
            let column = format!("{}.{column}", self.model.entities[ek].id);
            let site = site.clone();
            self.diag(vec![ek], StateDiag::DerivedColumnRead { column, site });
            return None;
        }
        self.slot(ek, column, at)
    }

    /// `column == value` on one entity slot.
    pub(super) fn eq_atom(
        &mut self,
        ek: EntityKey,
        at: At,
        column: &str,
        value: &EffectValue,
        site: &Site,
    ) -> Pred {
        if let Some(DerivedExpr::Logical(expr)) = self.derived(ek, column) {
            return self.derived_eq(ek, at, expr, value, site);
        }
        if let EffectValue::Int(n) = value {
            if !(0..=i64::from(self.bounds.int_max)).contains(n) {
                self.out_of_bounds(*n, site, ek);
            }
        }
        let Some((term, domain)) = self.read_slot(ek, column, at, site) else {
            // Not a state var: a non-null column is always present and never null.
            return match value.normalize() {
                EffectValue::Present => Pred::True,
                _ => Pred::False,
            };
        };
        let lit = |v: Value| Pred::Cmp(term.clone(), CmpOp::Eq, Term::Lit(v));
        match (&domain, value.normalize()) {
            (Domain::Enum(values), EffectValue::EnumVariant(s)) => {
                crate::lifecycle::build::variant_index(values, s)
                    .map_or(Pred::False, |i| lit(Value(i)))
            }
            (Domain::Bool, EffectValue::Bool(b)) => {
                lit(if *b { Value::TRUE } else { Value::FALSE })
            }
            (Domain::Nullable, EffectValue::Null) => lit(Value::NULL),
            (Domain::Nullable, EffectValue::Present | EffectValue::Int(_)) => lit(Value::PRESENT),
            (Domain::Int | Domain::NullableInt, EffectValue::Int(n)) => {
                Pred::Cmp(term, CmpOp::Eq, Term::Int(*n))
            }
            (Domain::Int, EffectValue::Present) => Pred::True,
            (Domain::NullableInt, EffectValue::Null) => lit(Value::NULL),
            (Domain::NullableInt, EffectValue::Present) => {
                Pred::Cmp(term, CmpOp::Ne, Term::Lit(Value::NULL))
            }
            _ => Pred::False,
        }
    }

    pub(super) fn numeric_term(
        &mut self,
        ek: EntityKey,
        column: &str,
        at: At,
        site: &Site,
        what: &str,
    ) -> Option<Term> {
        if let Some(DerivedExpr::Arith(_)) = self.derived(ek, column) {
            self.read_slot(ek, column, at, site);
            return None;
        }
        match self.slot(ek, column, at) {
            Some((term, domain)) if domain.is_numeric() => Some(term),
            _ => {
                self.diag(
                    vec![ek],
                    StateDiag::UnsupportedComparison {
                        comparison: what.to_string(),
                        reason: format!("{column} is not an Int-typed column"),
                        site: site.clone(),
                    },
                );
                None
            }
        }
    }

    pub(super) fn int_term(&mut self, n: i64, ek: EntityKey, site: &Site) -> Term {
        if !(0..=i64::from(self.bounds.int_max)).contains(&n) {
            self.out_of_bounds(n, site, ek);
        }
        Term::Int(n)
    }

    /// A local comparison: the proposition var when a `drives(.., cmp, bool)` names it,
    /// otherwise the comparison itself over the bounded Int domain.
    pub(super) fn cmp_atom(
        &mut self,
        ek: EntityKey,
        at: At,
        prop: &ComparisonProp,
        site: &Site,
    ) -> Pred {
        let e = self.ent_of[&ek];
        if is_driven(self.model, ek, prop) {
            let var = self.prop_var[&(e, prop.axis_key())];
            return Pred::Cmp(Term::Slot { var, at }, CmpOp::Eq, Term::Lit(Value::TRUE));
        }
        if let CmpRhs::Values(values) = &prop.rhs {
            return self.member_atom(ek, at, &prop.lhs_column, prop.op, values, site);
        }
        let what = prop.display();
        let op = cmp_op(prop.op);
        let rhs = match &prop.rhs {
            CmpRhs::Values(_) => unreachable!("a set is read above"),
            CmpRhs::Value(v) => return self.value_atom(ek, at, &prop.lhs_column, op, v, site),
            CmpRhs::Column(c) if matches!(op, CmpOp::Eq | CmpOp::Ne) => {
                return self.columns_eq(ek, at, &prop.lhs_column, op, c, site, what);
            }
            CmpRhs::Column(c) => match self.numeric_term(ek, c, at, site, &what) {
                Some(t) => t,
                None => return Pred::False,
            },
            CmpRhs::IntLit(n) => self.int_term(*n, ek, site),
            CmpRhs::Now => Term::Now,
        };
        match self.numeric_term(ek, &prop.lhs_column, at, site, &what) {
            Some(lhs) => Pred::Cmp(lhs, op, rhs),
            None => Pred::False,
        }
    }

    /// `lhs == rhs` / `lhs != rhs` over two columns of one entity: both Int-typed, or of one
    /// domain, as two Enum columns with the same values are.
    #[allow(clippy::too_many_arguments)]
    fn columns_eq(
        &mut self,
        ek: EntityKey,
        at: At,
        lhs: &str,
        op: CmpOp,
        rhs: &str,
        site: &Site,
        what: String,
    ) -> Pred {
        let l = self.read_slot(ek, lhs, at, site);
        let r = self.read_slot(ek, rhs, at, site);
        match (l, r) {
            (Some((lt, ld)), Some((rt, rd)))
                if ld == rd || (ld.is_numeric() && rd.is_numeric()) =>
            {
                Pred::Cmp(lt, op, rt)
            }
            _ => {
                self.diag(
                    vec![ek],
                    StateDiag::UnsupportedComparison {
                        comparison: what,
                        reason: "operands are not both Int-typed or of one domain".to_string(),
                        site: site.clone(),
                    },
                );
                Pred::False
            }
        }
    }

    /// `column == value`, or its negation for `!=`.
    pub(super) fn value_atom(
        &mut self,
        ek: EntityKey,
        at: At,
        column: &str,
        op: CmpOp,
        value: &EffectValue,
        site: &Site,
    ) -> Pred {
        let eq = self.eq_atom(ek, at, column, value, site);
        if op == CmpOp::Ne {
            Pred::Not(Box::new(eq))
        } else {
            eq
        }
    }

    /// `column in (values)`: some `column == value` holds; its negation for `not in`. When
    /// each equality is one slot against one value, the set is one `Pred::In`.
    pub(super) fn member_atom(
        &mut self,
        ek: EntityKey,
        at: At,
        column: &str,
        op: CmpOpModel,
        values: &[EffectValue],
        site: &Site,
    ) -> Pred {
        let eqs: Vec<Pred> = values
            .iter()
            .map(|v| self.eq_atom(ek, at, column, v, site))
            .collect();
        let any = match one_slot_set(&eqs) {
            Some(set) if self.lists_whole_domain(&set) => Pred::True,
            Some(set) => set,
            None => Pred::Or(eqs),
        };
        match (op, any) {
            (CmpOpModel::NotIn, Pred::True) => Pred::False,
            (CmpOpModel::NotIn, any) => Pred::Not(Box::new(any)),
            (_, any) => any,
        }
    }

    /// `t in (v, ..)` names every value of the slot's domain. A slot always holds one of them,
    /// present, created, or deleted at base, so the set holds wherever it is read and no
    /// cone needs the column for it (`.none(Child.status in (<every value>))` says "no child").
    fn lists_whole_domain(&self, set: &Pred) -> bool {
        let Pred::In(Term::Slot { var, .. }, values) = set else {
            return false;
        };
        let domain = &self.ts.vars[var.index()].domain;
        let listed: BTreeSet<Value> = values
            .iter()
            .filter_map(|v| match v {
                Term::Lit(v) => Some(*v),
                Term::Int(n) => domain.of_number(*n, self.bounds),
                _ => None,
            })
            .collect();
        listed.len() == domain.size(self.bounds)
    }
}
