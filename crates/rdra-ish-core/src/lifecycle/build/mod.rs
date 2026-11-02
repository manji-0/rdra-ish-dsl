//! `SemanticModel` -> `TransitionSystem`. The only reader of the model's lifecycle facts:
//! columns, `transitions`, `sets`, CRUD edges, relates, constraints, asserts, and properties.

mod naming;
mod preds;

use std::collections::{HashMap, HashSet};

use super::diag::{Diag, StateDiag};
use super::types::{
    Action, ActionOrigin, At, Birth, Bounds, CmpOp, Domain, Effect, EntId, EntityDecl, Guard,
    GuardWhy, ModelRef, Param, ParamRole, Pred, Rhs, Site, Start, Term, Value, VarDecl, VarId,
    VarKind,
};
use super::{Built, Scope, TransitionSystem};
use crate::analysis::applied_bodies;
use crate::model::{
    CmpOpModel, CmpRhs, ColumnType, ConditionKey, EffectValue, EntityKey, FkId, Join, Min,
    ModelColumn, NodeRef, RelKind, SemanticModel, SetValue, UseCaseKey,
};
use crate::resolver::reachable_from_bucs;
use crate::tx::{infer_usecase_transactions, UcWrite, WriteKind};
use naming::Named;

pub(super) fn build(model: &SemanticModel, bounds: Bounds, scope: &Scope) -> Built {
    let mut b = Builder::new(model, bounds);
    b.entities();
    b.clock();
    b.actions(scope);
    b.invariants();
    b.after_asserts();
    b.properties();
    b.unwritten_vars();
    b.finish(scope)
}

struct Builder<'m> {
    model: &'m SemanticModel,
    bounds: Bounds,
    named: Named,
    ts: TransitionSystem,
    ent_of: HashMap<EntityKey, EntId>,
    col_var: HashMap<(EntId, String), VarId>,
    prop_var: HashMap<(EntId, String), VarId>,
    /// (owner var, child, parent)
    owners: Vec<(VarId, EntId, EntId)>,
    /// Use cases whose actions exist, each with its actions in transaction order.
    uc_actions: HashMap<UseCaseKey, Vec<super::types::ActId>>,
    /// Use cases with any write in the full model (before the scope filter).
    active_ucs: HashSet<UseCaseKey>,
    diags: Vec<Diag>,
    /// Parallel to `diags`: the entities of the actions each diagnostic altered.
    altered: Vec<Vec<EntityKey>>,
    /// The entities of the use case action being built, if any.
    acting: Vec<EntityKey>,
}

impl<'m> Builder<'m> {
    fn new(model: &'m SemanticModel, bounds: Bounds) -> Self {
        Builder {
            model,
            bounds,
            named: Named::collect(model),
            ts: TransitionSystem::empty(bounds),
            ent_of: HashMap::new(),
            col_var: HashMap::new(),
            prop_var: HashMap::new(),
            owners: Vec::new(),
            uc_actions: HashMap::new(),
            active_ucs: HashSet::new(),
            diags: Vec::new(),
            altered: Vec::new(),
            acting: Vec::new(),
        }
    }

    fn finish(mut self, scope: &Scope) -> Built {
        self.ts.layout();
        let model = self.model;
        let mut inactive: Vec<UseCaseKey> = self
            .active_ucs
            .iter()
            .copied()
            .filter(|uc| !self.uc_actions.contains_key(uc))
            .collect();
        inactive.sort_by(|a, b| model.use_cases[*a].id.cmp(&model.use_cases[*b].id));
        let diags = self
            .diags
            .into_iter()
            .zip(self.altered)
            .map(|(d, mut involves)| {
                for k in &d.entities {
                    if !involves.contains(k) {
                        involves.push(*k);
                    }
                }
                (d, involves)
            })
            .collect();
        let (diags, selection) = self.ts.select(scope, diags, inactive);
        Built {
            ts: self.ts,
            diags,
            selection,
        }
    }

    /// Records a diagnostic once. One raised while a use case action is built may have
    /// dropped or changed a guard or an effect of it, so it also involves every entity that
    /// action binds: a selection that reads the action does not defer it.
    fn diag(&mut self, entities: Vec<EntityKey>, diag: StateDiag) {
        let d = Diag { entities, diag };
        let at = match self.diags.iter().position(|x| *x == d) {
            Some(at) => at,
            None => {
                self.diags.push(d);
                self.altered.push(Vec::new());
                self.diags.len() - 1
            }
        };
        for k in &self.acting {
            if !self.altered[at].contains(k) {
                self.altered[at].push(*k);
            }
        }
    }

    fn push_var(&mut self, kind: VarKind, domain: Domain, base: Value, site: Site) -> VarId {
        let id = VarId::at(self.ts.vars.len());
        self.ts.vars.push(VarDecl {
            kind,
            domain,
            base,
            start: Start::Base,
            site,
        });
        id
    }

    // ── R-var and R-start ────────────────────────────────────────────────────

    fn entities(&mut self) {
        let model = self.model;
        let created: HashSet<EntityKey> = infer_usecase_transactions(model)
            .iter()
            .flat_map(|utx| utx.writes())
            .filter(|w| w.kind == WriteKind::Creates)
            .map(|w| w.entity)
            .collect();
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

    fn clock(&mut self) {
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

    fn tick(&self) -> Option<Action> {
        self.ts.now?;
        Some(Action {
            name: "Tick".to_string(),
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

    fn actions(&mut self, scope: &Scope) {
        let model = self.model;
        let in_scope: Option<HashSet<NodeRef>> =
            (!scope.bucs.is_empty()).then(|| reachable_from_bucs(model, &scope.bucs));
        let mut txs = infer_usecase_transactions(model);
        self.active_ucs = txs.iter().map(|utx| utx.usecase).collect();
        // Transitions first in declaration order, then the rest by use case id.
        let first_transition = |uc: UseCaseKey| {
            model
                .state_transitions
                .iter()
                .position(|t| t.usecase == uc)
                .unwrap_or(usize::MAX)
        };
        txs.sort_by_key(|utx| {
            (
                first_transition(utx.usecase),
                model.use_cases[utx.usecase].id.clone(),
            )
        });
        for utx in &txs {
            let uc = utx.usecase;
            if in_scope
                .as_ref()
                .is_some_and(|r| !r.contains(&NodeRef::UseCase(uc)))
            {
                continue;
            }
            let groups: Vec<Vec<UcWrite>> = utx
                .groups()
                .map(|g| {
                    g.ordered_writes
                        .iter()
                        .filter(|w| self.ent_of.contains_key(&w.entity))
                        .cloned()
                        .collect()
                })
                .filter(|writes: &Vec<UcWrite>| !writes.is_empty())
                .collect();
            let entities: Vec<Vec<EntityKey>> = groups
                .iter()
                .map(|writes| writes.iter().map(|w| w.entity).collect())
                .collect();
            let id = &model.use_cases[uc].id;
            for (g, writes) in groups.iter().enumerate() {
                let name = match entities.len() {
                    1 => id.clone(),
                    _ => format!("{id}_{}", g + 1),
                };
                self.acting = entities[g].clone();
                let action = self.usecase_action(uc, name, writes, &entities);
                self.acting.clear();
                self.uc_actions
                    .entry(uc)
                    .or_default()
                    .push(super::types::ActId::at(self.ts.actions.len()));
                self.ts.actions.push(action);
            }
        }
        if let Some(tick) = self.tick() {
            self.ts.actions.push(tick);
        }
        self.inert_conditions();
    }

    /// One atomic action for one transaction group of the use case: every effect on every
    /// entity of the group in one step. `groups` lists every group's entities, in transaction
    /// order.
    fn usecase_action(
        &mut self,
        uc: UseCaseKey,
        name: String,
        writes: &[UcWrite],
        groups: &[Vec<EntityKey>],
    ) -> Action {
        let model = self.model;
        let mut a = ActionDraft::default();
        let mut sorted: Vec<(EntId, WriteKind, Option<crate::model::ApiKey>)> = writes
            .iter()
            .map(|w| (self.ent_of[&w.entity], w.kind.clone(), w.via_api))
            .collect();
        sorted.sort_by_key(|(e, _, _)| *e);
        for (e, kind, via_api) in sorted {
            let role = match kind {
                WriteKind::Creates => ParamRole::New,
                WriteKind::Updates | WriteKind::Deletes => ParamRole::Existing,
            };
            let p = a.param(e, role);
            if let (ParamRole::New, Some(api)) = (role, via_api) {
                a.params[usize::from(p)].via_api = Some(api);
                self.ts
                    .api_ids
                    .entry(api)
                    .or_insert_with(|| model.apis[api].id.clone());
            }
            match kind {
                WriteKind::Creates => a.effects.push(Effect::Create { param: p }),
                WriteKind::Deletes => a.effects.push(Effect::Delete { param: p }),
                WriteKind::Updates => {}
            }
        }
        self.sets_effects(uc, &mut a);
        self.chosen_columns(uc, &mut a);
        self.transition_effects(uc, &mut a);
        self.creation_choices(&mut a);
        self.owner_bindings(uc, &mut a);
        self.copy_effects(uc, &mut a, groups);
        let guards = self.condition_guards(uc, &a.params);
        a.guards.extend(guards);
        self.arm_guards(&mut a);
        Action {
            name,
            origin: ActionOrigin::UseCase {
                uc,
                buc: first_buc(model, uc),
            },
            site: Site::new(
                ModelRef::UseCase(uc),
                &model.decl_sites.located("usecase", &model.use_cases[uc].id),
            ),
            params: a.params,
            guards: a.guards,
            effects: a.effects,
        }
    }

    fn sets_effects(&mut self, uc: UseCaseKey, a: &mut ActionDraft) {
        let model = self.model;
        for (idx, ce) in model.column_effects.iter().enumerate() {
            if ce.origin != uc {
                continue;
            }
            let SetValue::Value(value) = &ce.value else {
                continue;
            };
            let Some(&e) = self.ent_of.get(&ce.entity) else {
                continue;
            };
            let Some(p) = a.param_of(e) else { continue };
            let Some(&var) = self.col_var.get(&(e, ce.column.clone())) else {
                continue;
            };
            let site = Site::new(ModelRef::ColumnEffect(idx), &ce.site);
            let decl = &self.ts.vars[var.index()];
            let lifecycle = matches!(
                decl.kind,
                VarKind::Column {
                    lifecycle: true,
                    ..
                }
            );
            if lifecycle && a.params[usize::from(p)].role != ParamRole::New {
                self.diag(
                    vec![ce.entity],
                    StateDiag::DoubleModeledEnum {
                        column: ce.column.clone(),
                        usecase: model.use_cases[uc].id.clone(),
                        site,
                    },
                );
                continue;
            }
            let domain = decl.domain.clone();
            match self.effect_value(&domain, value, &site, ce.entity) {
                EffectRhs::Value(v) => self.assign(a, uc, ce.entity, var, p, Rhs::Lit(v), site),
                EffectRhs::AnyNonNull => {
                    let values: Vec<Value> = (1..domain.size(self.bounds))
                        .map(|i| Value(i as u8))
                        .collect();
                    a.effects.push(Effect::Choose {
                        var,
                        param: p,
                        sites: vec![site; values.len()],
                        conds: Vec::new(),
                        values,
                    });
                }
                EffectRhs::None => {}
            }
        }
        for (idx, pe) in model.proposition_effects.iter().enumerate() {
            if pe.origin != uc {
                continue;
            }
            let Some(&e) = self.ent_of.get(&pe.entity) else {
                continue;
            };
            let (Some(p), Some(&var)) =
                (a.param_of(e), self.prop_var.get(&(e, pe.prop.axis_key())))
            else {
                continue;
            };
            let v = if pe.truth { Value::TRUE } else { Value::FALSE };
            let site = Site::new(ModelRef::PropositionEffect(idx), &pe.site);
            self.check_driven_truth(uc, idx, &site);
            self.assign(a, uc, pe.entity, var, p, Rhs::Lit(v), site);
        }
    }

    /// `sets(UC, E, column == F.source)`: the step leaves in `column` the value `F.source` holds
    /// in the row of `F` it binds, read as a condition atom on `F` reads it. The copy is a
    /// choice of the column's value that a successor guard pins to the source, so the explorer
    /// and the TLA+ printer read it as they read an arm guard. It runs after the owner
    /// bindings, so the parent a created child takes is already a param.
    fn copy_effects(&mut self, uc: UseCaseKey, a: &mut ActionDraft, groups: &[Vec<EntityKey>]) {
        let model = self.model;
        for (idx, ce) in model.column_effects.iter().enumerate() {
            let SetValue::Copy(source) = &ce.value else {
                continue;
            };
            if ce.origin != uc {
                continue;
            }
            let Some(&e) = self.ent_of.get(&ce.entity) else {
                continue;
            };
            let (Some(p), Some(&var)) = (a.param_of(e), self.col_var.get(&(e, ce.column.clone())))
            else {
                continue;
            };
            let site = Site::new(ModelRef::ColumnEffect(idx), &ce.site);
            // A state model starts at the values its creations name, so a lifecycle column
            // takes no copied value, on a creation either.
            let lifecycle = matches!(
                self.ts.vars[var.index()].kind,
                VarKind::Column {
                    lifecycle: true,
                    ..
                }
            );
            if lifecycle {
                self.diag(
                    vec![ce.entity],
                    StateDiag::DoubleModeledEnum {
                        column: ce.column.clone(),
                        usecase: model.use_cases[uc].id.clone(),
                        site,
                    },
                );
                continue;
            }
            let source_at = match self.copy_source(a, p, source.entity, &ce.by, groups) {
                Ok(at) => at,
                Err(CopyMiss::Ambiguous(candidates)) => {
                    let statement = format!(
                        "sets({}, {}, {} == {}.{})",
                        model.use_cases[uc].id,
                        model.entities[ce.entity].id,
                        ce.column,
                        model.entities[source.entity].id,
                        source.column
                    );
                    let (a, b) = (ce.entity, source.entity);
                    let diag =
                        preds::ambiguous_owner_link(model, &statement, a, b, &candidates, &site);
                    self.diag(vec![ce.entity], diag);
                    continue;
                }
                Err(CopyMiss::Unbound(reason)) => {
                    let source_id = &model.entities[source.entity].id;
                    self.diag(
                        vec![ce.entity],
                        StateDiag::UnboundCopySource {
                            usecase: model.use_cases[uc].id.clone(),
                            column: format!("{}.{}", model.entities[ce.entity].id, ce.column),
                            source: format!("{source_id}.{}", source.column),
                            reason,
                            site,
                        },
                    );
                    continue;
                }
            };
            let Some(&source_var) = self
                .ent_of
                .get(&source.entity)
                .and_then(|s| self.col_var.get(&(*s, source.column.clone())))
            else {
                continue;
            };
            let copy = Pred::Cmp(
                Term::Slot {
                    var,
                    at: At::Next(p),
                },
                CmpOp::Eq,
                Term::Slot {
                    var: source_var,
                    at: source_at,
                },
            );
            if a.guards.iter().any(|g| g.pred == copy) {
                continue;
            }
            if let Some(old) = prior_write(a, var, p) {
                let column = self.ts.var_name(var);
                self.diag(
                    vec![ce.entity],
                    StateDiag::ConflictingEffects {
                        usecase: model.use_cases[uc].id.clone(),
                        column,
                        sites: vec![old, site],
                    },
                );
                continue;
            }
            let size = self.ts.vars[var.index()].domain.size(self.bounds);
            a.effects.push(Effect::Choose {
                var,
                param: p,
                values: (0..size).map(|i| Value(i as u8)).collect(),
                sites: vec![site.clone(); size],
                conds: Vec::new(),
            });
            a.guards.push(Guard {
                pred: copy,
                why: GuardWhy::Copy,
                site,
            });
        }
    }

    /// The row of `source` a copy into param `p` reads, by the rule a condition atom follows:
    /// a param of the action (on the pre-state, or in the successor when the action creates
    /// it), else the parent `p` belongs to through the key [`SemanticModel::join_key`] picks
    /// (with the copy's `.by`), which joins the action as a `Parent` param that `p`'s owner
    /// names. Any other row is not bound to the step.
    fn copy_source(
        &mut self,
        a: &mut ActionDraft,
        p: u8,
        source: EntityKey,
        by: &[String],
        groups: &[Vec<EntityKey>],
    ) -> Result<At, CopyMiss> {
        let model = self.model;
        let Some(&s) = self.ent_of.get(&source) else {
            return Err(CopyMiss::Unbound(
                "it is published and has no state".to_string(),
            ));
        };
        if let Some(q) = self.read_param(&a.params, s, by) {
            return Ok(match a.params[usize::from(q)].role {
                ParamRole::New => At::Next(q),
                ParamRole::Existing | ParamRole::Parent { .. } => At::Param(q),
            });
        }
        let target = a.params[usize::from(p)].entity;
        let target_id = &self.ts.entities[target.index()].id;
        let source_id = &model.entities[source].id;
        let target_ek = self.ts.entities[target.index()].key;
        let key = match model.join_key(target_ek, source, by) {
            Join::Ambiguous(candidates) => return Err(CopyMiss::Ambiguous(candidates)),
            Join::Key(fk) => Some(fk),
            Join::Unrelated => None,
        };
        let owner = key
            .filter(|fk| model.foreign_key(*fk).child == target_ek)
            .and_then(|fk| Some((fk, self.owner_var(fk)?)));
        if let Some((fk, var)) = owner {
            if a.params[usize::from(p)].role == ParamRole::New {
                return Err(CopyMiss::Unbound(format!(
                    "the {target_id} it creates is not linked to a {source_id}: the relate is optional and the use case does not write {source_id}"
                )));
            }
            let q = a.param(s, ParamRole::Parent { key: fk });
            let site = self.ts.vars[var.index()].site.clone();
            a.guards.push(Guard {
                pred: Pred::Cmp(
                    Term::Slot {
                        var,
                        at: At::Param(p),
                    },
                    CmpOp::Eq,
                    Term::Index(At::Param(q)),
                ),
                why: GuardWhy::Owner,
                site,
            });
            return Ok(At::Param(q));
        }
        let child = key.is_some();
        let elsewhere = groups.iter().flatten().any(|ek| *ek == source);
        Err(CopyMiss::Unbound(if elsewhere {
            format!("the use case writes {source_id} in another transaction group, a step of its own; relate {target_id} to {source_id} so one step writes both")
        } else if child {
            format!("several {source_id} rows may belong to one {target_id}; copy from the row the {target_id} belongs to, or from one the use case writes")
        } else {
            format!("the use case does not write {source_id}, and no relate makes it the parent of {target_id}")
        }))
    }

    /// The owner var of key `fk`, if both its entities have state.
    fn owner_var(&self, fk: FkId) -> Option<VarId> {
        self.owners.iter().map(|(var, ..)| *var).find(|var| {
            matches!(self.ts.vars[var.index()].kind, VarKind::Owner { fk: id, .. } if id == fk)
        })
    }

    /// The param a statement with selector `by` reads for a row of entity `e`: the row of `e`
    /// the action writes, else the `Parent` param of the key the statement's join follows
    /// ([`SemanticModel::join_key`] from that key's child). One `Parent` param of `e` whose
    /// join no `.by` names is the row the action reads. Otherwise, for a key with no `Parent`
    /// param or for several such params and no `.by` naming one, `None`: the statement joins
    /// its own row of `e`.
    fn read_param(&self, params: &[Param], e: EntId, by: &[String]) -> Option<u8> {
        if let Some(p) = params
            .iter()
            .position(|p| p.entity == e && p.role.is_written())
        {
            return Some(p as u8);
        }
        let model = self.model;
        let ek = self.ts.entities[e.index()].key;
        let parents: Vec<(u8, FkId)> = params
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p.role {
                ParamRole::Parent { key } if p.entity == e => Some((i as u8, key)),
                _ => None,
            })
            .collect();
        let join = |fk: FkId| model.join_key(model.foreign_key(fk).child, ek, by);
        let named: Vec<u8> = parents
            .iter()
            .filter(|&&(_, fk)| join(fk) == Join::Key(fk))
            .map(|&(p, _)| p)
            .collect();
        match (parents.as_slice(), named.as_slice()) {
            (_, [p]) => Some(*p),
            ([(p, fk)], []) if matches!(join(*fk), Join::Ambiguous(_)) => Some(*p),
            _ => None,
        }
    }

    /// Whether a `holds` body applied to `uc` reads `parent` through key `fk` from a `child`
    /// the action creates, so the creation links to the row the guard reads.
    fn guard_reads_parent(&self, uc: UseCaseKey, fk: FkId) -> bool {
        let model = self.model;
        let key = model.foreign_key(fk);
        applied_bodies(model).into_iter().any(|(_, u, body)| {
            u == uc
                && body
                    .conds
                    .iter()
                    .flat_map(preds::cond_entities)
                    .any(|ek| ek == key.parent)
                && model.join_key(key.child, key.parent, &body.by) == Join::Key(fk)
        })
    }

    /// A use case that drives a comparison and also sets every Int operand of it to a literal
    /// must drive the truth those literals give. Otherwise the model contradicts itself.
    fn check_driven_truth(&mut self, uc: UseCaseKey, idx: usize, site: &Site) {
        let model = self.model;
        let pe = &model.proposition_effects[idx];
        let literal = |column: &str| {
            model.column_effects.iter().enumerate().find_map(|(i, ce)| {
                match (ce.origin == uc && ce.entity == pe.entity && ce.column == column)
                    .then(|| ce.value.literal().map(EffectValue::normalize))
                    .flatten()
                {
                    Some(EffectValue::Int(n)) => Some((*n, i)),
                    _ => None,
                }
            })
        };
        let Some((lhs, lhs_at)) = literal(&pe.prop.lhs_column) else {
            return;
        };
        let (rhs, rhs_at) = match &pe.prop.rhs {
            CmpRhs::IntLit(n) => (*n, None),
            CmpRhs::Column(c) => match literal(c) {
                Some((n, i)) => (n, Some(i)),
                None => return,
            },
            CmpRhs::Now | CmpRhs::Value(_) | CmpRhs::Values(_) => return,
        };
        let holds = match pe.prop.op {
            CmpOpModel::Lt => lhs < rhs,
            CmpOpModel::Gt => lhs > rhs,
            CmpOpModel::Le => lhs <= rhs,
            CmpOpModel::Ge => lhs >= rhs,
            CmpOpModel::Eq => lhs == rhs,
            CmpOpModel::Ne => lhs != rhs,
            // A driven comparison is never a set: `drives` rejects `in`.
            CmpOpModel::In | CmpOpModel::NotIn => return,
        };
        if holds == pe.truth {
            return;
        }
        let mut sites = vec![site.clone()];
        for i in std::iter::once(lhs_at).chain(rhs_at) {
            sites.push(Site::new(
                ModelRef::ColumnEffect(i),
                &model.column_effects[i].site,
            ));
        }
        self.diag(
            vec![pe.entity],
            StateDiag::ContradictoryDrivenComparison {
                usecase: model.use_cases[uc].id.clone(),
                proposition: pe.prop.display(),
                driven: pe.truth,
                sites,
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn assign(
        &mut self,
        a: &mut ActionDraft,
        uc: UseCaseKey,
        ek: EntityKey,
        var: VarId,
        param: u8,
        rhs: Rhs,
        site: Site,
    ) {
        let existing = a.effects.iter_mut().find_map(|eff| match eff {
            Effect::Assign {
                var: v,
                param: q,
                rhs,
                site,
            } if *v == var && *q == param => Some((rhs, site)),
            _ => None,
        });
        if let Some((old_rhs, old_site)) = existing {
            if *old_rhs != rhs {
                let sites = vec![old_site.clone(), site.clone()];
                *old_rhs = rhs;
                *old_site = site;
                let column = self.ts.var_name(var);
                self.diag(
                    vec![ek],
                    StateDiag::ConflictingEffects {
                        usecase: self.model.use_cases[uc].id.clone(),
                        column,
                        sites,
                    },
                );
            }
            return;
        }
        a.effects.push(Effect::Assign {
            var,
            param,
            rhs,
            site,
        });
    }

    fn effect_value(
        &mut self,
        domain: &Domain,
        value: &EffectValue,
        site: &Site,
        ek: EntityKey,
    ) -> EffectRhs {
        match (domain, value.normalize()) {
            (Domain::Enum(values), EffectValue::EnumVariant(s)) => {
                variant_index(values, s).map_or(EffectRhs::None, |i| EffectRhs::Value(Value(i)))
            }
            (Domain::Bool, EffectValue::Bool(b)) => {
                EffectRhs::Value(if *b { Value::TRUE } else { Value::FALSE })
            }
            (Domain::Nullable, EffectValue::Null) => EffectRhs::Value(Value::NULL),
            (Domain::Nullable, EffectValue::Present | EffectValue::Int(_)) => {
                EffectRhs::Value(Value::PRESENT)
            }
            (Domain::Int | Domain::NullableInt, EffectValue::Int(n)) => {
                match domain.of_number(*n, self.bounds) {
                    Some(v) => EffectRhs::Value(v),
                    None => {
                        self.out_of_bounds(*n, site, ek);
                        EffectRhs::None
                    }
                }
            }
            (Domain::NullableInt, EffectValue::Null) => EffectRhs::Value(Value::NULL),
            (Domain::NullableInt, EffectValue::Present) => EffectRhs::AnyNonNull,
            _ => EffectRhs::None,
        }
    }

    fn out_of_bounds(&mut self, value: i64, site: &Site, ek: EntityKey) {
        self.diag(
            vec![ek],
            StateDiag::IntLiteralOutOfBounds {
                value,
                max: self.bounds.int_max,
                site: site.clone(),
            },
        );
    }

    /// One `Transition` effect per column: its arms are the use case's `from -> to` pairs,
    /// and its guard is that the column holds one of the `from` values. Arms that share a
    /// `from` are a choice: one run takes one of them.
    fn transition_effects(&mut self, uc: UseCaseKey, a: &mut ActionDraft) {
        let model = self.model;
        let first = a.effects.len();
        for (idx, st) in model.state_transitions.iter().enumerate() {
            if st.usecase != uc {
                continue;
            }
            let e = self.ent_of[&st.entity];
            let (Some(p), Some(&var)) = (a.param_of(e), self.col_var.get(&(e, st.column.clone())))
            else {
                continue;
            };
            let Domain::Enum(values) = &self.ts.vars[var.index()].domain else {
                continue;
            };
            let (Some(from), Some(to)) = (
                variant_index(values, &st.from),
                variant_index(values, &st.to),
            ) else {
                continue;
            };
            let arm = (Value(from), Value(to));
            let site = Site::new(ModelRef::Transition(idx), &st.site);
            let guard: Vec<(ConditionKey, String)> = st
                .guard
                .map(|ck| (ck, model.conditions[ck].id.clone()))
                .into_iter()
                .collect();
            let existing = a.effects[first..].iter_mut().find_map(|eff| match eff {
                Effect::Transition {
                    var: v,
                    arms,
                    sites,
                    conds,
                    ..
                } if *v == var => Some((arms, sites, conds)),
                _ => None,
            });
            match existing {
                Some((arms, sites, conds)) => match arms.iter().position(|x| *x == arm) {
                    // The same arm declared again: either declaration enables it.
                    Some(i) if conds[i].is_empty() => {}
                    Some(i) if guard.is_empty() => conds[i].clear(),
                    Some(i) => {
                        for c in guard {
                            if !conds[i].contains(&c) {
                                conds[i].push(c);
                            }
                        }
                    }
                    None => {
                        arms.push(arm);
                        sites.push(site);
                        conds.push(guard);
                    }
                },
                None => a.effects.push(Effect::Transition {
                    var,
                    param: p,
                    arms: vec![arm],
                    sites: vec![site],
                    conds: vec![guard],
                }),
            }
        }
        for eff in &a.effects[first..] {
            let Effect::Transition {
                var,
                param,
                arms,
                sites,
                ..
            } = eff
            else {
                continue;
            };
            let at = At::Param(*param);
            let mut froms: Vec<Value> = Vec::new();
            for (from, _) in arms {
                if !froms.contains(from) {
                    froms.push(*from);
                }
            }
            let pred = Pred::Or(
                froms
                    .into_iter()
                    .map(|from| Pred::Cmp(Term::Slot { var: *var, at }, CmpOp::Eq, Term::Lit(from)))
                    .collect(),
            );
            a.guards.push(Guard {
                pred,
                why: GuardWhy::TransitionFrom,
                site: sites[0].clone(),
            });
        }
    }

    /// One successor guard per column with a `.when(Cond)` arm or chosen value: the step takes
    /// an arm whose guard held on the pre-state, `\/ (col = from /\ col' = to /\ guard)` over
    /// the `transitions` arms, `\/ (col' = v /\ guard)` over the `chooses` values (an unguarded
    /// arm's or value's guard is true). It filters the choice the `Transition` or `Choose`
    /// effect forks, so the use case runs when some arm or value is enabled, and a body on a
    /// created param reads it after the step, as a use case's condition does.
    fn arm_guards(&mut self, a: &mut ActionDraft) {
        let model = self.model;
        let mut guards = Vec::new();
        for eff in &a.effects {
            // Each arm or value: its comparisons on the column, its site, its conditions.
            let arms: Vec<GuardedArm> = match eff {
                Effect::Transition {
                    var,
                    param,
                    arms,
                    sites,
                    conds,
                } => {
                    let slot = |at| Term::Slot { var: *var, at };
                    arms.iter()
                        .zip(sites)
                        .zip(conds)
                        .map(|(((from, to), site), conds)| {
                            let parts = vec![
                                Pred::Cmp(slot(At::Param(*param)), CmpOp::Eq, Term::Lit(*from)),
                                Pred::Cmp(slot(At::Next(*param)), CmpOp::Eq, Term::Lit(*to)),
                            ];
                            (parts, site, conds)
                        })
                        .collect()
                }
                Effect::Choose {
                    var,
                    param,
                    values,
                    sites,
                    conds,
                } => {
                    let next = Term::Slot {
                        var: *var,
                        at: At::Next(*param),
                    };
                    values
                        .iter()
                        .zip(sites)
                        .zip(conds)
                        .map(|((v, site), conds)| {
                            let parts = vec![Pred::Cmp(next.clone(), CmpOp::Eq, Term::Lit(*v))];
                            (parts, site, conds)
                        })
                        .collect()
                }
                _ => continue,
            };
            let Some(first) = arms.iter().position(|(_, _, c)| !c.is_empty()) else {
                continue;
            };
            let mut any = Vec::new();
            for (mut parts, site, conds) in arms.iter().cloned() {
                if !conds.is_empty() {
                    let bodies = conds
                        .iter()
                        .filter(|(ck, _)| model.conditions[*ck].body.is_some())
                        .map(|(ck, _)| self.body_pred(*ck, &a.params, site))
                        .collect();
                    parts.push(Pred::Or(bodies));
                }
                any.push(Pred::And(parts));
            }
            guards.push(Guard {
                pred: Pred::Or(any),
                why: GuardWhy::ArmCondition {
                    cond: arms[first].2[0].0,
                },
                site: arms[first].1.clone(),
            });
        }
        a.guards.extend(guards);
    }

    /// `chooses(UC, E.col)`: the actor picks the variation column's value again, one successor
    /// per value, as on a creation; `chooses(UC, E.col, v)` lines list the values. A value
    /// listed twice is enabled by either line, as a `transitions` arm declared twice: an
    /// unguarded line clears its guards, guarded lines join theirs. On a created param it is
    /// the creation's own choice.
    fn chosen_columns(&mut self, uc: UseCaseKey, a: &mut ActionDraft) {
        let model = self.model;
        for (idx, choice) in model.column_choices.iter().enumerate() {
            if choice.origin != uc {
                continue;
            }
            let Some(&e) = self.ent_of.get(&choice.entity) else {
                continue;
            };
            let (Some(p), Some(&var)) =
                (a.param_of(e), self.col_var.get(&(e, choice.column.clone())))
            else {
                continue;
            };
            if a.writes(var, p) {
                continue;
            }
            let fact = ModelRef::ColumnChoice(idx);
            let mut values = Vec::new();
            let mut sites = Vec::new();
            let mut conds: Vec<Vec<(ConditionKey, String)>> = Vec::new();
            if choice.values.is_empty() {
                let size = self.ts.vars[var.index()].domain.size(self.bounds);
                values = (0..size).map(|i| Value(i as u8)).collect();
                sites = vec![Site::new(fact.clone(), &choice.site); values.len()];
                conds = vec![Vec::new(); values.len()];
            }
            let names = match &self.ts.vars[var.index()].domain {
                Domain::Enum(names) => names.as_slice(),
                _ => &[],
            };
            for chosen in &choice.values {
                let Some(v) = variant_index(names, &chosen.value).map(Value) else {
                    continue;
                };
                let guard: Vec<(ConditionKey, String)> = chosen
                    .guard
                    .map(|ck| (ck, model.conditions[ck].id.clone()))
                    .into_iter()
                    .collect();
                match values.iter().position(|x| *x == v) {
                    Some(i) if conds[i].is_empty() => {}
                    Some(i) if guard.is_empty() => conds[i].clear(),
                    Some(i) => {
                        for c in guard {
                            if !conds[i].contains(&c) {
                                conds[i].push(c);
                            }
                        }
                    }
                    None => {
                        values.push(v);
                        sites.push(Site::new(fact.clone(), &chosen.site));
                        conds.push(guard);
                    }
                }
            }
            a.effects.push(Effect::Choose {
                var,
                param: p,
                values,
                sites,
                conds,
            });
        }
    }

    /// Vision rule 5: an actor chooses a variation column's value, so a created instance takes
    /// any value unless the creating use case sets it. `@default` does not pin the choice.
    fn creation_choices(&mut self, a: &mut ActionDraft) {
        for p in 0..a.params.len() {
            let Param { entity, role, .. } = a.params[p];
            if role != ParamRole::New {
                continue;
            }
            for &var in &self.ts.entities[entity.index()].columns {
                let VarKind::Column {
                    variation: true, ..
                } = &self.ts.vars[var.index()].kind
                else {
                    continue;
                };
                if a.writes(var, p as u8) {
                    continue;
                }
                let size = self.ts.vars[var.index()].domain.size(self.bounds);
                a.effects.push(Effect::Choose {
                    var,
                    param: p as u8,
                    values: (0..size).map(|i| Value(i as u8)).collect(),
                    sites: Vec::new(),
                    conds: Vec::new(),
                });
            }
        }
    }

    /// R-bind: a created child belongs to its parent, a present one unless the relate is
    /// optional; a child the action updates must already belong to a parent the action only
    /// updates. An optional key also links a created child to a present parent when a guard
    /// of the action reads the parent through that key.
    ///
    /// Relink: an action that creates the parent and updates an existing child links the child.
    /// For an optional key the child must be unlinked, so a linked child is not relinked; a
    /// mandatory child always has a parent, so the link is a move. A parent param is a row
    /// the action reads, not writes, so it is never relinked. Each key has its own parent
    /// param, so a child created under two keys to one parent the action does not write may
    /// point at two rows.
    fn owner_bindings(&mut self, uc: UseCaseKey, a: &mut ActionDraft) {
        for &(var, child, parent) in &self.owners.clone() {
            let Some(c) = a.param_of(child) else { continue };
            let VarKind::Owner { fk, .. } = self.ts.vars[var.index()].kind else {
                continue;
            };
            let site = self.ts.vars[var.index()].site.clone();
            let parent_is_new = a
                .param_of(parent)
                .is_some_and(|p| a.params[usize::from(p)].role == ParamRole::New);
            let child_is_new = a.params[usize::from(c)].role == ParamRole::New;
            if child_is_new || parent_is_new {
                let optional = matches!(
                    self.ts.vars[var.index()].domain,
                    Domain::Instance { optional: true, .. }
                );
                if optional && !child_is_new {
                    a.guards.push(Guard {
                        pred: Pred::Cmp(
                            Term::Slot {
                                var,
                                at: At::Param(c),
                            },
                            CmpOp::Eq,
                            Term::Lit(Value::NULL),
                        ),
                        why: GuardWhy::Owner,
                        site: site.clone(),
                    });
                }
                let guarded = || self.guard_reads_parent(uc, fk);
                let rhs = match a.param_of(parent) {
                    Some(p) => Rhs::Param(p),
                    None if optional && !(child_is_new && guarded()) => Rhs::Lit(Value::NULL),
                    None => Rhs::Param(a.param(parent, ParamRole::Parent { key: fk })),
                };
                a.effects.push(Effect::Assign {
                    var,
                    param: c,
                    rhs,
                    site,
                });
            } else if let Some(p) = a.param_of(parent) {
                a.guards.push(Guard {
                    pred: Pred::Cmp(
                        Term::Slot {
                            var,
                            at: At::Param(c),
                        },
                        CmpOp::Eq,
                        Term::Index(At::Param(p)),
                    ),
                    why: GuardWhy::Owner,
                    site,
                });
            }
        }
    }
}

/// Why a copy has no source row: the reason for `UnboundCopySource`, or the keys that could
/// lead to it.
enum CopyMiss {
    Unbound(String),
    Ambiguous(Vec<FkId>),
}

enum EffectRhs {
    Value(Value),
    /// `sets(.., col == present)` on a nullable Int column: any non-null value.
    AnyNonNull,
    None,
}

#[derive(Default)]
struct ActionDraft {
    params: Vec<Param>,
    guards: Vec<Guard>,
    effects: Vec<Effect>,
}

impl ActionDraft {
    fn param(&mut self, entity: EntId, role: ParamRole) -> u8 {
        self.params.push(Param {
            entity,
            role,
            via_api: None,
        });
        (self.params.len() - 1) as u8
    }

    /// The param of a row of `entity` the action writes.
    fn param_of(&self, entity: EntId) -> Option<u8> {
        self.params
            .iter()
            .position(|p| p.entity == entity && p.role.is_written())
            .map(|i| i as u8)
    }

    fn writes(&self, var: VarId, param: u8) -> bool {
        self.effects.iter().any(|eff| match eff {
            Effect::Assign {
                var: v, param: p, ..
            }
            | Effect::Transition {
                var: v, param: p, ..
            }
            | Effect::Choose {
                var: v, param: p, ..
            } => *v == var && *p == param,
            _ => false,
        })
    }
}

/// Before a copy into `var` of param `p`: drops a creation's free choice of the column, which
/// the copy pins, and returns the site of any other write of it in the step, which conflicts
/// with the copy.
fn prior_write(a: &mut ActionDraft, var: VarId, p: u8) -> Option<Site> {
    let at = a.effects.iter().position(|eff| match eff {
        Effect::Assign {
            var: v, param: q, ..
        }
        | Effect::Transition {
            var: v, param: q, ..
        }
        | Effect::Choose {
            var: v, param: q, ..
        } => *v == var && *q == p,
        _ => false,
    })?;
    match &a.effects[at] {
        Effect::Choose { sites, .. } if sites.is_empty() => {
            a.effects.remove(at);
            None
        }
        Effect::Assign { site, .. } => Some(site.clone()),
        Effect::Choose { sites, .. } | Effect::Transition { sites, .. } => sites.first().cloned(),
        _ => None,
    }
}

fn first_buc(model: &SemanticModel, uc: UseCaseKey) -> Option<crate::model::BucKey> {
    model
        .relations
        .iter()
        .find_map(|rel| match (&rel.kind, &rel.from, &rel.to) {
            (RelKind::Contains, NodeRef::Buc(b), NodeRef::UseCase(u)) if *u == uc => Some(*b),
            _ => None,
        })
}

/// Declared casing wins: exact match first, then case-insensitive.
/// An arm or chosen value under `.when`: its comparisons on the column, its site, and its
/// conditions.
type GuardedArm<'a> = (Vec<Pred>, &'a Site, &'a Vec<(ConditionKey, String)>);

fn variant_index(values: &[String], name: &str) -> Option<u8> {
    values
        .iter()
        .position(|v| v == name)
        .or_else(|| values.iter().position(|v| v.eq_ignore_ascii_case(name)))
        .map(|i| i as u8)
}
