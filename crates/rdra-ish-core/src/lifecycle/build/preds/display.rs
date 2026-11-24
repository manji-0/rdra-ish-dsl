//! Rule text for diagnostics: the display of a local or cross-entity condition, a quantifier, and an entity list.

use crate::model::{
    ComparisonProp, CrossCmpRhs, CrossEntityCondition, EffectValue, EntityCondition, EntityKey,
    RelatedQuantifier,
};

pub(super) fn local_display(conds: &[EntityCondition]) -> String {
    local_displays(conds).join(" AND ")
}

/// Each condition as written, in the order `Builder::local_conds` builds their predicates:
/// the equalities, then the comparisons.
pub(super) fn local_displays(conds: &[EntityCondition]) -> Vec<String> {
    EntityCondition::equals(conds)
        .map(|(c, v)| format!("{c}={}", v.literal()))
        // A comparison prints as written, `status != oa` or `status in (a, b)`.
        .chain(EntityCondition::comparisons(conds).map(ComparisonProp::display))
        .collect()
}

pub(super) fn cross_rhs_display(model: &crate::model::SemanticModel, rhs: &CrossCmpRhs) -> String {
    match rhs {
        CrossCmpRhs::Column(c) => format!("{}.{}", model.entities[c.entity].id, c.column),
        CrossCmpRhs::IntLit(n) => n.to_string(),
        CrossCmpRhs::Now => "now".to_string(),
        CrossCmpRhs::Value(v) => v.literal(),
        CrossCmpRhs::Values(vs) => crate::model::values_display(vs, EffectValue::literal),
    }
}

pub(super) fn cross_display(
    model: &crate::model::SemanticModel,
    conds: &[CrossEntityCondition],
) -> String {
    cross_displays(model, conds).join(" AND ")
}

/// Each condition as written, in the order `Builder::cross_all` builds their predicates.
pub(super) fn cross_displays(
    model: &crate::model::SemanticModel,
    conds: &[CrossEntityCondition],
) -> Vec<String> {
    conds
        .iter()
        .map(|c| match c {
            CrossEntityCondition::Equals { column, value } => format!(
                "{}.{}={}",
                model.entities[column.entity].id,
                column.column,
                value.literal()
            ),
            CrossEntityCondition::Comparison(p) => format!(
                "{}.{} {} {}",
                model.entities[p.lhs.entity].id,
                p.lhs.column,
                p.op.as_str(),
                cross_rhs_display(model, &p.rhs)
            ),
        })
        .collect()
}

/// `.none(Related.col == v).role(role)` as written, for a diagnostic that names the call.
pub(super) fn quantifier_display(
    model: &crate::model::SemanticModel,
    q: &RelatedQuantifier,
) -> String {
    let method = q.kind.method();
    let arg = if q.conditions.is_empty() {
        model.entities[q.related].id.clone()
    } else {
        "...".to_string()
    };
    format!(".{method}({}{arg})", q.kind.count_arg())
}

pub(super) fn entity_list(model: &crate::model::SemanticModel, scope: &[EntityKey]) -> String {
    scope
        .iter()
        .map(|ek| model.entities[*ek].id.clone())
        .collect::<Vec<_>>()
        .join(", ")
}
