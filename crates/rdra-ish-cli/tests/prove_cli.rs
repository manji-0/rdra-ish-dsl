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
        stdout.contains("; 2 of 2 rules proved for any number of rows and any Int value\n"),
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
        stderr.contains(
            "1 of 1 rules unproved for any number of rows and any Int value (--deny-unproved)"
        ),
        "{stderr}"
    );
}

#[test]
fn a_lemma_the_model_marks_is_reported_apart_from_the_rules() {
    if !have("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove_cli_lemma");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("m.rdra");
    std::fs::write(
        &file,
        r#"entity Order "注文" {
  id: Int @pk
  status: Enum(placed, shipped) @default(placed)
}

entity Shipment "出荷" {
  id: Int @pk
}

usecase Place "注文確定"
usecase Ship "出荷"

creates(Place, Order)
creates(Ship, Shipment)
updates(Ship, Order)
transitions(Ship, Order.status, placed -> shipped)
relate(Shipment, Order, 1:1)

invariant(Order).when(status == placed).none(Shipment).label("no shipment yet").lemma()
"#,
    )
    .unwrap();
    let out = verify(&["--prove", file.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("1 state rules and 1 of your lemmas,"),
        "{stdout}"
    );
    assert!(
        stdout.contains("proved: lemma 「no shipment yet」: Quantifier_Order_0 [not enforced by the SQL schema: related rows]\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("proof: 1 proved, 0 unproved, 0 out of scope\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("proof: your lemmas: 1 proved, 0 unproved, 0 out of scope\n"),
        "{stdout}"
    );
}

#[test]
fn a_rule_that_holds_only_under_the_int_bound_is_unproved() {
    if !have("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    // At Int 0..5 no qty passes 5, so `check` passes; Adjust may write any number.
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove_cli_int_top");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("m.rdra");
    std::fs::write(
        &file,
        r#"entity Item "商品" {
  id: Int @pk
  qty: Int @null
}

usecase CreateItem "登録"
usecase Adjust "調整"

creates(CreateItem, Item)
updates(Adjust, Item)
sets(Adjust, Item.qty == present)
forbidden(Item, qty > 5)
"#,
    )
    .unwrap();
    let out = verify(&["--prove", file.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("unproved: Forbidden_Item_0: not inductive; Adjust(Item#1) breaks it"),
        "{stdout}"
    );
    // The value z3 picks depends on its version; it is some number above 5.
    let qty = stdout
        .lines()
        .find_map(|l| {
            l.strip_prefix("  Item#1 (present) ")?
                .split_once("-> qty=")
                .map(|(_, v)| v)
        })
        .unwrap_or_else(|| panic!("no CTI line that writes qty\n{stdout}"));
    assert!(qty.trim().parse::<i64>().is_ok_and(|n| n > 5), "{qty}");
}

#[test]
fn a_rule_the_sql_schema_leaves_to_the_application_says_so() {
    if !have("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    // A forbidden over two entities has no CHECK; a 1:1 key is a UNIQUE constraint.
    let out = verify(&["--prove", &sample("cross_order_payment")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(
            "proved: MultiForbidden_Order_0 [not enforced by the SQL schema: several entities]\n"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "proof: the SQL schema leaves 1 rules to the application: 1 proved, 0 unproved, 0 out of scope\n"
        ),
        "{stdout}"
    );
    let out = verify(&["--prove", &sample("one_to_one_shared")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("not enforced by the SQL schema"),
        "{stdout}"
    );
}

#[test]
fn a_cti_shows_the_run_mark_its_step_reads() {
    if !have("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    // Pay runs in two steps; step 1 reads and sets the Payment's run mark, which no view
    // column shows. The CTI prints it before the step as well as after.
    let out = verify(&["--prove", &sample("cross_order_payment_split")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with("  Payment#") && l.contains("_run_Pay=none -> ")),
        "{stdout}"
    );
}

#[test]
fn verify_runs_no_tlc_cone_for_a_lemma() {
    if !have("tlc", "RDRA_REQUIRE_TLC") {
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove_cli_lemma_cone");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("m.rdra");
    std::fs::write(
        &file,
        r#"entity Order "注文" {
  id: Int @pk
  status: Enum(placed, shipped) @default(placed)
}

entity Shipment "出荷" {
  id: Int @pk
}

usecase Place "注文確定"
usecase Ship "出荷"

creates(Place, Order)
creates(Ship, Shipment)
updates(Ship, Order)
transitions(Ship, Order.status, placed -> shipped)
relate(Shipment, Order, 1:1)

invariant(Order).when(status == placed).none(Shipment).label("no shipment yet").lemma()
"#,
    )
    .unwrap();
    let out_dir = dir.join("specs");
    let out = verify(&[file.to_str().unwrap(), "-o", out_dir.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    let cones = std::fs::read_to_string(out_dir.join("cones.txt")).unwrap();
    assert!(cones.contains("Unique_Shipment_Order"), "{cones}");
    assert!(!cones.contains("Cone_Quantifier_Order_0"), "{cones}");
}

fn corpus(name: &str) -> String {
    format!("crates/rdra-ish-cli/tests/engine_corpus/{name}.rdra")
}

/// The run line of the safety cone `module`.
fn cone_line<'a>(stdout: &'a str, module: &str) -> &'a str {
    stdout
        .lines()
        .find(|l| l.starts_with(&format!("{module} [")))
        .unwrap_or_else(|| panic!("no {module} line\n{stdout}"))
}

const BY_PROOF: &str = " states, BFS stopped at the memory limit, decided by proof: --prove proved every check for any number of rows)";

/// Whether `line` says its cone is not decided.
fn not_decided(line: &str) -> bool {
    line.contains("]: not decided (")
}

#[test]
fn a_proof_decides_a_cone_that_stopped_at_the_memory_limit() {
    if !have("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    // At four instances the cone has tens of states, and 1KiB stops BFS after a few. z3
    // proves both of its rules for any number of rows, so they hold at the run's bounds too,
    // and the run leaves nothing open.
    let args = ["--instance-count", "4", "--memory", "1KiB"];
    let out = verify(&[&args[..], &["--prove", &sample("check_met")]].concat());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    let line = cone_line(&stdout, "Cone_Forbidden_CartLine_0");
    assert!(
        line.starts_with(
            "Cone_Forbidden_CartLine_0 [Forbidden_CartLine_0, Check_CartLine_qty_0]: ok ("
        ) && line.ends_with(BY_PROOF),
        "{stdout}"
    );
    assert!(stdout.contains("\nresult: ok\n"), "{stdout}");
    // Without --prove the same cone is not decided.
    let out = verify(&[&args[..], &[&sample("check_met")]].concat());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        not_decided(cone_line(&stdout, "Cone_Forbidden_CartLine_0")),
        "{stdout}"
    );
}

#[test]
fn a_cone_at_the_memory_limit_is_decided_only_when_every_rule_in_it_is_proved() {
    if !have("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    // One true rule. Alone it is not inductive and stays unproved, so its cone, stopped at
    // the memory limit, stays not decided. With the `.lemma()` that z3_lemma adds, z3 proves
    // both checks of the cone, which decides it.
    let out = verify(&["--memory", "1KiB", "--prove", &corpus("z3_cti_true_rule")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\nunproved: MultiForbidden_Light_0 "),
        "{stdout}"
    );
    assert!(
        not_decided(cone_line(&stdout, "Cone_MultiForbidden_Light_0")),
        "{stdout}"
    );
    let out = verify(&["--memory", "1KiB", "--prove", &corpus("z3_lemma")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = cone_line(&stdout, "Cone_MultiForbidden_Light_0");
    assert!(
        line.starts_with(
            "Cone_MultiForbidden_Light_0 [MultiForbidden_Light_0, MultiForbidden_Light_1]: ok ("
        ) && line.ends_with(BY_PROOF),
        "{stdout}"
    );
}

#[test]
fn a_proof_leaves_an_after_assert_and_a_state_cap_undecided() {
    if !have("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    // z3 proves both rules of the cone, but the cone also checks an after-assert, which the
    // proof does not state.
    let out = verify(&["--memory", "1KiB", "--prove", &sample("order")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\nproof: 2 proved, 0 unproved, 0 out of scope\n"),
        "{stdout}"
    );
    assert!(
        not_decided(cone_line(&stdout, "Cone_Forbidden_Order_0")),
        "{stdout}"
    );
    // check_met's cone holds only proved rules, but a state cap stops it, not the memory limit.
    let out = verify(&["--max-states", "3", "--prove", &sample("check_met")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\nproof: 2 proved, 0 unproved, 0 out of scope\n"),
        "{stdout}"
    );
    assert!(
        not_decided(cone_line(&stdout, "Cone_Forbidden_CartLine_0")),
        "{stdout}"
    );
}
