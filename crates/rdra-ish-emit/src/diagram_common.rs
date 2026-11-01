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

/// `label` safe between the `|` marks of an edge label.
pub(crate) fn edge_label(label: &str) -> String {
    quote(label).replace('|', "#124;")
}

/// The Mermaid flowchart shape of each element kind, as the brackets around the quoted
/// label: `<id><open>"<label>"<close>:::<kind>`. Every flowchart draws its nodes with
/// these, so kinds tell apart by shape as well as by colour. Shapes repeat across layers
/// (the rdra diagram has more kinds than Mermaid has shapes) but not among the kinds one
/// focused diagram draws together, except a variation's `value`, a rectangle like
/// `system` in the condition diagram, which always sits in its variation's subgraph.
/// `business`, `flow`, and `variation` are also drawn as
/// subgraphs, which keep `subgraph <id>["<label>"]`; a `lane` is only ever a subgraph.
pub const NODE_SHAPES: &[(&str, &str, &str)] = &[
    ("actor", "([", "])"),
    ("extsystem", "[[", "]]"),
    ("system", "[", "]"),
    ("requirement", "[/", "\\]"),
    ("goal", "[/", "/]"),
    ("nfr", "[\\", "\\]"),
    ("adr", "[", "]"),
    ("quality", "[", "]"),
    ("constraint", "[\\", "\\]"),
    ("business", "[\\", "/]"),
    ("buc", "[", "]"),
    ("flow", "[", "]"),
    ("step", "[", "]"),
    ("timer", "((", "))"),
    ("medium", "[", "]"),
    ("permission", "[", "]"),
    ("usecase", "(", ")"),
    ("screen", "[", "]"),
    ("field", "[", "]"),
    ("event", ">", "]"),
    ("api", "{{", "}}"),
    ("dto", "[", "]"),
    ("entity", "[(", ")]"),
    ("state", "(((", ")))"),
    ("statemodel", "[[", "]]"),
    ("variation", "[\\", "\\]"),
    ("value", "[", "]"),
    ("condition", "{", "}"),
];

/// The opening and closing brackets of `kind`'s shape, or `None` for a kind no flowchart
/// draws as a node.
pub fn node_shape(kind: &str) -> Option<(&'static str, &'static str)> {
    NODE_SHAPES
        .iter()
        .find(|(k, _, _)| *k == kind)
        .map(|(_, open, close)| (*open, *close))
}

/// One node line: `id` in `kind`'s shape around the quoted `text` (escaped already), tagged
/// `:::<kind>`, at `indent`, newline included.
pub(crate) fn node_line(indent: &str, id: &str, text: &str, kind: &str) -> String {
    let (open, close) = node_shape(kind).unwrap_or(("[", "]"));
    format!("{indent}{id}{open}\"{text}\"{close}:::{kind}\n")
}

/// What the label of an edge says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeLabel {
    /// The relation's name and nothing more, such as `performs`. Left off the edge when the
    /// relation is the only one the diagram draws between the two kinds.
    Name,
    /// Something the endpoints do not give: the operations, an event, a use case, a column.
    /// Always written.
    Data,
}

/// One relation a flowchart draws: from a node of a `from` kind to one of a `to` kind.
pub struct EdgeRelation {
    pub diagram: &'static str,
    pub from: &'static [&'static str],
    pub to: &'static [&'static str],
    /// The relation's name; for a [`EdgeLabel::Name`] edge also its label.
    pub relation: &'static str,
    pub label: EdgeLabel,
}

const fn rel(
    diagram: &'static str,
    from: &'static [&'static str],
    to: &'static [&'static str],
    relation: &'static str,
    label: EdgeLabel,
) -> EdgeRelation {
    EdgeRelation {
        diagram,
        from,
        to,
        relation,
        label,
    }
}

use EdgeLabel::{Data, Name};

const CRUD: [&str; 5] = ["reads", "writes", "creates", "updates", "deletes"];
const STEP_ORDER: [&str; 4] = ["precedes", "branches", "excepts", "repeats"];
const CARDINALITIES: [&str; 4] = ["1:1", "1:N", "N:1", "N:M"];

/// Every relation each flowchart kind draws, by the kinds of its ends. An edge goes without
/// a label when its pair of kinds has exactly one relation in its diagram and that relation
/// is a [`EdgeLabel::Name`]: the relation is then read back from the endpoints
/// ([`implied_relation`]). The `rdra` rows follow the predicate signatures of
/// `rdra_ish_core::predicate`.
pub const EDGE_RELATIONS: &[EdgeRelation] = &[
    // rdra: every declared relation.
    rel(
        "rdra",
        &["actor"],
        &["usecase", "buc", "step"],
        "performs",
        Name,
    ),
    rel("rdra", &["usecase", "api"], &["entity"], CRUD[0], Name),
    rel("rdra", &["usecase", "api"], &["entity"], CRUD[1], Name),
    rel("rdra", &["usecase", "api"], &["entity"], CRUD[2], Name),
    rel("rdra", &["usecase", "api"], &["entity"], CRUD[3], Name),
    rel("rdra", &["usecase", "api"], &["entity"], CRUD[4], Name),
    rel("rdra", &["usecase"], &["api"], "invokes", Name),
    rel("rdra", &["api"], &["dto"], "request", Name),
    rel("rdra", &["api"], &["dto"], "response", Name),
    rel("rdra", &["api"], &["dto"], "error_response", Name),
    rel(
        "rdra",
        &["nfr", "condition"],
        &["usecase", "api", "system"],
        "applies_to",
        Name,
    ),
    // `.when(Cond)` on a `transitions` arm or a `chooses` value, derived: the use case
    // drives the guarded write. With `applies_to`, a condition to a use case has two
    // relations, so both are labelled.
    rel("rdra", &["condition"], &["usecase"], "when", Name),
    rel(
        "rdra",
        &["condition"],
        &["variation", "entity"],
        "combines",
        Name,
    ),
    rel(
        "rdra",
        &["nfr", "constraint"],
        &["quality"],
        "qualifies",
        Name,
    ),
    rel(
        "rdra",
        &["constraint"],
        &["usecase", "api", "system", "entity", "dto"],
        "constrains",
        Name,
    ),
    rel("rdra", &["system"], &["entity"], "owns", Name),
    rel("rdra", &["usecase"], &["screen"], "displays", Name),
    rel("rdra", &["screen"], &["entity"], "shows", Name),
    rel("rdra", &["screen"], &["screen"], "navigates", Name),
    rel("rdra", &["field"], &["entity"], "maps_field", Name),
    rel(
        "rdra",
        &["usecase", "extsystem"],
        &["event"],
        "raises",
        Name,
    ),
    rel(
        "rdra",
        &["event"],
        &["usecase", "buc", "extsystem"],
        "triggers",
        Name,
    ),
    rel("rdra", &["timer"], &["usecase", "buc"], "triggers", Name),
    rel(
        "rdra",
        &["buc", "system", "flow", "screen"],
        &["usecase", "api", "flow", "step", "field"],
        "contains",
        Name,
    ),
    rel("rdra", &["step"], &["step"], STEP_ORDER[0], Name),
    rel("rdra", &["step"], &["step"], STEP_ORDER[1], Name),
    rel("rdra", &["step"], &["step"], STEP_ORDER[2], Name),
    rel("rdra", &["step"], &["step"], STEP_ORDER[3], Name),
    rel(
        "rdra",
        &["step"],
        &["usecase", "api", "event", "timer"],
        "covers",
        Name,
    ),
    rel("rdra", &["usecase"], &["usecase"], "compensates", Name),
    rel("rdra", &["buc"], &["business"], "belongs", Name),
    rel("rdra", &["actor"], &["permission"], "has_permission", Name),
    rel(
        "rdra",
        &["usecase", "api"],
        &["permission"],
        "requires_permission",
        Name,
    ),
    rel(
        "rdra",
        &["usecase", "api"],
        &["medium"],
        "requires_medium",
        Name,
    ),
    rel("rdra", &["requirement"], &["buc"], "motivates", Name),
    rel("rdra", &["requirement"], &["goal"], "achieves", Name),
    rel(
        "rdra",
        &["adr"],
        &[
            "buc",
            "usecase",
            "api",
            "system",
            "entity",
            "requirement",
            "nfr",
            "constraint",
            "dto",
        ],
        "decides",
        Name,
    ),
    // `transitions <Entity>.<column> <from>-><to>`.
    rel("rdra", &["usecase"], &["state"], "transitions", Data),
    rel("rdra", &["entity"], &["entity"], CARDINALITIES[0], Name),
    rel("rdra", &["entity"], &["entity"], CARDINALITIES[1], Name),
    rel("rdra", &["entity"], &["entity"], CARDINALITIES[2], Name),
    rel("rdra", &["entity"], &["entity"], CARDINALITIES[3], Name),
    // context: an actor uses the system through the labelled BUCs; an external system
    // exchanges the labelled events.
    // An actor's edge is labelled with the BUCs it performs (`、`-joined).
    rel("context", &["actor"], &["system"], "performs", Data),
    rel("context", &["extsystem"], &["system"], "raises", Data),
    rel("context", &["system"], &["extsystem"], "triggers", Data),
    // requirement: the value tree.
    rel("requirement", &["requirement"], &["buc"], "motivates", Name),
    rel(
        "requirement",
        &["goal"],
        &["requirement"],
        "achieved_by",
        Name,
    ),
    // business: a hand-off is labelled with its event.
    rel("business", &["buc"], &["buc"], "handoff", Data),
    rel("business", &["buc"], &["business"], "belongs", Name),
    // usecase: the UC複合図.
    rel("usecase", &["actor"], &["usecase"], "performs", Name),
    rel(
        "usecase",
        &["event", "timer"],
        &["usecase"],
        "triggers",
        Name,
    ),
    rel("usecase", &["event"], &["extsystem"], "triggers", Name),
    rel("usecase", &["condition"], &["usecase"], "applies_to", Name),
    rel("usecase", &["condition"], &["usecase"], "when", Name),
    rel("usecase", &["usecase"], &["screen"], "displays", Name),
    rel(
        "usecase",
        &["usecase", "extsystem"],
        &["event"],
        "raises",
        Name,
    ),
    rel("usecase", &["usecase"], &["entity"], CRUD[0], Name),
    rel("usecase", &["usecase"], &["entity"], CRUD[1], Name),
    rel("usecase", &["usecase"], &["entity"], CRUD[2], Name),
    rel("usecase", &["usecase"], &["entity"], CRUD[3], Name),
    rel("usecase", &["usecase"], &["entity"], CRUD[4], Name),
    rel("usecase", &["screen"], &["entity"], "shows", Name),
    rel("usecase", &["usecase"], &["state"], "transitions", Data),
    // robustness: actor -> boundary -> control -> entity, and the events between them.
    rel("robustness", &["actor"], &["screen"], "uses", Name),
    // Labelled with the use case that joins them.
    rel(
        "robustness",
        &["screen"],
        &["api", "usecase"],
        "calls",
        Data,
    ),
    // Labelled with the CRUD letters of the control's operations.
    rel("robustness", &["api", "usecase"], &["entity"], "crud", Data),
    // Labelled with the event.
    rel(
        "robustness",
        &["api", "usecase", "extsystem"],
        &["api", "usecase", "extsystem"],
        "event",
        Data,
    ),
    // A timer starts the use case: the one relation from a timer, so unlabelled.
    rel(
        "robustness",
        &["timer"],
        &["api", "usecase"],
        "triggers",
        Name,
    ),
    // event: what raises each event and what it triggers.
    rel(
        "event",
        &["usecase", "extsystem"],
        &["event"],
        "raises",
        Name,
    ),
    rel(
        "event",
        &["event"],
        &["usecase", "buc", "extsystem"],
        "triggers",
        Name,
    ),
    rel("event", &["timer"], &["usecase", "buc"], "triggers", Name),
    // flow: steps in order, what each covers, and who performs one outside a lane.
    rel("flow", &["step"], &["step"], STEP_ORDER[0], Name),
    rel("flow", &["step"], &["step"], STEP_ORDER[1], Name),
    rel("flow", &["step"], &["step"], STEP_ORDER[2], Name),
    rel("flow", &["step"], &["step"], STEP_ORDER[3], Name),
    rel(
        "flow",
        &["step"],
        &["usecase", "api", "event", "timer"],
        "covers",
        Name,
    ),
    rel("flow", &["actor"], &["step"], "performs", Name),
    // information: relates, state models, and the variation a column takes.
    rel(
        "information",
        &["entity"],
        &["entity"],
        CARDINALITIES[0],
        Name,
    ),
    rel(
        "information",
        &["entity"],
        &["entity"],
        CARDINALITIES[1],
        Name,
    ),
    rel(
        "information",
        &["entity"],
        &["entity"],
        CARDINALITIES[2],
        Name,
    ),
    rel(
        "information",
        &["entity"],
        &["entity"],
        CARDINALITIES[3],
        Name,
    ),
    rel(
        "information",
        &["entity"],
        &["statemodel"],
        "statemodel",
        Name,
    ),
    // `variation <column>`.
    rel(
        "information",
        &["entity"],
        &["variation"],
        "variation",
        Data,
    ),
    // condition: what a condition combines and applies to.
    rel(
        "condition",
        &["condition"],
        &["variation", "statemodel"],
        "combines",
        Name,
    ),
    rel(
        "condition",
        &["condition"],
        &["usecase", "api", "system"],
        "applies_to",
        Name,
    ),
    rel("condition", &["condition"], &["usecase"], "when", Name),
    // screen: a navigates is labelled with its `.via` use case, else `navigates`; a dashed
    // edge is 業務上の順番.
    rel("screen", &["screen"], &["screen"], "navigates", Name),
    rel("screen", &["screen"], &["screen"], "via", Data),
    rel("screen", &["screen"], &["screen"], "業務上の順番", Name),
];

/// The relations `diagram` draws from a `from` node to a `to` node.
pub fn edge_relations<'a>(
    diagram: &'a str,
    from: &'a str,
    to: &'a str,
) -> impl Iterator<Item = &'static EdgeRelation> + 'a {
    EDGE_RELATIONS
        .iter()
        .filter(move |r| r.diagram == diagram && r.from.contains(&from) && r.to.contains(&to))
}

/// The relation an unlabelled `diagram` edge from a `from` node to a `to` node stands for:
/// the one relation the diagram draws between the two kinds, when its label is its name.
pub fn implied_relation(diagram: &str, from: &str, to: &str) -> Option<&'static str> {
    let mut relations = edge_relations(diagram, from, to);
    match (relations.next(), relations.next()) {
        (Some(only), None) if only.label == EdgeLabel::Name => Some(only.relation),
        _ => None,
    }
}

/// The kind of a contract id, `<kind>__<DSL id>`.
fn id_kind(id: &str) -> &str {
    id.split_once("__").map_or(id, |(kind, _)| kind)
}

/// One edge line at two spaces, newline included: `from arrow|label| to`, or `from arrow
/// to` when the label is the relation [`implied_relation`] reads back from the ends.
pub(crate) fn edge_line(diagram: &str, from: &str, arrow: &str, label: &str, to: &str) -> String {
    if implied_relation(diagram, id_kind(from), id_kind(to)) == Some(label) {
        format!("  {from} {arrow} {to}\n")
    } else {
        format!("  {from} {arrow}|{label}| {to}\n")
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
            Scope::Whole if !self.view.state.entities.is_empty() => {
                format!("entity:{}", self.view.state.entities.join(","))
            }
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
    edge_line("event", from, "-.->", predicate, to)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The relation label the rdra diagram writes for a predicate, when it draws one.
    fn drawn_labels(predicate: &'static str) -> Vec<&'static str> {
        match predicate {
            "relate" => CARDINALITIES.to_vec(),
            // Not an edge: a rule, an entity property, or no relation of the model.
            "holds" | "exposes" | "shares" | "outbox" | "coordinates" | "after" | "sets"
            | "chooses" | "forbidden" | "invariant" | "required" | "exclusive" | "when" => {
                Vec::new()
            }
            // Drawn from the use case to the state it enters, labelled with the move.
            "transitions" => Vec::new(),
            _ => vec![predicate],
        }
    }

    #[test]
    fn the_rdra_rows_cover_every_pair_a_predicate_signature_allows() {
        for predicate in rdra_ish_core::KNOWN_PREDICATES {
            let Some(signature) = rdra_ish_core::predicate_signature(predicate) else {
                continue;
            };
            let [from, to, ..] = signature.as_slice() else {
                continue;
            };
            // `triggers` takes only the listed pairs of its kinds.
            let pairs: Vec<(&str, &str)> = if *predicate == "triggers" {
                rdra_ish_core::predicate::TRIGGER_PAIRS.to_vec()
            } else {
                from.iter()
                    .flat_map(|f| to.iter().map(move |t| (*f, *t)))
                    .collect()
            };
            for label in drawn_labels(predicate) {
                for (f, t) in &pairs {
                    // `combines(Condition, Entity.column)` is drawn to the entity.
                    let t = if *t == "_col" { "entity" } else { *t };
                    assert!(
                        edge_relations("rdra", f, t).any(|r| r.relation == label),
                        "{predicate}({f}, {t}) has no `{label}` row"
                    );
                }
            }
        }
    }

    #[test]
    fn a_focused_diagram_draws_each_of_its_kinds_in_its_own_shape() {
        let diagrams: BTreeSet<&str> = EDGE_RELATIONS.iter().map(|r| r.diagram).collect();
        for diagram in diagrams.into_iter().filter(|d| *d != "rdra") {
            let kinds: BTreeSet<&str> = EDGE_RELATIONS
                .iter()
                .filter(|r| r.diagram == diagram)
                .flat_map(|r| r.from.iter().chain(r.to))
                .copied()
                // A variation's values have no edge but are drawn in `condition`.
                .chain((diagram == "condition").then_some("value"))
                .collect();
            let mut shapes: Vec<(&str, &str, &str)> = Vec::new();
            for kind in kinds {
                let (open, close) =
                    node_shape(kind).unwrap_or_else(|| panic!("{diagram}: {kind} has no shape"));
                if let Some((other, ..)) = shapes.iter().find(|(_, o, c)| (*o, *c) == (open, close))
                {
                    // A value sits inside its variation's subgraph, apart from the systems.
                    let apart = matches!((*other, kind), ("system", "value") | ("value", "system"));
                    assert!(apart, "{diagram}: {kind} and {other} share {open}..{close}");
                }
                shapes.push((kind, open, close));
            }
        }
    }

    #[test]
    fn an_edge_goes_unlabelled_only_when_its_ends_name_one_relation() {
        assert_eq!(
            implied_relation("rdra", "actor", "usecase"),
            Some("performs")
        );
        assert_eq!(implied_relation("rdra", "usecase", "entity"), None);
        assert_eq!(implied_relation("robustness", "screen", "api"), None);
        assert_eq!(implied_relation("screen", "screen", "screen"), None);
        // A context actor edge is a `performs` whose label carries its BUCs.
        assert_eq!(implied_relation("context", "actor", "system"), None);
        let relations: Vec<_> = edge_relations("context", "actor", "system")
            .map(|r| (r.relation, r.label))
            .collect();
        assert_eq!(relations, [("performs", EdgeLabel::Data)]);
        assert_eq!(
            implied_relation("rdra", "screen", "screen"),
            Some("navigates")
        );
        assert_eq!(
            edge_line("rdra", "actor__A", "-.->", "performs", "usecase__U"),
            "  actor__A -.-> usecase__U\n"
        );
        assert_eq!(
            edge_line("rdra", "usecase__U", "-.->", "reads", "entity__E"),
            "  usecase__U -.->|reads| entity__E\n"
        );
    }
}
