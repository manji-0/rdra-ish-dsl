//! The RDRA overview graph (`--kind rdra`).

use super::{graph_edges, kinds_by_layer, render_edges, render_layered_nodes, GraphEdge};
use crate::diagram_common::{class_defs, Diagram, Header, NodeScope};
use crate::{
    collect_object_graph_nodes, object_graph_rel_label, view_node_visible, view_relation_visible,
    EmitError, Emitter, Scope, View,
};
use rdra_ish_core::model::{NodeRef, RelKind, SemanticModel};
use rdra_ish_core::state_key;
use std::collections::HashSet;

pub struct RdraMermaidEmitter;

/// The token `--edge-kind` and the header's `omitted:` line use for a relation kind.
fn edge_kind_token(kind: &RelKind) -> &'static str {
    match kind {
        RelKind::RelateOneToOne
        | RelKind::RelateOneToMany
        | RelKind::RelateManyToOne
        | RelKind::RelateManyToMany => "relate",
        _ => object_graph_rel_label(kind),
    }
}

impl Emitter for RdraMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        // A BUC scope also holds the states its use cases move between.
        let reachable = match &view.scope {
            Scope::Bucs(buc_ids) => {
                let mut set = rdra_ish_core::reachable_from_bucs(model, buc_ids);
                let states: Vec<NodeRef> = model
                    .state_transitions
                    .iter()
                    .filter(|t| set.contains(&NodeRef::UseCase(t.usecase)))
                    .flat_map(|t| {
                        [&t.from, &t.to].map(|v| state_key(model, t.entity, &t.column, v))
                    })
                    .flatten()
                    .map(NodeRef::State)
                    .collect();
                set.extend(states);
                NodeScope::Only(set)
            }
            Scope::Whole | Scope::UseCases(_) => NodeScope::All,
        };
        let in_scope = |nr: &NodeRef| reachable.contains(nr);
        let scoped_nodes = collect_object_graph_nodes(model, &in_scope);
        let visible_nodes: Vec<NodeRef> = scoped_nodes
            .iter()
            .filter(|nr| view_node_visible(view, nr))
            .cloned()
            .collect();
        let visible_set: HashSet<NodeRef> = visible_nodes.iter().cloned().collect();

        let mut body = String::new();
        let layers = render_layered_nodes(&mut body, model, &visible_nodes);

        let mut omitted_edges: Vec<&'static str> = Vec::new();
        let mut edges: Vec<GraphEdge> = Vec::new();
        for (kind, edge) in graph_edges(model, &visible_set) {
            if view_relation_visible(view, &kind) {
                edges.push(edge);
            } else if !omitted_edges.contains(&edge_kind_token(&kind)) {
                omitted_edges.push(edge_kind_token(&kind));
            }
        }
        render_edges(&mut body, Diagram::Rdra, edges);

        let kinds = kinds_by_layer(&visible_nodes);
        omitted_edges.sort();
        let omitted = kinds_by_layer(&scoped_nodes)
            .into_iter()
            .filter(|kind| !kinds.contains(kind))
            .chain(omitted_edges)
            .map(str::to_string)
            .collect();
        body.push_str(&class_defs(&kinds));
        let header = Header {
            diagram: Diagram::Rdra,
            view,
            layers: Some(layers),
            kinds,
            omitted,
        };
        Ok(format!("{}flowchart LR\n{body}", header.render()))
    }
}
