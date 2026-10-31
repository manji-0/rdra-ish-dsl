//! The diagram encoding contract, checked on every sample for each kind:
//! each `.mmd` opens with the `%%` header, names each node `<kind>__<DSL id>` (the same id in
//! every kind; a variation's value is `variation__<Variation>__<value>`), tags a flowchart
//! node `:::<kind>`, draws edges only between declared nodes,
//! and lists in `kinds:` exactly the kinds its body draws.

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
    edges: Vec<(String, String, String)>,
    /// The edges a `%% derived` line follows: rdra-ish derived them from other facts.
    derived: BTreeSet<(String, String, String)>,
    class_defs: BTreeSet<String>,
}

fn parse_flowchart(diagram: &str, text: &str) -> Result<Flowchart, String> {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default();
    if !first.starts_with(&format!("%% rdra-ish diagram={diagram} scope=")) {
        return Err(format!("first line is not the header: {first}"));
    }
    let mut chart = Flowchart::default();
    let mut seen_type = false;
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
            let last = chart.edges.last().cloned();
            let follows = last
                .as_ref()
                .is_some_and(|(from, _, to)| rest.starts_with(&format!("{from} -> {to} ")));
            match (follows, last) {
                (true, Some(edge)) => {
                    chart.derived.insert(edge);
                }
                _ => return Err(format!("`%% derived` after another edge: {line}")),
            }
        } else if trimmed.starts_with("%% ") {
        } else if trimmed == "flowchart LR" || trimmed == "flowchart TD" {
            seen_type = true;
        } else if trimmed == "end"
            || (trimmed.starts_with("subgraph layer_") && trimmed.ends_with("\"]"))
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
        } else if let Some((id, rest)) = trimmed.split_once("[\"") {
            let (_, class) = rest
                .rsplit_once("\"]:::")
                .ok_or_else(|| format!("node without `:::<kind>`: {line}"))?;
            chart.nodes.insert(id.to_string(), class.to_string());
        } else {
            let (from, rest) = trimmed
                .split_once(' ')
                .ok_or_else(|| format!("unparsed line: {line}"))?;
            let (arrow, rest) = rest
                .split_once('|')
                .ok_or_else(|| format!("unparsed line: {line}"))?;
            let (label, to) = rest
                .rsplit_once("| ")
                .ok_or_else(|| format!("unparsed line: {line}"))?;
            if !["-->", "-.->", "---"].contains(&arrow) {
                return Err(format!("unknown arrow `{arrow}`: {line}"));
            }
            chart
                .edges
                .push((from.to_string(), label.to_string(), to.to_string()));
        }
    }
    if !seen_type {
        return Err("no `flowchart LR` or `flowchart TD` line".to_string());
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
    let good = "%% rdra-ish diagram=rdra scope=whole\n%% kinds: actor usecase\nflowchart LR\n  actor__A[\"A\"]:::actor\n  usecase__U[\"U\"]:::usecase\n  actor__A -.->|performs| usecase__U\n  classDef actor stroke:#333;\n  classDef usecase stroke:#333;\n";
    assert!(check_flowchart(&parse_flowchart("rdra", good).unwrap()).is_ok());
    for (broken, from, to) in [
        ("untagged", "\"]:::usecase\n", "\"]\n"),
        (
            "undeclared end",
            "usecase__U\n  classDef",
            "usecase__X\n  classDef",
        ),
        ("header kinds", "%% kinds: actor usecase", "%% kinds: actor"),
        ("wrong class", "\"]:::actor", "\"]:::usecase"),
        ("emoji-era id", "  actor__A -.->", "  A -.->"),
    ] {
        let text = good.replace(from, to);
        assert_ne!(text, good, "{broken}");
        let result = parse_flowchart("rdra", &text).and_then(|chart| check_flowchart(&chart));
        assert!(result.is_err(), "{broken} passed:\n{text}");
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
        } else if trimmed.starts_with("%% ") {
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
                let (_, id) = rest
                    .split_once("\" as ")
                    .ok_or_else(|| format!("unparsed state: {line}"))?;
                chart.states.insert(id.to_string(), model);
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
        if leaves == chart.ends.contains(state) {
            return Err(format!(
                "{state}: an end state must be one with no transition out"
            ));
        }
    }
    let mut body_kinds = BTreeSet::new();
    for (kind, present) in [
        ("statemodel", !chart.models.is_empty()),
        ("state", !chart.states.is_empty()),
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
fn the_state_contract_check_rejects_a_broken_diagram() {
    let good = "%% rdra-ish diagram=state scope=whole\n%% kinds: statemodel state usecase\nstateDiagram-v2\n  state \"Doc.status\" as statemodel__Doc_status {\n    [*] --> state__Doc_draft\n    state \"draft\" as state__Doc_draft\n    state \"done\" as state__Doc_done\n    %% transition usecase__Publish\n    state__Doc_draft --> state__Doc_done : Publish\n    state__Doc_done --> [*]\n  }\n";
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
        let state = parse_state(&diagram(&root, "state", None, &dir.join("s"))).unwrap();
        let ids = state
            .states
            .keys()
            .chain(state.transitions.iter().map(|(_, _, _, usecase)| usecase));
        for id in ids {
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
    let mut edges = 0;
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
}

#[test]
fn the_contract_check_rejects_a_misplaced_derived_line() {
    let good = "%% rdra-ish diagram=usecase scope=whole\n%% kinds: actor usecase\nflowchart LR\n  actor__A[\"A\"]:::actor\n  usecase__U[\"U\"]:::usecase\n  actor__A -.->|performs| usecase__U\n  %% derived actor__A -> usecase__U via buc__B\n  classDef actor stroke:#333;\n  classDef usecase stroke:#333;\n";
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
            for kind in chart.nodes.values() {
                assert!(
                    ["actor", "extsystem", "business", "buc"].contains(&kind.as_str()),
                    "{root} {scope:?}: a {kind} node\n{text}"
                );
            }
            for (from, label, to) in &chart.edges {
                assert!(
                    from.starts_with("buc__") || to.starts_with("buc__"),
                    "{root} {scope:?}: {from} -{label}-> {to}"
                );
            }
            drawn += usize::from(!chart.edges.is_empty());
        }
    }
    assert!(drawn > 10, "only {drawn} business diagrams draw an edge");
}

#[test]
fn a_business_diagram_draws_what_the_rdra_diagram_declares() {
    let dir = scratch("diagram_contract_business_ids");
    for root in roots() {
        let rdra = parse_flowchart("rdra", &diagram(&root, "rdra", None, &dir.join("r"))).unwrap();
        let business = parse_flowchart(
            "business",
            &diagram(&root, "business", None, &dir.join("b")),
        )
        .unwrap();
        for (id, kind) in &business.nodes {
            assert_eq!(rdra.nodes.get(id), Some(kind), "{root}: {id}");
        }
        for edge in &business.edges {
            if !business.derived.contains(edge) {
                assert!(rdra.edges.contains(edge), "{root}: {edge:?} not in rdra");
            }
        }
    }
}

#[test]
fn an_element_subgraph_is_a_node_of_its_kind() {
    let good = "%% rdra-ish diagram=business scope=whole\n%% kinds: business buc\nflowchart LR\n  subgraph layer_environment[\"E\"]\n    subgraph business__B[\"B\"]\n      buc__U[\"U\"]:::buc\n    end\n  end\n  classDef business stroke:#333;\n  classDef buc stroke:#333;\n";
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
            for edge @ (from, label, to) in &chart.edges {
                let fits = match label.split_once(' ') {
                    None => label == "performs" && from.starts_with("actor__"),
                    Some(("raises", _)) => from.starts_with("extsystem__"),
                    Some(("triggers", _)) => to.starts_with("extsystem__"),
                    _ => false,
                };
                let system_end = from.starts_with("system__") || to.starts_with("system__");
                assert!(
                    fits && system_end && chart.derived.contains(edge),
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
                    text.contains("    %% synthesized system__Model: "),
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

#[test]
fn every_requirement_diagram_of_every_sample_keeps_the_contract() {
    let dir = scratch("diagram_contract_requirement");
    let mut edges = BTreeSet::new();
    for root in roots() {
        for scope in scopes(&root) {
            let args: Vec<&str> = scope.iter().map(String::as_str).collect();
            let text = diagram_with(&root, "requirement", &args, &dir.join("out"));
            let chart = checked("requirement", &text, &format!("{root} {scope:?}"));
            for (from, label, to) in &chart.edges {
                let source = from.split_once("__").map_or("", |(kind, _)| kind);
                let fits = match label.as_str() {
                    "motivates" => source == "requirement" && to.starts_with("buc__"),
                    "applies_to" => source == "nfr",
                    "constrains" => source == "constraint",
                    "qualifies" => {
                        ["nfr", "constraint"].contains(&source) && to.starts_with("quality__")
                    }
                    _ => false,
                };
                assert!(fits, "{root} {scope:?}: {from} -{label}-> {to}");
                edges.insert(label.clone());
            }
            assert!(chart.derived.is_empty(), "{root} {scope:?}: a derived edge");
        }
    }
    assert_eq!(
        edges,
        BTreeSet::from(["applies_to", "constrains", "motivates", "qualifies"].map(String::from))
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
            assert_eq!(rdra.nodes.get(id), Some(kind), "{root}: {id}");
        }
        for edge in &requirement.edges {
            assert!(rdra.edges.contains(edge), "{root}: {edge:?} not in rdra");
        }
    }
}
