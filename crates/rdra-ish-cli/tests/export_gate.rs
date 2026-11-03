//! Lifecycle errors block only the exports and diagrams that print the lifecycle. The data
//! layer and the diagrams of declared facts still write, with one note on stderr.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A sample whose invariant is violated: `check` reports a lifecycle error.
const FAILING: &str = "samples/formal-verification-fail/order.rdra";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rdra_ish(args: &[&str], out: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(args)
        .arg("--out")
        .arg(out)
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

#[test]
fn data_layer_exports_despite_lifecycle_errors() {
    let dir = scratch("export_gate_data_layer");
    for (args, file) in [
        (vec!["export", FAILING, "--kind", "dbml"], "schema.dbml"),
        (
            vec!["export", FAILING, "--kind", "json-schema"],
            "schema.json",
        ),
        (vec!["csv", FAILING, "--kind", "entity"], "entities.csv"),
    ] {
        let out = dir.join(file);
        let output = rdra_ish(&args, &out);
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert!(out.is_file(), "{args:?} wrote nothing");
        assert_eq!(note_count(&output), 1, "{args:?}: {output:?}");
    }
}

/// Every diagram kind but `state` draws declared facts only, so a lifecycle error does not
/// stop it.
#[test]
fn diagrams_of_declared_facts_draw_despite_lifecycle_errors() {
    let dir = scratch("export_gate_fact_diagrams");
    for kind in [
        "rdra",
        "context",
        "requirement",
        "business",
        "usecase",
        "flow",
        "information",
        "condition",
        "er",
        "event",
    ] {
        let out = dir.join(kind);
        let output = rdra_ish(&["diagram", FAILING, "--kind", kind], &out);
        assert!(output.status.success(), "{kind}: {output:?}");
        assert!(
            out.with_extension("mmd").is_file(),
            "diagram --kind {kind} wrote nothing"
        );
        assert_eq!(note_count(&output), 1, "{kind}: {output:?}");
    }
}

/// TLC does not look for deadlocks or witnesses, so the note sends a hand-run TLC user to
/// `verify`; the export itself still writes and exits 0.
#[test]
fn tla_export_writes_with_a_note_that_names_verify() {
    let dir = scratch("export_gate_tla");
    for sample in [
        "samples/formal-verification/deadlock.rdra",
        "samples/formal-verification/unwitnessed_state.rdra",
        FAILING,
    ] {
        let out = dir.join(Path::new(sample).file_stem().unwrap());
        let output = rdra_ish(&["export", sample, "--kind", "tla"], &out);
        assert!(output.status.success(), "{sample}: {output:?}");
        assert!(out.join("RdraSpec.tla").is_file(), "{sample} wrote no .tla");
        assert!(out.join("RdraSpec.cfg").is_file(), "{sample} wrote no .cfg");
        assert_eq!(note_count(&output), 1, "{sample}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("CHECK_DEADLOCK FALSE"),
            "{sample}: {stderr}"
        );
        assert!(stderr.contains("rdra-ish verify"), "{sample}: {stderr}");
    }
}

#[test]
fn lifecycle_exports_refuse_on_lifecycle_errors() {
    let dir = scratch("export_gate_lifecycle");
    for (args, file) in [
        (
            vec!["export", FAILING, "--kind", "typescript-states"],
            "states.ts",
        ),
        (vec!["csv", FAILING, "--kind", "matrix"], "matrix.csv"),
        (
            vec!["diagram", FAILING, "--kind", "state", "--format", "mermaid"],
            "state.mmd",
        ),
    ] {
        let out = dir.join(file);
        let output = rdra_ish(&args, &out);
        assert!(!output.status.success(), "{args:?} exported: {output:?}");
        assert!(!out.exists(), "{args:?} wrote {}", out.display());
    }
}
