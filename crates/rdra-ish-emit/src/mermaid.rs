//! Mermaid emitters: RDRA全体図、システムコンテキスト、要求モデル、ビジネスコンテキスト、UC複合図、ロバストネス図、情報モデル、バリエーション・条件、業務フロー、画面遷移図、ER図、状態遷移図、イベント図。

mod business;
mod condition;
mod context;
mod er;
mod event;
mod flow;
mod information;
mod rdra;
mod requirement;
mod robustness;
mod screen;
mod sequence;
mod state;
mod usecase;

pub use business::BusinessMermaidEmitter;
pub use condition::ConditionMermaidEmitter;
pub use context::ContextMermaidEmitter;
pub use er::ErMermaidEmitter;
pub use event::EventMermaidEmitter;
pub use flow::FlowMermaidEmitter;
pub use information::InformationMermaidEmitter;
pub use rdra::RdraMermaidEmitter;
pub use requirement::RequirementMermaidEmitter;
pub use robustness::RobustnessMermaidEmitter;
pub use screen::ScreenMermaidEmitter;
pub use sequence::SequenceMermaidEmitter;
pub use state::StateMermaidEmitter;
pub use usecase::UsecaseMermaidEmitter;

use crate::diagram_common::{contract_id, desc_line, edge_line, node_line, quote};
use crate::{
    collect_object_graph_nodes, is_relate, object_graph_layer, object_graph_rel_label,
    ObjectGraphLayer, OBJECT_GRAPH_LAYERS,
};
use rdra_ish_core::model::{NodeRef, RelKind, Relation, SemanticModel, SetValue};
use rdra_ish_core::{buc_members, state_key};
use std::collections::{BTreeMap, BTreeSet, HashSet};

// ── 共通のフローチャート部品 ──────────────────────────────────────────────────

/// One edge of a flowchart before it is written.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct GraphEdge {
    from: String,
    label: String,
    to: String,
    arrow: &'static str,
    /// What the edge was derived from, written as a `%% derived` line after it.
    derived: Option<String>,
}

/// The nodes of a layered flowchart: one `subgraph layer_<token>` per layer that holds one,
/// each node with its `:::<kind>` and its `%% desc` line. Returns the layers drawn.
fn render_layered_nodes(
    out: &mut String,
    model: &SemanticModel,
    nodes: &[NodeRef],
) -> Vec<ObjectGraphLayer> {
    let mut layers = Vec::new();
    for layer in OBJECT_GRAPH_LAYERS {
        let nodes: Vec<(&NodeRef, String, &str)> = nodes
            .iter()
            .filter(|nr| object_graph_layer(nr) == layer)
            .filter_map(|nr| Some((nr, contract_id(model, nr)?, model.node_label(nr)?)))
            .collect();
        if nodes.is_empty() {
            continue;
        }
        layers.push(layer);
        out.push_str(&format!(
            "  subgraph layer_{}[\"{}\"]\n",
            layer.token(),
            layer.label()
        ));
        for (nr, id, label) in nodes {
            out.push_str(&node_line("    ", &id, &quote(label), nr.kind_tag()));
            if let Some(desc) = desc_line(model, nr, &id) {
                out.push_str(&format!("    {desc}\n"));
            }
        }
        out.push_str("  end\n");
    }
    layers
}

/// `node` with its `:::<kind>` and its `%% desc` line, at `indent`.
fn declare(out: &mut String, model: &SemanticModel, node: &NodeRef, indent: &str) {
    let (Some(id), Some(label)) = (contract_id(model, node), model.node_label(node)) else {
        return;
    };
    let kind = node.kind_tag();
    out.push_str(&node_line(indent, &id, &quote(label), kind));
    if let Some(desc) = desc_line(model, node, &id) {
        out.push_str(&format!("{indent}{desc}\n"));
    }
}

type EdgeKey = (String, String, String);

/// The edges of a flowchart keyed by `(from, label, to)`, each drawn once. A declared edge
/// wins over a derived one, and a derived edge names every node it goes through.
#[derive(Default)]
struct EdgeSet {
    /// `(from, label, to)` to the arrow and, for a derived edge, the ids it goes through.
    edges: BTreeMap<EdgeKey, (&'static str, Option<BTreeSet<String>>)>,
    /// The ends of every edge added.
    nodes: HashSet<NodeRef>,
}

impl EdgeSet {
    /// Add the edge `from -label-> to` of relation `kind`, derived through `via` when given.
    fn add(
        &mut self,
        model: &SemanticModel,
        (from, kind, label, to): (&NodeRef, &RelKind, String, &NodeRef),
        via: Option<&NodeRef>,
    ) {
        let (Some(from_id), Some(to_id)) = (contract_id(model, from), contract_id(model, to))
        else {
            return;
        };
        let entry = self
            .edges
            .entry((from_id, label, to_id))
            .or_insert_with(|| (rdra_arrow(kind), Some(BTreeSet::new())));
        match (via, &mut entry.1) {
            (None, vias) => *vias = None,
            (Some(via), Some(vias)) => vias.extend(contract_id(model, via)),
            (Some(_), None) => {}
        }
        self.nodes.insert(from.clone());
        self.nodes.insert(to.clone());
    }

    fn into_graph_edges(self) -> Vec<GraphEdge> {
        self.edges
            .into_iter()
            .map(|((from, label, to), (arrow, vias))| GraphEdge {
                from,
                label,
                to,
                arrow,
                derived: vias
                    .map(|vias| format!("via {}", vias.into_iter().collect::<Vec<_>>().join(","))),
            })
            .collect()
    }
}

/// What `--buc` scopes the context, condition, and requirement diagrams to: the BUCs `ids`
/// names, the use cases and steps they contain (as `buc_members` finds them), and the APIs
/// those use cases invoke.
fn buc_scope(model: &SemanticModel, ids: &[String]) -> HashSet<NodeRef> {
    let bucs: Vec<NodeRef> = model
        .bucs
        .iter()
        .filter(|(_, buc)| ids.contains(&buc.id))
        .map(|(key, _)| NodeRef::Buc(key))
        .collect();
    let (usecases, steps) = buc_members(model, &bucs);
    let mut scope: HashSet<NodeRef> = bucs.into_iter().chain(usecases).chain(steps).collect();
    let apis: Vec<NodeRef> = model
        .relations
        .iter()
        .filter(|rel| rel.kind == RelKind::Invokes && scope.contains(&rel.from))
        .map(|rel| rel.to.clone())
        .collect();
    scope.extend(apis);
    scope
}

/// The kinds of the nodes linked to an `anchor` node that the diagram does not draw and whose
/// kind it does not draw, in layer order: the header's `omitted:` line.
fn omitted_linked_kinds(
    model: &SemanticModel,
    anchor: impl Fn(&NodeRef) -> bool,
    drawn: &HashSet<NodeRef>,
    kinds: &[&str],
) -> Vec<String> {
    let linked: HashSet<NodeRef> = model
        .relations
        .iter()
        .flat_map(|rel| {
            let out = anchor(&rel.from).then(|| rel.to.clone());
            let back = anchor(&rel.to).then(|| rel.from.clone());
            out.into_iter().chain(back)
        })
        .filter(|nr| !drawn.contains(nr))
        .collect();
    // No diagram draws an ADR, so `collect_object_graph_nodes` leaves them out.
    let mut nodes = collect_object_graph_nodes(model, &|nr| linked.contains(nr));
    nodes.extend(
        model
            .adrs
            .keys()
            .map(NodeRef::Adr)
            .filter(|nr| linked.contains(nr)),
    );
    kinds_by_layer(&nodes)
        .into_iter()
        .filter(|kind| !kinds.contains(kind))
        .map(str::to_string)
        .collect()
}

/// Sort `edges`, draw each once, and follow a derived one with its `%% derived` line. An
/// edge whose label `diagram` reads back from its ends goes without one.
fn render_edges(out: &mut String, diagram: &str, mut edges: Vec<GraphEdge>) {
    edges.sort();
    edges.dedup();
    for edge in &edges {
        out.push_str(&edge_line(
            diagram,
            &edge.from,
            edge.arrow,
            &edge.label,
            &edge.to,
        ));
        if let Some(derived) = &edge.derived {
            out.push_str(&format!(
                "  %% derived {} -> {} {derived}\n",
                edge.from, edge.to
            ));
        }
    }
}

/// The line style of the RDRAGraph relation kind: hierarchy and flow order `-->`,
/// relate `---`, every other association `-.->`.
fn rdra_arrow(kind: &RelKind) -> &'static str {
    match kind {
        RelKind::Contains
        | RelKind::Belongs
        | RelKind::Owns
        | RelKind::Precedes
        | RelKind::Branches
        | RelKind::Repeats => "-->",
        RelKind::RelateOneToOne
        | RelKind::RelateOneToMany
        | RelKind::RelateManyToOne
        | RelKind::RelateManyToMany => "---",
        _ => "-.->",
    }
}

/// Node kinds of `nodes` in layer order, each once.
fn kinds_by_layer(nodes: &[NodeRef]) -> Vec<&'static str> {
    let mut kinds = Vec::new();
    for layer in OBJECT_GRAPH_LAYERS {
        for node in nodes.iter().filter(|nr| object_graph_layer(nr) == layer) {
            let kind = node.kind_tag();
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
    }
    kinds
}

/// What a `when` edge from `condition` to `usecase` is derived from: `via` and each
/// `transitions` arm (`transitions <Entity>.<column> <from>-><to>`), `chooses` value
/// (`chooses <Entity>.<column> <value>`), and `sets` value (`sets <Entity>.<column> <value>`)
/// of the use case that `.when(condition)` guards, in declaration order, `, `-joined.
fn guard_derivation(model: &SemanticModel, condition: &NodeRef, usecase: &NodeRef) -> String {
    let (NodeRef::Condition(ck), NodeRef::UseCase(uk)) = (condition, usecase) else {
        return String::new();
    };
    let entity = |key| model.entities.get(key).map_or("", |e| e.id.as_str());
    let arms = model
        .state_transitions
        .iter()
        .filter(|t| t.guard == Some(*ck) && t.usecase == *uk)
        .map(|t| {
            format!(
                "transitions {}.{} {}->{}",
                entity(t.entity),
                t.column,
                t.from,
                t.to
            )
        });
    let values = model
        .column_choices
        .iter()
        .filter(|c| c.origin == *uk)
        .flat_map(|c| {
            c.values
                .iter()
                .filter(|v| v.guard == Some(*ck))
                .map(move |v| format!("chooses {}.{} {}", entity(c.entity), c.column, v.value))
        });
    let sets = model
        .column_effects
        .iter()
        .filter(|e| e.origin == *uk && e.guard == Some(*ck))
        .filter_map(|e| match &e.value {
            SetValue::Value(v) => Some(format!(
                "sets {}.{} {}",
                entity(e.entity),
                e.column,
                v.literal()
            )),
            SetValue::Copy(_) => None,
        });
    format!(
        "via {}",
        arms.chain(values)
            .chain(sets)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// The `when` edge of a `Guards` relation: a condition to the use case whose arm or value
/// it guards, followed by the `%% derived` line naming them.
fn guard_edge(model: &SemanticModel, rel: &Relation) -> Option<GraphEdge> {
    Some(GraphEdge {
        from: contract_id(model, &rel.from)?,
        label: object_graph_rel_label(&RelKind::Guards).to_string(),
        to: contract_id(model, &rel.to)?,
        arrow: rdra_arrow(&RelKind::Guards),
        derived: Some(guard_derivation(model, &rel.from, &rel.to)),
    })
}

/// Every relation between two visible nodes, with `transitions` drawn from the use case to
/// the state it enters. Each comes with its kind, so the caller can apply `--edge-kind`.
fn graph_edges(model: &SemanticModel, visible: &HashSet<NodeRef>) -> Vec<(RelKind, GraphEdge)> {
    let mut edges = Vec::new();
    for rel in &model.relations {
        if !visible.contains(&rel.from) || !visible.contains(&rel.to) {
            continue;
        }
        if rel.kind == RelKind::Guards {
            edges.extend(guard_edge(model, rel).map(|edge| (RelKind::Guards, edge)));
            continue;
        }
        let (Some(from), Some(to)) = (contract_id(model, &rel.from), contract_id(model, &rel.to))
        else {
            continue;
        };
        let derived = match rel.from {
            NodeRef::Condition(condition)
                if rel.kind == RelKind::Combines
                    && model
                        .conditions
                        .get(condition)
                        .is_some_and(|c| c.body.is_some()) =>
            {
                Some("from holds".to_string())
            }
            _ => None,
        };
        edges.push((
            rel.kind.clone(),
            GraphEdge {
                from,
                label: object_graph_rel_label(&rel.kind).to_string(),
                to,
                arrow: rdra_arrow(&rel.kind),
                derived,
            },
        ));
    }

    for transition in &model.state_transitions {
        let Some(entity) = model.entities.get(transition.entity) else {
            continue;
        };
        let Some(state) = state_key(model, transition.entity, &transition.column, &transition.to)
        else {
            continue;
        };
        let (usecase, state) = (NodeRef::UseCase(transition.usecase), NodeRef::State(state));
        if !visible.contains(&usecase) || !visible.contains(&state) {
            continue;
        }
        let (Some(from), Some(to)) = (contract_id(model, &usecase), contract_id(model, &state))
        else {
            continue;
        };
        edges.push((
            RelKind::Transitions,
            GraphEdge {
                from,
                label: format!(
                    "transitions {}.{} {}->{}",
                    entity.id, transition.column, transition.from, transition.to
                ),
                to,
                arrow: "-.->",
                derived: None,
            },
        ));
    }
    edges
}

#[cfg(test)]
mod tests;

/// The named entities and every entity one `relate` away from one of them: what `--entity`
/// draws of the data model. `None` when no entity is named.
pub(crate) fn entity_neighbourhood(
    model: &SemanticModel,
    ids: &[String],
) -> Option<HashSet<NodeRef>> {
    if ids.is_empty() {
        return None;
    }
    let mut set: HashSet<NodeRef> = model
        .entities
        .iter()
        .filter(|(_, e)| ids.contains(&e.id))
        .map(|(key, _)| NodeRef::Entity(key))
        .collect();
    let named = set.clone();
    for (_, fk) in model.foreign_keys() {
        let (child, parent) = (NodeRef::Entity(fk.child), NodeRef::Entity(fk.parent));
        if named.contains(&child) {
            set.insert(parent.clone());
        }
        if named.contains(&parent) {
            set.insert(child);
        }
    }
    // An N:M joins its two ends through the junction, which a key reaches; the ends reach
    // each other by the relate itself.
    for rel in &model.relations {
        if is_relate(&rel.kind) {
            if named.contains(&rel.from) {
                set.insert(rel.to.clone());
            }
            if named.contains(&rel.to) {
                set.insert(rel.from.clone());
            }
        }
    }
    Some(set)
}
