//! rdra-emit: RDRA output emitters (PlantUML, CSV, Mermaid).

pub mod asyncapi;
pub mod csv;
pub mod dbml;
pub mod diff;
pub mod json_schema;
pub mod mermaid;
pub mod openapi;
pub mod plantuml;
pub mod state_pattern;

use rdra_ish_core::model::{NodeRef, RelKind, SemanticModel};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmitError {
    #[error("CSV write error: {0}")]
    Csv(#[from] ::csv::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// 出力のスコープ
#[derive(Debug, Clone)]
pub enum Scope {
    Whole,
    /// 特定BUC群（buc_id 文字列のリスト、和集合で絞り込む）
    Bucs(Vec<String>),
    /// 特定UseCase群（usecase_id 文字列のリスト、和集合で絞り込む）
    UseCases(Vec<String>),
}

/// 出力フィルタ
#[derive(Debug, Clone)]
pub enum Filter {
    None,
    ActorOnly,
    EntityOnly,
    Er,
}

#[derive(Debug, Clone)]
pub struct View {
    pub scope: Scope,
    pub filter: Filter,
    pub show_descriptions: bool,
    pub node_kinds: Vec<String>,
    pub edge_kinds: Vec<String>,
}

impl View {
    pub fn whole() -> Self {
        Self {
            scope: Scope::Whole,
            filter: Filter::None,
            show_descriptions: false,
            node_kinds: Vec::new(),
            edge_kinds: Vec::new(),
        }
    }

    pub fn er() -> Self {
        Self {
            scope: Scope::Whole,
            filter: Filter::Er,
            show_descriptions: false,
            node_kinds: Vec::new(),
            edge_kinds: Vec::new(),
        }
    }

    /// 1つ以上の BUC id を指定して絞り込むビューを作る。
    /// `buc_ids` が空の場合は `Scope::Whole` になる。
    pub fn bucs(buc_ids: Vec<String>) -> Self {
        let scope = if buc_ids.is_empty() {
            Scope::Whole
        } else {
            Scope::Bucs(buc_ids)
        };
        Self {
            scope,
            filter: Filter::None,
            show_descriptions: false,
            node_kinds: Vec::new(),
            edge_kinds: Vec::new(),
        }
    }

    /// 1つ以上の UseCase id を指定して絞り込むビューを作る。
    pub fn usecases(usecase_ids: Vec<String>) -> Self {
        let scope = if usecase_ids.is_empty() {
            Scope::Whole
        } else {
            Scope::UseCases(usecase_ids)
        };
        Self {
            scope,
            filter: Filter::None,
            show_descriptions: false,
            node_kinds: Vec::new(),
            edge_kinds: Vec::new(),
        }
    }

    pub fn with_descriptions(mut self, show_descriptions: bool) -> Self {
        self.show_descriptions = show_descriptions;
        self
    }

    pub fn with_graph_filters(mut self, node_kinds: Vec<String>, edge_kinds: Vec<String>) -> Self {
        self.node_kinds = node_kinds
            .into_iter()
            .map(|kind| normalize_filter_token(&kind))
            .collect();
        self.edge_kinds = edge_kinds
            .into_iter()
            .map(|kind| normalize_filter_token(&kind))
            .collect();
        self
    }
}

pub trait Emitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ObjectGraphLayer {
    SystemValue,
    ExternalEnvironment,
    SystemBoundary,
    System,
}

impl ObjectGraphLayer {
    pub(crate) fn mermaid_id(self) -> &'static str {
        match self {
            ObjectGraphLayer::SystemValue => "layer_value",
            ObjectGraphLayer::ExternalEnvironment => "layer_environment",
            ObjectGraphLayer::SystemBoundary => "layer_boundary",
            ObjectGraphLayer::System => "layer_system",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            ObjectGraphLayer::SystemValue => "System Value",
            ObjectGraphLayer::ExternalEnvironment => "External Environment",
            ObjectGraphLayer::SystemBoundary => "System Boundary",
            ObjectGraphLayer::System => "System",
        }
    }
}

pub(crate) const OBJECT_GRAPH_LAYERS: [ObjectGraphLayer; 4] = [
    ObjectGraphLayer::SystemValue,
    ObjectGraphLayer::ExternalEnvironment,
    ObjectGraphLayer::SystemBoundary,
    ObjectGraphLayer::System,
];

pub(crate) fn object_graph_layer(node: &NodeRef) -> ObjectGraphLayer {
    match node {
        NodeRef::Actor(_) | NodeRef::Requirement(_) | NodeRef::Adr(_) | NodeRef::Nfr(_) => {
            ObjectGraphLayer::SystemValue
        }
        NodeRef::ExtSystem(_)
        | NodeRef::Quality(_)
        | NodeRef::Constraint(_)
        | NodeRef::Concept(_)
        | NodeRef::DomainObject(_)
        | NodeRef::Aggregate(_)
        | NodeRef::ValueObject(_)
        | NodeRef::Business(_)
        | NodeRef::Buc(_)
        | NodeRef::Flow(_)
        | NodeRef::Step(_)
        | NodeRef::UsageScene(_)
        | NodeRef::Condition(_)
        | NodeRef::Variation(_)
        | NodeRef::Location(_)
        | NodeRef::Timing(_)
        | NodeRef::Medium(_)
        | NodeRef::Permission(_) => ObjectGraphLayer::ExternalEnvironment,
        NodeRef::UseCase(_) | NodeRef::Screen(_) | NodeRef::Field(_) | NodeRef::Event(_) => {
            ObjectGraphLayer::SystemBoundary
        }
        NodeRef::System(_)
        | NodeRef::Api(_)
        | NodeRef::Dto(_)
        | NodeRef::Entity(_)
        | NodeRef::State(_) => ObjectGraphLayer::System,
    }
}

pub(crate) fn object_graph_rel_label(kind: &RelKind) -> &'static str {
    match kind {
        RelKind::Performs => "performs",
        RelKind::Uses => "uses",
        RelKind::Reads => "reads",
        RelKind::Writes => "writes",
        RelKind::Creates => "creates",
        RelKind::Updates => "updates",
        RelKind::Deletes => "deletes",
        RelKind::Displays => "displays",
        RelKind::Shows => "shows",
        RelKind::Raises => "raises",
        RelKind::Triggers => "triggers",
        RelKind::Contains => "contains",
        RelKind::Belongs => "belongs",
        RelKind::HasPermission => "has_permission",
        RelKind::RequiresPermission => "requires_permission",
        RelKind::RequiresMedium => "requires_medium",
        RelKind::Motivates => "motivates",
        RelKind::Decides => "decides",
        RelKind::Transitions => "transitions",
        RelKind::Invokes => "invokes",
        RelKind::Precedes => "precedes",
        RelKind::Branches => "branches",
        RelKind::Excepts => "excepts",
        RelKind::Repeats => "repeats",
        RelKind::Covers => "covers",
        RelKind::Compensates => "compensates",
        RelKind::Request => "request",
        RelKind::Response => "response",
        RelKind::ErrorResponse => "error_response",
        RelKind::AppliesTo => "applies_to",
        RelKind::Qualifies => "qualifies",
        RelKind::Constrains => "constrains",
        RelKind::MapsTo => "maps_to",
        RelKind::MapsField => "maps_field",
        RelKind::Owns => "owns",
        RelKind::RelateOneToOne => "1:1",
        RelKind::RelateOneToMany => "1:N",
        RelKind::RelateManyToOne => "N:1",
        RelKind::RelateManyToMany => "N:M",
    }
}

pub(crate) fn normalize_filter_token(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('_', "-")
}

pub(crate) fn node_kind_filter_name(node: &NodeRef) -> &'static str {
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
        NodeRef::DomainObject(_) => "domain-object",
        NodeRef::Aggregate(_) => "aggregate",
        NodeRef::ValueObject(_) => "value-object",
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

pub(crate) fn view_node_visible(view: &View, node: &NodeRef) -> bool {
    if view.node_kinds.is_empty() {
        return true;
    }
    let kind = node_kind_filter_name(node);
    let normalized = normalize_filter_token(kind);
    let underscore = kind.replace('-', "_");
    view.node_kinds
        .iter()
        .any(|filter| filter == kind || filter == &normalized || filter == &underscore)
}

pub(crate) fn view_relation_visible(view: &View, kind: &RelKind) -> bool {
    if view.edge_kinds.is_empty() {
        return true;
    }
    let label = object_graph_rel_label(kind);
    let normalized = normalize_filter_token(label);
    view.edge_kinds.iter().any(|filter| {
        filter == label
            || filter == &normalized
            || (filter == "relate"
                && matches!(
                    kind,
                    RelKind::RelateOneToOne
                        | RelKind::RelateOneToMany
                        | RelKind::RelateManyToOne
                        | RelKind::RelateManyToMany
                ))
    })
}

pub(crate) fn node_description<'a>(model: &'a SemanticModel, node: &NodeRef) -> Option<&'a str> {
    match node {
        NodeRef::Actor(key) => model.actors.get(*key)?.description.as_deref(),
        NodeRef::ExtSystem(key) => model.ext_systems.get(*key)?.description.as_deref(),
        NodeRef::System(key) => model.systems.get(*key)?.description.as_deref(),
        NodeRef::Requirement(key) => model.requirements.get(*key)?.description.as_deref(),
        NodeRef::Adr(key) => model.adrs.get(*key)?.description.as_deref(),
        NodeRef::Nfr(key) => model.nfrs.get(*key)?.description.as_deref(),
        NodeRef::Quality(key) => model.qualities.get(*key)?.description.as_deref(),
        NodeRef::Constraint(key) => model.constraints.get(*key)?.description.as_deref(),
        NodeRef::Concept(key) => model.concepts.get(*key)?.description.as_deref(),
        NodeRef::DomainObject(key) => model.domain_objects.get(*key)?.description.as_deref(),
        NodeRef::Aggregate(key) => model.aggregates.get(*key)?.description.as_deref(),
        NodeRef::ValueObject(key) => model.value_objects.get(*key)?.description.as_deref(),
        NodeRef::Business(key) => model.businesses.get(*key)?.description.as_deref(),
        NodeRef::Buc(key) => model.bucs.get(*key)?.description.as_deref(),
        NodeRef::Flow(key) => model.flows.get(*key)?.description.as_deref(),
        NodeRef::Step(key) => model.steps.get(*key)?.description.as_deref(),
        NodeRef::UsageScene(key) => model.usage_scenes.get(*key)?.description.as_deref(),
        NodeRef::Condition(key) => model.conditions.get(*key)?.description.as_deref(),
        NodeRef::Variation(key) => model.variations.get(*key)?.description.as_deref(),
        NodeRef::Location(key) => model.locations.get(*key)?.description.as_deref(),
        NodeRef::Timing(key) => model.timings.get(*key)?.description.as_deref(),
        NodeRef::Medium(key) => model.media.get(*key)?.description.as_deref(),
        NodeRef::Permission(key) => model.permissions.get(*key)?.description.as_deref(),
        NodeRef::UseCase(key) => model.use_cases.get(*key)?.description.as_deref(),
        NodeRef::Screen(key) => model.screens.get(*key)?.description.as_deref(),
        NodeRef::Field(key) => model.fields.get(*key)?.description.as_deref(),
        NodeRef::Event(key) => model.events.get(*key)?.description.as_deref(),
        NodeRef::Api(key) => model.apis.get(*key)?.description.as_deref(),
        NodeRef::Dto(key) => model.dtos.get(*key)?.description.as_deref(),
        NodeRef::Entity(key) => model.entities.get(*key)?.description.as_deref(),
        NodeRef::State(key) => model.states.get(*key)?.description.as_deref(),
    }
}

pub(crate) fn node_emoji(node: &NodeRef) -> &'static str {
    match node {
        NodeRef::Actor(_) => "👤",
        NodeRef::ExtSystem(_) => "🌐",
        NodeRef::System(_) => "🧩",
        NodeRef::Requirement(_) => "🎯",
        NodeRef::Adr(_) => "📝",
        NodeRef::Nfr(_) => "📈",
        NodeRef::Quality(_) => "✨",
        NodeRef::Constraint(_) => "⚖️",
        NodeRef::Concept(_) => "💡",
        NodeRef::DomainObject(_) => "🧠",
        NodeRef::Aggregate(_) => "🧱",
        NodeRef::ValueObject(_) => "🔹",
        NodeRef::Business(_) => "💼",
        NodeRef::Buc(_) => "📦",
        NodeRef::Flow(_) => "🧭",
        NodeRef::Step(_) => "➡️",
        NodeRef::UsageScene(_) => "🎬",
        NodeRef::UseCase(_) => "✅",
        NodeRef::Screen(_) => "🖥️",
        NodeRef::Field(_) => "🔤",
        NodeRef::Event(_) => "⚡",
        NodeRef::Entity(_) => "🗄️",
        NodeRef::State(_) => "🔄",
        NodeRef::Condition(_) => "❓",
        NodeRef::Variation(_) => "🔀",
        NodeRef::Api(_) => "🔌",
        NodeRef::Dto(_) => "📄",
        NodeRef::Location(_) => "📍",
        NodeRef::Timing(_) => "⏱️",
        NodeRef::Medium(_) => "📱",
        NodeRef::Permission(_) => "🔑",
    }
}

pub(crate) fn prefixed_node_label(node: &NodeRef, label: &str) -> String {
    format!("{} {}", node_emoji(node), label)
}

pub(crate) fn prefixed_label(emoji: &str, label: &str) -> String {
    format!("{} {}", emoji, label)
}

pub(crate) fn collect_object_graph_nodes(
    model: &SemanticModel,
    is_visible: &impl Fn(&NodeRef) -> bool,
) -> Vec<NodeRef> {
    let mut nodes = Vec::new();

    let mut actors: Vec<_> = model.actors.iter().collect();
    actors.sort_by_key(|(_, a)| &a.id);
    nodes.extend(
        actors
            .into_iter()
            .map(|(k, _)| NodeRef::Actor(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut requirements: Vec<_> = model.requirements.iter().collect();
    requirements.sort_by_key(|(_, r)| &r.id);
    nodes.extend(
        requirements
            .into_iter()
            .map(|(k, _)| NodeRef::Requirement(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut nfrs: Vec<_> = model.nfrs.iter().collect();
    nfrs.sort_by_key(|(_, n)| &n.id);
    nodes.extend(
        nfrs.into_iter()
            .map(|(k, _)| NodeRef::Nfr(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut exts: Vec<_> = model.ext_systems.iter().collect();
    exts.sort_by_key(|(_, e)| &e.id);
    nodes.extend(
        exts.into_iter()
            .map(|(k, _)| NodeRef::ExtSystem(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut qualities: Vec<_> = model.qualities.iter().collect();
    qualities.sort_by_key(|(_, q)| &q.id);
    nodes.extend(
        qualities
            .into_iter()
            .map(|(k, _)| NodeRef::Quality(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut constraints: Vec<_> = model.constraints.iter().collect();
    constraints.sort_by_key(|(_, c)| &c.id);
    nodes.extend(
        constraints
            .into_iter()
            .map(|(k, _)| NodeRef::Constraint(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut concepts: Vec<_> = model.concepts.iter().collect();
    concepts.sort_by_key(|(_, c)| &c.id);
    nodes.extend(
        concepts
            .into_iter()
            .map(|(k, _)| NodeRef::Concept(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut domain_objects: Vec<_> = model.domain_objects.iter().collect();
    domain_objects.sort_by_key(|(_, d)| &d.id);
    nodes.extend(
        domain_objects
            .into_iter()
            .map(|(k, _)| NodeRef::DomainObject(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut aggregates: Vec<_> = model.aggregates.iter().collect();
    aggregates.sort_by_key(|(_, a)| &a.id);
    nodes.extend(
        aggregates
            .into_iter()
            .map(|(k, _)| NodeRef::Aggregate(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut value_objects: Vec<_> = model.value_objects.iter().collect();
    value_objects.sort_by_key(|(_, v)| &v.id);
    nodes.extend(
        value_objects
            .into_iter()
            .map(|(k, _)| NodeRef::ValueObject(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut businesses: Vec<_> = model.businesses.iter().collect();
    businesses.sort_by_key(|(_, b)| &b.id);
    nodes.extend(
        businesses
            .into_iter()
            .map(|(k, _)| NodeRef::Business(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut bucs: Vec<_> = model.bucs.iter().collect();
    bucs.sort_by_key(|(_, b)| &b.id);
    nodes.extend(
        bucs.into_iter()
            .map(|(k, _)| NodeRef::Buc(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut flows: Vec<_> = model.flows.iter().collect();
    flows.sort_by_key(|(_, f)| &f.id);
    nodes.extend(
        flows
            .into_iter()
            .map(|(k, _)| NodeRef::Flow(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut steps: Vec<_> = model.steps.iter().collect();
    steps.sort_by_key(|(_, s)| &s.id);
    nodes.extend(
        steps
            .into_iter()
            .map(|(k, _)| NodeRef::Step(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut scenes: Vec<_> = model.usage_scenes.iter().collect();
    scenes.sort_by_key(|(_, u)| &u.id);
    nodes.extend(
        scenes
            .into_iter()
            .map(|(k, _)| NodeRef::UsageScene(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut conditions: Vec<_> = model.conditions.iter().collect();
    conditions.sort_by_key(|(_, c)| &c.id);
    nodes.extend(
        conditions
            .into_iter()
            .map(|(k, _)| NodeRef::Condition(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut variations: Vec<_> = model.variations.iter().collect();
    variations.sort_by_key(|(_, v)| &v.id);
    nodes.extend(
        variations
            .into_iter()
            .map(|(k, _)| NodeRef::Variation(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut locations: Vec<_> = model.locations.iter().collect();
    locations.sort_by_key(|(_, l)| &l.id);
    nodes.extend(
        locations
            .into_iter()
            .map(|(k, _)| NodeRef::Location(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut timings: Vec<_> = model.timings.iter().collect();
    timings.sort_by_key(|(_, t)| &t.id);
    nodes.extend(
        timings
            .into_iter()
            .map(|(k, _)| NodeRef::Timing(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut media: Vec<_> = model.media.iter().collect();
    media.sort_by_key(|(_, m)| &m.id);
    nodes.extend(
        media
            .into_iter()
            .map(|(k, _)| NodeRef::Medium(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut permissions: Vec<_> = model.permissions.iter().collect();
    permissions.sort_by_key(|(_, p)| &p.id);
    nodes.extend(
        permissions
            .into_iter()
            .map(|(k, _)| NodeRef::Permission(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut usecases: Vec<_> = model.use_cases.iter().collect();
    usecases.sort_by_key(|(_, u)| &u.id);
    nodes.extend(
        usecases
            .into_iter()
            .map(|(k, _)| NodeRef::UseCase(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut screens: Vec<_> = model.screens.iter().collect();
    screens.sort_by_key(|(_, s)| &s.id);
    nodes.extend(
        screens
            .into_iter()
            .map(|(k, _)| NodeRef::Screen(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut fields: Vec<_> = model.fields.iter().collect();
    fields.sort_by_key(|(_, f)| &f.id);
    nodes.extend(
        fields
            .into_iter()
            .map(|(k, _)| NodeRef::Field(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut events: Vec<_> = model.events.iter().collect();
    events.sort_by_key(|(_, e)| &e.id);
    nodes.extend(
        events
            .into_iter()
            .map(|(k, _)| NodeRef::Event(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut apis: Vec<_> = model.apis.iter().collect();
    apis.sort_by_key(|(_, a)| &a.id);
    nodes.extend(
        apis.into_iter()
            .map(|(k, _)| NodeRef::Api(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut dtos: Vec<_> = model.dtos.iter().collect();
    dtos.sort_by_key(|(_, d)| &d.id);
    nodes.extend(
        dtos.into_iter()
            .map(|(k, _)| NodeRef::Dto(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut systems: Vec<_> = model.systems.iter().collect();
    systems.sort_by_key(|(_, s)| &s.id);
    nodes.extend(
        systems
            .into_iter()
            .map(|(k, _)| NodeRef::System(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut entities: Vec<_> = model.entities.iter().collect();
    entities.sort_by_key(|(_, e)| &e.id);
    nodes.extend(
        entities
            .into_iter()
            .map(|(k, _)| NodeRef::Entity(k))
            .filter(|nr| is_visible(nr)),
    );

    let mut states: Vec<_> = model.states.iter().collect();
    states.sort_by_key(|(_, s)| &s.id);
    nodes.extend(
        states
            .into_iter()
            .map(|(k, _)| NodeRef::State(k))
            .filter(|nr| is_visible(nr)),
    );

    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, errors) = parse(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let (model, diags) = build_model(&ast);
        let errs: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errs.is_empty(), "model errors: {:?}", errs);
        model
    }

    fn node_id(model: &SemanticModel, node: &NodeRef) -> String {
        match node {
            NodeRef::Actor(k) => model.actors[*k].id.clone(),
            NodeRef::Requirement(k) => model.requirements[*k].id.clone(),
            NodeRef::Adr(k) => model.adrs[*k].id.clone(),
            NodeRef::Nfr(k) => model.nfrs[*k].id.clone(),
            NodeRef::Quality(k) => model.qualities[*k].id.clone(),
            NodeRef::Constraint(k) => model.constraints[*k].id.clone(),
            NodeRef::Concept(k) => model.concepts[*k].id.clone(),
            NodeRef::DomainObject(k) => model.domain_objects[*k].id.clone(),
            NodeRef::Aggregate(k) => model.aggregates[*k].id.clone(),
            NodeRef::ValueObject(k) => model.value_objects[*k].id.clone(),
            NodeRef::ExtSystem(k) => model.ext_systems[*k].id.clone(),
            NodeRef::Business(k) => model.businesses[*k].id.clone(),
            NodeRef::Buc(k) => model.bucs[*k].id.clone(),
            NodeRef::Flow(k) => model.flows[*k].id.clone(),
            NodeRef::Step(k) => model.steps[*k].id.clone(),
            NodeRef::UsageScene(k) => model.usage_scenes[*k].id.clone(),
            NodeRef::Condition(k) => model.conditions[*k].id.clone(),
            NodeRef::Variation(k) => model.variations[*k].id.clone(),
            NodeRef::Location(k) => model.locations[*k].id.clone(),
            NodeRef::Timing(k) => model.timings[*k].id.clone(),
            NodeRef::Medium(k) => model.media[*k].id.clone(),
            NodeRef::Permission(k) => model.permissions[*k].id.clone(),
            NodeRef::UseCase(k) => model.use_cases[*k].id.clone(),
            NodeRef::Screen(k) => model.screens[*k].id.clone(),
            NodeRef::Field(k) => model.fields[*k].id.clone(),
            NodeRef::Event(k) => model.events[*k].id.clone(),
            NodeRef::Api(k) => model.apis[*k].id.clone(),
            NodeRef::Dto(k) => model.dtos[*k].id.clone(),
            NodeRef::System(k) => model.systems[*k].id.clone(),
            NodeRef::Entity(k) => model.entities[*k].id.clone(),
            NodeRef::State(k) => model.states[*k].id.clone(),
        }
    }

    #[test]
    fn collect_object_graph_nodes_uses_layer_order_and_id_sorting() {
        let model = model_from(
            r#"
actor Beta "Beta"
actor Alpha "Alpha"
requirement Req "Requirement"
nfr Latency "Latency"
extsystem Ext "External"
quality Performance "Performance"
constraint Retention "Retention"
concept PatientIdentity "Patient identity"
domain_object Appointment "Appointment"
aggregate SchedulingAggregate "Scheduling aggregate"
valueobject TimeSlot "Time slot"
business Biz "Business"
buc Buc "BUC"
flow Flow "Flow"
step Step "Step"
usagescene Scene "Scene"
condition Cond "Condition"
variation Var "Variation"
location Loc "Location"
timing Time "Timing"
medium Med "Medium"
permission Perm "Permission"
usecase Uc "UseCase"
screen Screen "Screen"
field ScreenTotal "Screen total" access readonly required true source system
event Event "Event"
api Api "API"
dto ApiRequest "API Request" { name: String }
system Sys "System"
entity Entity "Entity" { id: Int @pk }
state State "State"
"#,
        );

        let ids: Vec<_> = collect_object_graph_nodes(&model, &|_| true)
            .iter()
            .map(|node| node_id(&model, node))
            .collect();

        assert_eq!(
            ids,
            vec![
                "Alpha",
                "Beta",
                "Req",
                "Latency",
                "Ext",
                "Performance",
                "Retention",
                "PatientIdentity",
                "Appointment",
                "SchedulingAggregate",
                "TimeSlot",
                "Biz",
                "Buc",
                "Flow",
                "Step",
                "Scene",
                "Cond",
                "Var",
                "Loc",
                "Time",
                "Med",
                "Perm",
                "Uc",
                "Screen",
                "ScreenTotal",
                "Event",
                "Api",
                "ApiRequest",
                "Sys",
                "Entity",
                "State",
            ]
        );
    }

    #[test]
    fn collect_object_graph_nodes_applies_visibility_filter_after_collection() {
        let model = model_from(
            r#"
actor Customer "Customer"
permission Admin "Admin"
usecase Manage "Manage"
entity Account "Account" { id: Int @pk }
state Active "Active"
"#,
        );

        let nodes = collect_object_graph_nodes(&model, &|node| {
            !matches!(node, NodeRef::Permission(_) | NodeRef::Entity(_))
        });
        let ids: Vec<_> = nodes.iter().map(|node| node_id(&model, node)).collect();

        assert_eq!(ids, vec!["Customer", "Manage", "Active"]);
        assert!(nodes
            .iter()
            .all(|node| !matches!(node, NodeRef::Permission(_) | NodeRef::Entity(_))));
    }
}
