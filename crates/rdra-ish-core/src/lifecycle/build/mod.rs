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
use crate::analysis::{applied_bodies, AppliedBody};
use crate::model::{
    ApiKey, CmpOpModel, CmpRhs, ColumnType, ConditionKey, CrossConstraintScope, EffectValue,
    EntityKey, FkId, Join, LinkKind, Min, ModelColumn, NodeRef, RelKind, SemanticModel, SetValue,
    UseCaseKey,
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
    b.name_generated_checks_apart();
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
    /// The run mark of each entity a step before the last of a split use case writes.
    run_marks: HashMap<(UseCaseKey, EntId), VarId>,
    /// Use cases whose actions exist, each with its actions in transaction order.
    uc_actions: HashMap<UseCaseKey, Vec<super::types::ActId>>,
    /// Use cases with any write in the full model (before the scope filter).
    active_ucs: HashSet<UseCaseKey>,
    diags: Vec<Diag>,
    /// Parallel to `diags`: the entities of the actions each diagnostic altered.
    altered: Vec<Vec<EntityKey>>,
    /// The entities of the use case action being built, if any.
    acting: Vec<EntityKey>,
    /// The entity each `holds` body on a split use case reads before the later transaction
    /// group that writes it (`GuardReadsLaterStep`), for the guard's `Runs_` row.
    later_reads: HashMap<(UseCaseKey, ConditionKey), EntityKey>,
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
            run_marks: HashMap::new(),
            uc_actions: HashMap::new(),
            active_ucs: HashSet::new(),
            diags: Vec::new(),
            altered: Vec::new(),
            acting: Vec::new(),
            later_reads: HashMap::new(),
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
            source: super::types::ActId::at(0),
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
            let written_apis: HashSet<ApiKey> = utx.writes().filter_map(|w| w.via_api).collect();
            self.run_mark_vars(uc, &entities);
            self.later_step_reads(uc, &groups, &written_apis);
            for (g, writes) in groups.iter().enumerate() {
                let step = (entities.len() > 1).then(|| u8::try_from(g + 1).expect("few groups"));
                let name = match step {
                    None => id.clone(),
                    Some(n) => format!("{id}_{n}"),
                };
                let bodies = self.step_bodies(uc, writes, g, &written_apis);
                self.acting = entities[g].clone();
                let action = self.usecase_action(uc, name, writes, &entities, g, &bodies);
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
        for (i, a) in self.ts.actions.iter_mut().enumerate() {
            a.source = super::types::ActId::at(i);
        }
        self.name_generated_actions_apart();
        self.inert_conditions();
    }

    /// A generated action name, `<UC>_<n>` for a step of a split use case or `Tick` for the
    /// clock, that is a use case's id or an earlier generated name takes the first free
    /// `<name>_<k>`, k from 2, so action names are unique and a model whose generated names
    /// collide with nothing keeps every name.
    fn name_generated_actions_apart(&mut self) {
        let model = self.model;
        let generated = |a: &Action| match &a.origin {
            ActionOrigin::Clock => true,
            ActionOrigin::UseCase { uc, .. } => a.name != model.use_cases[*uc].id,
        };
        let mut taken: HashSet<String> = self
            .ts
            .actions
            .iter()
            .filter(|a| !generated(a))
            .map(|a| a.name.clone())
            .collect();
        for a in &mut self.ts.actions {
            if !generated(a) {
                continue;
            }
            let mut name = a.name.clone();
            let mut k = 2;
            while taken.contains(&name) {
                name = format!("{}_{k}", a.name);
                k += 1;
            }
            taken.insert(name.clone());
            a.name = name;
        }
    }

    /// An invariant or after-assert name (`Check_<E>_<column>_<n>`, `Unique_<Child>_<role>`,
    /// ...) that a temporal property's or use case's id, or an earlier check's name, already
    /// spells takes the first free `<name>_<k>`, k from 2, as a generated action name does.
    /// Entity, column, and role ids may hold `_`, so two generated names can coincide; a
    /// model whose names collide with nothing keeps every name.
    fn name_generated_checks_apart(&mut self) {
        let model = self.model;
        let mut taken: HashSet<String> = model
            .temporal_properties
            .iter()
            .map(|p| p.id.clone())
            .chain(model.use_cases.values().map(|uc| uc.id.clone()))
            .collect();
        let names = self
            .ts
            .invariants
            .iter_mut()
            .map(|i| &mut i.name)
            .chain(self.ts.after.iter_mut().map(|a| &mut a.name));
        for name in names {
            let base = name.clone();
            let mut k = 2;
            while !taken.insert(name.clone()) {
                *name = format!("{base}_{k}");
                k += 1;
            }
        }
    }

    /// One run mark per slot of each entity a step before the last of a split use case
    /// writes (docs/state-derivation.md, "Runs of a split use case"). `groups` lists every
    /// group's entities, in transaction order.
    fn run_mark_vars(&mut self, uc: UseCaseKey, groups: &[Vec<EntityKey>]) {
        let Some((_, earlier)) = groups.split_last() else {
            return;
        };
        let model = self.model;
        let names: Vec<String> = std::iter::once("none".to_string())
            .chain((1..groups.len()).map(|i| i.to_string()))
            .collect();
        let id = &model.use_cases[uc].id;
        let site = Site::new(
            ModelRef::UseCase(uc),
            &model.decl_sites.located("usecase", id),
        );
        for &ek in earlier.iter().flatten() {
            let e = self.ent_of[&ek];
            let var = self.push_var(
                VarKind::Run {
                    entity: e,
                    usecase: id.clone(),
                },
                Domain::Enum(names.clone()),
                Value(0),
                site.clone(),
            );
            self.run_marks.insert((uc, e), var);
        }
    }

    /// The `holds` bodies step `step` (0-based) of the use case carries: the bodies applied
    /// to the use case on step 1, the precondition of the run, and a body applied to an API
    /// on the step of that API's group, or on step 1 when the API is in `written_apis`, the
    /// APIs the use case writes through, for no group.
    fn step_bodies(
        &self,
        uc: UseCaseKey,
        writes: &[UcWrite],
        step: usize,
        written_apis: &HashSet<ApiKey>,
    ) -> Vec<AppliedBody<'m>> {
        applied_bodies(self.model)
            .into_iter()
            .filter(|b| b.usecase == uc)
            .filter(|b| match b.api {
                Some(api) if written_apis.contains(&api) => {
                    writes.iter().any(|w| w.via_api == Some(api))
                }
                _ => step == 0,
            })
            .collect()
    }

    /// One atomic action for one transaction group of the use case: every effect on every
    /// entity of the group in one step. `groups` lists every group's entities, in transaction
    /// order, `step` is this group's index in it, and `bodies` the `holds` bodies this step
    /// carries.
    fn usecase_action(
        &mut self,
        uc: UseCaseKey,
        name: String,
        writes: &[UcWrite],
        groups: &[Vec<EntityKey>],
        step: usize,
        bodies: &[AppliedBody<'m>],
    ) -> Action {
        let model = self.model;
        let mut a = ActionDraft::default();
        let mut sorted: Vec<(EntId, WriteKind, Option<ApiKey>)> = writes
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
        let ent_of = &self.ent_of;
        let mut earlier: Vec<(EntId, u8)> = groups[..step]
            .iter()
            .enumerate()
            .flat_map(|(j, ents)| ents.iter().map(move |ek| (ent_of[ek], j as u8 + 1)))
            .collect();
        earlier.sort();
        for (e, wrote) in earlier {
            a.param(e, ParamRole::Earlier { step: wrote });
        }
        self.sets_effects(uc, &mut a);
        self.chosen_columns(uc, &mut a);
        self.transition_effects(uc, &mut a);
        self.creation_choices(&mut a);
        self.owner_bindings(uc, bodies, &mut a);
        self.link_effects(uc, &mut a);
        self.copy_effects(uc, &mut a, groups);
        self.run_marks(uc, &mut a, step, groups.len());
        let guards = self.condition_guards(bodies, &a.params);
        a.guards.extend(guards);
        self.arm_guards(uc, &mut a);
        Action {
            name,
            source: super::types::ActId::at(0),
            origin: ActionOrigin::UseCase {
                uc,
                buc: first_buc(model, uc),
                step: (groups.len() > 1).then(|| u8::try_from(step + 1).expect("few groups")),
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

    /// The run guards and mark effects of step `step` (0-based) of `k`: a written row of a
    /// step before the last is in no run (`none`) and takes this step's number; an `Earlier`
    /// row carries the previous step's number and takes this step's, or `none` at the last
    /// step. A created row's mark is at base already.
    fn run_marks(&mut self, uc: UseCaseKey, a: &mut ActionDraft, step: usize, k: usize) {
        let this = step as u8 + 1;
        let next = if usize::from(this) < k {
            Value(this)
        } else {
            Value(0)
        };
        for p in 0..a.params.len() {
            let Param { entity, role, .. } = a.params[p];
            let Some(&var) = self.run_marks.get(&(uc, entity)) else {
                continue;
            };
            let expected = match role {
                ParamRole::New => None,
                ParamRole::Existing => Some(0),
                ParamRole::Earlier { .. } => Some(this - 1),
                ParamRole::Parent { .. } => continue,
            };
            let site = self.ts.vars[var.index()].site.clone();
            if let Some(mark) = expected {
                a.guards.push(Guard {
                    pred: Pred::Cmp(
                        Term::Slot {
                            var,
                            at: At::Param(p as u8),
                        },
                        CmpOp::Eq,
                        Term::Lit(Value(mark)),
                    ),
                    why: GuardWhy::Run { step: mark },
                    site: site.clone(),
                });
            }
            a.effects.push(Effect::Assign {
                var,
                param: p as u8,
                rhs: Rhs::Lit(next),
                site,
            });
        }
    }

    fn sets_effects(&mut self, uc: UseCaseKey, a: &mut ActionDraft) {
        let model = self.model;
        // Unguarded lines first, so a guarded line of a column they write conflicts with them.
        let lines = model
            .column_effects
            .iter()
            .enumerate()
            .filter(|(_, ce)| ce.origin == uc);
        let (guarded, unguarded): (Vec<_>, Vec<_>) = lines.partition(|(_, ce)| ce.guard.is_some());
        for (idx, ce) in unguarded.into_iter().chain(guarded) {
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
            let rhs = self.effect_value(&domain, value, &site, ce.entity);
            if let Some(ck) = ce.guard {
                let values = match rhs {
                    EffectRhs::Value(v) => vec![v],
                    EffectRhs::AnyNonNull => (1..domain.size(self.bounds))
                        .map(|i| Value(i as u8))
                        .collect(),
                    EffectRhs::None => continue,
                };
                self.guarded_set(a, uc, ce.entity, var, p, values, (ck, site));
                continue;
            }
            match rhs {
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

    /// `sets(UC, E.col == v).when(Cond)`: one guarded value of a choice that the step's
    /// `GuardWhy::ArmCondition` guard filters, as a guarded `chooses` value is. The lines of
    /// one column join one `Choose`, and a value two lines give is enabled by either. An
    /// unguarded `sets` of the column conflicts with it, and so does a copy (`copy_effects`).
    #[allow(clippy::too_many_arguments)]
    fn guarded_set(
        &mut self,
        a: &mut ActionDraft,
        uc: UseCaseKey,
        ek: EntityKey,
        var: VarId,
        p: u8,
        values: Vec<Value>,
        (ck, site): (ConditionKey, Site),
    ) {
        let cond = (ck, self.model.conditions[ck].id.clone());
        let guarded = a.effects.iter().position(|eff| {
            matches!(eff, Effect::Choose { var: v, param: q, conds, .. }
                if (*v, *q) == (var, p) && !conds.is_empty() && conds.iter().all(|c| !c.is_empty()))
        });
        let at = match guarded {
            Some(at) => at,
            None => {
                if let Some(old) = prior_write(a, var, p) {
                    let column = self.ts.var_name(var);
                    self.diag(
                        vec![ek],
                        StateDiag::ConflictingEffects {
                            usecase: self.model.use_cases[uc].id.clone(),
                            column,
                            sites: vec![old, site],
                        },
                    );
                    return;
                }
                a.effects.push(Effect::Choose {
                    var,
                    param: p,
                    values: Vec::new(),
                    sites: Vec::new(),
                    conds: Vec::new(),
                });
                a.effects.len() - 1
            }
        };
        let Effect::Choose {
            values: have,
            sites,
            conds,
            ..
        } = &mut a.effects[at]
        else {
            return;
        };
        for v in values {
            match have.iter().position(|x| *x == v) {
                Some(i) if !conds[i].contains(&cond) => conds[i].push(cond.clone()),
                Some(_) => {}
                None => {
                    have.push(v);
                    sites.push(site.clone());
                    conds.push(vec![cond.clone()]);
                }
            }
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
                        "sets({}, {}.{} == {}.{})",
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
                ParamRole::Existing | ParamRole::Parent { .. } | ParamRole::Earlier { .. } => {
                    At::Param(q)
                }
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
        let later = groups.iter().flatten().any(|ek| *ek == source);
        Err(CopyMiss::Unbound(if later {
            format!("the use case writes {source_id} in a later transaction group, a step of its own; order the use case's invokes so {source_id} is written first, or relate {target_id} to {source_id} so one step writes both")
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
    /// the action writes or an earlier step of its run wrote, else the `Parent` param of the
    /// key the statement's join follows ([`SemanticModel::join_key`] from that key's child).
    /// One `Parent` param of `e` whose join no `.by` names is the row the action reads.
    /// Otherwise, for a key with no `Parent` param or for several such params and no `.by`
    /// naming one, `None`: the statement joins its own row of `e`.
    fn read_param(&self, params: &[Param], e: EntId, by: &[String]) -> Option<u8> {
        if let Some(p) = params.iter().position(|p| p.entity == e && p.role.is_row()) {
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

    /// Whether a `holds` body the action carries reads `parent` through key `fk` from a
    /// `child` the action creates, so the creation links to the row the guard reads: the body
    /// compares the parent and its join follows `fk`, or its `.along(..)` takes a hop through
    /// `fk`.
    fn guard_reads_parent(&self, bodies: &[AppliedBody<'m>], fk: FkId) -> bool {
        let model = self.model;
        let key = model.foreign_key(fk);
        bodies.iter().any(|b| {
            let along = match &b.body.along {
                CrossConstraintScope::RelationPath { links, .. } => links.contains(&Some(fk)),
                CrossConstraintScope::GlobalProduct => false,
            };
            let compared = b
                .body
                .conds
                .iter()
                .flat_map(preds::cond_entities)
                .any(|ek| ek == key.parent)
                && model.join_key(key.child, key.parent, &b.body.by) == Join::Key(fk);
            along || compared
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
    fn arm_guards(&mut self, uc: UseCaseKey, a: &mut ActionDraft) {
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
                        .map(|(ck, _)| self.body_pred(uc, *ck, &a.params, site))
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
    /// of the action reads the parent through that key. A row an earlier step of the run
    /// wrote (`Earlier`) is the action's row of its entity here, as a written one is.
    ///
    /// Relink: an action that creates the parent and updates an existing child links the child.
    /// For an optional key the child must be unlinked, so a linked child is not relinked; a
    /// mandatory child always has a parent, so the link is a move. A parent param is a row
    /// the action reads, not writes, so it is never relinked. Each key has its own parent
    /// param, so a child created under two keys to one parent the action does not write may
    /// point at two rows. A parent the action writes binds only the keys
    /// [`Self::written_parent_key`] gives it; every other key is bound as if the action did
    /// not write the parent, and several keys none of which a `.by` names are
    /// `AmbiguousOwnerLink`.
    fn owner_bindings(&mut self, uc: UseCaseKey, bodies: &[AppliedBody<'m>], a: &mut ActionDraft) {
        for &(var, child, parent) in &self.owners.clone() {
            let Some(c) = a.row_of(child) else { continue };
            let VarKind::Owner { fk, .. } = self.ts.vars[var.index()].kind else {
                continue;
            };
            if self.linked_key(uc, fk) {
                // `links` binds this key: see `link_effects`.
                continue;
            }
            let site = self.ts.vars[var.index()].site.clone();
            let written = match a.row_of(parent) {
                Some(p) => match self.written_parent_key(uc, fk) {
                    Join::Key(k) if k == fk => Some(p),
                    Join::Ambiguous(candidates) => {
                        self.ambiguous_written_parent(uc, fk, &candidates);
                        None
                    }
                    _ => None,
                },
                None => None,
            };
            let parent_is_new =
                written.is_some_and(|p| a.params[usize::from(p)].role == ParamRole::New);
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
                let guarded = || self.guard_reads_parent(bodies, fk);
                let rhs = match written {
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
            } else if let Some(p) = written {
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

impl Builder<'_> {
    /// Whether a `links`, `relinks` or `unlinks` of the use case follows key `fk`.
    fn linked_key(&self, uc: UseCaseKey, fk: FkId) -> bool {
        self.model
            .links
            .iter()
            .any(|l| l.origin == uc && l.key == Some(fk))
    }

    /// `links`, `relinks` or `unlinks(UC, Child, Parent)` on the step that writes the child,
    /// each a guard (`GuardWhy::Link`) and an assignment on the child's owner slot of the key.
    /// The parent the step binds is the row it writes, the row an earlier step of its run
    /// wrote, or else any present parent, bound as a `Parent` param of the key.
    /// - `links`: the child is unlinked, and the owner becomes the bound parent.
    /// - `relinks`: the child is linked (no guard on a mandatory key, whose child always is),
    ///   and the owner becomes the bound parent, the one it already has included: a move
    ///   that had to differ could never run at one instance per entity, which is what
    ///   `check` and `states` explore.
    /// - `unlinks`: the child is linked, to the bound parent when the step writes one, and the
    ///   owner becomes unlinked. The step binds no parent otherwise, since nothing reads it.
    fn link_effects(&mut self, uc: UseCaseKey, a: &mut ActionDraft) {
        let model = self.model;
        for (idx, link) in model.links.iter().enumerate() {
            if link.origin != uc {
                continue;
            }
            let Some(fk) = link.key else { continue };
            let (Some(&child), Some(&parent)) =
                (self.ent_of.get(&link.child), self.ent_of.get(&link.parent))
            else {
                continue;
            };
            let (Some(c), Some(var)) = (a.param_of(child), self.owner_var(fk)) else {
                continue;
            };
            let site = Site::new(ModelRef::Link(idx), &link.site);
            let optional = matches!(
                self.ts.vars[var.index()].domain,
                Domain::Instance { optional: true, .. }
            );
            let owner = Term::Slot {
                var,
                at: At::Param(c),
            };
            let bound = a.row_of(parent);
            let guard = match link.kind {
                LinkKind::Links => Some(Pred::Cmp(owner, CmpOp::Eq, Term::Lit(Value::NULL))),
                LinkKind::Relinks if !optional => None,
                LinkKind::Relinks => Some(Pred::Cmp(owner, CmpOp::Ne, Term::Lit(Value::NULL))),
                LinkKind::Unlinks => Some(match bound {
                    Some(p) => Pred::Cmp(owner, CmpOp::Eq, Term::Index(At::Param(p))),
                    None => Pred::Cmp(owner, CmpOp::Ne, Term::Lit(Value::NULL)),
                }),
            };
            if let Some(pred) = guard {
                a.guards.push(Guard {
                    pred,
                    why: GuardWhy::Link,
                    site: site.clone(),
                });
            }
            let rhs = match link.kind {
                LinkKind::Unlinks => Rhs::Lit(Value::NULL),
                LinkKind::Links | LinkKind::Relinks => Rhs::Param(match bound {
                    Some(p) => p,
                    None => a.param(parent, ParamRole::Parent { key: fk }),
                }),
            };
            a.effects.push(Effect::Assign {
                var,
                param: c,
                rhs,
                site,
            });
        }
    }

    /// Which key joins a row of `fk`'s parent that the use case writes to a child it writes
    /// (#76): `Join::Key(fk)` for the one key from that child to that parent, or, among
    /// several, when a `.by` of the use case's `holds` guards (the body's or an `.along` hop's)
    /// or of its copies names `fk`, or a `links` of the use case follows it, so `.by` names the
    /// same row as elsewhere; `Join::Key` of another key when a `.by` names only others;
    /// `Join::Ambiguous` when no `.by` names any of them, which is `AmbiguousOwnerLink` as for
    /// any other join.
    fn written_parent_key(&self, uc: UseCaseKey, fk: FkId) -> Join {
        let model = self.model;
        let key = model.foreign_key(fk);
        let parallel: Vec<FkId> = model
            .foreign_keys()
            .filter(|(_, k)| (k.child, k.parent) == (key.child, key.parent))
            .map(|(id, _)| id)
            .collect();
        if parallel.len() == 1 {
            return Join::Key(fk);
        }
        let linked = model
            .links
            .iter()
            .filter(|l| l.origin == uc)
            .filter_map(|l| l.key)
            .filter(|id| parallel.contains(id));
        let guards = applied_bodies(model)
            .into_iter()
            .filter(|b| b.usecase == uc)
            .flat_map(|b| {
                let hops = match &b.body.along {
                    CrossConstraintScope::RelationPath { by, .. } => by.as_slice(),
                    CrossConstraintScope::GlobalProduct => &[],
                };
                b.body.by.iter().chain(hops)
            });
        let copies = model
            .column_effects
            .iter()
            .filter(|ce| ce.origin == uc && matches!(ce.value, SetValue::Copy(_)))
            .flat_map(|ce| ce.by.iter());
        let named: Vec<FkId> = guards
            .chain(copies)
            .filter_map(|role| {
                parallel
                    .iter()
                    .copied()
                    .find(|id| model.key_named(model.foreign_key(*id), role))
            })
            .chain(linked)
            .collect();
        match named.iter().find(|id| **id == fk).or(named.first()) {
            Some(id) => Join::Key(*id),
            None => Join::Ambiguous(parallel),
        }
    }

    /// `AmbiguousOwnerLink` for a parent the use case writes that several keys join to a
    /// child it writes, none of which a `.by` of the use case names. The statement is the
    /// first `holds` body applied to the use case that compares the parent, else its first
    /// copy from the parent, with `.role(<first role>)` as the rewrite; a use case with neither
    /// is told where a `.by` can go.
    fn ambiguous_written_parent(&mut self, uc: UseCaseKey, fk: FkId, candidates: &[FkId]) {
        let model = self.model;
        let key = model.foreign_key(fk);
        let (child, parent) = (key.child, key.parent);
        let holds = applied_bodies(model)
            .into_iter()
            .filter(|b| b.usecase == uc)
            .find(|b| {
                b.body
                    .conds
                    .iter()
                    .flat_map(preds::cond_entities)
                    .any(|ek| ek == parent)
            })
            .map(|b| {
                (
                    format!("holds({}, ...)", model.conditions[b.cond].id),
                    Site::new(ModelRef::Condition(b.cond), &b.body.site),
                )
            });
        let copy = model
            .column_effects
            .iter()
            .enumerate()
            .find_map(|(idx, ce)| {
                let SetValue::Copy(source) = &ce.value else {
                    return None;
                };
                (ce.origin == uc && ce.entity == child && source.entity == parent).then(|| {
                    (
                        format!(
                            "sets({}, {}.{} == {}.{})",
                            model.use_cases[uc].id,
                            model.entities[child].id,
                            ce.column,
                            model.entities[parent].id,
                            source.column
                        ),
                        Site::new(ModelRef::ColumnEffect(idx), &ce.site),
                    )
                })
            });
        let diag = match holds.or(copy) {
            Some((statement, site)) => {
                preds::ambiguous_owner_link(model, &statement, child, parent, candidates, &site)
            }
            None => {
                let uc_id = &model.use_cases[uc].id;
                let roles: Vec<&str> = candidates
                    .iter()
                    .map(|id| model.key_role(model.foreign_key(*id)))
                    .collect();
                StateDiag::AmbiguousOwnerLink {
                    statement: format!("usecase {uc_id}"),
                    a: model.entities[child].id.clone(),
                    b: model.entities[parent].id.clone(),
                    roles: roles.join(", "),
                    rewrite: format!(
                        ".role({}) on a holds guard applied to {uc_id} that compares {}, or on a copy from {} in it",
                        roles[0], model.entities[parent].id, model.entities[parent].id
                    ),
                    site: Site::new(
                        ModelRef::UseCase(uc),
                        &model.decl_sites.located("usecase", uc_id),
                    ),
                }
            }
        };
        self.diag(vec![child], diag);
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

    /// The param of the action's row of `entity`: one it writes, or one an earlier step of
    /// its run wrote.
    fn row_of(&self, entity: EntId) -> Option<u8> {
        self.params
            .iter()
            .position(|p| p.entity == entity && p.role.is_row())
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
