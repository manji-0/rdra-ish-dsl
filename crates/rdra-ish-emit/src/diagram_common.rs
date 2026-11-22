//! Helpers the Mermaid emitters share.

use crate::{node_description, ObjectGraphLayer, Scope, View};
use rdra_ish_core::lifecycle::{Bounds, Scope as LifecycleScope, TransitionSystem};
use rdra_ish_core::model::{ColumnType, EntityKey, NodeRef, SemanticModel, StateKey};
use std::collections::HashSet;

/// Start states of the lifecycle columns the visible transitions name, read from the
/// transition system (the creation status, else the column's base), sorted by node id.
pub(crate) fn lifecycle_start_states(
    model: &SemanticModel,
    columns: impl IntoIterator<Item = (EntityKey, String)>,
    resolve: impl Fn(EntityKey, &str, &str) -> Option<StateKey>,
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
                .filter_map(|variant| resolve(*entity, column, variant))
                .collect::<Vec<_>>()
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    starts.sort_by_key(|sk| model.node_id(&NodeRef::State(*sk)).unwrap_or(""));
    starts
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

/// The nodes a diagram may draw: every one, or only those of a set.
pub(crate) enum NodeScope {
    All,
    Only(HashSet<NodeRef>),
}

impl NodeScope {
    pub(crate) fn contains(&self, node: &NodeRef) -> bool {
        match self {
            NodeScope::All => true,
            NodeScope::Only(set) => set.contains(node),
        }
    }

    pub(crate) fn is_all(&self) -> bool {
        matches!(self, NodeScope::All)
    }
}

pub(crate) fn reachable_for_scope(model: &SemanticModel, scope: &Scope) -> NodeScope {
    match scope {
        Scope::Bucs(buc_ids) => NodeScope::Only(rdra_ish_core::reachable_from_bucs(model, buc_ids)),
        Scope::Whole | Scope::UseCases(_) => NodeScope::All,
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

/// A diagram kind, by the name its `%% rdra-ish diagram=` line carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Diagram {
    Rdra,
    Context,
    Event,
    Business,
    Flow,
    Usecase,
    Requirement,
    Condition,
    Screen,
    Information,
    Robustness,
    Er,
    State,
    Sequence,
}

impl Diagram {
    pub fn as_str(self) -> &'static str {
        match self {
            Diagram::Rdra => "rdra",
            Diagram::Context => "context",
            Diagram::Event => "event",
            Diagram::Business => "business",
            Diagram::Flow => "flow",
            Diagram::Usecase => "usecase",
            Diagram::Requirement => "requirement",
            Diagram::Condition => "condition",
            Diagram::Screen => "screen",
            Diagram::Information => "information",
            Diagram::Robustness => "robustness",
            Diagram::Er => "er",
            Diagram::State => "state",
            Diagram::Sequence => "sequence",
        }
    }

    pub fn from_name(name: &str) -> Option<Diagram> {
        Some(match name {
            "rdra" => Diagram::Rdra,
            "context" => Diagram::Context,
            "event" => Diagram::Event,
            "business" => Diagram::Business,
            "flow" => Diagram::Flow,
            "usecase" => Diagram::Usecase,
            "requirement" => Diagram::Requirement,
            "condition" => Diagram::Condition,
            "screen" => Diagram::Screen,
            "information" => Diagram::Information,
            "robustness" => Diagram::Robustness,
            "er" => Diagram::Er,
            "state" => Diagram::State,
            "sequence" => Diagram::Sequence,
            _ => return None,
        })
    }
}

/// One relation a flowchart draws: from a node of a `from` kind to one of a `to` kind.
pub struct EdgeRelation {
    pub diagram: Diagram,
    pub from: &'static [&'static str],
    pub to: &'static [&'static str],
    /// The relation's name; for a [`EdgeLabel::Name`] edge also its label.
    pub relation: &'static str,
    pub label: EdgeLabel,
}

const fn rel(
    diagram: Diagram,
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

const CRUD: [&str; 4] = ["reads", "creates", "updates", "deletes"];
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
        Diagram::Rdra,
        &["actor"],
        &["usecase", "buc", "step"],
        "performs",
        Name,
    ),
    rel(
        Diagram::Rdra,
        &["usecase", "api"],
        &["entity"],
        CRUD[0],
        Name,
    ),
    rel(
        Diagram::Rdra,
        &["usecase", "api"],
        &["entity"],
        CRUD[1],
        Name,
    ),
    rel(
        Diagram::Rdra,
        &["usecase", "api"],
        &["entity"],
        CRUD[2],
        Name,
    ),
    rel(
        Diagram::Rdra,
        &["usecase", "api"],
        &["entity"],
        CRUD[3],
        Name,
    ),
    rel(Diagram::Rdra, &["usecase"], &["api"], "invokes", Name),
    rel(Diagram::Rdra, &["api"], &["dto"], "request", Name),
    rel(Diagram::Rdra, &["api"], &["dto"], "response", Name),
    rel(Diagram::Rdra, &["api"], &["dto"], "error_response", Name),
    rel(
        Diagram::Rdra,
        &["nfr", "condition"],
        &["usecase", "api", "system"],
        "applies_to",
        Name,
    ),
    // `.when(Cond)` on a `transitions` arm or a `chooses` value, derived: the use case
    // drives the guarded write. With `applies_to`, a condition to a use case has two
    // relations, so both are labelled.
    rel(Diagram::Rdra, &["condition"], &["usecase"], "when", Name),
    rel(
        Diagram::Rdra,
        &["condition"],
        &["variation", "entity"],
        "combines",
        Name,
    ),
    rel(
        Diagram::Rdra,
        &["nfr", "constraint"],
        &["quality"],
        "qualifies",
        Name,
    ),
    rel(
        Diagram::Rdra,
        &["constraint"],
        &["usecase", "api", "system", "entity", "dto"],
        "constrains",
        Name,
    ),
    rel(Diagram::Rdra, &["system"], &["entity"], "owns", Name),
    rel(Diagram::Rdra, &["usecase"], &["screen"], "displays", Name),
    rel(Diagram::Rdra, &["screen"], &["entity"], "shows", Name),
    rel(Diagram::Rdra, &["screen"], &["screen"], "navigates", Name),
    rel(Diagram::Rdra, &["field"], &["entity"], "maps_field", Name),
    rel(
        Diagram::Rdra,
        &["usecase", "extsystem"],
        &["event"],
        "raises",
        Name,
    ),
    rel(
        Diagram::Rdra,
        &["event"],
        &["usecase", "buc", "extsystem"],
        "triggers",
        Name,
    ),
    rel(
        Diagram::Rdra,
        &["timer"],
        &["usecase", "buc"],
        "triggers",
        Name,
    ),
    rel(
        Diagram::Rdra,
        &["buc", "system", "flow", "screen"],
        &["usecase", "api", "flow", "step", "field"],
        "contains",
        Name,
    ),
    rel(Diagram::Rdra, &["step"], &["step"], STEP_ORDER[0], Name),
    rel(Diagram::Rdra, &["step"], &["step"], STEP_ORDER[1], Name),
    rel(Diagram::Rdra, &["step"], &["step"], STEP_ORDER[2], Name),
    rel(Diagram::Rdra, &["step"], &["step"], STEP_ORDER[3], Name),
    rel(
        Diagram::Rdra,
        &["step"],
        &["usecase", "api", "event", "timer"],
        "covers",
        Name,
    ),
    rel(
        Diagram::Rdra,
        &["usecase"],
        &["usecase"],
        "compensates",
        Name,
    ),
    rel(Diagram::Rdra, &["buc"], &["business"], "belongs", Name),
    rel(
        Diagram::Rdra,
        &["actor"],
        &["permission"],
        "has_permission",
        Name,
    ),
    rel(
        Diagram::Rdra,
        &["usecase", "api"],
        &["permission"],
        "requires_permission",
        Name,
    ),
    rel(
        Diagram::Rdra,
        &["usecase", "api"],
        &["medium"],
        "requires_medium",
        Name,
    ),
    rel(Diagram::Rdra, &["requirement"], &["buc"], "motivates", Name),
    rel(Diagram::Rdra, &["requirement"], &["goal"], "achieves", Name),
    rel(
        Diagram::Rdra,
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
    rel(Diagram::Rdra, &["usecase"], &["state"], "transitions", Data),
    rel(
        Diagram::Rdra,
        &["entity"],
        &["entity"],
        CARDINALITIES[0],
        Name,
    ),
    rel(
        Diagram::Rdra,
        &["entity"],
        &["entity"],
        CARDINALITIES[1],
        Name,
    ),
    rel(
        Diagram::Rdra,
        &["entity"],
        &["entity"],
        CARDINALITIES[2],
        Name,
    ),
    rel(
        Diagram::Rdra,
        &["entity"],
        &["entity"],
        CARDINALITIES[3],
        Name,
    ),
    // context: an actor uses the system through the labelled BUCs; an external system
    // exchanges the labelled events.
    // An actor's edge is labelled with the BUCs it performs (`、`-joined).
    rel(Diagram::Context, &["actor"], &["system"], "performs", Data),
    rel(
        Diagram::Context,
        &["extsystem"],
        &["system"],
        "raises",
        Data,
    ),
    rel(
        Diagram::Context,
        &["system"],
        &["extsystem"],
        "triggers",
        Data,
    ),
    // requirement: the value tree.
    rel(
        Diagram::Requirement,
        &["requirement"],
        &["buc"],
        "motivates",
        Name,
    ),
    rel(
        Diagram::Requirement,
        &["goal"],
        &["requirement"],
        "achieved_by",
        Name,
    ),
    // business: a hand-off is labelled with its event.
    rel(Diagram::Business, &["buc"], &["buc"], "handoff", Data),
    rel(Diagram::Business, &["buc"], &["business"], "belongs", Name),
    // usecase: the UC複合図.
    rel(Diagram::Usecase, &["actor"], &["usecase"], "performs", Name),
    rel(
        Diagram::Usecase,
        &["event", "timer"],
        &["usecase"],
        "triggers",
        Name,
    ),
    rel(
        Diagram::Usecase,
        &["event"],
        &["extsystem"],
        "triggers",
        Name,
    ),
    rel(
        Diagram::Usecase,
        &["condition"],
        &["usecase"],
        "applies_to",
        Name,
    ),
    rel(Diagram::Usecase, &["condition"], &["usecase"], "when", Name),
    rel(
        Diagram::Usecase,
        &["usecase"],
        &["screen"],
        "displays",
        Name,
    ),
    rel(
        Diagram::Usecase,
        &["usecase", "extsystem"],
        &["event"],
        "raises",
        Name,
    ),
    rel(Diagram::Usecase, &["usecase"], &["entity"], CRUD[0], Name),
    rel(Diagram::Usecase, &["usecase"], &["entity"], CRUD[1], Name),
    rel(Diagram::Usecase, &["usecase"], &["entity"], CRUD[2], Name),
    rel(Diagram::Usecase, &["usecase"], &["entity"], CRUD[3], Name),
    rel(Diagram::Usecase, &["screen"], &["entity"], "shows", Name),
    rel(
        Diagram::Usecase,
        &["usecase"],
        &["state"],
        "transitions",
        Data,
    ),
    // robustness: actor -> boundary -> control -> entity, and the events between them.
    rel(Diagram::Robustness, &["actor"], &["screen"], "uses", Name),
    // Labelled with the use case that joins them.
    rel(
        Diagram::Robustness,
        &["screen"],
        &["api", "usecase"],
        "calls",
        Data,
    ),
    // Labelled with the CRUD letters of the control's operations.
    rel(
        Diagram::Robustness,
        &["api", "usecase"],
        &["entity"],
        "crud",
        Data,
    ),
    // Labelled with the event.
    rel(
        Diagram::Robustness,
        &["api", "usecase", "extsystem"],
        &["api", "usecase", "extsystem"],
        "event",
        Data,
    ),
    // A timer starts the use case: the one relation from a timer, so unlabelled.
    rel(
        Diagram::Robustness,
        &["timer"],
        &["api", "usecase"],
        "triggers",
        Name,
    ),
    // event: what raises each event and what it triggers.
    rel(
        Diagram::Event,
        &["usecase", "extsystem"],
        &["event"],
        "raises",
        Name,
    ),
    rel(
        Diagram::Event,
        &["event"],
        &["usecase", "buc", "extsystem"],
        "triggers",
        Name,
    ),
    rel(
        Diagram::Event,
        &["timer"],
        &["usecase", "buc"],
        "triggers",
        Name,
    ),
    // flow: steps in order, what each covers, and who performs one outside a lane.
    rel(Diagram::Flow, &["step"], &["step"], STEP_ORDER[0], Name),
    rel(Diagram::Flow, &["step"], &["step"], STEP_ORDER[1], Name),
    rel(Diagram::Flow, &["step"], &["step"], STEP_ORDER[2], Name),
    rel(Diagram::Flow, &["step"], &["step"], STEP_ORDER[3], Name),
    rel(
        Diagram::Flow,
        &["step"],
        &["usecase", "api", "event", "timer"],
        "covers",
        Name,
    ),
    rel(Diagram::Flow, &["actor"], &["step"], "performs", Name),
    // information: relates, state models, and the variation a column takes.
    rel(
        Diagram::Information,
        &["entity"],
        &["entity"],
        CARDINALITIES[0],
        Name,
    ),
    rel(
        Diagram::Information,
        &["entity"],
        &["entity"],
        CARDINALITIES[1],
        Name,
    ),
    rel(
        Diagram::Information,
        &["entity"],
        &["entity"],
        CARDINALITIES[2],
        Name,
    ),
    rel(
        Diagram::Information,
        &["entity"],
        &["entity"],
        CARDINALITIES[3],
        Name,
    ),
    rel(
        Diagram::Information,
        &["entity"],
        &["statemodel"],
        "statemodel",
        Name,
    ),
    // `variation <column>`.
    rel(
        Diagram::Information,
        &["entity"],
        &["variation"],
        "variation",
        Data,
    ),
    // condition: what a condition combines and applies to.
    rel(
        Diagram::Condition,
        &["condition"],
        &["variation", "statemodel"],
        "combines",
        Name,
    ),
    rel(
        Diagram::Condition,
        &["condition"],
        &["usecase", "api", "system"],
        "applies_to",
        Name,
    ),
    rel(
        Diagram::Condition,
        &["condition"],
        &["usecase"],
        "when",
        Name,
    ),
    // screen: a navigates is labelled with its `.via` use case, else `navigates`; a dashed
    // edge is 業務上の順番.
    rel(Diagram::Screen, &["screen"], &["screen"], "navigates", Name),
    rel(Diagram::Screen, &["screen"], &["screen"], "via", Data),
    rel(
        Diagram::Screen,
        &["screen"],
        &["screen"],
        "業務上の順番",
        Name,
    ),
];

/// The relations `diagram` draws from a `from` node to a `to` node.
pub fn edge_relations<'a>(
    diagram: Diagram,
    from: &'a str,
    to: &'a str,
) -> impl Iterator<Item = &'static EdgeRelation> + 'a {
    EDGE_RELATIONS
        .iter()
        .filter(move |r| r.diagram == diagram && r.from.contains(&from) && r.to.contains(&to))
}

/// The relation an unlabelled `diagram` edge from a `from` node to a `to` node stands for:
/// the one relation the diagram draws between the two kinds, when its label is its name.
pub fn implied_relation(diagram: Diagram, from: &str, to: &str) -> Option<&'static str> {
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
pub(crate) fn edge_line(
    diagram: Diagram,
    from: &str,
    arrow: &str,
    label: &str,
    to: &str,
) -> String {
    if implied_relation(diagram, id_kind(from), id_kind(to)) == Some(label) {
        format!("  {from} {arrow} {to}\n")
    } else {
        format!("  {from} {arrow}|{label}| {to}\n")
    }
}

/// The node id of the encoding contract, `<kind token>__<DSL id>`, the same in every kind.
pub(crate) fn contract_id(model: &SemanticModel, node: &NodeRef) -> Option<String> {
    Some(format!("{}__{}", node.kind_tag(), model.node_id(node)?))
}

/// The `%%` lines that open every diagram: kind, scope, and the vocabulary the body uses.
pub(crate) struct Header<'a> {
    pub(crate) diagram: Diagram,
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
        let mut out = format!(
            "%% rdra-ish diagram={} scope={scope}",
            self.diagram.as_str()
        );
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

/// Appends `<indent>%% desc <id>: <text>` for a node that has a description, its lines
/// joined by a space.
pub(crate) fn push_desc(
    out: &mut String,
    model: &SemanticModel,
    node: &NodeRef,
    id: &str,
    indent: &str,
) {
    let Some(description) = node_description(model, node) else {
        return;
    };
    let text = description
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if !text.is_empty() {
        out.push_str(&format!("{indent}%% desc {id}: {text}\n"));
    }
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

#[cfg(test)]
pub(crate) fn model_from(src: &str) -> SemanticModel {
    let (ast, _) = rdra_ish_syntax::parse(src);
    let (model, _) = rdra_ish_core::build_model(&ast);
    model
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_core::predicate::{ArgKind, Predicate, TRIGGER_PAIRS};
    use std::collections::BTreeSet;

    /// The relation label the rdra diagram writes for a predicate, when it draws one.
    fn drawn_labels(predicate: &'static str) -> Vec<&'static str> {
        match predicate {
            "relate" => CARDINALITIES.to_vec(),
            // Not an edge: a rule, an entity property, or no relation of the model.
            "holds" | "exposes" | "shares" | "outbox" | "coordinates" | "after" | "sets"
            | "links" | "relinks" | "unlinks" | "drives" | "chooses" | "forbidden"
            | "invariant" | "required" | "exclusive" | "when" => Vec::new(),
            // Drawn from the use case to the state it enters, labelled with the move.
            "transitions" => Vec::new(),
            _ => vec![predicate],
        }
    }

    #[test]
    fn the_rdra_rows_cover_every_pair_a_predicate_signature_allows() {
        // The kind a drawn argument is: `combines(Condition, Entity.column)` is drawn to the
        // entity, and no drawn predicate takes a value or a cardinality.
        let drawn = |kind: &ArgKind| match kind {
            ArgKind::Node(kind) => *kind,
            ArgKind::Column => "entity",
            ArgKind::Value | ArgKind::Card => panic!("a drawn argument is a {kind:?}"),
        };
        for predicate in Predicate::ALL {
            let signature = predicate.signature();
            let [from, to, ..] = signature.as_slice() else {
                continue;
            };
            // `triggers` takes only the listed pairs of its kinds.
            let pairs: Vec<(ArgKind, ArgKind)> = if *predicate == Predicate::Triggers {
                TRIGGER_PAIRS
                    .iter()
                    .map(|&(f, t)| (ArgKind::Node(f), ArgKind::Node(t)))
                    .collect()
            } else {
                from.iter()
                    .flat_map(|f| to.iter().map(move |t| (*f, *t)))
                    .collect()
            };
            let predicate = predicate.name();
            for label in drawn_labels(predicate) {
                for (f, t) in &pairs {
                    let (f, t) = (drawn(f), drawn(t));
                    assert!(
                        edge_relations(Diagram::Rdra, f, t).any(|r| r.relation == label),
                        "{predicate}({f}, {t}) has no `{label}` row"
                    );
                }
            }
        }
    }

    #[test]
    fn a_focused_diagram_draws_each_of_its_kinds_in_its_own_shape() {
        let diagrams: BTreeSet<&str> = EDGE_RELATIONS.iter().map(|r| r.diagram.as_str()).collect();
        for diagram in diagrams.into_iter().filter(|d| *d != "rdra") {
            let kinds: BTreeSet<&str> = EDGE_RELATIONS
                .iter()
                .filter(|r| r.diagram.as_str() == diagram)
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
            implied_relation(Diagram::Rdra, "actor", "usecase"),
            Some("performs")
        );
        assert_eq!(implied_relation(Diagram::Rdra, "usecase", "entity"), None);
        assert_eq!(implied_relation(Diagram::Robustness, "screen", "api"), None);
        assert_eq!(implied_relation(Diagram::Screen, "screen", "screen"), None);
        // A context actor edge is a `performs` whose label carries its BUCs.
        assert_eq!(implied_relation(Diagram::Context, "actor", "system"), None);
        let relations: Vec<_> = edge_relations(Diagram::Context, "actor", "system")
            .map(|r| (r.relation, r.label))
            .collect();
        assert_eq!(relations, [("performs", EdgeLabel::Data)]);
        assert_eq!(
            implied_relation(Diagram::Rdra, "screen", "screen"),
            Some("navigates")
        );
        assert_eq!(
            edge_line(Diagram::Rdra, "actor__A", "-.->", "performs", "usecase__U"),
            "  actor__A -.-> usecase__U\n"
        );
        assert_eq!(
            edge_line(Diagram::Rdra, "usecase__U", "-.->", "reads", "entity__E"),
            "  usecase__U -.->|reads| entity__E\n"
        );
    }
}
