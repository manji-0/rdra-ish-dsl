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
