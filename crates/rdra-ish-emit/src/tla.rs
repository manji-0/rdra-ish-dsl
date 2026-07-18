//! TLA+ / TLC export from entity lifecycles and state constraints.

use crate::{EmitError, Emitter, View};
use rdra_ish_core::model::{
    CmpOpModel, ColumnType, ComparisonProp, EffectValue, EntityInvariant, EntityKey,
    ExclusiveConstraint, ForbiddenConstraint, RequiredConstraint, SemanticModel, TemporalAtom,
    TemporalExpr, TemporalFormula, TemporalProperty, TemporalRhs,
};
use rdra_ish_core::{collect_entity_lifecycles, AxisKind, EntityLifecycle};
use std::collections::{BTreeSet, HashMap};

/// Bundle written by `export --kind tla` / `verify --backend tlc`.
#[derive(Debug, Clone)]
pub struct TlaBundle {
    pub module_name: String,
    pub tla: String,
    pub cfg: String,
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
    pub fn emit_bundle(&self, model: &SemanticModel, _view: &View) -> Result<TlaBundle, EmitError> {
        let module_name = "RdraSpec".to_string();
        let mut warnings = Vec::new();
        let specs = build_entity_specs(model, self.multi_axis, &mut warnings);
        let tla = render_tla(&module_name, model, &specs, &warnings);
        let cfg = render_cfg(&module_name, &specs);
        Ok(TlaBundle {
            module_name,
            tla,
            cfg,
        })
    }
}

#[derive(Debug, Clone)]
struct EntitySpec {
    entity_id: String,
    axes: Vec<SpecAxis>,
    init: Vec<(String, String)>,
    actions: Vec<SpecAction>,
    invariants: Vec<(String, String)>,
    properties: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct SpecAxis {
    /// TLA variable name (prefixed).
    var_name: String,
    /// Source column / proposition key.
    column: String,
    domain: String,
}

#[derive(Debug, Clone)]
struct SpecAction {
    name: String,
    comment: String,
    /// Conjunction of `var = value` guards (TLA fragments).
    guards: Vec<String>,
    /// Conjunction of `var' = value` effects.
    effects: Vec<String>,
    /// Unchanged vars (UNCHANGED <<...>>).
    unchanged: Vec<String>,
}

fn build_entity_specs(
    model: &SemanticModel,
    multi_axis: bool,
    warnings: &mut Vec<String>,
) -> Vec<EntitySpec> {
    let lifecycles = collect_entity_lifecycles(model);
    let mut specs = Vec::new();

    for lc in &lifecycles {
        if let Some(spec) = build_one_entity(model, lc, multi_axis, warnings) {
            specs.push(spec);
        }
    }

    // Temporal properties that do not attach to a lifecycle entity still need a home;
    // they are emitted in the shared PROPERTIES section via render.
    if specs.is_empty() && !model.temporal_properties.is_empty() {
        warnings.push(
            "temporal properties declared but no entity lifecycle (status Enum + states) found"
                .into(),
        );
    }

    note_skipped_constraints(model, &lifecycles, warnings);
    specs
}

fn build_one_entity(
    model: &SemanticModel,
    lc: &EntityLifecycle,
    multi_axis: bool,
    warnings: &mut Vec<String>,
) -> Option<EntitySpec> {
    let entity = &model.entities[lc.entity];
    let entity_id = entity.id.clone();

    let axes = collect_spec_axes(model, lc, multi_axis);
    if axes.is_empty() {
        return None;
    }

    let init = build_init(model, lc, &axes);
    let actions = build_actions(model, lc, &axes, warnings);
    let invariants = build_invariants(model, lc.entity, &axes, warnings);
    let properties = build_properties_for_entity(model, &entity_id, &axes, warnings);

    Some(EntitySpec {
        entity_id,
        axes,
        init,
        actions,
        invariants,
        properties,
    })
}

fn collect_spec_axes(
    model: &SemanticModel,
    lc: &EntityLifecycle,
    multi_axis: bool,
) -> Vec<SpecAxis> {
    let entity = &model.entities[lc.entity];
    let prefix = sanitize_ident(&entity.id);
    let mut axes = Vec::new();

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
            });
        }
    }

    if !multi_axis {
        return axes;
    }

    let status_col = &lc.status_column;
    for col in &entity.columns {
        if col.name == *status_col {
            continue;
        }
        match &col.col_type {
            ColumnType::Bool => {
                axes.push(SpecAxis {
                    var_name: format!("{prefix}_{}", sanitize_ident(&col.name)),
                    column: col.name.clone(),
                    domain: "BOOLEAN".into(),
                });
            }
            _ if col.is_nullable => {
                axes.push(SpecAxis {
                    var_name: format!("{prefix}_{}", sanitize_ident(&col.name)),
                    column: col.name.clone(),
                    domain: "{\"null\", \"present\"}".into(),
                });
            }
            _ => {}
        }
    }

    // Proposition axes referenced by constraints / sets for this entity.
    let mut prop_keys: BTreeSet<String> = BTreeSet::new();
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
            prop_keys.insert(c.axis_key());
        }
    }
    for f in model
        .forbidden_constraints
        .iter()
        .filter(|f| f.entity == lc.entity)
    {
        for c in &f.comparisons {
            prop_keys.insert(c.axis_key());
        }
    }
    for r in model
        .required_constraints
        .iter()
        .filter(|r| r.entity == lc.entity)
    {
        for c in &r.comparisons {
            prop_keys.insert(c.axis_key());
        }
    }
    for e in model
        .exclusive_constraints
        .iter()
        .filter(|e| e.entity == lc.entity)
    {
        for c in &e.comparisons {
            prop_keys.insert(c.axis_key());
        }
    }
    for pe in model
        .proposition_effects
        .iter()
        .filter(|pe| pe.entity == lc.entity)
    {
        prop_keys.insert(pe.prop.axis_key());
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
        });
    }

    axes
}

fn build_init(
    model: &SemanticModel,
    lc: &EntityLifecycle,
    axes: &[SpecAxis],
) -> Vec<(String, String)> {
    let entity = &model.entities[lc.entity];
    let mut init = Vec::new();

    let entity_name = model.entities[lc.entity].id.clone();
    let prefix = format!("{entity_name}_");
    let initial_status = lc
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
        .unwrap_or_else(|| "pending".into());

    for ax in axes {
        let value = if ax.column == lc.status_column {
            format!("\"{}\"", escape_tla_string(&initial_status))
        } else if ax.column.starts_with("__cmp:") {
            "FALSE".into()
        } else if ax.domain == "BOOLEAN" {
            // Bool column default
            entity
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
                .unwrap_or_else(|| "FALSE".into())
        } else {
            // Nullable
            entity
                .columns
                .iter()
                .find(|c| c.name == ax.column)
                .and_then(|c| c.default_val.as_ref())
                .map(|d| {
                    if d.eq_ignore_ascii_case("null") {
                        "\"null\"".into()
                    } else {
                        "\"present\"".into()
                    }
                })
                .unwrap_or_else(|| "\"null\"".into())
        };
        init.push((ax.var_name.clone(), value));
    }
    init
}

fn build_actions(
    model: &SemanticModel,
    lc: &EntityLifecycle,
    axes: &[SpecAxis],
    warnings: &mut Vec<String>,
) -> Vec<SpecAction> {
    let mut actions = Vec::new();
    let col_to_var: HashMap<&str, &str> = axes
        .iter()
        .map(|a| (a.column.as_str(), a.var_name.as_str()))
        .collect();

    let all_vars: Vec<String> = axes.iter().map(|a| a.var_name.clone()).collect();

    for st in &lc.transitions {
        let from = st.from.to_lowercase();
        let to = st.to.to_lowercase();
        let event_id = model.events[st.event].id.clone();
        let status_var = col_to_var
            .get(lc.status_column.as_str())
            .copied()
            .unwrap_or("status");

        // Attach same-origin sets effects when a raising use case exists.
        let mut extra_effects: Vec<(String, String)> = Vec::new();
        let raising_ucs: Vec<_> = model
            .relations
            .iter()
            .filter_map(|rel| {
                if rel.kind != rdra_ish_core::model::RelKind::Raises {
                    return None;
                }
                match (&rel.from, &rel.to) {
                    (
                        rdra_ish_core::model::NodeRef::UseCase(u),
                        rdra_ish_core::model::NodeRef::Event(e),
                    ) if *e == st.event => Some(*u),
                    _ => None,
                }
            })
            .collect();

        for uc in &raising_ucs {
            for effect in model.column_effects.iter().filter(|e| {
                e.entity == lc.entity
                    && matches!(
                        &e.origin,
                        rdra_ish_core::model::NodeRef::UseCase(u) if u == uc
                    )
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
                e.entity == lc.entity
                    && matches!(
                        &e.origin,
                        rdra_ish_core::model::NodeRef::UseCase(u) if u == uc
                    )
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

        let mut effects = vec![format!("{status_var}' = \"{}\"", escape_tla_string(&to))];
        let mut changed: BTreeSet<String> = BTreeSet::new();
        changed.insert(status_var.to_string());
        for (var, val) in &extra_effects {
            effects.push(format!("{var}' = {val}"));
            changed.insert(var.clone());
        }

        let unchanged: Vec<String> = all_vars
            .iter()
            .filter(|v| !changed.contains(*v))
            .cloned()
            .collect();

        let action_name = format!(
            "{}_{}",
            sanitize_ident(&entity_id(model, lc.entity)),
            sanitize_ident(&event_id)
        );

        actions.push(SpecAction {
            name: action_name,
            comment: format!("{event_id}: {from} -> {to}"),
            guards: vec![format!("{status_var} = \"{}\"", escape_tla_string(&from))],
            effects,
            unchanged,
        });
    }

    if actions.is_empty() {
        warnings.push(format!(
            "{}: lifecycle has no transitions; Next is stuttering only",
            entity_id(model, lc.entity)
        ));
    }

    actions
}

fn build_invariants(
    model: &SemanticModel,
    ek: EntityKey,
    axes: &[SpecAxis],
    warnings: &mut Vec<String>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let col_to_var: HashMap<&str, &str> = axes
        .iter()
        .map(|a| (a.column.as_str(), a.var_name.as_str()))
        .collect();
    let eid = entity_id(model, ek);

    for (i, inv) in model
        .entity_invariants
        .iter()
        .filter(|inv| inv.entity == ek)
        .enumerate()
    {
        match invariant_to_tla(inv, &col_to_var) {
            Ok(formula) => {
                let name = format!("Inv_{eid}_{i}");
                out.push((sanitize_ident(&name), formula));
            }
            Err(reason) => warnings.push(format!("{eid}: invariant not exported: {reason}")),
        }
    }

    for (i, f) in model
        .forbidden_constraints
        .iter()
        .filter(|f| f.entity == ek)
        .enumerate()
    {
        match forbidden_to_tla(f, &col_to_var) {
            Ok(formula) => {
                let name = format!("Forbidden_{eid}_{i}");
                out.push((sanitize_ident(&name), formula));
            }
            Err(reason) => warnings.push(format!("{eid}: forbidden not exported: {reason}")),
        }
    }

    for (i, r) in model
        .required_constraints
        .iter()
        .filter(|r| r.entity == ek)
        .enumerate()
    {
        match required_to_tla(r, &col_to_var) {
            Ok(formula) => {
                let name = format!("Required_{eid}_{i}");
                out.push((sanitize_ident(&name), formula));
            }
            Err(reason) => warnings.push(format!("{eid}: required not exported: {reason}")),
        }
    }

    for (i, ex) in model
        .exclusive_constraints
        .iter()
        .filter(|e| e.entity == ek)
        .enumerate()
    {
        match exclusive_to_tla(ex, &col_to_var) {
            Ok(formula) => {
                let name = format!("Exclusive_{eid}_{i}");
                out.push((sanitize_ident(&name), formula));
            }
            Err(reason) => warnings.push(format!("{eid}: exclusive not exported: {reason}")),
        }
    }

    out
}

fn build_properties_for_entity(
    model: &SemanticModel,
    entity_id: &str,
    axes: &[SpecAxis],
    warnings: &mut Vec<String>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    // Global var map across all axes of this entity (qualified names in formulas use Entity.col).
    let mut lookup: HashMap<String, String> = HashMap::new();
    for ax in axes {
        lookup.insert(ax.column.clone(), ax.var_name.clone());
        lookup.insert(
            format!("{entity_id}.{}", ax.column.trim_start_matches("__cmp:")),
            ax.var_name.clone(),
        );
        if let Some(rest) = ax.column.strip_prefix("__cmp:") {
            lookup.insert(format!("{entity_id}.{rest}"), ax.var_name.clone());
        }
    }

    for prop in &model.temporal_properties {
        if !property_mentions_entity(prop, entity_id) {
            continue;
        }
        match temporal_property_to_tla(prop, &lookup) {
            Ok(formula) => out.push((sanitize_ident(&prop.id), formula)),
            Err(reason) => warnings.push(format!("property {}: not exported: {reason}", prop.id)),
        }
    }
    out
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

fn note_skipped_constraints(
    model: &SemanticModel,
    lifecycles: &[EntityLifecycle],
    warnings: &mut Vec<String>,
) {
    let lifecycle_entities: BTreeSet<EntityKey> = lifecycles.iter().map(|l| l.entity).collect();
    if !model.cross_forbidden_constraints.is_empty() {
        warnings.push(format!(
            "{} cross_forbidden constraint(s) not exported (Phase 3)",
            model.cross_forbidden_constraints.len()
        ));
    }
    if !model.cross_entity_invariants.is_empty() {
        warnings.push(format!(
            "{} cross_invariant constraint(s) not exported (Phase 3)",
            model.cross_entity_invariants.len()
        ));
    }
    if !model.quantifier_constraints.is_empty() {
        warnings.push(format!(
            "{} quantifier constraint(s) not exported (Phase 3)",
            model.quantifier_constraints.len()
        ));
    }
    if !model.temporal_assertions.is_empty() {
        warnings.push(format!(
            "{} after(...).assert constraint(s) not yet mapped to TLA action postconditions",
            model.temporal_assertions.len()
        ));
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
) -> Result<String, String> {
    let guards = conditions_and_comps_to_tla(&inv.guards, &inv.guard_comparisons, col_to_var)?;
    let reqs = conditions_and_comps_to_tla(&inv.requireds, &inv.required_comparisons, col_to_var)?;
    if guards.is_empty() {
        Ok(reqs)
    } else {
        Ok(format!("({guards}) => ({reqs})"))
    }
}

fn forbidden_to_tla(
    f: &ForbiddenConstraint,
    col_to_var: &HashMap<&str, &str>,
) -> Result<String, String> {
    let body = conditions_and_comps_to_tla(&f.conditions, &f.comparisons, col_to_var)?;
    Ok(format!("~({body})"))
}

fn required_to_tla(
    r: &RequiredConstraint,
    col_to_var: &HashMap<&str, &str>,
) -> Result<String, String> {
    conditions_and_comps_to_tla(&r.conditions, &r.comparisons, col_to_var)
}

fn exclusive_to_tla(
    ex: &ExclusiveConstraint,
    col_to_var: &HashMap<&str, &str>,
) -> Result<String, String> {
    let mut atoms = Vec::new();
    for (col, val) in &ex.conditions {
        atoms.push(eq_atom(col, val, col_to_var)?);
    }
    for c in &ex.comparisons {
        atoms.push(comp_atom(c, col_to_var)?);
    }
    if atoms.len() < 2 {
        return Err("exclusive needs at least two conditions".into());
    }
    // At most one of the atoms may hold: pairwise mutual exclusion.
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
) -> Result<String, String> {
    let mut parts = Vec::new();
    for (col, val) in conditions {
        parts.push(eq_atom(col, val, col_to_var)?);
    }
    for c in comparisons {
        parts.push(comp_atom(c, col_to_var)?);
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

fn comp_atom(c: &ComparisonProp, col_to_var: &HashMap<&str, &str>) -> Result<String, String> {
    let key = format!("__cmp:{}", c.axis_key());
    let var = col_to_var
        .get(key.as_str())
        .ok_or_else(|| format!("comparison `{}` is not a TLA state axis", c.axis_key()))?;
    Ok(format!("{var} = TRUE"))
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
            let var = lookup
                .get(&key)
                .or_else(|| lookup.get(column))
                .ok_or_else(|| format!("unresolved column `{key}` in temporal property"))?;
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
                    lookup
                        .get(&rhs_key)
                        .or_else(|| lookup.get(rhs_col))
                        .cloned()
                        .ok_or_else(|| {
                            format!("unresolved column `{rhs_key}` in temporal property")
                        })?
                }
            };
            let op_s = match op {
                CmpOpModel::Eq => "=",
                CmpOpModel::Ne => "#",
                CmpOpModel::Lt => "<",
                CmpOpModel::Le => "<=",
                CmpOpModel::Gt => ">",
                CmpOpModel::Ge => ">=",
            };
            Ok(format!("{var} {op_s} {rhs_s}"))
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

fn render_tla(
    module_name: &str,
    model: &SemanticModel,
    specs: &[EntitySpec],
    warnings: &[String],
) -> String {
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

    let all_vars: Vec<String> = specs
        .iter()
        .flat_map(|s| s.axes.iter().map(|a| a.var_name.clone()))
        .collect();

    if all_vars.is_empty() {
        out.push_str("\\* No entity lifecycles with exportable state axes.\n");
        out.push_str("====\n");
        return out;
    }

    out.push_str("VARIABLES ");
    out.push_str(&all_vars.join(", "));
    out.push_str("\n\n");

    out.push_str("vars == <<");
    out.push_str(&all_vars.join(", "));
    out.push_str(">>\n\n");

    // TypeOK
    out.push_str("TypeOK ==\n");
    for ax in specs.iter().flat_map(|s| s.axes.iter()) {
        out.push_str(&format!("  /\\ {} \\in {}\n", ax.var_name, ax.domain));
    }
    out.push('\n');

    // Init
    out.push_str("Init ==\n");
    for spec in specs {
        for (var, val) in &spec.init {
            out.push_str(&format!("  /\\ {var} = {val}\n"));
        }
    }
    out.push('\n');

    // Actions
    for spec in specs {
        for action in &spec.actions {
            out.push_str(&format!("\\* {}\n", action.comment));
            out.push_str(&format!("{} ==\n", action.name));
            for g in &action.guards {
                out.push_str(&format!("  /\\ {g}\n"));
            }
            for e in &action.effects {
                out.push_str(&format!("  /\\ {e}\n"));
            }
            if action.unchanged.is_empty() {
                // all vars changed — nothing
            } else if action.unchanged.len() == 1 {
                out.push_str(&format!("  /\\ UNCHANGED {}\n", action.unchanged[0]));
            } else {
                out.push_str(&format!(
                    "  /\\ UNCHANGED <<{}>>\n",
                    action.unchanged.join(", ")
                ));
            }
            out.push('\n');
        }
    }

    // Next
    let action_names: Vec<String> = specs
        .iter()
        .flat_map(|s| s.actions.iter().map(|a| a.name.clone()))
        .collect();
    out.push_str("Next ==\n");
    if action_names.is_empty() {
        out.push_str("  UNCHANGED vars\n\n");
    } else {
        for name in action_names {
            out.push_str(&format!("  \\/ {name}\n"));
        }
        out.push('\n');
    }

    out.push_str("Spec == Init /\\ [][Next]_vars\n\n");

    // Invariants
    let mut inv_names = Vec::new();
    for spec in specs {
        for (name, formula) in &spec.invariants {
            out.push_str(&format!("{name} == {formula}\n"));
            inv_names.push(name.clone());
        }
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

    // Temporal properties (dedupe across entities — emit once globally)
    let mut emitted_props: BTreeSet<String> = BTreeSet::new();
    let mut prop_names = Vec::new();
    for spec in specs {
        for (name, formula) in &spec.properties {
            if emitted_props.insert(name.clone()) {
                out.push_str(&format!("{name} == {formula}\n"));
                prop_names.push(name.clone());
            }
        }
    }
    // Also emit properties that might not have matched entity filter via global rebuild
    for prop in &model.temporal_properties {
        let name = sanitize_ident(&prop.id);
        if emitted_props.contains(&name) {
            continue;
        }
        // Build lookup from all specs
        let mut lookup: HashMap<String, String> = HashMap::new();
        for spec in specs {
            for ax in &spec.axes {
                lookup.insert(ax.column.clone(), ax.var_name.clone());
                lookup.insert(
                    format!(
                        "{}.{}",
                        spec.entity_id,
                        ax.column.trim_start_matches("__cmp:")
                    ),
                    ax.var_name.clone(),
                );
            }
        }
        if let Ok(formula) = temporal_property_to_tla(prop, &lookup) {
            out.push_str(&format!("{name} == {formula}\n"));
            prop_names.push(name);
        }
    }
    if !prop_names.is_empty() {
        out.push('\n');
    }

    out.push_str("THEOREM Spec => []Safety\n");
    out.push_str("====\n");
    out
}

fn render_cfg(module_name: &str, specs: &[EntitySpec]) -> String {
    let mut out = String::new();
    out.push_str("SPECIFICATION Spec\n");
    out.push_str("INVARIANT Safety\n");

    let mut prop_names: BTreeSet<String> = BTreeSet::new();
    for spec in specs {
        for (name, _) in &spec.properties {
            prop_names.insert(name.clone());
        }
    }
    for name in prop_names {
        out.push_str(&format!("PROPERTY {name}\n"));
    }

    // TLC needs the module; CONSTANTS none for now.
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

fn escape_tla_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

// Silence unused import when TemporalExpr path used fully-qualified.
#[allow(dead_code)]
fn _axis_kind_ref(_: &AxisKind) {}

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

    const ORDER_SRC: &str = r#"
entity Order "注文" {
  id:           Int @pk
  status:       Enum(pending, paid, shipped, delivered, cancelled) @default(pending)
  delivered_at: DateTime @null
}
usecase PlaceOrder  "注文確定"
usecase CapturePay  "決済確定"
usecase ShipOrder   "発送"
usecase DeliverOrder "配達確認"
usecase CancelOrder "注文キャンセル"
event EvCapture  "決済確定"
event EvShip     "発送開始"
event EvDeliver  "配達確認"
event EvCancel   "注文キャンセル"
creates(PlaceOrder, Order)
updates(CapturePay,   Order)
updates(ShipOrder,    Order)
updates(DeliverOrder, Order)
updates(CancelOrder,  Order)
raises(CapturePay,   EvCapture)
raises(ShipOrder,    EvShip)
raises(DeliverOrder, EvDeliver)
raises(CancelOrder,  EvCancel)
state Pending   "注文受付"
state Paid      "決済完了"
state Shipped   "発送済"
state Delivered "配達完了"
state Cancelled "キャンセル"
transitions(Order.status, EvCapture, pending -> paid)
transitions(Order.status, EvShip, paid -> shipped)
transitions(Order.status, EvDeliver, shipped -> delivered)
transitions(Order.status, EvCancel, pending -> cancelled)
sets(usecase::DeliverOrder, Order, delivered_at == present)
forbidden(Order, status == cancelled, delivered_at == present)
invariant(Order)
  .when(status == delivered)
  .then(delivered_at == present)
"#;

    #[test]
    fn tla_order_snapshot() {
        let model = model_from(ORDER_SRC);
        let bundle = TlaPlusEmitter::default()
            .emit_bundle(&model, &View::whole())
            .unwrap();
        insta::assert_snapshot!(bundle.tla);
        insta::assert_snapshot!("order_cfg", bundle.cfg);
    }
}
