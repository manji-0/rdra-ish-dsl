//! `after` asserts and temporal properties as predicates, and the warning for a constrained var nothing writes.

use crate::lifecycle::narrow::id8;
use std::collections::HashSet;

use super::display::cross_display;
use super::{at_of, cmp_op, collect_vars, columns_cmp, Binding};
use crate::lifecycle::build::Builder;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{
    ActId, ActionOrigin, AfterAssert, AfterStep, At, CmpOp, Effect, Guard, GuardWhy, Ltl, ModelRef,
    Pred, Property, Reach, Site, Start, Term, Value, VarId, VarKind,
};
use crate::model::{
    CrossEntityCondition, EffectValue, EntityKey, TemporalExpr, TemporalFormula, TemporalRhs,
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
                    CrossEntityCondition::Mark { entity, .. } => *entity,
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
                            binding.push((ek, At::Bound(id8(binders.len()))));
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
            let constraint = format!("property {}", prop.id);
            let Some((mut binding, link)) =
                self.scope_binding(&prop.scope, &prop.scope_semantics, &constraint, &site)
            else {
                continue;
            };
            let formula = match &prop.formula {
                TemporalFormula::Always(e) => {
                    Ltl::Always(self.temporal(&prop.id, e, &mut binding, &site))
                }
                TemporalFormula::Eventually(e) => {
                    let goal = self.temporal(&prop.id, e, &mut binding, &site);
                    Ltl::Eventually(self.goal(goal, prop.present_goal, &binding))
                }
                TemporalFormula::LeadsTo {
                    antecedent,
                    consequent,
                } => {
                    let a = self.temporal(&prop.id, antecedent, &mut binding, &site);
                    let c = self.temporal(&prop.id, consequent, &mut binding, &site);
                    Ltl::LeadsTo(a, self.goal(c, prop.present_goal, &binding))
                }
                // `.fair` on it is an error, so there is no fairness to check.
                TemporalFormula::CanReach(e) => {
                    let goal = self.temporal(&prop.id, e, &mut binding, &site);
                    self.ts.reach.push(Reach {
                        name: prop.id.clone(),
                        binders: binding.iter().map(|(ek, _)| self.ent_of[ek]).collect(),
                        link,
                        goal,
                        site,
                    });
                    continue;
                }
                TemporalFormula::Stable(e) => {
                    Ltl::Stable(self.temporal(&prop.id, e, &mut binding, &site))
                }
                TemporalFormula::Unchanged { columns, when } => {
                    let mut terms = Vec::new();
                    for col in columns {
                        let at = Self::bind(&mut binding, col.entity);
                        // A column no var holds never changes.
                        if let Some((t, _)) = self.read_slot(col.entity, &col.column, at, &site) {
                            terms.push(t);
                        }
                    }
                    let when = match when {
                        Some(e) => self.temporal(&prop.id, e, &mut binding, &site),
                        None => Pred::True,
                    };
                    Ltl::Unchanged {
                        when,
                        columns: terms,
                    }
                }
                TemporalFormula::Precedes { first, then } => {
                    let p = self.temporal(&prop.id, first, &mut binding, &site);
                    let q = self.temporal(&prop.id, then, &mut binding, &site);
                    Ltl::Precedes(p, q)
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
                link,
                formula,
                fair: prop.fair.clone(),
                site,
            });
        }
    }

    /// `GoalMetByDelete` for each `eventually` / `leads_to` whose goal holds on the base values
    /// a deleted row of its binders reads, where some action deletes a binder's entity. A
    /// `.present()` goal reads presence, which no deleted row has, so it is never named. Reads
    /// the laid-out system.
    pub(in crate::lifecycle::build) fn goals_met_by_delete(&mut self) {
        let base = self.ts.base_state();
        let mut found = Vec::new();
        for p in &self.ts.properties {
            let (Ltl::Eventually(goal) | Ltl::LeadsTo(_, goal)) = &p.formula else {
                continue;
            };
            let deleted = p.binders.iter().copied().find(|&e| {
                self.ts.actions.iter().any(|a| {
                    a.effects.iter().any(|eff| {
                        matches!(eff, Effect::Delete { param } if a.params[usize::from(*param)].entity == e)
                    })
                })
            });
            let Some(e) = deleted else { continue };
            if !self.ts.holds_bound(goal, &base, &vec![0; p.binders.len()]) {
                continue;
            }
            let keys = p.binders.iter().map(|b| self.ts.entity(*b).key).collect();
            let diag = StateDiag::GoalMetByDelete {
                property: p.name.clone(),
                entity: self.ts.entity(e).id.clone(),
                site: p.site.clone(),
            };
            found.push((keys, diag));
        }
        for (keys, diag) in found {
            self.diag(keys, diag);
        }
    }

    /// The goal of `eventually` / `leads_to`; with `.present()`, conjoined with the presence of
    /// every bound row, the `present` the trigger reads. `binding` is the whole binding, read
    /// after every operand is lowered. The presence vars are in the property's cone already,
    /// as the trigger reads them, so the cone, its reduction, and the symmetry scan see the
    /// same vars as without `.present()`.
    fn goal(&self, goal: Pred, present: bool, binding: &Binding) -> Pred {
        if !present {
            return goal;
        }
        let mut parts: Vec<Pred> = binding
            .iter()
            .map(|&(ek, at)| {
                let exists = self.ts.entities[self.ent_of[&ek].index()].exists;
                Pred::Cmp(
                    Term::Slot { var: exists, at },
                    CmpOp::Eq,
                    Term::Lit(Value::PRESENT),
                )
            })
            .collect();
        parts.push(goal);
        Pred::And(parts)
    }

    fn bind(binding: &mut Binding, ek: EntityKey) -> At {
        at_of(binding, ek).unwrap_or_else(|| {
            let at = At::Bound(id8(binding.len()));
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
                            (Some(l), Some(r)) => columns_cmp(l, op, r),
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
