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
use crate::model::{
    CmpOpModel, CmpRhs, ColumnType, EffectValue, EntityKey, Min, ModelColumn, NodeRef, RelKind,
    SemanticModel, UseCaseKey,
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
        let mut keys: Vec<EntityKey> = model.entities.keys().collect();
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
                EffectValue::TypedPresent(t) => Some(t.clone()),
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
            let entities: Vec<Vec<EntityKey>> = utx
                .groups()
                .map(|g| g.ordered_writes.iter().map(|w| w.entity).collect())
                .collect();
            let id = &model.use_cases[uc].id;
            for (g, group) in utx.groups().enumerate() {
                let name = match entities.len() {
                    1 => id.clone(),
                    _ => format!("{id}_{}", g + 1),
                };
                self.acting = group.ordered_writes.iter().map(|w| w.entity).collect();
                let action = self.usecase_action(uc, name, &group.ordered_writes, &entities, g);
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
    /// order; `group` is this one.
    fn usecase_action(
        &mut self,
        uc: UseCaseKey,
        name: String,
        writes: &[UcWrite],
        groups: &[Vec<EntityKey>],
        group: usize,
    ) -> Action {
        let model = self.model;
        let mut a = ActionDraft::default();
        let mut sorted: Vec<(EntId, WriteKind)> = writes
            .iter()
            .map(|w| (self.ent_of[&w.entity], w.kind.clone()))
            .collect();
        sorted.sort_by_key(|(e, _)| *e);
        for (e, kind) in sorted {
            let role = match kind {
                WriteKind::Creates => ParamRole::New,
                WriteKind::Updates | WriteKind::Deletes => ParamRole::Existing,
            };
            let p = a.param(e, role);
            match kind {
                WriteKind::Creates => a.effects.push(Effect::Create { param: p }),
                WriteKind::Deletes => a.effects.push(Effect::Delete { param: p }),
                WriteKind::Updates => {}
            }
        }
        self.sets_effects(uc, &mut a);
        self.transition_effects(uc, &mut a);
        self.creation_choices(&mut a);
        self.owner_bindings(&mut a);
        let guards = self.condition_guards(uc, &a.params, groups, group);
        a.guards.extend(guards);
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
            let e = self.ent_of[&ce.entity];
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
            match self.effect_value(&domain, &ce.value, &site, ce.entity) {
                EffectRhs::Value(v) => self.assign(a, uc, ce.entity, var, p, Rhs::Lit(v), site),
                EffectRhs::AnyNonNull => {
                    let values = (1..domain.size(self.bounds))
                        .map(|i| Value(i as u8))
                        .collect();
                    a.effects.push(Effect::Choose {
                        var,
                        param: p,
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
            let e = self.ent_of[&pe.entity];
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

    /// A use case that drives a comparison and also sets every Int operand of it to a literal
    /// must drive the truth those literals give. Otherwise the model contradicts itself.
    fn check_driven_truth(&mut self, uc: UseCaseKey, idx: usize, site: &Site) {
        let model = self.model;
        let pe = &model.proposition_effects[idx];
        let literal = |column: &str| {
            model.column_effects.iter().enumerate().find_map(|(i, ce)| {
                match (ce.origin == uc && ce.entity == pe.entity && ce.column == column)
                    .then(|| ce.value.normalize())
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
            CmpRhs::Now => return,
        };
        let holds = match pe.prop.op {
            CmpOpModel::Lt => lhs < rhs,
            CmpOpModel::Gt => lhs > rhs,
            CmpOpModel::Le => lhs <= rhs,
            CmpOpModel::Ge => lhs >= rhs,
            CmpOpModel::Eq => lhs == rhs,
            CmpOpModel::Ne => lhs != rhs,
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
    /// and its guard is that the column holds one of the `from` values.
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
            let existing = a.effects[first..].iter_mut().find_map(|eff| match eff {
                Effect::Transition {
                    var: v,
                    arms,
                    sites,
                    ..
                } if *v == var => Some((arms, sites)),
                _ => None,
            });
            match existing {
                Some((arms, sites)) => {
                    if !arms.iter().any(|(f, _)| *f == arm.0) {
                        arms.push(arm);
                        sites.push(site);
                    }
                }
                None => a.effects.push(Effect::Transition {
                    var,
                    param: p,
                    arms: vec![arm],
                    sites: vec![site],
                }),
            }
        }
        for eff in &a.effects[first..] {
            let Effect::Transition {
                var,
                param,
                arms,
                sites,
            } = eff
            else {
                continue;
            };
            let at = At::Param(*param);
            let pred = Pred::Or(
                arms.iter()
                    .map(|(from, _)| {
                        Pred::Cmp(Term::Slot { var: *var, at }, CmpOp::Eq, Term::Lit(*from))
                    })
                    .collect(),
            );
            a.guards.push(Guard {
                pred,
                why: GuardWhy::TransitionFrom,
                site: sites[0].clone(),
            });
        }
    }

    /// Vision rule 5: an actor chooses a variation column's value, so a created instance takes
    /// any value unless the creating use case sets it. `@default` does not pin the choice.
    fn creation_choices(&mut self, a: &mut ActionDraft) {
        for p in 0..a.params.len() {
            let Param { entity, role } = a.params[p];
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
                });
            }
        }
    }

    /// R-bind: a created child belongs to its parent, a present one unless the relate is
    /// optional; a child the action updates must already belong to a parent the action only
    /// updates.
    ///
    /// Relink: an action that creates the parent and updates an existing child links the child.
    /// For an optional key the child must be unlinked, so a linked child is not relinked; a
    /// mandatory child always has a parent, so the link is a move.
    fn owner_bindings(&mut self, a: &mut ActionDraft) {
        for &(var, child, parent) in &self.owners.clone() {
            let Some(c) = a.param_of(child) else { continue };
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
                let rhs = match a.param_of(parent) {
                    Some(p) => Rhs::Param(p),
                    None if optional => Rhs::Lit(Value::NULL),
                    None => Rhs::Param(a.param(parent, ParamRole::Parent)),
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
        self.params.push(Param { entity, role });
        (self.params.len() - 1) as u8
    }

    fn param_of(&self, entity: EntId) -> Option<u8> {
        self.params
            .iter()
            .position(|p| p.entity == entity)
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
fn variant_index(values: &[String], name: &str) -> Option<u8> {
    values
        .iter()
        .position(|v| v == name)
        .or_else(|| values.iter().position(|v| v.eq_ignore_ascii_case(name)))
        .map(|i| i as u8)
}
