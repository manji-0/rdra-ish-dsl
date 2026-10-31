//! Helpers the Mermaid emitters share.

use crate::{node_description, node_kind_filter_name, ObjectGraphLayer, Scope, View};
use rdra_ish_core::lifecycle::{Bounds, Scope as LifecycleScope, TransitionSystem};
use rdra_ish_core::model::{
    BucKey, ColumnType, EntityKey, EventKey, ExtSystemKey, NodeRef, Raiser, SemanticModel,
    StateKey, TimerKey, UseCaseKey,
};
use rdra_ish_core::{EventFlow, TimerFlow};
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
        NodeRef::Goal(k) => model.goals.get(*k).map(|g| g.id.as_str()),
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
        NodeRef::Goal(k) => model.goals.get(*k).map(|g| g.label.as_str()),
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

/// `label` safe inside a double-quoted string.
pub(crate) fn quote(label: &str) -> String {
    label.replace('"', "#quot;")
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

/// One `classDef` per kind, so Mermaid accepts the `:::<kind>` on each node. Each RDRA
/// element family has its own fill, so kinds tell apart in a plain Mermaid render; the
/// colour says no more than the `:::<kind>` tag does.
pub(crate) fn class_defs(kinds: &[&str]) -> String {
    kinds
        .iter()
        .map(|kind| format!("  classDef {kind} {};\n", kind_style(kind)))
        .collect()
}

/// Light fills with dark text and stroke, readable on a light or a dark page.
fn kind_style(kind: &str) -> &'static str {
    match kind {
        "actor" => "fill:#ffe3e3,stroke:#c92a2a,color:#000",
        "extsystem" => "fill:#e9ecef,stroke:#495057,color:#000",
        "requirement" | "goal" | "nfr" | "quality" | "constraint" | "adr" => {
            "fill:#fff3bf,stroke:#e67700,color:#000"
        }
        "business" | "buc" | "flow" | "step" | "lane" => "fill:#d3f9d8,stroke:#2b8a3e,color:#000",
        "timer" => "fill:#ffe8cc,stroke:#d9480f,color:#000",
        "usecase" => "fill:#d0ebff,stroke:#1864ab,color:#000",
        "screen" | "field" => "fill:#e5dbff,stroke:#5f3dc4,color:#000",
        "event" => "fill:#ffdeeb,stroke:#a61e4d,color:#000",
        "api" | "dto" | "system" => "fill:#c5f6fa,stroke:#0b7285,color:#000",
        "entity" | "statemodel" => "fill:#fff9db,stroke:#5c940d,color:#000",
        "state" => "fill:#f8f9fa,stroke:#5c940d,color:#000",
        "variation" | "value" | "condition" => "fill:#f3d9fa,stroke:#862e9c,color:#000",
        _ => "fill:#f1f3f5,stroke:#333,color:#000",
    }
}

/// A node the event diagram declares.
#[derive(Clone, Copy)]
pub(crate) enum FlowNode {
    Event(EventKey),
    UseCase(UseCaseKey),
    Buc(BucKey),
    ExtSystem(ExtSystemKey),
    Timer(TimerKey),
}

impl FlowNode {
    fn node_ref(self) -> NodeRef {
        match self {
            FlowNode::Event(key) => NodeRef::Event(key),
            FlowNode::UseCase(key) => NodeRef::UseCase(key),
            FlowNode::Buc(key) => NodeRef::Buc(key),
            FlowNode::ExtSystem(key) => NodeRef::ExtSystem(key),
            FlowNode::Timer(key) => NodeRef::Timer(key),
        }
    }
}

/// The nodes an event diagram has declared, and their kinds in the order first declared.
#[derive(Default)]
pub(crate) struct Declared {
    pub(crate) ids: HashSet<String>,
    pub(crate) kinds: Vec<&'static str>,
}

/// Declare `node` once per diagram and return its diagram id, or `None` when the model no
/// longer holds it.
pub(crate) fn declare_flow_node(
    out: &mut String,
    declared: &mut Declared,
    model: &SemanticModel,
    node: FlowNode,
) -> Option<String> {
    let node_ref = node.node_ref();
    let id = contract_id(model, &node_ref)?;
    let label = node_label(model, &node_ref)?;
    if declared.ids.insert(id.clone()) {
        let kind = node_kind_filter_name(&node_ref);
        out.push_str(&format!("  {id}[\"{}\"]:::{kind}\n", quote(label)));
        if let Some(desc) = desc_line(model, &node_ref, &id) {
            out.push_str(&format!("  {desc}\n"));
        }
        if !declared.kinds.contains(&kind) {
            declared.kinds.push(kind);
        }
    }
    Some(id)
}

/// One association edge, labelled with its DSL predicate, newline included.
fn association(from: &str, predicate: &str, to: &str) -> String {
    format!("  {from} -.->|{predicate}| {to}\n")
}

/// Every event flow and timer flow the scope shows.
pub(crate) fn render_event_flows(
    out: &mut String,
    model: &SemanticModel,
    scope: &Scope,
) -> Declared {
    let reachable = reachable_for_scope(model, scope);
    let mut declared = Declared::default();
    for flow in &rdra_ish_core::collect_event_flows(model) {
        render_event_flow(out, &mut declared, model, &reachable, flow);
    }
    for timer_flow in rdra_ish_core::collect_timer_flows(model) {
        render_timer_flow(out, &mut declared, model, &reachable, &timer_flow);
    }
    declared
}

/// Sort keys by the DSL id the model gives them.
fn by_id<K: Copy>(keys: &[K], id: impl Fn(K) -> Option<String>) -> Vec<K> {
    let mut keys = keys.to_vec();
    keys.sort_by_key(|&key| id(key).unwrap_or_default());
    keys
}

pub(crate) fn render_event_flow(
    out: &mut String,
    declared: &mut Declared,
    model: &SemanticModel,
    reachable: &Option<HashSet<NodeRef>>,
    flow: &EventFlow,
) {
    if !scoped_node_visible(reachable, &NodeRef::Event(flow.event)) {
        return;
    }
    let Some(event_id) = declare_flow_node(out, declared, model, FlowNode::Event(flow.event))
    else {
        return;
    };

    let mut raised_by = flow.raised_by.to_vec();
    raised_by.sort_by_key(|raiser| match raiser {
        Raiser::UseCase(usecase) => (0, contract_id(model, &NodeRef::UseCase(*usecase))),
        Raiser::ExtSystem(extsystem) => (1, contract_id(model, &NodeRef::ExtSystem(*extsystem))),
    });
    let raisers = raised_by.into_iter().map(|raiser| match raiser {
        Raiser::UseCase(usecase) => FlowNode::UseCase(usecase),
        Raiser::ExtSystem(extsystem) => FlowNode::ExtSystem(extsystem),
    });
    for raiser in raisers {
        if !scoped_node_visible(reachable, &raiser.node_ref()) {
            continue;
        }
        if let Some(raiser_id) = declare_flow_node(out, declared, model, raiser) {
            out.push_str(&association(&raiser_id, "raises", &event_id));
        }
    }

    let id = |node: NodeRef| contract_id(model, &node);
    let targets = by_id(&flow.triggers_ucs, |k| id(NodeRef::UseCase(k)))
        .into_iter()
        .map(FlowNode::UseCase)
        .chain(
            by_id(&flow.triggers_bucs, |k| id(NodeRef::Buc(k)))
                .into_iter()
                .map(FlowNode::Buc),
        )
        .chain(
            by_id(&flow.delivered_to, |k| id(NodeRef::ExtSystem(k)))
                .into_iter()
                .map(FlowNode::ExtSystem),
        );
    for target in targets {
        if !scoped_node_visible(reachable, &target.node_ref()) {
            continue;
        }
        if let Some(target_id) = declare_flow_node(out, declared, model, target) {
            out.push_str(&association(&event_id, "triggers", &target_id));
        }
    }
}

pub(crate) fn render_timer_flow(
    out: &mut String,
    declared: &mut Declared,
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
    let id = |node: NodeRef| contract_id(model, &node);
    let targets = by_id(&flow.triggers_ucs, |k| id(NodeRef::UseCase(k)))
        .into_iter()
        .map(FlowNode::UseCase)
        .chain(
            by_id(&flow.triggers_bucs, |k| id(NodeRef::Buc(k)))
                .into_iter()
                .map(FlowNode::Buc),
        );
    for target in targets {
        if !scoped_node_visible(reachable, &target.node_ref()) {
            continue;
        }
        if let Some(target_id) = declare_flow_node(out, declared, model, target) {
            out.push_str(&association(&timer_id, "triggers", &target_id));
        }
    }
}

#[cfg(test)]
pub(crate) fn model_from(src: &str) -> SemanticModel {
    let (ast, _) = rdra_ish_syntax::parse(src);
    let (model, _) = rdra_ish_core::build_model(&ast);
    model
}
