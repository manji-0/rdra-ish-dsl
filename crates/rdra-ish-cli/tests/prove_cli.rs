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

/// `model` written under the test's temporary directory as `<name>.rdra`.
fn model_file(name: &str, model: &str) -> String {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove_cli_models");
    std::fs::create_dir_all(&dir).expect("a directory");
    let path = dir.join(format!("{name}.rdra"));
    std::fs::write(&path, model).expect("the model is written");
    path.to_string_lossy().into_owned()
}

/// P1: the rule's cone leaves out Review, the only raiser of the event that triggers
/// Finalize, so it runs Finalize unordered and breaks the rule; Review never runs (no use case
/// enters phase b), so no run of the model does. The counterexample is decided again on the
/// rule's exact cone, which passes: the cone reads `ok`, the proof is not told the rule was
/// refuted, and proves it. Properties on such a cone go to the whole model.
#[test]
fn a_counterexample_that_runs_a_handler_without_its_event_is_no_refutation() {
    let model = model_file(
        "handler_without_event",
        r#"
entity Doc "文書" { id: Int @pk  status: Enum(draft, final) @default(draft)  phase: Enum(a, b, c) @default(a) }
usecase Write "作成"
usecase Review "確認"
usecase Finalize "確定"
event EvReviewed "確認済"
creates(Write, Doc)
transitions(Review, Doc.phase, b -> c)
raises(Review, EvReviewed)
triggers(EvReviewed, Finalize)
transitions(Finalize, Doc.status, draft -> final)
forbidden(Doc, status == final)
"#,
    );
    let out = verify(&[&model]);
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stdout.contains("Cone_Forbidden_Doc_0 [Forbidden_Doc_0]: ok (3 states)\n  note: Forbidden_Doc_0's counterexample runs a use case without its event having been raised, which this cone does not rule out; on its exact cone: ok\n"),
        "{stdout}"
    );
    assert!(!stderr.contains("error[Forbidden_Doc_0]"), "{stderr}");
    // A property on such a cone is decided again on the whole model.
    let text = std::fs::read_to_string(&model).expect("the model");
    let properties = model_file(
        "handler_without_event_properties",
        &format!("{text}property NeverFinal \"never final\"\n  always(Doc.status == draft)\nproperty DraftStays \"draft stays\"\n  stable(Doc.status == draft)\n"),
    );
    let out = verify(&[&properties]);
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stdout.contains("property DraftStays (BFS on its cone): ok\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Cone_NeverFinal [NeverFinal]: not decided (7 states, its counterexample may run a use case without its event having been raised)\n"),
        "{stdout}"
    );
    assert!(stdout.contains("[NeverFinal]: ok ("), "{stdout}");
    assert!(!stderr.contains("error[NeverFinal]"), "{stderr}");
    assert!(!stderr.contains("error[DraftStays]"), "{stderr}");
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let out = verify(&["--prove", &model]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\nproved: Forbidden_Doc_0\n"), "{stdout}");
    assert!(!stdout.contains("refuted"), "{stdout}");
}

/// P11: the rule's cone does not count Payment slots, and at two instances its counterexample
/// takes four Payments (one per Place and per Retry), which two or three instances do not
/// have. Escalation decides it again on the exact cone, which counts them: no counterexample
/// at 2 instances.
#[test]
fn escalation_reports_no_counterexample_past_the_bound_it_prints() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let model = model_file(
        "escalation_past_the_bound",
        r#"
entity Customer "顧客" { id: Int @pk }
entity Order "注文" { id: Int @pk  status: Enum(pending, retried) @default(pending) }
entity Payment "決済" { id: Int @pk }
usecase Join "入会"
usecase Place "注文"
usecase Retry "再試行"
creates(Join, Customer)
creates(Place, Order)
updates(Place, Customer)
creates(Place, Payment)
creates(Retry, Payment)
updates(Retry, Order)
transitions(Retry, Order.status, pending -> retried)
relate(Order, Customer, N:1)
relate(Payment, Order, N:1)
invariant(Customer).at_most(1, Order.status == retried)
"#,
    );
    let out = verify(&["--prove", "--instance-count", "1", &model]);
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stdout.contains("escalated: Quantifier_Customer_0: ok at 2 instances"),
        "{stdout}"
    );
    assert!(!stderr.contains("error[Quantifier_Customer_0]"), "{stderr}");
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    // The rule's cone does not count Payment slots either, and its counterexample at one
    // instance creates two Payments (Place, then Retry): the refutation names 2.
    let model = model_file(
        "refuted_past_the_bound",
        r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, retried) @default(pending) }
entity Payment "決済" { id: Int @pk }
usecase Place "注文"
usecase Retry "再試行"
creates(Place, Order)
creates(Place, Payment)
creates(Retry, Payment)
updates(Retry, Order)
transitions(Retry, Order.status, pending -> retried)
relate(Payment, Order, N:1)
forbidden(Order, status == retried)
"#,
    );
    let out = verify(&["--prove", "--instance-count", "1", &model]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("refuted: Forbidden_Order_0 (BFS counterexample at 2 instances)"),
        "{stdout}"
    );
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

/// A rule no mined lemma proves is proved with a lemma generalized from its CTI (#428): the
/// light off and the door ajar, which no reachable state has. The lemma prints as `.lemma()`
/// text, and that text added to the model is a lemma z3 proves, which proves the rule.
#[test]
fn a_lemma_from_the_cti_proves_the_rule_and_reads_as_model_text() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let out = verify(&["--prove", &corpus("z3_cti_true_rule")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\nproved: MultiForbidden_Light_0 "),
        "{stdout}"
    );
    let prefix = "lemma: from the CTI of MultiForbidden_Light_0: ";
    let text = stdout
        .lines()
        .find_map(|l| l.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("no lemma from the CTI\n{stdout}"));
    assert_eq!(
        text,
        "forbidden(Door, Light, Door.state == ajar, Light.state == off).global().lemma()"
    );
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove_cli_cti_lemma");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("m.rdra");
    let src = include_str!("engine_corpus/z3_cti_true_rule.rdra");
    std::fs::write(&file, format!("{src}\n{text}\n")).unwrap();
    let out = verify(&["--prove", file.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\nproved: lemma MultiForbidden_Door_1 "),
        "{stdout}"
    );
    assert!(
        stdout.contains("\nproved: MultiForbidden_Light_0 "),
        "{stdout}"
    );
    assert!(!stdout.contains("from the CTI"), "{stdout}");
}

/// The ∀∃ lemmas join only when a rule stays unproved with the fine ones in: a gate needs one
/// (no key links it to a guard), an order does not (a count of its shipments proves it).
#[test]
fn a_forall_exists_lemma_joins_only_past_the_fine_ones() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let out = verify(&["--prove", &corpus("z3_needs_exists_global")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\nproved: Quantifier_Gate_0"), "{stdout}");
    assert!(
        stdout.contains(
            "\nlemma: while a present Gate row has state == ajar, some present Guard row has state == on\n"
        ),
        "{stdout}"
    );
    let out = verify(&["--prove", &corpus("z3_needs_exists")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\nproved: Quantifier_Order_0"), "{stdout}");
    assert!(
        stdout.contains("\nlemma: a present Order row with status == ready has at least 1"),
        "{stdout}"
    );
    assert!(!stdout.contains("\nlemma: while "), "{stdout}");
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

/// A proof holds for any Int value, so no guard of a step may read the run's Int range. A
/// `present` write guarded by `.when(C)` lists the values it may write, `1..=max` at the run's
/// range, and the guard that pins the write to `C` named each of them: z3 then read the write
/// as capped at `max`, and proved `n > 5` at `--int-range-max 5` though `Fill` writes 6 at a
/// range of 7. The same write without `.when` was unproved, as both are now.
#[test]
fn a_guarded_present_write_reaches_past_the_int_range() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove_cli_guarded_present");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, when) in [("guarded", ".when(IsOpen)"), ("unguarded", "")] {
        let file = dir.join(format!("{name}.rdra"));
        std::fs::write(
            &file,
            format!(
                r#"entity Item "i" {{ id: Int @pk  open: Bool @default(true)  n: Int @null }}
usecase Add "a"
usecase Fill "f"
creates(Add, Item)
updates(Fill, Item)
condition IsOpen "o"
holds(IsOpen, Item.open == true)
sets(Fill, Item.n == present){when}
forbidden(Item, n > 5)
"#
            ),
        )
        .unwrap();
        let out = verify(&[
            "--prove",
            "--int-range-max",
            "5",
            "--instance-count",
            "1",
            file.to_str().unwrap(),
        ]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("unproved: Forbidden_Item_0: not inductive; Fill(Item#1) breaks it"),
            "{name}: {stdout}"
        );
    }
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

/// A false `.lemma()` never helps a proof. The engine-corpus case `z3_lemma` with its lemma
/// swapped for a false one: no Door is ever ajar, which `Crack` breaks. A lemma gets no cone of
/// its own, but this one reads only `Door.state`, which the rule's cone holds, so BFS checks it
/// there and its counterexample fails the run, as `check` fails a false lemma. Where BFS does
/// not reach it (a state cap of 2), Houdini drops it: it prints `unproved`, counts among
/// the user's lemmas, not the rules, and the rule is proved without it.
#[test]
fn a_false_lemma_is_refuted_where_bfs_reaches_it_and_dropped_where_not() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove_cli_false_lemma");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let case = std::fs::read_to_string(repo().join(corpus("z3_lemma"))).unwrap();
    let true_lemma =
        "forbidden(Light, Door, Light.state == off, Door.state == ajar).global().lemma()";
    assert!(case.contains(true_lemma));
    let file = dir.join("m.rdra");
    std::fs::write(
        &file,
        case.replace(true_lemma, "forbidden(Door, Door.state == ajar).lemma()"),
    )
    .unwrap();
    let file = file.to_str().unwrap();

    let out = verify(&["--prove", file]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.contains("refuted: lemma Forbidden_Door_0 (BFS counterexample at 2 instances)\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("proof: your lemmas: 0 proved, 0 unproved, 0 out of scope, 1 refuted\n"),
        "{stdout}"
    );

    let out = verify(&["--prove", "--max-states", "2", file]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("unproved: lemma Forbidden_Door_0: not inductive; Crack(Door#1) breaks it"),
        "{stdout}"
    );
    assert!(
        stdout.contains("proved: MultiForbidden_Light_0 "),
        "{stdout}"
    );
    assert!(
        stdout.contains("proof: your lemmas: 0 proved, 1 unproved, 0 out of scope\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("; 1 of 1 rules proved for any number of rows and any Int value\n"),
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

/// insurance-claim-ops without its `.lemma()` (#428, case 1): the fact its lemma states reads
/// one Claim column and two ClaimCase columns (a liability case before negotiation has no
/// completed claim), which a mined lemma over one child column and two parent columns states,
/// so every rule is proved with no lemma of the user's.
#[test]
fn insurance_claim_ops_is_proved_without_its_lemma() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove_cli_insurance_no_lemma");
    let _ = std::fs::remove_dir_all(&dir);
    let sample = repo().join("samples/insurance-claim-ops");
    for sub in ["buc", "shared"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
        for entry in std::fs::read_dir(sample.join(sub)).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap();
            if path.extension().is_some_and(|e| e == "rdra") && name != "lemmas.rdra" {
                std::fs::copy(&path, dir.join(sub).join(name)).unwrap();
            }
        }
    }
    let out = verify(&["--prove", dir.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("your lemmas"), "{stdout}");
    assert!(
        stdout.contains("\nproof: 22 proved, 0 unproved, 0 out of scope\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "\nlemma: a present Claim row and the present ClaimCase its Claim._owner_ClaimCase names are one of "
        ) && stdout.contains(" reached pairs of (completed_on) and (coverage, status)"),
        "{stdout}"
    );
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
    // One true rule. Alone it is not inductive and stays unproved (its proof needs a run
    // mark no mined lemma reads), so its cone, stopped at the memory limit, stays not
    // decided. In z3_lemma a `.lemma()` makes the rule there inductive, z3 proves both checks
    // of the cone, and that decides it.
    let out = verify(&["--memory", "1KiB", "--prove", &corpus("z3_needs_run_mark")]);
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
    let out = verify(&["--prove", &corpus("z3_int_listing_guard")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        escalated_line(&stdout, "Forbidden_Item_0"),
        "escalated: Forbidden_Item_0: ok at 3 instances, ok at 4 instances (BFS on its cone; ok at 2)",
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
/// `z3_needs_run_mark` grows its rule's store past that from 2 instances to 4, so that run
/// stops there and decides nothing, under no state cap and the default memory.
/// (`safety_count_guard_order` showed this until its count lemmas proved it, #428.)
#[test]
fn an_escalation_past_ten_times_the_store_decides_nothing() {
    if !tool("z3", "RDRA_REQUIRE_Z3") {
        return;
    }
    let out = verify(&["--prove", &corpus("z3_needs_run_mark")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        escalated_line(&stdout, "MultiForbidden_Light_0"),
        "escalated: MultiForbidden_Light_0: ok at 3 instances, not decided at 4 instances (BFS on its cone; ok at 2)"
    );
}
