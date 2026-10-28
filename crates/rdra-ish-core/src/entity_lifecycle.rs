//! エンティティの status Enum 列と、その列の遷移が結ぶ state ノードの対応。

use crate::model::{ColumnType, EntityKey, SemanticModel, StateKey};
use std::collections::{BTreeSet, HashMap};

/// Pick the lifecycle status column for an entity from its transitions.
/// Prefers `status` / `*_status`, then lexicographically first column.
fn preferred_lifecycle_column(model: &SemanticModel, ek: EntityKey) -> Option<String> {
    let cols: BTreeSet<String> = model
        .state_transitions
        .iter()
        .filter(|st| st.entity == ek)
        .map(|st| st.column.clone())
        .collect();
    if cols.is_empty() {
        return None;
    }
    cols.iter()
        .find(|c| *c == "status" || c.ends_with("_status"))
        .cloned()
        .or_else(|| cols.iter().next().cloned())
}

/// Resolve a transition/state variant string to the declared Enum casing.
fn declared_variant_casing(
    model: &SemanticModel,
    ek: EntityKey,
    column: &str,
    variant: &str,
) -> String {
    model
        .entities
        .get(ek)
        .and_then(|e| e.columns.iter().find(|c| c.name == column))
        .and_then(|c| match &c.col_type {
            ColumnType::Enum(variants) => variants
                .iter()
                .find(|v| *v == variant || v.eq_ignore_ascii_case(variant))
                .cloned(),
            _ => None,
        })
        .unwrap_or_else(|| variant.to_string())
}

/// entity の status 列と、その列の遷移が結ぶ entity-scoped state (`{Entity}_{variant}`) を返す。
///
/// state ノードは `transitions(Entity.col, ...)` から導出されるもののみ。遷移がなければ `None`。
/// Map values use the **declared Enum casing** (not lowercased).
pub fn link_entity_status_states(
    model: &SemanticModel,
    ek: EntityKey,
) -> Option<(String, HashMap<StateKey, String>)> {
    let entity_id = &model.entities[ek].id;
    let column = preferred_lifecycle_column(model, ek)?;
    let mut map: HashMap<StateKey, String> = HashMap::new();
    for st in model
        .state_transitions
        .iter()
        .filter(|st| st.entity == ek && st.column == column)
    {
        for variant_name in [&st.from, &st.to] {
            let declared = declared_variant_casing(model, ek, &column, variant_name);
            let state_id = format!("{entity_id}_{declared}");
            if let Some((sk, _)) = model.states.iter().find(|(_, s)| s.id == state_id) {
                map.insert(sk, declared);
            }
        }
    }
    (!map.is_empty()).then_some((column, map))
}
