//! Helpers the PlantUML and Mermaid emitters share.

use rdra_ish_core::lifecycle::{Bounds, Scope as LifecycleScope, TransitionSystem};
use rdra_ish_core::model::{ColumnType, EntityKey, NodeRef, SemanticModel, StateKey};
use std::collections::HashSet;

/// Start states of the lifecycle columns the visible transitions name, read from the
/// transition system (the creation status, else the column's base), sorted by node id.
pub(crate) fn lifecycle_start_states(
    model: &SemanticModel,
    columns: impl IntoIterator<Item = (EntityKey, String)>,
    resolve: impl Fn(EntityKey, &str) -> Option<StateKey>,
) -> Vec<StateKey> {
    let built = TransitionSystem::build(model, Bounds::STATES, &LifecycleScope::ALL);
    let columns: std::collections::BTreeSet<(EntityKey, String)> = columns.into_iter().collect();
    let mut starts: Vec<StateKey> = columns
        .iter()
        .flat_map(|(entity, column)| {
            built
                .ts
                .column_start_values(*entity, column)
                .into_iter()
                .filter_map(|variant| resolve(*entity, variant))
                .collect::<Vec<_>>()
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    starts.sort_by_key(|sk| node_id(model, &NodeRef::State(*sk)).unwrap_or(""));
    starts
}

pub(crate) fn node_id<'a>(model: &'a SemanticModel, node: &NodeRef) -> Option<&'a str> {
    match node {
        NodeRef::Actor(k) => model.actors.get(*k).map(|a| a.id.as_str()),
        NodeRef::ExtSystem(k) => model.ext_systems.get(*k).map(|e| e.id.as_str()),
        NodeRef::System(k) => model.systems.get(*k).map(|s| s.id.as_str()),
        NodeRef::Requirement(k) => model.requirements.get(*k).map(|r| r.id.as_str()),
        NodeRef::Adr(k) => model.adrs.get(*k).map(|a| a.id.as_str()),
        NodeRef::Nfr(k) => model.nfrs.get(*k).map(|n| n.id.as_str()),
        NodeRef::Quality(k) => model.qualities.get(*k).map(|q| q.id.as_str()),
        NodeRef::Constraint(k) => model.constraints.get(*k).map(|c| c.id.as_str()),
        NodeRef::Business(k) => model.businesses.get(*k).map(|b| b.id.as_str()),
        NodeRef::Buc(k) => model.bucs.get(*k).map(|b| b.id.as_str()),
        NodeRef::Flow(k) => model.flows.get(*k).map(|f| f.id.as_str()),
        NodeRef::Step(k) => model.steps.get(*k).map(|s| s.id.as_str()),
        NodeRef::UseCase(k) => model.use_cases.get(*k).map(|u| u.id.as_str()),
        NodeRef::Screen(k) => model.screens.get(*k).map(|s| s.id.as_str()),
        NodeRef::Field(k) => model.fields.get(*k).map(|f| f.id.as_str()),
        NodeRef::Event(k) => model.events.get(*k).map(|e| e.id.as_str()),
        NodeRef::Entity(k) => model.entities.get(*k).map(|e| e.id.as_str()),
        NodeRef::State(k) => model.states.get(*k).map(|s| s.id.as_str()),
        NodeRef::Api(k) => model.apis.get(*k).map(|a| a.id.as_str()),
        NodeRef::Dto(k) => model.dtos.get(*k).map(|d| d.id.as_str()),
        NodeRef::Timer(k) => model.timers.get(*k).map(|t| t.id.as_str()),
        NodeRef::Medium(k) => model.media.get(*k).map(|m| m.id.as_str()),
        NodeRef::Permission(k) => model.permissions.get(*k).map(|p| p.id.as_str()),
        NodeRef::Variation(k) => model.variations.get(*k).map(|v| v.id.as_str()),
        NodeRef::Condition(k) => model.conditions.get(*k).map(|c| c.id.as_str()),
    }
}

pub(crate) fn node_label<'a>(model: &'a SemanticModel, node: &NodeRef) -> Option<&'a str> {
    match node {
        NodeRef::Actor(k) => model.actors.get(*k).map(|a| a.label.as_str()),
        NodeRef::ExtSystem(k) => model.ext_systems.get(*k).map(|e| e.label.as_str()),
        NodeRef::System(k) => model.systems.get(*k).map(|s| s.label.as_str()),
        NodeRef::Requirement(k) => model.requirements.get(*k).map(|r| r.label.as_str()),
        NodeRef::Adr(k) => model.adrs.get(*k).map(|a| a.label.as_str()),
        NodeRef::Nfr(k) => model.nfrs.get(*k).map(|n| n.label.as_str()),
        NodeRef::Quality(k) => model.qualities.get(*k).map(|q| q.label.as_str()),
        NodeRef::Constraint(k) => model.constraints.get(*k).map(|c| c.label.as_str()),
        NodeRef::Business(k) => model.businesses.get(*k).map(|b| b.label.as_str()),
        NodeRef::Buc(k) => model.bucs.get(*k).map(|b| b.label.as_str()),
        NodeRef::Flow(k) => model.flows.get(*k).map(|f| f.label.as_str()),
        NodeRef::Step(k) => model.steps.get(*k).map(|s| s.label.as_str()),
        NodeRef::UseCase(k) => model.use_cases.get(*k).map(|u| u.label.as_str()),
        NodeRef::Screen(k) => model.screens.get(*k).map(|s| s.label.as_str()),
        NodeRef::Field(k) => model.fields.get(*k).map(|f| f.label.as_str()),
        NodeRef::Event(k) => model.events.get(*k).map(|e| e.label.as_str()),
        NodeRef::Entity(k) => model.entities.get(*k).map(|e| e.label.as_str()),
        NodeRef::State(k) => model.states.get(*k).map(|s| s.label.as_str()),
        NodeRef::Api(k) => model.apis.get(*k).map(|a| a.label.as_str()),
        NodeRef::Dto(k) => model.dtos.get(*k).map(|d| d.label.as_str()),
        NodeRef::Timer(k) => model.timers.get(*k).map(|t| t.label.as_str()),
        NodeRef::Medium(k) => model.media.get(*k).map(|m| m.label.as_str()),
        NodeRef::Permission(k) => model.permissions.get(*k).map(|p| p.label.as_str()),
        NodeRef::Variation(k) => model.variations.get(*k).map(|v| v.label.as_str()),
        NodeRef::Condition(k) => model.conditions.get(*k).map(|c| c.label.as_str()),
    }
}

/// Type name shown in ER diagrams and CSV: scalar names, `Enum`, or the variation's id.
pub(crate) fn col_type_str(model: &SemanticModel, ct: &ColumnType) -> String {
    match ct {
        ColumnType::Int => "Int".to_string(),
        ColumnType::String => "String".to_string(),
        ColumnType::Money => "Money".to_string(),
        ColumnType::DateTime => "DateTime".to_string(),
        ColumnType::Date => "Date".to_string(),
        ColumnType::Bool => "Bool".to_string(),
        ColumnType::Decimal => "Decimal".to_string(),
        ColumnType::Enum(_) => "Enum".to_string(),
        ColumnType::Variation(key) => model
            .variations
            .get(*key)
            .map(|v| v.id.clone())
            .unwrap_or_else(|| "Variation".to_string()),
    }
}
