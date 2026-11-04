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
mod tests {
    use super::*;
    use crate::diagram_common::{
        declare_flow_node, model_from, render_event_flow, Declared, FlowNode,
    };
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    /// A view that draws every state model whole, as `--entity` of each entity does.
    fn all_entities(model: &SemanticModel) -> View {
        let mut view = View::whole();
        view.state.entities = model.entities.values().map(|e| e.id.clone()).collect();
        view
    }

    #[test]
    fn test_rdra_mermaid_layers() {
        let src = r#"
actor Customer "顧客"
buc BucOrder "注文業務"
usecase PlaceOrder "注文する"
screen OrderScreen "注文画面"
api OrderApi "注文API"
entity Order "注文" { id: Int @pk }
performs(Customer, BucOrder)
contains(BucOrder, PlaceOrder)
displays(PlaceOrder, OrderScreen)
invokes(PlaceOrder, OrderApi)
creates(OrderApi, Order)
"#;
        let model = model_from(src);
        let result = RdraMermaidEmitter
            .emit(&model, &View::whole().with_model_path("m.rdra"))
            .unwrap();
        assert!(
            result.starts_with(
                "%% rdra-ish diagram=rdra scope=whole model=m.rdra\n\
                 %% layers: value=システム価値 environment=システム外部環境 boundary=システム境界 system=システム\n\
                 %% kinds: actor buc usecase screen api entity\n\
                 %% omitted: (none)\n\
                 flowchart LR\n"
            ),
            "{result}"
        );
        let value_pos = result
            .find("  subgraph layer_value[\"システム価値\"]\n")
            .unwrap();
        let boundary_pos = result
            .find("  subgraph layer_boundary[\"システム境界\"]\n")
            .unwrap();
        let system_pos = result
            .find("  subgraph layer_system[\"システム\"]\n")
            .unwrap();
        let actor_pos = result
            .find("    actor__Customer([\"顧客\"]):::actor\n")
            .unwrap();
        let screen_pos = result
            .find("    screen__OrderScreen[\"注文画面\"]:::screen\n")
            .unwrap();
        let api_pos = result
            .find("    api__OrderApi{{\"注文API\"}}:::api\n")
            .unwrap();
        assert!(value_pos < actor_pos && actor_pos < boundary_pos);
        assert!(boundary_pos < screen_pos && screen_pos < system_pos);
        assert!(system_pos < api_pos);
        assert!(result.contains("    buc__BucOrder[\"注文業務\"]:::buc\n"));
        assert!(result.contains("    entity__Order[(\"注文\")]:::entity\n"));
        assert!(result.contains("  actor__Customer -.-> buc__BucOrder\n"));
        assert!(result.contains("  buc__BucOrder --> usecase__PlaceOrder\n"));
        assert!(result.contains("  usecase__PlaceOrder -.-> api__OrderApi\n"));
        assert!(result.contains("  api__OrderApi -.->|creates| entity__Order\n"));
        assert!(result.ends_with("  classDef api fill:#c5f6fa,stroke:#0b7285,color:#000;\n  classDef entity fill:#fff9db,stroke:#5c940d,color:#000;\n"));
        assert!(!result.contains("direction TB"), "{result}");
    }

    #[test]
    fn test_rdra_mermaid_applies_node_and_edge_filters() {
        let src = r#"
actor Customer "Customer"
usecase PlaceOrder "Place order"
api OrderApi "Order API"
entity Order "Order" { id: Int @pk }
performs(Customer, PlaceOrder)
invokes(PlaceOrder, OrderApi)
creates(OrderApi, Order)
displays(PlaceOrder, Order)
"#;
        let model = model_from(src);
        let view = View::whole().with_graph_filters(
            vec![
                "usecase".to_string(),
                "api".to_string(),
                "entity".to_string(),
            ],
            vec!["invokes".to_string(), "creates".to_string()],
        );

        let result = RdraMermaidEmitter.emit(&model, &view).unwrap();

        assert!(result.contains("usecase__PlaceOrder(\"Place order\"):::usecase"));
        assert!(result.contains("api__OrderApi{{\"Order API\"}}:::api"));
        assert!(result.contains("usecase__PlaceOrder -.-> api__OrderApi"));
        assert!(!result.contains("actor__Customer"), "{result}");
        assert!(!result.contains("performs"), "{result}");
        assert!(
            result.contains("%% kinds: usecase api entity\n"),
            "{result}"
        );
        // The header names what the filters hid: the actor, and the displays edge whose
        // ends are both drawn.
        assert!(result.contains("%% omitted: actor displays\n"), "{result}");
    }

    #[test]
    fn test_rdra_mermaid_draws_one_edge_per_fact() {
        let src = r#"
screen StoreScreen "Store"
entity Store "Store" { id: Int @pk }
shows(StoreScreen, Store)
shows(StoreScreen, Store)
"#;
        let model = model_from(src);
        let result = RdraMermaidEmitter.emit(&model, &View::whole()).unwrap();
        let shows = result
            .lines()
            .filter(|line| line.contains("screen__StoreScreen -.-> entity__Store"))
            .collect::<Vec<_>>();
        assert_eq!(
            shows,
            vec!["  screen__StoreScreen -.-> entity__Store"],
            "{result}"
        );
    }

    #[test]
    fn test_rdra_mermaid_draws_a_transition_from_its_use_case() {
        let src = r#"
buc BucPay "Pay"
usecase Capture "Capture"
entity Order "Order" {
  id: Int @pk
  status: Enum(awaiting, paid) @default(awaiting)
}
contains(BucPay, Capture)
transitions(Capture, Order.status, awaiting -> paid)
"#;
        let model = model_from(src);
        for view in [View::whole(), View::bucs(vec!["BucPay".to_string()])] {
            let result = RdraMermaidEmitter.emit(&model, &view).unwrap();
            assert!(
                result.contains(
                    "  usecase__Capture -.->|transitions Order.status awaiting->paid| state__Order_paid\n"
                ),
                "{result}"
            );
            assert!(result.contains("    state__Order_awaiting(((\"awaiting\"))):::state\n"));
        }
        let view = View::whole().with_graph_filters(Vec::new(), vec!["contains".to_string()]);
        let result = RdraMermaidEmitter.emit(&model, &view).unwrap();
        assert!(!result.contains("|transitions"), "{result}");
        assert!(result.contains("%% omitted: transitions\n"), "{result}");
    }

    #[test]
    fn test_rdra_mermaid_writes_descriptions_as_comments() {
        let src = r#"
actor Customer "Customer" description "Places orders"
usecase Browse "Browse" description "Finds products"
performs(Customer, Browse)
"#;
        let model = model_from(src);
        let result = RdraMermaidEmitter.emit(&model, &View::whole()).unwrap();

        assert!(
            result.contains(
                "    actor__Customer([\"Customer\"]):::actor\n    %% desc actor__Customer: Places orders\n"
            ),
            "{result}"
        );
        assert!(result.contains("    %% desc usecase__Browse: Finds products\n"));
        assert!(!result.contains("_description"), "{result}");
    }
    #[test]
    fn test_er_mermaid_emit() {
        let src = r#"
entity Order "注文" { id: Int @pk  total: Money }
entity Customer "顧客" { id: Int @pk  name: String }
relate(Order, Customer, N:1)
"#;
        let model = model_from(src);
        let result = ErMermaidEmitter.emit(&model, &View::er()).unwrap();
        assert!(result.contains("erDiagram"));
        assert!(result.contains("Order {"));
        assert!(result.contains("Int id PK"));
        assert!(result.contains("Customer {"));
        assert!(result.contains("Int id PK"));
        assert!(result.contains("}}o--||") || result.contains("}o--||"));
    }

    #[test]
    fn test_er_mermaid_opens_with_the_header() {
        let model = model_from("entity Order \"O\" { id: Int @pk }\n");
        let view = View::er().with_model_path("m.rdra");
        let result = ErMermaidEmitter.emit(&model, &view).unwrap();
        assert!(
            result.starts_with(
                "%% rdra-ish diagram=er scope=whole model=m.rdra\n\
                 %% kinds: entity\n\
                 %% omitted: (none)\n\
                 erDiagram\n  Order {\n"
            ),
            "{result}"
        );
        let empty = ErMermaidEmitter
            .emit(&model_from("actor A \"A\"\n"), &View::er())
            .unwrap();
        assert_eq!(
            empty,
            "%% rdra-ish diagram=er scope=whole\n%% kinds: (none)\n%% omitted: (none)\nerDiagram\n"
        );
    }

    #[test]
    fn test_state_mermaid_names_an_enum_a_sets_writes_apart_from_omitted() {
        let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, published) @default(draft)
  kind: Enum(memo, report) @default(memo)
  tone: Enum(plain, formal)
}
usecase Publish "公開する"
transitions(Publish, Doc.status, draft -> published)
sets(Publish, Doc.kind == report)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter
            .emit(&model, &View::whole(), &Explored::default())
            .unwrap();
        assert!(
            result.contains(
                "%% omitted: Doc.tone (Enum without transitions)\n\
                 %% set-only: Doc.kind (Enum a `sets` writes and no `transitions` drive: a state axis of check, not a state model)\n"
            ),
            "{result}"
        );
    }

    #[test]
    fn an_entity_scoped_state_diagram_names_only_that_entitys_unmodeled_columns() {
        let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, published) @default(draft)
  tone: Enum(plain, formal)
}
entity Memo "メモ" {
  id: Int @pk
  kind: Enum(note, todo) @default(note)
  mood: Enum(calm, tense)
}
usecase Publish "公開する"
usecase Jot "書く"
transitions(Publish, Doc.status, draft -> published)
sets(Jot, Memo.kind == todo)
"#;
        let model = model_from(src);
        let mut view = View::whole();
        view.state.entities = vec!["Doc".to_string()];
        let result = StateMermaidEmitter
            .emit(&model, &view, &Explored::default())
            .unwrap();
        assert!(result.contains("%% omitted: Doc.tone "), "{result}");
        assert!(!result.contains("Memo."), "{result}");
        assert!(!result.contains("set-only"), "{result}");
    }

    #[test]
    fn test_er_mermaid_snapshot() {
        let src = r#"
entity Customer "顧客" { id: Int @pk  name: String }
entity Order "注文" { id: Int @pk  total: Money }
relate(Order, Customer, N:1)
"#;
        let (ast, _) = parse(src);
        let (model, _) = build_model(&ast);
        let result = ErMermaidEmitter.emit(&model, &View::er()).unwrap();
        insta::assert_snapshot!(result);
    }

    #[test]
    fn a_child_side_minimum_draws_one_or_more_at_the_child_end() {
        let src = "entity Order \"O\" { id: Int @pk }\nentity Line \"L\" { id: Int @pk }\nentity Tag \"T\" { id: Int @pk }\nrelate(Line, Order, 1..*:1)\nrelate(Order, Tag, 1:1..*)\n";
        let (ast, _) = parse(src);
        let (model, _) = build_model(&ast);
        let mermaid = ErMermaidEmitter.emit(&model, &View::er()).unwrap();
        assert!(
            mermaid.contains("  Line }|--|| Order : \"\"\n"),
            "{mermaid}"
        );
        assert!(mermaid.contains("  Order ||--|{ Tag : \"\"\n"), "{mermaid}");
    }

    #[test]
    fn test_state_mermaid_emit() {
        let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, published) @default(draft)
  kind: Enum(memo, report)
}
usecase Publish "公開する"
transitions(Publish, Doc.status, draft -> published)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter
            .emit(
                &model,
                &all_entities(&model).with_model_path("m.rdra"),
                &Explored::default(),
            )
            .unwrap();
        assert_eq!(
            result,
            "%% rdra-ish diagram=state scope=entity:Doc model=m.rdra\n\
             %% kinds: statemodel state usecase\n\
             %% omitted: Doc.kind (Enum without transitions)\n\
             %% legend: stateunreachable = no reachable state holds it, statedeadlock = an instance cannot leave it, stateend = no transition out\n\
             stateDiagram-v2\n  \
               state \"Doc.status\" as statemodel__Doc_status {\n    \
                 [*] --> state__Doc_draft\n    \
                 state \"draft\" as state__Doc_draft\n    \
                 state \"published\" as state__Doc_published\n    \
                 %% transition usecase__Publish\n    \
                 state__Doc_draft --> state__Doc_published : 公開する\n    \
                 state__Doc_published --> [*]\n  \
               }\n  \
               classDef stateend fill:#dfd,stroke:#383,color:#141\n  \
               class state__Doc_published stateend\n"
        );
    }

    #[test]
    fn test_state_mermaid_draws_the_label_of_a_value() {
        let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft "下書き", published) @default(draft)
}
usecase Publish "公開する"
transitions(Publish, Doc.status, draft -> published)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter
            .emit(&model, &all_entities(&model), &Explored::default())
            .unwrap();
        assert!(
            result.contains("    state \"下書き\" as state__Doc_draft\n"),
            "{result}"
        );
        assert!(
            result.contains("    state \"published\" as state__Doc_published\n"),
            "{result}"
        );
    }

    #[test]
    fn test_state_mermaid_names_the_conditions_that_guard_a_transition() {
        let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, published) @default(draft)
  reviewed: Bool @default(false)
}
usecase Publish "公開する"
transitions(Publish, Doc.status, draft -> published)
condition Reviewed "レビュー済み"
holds(Reviewed, Doc.reviewed == true)
applies_to(Reviewed, Publish)
condition Traced "追跡のみ"
applies_to(Traced, Publish)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter
            .emit(&model, &all_entities(&model), &Explored::default())
            .unwrap();
        assert!(
            result.contains(
                "    %% transition usecase__Publish\n    state__Doc_draft --> state__Doc_published : 公開する [レビュー済み]\n"
            ),
            "a condition without a body guards nothing: {result}"
        );
    }

    /// A use case that moves two columns is one step; each column's edge says so.
    #[test]
    fn test_state_mermaid_names_the_other_column_a_use_case_moves() {
        let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, published) @default(draft)
  period: Enum(open, closed) @default(open)
}
usecase Publish "公開する"
usecase Reopen "開き直す"
transitions(Publish, Doc.status, draft -> published)
transitions(Publish, Doc.period, open -> closed)
transitions(Reopen, Doc.period, closed -> open)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter
            .emit(&model, &all_entities(&model), &Explored::default())
            .unwrap();
        assert!(
            result.contains(
                "state__Doc_status_draft --> state__Doc_status_published : 公開する (Doc.period と同時)\n"
            ),
            "{result}"
        );
        assert!(
            result
                .contains("state__Doc_period_open --> state__Doc_period_closed : 公開する (Doc.status と同時)\n"),
            "{result}"
        );
        assert!(
            result.contains("state__Doc_period_closed --> state__Doc_period_open : 開き直す\n"),
            "{result}"
        );
    }

    /// An arm's own `.when(Cond)` joins the use case's guards on that arm only.
    #[test]
    fn test_state_mermaid_names_the_guard_of_one_arm() {
        let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, published, dropped) @default(draft)
  reviewed: Bool @default(false)
}
usecase Decide "判断する"
condition Reviewed "レビュー済み"
holds(Reviewed, Doc.reviewed == true)
transitions(Decide, Doc.status, draft -> published).when(Reviewed)
transitions(Decide, Doc.status, draft -> dropped)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter
            .emit(&model, &all_entities(&model), &Explored::default())
            .unwrap();
        assert!(
            result
                .contains("state__Doc_draft --> state__Doc_published : 判断する [レビュー済み]\n"),
            "{result}"
        );
        assert!(
            result.contains("state__Doc_draft --> state__Doc_dropped : 判断する\n"),
            "{result}"
        );
    }

    #[test]
    fn test_state_mermaid_draws_one_composite_state_per_state_model() {
        let src = r#"
entity Order "Order" {
  id: Int @pk
  status: Enum(open, paid, closed) @default(open)
}
entity Ticket "Ticket" {
  id: Int @pk
  phase: Enum(new, done) @default(new)
}
usecase Pay "Pay"
usecase Close "Close"
usecase Reopen "Reopen"
usecase Finish "Finish"
transitions(Pay, Order.status, open -> paid)
transitions(Close, Order.status, paid -> closed)
transitions(Reopen, Order.status, closed -> open)
transitions(Finish, Ticket.phase, new -> done)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter
            .emit(&model, &all_entities(&model), &Explored::default())
            .unwrap();
        assert_eq!(result.matches("as statemodel__").count(), 2, "{result}");
        let order = result
            .split("state \"Order.status\" as statemodel__Order_status {\n")
            .nth(1)
            .and_then(|rest| rest.split("  }\n").next())
            .unwrap();
        assert!(order.contains(
            "%% transition usecase__Reopen\n    state__Order_closed --> state__Order_open : Reopen"
        ));
        assert!(!order.contains("Ticket"), "{order}");
        // Every Order state leaves, so none is an end state; Ticket ends at done.
        assert!(!order.contains("--> [*]"), "{result}");
        assert!(
            result.contains("    state__Ticket_done --> [*]\n"),
            "{result}"
        );
        assert!(!result.contains("state__Ticket_new --> [*]"), "{result}");
    }

    /// Two lifecycle columns of one entity are two state models, and a value both hold is a
    /// state of each, with the label its own column gives it.
    #[test]
    fn test_state_mermaid_keeps_a_value_two_lifecycle_columns_share_apart() {
        let src = r#"
entity D "D" {
  id: Int @pk
  status: Enum(open, done) @default(open)
  review: Enum(none "未審査", open "審査中") @default(none)
}
usecase Close "Close"
usecase Review "Review"
transitions(Close, D.status, open -> done)
transitions(Review, D.review, none -> open)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter
            .emit(&model, &all_entities(&model), &Explored::default())
            .unwrap();
        let block = |column: &str| {
            result
                .split(&format!("as statemodel__D_{column} {{\n"))
                .nth(1)
                .and_then(|rest| rest.split("  }\n").next())
                .unwrap()
                .to_string()
        };
        let (status, review) = (block("status"), block("review"));
        assert!(
            status.contains("    [*] --> state__D_status_open\n"),
            "{result}"
        );
        assert!(
            status.contains("state \"open\" as state__D_status_open\n"),
            "{result}"
        );
        assert!(
            review.contains("    [*] --> state__D_review_none\n"),
            "{result}"
        );
        assert!(
            review.contains("state \"審査中\" as state__D_review_open\n"),
            "{result}"
        );
        assert!(!review.contains("state__D_status"), "{result}");
    }

    #[test]
    fn test_state_mermaid_buc_scope_draws_the_state_models_its_use_cases_move() {
        let src = r#"
buc BucPay "Pay"
buc BucShip "Ship"
usecase Pay "Pay"
usecase Ship "Ship"
entity Order "Order" {
  id: Int @pk
  status: Enum(open, paid, shipped) @default(open)
}
entity Parcel "Parcel" {
  id: Int @pk
  status: Enum(packed, sent) @default(packed)
}
contains(BucPay, Pay)
contains(BucShip, Ship)
transitions(Pay, Order.status, open -> paid)
transitions(Ship, Order.status, paid -> shipped)
transitions(Ship, Parcel.status, packed -> sent)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter
            .emit(
                &model,
                &View::bucs(vec!["BucPay".to_string()]),
                &Explored::default(),
            )
            .unwrap();
        assert!(result.contains("scope=buc:BucPay"), "{result}");
        assert!(result.contains("statemodel__Order_status {"), "{result}");
        assert!(
            result.contains("state__Order_paid --> state__Order_shipped : Ship"),
            "{result}"
        );
        assert!(!result.contains("Parcel"), "{result}");
    }

    #[test]
    fn test_state_start_is_the_creation_status_not_a_graph_source() {
        let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, review, published) @default(draft)
}
usecase Submit "提出"
usecase Publish "公開"
usecase Reopen "戻す"
creates(Submit, Doc)
sets(Submit, Doc.status == review)
transitions(Publish, Doc.status, review -> published)
transitions(Reopen, Doc.status, published -> review)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter
            .emit(&model, &all_entities(&model), &Explored::default())
            .unwrap();
        assert!(result.contains("[*] --> state__Doc_review"), "{result}");
        assert!(!result.contains("[*] --> state__Doc_draft"), "{result}");
    }

    #[test]
    fn test_state_start_of_a_preexisting_entity_is_its_base() {
        let src = r#"
entity Acct "口座" {
  id: Int @pk
  status: Enum(active, frozen) @default(active)
}
usecase Freeze "凍結"
usecase Thaw "解除"
transitions(Freeze, Acct.status, active -> frozen)
transitions(Thaw, Acct.status, frozen -> active)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter
            .emit(&model, &all_entities(&model), &Explored::default())
            .unwrap();
        assert!(result.contains("[*] --> state__Acct_active"), "{result}");
        assert!(!result.contains("[*] --> state__Acct_frozen"), "{result}");
    }

    #[test]
    fn test_rdra_mermaid_draws_a_condition_with_its_derived_links() {
        let src = r#"
variation Method "Method" {
  cash
  card
}
entity Pay "Pay" {
  id: Int @pk
  method: Method
  status: Enum(open, paid) @default(open)
}
usecase Collect "Collect"
transitions(Collect, Pay.status, open -> paid)
condition Insured "Insured"
holds(Insured, Pay.method == card, Pay.status == paid)
applies_to(Insured, Collect)
"#;
        let model = model_from(src);
        let result = RdraMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(
            result.contains("condition__Insured{\"Insured\"}:::condition"),
            "{result}"
        );
        assert!(
            result.contains("variation__Method[\\\"Method\"\\]:::variation"),
            "{result}"
        );
        assert!(
            result.contains(
                "  condition__Insured -.-> variation__Method\n  \
                 %% derived condition__Insured -> variation__Method from holds\n"
            ),
            "{result}"
        );
        assert!(
            result.contains(
                "  condition__Insured -.-> entity__Pay\n  \
                 %% derived condition__Insured -> entity__Pay from holds\n"
            ),
            "{result}"
        );
        assert!(
            result.contains("condition__Insured -.->|applies_to| usecase__Collect"),
            "{result}"
        );
    }

    #[test]
    fn test_rdra_mermaid_draws_a_when_guard_from_the_condition_to_the_use_case() {
        let src = r#"
entity Pay "Pay" {
  id: Int @pk
  insured: Bool
  status: Enum(open, paid, waived) @default(open)
}
usecase Collect "Collect"
condition Insured "Insured"
holds(Insured, Pay.insured == true)
transitions(Collect, Pay.status, open -> waived).when(Insured)
transitions(Collect, Pay.status, open -> paid)
"#;
        let model = model_from(src);
        let result = RdraMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(
            result.contains(
                "  condition__Insured -.->|when| usecase__Collect\n  \
                 %% derived condition__Insured -> usecase__Collect via transitions Pay.status open->waived\n"
            ),
            "{result}"
        );
        let usecase = UsecaseMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(
            usecase.contains("  condition__Insured -.->|when| usecase__Collect\n"),
            "{usecase}"
        );
        assert!(usecase.contains("condition__Insured{\"Insured\"}:::condition"));
    }

    #[test]
    fn test_rdra_mermaid_draws_a_declared_combines_without_derived() {
        let src = r#"
variation Method "Method" {
  cash
  card
}
condition ByMethod "By method"
combines(ByMethod, Method)
"#;
        let model = model_from(src);
        let result = RdraMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(
            result.contains("condition__ByMethod -.-> variation__Method"),
            "{result}"
        );
        assert!(!result.contains("%% derived"), "{result}");
    }

    #[test]
    fn test_event_mermaid_renders_timers_and_extsystems() {
        let src = r#"
usecase Book "Book"
usecase Receive "Receive"
usecase Sweep "Sweep"
extsystem Gateway "Gateway"
extsystem Lab "Lab"
timer Nightly "Nightly"
event EvBooked "Booked"
event EvResult "Result"
raises(Book, EvBooked)
triggers(EvBooked, Gateway)
raises(Lab, EvResult)
triggers(EvResult, Receive)
triggers(Nightly, Sweep)
"#;
        let model = model_from(src);
        let result = EventMermaidEmitter
            .emit(&model, &View::whole().with_model_path("m.rdra"))
            .unwrap();
        assert!(
            result.starts_with(
                "%% rdra-ish diagram=event scope=whole model=m.rdra\n\
                 %% kinds: event usecase extsystem timer\n\
                 %% omitted: (none)\n\
                 flowchart LR\n"
            ),
            "{result}"
        );
        assert!(
            result.contains("  extsystem__Lab -.-> event__EvResult\n"),
            "{result}"
        );
        // An event delivered to an external system is the DSL's `triggers`.
        assert!(
            result.contains("  event__EvBooked -.-> extsystem__Gateway\n"),
            "{result}"
        );
        assert!(
            result.contains("  timer__Nightly -.-> usecase__Sweep\n"),
            "{result}"
        );
        assert!(result.contains("  timer__Nightly((\"Nightly\")):::timer\n"));
        assert!(
            result.ends_with("  classDef timer fill:#ffe8cc,stroke:#d9480f,color:#000;\n"),
            "{result}"
        );
    }

    #[test]
    fn test_event_mermaid_renders_triggered_buc() {
        let src = r#"
buc BucBillingClaims "Billing Claims"
usecase SignEncounter "Sign Encounter"
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
triggers(EncounterSigned, BucBillingClaims)
"#;
        let model = model_from(src);
        let result = EventMermaidEmitter.emit(&model, &View::whole()).unwrap();

        assert!(
            result.contains("event__EncounterSigned -.-> buc__BucBillingClaims"),
            "{result}"
        );
        assert!(result.contains("  buc__BucBillingClaims[\"Billing Claims\"]:::buc\n"));
    }

    #[test]
    fn test_event_mermaid_declares_each_node_once_and_draws_no_states() {
        let src = r#"
usecase SignEncounter "Sign Encounter"
usecase ReviewClaim "Review Claim"
buc BucBillingClaims "Billing Claims"
entity Encounter "Encounter" {
  id: Int @pk
  status: Enum(pending, signed) @default(pending)
}
entity Claim "Claim" {
  id: Int @pk
  status: Enum(open, reviewed) @default(open)
}
event EncounterSigned "Encounter Signed" description "The doctor signed the record"
raises(SignEncounter, EncounterSigned)
triggers(EncounterSigned, ReviewClaim)
triggers(EncounterSigned, BucBillingClaims)
transitions(SignEncounter, Encounter.status, pending -> signed)
transitions(ReviewClaim, Claim.status, open -> reviewed)
"#;
        let model = model_from(src);
        let flow = rdra_ish_core::collect_event_flows(&model).pop().unwrap();
        let mut out = String::new();
        let mut declared = Declared::default();

        render_event_flow(&mut out, &mut declared, &model, &None, &flow);
        render_event_flow(&mut out, &mut declared, &model, &None, &flow);

        assert_eq!(
            out.matches("  event__EncounterSigned>\"Encounter Signed\"]:::event\n")
                .count(),
            1,
            "{out}"
        );
        assert!(out.contains("  %% desc event__EncounterSigned: The doctor signed the record\n"));
        assert!(out.contains("usecase__SignEncounter -.-> event__EncounterSigned"));
        assert!(out.contains("event__EncounterSigned -.-> usecase__ReviewClaim"));
        assert!(out.contains("event__EncounterSigned -.-> buc__BucBillingClaims"));
        assert!(!out.contains("state"), "{out}");
        assert_eq!(declared.kinds, vec!["event", "usecase", "buc"]);

        let result = EventMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(
            result.contains("%% omitted: state transitions\n"),
            "{result}"
        );
    }

    #[test]
    fn test_event_mermaid_ids_follow_the_contract() {
        let src = r#"
usecase SignEncounter "Sign Encounter"
buc BucBillingClaims "Billing Claims"
extsystem Lab "Lab"
timer Nightly "Nightly"
event EncounterSigned "Encounter Signed"
"#;
        let model = model_from(src);
        let key = |id: &str| model.symbols.get(id).cloned().unwrap();
        let mut out = String::new();
        let mut declared = Declared::default();
        for (node, id) in [
            (key("EncounterSigned"), "event__EncounterSigned"),
            (key("SignEncounter"), "usecase__SignEncounter"),
            (key("BucBillingClaims"), "buc__BucBillingClaims"),
            (key("Lab"), "extsystem__Lab"),
            (key("Nightly"), "timer__Nightly"),
        ] {
            let flow_node = match node {
                NodeRef::Event(k) => FlowNode::Event(k),
                NodeRef::UseCase(k) => FlowNode::UseCase(k),
                NodeRef::Buc(k) => FlowNode::Buc(k),
                NodeRef::ExtSystem(k) => FlowNode::ExtSystem(k),
                NodeRef::Timer(k) => FlowNode::Timer(k),
                other => panic!("{other:?}"),
            };
            assert_eq!(
                declare_flow_node(&mut out, &mut declared, &model, flow_node).as_deref(),
                Some(id)
            );
            assert_eq!(contract_id(&model, &node).as_deref(), Some(id));
        }
    }

    #[test]
    fn test_event_mermaid_visibility_skips_hidden_event_scope() {
        let src = r#"
usecase SignEncounter "Sign Encounter"
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
"#;
        let model = model_from(src);
        let flow = rdra_ish_core::collect_event_flows(&model).pop().unwrap();
        let mut out = String::new();
        let mut declared = Declared::default();
        let reachable = Some(HashSet::new());

        render_event_flow(&mut out, &mut declared, &model, &reachable, &flow);

        assert!(out.is_empty());
        assert!(declared.ids.is_empty());
    }

    #[test]
    fn test_er_mermaid_buc_filter() {
        let src = r#"
buc BucA "業務A"
usecase UcA "ユースケースA"
entity EntityA "エンティティA" { id: Int @pk }
entity EntityB "エンティティB" { id: Int @pk }
contains(BucA, UcA)
reads(UcA, EntityA)
"#;
        let model = model_from(src);
        let view = View {
            state: Default::default(),
            scope: crate::Scope::Bucs(vec!["BucA".to_string()]),
            filter: crate::Filter::Er,
            model_path: None,
            node_kinds: Vec::new(),
            edge_kinds: Vec::new(),
        };
        let result = ErMermaidEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("EntityA"), "EntityA should be included");
        assert!(!result.contains("EntityB"), "EntityB should be excluded");
    }

    const ANOMALIES: &str = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, shipped, lost) @default(pending)
}
usecase Place "注文確定"
usecase Pay "支払い"
usecase Ship "発送"
usecase Express "即日発送"
buc BucOrder "Order"
contains(BucOrder, Place)
contains(BucOrder, Pay)
contains(BucOrder, Ship)
contains(BucOrder, Express)
creates(Place, Order)
updates(Pay, Order)
updates(Ship, Order)
updates(Express, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Ship, Order.status, paid -> shipped)
transitions(Express, Order.status, pending -> shipped)
condition Packed "梱包済み"
holds(Packed, Order.status == pending)
applies_to(Packed, Ship)
"#;

    fn anomaly_view(entities: &[&str]) -> (View, Explored) {
        let model = model_from(ANOMALIES);
        let report = rdra_ish_core::lifecycle::derive_states(
            &model,
            &rdra_ish_core::lifecycle::Scope::ALL,
            rdra_ish_core::lifecycle::Caps::STATES,
        );
        let mut view = View::whole();
        view.state.entities = entities.iter().map(|e| e.to_string()).collect();
        (view, Explored::from_report(&report))
    }

    #[test]
    fn test_state_full_model_colours_unreachable_deadlock_and_end() {
        let model = model_from(ANOMALIES);
        let (view, explored) = anomaly_view(&["Order"]);
        let out = StateMermaidEmitter.emit(&model, &view, &explored).unwrap();
        assert!(out.contains("%% legend:"), "{out}");
        assert!(
            out.contains("class state__Order_lost stateunreachable"),
            "{out}"
        );
        assert!(
            out.contains("class state__Order_paid statedeadlock"),
            "{out}"
        );
        assert!(out.contains("class state__Order_shipped stateend"), "{out}");
        assert!(!out.contains("state__Order_lost --> [*]"), "{out}");
        for name in ["stateunreachable", "statedeadlock", "stateend"] {
            assert!(out.contains(&format!("classDef {name} ")), "{out}");
        }
        assert!(out.contains("scope=entity:Order"), "{out}");
    }

    #[test]
    fn test_state_whole_model_is_an_index() {
        let model = model_from(ANOMALIES);
        let (view, explored) = anomaly_view(&[]);
        let out = StateMermaidEmitter.emit(&model, &view, &explored).unwrap();
        assert!(out.contains("4 状態 / 3 遷移 (--entity Order)"), "{out}");
        assert!(!out.contains("-->"), "{out}");
        assert!(!out.contains("class "), "{out}");
    }
}
