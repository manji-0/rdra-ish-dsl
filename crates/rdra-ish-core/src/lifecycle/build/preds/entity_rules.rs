//! Rules of one entity: its invariants, driven columns, min cards, unique keys and column
//! sets, and cascade deletes.

use super::display::{local_display, local_displays};
use super::{forbidden_premises, implies, premise, Binding};
use crate::lifecycle::build::Builder;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{
    Action, ActionOrigin, At, Birth, CmpOp, Domain, Effect, InvText, Invariant, ModelRef,
    ParamRole, Pred, Premise, Rhs, Site, Term, Value, VarId, VarKind,
};
use crate::lifecycle::EntId;
use crate::model::{CmpRhs, EntityCondition, EntityKey, Min, RefAction, UniqueSource, UseCaseKey};

impl Builder<'_> {
    fn local_binding(ek: EntityKey) -> Binding {
        vec![(ek, At::Bound(0))]
    }

    fn local_conds(&mut self, ek: EntityKey, conds: &[EntityCondition], site: &Site) -> Vec<Pred> {
        let mut out: Vec<Pred> = EntityCondition::equals(conds)
            .map(|(c, v)| self.eq_atom(ek, At::Bound(0), c, v, site))
            .collect();
        out.extend(
            EntityCondition::comparisons(conds).map(|p| self.cmp_atom(ek, At::Bound(0), p, site)),
        );
        out
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn push_invariant(
        &mut self,
        prefix: &str,
        idx: usize,
        binding: &Binding,
        (body, premises): (Pred, Vec<Premise>),
        text: InvText,
        label: Option<String>,
        site: Site,
    ) {
        let entity = self.model.entities[binding[0].0].id.clone();
        self.ts.invariants.push(Invariant {
            name: format!("{prefix}_{entity}_{idx}"),
            binders: binding.iter().map(|(ek, _)| self.ent_of[ek]).collect(),
            body,
            premises,
            text,
            label,
            site,
        });
    }

    /// The overflow rule of each counted mark, after the model's own rules.
    pub(in crate::lifecycle::build) fn overflow_rules(&mut self) {
        let overflows = std::mem::take(&mut self.overflows);
        self.ts.invariants.extend(overflows);
    }

    pub(in crate::lifecycle::build) fn invariants(&mut self) {
        let model = self.model;
        for (i, c) in model.forbidden_constraints.iter().enumerate() {
            let site = Site::new(ModelRef::Forbidden(i), &c.site);
            let conds = self.local_conds(c.entity, &c.conditions, &site);
            let text = InvText::Forbidden(local_display(&c.conditions));
            let b = Self::local_binding(c.entity);
            let premises = forbidden_premises(&Pred::True, &conds, &local_displays(&c.conditions));
            let body = Pred::Not(Box::new(Pred::And(conds)));
            let label = c.label.clone();
            self.push_invariant("Forbidden", i, &b, (body, premises), text, label, site);
        }
        for (i, c) in model.entity_invariants.iter().enumerate() {
            let site = Site::new(ModelRef::EntityInvariant(i), &c.site);
            let guards = self.local_conds(c.entity, &c.guards, &site);
            let reqs = self.local_conds(c.entity, &c.requireds, &site);
            let text = InvText::Invariant {
                guards: local_display(&c.guards),
                requireds: local_display(&c.requireds),
            };
            let b = Self::local_binding(c.entity);
            let premises = vec![premise(guards.clone(), local_display(&c.guards))];
            let body = implies(Pred::And(guards), Pred::And(reqs));
            let label = c.label.clone();
            self.push_invariant("Inv", i, &b, (body, premises), text, label, site);
        }
        for (i, c) in model.required_constraints.iter().enumerate() {
            let site = Site::new(ModelRef::Required(i), &c.site);
            let conds = self.local_conds(c.entity, &c.conditions, &site);
            let text = InvText::Required(local_display(&c.conditions));
            let b = Self::local_binding(c.entity);
            self.push_invariant(
                "Required",
                i,
                &b,
                (Pred::And(conds), vec![Premise::rows()]),
                text,
                c.label.clone(),
                site,
            );
        }
        for (i, c) in model.exclusive_constraints.iter().enumerate() {
            let site = Site::new(ModelRef::Exclusive(i), &c.site);
            let conds = self.local_conds(c.entity, &c.conditions, &site);
            let mut pairs = Vec::new();
            for x in 0..conds.len() {
                for y in x + 1..conds.len() {
                    pairs.push(Pred::And(vec![conds[x].clone(), conds[y].clone()]));
                }
            }
            let text = InvText::Exclusive(local_display(&c.conditions));
            let b = Self::local_binding(c.entity);
            let body = Pred::Not(Box::new(Pred::Or(pairs)));
            let label = c.label.clone();
            let premises = vec![Premise::rows()];
            self.push_invariant("Exclusive", i, &b, (body, premises), text, label, site);
        }
        self.column_checks();
        self.cross_invariants();
        self.quantifiers();
        self.unique_keys();
        self.unique_columns();
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
                binders: vec![self.ent_of[&fk.parent]],
                premises: vec![Premise::rows()],
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

    /// Two present rows of one entity that agree on every var of `vars` break it. A row whose
    /// var of a null-capable domain is null agrees with none, as SQL's NULLs are distinct.
    /// Two rows exist only above one instance per entity, which the bounds say, so at one the
    /// rule has no premise to miss.
    fn push_unique(&mut self, name: String, e: EntId, vars: &[VarId], text: InvText, site: Site) {
        let index = |b| Term::Index(At::Bound(b));
        let slot = |var, b| Term::Slot {
            var,
            at: At::Bound(b),
        };
        let mut apart = vec![Pred::Cmp(index(0), CmpOp::Eq, index(1))];
        apart.extend(
            vars.iter()
                .map(|&v| Pred::Cmp(slot(v, 0), CmpOp::Ne, slot(v, 1))),
        );
        apart.extend(
            vars.iter()
                .filter(|v| {
                    matches!(
                        self.ts.vars[v.index()].domain,
                        Domain::NullableInt | Domain::Instance { optional: true, .. }
                    )
                })
                .map(|&v| Pred::Cmp(slot(v, 0), CmpOp::Eq, Term::Lit(Value::NULL))),
        );
        let premises = match self.ts.bounds.slots() {
            1 => Vec::new(),
            _ => vec![premise(
                vec![Pred::Cmp(index(0), CmpOp::Ne, index(1))],
                String::new(),
            )],
        };
        self.ts.invariants.push(Invariant {
            name,
            binders: vec![e, e],
            premises,
            body: Pred::Or(apart),
            text,
            label: None,
            site,
        });
    }

    /// `Unique_{child}_{role}` per 1:1 key: two distinct present children never share a
    /// parent slot. An unlinked optional owner shares nothing.
    fn unique_keys(&mut self) {
        let model = self.model;
        for (id, fk) in model.foreign_keys().filter(|(_, fk)| fk.unique()) {
            let Some(owner) = self.owner_var(id) else {
                continue;
            };
            let (child, parent) = (
                model.entities[fk.child].id.clone(),
                model.entities[fk.parent].id.clone(),
            );
            let role = fk.role_or(&parent).to_string();
            let name = format!("Unique_{child}_{role}");
            let text = InvText::Unique {
                child,
                parent,
                role,
            };
            let site = Site::new(ModelRef::Relation(fk.relation), &fk.site);
            self.push_unique(name, self.ent_of[&fk.child], &[owner], text, site);
        }
    }

    /// `Unique_{entity}_{columns}` per unique column set whose every column the system holds
    /// by value: a single `@unique` column, a `@unique(a, b)`, and an N:M junction's pair of
    /// keys. A key column reads as its key's owner, so a set must hold every column of the
    /// key. A set with a column held as null or present only, or with none, is the database's
    /// to check; so is a declared set on an entity with a `@soft_delete` column, whose index
    /// covers only live rows; and a set that is a 1:1 key already has `Unique_{child}_{role}`.
    fn unique_columns(&mut self) {
        let model = self.model;
        let mut keys: Vec<(EntityKey, EntId)> = self.ent_of.iter().map(|(&k, &e)| (k, e)).collect();
        keys.sort_by_key(|&(_, e)| e);
        for (ek, e) in keys {
            let entity = &model.entities[ek];
            let soft_delete = entity.columns.iter().any(|c| c.is_soft_delete);
            let singles = entity
                .columns
                .iter()
                .filter(|c| c.is_unique && entity.primary_key != [c.name.clone()])
                .map(|c| (vec![c.name.clone()], UniqueSource::Declared));
            let sets = singles.chain(
                model
                    .unique_keys(ek)
                    .into_iter()
                    .map(|u| (u.columns, u.source)),
            );
            for (columns, source) in sets.collect::<Vec<_>>() {
                let site = match source {
                    UniqueSource::OneToOne(_) => continue,
                    UniqueSource::Declared if soft_delete => continue,
                    UniqueSource::Declared => Site::bare(ModelRef::Column {
                        entity: ek,
                        column: columns[0].clone(),
                    }),
                    UniqueSource::Junction(a, _) => {
                        let fk = model.foreign_key(a);
                        Site::new(ModelRef::Relation(fk.relation), &fk.site)
                    }
                };
                let Some(vars) = self.unique_set_vars(ek, e, &columns) else {
                    continue;
                };
                let one_to_one = |&v: &VarId| {
                    matches!(self.ts.vars[v.index()].kind,
                        VarKind::Owner { fk, .. } if model.foreign_key(fk).unique())
                };
                let name = format!("Unique_{}_{}", entity.id, columns.join("_"));
                if matches!(vars.as_slice(), [v] if one_to_one(v))
                    || self.ts.invariants.iter().any(|i| i.name == name)
                {
                    continue;
                }
                let text = InvText::UniqueSet {
                    entity: entity.id.clone(),
                    columns: columns.join(", "),
                };
                self.push_unique(name, e, &vars, text, site);
            }
        }
    }

    /// The vars that hold the values of `columns`, each once, or `None` when one is not held
    /// by value.
    fn unique_set_vars(&self, ek: EntityKey, e: EntId, columns: &[String]) -> Option<Vec<VarId>> {
        let mut vars = Vec::new();
        for column in columns {
            let var = match self.col_var.get(&(e, column.clone())) {
                Some(&v) if self.ts.vars[v.index()].domain == Domain::Nullable => return None,
                Some(&v) => v,
                None => {
                    let (id, fk) = self.model.column_fk(ek, column)?;
                    if !fk.columns.iter().all(|(c, _)| columns.contains(c)) {
                        return None;
                    }
                    self.owner_var(id)?
                }
            };
            if !vars.contains(&var) {
                vars.push(var);
            }
        }
        Some(vars)
    }
}
