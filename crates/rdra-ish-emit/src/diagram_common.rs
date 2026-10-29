//! Helpers the Mermaid emitters share.

use crate::{
    node_description, node_kind_filter_name, prefixed_label, ObjectGraphLayer, Scope, View,
};
use rdra_ish_core::lifecycle::{Bounds, Scope as LifecycleScope, TransitionSystem};
use rdra_ish_core::model::{
    BucKey, ColumnType, EntityKey, EventKey, ExtSystemKey, NodeRef, Raiser, SemanticModel,
    StateKey, TimerKey, UseCaseKey,
};
use rdra_ish_core::{EventFlow, TimerFlow};
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

/// An edge the shared emitters draw.
pub(crate) enum Link<'a> {
    /// Solid arrow with a label.
    Labeled(&'a str),
    /// Dotted arrow with a label.
    Dotted(&'a str),
}

/// `label` safe inside a double-quoted string.
pub(crate) fn quote(label: &str) -> String {
    label.replace('"', "#quot;")
}

/// One edge statement, newline included.
pub(crate) fn link_statement(link: Link, from: &str, to: &str) -> String {
    match link {
        Link::Labeled(label) => format!("  {from} -->|{label}| {to}\n"),
        Link::Dotted(label) => format!("  {from} -.->|{label}| {to}\n"),
    }
}

/// The node id of the encoding contract, `<kind token>__<DSL id>`, the same in every kind.
pub(crate) fn contract_id(model: &SemanticModel, node: &NodeRef) -> Option<String> {
    Some(format!(
        "{}__{}",
        node_kind_filter_name(node),
        node_id(model, node)?
    ))
}

/// The `%%` lines that open every diagram: kind, scope, and the vocabulary the body uses.
pub(crate) struct Header<'a> {
    pub(crate) diagram: &'a str,
    pub(crate) view: &'a View,
    /// `None` for a kind without RDRA layers: the `layers:` line is left out.
    pub(crate) layers: Option<Vec<ObjectGraphLayer>>,
    pub(crate) kinds: Vec<&'static str>,
    pub(crate) omitted: Vec<String>,
}

impl Header<'_> {
    pub(crate) fn render(&self) -> String {
        let scope = match &self.view.scope {
            Scope::Whole => "whole".to_string(),
            Scope::Bucs(ids) => format!("buc:{}", ids.join(",")),
            Scope::UseCases(ids) => format!("usecase:{}", ids.join(",")),
        };
        let mut out = format!("%% rdra-ish diagram={} scope={scope}", self.diagram);
        if let Some(model_path) = &self.view.model_path {
            out.push_str(&format!(" model={model_path}"));
        }
        out.push('\n');
        if let Some(layers) = &self.layers {
            let layers: Vec<String> = layers
                .iter()
                .map(|layer| format!("{}={}", layer.token(), layer.label()))
                .collect();
            out.push_str(&format!("%% layers: {}\n", or_none(&layers.join(" "))));
        }
        out.push_str(&format!("%% kinds: {}\n", or_none(&self.kinds.join(" "))));
        out.push_str(&format!(
            "%% omitted: {}\n",
            or_none(&self.omitted.join(" "))
        ));
        out
    }
}

fn or_none(list: &str) -> &str {
    if list.is_empty() {
        "(none)"
    } else {
        list
    }
}

/// `%% desc <id>: <text>` for a node that has a description, its lines joined by a space.
pub(crate) fn desc_line(model: &SemanticModel, node: &NodeRef, id: &str) -> Option<String> {
    let text = node_description(model, node)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!text.is_empty()).then(|| format!("%% desc {id}: {text}"))
}

/// One `classDef` per kind, so Mermaid accepts the `:::<kind>` on each node. The style
/// carries no meaning.
pub(crate) fn class_defs(kinds: &[&str]) -> String {
    kinds
        .iter()
        .map(|kind| format!("  classDef {kind} stroke:#333;\n"))
        .collect()
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

/// The statement that declares an event-flow node: a Mermaid shape.
fn flow_node_declaration(node: FlowNode, id: &str, label: &str) -> String {
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

/// Declare `node` once per diagram and return its diagram id, or `None` when the model no
/// longer holds it.
pub(crate) fn declare_flow_node(
    out: &mut String,
    declared: &mut HashSet<String>,
    model: &SemanticModel,
    node: FlowNode,
) -> Option<String> {
    let node_ref = node.node_ref();
    let id = format!("{}__{}", node.id_prefix(), node_id(model, &node_ref)?);
    if declared.insert(id.clone()) {
        let label = prefixed_label(node.icon(), node_label(model, &node_ref)?);
        out.push_str(&flow_node_declaration(node, &id, &label));
    }
    Some(id)
}

/// Every event flow and timer flow the scope shows.
pub(crate) fn render_event_flows(out: &mut String, model: &SemanticModel, scope: &Scope) {
    let reachable = reachable_for_scope(model, scope);
    let mut declared: HashSet<String> = HashSet::new();
    for flow in &rdra_ish_core::collect_event_flows(model) {
        render_event_flow(out, &mut declared, model, &reachable, flow);
    }
    for timer_flow in rdra_ish_core::collect_timer_flows(model) {
        render_timer_flow(out, &mut declared, model, &reachable, &timer_flow);
    }
}

pub(crate) fn render_event_flow(
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
    let Some(event_id) = declare_flow_node(out, declared, model, FlowNode::Event(flow.event))
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
        let declared_id = declare_flow_node(out, declared, model, flow_node);
        if !scoped_node_visible(reachable, &node) {
            continue;
        }
        if let Some(raiser_id) = declared_id {
            out.push_str(&link_statement(
                Link::Dotted("raises"),
                &raiser_id,
                &event_id,
            ));
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
            declare_flow_node(out, declared, model, FlowNode::UseCase(usecase))
        {
            out.push_str(&link_statement(
                Link::Dotted("triggers"),
                &event_id,
                &usecase_id,
            ));
        }
    }

    let mut triggered_bucs = flow.triggers_bucs.to_vec();
    triggered_bucs.sort_by_key(|&bk| model.bucs.get(bk).map(|buc| buc.id.as_str()).unwrap_or(""));
    for buc in triggered_bucs {
        let node = NodeRef::Buc(buc);
        if !scoped_node_visible(reachable, &node) {
            continue;
        }
        if let Some(buc_id) = declare_flow_node(out, declared, model, FlowNode::Buc(buc)) {
            out.push_str(&link_statement(
                Link::Dotted("triggers"),
                &event_id,
                &buc_id,
            ));
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
            declare_flow_node(out, declared, model, FlowNode::ExtSystem(extsystem))
        {
            out.push_str(&link_statement(
                Link::Dotted("delivers"),
                &event_id,
                &extsystem_id,
            ));
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
        let Some(from_id) = declare_flow_node(out, declared, model, FlowNode::State(*from)) else {
            continue;
        };
        let Some(to_id) = declare_flow_node(out, declared, model, FlowNode::State(*to)) else {
            continue;
        };
        out.push_str(&link_statement(
            Link::Labeled(&event_label),
            &from_id,
            &to_id,
        ));
    }
}

pub(crate) fn render_timer_flow(
    out: &mut String,
    declared: &mut HashSet<String>,
    model: &SemanticModel,
    reachable: &Option<HashSet<NodeRef>>,
    flow: &TimerFlow,
) {
    if !scoped_node_visible(reachable, &NodeRef::Timer(flow.timer)) {
        return;
    }
    let Some(timer_id) = declare_flow_node(out, declared, model, FlowNode::Timer(flow.timer))
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
            declare_flow_node(out, declared, model, FlowNode::UseCase(usecase))
        {
            out.push_str(&link_statement(
                Link::Dotted("triggers"),
                &timer_id,
                &usecase_id,
            ));
        }
    }

    let mut bucs = flow.triggers_bucs.to_vec();
    bucs.sort_by_key(|&bk| model.bucs.get(bk).map(|buc| buc.id.as_str()).unwrap_or(""));
    for buc in bucs {
        if !scoped_node_visible(reachable, &NodeRef::Buc(buc)) {
            continue;
        }
        if let Some(buc_id) = declare_flow_node(out, declared, model, FlowNode::Buc(buc)) {
            out.push_str(&link_statement(
                Link::Dotted("triggers"),
                &timer_id,
                &buc_id,
            ));
        }
    }
}

#[cfg(test)]
pub(crate) fn model_from(src: &str) -> SemanticModel {
    let (ast, _) = rdra_ish_syntax::parse(src);
    let (model, _) = rdra_ish_core::build_model(&ast);
    model
}
