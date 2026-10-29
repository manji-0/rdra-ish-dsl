//! Helpers the PlantUML and Mermaid emitters share.

use crate::{
    node_description, object_graph_rel_label, prefixed_label, view_node_visible,
    view_relation_visible, Scope, View,
};
use rdra_ish_core::lifecycle::{Bounds, Scope as LifecycleScope, TransitionSystem};
use rdra_ish_core::model::{
    ApiKey, BucKey, ColumnType, EntityKey, EventKey, ExtSystemKey, NodeRef, Raiser, RelKind,
    SemanticModel, StateKey, TimerKey, UseCaseKey,
};
use rdra_ish_core::{ActorInputInference, EventFlow, TimerFlow};
use std::collections::{HashMap, HashSet};

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

pub(crate) fn graph_node_visible(
    reachable: &Option<HashSet<NodeRef>>,
    view: &View,
    node: &NodeRef,
) -> bool {
    scoped_node_visible(reachable, node) && view_node_visible(view, node)
}

pub(crate) fn rdra_description_nodes(
    model: &SemanticModel,
    reachable: &Option<HashSet<NodeRef>>,
) -> Vec<NodeRef> {
    let mut nodes = Vec::new();
    nodes.extend(
        model
            .actors
            .iter()
            .map(|(key, _)| NodeRef::Actor(key))
            .filter(|node| scoped_node_visible(reachable, node)),
    );
    nodes.extend(
        model
            .adrs
            .iter()
            .map(|(key, _)| NodeRef::Adr(key))
            .filter(|node| scoped_node_visible(reachable, node)),
    );
    nodes.extend(
        model
            .use_cases
            .iter()
            .map(|(key, _)| NodeRef::UseCase(key))
            .filter(|node| scoped_node_visible(reachable, node)),
    );
    nodes.extend(
        model
            .bucs
            .iter()
            .map(|(key, _)| NodeRef::Buc(key))
            .filter(|node| scoped_node_visible(reachable, node)),
    );
    nodes.extend(
        model
            .flows
            .iter()
            .map(|(key, _)| NodeRef::Flow(key))
            .filter(|node| scoped_node_visible(reachable, node)),
    );
    nodes.extend(
        model
            .steps
            .iter()
            .map(|(key, _)| NodeRef::Step(key))
            .filter(|node| scoped_node_visible(reachable, node)),
    );
    nodes.extend(
        model
            .systems
            .iter()
            .map(|(key, _)| NodeRef::System(key))
            .filter(|node| scoped_node_visible(reachable, node)),
    );
    nodes.extend(
        model
            .ext_systems
            .iter()
            .map(|(key, _)| NodeRef::ExtSystem(key))
            .filter(|node| scoped_node_visible(reachable, node)),
    );
    nodes.extend(
        model
            .entities
            .iter()
            .map(|(key, _)| NodeRef::Entity(key))
            .filter(|node| scoped_node_visible(reachable, node)),
    );
    nodes.extend(
        model
            .screens
            .iter()
            .map(|(key, _)| NodeRef::Screen(key))
            .filter(|node| scoped_node_visible(reachable, node)),
    );
    nodes.extend(
        model
            .fields
            .iter()
            .map(|(key, _)| NodeRef::Field(key))
            .filter(|node| scoped_node_visible(reachable, node)),
    );
    nodes.extend(
        model
            .events
            .iter()
            .map(|(key, _)| NodeRef::Event(key))
            .filter(|node| scoped_node_visible(reachable, node)),
    );
    nodes.extend(
        model
            .states
            .iter()
            .map(|(key, _)| NodeRef::State(key))
            .filter(|node| scoped_node_visible(reachable, node)),
    );
    nodes.sort_by_key(|node| node_id(model, node).unwrap_or_default().to_string());
    nodes
}

pub(crate) fn sequence_usecase_scope(
    model: &SemanticModel,
    scope: &Scope,
) -> Option<HashSet<UseCaseKey>> {
    match scope {
        Scope::Whole => None,
        Scope::UseCases(usecase_ids) => {
            let wanted: HashSet<&str> = usecase_ids.iter().map(String::as_str).collect();
            Some(
                model
                    .use_cases
                    .iter()
                    .filter_map(|(key, uc)| wanted.contains(uc.id.as_str()).then_some(key))
                    .collect(),
            )
        }
        Scope::Bucs(buc_ids) => {
            let wanted: HashSet<&str> = buc_ids.iter().map(String::as_str).collect();
            let buc_keys: HashSet<BucKey> = model
                .bucs
                .iter()
                .filter_map(|(key, buc)| wanted.contains(buc.id.as_str()).then_some(key))
                .collect();
            Some(
                model
                    .relations
                    .iter()
                    .filter_map(|rel| {
                        if rel.kind == RelKind::Contains {
                            if let (NodeRef::Buc(buc), NodeRef::UseCase(usecase)) =
                                (&rel.from, &rel.to)
                            {
                                return buc_keys.contains(buc).then_some(*usecase);
                            }
                        }
                        None
                    })
                    .collect(),
            )
        }
    }
}

pub(crate) fn business_input_visible(
    model: &SemanticModel,
    view: &View,
    entry: &ActorInputInference,
) -> bool {
    match &view.scope {
        Scope::Whole => true,
        Scope::UseCases(usecase_ids) => usecase_ids
            .iter()
            .any(|id| id == &model.use_cases[entry.usecase].id),
        Scope::Bucs(buc_ids) => entry
            .buc
            .is_some_and(|buc| buc_ids.iter().any(|id| id == &model.bucs[buc].id)),
    }
}

pub(crate) fn business_input_node_id(model: &SemanticModel, entry: &ActorInputInference) -> String {
    scoped_id(
        "input",
        &format!(
            "{}_{}_{}_{}",
            model.actors[entry.actor].id,
            model.use_cases[entry.usecase].id,
            model.entities[entry.entity].id,
            entry.column
        ),
    )
}

pub(crate) fn business_actor_node_id(model: &SemanticModel, entry: &ActorInputInference) -> String {
    scoped_id("actor", &model.actors[entry.actor].id)
}

pub(crate) fn business_usecase_node_id(
    model: &SemanticModel,
    entry: &ActorInputInference,
) -> String {
    scoped_id("usecase", &model.use_cases[entry.usecase].id)
}

pub(crate) fn technical_api_node_id(
    model: &SemanticModel,
    system: rdra_ish_core::SystemKey,
    api: ApiKey,
) -> String {
    scoped_id(
        "api",
        &format!("{}_{}", model.systems[system].id, model.apis[api].id),
    )
}

pub(crate) fn technical_entity_node_id(
    model: &SemanticModel,
    system: rdra_ish_core::SystemKey,
    entity: EntityKey,
) -> String {
    scoped_id(
        "entity",
        &format!("{}_{}", model.systems[system].id, model.entities[entity].id),
    )
}

pub(crate) fn scoped_id(prefix: &str, raw: &str) -> String {
    let safe: String = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    format!("{}__{}", prefix, safe)
}

pub(crate) fn reachable_for_scope(
    model: &SemanticModel,
    scope: &Scope,
) -> Option<HashSet<NodeRef>> {
    match scope {
        Scope::Bucs(buc_ids) => Some(rdra_ish_core::reachable_from_bucs(model, buc_ids)),
        Scope::Whole | Scope::UseCases(_) => None,
    }
}

pub(crate) fn scoped_node_visible(reachable: &Option<HashSet<NodeRef>>, node: &NodeRef) -> bool {
    match reachable {
        Some(set) => set.contains(node),
        None => true,
    }
}

pub(crate) fn is_entity_operation(kind: &RelKind) -> bool {
    matches!(
        kind,
        RelKind::Reads | RelKind::Writes | RelKind::Creates | RelKind::Updates | RelKind::Deletes
    )
}

/// The text format a shared emitter writes. The two differ in how they quote a label and draw
/// an edge, and in nothing else the shared code looks at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Syntax {
    PlantUml,
    Mermaid,
}

/// An edge, independent of syntax.
pub(crate) enum Link<'a> {
    /// Solid arrow, no label.
    Arrow,
    /// Solid arrow with a label.
    Labeled(&'a str),
    /// Dotted arrow with a label.
    Dotted(&'a str),
    /// Plain line.
    Undirected,
}

impl Syntax {
    /// `label` safe inside a double-quoted string.
    pub(crate) fn quote(self, label: &str) -> String {
        match self {
            Syntax::PlantUml => label.replace('"', "\\\""),
            Syntax::Mermaid => label.replace('"', "#quot;"),
        }
    }

    /// One edge statement, newline included.
    pub(crate) fn link(self, link: Link, from: &str, to: &str) -> String {
        match (self, link) {
            (Syntax::PlantUml, Link::Arrow) => format!("{from} --> {to}\n"),
            (Syntax::PlantUml, Link::Labeled(label)) => format!("{from} --> {to} : {label}\n"),
            (Syntax::PlantUml, Link::Dotted(label)) => format!("{from} ..> {to} : {label}\n"),
            (Syntax::PlantUml, Link::Undirected) => format!("{from} -- {to}\n"),
            (Syntax::Mermaid, Link::Arrow) => format!("  {from} --> {to}\n"),
            (Syntax::Mermaid, Link::Labeled(label)) => format!("  {from} -->|{label}| {to}\n"),
            (Syntax::Mermaid, Link::Dotted(label)) => format!("  {from} -.->|{label}| {to}\n"),
            (Syntax::Mermaid, Link::Undirected) => format!("  {from} --- {to}\n"),
        }
    }

    /// The edge the RDRA graph draws for a relation, or `None` for the kinds it leaves out.
    pub(crate) fn relation_edge(self, kind: &RelKind, from: &str, to: &str) -> Option<String> {
        let label = object_graph_rel_label(kind);
        let link = match kind {
            RelKind::Transitions | RelKind::Invokes => return None,
            RelKind::Performs => Link::Arrow,
            RelKind::Contains | RelKind::Belongs if self == Syntax::Mermaid => Link::Arrow,
            RelKind::Precedes | RelKind::Owns => Link::Labeled(label),
            RelKind::RelateOneToOne
            | RelKind::RelateOneToMany
            | RelKind::RelateManyToOne
            | RelKind::RelateManyToMany => Link::Undirected,
            RelKind::Reads
            | RelKind::Writes
            | RelKind::Creates
            | RelKind::Updates
            | RelKind::Deletes
            | RelKind::Displays
            | RelKind::Shows
            | RelKind::Raises
            | RelKind::Triggers
            | RelKind::Contains
            | RelKind::Belongs
            | RelKind::HasPermission
            | RelKind::RequiresPermission
            | RelKind::RequiresMedium
            | RelKind::Motivates
            | RelKind::Decides
            | RelKind::Branches
            | RelKind::Excepts
            | RelKind::Repeats
            | RelKind::Covers
            | RelKind::Compensates
            | RelKind::Request
            | RelKind::Response
            | RelKind::ErrorResponse
            | RelKind::AppliesTo
            | RelKind::Combines
            | RelKind::Qualifies
            | RelKind::Constrains
            | RelKind::MapsField => Link::Dotted(label),
        };
        Some(self.link(link, from, to))
    }

    /// A note per node that has a description, attached to the node.
    pub(crate) fn render_descriptions(
        self,
        out: &mut String,
        model: &SemanticModel,
        nodes: &[NodeRef],
    ) {
        for node in nodes {
            let (Some(id), Some(description)) =
                (node_id(model, node), node_description(model, node))
            else {
                continue;
            };
            let description = description.trim();
            if description.is_empty() {
                continue;
            }
            match self {
                Syntax::PlantUml => out.push_str(&format!(
                    "note right of {}\n{}\nend note\n",
                    id,
                    self.quote(description)
                )),
                Syntax::Mermaid => {
                    let note_id = format!("{}_description", id);
                    let description = self.quote(description).replace('\n', "<br/>");
                    out.push_str(&format!("  {}[\"{}\"]\n", note_id, description));
                    out.push_str(&format!("  {} -. description .- {}\n", note_id, id));
                }
            }
        }
    }
}

/// The edges between the visible nodes of the RDRA graph, in a stable order.
pub(crate) fn render_relations(
    syntax: Syntax,
    out: &mut String,
    model: &SemanticModel,
    reachable: &Option<HashSet<NodeRef>>,
    view: &View,
) {
    let mut relations: Vec<_> = model.relations.iter().collect();
    relations.sort_by_key(|relation| format!("{:?}{:?}", relation.from, relation.to));
    for relation in &relations {
        if !graph_node_visible(reachable, view, &relation.from)
            || !graph_node_visible(reachable, view, &relation.to)
            || !view_relation_visible(view, &relation.kind)
        {
            continue;
        }
        if matches!(&relation.from, NodeRef::Api(_)) || matches!(&relation.to, NodeRef::Api(_)) {
            continue;
        }
        if let (Some(from_id), Some(to_id)) =
            (node_id(model, &relation.from), node_id(model, &relation.to))
        {
            if let Some(edge) = syntax.relation_edge(&relation.kind, from_id, to_id) {
                out.push_str(&edge);
            }
        }
    }
}

/// A node the event-flow diagram declares.
#[derive(Clone, Copy)]
pub(crate) enum FlowNode {
    Event(EventKey),
    UseCase(UseCaseKey),
    Buc(BucKey),
    State(StateKey),
    ExtSystem(ExtSystemKey),
    Timer(TimerKey),
}

impl FlowNode {
    fn node_ref(self) -> NodeRef {
        match self {
            FlowNode::Event(key) => NodeRef::Event(key),
            FlowNode::UseCase(key) => NodeRef::UseCase(key),
            FlowNode::Buc(key) => NodeRef::Buc(key),
            FlowNode::State(key) => NodeRef::State(key),
            FlowNode::ExtSystem(key) => NodeRef::ExtSystem(key),
            FlowNode::Timer(key) => NodeRef::Timer(key),
        }
    }

    fn id_prefix(self) -> &'static str {
        match self {
            FlowNode::Event(_) => "ev",
            FlowNode::UseCase(_) => "uc",
            FlowNode::Buc(_) => "buc",
            FlowNode::State(_) => "st",
            FlowNode::ExtSystem(_) => "xs",
            FlowNode::Timer(_) => "tm",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            FlowNode::Event(_) => "⚡",
            FlowNode::UseCase(_) => "✅",
            FlowNode::Buc(_) => "📦",
            FlowNode::State(_) => "🔄",
            FlowNode::ExtSystem(_) => "🌐",
            FlowNode::Timer(_) => "⏱️",
        }
    }
}

impl Syntax {
    /// The statement that declares an event-flow node: a PlantUML element keyword, or a
    /// Mermaid shape.
    fn flow_node_declaration(self, node: FlowNode, id: &str, label: &str) -> String {
        match self {
            Syntax::PlantUml => {
                let keyword = match node {
                    FlowNode::Event(_) => "card",
                    FlowNode::UseCase(_) => "usecase",
                    FlowNode::Buc(_) | FlowNode::Timer(_) => "rectangle",
                    FlowNode::State(_) => "state",
                    FlowNode::ExtSystem(_) => "component",
                };
                format!("{keyword} \"{label}\" as {id}\n")
            }
            Syntax::Mermaid => {
                let (open, close) = match node {
                    FlowNode::Event(_) => ("{", "}"),
                    FlowNode::UseCase(_) => ("([", "])"),
                    FlowNode::Buc(_) => ("[[", "]]"),
                    FlowNode::State(_) => ("(", ")"),
                    FlowNode::ExtSystem(_) => ("[/", "/]"),
                    FlowNode::Timer(_) => ("{{", "}}"),
                };
                format!("  {id}{open}\"{label}\"{close}\n")
            }
        }
    }
}

/// Declare `node` once per diagram and return its diagram id, or `None` when the model no
/// longer holds it.
pub(crate) fn declare_flow_node(
    syntax: Syntax,
    out: &mut String,
    declared: &mut HashSet<String>,
    model: &SemanticModel,
    node: FlowNode,
) -> Option<String> {
    let node_ref = node.node_ref();
    let id = format!("{}__{}", node.id_prefix(), node_id(model, &node_ref)?);
    if declared.insert(id.clone()) {
        let label = prefixed_label(node.icon(), node_label(model, &node_ref)?);
        out.push_str(&syntax.flow_node_declaration(node, &id, &label));
    }
    Some(id)
}

/// Every event flow and timer flow the scope shows.
pub(crate) fn render_event_flows(
    syntax: Syntax,
    out: &mut String,
    model: &SemanticModel,
    scope: &Scope,
) {
    let reachable = reachable_for_scope(model, scope);
    let mut declared: HashSet<String> = HashSet::new();
    for flow in &rdra_ish_core::collect_event_flows(model) {
        render_event_flow(syntax, out, &mut declared, model, &reachable, flow);
    }
    for timer_flow in rdra_ish_core::collect_timer_flows(model) {
        render_timer_flow(syntax, out, &mut declared, model, &reachable, &timer_flow);
    }
}

pub(crate) fn render_event_flow(
    syntax: Syntax,
    out: &mut String,
    declared: &mut HashSet<String>,
    model: &SemanticModel,
    reachable: &Option<HashSet<NodeRef>>,
    flow: &EventFlow,
) {
    let event_node = NodeRef::Event(flow.event);
    if !scoped_node_visible(reachable, &event_node) {
        return;
    }
    let Some(event_id) =
        declare_flow_node(syntax, out, declared, model, FlowNode::Event(flow.event))
    else {
        return;
    };

    let mut raised_by = flow.raised_by.to_vec();
    raised_by.sort_by_key(|raiser| match raiser {
        Raiser::UseCase(usecase) => (
            0,
            model
                .use_cases
                .get(*usecase)
                .map(|uc| uc.id.as_str())
                .unwrap_or(""),
        ),
        Raiser::ExtSystem(extsystem) => (
            1,
            model
                .ext_systems
                .get(*extsystem)
                .map(|x| x.id.as_str())
                .unwrap_or(""),
        ),
    });
    for raiser in raised_by {
        let (node, flow_node) = match raiser {
            Raiser::UseCase(usecase) => (NodeRef::UseCase(usecase), FlowNode::UseCase(usecase)),
            Raiser::ExtSystem(extsystem) => (
                NodeRef::ExtSystem(extsystem),
                FlowNode::ExtSystem(extsystem),
            ),
        };
        let declared_id = declare_flow_node(syntax, out, declared, model, flow_node);
        if !scoped_node_visible(reachable, &node) {
            continue;
        }
        if let Some(raiser_id) = declared_id {
            out.push_str(&syntax.link(Link::Dotted("raises"), &raiser_id, &event_id));
        }
    }

    let mut triggers = flow.triggers_ucs.to_vec();
    triggers.sort_by_key(|&uk| {
        model
            .use_cases
            .get(uk)
            .map(|uc| uc.id.as_str())
            .unwrap_or("")
    });
    for usecase in triggers {
        let node = NodeRef::UseCase(usecase);
        if !scoped_node_visible(reachable, &node) {
            continue;
        }
        if let Some(usecase_id) =
            declare_flow_node(syntax, out, declared, model, FlowNode::UseCase(usecase))
        {
            out.push_str(&syntax.link(Link::Dotted("triggers"), &event_id, &usecase_id));
        }
    }

    let mut triggered_bucs = flow.triggers_bucs.to_vec();
    triggered_bucs.sort_by_key(|&bk| model.bucs.get(bk).map(|buc| buc.id.as_str()).unwrap_or(""));
    for buc in triggered_bucs {
        let node = NodeRef::Buc(buc);
        if !scoped_node_visible(reachable, &node) {
            continue;
        }
        if let Some(buc_id) = declare_flow_node(syntax, out, declared, model, FlowNode::Buc(buc)) {
            out.push_str(&syntax.link(Link::Dotted("triggers"), &event_id, &buc_id));
        }
    }

    let mut delivered_to = flow.delivered_to.to_vec();
    delivered_to.sort_by_key(|&xk| {
        model
            .ext_systems
            .get(xk)
            .map(|x| x.id.as_str())
            .unwrap_or("")
    });
    for extsystem in delivered_to {
        let node = NodeRef::ExtSystem(extsystem);
        if !scoped_node_visible(reachable, &node) {
            continue;
        }
        if let Some(extsystem_id) =
            declare_flow_node(syntax, out, declared, model, FlowNode::ExtSystem(extsystem))
        {
            out.push_str(&syntax.link(Link::Dotted("delivers"), &event_id, &extsystem_id));
        }
    }

    let state_id_to_key: HashMap<String, StateKey> = model
        .states
        .iter()
        .map(|(sk, s)| (s.id.clone(), sk))
        .collect();
    let resolved_transitions: Vec<(StateKey, StateKey)> = flow
        .transitions
        .iter()
        .filter_map(|st| {
            let entity_id = &model.entities.get(st.entity)?.id;
            let from_sk = state_id_to_key
                .get(&format!("{entity_id}_{}", st.from))
                .copied()?;
            let to_sk = state_id_to_key
                .get(&format!("{entity_id}_{}", st.to))
                .copied()?;
            Some((from_sk, to_sk))
        })
        .collect();
    let mut transitions = resolved_transitions;
    transitions.sort_by_key(|(from, _)| {
        model
            .states
            .get(*from)
            .map(|state| state.id.clone())
            .unwrap_or_default()
    });
    let event_label = model
        .events
        .get(flow.event)
        .map(|event| prefixed_label("⚡", &event.label))
        .unwrap_or_default();
    for (from, to) in &transitions {
        let Some(from_id) = declare_flow_node(syntax, out, declared, model, FlowNode::State(*from))
        else {
            continue;
        };
        let Some(to_id) = declare_flow_node(syntax, out, declared, model, FlowNode::State(*to))
        else {
            continue;
        };
        out.push_str(&syntax.link(Link::Labeled(&event_label), &from_id, &to_id));
    }
}

pub(crate) fn render_timer_flow(
    syntax: Syntax,
    out: &mut String,
    declared: &mut HashSet<String>,
    model: &SemanticModel,
    reachable: &Option<HashSet<NodeRef>>,
    flow: &TimerFlow,
) {
    if !scoped_node_visible(reachable, &NodeRef::Timer(flow.timer)) {
        return;
    }
    let Some(timer_id) =
        declare_flow_node(syntax, out, declared, model, FlowNode::Timer(flow.timer))
    else {
        return;
    };

    let mut usecases = flow.triggers_ucs.to_vec();
    usecases.sort_by_key(|&uk| {
        model
            .use_cases
            .get(uk)
            .map(|uc| uc.id.as_str())
            .unwrap_or("")
    });
    for usecase in usecases {
        if !scoped_node_visible(reachable, &NodeRef::UseCase(usecase)) {
            continue;
        }
        if let Some(usecase_id) =
            declare_flow_node(syntax, out, declared, model, FlowNode::UseCase(usecase))
        {
            out.push_str(&syntax.link(Link::Dotted("triggers"), &timer_id, &usecase_id));
        }
    }

    let mut bucs = flow.triggers_bucs.to_vec();
    bucs.sort_by_key(|&bk| model.bucs.get(bk).map(|buc| buc.id.as_str()).unwrap_or(""));
    for buc in bucs {
        if !scoped_node_visible(reachable, &NodeRef::Buc(buc)) {
            continue;
        }
        if let Some(buc_id) = declare_flow_node(syntax, out, declared, model, FlowNode::Buc(buc)) {
            out.push_str(&syntax.link(Link::Dotted("triggers"), &timer_id, &buc_id));
        }
    }
}

#[cfg(test)]
pub(crate) fn model_from(src: &str) -> SemanticModel {
    let (ast, _) = rdra_ish_syntax::parse(src);
    let (model, _) = rdra_ish_core::build_model(&ast);
    model
}
