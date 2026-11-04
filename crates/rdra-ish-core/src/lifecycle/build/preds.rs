//! Constraints, after-asserts, and properties as predicates over the system's vars.

use std::collections::{BTreeSet, HashSet};

use super::naming::{is_driven, property_entity};
use super::Builder;
use crate::analysis::{applied_bodies, AppliedBody};
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{
    Action, ActionOrigin, AfterAssert, AfterStep, At, Birth, CmpOp, Domain, Effect, Guard,
    GuardWhy, InvKind, InvText, Invariant, Ltl, ModelRef, Param, ParamRole, Pred, Property, Rhs,
    Site, Start, Term, Value, VarId, VarKind,
};
use crate::lifecycle::EntId;
use crate::model::{
    ApiKey, CheckExpr, CmpOpModel, CmpRhs, ComparisonProp, ConditionKey, CrossCmpRhs,
    CrossConstraintScope, CrossEntityCondition, DerivedExpr, EffectValue, EntityKey, FkId,
    HoldsBody, Join, Min, QuantifierKind, RefAction, RelatedQuantifier, TemporalExpr,
    TemporalFormula, TemporalRhs, UseCaseKey,
};
use crate::tx::{UcWrite, WriteKind};

/// Binds each entity of a predicate to a slot reference.
type Binding = Vec<(EntityKey, At)>;

/// What an entity of a `holds` body's `.along` path is to the guard ([`Builder::path_roles`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PathRole {
    /// A row the action binds.
    Bound,
    /// A row some present instance supplies: the body compares it, or nothing counts it.
    Witness,
    /// The row a `.has`/`.none` counts along the path.
    Counted,
    /// A row of a counted entity's chain, between its anchor and it.
    Hop,
}

impl PathRole {
    /// Bound in the guard's outer scope, where a counted entity's chain starts.
    fn is_outer(self) -> bool {
        matches!(self, PathRole::Bound | PathRole::Witness)
    }
}

/// The part of an `.along` path a `.has`/`.none` counts along ([`Builder::quantified_along`]):
/// from its anchor to the counted entity, with the keys of its hops, and what an unbindable
/// hop is reported on.
struct Chain<'a> {
    segment: &'a [EntityKey],
    keys: &'a [Option<FkId>],
    owner: PathOwner<'a>,
    site: &'a Site,
}

/// The statement an `.along` path belongs to, which a hop no key binds is reported on
/// ([`Builder::path_links`]).
enum PathOwner<'a> {
    /// The `holds` body of `ck` applied to `uc`, comparing `read`: `UnbindableCondition`.
    Body {
        uc: UseCaseKey,
        ck: ConditionKey,
        read: &'a [EntityKey],
    },
    /// A rule over `scope`: `UnbindableScope`.
    Rule {
        constraint: &'a str,
        scope: &'a [EntityKey],
    },
}

fn at_of(binding: &Binding, ek: EntityKey) -> Option<At> {
    binding.iter().find(|(k, _)| *k == ek).map(|(_, at)| *at)
}

impl<'m> Builder<'m> {
    // ── Atoms ────────────────────────────────────────────────────────────────

    fn slot(&self, ek: EntityKey, column: &str, at: At) -> Option<(Term, Domain)> {
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
    fn read_slot(
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
    fn eq_atom(
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
                super::variant_index(values, s).map_or(Pred::False, |i| lit(Value(i)))
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

    fn numeric_term(
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

    fn int_term(&mut self, n: i64, ek: EntityKey, site: &Site) -> Term {
        if !(0..=i64::from(self.bounds.int_max)).contains(&n) {
            self.out_of_bounds(n, site, ek);
        }
        Term::Int(n)
    }

    /// A local comparison: the proposition var when a `drives(.., cmp, bool)` names it,
    /// otherwise the comparison itself over the bounded Int domain.
    fn cmp_atom(&mut self, ek: EntityKey, at: At, prop: &ComparisonProp, site: &Site) -> Pred {
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
    fn value_atom(
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
    fn member_atom(
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

    fn cross_atom(&mut self, cond: &CrossEntityCondition, binding: &Binding, site: &Site) -> Pred {
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

    fn cross_all(
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
    fn owner_link(
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
    fn key_link(&self, fk: FkId, a: (EntityKey, At), b: (EntityKey, At)) -> Option<Pred> {
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
    fn derived_eq(
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
    fn column_checks(&mut self) {
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

    // ── Invariants ───────────────────────────────────────────────────────────

    fn local_binding(ek: EntityKey) -> Binding {
        vec![(ek, At::Bound(0))]
    }

    fn local_conds(
        &mut self,
        ek: EntityKey,
        eqs: &[(String, EffectValue)],
        cmps: &[ComparisonProp],
        site: &Site,
    ) -> Vec<Pred> {
        let mut out: Vec<Pred> = eqs
            .iter()
            .map(|(c, v)| self.eq_atom(ek, At::Bound(0), c, v, site))
            .collect();
        out.extend(
            cmps.iter()
                .map(|p| self.cmp_atom(ek, At::Bound(0), p, site)),
        );
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn push_invariant(
        &mut self,
        prefix: &str,
        idx: usize,
        kind: InvKind,
        binding: &Binding,
        body: Pred,
        text: InvText,
        label: Option<String>,
        site: Site,
    ) {
        let entity = self.model.entities[binding[0].0].id.clone();
        self.ts.invariants.push(Invariant {
            name: format!("{prefix}_{entity}_{idx}"),
            kind,
            binders: binding.iter().map(|(ek, _)| self.ent_of[ek]).collect(),
            body,
            text,
            label,
            site,
        });
    }

    pub(super) fn invariants(&mut self) {
        let model = self.model;
        for (i, c) in model.forbidden_constraints.iter().enumerate() {
            let site = Site::new(ModelRef::Forbidden(i), &c.site);
            let conds = self.local_conds(c.entity, &c.conditions, &c.comparisons, &site);
            let text = InvText::Conditions(local_display(&c.conditions, &c.comparisons));
            let b = Self::local_binding(c.entity);
            let body = Pred::Not(Box::new(Pred::And(conds)));
            let label = c.label.clone();
            self.push_invariant(
                "Forbidden",
                i,
                InvKind::Forbidden,
                &b,
                body,
                text,
                label,
                site,
            );
        }
        for (i, c) in model.entity_invariants.iter().enumerate() {
            let site = Site::new(ModelRef::EntityInvariant(i), &c.site);
            let guards = self.local_conds(c.entity, &c.guards, &c.guard_comparisons, &site);
            let reqs = self.local_conds(c.entity, &c.requireds, &c.required_comparisons, &site);
            let text = InvText::WhenThen {
                guards: local_display(&c.guards, &c.guard_comparisons),
                requireds: local_display(&c.requireds, &c.required_comparisons),
            };
            let b = Self::local_binding(c.entity);
            let body = implies(Pred::And(guards), Pred::And(reqs));
            let label = c.label.clone();
            self.push_invariant("Inv", i, InvKind::Invariant, &b, body, text, label, site);
        }
        for (i, c) in model.required_constraints.iter().enumerate() {
            let site = Site::new(ModelRef::Required(i), &c.site);
            let conds = self.local_conds(c.entity, &c.conditions, &c.comparisons, &site);
            let text = InvText::Conditions(local_display(&c.conditions, &c.comparisons));
            let b = Self::local_binding(c.entity);
            self.push_invariant(
                "Required",
                i,
                InvKind::Required,
                &b,
                Pred::And(conds),
                text,
                c.label.clone(),
                site,
            );
        }
        for (i, c) in model.exclusive_constraints.iter().enumerate() {
            let site = Site::new(ModelRef::Exclusive(i), &c.site);
            let conds = self.local_conds(c.entity, &c.conditions, &c.comparisons, &site);
            let mut pairs = Vec::new();
            for x in 0..conds.len() {
                for y in x + 1..conds.len() {
                    pairs.push(Pred::And(vec![conds[x].clone(), conds[y].clone()]));
                }
            }
            let text = InvText::Conditions(local_display(&c.conditions, &c.comparisons));
            let b = Self::local_binding(c.entity);
            let body = Pred::Not(Box::new(Pred::Or(pairs)));
            let label = c.label.clone();
            self.push_invariant(
                "Exclusive",
                i,
                InvKind::Exclusive,
                &b,
                body,
                text,
                label,
                site,
            );
        }
        self.column_checks();
        self.cross_invariants();
        self.quantifiers();
        self.unique_keys();
        self.min_cards();
        self.cascade_deletes();
        self.driven_columns_written();
    }

    /// `DrivenComparisonColumnWritten` per use case that `sets` a column a driven comparison
    /// reads and does not drive that comparison: the comparison is a variable of its own,
    /// which only a `drives` moves, so it keeps its last driven truth.
    fn driven_columns_written(&mut self) {
        let model = self.model;
        let mut seen: Vec<(EntityKey, String)> = Vec::new();
        for pe in &model.proposition_effects {
            let key = pe.prop.axis_key();
            let Some(&e) = self.ent_of.get(&pe.entity) else {
                continue;
            };
            if seen.contains(&(pe.entity, key.clone()))
                || !self.prop_var.contains_key(&(e, key.clone()))
            {
                continue;
            }
            seen.push((pe.entity, key.clone()));
            let driving: Vec<(UseCaseKey, Site)> = model
                .proposition_effects
                .iter()
                .enumerate()
                .filter(|(_, other)| other.entity == pe.entity && other.prop.axis_key() == key)
                .map(|(i, other)| {
                    (
                        other.origin,
                        Site::new(ModelRef::PropositionEffect(i), &other.site),
                    )
                })
                .collect();
            let mut drivers: Vec<String> = Vec::new();
            for (uc, _) in &driving {
                let id = model.use_cases[*uc].id.clone();
                if !drivers.contains(&id) {
                    drivers.push(id);
                }
            }
            let mut columns = vec![pe.prop.lhs_column.as_str()];
            if let CmpRhs::Column(c) = &pe.prop.rhs {
                columns.push(c);
            }
            let proposition = format!("{}.{}", model.entities[pe.entity].id, pe.prop.display());
            for (i, ce) in model.column_effects.iter().enumerate() {
                if ce.entity != pe.entity
                    || !columns.contains(&ce.column.as_str())
                    || driving.iter().any(|(uc, _)| *uc == ce.origin)
                {
                    continue;
                }
                let mut sites = vec![Site::new(ModelRef::ColumnEffect(i), &ce.site)];
                sites.extend(driving.iter().map(|(_, site)| site.clone()));
                self.diag(
                    vec![pe.entity],
                    StateDiag::DrivenComparisonColumnWritten {
                        usecase: model.use_cases[ce.origin].id.clone(),
                        column: ce.column.clone(),
                        proposition: proposition.clone(),
                        drivers: drivers.clone(),
                        sites,
                    },
                );
            }
        }
    }

    /// `DeleteLeavesChildren` per key with `.on_delete(cascade)` and use case that deletes
    /// its parent and not its child: a delete removes the one row, so the children stay,
    /// and the lifecycle does not read the cascade.
    fn cascade_deletes(&mut self) {
        let model = self.model;
        for (_, fk) in model
            .foreign_keys()
            .filter(|(_, fk)| fk.on_delete == Some(RefAction::Cascade))
        {
            let (parent, child) = (self.ent_of[&fk.parent], self.ent_of[&fk.child]);
            let deletes = |a: &Action, e: EntId| {
                a.effects.iter().any(|eff| {
                    matches!(eff, Effect::Delete { param } if a.params[usize::from(*param)].entity == e)
                })
            };
            let deleting: Vec<UseCaseKey> = self
                .ts
                .actions
                .iter()
                .filter(|a| deletes(a, parent) && !deletes(a, child))
                .filter_map(|a| match a.origin {
                    ActionOrigin::UseCase { uc, .. } => Some(uc),
                    _ => None,
                })
                .collect();
            for uc in deleting {
                self.diag(
                    vec![fk.parent],
                    StateDiag::DeleteLeavesChildren {
                        usecase: model.use_cases[uc].id.clone(),
                        parent: model.entities[fk.parent].id.clone(),
                        child: model.entities[fk.child].id.clone(),
                        site: Site::new(ModelRef::Relation(fk.relation), &fk.site),
                    },
                );
            }
        }
    }

    /// `MinCard_{child}_{role}` per child-side range `1..*` / `1..1`: every present parent
    /// slot is owned by a present child.
    fn min_cards(&mut self) {
        let model = self.model;
        for (id, fk) in model
            .foreign_keys()
            .filter(|(_, fk)| fk.children.min == Min::One)
        {
            let at = |ek, b| (ek, At::Bound(b));
            // Child first, so a self-relate reads the child at the inner binder.
            let Some(link) = self.key_link(id, at(fk.child, 1), at(fk.parent, 0)) else {
                continue;
            };
            let (child, parent) = (
                model.entities[fk.child].id.clone(),
                model.entities[fk.parent].id.clone(),
            );
            let role = fk.role_or(&parent).to_string();
            let site = Site::new(ModelRef::Relation(fk.relation), &fk.site);
            let (c, p) = (self.ent_of[&fk.child], self.ent_of[&fk.parent]);
            let birth = |e: EntId| self.ts.entities[e.index()].birth;
            if birth(p) == Birth::PreExisting && birth(c) == Birth::Created {
                self.diag(
                    vec![fk.parent],
                    StateDiag::MinCardOfPreExistingParent {
                        child: child.clone(),
                        site: site.clone(),
                    },
                );
            }
            let creates = |a: &Action, e: EntId| {
                a.params
                    .iter()
                    .any(|q| q.entity == e && q.role == ParamRole::New)
            };
            // An action that creates the parent and links a child it creates or updates to it
            // through this key (`owner_bindings`) gives the parent its first child.
            let owner = self.owner_var(id);
            let gives_child = |a: &Action| {
                a.effects.iter().any(|eff| match eff {
                    Effect::Assign {
                        var,
                        rhs: Rhs::Param(q),
                        ..
                    } => Some(*var) == owner && a.params[usize::from(*q)].role == ParamRole::New,
                    _ => false,
                })
            };
            let lone: Vec<UseCaseKey> = self
                .ts
                .actions
                .iter()
                .filter(|a| creates(a, p) && !gives_child(a))
                .filter_map(|a| match a.origin {
                    ActionOrigin::UseCase { uc, .. } => Some(uc),
                    _ => None,
                })
                .collect();
            for uc in lone {
                self.diag(
                    vec![fk.parent],
                    StateDiag::MinCardNeedsCreate {
                        usecase: model.use_cases[uc].id.clone(),
                        child: child.clone(),
                        site: site.clone(),
                    },
                );
            }
            self.ts.invariants.push(Invariant {
                name: format!("MinCard_{child}_{role}"),
                kind: InvKind::MinCard,
                binders: vec![self.ent_of[&fk.parent]],
                body: Pred::Exists {
                    entity: self.ent_of[&fk.child],
                    body: Box::new(link),
                },
                text: InvText::MinCard {
                    child,
                    parent,
                    role,
                },
                label: None,
                site,
            });
        }
    }

    /// `Unique_{child}_{role}` per 1:1 key: two distinct present children never share a
    /// parent slot. An unlinked optional owner shares nothing.
    fn unique_keys(&mut self) {
        let model = self.model;
        for (id, fk) in model.foreign_keys().filter(|(_, fk)| fk.unique()) {
            let Some(owner) = self
                .ts
                .vars
                .iter()
                .position(|d| matches!(d.kind, VarKind::Owner { fk: f, .. } if f == id))
            else {
                continue;
            };
            let owner = VarId::at(owner);
            let c = self.ent_of[&fk.child];
            let slot = |b| Term::Slot {
                var: owner,
                at: At::Bound(b),
            };
            let mut apart = vec![
                Pred::Cmp(
                    Term::Index(At::Bound(0)),
                    CmpOp::Eq,
                    Term::Index(At::Bound(1)),
                ),
                Pred::Cmp(slot(0), CmpOp::Ne, slot(1)),
            ];
            if fk.optional() {
                apart.push(Pred::Cmp(slot(0), CmpOp::Eq, Term::Lit(Value::NULL)));
            }
            let (child, parent) = (
                model.entities[fk.child].id.clone(),
                model.entities[fk.parent].id.clone(),
            );
            let role = fk.role_or(&parent).to_string();
            self.ts.invariants.push(Invariant {
                name: format!("Unique_{child}_{role}"),
                kind: InvKind::Unique,
                binders: vec![c, c],
                body: Pred::Or(apart),
                text: InvText::Unique {
                    child,
                    parent,
                    role,
                },
                label: None,
                site: Site::new(ModelRef::Relation(fk.relation), &fk.site),
            });
        }
    }

    /// Binders for a cross constraint's scope, plus the relate links its `.along` names.
    fn scope_binding(
        &mut self,
        scope: &[EntityKey],
        semantics: &CrossConstraintScope,
        constraint: &str,
        site: &Site,
    ) -> Option<(Binding, Pred)> {
        let mut binding: Binding = Vec::new();
        for &ek in scope {
            if at_of(&binding, ek).is_none() {
                binding.push((ek, At::Bound(binding.len() as u8)));
            }
        }
        let CrossConstraintScope::RelationPath {
            path, links: keys, ..
        } = semantics
        else {
            return Some((binding, Pred::True));
        };
        if let Some(out) = scope.iter().find(|ek| !path.contains(ek)) {
            let id = self.model.entities[*out].id.clone();
            let reason = format!("along(..) does not cover {id}");
            self.unbindable_scope(scope, constraint, reason, site);
            return None;
        }
        // A path entity the conditions do not read joins the binders. The conditions do not
        // depend on it, so binding it universally under the link is binding it existentially.
        for &ek in path {
            if at_of(&binding, ek).is_none() {
                binding.push((ek, At::Bound(binding.len() as u8)));
            }
        }
        match self.hop_links(path, keys, &binding) {
            Ok(links) => Some((binding, Pred::And(links))),
            Err(reason) => {
                self.unbindable_scope(scope, constraint, reason, site);
                None
            }
        }
    }

    /// `UnbindableScope` on `scope`: the rule `constraint` binds nothing for `reason`.
    fn unbindable_scope(
        &mut self,
        scope: &[EntityKey],
        constraint: &str,
        reason: String,
        site: &Site,
    ) {
        self.diag(
            scope.to_vec(),
            StateDiag::UnbindableScope {
                constraint: constraint.to_string(),
                reason,
                site: site.clone(),
            },
        );
    }

    /// The owner links of the hops of an `.along(..)` path, each through the key resolved
    /// for it, over a `binding` that binds every path entity. `Err` says why the path binds
    /// nothing: it is too short, or no key joins a hop.
    fn hop_links(
        &self,
        path: &[EntityKey],
        keys: &[Option<FkId>],
        binding: &Binding,
    ) -> Result<Vec<Pred>, String> {
        if path.len() < 2 {
            return Err("along(..) needs at least two entities".to_string());
        }
        let mut links = Vec::new();
        for (i, pair) in path.windows(2).enumerate() {
            let key = keys.get(i).copied().flatten();
            let bound = |ek| at_of(binding, ek).expect("every path entity is bound");
            let (a, b) = (bound(pair[0]), bound(pair[1]));
            match key.and_then(|fk| self.key_link(fk, (pair[0], a), (pair[1], b))) {
                Some(link) => links.push(link),
                None => {
                    let (x, y) = (
                        &self.model.entities[pair[0]].id,
                        &self.model.entities[pair[1]].id,
                    );
                    return Err(format!("no N:1, 1:N, or 1:1 relate binds {x} and {y}"));
                }
            }
        }
        Ok(links)
    }

    fn cross_invariants(&mut self) {
        let model = self.model;
        for (i, c) in model.cross_forbidden_constraints.iter().enumerate() {
            let site = Site::new(ModelRef::CrossForbidden(i), &c.site);
            let conditions = cross_display(model, &c.conditions);
            let constraint = format!("forbidden({conditions})");
            let Some((binding, link)) =
                self.scope_binding(&c.scope, &c.scope_semantics, &constraint, &site)
            else {
                continue;
            };
            let conds = self.cross_all(&c.conditions, &binding, &site);
            let body = implies(link, Pred::Not(Box::new(conds)));
            let text = InvText::Cross {
                entities: entity_list(model, &c.scope),
                conditions,
            };
            self.push_invariant(
                "CrossForbidden",
                i,
                InvKind::CrossForbidden,
                &binding,
                body,
                text,
                c.label.clone(),
                site,
            );
        }
        for (i, c) in model.cross_entity_invariants.iter().enumerate() {
            let site = Site::new(ModelRef::CrossInvariant(i), &c.site);
            let guards = cross_display(model, &c.guards);
            let requireds = cross_display(model, &c.requireds);
            let constraint = format!("invariant(when {guards} then {requireds})");
            let Some((binding, link)) =
                self.scope_binding(&c.scope, &c.scope_semantics, &constraint, &site)
            else {
                continue;
            };
            let g = self.cross_all(&c.guards, &binding, &site);
            let r = self.cross_all(&c.requireds, &binding, &site);
            let body = implies(link, implies(g, r));
            let text = InvText::CrossWhenThen {
                entities: entity_list(model, &c.scope),
                guards,
                requireds,
            };
            self.push_invariant(
                "CrossInv",
                i,
                InvKind::CrossInvariant,
                &binding,
                body,
                text,
                c.label.clone(),
                site,
            );
        }
    }

    /// `Has` is a witness among the related rows, `None` a universal over them. The related
    /// instance is `At::Bound(depth)`, linked to every entity of `binding` it has an owner var
    /// with; with no such relate the related rows are quantified independently.
    ///
    /// The quantifier ranges over the rows present in the pre-state. Its conditions read the
    /// quantified row even where `binding` already holds the related entity (the instance a
    /// use case creates or updates, a witness, or the anchor itself), so a guard on a use case
    /// that creates the related entity never reads the instance being created.
    fn quantified(
        &mut self,
        q: &RelatedQuantifier,
        binding: &Binding,
        depth: u8,
        site: &Site,
    ) -> Pred {
        let mut inner: Binding = binding
            .iter()
            .filter(|(ek, _)| *ek != q.related)
            .copied()
            .collect();
        inner.push((q.related, At::Bound(depth)));
        let conds = self.cross_all(&q.conditions, &inner, site);
        let statement = quantifier_display(self.model, q);
        let links: Vec<Pred> = binding
            .iter()
            .filter_map(|&owner| {
                let related = (q.related, At::Bound(depth));
                self.owner_link(owner, related, &q.by, &statement, site)
            })
            .collect();
        let link = if links.is_empty() {
            Pred::True
        } else {
            Pred::And(links)
        };
        let entity = self.ent_of[&q.related];
        match q.kind {
            QuantifierKind::Has => Pred::Exists {
                entity,
                body: Box::new(Pred::And(vec![link, conds])),
            },
            QuantifierKind::None => Pred::Forall {
                entity,
                body: Box::new(implies(link, Pred::Not(Box::new(conds)))),
            },
        }
    }

    fn quantifiers(&mut self) {
        let model = self.model;
        for (i, c) in model.quantifier_constraints.iter().enumerate() {
            let q = &c.quantifier;
            let site = Site::new(ModelRef::Quantifier(i), &q.site);
            let anchor: Binding = vec![(c.anchor, At::Bound(0))];
            let guards = self.cross_all(&c.guards, &anchor, &site);
            let kind = match q.kind {
                QuantifierKind::Has => "has",
                QuantifierKind::None => "none",
            };
            let mut constraint = format!(
                "{} when ({}) {} {}",
                model.entities[c.anchor].id,
                cross_display(model, &c.guards),
                kind,
                model.entities[q.related].id,
            );
            if !q.conditions.is_empty() {
                constraint.push_str(&format!(" where ({})", cross_display(model, &q.conditions)));
            }
            let inner = match &c.along {
                CrossConstraintScope::RelationPath { path, links, .. } => {
                    constraint.push_str(&format!(" along {}", entity_list(model, path)));
                    let chain = Chain {
                        segment: path,
                        keys: links,
                        owner: PathOwner::Rule {
                            constraint: &constraint,
                            scope: &[c.anchor],
                        },
                        site: &site,
                    };
                    self.quantified_along(q, &anchor, 1, chain, &site)
                }
                CrossConstraintScope::GlobalProduct => self.quantified(q, &anchor, 1, &site),
            };
            let text = InvText::Quantifier {
                anchor: model.entities[c.anchor].id.clone(),
                related: model.entities[q.related].id.clone(),
                constraint,
            };
            self.push_invariant(
                "Quantifier",
                i,
                InvKind::Quantifier,
                &anchor,
                implies(guards, inner),
                text,
                c.label.clone(),
                site,
            );
        }
    }

    // ── R-guard: condition bodies ────────────────────────────────────────────

    /// One guard per `holds` body the action carries (`Builder::step_bodies`), each read on
    /// the action's pre-state. An entity the action binds reads its param: on the pre-state,
    /// or in the successor (`At::Next`) when the action creates it, so the body constrains
    /// what is created. Any other entity, an entity a later step of the use case writes
    /// included, is `Exists` over present instances of the pre-state, one witness per entity
    /// shared by the body's atoms, linked by a direct owner var (one hop) to each param or
    /// other witness it relates to; with no such relate it is independent.
    pub(super) fn condition_guards(
        &mut self,
        bodies: &[AppliedBody<'m>],
        params: &[Param],
    ) -> Vec<Guard> {
        let mut guards = Vec::new();
        for b in bodies {
            let site = Site::new(ModelRef::Condition(b.cond), &b.body.site);
            let pred = self.body_pred(b.usecase, b.cond, params, &site);
            guards.push(Guard {
                pred,
                why: GuardWhy::Condition {
                    cond: b.cond,
                    later: self.later_reads.get(&(b.usecase, b.cond)).copied(),
                },
                site,
            });
        }
        guards
    }

    /// `GuardReadsLaterStep` for each `holds` body that step 1 of the split use case `uc`
    /// carries and that compares an entity step 1 does not write and a later step creates:
    /// the guard is read before step 1, when the run has no row of it yet, so it reads rows
    /// that are not the run's. `groups` lists each step's writes, in transaction order.
    pub(super) fn later_step_reads(
        &mut self,
        uc: UseCaseKey,
        groups: &[Vec<UcWrite>],
        written_apis: &HashSet<ApiKey>,
    ) {
        let model = self.model;
        let Some(first) = groups.first() else {
            return;
        };
        for b in self.step_bodies(uc, first, 0, written_apis) {
            let later = b
                .body
                .conds
                .iter()
                .flat_map(cond_entities)
                .filter(|ek| !first.iter().any(|w| w.entity == *ek))
                .find_map(|ek| {
                    let (step, writes) =
                        groups.iter().enumerate().skip(1).find(|(_, writes)| {
                            writes
                                .iter()
                                .any(|w| w.entity == ek && w.kind == WriteKind::Creates)
                        })?;
                    let mut apis: Vec<String> = Vec::new();
                    for api in writes
                        .iter()
                        .filter(|w| w.entity == ek && w.kind == WriteKind::Creates)
                        .filter_map(|w| w.via_api)
                    {
                        let id = model.apis[api].id.clone();
                        if !apis.contains(&id) {
                            apis.push(id);
                        }
                    }
                    Some((ek, step + 1, apis))
                });
            let Some((ek, step, apis)) = later else {
                continue;
            };
            self.later_reads.insert((uc, b.cond), ek);
            self.diag(
                vec![ek],
                StateDiag::GuardReadsLaterStep {
                    condition: model.conditions[b.cond].id.clone(),
                    usecase: model.use_cases[uc].id.clone(),
                    entity: model.entities[ek].id.clone(),
                    step,
                    apis,
                    site: Site::new(ModelRef::Condition(b.cond), &b.body.site),
                },
            );
        }
    }

    /// The `holds` body of `ck` over an action's params, as [`Self::condition_guards`] reads it.
    /// With `.along(..)`, the path's keys join its rows and only the hops link: a path entity
    /// the action does not bind is a witness, or, when a `.has`/`.none` counts it and the body
    /// does not compare it, the quantifier's row, counted along the path from the nearest
    /// entity before it that the action binds or the body compares ([`Self::path_roles`]).
    /// Without a path, each witness links to each bound entity or other witness that one key
    /// joins it to.
    pub(super) fn body_pred(
        &mut self,
        uc: UseCaseKey,
        ck: ConditionKey,
        params: &[Param],
        site: &Site,
    ) -> Pred {
        let model = self.model;
        let Some(body) = model.conditions[ck].body.as_ref() else {
            return Pred::True;
        };
        let statement = format!("holds({}, ...)", model.conditions[ck].id);
        let body_site = Site::new(ModelRef::Condition(ck), &body.site);
        let mut binding: Binding = Vec::new();
        for e in params.iter().map(|p| p.entity) {
            let ek = self.ts.entities[e.index()].key;
            if at_of(&binding, ek).is_some() {
                continue;
            }
            let Some(i) = self.read_param(params, e, &body.by) else {
                continue;
            };
            let at = match params[usize::from(i)].role {
                ParamRole::New => At::Next(i),
                ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Earlier { .. } => {
                    At::Param(i)
                }
            };
            binding.push((ek, at));
        }
        let read: Vec<EntityKey> = body.conds.iter().flat_map(cond_entities).collect();
        let (path, keys): (&[EntityKey], &[Option<FkId>]) = match &body.along {
            CrossConstraintScope::RelationPath { path, links, .. } => (path, links),
            CrossConstraintScope::GlobalProduct => (&[], &[]),
        };
        let roles = self.path_roles(uc, ck, body, path, &binding, &read, &body_site);
        let mut witnesses = Vec::new();
        let outer = path
            .iter()
            .zip(&roles)
            .filter(|(_, role)| **role == PathRole::Witness)
            .map(|(ek, _)| *ek)
            .chain(read.iter().copied());
        for ek in outer {
            if at_of(&binding, ek).is_none() {
                binding.push((ek, At::Bound(witnesses.len() as u8)));
                witnesses.push(self.ent_of[&ek]);
            }
        }
        let witness = |(_, at): (EntityKey, At)| matches!(at, At::Bound(_));
        let mut parts = Vec::new();
        let owner = PathOwner::Body {
            uc,
            ck,
            read: &read,
        };
        // The pairs of entities a key joins, linked or reported as ambiguous.
        let mut joined: Vec<(EntityKey, EntityKey)> = Vec::new();
        if path.is_empty() {
            for (i, &x) in binding.iter().enumerate() {
                for &y in &binding[i + 1..] {
                    if !(witness(x) || witness(y)) {
                        continue;
                    }
                    if model.join_key(x.0, y.0, &body.by) != Join::Unrelated {
                        joined.push((x.0, y.0));
                    }
                    parts.extend(self.owner_link(x, y, &body.by, &statement, &body_site));
                }
            }
        } else {
            // The hops between two entities of the outer scope link there; a hop into a
            // counted entity's chain links inside its quantifier.
            let mut from = 0;
            while from < path.len() {
                let mut to = from + 1;
                while to < path.len() && roles[from].is_outer() && roles[to].is_outer() {
                    to += 1;
                }
                if to - from >= 2 {
                    let (run, hops) = (&path[from..to], &keys[from..to - 1]);
                    parts.extend(self.path_links(run, hops, &binding, &owner, &body_site));
                }
                from = to;
            }
            if path.len() < 2 {
                parts.extend(self.path_links(path, keys, &binding, &owner, &body_site));
            }
            joined.extend(path.windows(2).map(|hop| (hop[0], hop[1])));
        }
        self.unjoined_reads(uc, ck, &binding, &joined, &read, &body_site);
        parts.extend(
            body.conds
                .iter()
                .map(|c| self.cross_atom(c, &binding, site)),
        );
        let depth = witnesses.len() as u8;
        for q in &body.quantifiers {
            let counted = path
                .iter()
                .position(|ek| *ek == q.related)
                .filter(|&r| roles[r] == PathRole::Counted);
            let pred = match counted {
                Some(r) => {
                    let anchor = (0..r)
                        .rev()
                        .find(|&i| roles[i].is_outer())
                        .expect("a counted entity follows an anchor");
                    let chain = Chain {
                        segment: &path[anchor..=r],
                        keys: &keys[anchor..r],
                        owner: PathOwner::Body {
                            uc,
                            ck,
                            read: &read,
                        },
                        site: &body_site,
                    };
                    self.quantified_along(q, &binding, depth, chain, site)
                }
                None => self.quantified(q, &binding, depth, site),
            };
            parts.push(pred);
        }
        witnesses
            .into_iter()
            .rev()
            .fold(Pred::And(parts), |inner, entity| Pred::Exists {
                entity,
                body: Box::new(inner),
            })
    }

    /// The role of each entity of a `holds` body's `.along` path on an action that binds
    /// `binding` (its params), where the body compares `read`. A counted entity's chain runs
    /// from the nearest bound or compared entity before it; a path that names a counted
    /// entity before any such anchor, or goes on past a counted entity to an entity nothing
    /// counts, is `UnbindableCondition`, and the entity is read as a witness.
    #[allow(clippy::too_many_arguments)]
    fn path_roles(
        &mut self,
        uc: UseCaseKey,
        ck: ConditionKey,
        body: &HoldsBody,
        path: &[EntityKey],
        binding: &Binding,
        read: &[EntityKey],
        site: &Site,
    ) -> Vec<PathRole> {
        let model = self.model;
        let outer = |ek: EntityKey| at_of(binding, ek).is_some() || read.contains(&ek);
        let counting = |ek: EntityKey| {
            body.quantifiers
                .iter()
                .find(|q| q.related == ek)
                .map(|q| match q.kind {
                    QuantifierKind::Has => "has",
                    QuantifierKind::None => "none",
                })
        };
        let id = |ek: EntityKey| model.entities[ek].id.as_str();
        let mut roles: Vec<PathRole> = Vec::with_capacity(path.len());
        for (i, &ek) in path.iter().enumerate() {
            let anchor = (0..i).rev().find(|&j| outer(path[j]));
            let run_end = (i + 1..path.len())
                .find(|&j| outer(path[j]))
                .unwrap_or(path.len());
            let last_counted = (anchor.map_or(0, |a| a + 1)..i)
                .rev()
                .find(|&j| roles[j] == PathRole::Counted);
            let counted_after = (i + 1..run_end).any(|j| counting(path[j]).is_some());
            let reason = if at_of(binding, ek).is_some() {
                roles.push(PathRole::Bound);
                continue;
            } else if read.contains(&ek) {
                roles.push(PathRole::Witness);
                let Some(c) = last_counted else {
                    continue;
                };
                format!(
                    "along(..) joins {}, which the body compares, through {}, which `.{}` counts; put {} before {}",
                    id(ek), id(path[c]), counting(path[c]).unwrap_or("none"), id(ek), id(path[c])
                )
            } else if let Some(kind) = counting(ek) {
                if anchor.is_some() {
                    roles.push(PathRole::Counted);
                    continue;
                }
                roles.push(PathRole::Witness);
                format!(
                    "along(..) names {}, which `.{kind}` counts, before any entity {} binds or the body compares; start the path at one",
                    id(ek),
                    model.use_cases[uc].id
                )
            } else if counted_after && anchor.is_some() {
                roles.push(PathRole::Hop);
                continue;
            } else {
                roles.push(PathRole::Witness);
                let Some(c) = last_counted else {
                    continue;
                };
                format!(
                    "along(..) goes on past {}, which `.{}` counts, to {}, which nothing reads; end the path at {}",
                    id(path[c]), counting(path[c]).unwrap_or("none"), id(ek), id(path[c])
                )
            };
            self.unbindable(uc, ck, vec![ek], reason, site);
        }
        roles
    }

    /// `UnbindableCondition` on `entities`: the body of `ck` applied to `uc` guards nothing
    /// for `reason`.
    fn unbindable(
        &mut self,
        uc: UseCaseKey,
        ck: ConditionKey,
        entities: Vec<EntityKey>,
        reason: String,
        site: &Site,
    ) {
        let model = self.model;
        self.diag(
            entities,
            StateDiag::UnbindableCondition {
                condition: model.conditions[ck].id.clone(),
                usecase: model.use_cases[uc].id.clone(),
                reason,
                site: site.clone(),
            },
        );
    }

    /// [`Self::hop_links`] over `path`, or the error it reports on `owner` and no link.
    fn path_links(
        &mut self,
        path: &[EntityKey],
        keys: &[Option<FkId>],
        binding: &Binding,
        owner: &PathOwner<'_>,
        site: &Site,
    ) -> Vec<Pred> {
        match self.hop_links(path, keys, binding) {
            Ok(links) => links,
            Err(reason) => {
                match *owner {
                    PathOwner::Body { uc, ck, read } => {
                        self.unbindable(uc, ck, read.to_vec(), reason, site);
                    }
                    PathOwner::Rule { constraint, scope } => {
                        self.unbindable_scope(scope, constraint, reason, site);
                    }
                }
                Vec::new()
            }
        }
    }

    /// [`Self::quantified`] along a path: `chain.segment` runs from an entity `binding` binds
    /// to `q.related`, whose entities after the first are bound at `depth`, `depth + 1`, .. in
    /// path order. Every row of each entity between, and of `q.related`, that the hops' keys
    /// chain to the anchor's row is counted: `Forall` for `.none`, so no chain reaches a
    /// matching row, and `Exists` for `.has`, so some chain does.
    fn quantified_along(
        &mut self,
        q: &RelatedQuantifier,
        binding: &Binding,
        depth: u8,
        chain: Chain<'_>,
        site: &Site,
    ) -> Pred {
        let Chain {
            segment,
            keys,
            owner,
            site: path_site,
        } = chain;
        let rows = &segment[1..];
        let mut inner: Binding = binding
            .iter()
            .filter(|(ek, _)| !rows.contains(ek))
            .copied()
            .collect();
        for (i, &ek) in rows.iter().enumerate() {
            inner.push((ek, At::Bound(depth + i as u8)));
        }
        let links = self.path_links(segment, keys, &inner, &owner, path_site);
        let conds = self.cross_all(&q.conditions, &inner, site);
        let link = Pred::And(links);
        let body = match q.kind {
            QuantifierKind::Has => Pred::And(vec![link, conds]),
            QuantifierKind::None => implies(link, Pred::Not(Box::new(conds))),
        };
        rows.iter().rev().fold(body, |body, ek| {
            let entity = self.ent_of[ek];
            let body = Box::new(body);
            match q.kind {
                QuantifierKind::Has => Pred::Exists { entity, body },
                QuantifierKind::None => Pred::Forall { entity, body },
            }
        })
    }

    /// `UnjoinedGuardRead` for each entity the body compares that is a witness no chain of
    /// `joined` pairs reaches from a row the action binds: the guard holds when any row of it
    /// satisfies the body, wherever it is.
    fn unjoined_reads(
        &mut self,
        uc: UseCaseKey,
        ck: ConditionKey,
        binding: &Binding,
        joined: &[(EntityKey, EntityKey)],
        read: &[EntityKey],
        site: &Site,
    ) {
        let model = self.model;
        let bound: Vec<EntityKey> = binding
            .iter()
            .filter(|(_, at)| !matches!(at, At::Bound(_)))
            .map(|(ek, _)| *ek)
            .collect();
        let mut reached = bound.clone();
        let mut grew = true;
        while grew {
            grew = false;
            for &(a, b) in joined {
                for (from, to) in [(a, b), (b, a)] {
                    if reached.contains(&from) && !reached.contains(&to) {
                        reached.push(to);
                        grew = true;
                    }
                }
            }
        }
        let mut reported: Vec<EntityKey> = Vec::new();
        for &ek in read {
            if reached.contains(&ek) || reported.contains(&ek) {
                continue;
            }
            reported.push(ek);
            let usecase = &model.use_cases[uc].id;
            let entity = &model.entities[ek].id;
            let fix = match model.key_path(&bound, ek) {
                Some(path) => {
                    let ids: Vec<&str> =
                        path.iter().map(|e| model.entities[*e].id.as_str()).collect();
                    format!(
                        "add `.along({})` to read the row joined to the use case's, or write {entity} in {usecase}",
                        ids.join(", ")
                    )
                }
                None => format!(
                    "relate {entity} to an entity {usecase} writes and add `.along(...)`, or write {entity} in {usecase}"
                ),
            };
            self.diag(
                vec![ek],
                StateDiag::UnjoinedGuardRead {
                    condition: model.conditions[ck].id.clone(),
                    usecase: usecase.clone(),
                    entity: entity.clone(),
                    fix,
                    site: site.clone(),
                },
            );
        }
    }

    /// A body applied to a use case that writes nothing has no action to guard.
    pub(super) fn inert_conditions(&mut self) {
        let model = self.model;
        for b in applied_bodies(model) {
            if self.active_ucs.contains(&b.usecase) {
                continue;
            }
            self.unbindable(
                b.usecase,
                b.cond,
                b.body.conds.iter().flat_map(cond_entities).collect(),
                format!(
                    "the use case writes nothing, so no action carries the guard; to give it a step, write what it records, such as a flag `sets({uc}, Entity.reminded == true)`, or raise an event and guard the use case it triggers",
                    uc = model.use_cases[b.usecase].id
                ),
                &Site::new(ModelRef::Condition(b.cond), &b.body.site),
            );
        }
    }

    // ── R-after ──────────────────────────────────────────────────────────────

    /// One step, the last action of the anchor use case, so a split use case is checked once
    /// per run, after its last transaction group, on the run's rows: an entity an earlier
    /// step wrote is the last step's `Earlier` param. An entity no param of the step binds is
    /// universally bound, and `UnboundAfterRead`.
    pub(super) fn after_asserts(&mut self) {
        let model = self.model;
        for (i, ta) in model.temporal_assertions.iter().enumerate() {
            let site = Site::new(ModelRef::TemporalAssertion(i), &ta.site);
            let anchor = model.use_cases[ta.anchor].id.clone();
            let Some(&act) = self.uc_actions.get(&ta.anchor).and_then(|acts| acts.last()) else {
                if !self.active_ucs.contains(&ta.anchor) {
                    self.diag(
                        ta.scope.clone(),
                        StateDiag::AssertOnInertUseCase { anchor, site },
                    );
                }
                continue;
            };
            let mut entities: Vec<EntityKey> = ta.scope.clone();
            for cond in &ta.requireds {
                let ek = match cond {
                    CrossEntityCondition::Equals { column, .. } => column.entity,
                    CrossEntityCondition::Comparison(p) => p.lhs.entity,
                };
                if !entities.contains(&ek) {
                    entities.push(ek);
                }
            }
            let params = self.ts.actions[act.index()].params.clone();
            let mut binding: Binding = Vec::new();
            let mut binders = Vec::new();
            for &ek in &entities {
                let e = self.ent_of[&ek];
                match self.read_param(&params, e, &[]) {
                    Some(p) => binding.push((ek, At::Param(p))),
                    None => {
                        binding.push((ek, At::Bound(binders.len() as u8)));
                        binders.push(e);
                        if !params.iter().any(|p| p.entity == e) {
                            self.diag(
                                vec![ek],
                                StateDiag::UnboundAfterRead {
                                    anchor: anchor.clone(),
                                    entity: model.entities[ek].id.clone(),
                                    site: site.clone(),
                                },
                            );
                        }
                    }
                }
            }
            let post = self.cross_all(&ta.requireds, &binding, &site);
            let steps = vec![AfterStep { act, binders, post }];
            self.ts.after.push(AfterAssert {
                name: format!("AfterAssert_{anchor}_{i}"),
                anchor,
                steps,
                requireds: cross_display(model, &ta.requireds),
                label: ta.label.clone(),
                site,
            });
        }
    }

    // ── R-prop ───────────────────────────────────────────────────────────────

    pub(super) fn properties(&mut self) {
        let model = self.model;
        for (i, prop) in model.temporal_properties.iter().enumerate() {
            let site = Site::new(ModelRef::TemporalProperty(i), &prop.site);
            let mut binding: Binding = Vec::new();
            let formula = match &prop.formula {
                TemporalFormula::Always(e) => {
                    Ltl::Always(self.temporal(&prop.id, e, &mut binding, &site))
                }
                TemporalFormula::Eventually(e) => {
                    Ltl::Eventually(self.temporal(&prop.id, e, &mut binding, &site))
                }
                TemporalFormula::LeadsTo {
                    antecedent,
                    consequent,
                } => {
                    let a = self.temporal(&prop.id, antecedent, &mut binding, &site);
                    let c = self.temporal(&prop.id, consequent, &mut binding, &site);
                    Ltl::LeadsTo(a, c)
                }
            };
            for uk in &prop.fair {
                if !self.uc_actions.contains_key(uk) && !self.active_ucs.contains(uk) {
                    let diag = StateDiag::FairOnInertUseCase {
                        property: prop.id.clone(),
                        usecase: model.use_cases[*uk].id.clone(),
                        site: site.clone(),
                    };
                    self.diag(binding.iter().map(|(ek, _)| *ek).collect(), diag);
                }
            }
            self.ts.properties.push(Property {
                name: prop.id.clone(),
                binders: binding.iter().map(|(ek, _)| self.ent_of[ek]).collect(),
                formula,
                fair: prop.fair.clone(),
                site,
            });
        }
    }

    fn bind(binding: &mut Binding, ek: EntityKey) -> At {
        at_of(binding, ek).unwrap_or_else(|| {
            let at = At::Bound(binding.len() as u8);
            binding.push((ek, at));
            at
        })
    }

    fn temporal(
        &mut self,
        property: &str,
        expr: &TemporalExpr,
        binding: &mut Binding,
        site: &Site,
    ) -> Pred {
        match expr {
            TemporalExpr::Not(inner) => {
                Pred::Not(Box::new(self.temporal(property, inner, binding, site)))
            }
            TemporalExpr::And(a, b) => Pred::And(vec![
                self.temporal(property, a, binding, site),
                self.temporal(property, b, binding, site),
            ]),
            TemporalExpr::Or(a, b) => Pred::Or(vec![
                self.temporal(property, a, binding, site),
                self.temporal(property, b, binding, site),
            ]),
            TemporalExpr::Atom(atom) => {
                let model = self.model;
                let Some(ek) = property_entity(model, atom.entity.as_deref(), &atom.column) else {
                    self.diag(
                        Vec::new(),
                        StateDiag::UnresolvedPropertyAtom {
                            property: property.to_string(),
                            column: atom.column.clone(),
                            site: site.clone(),
                        },
                    );
                    return Pred::False;
                };
                let at = Self::bind(binding, ek);
                if let TemporalRhs::Values(values) = &atom.rhs {
                    return self.member_atom(ek, at, &atom.column, atom.op, values, site);
                }
                let op = cmp_op(atom.op);
                let what = format!("property {property}: {}", atom.column);
                match &atom.rhs {
                    TemporalRhs::Value(v) if matches!(op, CmpOp::Eq | CmpOp::Ne) => {
                        self.value_atom(ek, at, &atom.column, op, v, site)
                    }
                    TemporalRhs::Value(EffectValue::Int(n)) | TemporalRhs::IntLit(n) => {
                        match self.numeric_term(ek, &atom.column, at, site, &what) {
                            Some(l) => {
                                let r = self.int_term(*n, ek, site);
                                Pred::Cmp(l, op, r)
                            }
                            None => Pred::False,
                        }
                    }
                    TemporalRhs::Column { entity, column } => {
                        let rk = property_entity(
                            model,
                            entity.as_deref().or(atom.entity.as_deref()),
                            column,
                        );
                        let Some(rk) = rk else { return Pred::False };
                        let rat = Self::bind(binding, rk);
                        match (
                            self.read_slot(ek, &atom.column, at, site),
                            self.read_slot(rk, column, rat, site),
                        ) {
                            (Some((l, _)), Some((r, _))) => Pred::Cmp(l, op, r),
                            _ => Pred::False,
                        }
                    }
                    TemporalRhs::Value(_) => Pred::False,
                    TemporalRhs::Values(_) => unreachable!("a set is read above"),
                }
            }
        }
    }

    // ── Phase 1's frozen-constraint warning ─────────────────────────────────

    pub(super) fn unwritten_vars(&mut self) {
        let mut read: Vec<VarId> = Vec::new();
        for inv in &self.ts.invariants {
            collect_vars(&inv.body, &mut read);
        }
        for step in self.ts.after.iter().flat_map(|a| a.steps.iter()) {
            collect_vars(&step.post, &mut read);
        }
        let condition_guards = self.ts.actions.iter().flat_map(|a| a.guards.iter());
        let read_by_condition = |g: &&Guard| {
            matches!(
                g.why,
                GuardWhy::Condition { .. } | GuardWhy::ArmCondition { .. }
            )
        };
        for g in condition_guards.filter(read_by_condition) {
            collect_vars(&g.pred, &mut read);
        }
        let written: HashSet<VarId> = self
            .ts
            .actions
            .iter()
            .flat_map(|a| a.effects.iter())
            .filter_map(|eff| match eff {
                Effect::Assign { var, .. }
                | Effect::Transition { var, .. }
                | Effect::Choose { var, .. } => Some(*var),
                _ => None,
            })
            .collect();
        let mut seen = HashSet::new();
        for var in read {
            let decl = &self.ts.vars[var.index()];
            let entity = match &decl.kind {
                VarKind::Column { entity, .. } | VarKind::Prop { entity, .. } => *entity,
                _ => continue,
            };
            if written.contains(&var) || decl.start == Start::Any || !seen.insert(var) {
                continue;
            }
            let column = self.ts.var_name(var);
            let site = decl.site.clone();
            let ek = self.ts.entities[entity.index()].key;
            self.diag(
                vec![ek],
                StateDiag::UnwrittenConstrainedVar { column, site },
            );
        }
    }
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

fn local_display(eqs: &[(String, EffectValue)], cmps: &[ComparisonProp]) -> String {
    eqs.iter()
        .map(|(c, v)| format!("{c}={}", v.literal()))
        // A comparison prints as written, `status != oa` or `status in (a, b)`.
        .chain(cmps.iter().map(ComparisonProp::display))
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn cross_rhs_display(model: &crate::model::SemanticModel, rhs: &CrossCmpRhs) -> String {
    match rhs {
        CrossCmpRhs::Column(c) => format!("{}.{}", model.entities[c.entity].id, c.column),
        CrossCmpRhs::IntLit(n) => n.to_string(),
        CrossCmpRhs::Now => "now".to_string(),
        CrossCmpRhs::Value(v) => v.literal(),
        CrossCmpRhs::Values(vs) => crate::model::values_display(vs, EffectValue::literal),
    }
}

fn cross_display(model: &crate::model::SemanticModel, conds: &[CrossEntityCondition]) -> String {
    conds
        .iter()
        .map(|c| match c {
            CrossEntityCondition::Equals { column, value } => format!(
                "{}.{}={}",
                model.entities[column.entity].id,
                column.column,
                value.literal()
            ),
            CrossEntityCondition::Comparison(p) => format!(
                "{}.{} {} {}",
                model.entities[p.lhs.entity].id,
                p.lhs.column,
                p.op.as_str(),
                cross_rhs_display(model, &p.rhs)
            ),
        })
        .collect::<Vec<_>>()
        .join(" AND ")
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

/// `.none(Related.col == v).role(role)` as written, for a diagnostic that names the call.
fn quantifier_display(model: &crate::model::SemanticModel, q: &RelatedQuantifier) -> String {
    let method = match q.kind {
        QuantifierKind::Has => "has",
        QuantifierKind::None => "none",
    };
    let arg = if q.conditions.is_empty() {
        model.entities[q.related].id.clone()
    } else {
        "...".to_string()
    };
    format!(".{method}({arg})")
}

fn entity_list(model: &crate::model::SemanticModel, scope: &[EntityKey]) -> String {
    scope
        .iter()
        .map(|ek| model.entities[*ek].id.clone())
        .collect::<Vec<_>>()
        .join(", ")
}
