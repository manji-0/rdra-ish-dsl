//! Rules of one entity: its invariants, driven columns, min cards, unique keys, and cascade deletes.

use super::display::local_display;
use super::{implies, Binding};
use crate::lifecycle::build::Builder;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{
    Action, ActionOrigin, At, Birth, CmpOp, Effect, InvKind, InvText, Invariant, ModelRef,
    ParamRole, Pred, Rhs, Site, Term, Value, VarId, VarKind,
};
use crate::lifecycle::EntId;
use crate::model::{CmpRhs, ComparisonProp, EffectValue, EntityKey, Min, RefAction, UseCaseKey};

impl Builder<'_> {
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
    pub(super) fn push_invariant(
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

    pub(in crate::lifecycle::build) fn invariants(&mut self) {
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
}
