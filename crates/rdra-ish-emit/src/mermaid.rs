//! Mermaid emitters: RDRA全体図、システムコンテキスト、要求モデル、ビジネスコンテキスト、UC複合図、ロバストネス図、情報モデル、バリエーション・条件、業務フロー、画面遷移図、ER図、状態遷移図、イベント図。

mod business;
mod condition;
mod context;
mod flow;
mod information;
mod requirement;
mod robustness;
mod screen;
mod sequence;
mod usecase;

pub use business::BusinessMermaidEmitter;
pub use condition::ConditionMermaidEmitter;
pub use context::ContextMermaidEmitter;
pub use flow::FlowMermaidEmitter;
pub use information::InformationMermaidEmitter;
pub use requirement::RequirementMermaidEmitter;
pub use robustness::RobustnessMermaidEmitter;
pub use screen::ScreenMermaidEmitter;
pub use sequence::SequenceMermaidEmitter;
pub use usecase::UsecaseMermaidEmitter;

use crate::diagram_common::{
    class_defs, col_type_str, contract_id, desc_line, edge_line, lifecycle_start_states, node_id,
    node_label, node_line, quote, render_event_flows, Header,
};
use crate::{
    collect_object_graph_nodes, node_kind_filter_name, object_graph_layer, object_graph_rel_label,
    view_node_visible, view_relation_visible, EmitError, Emitter, Explored, ObjectGraphLayer,
    OnDeadlock, Scope, StatesEmitter, View, OBJECT_GRAPH_LAYERS,
};
use rdra_ish_core::analysis::transition_guard_labels;
use rdra_ish_core::model::{ColumnType, NodeRef, RelKind, Relation, SemanticModel, SetValue};
use rdra_ish_core::{buc_members, state_id, state_key, state_models, StateModel};
use std::collections::{BTreeMap, BTreeSet, HashSet};

// ── RDRA グラフ図エミッタ (Mermaid) ─────────────────────────────────────────

pub struct RdraMermaidEmitter;

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
            .filter_map(|nr| Some((nr, contract_id(model, nr)?, node_label(model, nr)?)))
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
            out.push_str(&node_line(
                "    ",
                &id,
                &quote(label),
                node_kind_filter_name(nr),
            ));
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
    let (Some(id), Some(label)) = (contract_id(model, node), node_label(model, node)) else {
        return;
    };
    let kind = node_kind_filter_name(node);
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

/// Node kinds of `nodes` in layer order, each once.
fn kinds_by_layer(nodes: &[NodeRef]) -> Vec<&'static str> {
    let mut kinds = Vec::new();
    for layer in OBJECT_GRAPH_LAYERS {
        for node in nodes.iter().filter(|nr| object_graph_layer(nr) == layer) {
            let kind = node_kind_filter_name(node);
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

impl Emitter for RdraMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        // A BUC scope also holds the states its use cases move between.
        let reachable: Option<HashSet<NodeRef>> = match &view.scope {
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
                Some(set)
            }
            Scope::Whole | Scope::UseCases(_) => None,
        };
        let in_scope = |nr: &NodeRef| reachable.as_ref().is_none_or(|set| set.contains(nr));
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
        render_edges(&mut body, "rdra", edges);

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
            diagram: "rdra",
            view,
            layers: Some(layers),
            kinds,
            omitted,
        };
        Ok(format!("{}flowchart LR\n{body}", header.render()))
    }
}

// ── 状態遷移図エミッタ (Mermaid) ──────────────────────────────────────────────

pub struct StateMermaidEmitter;

/// Every state model with a transition in scope, sorted by entity and column.
fn state_models_in_scope<'a>(
    model: &'a SemanticModel,
    in_scope: &HashSet<NodeRef>,
) -> Vec<StateModel<'a>> {
    let mut models = state_models(model);
    models.retain(|sm| {
        sm.transitions
            .iter()
            .any(|t| in_scope.contains(&NodeRef::UseCase(t.usecase)))
    });
    models
}

impl StatesEmitter for StateMermaidEmitter {
    fn on_deadlock(&self) -> OnDeadlock {
        OnDeadlock::Draw
    }

    fn emit(
        &self,
        model: &SemanticModel,
        view: &View,
        explored: &Explored,
    ) -> Result<String, EmitError> {
        // A BUC scope draws each state model one of its use cases moves, whole.
        let in_scope: HashSet<NodeRef> = match &view.scope {
            Scope::Bucs(buc_ids) => rdra_ish_core::reachable_from_bucs(model, buc_ids),
            Scope::Whole | Scope::UseCases(_) => model
                .use_cases
                .keys()
                .map(NodeRef::UseCase)
                .chain(model.entities.keys().map(NodeRef::Entity))
                .collect(),
        };
        let mut models = state_models_in_scope(model, &in_scope);
        let entities = &view.state.entities;
        if !entities.is_empty() {
            models.retain(|sm| entities.iter().any(|id| id == sm.entity_id));
        }
        // The whole model is an index; a BUC or an entity scope already narrows it to full models.
        let index = matches!(view.scope, Scope::Whole | Scope::UseCases(_)) && entities.is_empty();
        let marks = explored.marks();
        let starts = lifecycle_start_states(
            model,
            models.iter().map(|sm| (sm.entity, sm.column.to_string())),
            |entity, column, variant| state_key(model, entity, column, variant),
        );

        let mut body = String::new();
        let mut classes = String::new();
        let mut used_classes: Vec<&'static str> = Vec::new();
        let mut kinds: Vec<&'static str> = Vec::new();
        let mut add_kind = |kind: &'static str| {
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        };
        for sm in &models {
            let mark = |set: &BTreeSet<(String, String, String)>, v: &str| {
                set.contains(&(
                    sm.entity_id.to_string(),
                    sm.column.to_string(),
                    v.to_string(),
                ))
            };
            // A value no transition names has no State node; an unreachable one is still drawn.
            let state_id = |variant: &str| {
                state_key(model, sm.entity, sm.column, variant)
                    .and_then(|sk| contract_id(model, &NodeRef::State(sk)))
                    .or_else(|| {
                        mark(&marks.unreachable, variant).then(|| {
                            format!("state__{}", state_id(model, sm.entity, sm.column, variant))
                        })
                    })
            };
            let mut arms: Vec<(&str, &str, String, String)> = sm
                .transitions
                .iter()
                .filter_map(|t| {
                    let usecase = contract_id(model, &NodeRef::UseCase(t.usecase))?;
                    Some((
                        t.from.as_str(),
                        t.to.as_str(),
                        usecase,
                        transition_label(model, t),
                    ))
                })
                .collect();
            arms.sort_by(|a, b| {
                (sm.position(a.0), sm.position(a.1), &a.2).cmp(&(
                    sm.position(b.0),
                    sm.position(b.1),
                    &b.2,
                ))
            });
            arms.dedup();
            let start_variants: Vec<&str> = sm
                .variants
                .iter()
                .copied()
                .filter(|v| {
                    state_key(model, sm.entity, sm.column, v).is_some_and(|sk| starts.contains(&sk))
                })
                .collect();
            let drawn: Vec<&str> = sm
                .variants
                .iter()
                .copied()
                .filter(|v| {
                    start_variants.contains(v)
                        || arms.iter().any(|(from, to, ..)| from == v || to == v)
                        || mark(&marks.unreachable, v)
                })
                .collect();

            add_kind("statemodel");
            body.push_str(&format!(
                "  state \"{}.{}\" as statemodel__{}_{} {{\n",
                sm.entity_id, sm.column, sm.entity_id, sm.column
            ));
            if index {
                add_kind("state");
                body.push_str(&format!(
                    "    state \"{} 状態 / {} 遷移 (--entity {})\" as index__{}_{}\n  }}\n",
                    sm.variants.len(),
                    arms.len(),
                    sm.entity_id,
                    sm.entity_id,
                    sm.column
                ));
                continue;
            }
            for variant in &start_variants {
                if let Some(id) = state_id(variant) {
                    body.push_str(&format!("    [*] --> {id}\n"));
                }
            }
            for variant in &drawn {
                if let Some(id) = state_id(variant) {
                    add_kind("state");
                    let label = state_key(model, sm.entity, sm.column, variant)
                        .map_or(*variant, |key| model.states[key].label.as_str());
                    body.push_str(&format!("    state \"{}\" as {id}\n", quote(label)));
                }
            }
            for (from, to, usecase, label) in &arms {
                if let (Some(from), Some(to)) = (state_id(from), state_id(to)) {
                    add_kind("usecase");
                    body.push_str(&format!("    %% transition {usecase}\n"));
                    body.push_str(&format!("    {from} --> {to} : {label}\n"));
                }
            }
            // An end state has no transition to another state.
            for variant in &drawn {
                let leaves = arms
                    .iter()
                    .any(|(from, to, ..)| from == variant && to != variant);
                let Some(id) = state_id(variant) else {
                    continue;
                };
                let unreachable = mark(&marks.unreachable, variant);
                if !leaves && !unreachable {
                    body.push_str(&format!("    {id} --> [*]\n"));
                }
                let class = if mark(&marks.deadlock, variant) {
                    Some("statedeadlock")
                } else if unreachable {
                    Some("stateunreachable")
                } else if !leaves {
                    Some("stateend")
                } else {
                    None
                };
                if let Some(class) = class {
                    classes.push_str(&format!("  class {id} {class}\n"));
                    if !used_classes.contains(&class) {
                        used_classes.push(class);
                    }
                }
            }
            body.push_str("  }\n");
        }

        // An Enum column no `transitions` drive is no state model. One a `sets` writes is still
        // a state axis of `states` and `check`, so it is named apart from one that never moves.
        let (mut unmodeled, mut set_only): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
        let mut all_entities: Vec<_> = model.entities.iter().collect();
        all_entities.sort_by_key(|(_, e)| &e.id);
        for (key, entity) in all_entities {
            if !in_scope.contains(&NodeRef::Entity(key))
                || (!entities.is_empty() && !entities.contains(&entity.id))
            {
                continue;
            }
            for column in &entity.columns {
                if !matches!(column.col_type, ColumnType::Enum(_))
                    || model.is_lifecycle_column(key, &column.name)
                {
                    continue;
                }
                let name = format!("{}.{}", entity.id, column.name);
                let set = model
                    .column_effects
                    .iter()
                    .any(|e| e.entity == key && e.column == column.name);
                match set {
                    true => set_only.push(name),
                    false => unmodeled.push(name),
                }
            }
        }
        let omitted = if unmodeled.is_empty() {
            Vec::new()
        } else {
            vec![format!(
                "{} (Enum without transitions)",
                unmodeled.join(" ")
            )]
        };
        let header = Header {
            diagram: "state",
            view,
            layers: None,
            kinds,
            omitted,
        };
        let set_only = if set_only.is_empty() {
            String::new()
        } else {
            format!(
                "%% set-only: {} (Enum a `sets` writes and no `transitions` drive: a state axis of check, not a state model)\n",
                set_only.join(" ")
            )
        };
        let legend = if index {
            ""
        } else {
            "%% legend: stateunreachable = no reachable state holds it, statedeadlock = an instance cannot leave it, stateend = no transition out\n"
        };
        let defs: String = [
            (
                "stateunreachable",
                "fill:#fdd,stroke:#c00,stroke-dasharray:4 2,color:#600",
            ),
            ("statedeadlock", "fill:#fd9,stroke:#d60,color:#630"),
            ("stateend", "fill:#dfd,stroke:#383,color:#141"),
        ]
        .iter()
        .filter(|(name, _)| used_classes.contains(name))
        .map(|(name, style)| format!("  classDef {name} {style}\n"))
        .collect();
        Ok(format!(
            "{}{set_only}{legend}stateDiagram-v2\n{body}{defs}{classes}",
            header.render()
        ))
    }
}

/// A transition's label: the use case's label, then `[...]` with the label of each condition
/// that guards it, the use case's and the arm's own `.when` (UML guard notation).
fn transition_label(model: &SemanticModel, t: &rdra_ish_core::StateTransition) -> String {
    let guards = transition_guard_labels(model, t);
    let mut label = quote(&model.use_cases[t.usecase].label);
    let mut together: Vec<String> = model
        .state_transitions
        .iter()
        .filter(|o| o.usecase == t.usecase && o.entity == t.entity && o.column != t.column)
        .map(|o| format!("{}.{}", model.entities[o.entity].id, o.column))
        .collect();
    together.sort();
    together.dedup();
    if !together.is_empty() {
        // One step of the use case moves every column it names, and runs only where each has an arm.
        label.push_str(&format!(" ({} と同時)", together.join(", ")));
    }
    if guards.is_empty() {
        label
    } else {
        format!("{label} [{}]", quote(&guards.join(", ")))
    }
}

// ── ER図エミッタ (Mermaid) ────────────────────────────────────────────────────

pub struct ErMermaidEmitter;

impl Emitter for ErMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        // BUCフィルタ
        let reachable: Option<HashSet<NodeRef>> = match &view.scope {
            Scope::Bucs(buc_ids) => Some(rdra_ish_core::reachable_from_bucs(model, buc_ids)),
            Scope::Whole | Scope::UseCases(_) => None,
        };

        let is_visible = |nr: &NodeRef| -> bool {
            match &reachable {
                Some(set) => set.contains(nr),
                None => true,
            }
        };

        let mut ents: Vec<_> = model.entities.iter().collect();
        ents.sort_by_key(|(_, e)| &e.id);

        let header = Header {
            diagram: "er",
            view,
            layers: None,
            kinds: if ents.iter().any(|(k, _)| is_visible(&NodeRef::Entity(*k))) {
                vec!["entity"]
            } else {
                Vec::new()
            },
            omitted: Vec::new(),
        };
        let mut out = header.render();
        out.push_str("erDiagram\n");

        for (k, ent) in &ents {
            if !is_visible(&NodeRef::Entity(*k)) {
                continue;
            }
            out.push_str(&format!("  {} {{\n", ent.id));

            for col in &ent.columns {
                let type_str = col_type_str(model, &col.col_type);
                if col.is_pk {
                    out.push_str(&format!("    {} {} PK\n", type_str, col.name));
                } else if model.column_fk(*k, &col.name).is_some() {
                    out.push_str(&format!("    {} {} FK\n", type_str, col.name));
                } else {
                    out.push_str(&format!("    {} {}\n", type_str, col.name));
                }
            }

            out.push_str("  }\n");
        }

        // ER relations
        let mut er_rels: Vec<_> = model
            .relations
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                matches!(
                    r.kind,
                    RelKind::RelateOneToOne
                        | RelKind::RelateOneToMany
                        | RelKind::RelateManyToOne
                        | RelKind::RelateManyToMany
                )
            })
            .collect();
        er_rels.sort_by_key(|(_, r)| format!("{:?}{:?}", r.from, r.to));

        for &(index, rel) in &er_rels {
            if !is_visible(&rel.from) || !is_visible(&rel.to) {
                continue;
            }
            if let (Some(from_id), Some(to_id)) =
                (node_id(model, &rel.from), node_id(model, &rel.to))
            {
                let Some(glyph) = crate::er_glyph(model, index) else {
                    continue;
                };
                out.push_str(&format!("  {from_id} {glyph} {to_id} : \"\"\n"));
            }
        }

        Ok(out)
    }
}

// ── イベント図エミッタ (Mermaid) ─────────────────────────────────────────────

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
            diagram: "event",
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

#[cfg(test)]
mod tests;
