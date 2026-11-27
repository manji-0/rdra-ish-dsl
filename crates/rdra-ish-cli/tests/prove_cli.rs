//! `verify --prove`: one line per state rule, proved or unproved with a counterexample to
//! induction, printed before TLC runs, so these tests read it with or without TLC.
//!
//! Without `z3` on PATH each test that needs it is skipped with a printed reason;
//! `RDRA_REQUIRE_Z3=1` turns the skip into a failure.

use std::path::Path;
use std::process::{Command, Output};

#[path = "common/root.rs"]
mod root;
use root::repo;

#[path = "common/tools.rs"]
mod tools;
use tools::tool;

/// `verify` with no certificate cache: every proof runs its Houdini rounds.
fn verify(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .env_remove("RDRA_Z3")
        .env("RDRA_ISH_NO_PROOF_CACHE", "1")
        .arg("verify")
        .args(args)
        .output()
        .expect("rdra-ish runs")
}

/// `verify`, with the proof's certificates kept under `cache` (`XDG_CACHE_HOME`).
fn verify_cached(args: &[&str], cache: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .env_remove("RDRA_Z3")
        .env_remove("RDRA_ISH_NO_PROOF_CACHE")
        .env("XDG_CACHE_HOME", cache)
        .arg("verify")
        .args(args)
        .output()
        .expect("rdra-ish runs")
}

/// What a run printed, and how it exited.
fn printed(out: Output) -> (Option<i32>, Vec<u8>, Vec<u8>) {
    (out.status.code(), out.stdout, out.stderr)
}

/// The proof keeps its certificate under the user's cache directory and offers it to the next
/// run, which re-checks it: the run prints the same, byte for byte, with no cache, a fresh
/// one, its own certificate, a damaged file, and another model's certificate.
/// `RDRA_ISH_NO_PROOF_CACHE` writes none.
#[test]
fn a_cached_certificate_changes_nothing_a_run_prints() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove_cli_certificate");
    let proofs = dir.join("rdra-ish").join("proofs");
    let files = || -> Vec<std::path::PathBuf> {
        match std::fs::read_dir(&proofs) {
            Ok(entries) => entries.map(|e| e.unwrap().path()).collect(),
            Err(_) => Vec::new(),
        }
    };
    let mut certificates = Vec::new();
    for name in ["check_met", "check_unmet"] {
        let _ = std::fs::remove_dir_all(&dir);
        let args = ["--prove", &sample(name)];
        let off = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
            .current_dir(repo())
            .env_remove("RDRA_Z3")
            .env("RDRA_ISH_NO_PROOF_CACHE", "1")
            .env("XDG_CACHE_HOME", &dir)
            .arg("verify")
            .args(args)
            .output()
            .expect("rdra-ish runs");
        assert!(files().is_empty(), "{name}: {:?}", files());
        let expected = printed(off);
        assert_eq!(
            printed(verify_cached(&args, &dir)),
            expected,
            "{name}: fresh"
        );
        let written = files();
        assert_eq!(written.len(), 1, "{name}: {written:?}");
        let file = &written[0];
        let text = std::fs::read_to_string(file).unwrap();
        assert!(text.starts_with("rdra-ish proof certificate 1\n"), "{text}");
        assert_eq!(
            printed(verify_cached(&args, &dir)),
            expected,
            "{name}: reused"
        );
        assert_eq!(std::fs::read_to_string(file).unwrap(), text, "{name}");
        // Damaged: a kept index past the candidates, a check line that does not match, or
        // not a certificate at all. Each is refused and written over.
        let damaged = [
            text.replacen("kept", "kept 999999", 1),
            text.replacen("check ", "check 0", 1),
            "\u{0}garbage".to_string(),
        ];
        for damaged in damaged.into_iter().chain(certificates.pop()) {
            std::fs::write(file, &damaged).unwrap();
            assert_eq!(
                printed(verify_cached(&args, &dir)),
                expected,
                "{name}: {damaged}"
            );
            assert_eq!(std::fs::read_to_string(file).unwrap(), text, "{name}");
        }
        certificates.push(text);
    }
}

fn sample(name: &str) -> String {
    format!("samples/formal-verification/{name}.rdra")
}

#[test]
fn a_rule_that_holds_for_any_number_of_rows_is_proved() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
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
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    // `check` passes at one instance; Ship and Reship link two Shipments to one Order. At
    // one instance BFS finds no violation, so the proof tries the rule.
    let out = verify(&[
        "--prove",
        "--instance-count",
        "1",
        &sample("one_to_one_shared"),
    ]);
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
fn a_rule_bfs_refutes_is_not_tried_and_the_others_are() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    // At two instances BFS violates the rule, so the proof prints no CTI for it.
    let out = verify(&["--prove", &sample("one_to_one_shared")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\nrefuted: Unique_Shipment_Order (BFS counterexample at 2 instances)\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("\nproof: 0 proved, 0 unproved, 0 out of scope, 1 refuted\n"),
        "{stdout}"
    );
    assert!(!stdout.contains("not inductive"), "{stdout}");
    // Forbidden_Order_1 is a real violation and Forbidden_Order_0 is inductive: the one is
    // refuted, the other still proved, and --deny-unproved counts neither.
    let out = verify(&["--prove", "--deny-unproved", &corpus("z3_inductive")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stdout.contains("\nproved: Forbidden_Order_0\n"), "{stdout}");
    assert!(
        stdout.contains("\nrefuted: Forbidden_Order_1 (BFS counterexample at 2 instances)\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("\nproof: 1 proved, 0 unproved, 0 out of scope, 1 refuted\n"),
        "{stdout}"
    );
    assert!(stderr.contains("error[Forbidden_Order_1]"), "{stderr}");
    assert!(!stderr.contains("unproved"), "{stderr}");
}

#[test]
fn an_unreachable_cti_is_ruled_out_by_a_mined_lemma() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
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

/// A z3 whose `--version` line names a version the resource limits were not measured on
/// prints one `note:` on stderr; the proof, stdout past the version in its header, and the
/// exit code are the same. A verified version prints none; a proof that then fails prints it
/// too. The version comes from a script
/// in front of the z3 on PATH, so the test does not depend on which z3 that is.
#[cfg(unix)]
#[test]
fn an_unverified_z3_version_is_a_note() {
    use std::os::unix::fs::PermissionsExt;
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let z3 = tools::on_path("z3").expect("z3 on PATH");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove_cli_z3_version");
    std::fs::create_dir_all(&dir).unwrap();
    // `solve` runs for every call but `--version`.
    let run = |version: &str, solve: &str| {
        let script = dir.join(format!("z3-{version}-{}", solve.len()));
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'Z3 version {version} - 64 bit'; else {solve}; fi\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let script = script.to_string_lossy().into_owned();
        let out = verify(&["--prove", "--z3", &script, &sample("check_unmet")]);
        let stdout = String::from_utf8_lossy(&out.stdout).replace(version, "<version>");
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        (out.status.code(), stdout, stderr)
    };
    let real = format!("exec '{}' \"$@\"", z3.display());
    let (verified_code, verified_out, verified_err) = run("5.1.0", &real);
    let (code, out, err) = run("5.9.1", &real);
    assert!(
        out.contains("proof (Z3 version <version> - 64 bit): "),
        "{out}"
    );
    assert_eq!((code, &out), (verified_code, &verified_out));
    assert!(!verified_err.contains("note: z3"), "{verified_err}");
    let note = "note: z3 5.9.1 is not a version the proof's resource limits were measured on (5.1.0); a check may give up, or settle, where it would not on those\n";
    assert_eq!(err.matches(note).count(), 1, "{err}");
    assert_eq!(err.replacen(note, "", 1), verified_err);
    // A z3 that cannot solve: the note still prints, beside the error.
    let (code, _, err) = run("5.9.1", "exit 3");
    assert_eq!(code, Some(1), "{err}");
    assert_eq!(err.matches(note).count(), 1, "{err}");
    assert!(err.contains("z3"), "{err}");
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
    if !tool("z3", "RDRA_REQUIRE_Z3") || !tool("tlc", "RDRA_REQUIRE_TLC") {
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
fn deny_unproved_fails_a_run_that_bfs_passes() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    // BFS passes the rule at 2 instances and again at 3 and 4, and the proof leaves it
    // unproved (it fails only past the Int range), so only the proof fails the run.
    let out = verify(&[
        "--prove",
        "--deny-unproved",
        &corpus("z3_int_listing_whole_range"),
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stdout.contains("escalated: Forbidden_Item_0: ok at 3 instances, ok at 4 instances"),
        "{stdout}"
    );
    assert!(
        stderr.contains(
            "1 of 1 rules unproved for any number of rows and any Int value (--deny-unproved)"
        ),
        "{stderr}"
    );
}

#[test]
fn a_lemma_the_model_marks_is_reported_apart_from_the_rules() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
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
    if !tool("z3", "RDRA_REQUIRE_Z3") {
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
    if !tool("z3", "RDRA_REQUIRE_Z3") {
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
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    // Pay runs in two steps; step 1 reads and sets the Payment's run mark, which no view
    // column shows. The CTI prints it before the step as well as after. BFS reaches the
    // violation within 31 states, which would leave the rule untried, so a cap stops it first.
    let out = verify(&[
        "--prove",
        "--max-states",
        "3",
        &sample("cross_order_payment_split"),
    ]);
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
    if !tool("tlc", "RDRA_REQUIRE_TLC") {
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
    if !tool("z3", "RDRA_REQUIRE_Z3") {
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
    if !tool("z3", "RDRA_REQUIRE_Z3") {
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
    if !tool("z3", "RDRA_REQUIRE_Z3") {
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

/// The `escalated:` line of `rule`.
fn escalated_line<'a>(stdout: &'a str, rule: &str) -> &'a str {
    stdout
        .lines()
        .find(|l| l.starts_with(&format!("escalated: {rule}: ")))
        .unwrap_or_else(|| panic!("no escalated line for {rule}\n{stdout}"))
}

/// A rule z3 leaves unproved and BFS passes at the run's 2 instances is checked again on its
/// own cone at 3: the counterexample found there is an error, printed with its trace and the
/// instances it needed, and fails the run.
#[test]
fn a_counterexample_past_the_bound_of_an_unproved_rule_is_an_error() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let out = verify(&["--prove", &corpus("z3_two_ok_three_fails")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stdout}{stderr}");
    assert_eq!(
        escalated_line(&stdout, "Forbidden_Order_0"),
        "escalated: Forbidden_Order_0: counterexample at 3 instances (BFS on its cone; ok at 2)"
    );
    let block = stderr
        .split("error[")
        .find(|b| b.starts_with("Forbidden_Order_0]"))
        .unwrap_or_else(|| panic!("no error for Forbidden_Order_0\n{stderr}"));
    assert!(block.contains(", at 3 instances):"), "{block}");
    assert!(
        stderr.contains("invariant or after-assert violation(s) at more instances than the run's"),
        "{stderr}"
    );
    // The cone line still says what BFS found at the run's bounds.
    assert!(
        stdout.contains("Cone_Forbidden_Order_0 [Forbidden_Order_0]: ok ("),
        "{stdout}"
    );
    // Without --prove nothing is escalated.
    let out = verify(&[&corpus("z3_two_ok_three_fails")]);
    assert!(!String::from_utf8_lossy(&out.stdout).contains("escalated:"));
}

/// The escalation goes on to 4 instances when 3 passes, and stops there.
#[test]
fn an_unproved_rule_that_passes_at_3_is_checked_at_4() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let out = verify(&["--prove", &corpus("safety_at_most_bound")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        escalated_line(&stdout, "Quantifier_Member_0"),
        "escalated: Quantifier_Member_0: ok at 3 instances, counterexample at 4 instances (BFS on its cone; ok at 2)",
        "{stdout}"
    );
    let out = verify(&["--prove", &corpus("z3_cti_true_rule")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        escalated_line(&stdout, "MultiForbidden_Light_0"),
        "escalated: MultiForbidden_Light_0: ok at 3 instances, ok at 4 instances (BFS on its cone; ok at 2)",
        "{stdout}"
    );
    assert!(
        !stderr.contains("more instances than the run's"),
        "{stderr}"
    );
}

/// A rule whose cone did not run, because a larger cone that passed covers it, is escalated
/// on its own cone from that cover's run.
#[test]
fn an_unproved_rule_whose_cone_is_covered_is_escalated() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove_cli_covered");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("m.rdra");
    // The corpus trap, and a rule over its orders and a lamp: that rule's cone holds the
    // trap's, checks its rule too, and covers it.
    let trap = std::fs::read_to_string(repo().join(corpus("z3_two_ok_three_fails"))).unwrap();
    std::fs::write(
        &file,
        format!(
            r#"{trap}
entity Lamp "灯" {{ id: Int @pk  state: Enum(on, off) @default(off) }}
usecase Switch "切替"
transitions(Switch, Lamp.state, off -> on)
forbidden(Lamp, Order, Lamp.state == on, Order.status == d).global()
"#
        ),
    )
    .unwrap();
    let out = verify(&["--prove", file.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("cones: 2 for 2 invariant(s)"), "{stdout}");
    assert_eq!(
        escalated_line(&stdout, "Forbidden_Order_0"),
        "escalated: Forbidden_Order_0: counterexample at 3 instances (BFS on its cone; ok at 2)",
        "{stdout}"
    );
}

/// A rule whose cone splits off a sub-lifecycle is escalated on the reduced cone, as BFS
/// explores it at the run's bounds: the sub-lifecycle keeps its whole graph, which the
/// quotient reads, even though the rule's own run stops once the rule fails. The run used
/// to panic there.
#[test]
fn an_unproved_rule_whose_cone_reduces_is_escalated() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let out = verify(&["--prove", &corpus("z3_needs_run_mark")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("panicked"), "{stderr}");
    assert!(
        escalated_line(&stdout, "MultiForbidden_Light_0")
            .starts_with("escalated: MultiForbidden_Light_0: ok at 3 instances, "),
        "{stdout}"
    );
    assert!(
        !stderr.contains("more instances than the run's"),
        "{stderr}"
    );
}

/// An escalation the state cap stops decides nothing: it says so and leaves the run's exit
/// to what the run decided at its own bounds.
#[test]
fn an_escalation_the_cap_stops_decides_nothing() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let case = corpus("z3_two_ok_three_fails");
    let capped = verify(&["--prove", "--max-states", "20", &case]);
    let stdout = String::from_utf8_lossy(&capped.stdout);
    let stderr = String::from_utf8_lossy(&capped.stderr);
    assert_eq!(
        escalated_line(&stdout, "Forbidden_Order_0"),
        "escalated: Forbidden_Order_0: not decided at 3 instances (BFS on its cone; ok at 2)"
    );
    assert!(
        !stderr.contains("more instances than the run's"),
        "{stderr}"
    );
    let plain = verify(&["--max-states", "20", &case]);
    assert_eq!(capped.status.code(), plain.status.code(), "{stderr}");
    // A cap that stops the cone at the run's 2 instances leaves the rule to no escalation.
    let out = verify(&["--prove", "--max-states", "5", &case]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("escalated:"), "{stdout}");
}

/// The escalation's counterexample alone fails a run: at one instance BFS passes the rule
/// and the proof leaves it unproved, and at two BFS breaks it. Without `--prove` the run
/// passes.
#[test]
fn an_escalation_counterexample_alone_fails_the_run() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let args = ["--instance-count", "1", &sample("one_to_one_shared")];
    let out = verify(&[&["--prove"][..], &args].concat());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stdout}{stderr}");
    assert_eq!(
        escalated_line(&stdout, "Unique_Shipment_Order"),
        "escalated: Unique_Shipment_Order: counterexample at 2 instances (BFS on its cone; ok at 1)"
    );
    assert!(stderr.contains("result: counterexample"), "{stderr}");
    assert!(
        stderr.contains("verification failed: 1 invariant or after-assert violation(s) at more instances than the run's\n"),
        "{stderr}"
    );
    let out = verify(&args);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A run at more instances may hold ten times the store its cone held at the run's bounds:
/// `safety_count_guard_order` grows its rule's store about tenfold from 2 instances to 4, so
/// that run stops there and decides nothing, under no state cap and the default memory.
#[test]
fn an_escalation_past_ten_times_the_store_decides_nothing() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let out = verify(&["--prove", &corpus("safety_count_guard_order")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        escalated_line(&stdout, "Forbidden_Shelf_0"),
        "escalated: Forbidden_Shelf_0: ok at 3 instances, not decided at 4 instances (BFS on its cone; ok at 2)"
    );
}
