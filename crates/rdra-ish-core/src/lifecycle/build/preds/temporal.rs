//! `after` asserts and temporal properties as predicates, and the warning for a constrained var nothing writes.

use std::collections::HashSet;

use super::display::cross_display;
use super::{at_of, cmp_op, collect_vars, Binding};
use crate::lifecycle::build::naming::property_entity;
use crate::lifecycle::build::Builder;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{
    ActId, ActionOrigin, AfterAssert, AfterStep, At, CmpOp, Effect, Guard, GuardWhy, Ltl, ModelRef,
    Pred, Property, Site, Start, VarId, VarKind,
};
use crate::model::{
    CrossEntityCondition, EffectValue, EntityKey, TemporalExpr, TemporalFormula, TemporalRhs,
};

impl Builder<'_> {
    // ── R-after ──────────────────────────────────────────────────────────────

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

    // ── R-prop ───────────────────────────────────────────────────────────────

    pub(in crate::lifecycle::build) fn properties(&mut self) {
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

    pub(in crate::lifecycle::build) fn unwritten_vars(&mut self) {
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
