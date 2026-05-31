//! rdra-emit: RDRA output emitters (PlantUML, CSV, Mermaid).

pub mod csv;
pub mod mermaid;
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
}

impl View {
    pub fn whole() -> Self {
        Self {
            scope: Scope::Whole,
            filter: Filter::None,
        }
    }

    pub fn er() -> Self {
        Self {
            scope: Scope::Whole,
            filter: Filter::Er,
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
        }
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
        NodeRef::Actor(_) | NodeRef::Requirement(_) => ObjectGraphLayer::SystemValue,
        NodeRef::ExtSystem(_)
        | NodeRef::Business(_)
        | NodeRef::Buc(_)
        | NodeRef::UsageScene(_)
        | NodeRef::Condition(_)
        | NodeRef::Variation(_)
        | NodeRef::Location(_)
        | NodeRef::Timing(_)
        | NodeRef::Medium(_)
        | NodeRef::Permission(_) => ObjectGraphLayer::ExternalEnvironment,
        NodeRef::UseCase(_) | NodeRef::Screen(_) | NodeRef::Event(_) => {
            ObjectGraphLayer::SystemBoundary
        }
        NodeRef::System(_) | NodeRef::Api(_) | NodeRef::Entity(_) | NodeRef::State(_) => {
            ObjectGraphLayer::System
        }
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
        RelKind::Transitions => "transitions",
        RelKind::Invokes => "invokes",
        RelKind::RelateOneToOne => "1:1",
        RelKind::RelateOneToMany => "1:N",
        RelKind::RelateManyToOne => "N:1",
        RelKind::RelateManyToMany => "N:M",
    }
}

pub(crate) fn node_emoji(node: &NodeRef) -> &'static str {
    match node {
        NodeRef::Actor(_) => "👤",
        NodeRef::ExtSystem(_) => "🌐",
        NodeRef::System(_) => "🧩",
        NodeRef::Requirement(_) => "🎯",
        NodeRef::Business(_) => "💼",
        NodeRef::Buc(_) => "📦",
        NodeRef::UsageScene(_) => "🎬",
        NodeRef::UseCase(_) => "✅",
        NodeRef::Screen(_) => "🖥️",
        NodeRef::Event(_) => "⚡",
        NodeRef::Entity(_) => "🗄️",
        NodeRef::State(_) => "🔄",
        NodeRef::Condition(_) => "❓",
        NodeRef::Variation(_) => "🔀",
        NodeRef::Api(_) => "🔌",
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

    let mut exts: Vec<_> = model.ext_systems.iter().collect();
    exts.sort_by_key(|(_, e)| &e.id);
    nodes.extend(
        exts.into_iter()
            .map(|(k, _)| NodeRef::ExtSystem(k))
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
