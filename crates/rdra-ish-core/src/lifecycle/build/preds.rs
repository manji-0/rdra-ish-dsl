//! Constraints, after-asserts, and properties as predicates over the system's vars.

use std::collections::HashSet;

use super::naming::{applied_bodies, is_driven, property_entity};
use super::Builder;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{
    AfterAssert, AfterStep, At, CmpOp, Domain, Effect, Guard, GuardWhy, InvKind, InvText,
    Invariant, Ltl, ModelRef, Param, ParamRole, Pred, Property, Site, Start, Term, Value, VarId,
    VarKind,
};
use crate::lifecycle::EntId;
use crate::model::{
    CmpOpModel, CmpRhs, ComparisonProp, CrossCmpRhs, CrossConstraintScope, CrossEntityCondition,
    EffectValue, EntityKey, FkId, HoldsBody, QuantifierKind, TemporalExpr, TemporalFormula,
    TemporalRhs, UseCaseKey,
};

/// Binds each entity of a predicate to a slot reference.
type Binding = Vec<(EntityKey, At)>;

fn at_of(binding: &Binding, ek: EntityKey) -> Option<At> {
    binding.iter().find(|(k, _)| *k == ek).map(|(_, at)| *at)
}

impl Builder<'_> {
    // ── Atoms ────────────────────────────────────────────────────────────────

    fn slot(&self, ek: EntityKey, column: &str, at: At) -> Option<(Term, Domain)> {
        let e = self.ent_of[&ek];
        let var = *self.col_var.get(&(e, column.to_string()))?;
        Some((
            Term::Slot { var, at },
            self.ts.vars[var.index()].domain.clone(),
        ))
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
        if let EffectValue::Int(n) = value {
            if !(0..=i64::from(self.bounds.int_max)).contains(n) {
                self.out_of_bounds(*n, site, ek);
            }
        }
        let Some((term, domain)) = self.slot(ek, column, at) else {
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

    /// A local comparison: the proposition var when `sets(.., cmp, bool)` drives it,
    /// otherwise the comparison itself over the bounded Int domain.
    fn cmp_atom(&mut self, ek: EntityKey, at: At, prop: &ComparisonProp, site: &Site) -> Pred {
        let e = self.ent_of[&ek];
        if is_driven(self.model, ek, prop) {
            let var = self.prop_var[&(e, prop.axis_key())];
            return Pred::Cmp(Term::Slot { var, at }, CmpOp::Eq, Term::Lit(Value::TRUE));
        }
        let what = prop.display();
        let Some(lhs) = self.numeric_term(ek, &prop.lhs_column, at, site, &what) else {
            return Pred::False;
        };
        let rhs = match &prop.rhs {
            CmpRhs::Column(c) => match self.numeric_term(ek, c, at, site, &what) {
                Some(t) => t,
                None => return Pred::False,
            },
            CmpRhs::IntLit(n) => self.int_term(*n, ek, site),
            CmpRhs::Now => Term::Now,
        };
        Pred::Cmp(lhs, cmp_op(prop.op), rhs)
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
                let op = cmp_op(prop.op);
                let ordered = !matches!(op, CmpOp::Eq | CmpOp::Ne);
                match &prop.rhs {
                    CrossCmpRhs::Column(rc) => {
                        let Some(rat) = at_of(binding, rc.entity) else {
                            return Pred::False;
                        };
                        let l = self.slot(prop.lhs.entity, &prop.lhs.column, lat);
                        let r = self.slot(rc.entity, &rc.column, rat);
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

    /// `owner[child] == parent` for the first relate between the two bound entities.
    fn owner_link(&self, a: (EntityKey, At), b: (EntityKey, At)) -> Option<Pred> {
        self.owners.iter().find_map(|&owner| {
            Self::link_pred(self.ent_of[&a.0], self.ent_of[&b.0], owner, a.1, b.1)
        })
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
        site: Site,
    ) {
        let entity = self.model.entities[binding[0].0].id.clone();
        self.ts.invariants.push(Invariant {
            name: format!("{prefix}_{entity}_{idx}"),
            kind,
            binders: binding.iter().map(|(ek, _)| self.ent_of[ek]).collect(),
            body,
            text,
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
            self.push_invariant("Forbidden", i, InvKind::Forbidden, &b, body, text, site);
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
            self.push_invariant("Inv", i, InvKind::Invariant, &b, body, text, site);
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
            self.push_invariant("Exclusive", i, InvKind::Exclusive, &b, body, text, site);
        }
        self.cross_invariants();
        self.quantifiers();
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
        let fail = |b: &mut Self, reason: String| {
            b.diag(
                scope.to_vec(),
                StateDiag::UnbindableScope {
                    constraint: constraint.to_string(),
                    reason,
                    site: site.clone(),
                },
            );
            None
        };
        if path.len() < 2 {
            return fail(self, "along(..) needs at least two entities".to_string());
        }
        if let Some(out) = scope.iter().find(|ek| !path.contains(ek)) {
            let id = self.model.entities[*out].id.clone();
            return fail(self, format!("along(..) does not cover {id}"));
        }
        let mut links = Vec::new();
        for (i, pair) in path.windows(2).enumerate() {
            let key = keys.get(i).copied().flatten();
            let (Some(a), Some(b)) = (at_of(&binding, pair[0]), at_of(&binding, pair[1])) else {
                return fail(
                    self,
                    "along(..) names an entity outside the scope".to_string(),
                );
            };
            match key.and_then(|fk| self.key_link(fk, (pair[0], a), (pair[1], b))) {
                Some(link) => links.push(link),
                None => {
                    let (x, y) = (
                        self.model.entities[pair[0]].id.clone(),
                        self.model.entities[pair[1]].id.clone(),
                    );
                    return fail(
                        self,
                        format!("no N:1, 1:N, or 1:1 relate binds {x} and {y}"),
                    );
                }
            }
        }
        Some((binding, Pred::And(links)))
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
                site,
            );
        }
    }

    fn quantifiers(&mut self) {
        let model = self.model;
        for (i, q) in model.quantifier_constraints.iter().enumerate() {
            let site = Site::new(ModelRef::Quantifier(i), &q.site);
            let anchor: Binding = vec![(q.anchor, At::Bound(0))];
            let both: Binding = vec![(q.anchor, At::Bound(0)), (q.related, At::Bound(1))];
            let guards = self.cross_all(&q.guards, &anchor, &site);
            let related = self.cross_all(&q.related_conditions, &both, &site);
            // Without a relate the related rows are quantified independently.
            let link = self
                .owner_link((q.anchor, At::Bound(0)), (q.related, At::Bound(1)))
                .unwrap_or(Pred::True);
            let r = self.ent_of[&q.related];
            let (kind, inner) = match q.kind {
                QuantifierKind::Has => (
                    "has",
                    Pred::Exists {
                        entity: r,
                        body: Box::new(Pred::And(vec![link, related])),
                    },
                ),
                QuantifierKind::None => (
                    "none",
                    Pred::Forall {
                        entity: r,
                        body: Box::new(implies(link, Pred::Not(Box::new(related)))),
                    },
                ),
            };
            let constraint = format!(
                "{} when ({}) {} {} where ({})",
                model.entities[q.anchor].id,
                cross_display(model, &q.guards),
                kind,
                model.entities[q.related].id,
                cross_display(model, &q.related_conditions)
            );
            let text = InvText::Quantifier {
                anchor: model.entities[q.anchor].id.clone(),
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
                site,
            );
        }
    }

    // ── R-guard: condition bodies ────────────────────────────────────────────

    /// One guard per `holds` body applied to the use case and placed on this transaction group
    /// (see [`condition_group`]). An entity the action binds reads its param: on the pre-state,
    /// or in the successor (`At::Next`) when the action creates it, so the body constrains what
    /// is created. Any other entity, an entity of a sibling group included, is `Exists` over
    /// present instances of the pre-state, one witness per entity shared by the body's atoms,
    /// linked by a direct owner var (one hop) to each param or other witness it relates to; with
    /// no such relate it is independent.
    pub(super) fn condition_guards(
        &mut self,
        uc: UseCaseKey,
        params: &[Param],
        groups: &[Vec<EntityKey>],
        group: usize,
    ) -> Vec<Guard> {
        let model = self.model;
        let mut guards = Vec::new();
        for (ck, _, body) in applied_bodies(model)
            .into_iter()
            .filter(|(_, u, body)| *u == uc && condition_group(body, groups) == group)
        {
            let site = Site::new(ModelRef::Condition(ck), &body.site);
            let mut binding: Binding = params
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let at = match p.role {
                        ParamRole::New => At::Next(i as u8),
                        ParamRole::Existing | ParamRole::Parent => At::Param(i as u8),
                    };
                    (self.ts.entities[p.entity.index()].key, at)
                })
                .collect();
            let mut witnesses = Vec::new();
            for ek in body.conds.iter().flat_map(cond_entities) {
                if at_of(&binding, ek).is_none() {
                    binding.push((ek, At::Bound(witnesses.len() as u8)));
                    witnesses.push(self.ent_of[&ek]);
                }
            }
            let witness = |(_, at): (EntityKey, At)| matches!(at, At::Bound(_));
            let mut parts = Vec::new();
            for (i, &x) in binding.iter().enumerate() {
                for &y in &binding[i + 1..] {
                    if witness(x) || witness(y) {
                        parts.extend(self.owner_link(x, y));
                    }
                }
            }
            parts.extend(
                body.conds
                    .iter()
                    .map(|c| self.cross_atom(c, &binding, &site)),
            );
            let pred = witnesses
                .into_iter()
                .rev()
                .fold(Pred::And(parts), |inner, entity| Pred::Exists {
                    entity,
                    body: Box::new(inner),
                });
            guards.push(Guard {
                pred,
                why: GuardWhy::Condition { cond: ck },
                site,
            });
        }
        guards
    }

    /// A body applied to a use case that writes nothing has no action to guard.
    pub(super) fn inert_conditions(&mut self) {
        let model = self.model;
        for (ck, uc, body) in applied_bodies(model) {
            if self.active_ucs.contains(&uc) {
                continue;
            }
            self.diag(
                body.conds.iter().flat_map(cond_entities).collect(),
                StateDiag::UnbindableCondition {
                    condition: model.conditions[ck].id.clone(),
                    usecase: model.use_cases[uc].id.clone(),
                    reason: "the use case writes nothing, so no action carries the guard"
                        .to_string(),
                    site: Site::new(ModelRef::Condition(ck), &body.site),
                },
            );
        }
    }

    // ── R-after ──────────────────────────────────────────────────────────────

    /// One step per action of the anchor use case, so a split use case is checked after each
    /// transaction group. A scope entity the step does not bind is universally bound.
    pub(super) fn after_asserts(&mut self) {
        let model = self.model;
        for (i, ta) in model.temporal_assertions.iter().enumerate() {
            let site = Site::new(ModelRef::TemporalAssertion(i), &ta.site);
            let anchor = model.use_cases[ta.anchor].id.clone();
            let Some(acts) = self.uc_actions.get(&ta.anchor).cloned() else {
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
            let mut steps = Vec::new();
            for act in acts {
                let params = self.ts.actions[act.index()].params.clone();
                let mut binding: Binding = Vec::new();
                let mut binders = Vec::new();
                for &ek in &entities {
                    let e = self.ent_of[&ek];
                    match params.iter().position(|p| p.entity == e) {
                        Some(p) => binding.push((ek, At::Param(p as u8))),
                        None => {
                            binding.push((ek, At::Bound(binders.len() as u8)));
                            binders.push(e);
                        }
                    }
                }
                let post = self.cross_all(&ta.requireds, &binding, &site);
                steps.push(AfterStep { act, binders, post });
            }
            self.ts.after.push(AfterAssert {
                name: format!("AfterAssert_{anchor}_{i}"),
                anchor,
                steps,
                requireds: cross_display(model, &ta.requireds),
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
            self.ts.properties.push(Property {
                name: prop.id.clone(),
                binders: binding.iter().map(|(ek, _)| self.ent_of[ek]).collect(),
                formula,
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
                let op = cmp_op(atom.op);
                let what = format!("property {property}: {}", atom.column);
                match &atom.rhs {
                    TemporalRhs::Value(v) if matches!(op, CmpOp::Eq | CmpOp::Ne) => {
                        let eq = self.eq_atom(ek, at, &atom.column, v, site);
                        if op == CmpOp::Ne {
                            Pred::Not(Box::new(eq))
                        } else {
                            eq
                        }
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
                        match (self.slot(ek, &atom.column, at), self.slot(rk, column, rat)) {
                            (Some((l, _)), Some((r, _))) => Pred::Cmp(l, op, r),
                            _ => Pred::False,
                        }
                    }
                    TemporalRhs::Value(_) => Pred::False,
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
        for g in condition_guards.filter(|g| matches!(g.why, GuardWhy::Condition { .. })) {
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

/// The transaction group whose action carries a condition body: the first, in transaction
/// order, that writes an entity the body reads, else the first. A body is a precondition of the
/// use case, so it guards one step only, and an atom on a sibling group's entity reads that
/// entity's pre-state.
fn condition_group(body: &HoldsBody, groups: &[Vec<EntityKey>]) -> usize {
    let read: Vec<EntityKey> = body.conds.iter().flat_map(cond_entities).collect();
    groups
        .iter()
        .position(|g| g.iter().any(|ek| read.contains(ek)))
        .unwrap_or(0)
}

/// The entities a body atom reads, left operand first.
fn cond_entities(cond: &CrossEntityCondition) -> Vec<EntityKey> {
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
    }
}

pub(crate) fn effect_display(v: &EffectValue) -> String {
    match v {
        EffectValue::EnumVariant(s) => s.clone(),
        EffectValue::Bool(b) => b.to_string(),
        EffectValue::Int(n) => n.to_string(),
        EffectValue::Present | EffectValue::TypedPresent(_) => "present".to_string(),
        EffectValue::Null => "null".to_string(),
    }
}

fn local_display(eqs: &[(String, EffectValue)], cmps: &[ComparisonProp]) -> String {
    eqs.iter()
        .map(|(c, v)| format!("{c}={}", effect_display(v)))
        .chain(cmps.iter().map(|p| format!("{}=true", p.display())))
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn cross_rhs_display(model: &crate::model::SemanticModel, rhs: &CrossCmpRhs) -> String {
    match rhs {
        CrossCmpRhs::Column(c) => format!("{}.{}", model.entities[c.entity].id, c.column),
        CrossCmpRhs::IntLit(n) => n.to_string(),
        CrossCmpRhs::Now => "now".to_string(),
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
                effect_display(value)
            ),
            CrossEntityCondition::Comparison(p) => format!(
                "{}.{} {} {}=true",
                model.entities[p.lhs.entity].id,
                p.lhs.column,
                p.op.as_str(),
                cross_rhs_display(model, &p.rhs)
            ),
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn entity_list(model: &crate::model::SemanticModel, scope: &[EntityKey]) -> String {
    scope
        .iter()
        .map(|ek| model.entities[*ek].id.clone())
        .collect::<Vec<_>>()
        .join(", ")
}
