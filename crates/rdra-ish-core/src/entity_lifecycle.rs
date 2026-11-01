//! The lifecycle columns of an entity, and the state node each value they move between derives.
//!
//! An entity may have several lifecycle columns: each Enum column `transitions` names is its own
//! state model, and its values are its own states.

use crate::model::{EntityKey, SemanticModel, State, StateKey, StateTransition};
use std::collections::{BTreeMap, BTreeSet};

/// The columns `transitions` names on `entity`, sorted.
pub fn lifecycle_columns(model: &SemanticModel, entity: EntityKey) -> BTreeSet<&str> {
    model
        .state_transitions
        .iter()
        .filter(|st| st.entity == entity)
        .map(|st| st.column.as_str())
        .collect()
}

/// The column a single-tag view of `entity` (a TypeScript union) discriminates on: `status`,
/// then a `*_status` column, then the first lifecycle column by name.
pub fn discriminator_column(model: &SemanticModel, entity: EntityKey) -> Option<&str> {
    let columns = lifecycle_columns(model, entity);
    columns
        .iter()
        .find(|c| **c == "status" || c.ends_with("_status"))
        .or_else(|| columns.iter().next())
        .copied()
}

/// The id of the state `value` of `entity.column`: `{Entity}_{value}`, or
/// `{Entity}_{column}_{value}` when the entity has more than one lifecycle column.
pub fn state_id(model: &SemanticModel, entity: EntityKey, column: &str, value: &str) -> String {
    let entity_id = &model.entities[entity].id;
    if lifecycle_columns(model, entity).len() > 1 {
        format!("{entity_id}_{column}_{value}")
    } else {
        format!("{entity_id}_{value}")
    }
}

/// Derive one state node per value a `transitions` arm moves from or to, labelled by the
/// column's `Enum` label for the value.
pub(crate) fn derive_lifecycle_states(model: &mut SemanticModel) {
    let mut seen: BTreeSet<(EntityKey, &str, &str)> = BTreeSet::new();
    let mut states: Vec<State> = Vec::new();
    for st in &model.state_transitions {
        for value in [&st.from, &st.to] {
            if !seen.insert((st.entity, st.column.as_str(), value.as_str())) {
                continue;
            }
            let label = model.entities[st.entity]
                .columns
                .iter()
                .find(|c| c.name == st.column)
                .and_then(|c| c.value_label(value))
                .unwrap_or(value);
            states.push(State {
                id: state_id(model, st.entity, &st.column, value),
                label: label.to_string(),
                description: None,
                entity: st.entity,
                column: st.column.clone(),
                value: value.clone(),
            });
        }
    }
    for state in states {
        model.states.insert(state);
    }
}

/// The state node of `value` on `entity.column`, when a `transitions` arm names the value.
pub fn state_key(
    model: &SemanticModel,
    entity: EntityKey,
    column: &str,
    value: &str,
) -> Option<StateKey> {
    model
        .states
        .iter()
        .find(|(_, s)| s.entity == entity && s.column == column && s.value == value)
        .map(|(key, _)| key)
}

/// A lifecycle state's label, or the raw value when no `transitions` arm names it.
pub fn state_label(model: &SemanticModel, entity: EntityKey, column: &str, value: &str) -> String {
    state_key(model, entity, column, value)
        .map_or_else(|| value.to_string(), |key| model.states[key].label.clone())
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
