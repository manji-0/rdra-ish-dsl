//! Which columns something names (R-var): a column becomes a state var only when a `sets`,
//! a constraint, an applied or `.when` condition body, a guarded `chooses` value, an assert,
//! or a property reads or writes it.

use std::collections::HashSet;

use crate::analysis::applied_bodies;
use crate::model::{
    CheckExpr, CmpRhs, ColumnType, ComparisonProp, ConditionKey, CrossCmpRhs, CrossEntityCondition,
    DerivedExpr, EffectValue, EntityKey, SemanticModel, SetValue, TemporalExpr, TemporalRhs,
};

/// The conditions of every `.when(Cond)` guard, on a `transitions` arm, a `chooses` value or a
/// `sets` value.
fn when_guards(model: &SemanticModel) -> impl Iterator<Item = ConditionKey> + '_ {
    let arms = model.state_transitions.iter().filter_map(|t| t.guard);
    let values = model
        .column_choices
        .iter()
        .flat_map(|c| c.values.iter().filter_map(|v| v.guard));
    let sets = model.column_effects.iter().filter_map(|ce| ce.guard);
    arms.chain(values).chain(sets)
}

#[derive(Default)]
pub(super) struct Named {
    /// Named at all (variation columns become vars).
    pub any: HashSet<(EntityKey, String)>,
    /// Named by an Int literal or a comparison (scalar columns become Int vars).
    pub numeric: HashSet<(EntityKey, String)>,
    pub reads_now: bool,
}

impl Named {
    pub fn collect(model: &SemanticModel) -> Self {
        let mut n = Named::default();
        for ce in &model.column_effects {
            match &ce.value {
                SetValue::Value(v) => n.value(ce.entity, &ce.column, v),
                SetValue::Copy(source) => {
                    n.typed(model, ce.entity, &ce.column);
                    n.typed(model, source.entity, &source.column);
                }
            }
        }
        let local_eqs = model
            .forbidden_constraints
            .iter()
            .map(|c| (c.entity, &c.conditions))
            .chain(
                model
                    .required_constraints
                    .iter()
                    .map(|c| (c.entity, &c.conditions)),
            )
            .chain(
                model
                    .exclusive_constraints
                    .iter()
                    .map(|c| (c.entity, &c.conditions)),
            )
            .chain(
                model
                    .entity_invariants
                    .iter()
                    .map(|c| (c.entity, &c.guards)),
            )
            .chain(
                model
                    .entity_invariants
                    .iter()
                    .map(|c| (c.entity, &c.requireds)),
            );
        for (ek, conds) in local_eqs {
            for (column, value) in conds {
                n.value(ek, column, value);
            }
        }
        let local_cmps = model
            .forbidden_constraints
            .iter()
            .map(|c| (c.entity, &c.comparisons))
            .chain(
                model
                    .required_constraints
                    .iter()
                    .map(|c| (c.entity, &c.comparisons)),
            )
            .chain(
                model
                    .exclusive_constraints
                    .iter()
                    .map(|c| (c.entity, &c.comparisons)),
            )
            .chain(
                model
                    .entity_invariants
                    .iter()
                    .map(|c| (c.entity, &c.guard_comparisons)),
            )
            .chain(
                model
                    .entity_invariants
                    .iter()
                    .map(|c| (c.entity, &c.required_comparisons)),
            );
        for (ek, cmps) in local_cmps {
            for prop in cmps {
                if !is_driven(model, ek, prop) {
                    n.comparison(ek, prop);
                }
            }
        }
        let cross = model
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
                    .flat_map(|c| c.guards.iter().chain(c.quantifier.conditions.iter())),
            )
            .chain(
                model
                    .temporal_assertions
                    .iter()
                    .flat_map(|c| c.requireds.iter()),
            )
            .chain(
                applied_bodies(model)
                    .into_iter()
                    .flat_map(|b| b.body.read_conditions()),
            )
            .chain(
                when_guards(model)
                    .filter_map(|ck| model.conditions[ck].body.as_ref())
                    .flat_map(|body| body.read_conditions()),
            );
        for cond in cross {
            n.cross(model, cond);
        }
        // A guarded `chooses` value is read by its guard on the successor.
        for choice in &model.column_choices {
            if choice.values.iter().any(|v| v.guard.is_some()) {
                n.any.insert((choice.entity, choice.column.clone()));
            }
        }
        for (ek, entity) in &model.entities {
            for check in entity.columns.iter().flat_map(|c| &c.check_constraints) {
                n.check(model, ek, &check.expr);
            }
        }
        for prop in &model.temporal_properties {
            let (a, b) = match &prop.formula {
                crate::model::TemporalFormula::Always(e)
                | crate::model::TemporalFormula::Eventually(e) => (e, None),
                crate::model::TemporalFormula::LeadsTo {
                    antecedent,
                    consequent,
                } => (antecedent, Some(consequent)),
            };
            n.temporal(model, a);
            if let Some(b) = b {
                n.temporal(model, b);
            }
        }
        // A read of a logical `@derived` column reads the columns of its expression.
        for (ek, entity) in &model.entities {
            for col in &entity.columns {
                if let Some(DerivedExpr::Logical(expr)) = &col.derived_expr {
                    if n.any.contains(&(ek, col.name.clone())) {
                        n.check(model, ek, expr);
                    }
                }
            }
        }
        n
    }

    fn check(&mut self, model: &SemanticModel, ek: EntityKey, expr: &CheckExpr) {
        for prop in expr.comparisons() {
            if !is_driven(model, ek, prop) {
                self.comparison(ek, prop);
            }
        }
    }

    fn name(&mut self, ek: EntityKey, column: &str) {
        self.any.insert((ek, column.to_string()));
    }

    fn number(&mut self, ek: EntityKey, column: &str) {
        self.name(ek, column);
        self.numeric.insert((ek, column.to_string()));
    }

    /// A column compared with or copied from another column: an Int var when it is Int- or
    /// Date-typed.
    fn typed(&mut self, model: &SemanticModel, ek: EntityKey, column: &str) {
        let numeric = model.entities[ek].columns.iter().any(|c| {
            c.name == column
                && matches!(
                    c.col_type,
                    ColumnType::Int
                        | ColumnType::Money
                        | ColumnType::Decimal
                        | ColumnType::Date
                        | ColumnType::DateTime
                )
        });
        if numeric {
            self.number(ek, column);
        } else {
            self.name(ek, column);
        }
    }

    fn value(&mut self, ek: EntityKey, column: &str, value: &EffectValue) {
        if matches!(value, EffectValue::Int(_)) {
            self.number(ek, column);
        } else {
            self.name(ek, column);
        }
    }

    fn comparison(&mut self, ek: EntityKey, prop: &ComparisonProp) {
        match &prop.rhs {
            CmpRhs::Value(v) => return self.value(ek, &prop.lhs_column, v),
            CmpRhs::Values(vs) => {
                return vs.iter().for_each(|v| self.value(ek, &prop.lhs_column, v));
            }
            _ => {}
        }
        self.number(ek, &prop.lhs_column);
        match &prop.rhs {
            CmpRhs::Column(c) => self.number(ek, c),
            CmpRhs::IntLit(_) | CmpRhs::Value(_) | CmpRhs::Values(_) => {}
            CmpRhs::Now => self.reads_now = true,
        }
    }

    fn cross(&mut self, model: &SemanticModel, cond: &CrossEntityCondition) {
        match cond {
            CrossEntityCondition::Equals { column, value } => {
                self.value(column.entity, &column.column, value)
            }
            CrossEntityCondition::Comparison(prop) => {
                let ordered = prop.op.is_ordered();
                match &prop.rhs {
                    CrossCmpRhs::Column(c) if ordered => {
                        self.number(prop.lhs.entity, &prop.lhs.column);
                        self.number(c.entity, &c.column);
                    }
                    CrossCmpRhs::Column(c) => {
                        self.typed(model, prop.lhs.entity, &prop.lhs.column);
                        self.typed(model, c.entity, &c.column);
                    }
                    CrossCmpRhs::IntLit(_) => self.number(prop.lhs.entity, &prop.lhs.column),
                    CrossCmpRhs::Value(v) => self.value(prop.lhs.entity, &prop.lhs.column, v),
                    CrossCmpRhs::Values(vs) => vs
                        .iter()
                        .for_each(|v| self.value(prop.lhs.entity, &prop.lhs.column, v)),
                    CrossCmpRhs::Now => {
                        self.number(prop.lhs.entity, &prop.lhs.column);
                        self.reads_now = true;
                    }
                }
            }
        }
    }

    fn temporal(&mut self, model: &SemanticModel, expr: &TemporalExpr) {
        match expr {
            TemporalExpr::Atom(atom) => {
                let Some(ek) = property_entity(model, atom.entity.as_deref(), &atom.column) else {
                    return;
                };
                let ordered = atom.op.is_ordered();
                match &atom.rhs {
                    TemporalRhs::Value(v) if !ordered => self.value(ek, &atom.column, v),
                    TemporalRhs::Values(vs) => {
                        vs.iter().for_each(|v| self.value(ek, &atom.column, v))
                    }
                    TemporalRhs::Column { entity, column } => {
                        let rk = property_entity(
                            model,
                            entity.as_deref().or(atom.entity.as_deref()),
                            column,
                        );
                        if ordered {
                            self.number(ek, &atom.column);
                            if let Some(rk) = rk {
                                self.number(rk, column);
                            }
                        } else {
                            self.name(ek, &atom.column);
                            if let Some(rk) = rk {
                                self.name(rk, column);
                            }
                        }
                    }
                    _ => self.number(ek, &atom.column),
                }
            }
            TemporalExpr::Not(inner) => self.temporal(model, inner),
            TemporalExpr::And(a, b) | TemporalExpr::Or(a, b) => {
                self.temporal(model, a);
                self.temporal(model, b);
            }
        }
    }
}

/// A property atom's entity: the named one, or the only entity that has the column.
pub(super) fn property_entity(
    model: &SemanticModel,
    entity: Option<&str>,
    column: &str,
) -> Option<EntityKey> {
    model.property_entity(entity, column)
}

/// A `drives(UC, E, cmp, bool)` names this comparison, so it reads a proposition var.
pub(super) fn is_driven(model: &SemanticModel, ek: EntityKey, prop: &ComparisonProp) -> bool {
    let key = prop.axis_key();
    model
        .proposition_effects
        .iter()
        .any(|pe| pe.entity == ek && pe.prop.axis_key() == key)
}

/// Driven proposition keys of an entity, in the order `states` has listed them: constraint
/// comparisons first, then the remaining `drives(.., cmp, bool)` keys.
pub(super) fn driven_props(model: &SemanticModel, ek: EntityKey) -> Vec<String> {
    let constraint_props = model
        .forbidden_constraints
        .iter()
        .filter(|c| c.entity == ek)
        .flat_map(|c| c.comparisons.iter())
        .chain(
            model
                .entity_invariants
                .iter()
                .filter(|c| c.entity == ek)
                .flat_map(|c| {
                    c.guard_comparisons
                        .iter()
                        .chain(c.required_comparisons.iter())
                }),
        )
        .chain(
            model
                .required_constraints
                .iter()
                .filter(|c| c.entity == ek)
                .flat_map(|c| c.comparisons.iter()),
        )
        .chain(
            model
                .exclusive_constraints
                .iter()
                .filter(|c| c.entity == ek)
                .flat_map(|c| c.comparisons.iter()),
        );
    let effect_props = model
        .proposition_effects
        .iter()
        .filter(|pe| pe.entity == ek)
        .map(|pe| &pe.prop);
    let mut keys: Vec<String> = Vec::new();
    for prop in constraint_props.chain(effect_props) {
        let key = prop.axis_key();
        if is_driven(model, ek, prop) && !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}
