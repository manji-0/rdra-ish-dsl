//! A lifecycle error stops only the outputs that print a reachability result. The TLA+
//! export writes with one note on stderr; every other output writes without exploring the
//! lifecycle, so it says nothing about it. `EXPECTED` classifies every `--kind` of `list`,
//! `csv`, `diagram`, and `export`; the table in docs/cli-reference.md#lifecycle-errors must
//! say the same. The last two tests hold docs/theory to its claims inventory: the summary by
//! evidence class recounts the rows, and the index's known limitations name every open gap.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Output};

#[path = "common/samples.rs"]
mod samples;
#[path = "common/scratch.rs"]
mod scratch;
use samples::repo;
use scratch::scratch;

/// A sample whose invariant is violated: `check` reports a lifecycle error, not a deadlock.
const FAILING: &str = "samples/formal-verification-fail/order.rdra";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gate {
    /// Exits 0, writes the output, and prints no `note: the lifecycle has` line: it explores
    /// nothing, so it knows of no error.
    Writes,
    /// Exits 0, writes the output, and prints one `note: the lifecycle has` line.
    Note,
    /// Exits 1 and writes nothing.
    Stop,
    /// Prints the whole report and exits 1 (`states`).
    Report,
}

use Gate::{Note, Stop, Writes};

const EXPECTED: &[(&str, &[(&str, Gate)])] = &[
    (
        "list",
        &[
            ("actor", Writes),
            ("entity", Writes),
            ("requirement", Writes),
            ("adr", Writes),
            ("adr-impact", Writes),
            ("goal", Writes),
            ("nfr", Writes),
            ("quality", Writes),
            ("constraint", Writes),
            ("buc", Writes),
            ("flow", Writes),
            ("step", Writes),
            ("usecase", Writes),
            ("field", Writes),
            ("system", Writes),
            ("api", Writes),
            ("dto", Writes),
            ("variation", Writes),
            ("screen", Writes),
            ("event", Writes),
            ("timer", Writes),
            ("condition", Writes),
            ("rule", Writes),
            ("permission-callables", Writes),
            ("actor-permission-audit", Writes),
            ("business-inputs", Writes),
            ("buc-roles", Writes),
            ("transition", Writes),
            ("surface", Writes),
        ],
    ),
    (
        "csv",
        &[
            ("actor", Writes),
            ("entity", Writes),
            ("matrix", Writes),
            ("api", Writes),
            ("api-matrix", Writes),
            ("screen-constraints", Writes),
            ("permission-callables", Writes),
            ("actor-permission-audit", Writes),
            ("business-inputs", Writes),
        ],
    ),
    (
        "diagram",
        &[
            ("rdra", Writes),
            ("context", Writes),
            ("requirement", Writes),
            ("business", Writes),
            ("usecase", Writes),
            ("robustness", Writes),
            ("sequence", Writes),
            ("flow", Writes),
            ("screen", Writes),
            ("information", Writes),
            ("condition", Writes),
            ("er", Writes),
            ("state", Stop),
            ("event", Writes),
        ],
    ),
    (
        "export",
        &[
            ("openapi", Writes),
            ("asyncapi", Writes),
            ("dbml", Writes),
            ("sql", Writes),
            ("json-schema", Writes),
            ("mermaid-er", Writes),
            ("tla", Note),
        ],
    ),
];

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(args)
        .output()
        .expect("rdra-ish runs")
}

fn note_count(output: &Output) -> usize {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter(|l| l.starts_with("note: the lifecycle has"))
        .count()
}

/// The `--kind` values clap accepts for `command`, read from its error for an unknown one.
fn kinds_of(command: &str) -> BTreeSet<String> {
    let output = run(&[command, FAILING, "--kind", "no-such-kind"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let (_, values) = stderr
        .split_once("[possible values: ")
        .unwrap_or_else(|| panic!("{command}: no possible values in {stderr}"));
    let (values, _) = values.split_once(']').unwrap();
    values.split(", ").map(str::to_string).collect()
}

#[test]
fn every_kind_is_classified() {
    for (command, kinds) in EXPECTED {
        let listed: BTreeSet<String> = kinds.iter().map(|(k, _)| k.to_string()).collect();
        assert_eq!(
            listed,
            kinds_of(command),
            "{command}: classify every --kind"
        );
    }
}

/// Runs `command --kind kind` on `FAILING` and reports what the lifecycle error did to it.
fn observe(dir: &Path, command: &str, kind: &str) -> Gate {
    let out = dir.join(format!("{command}-{kind}"));
    let mut args = vec![command, FAILING, "--kind", kind];
    if kind == "sequence" {
        args.extend(["--usecase", "PlaceOrder"]);
    }
    let out_arg = out.to_str().unwrap();
    if command != "list" {
        args.extend(["--out", out_arg]);
    }
    let output = run(&args);
    let written = match command {
        // The sample exposes and shares nothing, so its surface is empty.
        "list" => output.status.success() && (kind == "surface" || !output.stdout.is_empty()),
        "diagram" => out.with_extension("mmd").is_file(),
        "export" if kind == "tla" => out.join("RdraSpec.tla").is_file(),
        _ => std::fs::read_dir(dir).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("{command}-{kind}."))
        }),
    };
    match (output.status.code(), written, note_count(&output)) {
        (Some(0), true, 0) => Writes,
        (Some(0), true, 1) => Note,
        (Some(1), false, 0) => Stop,
        other => panic!("{command} --kind {kind}: {other:?} {output:?}"),
    }
}

#[test]
fn a_lifecycle_error_stops_only_the_outputs_that_read_explored_states() {
    let dir = scratch("export_gate_table");
    for (command, kinds) in EXPECTED {
        for (kind, expected) in *kinds {
            assert_eq!(
                observe(&dir, command, kind),
                *expected,
                "{command} --kind {kind}"
            );
        }
    }
}

#[test]
fn states_prints_the_report_and_exits_1() {
    let output = run(&["states", FAILING]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Entity: Order"), "{stdout}");
    assert!(stdout.contains("[error][Inv_Order_0]"), "{stdout}");
}

/// The state diagram colours a deadlock instead of refusing on it.
#[test]
fn the_state_diagram_draws_a_model_whose_only_error_is_a_deadlock() {
    let out = scratch("export_gate_deadlock").join("state");
    let output = run(&[
        "diagram",
        "samples/formal-verification/deadlock.rdra",
        "--kind",
        "state",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{output:?}");
    assert!(out.with_extension("mmd").is_file());
}

/// `diagram --all` explores the lifecycle for its state diagrams only: on a model with a
/// lifecycle error it says once that they are left out, and writes the rest with no note
/// of its own.
#[test]
fn diagram_all_notes_the_lifecycle_only_where_it_leaves_the_state_diagrams_out() {
    let dir = scratch("export_gate_all");
    let output = run(&["diagram", FAILING, "--all", "--out", dir.to_str().unwrap()]);
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let notes: Vec<&str> = stderr.lines().filter(|l| l.starts_with("note:")).collect();
    assert_eq!(notes.len(), 1, "{stderr}");
    assert!(
        notes[0].starts_with("note: --kind state is left out"),
        "{stderr}"
    );
    assert!(dir.join("context.mmd").is_file());
    assert!(!dir.join("state.mmd").exists());
}

/// The whole-model cfg checks the rules, so the note sends a hand-run TLC user to the
/// `StateModel_` specs beside it for the deadlocks and witnesses, and names `verify`; the
/// export itself still writes them all and exits 0.
#[test]
fn tla_export_writes_with_a_note_that_names_the_state_model_specs() {
    let dir = scratch("export_gate_tla");
    for sample in [
        "samples/formal-verification/deadlock.rdra",
        "samples/formal-verification/unwitnessed_state.rdra",
        FAILING,
    ] {
        let out = dir.join(Path::new(sample).file_stem().unwrap());
        let output = run(&[
            "export",
            sample,
            "--kind",
            "tla",
            "--out",
            out.to_str().unwrap(),
        ]);
        assert!(output.status.success(), "{sample}: {output:?}");
        assert!(out.join("RdraSpec.tla").is_file(), "{sample} wrote no .tla");
        assert!(out.join("RdraSpec.cfg").is_file(), "{sample} wrote no .cfg");
        for file in [
            "StateModel_Order.tla",
            "StateModel_Order.cfg",
            "StateModel_Order_witnesses.cfg",
        ] {
            assert!(out.join(file).is_file(), "{sample} wrote no {file}");
        }
        assert_eq!(note_count(&output), 1, "{sample}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("StateModel_"), "{sample}: {stderr}");
        assert!(stderr.contains("rdra-ish verify"), "{sample}: {stderr}");
    }
}

/// A row of the docs table: the command, the kind it names (none for "every kind" and
/// "every other kind"), and the gate its last cell states.
fn doc_rows() -> Vec<(String, Option<String>, Gate)> {
    let doc = std::fs::read_to_string(repo().join("docs/cli-reference.md")).unwrap();
    let (_, section) = doc.split_once("\n## Lifecycle errors\n").expect("section");
    let section = section.split("\n## ").next().unwrap();
    section
        .lines()
        .filter(|l| l.starts_with("| `"))
        .map(|line| {
            let cells: Vec<&str> = line.trim_matches('|').split(" | ").map(str::trim).collect();
            let (_, output) = cells[0].split_once('`').unwrap();
            let (output, _) = output.split_once('`').unwrap();
            let mut words = output.split(' ');
            let command = words.next().unwrap().to_string();
            let kind = match (words.next(), words.next()) {
                (Some("--kind"), Some(kind)) => Some(kind.to_string()),
                (None, None) => None,
                other => panic!("{line}: {other:?}"),
            };
            let last = cells.last().unwrap();
            let gate = if last.starts_with("writes, with a note") {
                Note
            } else if last.starts_with("writes") {
                Writes
            } else if last.starts_with("refuses") {
                Stop
            } else if last.starts_with("prints the report") {
                Gate::Report
            } else {
                panic!("{line}: unknown gate")
            };
            (command, kind, gate)
        })
        .collect()
}

#[test]
fn the_cli_reference_table_states_every_kinds_gate() {
    let rows = doc_rows();
    let gate_of = |command: &str, kind: &str| {
        rows.iter()
            .find(|(c, k, _)| c == command && k.as_deref() == Some(kind))
            .or_else(|| rows.iter().find(|(c, k, _)| c == command && k.is_none()))
            .map(|(_, _, gate)| *gate)
            .unwrap_or_else(|| panic!("docs: no row for {command} --kind {kind}"))
    };
    for (command, kinds) in EXPECTED {
        for (kind, expected) in *kinds {
            assert_eq!(
                gate_of(command, kind),
                *expected,
                "docs: {command} --kind {kind}"
            );
        }
    }
    assert!(rows.contains(&("states".to_string(), None, Gate::Report)));
}

/// The themes of docs/theory/claims.md, by the letter of a row's ID, in the column order of
/// its summary table.
const THEMES: [(char, &str); 5] = [
    ('I', "(0)"),
    ('A', "(a)"),
    ('B', "(b)"),
    ('N', "(c)"),
    ('T', "(d)"),
];

/// A row of the claims inventory: its ID, the classes its Classes cell lists, and whether its
/// Open gap is empty.
struct Claim {
    id: String,
    classes: Vec<String>,
    gap: bool,
}

fn claims() -> Vec<Claim> {
    let doc = std::fs::read_to_string(repo().join("docs/theory/claims.md")).unwrap();
    doc.lines()
        .filter(|l| {
            let mut c = l.chars().skip(2);
            l.starts_with("| ")
                && c.next()
                    .is_some_and(|t| THEMES.iter().any(|(x, _)| *x == t))
                && c.next().is_some_and(|d| d.is_ascii_digit())
        })
        .map(|line| {
            let cells: Vec<&str> = line
                .trim()
                .trim_matches('|')
                .split(" | ")
                .map(str::trim)
                .collect();
            assert_eq!(cells.len(), 8, "claims.md: a row of 8 cells: {line}");
            Claim {
                id: cells[0].to_string(),
                classes: cells[3].split(',').map(|c| c.trim().to_string()).collect(),
                gap: !cells[7].is_empty(),
            }
        })
        .collect()
}

/// Every row with an open gap has a bullet under "Known limitations" in docs/theory/README.md
/// that leads with its ID, and every such bullet names a row with a gap.
#[test]
fn the_theory_index_lists_every_open_gap_of_the_claims() {
    let doc = std::fs::read_to_string(repo().join("docs/theory/README.md")).unwrap();
    let (_, section) = doc.split_once("\n## Known limitations\n").expect("section");
    let section = section.split("\n## ").next().unwrap();
    let listed: BTreeSet<String> = section
        .lines()
        .filter_map(|l| l.strip_prefix("- **"))
        .filter_map(|l| l.split_once(".**"))
        .map(|(id, _)| id.to_string())
        .collect();
    let open: BTreeSet<String> = claims()
        .into_iter()
        .filter(|c| c.gap)
        .map(|c| c.id)
        .collect();
    let missing: Vec<_> = open.difference(&listed).collect();
    let extra: Vec<_> = listed.difference(&open).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "docs/theory/README.md, Known limitations: no bullet for the open gaps of {missing:?}; \
         a bullet for {extra:?}, whose rows in claims.md have no open gap"
    );
}

/// The summary by evidence class in docs/theory/claims.md is a count of the rows' Classes
/// cells, theme by theme, and its first line counts the rows.
#[test]
fn the_claims_summary_counts_the_classes_of_the_rows() {
    let rows = claims();
    let doc = std::fs::read_to_string(repo().join("docs/theory/claims.md")).unwrap();
    let (_, section) = doc
        .split_once("\n## Summary by evidence class\n")
        .expect("section");
    let line = format!(
        "{} rows: {}.",
        rows.len(),
        THEMES
            .iter()
            .map(|(t, name)| {
                let n = rows.iter().filter(|c| c.id.starts_with(*t)).count();
                format!("{n} in {name}")
            })
            .collect::<Vec<_>>()
            .join(", ")
    );
    let first = section.trim_start().split('\n').next().unwrap();
    assert!(
        first.starts_with(&line),
        "claims.md: the summary should open with {line:?}, not {first:?}"
    );
    let mut seen = BTreeSet::new();
    let table = section
        .lines()
        .filter(|l| l.starts_with("| ") && !l.starts_with("| Class "));
    for table_row in table {
        let cells: Vec<&str> = table_row
            .trim_matches('|')
            .split(" | ")
            .map(str::trim)
            .collect();
        // The row "none written (true by construction, ...)" counts the class "none written".
        let class = cells[0].split(" (").next().unwrap();
        let counts: Vec<usize> = THEMES
            .iter()
            .map(|(t, _)| {
                (rows.iter())
                    .filter(|c| c.id.starts_with(*t) && c.classes.iter().any(|k| k == class))
                    .count()
            })
            .collect();
        let mut want: Vec<String> = counts.iter().map(usize::to_string).collect();
        want.push(counts.iter().sum::<usize>().to_string());
        assert_eq!(
            cells[1..],
            want.iter().map(String::as_str).collect::<Vec<_>>()[..],
            "claims.md, summary by evidence class, row {class:?}: (0), (a), (b), (c), (d), rows"
        );
        seen.insert(class.to_string());
    }
    let used: BTreeSet<String> = rows.iter().flat_map(|c| c.classes.clone()).collect();
    assert_eq!(
        seen, used,
        "claims.md: the summary's classes are the classes the rows use"
    );
}
