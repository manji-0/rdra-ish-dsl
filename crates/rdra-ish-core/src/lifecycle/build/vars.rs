//! The state vars of a system: one per entity column, one owner slot per foreign key, and the clock.

use std::collections::{HashMap, HashSet};

use super::{variant_index, Builder};
use crate::lifecycle::build::naming;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{
    Action, ActionOrigin, Birth, CmpOp, Domain, Effect, EntId, EntityDecl, Guard, GuardWhy,
    ModelRef, Pred, Site, Start, Term, Value, VarId, VarKind,
};
use crate::model::{ColumnType, EffectValue, EntityKey, Min, ModelColumn, SetValue, UseCaseKey};
use crate::tx::{infer_usecase_transactions, WriteKind};

impl Builder<'_> {
    pub(super) fn entities(&mut self) {
        let model = self.model;
        let mut creators: HashMap<EntityKey, HashSet<UseCaseKey>> = HashMap::new();
        for utx in infer_usecase_transactions(model) {
            for w in utx.writes().filter(|w| w.kind == WriteKind::Creates) {
                creators.entry(w.entity).or_default().insert(utx.usecase);
            }
        }
        let created: HashSet<EntityKey> = creators.keys().copied().collect();
        let mut keys: Vec<EntityKey> = model
            .entities
            .keys()
            .filter(|ek| !model.publication.is_published(*ek))
            .collect();
        keys.sort_by(|a, b| model.entities[*a].id.cmp(&model.entities[*b].id));
        for (i, &ek) in keys.iter().enumerate() {
            self.ent_of.insert(ek, EntId::at(i));
        }
        for &ek in &keys {
            let e = self.ent_of[&ek];
            let birth = if created.contains(&ek) {
                Birth::Created
            } else {
                Birth::PreExisting
            };
            let exists = self.push_var(
                VarKind::Exists(e),
                Domain::Existence,
                Value::ABSENT,
                Site::bare(ModelRef::Entity(ek)),
            );
            let mut columns = Vec::new();
            for col in &model.entities[ek].columns {
                if let Some(var) = self.column_var(e, ek, col, birth) {
                    columns.push(var);
                    self.variation_default(ek, col, var, creators.get(&ek));
                }
            }
            for key in naming::driven_props(model, ek) {
                let var = self.push_var(
                    VarKind::Prop {
                        entity: e,
                        key: key.clone(),
                    },
                    Domain::Bool,
                    Value::FALSE,
                    Site::bare(ModelRef::Entity(ek)),
                );
                self.prop_var.insert((e, key), var);
                columns.push(var);
            }
            if birth == Birth::PreExisting {
                self.diag(vec![ek], StateDiag::NoCreationPath);
            }
            let entity = &model.entities[ek];
            self.ts.entities.push(EntityDecl {
                key: ek,
                id: entity.id.clone(),
                label: entity.label.clone(),
                birth,
                exists,
                columns,
                owners: Vec::new(),
            });
        }
        self.owner_vars();
    }

    fn column_var(
        &mut self,
        e: EntId,
        ek: EntityKey,
        col: &ModelColumn,
        birth: Birth,
    ) -> Option<VarId> {
        let model = self.model;
        if model.column_fk(ek, &col.name).is_some() {
            // A relate's foreign key is the owner var, not a state column.
            return None;
        }
        if col.derived_expr.is_some() {
            // Its readers substitute the expression (or reject arithmetic); nothing writes it.
            return None;
        }
        let key = (ek, col.name.clone());
        let (domain, base, variation) = match &col.col_type {
            ColumnType::Enum(values) => {
                let base = col
                    .default_val
                    .as_deref()
                    .and_then(|d| variant_index(values, d))
                    .unwrap_or(0);
                (Domain::Enum(values.clone()), Value(base), false)
            }
            ColumnType::Bool => {
                let on = col
                    .default_val
                    .as_deref()
                    .is_some_and(|d| d.eq_ignore_ascii_case("true"));
                (
                    Domain::Bool,
                    if on { Value::TRUE } else { Value::FALSE },
                    false,
                )
            }
            ColumnType::Variation(vk) => {
                if !self.named.any.contains(&key) {
                    return None;
                }
                let values = model.variations.get(*vk)?.values.clone();
                if values.is_empty() {
                    return None;
                }
                let base = col
                    .default_val
                    .as_deref()
                    .and_then(|d| variant_index(&values, d))
                    .unwrap_or(0);
                (Domain::Enum(values), Value(base), true)
            }
            ColumnType::Int
            | ColumnType::Money
            | ColumnType::Decimal
            | ColumnType::Date
            | ColumnType::DateTime
                if self.named.numeric.contains(&key) =>
            {
                let domain = if col.is_nullable {
                    Domain::NullableInt
                } else {
                    Domain::Int
                };
                let base = col
                    .default_val
                    .as_deref()
                    .and_then(|d| d.parse::<i64>().ok())
                    .and_then(|n| domain.of_number(n, self.bounds))
                    .unwrap_or(if col.is_nullable {
                        Value::NULL
                    } else {
                        Value(0)
                    });
                (domain, base, false)
            }
            _ if col.is_nullable => {
                let present = col.default_val.as_deref().is_some_and(|d| d != "null");
                (
                    Domain::Nullable,
                    if present { Value::PRESENT } else { Value::NULL },
                    false,
                )
            }
            _ => return None,
        };
        let pg_type = model
            .column_effects
            .iter()
            .filter(|ce| ce.entity == ek && ce.column == col.name)
            .find_map(|ce| match &ce.value {
                SetValue::Value(EffectValue::TypedPresent(t)) => Some(t.clone()),
                _ => None,
            });
        let var = self.push_var(
            VarKind::Column {
                entity: e,
                column: col.name.clone(),
                lifecycle: model.is_lifecycle_column(ek, &col.name),
                variation,
                pg_type,
            },
            domain,
            base,
            Site::bare(ModelRef::Column {
                entity: ek,
                column: col.name.clone(),
            }),
        );
        if variation && birth == Birth::PreExisting {
            self.ts.vars[var.index()].start = Start::Any;
        }
        self.col_var.insert((e, col.name.clone()), var);
        Some(var)
    }

    /// `DefaultOnVariationIgnored` for a variation column with a `@default`, whose start is
    /// any value of the variation, unless every use case that creates the entity (`creators`)
    /// sets the column, so no row takes the start the `@default` names.
    fn variation_default(
        &mut self,
        ek: EntityKey,
        col: &ModelColumn,
        var: VarId,
        creators: Option<&HashSet<UseCaseKey>>,
    ) {
        let model = self.model;
        let (ColumnType::Variation(_), Some(value)) = (&col.col_type, &col.default_val) else {
            return;
        };
        let sets = |uc: &UseCaseKey| {
            model
                .column_effects
                .iter()
                .any(|ce| ce.origin == *uc && ce.entity == ek && ce.column == col.name)
        };
        if creators.is_some_and(|ucs| ucs.iter().all(sets)) {
            return;
        }
        let site = self.ts.vars[var.index()].site.clone();
        self.diag(
            vec![ek],
            StateDiag::DefaultOnVariationIgnored {
                column: col.name.clone(),
                value: value.clone(),
                created: creators.is_some(),
                site,
            },
        );
    }

    /// One owner var per foreign key. An `.optional()` key's owner starts and may stay
    /// unlinked (`null`). A pre-existing child starts linked: an N:1 child to any parent slot
    /// (or unlinked, when optional), a 1:1 child or the child of a `1..*` key to the parent
    /// slot of its own index. Under a
    /// created parent that slot is absent at the start, so an optional child starts unlinked
    /// and a mandatory one draws `PreExistingChildOfCreatedParent`.
    fn owner_vars(&mut self) {
        for (id, fk) in self.model.foreign_keys() {
            let (Some(&c), Some(&p)) = (self.ent_of.get(&fk.child), self.ent_of.get(&fk.parent))
            else {
                continue;
            };
            let child_pre = self.ts.entities[c.index()].birth == Birth::PreExisting;
            let parent_created = self.ts.entities[p.index()].birth == Birth::Created;
            let start = match (child_pre, parent_created && fk.optional()) {
                (false, _) | (true, true) => Start::Base,
                // Under a minimum of one child per parent, with as many child slots as
                // parent slots, every start that holds is a permutation of the identity.
                (true, false) if fk.unique() || fk.children.min == Min::One => Start::Identity,
                (true, false) => Start::Any,
            };
            if child_pre && parent_created && !fk.optional() {
                let model = self.model;
                self.diag(
                    vec![fk.child],
                    StateDiag::PreExistingChildOfCreatedParent {
                        parent: model.entities[fk.parent].id.clone(),
                        site: Site::new(ModelRef::Relation(fk.relation), &fk.site),
                    },
                );
            }
            let var = self.push_var(
                VarKind::Owner {
                    child: c,
                    parent: p,
                    fk: id,
                    role: fk.role.clone(),
                },
                Domain::Instance {
                    parent: p,
                    optional: fk.optional(),
                },
                Value(0),
                Site::new(ModelRef::Relation(fk.relation), &fk.site),
            );
            self.ts.vars[var.index()].start = start;
            self.ts.entities[c.index()].owners.push(var);
            self.owners.push((var, c, p));
        }
    }

    // ── R-now ────────────────────────────────────────────────────────────────

    pub(super) fn clock(&mut self) {
        if self.named.reads_now {
            let var = self.push_var(
                VarKind::Now,
                Domain::Int,
                Value(0),
                Site::bare(ModelRef::Synthesized),
            );
            self.ts.now = Some(var);
        }
    }

    pub(super) fn tick(&self) -> Option<Action> {
        self.ts.now?;
        Some(Action {
            name: "Tick".to_string(),
            source: crate::lifecycle::types::ActId::at(0),
            origin: ActionOrigin::Clock,
            site: Site::bare(ModelRef::Synthesized),
            params: Vec::new(),
            guards: vec![Guard {
                pred: Pred::Cmp(
                    Term::Now,
                    CmpOp::Lt,
                    Term::Int(i64::from(self.bounds.int_max)),
                ),
                why: GuardWhy::Clock,
                site: Site::bare(ModelRef::Synthesized),
            }],
            effects: vec![Effect::Tick],
        })
    }

    // ── Actions: R-create, R-delete, R-update, R-bind ────────────────────────
}
