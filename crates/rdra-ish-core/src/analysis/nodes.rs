//! NodeRef kind string labels used during predicate validation.

use crate::model::NodeRef;

pub(crate) fn node_kind_tag_str(node: &NodeRef) -> &'static str {
    match node {
        NodeRef::Actor(_) => "actor",
        NodeRef::ExtSystem(_) => "extsystem",
        NodeRef::System(_) => "system",
        NodeRef::Requirement(_) => "requirement",
        NodeRef::Adr(_) => "adr",
        NodeRef::Nfr(_) => "nfr",
        NodeRef::Quality(_) => "quality",
        NodeRef::Constraint(_) => "constraint",
        NodeRef::Concept(_) => "concept",
        NodeRef::DomainObject(_) => "domain_object",
        NodeRef::Aggregate(_) => "aggregate",
        NodeRef::ValueObject(_) => "valueobject",
        NodeRef::Business(_) => "business",
        NodeRef::Buc(_) => "buc",
        NodeRef::Flow(_) => "flow",
        NodeRef::Step(_) => "step",
        NodeRef::UsageScene(_) => "usagescene",
        NodeRef::UseCase(_) => "usecase",
        NodeRef::Screen(_) => "screen",
        NodeRef::Field(_) => "field",
        NodeRef::Event(_) => "event",
        NodeRef::Entity(_) => "entity",
        NodeRef::State(_) => "state",
        NodeRef::Condition(_) => "condition",
        NodeRef::Variation(_) => "variation",
        NodeRef::Api(_) => "api",
        NodeRef::Dto(_) => "dto",
        NodeRef::Location(_) => "location",
        NodeRef::Timing(_) => "timing",
        NodeRef::Medium(_) => "medium",
        NodeRef::Permission(_) => "permission",
    }
}
