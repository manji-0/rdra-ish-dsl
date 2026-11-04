//! A lifecycle error stops only the outputs that print a reachability result. Every other
//! output writes, with one note on stderr. `EXPECTED` classifies every `--kind` of `list`,
//! `csv`, `diagram`, and `export`; the table in docs/cli-reference.md#lifecycle-errors must
//! say the same.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A sample whose invariant is violated: `check` reports a lifecycle error, not a deadlock.
const FAILING: &str = "samples/formal-verification-fail/order.rdra";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gate {
    /// Exits 0, writes the output, and prints one `note: the lifecycle has` line.
    Note,
    /// Exits 1 and writes nothing.
    Stop,
    /// Prints the whole report and exits 1 (`states`).
    Report,
}

use Gate::{Note, Stop};

const EXPECTED: &[(&str, &[(&str, Gate)])] = &[
    (
        "list",
        &[
            ("actor", Note),
            ("entity", Note),
            ("requirement", Note),
            ("adr", Note),
            ("adr-impact", Note),
            ("goal", Note),
            ("nfr", Note),
            ("quality", Note),
            ("constraint", Note),
            ("buc", Note),
            ("flow", Note),
            ("step", Note),
            ("usecase", Note),
            ("field", Note),
            ("system", Note),
            ("api", Note),
            ("dto", Note),
            ("variation", Note),
            ("screen", Note),
            ("event", Note),
            ("timer", Note),
            ("condition", Note),
            ("rule", Note),
            ("permission-callables", Note),
            ("actor-permission-audit", Note),
            ("business-inputs", Note),
            ("buc-roles", Note),
            ("transition", Note),
            ("surface", Note),
        ],
    ),
    (
        "csv",
        &[
            ("actor", Note),
            ("entity", Note),
            ("matrix", Note),
            ("api", Note),
            ("api-matrix", Note),
            ("screen-constraints", Note),
            ("permission-callables", Note),
            ("actor-permission-audit", Note),
            ("business-inputs", Note),
        ],
    ),
    (
        "diagram",
        &[
            ("rdra", Note),
            ("context", Note),
            ("requirement", Note),
            ("business", Note),
            ("usecase", Note),
            ("robustness", Note),
            ("sequence", Note),
            ("flow", Note),
            ("screen", Note),
            ("information", Note),
            ("condition", Note),
            ("er", Note),
            ("state", Stop),
            ("event", Note),
        ],
    ),
    (
        "export",
        &[
            ("openapi", Note),
            ("asyncapi", Note),
            ("dbml", Note),
            ("json-schema", Note),
            ("type-script-states", Stop),
            ("mermaid-er", Note),
            ("tla", Note),
        ],
    ),
];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(args)
        .output()
        .expect("rdra-ish runs")
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
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
        // The docs spell `type-script-states` by its alias.
        let kind = kind.replace("type-script", "typescript");
        rows.iter()
            .find(|(c, k, _)| c == command && k.as_deref() == Some(kind.as_str()))
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
