//! The diagram encoding contract, checked on every sample: each `.mmd` opens with the `%%`
//! header, names each node `<kind>__<DSL id>`, tags it `:::<kind>`, draws edges only between
//! declared nodes, and lists in `kinds:` exactly the kinds its body draws.

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

/// Every samples.txt root outside the formal-verification fixtures, several of which
/// violate their lifecycle on purpose and so are refused by the kinds that read it.
fn roots() -> Vec<String> {
    std::fs::read_to_string(repo().join("samples.txt"))
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|line| !line.starts_with("samples/formal-verification"))
        .map(str::to_string)
        .collect()
}

fn buc_ids(root: &str) -> Vec<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(["list", root, "--kind", "buc", "--format", "csv"])
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
    let mut command = Command::new(env!("CARGO_BIN_EXE_rdra-ish"));
    command
        .current_dir(repo())
        .args(["diagram", root, "--kind", kind])
        .arg("--out")
        .arg(out);
    if let Some(buc) = buc {
        command.args(["--buc", buc]);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{root} {kind} {buc:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(out.with_extension("mmd")).unwrap()
}

/// What a flowchart under the contract says, read line by line. Any line the contract
/// does not define fails the parse.
#[derive(Default)]
struct Flowchart {
    header_kinds: BTreeSet<String>,
    /// Node id to its `:::` class.
    nodes: HashMap<String, String>,
    edges: Vec<(String, String, String)>,
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
        } else if trimmed.starts_with("%% ") {
        } else if trimmed == "flowchart LR" {
            seen_type = true;
        } else if trimmed == "end"
            || (trimmed.starts_with("subgraph layer_") && trimmed.ends_with("\"]"))
        {
        } else if let Some(kind) = trimmed
            .strip_prefix("classDef ")
            .and_then(|rest| rest.strip_suffix(" stroke:#333;"))
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
        return Err("no `flowchart LR` line".to_string());
    }
    Ok(chart)
}

/// The contract's structural rules on one parsed flowchart.
fn check_flowchart(chart: &Flowchart) -> Result<(), String> {
    for (id, class) in &chart.nodes {
        let (kind, dsl_id) = id
            .split_once("__")
            .ok_or_else(|| format!("node id without `<kind>__`: {id}"))?;
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
    for line in lines {
        let trimmed = line.trim();
        if let Some(kinds) = trimmed.strip_prefix("%% kinds: ") {
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
                let (arm, usecase) = trimmed
                    .split_once(" : ")
                    .ok_or_else(|| format!("unparsed line: {line}"))?;
                let (from, to) = arm
                    .split_once(" --> ")
                    .ok_or_else(|| format!("unparsed line: {line}"))?;
                chart.transitions.push((
                    model,
                    from.to_string(),
                    to.to_string(),
                    usecase.to_string(),
                ));
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
    for root in roots() {
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
    let good = "%% rdra-ish diagram=state scope=whole\n%% kinds: statemodel state usecase\nstateDiagram-v2\n  state \"Doc.status\" as statemodel__Doc_status {\n    [*] --> state__Doc_draft\n    state \"draft\" as state__Doc_draft\n    state \"done\" as state__Doc_done\n    state__Doc_draft --> state__Doc_done : usecase__Publish\n    state__Doc_done --> [*]\n  }\n";
    assert!(check_state(&parse_state(good).unwrap()).is_ok());
    for (broken, from, to) in [
        ("missing end", "    state__Doc_done --> [*]\n", ""),
        (
            "end with a way out",
            "    state__Doc_done --> [*]\n",
            "    state__Doc_done --> [*]\n    state__Doc_draft --> [*]\n",
        ),
        ("label is not an id", ": usecase__Publish", ": Publish"),
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
    for root in roots() {
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
