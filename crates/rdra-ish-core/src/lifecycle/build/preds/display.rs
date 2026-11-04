//! Rule text for diagnostics: the display of a local or cross-entity condition, a quantifier, and an entity list.

use crate::model::{
    ComparisonProp, CrossCmpRhs, CrossEntityCondition, EffectValue, EntityKey, QuantifierKind,
    RelatedQuantifier,
};

pub(super) fn local_display(eqs: &[(String, EffectValue)], cmps: &[ComparisonProp]) -> String {
    eqs.iter()
        .map(|(c, v)| format!("{c}={}", v.literal()))
        // A comparison prints as written, `status != oa` or `status in (a, b)`.
        .chain(cmps.iter().map(ComparisonProp::display))
        .collect::<Vec<_>>()
        .join(" AND ")
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
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// `.none(Related.col == v).role(role)` as written, for a diagnostic that names the call.
pub(super) fn quantifier_display(
    model: &crate::model::SemanticModel,
    q: &RelatedQuantifier,
) -> String {
    let method = match q.kind {
        QuantifierKind::Has => "has",
        QuantifierKind::None => "none",
    };
    let arg = if q.conditions.is_empty() {
        model.entities[q.related].id.clone()
    } else {
        "...".to_string()
    };
    format!(".{method}({arg})")
}

pub(super) fn entity_list(model: &crate::model::SemanticModel, scope: &[EntityKey]) -> String {
    scope
        .iter()
        .map(|ek| model.entities[*ek].id.clone())
        .collect::<Vec<_>>()
        .join(", ")
}
