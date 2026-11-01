//! The diagram encoding contract, checked on every sample for each kind:
//! each `.mmd` opens with the `%%` header, names each node `<kind>__<DSL id>` (the same id in
//! every kind; a variation's value is `variation__<Variation>__<value>`), tags a flowchart
//! node `:::<kind>` in its kind's shape (`rdra_ish_emit::node_shape`), draws edges only
//! between declared nodes, leaves an edge unlabelled exactly when its ends imply the relation
//! (`rdra_ish_emit::implied_relation`), and lists in `kinds:` exactly the kinds its body draws.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Every samples.txt root.
fn roots() -> Vec<String> {
    std::fs::read_to_string(repo().join("samples.txt"))
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// The roots outside the formal-verification fixtures, several of which violate their
/// lifecycle on purpose and so are refused by `state`, the one kind that reads it.
fn lifecycle_roots() -> Vec<String> {
    roots()
        .into_iter()
        .filter(|root| !root.starts_with("samples/formal-verification"))
        .collect()
}

fn buc_ids(root: &str) -> Vec<String> {
    ids(root, "buc")
}

/// The ids `list --kind <kind>` prints for `root`.
fn ids(root: &str, kind: &str) -> Vec<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(["list", root, "--kind", kind, "--format", "csv"])
        .output()
        .unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .skip(1)
        .filter_map(|line| line.split(',').next())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}

/// `rdra-ish diagram <root> --kind <kind> [--buc <id>]`, the written `.mmd`.
fn diagram(root: &str, kind: &str, buc: Option<&str>, out: &Path) -> String {
    match buc {
        Some(buc) => diagram_with(root, kind, &["--buc", buc], out),
        None => diagram_with(root, kind, &[], out),
    }
}

/// `rdra-ish diagram <root> --kind <kind> <args>`, the written `.mmd`.
fn diagram_with(root: &str, kind: &str, args: &[&str], out: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(["diagram", root, "--kind", kind])
        .args(args)
        .arg("--out")
        .arg(out)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{root} {kind} {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(out.with_extension("mmd")).unwrap()
}

/// The scopes each flowchart kind is checked in: the whole model, then each BUC.
fn scopes(root: &str) -> Vec<Vec<String>> {
    std::iter::once(Vec::new())
        .chain(
            buc_ids(root)
                .into_iter()
                .map(|buc| vec!["--buc".to_string(), buc]),
        )
        .collect()
}

/// Parse and check `text` as a `kind` flowchart, panicking with the diagram on a breach.
fn checked(kind: &str, text: &str, scope: &str) -> Flowchart {
    parse_flowchart(kind, text)
        .and_then(|chart| check_flowchart(&chart).map(|()| chart))
        .unwrap_or_else(|error| panic!("{scope}: {error}\n{text}"))
}

/// What a flowchart under the contract says, read line by line. Any line the contract
/// does not define fails the parse.
#[derive(Default)]
struct Flowchart {
    header_kinds: BTreeSet<String>,
    /// Node id to its `:::` class.
    nodes: HashMap<String, String>,
    /// `(from, relation, to)`: the label, or for an unlabelled edge the relation its ends
    /// imply.
    edges: Vec<(String, String, String)>,
    /// The edges written without a label.
    unlabelled: BTreeSet<(String, String, String)>,
    /// Each edge's arrow: `-->`, `-.->`, or `---`.
    arrows: HashMap<(String, String, String), String>,
    /// The edges a `%% derived` line follows: rdra-ish derived them from other facts.
    derived: BTreeSet<(String, String, String)>,
    class_defs: BTreeSet<String>,
}

/// A node line, `<id><open>"<label>"<close>:::<kind>`, as `(id, kind)`: `None` when the line
/// is no node, an error when it is one in a shape other than its kind's.
fn parse_node(line: &str) -> Option<Result<(String, String), String>> {
    let end = line.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
    let (id, rest) = line.split_at(end);
    if id.is_empty() || !rest.starts_with(['[', '(', '{', '>']) {
        return None;
    }
    let Some((body, kind)) = rest.rsplit_once(":::") else {
        return Some(Err(format!("node without `:::<kind>`: {line}")));
    };
    let Some((open, close)) = rdra_ish_emit::node_shape(kind) else {
        return Some(Err(format!("no shape for kind `{kind}`: {line}")));
    };
    let quoted = body
        .strip_prefix(open)
        .and_then(|b| b.strip_suffix(close))
        .filter(|b| b.len() >= 2 && b.starts_with('"') && b.ends_with('"'));
    Some(match quoted {
        Some(_) => Ok((id.to_string(), kind.to_string())),
        None => Err(format!("a {kind} is drawn `{open}\"..\"{close}`: {line}")),
    })
}

/// An edge line as `(from, arrow, label, to)`: `from arrow|label| to`, or `from arrow to`
/// without a label.
fn parse_edge(line: &str) -> Result<(String, String, Option<String>, String), String> {
    let unparsed = || format!("unparsed line: {line}");
    let (from, rest) = line.split_once(' ').ok_or_else(unparsed)?;
    let (arrow, label, to) = match rest.split_once('|') {
        Some((arrow, rest)) => {
            let (label, to) = rest.rsplit_once("| ").ok_or_else(unparsed)?;
            (arrow, Some(label.to_string()), to)
        }
        None => {
            let (arrow, to) = rest.split_once(' ').ok_or_else(unparsed)?;
            (arrow, None, to)
        }
    };
    if !["-->", "-.->", "---"].contains(&arrow) {
        return Err(format!("unknown arrow `{arrow}`: {line}"));
    }
    if to.contains(' ') {
        return Err(unparsed());
    }
    Ok((from.to_string(), arrow.to_string(), label, to.to_string()))
}

fn parse_flowchart(diagram: &str, text: &str) -> Result<Flowchart, String> {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default();
    if !first.starts_with(&format!("%% rdra-ish diagram={diagram} scope=")) {
        return Err(format!("first line is not the header: {first}"));
    }
    let mut chart = Flowchart::default();
    let mut seen_type = false;
    // `(from, arrow, label, to)` until every node is known, and the edge each `%% derived`
    // line follows.
    let mut raw: Vec<(String, String, Option<String>, String)> = Vec::new();
    let mut derived: Vec<usize> = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if let Some(kinds) = trimmed.strip_prefix("%% kinds: ") {
            if seen_type {
                return Err(format!("header line after the type line: {line}"));
            }
            chart.header_kinds = kinds
                .split(' ')
                .filter(|kind| *kind != "(none)")
                .map(str::to_string)
                .collect();
        } else if let Some(rest) = trimmed.strip_prefix("%% derived ") {
            let follows = raw
                .last()
                .is_some_and(|(from, _, _, to)| rest.starts_with(&format!("{from} -> {to} ")));
            if !follows {
                return Err(format!("`%% derived` after another edge: {line}"));
            }
            derived.push(raw.len() - 1);
        } else if trimmed.starts_with("%% ") {
        } else if trimmed == "flowchart LR" || trimmed == "flowchart TD" {
            seen_type = true;
        } else if trimmed == "end"
            || ((trimmed.starts_with("subgraph layer_") || trimmed.starts_with("subgraph group_"))
                && trimmed.ends_with("\"]"))
        {
        } else if let Some(rest) = trimmed.strip_prefix("subgraph ") {
            // A subgraph that is an element itself, such as a business holding its BUCs.
            let (id, _) = rest
                .split_once("[\"")
                .filter(|_| rest.ends_with("\"]"))
                .ok_or_else(|| format!("unparsed subgraph: {line}"))?;
            let (kind, _) = id
                .split_once("__")
                .ok_or_else(|| format!("subgraph id without `<kind>__`: {line}"))?;
            chart.nodes.insert(id.to_string(), kind.to_string());
        } else if let Some((kind, _)) = trimmed
            .strip_prefix("classDef ")
            .filter(|rest| rest.ends_with(';'))
            .and_then(|rest| rest.split_once(' '))
        {
            chart.class_defs.insert(kind.to_string());
        } else if let Some(node) = parse_node(trimmed) {
            let (id, class) = node?;
            chart.nodes.insert(id, class);
        } else {
            raw.push(parse_edge(trimmed)?);
        }
    }
    if !seen_type {
        return Err("no `flowchart LR` or `flowchart TD` line".to_string());
    }
    // An unlabelled edge stands for the one relation its ends imply; a labelled one names a
    // relation the diagram draws between its ends and that they do not imply. An end that is
    // not declared is left to `check_flowchart`.
    for (n, (from, arrow, label, to)) in raw.into_iter().enumerate() {
        let kinds = chart.nodes.get(&from).zip(chart.nodes.get(&to));
        let implied = kinds.and_then(|(f, t)| rdra_ish_emit::implied_relation(diagram, f, t));
        let relation = match (label, kinds) {
            (None, None) => String::new(),
            (None, Some((f, t))) => {
                let relation = implied.ok_or_else(|| {
                    format!("{from} {arrow} {to}: no label, and a {f} to a {t} is not one relation")
                })?;
                chart
                    .unlabelled
                    .insert((from.clone(), relation.to_string(), to.clone()));
                relation.to_string()
            }
            (Some(label), None) => label,
            (Some(label), Some((f, t))) => {
                if let Some(implied) = implied {
                    return Err(format!(
                        "{from} -{label}-> {to}: a {f} to a {t} is `{implied}`, drawn without a label"
                    ));
                }
                let listed = rdra_ish_emit::edge_relations(diagram, f, t)
                    .any(|r| r.label == rdra_ish_emit::EdgeLabel::Data || r.relation == label);
                if !listed {
                    return Err(format!(
                        "{from} -{label}-> {to}: {diagram} draws no such relation from a {f} to a {t}"
                    ));
                }
                label
            }
        };
        let edge = (from, relation, to);
        if derived.contains(&n) {
            chart.derived.insert(edge.clone());
        }
        chart.arrows.insert(edge.clone(), arrow);
        chart.edges.push(edge);
    }
    Ok(chart)
}

/// The contract's structural rules on one parsed flowchart.
fn check_flowchart(chart: &Flowchart) -> Result<(), String> {
    for (id, class) in &chart.nodes {
        let (kind, dsl_id) = id
            .split_once("__")
            .ok_or_else(|| format!("node id without `<kind>__`: {id}"))?;
        // A variation's value is `variation__<Variation>__<value>`, tagged `:::value`.
        let (kind, dsl_id) = match (class.as_str(), dsl_id.split_once("__")) {
            ("value", Some((variation, value))) if kind == "variation" && !variation.is_empty() => {
                ("value", value)
            }
            _ => (kind, dsl_id),
        };
        if kind != class || dsl_id.is_empty() {
            return Err(format!("node {id} is tagged `:::{class}`"));
        }
    }
    for (from, label, to) in &chart.edges {
        for end in [from, to] {
            if !chart.nodes.contains_key(end) {
                return Err(format!(
                    "edge {from} -{label}-> {to}: {end} is not declared"
                ));
            }
        }
    }
    let body_kinds: BTreeSet<String> = chart.nodes.values().cloned().collect();
    if chart.header_kinds != body_kinds {
        return Err(format!(
            "header kinds {:?} but body kinds {body_kinds:?}",
            chart.header_kinds
        ));
    }
    if chart.class_defs != body_kinds {
        return Err(format!(
            "classDef for {:?} but body kinds {body_kinds:?}",
            chart.class_defs
        ));
    }
    let mut edges = chart.edges.clone();
    edges.sort();
    edges.dedup();
    if edges.len() != chart.edges.len() {
        return Err("an edge is drawn twice".to_string());
    }
    Ok(())
}

#[test]
fn every_rdra_diagram_of_every_sample_keeps_the_contract() {
    let dir = scratch("diagram_contract_rdra");
    let mut checked = 0;
    for root in roots() {
        let scopes: Vec<Option<String>> = std::iter::once(None)
            .chain(buc_ids(&root).into_iter().map(Some))
            .collect();
        for buc in scopes {
            let text = diagram(&root, "rdra", buc.as_deref(), &dir.join("out"));
            let chart = parse_flowchart("rdra", &text)
                .and_then(|chart| check_flowchart(&chart).map(|()| chart))
                .unwrap_or_else(|error| panic!("{root} {buc:?}: {error}\n{text}"));
            assert!(!chart.nodes.is_empty(), "{root} {buc:?}: no nodes\n{text}");
            checked += 1;
        }
    }
    assert!(checked > 20, "only {checked} diagrams checked");
}

#[test]
fn the_contract_check_rejects_a_broken_diagram() {
    let good = "%% rdra-ish diagram=rdra scope=whole\n%% kinds: actor usecase\nflowchart LR\n  actor__A([\"A\"]):::actor\n  usecase__U(\"U\"):::usecase\n  actor__A -.-> usecase__U\n  classDef actor stroke:#333;\n  classDef usecase stroke:#333;\n";
    assert!(check_flowchart(&parse_flowchart("rdra", good).unwrap()).is_ok());
    for (broken, from, to) in [
        ("untagged", "\"):::usecase\n", "\")\n"),
        (
            "undeclared end",
            "usecase__U\n  classDef",
            "usecase__X\n  classDef",
        ),
        ("header kinds", "%% kinds: actor usecase", "%% kinds: actor"),
        ("wrong class", "\"]):::actor", "\"]):::usecase"),
        ("wrong shape", "actor__A([\"A\"])", "actor__A[\"A\"]"),
        ("no shape", "actor__A([", "actor__A\""),
        (
            "implied label",
            "actor__A -.-> usecase__U",
            "actor__A -.->|performs| usecase__U",
        ),
        (
            "unlisted label",
            "actor__A -.-> usecase__U",
            "actor__A -.->|reads| usecase__U",
        ),
        (
            "no relation between the ends",
            "actor__A -.-> usecase__U",
            "actor__A -.-> actor__A",
        ),
        ("emoji-era id", "  actor__A -.->", "  A -.->"),
    ] {
        let text = good.replace(from, to);
        assert_ne!(text, good, "{broken}");
        let result = parse_flowchart("rdra", &text).and_then(|chart| check_flowchart(&chart));
        assert!(result.is_err(), "{broken} passed:\n{text}");
    }
}

/// The flowchart kinds of `diagram`, `screen` aside.
const FLOWCHARTS: [&str; 10] = [
    "rdra",
    "context",
    "requirement",
    "business",
    "usecase",
    "robustness",
    "event",
    "flow",
    "information",
    "condition",
];

#[test]
fn every_drawn_kind_has_a_shape_and_every_unlabelled_edge_one_relation() {
    let dir = scratch("diagram_contract_shapes");
    let mut kinds: BTreeSet<String> = BTreeSet::new();
    let mut unlabelled: BTreeSet<(String, String)> = BTreeSet::new();
    for root in roots() {
        for diagram in FLOWCHARTS.into_iter().chain(["screen"]) {
            let text = diagram_with(&root, diagram, &[], &dir.join("out"));
            let chart = checked(diagram, &text, &format!("{root} {diagram}"));
            for (id, kind) in &chart.nodes {
                // A subgraph line names an element too; any other line is in its kind's shape.
                let subgraph = text.contains(&format!("subgraph {id}[\""));
                assert!(
                    subgraph || rdra_ish_emit::node_shape(kind).is_some(),
                    "{root} {diagram}: {id} is a {kind}, which has no shape"
                );
                kinds.insert(kind.clone());
            }
            for edge @ (from, relation, to) in &chart.unlabelled {
                let (f, t) = (&chart.nodes[from], &chart.nodes[to]);
                let relations: Vec<_> = rdra_ish_emit::edge_relations(diagram, f, t).collect();
                assert!(
                    matches!(relations.as_slice(), [only] if only.relation == relation
                        && only.label == rdra_ish_emit::EdgeLabel::Name),
                    "{root} {diagram}: {edge:?} is unlabelled, but a {f} to a {t} is {} relations",
                    relations.len()
                );
                unlabelled.insert((diagram.to_string(), relation.clone()));
            }
        }
    }
    for kind in [
        "actor",
        "usecase",
        "screen",
        "api",
        "entity",
        "extsystem",
        "event",
        "timer",
        "condition",
        "goal",
        "requirement",
        "buc",
        "state",
        "statemodel",
        "value",
        "step",
        "system",
    ] {
        assert!(kinds.contains(kind), "no sample draws a {kind}");
    }
    for (diagram, relation) in [
        ("rdra", "performs"),
        ("requirement", "achieved_by"),
        ("usecase", "displays"),
        ("robustness", "uses"),
        ("event", "raises"),
        ("flow", "covers"),
        ("information", "statemodel"),
        ("condition", "applies_to"),
    ] {
        assert!(
            unlabelled.contains(&(diagram.to_string(), relation.to_string())),
            "no unlabelled {relation} in any {diagram} diagram"
        );
    }
}

#[test]
fn an_element_has_one_id_in_every_scope() {
    let dir = scratch("diagram_contract_scopes");
    let root = "samples/ec-site";
    let whole = parse_flowchart("rdra", &diagram(root, "rdra", None, &dir.join("w"))).unwrap();
    for buc in buc_ids(root) {
        let scoped =
            parse_flowchart("rdra", &diagram(root, "rdra", Some(&buc), &dir.join("b"))).unwrap();
        for id in scoped.nodes.keys() {
            assert!(whole.nodes.contains_key(id), "{buc}: {id} not in whole");
        }
    }
}

/// What a state diagram under the contract says: one composite state per state model, its
/// states, its transitions labelled with use-case ids, and its start and end markers.
#[derive(Default)]
struct StateChart {
    header_kinds: BTreeSet<String>,
    /// `statemodel__<Entity>_<column>` to its label `<Entity>.<column>`.
    models: Vec<(String, String)>,
    /// State id to the state model that declares it.
    states: HashMap<String, String>,
    /// (state model, from, to, use case).
    transitions: Vec<(String, String, String, String)>,
    starts: Vec<String>,
    ends: Vec<String>,
    /// An index form's one note per state model: (state model, label).
    index_notes: Vec<(String, String)>,
    /// (state, class) from the `class <state> <class>` lines.
    classes: Vec<(String, String)>,
    class_defs: BTreeSet<String>,
    legend: bool,
}

fn parse_state(text: &str) -> Result<StateChart, String> {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default();
    if !first.starts_with("%% rdra-ish diagram=state scope=") {
        return Err(format!("first line is not the header: {first}"));
    }
    let mut chart = StateChart::default();
    let mut seen_type = false;
    let mut current: Option<String> = None;
    let mut transition: Option<String> = None;
    for line in lines {
        let trimmed = line.trim();
        if let Some(usecase) = trimmed.strip_prefix("%% transition ") {
            transition = Some(usecase.to_string());
        } else if let Some(kinds) = trimmed.strip_prefix("%% kinds: ") {
            chart.header_kinds = kinds
                .split(' ')
                .filter(|kind| *kind != "(none)")
                .map(str::to_string)
                .collect();
        } else if trimmed.starts_with("%% legend: ") {
            chart.legend = true;
        } else if trimmed.starts_with("%% ") {
        } else if let Some(rest) = trimmed.strip_prefix("classDef ") {
            let name = rest.split(' ').next().unwrap_or_default();
            chart.class_defs.insert(name.to_string());
        } else if let Some(rest) = trimmed.strip_prefix("class ") {
            let (state, class) = rest
                .split_once(' ')
                .ok_or_else(|| format!("unparsed class line: {line}"))?;
            chart.classes.push((state.to_string(), class.to_string()));
        } else if trimmed == "stateDiagram-v2" {
            seen_type = true;
        } else if trimmed == "}" {
            current = current
                .take()
                .map(|_| None)
                .ok_or_else(|| format!("`}}` outside a state model: {line}"))?;
        } else if let Some(rest) = trimmed
            .strip_prefix("state \"")
            .and_then(|rest| rest.strip_suffix(" {"))
        {
            let (label, id) = rest
                .split_once("\" as ")
                .ok_or_else(|| format!("unparsed state model: {line}"))?;
            if current.is_some() {
                return Err(format!("nested state model: {line}"));
            }
            chart.models.push((id.to_string(), label.to_string()));
            current = Some(id.to_string());
        } else {
            let model = current
                .clone()
                .ok_or_else(|| format!("line outside a state model: {line}"))?;
            if let Some(id) = trimmed.strip_prefix("[*] --> ") {
                chart.starts.push(id.to_string());
            } else if let Some(id) = trimmed.strip_suffix(" --> [*]") {
                chart.ends.push(id.to_string());
            } else if let Some(rest) = trimmed.strip_prefix("state \"") {
                let (label, id) = rest
                    .split_once("\" as ")
                    .ok_or_else(|| format!("unparsed state: {line}"))?;
                if id.starts_with("index__") {
                    chart.index_notes.push((model, label.to_string()));
                } else if let Some(other) = chart.states.insert(id.to_string(), model.clone()) {
                    if other != model {
                        return Err(format!("state {id} in both {other} and {model}"));
                    }
                }
            } else {
                let (arm, _label) = trimmed
                    .split_once(" : ")
                    .ok_or_else(|| format!("unparsed line: {line}"))?;
                let usecase = transition
                    .take()
                    .ok_or_else(|| format!("no `%% transition` line before: {line}"))?;
                let (from, to) = arm
                    .split_once(" --> ")
                    .ok_or_else(|| format!("unparsed line: {line}"))?;
                chart
                    .transitions
                    .push((model, from.to_string(), to.to_string(), usecase));
            }
        }
    }
    if !seen_type || current.is_some() {
        return Err("no `stateDiagram-v2` line, or an unclosed state model".to_string());
    }
    Ok(chart)
}

fn check_state(chart: &StateChart) -> Result<(), String> {
    let mut models = BTreeSet::new();
    for (id, label) in &chart.models {
        let expected = id
            .strip_prefix("statemodel__")
            .map(|rest| label.replacen('.', "_", 1) == rest);
        if expected != Some(true) || !models.insert(id) {
            return Err(format!(
                "state model {id} \"{label}\" is not one per column"
            ));
        }
    }
    for (state, model) in &chart.states {
        let label = &chart.models.iter().find(|(id, _)| id == model).unwrap().1;
        let entity = label.split('.').next().unwrap_or_default();
        if !state.starts_with(&format!("state__{entity}_")) {
            return Err(format!("state {state} in {model}"));
        }
    }
    for (model, from, to, usecase) in &chart.transitions {
        for end in [from, to] {
            if chart.states.get(end) != Some(model) {
                return Err(format!("{from} --> {to}: {end} is not a state of {model}"));
            }
        }
        if !usecase.starts_with("usecase__") {
            return Err(format!("{from} --> {to} is labelled {usecase}"));
        }
    }
    for state in chart.starts.iter().chain(&chart.ends) {
        if !chart.states.contains_key(state) {
            return Err(format!("start or end {state} is not declared"));
        }
    }
    for state in chart.states.keys() {
        let leaves = chart
            .transitions
            .iter()
            .any(|(_, from, to, _)| from == state && to != state);
        let unreachable = chart
            .classes
            .iter()
            .any(|(id, class)| id == state && class == "stateunreachable");
        if unreachable {
            if leaves || chart.ends.contains(state) || chart.starts.contains(state) {
                return Err(format!("{state}: an unreachable state has a way in or out"));
            }
        } else if leaves == chart.ends.contains(state) {
            return Err(format!(
                "{state}: an end state must be one with no transition out"
            ));
        }
    }
    check_state_index(chart)?;
    check_state_classes(chart)?;
    let mut body_kinds = BTreeSet::new();
    for (kind, present) in [
        ("statemodel", !chart.models.is_empty()),
        (
            "state",
            !chart.states.is_empty() || !chart.index_notes.is_empty(),
        ),
        ("usecase", !chart.transitions.is_empty()),
    ] {
        if present {
            body_kinds.insert(kind.to_string());
        }
    }
    if chart.header_kinds != body_kinds {
        return Err(format!(
            "header kinds {:?} but body kinds {body_kinds:?}",
            chart.header_kinds
        ));
    }
    Ok(())
}

/// An index form draws one `<n> 状態 / <m> 遷移 (--entity <Entity>)` note per state model and
/// nothing else; a full model has no note.
fn check_state_index(chart: &StateChart) -> Result<(), String> {
    if chart.index_notes.is_empty() {
        return Ok(());
    }
    if chart.index_notes.len() != chart.models.len()
        || !chart.states.is_empty()
        || !chart.transitions.is_empty()
        || !chart.classes.is_empty()
    {
        return Err("an index draws one note per state model and no state".to_string());
    }
    for (model, label) in &chart.index_notes {
        let entity = chart
            .models
            .iter()
            .find(|(id, _)| id == model)
            .and_then(|(_, label)| label.split('.').next())
            .unwrap_or_default();
        let counts = label
            .strip_suffix(&format!(" (--entity {entity})"))
            .and_then(|rest| rest.split_once(" 状態 / "))
            .and_then(|(n, rest)| Some((n, rest.strip_suffix(" 遷移")?)))
            .filter(|(n, m)| n.parse::<usize>().is_ok() && m.parse::<usize>().is_ok());
        if counts.is_none() {
            return Err(format!("index note of {model} is `{label}`"));
        }
    }
    Ok(())
}

/// A class line colours a declared state by what the lifecycle check found; each class is
/// defined, and the legend comment is there.
fn check_state_classes(chart: &StateChart) -> Result<(), String> {
    for (state, class) in &chart.classes {
        if !chart.states.contains_key(state) {
            return Err(format!("class of {state}, which is not declared"));
        }
        if !["stateunreachable", "statedeadlock", "stateend"].contains(&class.as_str()) {
            return Err(format!("{state} has the class {class}"));
        }
        if !chart.class_defs.contains(class) {
            return Err(format!("class {class} has no classDef"));
        }
        let leaves = chart
            .transitions
            .iter()
            .any(|(_, from, to, _)| from == state && to != state);
        let ok = match class.as_str() {
            "stateend" => chart.ends.contains(state),
            "statedeadlock" => leaves,
            _ => true,
        };
        if !ok {
            return Err(format!("{state} is wrongly {class}"));
        }
    }
    let ends: BTreeSet<&String> = chart.ends.iter().collect();
    let end_classed: BTreeSet<&String> = chart
        .classes
        .iter()
        .filter(|(_, class)| class == "stateend")
        .map(|(state, _)| state)
        .collect();
    if ends != end_classed {
        return Err("an end state has no stateend class, or the reverse".to_string());
    }
    let full = !chart.models.is_empty() && chart.index_notes.is_empty();
    if full && !chart.legend {
        return Err("a full state model has no `%% legend:` line".to_string());
    }
    Ok(())
}

/// The entities whose state models the unscoped diagram indexes, from its model labels.
fn indexed_entities(chart: &StateChart) -> Vec<String> {
    let entities: BTreeSet<String> = chart
        .models
        .iter()
        .filter_map(|(_, label)| label.split('.').next().map(str::to_string))
        .collect();
    entities.into_iter().collect()
}

#[test]
fn every_state_diagram_of_every_sample_keeps_the_contract() {
    let dir = scratch("diagram_contract_state");
    let mut drawn = 0;
    for root in lifecycle_roots() {
        let scopes: Vec<Option<String>> = std::iter::once(None)
            .chain(buc_ids(&root).into_iter().map(Some))
            .collect();
        for buc in scopes {
            let text = diagram(&root, "state", buc.as_deref(), &dir.join("out"));
            let chart = parse_state(&text)
                .and_then(|chart| check_state(&chart).map(|()| chart))
                .unwrap_or_else(|error| panic!("{root} {buc:?}: {error}\n{text}"));
            drawn += usize::from(!chart.models.is_empty());
        }
    }
    assert!(drawn > 10, "only {drawn} state diagrams draw a state model");
}

#[test]
fn the_unscoped_state_diagram_is_an_index_and_each_entity_draws_its_models() {
    let dir = scratch("diagram_contract_state_entity");
    let mut full = 0;
    for root in lifecycle_roots() {
        let text = diagram(&root, "state", None, &dir.join("index"));
        let index = parse_state(&text)
            .and_then(|chart| check_state(&chart).map(|()| chart))
            .unwrap_or_else(|error| panic!("{root}: {error}\n{text}"));
        assert_eq!(index.index_notes.len(), index.models.len(), "{root}");
        for entity in indexed_entities(&index) {
            let text = diagram_with(&root, "state", &["--entity", &entity], &dir.join("full"));
            let chart = parse_state(&text)
                .and_then(|chart| check_state(&chart).map(|()| chart))
                .unwrap_or_else(|error| panic!("{root} {entity}: {error}\n{text}"));
            assert!(chart.index_notes.is_empty(), "{root} {entity}");
            assert!(chart
                .models
                .iter()
                .all(|(_, l)| l.starts_with(&format!("{entity}."))));
            for (model, label) in &index.index_notes {
                let Some((_, at)) = chart.models.iter().find(|(id, _)| id == model) else {
                    continue;
                };
                let moves = chart.transitions.iter().filter(|t| &t.0 == model).count();
                let drawn = chart.states.values().filter(|m| *m == model).count();
                let (n, rest) = label.split_once(" 状態 / ").unwrap();
                let m = rest.split_once(" 遷移").unwrap().0;
                assert_eq!(m.parse::<usize>().unwrap(), moves, "{root} {at}");
                assert!(n.parse::<usize>().unwrap() >= drawn, "{root} {at}");
            }
            full += 1;
        }
    }
    assert!(full > 5, "only {full} entity diagrams");
}

/// The columns a state header line `%% <prefix>: <a> <b> (<why>)` names.
fn header_columns(text: &str, prefix: &str) -> BTreeSet<String> {
    text.lines()
        .find_map(|line| line.strip_prefix(&format!("%% {prefix}: ")))
        .and_then(|rest| rest.split_once(" ("))
        .map(|(columns, _)| columns.split(' ').map(str::to_string).collect())
        .unwrap_or_default()
}

/// An Enum column a `sets` writes is a state axis of `check` even without `transitions`: the
/// state diagram names it on `%% set-only:`, not on `omitted:` (#61).
#[test]
fn a_state_axis_set_by_sets_is_not_omitted() {
    let dir = scratch("diagram_contract_state_set_only");
    let mut set_only = 0;
    for root in lifecycle_roots() {
        let text = diagram(&root, "state", None, &dir.join("index"));
        let (omitted, set) = (
            header_columns(&text, "omitted"),
            header_columns(&text, "set-only"),
        );
        assert!(omitted.is_disjoint(&set), "{root}\n{text}");
        set_only += set.len();
        if root == "samples/patent-ops" {
            for column in ["OfficeAction.kind", "Patent.last_payment"] {
                assert!(set.contains(column), "{root}: {column}\n{text}");
            }
        }
    }
    assert!(set_only >= 2, "only {set_only} set-only columns");
}

#[test]
fn the_state_contract_check_rejects_a_broken_diagram() {
    let good = "%% rdra-ish diagram=state scope=entity:Doc\n%% kinds: statemodel state usecase\n%% legend: stateunreachable = a, statedeadlock = b, stateend = c\nstateDiagram-v2\n  state \"Doc.status\" as statemodel__Doc_status {\n    [*] --> state__Doc_draft\n    state \"draft\" as state__Doc_draft\n    state \"done\" as state__Doc_done\n    %% transition usecase__Publish\n    state__Doc_draft --> state__Doc_done : Publish\n    state__Doc_done --> [*]\n  }\n  classDef stateend fill:#dfd\n  class state__Doc_done stateend\n";
    assert!(check_state(&parse_state(good).unwrap()).is_ok());
    for (broken, from, to) in [
        ("missing end", "    state__Doc_done --> [*]\n", ""),
        (
            "end with a way out",
            "    state__Doc_done --> [*]\n",
            "    state__Doc_done --> [*]\n    state__Doc_draft --> [*]\n",
        ),
        (
            "transition is not an id",
            "%% transition usecase__Publish",
            "%% transition Publish",
        ),
        (
            "transition without its id",
            "    %% transition usecase__Publish\n",
            "",
        ),
        ("emoji-era id", "as state__Doc_done", "as Doc_done"),
        (
            "no legend",
            "%% legend: stateunreachable = a, statedeadlock = b, stateend = c\n",
            "",
        ),
        ("no classDef", "  classDef stateend fill:#dfd\n", ""),
        (
            "end without its class",
            "  class state__Doc_done stateend\n",
            "",
        ),
        (
            "end class on a state with a way out",
            "  class state__Doc_done stateend\n",
            "  class state__Doc_done stateend\n  class state__Doc_draft stateend\n",
        ),
        (
            "deadlock on a state with no way out",
            "  class state__Doc_done stateend\n",
            "  class state__Doc_done statedeadlock\n",
        ),
        (
            "unknown class",
            "  class state__Doc_done stateend\n",
            "  class state__Doc_done stateend\n  class state__Doc_draft stateweird\n",
        ),
        (
            "header kinds",
            "statemodel state usecase",
            "statemodel state",
        ),
    ] {
        let text = good.replace(from, to);
        assert_ne!(text, good, "{broken}");
        let result = parse_state(&text).and_then(|chart| check_state(&chart));
        assert!(result.is_err(), "{broken} passed:\n{text}");
    }
}

#[test]
fn a_state_and_its_use_cases_have_the_ids_the_rdra_diagram_gives_them() {
    let dir = scratch("diagram_contract_state_ids");
    for root in lifecycle_roots() {
        let rdra = parse_flowchart("rdra", &diagram(&root, "rdra", None, &dir.join("r"))).unwrap();
        let index = parse_state(&diagram(&root, "state", None, &dir.join("i"))).unwrap();
        let ids: Vec<String> = indexed_entities(&index)
            .iter()
            .flat_map(|entity| {
                let text = diagram_with(&root, "state", &["--entity", entity], &dir.join("s"));
                let state = parse_state(&text).unwrap();
                let usecases: Vec<String> = state.transitions.iter().map(|t| t.3.clone()).collect();
                state.states.into_keys().chain(usecases).collect::<Vec<_>>()
            })
            .collect();
        for id in &ids {
            assert!(rdra.nodes.contains_key(id), "{root}: {id} not in rdra");
        }
    }
}

#[test]
fn every_event_diagram_of_every_sample_keeps_the_contract() {
    let dir = scratch("diagram_contract_event");
    let mut drawn = 0;
    for root in roots() {
        let scopes: Vec<Option<String>> = std::iter::once(None)
            .chain(buc_ids(&root).into_iter().map(Some))
            .collect();
        for buc in scopes {
            let text = diagram(&root, "event", buc.as_deref(), &dir.join("out"));
            let chart = parse_flowchart("event", &text)
                .and_then(|chart| check_flowchart(&chart).map(|()| chart))
                .unwrap_or_else(|error| panic!("{root} {buc:?}: {error}\n{text}"));
            for (from, label, to) in &chart.edges {
                assert!(
                    label == "raises" || label == "triggers",
                    "{root} {buc:?}: {from} -{label}-> {to}"
                );
            }
            assert!(
                !chart.nodes.values().any(|kind| kind == "state"),
                "{root} {buc:?}: a state node\n{text}"
            );
            drawn += usize::from(!chart.edges.is_empty());
        }
    }
    assert!(drawn > 10, "only {drawn} event diagrams draw an edge");
}

#[test]
fn an_event_diagram_node_has_the_id_the_rdra_diagram_gives_it() {
    let dir = scratch("diagram_contract_event_ids");
    for root in roots() {
        let rdra = parse_flowchart("rdra", &diagram(&root, "rdra", None, &dir.join("r"))).unwrap();
        let event =
            parse_flowchart("event", &diagram(&root, "event", None, &dir.join("e"))).unwrap();
        for (id, kind) in &event.nodes {
            assert_eq!(rdra.nodes.get(id), Some(kind), "{root}: {id}");
        }
        for edge in &event.edges {
            assert!(rdra.edges.contains(edge), "{root}: {edge:?} not in rdra");
        }
    }
}

#[test]
fn every_usecase_diagram_of_every_sample_keeps_the_contract() {
    let dir = scratch("diagram_contract_usecase");
    let mut derived = 0;
    for root in roots() {
        let mut scopes = scopes(&root);
        scopes.extend(
            ids(&root, "usecase")
                .into_iter()
                .take(3)
                .map(|usecase| vec!["--usecase".to_string(), usecase]),
        );
        for scope in scopes {
            let args: Vec<&str> = scope.iter().map(String::as_str).collect();
            let text = diagram_with(&root, "usecase", &args, &dir.join("out"));
            let chart = checked("usecase", &text, &format!("{root} {scope:?}"));
            for (from, label, to) in &chart.edges {
                let usecase_end = from.starts_with("usecase__") || to.starts_with("usecase__");
                let screen_shows = label == "shows" && from.starts_with("screen__");
                let event_link = from.starts_with("event__") || to.starts_with("event__");
                assert!(
                    usecase_end || screen_shows || event_link,
                    "{root} {scope:?}: {from} -{label}-> {to}"
                );
            }
            for kind in ["api", "dto", "permission", "medium", "field", "buc"] {
                assert!(
                    !chart.nodes.values().any(|k| k == kind),
                    "{root} {scope:?}: a {kind} node\n{text}"
                );
            }
            derived += chart.derived.len();
        }
    }
    assert!(derived > 20, "only {derived} derived edges");
}

#[test]
fn a_usecase_diagram_draws_what_the_rdra_diagram_declares() {
    let dir = scratch("diagram_contract_usecase_ids");
    for root in roots() {
        let rdra = parse_flowchart("rdra", &diagram(&root, "rdra", None, &dir.join("r"))).unwrap();
        let usecase =
            parse_flowchart("usecase", &diagram(&root, "usecase", None, &dir.join("u"))).unwrap();
        for (id, kind) in &usecase.nodes {
            assert_eq!(rdra.nodes.get(id), Some(kind), "{root}: {id}");
        }
        for edge in &usecase.edges {
            if !usecase.derived.contains(edge) {
                assert!(rdra.edges.contains(edge), "{root}: {edge:?} not in rdra");
            }
        }
    }
}

#[test]
fn every_robustness_diagram_of_every_sample_keeps_the_contract_and_the_grammar() {
    let dir = scratch("diagram_contract_robustness");
    let (mut edges, mut timers) = (0, 0);
    for root in roots() {
        let mut scopes = scopes(&root);
        scopes.extend(
            ids(&root, "usecase")
                .into_iter()
                .take(3)
                .map(|usecase| vec!["--usecase".to_string(), usecase]),
        );
        for scope in scopes {
            let args: Vec<&str> = scope.iter().map(String::as_str).collect();
            let text = diagram_with(&root, "robustness", &args, &dir.join("out"));
            let chart = checked("robustness", &text, &format!("{root} {scope:?}"));
            let kind = |id: &String| chart.nodes[id].as_str();
            let control = |id: &String| matches!(kind(id), "api" | "usecase");
            let boundary = |id: &String| matches!(kind(id), "screen" | "extsystem" | "timer");
            for (from, label, to) in &chart.edges {
                let ok = match kind(from) {
                    "actor" => kind(to) == "screen",
                    "screen" => control(to),
                    "api" | "usecase" => {
                        matches!(kind(to), "entity") || control(to) || boundary(to)
                    }
                    "extsystem" | "timer" => control(to),
                    _ => false,
                };
                assert!(
                    ok && kind(from) != "entity",
                    "{root} {scope:?}: {from} -{label}-> {to}"
                );
                if kind(to) == "entity" {
                    assert!(control(from), "{root} {scope:?}: {from} -> {to}");
                }
                // A timer starts the use case, the one relation from a timer (#61, #43).
                if kind(from) == "timer" {
                    let edge = (from.clone(), label.clone(), to.clone());
                    assert!(
                        label == "triggers" && chart.unlabelled.contains(&edge),
                        "{root} {scope:?}: {from} -{label}-> {to}"
                    );
                    timers += 1;
                }
            }
            for kind in [
                "dto",
                "permission",
                "medium",
                "field",
                "buc",
                "event",
                "state",
            ] {
                assert!(
                    !chart.nodes.values().any(|k| k == kind),
                    "{root} {scope:?}: a {kind} node\n{text}"
                );
            }
            edges += chart.edges.len();
        }
    }
    assert!(edges > 100, "only {edges} edges");
    assert!(timers > 5, "only {timers} timer edges");
}

/// The participant ids and the message endpoints of a `sequence` diagram, after checking its
/// header and that every message joins two declared participants.
fn parse_sequence(text: &str, scope: &str) -> Result<Vec<String>, String> {
    let mut lines = text.lines();
    let header = lines.next().ok_or("empty")?;
    let want = format!("%% rdra-ish diagram=sequence scope=usecase:{scope} model=");
    if !header.starts_with(&want) {
        return Err(format!("header: {header}"));
    }
    let kinds = lines
        .next()
        .and_then(|l| l.strip_prefix("%% kinds: "))
        .ok_or("no kinds line")?;
    if !lines.next().is_some_and(|l| l.starts_with("%% omitted: ")) {
        return Err("no omitted line".to_string());
    }
    if lines.next() != Some("sequenceDiagram") {
        return Err("no sequenceDiagram".to_string());
    }
    let mut participants: Vec<String> = Vec::new();
    let mut drawn_kinds: BTreeSet<String> = BTreeSet::new();
    let mut messages = 0;
    for line in lines {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix("participant ")
            .or_else(|| trimmed.strip_prefix("actor "))
        {
            let (id, _) = rest.split_once(" as ").ok_or(format!("alias: {line}"))?;
            let (kind, _) = id.split_once("__").ok_or(format!("id: {line}"))?;
            drawn_kinds.insert(kind.to_string());
            participants.push(id.to_string());
        } else if trimmed.starts_with("alt ") || trimmed == "else guard fails" || trimmed == "end" {
        } else if let Some(rest) = trimmed.strip_prefix("Note over ") {
            let (id, _) = rest.split_once(": ").ok_or(format!("note: {line}"))?;
            if !participants.iter().any(|p| p == id) {
                return Err(format!("note over an undeclared {id}"));
            }
        } else {
            let (ends, _) = trimmed.split_once(": ").ok_or(format!("message: {line}"))?;
            let (from, to) = ends
                .split_once(" ->> ")
                .or_else(|| ends.split_once(" -->> "))
                .or_else(|| ends.split_once(" -) "))
                .ok_or(format!("arrow: {line}"))?;
            for id in [from, to] {
                if !participants.iter().any(|p| p == id) {
                    return Err(format!("{id} is not a participant: {line}"));
                }
            }
            messages += 1;
        }
    }
    let listed: BTreeSet<String> = kinds.split(' ').map(str::to_string).collect();
    if listed != drawn_kinds {
        return Err(format!("kinds {listed:?} but drawn {drawn_kinds:?}"));
    }
    let unique: BTreeSet<&String> = participants.iter().collect();
    if unique.len() != participants.len() || messages == 0 && participants.len() > 1 {
        return Err("duplicate participant or no message".to_string());
    }
    Ok(participants)
}

#[test]
fn every_sequence_diagram_of_every_sample_keeps_the_contract() {
    let dir = scratch("diagram_contract_sequence");
    let mut drawn = 0;
    for root in roots() {
        let rdra = parse_flowchart("rdra", &diagram(&root, "rdra", None, &dir.join("r"))).unwrap();
        for usecase in ids(&root, "usecase").into_iter().take(4) {
            let text = diagram_with(
                &root,
                "sequence",
                &["--usecase", &usecase],
                &dir.join("out"),
            );
            let participants = parse_sequence(&text, &usecase)
                .unwrap_or_else(|why| panic!("{root} {usecase}: {why}\n{text}"));
            for id in &participants {
                assert!(
                    rdra.nodes.contains_key(id),
                    "{root} {usecase}: {id} not in rdra"
                );
            }
            drawn += participants.len();
        }
    }
    assert!(drawn > 20, "only {drawn} participants");
}

#[test]
fn a_sequence_diagram_needs_exactly_one_use_case() {
    let root = &roots()[0];
    let usecases = ids(root, "usecase");
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
            .current_dir(repo())
            .args(["diagram", root, "--kind", "sequence"])
            .args(args)
            .arg("--out")
            .arg(scratch("diagram_contract_sequence_args").join("out"))
            .output()
            .unwrap()
    };
    let none = run(&[]);
    assert!(!none.status.success());
    assert!(String::from_utf8_lossy(&none.stderr).contains("exactly one --usecase"));
    let two = run(&["--usecase", &usecases[0], "--usecase", &usecases[1]]);
    assert!(!two.status.success());
    let buc = run(&["--buc", &buc_ids(root)[0]]);
    assert!(!buc.status.success());
}

#[test]
fn the_contract_check_rejects_a_misplaced_derived_line() {
    let good = "%% rdra-ish diagram=usecase scope=whole\n%% kinds: actor usecase\nflowchart LR\n  actor__A([\"A\"]):::actor\n  usecase__U(\"U\"):::usecase\n  actor__A -.-> usecase__U\n  %% derived actor__A -> usecase__U via buc__B\n  classDef actor stroke:#333;\n  classDef usecase stroke:#333;\n";
    let chart = parse_flowchart("usecase", good).unwrap();
    assert_eq!(chart.derived.len(), 1);
    let broken = good.replace(
        "%% derived actor__A -> usecase__U",
        "%% derived usecase__U -> actor__A",
    );
    assert!(parse_flowchart("usecase", &broken).is_err(), "{broken}");
}

#[test]
fn every_business_diagram_of_every_sample_keeps_the_contract() {
    let dir = scratch("diagram_contract_business");
    let mut drawn = 0;
    for root in roots() {
        for scope in scopes(&root) {
            let args: Vec<&str> = scope.iter().map(String::as_str).collect();
            let text = diagram_with(&root, "business", &args, &dir.join("out"));
            let chart = checked("business", &text, &format!("{root} {scope:?}"));
            // A business flow: businesses holding BUCs, and no actor, external system, or
            // layer.
            for kind in chart.nodes.values() {
                assert!(
                    ["business", "buc"].contains(&kind.as_str()),
                    "{root} {scope:?}: a {kind} node\n{text}"
                );
            }
            assert!(
                !text.contains("subgraph layer_") && !text.contains("%% layers:"),
                "{root} {scope:?}: a layer\n{text}"
            );
            let mut handoffs = BTreeSet::new();
            for edge @ (from, label, to) in &chart.edges {
                if label == "belongs" {
                    // A BUC of several businesses sits in one and belongs to the others.
                    assert!(
                        from.starts_with("buc__")
                            && to.starts_with("business__")
                            && !chart.derived.contains(edge),
                        "{root} {scope:?}: {from} -{label}-> {to}\n{text}"
                    );
                    continue;
                }
                // A hand-off: BUC to another BUC, named by the event label, derived through
                // the event ids.
                let vias = derived_vias(&text, from, to);
                assert!(
                    from.starts_with("buc__")
                        && to.starts_with("buc__")
                        && from != to
                        && chart.derived.contains(edge)
                        && !vias.is_empty()
                        && vias.iter().all(|via| via.starts_with("event__"))
                        && handoffs.insert((from.clone(), label.clone(), to.clone())),
                    "{root} {scope:?}: {from} -{label}-> {to}\n{text}"
                );
            }
            drawn += handoffs.len();
        }
    }
    assert!(drawn > 10, "only {drawn} hand-offs drawn");
}

#[test]
fn a_business_diagram_draws_what_the_rdra_diagram_declares() {
    let dir = scratch("diagram_contract_business_ids");
    for root in roots() {
        let rdra = parse_flowchart("rdra", &diagram(&root, "rdra", None, &dir.join("r"))).unwrap();
        let text = diagram(&root, "business", None, &dir.join("b"));
        let business = parse_flowchart("business", &text).unwrap();
        for (id, kind) in &business.nodes {
            assert_eq!(rdra.nodes.get(id), Some(kind), "{root}: {id}");
        }
        for edge @ (from, _, to) in &business.edges {
            if business.derived.contains(edge) {
                // A hand-off: a use case of `from` raises each event and it triggers a use
                // case of `to` or `to` itself, which the rdra diagram draws.
                for event in derived_vias(&text, from, to) {
                    let raised = rdra
                        .edges
                        .iter()
                        .any(|(_, label, target)| label == "raises" && *target == event);
                    let triggers = rdra
                        .edges
                        .iter()
                        .any(|(source, label, _)| label == "triggers" && *source == event);
                    assert!(
                        raised && triggers,
                        "{root}: {event} of {edge:?} is not raised and triggered in rdra"
                    );
                }
            } else {
                assert!(rdra.edges.contains(edge), "{root}: {edge:?} not in rdra");
            }
        }
    }
}

#[test]
fn clinic_ops_business_flow_has_its_eight_hand_offs() {
    let dir = scratch("diagram_contract_business_clinic_ops");
    let text = diagram("samples/clinic-ops", "business", None, &dir.join("b"));
    let chart = parse_flowchart("business", &text).unwrap();
    let handoffs = chart
        .edges
        .iter()
        .filter(|(_, label, _)| label != "belongs")
        .count();
    assert_eq!(handoffs, 8, "{text}");
    assert!(
        chart.edges.contains(&(
            "buc__BucVisitCheckIn".to_string(),
            "受付完了".to_string(),
            "buc__BucClinicalEncounter".to_string()
        )),
        "{text}"
    );
}

#[test]
fn an_element_subgraph_is_a_node_of_its_kind() {
    let good = "%% rdra-ish diagram=business scope=whole\n%% kinds: business buc\nflowchart LR\n  subgraph business__B[\"B\"]\n    buc__U[\"U\"]:::buc\n  end\n  classDef business stroke:#333;\n  classDef buc stroke:#333;\n";
    let chart = parse_flowchart("business", good).unwrap();
    assert!(check_flowchart(&chart).is_ok());
    assert_eq!(
        chart.nodes.get("business__B").map(String::as_str),
        Some("business")
    );
    for (broken, from, to) in [
        ("no kind", "subgraph business__B[", "subgraph B["),
        (
            "unterminated",
            "business__B[\"B\"]\n",
            "business__B[\"B\"\n",
        ),
    ] {
        let text = good.replace(from, to);
        assert_ne!(text, good, "{broken}");
        assert!(
            parse_flowchart("business", &text).is_err(),
            "{broken} passed:\n{text}"
        );
    }
}

#[test]
fn every_information_diagram_of_every_sample_keeps_the_contract() {
    let dir = scratch("diagram_contract_information");
    let mut drawn = 0;
    for root in roots() {
        for scope in scopes(&root) {
            let args: Vec<&str> = scope.iter().map(String::as_str).collect();
            let text = diagram_with(&root, "information", &args, &dir.join("out"));
            let chart = checked("information", &text, &format!("{root} {scope:?}"));
            for (from, label, to) in &chart.edges {
                let fits = match label.as_str() {
                    "1:1" | "1:N" | "N:1" | "N:M" => to.starts_with("entity__"),
                    "statemodel" => to.starts_with("statemodel__"),
                    _ => label.starts_with("variation ") && to.starts_with("variation__"),
                };
                assert!(
                    fits && from.starts_with("entity__"),
                    "{root} {scope:?}: {from} -{label}-> {to}"
                );
            }
            drawn += usize::from(!chart.edges.is_empty());
        }
    }
    assert!(drawn > 20, "only {drawn} information diagrams draw an edge");
}

/// `(child, cardinality, parent)` for each relationship line of an `erDiagram`.
fn er_relates(text: &str) -> BTreeSet<(String, String, String)> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.trim().split(' ');
            let (from, glyph, to) = (parts.next()?, parts.next()?, parts.next()?);
            let card = match glyph {
                "||--||" => "1:1",
                "||--o{" | "||--|{" => "1:N",
                "}o--||" | "}|--||" => "N:1",
                "}o--o{" => "N:M",
                _ => return None,
            };
            Some((
                format!("entity__{from}"),
                card.to_string(),
                format!("entity__{to}"),
            ))
        })
        .collect()
}

#[test]
fn an_information_diagram_draws_the_relates_of_the_er_diagram() {
    let dir = scratch("diagram_contract_information_er");
    let mut relates = 0;
    for root in roots() {
        for scope in scopes(&root) {
            let args: Vec<&str> = scope.iter().map(String::as_str).collect();
            let info = parse_flowchart(
                "information",
                &diagram_with(&root, "information", &args, &dir.join("i")),
            )
            .unwrap();
            let er = er_relates(&diagram_with(&root, "er", &args, &dir.join("e")));
            let drawn: BTreeSet<(String, String, String)> = info
                .edges
                .iter()
                .filter(|(_, label, _)| ["1:1", "1:N", "N:1", "N:M"].contains(&label.as_str()))
                .cloned()
                .collect();
            assert_eq!(drawn, er, "{root} {scope:?}");
            let entities: BTreeSet<&String> = info
                .nodes
                .iter()
                .filter(|(_, kind)| *kind == "entity")
                .map(|(id, _)| id)
                .collect();
            assert!(
                er.iter()
                    .all(|(from, _, to)| entities.contains(from) && entities.contains(to)),
                "{root} {scope:?}"
            );
            relates += drawn.len();
        }
    }
    assert!(relates > 50, "only {relates} relates compared");
}

#[test]
fn an_information_diagram_node_has_the_id_the_rdra_and_state_diagrams_give_it() {
    let dir = scratch("diagram_contract_information_ids");
    for root in lifecycle_roots() {
        let rdra = parse_flowchart("rdra", &diagram(&root, "rdra", None, &dir.join("r"))).unwrap();
        let state = parse_state(&diagram(&root, "state", None, &dir.join("s"))).unwrap();
        let info = parse_flowchart(
            "information",
            &diagram(&root, "information", None, &dir.join("i")),
        )
        .unwrap();
        let state_models: BTreeSet<&String> = state.models.iter().map(|(id, _)| id).collect();
        let info_models: BTreeSet<&String> = info
            .nodes
            .iter()
            .filter(|(_, kind)| *kind == "statemodel")
            .map(|(id, _)| id)
            .collect();
        assert_eq!(info_models, state_models, "{root}");
        for (id, kind) in info.nodes.iter().filter(|(_, kind)| *kind != "statemodel") {
            assert_eq!(rdra.nodes.get(id), Some(kind), "{root}: {id}");
        }
        for edge in info
            .edges
            .iter()
            .filter(|(_, _, to)| to.starts_with("entity__"))
        {
            assert!(rdra.edges.contains(edge), "{root}: {edge:?} not in rdra");
        }
    }
}

#[test]
fn every_condition_diagram_of_every_sample_keeps_the_contract() {
    let dir = scratch("diagram_contract_condition");
    let (mut derived, mut values) = (0, 0);
    for root in roots() {
        for scope in scopes(&root) {
            let args: Vec<&str> = scope.iter().map(String::as_str).collect();
            let text = diagram_with(&root, "condition", &args, &dir.join("out"));
            let chart = checked("condition", &text, &format!("{root} {scope:?}"));
            for (from, label, to) in &chart.edges {
                let fits = match label.as_str() {
                    "combines" => to.starts_with("variation__") || to.starts_with("statemodel__"),
                    "applies_to" => ["usecase__", "api__", "system__"]
                        .iter()
                        .any(|kind| to.starts_with(kind)),
                    "when" => to.starts_with("usecase__"),
                    _ => false,
                };
                assert!(
                    fits && from.starts_with("condition__"),
                    "{root} {scope:?}: {from} -{label}-> {to}"
                );
            }
            for (id, kind) in &chart.nodes {
                let variation = id.rsplit_once("__").map(|(variation, _)| variation);
                assert!(
                    kind != "value" || variation.is_some_and(|v| chart.nodes.contains_key(v)),
                    "{root} {scope:?}: value {id} outside its variation"
                );
            }
            derived += chart.derived.len();
            values += chart.nodes.values().filter(|kind| *kind == "value").count();
        }
    }
    assert!(derived >= 8, "only {derived} combines read from holds");
    assert!(values > 40, "only {values} values drawn");
}

/// A `.when(Cond)` on a `transitions` arm or a `chooses` value is drawn as a `when` edge from
/// the condition to the use case in each diagram that draws `applies_to`, labelled because a
/// condition to a use case is two relations there, and derived from the arms and values it
/// guards: the same edges in every kind.
#[test]
fn a_when_guard_is_a_labelled_derived_edge_wherever_applies_to_is_drawn() {
    let dir = scratch("diagram_contract_when");
    let mut guards = 0;
    for root in roots() {
        let mut drawn: Vec<BTreeSet<(String, String, String)>> = Vec::new();
        for kind in ["rdra", "usecase", "condition"] {
            assert_eq!(
                rdra_ish_emit::implied_relation(kind, "condition", "usecase"),
                None,
                "{kind}: a condition to a use case is `applies_to` or `when`"
            );
            let text = diagram(&root, kind, None, &dir.join(kind));
            let chart = checked(kind, &text, &format!("{root} {kind}"));
            let when: BTreeSet<_> = chart
                .edges
                .iter()
                .filter(|(_, label, _)| label == "when")
                .cloned()
                .collect();
            for edge @ (from, _, to) in &when {
                assert!(
                    from.starts_with("condition__") && to.starts_with("usecase__"),
                    "{root} {kind}: {edge:?}"
                );
                assert!(chart.derived.contains(edge), "{root} {kind}: {edge:?}");
                assert_eq!(chart.arrows[edge], "-.->", "{root} {kind}: {edge:?}");
                let vias = derived_vias(&text, from, to);
                assert!(
                    !vias.is_empty()
                        && vias.iter().all(|via| {
                            let via = via.trim();
                            via.starts_with("transitions ") || via.starts_with("chooses ")
                        }),
                    "{root} {kind}: {edge:?} via {vias:?}"
                );
            }
            drawn.push(when);
        }
        assert!(
            drawn.windows(2).all(|pair| pair[0] == pair[1]),
            "{root}: rdra, usecase, and condition draw different `when` edges: {drawn:?}"
        );
        guards += drawn[0].len();
    }
    assert!(guards >= 2, "only {guards} `when` edges drawn");
}

#[test]
fn the_contract_check_rejects_a_misplaced_value() {
    let good = "%% rdra-ish diagram=condition scope=whole\n%% kinds: variation value\nflowchart LR\n  subgraph variation__V[\"V\"]\n    variation__V__a[\"A\"]:::value\n  end\n  classDef variation stroke:#333;\n  classDef value stroke:#333;\n";
    assert!(check_flowchart(&parse_flowchart("condition", good).unwrap()).is_ok());
    for (broken, from, to) in [
        ("no variation", "variation__V__a[", "variation__a["),
        ("another kind", "variation__V__a[", "condition__V__a["),
        ("untagged value", "\"]:::value", "\"]:::variation"),
    ] {
        let text = good.replace(from, to);
        assert_ne!(text, good, "{broken}");
        let result = parse_flowchart("condition", &text).and_then(|chart| check_flowchart(&chart));
        assert!(result.is_err(), "{broken} passed:\n{text}");
    }
}

#[test]
fn a_condition_diagram_node_has_the_id_the_rdra_and_state_diagrams_give_it() {
    let dir = scratch("diagram_contract_condition_ids");
    for root in lifecycle_roots() {
        let rdra = parse_flowchart("rdra", &diagram(&root, "rdra", None, &dir.join("r"))).unwrap();
        let state = parse_state(&diagram(&root, "state", None, &dir.join("s"))).unwrap();
        let condition = parse_flowchart(
            "condition",
            &diagram(&root, "condition", None, &dir.join("c")),
        )
        .unwrap();
        for (id, kind) in &condition.nodes {
            match kind.as_str() {
                "statemodel" => assert!(
                    state.models.iter().any(|(model, _)| model == id),
                    "{root}: {id} not in state"
                ),
                "value" => {}
                _ => assert_eq!(rdra.nodes.get(id), Some(kind), "{root}: {id}"),
            }
        }
        for edge in condition
            .edges
            .iter()
            .filter(|(_, _, to)| !to.starts_with("statemodel__"))
        {
            assert!(rdra.edges.contains(edge), "{root}: {edge:?} not in rdra");
            assert_eq!(
                condition.derived.contains(edge),
                rdra.derived.contains(edge),
                "{root}: {edge:?} derived in one diagram only"
            );
        }
    }
}

/// A flow model with lanes, which no sample declares: `performs(Actor, Step)`.
const LANES: &str = r#"
actor Clerk "Clerk"
actor Courier "Courier"
actor Manager "Manager"
buc BucShip "Ship"
flow ShipFlow "Ship flow"
flow ReturnFlow "Return flow"
step Pick "Pick"
step Pack "Pack"
step Handover "Handover"
step Receive "Receive"
usecase PickItems "Pick items"
contains(BucShip, ShipFlow)
contains(BucShip, ReturnFlow)
contains(ShipFlow, Pick)
contains(ShipFlow, Pack)
contains(ShipFlow, Handover)
contains(ReturnFlow, Receive)
precedes(Pick, Pack)
precedes(Pack, Handover)
covers(Pick, PickItems)
performs(Clerk, Pick)
performs(Clerk, Pack)
performs(Clerk, Receive)
performs(Courier, Handover)
performs(Manager, Handover)
"#;

/// The flow contract on one parsed flow diagram: ordering edges between steps, `covers`
/// from a step to a use case, API, event, or timer, `performs` from an actor to a step, and
/// each lane `lane__<Flow>__<Actor>` inside a drawn flow.
fn check_flow(chart: &Flowchart) -> Result<(), String> {
    for (from, label, to) in &chart.edges {
        let fits = match label.as_str() {
            "precedes" | "branches" | "excepts" | "repeats" => {
                from.starts_with("step__") && to.starts_with("step__")
            }
            "covers" => {
                from.starts_with("step__")
                    && ["usecase__", "api__", "event__", "timer__"]
                        .iter()
                        .any(|kind| to.starts_with(kind))
            }
            "performs" => from.starts_with("actor__") && to.starts_with("step__"),
            _ => false,
        };
        if !fits {
            return Err(format!("{from} -{label}-> {to}"));
        }
    }
    for (id, kind) in &chart.nodes {
        if ![
            "flow", "step", "lane", "actor", "usecase", "api", "event", "timer",
        ]
        .contains(&kind.as_str())
        {
            return Err(format!("a {kind} node {id}"));
        }
        if kind == "lane" {
            let flow = id
                .strip_prefix("lane__")
                .and_then(|rest| rest.split_once("__"))
                .map(|(flow, _)| format!("flow__{flow}"));
            if !flow.is_some_and(|flow| chart.nodes.contains_key(&flow)) {
                return Err(format!("lane {id} outside a drawn flow"));
            }
        }
    }
    Ok(())
}

#[test]
fn every_flow_diagram_of_every_sample_keeps_the_contract() {
    let dir = scratch("diagram_contract_flow");
    let (mut drawn, mut orders) = (0, BTreeSet::new());
    for root in roots() {
        for scope in scopes(&root) {
            let args: Vec<&str> = scope.iter().map(String::as_str).collect();
            let text = diagram_with(&root, "flow", &args, &dir.join("out"));
            let chart = checked("flow", &text, &format!("{root} {scope:?}"));
            check_flow(&chart).unwrap_or_else(|error| panic!("{root} {scope:?}: {error}\n{text}"));
            drawn += usize::from(chart.nodes.values().any(|kind| kind == "flow"));
            orders.extend(chart.edges.iter().map(|(_, label, to)| {
                let kind = to.split_once("__").map_or("", |(kind, _)| kind);
                format!("{label} {kind}")
            }));
        }
    }
    assert!(drawn >= 5, "only {drawn} flow diagrams draw a flow");
    for edge in [
        "precedes step",
        "branches step",
        "excepts step",
        "repeats step",
        "covers usecase",
        "covers event",
        "covers timer",
    ] {
        assert!(orders.contains(edge), "no `{edge}` edge in any sample");
    }
}

#[test]
fn a_flow_diagram_with_lanes_keeps_the_contract() {
    let dir = scratch("diagram_contract_flow_lanes");
    let model = dir.join("lanes.rdra");
    std::fs::write(&model, LANES).unwrap();
    let root = model.to_str().unwrap();
    let text = diagram(root, "flow", None, &dir.join("out"));
    let chart = checked("flow", &text, "lanes");
    check_flow(&chart).unwrap_or_else(|error| panic!("{error}\n{text}"));
    let lanes: BTreeSet<&str> = chart
        .nodes
        .iter()
        .filter(|(_, kind)| *kind == "lane")
        .map(|(id, _)| id.as_str())
        .collect();
    assert_eq!(
        lanes,
        BTreeSet::from(["lane__ReturnFlow__Clerk", "lane__ShipFlow__Clerk"]),
        "{text}"
    );
    assert!(
        text.contains("    subgraph lane__ShipFlow__Clerk[\"Clerk\"]\n      %% lane lane__ShipFlow__Clerk: actor__Clerk\n"),
        "{text}"
    );
    let rdra = parse_flowchart("rdra", &diagram(root, "rdra", None, &dir.join("r"))).unwrap();
    for edge in &chart.edges {
        assert!(rdra.edges.contains(edge), "{edge:?} not in rdra");
    }
    for broken in ["lane__NoFlow__Clerk", "lane__ShipFlow"] {
        let text = text.replace("lane__ShipFlow__Clerk[", &format!("{broken}["));
        let result = parse_flowchart("flow", &text).and_then(|chart| check_flow(&chart));
        assert!(result.is_err(), "{broken} passed:\n{text}");
    }
}

#[test]
fn a_flow_diagram_draws_what_the_rdra_diagram_declares() {
    let dir = scratch("diagram_contract_flow_ids");
    for root in roots() {
        let rdra = parse_flowchart("rdra", &diagram(&root, "rdra", None, &dir.join("r"))).unwrap();
        let flow = parse_flowchart("flow", &diagram(&root, "flow", None, &dir.join("f"))).unwrap();
        for (id, kind) in flow.nodes.iter().filter(|(_, kind)| *kind != "lane") {
            assert_eq!(rdra.nodes.get(id), Some(kind), "{root}: {id}");
        }
        for edge in &flow.edges {
            assert!(rdra.edges.contains(edge), "{root}: {edge:?} not in rdra");
        }
        assert!(flow.derived.is_empty(), "{root}: a derived edge");
    }
}

/// The ids the `%% derived <from> -> <to> via ...` line of `text` names.
fn derived_vias(text: &str, from: &str, to: &str) -> Vec<String> {
    let prefix = format!("%% derived {from} -> {to} via ");
    text.lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
        .map(|vias| vias.split(',').map(str::to_string).collect())
        .unwrap_or_default()
}

#[test]
fn every_context_diagram_of_every_sample_keeps_the_contract() {
    let dir = scratch("diagram_contract_context");
    let mut edges = 0;
    for root in roots() {
        for scope in scopes(&root) {
            let args: Vec<&str> = scope.iter().map(String::as_str).collect();
            let text = diagram_with(&root, "context", &args, &dir.join("out"));
            let chart = checked("context", &text, &format!("{root} {scope:?}"));
            for kind in chart.nodes.values() {
                assert!(
                    ["actor", "extsystem", "system"].contains(&kind.as_str()),
                    "{root} {scope:?}: a {kind} node\n{text}"
                );
            }
            assert!(
                !text.contains("subgraph"),
                "{root} {scope:?}: a subgraph\n{text}"
            );
            assert!(
                !text.contains("%% layers:"),
                "{root} {scope:?}: a layers line\n{text}"
            );
            let mut pairs = BTreeSet::new();
            for edge @ (from, label, to) in &chart.edges {
                // One edge per outside node, system, and direction, labelled with what it
                // was derived through: an actor's BUCs (a `performs`), an external
                // system's events.
                let via_kinds: &[&str] = if from.starts_with("actor__") {
                    &["buc__", "usecase__", "step__"]
                } else if from.starts_with("extsystem__") || to.starts_with("extsystem__") {
                    &["event__"]
                } else {
                    &[]
                };
                let system_end = from.starts_with("system__") || to.starts_with("system__");
                let vias = derived_vias(&text, from, to);
                assert!(
                    !via_kinds.is_empty()
                        && system_end
                        && !label.is_empty()
                        && !chart.unlabelled.contains(edge)
                        && chart.derived.contains(edge)
                        && !vias.is_empty()
                        && vias
                            .iter()
                            .all(|via| via_kinds.iter().any(|kind| via.starts_with(kind)))
                        && pairs.insert((from.clone(), to.clone())),
                    "{root} {scope:?}: {from} -{label}-> {to}\n{text}"
                );
            }
            edges += chart.edges.len();
        }
    }
    assert!(edges > 50, "only {edges} context edges");
}

#[test]
fn a_context_diagram_node_has_the_id_the_rdra_diagram_gives_it() {
    let dir = scratch("diagram_contract_context_ids");
    for root in roots() {
        let rdra = parse_flowchart("rdra", &diagram(&root, "rdra", None, &dir.join("r"))).unwrap();
        let text = diagram(&root, "context", None, &dir.join("c"));
        let context = parse_flowchart("context", &text).unwrap();
        for (id, kind) in &context.nodes {
            if id == "system__Model" {
                assert!(
                    text.contains("  %% synthesized system__Model: "),
                    "{root}: system__Model without its reason"
                );
                continue;
            }
            assert_eq!(rdra.nodes.get(id), Some(kind), "{root}: {id}");
        }
        for line in text
            .lines()
            .filter_map(|l| l.trim().strip_prefix("%% derived "))
        {
            let (_, vias) = line.split_once(" via ").unwrap();
            for via in vias.split(',') {
                assert!(
                    rdra.nodes.contains_key(via),
                    "{root}: via {via} not in rdra"
                );
            }
        }
    }
}

/// The requirement ids that sit in the diagram's `subgraph group_out_of_scope`.
fn apart_requirements(text: &str) -> BTreeSet<String> {
    let mut apart = BTreeSet::new();
    let mut inside = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with("subgraph group_out_of_scope[") {
            inside = true;
        } else if line == "end" {
            inside = false;
        } else if inside {
            if let Some(Ok((id, _))) = parse_node(line) {
                apart.insert(id);
            }
        }
    }
    apart
}

#[test]
fn every_requirement_diagram_of_every_sample_keeps_the_contract() {
    let dir = scratch("diagram_contract_requirement");
    let mut labels = BTreeSet::new();
    let mut apart_seen = BTreeSet::new();
    for root in roots() {
        for scope in scopes(&root) {
            let args: Vec<&str> = scope.iter().map(String::as_str).collect();
            let text = diagram_with(&root, "requirement", &args, &dir.join("out"));
            let chart = checked("requirement", &text, &format!("{root} {scope:?}"));
            // A value tree: goals, requirements, and BUCs, and no layer subgraph.
            for kind in chart.nodes.values() {
                assert!(
                    ["goal", "requirement", "buc"].contains(&kind.as_str()),
                    "{root} {scope:?}: a {kind} node\n{text}"
                );
            }
            assert!(
                !text.contains("subgraph layer_") && !text.contains("%% layers:"),
                "{root} {scope:?}: a layer\n{text}"
            );
            for (from, label, to) in &chart.edges {
                let source = from.split_once("__").map_or("", |(kind, _)| kind);
                let fits = match label.as_str() {
                    "motivates" => {
                        source == "requirement"
                            && to.starts_with("buc__")
                            && !chart
                                .derived
                                .contains(&(from.clone(), label.clone(), to.clone()))
                    }
                    // `achieves(Requirement, Goal)` read from the goal: always derived, and
                    // `via` names the requirement the edge ends at.
                    "achieved_by" => {
                        source == "goal"
                            && to.starts_with("requirement__")
                            && derived_vias(&text, from, to) == [to.clone()]
                    }
                    _ => false,
                };
                assert!(fits, "{root} {scope:?}: {from} -{label}-> {to}\n{text}");
                labels.insert(label.clone());
            }
            // A requirement apart has only dashed edges, and any other only solid ones.
            let apart = apart_requirements(&text);
            for (edge @ (from, ..), arrow) in &chart.arrows {
                if from.starts_with("goal__") {
                    continue;
                }
                assert_eq!(
                    arrow == "-.->",
                    apart.contains(from),
                    "{root} {scope:?}: {edge:?}\n{text}"
                );
            }
            for id in &apart {
                assert!(
                    chart
                        .nodes
                        .get(id)
                        .is_some_and(|kind| kind == "requirement"),
                    "{root} {scope:?}: {id} apart is not a requirement"
                );
            }
            apart_seen.extend(apart);
        }
    }
    assert_eq!(
        labels,
        BTreeSet::from(["achieved_by", "motivates"].map(String::from))
    );
    assert!(
        apart_seen.contains("requirement__ReqNoOnlineConsultation"),
        "no sample draws a requirement apart: {apart_seen:?}"
    );
}

#[test]
fn a_requirement_diagram_draws_what_the_rdra_diagram_declares() {
    let dir = scratch("diagram_contract_requirement_ids");
    for root in roots() {
        let rdra = parse_flowchart("rdra", &diagram(&root, "rdra", None, &dir.join("r"))).unwrap();
        let requirement = parse_flowchart(
            "requirement",
            &diagram(&root, "requirement", None, &dir.join("q")),
        )
        .unwrap();
        for (id, kind) in &requirement.nodes {
            // The out-of-scope subgraph is no element; its members are.
            assert_eq!(rdra.nodes.get(id), Some(kind), "{root}: {id}");
        }
        for edge @ (from, label, to) in &requirement.edges {
            // A derived `achieved_by` is the declared `achieves`, read from the goal.
            let declared = if requirement.derived.contains(edge) {
                assert_eq!(label, "achieved_by", "{root}: {edge:?}");
                (to.clone(), "achieves".to_string(), from.clone())
            } else {
                edge.clone()
            };
            assert!(
                rdra.edges.contains(&declared),
                "{root}: {edge:?} not in rdra"
            );
        }
    }
}

/// The arrow `chart` draws for `edge`.
fn arrow_of<'a>(chart: &'a Flowchart, edge: &(String, String, String)) -> &'a str {
    chart
        .arrows
        .get(edge)
        .unwrap_or_else(|| panic!("no arrow for {edge:?}"))
}

#[test]
fn every_screen_diagram_of_every_sample_keeps_the_contract() {
    let dir = scratch("diagram_contract_screen");
    let (mut solid, mut dashed) = (0, 0);
    for root in roots() {
        for scope in scopes(&root) {
            let args: Vec<&str> = scope.iter().map(String::as_str).collect();
            let text = diagram_with(&root, "screen", &args, &dir.join("out"));
            let chart = checked("screen", &text, &format!("{root} {scope:?}"));
            assert!(
                chart.nodes.values().all(|kind| kind == "screen"),
                "{root} {scope:?}\n{text}"
            );
            for edge in &chart.edges {
                // A dashed edge is the business order, derived; a solid one is a navigates.
                let derived = chart.derived.contains(edge);
                if arrow_of(&chart, edge) == "-.->" {
                    assert!(
                        derived && edge.1 == "業務上の順番",
                        "{root} {scope:?}: {edge:?}"
                    );
                    dashed += 1;
                } else {
                    assert!(!derived, "{root} {scope:?}: {edge:?}");
                    solid += 1;
                }
            }
        }
    }
    assert!(
        solid > 5 && dashed > 5,
        "{solid} solid, {dashed} dashed edges"
    );
}

#[test]
fn a_screen_diagram_draws_the_screens_and_navigates_of_the_rdra_diagram() {
    let dir = scratch("diagram_contract_screen_ids");
    for root in roots() {
        let rdra = parse_flowchart("rdra", &diagram(&root, "rdra", None, &dir.join("r"))).unwrap();
        let screen =
            parse_flowchart("screen", &diagram(&root, "screen", None, &dir.join("s"))).unwrap();
        let rdra_screens: BTreeSet<&String> = rdra
            .nodes
            .iter()
            .filter(|(_, kind)| *kind == "screen")
            .map(|(id, _)| id)
            .collect();
        assert_eq!(
            screen.nodes.keys().collect::<BTreeSet<_>>(),
            rdra_screens,
            "{root}"
        );
        for edge @ (from, _, to) in &screen.edges {
            if !screen.derived.contains(edge) {
                let navigates = (from.clone(), "navigates".to_string(), to.clone());
                assert!(
                    rdra.edges.contains(&navigates),
                    "{root}: {edge:?} not in rdra"
                );
            }
        }
    }
}

/// The ids `lint <root>` reports under `rule`.
fn lint_subjects(root: &str, rule: &str) -> BTreeSet<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(["lint", root, "--format", "json"])
        .output()
        .unwrap();
    let issues: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{root}: {error}: {output:?}"));
    issues
        .as_array()
        .unwrap()
        .iter()
        .filter(|issue| issue["rule"] == rule)
        .inspect(|issue| assert_eq!(issue["severity"], "info", "{issue}"))
        .map(|issue| issue["subject_id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn lint_reports_each_screen_the_screen_diagram_leaves_unconnected() {
    let dir = scratch("diagram_contract_screen_lint");
    let mut reported = 0;
    for root in roots() {
        let chart =
            parse_flowchart("screen", &diagram(&root, "screen", None, &dir.join("s"))).unwrap();
        let unconnected: BTreeSet<String> = chart
            .nodes
            .keys()
            .filter(|id| {
                !chart
                    .edges
                    .iter()
                    .any(|(from, _, to)| from == *id || to == *id)
            })
            .map(|id| id.trim_start_matches("screen__").to_string())
            .collect();
        let linted = lint_subjects(&root, "screen-no-transition");
        assert_eq!(linted, unconnected, "{root}");
        reported += linted.len();
    }
    assert!(reported > 0, "no sample has an unconnected screen");
}

#[test]
fn clinic_ops_screen_transitions_cover_check_in_encounter_and_payment() {
    let dir = scratch("diagram_contract_screen_clinic_ops");
    let text = diagram("samples/clinic-ops", "screen", None, &dir.join("s"));
    let chart = parse_flowchart("screen", &text).unwrap();
    for (from, label, to) in [
        ("CheckInScreen", "受付（初診・再診）", "EligibilityScreen"),
        (
            "EncounterWorkspaceScreen",
            "診察・診断の記録",
            "DiagnosisScreen",
        ),
        ("BillCalculationScreen", "会計の呼出", "PaymentWindowScreen"),
    ] {
        let edge = (
            format!("screen__{from}"),
            label.to_string(),
            format!("screen__{to}"),
        );
        assert!(chart.edges.contains(&edge), "{edge:?}\n{text}");
        assert_eq!(arrow_of(&chart, &edge), "-->", "{edge:?}");
    }
    assert!(!chart.derived.is_empty(), "{text}");
}
