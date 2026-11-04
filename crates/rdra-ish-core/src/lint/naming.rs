//! Node enumeration and identifier naming checks.

use crate::model::{NodeRef, SemanticModel};

pub(super) fn lint_node_refs(model: &SemanticModel) -> Vec<NodeRef> {
    let mut nodes = Vec::new();
    nodes.extend(model.actors.iter().map(|(key, _)| NodeRef::Actor(key)));
    nodes.extend(
        model
            .ext_systems
            .iter()
            .map(|(key, _)| NodeRef::ExtSystem(key)),
    );
    nodes.extend(model.systems.iter().map(|(key, _)| NodeRef::System(key)));
    nodes.extend(
        model
            .requirements
            .iter()
            .map(|(key, _)| NodeRef::Requirement(key)),
    );
    nodes.extend(model.adrs.iter().map(|(key, _)| NodeRef::Adr(key)));
    nodes.extend(model.goals.iter().map(|(key, _)| NodeRef::Goal(key)));
    nodes.extend(model.nfrs.iter().map(|(key, _)| NodeRef::Nfr(key)));
    nodes.extend(model.qualities.iter().map(|(key, _)| NodeRef::Quality(key)));
    nodes.extend(
        model
            .constraints
            .iter()
            .map(|(key, _)| NodeRef::Constraint(key)),
    );
    nodes.extend(
        model
            .businesses
            .iter()
            .map(|(key, _)| NodeRef::Business(key)),
    );
    nodes.extend(model.bucs.iter().map(|(key, _)| NodeRef::Buc(key)));
    nodes.extend(model.flows.iter().map(|(key, _)| NodeRef::Flow(key)));
    nodes.extend(model.steps.iter().map(|(key, _)| NodeRef::Step(key)));
    nodes.extend(model.use_cases.iter().map(|(key, _)| NodeRef::UseCase(key)));
    nodes.extend(model.screens.iter().map(|(key, _)| NodeRef::Screen(key)));
    nodes.extend(model.fields.iter().map(|(key, _)| NodeRef::Field(key)));
    nodes.extend(model.events.iter().map(|(key, _)| NodeRef::Event(key)));
    nodes.extend(model.entities.iter().map(|(key, _)| NodeRef::Entity(key)));
    nodes.extend(model.states.iter().map(|(key, _)| NodeRef::State(key)));
    nodes.extend(model.apis.iter().map(|(key, _)| NodeRef::Api(key)));
    nodes.extend(model.dtos.iter().map(|(key, _)| NodeRef::Dto(key)));
    nodes.extend(model.timers.iter().map(|(key, _)| NodeRef::Timer(key)));
    nodes.extend(model.media.iter().map(|(key, _)| NodeRef::Medium(key)));
    nodes.extend(
        model
            .permissions
            .iter()
            .map(|(key, _)| NodeRef::Permission(key)),
    );
    nodes.extend(
        model
            .variations
            .iter()
            .map(|(key, _)| NodeRef::Variation(key)),
    );
    nodes.extend(
        model
            .conditions
            .iter()
            .map(|(key, _)| NodeRef::Condition(key)),
    );
    nodes
}

pub(super) fn is_upper_camelish_id(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase()
        && id.chars().all(|c| c.is_ascii_alphanumeric())
        && id.chars().any(|c| c.is_ascii_lowercase())
}

pub(super) fn is_snake_caseish(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first == '_')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
