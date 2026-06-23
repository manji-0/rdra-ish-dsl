//! PlantUML emitters: RDRA全体図、BUC別図、ER図、状態遷移図、sequence図。

use crate::{
    collect_object_graph_nodes, node_description, object_graph_layer, object_graph_rel_label,
    prefixed_label, prefixed_node_label, view_node_visible, view_relation_visible, EmitError,
    Emitter, Scope, View, OBJECT_GRAPH_LAYERS,
};
use rdra_ish_core::model::{
    ActorKey, ApiKey, BucKey, ColumnType, EntityKey, NodeRef, RelKind, ScreenKey, SemanticModel,
    StateKey, UseCaseKey,
};
use rdra_ish_core::tx::infer_usecase_transactions;
use rdra_ish_core::{
    derive_actor_input_inferences, derive_system_boundaries, ActorInputInference, EventFlow,
};
use std::collections::{HashMap, HashSet};

// ── RDRA全体図エミッタ ────────────────────────────────────────────────────────

pub struct RdraPlantUmlEmitter;

fn graph_node_visible(reachable: &Option<HashSet<NodeRef>>, view: &View, node: &NodeRef) -> bool {
    scoped_node_visible(reachable, node) && view_node_visible(view, node)
}

fn render_rdra_node_declarations(
    out: &mut String,
    model: &SemanticModel,
    reachable: &Option<HashSet<NodeRef>>,
    view: &View,
) {
    let mut actor_ids: Vec<_> = model.actors.iter().collect();
    actor_ids.sort_by_key(|(_, actor)| &actor.id);
    for (key, actor) in &actor_ids {
        let node = NodeRef::Actor(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "actor \"{}\" as {}\n",
                prefixed_node_label(&node, &actor.label),
                actor.id
            ));
        }
    }
    out.push('\n');

    let mut usecase_ids: Vec<_> = model.use_cases.iter().collect();
    usecase_ids.sort_by_key(|(_, usecase)| &usecase.id);
    for (key, usecase) in &usecase_ids {
        let node = NodeRef::UseCase(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "usecase \"{}\" as {}\n",
                prefixed_node_label(&node, &usecase.label),
                usecase.id
            ));
        }
    }
    out.push('\n');

    let mut buc_ids: Vec<_> = model.bucs.iter().collect();
    buc_ids.sort_by_key(|(_, buc)| &buc.id);
    for (key, buc) in &buc_ids {
        let node = NodeRef::Buc(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "rectangle \"{}\" as {}\n",
                prefixed_node_label(&node, &buc.label),
                buc.id
            ));
        }
    }
    out.push('\n');

    let mut flow_ids: Vec<_> = model.flows.iter().collect();
    flow_ids.sort_by_key(|(_, flow)| &flow.id);
    for (key, flow) in &flow_ids {
        let node = NodeRef::Flow(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "rectangle \"{}\" as {}\n",
                prefixed_node_label(&node, &flow.label),
                flow.id
            ));
        }
    }
    out.push('\n');

    let mut step_ids: Vec<_> = model.steps.iter().collect();
    step_ids.sort_by_key(|(_, step)| &step.id);
    for (key, step) in &step_ids {
        let node = NodeRef::Step(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "usecase \"{}\" as {}\n",
                prefixed_node_label(&node, &step.label),
                step.id
            ));
        }
    }
    out.push('\n');

    let mut system_ids: Vec<_> = model.systems.iter().collect();
    system_ids.sort_by_key(|(_, system)| &system.id);
    for (key, system) in &system_ids {
        let node = NodeRef::System(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "package \"{}\" as {}\n",
                prefixed_node_label(&node, &system.label),
                system.id
            ));
        }
    }
    out.push('\n');

    let mut ext_ids: Vec<_> = model.ext_systems.iter().collect();
    ext_ids.sort_by_key(|(_, ext_system)| &ext_system.id);
    for (key, ext_system) in &ext_ids {
        let node = NodeRef::ExtSystem(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "component \"{}\" as {}\n",
                prefixed_node_label(&node, &ext_system.label),
                ext_system.id
            ));
        }
    }
    out.push('\n');

    let mut entity_ids: Vec<_> = model.entities.iter().collect();
    entity_ids.sort_by_key(|(_, entity)| &entity.id);
    for (key, entity) in &entity_ids {
        let node = NodeRef::Entity(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "database \"{}\" as {}\n",
                prefixed_node_label(&node, &entity.label),
                entity.id
            ));
        }
    }
    out.push('\n');

    let mut screen_ids: Vec<_> = model.screens.iter().collect();
    screen_ids.sort_by_key(|(_, screen)| &screen.id);
    for (key, screen) in &screen_ids {
        let node = NodeRef::Screen(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "boundary \"{}\" as {}\n",
                prefixed_node_label(&node, &screen.label),
                screen.id
            ));
        }
    }
    out.push('\n');

    let mut field_ids: Vec<_> = model.fields.iter().collect();
    field_ids.sort_by_key(|(_, field)| &field.id);
    for (key, field) in &field_ids {
        let node = NodeRef::Field(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "boundary \"{}\" as {}\n",
                prefixed_node_label(&node, &field.label),
                field.id
            ));
        }
    }
    out.push('\n');

    let mut event_ids: Vec<_> = model.events.iter().collect();
    event_ids.sort_by_key(|(_, event)| &event.id);
    for (key, event) in &event_ids {
        let node = NodeRef::Event(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "control \"{}\" as {}\n",
                prefixed_node_label(&node, &event.label),
                event.id
            ));
        }
    }
    out.push('\n');

    let mut state_ids: Vec<_> = model.states.iter().collect();
    state_ids.sort_by_key(|(_, state)| &state.id);
    for (key, state) in &state_ids {
        let node = NodeRef::State(*key);
        if graph_node_visible(reachable, view, &node) {
            out.push_str(&format!(
                "collections \"{}\" as {}\n",
                prefixed_node_label(&node, &state.label),
                state.id
            ));
        }
    }
    out.push('\n');
}

fn rdra_relation_arrow(kind: &RelKind, from_id: &str, to_id: &str) -> Option<String> {
    let arrow = match kind {
        RelKind::Performs => format!("{} --> {}", from_id, to_id),
        RelKind::Uses => format!("{} --> {}", from_id, to_id),
        RelKind::Reads => format!("{} ..> {} : reads", from_id, to_id),
        RelKind::Writes => format!("{} ..> {} : writes", from_id, to_id),
        RelKind::Creates => format!("{} ..> {} : creates", from_id, to_id),
        RelKind::Updates => format!("{} ..> {} : updates", from_id, to_id),
        RelKind::Deletes => format!("{} ..> {} : deletes", from_id, to_id),
        RelKind::Displays => format!("{} ..> {} : displays", from_id, to_id),
        RelKind::Shows => format!("{} ..> {} : shows", from_id, to_id),
        RelKind::Raises => format!("{} ..> {} : raises", from_id, to_id),
        RelKind::Triggers => format!("{} ..> {} : triggers", from_id, to_id),
        RelKind::Contains => format!("{} ..> {} : contains", from_id, to_id),
        RelKind::Belongs => format!("{} ..> {} : belongs", from_id, to_id),
        RelKind::HasPermission => format!("{} ..> {} : has_permission", from_id, to_id),
        RelKind::RequiresPermission => format!("{} ..> {} : requires_permission", from_id, to_id),
        RelKind::RequiresMedium => format!("{} ..> {} : requires_medium", from_id, to_id),
        RelKind::Motivates => format!("{} ..> {} : motivates", from_id, to_id),
        RelKind::Decides => format!("{} ..> {} : decides", from_id, to_id),
        RelKind::Precedes => format!("{} --> {} : precedes", from_id, to_id),
        RelKind::Branches => format!("{} ..> {} : branches", from_id, to_id),
        RelKind::Excepts => format!("{} ..> {} : excepts", from_id, to_id),
        RelKind::Repeats => format!("{} ..> {} : repeats", from_id, to_id),
        RelKind::Covers => format!("{} ..> {} : covers", from_id, to_id),
        RelKind::Compensates => format!("{} ..> {} : compensates", from_id, to_id),
        RelKind::Request => format!("{} ..> {} : request", from_id, to_id),
        RelKind::Response => format!("{} ..> {} : response", from_id, to_id),
        RelKind::ErrorResponse => format!("{} ..> {} : error_response", from_id, to_id),
        RelKind::AppliesTo => format!("{} ..> {} : applies_to", from_id, to_id),
        RelKind::Qualifies => format!("{} ..> {} : qualifies", from_id, to_id),
        RelKind::Constrains => format!("{} ..> {} : constrains", from_id, to_id),
        RelKind::MapsTo => format!("{} ..> {} : maps_to", from_id, to_id),
        RelKind::MapsField => format!("{} ..> {} : maps_field", from_id, to_id),
        RelKind::Owns => format!("{} --> {} : owns", from_id, to_id),
        RelKind::Transitions | RelKind::Invokes => return None,
        RelKind::RelateOneToOne
        | RelKind::RelateOneToMany
        | RelKind::RelateManyToOne
        | RelKind::RelateManyToMany => format!("{} -- {}", from_id, to_id),
    };
    Some(arrow)
}

fn render_rdra_relations(
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
            if let Some(arrow) = rdra_relation_arrow(&relation.kind, from_id, to_id) {
                out.push_str(&arrow);
                out.push('\n');
            }
        }
    }
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

fn render_plantuml_description_notes(out: &mut String, model: &SemanticModel, nodes: &[NodeRef]) {
    for node in nodes {
        let (Some(id), Some(description)) = (node_id(model, node), node_description(model, node))
        else {
            continue;
        };
        let description = description.trim();
        if description.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "note right of {}\n{}\nend note\n",
            id,
            plantuml_label(description)
        ));
    }
}

impl Emitter for RdraPlantUmlEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let reachable = reachable_for_scope(model, &view.scope);
        let mut out = String::new();
        out.push_str("@startuml\n");
        out.push_str("!theme plain\n");
        out.push('\n');
        render_rdra_node_declarations(&mut out, model, &reachable, view);
        render_rdra_relations(&mut out, model, &reachable, view);
        if view.show_descriptions {
            let nodes: Vec<_> = rdra_description_nodes(model, &reachable)
                .into_iter()
                .filter(|node| view_node_visible(view, node))
                .collect();
            render_plantuml_description_notes(&mut out, model, &nodes);
        }
        out.push_str("@enduml\n");
        Ok(out)
    }
}

// ── RDRA レイヤ図エミッタ ────────────────────────────────────────────────────

pub struct ObjectGraphPlantUmlEmitter;

impl Emitter for ObjectGraphPlantUmlEmitter {
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
        out.push_str("@startuml\n");
        out.push_str("!theme plain\n");
        out.push_str("left to right direction\n\n");

        for layer in OBJECT_GRAPH_LAYERS {
            out.push_str(&format!("rectangle \"{}\" {{\n", layer.label()));
            for nr in visible_nodes
                .iter()
                .filter(|nr| object_graph_layer(nr) == layer)
            {
                if let (Some(id), Some(label)) = (node_id(model, nr), node_label(model, nr)) {
                    let label = prefixed_node_label(nr, label);
                    let line = match nr {
                        NodeRef::Actor(_) => format!("  actor \"{}\" as {}\n", label, id),
                        NodeRef::Requirement(_) => format!("  rectangle \"{}\" as {}\n", label, id),
                        NodeRef::Adr(_) => format!("  rectangle \"{}\" as {}\n", label, id),
                        NodeRef::Nfr(_) => format!("  rectangle \"{}\" as {}\n", label, id),
                        NodeRef::ExtSystem(_) => format!("  component \"{}\" as {}\n", label, id),
                        NodeRef::Quality(_) => format!("  rectangle \"{}\" as {}\n", label, id),
                        NodeRef::Constraint(_) => format!("  rectangle \"{}\" as {}\n", label, id),
                        NodeRef::Concept(_) => format!("  rectangle \"{}\" as {}\n", label, id),
                        NodeRef::DomainObject(_) => {
                            format!("  rectangle \"{}\" as {}\n", label, id)
                        }
                        NodeRef::Aggregate(_) => format!("  rectangle \"{}\" as {}\n", label, id),
                        NodeRef::ValueObject(_) => {
                            format!("  rectangle \"{}\" as {}\n", label, id)
                        }
                        NodeRef::Business(_) => format!("  rectangle \"{}\" as {}\n", label, id),
                        NodeRef::Buc(_) => format!("  rectangle \"{}\" as {}\n", label, id),
                        NodeRef::Flow(_) => format!("  rectangle \"{}\" as {}\n", label, id),
                        NodeRef::Step(_) => format!("  usecase \"{}\" as {}\n", label, id),
                        NodeRef::UsageScene(_) => format!("  usecase \"{}\" as {}\n", label, id),
                        NodeRef::Condition(_) => format!("  rectangle \"{}\" as {}\n", label, id),
                        NodeRef::Variation(_) => format!("  rectangle \"{}\" as {}\n", label, id),
                        NodeRef::Location(_) => format!("  rectangle \"{}\" as {}\n", label, id),
                        NodeRef::Timing(_) => format!("  rectangle \"{}\" as {}\n", label, id),
                        NodeRef::Medium(_) => format!("  component \"{}\" as {}\n", label, id),
                        NodeRef::Permission(_) => format!("  rectangle \"{}\" as {}\n", label, id),
                        NodeRef::UseCase(_) => format!("  usecase \"{}\" as {}\n", label, id),
                        NodeRef::Screen(_) => format!("  boundary \"{}\" as {}\n", label, id),
                        NodeRef::Field(_) => format!("  boundary \"{}\" as {}\n", label, id),
                        NodeRef::Event(_) => format!("  control \"{}\" as {}\n", label, id),
                        NodeRef::Api(_) => format!("  control \"{}\" as {}\n", label, id),
                        NodeRef::Dto(_) => format!("  artifact \"{}\" as {}\n", label, id),
                        NodeRef::System(_) => format!("  package \"{}\" as {}\n", label, id),
                        NodeRef::Entity(_) => format!("  database \"{}\" as {}\n", label, id),
                        NodeRef::State(_) => format!("  collections \"{}\" as {}\n", label, id),
                    };
                    out.push_str(&line);
                }
            }
            out.push_str("}\n\n");
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
                        format!("{} --> {} : {}\n", from_id, to_id, label)
                    }
                    RelKind::RelateOneToOne
                    | RelKind::RelateOneToMany
                    | RelKind::RelateManyToOne
                    | RelKind::RelateManyToMany => {
                        format!("{} -- {} : {}\n", from_id, to_id, label)
                    }
                    _ => format!("{} ..> {} : {}\n", from_id, to_id, label),
                };
                out.push_str(&line);
            }
        }

        if view.show_descriptions {
            render_plantuml_description_notes(&mut out, model, &visible_nodes);
        }

        out.push_str("@enduml\n");
        Ok(out)
    }
}

// ── Business area diagram ────────────────────────────────────────────────────

pub struct BusinessAreaPlantUmlEmitter;

impl Emitter for BusinessAreaPlantUmlEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let mut out = String::new();
        out.push_str("@startuml\n");
        out.push_str("!theme plain\n");
        out.push_str("left to right direction\n\n");

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
            let input_label = plantuml_label(&format!(
                "{}.{}, {}",
                entity.id,
                entry.column,
                entry.operation.as_str()
            ));

            if nodes.insert(actor_id.clone()) {
                out.push_str(&format!(
                    "actor \"{}\" as {}\n",
                    plantuml_label(&prefixed_label("Business Actor", &actor.label)),
                    actor_id
                ));
            }
            if nodes.insert(input_id.clone()) {
                out.push_str(&format!("rectangle \"{}\" as {}\n", input_label, input_id));
            }
            if nodes.insert(usecase_id.clone()) {
                out.push_str(&format!(
                    "usecase \"{}\" as {}\n",
                    plantuml_label(&prefixed_label("UseCase", &usecase.label)),
                    usecase_id
                ));
            }

            let actor_edge = format!("{} --> {}\n", actor_id, input_id);
            if edges.insert(actor_edge.clone()) {
                out.push_str(&actor_edge);
            }
            let uc_edge = format!(
                "{} --> {} : {}\n",
                input_id,
                usecase_id,
                entry.operation.as_str()
            );
            if edges.insert(uc_edge.clone()) {
                out.push_str(&uc_edge);
            }
        }

        out.push_str("@enduml\n");
        Ok(out)
    }
}

// ── Technical area diagram ───────────────────────────────────────────────────

pub struct TechnicalAreaPlantUmlEmitter;

impl Emitter for TechnicalAreaPlantUmlEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let reachable = reachable_for_scope(model, &view.scope);
        let mut out = String::new();
        out.push_str("@startuml\n");
        out.push_str("!theme plain\n");
        out.push_str("left to right direction\n\n");

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

            out.push_str(&format!(
                "package \"{}\" {{\n",
                plantuml_label(&system.label)
            ));
            for api in &apis {
                let api_model = &model.apis[*api];
                out.push_str(&format!(
                    "  control \"{}\" as {}\n",
                    plantuml_label(&prefixed_label("API", &api_model.label)),
                    technical_api_node_id(model, boundary.system, *api)
                ));
            }
            for entity in &entities {
                let entity_model = &model.entities[*entity];
                out.push_str(&format!(
                    "  database \"{}\" as {}\n",
                    plantuml_label(&prefixed_label("Entity", &entity_model.label)),
                    technical_entity_node_id(model, boundary.system, *entity)
                ));
            }
            out.push_str("}\n\n");

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
                    "{} ..> {} : {}\n",
                    technical_api_node_id(model, boundary.system, *api),
                    technical_entity_node_id(model, boundary.system, *entity),
                    object_graph_rel_label(&rel.kind)
                ));
            }
        }

        out.push_str("@enduml\n");
        Ok(out)
    }
}

// ── 状態遷移図エミッタ ─────────────────────────────────────────────────────────

pub struct StateDiagramEmitter;

impl Emitter for StateDiagramEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        // BUCフィルタ: Scope::Bucs の場合は到達可能ノードのみに絞る
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

        // state_transitions は完全な (event, from, to) 三つ組
        // BUCフィルタ適用: from/to が両方 visible な遷移のみ
        let transitions: Vec<_> = model
            .state_transitions
            .iter()
            .filter(|t| is_state_visible(t.from) && is_state_visible(t.to))
            .collect();

        if transitions.is_empty() {
            return Ok("@startuml\n@enduml\n".to_string());
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
        out.push_str("@startuml\n");

        for initial in &initial_states {
            if let Some(id) = node_id(model, &NodeRef::State(*initial)) {
                out.push_str(&format!("[*] --> {}\n", id));
            }
        }

        // 遷移（event, from, to の三つ組をそのまま出力）
        let mut sorted: Vec<_> = transitions.iter().collect();
        sorted.sort_by_key(|t| {
            format!(
                "{}{}{}",
                node_id(model, &NodeRef::State(t.from)).unwrap_or(""),
                node_id(model, &NodeRef::State(t.to)).unwrap_or(""),
                node_id(model, &NodeRef::Event(t.event)).unwrap_or(""),
            )
        });

        let mut defined: HashSet<String> = HashSet::new();
        for t in &sorted {
            for sk in [t.from, t.to] {
                let nr = NodeRef::State(sk);
                if let (Some(id), Some(label)) = (node_id(model, &nr), node_label(model, &nr)) {
                    if defined.insert(id.to_string()) {
                        out.push_str(&format!(
                            "state \"{}\" as {}\n",
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
                    "{} --> {} : {}\n",
                    from_id,
                    to_id,
                    prefixed_node_label(&event_nr, ev_label)
                ));
            }
        }

        out.push_str("@enduml\n");
        Ok(out)
    }
}

// ── ER図エミッタ ──────────────────────────────────────────────────────────────

pub struct ErPlantUmlEmitter;

impl Emitter for ErPlantUmlEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        // BUCフィルタ: Scope::Bucs の場合は到達可能ノードのみに絞る
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
        out.push_str("@startuml\n");
        out.push_str("!theme plain\n");
        out.push('\n');

        // entities
        let mut ents: Vec<_> = model.entities.iter().collect();
        ents.sort_by_key(|(_, e)| &e.id);

        for (k, ent) in &ents {
            let nr = NodeRef::Entity(*k);
            if !is_visible(&nr) {
                continue;
            }
            out.push_str(&format!(
                "entity \"{}\" as {} {{\n",
                prefixed_node_label(&nr, &ent.label),
                ent.id
            ));

            // PKs first
            let pks: Vec<_> = ent.columns.iter().filter(|c| c.is_pk).collect();
            for col in &pks {
                let type_str = col_type_str(&col.col_type);
                out.push_str(&format!("  *{} : {} <<PK>>\n", col.name, type_str));
            }

            // Separator
            if !pks.is_empty() {
                out.push_str("  --\n");
            }

            // Non-PK columns
            for col in ent.columns.iter().filter(|c| !c.is_pk) {
                let type_str = col_type_str(&col.col_type);
                if col.is_fk {
                    out.push_str(&format!("  {} : {} <<FK>>\n", col.name, type_str));
                } else {
                    out.push_str(&format!("  {} : {}\n", col.name, type_str));
                }
            }

            out.push_str("}\n");
        }

        out.push('\n');

        // ER relations (relate only)
        // Collect entity id → key mapping
        let entity_key_map: std::collections::HashMap<&str, EntityKey> = model
            .entities
            .iter()
            .map(|(k, e)| (e.id.as_str(), k))
            .collect();

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
                let _ = entity_key_map; // suppress unused warning
                let line = match &rel.kind {
                    RelKind::RelateManyToOne => {
                        format!("{} }}o--|| {} : \"\"\n", from_id, to_id)
                    }
                    RelKind::RelateOneToMany => {
                        format!("{} ||--o{{ {} : \"\"\n", from_id, to_id)
                    }
                    RelKind::RelateOneToOne => {
                        format!("{} ||--|| {} : \"\"\n", from_id, to_id)
                    }
                    RelKind::RelateManyToMany => {
                        format!("{} }}o--o{{ {} : \"\"\n", from_id, to_id)
                    }
                    _ => continue,
                };
                out.push_str(&line);
            }
        }

        out.push_str("@enduml\n");
        Ok(out)
    }
}

// ── sequence図エミッタ ────────────────────────────────────────────────────────

/// 書き込み系ユースケースのシーケンス図を生成する。
///
/// FK連結成分を `group transaction (inferred from FK)` で囲み、
/// FK非連結の孤立書き込みには `note right` で診断ヒントを付ける。
/// `--buc` による絞り込み（`Scope::Bucs`）に対応。
pub struct SequenceDiagramEmitter;

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

impl Emitter for SequenceDiagramEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let scoped_usecases = sequence_usecase_scope(model, &view.scope);
        let is_visible_usecase = |key: UseCaseKey| -> bool {
            match &scoped_usecases {
                Some(set) => set.contains(&key),
                None => true,
            }
        };

        // ── アクター解決マップ構築 ─────────────────────────────────────────
        let mut uc_to_bucs: HashMap<UseCaseKey, Vec<BucKey>> = HashMap::new();
        let mut buc_to_actors: HashMap<BucKey, Vec<ActorKey>> = HashMap::new();
        let mut uc_to_screens: HashMap<UseCaseKey, Vec<ScreenKey>> = HashMap::new();
        let mut uc_to_apis: HashMap<UseCaseKey, Vec<ApiKey>> = HashMap::new();
        let mut uc_to_reads: HashMap<UseCaseKey, Vec<EntityKey>> = HashMap::new();
        let mut api_to_reads: HashMap<ApiKey, Vec<EntityKey>> = HashMap::new();

        for rel in &model.relations {
            match &rel.kind {
                RelKind::Contains => {
                    if let (NodeRef::Buc(bk), NodeRef::UseCase(uk)) = (&rel.from, &rel.to) {
                        uc_to_bucs.entry(*uk).or_default().push(*bk);
                    }
                }
                RelKind::Performs => {
                    if let (NodeRef::Actor(ak), NodeRef::Buc(bk)) = (&rel.from, &rel.to) {
                        buc_to_actors.entry(*bk).or_default().push(*ak);
                    }
                }
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
        let mut direct_actor_of: HashMap<UseCaseKey, Vec<ActorKey>> = HashMap::new();
        for rel in &model.relations {
            if rel.kind == RelKind::Performs {
                if let (NodeRef::Actor(ak), NodeRef::UseCase(uk)) = (&rel.from, &rel.to) {
                    direct_actor_of.entry(*uk).or_default().push(*ak);
                }
            }
        }

        // ── TX境界推論 ────────────────────────────────────────────────────
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

        // ── 表示対象ユースケース（CRUD 参照/書き込みありのもの、可視なもの） ─
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
            return Ok("@startuml\n' no sequenceable usecases found\n@enduml\n".to_string());
        }

        // ── 必要な参加者を収集 ─────────────────────────────────────────────
        let mut actor_keys: HashSet<ActorKey> = HashSet::new();
        let mut entity_keys: HashSet<EntityKey> = HashSet::new();
        let mut screen_keys: HashSet<ScreenKey> = HashSet::new();
        let mut api_keys: HashSet<ApiKey> = HashSet::new();
        let mut has_legacy_uc = false; // API のない usecase が1つでもあれば System レーンを出す

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

        // ── 出力組み立て ────────────────────────────────────────────────────
        let mut out = String::from("@startuml\n!theme plain\n\n");

        let mut actors_sorted: Vec<(ActorKey, &rdra_ish_core::model::Actor)> = model
            .actors
            .iter()
            .filter(|(k, _)| actor_keys.contains(k))
            .collect();
        actors_sorted.sort_by_key(|(_, a)| a.id.as_str());
        if !actors_sorted.is_empty() {
            out.push_str("box \"System Value\" #E3F2FD\n");
            for (_, actor) in &actors_sorted {
                out.push_str(&format!(
                    "actor \"{}\" as {}\n",
                    prefixed_label("👤", &actor.label),
                    actor.id
                ));
            }
            out.push_str("end box\n");
        }

        let mut scrs_sorted: Vec<(ScreenKey, &rdra_ish_core::model::Screen)> = model
            .screens
            .iter()
            .filter(|(k, _)| screen_keys.contains(k))
            .collect();
        scrs_sorted.sort_by_key(|(_, s)| s.id.as_str());

        // API 参加者宣言（actor → screen → api → entity の左→右順）
        let mut apis_sorted: Vec<(ApiKey, &rdra_ish_core::model::Api)> = model
            .apis
            .iter()
            .filter(|(k, _)| api_keys.contains(k))
            .collect();
        apis_sorted.sort_by_key(|(_, a)| a.id.as_str());
        if !scrs_sorted.is_empty() {
            out.push_str("box \"System Boundary\" #E0F7FA\n");
            for (_, scr) in &scrs_sorted {
                out.push_str(&format!(
                    "boundary \"{}\" as {}\n",
                    prefixed_label("🖥️", &scr.label),
                    scr.id
                ));
            }
            out.push_str("end box\n");
        }

        let mut ents_sorted: Vec<(EntityKey, &rdra_ish_core::model::Entity)> = model
            .entities
            .iter()
            .filter(|(k, _)| entity_keys.contains(k))
            .collect();
        ents_sorted.sort_by_key(|(_, e)| e.id.as_str());
        if has_legacy_uc || !apis_sorted.is_empty() || !ents_sorted.is_empty() {
            out.push_str("box \"System\" #F3E5F5\n");
            // System レーン: レガシー UC が1件でもあれば維持（後方互換）
            if has_legacy_uc {
                out.push_str("participant \"🧩 システム\" as System\n");
            }
            for (_, api) in &apis_sorted {
                out.push_str(&format!(
                    "control \"{}\" as {}\n",
                    prefixed_label("🔌", &api.label),
                    api.id
                ));
            }
            for (_, ent) in &ents_sorted {
                out.push_str(&format!(
                    "database \"{}\" as {}\n",
                    prefixed_label("🗄️", &ent.label),
                    ent.id
                ));
            }
            out.push_str("end box\n");
        }
        out.push('\n');

        // ── ユースケースごとのシーケンス ──────────────────────────────────
        for (uk, uc) in &uc_list {
            let uc_label = prefixed_label("✅", &uc.label);
            out.push_str(&format!("== {} ==\n", uc_label));

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
                // 最初のAPIを代表として使用（複数APIは各書き込みの via_api で振り分け）
                let first_api_id = model
                    .apis
                    .get(apis[0])
                    .map(|a| a.id.as_str())
                    .unwrap_or("System");

                // Actor → Screen（あれば）→ API
                if let Some(ref sid) = screen_id {
                    out.push_str(&format!("{} -> {} : {}\n", actor_ref, sid, uc_label));
                    out.push_str(&format!("{} -> {} : {}\n", sid, first_api_id, uc_label));
                } else {
                    out.push_str(&format!(
                        "{} -> {} : {}\n",
                        actor_ref, first_api_id, uc_label
                    ));
                }
                out.push_str(&format!("activate {}\n", first_api_id));

                if let Some(apis) = invoked_apis {
                    for &ak in apis {
                        let src = model
                            .apis
                            .get(ak)
                            .map(|a| a.id.as_str())
                            .unwrap_or(first_api_id);
                        for &ek in api_to_reads.get(&ak).into_iter().flatten() {
                            if let Some(ent) = model.entities.get(ek) {
                                out.push_str(&format!("{} -> {} : read\n", src, ent.id));
                            }
                        }
                    }
                }
                for &ek in uc_to_reads.get(uk).into_iter().flatten() {
                    if let Some(ent) = model.entities.get(ek) {
                        out.push_str(&format!("{} -> {} : read\n", first_api_id, ent.id));
                    }
                }

                if let Some(tx) = uc_tx_map.get(uk) {
                    let singletons_set: HashSet<EntityKey> =
                        tx.singletons_note.iter().cloned().collect();

                    for group in &tx.fk_groups {
                        let label = if group.inferred {
                            "transaction (inferred from FK)"
                        } else {
                            "transaction (API atomic boundary)"
                        };
                        out.push_str(&format!("group {}\n", label));
                        for w in &group.ordered_writes {
                            if let Some(ent) = model.entities.get(w.entity) {
                                // via_api に対応するAPIのID、なければ最初のAPIを使用
                                let src = w
                                    .via_api
                                    .and_then(|ak| model.apis.get(ak))
                                    .map(|a| a.id.as_str())
                                    .unwrap_or(first_api_id);
                                out.push_str(&format!(
                                    "{} -> {} : {}\n",
                                    src,
                                    ent.id,
                                    w.kind.label()
                                ));
                            }
                        }
                        out.push_str("end\n");
                    }

                    for w in &tx.isolated_writes {
                        if let Some(ent) = model.entities.get(w.entity) {
                            let src = w
                                .via_api
                                .and_then(|ak| model.apis.get(ak))
                                .map(|a| a.id.as_str())
                                .unwrap_or(first_api_id);
                            out.push_str(&format!("{} -> {} : {}\n", src, ent.id, w.kind.label()));
                            if singletons_set.contains(&w.entity) {
                                out.push_str("note right : FK非連結 — 別TX？API境界で明示を\n");
                            }
                        }
                    }
                }

                // API → Screen（あれば）→ Actor へ返す
                if let Some(ref sid) = screen_id {
                    out.push_str(&format!(
                        "{} --> {} : {}\n",
                        first_api_id,
                        sid,
                        screen_label.as_deref().unwrap_or("")
                    ));
                    out.push_str(&format!(
                        "{} --> {} : {}\n",
                        sid,
                        actor_ref,
                        screen_label.as_deref().unwrap_or("")
                    ));
                } else {
                    out.push_str(&format!(
                        "{} --> {} : {}\n",
                        first_api_id, actor_ref, uc_label
                    ));
                }
                out.push_str(&format!("deactivate {}\n", first_api_id));
            } else {
                // ── レガシーパス（System ライン）─────────────────────────────
                out.push_str(&format!("{} -> System : {}\n", actor_ref, uc_label));
                out.push_str("activate System\n");

                for &ek in uc_to_reads.get(uk).into_iter().flatten() {
                    if let Some(ent) = model.entities.get(ek) {
                        out.push_str(&format!("System -> {} : read\n", ent.id));
                    }
                }

                if let Some(tx) = uc_tx_map.get(uk) {
                    let singletons_set: HashSet<EntityKey> =
                        tx.singletons_note.iter().cloned().collect();

                    for group in &tx.fk_groups {
                        let label = if group.inferred {
                            "transaction (inferred from FK)"
                        } else {
                            "transaction (API atomic boundary)"
                        };
                        out.push_str(&format!("group {}\n", label));
                        for w in &group.ordered_writes {
                            if let Some(ent) = model.entities.get(w.entity) {
                                out.push_str(&format!(
                                    "System -> {} : {}\n",
                                    ent.id,
                                    w.kind.label()
                                ));
                            }
                        }
                        out.push_str("end\n");
                    }

                    for w in &tx.isolated_writes {
                        if let Some(ent) = model.entities.get(w.entity) {
                            out.push_str(&format!("System -> {} : {}\n", ent.id, w.kind.label()));
                            if singletons_set.contains(&w.entity) {
                                out.push_str("note right : FK非連結 — 別TX？API境界で明示を\n");
                            }
                        }
                    }
                }

                if let Some(ref sid) = screen_id {
                    if let Some(ref slabel) = screen_label {
                        out.push_str(&format!("System --> {} : {}\n", actor_ref, slabel));
                    } else {
                        out.push_str(&format!("System --> {} : {}\n", actor_ref, sid));
                    }
                }

                out.push_str("deactivate System\n");
            }

            out.push('\n');
        }

        out.push_str("@enduml\n");
        Ok(out)
    }
}

// ── イベントフロー図エミッタ (PlantUML) ──────────────────────────────────────

pub struct EventFlowPlantUmlEmitter;

fn event_flow_node_visible(reachable: &Option<HashSet<NodeRef>>, node: &NodeRef) -> bool {
    match reachable {
        Some(set) => set.contains(node),
        None => true,
    }
}

fn event_flow_event_id(id: &str) -> String {
    format!("ev__{}", id)
}

fn event_flow_usecase_id(id: &str) -> String {
    format!("uc__{}", id)
}

fn event_flow_buc_id(id: &str) -> String {
    format!("buc__{}", id)
}

fn event_flow_state_id(id: &str) -> String {
    format!("st__{}", id)
}

fn declare_event_flow_event(
    out: &mut String,
    declared: &mut HashSet<String>,
    model: &SemanticModel,
    flow: &EventFlow,
) -> Option<String> {
    let event = model.events.get(flow.event)?;
    let event_id = event_flow_event_id(&event.id);
    if declared.insert(event_id.clone()) {
        out.push_str(&format!(
            "card \"{}\" as {}\n",
            prefixed_label("⚡", &event.label),
            event_id
        ));
    }
    Some(event_id)
}

fn declare_event_flow_usecase(
    out: &mut String,
    declared: &mut HashSet<String>,
    model: &SemanticModel,
    usecase: UseCaseKey,
) -> Option<String> {
    let uc = model.use_cases.get(usecase)?;
    let usecase_id = event_flow_usecase_id(&uc.id);
    if declared.insert(usecase_id.clone()) {
        out.push_str(&format!(
            "usecase \"{}\" as {}\n",
            prefixed_label("✅", &uc.label),
            usecase_id
        ));
    }
    Some(usecase_id)
}

fn declare_event_flow_buc(
    out: &mut String,
    declared: &mut HashSet<String>,
    model: &SemanticModel,
    buc: BucKey,
) -> Option<String> {
    let buc_model = model.bucs.get(buc)?;
    let buc_id = event_flow_buc_id(&buc_model.id);
    if declared.insert(buc_id.clone()) {
        out.push_str(&format!(
            "rectangle \"{}\" as {}\n",
            prefixed_label("📦", &buc_model.label),
            buc_id
        ));
    }
    Some(buc_id)
}

fn declare_event_flow_state(
    out: &mut String,
    declared: &mut HashSet<String>,
    model: &SemanticModel,
    state: rdra_ish_core::model::StateKey,
) -> Option<String> {
    let state_model = model.states.get(state)?;
    let state_id = event_flow_state_id(&state_model.id);
    if declared.insert(state_id.clone()) {
        out.push_str(&format!(
            "state \"{}\" as {}\n",
            prefixed_label("🔄", &state_model.label),
            state_id
        ));
    }
    Some(state_id)
}

fn render_event_flow(
    out: &mut String,
    declared: &mut HashSet<String>,
    model: &SemanticModel,
    reachable: &Option<HashSet<NodeRef>>,
    flow: &EventFlow,
) {
    let event_node = NodeRef::Event(flow.event);
    if !event_flow_node_visible(reachable, &event_node) {
        return;
    }
    let Some(event_id) = declare_event_flow_event(out, declared, model, flow) else {
        return;
    };

    let mut raised_by = flow.raised_by.to_vec();
    raised_by.sort_by_key(|&uk| {
        model
            .use_cases
            .get(uk)
            .map(|uc| uc.id.as_str())
            .unwrap_or("")
    });
    for usecase in raised_by {
        let node = NodeRef::UseCase(usecase);
        if !event_flow_node_visible(reachable, &node) {
            continue;
        }
        if let Some(usecase_id) = declare_event_flow_usecase(out, declared, model, usecase) {
            out.push_str(&format!("{} ..> {} : raises\n", usecase_id, event_id));
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
        if !event_flow_node_visible(reachable, &node) {
            continue;
        }
        if let Some(usecase_id) = declare_event_flow_usecase(out, declared, model, usecase) {
            out.push_str(&format!("{} ..> {} : triggers\n", event_id, usecase_id));
        }
    }

    let mut triggered_bucs = flow.triggers_bucs.to_vec();
    triggered_bucs.sort_by_key(|&bk| model.bucs.get(bk).map(|buc| buc.id.as_str()).unwrap_or(""));
    for buc in triggered_bucs {
        let node = NodeRef::Buc(buc);
        if !event_flow_node_visible(reachable, &node) {
            continue;
        }
        if let Some(buc_id) = declare_event_flow_buc(out, declared, model, buc) {
            out.push_str(&format!("{} ..> {} : triggers\n", event_id, buc_id));
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
        let Some(from_id) = declare_event_flow_state(out, declared, model, from) else {
            continue;
        };
        let Some(to_id) = declare_event_flow_state(out, declared, model, to) else {
            continue;
        };
        out.push_str(&format!("{} --> {} : {}\n", from_id, to_id, event_label));
    }
}

impl Emitter for EventFlowPlantUmlEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let reachable: Option<HashSet<NodeRef>> = match &view.scope {
            Scope::Bucs(buc_ids) => Some(rdra_ish_core::reachable_from_bucs(model, buc_ids)),
            Scope::Whole | Scope::UseCases(_) => None,
        };
        let flows = rdra_ish_core::collect_event_flows(model);

        let mut out = String::new();
        out.push_str("@startuml\n");
        out.push_str("!theme plain\n");
        out.push_str("left to right direction\n\n");

        let mut declared: HashSet<String> = HashSet::new();

        for flow in &flows {
            render_event_flow(&mut out, &mut declared, model, &reachable, flow);
        }

        out.push_str("\n@enduml\n");
        Ok(out)
    }
}

// ── ヘルパー ──────────────────────────────────────────────────────────────────

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
    scoped_plantuml_id(
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
    scoped_plantuml_id("actor", &model.actors[entry.actor].id)
}

fn business_usecase_node_id(model: &SemanticModel, entry: &ActorInputInference) -> String {
    scoped_plantuml_id("usecase", &model.use_cases[entry.usecase].id)
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
    scoped_plantuml_id(
        "api",
        &format!("{}_{}", model.systems[system].id, model.apis[api].id),
    )
}

fn technical_entity_node_id(
    model: &SemanticModel,
    system: rdra_ish_core::SystemKey,
    entity: EntityKey,
) -> String {
    scoped_plantuml_id(
        "entity",
        &format!("{}_{}", model.systems[system].id, model.entities[entity].id),
    )
}

fn scoped_plantuml_id(prefix: &str, raw: &str) -> String {
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

fn plantuml_label(label: &str) -> String {
    label.replace('"', "\\\"")
}

fn is_entity_operation(kind: &RelKind) -> bool {
    matches!(
        kind,
        RelKind::Reads | RelKind::Writes | RelKind::Creates | RelKind::Updates | RelKind::Deletes
    )
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
        NodeRef::Concept(k) => model.concepts.get(*k).map(|c| c.id.as_str()),
        NodeRef::DomainObject(k) => model.domain_objects.get(*k).map(|d| d.id.as_str()),
        NodeRef::Aggregate(k) => model.aggregates.get(*k).map(|a| a.id.as_str()),
        NodeRef::ValueObject(k) => model.value_objects.get(*k).map(|v| v.id.as_str()),
        NodeRef::Business(k) => model.businesses.get(*k).map(|b| b.id.as_str()),
        NodeRef::Buc(k) => model.bucs.get(*k).map(|b| b.id.as_str()),
        NodeRef::Flow(k) => model.flows.get(*k).map(|f| f.id.as_str()),
        NodeRef::Step(k) => model.steps.get(*k).map(|s| s.id.as_str()),
        NodeRef::UsageScene(k) => model.usage_scenes.get(*k).map(|u| u.id.as_str()),
        NodeRef::UseCase(k) => model.use_cases.get(*k).map(|u| u.id.as_str()),
        NodeRef::Screen(k) => model.screens.get(*k).map(|s| s.id.as_str()),
        NodeRef::Field(k) => model.fields.get(*k).map(|f| f.id.as_str()),
        NodeRef::Event(k) => model.events.get(*k).map(|e| e.id.as_str()),
        NodeRef::Entity(k) => model.entities.get(*k).map(|e| e.id.as_str()),
        NodeRef::State(k) => model.states.get(*k).map(|s| s.id.as_str()),
        NodeRef::Condition(k) => model.conditions.get(*k).map(|c| c.id.as_str()),
        NodeRef::Variation(k) => model.variations.get(*k).map(|v| v.id.as_str()),
        NodeRef::Api(k) => model.apis.get(*k).map(|a| a.id.as_str()),
        NodeRef::Dto(k) => model.dtos.get(*k).map(|d| d.id.as_str()),
        NodeRef::Location(k) => model.locations.get(*k).map(|l| l.id.as_str()),
        NodeRef::Timing(k) => model.timings.get(*k).map(|t| t.id.as_str()),
        NodeRef::Medium(k) => model.media.get(*k).map(|m| m.id.as_str()),
        NodeRef::Permission(k) => model.permissions.get(*k).map(|p| p.id.as_str()),
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
        NodeRef::Concept(k) => model.concepts.get(*k).map(|c| c.label.as_str()),
        NodeRef::DomainObject(k) => model.domain_objects.get(*k).map(|d| d.label.as_str()),
        NodeRef::Aggregate(k) => model.aggregates.get(*k).map(|a| a.label.as_str()),
        NodeRef::ValueObject(k) => model.value_objects.get(*k).map(|v| v.label.as_str()),
        NodeRef::Business(k) => model.businesses.get(*k).map(|b| b.label.as_str()),
        NodeRef::Buc(k) => model.bucs.get(*k).map(|b| b.label.as_str()),
        NodeRef::Flow(k) => model.flows.get(*k).map(|f| f.label.as_str()),
        NodeRef::Step(k) => model.steps.get(*k).map(|s| s.label.as_str()),
        NodeRef::UsageScene(k) => model.usage_scenes.get(*k).map(|u| u.label.as_str()),
        NodeRef::UseCase(k) => model.use_cases.get(*k).map(|u| u.label.as_str()),
        NodeRef::Screen(k) => model.screens.get(*k).map(|s| s.label.as_str()),
        NodeRef::Field(k) => model.fields.get(*k).map(|f| f.label.as_str()),
        NodeRef::Event(k) => model.events.get(*k).map(|e| e.label.as_str()),
        NodeRef::Entity(k) => model.entities.get(*k).map(|e| e.label.as_str()),
        NodeRef::State(k) => model.states.get(*k).map(|s| s.label.as_str()),
        NodeRef::Condition(k) => model.conditions.get(*k).map(|c| c.label.as_str()),
        NodeRef::Variation(k) => model.variations.get(*k).map(|v| v.label.as_str()),
        NodeRef::Api(k) => model.apis.get(*k).map(|a| a.label.as_str()),
        NodeRef::Dto(k) => model.dtos.get(*k).map(|d| d.label.as_str()),
        NodeRef::Location(k) => model.locations.get(*k).map(|l| l.label.as_str()),
        NodeRef::Timing(k) => model.timings.get(*k).map(|t| t.label.as_str()),
        NodeRef::Medium(k) => model.media.get(*k).map(|m| m.label.as_str()),
        NodeRef::Permission(k) => model.permissions.get(*k).map(|p| p.label.as_str()),
    }
}

pub(crate) fn col_type_str(ct: &ColumnType) -> &'static str {
    match ct {
        ColumnType::Int => "Int",
        ColumnType::String => "String",
        ColumnType::Money => "Money",
        ColumnType::DateTime => "DateTime",
        ColumnType::Date => "Date",
        ColumnType::Bool => "Bool",
        ColumnType::Decimal => "Decimal",
        ColumnType::Enum(_) => "Enum",
    }
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
    fn test_rdra_plantuml_emit() {
        let src = r#"
actor Customer "顧客"
usecase Browse "商品を探す"
performs(Customer, Browse)
"#;
        let model = model_from(src);
        let view = View::whole();
        let result = RdraPlantUmlEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("@startuml"));
        assert!(result.contains("actor \"👤 顧客\" as Customer"));
        assert!(result.contains("usecase \"✅ 商品を探す\" as Browse"));
        assert!(result.contains("Customer --> Browse"));
        assert!(result.contains("@enduml"));
    }

    #[test]
    fn test_rdra_relation_arrow_labels_and_overview_skips() {
        assert_eq!(
            rdra_relation_arrow(&RelKind::Reads, "Browse", "Catalog").as_deref(),
            Some("Browse ..> Catalog : reads")
        );
        assert_eq!(
            rdra_relation_arrow(&RelKind::RelateManyToOne, "Order", "Customer").as_deref(),
            Some("Order -- Customer")
        );
        assert_eq!(
            rdra_relation_arrow(&RelKind::Precedes, "ReviewCart", "AuthorizePayment").as_deref(),
            Some("ReviewCart --> AuthorizePayment : precedes")
        );
        assert_eq!(
            rdra_relation_arrow(&RelKind::Branches, "ReviewCart", "PaymentFailed").as_deref(),
            Some("ReviewCart ..> PaymentFailed : branches")
        );
        assert_eq!(
            rdra_relation_arrow(&RelKind::Excepts, "AuthorizePayment", "PaymentFailed").as_deref(),
            Some("AuthorizePayment ..> PaymentFailed : excepts")
        );
        assert_eq!(
            rdra_relation_arrow(&RelKind::Repeats, "PaymentFailed", "ReviewCart").as_deref(),
            Some("PaymentFailed ..> ReviewCart : repeats")
        );
        assert_eq!(
            rdra_relation_arrow(&RelKind::Covers, "AuthorizePayment", "CapturePayment").as_deref(),
            Some("AuthorizePayment ..> CapturePayment : covers")
        );
        assert_eq!(
            rdra_relation_arrow(&RelKind::Compensates, "RefundPayment", "CapturePayment")
                .as_deref(),
            Some("RefundPayment ..> CapturePayment : compensates")
        );
        assert!(rdra_relation_arrow(&RelKind::Transitions, "Draft", "Published").is_none());
        assert!(rdra_relation_arrow(&RelKind::Invokes, "Browse", "BrowseApi").is_none());
    }

    #[test]
    fn test_rdra_render_helpers_keep_nodes_relations_and_api_out_of_overview() {
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
        render_rdra_node_declarations(&mut declarations, &model, &reachable, &View::whole());
        assert!(declarations.contains("actor \"👤 顧客\" as Customer"));
        assert!(declarations.contains("usecase \"✅ 商品を探す\" as Browse"));
        assert!(declarations.contains("database \"🗄️ 商品台帳\" as Catalog"));
        assert!(!declarations.contains("BrowseApi"));

        let mut relations = String::new();
        render_rdra_relations(&mut relations, &model, &reachable, &View::whole());
        assert!(relations.contains("Customer --> Browse"));
        assert!(relations.contains("Browse ..> Catalog : reads"));
        assert!(!relations.contains("BrowseApi"));
    }

    #[test]
    fn test_business_area_plantuml_emit() {
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
        let result = BusinessAreaPlantUmlEmitter
            .emit(&model, &View::whole())
            .unwrap();
        assert!(result.contains("actor \"Business Actor Staff\" as actor__Staff"));
        assert!(result.contains("rectangle \"Appointment.patient_name, create\""));
        assert!(result.contains(": create"));
        assert!(!result.contains("BucScheduling"));
    }

    #[test]
    fn test_business_area_plantuml_disambiguates_actor_and_usecase_ids() {
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

        let result = BusinessAreaPlantUmlEmitter
            .emit(&model, &View::whole())
            .unwrap();
        assert!(result.contains("actor \"Business Actor Actor Same\" as actor__Same"));
        assert!(result.contains("usecase \"UseCase UseCase Same\" as usecase__Same"));
        assert!(result.contains("actor__Same --> input__Same_Same_Thing_name"));
        assert!(result.contains("input__Same_Same_Thing_name --> usecase__Same : create"));
    }

    #[test]
    fn test_technical_area_plantuml_emit() {
        let src = r#"
system SchedulingSystem "Scheduling System"
api BookingApi "Booking API"
entity Appointment "Appointment" { id: Int @pk }
contains(SchedulingSystem, BookingApi)
creates(BookingApi, Appointment)
"#;
        let model = model_from(src);
        let result = TechnicalAreaPlantUmlEmitter
            .emit(&model, &View::whole())
            .unwrap();
        assert!(result.contains("package \"Scheduling System\""));
        assert!(result.contains("control \"API Booking API\""));
        assert!(result.contains("database \"Entity Appointment\""));
        assert!(result.contains(": creates"));
    }

    #[test]
    fn test_object_graph_plantuml_layers() {
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
        let result = ObjectGraphPlantUmlEmitter
            .emit(&model, &View::whole())
            .unwrap();
        assert!(result.contains("@startuml"));
        assert!(result.contains("left to right direction"));
        assert!(result.contains("rectangle \"System Value\""));
        assert!(result.contains("rectangle \"External Environment\""));
        assert!(result.contains("rectangle \"System Boundary\""));
        assert!(result.contains("rectangle \"System\""));
        assert!(result.contains("actor \"👤 顧客\" as Customer"));
        assert!(result.contains("rectangle \"📦 注文業務\" as BucOrder"));
        assert!(result.contains("boundary \"🖥️ 注文画面\" as OrderScreen"));
        assert!(result.contains("control \"🔌 注文API\" as OrderApi"));
        assert!(result.contains("database \"🗄️ 注文\" as Order"));
        let boundary_pos = result.find("rectangle \"System Boundary\"").unwrap();
        let system_pos = result.find("rectangle \"System\"").unwrap();
        let screen_pos = result
            .find("boundary \"🖥️ 注文画面\" as OrderScreen")
            .unwrap();
        let api_pos = result.find("control \"🔌 注文API\" as OrderApi").unwrap();
        assert!(boundary_pos < screen_pos);
        assert!(screen_pos < system_pos);
        assert!(system_pos < api_pos);
        assert!(result.contains("Customer --> BucOrder : performs"));
        assert!(result.contains("PlaceOrder ..> OrderApi : invokes"));
        assert!(result.contains("OrderApi ..> Order : creates"));
    }

    #[test]
    fn test_object_graph_plantuml_applies_node_and_edge_filters() {
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

        let result = ObjectGraphPlantUmlEmitter.emit(&model, &view).unwrap();

        assert!(result.contains("usecase \"✅ Place order\" as PlaceOrder"));
        assert!(result.contains("control \"🔌 Order API\" as OrderApi"));
        assert!(result.contains("PlaceOrder ..> OrderApi : invokes"));
        assert!(!result.contains("actor \"👤 Customer\""));
        assert!(!result.contains("database \"🗄️ Order\""));
        assert!(!result.contains("performs"));
        assert!(!result.contains("creates"));
    }

    #[test]
    fn test_er_plantuml_emit() {
        let src = r#"
entity Order "注文" { id: Int @pk  total: Money }
entity Customer "顧客" { id: Int @pk  name: String }
relate(Order, Customer, "N:1")
"#;
        let model = model_from(src);
        let view = View::er();
        let result = ErPlantUmlEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("@startuml"));
        assert!(result.contains("entity \"🗄️ 注文\" as Order"));
        assert!(result.contains("*id : Int <<PK>>"));
        assert!(result.contains("customer_id : Int <<FK>>"));
        assert!(result.contains("}o--||"));
        assert!(result.contains("@enduml"));
    }

    #[test]
    fn test_er_plantuml_snapshot() {
        let src = r#"
entity Customer "顧客" { id: Int @pk  name: String }
entity Order "注文" { id: Int @pk  total: Money }
relate(Order, Customer, "N:1")
"#;
        let (ast, _) = parse(src);
        let (model, _) = build_model(&ast);
        let result = ErPlantUmlEmitter.emit(&model, &View::er()).unwrap();
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_er_plantuml_buc_filter() {
        // BUCフィルタを使うとBUCが触れるエンティティのみが出力される
        let src = r#"
buc BucA "業務A"
usecase UcA "ユースケースA"
entity EntityA "エンティティA" { id: Int @pk }
entity EntityB "エンティティB" { id: Int @pk }
contains(BucA, UcA)
reads(UcA, EntityA)
"#;
        let model = model_from(src);
        // BUCフィルタあり
        let view = View {
            scope: crate::Scope::Bucs(vec!["BucA".to_string()]),
            filter: crate::Filter::Er,
            show_descriptions: false,
            node_kinds: Vec::new(),
            edge_kinds: Vec::new(),
        };
        let result = ErPlantUmlEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("EntityA"), "EntityA should be included");
        assert!(!result.contains("EntityB"), "EntityB should be excluded");
    }

    #[test]
    fn test_plantuml_show_description_renders_notes() {
        let src = r#"
actor Customer "Customer" description "Places orders"
usecase Browse "Browse" description "Finds products"
performs(Customer, Browse)
"#;
        let model = model_from(src);
        let view = View::whole().with_descriptions(true);

        let result = RdraPlantUmlEmitter.emit(&model, &view).unwrap();

        assert!(result.contains("note right of Customer\nPlaces orders\nend note"));
        assert!(result.contains("note right of Browse\nFinds products\nend note"));
    }

    #[test]
    fn test_sequence_fk_group_and_singleton() {
        let src = r#"
actor Customer "顧客"
buc BucOrder "注文を処理する"
usecase PlaceOrder "注文を確定する"
screen OrderCompleteScreen "注文完了画面"
entity Order     "注文"     { id: Int @pk }
entity OrderLine "注文明細" { id: Int @pk }
entity Cart      "カート"   { id: Int @pk }
relate(OrderLine, Order, "N:1")
performs(Customer, BucOrder)
contains(BucOrder, PlaceOrder)
creates(PlaceOrder, Order)
creates(PlaceOrder, OrderLine)
updates(PlaceOrder, Cart)
displays(PlaceOrder, OrderCompleteScreen)
"#;
        let model = model_from(src);
        let view = View::whole();
        let result = SequenceDiagramEmitter.emit(&model, &view).unwrap();

        // 参加者宣言
        assert!(result.contains("box \"System Value\""));
        assert!(result.contains("box \"System Boundary\""));
        assert!(result.contains("box \"System\""));
        assert!(result.contains("actor \"👤 顧客\" as Customer"));
        assert!(result.contains("database \"🗄️ 注文\" as Order"));
        assert!(result.contains("database \"🗄️ 注文明細\" as OrderLine"));
        assert!(result.contains("database \"🗄️ カート\" as Cart"));
        assert!(result.contains("boundary \"🖥️ 注文完了画面\" as OrderCompleteScreen"));

        // UCセクション見出し
        assert!(result.contains("== ✅ 注文を確定する =="));

        // アクター → System メッセージ
        assert!(result.contains("Customer -> System : ✅ 注文を確定する"));

        // FK連結グループ（Order → OrderLine の順）
        assert!(result.contains("group transaction (inferred from FK)"));
        assert!(result.contains("System -> Order : create"));
        assert!(result.contains("System -> OrderLine : create"));
        assert!(result.contains("end\n"));

        // FK非連結書き込みと note
        assert!(result.contains("System -> Cart : update"));
        assert!(result.contains("note right : FK非連結"));

        // 画面レスポンス
        assert!(result.contains("System --> Customer : 🖥️ 注文完了画面"));

        // Order が OrderLine より前に出現する
        let order_pos = result.find("System -> Order : create").unwrap();
        let orderline_pos = result.find("System -> OrderLine : create").unwrap();
        assert!(
            order_pos < orderline_pos,
            "Order(parent) must precede OrderLine(child)"
        );

        // スナップショット
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_event_flow_plantuml_renders_triggered_buc() {
        let src = r#"
buc BucBillingClaims "Billing Claims"
usecase SignEncounter "Sign Encounter"
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
triggers(EncounterSigned, BucBillingClaims)
"#;
        let model = model_from(src);
        let result = EventFlowPlantUmlEmitter
            .emit(&model, &View::whole())
            .unwrap();

        assert!(result.contains("ev__EncounterSigned ..> buc__BucBillingClaims : triggers"));
        assert!(result.contains("rectangle \"📦 Billing Claims\" as buc__BucBillingClaims"));
    }

    #[test]
    fn test_event_flow_helpers_deduplicate_declarations_and_render_transitions() {
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

        render_event_flow(&mut out, &mut declared, &model, &None, flow);
        render_event_flow(&mut out, &mut declared, &model, &None, flow);

        assert_eq!(out.matches("card \"⚡ Encounter Signed\"").count(), 1);
        assert!(out.contains("uc__SignEncounter ..> ev__EncounterSigned : raises"));
        assert!(out.contains("ev__EncounterSigned ..> uc__ReviewClaim : triggers"));
        assert!(out.contains("ev__EncounterSigned ..> buc__BucBillingClaims : triggers"));
        assert!(out.contains("st__Pending --> st__Signed : ⚡ Encounter Signed"));
    }

    #[test]
    fn test_event_flow_visibility_skips_hidden_event_scope() {
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
            event_flow_event_id("EncounterSigned"),
            "ev__EncounterSigned"
        );
        assert_eq!(event_flow_usecase_id("SignEncounter"), "uc__SignEncounter");
        render_event_flow(&mut out, &mut declared, &model, &reachable, &flow);

        assert!(out.is_empty());
        assert!(declared.is_empty());
    }

    #[test]
    fn test_sequence_buc_filter() {
        // BUCフィルタで絞り込んだとき、対象BUCに直接含まれるUCのみ出力される。
        // triggers で到達する別BUCのUC/APIは sequence には混ぜない。
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
        let view = View::bucs(vec!["BucA".to_string()]);
        let result = SequenceDiagramEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("ユースケースA"), "BucA's UC should appear");
        assert!(result.contains("ApiA"), "BucA's API should appear");
        assert!(
            !result.contains("ユースケースB"),
            "BucB's UC should be excluded"
        );
        assert!(!result.contains("ApiB"), "BucB's API should be excluded");
    }

    #[test]
    fn test_sequence_usecase_filter() {
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
        let view = View::usecases(vec!["UcB".to_string()]);
        let result = SequenceDiagramEmitter.emit(&model, &view).unwrap();
        assert!(!result.contains("ユースケースA"));
        assert!(!result.contains("ApiA"));
        assert!(!result.contains("actor \"👤 顧客\" as Customer"));
        assert!(result.contains("actor \"👤 担当者\" as Clerk"));
        assert!(result.contains("ユースケースB"));
        assert!(result.contains("ApiB"));
    }

    #[test]
    fn test_sequence_read_only_usecase() {
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
        let result = SequenceDiagramEmitter
            .emit(&model, &View::usecases(vec!["Search".to_string()]))
            .unwrap();
        assert!(result.contains("actor \"👤 顧客\" as Customer"));
        assert!(result.contains("control \"🔌 検索API\" as SearchApi"));
        assert!(result.contains("database \"🗄️ 品目\" as Item"));
        assert!(result.contains("SearchApi -> Item : read"));
        let boundary_box_pos = result.find("box \"System Boundary\"").unwrap();
        let system_box_pos = result.find("box \"System\"").unwrap();
        let screen_pos = result
            .find("boundary \"🖥️ 検索画面\" as SearchScreen")
            .unwrap();
        let api_pos = result.find("control \"🔌 検索API\" as SearchApi").unwrap();
        let entity_pos = result.find("database \"🗄️ 品目\" as Item").unwrap();
        assert!(boundary_box_pos < screen_pos);
        assert!(screen_pos < system_box_pos);
        assert!(system_box_pos < api_pos);
        assert!(screen_pos < api_pos);
        assert!(api_pos < entity_pos);
        assert!(!result.contains("no sequenceable usecases"));
    }

    #[test]
    fn test_rdra_plantuml_multi_buc_filter() {
        // 複数BUC指定で両BUCの到達ノードが出力される
        let src = r#"
buc BucA "業務A"
buc BucB "業務B"
usecase UcA "ユースケースA"
usecase UcB "ユースケースB"
usecase UcC "ユースケースC"
contains(BucA, UcA)
contains(BucB, UcB)
"#;
        let model = model_from(src);
        let view = View::bucs(vec!["BucA".to_string(), "BucB".to_string()]);
        let result = RdraPlantUmlEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("BucA"), "BucA should be included");
        assert!(result.contains("BucB"), "BucB should be included");
        assert!(
            result.contains("UcA"),
            "UcA should be included (reachable from BucA)"
        );
        assert!(
            result.contains("UcB"),
            "UcB should be included (reachable from BucB)"
        );
        assert!(
            !result.contains("UcC"),
            "UcC should be excluded (unreachable)"
        );
    }
}
