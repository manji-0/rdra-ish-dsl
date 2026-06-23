//! Mermaid emitters: RDRA全体図、ER図、状態遷移図。
//!
//! plantuml.rs と同じ3エミッタをMermaid記法で出力する。
//! ヘルパー関数 (node_id / node_label / col_type_str) は plantuml モジュールから再利用。

use crate::plantuml::{col_type_str, node_id, node_label};
use crate::{
    collect_object_graph_nodes, node_description, object_graph_layer, object_graph_rel_label,
    prefixed_label, prefixed_node_label, view_node_visible, view_relation_visible, EmitError,
    Emitter, Scope, View, OBJECT_GRAPH_LAYERS,
};
use rdra_ish_core::model::{
    ActorKey, ApiKey, BucKey, EntityKey, NodeRef, RelKind, ScreenKey, SemanticModel, StateKey,
    UseCaseKey,
};
use rdra_ish_core::tx::infer_usecase_transactions;
use rdra_ish_core::{
    derive_actor_input_inferences, derive_system_boundaries, ActorInputInference, EventFlow,
};
use std::collections::{HashMap, HashSet};

// ── RDRA全体図エミッタ (Mermaid) ──────────────────────────────────────────────

pub struct RdraMermaidEmitter;

fn graph_node_visible(reachable: &Option<HashSet<NodeRef>>, view: &View, node: &NodeRef) -> bool {
    scoped_node_visible(reachable, node) && view_node_visible(view, node)
}

fn render_mermaid_rdra_node_declarations(
    out: &mut String,
    model: &SemanticModel,
    reachable: &Option<HashSet<NodeRef>>,
    view: &View,
) {
    let mut actors: Vec<_> = model.actors.iter().collect();
    actors.sort_by_key(|(_, actor)| &actor.id);
    for (key, actor) in &actors {
        let node = NodeRef::Actor(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "  {}([\"{}\"])\n",
                actor.id,
                prefixed_node_label(&node, &actor.label)
            ));
        }
    }

    let mut usecases: Vec<_> = model.use_cases.iter().collect();
    usecases.sort_by_key(|(_, usecase)| &usecase.id);
    for (key, usecase) in &usecases {
        let node = NodeRef::UseCase(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "  {}([\"{}\"])\n",
                usecase.id,
                prefixed_node_label(&node, &usecase.label)
            ));
        }
    }

    let mut bucs: Vec<_> = model.bucs.iter().collect();
    bucs.sort_by_key(|(_, buc)| &buc.id);
    for (key, buc) in &bucs {
        let node = NodeRef::Buc(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "  {}[\"{}\"]\n",
                buc.id,
                prefixed_node_label(&node, &buc.label)
            ));
        }
    }

    let mut flows: Vec<_> = model.flows.iter().collect();
    flows.sort_by_key(|(_, flow)| &flow.id);
    for (key, flow) in &flows {
        let node = NodeRef::Flow(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "  {}[\"{}\"]\n",
                flow.id,
                prefixed_node_label(&node, &flow.label)
            ));
        }
    }

    let mut steps: Vec<_> = model.steps.iter().collect();
    steps.sort_by_key(|(_, step)| &step.id);
    for (key, step) in &steps {
        let node = NodeRef::Step(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "  {}([\"{}\"])\n",
                step.id,
                prefixed_node_label(&node, &step.label)
            ));
        }
    }

    let mut systems: Vec<_> = model.systems.iter().collect();
    systems.sort_by_key(|(_, system)| &system.id);
    for (key, system) in &systems {
        let node = NodeRef::System(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "  {}[\"{}\"]\n",
                system.id,
                prefixed_node_label(&node, &system.label)
            ));
        }
    }

    let mut ext_systems: Vec<_> = model.ext_systems.iter().collect();
    ext_systems.sort_by_key(|(_, ext_system)| &ext_system.id);
    for (key, ext_system) in &ext_systems {
        let node = NodeRef::ExtSystem(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "  {}[/\"{}\"/]\n",
                ext_system.id,
                prefixed_node_label(&node, &ext_system.label)
            ));
        }
    }

    let mut entities: Vec<_> = model.entities.iter().collect();
    entities.sort_by_key(|(_, entity)| &entity.id);
    for (key, entity) in &entities {
        let node = NodeRef::Entity(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "  {}[(\"{}\")]\n",
                entity.id,
                prefixed_node_label(&node, &entity.label)
            ));
        }
    }

    let mut screens: Vec<_> = model.screens.iter().collect();
    screens.sort_by_key(|(_, screen)| &screen.id);
    for (key, screen) in &screens {
        let node = NodeRef::Screen(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "  {}[[\"{}\"]]\n",
                screen.id,
                prefixed_node_label(&node, &screen.label)
            ));
        }
    }

    let mut fields: Vec<_> = model.fields.iter().collect();
    fields.sort_by_key(|(_, field)| &field.id);
    for (key, field) in &fields {
        let node = NodeRef::Field(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "  {}[[\"{}\"]]\n",
                field.id,
                prefixed_node_label(&node, &field.label)
            ));
        }
    }

    let mut events: Vec<_> = model.events.iter().collect();
    events.sort_by_key(|(_, event)| &event.id);
    for (key, event) in &events {
        let node = NodeRef::Event(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "  {}{{\"{}\"}}\n",
                event.id,
                prefixed_node_label(&node, &event.label)
            ));
        }
    }

    let mut states: Vec<_> = model.states.iter().collect();
    states.sort_by_key(|(_, state)| &state.id);
    for (key, state) in &states {
        let node = NodeRef::State(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "  {}(\"{}\")  \n",
                state.id,
                prefixed_node_label(&node, &state.label)
            ));
        }
    }
}

fn mermaid_rdra_relation_arrow(kind: &RelKind, from_id: &str, to_id: &str) -> Option<String> {
    let arrow = match kind {
        RelKind::Performs | RelKind::Uses => format!("  {} --> {}\n", from_id, to_id),
        RelKind::Reads => format!("  {} -.->|reads| {}\n", from_id, to_id),
        RelKind::Writes => format!("  {} -.->|writes| {}\n", from_id, to_id),
        RelKind::Creates => format!("  {} -.->|creates| {}\n", from_id, to_id),
        RelKind::Updates => format!("  {} -.->|updates| {}\n", from_id, to_id),
        RelKind::Deletes => format!("  {} -.->|deletes| {}\n", from_id, to_id),
        RelKind::Displays => format!("  {} -.->|displays| {}\n", from_id, to_id),
        RelKind::Shows => format!("  {} -.->|shows| {}\n", from_id, to_id),
        RelKind::Raises => format!("  {} -.->|raises| {}\n", from_id, to_id),
        RelKind::Triggers => format!("  {} -.->|triggers| {}\n", from_id, to_id),
        RelKind::Contains | RelKind::Belongs => format!("  {} --> {}\n", from_id, to_id),
        RelKind::HasPermission => format!("  {} -.->|has_permission| {}\n", from_id, to_id),
        RelKind::RequiresPermission => {
            format!("  {} -.->|requires_permission| {}\n", from_id, to_id)
        }
        RelKind::RequiresMedium => format!("  {} -.->|requires_medium| {}\n", from_id, to_id),
        RelKind::Motivates => format!("  {} -.->|motivates| {}\n", from_id, to_id),
        RelKind::Decides => format!("  {} -.->|decides| {}\n", from_id, to_id),
        RelKind::Precedes => format!("  {} -->|precedes| {}\n", from_id, to_id),
        RelKind::Branches => format!("  {} -.->|branches| {}\n", from_id, to_id),
        RelKind::Excepts => format!("  {} -.->|excepts| {}\n", from_id, to_id),
        RelKind::Repeats => format!("  {} -.->|repeats| {}\n", from_id, to_id),
        RelKind::Covers => format!("  {} -.->|covers| {}\n", from_id, to_id),
        RelKind::Compensates => format!("  {} -.->|compensates| {}\n", from_id, to_id),
        RelKind::Request => format!("  {} -.->|request| {}\n", from_id, to_id),
        RelKind::Response => format!("  {} -.->|response| {}\n", from_id, to_id),
        RelKind::ErrorResponse => format!("  {} -.->|error_response| {}\n", from_id, to_id),
        RelKind::AppliesTo => format!("  {} -.->|applies_to| {}\n", from_id, to_id),
        RelKind::Qualifies => format!("  {} -.->|qualifies| {}\n", from_id, to_id),
        RelKind::Constrains => format!("  {} -.->|constrains| {}\n", from_id, to_id),
        RelKind::MapsTo => format!("  {} -.->|maps_to| {}\n", from_id, to_id),
        RelKind::MapsField => format!("  {} -.->|maps_field| {}\n", from_id, to_id),
        RelKind::Owns => format!("  {} -->|owns| {}\n", from_id, to_id),
        RelKind::Transitions | RelKind::Invokes => return None,
        RelKind::RelateOneToOne
        | RelKind::RelateOneToMany
        | RelKind::RelateManyToOne
        | RelKind::RelateManyToMany => format!("  {} --- {}\n", from_id, to_id),
    };
    Some(arrow)
}

fn rdra_description_nodes(
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

fn render_mermaid_description_annotations(
    out: &mut String,
    model: &SemanticModel,
    nodes: &[NodeRef],
) {
    for node in nodes {
        let (Some(id), Some(description)) = (node_id(model, node), node_description(model, node))
        else {
            continue;
        };
        let description = description.trim();
        if description.is_empty() {
            continue;
        }
        let note_id = format!("{}_description", id);
        let description = mermaid_label(description).replace('\n', "<br/>");
        out.push_str(&format!("  {}[\"{}\"]\n", note_id, description));
        out.push_str(&format!("  {} -. description .- {}\n", note_id, id));
    }
}

fn render_mermaid_rdra_relations(
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
            if let Some(arrow) = mermaid_rdra_relation_arrow(&relation.kind, from_id, to_id) {
                out.push_str(&arrow);
            }
        }
    }
}

impl Emitter for RdraMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let reachable = reachable_for_scope(model, &view.scope);
        let mut out = String::new();
        out.push_str("graph TD\n");
        render_mermaid_rdra_node_declarations(&mut out, model, &reachable, view);
        render_mermaid_rdra_relations(&mut out, model, &reachable, view);
        if view.show_descriptions {
            let nodes: Vec<_> = rdra_description_nodes(model, &reachable)
                .into_iter()
                .filter(|node| view_node_visible(view, node))
                .collect();
            render_mermaid_description_annotations(&mut out, model, &nodes);
        }

        while out.ends_with("\n\n") {
            out.pop();
        }
        Ok(out)
    }
}

// ── RDRA レイヤ図エミッタ (Mermaid) ─────────────────────────────────────────

pub struct ObjectGraphMermaidEmitter;

fn mermaid_label(label: &str) -> String {
    label.replace('"', "#quot;")
}

impl Emitter for ObjectGraphMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let reachable: Option<HashSet<NodeRef>> = match &view.scope {
            Scope::Bucs(buc_ids) => Some(rdra_ish_core::reachable_from_bucs(model, buc_ids)),
            Scope::Whole | Scope::UseCases(_) => None,
        };

        let is_visible = |nr: &NodeRef| -> bool {
            let scoped = match &reachable {
                Some(set) => set.contains(nr),
                None => true,
            };
            scoped && view_node_visible(view, nr)
        };

        let visible_nodes = collect_object_graph_nodes(model, &is_visible);
        let visible_set: HashSet<NodeRef> = visible_nodes.iter().cloned().collect();

        let mut out = String::new();
        out.push_str("flowchart LR\n");

        for layer in OBJECT_GRAPH_LAYERS {
            out.push_str(&format!(
                "  subgraph {}[{}]\n",
                layer.mermaid_id(),
                layer.label()
            ));
            out.push_str("    direction TB\n");
            for nr in visible_nodes
                .iter()
                .filter(|nr| object_graph_layer(nr) == layer)
            {
                if let (Some(id), Some(label)) = (node_id(model, nr), node_label(model, nr)) {
                    let label = mermaid_label(&prefixed_node_label(nr, label));
                    let line = match nr {
                        NodeRef::Actor(_) => format!("    {}([\"{}\"])\n", id, label),
                        NodeRef::Requirement(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Adr(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Nfr(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::ExtSystem(_) => format!("    {}[/\"{}\"/]\n", id, label),
                        NodeRef::Quality(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Constraint(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Concept(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::DomainObject(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Aggregate(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::ValueObject(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Business(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Buc(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Flow(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Step(_) => format!("    {}([\"{}\"])\n", id, label),
                        NodeRef::UsageScene(_) => format!("    {}([\"{}\"])\n", id, label),
                        NodeRef::Condition(_) => format!("    {}{{\"{}\"}}\n", id, label),
                        NodeRef::Variation(_) => format!("    {}{{\"{}\"}}\n", id, label),
                        NodeRef::Location(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Timing(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Medium(_) => format!("    {}[/\"{}\"/]\n", id, label),
                        NodeRef::Permission(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::UseCase(_) => format!("    {}([\"{}\"])\n", id, label),
                        NodeRef::Screen(_) => format!("    {}[[\"{}\"]]\n", id, label),
                        NodeRef::Field(_) => format!("    {}[[\"{}\"]]\n", id, label),
                        NodeRef::Event(_) => format!("    {}{{\"{}\"}}\n", id, label),
                        NodeRef::Api(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Dto(_) => format!("    {}[[\"{}\"]]\n", id, label),
                        NodeRef::System(_) => format!("    {}[\"{}\"]\n", id, label),
                        NodeRef::Entity(_) => format!("    {}[(\"{}\")]\n", id, label),
                        NodeRef::State(_) => format!("    {}(\"{}\")\n", id, label),
                    };
                    out.push_str(&line);
                }
            }
            out.push_str("  end\n");
        }

        let mut relations: Vec<_> = model.relations.iter().collect();
        relations.sort_by_key(|r| format!("{:?}{:?}{:?}", r.from, r.kind, r.to));
        for rel in relations {
            if !visible_set.contains(&rel.from) || !visible_set.contains(&rel.to) {
                continue;
            }
            if !view_relation_visible(view, &rel.kind) {
                continue;
            }
            if let (Some(from_id), Some(to_id)) =
                (node_id(model, &rel.from), node_id(model, &rel.to))
            {
                let label = object_graph_rel_label(&rel.kind);
                let line = match rel.kind {
                    RelKind::Performs | RelKind::Contains | RelKind::Uses | RelKind::Owns => {
                        format!("  {} -->|{}| {}\n", from_id, label, to_id)
                    }
                    RelKind::RelateOneToOne
                    | RelKind::RelateOneToMany
                    | RelKind::RelateManyToOne
                    | RelKind::RelateManyToMany => {
                        format!("  {} ---|{}| {}\n", from_id, label, to_id)
                    }
                    _ => format!("  {} -.->|{}| {}\n", from_id, label, to_id),
                };
                out.push_str(&line);
            }
        }

        if view.show_descriptions {
            render_mermaid_description_annotations(&mut out, model, &visible_nodes);
        }

        while out.ends_with("\n\n") {
            out.pop();
        }
        Ok(out)
    }
}

// ── Business area diagram (Mermaid) ──────────────────────────────────────────

pub struct BusinessAreaMermaidEmitter;

impl Emitter for BusinessAreaMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let mut out = String::from("flowchart LR\n");
        let mut nodes = HashSet::new();
        let mut edges = HashSet::new();

        for entry in derive_actor_input_inferences(model)
            .into_iter()
            .filter(|entry| business_input_visible(model, view, entry))
        {
            let actor = &model.actors[entry.actor];
            let usecase = &model.use_cases[entry.usecase];
            let entity = &model.entities[entry.entity];
            let actor_id = business_actor_node_id(model, &entry);
            let usecase_id = business_usecase_node_id(model, &entry);
            let input_id = business_input_node_id(model, &entry);
            let input_label = mermaid_label(&format!(
                "{}.{}, {}",
                entity.id,
                entry.column,
                entry.operation.as_str()
            ));

            if nodes.insert(actor_id.clone()) {
                out.push_str(&format!(
                    "  {}([\"{}\"])\n",
                    actor_id,
                    mermaid_label(&prefixed_label("Business Actor", &actor.label))
                ));
            }
            if nodes.insert(input_id.clone()) {
                out.push_str(&format!("  {}[\"{}\"]\n", input_id, input_label));
            }
            if nodes.insert(usecase_id.clone()) {
                out.push_str(&format!(
                    "  {}([\"{}\"])\n",
                    usecase_id,
                    mermaid_label(&prefixed_label("UseCase", &usecase.label))
                ));
            }

            let actor_edge = format!("  {} --> {}\n", actor_id, input_id);
            if edges.insert(actor_edge.clone()) {
                out.push_str(&actor_edge);
            }
            let uc_edge = format!(
                "  {} -->|{}| {}\n",
                input_id,
                entry.operation.as_str(),
                usecase_id
            );
            if edges.insert(uc_edge.clone()) {
                out.push_str(&uc_edge);
            }
        }

        Ok(out)
    }
}

// ── Technical area diagram (Mermaid) ─────────────────────────────────────────

pub struct TechnicalAreaMermaidEmitter;

impl Emitter for TechnicalAreaMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let reachable = reachable_for_scope(model, &view.scope);
        let mut out = String::from("flowchart LR\n");

        for boundary in derive_system_boundaries(model) {
            let system = &model.systems[boundary.system];
            let apis: Vec<_> = boundary
                .apis
                .iter()
                .copied()
                .filter(|api| scoped_node_visible(&reachable, &NodeRef::Api(*api)))
                .collect();
            let entities: Vec<_> = boundary
                .entities
                .iter()
                .copied()
                .filter(|entity| scoped_node_visible(&reachable, &NodeRef::Entity(*entity)))
                .collect();
            if apis.is_empty() && entities.is_empty() {
                continue;
            }

            let system_id = scoped_mermaid_id("system", &system.id);
            out.push_str(&format!(
                "  subgraph {}[{}]\n",
                system_id,
                mermaid_label(&system.label)
            ));
            out.push_str("    direction TB\n");
            for api in &apis {
                let api_model = &model.apis[*api];
                out.push_str(&format!(
                    "    {}[\"{}\"]\n",
                    technical_api_node_id(model, boundary.system, *api),
                    mermaid_label(&prefixed_label("API", &api_model.label))
                ));
            }
            for entity in &entities {
                let entity_model = &model.entities[*entity];
                out.push_str(&format!(
                    "    {}[(\"{}\")]\n",
                    technical_entity_node_id(model, boundary.system, *entity),
                    mermaid_label(&prefixed_label("Entity", &entity_model.label))
                ));
            }
            out.push_str("  end\n");

            let entity_set: HashSet<_> = entities.iter().copied().collect();
            for rel in &model.relations {
                let (NodeRef::Api(api), NodeRef::Entity(entity)) = (&rel.from, &rel.to) else {
                    continue;
                };
                if !apis.contains(api)
                    || !entity_set.contains(entity)
                    || !is_entity_operation(&rel.kind)
                {
                    continue;
                }
                out.push_str(&format!(
                    "  {} -.->|{}| {}\n",
                    technical_api_node_id(model, boundary.system, *api),
                    object_graph_rel_label(&rel.kind),
                    technical_entity_node_id(model, boundary.system, *entity)
                ));
            }
        }

        Ok(out)
    }
}

// ── 状態遷移図エミッタ (Mermaid) ──────────────────────────────────────────────

pub struct StateMermaidEmitter;

impl Emitter for StateMermaidEmitter {
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

        let is_state_visible = |sk: StateKey| -> bool { is_visible(&NodeRef::State(sk)) };

        let transitions: Vec<_> = model
            .state_transitions
            .iter()
            .filter(|t| is_state_visible(t.from) && is_state_visible(t.to))
            .collect();

        if transitions.is_empty() {
            return Ok("stateDiagram-v2\n".to_string());
        }

        // 初期状態 = いずれの to にも登場しない from
        let to_set: HashSet<StateKey> = transitions.iter().map(|t| t.to).collect();
        let mut initial_states: Vec<StateKey> = transitions
            .iter()
            .map(|t| t.from)
            .filter(|sk| !to_set.contains(sk))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        initial_states.sort_by_key(|sk| node_id(model, &NodeRef::State(*sk)).unwrap_or(""));

        let mut out = String::new();
        out.push_str("stateDiagram-v2\n");

        for initial in &initial_states {
            if let Some(id) = node_id(model, &NodeRef::State(*initial)) {
                out.push_str(&format!("  [*] --> {}\n", id));
            }
        }

        let mut sorted: Vec<_> = transitions.iter().collect();
        sorted.sort_by_key(|t| {
            format!(
                "{}{}{}",
                node_id(model, &NodeRef::State(t.from)).unwrap_or(""),
                node_id(model, &NodeRef::State(t.to)).unwrap_or(""),
                node_id(model, &NodeRef::Event(t.event)).unwrap_or(""),
            )
        });

        // ノード名ラベル（state "label" as id）を出力してから遷移を出力
        let mut defined: HashSet<String> = HashSet::new();
        for t in &sorted {
            for sk in [t.from, t.to] {
                let nr = NodeRef::State(sk);
                if let (Some(id), Some(label)) = (node_id(model, &nr), node_label(model, &nr)) {
                    if defined.insert(id.to_string()) {
                        out.push_str(&format!(
                            "  state \"{}\" as {}\n",
                            prefixed_node_label(&nr, label),
                            id
                        ));
                    }
                }
            }
        }

        for t in &sorted {
            let from_nr = NodeRef::State(t.from);
            let to_nr = NodeRef::State(t.to);
            let event_nr = NodeRef::Event(t.event);
            if let (Some(from_id), Some(to_id), Some(ev_label)) = (
                node_id(model, &from_nr),
                node_id(model, &to_nr),
                node_label(model, &event_nr),
            ) {
                out.push_str(&format!(
                    "  {} --> {} : {}\n",
                    from_id,
                    to_id,
                    prefixed_node_label(&event_nr, ev_label)
                ));
            }
        }

        Ok(out)
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

        let mut out = String::new();
        out.push_str("erDiagram\n");

        // entities
        let mut ents: Vec<_> = model.entities.iter().collect();
        ents.sort_by_key(|(_, e)| &e.id);

        for (k, ent) in &ents {
            if !is_visible(&NodeRef::Entity(*k)) {
                continue;
            }
            out.push_str(&format!("  {} {{\n", ent.id));

            for col in &ent.columns {
                let type_str = col_type_str(&col.col_type);
                if col.is_pk {
                    out.push_str(&format!("    {} {} PK\n", type_str, col.name));
                } else if col.is_fk {
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
            .filter(|r| {
                matches!(
                    r.kind,
                    RelKind::RelateOneToOne
                        | RelKind::RelateOneToMany
                        | RelKind::RelateManyToOne
                        | RelKind::RelateManyToMany
                )
            })
            .collect();
        er_rels.sort_by_key(|r| format!("{:?}{:?}", r.from, r.to));

        for rel in &er_rels {
            if !is_visible(&rel.from) || !is_visible(&rel.to) {
                continue;
            }
            if let (Some(from_id), Some(to_id)) =
                (node_id(model, &rel.from), node_id(model, &rel.to))
            {
                // Mermaid erDiagram の基数記法:
                //   ||--||  1対1
                //   ||--o{  1対多
                //   }o--||  多対1
                //   }o--o{  多対多
                let line = match &rel.kind {
                    RelKind::RelateOneToOne => {
                        format!("  {} ||--|| {} : \"\"\n", from_id, to_id)
                    }
                    RelKind::RelateOneToMany => {
                        format!("  {} ||--o{{ {} : \"\"\n", from_id, to_id)
                    }
                    RelKind::RelateManyToOne => {
                        format!("  {} }}o--|| {} : \"\"\n", from_id, to_id)
                    }
                    RelKind::RelateManyToMany => {
                        format!("  {} }}o--o{{ {} : \"\"\n", from_id, to_id)
                    }
                    _ => continue,
                };
                out.push_str(&line);
            }
        }

        Ok(out)
    }
}

// ── シーケンス図エミッタ (Mermaid) ───────────────────────────────────────────

pub struct SequenceMermaidEmitter;

fn sequence_usecase_scope(model: &SemanticModel, scope: &Scope) -> Option<HashSet<UseCaseKey>> {
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

impl Emitter for SequenceMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let scoped_usecases = sequence_usecase_scope(model, &view.scope);
        let is_visible_usecase = |key: UseCaseKey| -> bool {
            match &scoped_usecases {
                Some(set) => set.contains(&key),
                None => true,
            }
        };

        let mut uc_to_bucs: HashMap<UseCaseKey, Vec<BucKey>> = HashMap::new();
        let mut buc_to_actors: HashMap<BucKey, Vec<ActorKey>> = HashMap::new();
        let mut uc_to_screens: HashMap<UseCaseKey, Vec<ScreenKey>> = HashMap::new();
        let mut uc_to_apis: HashMap<UseCaseKey, Vec<ApiKey>> = HashMap::new();
        let mut direct_actor_of: HashMap<UseCaseKey, Vec<ActorKey>> = HashMap::new();
        let mut uc_to_reads: HashMap<UseCaseKey, Vec<EntityKey>> = HashMap::new();
        let mut api_to_reads: HashMap<ApiKey, Vec<EntityKey>> = HashMap::new();

        for rel in &model.relations {
            match &rel.kind {
                RelKind::Contains => {
                    if let (NodeRef::Buc(bk), NodeRef::UseCase(uk)) = (&rel.from, &rel.to) {
                        uc_to_bucs.entry(*uk).or_default().push(*bk);
                    }
                }
                RelKind::Performs => match (&rel.from, &rel.to) {
                    (NodeRef::Actor(ak), NodeRef::Buc(bk)) => {
                        buc_to_actors.entry(*bk).or_default().push(*ak);
                    }
                    (NodeRef::Actor(ak), NodeRef::UseCase(uk)) => {
                        direct_actor_of.entry(*uk).or_default().push(*ak);
                    }
                    _ => {}
                },
                RelKind::Displays => {
                    if let (NodeRef::UseCase(uk), NodeRef::Screen(sk)) = (&rel.from, &rel.to) {
                        uc_to_screens.entry(*uk).or_default().push(*sk);
                    }
                }
                RelKind::Invokes => {
                    if let (NodeRef::UseCase(uk), NodeRef::Api(ak)) = (&rel.from, &rel.to) {
                        uc_to_apis.entry(*uk).or_default().push(*ak);
                    }
                }
                RelKind::Reads => match (&rel.from, &rel.to) {
                    (NodeRef::UseCase(uk), NodeRef::Entity(ek)) => {
                        uc_to_reads.entry(*uk).or_default().push(*ek);
                    }
                    (NodeRef::Api(ak), NodeRef::Entity(ek)) => {
                        api_to_reads.entry(*ak).or_default().push(*ek);
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        let uc_txs = infer_usecase_transactions(model);
        let uc_tx_map: HashMap<UseCaseKey, &rdra_ish_core::UsecaseTx> =
            uc_txs.iter().map(|t| (t.usecase, t)).collect();
        let has_reads = |uk: UseCaseKey| -> bool {
            uc_to_reads.get(&uk).map(|r| !r.is_empty()).unwrap_or(false)
                || uc_to_apis.get(&uk).is_some_and(|apis| {
                    apis.iter()
                        .any(|ak| api_to_reads.get(ak).map(|r| !r.is_empty()).unwrap_or(false))
                })
        };

        let mut uc_list: Vec<(UseCaseKey, &rdra_ish_core::model::UseCase)> = model
            .use_cases
            .iter()
            .filter(|(k, _)| {
                is_visible_usecase(*k)
                    && (uc_tx_map.get(k).map(|t| t.has_writes()).unwrap_or(false) || has_reads(*k))
            })
            .collect();
        uc_list.sort_by_key(|(_, u)| u.id.as_str());

        if uc_list.is_empty() {
            return Ok("sequenceDiagram\n%% no sequenceable usecases found\n".to_string());
        }

        let mut actor_keys: HashSet<ActorKey> = HashSet::new();
        let mut entity_keys: HashSet<EntityKey> = HashSet::new();
        let mut screen_keys: HashSet<ScreenKey> = HashSet::new();
        let mut api_keys: HashSet<ApiKey> = HashSet::new();
        let mut has_legacy_uc = false;

        for (uk, _) in &uc_list {
            let actor_key = direct_actor_of
                .get(uk)
                .and_then(|actors| actors.first())
                .copied()
                .or_else(|| {
                    uc_to_bucs
                        .get(uk)
                        .and_then(|bucs| bucs.first())
                        .and_then(|bk| buc_to_actors.get(bk))
                        .and_then(|actors| actors.first())
                        .copied()
                });
            if let Some(ak) = actor_key {
                actor_keys.insert(ak);
            }
            if let Some(tx) = uc_tx_map.get(uk) {
                for g in &tx.fk_groups {
                    for w in &g.ordered_writes {
                        entity_keys.insert(w.entity);
                    }
                }
                for w in &tx.isolated_writes {
                    entity_keys.insert(w.entity);
                }
            }
            for &ek in uc_to_reads.get(uk).into_iter().flatten() {
                entity_keys.insert(ek);
            }
            for &sk in uc_to_screens.get(uk).into_iter().flatten() {
                screen_keys.insert(sk);
            }
            if let Some(apis) = uc_to_apis.get(uk) {
                for &ak in apis {
                    api_keys.insert(ak);
                    for &ek in api_to_reads.get(&ak).into_iter().flatten() {
                        entity_keys.insert(ek);
                    }
                }
            } else {
                has_legacy_uc = true;
            }
        }

        let mut out = String::from("sequenceDiagram\n");

        let mut actors_sorted: Vec<(ActorKey, &rdra_ish_core::model::Actor)> = model
            .actors
            .iter()
            .filter(|(k, _)| actor_keys.contains(k))
            .collect();
        actors_sorted.sort_by_key(|(_, a)| a.id.as_str());
        if !actors_sorted.is_empty() {
            out.push_str("  box System Value\n");
            for (_, actor) in &actors_sorted {
                out.push_str(&format!(
                    "    actor {} as {}\n",
                    actor.id,
                    prefixed_label("👤", &actor.label)
                ));
            }
            out.push_str("  end\n");
        }

        let mut scrs_sorted: Vec<(ScreenKey, &rdra_ish_core::model::Screen)> = model
            .screens
            .iter()
            .filter(|(k, _)| screen_keys.contains(k))
            .collect();
        scrs_sorted.sort_by_key(|(_, s)| s.id.as_str());

        let mut apis_sorted: Vec<(ApiKey, &rdra_ish_core::model::Api)> = model
            .apis
            .iter()
            .filter(|(k, _)| api_keys.contains(k))
            .collect();
        apis_sorted.sort_by_key(|(_, a)| a.id.as_str());
        if !scrs_sorted.is_empty() {
            out.push_str("  box System Boundary\n");
            for (_, scr) in &scrs_sorted {
                out.push_str(&format!(
                    "    participant {} as {}\n",
                    scr.id,
                    prefixed_label("🖥️", &scr.label)
                ));
            }
            out.push_str("  end\n");
        }

        let mut ents_sorted: Vec<(EntityKey, &rdra_ish_core::model::Entity)> = model
            .entities
            .iter()
            .filter(|(k, _)| entity_keys.contains(k))
            .collect();
        ents_sorted.sort_by_key(|(_, e)| e.id.as_str());
        if has_legacy_uc || !apis_sorted.is_empty() || !ents_sorted.is_empty() {
            out.push_str("  box System\n");
            if has_legacy_uc {
                out.push_str("    participant System as 🧩 システム\n");
            }
            for (_, api) in &apis_sorted {
                out.push_str(&format!(
                    "    participant {} as {}\n",
                    api.id,
                    prefixed_label("🔌", &api.label)
                ));
            }
            for (_, ent) in &ents_sorted {
                out.push_str(&format!(
                    "    participant {} as {}\n",
                    ent.id,
                    prefixed_label("🗄️", &ent.label)
                ));
            }
            out.push_str("  end\n");
        }
        out.push('\n');

        // セクション見出し用参加者ID
        let first_id = actors_sorted
            .first()
            .map(|(_, a)| a.id.as_str())
            .unwrap_or("System");
        let last_id = ents_sorted
            .last()
            .map(|(_, e)| e.id.as_str())
            .or_else(|| apis_sorted.last().map(|(_, a)| a.id.as_str()))
            .or_else(|| scrs_sorted.last().map(|(_, s)| s.id.as_str()))
            .unwrap_or("System");

        for (uk, uc) in &uc_list {
            let uc_label = prefixed_label("✅", &uc.label);
            if first_id == last_id {
                out.push_str(&format!("  Note over {}: {}\n", first_id, uc_label));
            } else {
                out.push_str(&format!(
                    "  Note over {},{}: {}\n",
                    first_id, last_id, uc_label
                ));
            }

            let actor_id: Option<String> = direct_actor_of
                .get(uk)
                .and_then(|actors| actors.first())
                .and_then(|ak| model.actors.get(*ak))
                .map(|a| a.id.clone())
                .or_else(|| {
                    uc_to_bucs
                        .get(uk)
                        .and_then(|bucs| bucs.first())
                        .and_then(|bk| buc_to_actors.get(bk))
                        .and_then(|actors| actors.first())
                        .and_then(|ak| model.actors.get(*ak))
                        .map(|a| a.id.clone())
                });
            let actor_ref = actor_id.as_deref().unwrap_or("System");

            let screen_id: Option<String> = uc_to_screens
                .get(uk)
                .and_then(|s| s.first())
                .and_then(|sk| model.screens.get(*sk))
                .map(|s| s.id.clone());
            let screen_label: Option<String> = uc_to_screens
                .get(uk)
                .and_then(|s| s.first())
                .and_then(|sk| model.screens.get(*sk))
                .map(|s| prefixed_label("🖥️", &s.label));

            let invoked_apis = uc_to_apis.get(uk);

            if let Some(apis) = invoked_apis.filter(|a| !a.is_empty()) {
                // ── API有りパス ──────────────────────────────────────────────
                let first_api_id = model
                    .apis
                    .get(apis[0])
                    .map(|a| a.id.as_str())
                    .unwrap_or("System");

                if let Some(ref sid) = screen_id {
                    out.push_str(&format!("  {}->>{}: {}\n", actor_ref, sid, uc_label));
                    out.push_str(&format!("  {}->>{}: {}\n", sid, first_api_id, uc_label));
                } else {
                    out.push_str(&format!(
                        "  {}->>{}: {}\n",
                        actor_ref, first_api_id, uc_label
                    ));
                }
                out.push_str(&format!("  activate {}\n", first_api_id));

                if let Some(apis) = invoked_apis {
                    for &ak in apis {
                        let src = model
                            .apis
                            .get(ak)
                            .map(|a| a.id.as_str())
                            .unwrap_or(first_api_id);
                        for &ek in api_to_reads.get(&ak).into_iter().flatten() {
                            if let Some(ent) = model.entities.get(ek) {
                                out.push_str(&format!("  {}->>{}: read\n", src, ent.id));
                            }
                        }
                    }
                }
                for &ek in uc_to_reads.get(uk).into_iter().flatten() {
                    if let Some(ent) = model.entities.get(ek) {
                        out.push_str(&format!("  {}->>{}: read\n", first_api_id, ent.id));
                    }
                }

                if let Some(tx) = uc_tx_map.get(uk) {
                    let singletons_set: HashSet<EntityKey> =
                        tx.singletons_note.iter().cloned().collect();

                    for group in &tx.fk_groups {
                        out.push_str("  rect rgb(245,245,245)\n");
                        let note = if group.inferred {
                            "transaction (inferred from FK)"
                        } else {
                            "transaction (API atomic boundary)"
                        };
                        out.push_str(&format!("    Note right of {}: {}\n", first_api_id, note));
                        for w in &group.ordered_writes {
                            if let Some(ent) = model.entities.get(w.entity) {
                                let src = w
                                    .via_api
                                    .and_then(|ak| model.apis.get(ak))
                                    .map(|a| a.id.as_str())
                                    .unwrap_or(first_api_id);
                                out.push_str(&format!(
                                    "    {}->>{}: {}\n",
                                    src,
                                    ent.id,
                                    w.kind.label()
                                ));
                            }
                        }
                        out.push_str("  end\n");
                    }

                    for w in &tx.isolated_writes {
                        if let Some(ent) = model.entities.get(w.entity) {
                            let src = w
                                .via_api
                                .and_then(|ak| model.apis.get(ak))
                                .map(|a| a.id.as_str())
                                .unwrap_or(first_api_id);
                            out.push_str(&format!("  {}->>{}: {}\n", src, ent.id, w.kind.label()));
                            if singletons_set.contains(&w.entity) {
                                out.push_str(&format!(
                                    "  Note right of {}: FK非連結 — 別TX？API境界で明示を\n",
                                    src
                                ));
                            }
                        }
                    }
                }

                if let Some(ref sid) = screen_id {
                    out.push_str(&format!(
                        "  {}-->>{}: {}\n",
                        first_api_id,
                        sid,
                        screen_label.as_deref().unwrap_or("")
                    ));
                    out.push_str(&format!(
                        "  {}-->>{}: {}\n",
                        sid,
                        actor_ref,
                        screen_label.as_deref().unwrap_or("")
                    ));
                } else {
                    out.push_str(&format!(
                        "  {}-->>{}: {}\n",
                        first_api_id, actor_ref, uc_label
                    ));
                }
                out.push_str(&format!("  deactivate {}\n\n", first_api_id));
            } else {
                // ── レガシーパス（System ライン）─────────────────────────────
                out.push_str(&format!("  {}->System: {}\n", actor_ref, uc_label));
                out.push_str("  activate System\n");

                for &ek in uc_to_reads.get(uk).into_iter().flatten() {
                    if let Some(ent) = model.entities.get(ek) {
                        out.push_str(&format!("  System->>{}: read\n", ent.id));
                    }
                }

                if let Some(tx) = uc_tx_map.get(uk) {
                    let singletons_set: HashSet<EntityKey> =
                        tx.singletons_note.iter().cloned().collect();

                    for group in &tx.fk_groups {
                        out.push_str("  rect rgb(245,245,245)\n");
                        let note = if group.inferred {
                            "transaction (inferred from FK)"
                        } else {
                            "transaction (API atomic boundary)"
                        };
                        out.push_str(&format!("    Note right of System: {}\n", note));
                        for w in &group.ordered_writes {
                            if let Some(ent) = model.entities.get(w.entity) {
                                out.push_str(&format!(
                                    "    System->>{}: {}\n",
                                    ent.id,
                                    w.kind.label()
                                ));
                            }
                        }
                        out.push_str("  end\n");
                    }

                    for w in &tx.isolated_writes {
                        if let Some(ent) = model.entities.get(w.entity) {
                            out.push_str(&format!("  System->>{}: {}\n", ent.id, w.kind.label()));
                            if singletons_set.contains(&w.entity) {
                                out.push_str(
                                    "  Note right of System: FK非連結 — 別TX？API境界で明示を\n",
                                );
                            }
                        }
                    }
                }

                if let Some(ref slabel) = screen_label {
                    out.push_str(&format!("  System-->>{}: {}\n", actor_ref, slabel));
                }

                out.push_str("  deactivate System\n\n");
            }
        }

        while out.ends_with("\n\n") {
            out.pop();
        }
        Ok(out)
    }
}

// ── イベントフロー図エミッタ (Mermaid) ───────────────────────────────────────

pub struct EventFlowMermaidEmitter;

fn mermaid_event_flow_event_id(id: &str) -> String {
    format!("ev__{}", id)
}

fn mermaid_event_flow_usecase_id(id: &str) -> String {
    format!("uc__{}", id)
}

fn mermaid_event_flow_buc_id(id: &str) -> String {
    format!("buc__{}", id)
}

fn mermaid_event_flow_state_id(id: &str) -> String {
    format!("st__{}", id)
}

fn declare_mermaid_event_flow_event(
    out: &mut String,
    declared: &mut HashSet<String>,
    model: &SemanticModel,
    flow: &EventFlow,
) -> Option<String> {
    let event = model.events.get(flow.event)?;
    let event_id = mermaid_event_flow_event_id(&event.id);
    if declared.insert(event_id.clone()) {
        out.push_str(&format!(
            "  {}{{\"{}\"}}\n",
            event_id,
            prefixed_label("⚡", &event.label)
        ));
    }
    Some(event_id)
}

fn declare_mermaid_event_flow_usecase(
    out: &mut String,
    declared: &mut HashSet<String>,
    model: &SemanticModel,
    usecase: UseCaseKey,
) -> Option<String> {
    let usecase_model = model.use_cases.get(usecase)?;
    let usecase_id = mermaid_event_flow_usecase_id(&usecase_model.id);
    if declared.insert(usecase_id.clone()) {
        out.push_str(&format!(
            "  {}([\"{}\"])\n",
            usecase_id,
            prefixed_label("✅", &usecase_model.label)
        ));
    }
    Some(usecase_id)
}

fn declare_mermaid_event_flow_buc(
    out: &mut String,
    declared: &mut HashSet<String>,
    model: &SemanticModel,
    buc: BucKey,
) -> Option<String> {
    let buc_model = model.bucs.get(buc)?;
    let buc_id = mermaid_event_flow_buc_id(&buc_model.id);
    if declared.insert(buc_id.clone()) {
        out.push_str(&format!(
            "  {}[[\"{}\"]]\n",
            buc_id,
            prefixed_label("📦", &buc_model.label)
        ));
    }
    Some(buc_id)
}

fn declare_mermaid_event_flow_state(
    out: &mut String,
    declared: &mut HashSet<String>,
    model: &SemanticModel,
    state: rdra_ish_core::model::StateKey,
) -> Option<String> {
    let state_model = model.states.get(state)?;
    let state_id = mermaid_event_flow_state_id(&state_model.id);
    if declared.insert(state_id.clone()) {
        out.push_str(&format!(
            "  {}(\"{}\")\n",
            state_id,
            prefixed_label("🔄", &state_model.label)
        ));
    }
    Some(state_id)
}

fn render_mermaid_event_flow(
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
    let Some(event_id) = declare_mermaid_event_flow_event(out, declared, model, flow) else {
        return;
    };

    let mut raised_by = flow.raised_by.to_vec();
    raised_by.sort_by_key(|&usecase| {
        model
            .use_cases
            .get(usecase)
            .map(|uc| uc.id.as_str())
            .unwrap_or("")
    });
    for usecase in raised_by {
        let node = NodeRef::UseCase(usecase);
        if !scoped_node_visible(reachable, &node) {
            continue;
        }
        if let Some(usecase_id) = declare_mermaid_event_flow_usecase(out, declared, model, usecase)
        {
            out.push_str(&format!("  {} -.->|raises| {}\n", usecase_id, event_id));
        }
    }

    let mut triggers = flow.triggers_ucs.to_vec();
    triggers.sort_by_key(|&usecase| {
        model
            .use_cases
            .get(usecase)
            .map(|uc| uc.id.as_str())
            .unwrap_or("")
    });
    for usecase in triggers {
        let node = NodeRef::UseCase(usecase);
        if !scoped_node_visible(reachable, &node) {
            continue;
        }
        if let Some(usecase_id) = declare_mermaid_event_flow_usecase(out, declared, model, usecase)
        {
            out.push_str(&format!("  {} -.->|triggers| {}\n", event_id, usecase_id));
        }
    }

    let mut triggered_bucs = flow.triggers_bucs.to_vec();
    triggered_bucs.sort_by_key(|&buc| model.bucs.get(buc).map(|b| b.id.as_str()).unwrap_or(""));
    for buc in triggered_bucs {
        let node = NodeRef::Buc(buc);
        if !scoped_node_visible(reachable, &node) {
            continue;
        }
        if let Some(buc_id) = declare_mermaid_event_flow_buc(out, declared, model, buc) {
            out.push_str(&format!("  {} -.->|triggers| {}\n", event_id, buc_id));
        }
    }

    let mut transitions = flow.transitions.to_vec();
    transitions.sort_by_key(|(from, _)| {
        model
            .states
            .get(*from)
            .map(|state| state.id.as_str())
            .unwrap_or("")
    });
    let event_label = model
        .events
        .get(flow.event)
        .map(|event| prefixed_label("⚡", &event.label))
        .unwrap_or_default();
    for (from, to) in transitions {
        let Some(from_id) = declare_mermaid_event_flow_state(out, declared, model, from) else {
            continue;
        };
        let Some(to_id) = declare_mermaid_event_flow_state(out, declared, model, to) else {
            continue;
        };
        out.push_str(&format!("  {} -->|{}| {}\n", from_id, event_label, to_id));
    }
}

impl Emitter for EventFlowMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let reachable = reachable_for_scope(model, &view.scope);
        let flows = rdra_ish_core::collect_event_flows(model);

        let mut out = String::new();
        out.push_str("flowchart LR\n");

        let mut declared: HashSet<String> = HashSet::new();

        for flow in &flows {
            render_mermaid_event_flow(&mut out, &mut declared, model, &reachable, flow);
        }

        Ok(out)
    }
}

fn business_input_visible(model: &SemanticModel, view: &View, entry: &ActorInputInference) -> bool {
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

fn business_input_node_id(model: &SemanticModel, entry: &ActorInputInference) -> String {
    scoped_mermaid_id(
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

fn business_actor_node_id(model: &SemanticModel, entry: &ActorInputInference) -> String {
    scoped_mermaid_id("actor", &model.actors[entry.actor].id)
}

fn business_usecase_node_id(model: &SemanticModel, entry: &ActorInputInference) -> String {
    scoped_mermaid_id("usecase", &model.use_cases[entry.usecase].id)
}

fn reachable_for_scope(model: &SemanticModel, scope: &Scope) -> Option<HashSet<NodeRef>> {
    match scope {
        Scope::Bucs(buc_ids) => Some(rdra_ish_core::reachable_from_bucs(model, buc_ids)),
        Scope::Whole | Scope::UseCases(_) => None,
    }
}

fn scoped_node_visible(reachable: &Option<HashSet<NodeRef>>, node: &NodeRef) -> bool {
    match reachable {
        Some(set) => set.contains(node),
        None => true,
    }
}

fn technical_api_node_id(
    model: &SemanticModel,
    system: rdra_ish_core::SystemKey,
    api: ApiKey,
) -> String {
    scoped_mermaid_id(
        "api",
        &format!("{}_{}", model.systems[system].id, model.apis[api].id),
    )
}

fn technical_entity_node_id(
    model: &SemanticModel,
    system: rdra_ish_core::SystemKey,
    entity: EntityKey,
) -> String {
    scoped_mermaid_id(
        "entity",
        &format!("{}_{}", model.systems[system].id, model.entities[entity].id),
    )
}

fn scoped_mermaid_id(prefix: &str, raw: &str) -> String {
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

fn is_entity_operation(kind: &RelKind) -> bool {
    matches!(
        kind,
        RelKind::Reads | RelKind::Writes | RelKind::Creates | RelKind::Updates | RelKind::Deletes
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, _) = parse(src);
        let (model, _) = build_model(&ast);
        model
    }

    #[test]
    fn test_rdra_mermaid_emit() {
        let src = r#"
actor Customer "顧客"
usecase Browse "商品を探す"
performs(Customer, Browse)
"#;
        let model = model_from(src);
        let result = RdraMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(result.contains("graph TD"));
        assert!(result.contains("Customer"));
        assert!(result.contains("顧客"));
        assert!(result.contains("Browse"));
        assert!(result.contains("商品を探す"));
        assert!(result.contains("Customer --> Browse"));
    }

    #[test]
    fn test_mermaid_rdra_relation_arrow_labels_and_overview_skips() {
        assert_eq!(
            mermaid_rdra_relation_arrow(&RelKind::Reads, "Browse", "Catalog").as_deref(),
            Some("  Browse -.->|reads| Catalog\n")
        );
        assert_eq!(
            mermaid_rdra_relation_arrow(&RelKind::RelateManyToOne, "Order", "Customer").as_deref(),
            Some("  Order --- Customer\n")
        );
        assert_eq!(
            mermaid_rdra_relation_arrow(&RelKind::Precedes, "ReviewCart", "AuthorizePayment")
                .as_deref(),
            Some("  ReviewCart -->|precedes| AuthorizePayment\n")
        );
        assert_eq!(
            mermaid_rdra_relation_arrow(&RelKind::Branches, "ReviewCart", "PaymentFailed")
                .as_deref(),
            Some("  ReviewCart -.->|branches| PaymentFailed\n")
        );
        assert_eq!(
            mermaid_rdra_relation_arrow(&RelKind::Excepts, "AuthorizePayment", "PaymentFailed")
                .as_deref(),
            Some("  AuthorizePayment -.->|excepts| PaymentFailed\n")
        );
        assert_eq!(
            mermaid_rdra_relation_arrow(&RelKind::Repeats, "PaymentFailed", "ReviewCart")
                .as_deref(),
            Some("  PaymentFailed -.->|repeats| ReviewCart\n")
        );
        assert_eq!(
            mermaid_rdra_relation_arrow(&RelKind::Covers, "AuthorizePayment", "CapturePayment")
                .as_deref(),
            Some("  AuthorizePayment -.->|covers| CapturePayment\n")
        );
        assert_eq!(
            mermaid_rdra_relation_arrow(&RelKind::Compensates, "RefundPayment", "CapturePayment")
                .as_deref(),
            Some("  RefundPayment -.->|compensates| CapturePayment\n")
        );
        assert!(mermaid_rdra_relation_arrow(&RelKind::Transitions, "Draft", "Published").is_none());
        assert!(mermaid_rdra_relation_arrow(&RelKind::Invokes, "Browse", "BrowseApi").is_none());
    }

    #[test]
    fn test_mermaid_rdra_render_helpers_keep_nodes_relations_and_api_out_of_overview() {
        let src = r#"
actor Customer "顧客"
usecase Browse "商品を探す"
api BrowseApi "商品API"
entity Catalog "商品台帳" { id: Int @pk }
performs(Customer, Browse)
reads(Browse, Catalog)
invokes(Browse, BrowseApi)
"#;
        let model = model_from(src);
        let reachable = None;
        let mut declarations = String::new();
        render_mermaid_rdra_node_declarations(
            &mut declarations,
            &model,
            &reachable,
            &View::whole(),
        );
        assert!(declarations.contains("Customer([\"👤 顧客\"])"));
        assert!(declarations.contains("Browse([\"✅ 商品を探す\"])"));
        assert!(declarations.contains("Catalog[(\"🗄️ 商品台帳\")]"));
        assert!(!declarations.contains("BrowseApi"));

        let mut relations = String::new();
        render_mermaid_rdra_relations(&mut relations, &model, &reachable, &View::whole());
        assert!(relations.contains("Customer --> Browse"));
        assert!(relations.contains("Browse -.->|reads| Catalog"));
        assert!(!relations.contains("BrowseApi"));
    }

    #[test]
    fn test_business_area_mermaid_emit() {
        let src = r#"
actor Staff "Staff"
buc BucScheduling "Scheduling"
usecase BookAppointment "Book Appointment"
entity Appointment "Appointment" { id: Int @pk  patient_name: String }
performs(Staff, BucScheduling)
contains(BucScheduling, BookAppointment)
creates(BookAppointment, Appointment)
"#;
        let model = model_from(src);
        let result = BusinessAreaMermaidEmitter
            .emit(&model, &View::whole())
            .unwrap();
        assert!(result.contains("flowchart LR"));
        assert!(result.contains("Business Actor Staff"));
        assert!(result.contains("Appointment.patient_name, create"));
        assert!(result.contains("-->|create| usecase__BookAppointment"));
        assert!(!result.contains("BucScheduling["));
    }

    #[test]
    fn test_business_area_mermaid_disambiguates_actor_and_usecase_ids() {
        let src = r#"
actor Same "Actor Same"
buc BucA "BUC A"
usecase Same "UseCase Same"
entity Thing "Thing" { id: Int @pk  name: String }
performs(actor::Same, BucA)
contains(BucA, usecase::Same)
creates(usecase::Same, Thing)
"#;
        let model = model_from(src);
        let entries = derive_actor_input_inferences(&model);
        let entry = entries.first().unwrap();
        assert_eq!(business_actor_node_id(&model, entry), "actor__Same");
        assert_eq!(business_usecase_node_id(&model, entry), "usecase__Same");

        let result = BusinessAreaMermaidEmitter
            .emit(&model, &View::whole())
            .unwrap();
        assert!(result.contains("actor__Same([\"Business Actor Actor Same\"]"));
        assert!(result.contains("usecase__Same([\"UseCase UseCase Same\"]"));
        assert!(result.contains("actor__Same --> input__Same_Same_Thing_name"));
        assert!(result.contains("input__Same_Same_Thing_name -->|create| usecase__Same"));
    }

    #[test]
    fn test_technical_area_mermaid_emit() {
        let src = r#"
system SchedulingSystem "Scheduling System"
api BookingApi "Booking API"
entity Appointment "Appointment" { id: Int @pk }
contains(SchedulingSystem, BookingApi)
creates(BookingApi, Appointment)
"#;
        let model = model_from(src);
        let result = TechnicalAreaMermaidEmitter
            .emit(&model, &View::whole())
            .unwrap();
        assert!(result.contains("subgraph system__SchedulingSystem[Scheduling System]"));
        assert!(result.contains("API Booking API"));
        assert!(result.contains("Entity Appointment"));
        assert!(result.contains("-.->|creates|"));
    }

    #[test]
    fn test_object_graph_mermaid_layers() {
        let src = r#"
actor Customer "顧客"
buc BucOrder "注文業務"
usecase PlaceOrder "注文する"
screen OrderScreen "注文画面"
api OrderApi "注文API"
entity Order "注文" { id: Int @pk }
state Draft "下書き"
performs(Customer, BucOrder)
contains(BucOrder, PlaceOrder)
displays(PlaceOrder, OrderScreen)
invokes(PlaceOrder, OrderApi)
creates(OrderApi, Order)
"#;
        let model = model_from(src);
        let result = ObjectGraphMermaidEmitter
            .emit(&model, &View::whole())
            .unwrap();
        assert!(result.contains("flowchart LR"));
        assert!(result.contains("subgraph layer_value[System Value]"));
        assert!(result.contains("subgraph layer_environment[External Environment]"));
        assert!(result.contains("subgraph layer_boundary[System Boundary]"));
        assert!(result.contains("subgraph layer_system[System]"));
        assert!(result.contains("Customer([\"👤 顧客\"])"));
        assert!(result.contains("BucOrder[\"📦 注文業務\"]"));
        assert!(result.contains("OrderScreen[[\"🖥️ 注文画面\"]]"));
        assert!(result.contains("OrderApi[\"🔌 注文API\"]"));
        assert!(result.contains("Order[(\"🗄️ 注文\")]"));
        let boundary_pos = result
            .find("subgraph layer_boundary[System Boundary]")
            .unwrap();
        let system_pos = result.find("subgraph layer_system[System]").unwrap();
        let screen_pos = result.find("OrderScreen[[\"🖥️ 注文画面\"]]").unwrap();
        let api_pos = result.find("OrderApi[\"🔌 注文API\"]").unwrap();
        assert!(boundary_pos < screen_pos);
        assert!(screen_pos < system_pos);
        assert!(system_pos < api_pos);
        assert!(result.contains("Customer -->|performs| BucOrder"));
        assert!(result.contains("PlaceOrder -.->|invokes| OrderApi"));
        assert!(result.contains("OrderApi -.->|creates| Order"));
    }

    #[test]
    fn test_object_graph_mermaid_applies_node_and_edge_filters() {
        let src = r#"
actor Customer "Customer"
usecase PlaceOrder "Place order"
api OrderApi "Order API"
entity Order "Order" { id: Int @pk }
performs(Customer, PlaceOrder)
invokes(PlaceOrder, OrderApi)
creates(OrderApi, Order)
"#;
        let model = model_from(src);
        let view = View::whole().with_graph_filters(
            vec!["usecase".to_string(), "api".to_string()],
            vec!["invokes".to_string()],
        );

        let result = ObjectGraphMermaidEmitter.emit(&model, &view).unwrap();

        assert!(result.contains("PlaceOrder([\"✅ Place order\"])"));
        assert!(result.contains("OrderApi[\"🔌 Order API\"]"));
        assert!(result.contains("PlaceOrder -.->|invokes| OrderApi"));
        assert!(!result.contains("Customer(["));
        assert!(!result.contains("Order[(\""));
        assert!(!result.contains("performs"));
        assert!(!result.contains("creates"));
    }

    #[test]
    fn test_er_mermaid_emit() {
        let src = r#"
entity Order "注文" { id: Int @pk  total: Money }
entity Customer "顧客" { id: Int @pk  name: String }
relate(Order, Customer, "N:1")
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
    fn test_er_mermaid_snapshot() {
        let src = r#"
entity Customer "顧客" { id: Int @pk  name: String }
entity Order "注文" { id: Int @pk  total: Money }
relate(Order, Customer, "N:1")
"#;
        let (ast, _) = parse(src);
        let (model, _) = build_model(&ast);
        let result = ErMermaidEmitter.emit(&model, &View::er()).unwrap();
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_state_mermaid_emit() {
        // transitions の引数順は (event, from, to)
        let src = r#"
state Draft "下書き"
state Published "公開"
event Publish "公開する"
transitions(Publish, Draft, Published)
"#;
        let model = model_from(src);
        let result = StateMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(result.contains("stateDiagram-v2"));
        assert!(result.contains("[*] --> Draft"));
        assert!(result.contains("Draft --> Published"));
        assert!(result.contains("公開する"));
    }

    #[test]
    fn test_event_flow_mermaid_renders_triggered_buc() {
        let src = r#"
buc BucBillingClaims "Billing Claims"
usecase SignEncounter "Sign Encounter"
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
triggers(EncounterSigned, BucBillingClaims)
"#;
        let model = model_from(src);
        let result = EventFlowMermaidEmitter
            .emit(&model, &View::whole())
            .unwrap();

        assert!(result.contains("ev__EncounterSigned -.->|triggers| buc__BucBillingClaims"));
        assert!(result.contains("buc__BucBillingClaims[[\"📦 Billing Claims\"]]"));
    }

    #[test]
    fn test_mermaid_event_flow_helpers_deduplicate_declarations_and_render_transitions() {
        let src = r#"
usecase SignEncounter "Sign Encounter"
usecase ReviewClaim "Review Claim"
buc BucBillingClaims "Billing Claims"
state Pending "Pending"
state Signed "Signed"
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
triggers(EncounterSigned, ReviewClaim)
triggers(EncounterSigned, BucBillingClaims)
transitions(EncounterSigned, Pending, Signed)
"#;
        let model = model_from(src);
        let flows = rdra_ish_core::collect_event_flows(&model);
        let flow = flows
            .iter()
            .find(|flow| {
                model
                    .events
                    .get(flow.event)
                    .map(|event| event.id == "EncounterSigned")
                    .unwrap_or(false)
            })
            .unwrap();
        let mut out = String::new();
        let mut declared = HashSet::new();

        render_mermaid_event_flow(&mut out, &mut declared, &model, &None, flow);
        render_mermaid_event_flow(&mut out, &mut declared, &model, &None, flow);

        assert_eq!(
            out.matches("ev__EncounterSigned{\"⚡ Encounter Signed\"}")
                .count(),
            1
        );
        assert!(out.contains("uc__SignEncounter -.->|raises| ev__EncounterSigned"));
        assert!(out.contains("ev__EncounterSigned -.->|triggers| uc__ReviewClaim"));
        assert!(out.contains("ev__EncounterSigned -.->|triggers| buc__BucBillingClaims"));
        assert!(out.contains("st__Pending -->|⚡ Encounter Signed| st__Signed"));
    }

    #[test]
    fn test_mermaid_event_flow_declare_helpers_emit_prefixed_nodes_once() {
        let src = r#"
usecase SignEncounter "Sign Encounter"
buc BucBillingClaims "Billing Claims"
state Pending "Pending"
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
"#;
        let model = model_from(src);
        let flow = rdra_ish_core::collect_event_flows(&model).pop().unwrap();
        let usecase = model
            .use_cases
            .iter()
            .find(|(_, usecase)| usecase.id == "SignEncounter")
            .map(|(key, _)| key)
            .unwrap();
        let buc = model
            .bucs
            .iter()
            .find(|(_, buc)| buc.id == "BucBillingClaims")
            .map(|(key, _)| key)
            .unwrap();
        let state = model
            .states
            .iter()
            .find(|(_, state)| state.id == "Pending")
            .map(|(key, _)| key)
            .unwrap();
        let mut out = String::new();
        let mut declared = HashSet::new();

        assert_eq!(
            declare_mermaid_event_flow_event(&mut out, &mut declared, &model, &flow).as_deref(),
            Some("ev__EncounterSigned")
        );
        assert_eq!(
            declare_mermaid_event_flow_usecase(&mut out, &mut declared, &model, usecase).as_deref(),
            Some("uc__SignEncounter")
        );
        assert_eq!(
            declare_mermaid_event_flow_buc(&mut out, &mut declared, &model, buc).as_deref(),
            Some("buc__BucBillingClaims")
        );
        assert_eq!(mermaid_event_flow_state_id("Pending"), "st__Pending");
        assert_eq!(
            declare_mermaid_event_flow_state(&mut out, &mut declared, &model, state).as_deref(),
            Some("st__Pending")
        );

        assert!(out.contains("ev__EncounterSigned{\"⚡ Encounter Signed\"}"));
        assert!(out.contains("uc__SignEncounter([\"✅ Sign Encounter\"])"));
        assert!(out.contains("buc__BucBillingClaims[[\"📦 Billing Claims\"]]"));
        assert!(out.contains("st__Pending(\"🔄 Pending\")"));
    }

    #[test]
    fn test_mermaid_event_flow_visibility_skips_hidden_event_scope() {
        let src = r#"
usecase SignEncounter "Sign Encounter"
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
"#;
        let model = model_from(src);
        let flow = rdra_ish_core::collect_event_flows(&model).pop().unwrap();
        let mut out = String::new();
        let mut declared = HashSet::new();
        let reachable = Some(HashSet::new());

        assert_eq!(
            mermaid_event_flow_event_id("EncounterSigned"),
            "ev__EncounterSigned"
        );
        assert_eq!(
            mermaid_event_flow_usecase_id("SignEncounter"),
            "uc__SignEncounter"
        );
        render_mermaid_event_flow(&mut out, &mut declared, &model, &reachable, &flow);

        assert!(out.is_empty());
        assert!(declared.is_empty());
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
            scope: crate::Scope::Bucs(vec!["BucA".to_string()]),
            filter: crate::Filter::Er,
            show_descriptions: false,
            node_kinds: Vec::new(),
            edge_kinds: Vec::new(),
        };
        let result = ErMermaidEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("EntityA"), "EntityA should be included");
        assert!(!result.contains("EntityB"), "EntityB should be excluded");
    }

    #[test]
    fn test_mermaid_show_description_renders_annotations() {
        let src = r#"
actor Customer "Customer" description "Places orders"
usecase Browse "Browse" description "Finds products"
performs(Customer, Browse)
"#;
        let model = model_from(src);
        let view = View::whole().with_descriptions(true);

        let result = RdraMermaidEmitter.emit(&model, &view).unwrap();

        assert!(result.contains("Customer_description[\"Places orders\"]"));
        assert!(result.contains("Customer_description -. description .- Customer"));
        assert!(result.contains("Browse_description[\"Finds products\"]"));
        assert!(result.contains("Browse_description -. description .- Browse"));
    }

    #[test]
    fn test_sequence_mermaid_buc_filter_excludes_triggered_buc() {
        let src = r#"
actor Customer "顧客"
actor Clerk "担当者"
buc BucA "BUC-A"
buc BucB "BUC-B"
usecase UcA "ユースケースA"
usecase UcB "ユースケースB"
event EvA "イベントA"
api ApiA "API-A"
api ApiB "API-B"
entity EntityA "エンティティA" { id: Int @pk }
entity EntityB "エンティティB" { id: Int @pk }
performs(Customer, BucA)
performs(Clerk, UcB)
contains(BucA, UcA)
invokes(UcA, ApiA)
creates(ApiA, EntityA)
raises(UcA, EvA)
performs(Customer, BucB)
contains(BucB, UcB)
invokes(UcB, ApiB)
creates(ApiB, EntityB)
triggers(EvA, UcB)
"#;
        let model = model_from(src);
        let result = SequenceMermaidEmitter
            .emit(&model, &View::bucs(vec!["BucA".to_string()]))
            .unwrap();
        assert!(result.contains("ユースケースA"));
        assert!(result.contains("ApiA"));
        assert!(!result.contains("ユースケースB"));
        assert!(!result.contains("ApiB"));
    }

    #[test]
    fn test_sequence_mermaid_usecase_filter() {
        let src = r#"
actor Customer "顧客"
actor Clerk "担当者"
buc BucA "BUC-A"
usecase UcA "ユースケースA"
usecase UcB "ユースケースB"
api ApiA "API-A"
api ApiB "API-B"
entity EntityA "エンティティA" { id: Int @pk }
entity EntityB "エンティティB" { id: Int @pk }
performs(Customer, BucA)
performs(Clerk, UcB)
contains(BucA, UcA)
contains(BucA, UcB)
invokes(UcA, ApiA)
creates(ApiA, EntityA)
invokes(UcB, ApiB)
creates(ApiB, EntityB)
"#;
        let model = model_from(src);
        let result = SequenceMermaidEmitter
            .emit(&model, &View::usecases(vec!["UcB".to_string()]))
            .unwrap();
        assert!(!result.contains("ユースケースA"));
        assert!(!result.contains("ApiA"));
        assert!(!result.contains("actor Customer"));
        assert!(result.contains("actor Clerk"));
        assert!(result.contains("ユースケースB"));
        assert!(result.contains("ApiB"));
    }

    #[test]
    fn test_sequence_mermaid_read_only_usecase() {
        let src = r#"
actor Customer "顧客"
buc BucA "BUC-A"
usecase Search "検索"
api SearchApi "検索API"
entity Item "品目" { id: Int @pk }
screen SearchScreen "検索画面"
performs(Customer, Search)
contains(BucA, Search)
displays(Search, SearchScreen)
invokes(Search, SearchApi)
reads(SearchApi, Item)
"#;
        let model = model_from(src);
        let result = SequenceMermaidEmitter
            .emit(&model, &View::usecases(vec!["Search".to_string()]))
            .unwrap();
        assert!(result.contains("box System Value"));
        assert!(result.contains("box System Boundary"));
        assert!(result.contains("box System"));
        assert!(result.contains("actor Customer"));
        assert!(result.contains("participant SearchApi"));
        assert!(result.contains("participant Item"));
        assert!(result.contains("SearchApi->>Item: read"));
        let boundary_box_pos = result.find("  box System Boundary\n").unwrap();
        let system_box_pos = result.find("  box System\n").unwrap();
        let screen_pos = result.find("participant SearchScreen").unwrap();
        let api_pos = result.find("participant SearchApi").unwrap();
        let entity_pos = result.find("participant Item").unwrap();
        assert!(boundary_box_pos < screen_pos);
        assert!(screen_pos < system_box_pos);
        assert!(system_box_pos < api_pos);
        assert!(screen_pos < api_pos);
        assert!(api_pos < entity_pos);
        assert!(!result.contains("no sequenceable usecases"));
    }
}
