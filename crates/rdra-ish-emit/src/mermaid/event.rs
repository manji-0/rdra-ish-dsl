//! The event diagram (`--kind event`).

use crate::diagram_common::{
    class_defs, contract_id, desc_line, edge_line, node_line, quote, reachable_for_scope, Diagram,
    Header, NodeScope,
};
use crate::{EmitError, Emitter, Scope, View};
use rdra_ish_core::model::{
    BucKey, EventKey, ExtSystemKey, NodeRef, Raiser, SemanticModel, TimerKey, UseCaseKey,
};
use rdra_ish_core::{EventFlow, TimerFlow};
use std::collections::HashSet;

/// A node the event diagram declares.
#[derive(Clone, Copy)]
pub(super) enum FlowNode {
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
pub(super) struct Declared {
    pub(super) ids: HashSet<String>,
    pub(super) kinds: Vec<&'static str>,
}

/// Declare `node` once per diagram and return its diagram id, or `None` when the model no
/// longer holds it.
pub(super) fn declare_flow_node(
    out: &mut String,
    declared: &mut Declared,
    model: &SemanticModel,
    node: FlowNode,
) -> Option<String> {
    let node_ref = node.node_ref();
    let id = contract_id(model, &node_ref)?;
    let label = model.node_label(&node_ref)?;
    if declared.ids.insert(id.clone()) {
        let kind = node_ref.kind_tag();
        out.push_str(&node_line("  ", &id, &quote(label), kind));
        if let Some(desc) = desc_line(model, &node_ref, &id) {
            out.push_str(&format!("  {desc}\n"));
        }
        if !declared.kinds.contains(&kind) {
            declared.kinds.push(kind);
        }
    }
    Some(id)
}

/// One association edge of the event diagram, newline included.
fn association(from: &str, predicate: &str, to: &str) -> String {
    edge_line(Diagram::Event, from, "-.->", predicate, to)
}

/// Every event flow and timer flow the scope shows.
pub(super) fn render_event_flows(
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

pub(super) fn render_event_flow(
    out: &mut String,
    declared: &mut Declared,
    model: &SemanticModel,
    reachable: &NodeScope,
    flow: &EventFlow,
) {
    if !reachable.contains(&NodeRef::Event(flow.event)) {
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
        if !reachable.contains(&raiser.node_ref()) {
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
        if !reachable.contains(&target.node_ref()) {
            continue;
        }
        if let Some(target_id) = declare_flow_node(out, declared, model, target) {
            out.push_str(&association(&event_id, "triggers", &target_id));
        }
    }
}

fn render_timer_flow(
    out: &mut String,
    declared: &mut Declared,
    model: &SemanticModel,
    reachable: &NodeScope,
    flow: &TimerFlow,
) {
    if !reachable.contains(&NodeRef::Timer(flow.timer)) {
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
        if !reachable.contains(&target.node_ref()) {
            continue;
        }
        if let Some(target_id) = declare_flow_node(out, declared, model, target) {
            out.push_str(&association(&timer_id, "triggers", &target_id));
        }
    }
}

/// The system-to-system event links: `raises` into each event and `triggers` out of it, and
/// each timer's `triggers`. The transitions a triggered use case drives are the state
/// diagram's.
pub struct EventMermaidEmitter;

impl Emitter for EventMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let mut body = String::new();
        let declared = render_event_flows(&mut body, model, &view.scope);
        body.push_str(&class_defs(&declared.kinds));
        let drops_transitions = model.state_transitions.iter().any(|t| {
            contract_id(model, &NodeRef::UseCase(t.usecase))
                .is_some_and(|id| declared.ids.contains(&id))
        });
        let header = Header {
            diagram: Diagram::Event,
            view,
            layers: None,
            kinds: declared.kinds,
            omitted: if drops_transitions {
                vec!["state transitions".to_string()]
            } else {
                Vec::new()
            },
        };
        Ok(format!("{}flowchart LR\n{body}", header.render()))
    }
}
