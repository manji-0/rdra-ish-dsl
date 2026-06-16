use crate::plantuml::{node_id, node_label};
use crate::{
    collect_object_graph_nodes, node_kind_filter_name, object_graph_rel_label, view_node_visible,
    view_relation_visible, EmitError, Scope, View,
};
use rdra_ish_core::model::{NodeRef, SemanticModel};
use std::collections::{BTreeMap, BTreeSet, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NodeKey {
    kind: String,
    id: String,
}

#[derive(Debug, Clone)]
struct NodeInfo {
    key: NodeKey,
    label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct EdgeKey {
    from: NodeKey,
    label: String,
    to: NodeKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffStatus {
    Added,
    Removed,
    Changed,
    Context,
}

#[derive(Debug, Clone)]
struct DiffNode {
    info: NodeInfo,
    status: DiffStatus,
}

#[derive(Debug, Clone)]
struct DiffEdge {
    key: EdgeKey,
    status: DiffStatus,
}

#[derive(Debug)]
struct GraphSnapshot {
    nodes: BTreeMap<NodeKey, NodeInfo>,
    edges: BTreeSet<EdgeKey>,
}

#[derive(Debug)]
struct GraphDiff {
    nodes: Vec<DiffNode>,
    edges: Vec<DiffEdge>,
}

pub struct DiffMermaidEmitter<'a> {
    pub base: &'a SemanticModel,
}

pub struct DiffPlantUmlEmitter<'a> {
    pub base: &'a SemanticModel,
}

impl DiffMermaidEmitter<'_> {
    pub fn emit_diff(&self, target: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let diff = graph_diff(self.base, target, view);
        Ok(mermaid_document(&diff))
    }
}

impl DiffPlantUmlEmitter<'_> {
    pub fn emit_diff(&self, target: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let diff = graph_diff(self.base, target, view);
        Ok(plantuml_document(&diff))
    }
}

fn graph_diff(base: &SemanticModel, target: &SemanticModel, view: &View) -> GraphDiff {
    let base_graph = snapshot(base, view);
    let target_graph = snapshot(target, view);

    let base_keys: BTreeSet<_> = base_graph.nodes.keys().cloned().collect();
    let target_keys: BTreeSet<_> = target_graph.nodes.keys().cloned().collect();
    let mut node_keys: BTreeSet<NodeKey> = BTreeSet::new();

    let mut edges = Vec::new();
    for edge in target_graph.edges.difference(&base_graph.edges) {
        node_keys.insert(edge.from.clone());
        node_keys.insert(edge.to.clone());
        edges.push(DiffEdge {
            key: edge.clone(),
            status: DiffStatus::Added,
        });
    }
    for edge in base_graph.edges.difference(&target_graph.edges) {
        node_keys.insert(edge.from.clone());
        node_keys.insert(edge.to.clone());
        edges.push(DiffEdge {
            key: edge.clone(),
            status: DiffStatus::Removed,
        });
    }

    for key in target_keys.difference(&base_keys) {
        node_keys.insert(key.clone());
    }
    for key in base_keys.difference(&target_keys) {
        node_keys.insert(key.clone());
    }
    for key in base_keys.intersection(&target_keys) {
        let base_label = base_graph
            .nodes
            .get(key)
            .map(|node| node.label.as_str())
            .unwrap_or_default();
        let target_label = target_graph
            .nodes
            .get(key)
            .map(|node| node.label.as_str())
            .unwrap_or_default();
        if base_label != target_label {
            node_keys.insert(key.clone());
        }
    }

    let mut nodes = Vec::new();
    for key in node_keys {
        let status = if !base_graph.nodes.contains_key(&key) {
            DiffStatus::Added
        } else if !target_graph.nodes.contains_key(&key) {
            DiffStatus::Removed
        } else if base_graph.nodes[&key].label != target_graph.nodes[&key].label {
            DiffStatus::Changed
        } else {
            DiffStatus::Context
        };
        let info = target_graph
            .nodes
            .get(&key)
            .or_else(|| base_graph.nodes.get(&key))
            .cloned()
            .unwrap_or_else(|| NodeInfo {
                key: key.clone(),
                label: key.id.clone(),
            });
        nodes.push(DiffNode { info, status });
    }

    edges.sort_by(|left, right| {
        edge_sort_key(&left.key)
            .cmp(&edge_sort_key(&right.key))
            .then_with(|| status_rank(left.status).cmp(&status_rank(right.status)))
    });

    GraphDiff { nodes, edges }
}

fn snapshot(model: &SemanticModel, view: &View) -> GraphSnapshot {
    let reachable: Option<HashSet<NodeRef>> = match &view.scope {
        Scope::Bucs(buc_ids) => Some(rdra_ish_core::reachable_from_bucs(model, buc_ids)),
        Scope::Whole | Scope::UseCases(_) => None,
    };
    let is_visible = |node: &NodeRef| {
        reachable
            .as_ref()
            .is_none_or(|reachable| reachable.contains(node))
            && view_node_visible(view, node)
    };

    let mut nodes = BTreeMap::new();
    for node in collect_object_graph_nodes(model, &is_visible) {
        if let Some(info) = node_info(model, &node) {
            nodes.insert(info.key.clone(), info);
        }
    }

    let mut edges = BTreeSet::new();
    for relation in &model.relations {
        if !view_relation_visible(view, &relation.kind) {
            continue;
        }
        let Some(from) = node_info(model, &relation.from) else {
            continue;
        };
        let Some(to) = node_info(model, &relation.to) else {
            continue;
        };
        if nodes.contains_key(&from.key) && nodes.contains_key(&to.key) {
            edges.insert(EdgeKey {
                from: from.key,
                label: object_graph_rel_label(&relation.kind).to_string(),
                to: to.key,
            });
        }
    }

    GraphSnapshot { nodes, edges }
}

fn node_info(model: &SemanticModel, node: &NodeRef) -> Option<NodeInfo> {
    Some(NodeInfo {
        key: NodeKey {
            kind: node_kind_filter_name(node).to_string(),
            id: node_id(model, node)?.to_string(),
        },
        label: node_label(model, node)?.to_string(),
    })
}

fn mermaid_document(diff: &GraphDiff) -> String {
    let mut out = String::from("flowchart LR\n");
    if diff.nodes.is_empty() && diff.edges.is_empty() {
        out.push_str("  no_changes[\"No model graph differences\"]\n");
    }
    for node in &diff.nodes {
        out.push_str(&format!(
            "  {}[\"{}\"]:::{}\n",
            mermaid_node_id(&node.info.key),
            mermaid_label(&node_label_text(&node.info)),
            status_class(node.status)
        ));
    }
    for edge in &diff.edges {
        let marker = match edge.status {
            DiffStatus::Added => "+",
            DiffStatus::Removed => "-",
            DiffStatus::Changed | DiffStatus::Context => "~",
        };
        out.push_str(&format!(
            "  {} -.->|{} {}| {}\n",
            mermaid_node_id(&edge.key.from),
            marker,
            edge.key.label,
            mermaid_node_id(&edge.key.to)
        ));
    }
    out.push_str("  classDef added fill:#dcfce7,stroke:#15803d,color:#14532d\n");
    out.push_str("  classDef removed fill:#fee2e2,stroke:#b91c1c,color:#7f1d1d\n");
    out.push_str("  classDef changed fill:#fef3c7,stroke:#b45309,color:#78350f\n");
    out.push_str("  classDef context fill:#f8fafc,stroke:#64748b,color:#334155\n");
    out
}

fn plantuml_document(diff: &GraphDiff) -> String {
    let mut out = String::from("@startuml\n");
    out.push_str("skinparam shadowing false\n");
    out.push_str("skinparam defaultTextAlignment center\n");
    if diff.nodes.is_empty() && diff.edges.is_empty() {
        out.push_str("rectangle \"No model graph differences\" as no_changes #F8FAFC\n");
    }
    for node in &diff.nodes {
        out.push_str(&format!(
            "rectangle \"{}\" as {} {}\n",
            plantuml_label(&node_label_text(&node.info)),
            plantuml_node_id(&node.info.key),
            plantuml_color(node.status)
        ));
    }
    for edge in &diff.edges {
        let marker = match edge.status {
            DiffStatus::Added => "+",
            DiffStatus::Removed => "-",
            DiffStatus::Changed | DiffStatus::Context => "~",
        };
        out.push_str(&format!(
            "{} ..> {} : {} {}\n",
            plantuml_node_id(&edge.key.from),
            plantuml_node_id(&edge.key.to),
            marker,
            edge.key.label
        ));
    }
    out.push_str("legend right\n");
    out.push_str("|<#dcfce7> added |<#fee2e2> removed |<#fef3c7> changed label |\n");
    out.push_str("endlegend\n");
    out.push_str("@enduml\n");
    out
}

fn node_label_text(info: &NodeInfo) -> String {
    if info.label == info.key.id {
        format!("{}::{}", info.key.kind, info.key.id)
    } else {
        format!("{}::{}\n{}", info.key.kind, info.key.id, info.label)
    }
}

fn status_class(status: DiffStatus) -> &'static str {
    match status {
        DiffStatus::Added => "added",
        DiffStatus::Removed => "removed",
        DiffStatus::Changed => "changed",
        DiffStatus::Context => "context",
    }
}

fn plantuml_color(status: DiffStatus) -> &'static str {
    match status {
        DiffStatus::Added => "#DCFCE7",
        DiffStatus::Removed => "#FEE2E2",
        DiffStatus::Changed => "#FEF3C7",
        DiffStatus::Context => "#F8FAFC",
    }
}

fn status_rank(status: DiffStatus) -> u8 {
    match status {
        DiffStatus::Added => 0,
        DiffStatus::Removed => 1,
        DiffStatus::Changed => 2,
        DiffStatus::Context => 3,
    }
}

fn edge_sort_key(edge: &EdgeKey) -> String {
    format!(
        "{}::{}:{}:{}::{}",
        edge.from.kind, edge.from.id, edge.label, edge.to.kind, edge.to.id
    )
}

fn mermaid_node_id(key: &NodeKey) -> String {
    format!("n_{}", stable_ident(&format!("{}_{}", key.kind, key.id)))
}

fn plantuml_node_id(key: &NodeKey) -> String {
    format!("N_{}", stable_ident(&format!("{}_{}", key.kind, key.id)))
}

fn stable_ident(value: &str) -> String {
    let mut ident = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            ident.push(ch);
        } else {
            ident.push('_');
        }
    }
    if ident.is_empty() {
        "node".to_string()
    } else {
        ident
    }
}

fn mermaid_label(value: &str) -> String {
    value
        .replace('"', "#quot;")
        .replace('\n', "<br/>")
        .replace('\\', " ")
}

fn plantuml_label(value: &str) -> String {
    value.replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|diag| !diag.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        model
    }

    #[test]
    fn emits_mermaid_diff_for_added_removed_and_changed_graph_items() {
        let base = model_from(
            r#"
actor Customer "Customer"
usecase Browse "Browse catalog"
entity Product "Product" { id: Int @pk }
performs(Customer, Browse)
reads(Browse, Product)
"#,
        );
        let target = model_from(
            r#"
actor Customer "Customer"
usecase Browse "Browse products"
entity Product "Product" { id: Int @pk }
entity Cart "Cart" { id: Int @pk }
performs(Customer, Browse)
writes(Browse, Cart)
"#,
        );

        let out = DiffMermaidEmitter { base: &base }
            .emit_diff(&target, &View::whole())
            .unwrap();

        assert!(out.contains("usecase::Browse<br/>Browse products"));
        assert!(out.contains(":::changed"));
        assert!(out.contains("entity::Cart"));
        assert!(out.contains("|+ writes|"));
        assert!(out.contains("|- reads|"));
    }

    #[test]
    fn emits_no_changes_when_graphs_match() {
        let base = model_from(r#"actor Customer "Customer""#);
        let target = model_from(r#"actor Customer "Customer""#);

        let out = DiffPlantUmlEmitter { base: &base }
            .emit_diff(&target, &View::whole())
            .unwrap();

        assert!(out.contains("No model graph differences"));
    }
}
