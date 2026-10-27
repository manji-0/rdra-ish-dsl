//! NodeRef display helpers for list output.

use rdra_ish_core::model::{NodeRef, RelKind};

pub(crate) fn node_kind_name(node: &NodeRef) -> &'static str {
    match node {
        NodeRef::Actor(_) => "actor",
        NodeRef::ExtSystem(_) => "extsystem",
        NodeRef::System(_) => "system",
        NodeRef::Requirement(_) => "requirement",
        NodeRef::Adr(_) => "adr",
        NodeRef::Nfr(_) => "nfr",
        NodeRef::Quality(_) => "quality",
        NodeRef::Constraint(_) => "constraint",
        NodeRef::Business(_) => "business",
        NodeRef::Buc(_) => "buc",
        NodeRef::Flow(_) => "flow",
        NodeRef::Step(_) => "step",
        NodeRef::UseCase(_) => "usecase",
        NodeRef::Screen(_) => "screen",
        NodeRef::Field(_) => "field",
        NodeRef::Event(_) => "event",
        NodeRef::Entity(_) => "entity",
        NodeRef::State(_) => "state",
        NodeRef::Api(_) => "api",
        NodeRef::Dto(_) => "dto",
        NodeRef::Timer(_) => "timer",
        NodeRef::Medium(_) => "medium",
        NodeRef::Permission(_) => "permission",
        NodeRef::Variation(_) => "variation",
    }
}

pub(crate) fn node_id(model: &rdra_ish_core::SemanticModel, node: &NodeRef) -> Option<String> {
    Some(match node {
        NodeRef::Actor(key) => model.actors.get(*key)?.id.clone(),
        NodeRef::ExtSystem(key) => model.ext_systems.get(*key)?.id.clone(),
        NodeRef::System(key) => model.systems.get(*key)?.id.clone(),
        NodeRef::Requirement(key) => model.requirements.get(*key)?.id.clone(),
        NodeRef::Adr(key) => model.adrs.get(*key)?.id.clone(),
        NodeRef::Nfr(key) => model.nfrs.get(*key)?.id.clone(),
        NodeRef::Quality(key) => model.qualities.get(*key)?.id.clone(),
        NodeRef::Constraint(key) => model.constraints.get(*key)?.id.clone(),
        NodeRef::Business(key) => model.businesses.get(*key)?.id.clone(),
        NodeRef::Buc(key) => model.bucs.get(*key)?.id.clone(),
        NodeRef::Flow(key) => model.flows.get(*key)?.id.clone(),
        NodeRef::Step(key) => model.steps.get(*key)?.id.clone(),
        NodeRef::UseCase(key) => model.use_cases.get(*key)?.id.clone(),
        NodeRef::Screen(key) => model.screens.get(*key)?.id.clone(),
        NodeRef::Field(key) => model.fields.get(*key)?.id.clone(),
        NodeRef::Event(key) => model.events.get(*key)?.id.clone(),
        NodeRef::Entity(key) => model.entities.get(*key)?.id.clone(),
        NodeRef::State(key) => model.states.get(*key)?.id.clone(),
        NodeRef::Api(key) => model.apis.get(*key)?.id.clone(),
        NodeRef::Dto(key) => model.dtos.get(*key)?.id.clone(),
        NodeRef::Timer(key) => model.timers.get(*key)?.id.clone(),
        NodeRef::Medium(key) => model.media.get(*key)?.id.clone(),
        NodeRef::Permission(key) => model.permissions.get(*key)?.id.clone(),
        NodeRef::Variation(key) => model.variations.get(*key)?.id.clone(),
    })
}
pub(crate) fn adr_targets(
    model: &rdra_ish_core::SemanticModel,
    adr: rdra_ish_core::model::AdrKey,
) -> Vec<NodeRef> {
    let mut targets: Vec<_> = model
        .relations
        .iter()
        .filter(|relation| relation.kind == RelKind::Decides && relation.from == NodeRef::Adr(adr))
        .map(|relation| relation.to.clone())
        .collect();
    targets.sort_by_key(|target| {
        (
            node_kind_name(target).to_string(),
            node_id(model, target).unwrap_or_default(),
        )
    });
    targets
}

pub(crate) fn node_label(model: &rdra_ish_core::SemanticModel, node: &NodeRef) -> Option<String> {
    Some(match node {
        NodeRef::Actor(key) => model.actors.get(*key)?.label.clone(),
        NodeRef::ExtSystem(key) => model.ext_systems.get(*key)?.label.clone(),
        NodeRef::System(key) => model.systems.get(*key)?.label.clone(),
        NodeRef::Requirement(key) => model.requirements.get(*key)?.label.clone(),
        NodeRef::Adr(key) => model.adrs.get(*key)?.label.clone(),
        NodeRef::Nfr(key) => model.nfrs.get(*key)?.label.clone(),
        NodeRef::Quality(key) => model.qualities.get(*key)?.label.clone(),
        NodeRef::Constraint(key) => model.constraints.get(*key)?.label.clone(),
        NodeRef::Business(key) => model.businesses.get(*key)?.label.clone(),
        NodeRef::Buc(key) => model.bucs.get(*key)?.label.clone(),
        NodeRef::Flow(key) => model.flows.get(*key)?.label.clone(),
        NodeRef::Step(key) => model.steps.get(*key)?.label.clone(),
        NodeRef::UseCase(key) => model.use_cases.get(*key)?.label.clone(),
        NodeRef::Screen(key) => model.screens.get(*key)?.label.clone(),
        NodeRef::Field(key) => model.fields.get(*key)?.label.clone(),
        NodeRef::Event(key) => model.events.get(*key)?.label.clone(),
        NodeRef::Entity(key) => model.entities.get(*key)?.label.clone(),
        NodeRef::State(key) => model.states.get(*key)?.label.clone(),
        NodeRef::Api(key) => model.apis.get(*key)?.label.clone(),
        NodeRef::Dto(key) => model.dtos.get(*key)?.label.clone(),
        NodeRef::Timer(key) => model.timers.get(*key)?.label.clone(),
        NodeRef::Medium(key) => model.media.get(*key)?.label.clone(),
        NodeRef::Permission(key) => model.permissions.get(*key)?.label.clone(),
        NodeRef::Variation(key) => model.variations.get(*key)?.label.clone(),
    })
}
