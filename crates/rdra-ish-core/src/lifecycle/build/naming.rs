//! Which columns something names (R-var): a column becomes a state var only when a `sets`,
//! a constraint, an applied condition body, an assert, or a property reads or writes it.

use std::collections::HashSet;

use crate::model::{
    CmpRhs, ComparisonProp, ConditionKey, CrossCmpRhs, CrossEntityCondition, EffectValue,
    EntityKey, HoldsBody, NodeRef, RelKind, SemanticModel, TemporalExpr, TemporalRhs, UseCaseKey,
};

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
            n.value(ce.entity, &ce.column, &ce.value);
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
                    .flat_map(|c| c.guards.iter().chain(c.related_conditions.iter())),
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
                    .flat_map(|(_, _, body)| body.conds.iter()),
            );
        for cond in cross {
            n.cross(cond);
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
        n
    }

    fn name(&mut self, ek: EntityKey, column: &str) {
        self.any.insert((ek, column.to_string()));
    }

    fn number(&mut self, ek: EntityKey, column: &str) {
        self.name(ek, column);
        self.numeric.insert((ek, column.to_string()));
    }

    fn value(&mut self, ek: EntityKey, column: &str, value: &EffectValue) {
        if matches!(value, EffectValue::Int(_)) {
            self.number(ek, column);
        } else {
            self.name(ek, column);
        }
    }

    fn comparison(&mut self, ek: EntityKey, prop: &ComparisonProp) {
        self.number(ek, &prop.lhs_column);
        match &prop.rhs {
            CmpRhs::Column(c) => self.number(ek, c),
            CmpRhs::IntLit(_) => {}
            CmpRhs::Now => self.reads_now = true,
        }
    }

    fn cross(&mut self, cond: &CrossEntityCondition) {
        match cond {
            CrossEntityCondition::Equals { column, value } => {
                self.value(column.entity, &column.column, value)
            }
            CrossEntityCondition::Comparison(prop) => {
                let ordered = !matches!(
                    prop.op,
                    crate::model::CmpOpModel::Eq | crate::model::CmpOpModel::Ne
                );
                match &prop.rhs {
                    CrossCmpRhs::Column(c) if ordered => {
                        self.number(prop.lhs.entity, &prop.lhs.column);
                        self.number(c.entity, &c.column);
                    }
                    CrossCmpRhs::Column(c) => {
                        self.name(prop.lhs.entity, &prop.lhs.column);
                        self.name(c.entity, &c.column);
                    }
                    CrossCmpRhs::IntLit(_) => self.number(prop.lhs.entity, &prop.lhs.column),
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
                let ordered = !matches!(
                    atom.op,
                    crate::model::CmpOpModel::Eq | crate::model::CmpOpModel::Ne
                );
                match &atom.rhs {
                    TemporalRhs::Value(v) if !ordered => self.value(ek, &atom.column, v),
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

/// Every `holds` body that guards a use case: `applies_to(Cond, UC)`, or `applies_to(Cond, Api)`
/// for each use case that invokes the API. In relation order, without duplicates.
pub(super) fn applied_bodies(model: &SemanticModel) -> Vec<(ConditionKey, UseCaseKey, &HoldsBody)> {
    let mut out: Vec<(ConditionKey, UseCaseKey, &HoldsBody)> = Vec::new();
    for rel in model
        .relations
        .iter()
        .filter(|r| r.kind == RelKind::AppliesTo)
    {
        let NodeRef::Condition(ck) = rel.from else {
            continue;
        };
        let Some(body) = model.conditions[ck].body.as_ref() else {
            continue;
        };
        let ucs: Vec<UseCaseKey> = match rel.to {
            NodeRef::UseCase(uc) => vec![uc],
            NodeRef::Api(api) => model
                .relations
                .iter()
                .filter_map(|r| match (&r.kind, &r.from, &r.to) {
                    (RelKind::Invokes, NodeRef::UseCase(uc), NodeRef::Api(a)) if *a == api => {
                        Some(*uc)
                    }
                    _ => None,
                })
                .collect(),
            _ => continue,
        };
        for uc in ucs {
            if !out.iter().any(|(c, u, _)| (*c, *u) == (ck, uc)) {
                out.push((ck, uc, body));
            }
        }
    }
    out
}

/// A property atom's entity: the named one, or the only entity that has the column.
pub(super) fn property_entity(
    model: &SemanticModel,
    entity: Option<&str>,
    column: &str,
) -> Option<EntityKey> {
    match entity {
        Some(id) => model
            .entities
            .iter()
            .find(|(_, e)| e.id == id)
            .map(|(k, _)| k),
        None => {
            let mut owners = model
                .entities
                .iter()
                .filter(|(_, e)| e.columns.iter().any(|c| c.name == column))
                .map(|(k, _)| k);
            let first = owners.next()?;
            owners.next().is_none().then_some(first)
        }
    }
}

/// `sets(UC, E, cmp, bool)` drives this comparison, so it reads a proposition var.
pub(super) fn is_driven(model: &SemanticModel, ek: EntityKey, prop: &ComparisonProp) -> bool {
    let key = prop.axis_key();
    model
        .proposition_effects
        .iter()
        .any(|pe| pe.entity == ek && pe.prop.axis_key() == key)
}

/// Driven proposition keys of an entity, in the order `states` has listed them: constraint
/// comparisons first, then the remaining `sets(.., cmp, bool)` keys.
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
