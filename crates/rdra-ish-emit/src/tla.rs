//! TLA+ / TLC export from entity lifecycles and state constraints.

use crate::{EmitError, Emitter, View};
use rdra_ish_core::model::{
    CmpOpModel, CmpRhs, ColumnType, ComparisonProp, CrossCmpRhs, CrossConstraintScope,
    CrossEntityCondition, CrossEntityInvariant, EffectValue, EntityInvariant, EntityKey,
    ExclusiveConstraint, ForbiddenConstraint, NodeRef, QuantifierConstraint, QuantifierKind,
    RelKind, RequiredConstraint, SemanticModel, TemporalAtom, TemporalExpr, TemporalFormula,
    TemporalProperty, TemporalRhs,
};
use rdra_ish_core::{collect_entity_lifecycles, EntityLifecycle};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

const INSTANCE_COUNT: i64 = 2;
const INT_RANGE_MAX: i64 = 5;

/// Bundle written by `export --kind tla` / `verify --backend tlc`.
#[derive(Debug, Clone)]
pub struct TlaBundle {
    pub module_name: String,
    pub tla: String,
    pub cfg: String,
    /// Export approximations / skipped mappings (also embedded as `\\* WARNING:` in `.tla`).
    pub warnings: Vec<String>,
}

pub struct TlaPlusEmitter {
    /// When true, emit Bool / Nullable / Proposition axes alongside status (Phase 1.5).
    pub multi_axis: bool,
}

impl Default for TlaPlusEmitter {
    fn default() -> Self {
        Self { multi_axis: true }
    }
}

impl Emitter for TlaPlusEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        Ok(self.emit_bundle(model, view)?.tla)
    }
}

impl TlaPlusEmitter {
    pub fn emit_bundle(&self, model: &SemanticModel, view: &View) -> Result<TlaBundle, EmitError> {
        self.emit_bundle_named(model, view, "RdraSpec")
    }

    pub fn emit_bundle_named(
        &self,
        model: &SemanticModel,
        _view: &View,
        module_name: &str,
    ) -> Result<TlaBundle, EmitError> {
        let module_name = sanitize_module_name(module_name);
        let mut warnings = Vec::new();
        let export = build_export(model, self.multi_axis, &mut warnings);
        let tla = render_tla(&module_name, &export, &warnings);
        let cfg = render_cfg(&module_name, &export);
        Ok(TlaBundle {
            module_name,
            tla,
            cfg,
            warnings,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AxisKind {
    Status,
    Bool,
    Nullable,
    Int,
    /// Boolean proposition axis for non-arithmetic comparisons.
    Prop,
}

#[derive(Debug, Clone)]
struct SpecAxis {
    /// TLA variable name (prefixed).
    var_name: String,
    /// Source column / proposition key.
    column: String,
    domain: String,
    kind: AxisKind,
}

#[derive(Debug, Clone)]
struct SpecAction {
    name: String,
    comment: String,
    /// Optional `\E i \in Entity_Ids:` binder for multi-instance actions.
    exists_binder: Option<(String, String)>,
    /// Conjunction of guards (TLA fragments).
    guards: Vec<String>,
    /// Conjunction of `var' = value` effects.
    effects: Vec<String>,
    /// Unchanged vars (UNCHANGED <<...>>).
    unchanged: Vec<String>,
}

#[derive(Debug, Clone)]
struct EntitySpec {
    entity_key: EntityKey,
    entity_id: String,
    binder: String,
    axes: Vec<SpecAxis>,
    init: Vec<(String, String)>,
    actions: Vec<SpecAction>,
    invariants: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct OwnerLink {
    child_id: String,
    parent_id: String,
    var_name: String,
}

#[derive(Debug, Clone)]
struct TlaExport {
    multi_instance: bool,
    needs_integers: bool,
    needs_now: bool,
    needs_wf: bool,
    specs: Vec<EntitySpec>,
    owners: Vec<OwnerLink>,
    /// Cross / quantifier Safety conjuncts (name, formula).
    global_safety: Vec<(String, String)>,
    /// Temporal properties (name, formula) — single source for `.tla` and `.cfg`.
    properties: Vec<(String, String)>,
}

fn needs_multi_instance(model: &SemanticModel) -> bool {
    !model.cross_forbidden_constraints.is_empty()
        || !model.cross_entity_invariants.is_empty()
        || !model.quantifier_constraints.is_empty()
}

fn needs_wf(model: &SemanticModel) -> bool {
    model.temporal_properties.iter().any(|p| {
        matches!(
            p.formula,
            TemporalFormula::Eventually(_) | TemporalFormula::LeadsTo { .. }
        )
    })
}

fn build_export(model: &SemanticModel, multi_axis: bool, warnings: &mut Vec<String>) -> TlaExport {
    let lifecycles = collect_entity_lifecycles(model);
    let multi_instance = needs_multi_instance(model);
    let int_now = collect_int_now_usage(model, &lifecycles);

    let binders = assign_binders(&lifecycles, model);
    let mut specs = Vec::new();
    for lc in &lifecycles {
        if let Some(spec) = build_one_entity(
            model,
            lc,
            multi_axis,
            multi_instance,
            &int_now,
            &binders,
            warnings,
        ) {
            specs.push(spec);
        }
    }

    if specs.is_empty() && !model.temporal_properties.is_empty() {
        warnings.push(
            "temporal properties declared but no entity lifecycle (status Enum + states) found"
                .into(),
        );
    }

    let owners = if multi_instance {
        collect_owner_links(model)
    } else {
        Vec::new()
    };

    let assertion_props = apply_temporal_assertions(model, &specs, multi_instance, warnings);

    let mut global_safety = Vec::new();
    if multi_instance {
        emit_cross_and_quantifier_safety(model, &specs, &owners, &mut global_safety, warnings);
    }

    note_skipped_constraints(model, &lifecycles, multi_instance, warnings);

    let needs_integers = int_now.needs_integers
        || specs
            .iter()
            .flat_map(|s| s.axes.iter())
            .any(|a| a.kind == AxisKind::Int);
    let needs_now = int_now.needs_now;

    let mut properties = build_all_temporal_properties(model, &specs, multi_instance, warnings);
    properties.extend(assertion_props);

    TlaExport {
        multi_instance,
        needs_integers: needs_integers || needs_now,
        needs_now,
        needs_wf: needs_wf(model),
        specs,
        owners,
        global_safety,
        properties,
    }
}

#[derive(Debug, Default)]
struct IntNowUsage {
    /// Entity id → Int/now-promoted column names.
    int_columns: BTreeMap<String, BTreeSet<String>>,
    /// Comparison axis keys that are expressed as Int arithmetic (skip prop axes).
    arithmetic_cmp_keys: HashSet<String>,
    needs_integers: bool,
    needs_now: bool,
}

fn mark_int_column(usage: &mut IntNowUsage, entity_id: &str, col: &str) {
    usage.needs_integers = true;
    usage
        .int_columns
        .entry(entity_id.to_string())
        .or_default()
        .insert(col.to_string());
}

fn consider_entity_cmp(
    usage: &mut IntNowUsage,
    model: &SemanticModel,
    ek: EntityKey,
    c: &ComparisonProp,
) {
    let eid = model.entities[ek].id.clone();
    let entity = &model.entities[ek];
    if !comparison_is_arithmetic(entity, c) {
        return;
    }
    usage
        .arithmetic_cmp_keys
        .insert(format!("{}:{}", eid, c.axis_key()));
    mark_int_column(usage, &eid, &c.lhs_column);
    match &c.rhs {
        CmpRhs::Column(other) => mark_int_column(usage, &eid, other),
        CmpRhs::IntLit(_) => {}
        CmpRhs::Now => {
            usage.needs_now = true;
            usage.needs_integers = true;
        }
    }
}

fn collect_int_now_usage(model: &SemanticModel, lifecycles: &[EntityLifecycle]) -> IntNowUsage {
    let mut usage = IntNowUsage::default();
    let lifecycle_entities: HashSet<EntityKey> = lifecycles.iter().map(|l| l.entity).collect();

    // sets(..., col == N) Int effects.
    for effect in &model.column_effects {
        if !lifecycle_entities.contains(&effect.entity) {
            continue;
        }
        if matches!(effect.value, EffectValue::Int(_)) {
            let eid = model.entities[effect.entity].id.clone();
            mark_int_column(&mut usage, &eid, &effect.column);
        }
    }

    for inv in &model.entity_invariants {
        for c in inv
            .guard_comparisons
            .iter()
            .chain(inv.required_comparisons.iter())
        {
            consider_entity_cmp(&mut usage, model, inv.entity, c);
        }
    }
    for f in &model.forbidden_constraints {
        for c in &f.comparisons {
            consider_entity_cmp(&mut usage, model, f.entity, c);
        }
    }
    for r in &model.required_constraints {
        for c in &r.comparisons {
            consider_entity_cmp(&mut usage, model, r.entity, c);
        }
    }
    for e in &model.exclusive_constraints {
        for c in &e.comparisons {
            consider_entity_cmp(&mut usage, model, e.entity, c);
        }
    }

    // Cross-entity comparisons involving Int / now.
    let cross_conds: Vec<&CrossEntityCondition> = model
        .cross_forbidden_constraints
        .iter()
        .flat_map(|c| c.conditions.iter())
        .chain(
            model
                .cross_entity_invariants
                .iter()
                .flat_map(|c| c.guards.iter().chain(c.requireds.iter())),
        )
        .chain(
            model
                .quantifier_constraints
                .iter()
                .flat_map(|c| c.guards.iter().chain(c.related_conditions.iter())),
        )
        .chain(
            model
                .temporal_assertions
                .iter()
                .flat_map(|a| a.requireds.iter()),
        )
        .collect();

    for cond in cross_conds {
        if let CrossEntityCondition::Comparison(cmp) = cond {
            let lhs_ent = &model.entities[cmp.lhs.entity];
            if column_is_int_like(lhs_ent, &cmp.lhs.column) || matches!(cmp.rhs, CrossCmpRhs::Now) {
                mark_int_column(&mut usage, &lhs_ent.id, &cmp.lhs.column);
                match &cmp.rhs {
                    CrossCmpRhs::Column(r) => {
                        let rent = &model.entities[r.entity];
                        if column_is_int_like(rent, &r.column) {
                            mark_int_column(&mut usage, &rent.id, &r.column);
                        }
                    }
                    CrossCmpRhs::IntLit(_) => {}
                    CrossCmpRhs::Now => {
                        usage.needs_now = true;
                        usage.needs_integers = true;
                    }
                }
            }
            if matches!(cmp.rhs, CrossCmpRhs::Now) {
                // DateTime vs now → promote lhs to Int axis.
                mark_int_column(&mut usage, &lhs_ent.id, &cmp.lhs.column);
                usage.needs_now = true;
                usage.needs_integers = true;
            }
        }
    }

    // Temporal property atoms.
    let mut int_refs: Vec<(String, String)> = Vec::new();
    for prop in &model.temporal_properties {
        walk_temporal_for_int(model, &prop.formula, &mut |entity_id, col| {
            int_refs.push((entity_id.to_string(), col.to_string()));
        });
    }
    for (entity_id, col) in int_refs {
        if let Some(ek) = find_entity_key(model, &entity_id) {
            let ent = &model.entities[ek];
            if column_is_int_like(ent, &col) {
                mark_int_column(&mut usage, &entity_id, &col);
            }
        }
    }

    usage
}

fn walk_temporal_for_int(
    model: &SemanticModel,
    formula: &TemporalFormula,
    f: &mut dyn FnMut(&str, &str),
) {
    fn walk_expr(expr: &TemporalExpr, f: &mut dyn FnMut(&str, &str)) {
        match expr {
            TemporalExpr::Atom(TemporalAtom {
                entity,
                column,
                rhs,
                ..
            }) => {
                if let Some(e) = entity {
                    f(e, column);
                }
                if let TemporalRhs::Column {
                    entity: Some(e),
                    column: c,
                } = rhs
                {
                    f(e, c);
                }
            }
            TemporalExpr::Not(inner) => walk_expr(inner, f),
            TemporalExpr::And(a, b) | TemporalExpr::Or(a, b) => {
                walk_expr(a, f);
                walk_expr(b, f);
            }
        }
    }
    let _ = model;
    match formula {
        TemporalFormula::Always(e) | TemporalFormula::Eventually(e) => walk_expr(e, f),
        TemporalFormula::LeadsTo {
            antecedent,
            consequent,
        } => {
            walk_expr(antecedent, f);
            walk_expr(consequent, f);
        }
    }
}

fn comparison_is_arithmetic(entity: &rdra_ish_core::model::Entity, c: &ComparisonProp) -> bool {
    if matches!(c.rhs, CmpRhs::Now) {
        return true;
    }
    if matches!(c.rhs, CmpRhs::IntLit(_)) {
        return column_is_int_like(entity, &c.lhs_column)
            || column_promotable_to_int(entity, &c.lhs_column);
    }
    if column_is_int_like(entity, &c.lhs_column) {
        return true;
    }
    if let CmpRhs::Column(other) = &c.rhs {
        return column_is_int_like(entity, other);
    }
    false
}

fn column_is_int_like(entity: &rdra_ish_core::model::Entity, col: &str) -> bool {
    entity
        .columns
        .iter()
        .find(|c| c.name == col)
        .is_some_and(|c| {
            matches!(
                c.col_type,
                ColumnType::Int | ColumnType::Money | ColumnType::Decimal
            )
        })
}

fn column_promotable_to_int(entity: &rdra_ish_core::model::Entity, col: &str) -> bool {
    entity
        .columns
        .iter()
        .find(|c| c.name == col)
        .is_some_and(|c| matches!(c.col_type, ColumnType::DateTime | ColumnType::Date))
}

fn find_entity_key(model: &SemanticModel, id: &str) -> Option<EntityKey> {
    model
        .entities
        .iter()
        .find_map(|(k, e)| (e.id == id).then_some(k))
}

fn assign_binders(
    lifecycles: &[EntityLifecycle],
    model: &SemanticModel,
) -> HashMap<EntityKey, String> {
    let mut used = HashSet::new();
    let mut out = HashMap::new();
    for lc in lifecycles {
        let id = &model.entities[lc.entity].id;
        let mut candidate = id
            .chars()
            .next()
            .map(|c| c.to_ascii_lowercase().to_string())
            .unwrap_or_else(|| "e".into());
        if !used.insert(candidate.clone()) {
            candidate = sanitize_ident(&id.to_ascii_lowercase());
            used.insert(candidate.clone());
        }
        out.insert(lc.entity, candidate);
    }
    out
}

fn build_one_entity(
    model: &SemanticModel,
    lc: &EntityLifecycle,
    multi_axis: bool,
    multi_instance: bool,
    int_now: &IntNowUsage,
    binders: &HashMap<EntityKey, String>,
    warnings: &mut Vec<String>,
) -> Option<EntitySpec> {
    let entity = &model.entities[lc.entity];
    let entity_id = entity.id.clone();
    let binder = binders
        .get(&lc.entity)
        .cloned()
        .unwrap_or_else(|| "e".into());

    let axes = collect_spec_axes(model, lc, multi_axis, int_now);
    if axes.is_empty() {
        return None;
    }

    let init = build_init(model, lc, &axes, multi_instance, &binder, int_now, warnings);
    let actions = build_actions(model, lc, &axes, multi_instance, &binder, warnings);
    let mut actions = actions;
    append_undriven_int_assigns(
        model,
        lc,
        &axes,
        multi_instance,
        &binder,
        int_now,
        &mut actions,
    );

    let invariants = build_invariants(
        model,
        lc.entity,
        &axes,
        int_now,
        multi_instance,
        &binder,
        warnings,
    );

    Some(EntitySpec {
        entity_key: lc.entity,
        entity_id,
        binder,
        axes,
        init,
        actions,
        invariants,
    })
}

fn collect_spec_axes(
    model: &SemanticModel,
    lc: &EntityLifecycle,
    multi_axis: bool,
    int_now: &IntNowUsage,
) -> Vec<SpecAxis> {
    let entity = &model.entities[lc.entity];
    let prefix = sanitize_ident(&entity.id);
    let mut axes = Vec::new();
    let mut seen_cols: HashSet<String> = HashSet::new();

    // Status Enum axis from lifecycle.
    if let Some(col) = entity.columns.iter().find(|c| c.name == lc.status_column) {
        if let ColumnType::Enum(variants) = &col.col_type {
            let domain = format!(
                "{{{}}}",
                variants
                    .iter()
                    .map(|v| format!("\"{}\"", escape_tla_string(v)))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            axes.push(SpecAxis {
                var_name: format!("{prefix}_{}", sanitize_ident(&lc.status_column)),
                column: lc.status_column.clone(),
                domain,
                kind: AxisKind::Status,
            });
            seen_cols.insert(lc.status_column.clone());
        }
    }

    if !multi_axis {
        return axes;
    }

    let status_col = &lc.status_column;
    for col in &entity.columns {
        if col.name == *status_col || seen_cols.contains(&col.name) {
            continue;
        }
        // Columns promoted to IntRange must not become Nullable/Bool axes first.
        if int_now
            .int_columns
            .get(&entity.id)
            .is_some_and(|cols| cols.contains(&col.name))
        {
            continue;
        }
        match &col.col_type {
            ColumnType::Bool => {
                axes.push(SpecAxis {
                    var_name: format!("{prefix}_{}", sanitize_ident(&col.name)),
                    column: col.name.clone(),
                    domain: "BOOLEAN".into(),
                    kind: AxisKind::Bool,
                });
                seen_cols.insert(col.name.clone());
            }
            _ if col.is_nullable => {
                axes.push(SpecAxis {
                    var_name: format!("{prefix}_{}", sanitize_ident(&col.name)),
                    column: col.name.clone(),
                    domain: "{\"null\", \"present\"}".into(),
                    kind: AxisKind::Nullable,
                });
                seen_cols.insert(col.name.clone());
            }
            _ => {}
        }
    }

    // Int / now-promoted axes.
    if let Some(cols) = int_now.int_columns.get(&entity.id) {
        for col_name in cols {
            if seen_cols.contains(col_name) || col_name == status_col {
                continue;
            }
            // Skip pure PK id unless it was explicitly marked (rare).
            if let Some(col) = entity.columns.iter().find(|c| c.name == *col_name) {
                if col.is_pk
                    && !int_now
                        .arithmetic_cmp_keys
                        .iter()
                        .any(|k| k.contains(col_name))
                {
                    // Still allow if explicitly in int_columns from sets/cmp.
                }
            }
            axes.push(SpecAxis {
                var_name: format!("{prefix}_{}", sanitize_ident(col_name)),
                column: col_name.clone(),
                domain: "IntRange".into(),
                kind: AxisKind::Int,
            });
            seen_cols.insert(col_name.clone());
        }
    }

    // Proposition axes for non-arithmetic comparisons.
    let mut prop_keys: BTreeSet<String> = BTreeSet::new();
    let mut maybe_prop = |ek: EntityKey, c: &ComparisonProp| {
        if ek != lc.entity {
            return;
        }
        let key = format!("{}:{}", entity.id, c.axis_key());
        if int_now.arithmetic_cmp_keys.contains(&key) {
            return;
        }
        prop_keys.insert(c.axis_key());
    };

    for inv in model
        .entity_invariants
        .iter()
        .filter(|i| i.entity == lc.entity)
    {
        for c in inv
            .guard_comparisons
            .iter()
            .chain(inv.required_comparisons.iter())
        {
            maybe_prop(inv.entity, c);
        }
    }
    for f in model
        .forbidden_constraints
        .iter()
        .filter(|f| f.entity == lc.entity)
    {
        for c in &f.comparisons {
            maybe_prop(f.entity, c);
        }
    }
    for r in model
        .required_constraints
        .iter()
        .filter(|r| r.entity == lc.entity)
    {
        for c in &r.comparisons {
            maybe_prop(r.entity, c);
        }
    }
    for e in model
        .exclusive_constraints
        .iter()
        .filter(|e| e.entity == lc.entity)
    {
        for c in &e.comparisons {
            maybe_prop(e.entity, c);
        }
    }
    for pe in model
        .proposition_effects
        .iter()
        .filter(|pe| pe.entity == lc.entity)
    {
        let key = format!("{}:{}", entity.id, pe.prop.axis_key());
        if !int_now.arithmetic_cmp_keys.contains(&key) {
            prop_keys.insert(pe.prop.axis_key());
        }
    }

    for key in prop_keys {
        let var = format!(
            "{prefix}_prop_{}",
            sanitize_ident(&key.replace(['<', '>', '=', '!'], "_"))
        );
        axes.push(SpecAxis {
            var_name: var,
            column: format!("__cmp:{key}"),
            domain: "BOOLEAN".into(),
            kind: AxisKind::Prop,
        });
    }

    axes
}

fn build_init(
    model: &SemanticModel,
    lc: &EntityLifecycle,
    axes: &[SpecAxis],
    multi_instance: bool,
    binder: &str,
    int_now: &IntNowUsage,
    warnings: &mut Vec<String>,
) -> Vec<(String, String)> {
    let entity = &model.entities[lc.entity];
    let mut init = Vec::new();

    let entity_name = model.entities[lc.entity].id.clone();
    let prefix = format!("{entity_name}_");
    let resolved = lc
        .initial
        .first()
        .map(|sk| {
            let id = &model.states[*sk].id;
            id.strip_prefix(&prefix).unwrap_or(id).to_lowercase()
        })
        .or_else(|| {
            entity
                .columns
                .iter()
                .find(|c| c.name == lc.status_column)
                .and_then(|c| c.default_val.as_ref().map(|d| d.to_lowercase()))
        })
        .or_else(|| {
            entity
                .columns
                .iter()
                .find(|c| c.name == lc.status_column)
                .and_then(|c| match &c.col_type {
                    ColumnType::Enum(variants) => variants.first().map(|v| v.to_lowercase()),
                    _ => None,
                })
        });
    let initial_status = match resolved {
        Some(status) => status,
        None => {
            let status_col = entity.columns.iter().find(|c| c.name == lc.status_column);
            let pending_ok = match status_col.map(|c| &c.col_type) {
                Some(ColumnType::Enum(variants)) => {
                    variants.iter().any(|v| v.eq_ignore_ascii_case("pending"))
                }
                _ => false,
            };
            if !pending_ok {
                push_tla_fatal(
                    warnings,
                    "init_pending_fallback",
                    format!(
                        "{entity_name}: Init fell back to \"pending\" but that is not a declared status; declare initial / default / Enum variants"
                    ),
                );
            }
            "pending".into()
        }
    };

    let ids = format!("{}_Ids", sanitize_ident(&entity_name));

    for ax in axes {
        let scalar = init_scalar_value(entity, lc, ax, &initial_status, int_now);
        let value = if multi_instance {
            format!("[{binder} \\in {ids} |-> {scalar}]")
        } else {
            scalar
        };
        init.push((ax.var_name.clone(), value));
    }
    init
}

fn init_scalar_value(
    entity: &rdra_ish_core::model::Entity,
    lc: &EntityLifecycle,
    ax: &SpecAxis,
    initial_status: &str,
    _int_now: &IntNowUsage,
) -> String {
    if ax.column == lc.status_column {
        return format!("\"{}\"", escape_tla_string(initial_status));
    }
    if ax.column.starts_with("__cmp:") {
        return "FALSE".into();
    }
    match ax.kind {
        AxisKind::Bool => entity
            .columns
            .iter()
            .find(|c| c.name == ax.column)
            .and_then(|c| c.default_val.as_ref())
            .map(|d| {
                if d.eq_ignore_ascii_case("true") {
                    "TRUE".into()
                } else {
                    "FALSE".into()
                }
            })
            .unwrap_or_else(|| "FALSE".into()),
        AxisKind::Int => {
            let from_default = entity
                .columns
                .iter()
                .find(|c| c.name == ax.column)
                .and_then(|c| c.default_val.as_ref())
                .and_then(|d| d.parse::<i64>().ok())
                .map(|n| n.to_string());
            from_default.unwrap_or_else(|| "0".into())
        }
        AxisKind::Nullable | AxisKind::Status | AxisKind::Prop => entity
            .columns
            .iter()
            .find(|c| c.name == ax.column)
            .and_then(|c| c.default_val.as_ref())
            .map(|d| {
                if d.eq_ignore_ascii_case("null") {
                    "\"null\"".into()
                } else if ax.kind == AxisKind::Nullable {
                    "\"present\"".into()
                } else {
                    format!("\"{}\"", escape_tla_string(&d.to_lowercase()))
                }
            })
            .unwrap_or_else(|| {
                if ax.kind == AxisKind::Nullable {
                    "\"null\"".into()
                } else {
                    "FALSE".into()
                }
            }),
    }
}

fn build_actions(
    model: &SemanticModel,
    lc: &EntityLifecycle,
    axes: &[SpecAxis],
    multi_instance: bool,
    binder: &str,
    warnings: &mut Vec<String>,
) -> Vec<SpecAction> {
    let mut actions: Vec<SpecAction> = Vec::new();
    let col_to_var: HashMap<&str, &str> = axes
        .iter()
        .map(|a| (a.column.as_str(), a.var_name.as_str()))
        .collect();

    let all_vars: Vec<String> = axes.iter().map(|a| a.var_name.clone()).collect();
    let entity_name = entity_id(model, lc.entity);
    let ids = format!("{}_Ids", sanitize_ident(&entity_name));

    for st in &lc.transitions {
        let from = st.from.to_lowercase();
        let to = st.to.to_lowercase();
        let event_id = model.events[st.event].id.clone();
        let status_var = col_to_var
            .get(lc.status_column.as_str())
            .copied()
            .unwrap_or("status");

        let mut extra_effects: Vec<(String, String)> = Vec::new();
        let raising_ucs: Vec<_> = model
            .relations
            .iter()
            .filter_map(|rel| {
                if rel.kind != RelKind::Raises {
                    return None;
                }
                match (&rel.from, &rel.to) {
                    (NodeRef::UseCase(u), NodeRef::Event(e)) if *e == st.event => Some(*u),
                    _ => None,
                }
            })
            .collect();

        for uc in &raising_ucs {
            for effect in model.column_effects.iter().filter(|e| {
                e.entity == lc.entity
                    && matches!(&e.origin, NodeRef::UseCase(u) if u == uc)
                    && e.column != lc.status_column
            }) {
                if let Some(var) = col_to_var.get(effect.column.as_str()) {
                    extra_effects.push(((*var).to_string(), effect_value_to_tla(&effect.value)));
                } else {
                    warnings.push(format!(
                        "{}: sets effect on non-axis column {} not exported",
                        entity_id(model, lc.entity),
                        effect.column
                    ));
                }
            }
            for pe in model.proposition_effects.iter().filter(|e| {
                e.entity == lc.entity && matches!(&e.origin, NodeRef::UseCase(u) if u == uc)
            }) {
                let key = format!("__cmp:{}", pe.prop.axis_key());
                if let Some(var) = col_to_var.get(key.as_str()) {
                    extra_effects.push((
                        (*var).to_string(),
                        if pe.truth {
                            "TRUE".into()
                        } else {
                            "FALSE".into()
                        },
                    ));
                }
            }
        }

        // sets(Event, ...) on the transition event itself.
        for effect in model.column_effects.iter().filter(|e| {
            e.entity == lc.entity
                && matches!(&e.origin, NodeRef::Event(ev) if *ev == st.event)
                && e.column != lc.status_column
        }) {
            if let Some(var) = col_to_var.get(effect.column.as_str()) {
                extra_effects.push(((*var).to_string(), effect_value_to_tla(&effect.value)));
            } else {
                warnings.push(format!(
                    "{}: sets effect on non-axis column {} not exported",
                    entity_id(model, lc.entity),
                    effect.column
                ));
            }
        }
        for pe in model.proposition_effects.iter().filter(|e| {
            e.entity == lc.entity && matches!(&e.origin, NodeRef::Event(ev) if *ev == st.event)
        }) {
            let key = format!("__cmp:{}", pe.prop.axis_key());
            if let Some(var) = col_to_var.get(key.as_str()) {
                extra_effects.push((
                    (*var).to_string(),
                    if pe.truth {
                        "TRUE".into()
                    } else {
                        "FALSE".into()
                    },
                ));
            }
        }

        let mut effects = Vec::new();
        let mut changed: BTreeSet<String> = BTreeSet::new();

        if multi_instance {
            effects.push(format!(
                "{status_var}' = [{status_var} EXCEPT ![{binder}] = \"{}\"]",
                escape_tla_string(&to)
            ));
            changed.insert(status_var.to_string());
            for (var, val) in &extra_effects {
                effects.push(format!("{var}' = [{var} EXCEPT ![{binder}] = {val}]"));
                changed.insert(var.clone());
            }
        } else {
            effects.push(format!("{status_var}' = \"{}\"", escape_tla_string(&to)));
            changed.insert(status_var.to_string());
            for (var, val) in &extra_effects {
                effects.push(format!("{var}' = {val}"));
                changed.insert(var.clone());
            }
        }

        let unchanged: Vec<String> = all_vars
            .iter()
            .filter(|v| !changed.contains(*v))
            .cloned()
            .collect();

        let action_name = {
            let base = format!(
                "{}_{}",
                sanitize_ident(&entity_id(model, lc.entity)),
                sanitize_ident(&event_id)
            );
            // Same event can drive multiple from→to edges; disambiguate operator names.
            let candidate = format!(
                "{}_{}_to_{}",
                base,
                sanitize_ident(&from),
                sanitize_ident(&to)
            );
            if actions.iter().any(|a| a.name == candidate) {
                format!("{}_{}", candidate, actions.len())
            } else {
                candidate
            }
        };

        let status_guard = if multi_instance {
            format!("{status_var}[{binder}] = \"{}\"", escape_tla_string(&from))
        } else {
            format!("{status_var} = \"{}\"", escape_tla_string(&from))
        };

        actions.push(SpecAction {
            name: action_name,
            comment: format!("{event_id}: {from} -> {to}"),
            exists_binder: multi_instance.then(|| (binder.to_string(), ids.clone())),
            guards: vec![status_guard],
            effects,
            unchanged,
        });
    }

    if actions.is_empty() {
        append_sets_driven_actions(
            model,
            lc,
            &col_to_var,
            &all_vars,
            multi_instance,
            binder,
            &ids,
            warnings,
            &mut actions,
        );
    }

    if actions.is_empty() {
        push_tla_fatal(
            warnings,
            "stuttering_only",
            format!(
                "{}: lifecycle has no transitions; Next is stuttering only",
                entity_id(model, lc.entity)
            ),
        );
    }

    actions
}

/// When there are no status transitions, still emit actions for usecase/event `sets`.
#[allow(clippy::too_many_arguments)]
fn append_sets_driven_actions(
    model: &SemanticModel,
    lc: &EntityLifecycle,
    col_to_var: &HashMap<&str, &str>,
    all_vars: &[String],
    multi_instance: bool,
    binder: &str,
    ids: &str,
    warnings: &mut Vec<String>,
    actions: &mut Vec<SpecAction>,
) {
    type OriginEffects = BTreeMap<String, (String, Vec<(String, String)>)>;
    let mut by_origin: OriginEffects = BTreeMap::new();

    let push_effect =
        |origin_key: String, comment: String, var: String, val: String, map: &mut OriginEffects| {
            map.entry(origin_key)
                .or_insert_with(|| (comment, Vec::new()))
                .1
                .push((var, val));
        };

    for effect in model
        .column_effects
        .iter()
        .filter(|e| e.entity == lc.entity)
    {
        let Some(var) = col_to_var.get(effect.column.as_str()) else {
            warnings.push(format!(
                "{}: sets effect on non-axis column {} not exported",
                entity_id(model, lc.entity),
                effect.column
            ));
            continue;
        };
        let (key, comment) = match &effect.origin {
            NodeRef::UseCase(u) => {
                let id = model.use_cases[*u].id.clone();
                (format!("uc:{id}"), format!("sets via usecase {id}"))
            }
            NodeRef::Event(e) => {
                let id = model.events[*e].id.clone();
                (format!("ev:{id}"), format!("sets via event {id}"))
            }
            _ => continue,
        };
        push_effect(
            key,
            comment,
            (*var).to_string(),
            effect_value_to_tla(&effect.value),
            &mut by_origin,
        );
    }

    for pe in model
        .proposition_effects
        .iter()
        .filter(|e| e.entity == lc.entity)
    {
        let key_axis = format!("__cmp:{}", pe.prop.axis_key());
        let Some(var) = col_to_var.get(key_axis.as_str()) else {
            continue;
        };
        let (key, comment) = match &pe.origin {
            NodeRef::UseCase(u) => {
                let id = model.use_cases[*u].id.clone();
                (format!("uc:{id}"), format!("sets via usecase {id}"))
            }
            NodeRef::Event(e) => {
                let id = model.events[*e].id.clone();
                (format!("ev:{id}"), format!("sets via event {id}"))
            }
            _ => continue,
        };
        let val = if pe.truth {
            "TRUE".into()
        } else {
            "FALSE".into()
        };
        push_effect(key, comment, (*var).to_string(), val, &mut by_origin);
    }

    let entity_name = entity_id(model, lc.entity);
    for (origin_key, (comment, effects_src)) in by_origin {
        let mut effects = Vec::new();
        let mut changed: BTreeSet<String> = BTreeSet::new();
        for (var, val) in &effects_src {
            if multi_instance {
                effects.push(format!("{var}' = [{var} EXCEPT ![{binder}] = {val}]"));
            } else {
                effects.push(format!("{var}' = {val}"));
            }
            changed.insert(var.clone());
        }
        let unchanged: Vec<String> = all_vars
            .iter()
            .filter(|v| !changed.contains(*v))
            .cloned()
            .collect();
        let suffix = origin_key.replace(':', "_");
        let name = format!(
            "{}_{}",
            sanitize_ident(&entity_name),
            sanitize_ident(&suffix)
        );
        actions.push(SpecAction {
            name,
            comment,
            exists_binder: multi_instance.then(|| (binder.to_string(), ids.to_string())),
            guards: Vec::new(),
            effects,
            unchanged,
        });
    }
}

fn append_undriven_int_assigns(
    model: &SemanticModel,
    lc: &EntityLifecycle,
    axes: &[SpecAxis],
    multi_instance: bool,
    binder: &str,
    _int_now: &IntNowUsage,
    actions: &mut Vec<SpecAction>,
) {
    let driven: HashSet<&str> = model
        .column_effects
        .iter()
        .filter(|e| e.entity == lc.entity && matches!(e.value, EffectValue::Int(_)))
        .map(|e| e.column.as_str())
        .collect();

    let all_vars: Vec<String> = axes.iter().map(|a| a.var_name.clone()).collect();
    let entity_name = entity_id(model, lc.entity);
    let ids = format!("{}_Ids", sanitize_ident(&entity_name));

    for ax in axes.iter().filter(|a| a.kind == AxisKind::Int) {
        if driven.contains(ax.column.as_str()) {
            continue;
        }
        let unchanged: Vec<String> = all_vars
            .iter()
            .filter(|v| *v != &ax.var_name)
            .cloned()
            .collect();
        let name = format!("Assign_{}", sanitize_ident(&ax.var_name));
        // Allow Assign to choose any IntRange value so `forbidden(col < now)` Safety
        // can actually observe violations (do not bake the safety into Next).
        let (exists_binder, effects) = if multi_instance {
            let assign = format!(
                "\\E v \\in IntRange: {var}' = [{var} EXCEPT ![{binder}] = v]",
                var = ax.var_name
            );
            (Some((binder.to_string(), ids.clone())), vec![assign])
        } else {
            let assign = format!("\\E v \\in IntRange: {}' = v", ax.var_name);
            (None, vec![assign])
        };
        actions.push(SpecAction {
            name,
            comment: format!("nondet assign {}", ax.column),
            exists_binder,
            guards: Vec::new(),
            effects,
            unchanged,
        });
    }
}

fn build_invariants(
    model: &SemanticModel,
    ek: EntityKey,
    axes: &[SpecAxis],
    int_now: &IntNowUsage,
    multi_instance: bool,
    binder: &str,
    warnings: &mut Vec<String>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let eid = entity_id(model, ek);
    let indexed: HashMap<String, String> = if multi_instance {
        axes.iter()
            .map(|a| (a.column.clone(), format!("{}[{binder}]", a.var_name)))
            .collect()
    } else {
        HashMap::new()
    };
    let col_to_var: HashMap<&str, &str> = if multi_instance {
        indexed
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    } else {
        axes.iter()
            .map(|a| (a.column.as_str(), a.var_name.as_str()))
            .collect()
    };
    let axis_kinds: HashMap<&str, AxisKind> =
        axes.iter().map(|a| (a.column.as_str(), a.kind)).collect();
    let wrap = |formula: String| -> String {
        if multi_instance {
            format!(
                "\\A {binder} \\in {}_Ids: ({formula})",
                sanitize_ident(&eid)
            )
        } else {
            formula
        }
    };

    for (i, inv) in model
        .entity_invariants
        .iter()
        .filter(|inv| inv.entity == ek)
        .enumerate()
    {
        match invariant_to_tla(inv, &col_to_var, &axis_kinds, int_now, &eid) {
            Ok(formula) => {
                let name = format!("Inv_{eid}_{i}");
                out.push((sanitize_ident(&name), wrap(formula)));
            }
            Err(reason) => push_tla_fatal(
                warnings,
                "constraint_unexported",
                format!("{eid}: invariant not exported: {reason}"),
            ),
        }
    }

    for (i, f) in model
        .forbidden_constraints
        .iter()
        .filter(|f| f.entity == ek)
        .enumerate()
    {
        match forbidden_to_tla(f, &col_to_var, &axis_kinds, int_now, &eid) {
            Ok(formula) => {
                let name = format!("Forbidden_{eid}_{i}");
                out.push((sanitize_ident(&name), wrap(formula)));
            }
            Err(reason) => push_tla_fatal(
                warnings,
                "constraint_unexported",
                format!("{eid}: forbidden not exported: {reason}"),
            ),
        }
    }

    for (i, r) in model
        .required_constraints
        .iter()
        .filter(|r| r.entity == ek)
        .enumerate()
    {
        match required_to_tla(r, &col_to_var, &axis_kinds, int_now, &eid) {
            Ok(formula) => {
                let name = format!("Required_{eid}_{i}");
                out.push((sanitize_ident(&name), wrap(formula)));
            }
            Err(reason) => push_tla_fatal(
                warnings,
                "constraint_unexported",
                format!("{eid}: required not exported: {reason}"),
            ),
        }
    }

    for (i, ex) in model
        .exclusive_constraints
        .iter()
        .filter(|e| e.entity == ek)
        .enumerate()
    {
        match exclusive_to_tla(ex, &col_to_var, &axis_kinds, int_now, &eid) {
            Ok(formula) => {
                let name = format!("Exclusive_{eid}_{i}");
                out.push((sanitize_ident(&name), wrap(formula)));
            }
            Err(reason) => push_tla_fatal(
                warnings,
                "constraint_unexported",
                format!("{eid}: exclusive not exported: {reason}"),
            ),
        }
    }

    out
}

/// Lower every temporal property once against a global axis map (for `.tla` + `.cfg`).
fn build_all_temporal_properties(
    model: &SemanticModel,
    specs: &[EntitySpec],
    multi_instance: bool,
    warnings: &mut Vec<String>,
) -> Vec<(String, String)> {
    let mut lookup: HashMap<String, String> = HashMap::new();
    let mut bare_column_owners: HashMap<String, Vec<String>> = HashMap::new();
    let mut mentioned: Vec<(String, String, String)> = Vec::new(); // entity_id, binder, ids
    for spec in specs {
        let ids = format!("{}_Ids", sanitize_ident(&spec.entity_id));
        mentioned.push((spec.entity_id.clone(), spec.binder.clone(), ids));
        for ax in &spec.axes {
            let access = if multi_instance {
                format!("{}[{}]", ax.var_name, spec.binder)
            } else {
                ax.var_name.clone()
            };
            let col_key = ax.column.trim_start_matches("__cmp:").to_string();
            lookup.insert(format!("{}.{}", spec.entity_id, col_key), access);
            bare_column_owners
                .entry(col_key)
                .or_default()
                .push(spec.entity_id.clone());
        }
    }
    // Unqualified column keys only when unique across exported entities.
    for (col, owners) in &bare_column_owners {
        if let [only] = owners.as_slice() {
            if let Some(access) = lookup.get(&format!("{only}.{col}")).cloned() {
                lookup.insert(col.clone(), access);
            }
        }
    }

    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for prop in &model.temporal_properties {
        let name = sanitize_ident(&prop.id);
        if !seen.insert(name.clone()) {
            push_tla_fatal(
                warnings,
                "duplicate_property",
                format!(
                    "duplicate property id `{}` in TLA export; keeping first only",
                    prop.id
                ),
            );
            continue;
        }
        match temporal_property_to_tla(prop, &lookup) {
            Ok(mut formula) => {
                if multi_instance {
                    let binders: Vec<(String, String)> = mentioned
                        .iter()
                        .filter(|(eid, _, _)| property_mentions_entity(prop, eid))
                        .map(|(_, binder, ids)| (binder.clone(), ids.clone()))
                        .collect();
                    if !binders.is_empty() {
                        formula =
                            wrap_temporal_with_multi_binders(&prop.formula, formula, &binders);
                    }
                }
                out.push((name, formula));
            }
            Err(reason) => push_tla_fatal(
                warnings,
                "property_unexported",
                format!("property {}: not exported: {reason}", prop.id),
            ),
        }
    }
    out
}

/// Quantify multi-instance temporal formulas over one or more entity binders.
///
/// - `always(body)` → `[](\A b \in Ids, …: body)`
/// - `eventually(body)` → `\A b \in Ids, …: <>(body)`
/// - `leads_to(a, b)` → `\A b \in Ids, …: ((a) ~> (b))`
fn wrap_temporal_with_multi_binders(
    kind: &TemporalFormula,
    formula: String,
    binders: &[(String, String)],
) -> String {
    if binders.is_empty() {
        return formula;
    }
    let quant = binders
        .iter()
        .map(|(b, ids)| format!("{b} \\in {ids}"))
        .collect::<Vec<_>>()
        .join(", ");
    match kind {
        TemporalFormula::Always(_) => {
            if let Some(rest) = formula.strip_prefix("[](") {
                if let Some(body) = rest.strip_suffix(')') {
                    return format!("[](\\A {quant}: {body})");
                }
            }
            formula
        }
        TemporalFormula::Eventually(_) => {
            if let Some(rest) = formula.strip_prefix("<>(") {
                if let Some(body) = rest.strip_suffix(')') {
                    return format!("\\A {quant}: <>({body})");
                }
            }
            formula
        }
        TemporalFormula::LeadsTo { .. } => {
            format!("\\A {quant}: ({formula})")
        }
    }
}

fn property_mentions_entity(prop: &TemporalProperty, entity_id: &str) -> bool {
    fn walk(expr: &TemporalExpr, entity_id: &str) -> bool {
        match expr {
            TemporalExpr::Atom(TemporalAtom {
                entity: Some(e), ..
            }) => e == entity_id,
            TemporalExpr::Atom(TemporalAtom { entity: None, .. }) => true,
            TemporalExpr::Not(inner) => walk(inner, entity_id),
            TemporalExpr::And(a, b) | TemporalExpr::Or(a, b) => {
                walk(a, entity_id) || walk(b, entity_id)
            }
        }
    }
    match &prop.formula {
        TemporalFormula::Always(e) | TemporalFormula::Eventually(e) => walk(e, entity_id),
        TemporalFormula::LeadsTo {
            antecedent,
            consequent,
        } => walk(antecedent, entity_id) || walk(consequent, entity_id),
    }
}

fn collect_owner_links(model: &SemanticModel) -> Vec<OwnerLink> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for rel in &model.relations {
        let (child, parent) = match rel.kind {
            RelKind::RelateManyToOne => match (&rel.from, &rel.to) {
                (NodeRef::Entity(c), NodeRef::Entity(p)) => (*c, *p),
                _ => continue,
            },
            RelKind::RelateOneToMany => match (&rel.from, &rel.to) {
                (NodeRef::Entity(p), NodeRef::Entity(c)) => (*c, *p),
                _ => continue,
            },
            _ => continue,
        };
        let child_id = model.entities[child].id.clone();
        let parent_id = model.entities[parent].id.clone();
        let var_name = format!("{}_owner", sanitize_ident(&child_id));
        if seen.insert(var_name.clone()) {
            out.push(OwnerLink {
                child_id,
                parent_id,
                var_name,
            });
        }
    }
    out
}

fn emit_cross_and_quantifier_safety(
    model: &SemanticModel,
    specs: &[EntitySpec],
    owners: &[OwnerLink],
    global_safety: &mut Vec<(String, String)>,
    warnings: &mut Vec<String>,
) {
    let spec_by_key: HashMap<EntityKey, &EntitySpec> =
        specs.iter().map(|s| (s.entity_key, s)).collect();

    for (i, c) in model.cross_forbidden_constraints.iter().enumerate() {
        match cross_forbidden_to_tla(model, c, &spec_by_key, owners, warnings) {
            Ok(formula) => global_safety.push((format!("CrossForbidden_{i}"), formula)),
            Err(reason) => push_tla_fatal(
                warnings,
                "cross_unexported",
                format!("cross_forbidden not exported: {reason}"),
            ),
        }
    }
    for (i, inv) in model.cross_entity_invariants.iter().enumerate() {
        match cross_invariant_to_tla(model, inv, &spec_by_key, owners, warnings) {
            Ok(formula) => global_safety.push((format!("CrossInvariant_{i}"), formula)),
            Err(reason) => push_tla_fatal(
                warnings,
                "cross_unexported",
                format!("cross_invariant not exported: {reason}"),
            ),
        }
    }
    for (i, q) in model.quantifier_constraints.iter().enumerate() {
        match quantifier_to_tla(model, q, &spec_by_key, owners, warnings) {
            Ok(formula) => global_safety.push((format!("Quantifier_{i}"), formula)),
            Err(reason) => push_tla_fatal(
                warnings,
                "cross_unexported",
                format!("quantifier not exported: {reason}"),
            ),
        }
    }
}

fn owner_for_pair<'a>(owners: &'a [OwnerLink], a: &str, b: &str) -> Option<&'a OwnerLink> {
    owners
        .iter()
        .find(|o| (o.child_id == a && o.parent_id == b) || (o.child_id == b && o.parent_id == a))
}

fn cross_forbidden_to_tla(
    model: &SemanticModel,
    c: &rdra_ish_core::model::CrossForbiddenConstraint,
    specs: &HashMap<EntityKey, &EntitySpec>,
    owners: &[OwnerLink],
    warnings: &mut Vec<String>,
) -> Result<String, String> {
    let binders = binders_for_scope(model, &c.scope, specs)?;
    let body = cross_conditions_to_tla(model, &c.conditions, specs, &binders)?;
    let link = along_link_filter(model, &c.scope_semantics, &binders, owners, warnings);
    let quant = quantifiers_for_binders(&binders);
    if let Some(link) = link {
        Ok(format!("{quant}: ({link}) => ~({body})"))
    } else {
        Ok(format!("{quant}: ~({body})"))
    }
}

fn cross_invariant_to_tla(
    model: &SemanticModel,
    inv: &CrossEntityInvariant,
    specs: &HashMap<EntityKey, &EntitySpec>,
    owners: &[OwnerLink],
    warnings: &mut Vec<String>,
) -> Result<String, String> {
    let binders = binders_for_scope(model, &inv.scope, specs)?;
    let guards = cross_conditions_to_tla(model, &inv.guards, specs, &binders)?;
    let reqs = cross_conditions_to_tla(model, &inv.requireds, specs, &binders)?;
    let link = along_link_filter(model, &inv.scope_semantics, &binders, owners, warnings);
    let quant = quantifiers_for_binders(&binders);
    let impl_body = format!("({guards}) => ({reqs})");
    if let Some(link) = link {
        Ok(format!("{quant}: ({link}) => ({impl_body})"))
    } else {
        Ok(format!("{quant}: {impl_body}"))
    }
}

fn quantifier_to_tla(
    model: &SemanticModel,
    q: &QuantifierConstraint,
    specs: &HashMap<EntityKey, &EntitySpec>,
    owners: &[OwnerLink],
    warnings: &mut Vec<String>,
) -> Result<String, String> {
    let anchor = specs
        .get(&q.anchor)
        .ok_or_else(|| "anchor entity has no exportable lifecycle".to_string())?;
    let related = specs
        .get(&q.related)
        .ok_or_else(|| "related entity has no exportable lifecycle".to_string())?;
    let mut binders = BTreeMap::new();
    binders.insert(q.anchor, (anchor.binder.clone(), anchor.entity_id.clone()));
    binders.insert(
        q.related,
        (related.binder.clone(), related.entity_id.clone()),
    );

    let guards = cross_conditions_to_tla(model, &q.guards, specs, &binders)?;
    let related_body = cross_conditions_to_tla(model, &q.related_conditions, specs, &binders)?;

    let anchor_ids = format!("{}_Ids", sanitize_ident(&anchor.entity_id));
    let related_ids = format!("{}_Ids", sanitize_ident(&related.entity_id));
    let ab = &anchor.binder;
    let rb = &related.binder;

    let owner = owner_for_pair(owners, &anchor.entity_id, &related.entity_id);
    let link = if let Some(o) = owner {
        if o.child_id == related.entity_id {
            format!("{}[{rb}] = {ab}", o.var_name)
        } else {
            format!("{}[{ab}] = {rb}", o.var_name)
        }
    } else {
        warnings.push(format!(
            "quantifier on {}/{}: no relate N:1/1:N link; quantifying independently",
            anchor.entity_id, related.entity_id
        ));
        "TRUE".into()
    };

    match q.kind {
        QuantifierKind::None => Ok(format!(
            "\\A {ab} \\in {anchor_ids}: ({guards}) => (\\A {rb} \\in {related_ids}: ({link}) => ~({related_body}))"
        )),
        QuantifierKind::Has => Ok(format!(
            "\\A {ab} \\in {anchor_ids}: ({guards}) => (\\E {rb} \\in {related_ids}: ({link}) /\\ ({related_body}))"
        )),
    }
}

fn binders_for_scope(
    model: &SemanticModel,
    scope: &[EntityKey],
    specs: &HashMap<EntityKey, &EntitySpec>,
) -> Result<BTreeMap<EntityKey, (String, String)>, String> {
    let mut binders = BTreeMap::new();
    for &ek in scope {
        let spec = specs
            .get(&ek)
            .ok_or_else(|| format!("entity {} missing from export", model.entities[ek].id))?;
        binders.insert(ek, (spec.binder.clone(), spec.entity_id.clone()));
    }
    Ok(binders)
}

fn quantifiers_for_binders(binders: &BTreeMap<EntityKey, (String, String)>) -> String {
    let parts: Vec<String> = binders
        .values()
        .map(|(b, eid)| format!("{b} \\in {}_Ids", sanitize_ident(eid)))
        .collect();
    format!("\\A {}", parts.join(", "))
}

fn along_link_filter(
    model: &SemanticModel,
    scope: &CrossConstraintScope,
    binders: &BTreeMap<EntityKey, (String, String)>,
    owners: &[OwnerLink],
    warnings: &mut Vec<String>,
) -> Option<String> {
    let CrossConstraintScope::RelationPath(path) = scope else {
        return None;
    };
    if path.len() < 2 {
        return None;
    }
    // Pairwise owner filters along the path.
    let mut filters = Vec::new();
    for w in path.windows(2) {
        let a_id = &model.entities[w[0]].id;
        let b_id = &model.entities[w[1]].id;
        let (ab, _) = binders.get(&w[0])?;
        let (bb, _) = binders.get(&w[1])?;
        if let Some(o) = owner_for_pair(owners, a_id, b_id) {
            if o.child_id == *a_id {
                filters.push(format!("{}[{ab}] = {bb}", o.var_name));
            } else {
                filters.push(format!("{}[{bb}] = {ab}", o.var_name));
            }
        } else {
            warnings.push(format!(
                ".along({a_id}, {b_id}): no relate N:1/1:N link; quantifying over instance product"
            ));
            return None;
        }
    }
    Some(filters.join(" /\\ "))
}

fn cross_conditions_to_tla(
    model: &SemanticModel,
    conditions: &[CrossEntityCondition],
    specs: &HashMap<EntityKey, &EntitySpec>,
    binders: &BTreeMap<EntityKey, (String, String)>,
) -> Result<String, String> {
    let mut parts = Vec::new();
    for cond in conditions {
        parts.push(cross_condition_atom(model, cond, specs, binders)?);
    }
    if parts.is_empty() {
        return Err("empty condition list".into());
    }
    Ok(parts.join(" /\\ "))
}

fn cross_condition_atom(
    model: &SemanticModel,
    cond: &CrossEntityCondition,
    specs: &HashMap<EntityKey, &EntitySpec>,
    binders: &BTreeMap<EntityKey, (String, String)>,
) -> Result<String, String> {
    match cond {
        CrossEntityCondition::Equals { column, value } => {
            let spec = specs
                .get(&column.entity)
                .ok_or_else(|| format!("no axis for {}", model.entities[column.entity].id))?;
            let (binder, _) = binders
                .get(&column.entity)
                .ok_or_else(|| "missing binder".to_string())?;
            let ax = spec
                .axes
                .iter()
                .find(|a| a.column == column.column)
                .ok_or_else(|| format!("column `{}` is not a TLA state axis", column.column))?;
            Ok(format!(
                "{}[{binder}] = {}",
                ax.var_name,
                effect_value_to_tla(value)
            ))
        }
        CrossEntityCondition::Comparison(cmp) => {
            let lhs_spec = specs
                .get(&cmp.lhs.entity)
                .ok_or_else(|| format!("no axis for {}", model.entities[cmp.lhs.entity].id))?;
            let (lhs_b, _) = binders
                .get(&cmp.lhs.entity)
                .ok_or_else(|| "missing binder".to_string())?;
            let lhs_ax = lhs_spec
                .axes
                .iter()
                .find(|a| a.column == cmp.lhs.column)
                .ok_or_else(|| format!("column `{}` is not a TLA state axis", cmp.lhs.column))?;
            let lhs = format!("{}[{lhs_b}]", lhs_ax.var_name);
            let rhs = match &cmp.rhs {
                CrossCmpRhs::Column(r) => {
                    let rspec = specs
                        .get(&r.entity)
                        .ok_or_else(|| format!("no axis for {}", model.entities[r.entity].id))?;
                    let (rb, _) = binders
                        .get(&r.entity)
                        .ok_or_else(|| "missing binder".to_string())?;
                    let rax = rspec
                        .axes
                        .iter()
                        .find(|a| a.column == r.column)
                        .ok_or_else(|| format!("column `{}` is not a TLA state axis", r.column))?;
                    format!("{}[{rb}]", rax.var_name)
                }
                CrossCmpRhs::IntLit(n) => n.to_string(),
                CrossCmpRhs::Now => "now".into(),
            };
            Ok(format!("{lhs} {} {rhs}", cmp_op_tla(cmp.op)))
        }
    }
}

/// Lower `after(UC).assert(...)` to independent TLA properties
/// `[][ (actions) => (primed posts) ]_vars` — never inject into action effects.
fn apply_temporal_assertions(
    model: &SemanticModel,
    specs: &[EntitySpec],
    multi_instance: bool,
    warnings: &mut Vec<String>,
) -> Vec<(String, String)> {
    let mut props = Vec::new();
    if model.temporal_assertions.is_empty() {
        return props;
    }

    let mut unmapped = 0usize;
    for (ai, assertion) in model.temporal_assertions.iter().enumerate() {
        let uc_id = match model.use_cases.get(assertion.anchor) {
            Some(uc) => uc.id.clone(),
            None => {
                unmapped += 1;
                continue;
            }
        };
        let raising_events: Vec<_> = model
            .relations
            .iter()
            .filter_map(|rel| {
                if rel.kind != RelKind::Raises {
                    return None;
                }
                match (&rel.from, &rel.to) {
                    (NodeRef::UseCase(u), NodeRef::Event(e)) if *u == assertion.anchor => Some(*e),
                    _ => None,
                }
            })
            .collect();

        // Per-required implications: (matching action names, primed postconditions).
        let mut implications: Vec<(Vec<String>, Vec<String>)> = Vec::new();
        let mut assertion_ok = true;

        for req in &assertion.requireds {
            match req {
                CrossEntityCondition::Equals { column, value } => {
                    let Some(spec) = specs.iter().find(|s| s.entity_key == column.entity) else {
                        assertion_ok = false;
                        continue;
                    };
                    let Some(ax) = spec.axes.iter().find(|a| a.column == column.column) else {
                        assertion_ok = false;
                        continue;
                    };
                    let var = ax.var_name.clone();
                    let val = effect_value_to_tla(value);
                    let binder = spec.binder.clone();
                    let event_names: HashSet<String> = raising_events
                        .iter()
                        .map(|ek| {
                            format!(
                                "{}_{}",
                                sanitize_ident(&spec.entity_id),
                                sanitize_ident(&model.events[*ek].id)
                            )
                        })
                        .collect();

                    let matching: Vec<String> = spec
                        .actions
                        .iter()
                        .filter(|a| action_matches_raising_event(&a.name, &event_names))
                        .map(|a| a.name.clone())
                        .collect();
                    if matching.is_empty() {
                        assertion_ok = false;
                        continue;
                    }

                    for action in spec
                        .actions
                        .iter()
                        .filter(|a| action_matches_raising_event(&a.name, &event_names))
                    {
                        if effect_conflicts_assignment(
                            &action.effects,
                            &var,
                            &val,
                            multi_instance,
                            &binder,
                        ) {
                            push_tla_fatal(
                                warnings,
                                "contradictory_assert",
                                format!(
                                    "contradictory after.assert on `{var}` for action `{}` (wanted {val})",
                                    action.name
                                ),
                            );
                            assertion_ok = false;
                        }
                    }

                    let post = if multi_instance {
                        format!("{var}'[{binder}] = {val}")
                    } else {
                        format!("{var}' = {val}")
                    };
                    implications.push((matching, vec![post]));
                }
                CrossEntityCondition::Comparison(cmp) => {
                    let axis_lookup: Vec<(EntityKey, String, String, AxisKind, String)> = specs
                        .iter()
                        .flat_map(|s| {
                            s.axes.iter().map(|a| {
                                (
                                    s.entity_key,
                                    a.column.clone(),
                                    a.var_name.clone(),
                                    a.kind,
                                    s.binder.clone(),
                                )
                            })
                        })
                        .collect();
                    let find_axis =
                        |ek: EntityKey, col: &str| -> Option<(String, AxisKind, String)> {
                            axis_lookup.iter().find_map(|(k, c, var, kind, binder)| {
                                (*k == ek && c == col).then(|| (var.clone(), *kind, binder.clone()))
                            })
                        };

                    let Some((lhs_var, lhs_kind, lhs_binder)) =
                        find_axis(cmp.lhs.entity, &cmp.lhs.column)
                    else {
                        assertion_ok = false;
                        continue;
                    };

                    let Some(spec) = specs.iter().find(|s| s.entity_key == cmp.lhs.entity) else {
                        assertion_ok = false;
                        continue;
                    };
                    let event_names: HashSet<String> = raising_events
                        .iter()
                        .map(|ek| {
                            format!(
                                "{}_{}",
                                sanitize_ident(&spec.entity_id),
                                sanitize_ident(&model.events[*ek].id)
                            )
                        })
                        .collect();
                    let matching: Vec<String> = spec
                        .actions
                        .iter()
                        .filter(|a| action_matches_raising_event(&a.name, &event_names))
                        .map(|a| a.name.clone())
                        .collect();
                    if matching.is_empty() {
                        assertion_ok = false;
                        continue;
                    }

                    let post = if lhs_kind == AxisKind::Int {
                        let rhs = match &cmp.rhs {
                            CrossCmpRhs::IntLit(n) => Some(n.to_string()),
                            CrossCmpRhs::Now => Some("now".into()),
                            CrossCmpRhs::Column(r) => {
                                find_axis(r.entity, &r.column).map(|(var, _kind, rbinder)| {
                                    if multi_instance {
                                        format!("{var}[{rbinder}]")
                                    } else {
                                        var
                                    }
                                })
                            }
                        };
                        let Some(rhs) = rhs else {
                            assertion_ok = false;
                            continue;
                        };
                        let lhs_prime = if multi_instance {
                            format!("{lhs_var}'[{lhs_binder}]")
                        } else {
                            format!("{lhs_var}'")
                        };
                        format!("{lhs_prime} {} {rhs}", cmp_op_tla(cmp.op))
                    } else {
                        let prop_key = format!(
                            "__cmp:{}{}{}",
                            cmp.lhs.column,
                            cmp.op.as_str(),
                            match &cmp.rhs {
                                CrossCmpRhs::Column(r) => r.column.clone(),
                                CrossCmpRhs::IntLit(n) => n.to_string(),
                                CrossCmpRhs::Now => "now".into(),
                            }
                        );
                        let Some((var, _kind, binder)) = find_axis(cmp.lhs.entity, &prop_key)
                        else {
                            assertion_ok = false;
                            continue;
                        };
                        if multi_instance {
                            format!("{var}'[{binder}] = TRUE")
                        } else {
                            format!("{var}' = TRUE")
                        }
                    };
                    implications.push((matching, vec![post]));
                }
            }
        }

        if !assertion_ok || implications.is_empty() {
            unmapped += 1;
            continue;
        }

        // Fold posts that share the same action set: Act => (p1 /\ p2).
        let mut merged: BTreeMap<Vec<String>, Vec<String>> = BTreeMap::new();
        for (mut acts, posts) in implications {
            acts.sort();
            acts.dedup();
            merged.entry(acts).or_default().extend(posts);
        }
        let inner = merged
            .into_iter()
            .map(|(acts, mut posts)| {
                posts.sort();
                posts.dedup();
                let ant = if acts.len() == 1 {
                    acts[0].clone()
                } else {
                    format!("({})", acts.join(" \\/ "))
                };
                let cons = if posts.len() == 1 {
                    posts[0].clone()
                } else {
                    format!("({})", posts.join(" /\\ "))
                };
                format!("({ant}) => ({cons})")
            })
            .collect::<Vec<_>>()
            .join(" /\\ ");
        let name = format!("AfterAssert_{}_{}", sanitize_ident(&uc_id), ai);
        props.push((name, format!("[][{inner}]_vars")));
    }

    if unmapped > 0 {
        push_tla_fatal(
            warnings,
            "assert_unmapped",
            format!("{unmapped} after(...).assert constraint(s) not yet mapped to TLA properties"),
        );
    }
    props
}

fn push_tla_fatal(warnings: &mut Vec<String>, code: &str, message: String) {
    warnings.push(format!("[TLA_FATAL:{code}] {message}"));
}

fn effect_already_sets(
    effect_line: &str,
    var: &str,
    val: &str,
    multi_instance: bool,
    binder: &str,
) -> bool {
    if multi_instance {
        effect_line.contains(&format!("{var}' = [{var} EXCEPT ![{binder}] = {val}]"))
            || effect_line.contains(&format!("EXCEPT ![{binder}] = {val}]"))
                && effect_line.contains(var)
    } else {
        effect_line == format!("{var}' = {val}")
            || effect_line.starts_with(&format!("{var}' = ")) && effect_line.ends_with(val)
    }
}

/// True when `action_name` is `Entity_Event` or `Entity_Event_from_to_to` for a raising event.
fn action_matches_raising_event(action_name: &str, event_names: &HashSet<String>) -> bool {
    event_names
        .iter()
        .any(|en| action_name == en || action_name.starts_with(&format!("{en}_")))
}

/// Detect an existing primed assignment to `var` with a different value.
fn effect_conflicts_assignment(
    effects: &[String],
    var: &str,
    val: &str,
    multi_instance: bool,
    binder: &str,
) -> bool {
    let prefix = if multi_instance {
        format!("{var}' = [{var} EXCEPT ![{binder}] = ")
    } else {
        format!("{var}' = ")
    };
    effects.iter().any(|e| {
        if !e.starts_with(&prefix) {
            return false;
        }
        !effect_already_sets(e, var, val, multi_instance, binder)
    })
}

fn note_skipped_constraints(
    model: &SemanticModel,
    lifecycles: &[EntityLifecycle],
    multi_instance: bool,
    warnings: &mut Vec<String>,
) {
    let lifecycle_entities: BTreeSet<EntityKey> = lifecycles.iter().map(|l| l.entity).collect();
    // Cross / quantifier are exported when multi_instance; otherwise still Phase-3 skip.
    if !multi_instance {
        if !model.cross_forbidden_constraints.is_empty() {
            push_tla_fatal(
                warnings,
                "cross_unexported",
                format!(
                    "{} cross_forbidden constraint(s) not exported (need multi-instance export)",
                    model.cross_forbidden_constraints.len()
                ),
            );
        }
        if !model.cross_entity_invariants.is_empty() {
            push_tla_fatal(
                warnings,
                "cross_unexported",
                format!(
                    "{} cross_invariant constraint(s) not exported (need multi-instance export)",
                    model.cross_entity_invariants.len()
                ),
            );
        }
        if !model.quantifier_constraints.is_empty() {
            push_tla_fatal(
                warnings,
                "cross_unexported",
                format!(
                    "{} quantifier constraint(s) not exported (need multi-instance export)",
                    model.quantifier_constraints.len()
                ),
            );
        }
    }
    for inv in &model.entity_invariants {
        if !lifecycle_entities.contains(&inv.entity) {
            warnings.push(format!(
                "invariant on {} skipped (no status lifecycle)",
                entity_id(model, inv.entity)
            ));
        }
    }
}

fn invariant_to_tla(
    inv: &EntityInvariant,
    col_to_var: &HashMap<&str, &str>,
    axis_kinds: &HashMap<&str, AxisKind>,
    int_now: &IntNowUsage,
    eid: &str,
) -> Result<String, String> {
    let guards = conditions_and_comps_to_tla(
        &inv.guards,
        &inv.guard_comparisons,
        col_to_var,
        axis_kinds,
        int_now,
        eid,
    )?;
    let reqs = conditions_and_comps_to_tla(
        &inv.requireds,
        &inv.required_comparisons,
        col_to_var,
        axis_kinds,
        int_now,
        eid,
    )?;
    if guards.is_empty() {
        Ok(reqs)
    } else {
        Ok(format!("({guards}) => ({reqs})"))
    }
}

fn forbidden_to_tla(
    f: &ForbiddenConstraint,
    col_to_var: &HashMap<&str, &str>,
    axis_kinds: &HashMap<&str, AxisKind>,
    int_now: &IntNowUsage,
    eid: &str,
) -> Result<String, String> {
    let body = conditions_and_comps_to_tla(
        &f.conditions,
        &f.comparisons,
        col_to_var,
        axis_kinds,
        int_now,
        eid,
    )?;
    Ok(format!("~({body})"))
}

fn required_to_tla(
    r: &RequiredConstraint,
    col_to_var: &HashMap<&str, &str>,
    axis_kinds: &HashMap<&str, AxisKind>,
    int_now: &IntNowUsage,
    eid: &str,
) -> Result<String, String> {
    conditions_and_comps_to_tla(
        &r.conditions,
        &r.comparisons,
        col_to_var,
        axis_kinds,
        int_now,
        eid,
    )
}

fn exclusive_to_tla(
    ex: &ExclusiveConstraint,
    col_to_var: &HashMap<&str, &str>,
    axis_kinds: &HashMap<&str, AxisKind>,
    int_now: &IntNowUsage,
    eid: &str,
) -> Result<String, String> {
    let mut atoms = Vec::new();
    for (col, val) in &ex.conditions {
        atoms.push(eq_atom(col, val, col_to_var)?);
    }
    for c in &ex.comparisons {
        atoms.push(comp_atom(c, col_to_var, axis_kinds, int_now, eid)?);
    }
    if atoms.len() < 2 {
        return Err("exclusive needs at least two conditions".into());
    }
    let mut pairs = Vec::new();
    for i in 0..atoms.len() {
        for j in (i + 1)..atoms.len() {
            pairs.push(format!("~( ({}) /\\ ({}) )", atoms[i], atoms[j]));
        }
    }
    Ok(pairs.join(" /\\ "))
}

fn conditions_and_comps_to_tla(
    conditions: &[(String, EffectValue)],
    comparisons: &[ComparisonProp],
    col_to_var: &HashMap<&str, &str>,
    axis_kinds: &HashMap<&str, AxisKind>,
    int_now: &IntNowUsage,
    eid: &str,
) -> Result<String, String> {
    let mut parts = Vec::new();
    for (col, val) in conditions {
        parts.push(eq_atom(col, val, col_to_var)?);
    }
    for c in comparisons {
        parts.push(comp_atom(c, col_to_var, axis_kinds, int_now, eid)?);
    }
    if parts.is_empty() {
        return Err("empty condition list".into());
    }
    Ok(parts.join(" /\\ "))
}

fn eq_atom(
    col: &str,
    val: &EffectValue,
    col_to_var: &HashMap<&str, &str>,
) -> Result<String, String> {
    let var = col_to_var
        .get(col)
        .ok_or_else(|| format!("column `{col}` is not a TLA state axis"))?;
    Ok(format!("{var} = {}", effect_value_to_tla(val)))
}

fn comp_atom(
    c: &ComparisonProp,
    col_to_var: &HashMap<&str, &str>,
    axis_kinds: &HashMap<&str, AxisKind>,
    int_now: &IntNowUsage,
    eid: &str,
) -> Result<String, String> {
    let arith_key = format!("{}:{}", eid, c.axis_key());
    if int_now.arithmetic_cmp_keys.contains(&arith_key)
        || axis_kinds
            .get(c.lhs_column.as_str())
            .is_some_and(|k| *k == AxisKind::Int)
    {
        let lhs = col_to_var
            .get(c.lhs_column.as_str())
            .ok_or_else(|| format!("column `{}` is not a TLA state axis", c.lhs_column))?;
        let rhs = match &c.rhs {
            CmpRhs::Column(other) => col_to_var
                .get(other.as_str())
                .ok_or_else(|| format!("column `{other}` is not a TLA state axis"))?
                .to_string(),
            CmpRhs::IntLit(n) => n.to_string(),
            CmpRhs::Now => "now".into(),
        };
        return Ok(format!("{lhs} {} {rhs}", cmp_op_tla(c.op)));
    }

    let key = format!("__cmp:{}", c.axis_key());
    let var = col_to_var
        .get(key.as_str())
        .ok_or_else(|| format!("comparison `{}` is not a TLA state axis", c.axis_key()))?;
    Ok(format!("{var} = TRUE"))
}

fn cmp_op_tla(op: CmpOpModel) -> &'static str {
    match op {
        CmpOpModel::Eq => "=",
        CmpOpModel::Ne => "#",
        CmpOpModel::Lt => "<",
        CmpOpModel::Le => "<=",
        CmpOpModel::Gt => ">",
        CmpOpModel::Ge => ">=",
    }
}

fn effect_value_to_tla(v: &EffectValue) -> String {
    match v {
        EffectValue::EnumVariant(s) => format!("\"{}\"", escape_tla_string(s)),
        EffectValue::Bool(true) => "TRUE".into(),
        EffectValue::Bool(false) => "FALSE".into(),
        EffectValue::Int(n) => n.to_string(),
        EffectValue::Present | EffectValue::TypedPresent(_) => "\"present\"".into(),
        EffectValue::Null => "\"null\"".into(),
    }
}

fn temporal_property_to_tla(
    prop: &TemporalProperty,
    lookup: &HashMap<String, String>,
) -> Result<String, String> {
    match &prop.formula {
        TemporalFormula::Always(e) => Ok(format!("[]({})", temporal_expr_to_tla(e, lookup)?)),
        TemporalFormula::Eventually(e) => Ok(format!("<>({})", temporal_expr_to_tla(e, lookup)?)),
        TemporalFormula::LeadsTo {
            antecedent,
            consequent,
        } => Ok(format!(
            "({}) ~> ({})",
            temporal_expr_to_tla(antecedent, lookup)?,
            temporal_expr_to_tla(consequent, lookup)?
        )),
    }
}

fn temporal_expr_to_tla(
    expr: &TemporalExpr,
    lookup: &HashMap<String, String>,
) -> Result<String, String> {
    match expr {
        TemporalExpr::Atom(TemporalAtom {
            entity,
            column,
            op,
            rhs,
        }) => {
            let key = match entity {
                Some(e) => format!("{e}.{column}"),
                None => column.clone(),
            };
            let var = match entity {
                Some(_) => lookup.get(&key).ok_or_else(|| {
                    format!("unresolved column `{key}` in temporal property")
                })?,
                None => lookup.get(&key).ok_or_else(|| {
                    if lookup.keys().any(|k| k.ends_with(&format!(".{column}"))) {
                        format!(
                            "ambiguous column `{column}` in temporal property; qualify with Entity.column"
                        )
                    } else {
                        format!("unresolved column `{key}` in temporal property")
                    }
                })?,
            };
            let rhs_s = match rhs {
                TemporalRhs::Value(v) => effect_value_to_tla(v),
                TemporalRhs::IntLit(n) => n.to_string(),
                TemporalRhs::Column {
                    entity: rhs_ent,
                    column: rhs_col,
                } => {
                    let rhs_key = match rhs_ent {
                        Some(e) => format!("{e}.{rhs_col}"),
                        None => rhs_col.clone(),
                    };
                    match rhs_ent {
                        Some(_) => lookup.get(&rhs_key).cloned().ok_or_else(|| {
                            format!("unresolved column `{rhs_key}` in temporal property")
                        })?,
                        None => lookup.get(&rhs_key).cloned().ok_or_else(|| {
                            if lookup.keys().any(|k| k.ends_with(&format!(".{rhs_col}"))) {
                                format!(
                                    "ambiguous column `{rhs_col}` in temporal property; qualify with Entity.column"
                                )
                            } else {
                                format!("unresolved column `{rhs_key}` in temporal property")
                            }
                        })?,
                    }
                }
            };
            Ok(format!("{var} {} {rhs_s}", cmp_op_tla(*op)))
        }
        TemporalExpr::Not(inner) => Ok(format!("~({})", temporal_expr_to_tla(inner, lookup)?)),
        TemporalExpr::And(a, b) => Ok(format!(
            "({}) /\\ ({})",
            temporal_expr_to_tla(a, lookup)?,
            temporal_expr_to_tla(b, lookup)?
        )),
        TemporalExpr::Or(a, b) => Ok(format!(
            "({}) \\/ ({})",
            temporal_expr_to_tla(a, lookup)?,
            temporal_expr_to_tla(b, lookup)?
        )),
    }
}

fn all_export_vars(export: &TlaExport) -> Vec<String> {
    let mut vars: Vec<String> = export
        .specs
        .iter()
        .flat_map(|s| s.axes.iter().map(|a| a.var_name.clone()))
        .collect();
    for o in &export.owners {
        vars.push(o.var_name.clone());
    }
    if export.needs_now {
        vars.push("now".into());
    }
    vars
}

fn render_tla(module_name: &str, export: &TlaExport, warnings: &[String]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "---- MODULE {module_name} ----\n\
         \\* Generated by rdra-ish. Do not edit manually.\n\n"
    ));

    if !warnings.is_empty() {
        out.push_str("\\* --- export warnings ---\n");
        for w in warnings {
            out.push_str(&format!("\\* WARNING: {}\n", w.replace('\n', " ")));
        }
        out.push('\n');
    }

    let all_vars = all_export_vars(export);
    if all_vars.is_empty() {
        out.push_str("\\* No entity lifecycles with exportable state axes.\n");
        out.push_str("====\n");
        return out;
    }

    if export.needs_integers {
        out.push_str("EXTENDS Integers\n\n");
    }

    if export.multi_instance {
        out.push_str(&format!("InstanceCount == {INSTANCE_COUNT}\n"));
    }
    if export.needs_integers {
        out.push_str(&format!("IntRange == 0..{INT_RANGE_MAX}\n"));
    }
    if export.multi_instance {
        for spec in &export.specs {
            out.push_str(&format!(
                "{}_Ids == 1..InstanceCount\n",
                sanitize_ident(&spec.entity_id)
            ));
        }
        out.push('\n');
    } else if export.needs_integers {
        out.push('\n');
    }

    out.push_str("VARIABLES ");
    out.push_str(&all_vars.join(", "));
    out.push_str("\n\n");

    out.push_str("vars == <<");
    out.push_str(&all_vars.join(", "));
    out.push_str(">>\n\n");

    // TypeOK
    out.push_str("TypeOK ==\n");
    for spec in &export.specs {
        let ids = format!("{}_Ids", sanitize_ident(&spec.entity_id));
        for ax in &spec.axes {
            if export.multi_instance {
                out.push_str(&format!(
                    "  /\\ {} \\in [{ids} -> {}]\n",
                    ax.var_name, ax.domain
                ));
            } else {
                out.push_str(&format!("  /\\ {} \\in {}\n", ax.var_name, ax.domain));
            }
        }
    }
    for o in &export.owners {
        out.push_str(&format!(
            "  /\\ {} \\in [{}_Ids -> {}_Ids]\n",
            o.var_name,
            sanitize_ident(&o.child_id),
            sanitize_ident(&o.parent_id)
        ));
    }
    if export.needs_now {
        out.push_str("  /\\ now \\in IntRange\n");
    }
    out.push('\n');

    // Init
    out.push_str("Init ==\n");
    for spec in &export.specs {
        for (var, val) in &spec.init {
            out.push_str(&format!("  /\\ {var} = {val}\n"));
        }
    }
    for o in &export.owners {
        out.push_str(&format!(
            "  /\\ {} \\in [{}_Ids -> {}_Ids]\n",
            o.var_name,
            sanitize_ident(&o.child_id),
            sanitize_ident(&o.parent_id)
        ));
    }
    if export.needs_now {
        out.push_str("  /\\ now = 0\n");
    }
    out.push('\n');

    // Collect global unchanged helpers for TickNow / cross-entity actions.
    let owner_vars: Vec<String> = export.owners.iter().map(|o| o.var_name.clone()).collect();

    // Actions
    for spec in &export.specs {
        for action in &spec.actions {
            // Unchanged must include other entities' vars, owners, now.
            let mut unchanged = action.unchanged.clone();
            for other in &export.specs {
                if other.entity_id == spec.entity_id {
                    continue;
                }
                for ax in &other.axes {
                    if !unchanged.contains(&ax.var_name) {
                        unchanged.push(ax.var_name.clone());
                    }
                }
            }
            for ov in &owner_vars {
                if !unchanged.contains(ov) {
                    unchanged.push(ov.clone());
                }
            }
            if export.needs_now && !unchanged.contains(&"now".to_string()) {
                unchanged.push("now".into());
            }

            out.push_str(&format!("\\* {}\n", action.comment));
            out.push_str(&format!("{} ==\n", action.name));
            if let Some((binder, ids)) = &action.exists_binder {
                out.push_str(&format!("  \\E {binder} \\in {ids}:\n"));
                for g in &action.guards {
                    out.push_str(&format!("    /\\ {g}\n"));
                }
                for e in &action.effects {
                    out.push_str(&format!("    /\\ {e}\n"));
                }
                push_unchanged(&mut out, &unchanged, "    ");
            } else {
                for g in &action.guards {
                    out.push_str(&format!("  /\\ {g}\n"));
                }
                for e in &action.effects {
                    out.push_str(&format!("  /\\ {e}\n"));
                }
                push_unchanged(&mut out, &unchanged, "  ");
            }
            out.push('\n');
        }
    }

    if export.needs_now {
        let mut unchanged: Vec<String> = export
            .specs
            .iter()
            .flat_map(|s| s.axes.iter().map(|a| a.var_name.clone()))
            .chain(owner_vars.iter().cloned())
            .collect();
        unchanged.sort();
        unchanged.dedup();
        out.push_str(
            "\\* advance global clock (Safety checks col vs now; do not constrain TickNow)\n",
        );
        out.push_str("TickNow ==\n");
        out.push_str("  /\\ \\E t \\in IntRange:\n");
        out.push_str("       /\\ t > now\n");
        out.push_str("       /\\ now' = t\n");
        push_unchanged(&mut out, &unchanged, "  ");
        out.push('\n');
    }

    // Next
    let mut action_names: Vec<String> = export
        .specs
        .iter()
        .flat_map(|s| s.actions.iter().map(|a| a.name.clone()))
        .collect();
    if export.needs_now {
        action_names.push("TickNow".into());
    }
    out.push_str("Next ==\n");
    if action_names.is_empty() {
        out.push_str("  UNCHANGED vars\n\n");
    } else {
        for name in &action_names {
            out.push_str(&format!("  \\/ {name}\n"));
        }
        out.push('\n');
    }

    if export.needs_wf {
        out.push_str("Spec == Init /\\ [][Next]_vars /\\ WF_vars(Next)\n\n");
    } else {
        out.push_str("Spec == Init /\\ [][Next]_vars\n\n");
    }

    // Invariants + global safety
    let mut inv_names = Vec::new();
    for spec in &export.specs {
        for (name, formula) in &spec.invariants {
            // Formulas are already indexed + quantified when multi_instance.
            out.push_str(&format!("{name} == {formula}\n"));
            inv_names.push(name.clone());
        }
    }
    for (name, formula) in &export.global_safety {
        out.push_str(&format!("{name} == {formula}\n"));
        inv_names.push(name.clone());
    }

    if !inv_names.is_empty() {
        out.push_str("\nSafety ==\n  /\\ TypeOK\n");
        for name in &inv_names {
            out.push_str(&format!("  /\\ {name}\n"));
        }
        out.push('\n');
    } else {
        out.push_str("Safety == TypeOK\n\n");
    }

    // Temporal properties (single authoritative list)
    let mut prop_names = Vec::new();
    for (name, formula) in &export.properties {
        out.push_str(&format!("{name} == {formula}\n"));
        prop_names.push(name.clone());
    }
    if !prop_names.is_empty() {
        out.push('\n');
    }

    out.push_str("THEOREM Spec => []Safety\n");
    out.push_str("====\n");
    out
}

fn push_unchanged(out: &mut String, unchanged: &[String], indent: &str) {
    if unchanged.is_empty() {
        return;
    }
    if unchanged.len() == 1 {
        out.push_str(&format!("{indent}/\\ UNCHANGED {}\n", unchanged[0]));
    } else {
        out.push_str(&format!(
            "{indent}/\\ UNCHANGED <<{}>>\n",
            unchanged.join(", ")
        ));
    }
}

fn render_cfg(module_name: &str, export: &TlaExport) -> String {
    let mut out = String::new();
    out.push_str("SPECIFICATION Spec\n");
    out.push_str("INVARIANT Safety\n");
    // Lifecycle models have terminal states with no enabled Next action.
    // TLC's default deadlock check treats those as errors; Spec already allows
    // stuttering via [][Next]_vars, so disable the Next-only deadlock check.
    out.push_str("CHECK_DEADLOCK FALSE\n");

    for (name, _) in &export.properties {
        out.push_str(&format!("PROPERTY {name}\n"));
    }

    let _ = module_name;
    out
}

fn entity_id(model: &SemanticModel, ek: EntityKey) -> String {
    model.entities[ek].id.clone()
}

fn sanitize_ident(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        return "X".into();
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, 'X');
    }
    out
}

fn sanitize_module_name(s: &str) -> String {
    let stem = s.trim();
    let stem = stem
        .strip_suffix(".tla")
        .or_else(|| stem.strip_suffix(".cfg"))
        .unwrap_or(stem);
    let name = sanitize_ident(stem);
    if name == "X" && stem.is_empty() {
        "RdraSpec".into()
    } else {
        name
    }
}

fn escape_tla_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_core::analysis::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        assert!(
            diags.iter().all(|d| d.is_warning),
            "unexpected diagnostics: {diags:?}"
        );
        model
    }

    fn order_sample_src() -> String {
        std::fs::read_to_string("samples/formal-verification/order.rdra")
            .or_else(|_| std::fs::read_to_string("../../samples/formal-verification/order.rdra"))
            .or_else(|_| std::fs::read_to_string("skills/rdra-ish-verify/samples/order.rdra"))
            .or_else(|_| std::fs::read_to_string("../../skills/rdra-ish-verify/samples/order.rdra"))
            .expect("order.rdra sample")
    }

    #[test]
    fn tla_order_snapshot() {
        let model = model_from(&order_sample_src());
        let bundle = TlaPlusEmitter::default()
            .emit_bundle(&model, &View::whole())
            .unwrap();
        insta::assert_snapshot!(bundle.tla);
        insta::assert_snapshot!("order_cfg", bundle.cfg);
    }

    #[test]
    fn tla_int_stock_exports_arithmetic_safety() {
        let src = std::fs::read_to_string("samples/formal-verification/int_stock.rdra")
            .or_else(|_| {
                std::fs::read_to_string("../../samples/formal-verification/int_stock.rdra")
            })
            .expect("int_stock.rdra");
        let model = model_from(&src);
        let bundle = TlaPlusEmitter::default()
            .emit_bundle(&model, &View::whole())
            .unwrap();
        assert!(!bundle.tla.contains("Phase 3"));
        assert!(bundle.tla.contains("EXTENDS Integers"));
        assert!(bundle.tla.contains("Item_stock"));
        assert!(bundle.tla.contains("Item_selling"));
        assert!(bundle.tla.contains("~(Item_stock < Item_selling)"));
        assert!(bundle.tla.contains("Item_stock >= Item_selling"));
        assert!(
            bundle.tla.contains("Item_stock = 1") && bundle.tla.contains("Item_selling = 1"),
            "Init must satisfy Safety; got:\n{}",
            bundle.tla
        );
        assert!(!bundle.tla.contains("prop_stock"));
    }

    #[test]
    fn tla_now_coupon_exports_ticknow() {
        let src = std::fs::read_to_string("samples/formal-verification/now_coupon.rdra")
            .or_else(|_| {
                std::fs::read_to_string("../../samples/formal-verification/now_coupon.rdra")
            })
            .expect("now_coupon.rdra");
        let model = model_from(&src);
        let bundle = TlaPlusEmitter::default()
            .emit_bundle(&model, &View::whole())
            .unwrap();
        assert!(!bundle.tla.contains("Phase 3"));
        assert!(bundle.tla.contains("TickNow"));
        assert!(bundle.tla.contains("Coupon_expired_at"));
        assert!(bundle.tla.contains("~(Coupon_expired_at < now)"));
        // Next must be able to violate Safety so TLC can find counterexamples.
        assert!(
            bundle.tla.contains("Assign_Coupon_expired_at"),
            "missing Assign on now-lhs:\n{}",
            bundle.tla
        );
        assert!(
            !bundle.tla.contains("v >= now"),
            "Assign must not bake Safety into Next; got:\n{}",
            bundle.tla
        );
        assert!(
            !bundle.tla.contains("t <= Coupon_expired_at"),
            "TickNow must not bake Safety into Next; got:\n{}",
            bundle.tla
        );
    }

    #[test]
    fn tla_duplicate_event_transitions_get_unique_action_names() {
        let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, cancelled) @default(pending)
}
usecase Pay "pay"
event EvPay "pay"
updates(Pay, Order)
raises(Pay, EvPay)
transitions(Order.status, EvPay, pending -> paid)
transitions(Order.status, EvPay, pending -> cancelled)
"#;
        let model = model_from(src);
        let bundle = TlaPlusEmitter::default()
            .emit_bundle(&model, &View::whole())
            .unwrap();
        assert!(bundle.tla.contains("Order_EvPay_pending_to_paid"));
        assert!(bundle.tla.contains("Order_EvPay_pending_to_cancelled"));
        let paid_defs = bundle.tla.matches("Order_EvPay_pending_to_paid ==").count();
        assert_eq!(paid_defs, 1, "duplicate TLA operators:\n{}", bundle.tla);
    }

    #[test]
    fn tla_contradictory_after_assert_is_warned() {
        let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, cancelled) @default(pending)
}
usecase Pay "pay"
event EvPay "pay"
updates(Pay, Order)
raises(Pay, EvPay)
transitions(Order.status, EvPay, pending -> paid)
sets(Pay, Order, status == paid)
after(Pay).assert(Order.status == cancelled)
"#;
        let model = model_from(src);
        let bundle = TlaPlusEmitter::default()
            .emit_bundle(&model, &View::whole())
            .unwrap();
        assert!(
            bundle
                .warnings
                .iter()
                .any(|w| w.contains("contradictory after.assert")
                    || w.contains("[TLA_FATAL:contradictory_assert]")),
            "expected contradictory warning, got: {:?}",
            bundle.warnings
        );
    }

    #[test]
    fn tla_after_assert_emits_property_not_action_effect() {
        let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid) @default(pending)
}
usecase Pay "pay"
event EvPay "pay"
updates(Pay, Order)
raises(Pay, EvPay)
transitions(Order.status, EvPay, pending -> paid)
after(Pay).assert(Order.status == paid)
"#;
        let model = model_from(src);
        let bundle = TlaPlusEmitter::default()
            .emit_bundle(&model, &View::whole())
            .unwrap();
        assert!(
            bundle.tla.contains("AfterAssert_Pay_"),
            "expected AfterAssert property:\n{}",
            bundle.tla
        );
        assert!(
            bundle.tla.contains("=> (Order_status' = \"paid\")")
                || bundle.tla.contains(
                    "=> (Order_status' = \"paid\" /\\ Order_delivered_at' = \"present\")"
                )
                || bundle.tla.contains("Order_status' = \"paid\""),
            "expected action => primed post:\n{}",
            bundle.tla
        );
        assert!(
            bundle.cfg.contains("PROPERTY AfterAssert_Pay_"),
            "expected cfg PROPERTY:\n{}",
            bundle.cfg
        );
        // Transition already sets paid; assert must not add a second assignment line.
        let pay_block = bundle
            .tla
            .split("Order_EvPay_pending_to_paid ==")
            .nth(1)
            .unwrap_or("")
            .split("Next ==")
            .next()
            .unwrap_or("");
        assert_eq!(
            pay_block.matches("Order_status' = \"paid\"").count(),
            1,
            "assert must not inject into action:\n{pay_block}"
        );
    }

    #[test]
    fn tla_sets_without_transitions_still_exports_actions() {
        let src = r#"
entity Item "商品" {
  id: Int @pk
  status: Enum(active, inactive) @default(active)
  stock: Int @default(1)
}
usecase Restock "補充"
updates(Restock, Item)
sets(Restock, Item, stock == 5)
sets(Restock, Item, status == active)
forbidden(Item, stock < 0)
"#;
        let model = model_from(src);
        let bundle = TlaPlusEmitter::default()
            .emit_bundle(&model, &View::whole())
            .unwrap();
        assert!(
            bundle.tla.contains("Item_uc_Restock") || bundle.tla.contains("Item_stock"),
            "expected sets-driven export, got:\n{}",
            bundle.tla
        );
        assert!(
            !bundle
                .warnings
                .iter()
                .any(|w| w.contains("skipped (no status lifecycle)")),
            "sets-only entity should not be skipped: {:?}",
            bundle.warnings
        );
    }

    #[test]
    fn tla_cross_order_payment_multi_instance() {
        let src = std::fs::read_to_string("samples/formal-verification/cross_order_payment.rdra")
            .or_else(|_| {
                std::fs::read_to_string(
                    "../../samples/formal-verification/cross_order_payment.rdra",
                )
            })
            .expect("cross_order_payment.rdra");
        let model = model_from(&src);
        let bundle = TlaPlusEmitter::default()
            .emit_bundle(&model, &View::whole())
            .unwrap();
        assert!(!bundle.tla.contains("Phase 3"), "warnings: {}", bundle.tla);
        assert!(bundle.tla.contains("InstanceCount == 2"));
        assert!(bundle.tla.contains("Payment_owner"));
        assert!(bundle.tla.contains("CrossForbidden_0"));
        assert!(bundle.tla.contains("Payment_owner["));
    }

    #[test]
    fn tla_quantifier_none_multi_instance() {
        let src = std::fs::read_to_string("samples/formal-verification/quantifier_none.rdra")
            .or_else(|_| {
                std::fs::read_to_string("../../samples/formal-verification/quantifier_none.rdra")
            })
            .expect("quantifier_none.rdra");
        let model = model_from(&src);
        let bundle = TlaPlusEmitter::default()
            .emit_bundle(&model, &View::whole())
            .unwrap();
        assert!(!bundle.tla.contains("Phase 3"), "warnings: {}", bundle.tla);
        assert!(bundle.tla.contains("Assign_owner"));
        assert!(bundle.tla.contains("Quantifier_0"));
        assert!(bundle.tla.contains("\\A"));
    }

    #[test]
    fn tla_after_assert_maps_equality_postconditions() {
        let src = std::fs::read_to_string("samples/formal-verification/order.rdra")
            .or_else(|_| std::fs::read_to_string("../../samples/formal-verification/order.rdra"))
            .expect("order.rdra");
        let model = model_from(&src);
        let bundle = TlaPlusEmitter::default()
            .emit_bundle(&model, &View::whole())
            .unwrap();
        assert!(
            !bundle.tla.contains("not yet mapped"),
            "unexpected warning in: {}",
            bundle.tla
        );
        assert!(bundle.tla.contains("WF_vars(Next)"));
        assert!(bundle.tla.contains("PaidLeadsToShipped"));
    }

    #[test]
    fn tla_multi_instance_indexes_entity_local_invariants() {
        let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, cancelled) @default(pending)
  paid_at: DateTime @null
}
entity Payment "決済" {
  id: Int @pk
  status: Enum(pending, captured) @default(pending)
}
usecase Place "注文"
usecase Pay "決済"
usecase Cancel "取消"
event EvPay "決済"
event EvCancel "取消"
creates(Place, Order)
creates(Place, Payment)
updates(Pay, Order)
updates(Pay, Payment)
updates(Cancel, Order)
raises(Pay, EvPay)
raises(Cancel, EvCancel)
transitions(Order.status, EvPay, pending -> paid)
transitions(Order.status, EvCancel, pending -> cancelled)
transitions(Payment.status, EvPay, pending -> captured)
sets(Pay, Order, paid_at == present)
relate(Payment, Order, N:1)
invariant(Order).when(status == paid).then(paid_at == present)
forbidden(Order, Payment, Order.status == cancelled, Payment.status == captured)
  .along(Order, Payment)
"#;
        let model = model_from(src);
        let bundle = TlaPlusEmitter::default()
            .emit_bundle(&model, &View::whole())
            .unwrap();
        assert!(
            bundle.tla.contains("\\A o \\in Order_Ids:"),
            "expected quantified inv: {}",
            bundle.tla
        );
        assert!(
            bundle.tla.contains("Order_status[o] = \"paid\""),
            "expected indexed status: {}",
            bundle.tla
        );
        assert!(
            bundle.tla.contains("Order_paid_at[o] = \"present\""),
            "expected indexed paid_at: {}",
            bundle.tla
        );
        assert!(!bundle.tla.contains("rewrite"));
    }

    #[test]
    fn tla_nullable_money_becomes_int_axis() {
        let src = r#"
entity Quote "見積" {
  id: Int @pk
  status: Enum(open, closed) @default(open)
  total: Money @null
  floor: Money @default(0)
}
usecase Open "開く"
usecase Close "閉じる"
event EvClose "閉じる"
creates(Open, Quote)
updates(Close, Quote)
raises(Close, EvClose)
transitions(Quote.status, EvClose, open -> closed)
sets(Close, Quote, total == 5)
forbidden(Quote, total < floor)
"#;
        let model = model_from(src);
        let bundle = TlaPlusEmitter::default()
            .emit_bundle(&model, &View::whole())
            .unwrap();
        assert!(bundle.tla.contains("EXTENDS Integers"));
        assert!(bundle.tla.contains("Quote_total \\in IntRange"));
        assert!(bundle.tla.contains("Quote_floor \\in IntRange"));
        assert!(bundle.tla.contains("~(Quote_total < Quote_floor)"));
        assert!(
            !bundle.tla.contains("Quote_total \\in {\"null\""),
            "nullable Money must not stay as Nullable axis: {}",
            bundle.tla
        );
    }

    #[test]
    fn tla_multi_leads_to_quantifies_per_instance() {
        let src = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, shipped) @default(pending)
}
entity Payment "決済" {
  id: Int @pk
  status: Enum(pending, captured) @default(pending)
}
usecase Place "注文"
usecase Pay "決済"
usecase Ship "発送"
event EvPay "決済"
event EvShip "発送"
creates(Place, Order)
creates(Place, Payment)
updates(Pay, Order)
updates(Pay, Payment)
updates(Ship, Order)
raises(Pay, EvPay)
raises(Ship, EvShip)
transitions(Order.status, EvPay, pending -> paid)
transitions(Order.status, EvShip, paid -> shipped)
transitions(Payment.status, EvPay, pending -> captured)
relate(Payment, Order, N:1)
forbidden(Order, Payment, Order.status == pending, Payment.status == captured)
  .along(Order, Payment)
property PaidLeadsToShipped "paid ships"
  leads_to(Order.status == paid, Order.status == shipped)
property EventuallyShipped "ships"
  eventually(Order.status == shipped)
"#;
        let model = model_from(src);
        let bundle = TlaPlusEmitter::default()
            .emit_bundle(&model, &View::whole())
            .unwrap();
        assert!(
            bundle
                .tla
                .contains("\\A o \\in Order_Ids: ((Order_status[o] = \"paid\") ~> (Order_status[o] = \"shipped\"))")
                || bundle.tla.contains("\\A o \\in Order_Ids: ("),
            "expected per-instance leads_to: {}",
            bundle.tla
        );
        assert!(
            bundle
                .tla
                .contains("\\A o \\in Order_Ids: <>(Order_status[o] = \"shipped\")"),
            "expected per-instance eventually: {}",
            bundle.tla
        );
    }

    #[test]
    fn tla_after_assert_comparison_across_int_columns() {
        let src = r#"
entity Item "商品" {
  id: Int @pk
  status: Enum(active, sold) @default(active)
  stock: Int @default(5)
  sold: Int @default(0)
}
usecase Create "登録"
usecase Sell "販売"
event EvSell "販売"
creates(Create, Item)
updates(Sell, Item)
raises(Sell, EvSell)
transitions(Item.status, EvSell, active -> sold)
sets(Sell, Item, sold == 1)
after(Sell).assert(Item.sold >= 1)
"#;
        let model = model_from(src);
        let bundle = TlaPlusEmitter::default()
            .emit_bundle(&model, &View::whole())
            .unwrap();
        assert!(
            !bundle.tla.contains("not yet mapped"),
            "warnings: {}",
            bundle.tla
        );
        assert!(
            bundle.tla.contains("Item_sold' >= 1") || bundle.tla.contains("Item_sold'"),
            "expected comparison postcondition: {}",
            bundle.tla
        );
    }
}
