//! `verify --prove`: one line per state rule, proved or unproved with a counterexample to
//! induction, printed before TLC runs, so these tests read it with or without TLC.
//!
//! Without `z3` on PATH each test that needs it is skipped with a printed reason;
//! `RDRA_REQUIRE_Z3=1` turns the skip into a failure.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo() -> PathBuf {
    dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).expect("repo root")
}

fn have(tool: &str, require: &str) -> bool {
    let found = std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|d| d.join(tool).is_file()));
    if !found {
        let reason = format!("{tool} not on PATH (run inside `nix develop`)");
        assert!(
            std::env::var(require).as_deref() != Ok("1"),
            "{require}=1 but {reason}"
        );
        println!("skipped: {reason}");
    }
    found
}

fn verify(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .env_remove("RDRA_Z3")
        .arg("verify")
        .args(args)
        .output()
        .expect("rdra-ish runs")
}

fn sample(name: &str) -> String {
    format!("samples/formal-verification/{name}.rdra")
}

#[test]
fn a_rule_that_holds_for_any_number_of_rows_is_proved() {
    if !have("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let out = verify(&["--prove", &sample("check_met")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("proved: Forbidden_CartLine_0\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("proved: Check_CartLine_qty_0\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("proof: 2 proved, 0 unproved, 0 out of scope\n"),
        "{stdout}"
    );
}

#[test]
fn a_rule_that_breaks_at_two_rows_is_unproved_with_its_step() {
    if !have("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    // `check` passes at one instance; Ship and Reship link two Shipments to one Order.
    let out = verify(&["--prove", &sample("one_to_one_shared")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout
        .lines()
        .find(|l| l.starts_with("unproved: Unique_Shipment_Order: not inductive;"))
        .unwrap_or_else(|| panic!("no unproved line\n{stdout}"));
    assert!(line.contains("(not necessarily reachable)"), "{line}");
    assert!(
        stdout.lines().any(|l| l.starts_with("  Shipment#")),
        "the CTI names its rows\n{stdout}"
    );
}

#[test]
fn an_unreachable_cti_is_ruled_out_by_a_mined_lemma() {
    if !have("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    // Forbidden_Order_0 alone is not inductive: CancelOrder breaks it from a pending Order
    // with a delivery date, which no run reaches. The reached patterns rule that state out.
    let out = verify(&["--prove", &sample("order")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("proved: Forbidden_Order_0\n"), "{stdout}");
    assert!(
        stdout.contains("lemma: every present Order row is one of"),
        "{stdout}"
    );
}

#[test]
fn a_missing_z3_is_a_tool_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .env("RDRA_Z3", "/nonexistent/z3")
        .args(["verify", "--prove", &sample("check_met")])
        .output()
        .expect("rdra-ish runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("z3 not found at /nonexistent/z3 (from --z3 or RDRA_Z3)"),
        "{stderr}"
    );
}

#[test]
fn deny_unproved_needs_prove() {
    let out = verify(&["--deny-unproved", &sample("check_met")]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("--prove"), "{stderr}");
}

#[test]
fn the_ok_line_says_how_many_rules_were_proved() {
    if !have("z3", "RDRA_REQUIRE_Z3") || !have("tlc", "RDRA_REQUIRE_TLC") {
        return;
    }
    let out = verify(&["--prove", "--deny-unproved", &sample("check_met")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(
        stdout.contains("; 2 of 2 rules proved for any number of rows\n"),
        "{stdout}"
    );
}

#[test]
fn deny_unproved_fails_a_run_that_tlc_passes() {
    if !have("z3", "RDRA_REQUIRE_Z3") || !have("tlc", "RDRA_REQUIRE_TLC") {
        return;
    }
    // `check` and TLC pass at one instance; at two TLC fails, and the proof leaves the rule
    // unproved either way. `--instance-count 1` makes TLC pass, so only the proof fails.
    let out = verify(&[
        "--prove",
        "--deny-unproved",
        "--instance-count",
        "1",
        &sample("one_to_one_shared"),
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("1 of 1 rules unproved for any number of rows (--deny-unproved)"),
        "{stderr}"
    );
}
