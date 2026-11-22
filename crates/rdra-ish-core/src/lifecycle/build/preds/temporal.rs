//! `after` asserts and temporal properties as predicates, and the warning for a constrained var nothing writes.

use std::collections::HashSet;

use super::display::cross_display;
use super::{at_of, cmp_op, collect_vars, implies, Binding};
use crate::lifecycle::build::Builder;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{
    ActId, ActionOrigin, AfterAssert, AfterStep, At, CmpOp, Effect, Guard, GuardWhy, Ltl, ModelRef,
    Pred, Property, Site, Start, StepKind, StepRule, Term, Value, VarId, VarKind,
};
use crate::model::{
    CrossConstraintScope, CrossEntityCondition, EffectValue, EntityKey, TemporalExpr,
    TemporalFormula, TemporalProperty, TemporalRhs,
};

impl Builder<'_> {
    /// One step per last action of the anchor use case (one, or one per `<UC>_by_<Ev>`
    /// variant of a use case several events trigger), so a split use case is checked once
    /// per run, after its last transaction group, on the run's rows: an entity an earlier
    /// step wrote is the last step's `Earlier` param. An entity no param of the step binds is
    /// universally bound, and `UnboundAfterRead`.
    pub(in crate::lifecycle::build) fn after_asserts(&mut self) {
        let model = self.model;
        for (i, ta) in model.temporal_assertions.iter().enumerate() {
            let site = Site::new(ModelRef::TemporalAssertion(i), &ta.site);
            let anchor = model.use_cases[ta.anchor].id.clone();
            let Some(&last) = self.uc_actions.get(&ta.anchor).and_then(|acts| acts.last()) else {
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
            let step_of = |act: ActId| match self.ts.actions[act.index()].origin {
                ActionOrigin::UseCase { step, .. } => step,
                ActionOrigin::Clock => None,
            };
            let lasts: Vec<ActId> = self.uc_actions[&ta.anchor]
                .iter()
                .copied()
                .filter(|a| step_of(*a) == step_of(last))
                .collect();
            let mut steps = Vec::new();
            for act in lasts {
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
                steps.push(AfterStep { act, binders, post });
            }
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

    pub(in crate::lifecycle::build) fn properties(&mut self) {
        let model = self.model;
        for (i, prop) in model.temporal_properties.iter().enumerate() {
            let site = Site::new(ModelRef::TemporalProperty(i), &prop.site);
            if prop.formula.is_step() {
                self.step_rule(prop, site);
                continue;
            }
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
                TemporalFormula::Stable(_) | TemporalFormula::Unchanged { .. } => {
                    unreachable!("a step property is a step rule")
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
            let Some(scope) = self.property_scope(prop, &mut binding, &site) else {
                continue;
            };
            self.ts.properties.push(Property {
                name: prop.id.clone(),
                binders: binding.iter().map(|(ek, _)| self.ent_of[ek]).collect(),
                scope,
                formula,
                fair: prop.fair.clone(),
                site,
            });
        }
    }

    /// A `stable` or `unchanged` property as a step rule: on a step from a state where its
    /// binders are present, the scope and the trigger in the pre-state imply, for binders still
    /// present in the successor, the trigger again (`stable`) or each column at its old value
    /// (`unchanged`).
    fn step_rule(&mut self, prop: &TemporalProperty, site: Site) {
        let mut binding: Binding = Vec::new();
        let (kind, when, post) = match &prop.formula {
            TemporalFormula::Stable(e) => {
                let p = self.temporal(&prop.id, e, &mut binding, &site);
                let next = successor(&p);
                (StepKind::Stable, p, next)
            }
            TemporalFormula::Unchanged { columns, when } => {
                let w = match when {
                    Some(e) => self.temporal(&prop.id, e, &mut binding, &site),
                    None => Pred::True,
                };
                let mut same = Vec::new();
                for c in columns {
                    let at = Self::bind(&mut binding, c.entity);
                    if let Some((term, _)) = self.read_slot(c.entity, &c.column, at, &site) {
                        same.push(Pred::Cmp(successor_term(&term), CmpOp::Eq, term));
                    }
                }
                (StepKind::Unchanged, w, Pred::And(same))
            }
            _ => unreachable!("only `stable` and `unchanged` are step rules"),
        };
        let Some(scope) = self.property_scope(prop, &mut binding, &site) else {
            return;
        };
        let present_next: Vec<Pred> = binding
            .iter()
            .map(|(ek, at)| {
                let At::Bound(k) = at else {
                    unreachable!("a property binds every entity it reads")
                };
                let exists = self.ts.entities[self.ent_of[ek].index()].exists;
                Pred::Cmp(
                    Term::Slot {
                        var: exists,
                        at: At::NextBound(*k),
                    },
                    CmpOp::Eq,
                    Term::Lit(Value::PRESENT),
                )
            })
            .collect();
        let pre: Vec<Pred> = [scope, when]
            .into_iter()
            .filter(|p| *p != Pred::True)
            .collect();
        let pre = match pre.len() {
            0 => Pred::True,
            1 => pre.into_iter().next().expect("one part"),
            _ => Pred::And(pre),
        };
        let body = implies(pre, implies(Pred::And(present_next), post));
        self.ts.steps.push(StepRule {
            name: prop.id.clone(),
            binders: binding.iter().map(|(ek, _)| self.ent_of[ek]).collect(),
            body,
            kind,
            label: prop.label.clone(),
            site,
        });
    }

    /// The owner links of a property's `.along(..)` path, binding each path entity its atoms
    /// do not read; `Pred::True` without a path. `None` after `UnbindableScope`: the path does
    /// not cover an entity the atoms read, or no key joins a hop.
    fn property_scope(
        &mut self,
        prop: &TemporalProperty,
        binding: &mut Binding,
        site: &Site,
    ) -> Option<Pred> {
        let CrossConstraintScope::RelationPath { path, links, .. } = &prop.scope_semantics else {
            return Some(Pred::True);
        };
        let read: Vec<EntityKey> = binding.iter().map(|(ek, _)| *ek).collect();
        let constraint = format!("property {}", prop.id);
        if let Some(out) = read.iter().find(|ek| !path.contains(ek)) {
            let id = self.model.entities[*out].id.clone();
            let reason = format!("along(..) does not cover {id}");
            self.unbindable_scope(&read, &constraint, reason, site);
            return None;
        }
        // A path entity the atoms do not read joins the binders: bound universally under the
        // link, it is bound existentially, as in a multi-entity rule.
        for &ek in path {
            Self::bind(binding, ek);
        }
        match self.hop_links(path, links, binding) {
            Ok(links) => Some(Pred::And(links)),
            Err(reason) => {
                self.unbindable_scope(&read, &constraint, reason, site);
                None
            }
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
                let ek = atom.entity;
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
                    TemporalRhs::Column { entity: rk, column } => {
                        let rk = *rk;
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

    pub(in crate::lifecycle::build) fn unwritten_vars(&mut self) {
        let mut read: Vec<VarId> = Vec::new();
        for inv in &self.ts.invariants {
            collect_vars(&inv.body, &mut read);
        }
        for step in self.ts.after.iter().flat_map(|a| a.steps.iter()) {
            collect_vars(&step.post, &mut read);
        }
        for rule in &self.ts.steps {
            collect_vars(&rule.body, &mut read);
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

/// `term` read in the successor: a binder's slot becomes `At::NextBound`.
fn successor_term(term: &Term) -> Term {
    match term {
        Term::Slot {
            var,
            at: At::Bound(b),
        } => Term::Slot {
            var: *var,
            at: At::NextBound(*b),
        },
        t => t.clone(),
    }
}

/// `pred` read in the successor. A temporal atom has no quantifier, whose body would read the
/// rows of the pre-state.
fn successor(pred: &Pred) -> Pred {
    match pred {
        Pred::True | Pred::False => pred.clone(),
        Pred::Cmp(a, op, b) => Pred::Cmp(successor_term(a), *op, successor_term(b)),
        Pred::In(t, vs) => Pred::In(successor_term(t), vs.iter().map(successor_term).collect()),
        Pred::Not(p) => Pred::Not(Box::new(successor(p))),
        Pred::And(ps) => Pred::And(ps.iter().map(successor).collect()),
        Pred::Or(ps) => Pred::Or(ps.iter().map(successor).collect()),
        Pred::Forall { .. } | Pred::Exists { .. } => {
            unreachable!("a temporal atom has no quantifier")
        }
    }
}
