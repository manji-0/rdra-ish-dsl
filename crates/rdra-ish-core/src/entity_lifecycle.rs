//! エンティティの status Enum 列と、その列の遷移が結ぶ state ノードの対応。

use crate::model::{ColumnType, EntityKey, SemanticModel, StateKey, StateTransition};
use std::collections::{BTreeMap, BTreeSet, HashMap};

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

/// The state node of `variant` on `entity`'s lifecycle column (`{Entity}_{variant}`, exact case).
pub fn state_key(model: &SemanticModel, entity: EntityKey, variant: &str) -> Option<StateKey> {
    let id = format!("{}_{variant}", model.entities.get(entity)?.id);
    model
        .states
        .iter()
        .find(|(_, state)| state.id == id)
        .map(|(key, _)| key)
}

/// A lifecycle state's label, or the raw variant when the entity declares no such state.
pub fn state_label(model: &SemanticModel, entity: EntityKey, variant: &str) -> String {
    state_key(model, entity, variant)
        .and_then(|key| model.states.get(key))
        .map_or_else(|| variant.to_string(), |state| state.label.clone())
}

/// One state model: the transitions of one lifecycle column, its states in `Enum` order.
pub struct StateModel<'a> {
    pub entity: EntityKey,
    pub entity_id: &'a str,
    pub column: &'a str,
    pub variants: Vec<&'a str>,
    pub transitions: Vec<&'a StateTransition>,
}

impl StateModel<'_> {
    /// Where `variant` sits in the column's `Enum` order; an undeclared value sorts last.
    pub fn position(&self, variant: &str) -> usize {
        self.variants
            .iter()
            .position(|v| *v == variant)
            .unwrap_or(usize::MAX)
    }
}

/// Every state model, sorted by entity id and column. A transition on an entity the model
/// does not hold is skipped.
pub fn state_models(model: &SemanticModel) -> Vec<StateModel<'_>> {
    let mut groups: BTreeMap<(&str, &str), StateModel<'_>> = BTreeMap::new();
    for transition in &model.state_transitions {
        let Some(entity) = model.entities.get(transition.entity) else {
            continue;
        };
        let group = groups
            .entry((entity.id.as_str(), transition.column.as_str()))
            .or_insert_with(|| StateModel {
                entity: transition.entity,
                entity_id: &entity.id,
                column: &transition.column,
                variants: entity
                    .columns
                    .iter()
                    .find(|c| c.name == transition.column)
                    .and_then(|c| model.column_values(c))
                    .map(|values| values.iter().map(String::as_str).collect())
                    .unwrap_or_default(),
                transitions: Vec::new(),
            });
        group.transitions.push(transition);
    }
    groups.into_values().collect()
}
