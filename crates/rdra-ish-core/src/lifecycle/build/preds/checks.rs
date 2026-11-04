//! Column expressions (`@check`, logical `@derived`) as predicates, and the invariants they give.

use crate::lifecycle::build::naming::is_driven;
use crate::lifecycle::build::Builder;
use crate::lifecycle::types::{
    At, CmpOp, Domain, InvKind, InvText, Invariant, ModelRef, Pred, Site, Term, Value,
};
use crate::model::{CheckExpr, CmpRhs, ComparisonProp, EffectValue, EntityKey};

impl Builder<'_> {
    // ── Column expressions (`@check`, logical `@derived`) ────────────────────

    /// `expr` as every rule reads a comparison: two-valued, so a null operand satisfies no
    /// comparison but `!=`, and `col == null` admits it.
    fn check_expr(&mut self, ek: EntityKey, at: At, expr: &CheckExpr, site: &Site) -> Pred {
        match expr {
            CheckExpr::Cmp(prop) => self.cmp_atom(ek, at, prop, site),
            CheckExpr::Not(inner) => Pred::Not(Box::new(self.check_expr(ek, at, inner, site))),
            CheckExpr::And(a, b) | CheckExpr::Or(a, b) => {
                let parts = vec![
                    self.check_expr(ek, at, a, site),
                    self.check_expr(ek, at, b, site),
                ];
                if matches!(expr, CheckExpr::And(..)) {
                    Pred::And(parts)
                } else {
                    Pred::Or(parts)
                }
            }
        }
    }

    /// `expr` under SQL semantics, where a comparison over a null operand is unknown, for a
    /// logical `@derived` column, whose value is then neither true nor false.
    /// `passes(expr, true)` holds unless `expr` is false; `passes(expr, false)` holds unless
    /// it is true. Without a nullable operand they are `expr` and `not expr`.
    fn passes(
        &mut self,
        ek: EntityKey,
        at: At,
        expr: &CheckExpr,
        polarity: bool,
        site: &Site,
    ) -> Pred {
        match expr {
            CheckExpr::Cmp(prop) => {
                let cmp = self.cmp_atom(ek, at, prop, site);
                let cmp = if polarity {
                    cmp
                } else {
                    Pred::Not(Box::new(cmp))
                };
                let mut unknown = self.null_operands(ek, at, prop);
                if unknown.is_empty() {
                    return cmp;
                }
                unknown.push(cmp);
                Pred::Or(unknown)
            }
            CheckExpr::Not(inner) => self.passes(ek, at, inner, !polarity, site),
            CheckExpr::And(a, b) | CheckExpr::Or(a, b) => {
                let parts = vec![
                    self.passes(ek, at, a, polarity, site),
                    self.passes(ek, at, b, polarity, site),
                ];
                if matches!(expr, CheckExpr::And(..)) == polarity {
                    Pred::And(parts)
                } else {
                    Pred::Or(parts)
                }
            }
        }
    }

    /// `operand = null` for each nullable Int operand of an undriven comparison.
    fn null_operands(&self, ek: EntityKey, at: At, prop: &ComparisonProp) -> Vec<Pred> {
        if is_driven(self.model, ek, prop) {
            return Vec::new();
        }
        let rhs = match &prop.rhs {
            CmpRhs::Column(c) => Some(c.as_str()),
            CmpRhs::IntLit(_) | CmpRhs::Now => None,
            // `n == null` / `n == present` ask about null itself, so null is no unknown.
            CmpRhs::Value(_) | CmpRhs::Values(_) => return Vec::new(),
        };
        std::iter::once(prop.lhs_column.as_str())
            .chain(rhs)
            .filter_map(|column| match self.slot(ek, column, at)? {
                (term, Domain::NullableInt) => {
                    Some(Pred::Cmp(term, CmpOp::Eq, Term::Lit(Value::NULL)))
                }
                _ => None,
            })
            .collect()
    }

    /// A logical `@derived` column read as `column == value`: its expression, substituted.
    pub(super) fn derived_eq(
        &mut self,
        ek: EntityKey,
        at: At,
        expr: &CheckExpr,
        value: &EffectValue,
        site: &Site,
    ) -> Pred {
        let not = |p: Pred| Pred::Not(Box::new(p));
        let nullable = expr
            .comparisons()
            .iter()
            .any(|prop| !self.null_operands(ek, at, prop).is_empty());
        match value.normalize() {
            EffectValue::Bool(b) if !nullable => self.passes(ek, at, expr, *b, site),
            EffectValue::Bool(true) => not(self.passes(ek, at, expr, false, site)),
            EffectValue::Bool(false) => not(self.passes(ek, at, expr, true, site)),
            // Neither true nor false: a null operand left it unknown.
            EffectValue::Null => Pred::And(vec![
                self.passes(ek, at, expr, true, site),
                self.passes(ek, at, expr, false, site),
            ]),
            EffectValue::Present => not(Pred::And(vec![
                self.passes(ek, at, expr, true, site),
                self.passes(ek, at, expr, false, site),
            ])),
            _ => Pred::False,
        }
    }

    /// `Check_{entity}_{column}_{i}` per `@check`: every present slot satisfies the expression.
    pub(super) fn column_checks(&mut self) {
        let model = self.model;
        for (ek, entity) in &model.entities {
            for col in &entity.columns {
                for (i, check) in col.check_constraints.iter().enumerate() {
                    let fact = ModelRef::Column {
                        entity: ek,
                        column: col.name.clone(),
                    };
                    let site = Site::new(fact, &check.site);
                    let body = self.check_expr(ek, At::Bound(0), &check.expr, &site);
                    self.ts.invariants.push(Invariant {
                        name: format!("Check_{}_{}_{i}", entity.id, col.name),
                        kind: InvKind::Check,
                        binders: vec![self.ent_of[&ek]],
                        body,
                        text: InvText::Conditions(check.expr.display()),
                        label: None,
                        site,
                    });
                }
            }
        }
    }
}
