//! `verify` as users run it: the native checker decides every check with no TLC on PATH and
//! no jar, and `--with-tlc` runs TLC beside it. Without `-o` a `--with-tlc` run that fails
//! keeps its temp dir and names it; a relative `-o` resolves against the working directory,
//! not TLC's.
//!
//! Without `tlc` on PATH each `--with-tlc` test is skipped with a printed reason;
//! `RDRA_REQUIRE_TLC=1` turns the skip into a failure.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo() -> PathBuf {
    dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).expect("repo root")
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn have_tlc() -> bool {
    let found = Command::new("which")
        .arg("tlc")
        .output()
        .is_ok_and(|o| o.status.success());
    if !found {
        let reason = "tlc not on PATH (run inside `nix develop`)";
        assert!(
            std::env::var("RDRA_REQUIRE_TLC").as_deref() != Ok("1"),
            "RDRA_REQUIRE_TLC=1 but {reason}"
        );
        println!("skipped: {reason}");
    }
    found
}

/// Runs `verify` with no `tlc` on PATH and no jar, so a run that passes or fails here
/// started no TLC.
fn verify(cwd: &Path, tmpdir: &Path, args: &[&str]) -> Output {
    let empty = tmpdir.join("no-tools");
    std::fs::create_dir_all(&empty).unwrap();
    verify_on_path(cwd, tmpdir, &empty, args)
}

/// Runs `verify` with `path` as PATH and no jar.
fn verify_on_path(cwd: &Path, tmpdir: &Path, path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(cwd)
        .env("TMPDIR", tmpdir)
        .env("PATH", path)
        .env_remove("RDRA_TLC_JAR")
        .arg("verify")
        .args(args)
        .output()
        .expect("rdra-ish runs")
}

/// Runs `verify --with-tlc` with the caller's PATH, which `have_tlc` has found `tlc` on.
fn verify_with_tlc(cwd: &Path, tmpdir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(cwd)
        .env("TMPDIR", tmpdir)
        .arg("verify")
        .arg("--with-tlc")
        .args(args)
        .output()
        .expect("rdra-ish runs")
}

/// `order_stability.rdra`: BFS decides `stable`, `unchanged`, and `precedes` on each one's
/// cone, with no TLC run, and prints the shortest trace to the step or state that breaks each
/// failing one. `check` evaluates none of them (#166).
#[test]
fn verify_decides_order_and_stability_by_bfs_and_check_does_not() {
    let tmp = scratch("verify_cli_order_stability");
    let sample = repo().join("samples/formal-verification/order_stability.rdra");
    let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    assert!(
        stdout.contains("\norder and stability properties: 6, each decided by BFS on its cone\n"),
        "{stdout}"
    );
    for (name, verdict) in [
        ("ShippedStaysShipped", "ok"),
        ("CancelledStaysCancelled", "violated"),
        ("AmountKeptOnceApproved", "ok"),
        ("AmountKeptOnceDecided", "violated"),
        ("ShippedAfterPaid", "ok"),
        ("ShippedAfterInvoiced", "violated"),
    ] {
        assert!(
            stdout.contains(&format!("property {name} (BFS on its cone): {verdict}\n")),
            "{name}\n{stdout}"
        );
    }
    let stable = "error[CancelledStaysCancelled]: property CancelledStaysCancelled is violated: its stable condition holds for Order#1 before step 3 and is false after it";
    assert!(stderr.contains(stable), "{stderr}");
    assert!(
        stderr.contains("counterexample (3 steps):\n    1. Place [Order#1]"),
        "{stderr}"
    );
    assert!(stderr.contains("    3. Reopen [Order#1]"), "{stderr}");
    assert!(
        stderr.contains("error[AmountKeptOnceDecided]: property AmountKeptOnceDecided is violated: step 4 changes a column unchanged(..) keeps for Order#1 while its when condition holds"),
        "{stderr}"
    );
    assert!(
        stderr.contains("error[ShippedAfterInvoiced]: property ShippedAfterInvoiced is violated: its second condition holds for Order#1 after step 4"),
        "{stderr}"
    );
    assert!(!stderr.contains("then nothing changes forever"), "{stderr}");
    assert!(
        stderr.contains("verification failed: 3 property violation(s)"),
        "{stderr}"
    );

    let check = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .arg("check")
        .arg(&sample)
        .output()
        .expect("rdra-ish runs");
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    assert_eq!(check.status.code(), Some(0), "{out}");
    for name in [
        "CancelledStaysCancelled",
        "AmountKeptOnceDecided",
        "ShippedAfterInvoiced",
    ] {
        assert!(!out.contains(name), "check reports {name}\n{out}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn failed_verify_without_out_keeps_and_names_its_specs() {
    if !have_tlc() {
        return;
    }
    let tmp = scratch("verify_cli_kept_specs");
    let sample = repo().join("samples/formal-verification-fail/order.rdra");
    let output = verify_with_tlc(&repo(), &tmp, &[sample.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    let line = stderr
        .lines()
        .find_map(|l| l.strip_prefix("specs: "))
        .unwrap_or_else(|| panic!("no specs: line\n{stderr}"));
    let dir = line
        .strip_suffix(" (kept because a check failed)")
        .unwrap_or_else(|| panic!("specs: line does not say it was kept: {line}"));
    let dir = Path::new(dir);
    assert!(
        dir.join("cones.txt").is_file(),
        "{} is not kept",
        dir.display()
    );
    assert!(dir.join("Cone_Inv_Order_0.tla").is_file());
    let _ = std::fs::remove_dir_all(&tmp);
}

/// A native run without `-o` writes no spec, failing or not; a passing `--with-tlc` run
/// removes the temp dir it wrote.
#[test]
fn passing_verify_without_out_removes_its_specs() {
    let tmp = scratch("verify_cli_removed_specs");
    let left = |tmp: &Path| -> usize {
        std::fs::read_dir(tmp)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("rdra-ish-tlc-"))
            .count()
    };
    for sample in ["check_met.rdra", "check_unmet.rdra"] {
        let sample = repo().join("samples/formal-verification").join(sample);
        let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
        assert!(output.status.code().is_some(), "{output:?}");
        assert_eq!(left(&tmp), 0, "a native run wrote specs");
    }
    if have_tlc() {
        let sample = repo().join("samples/formal-verification/check_met.rdra");
        let output = verify_with_tlc(&repo(), &tmp, &[sample.to_str().unwrap()]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(left(&tmp), 0, "a passing run left its specs");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn relative_out_is_relative_to_the_working_directory() {
    let cwd = scratch("verify_cli_relative_out");
    let sample = repo().join("samples/formal-verification/check_met.rdra");
    let output = verify(&cwd, &cwd, &[sample.to_str().unwrap(), "-o", "specs"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let index = std::fs::read_to_string(cwd.join("specs/cones.txt")).expect("specs/cones.txt");
    assert!(index.contains("Check_CartLine_qty_0"), "{index}");
    let _ = std::fs::remove_dir_all(&cwd);
}

/// A counterexample prints in model terms, through the renderer `check` uses. Natively it
/// is BFS's shortest trace; under `--with-tlc` TLC's prints too, before TLC's own error line.
#[test]
fn a_counterexample_prints_in_model_terms() {
    let tmp = scratch("verify_cli_model_terms");
    let sample = repo().join("samples/formal-verification/cross_order_payment_split.rdra");
    let in_order = |stderr: &str, lines: &[&str]| {
        let mut rest = stderr;
        for expected in lines {
            let at = rest
                .find(expected)
                .unwrap_or_else(|| panic!("{expected:?} missing or out of order\n{stderr}"));
            rest = &rest[at + expected.len()..];
        }
    };
    let trace = [
        "error[MultiForbidden_Order_0]: invariant MultiForbidden_Order_0 is violated: ",
        "  rule: ",
        "cross_order_payment_split.rdra:44:1",
        "  counterexample (3 steps):",
        "    1. Place [Order#1, Payment#1]",
        "    2. Pay_1, transaction 1 of Pay [Payment#1]",
        "         Payment#1.status: pending -> captured",
        "    3. Cancel [Order#1]",
    ];
    let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    in_order(&stderr, &trace);
    assert!(!stderr.contains("from TLC"), "{stderr}");
    assert!(!stderr.contains("violated: Error:"), "{stderr}");
    assert!(stderr.contains("\nresult: counterexample\n"), "{stderr}");
    if have_tlc() {
        let output = verify_with_tlc(&repo(), &tmp, &[sample.to_str().unwrap()]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{stderr}");
        in_order(&stderr, &trace);
        in_order(
            &stderr,
            &[
                "tlc: Cone_MultiForbidden_Order_0 (",
                "  counterexample (3 steps, from TLC):",
                "violated: Error: Invariant MultiForbidden_Order_0 is violated.",
                "\nresult: counterexample\ntlc_result: counterexample\n",
            ],
        );
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Two rules that fail in exactly the same states (#64): the cross-entity rule, listed after
/// the forbidden state, still prints. TLC reports one violated check per state, so `verify`
/// reruns the cone without the forbidden state to find it.
const SAME_STATE_RULES: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid) @default(pending) }
entity Parcel "荷物" { id: Int @pk  state: Enum(packed, sent) @default(packed) }
relate(Parcel, Order, N:1)
usecase Place "注文"
usecase Pack "梱包"
usecase Send "発送"
creates(Place, Order)
creates(Pack, Parcel)
transitions(Send, Parcel.state, packed -> sent)
forbidden(Parcel, state == sent)
invariant(Order, Parcel).along(Parcel, Order).when(Parcel.state == sent).then(Order.status == paid)
"#;

#[test]
fn verify_reports_every_rule_that_fails_in_the_same_state() {
    let tmp = scratch("verify_cli_same_state");
    let model = tmp.join("m.rdra");
    std::fs::write(&model, SAME_STATE_RULES).unwrap();
    let output = verify(&repo(), &tmp, &[model.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    for name in ["Forbidden_Parcel_0", "MultiInv_Order_0"] {
        assert!(
            stderr.contains(&format!("error[{name}]: invariant {name} is violated: ")),
            "{name} missing\n{stderr}"
        );
    }
    if !have_tlc() {
        return;
    }
    let output = verify_with_tlc(&repo(), &tmp, &[model.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    let cone = stderr
        .split("\ntlc: Cone_")
        .find(|block| block.starts_with("MultiInv_Order_0 ("))
        .unwrap_or_else(|| panic!("no block for the cross-entity cone\n{stderr}"));
    for name in ["Forbidden_Parcel_0", "MultiInv_Order_0"] {
        assert!(
            cone.contains(&format!("error[{name}]: invariant {name} is violated: ")),
            "{name} missing\n{stderr}"
        );
        assert!(
            cone.contains(&format!("violated: Error: Invariant {name} is violated.")),
            "{name}'s TLC line missing\n{stderr}"
        );
    }
    assert!(!stderr.contains("BFS and TLC disagree"), "{stderr}");
    assert!(!stderr.contains("EngineDisagreement"), "{stderr}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Each temporal property is decided on its own cone and its counterexample prints in model
/// terms, named after the property, with no TLC on PATH: a leads_to that an endless cancel and
/// reopen breaks loops back. `-o` still writes a module per run.
#[test]
fn a_temporal_counterexample_prints_its_lasso_in_model_terms() {
    let tmp = scratch("verify_cli_temporal");
    let sample = repo().join("samples/formal-verification/temporal_unmet.rdra");
    let out = tmp.join("M.tla");
    let output = verify(
        &repo(),
        &tmp,
        &[sample.to_str().unwrap(), "-o", out.to_str().unwrap()],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    // The always needs one cone run; the leads_to's counterexample is confirmed by the `_WF`
    // run, the same cone with `WF_vars(Next)`.
    assert!(
        stdout.contains("\nCone_NeverShipped [NeverShipped]: counterexample (31 states)\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("\nCone_PendingLeadsToDelivered [PendingLeadsToDelivered]: counterexample (31 states, Cone_PendingLeadsToDelivered_WF: counterexample)\n"),
        "{stdout}"
    );
    for (module, property, wf_next) in [
        ("Cone_NeverShipped", "NeverShipped", false),
        (
            "Cone_PendingLeadsToDelivered",
            "PendingLeadsToDelivered",
            false,
        ),
        (
            "Cone_PendingLeadsToDelivered_WF",
            "PendingLeadsToDelivered",
            true,
        ),
    ] {
        let cfg = std::fs::read_to_string(tmp.join(format!("{module}.cfg"))).unwrap();
        assert_eq!(cfg.matches("PROPERTY ").count(), 1, "{cfg}");
        assert!(cfg.contains(&format!("PROPERTY {property}\n")), "{cfg}");
        let tla = std::fs::read_to_string(tmp.join(format!("{module}.tla"))).unwrap();
        assert_eq!(tla.contains("WF_vars(Next)"), wf_next, "{module}");
    }
    assert!(!tmp.join("Cone_NeverShipped_WF.tla").exists());
    let whole = std::fs::read_to_string(tmp.join("M.cfg")).expect("the whole-model cfg");
    assert_eq!(whole.matches("PROPERTY ").count(), 2, "{whole}");
    let index = std::fs::read_to_string(tmp.join("cones.txt")).unwrap();
    assert_eq!(
        index,
        "Cone_NeverShipped.tla: NeverShipped\n\
         Cone_PendingLeadsToDelivered.tla: PendingLeadsToDelivered\n\
         Cone_PendingLeadsToDelivered_WF.tla: PendingLeadsToDelivered\n"
    );

    let block = |name: &str| -> &str {
        let head = format!("error[{name}]: property {name} is violated: ");
        let at = stderr
            .find(&head)
            .unwrap_or_else(|| panic!("no {head:?}\n{stderr}"));
        let rest = &stderr[at..];
        let end = ["\nerror[", "\nresult: "]
            .iter()
            .filter_map(|m| rest[1..].find(m).map(|k| k + 1))
            .min()
            .unwrap_or(rest.len());
        &rest[..end]
    };
    // An always fails at a state: BFS's shortest trace to it, with no loop.
    let always = block("NeverShipped");
    assert!(
        always.contains("its always condition is false for Order#1 after step 3"),
        "{always}"
    );
    assert!(always.contains("  counterexample (3 steps):"), "{always}");
    assert!(!always.contains("then "), "{always}");
    let leads_to = block("PendingLeadsToDelivered");
    assert!(
        leads_to.contains("its leads_to trigger holds for Order#"),
        "{leads_to}"
    );
    assert!(leads_to.contains("ReopenOrder [Order#"), "{leads_to}");
    assert!(
        leads_to.contains("\n    then loops back to step "),
        "{leads_to}"
    );
    assert!(!stderr.contains("BFS and TLC disagree"), "{stderr}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// `.fair(Register, Dismiss)` rules out the run where another application amends forever: the
/// fair property holds, the same claim without it fails with a lasso, and verify names the
/// assumption the passing verdict rests on.
#[test]
fn a_fair_property_holds_where_the_unfair_one_loops() {
    let tmp = scratch("verify_cli_fair");
    let sample = repo().join("samples/formal-verification/temporal_fair.rdra");
    let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    assert!(
        stdout.contains("\n  GrantEnds assumes weak fairness per instance on Register, Dismiss\n"),
        "{stdout}"
    );
    assert!(stdout.contains("[GrantEnds]: ok"), "{stdout}");
    assert!(
        stdout.contains("[GrantEndsUnfair]: counterexample"),
        "{stdout}"
    );
    assert!(!stderr.contains("error[GrantEnds]:"), "{stderr}");
    assert!(stderr.contains("error[GrantEndsUnfair]:"), "{stderr}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// `temporal_along.rdra`: the property with `.along(Payment, Order)` holds on its cone, which
/// keeps the owner link, and the same formula without it fails on a captured payment beside
/// an order it does not belong to (#165).
#[test]
fn a_property_along_reads_only_linked_rows() {
    let tmp = scratch("verify_cli_along");
    let sample = repo().join("samples/formal-verification/temporal_along.rdra");
    let out = tmp.join("M.tla");
    let output = verify(
        &repo(),
        &tmp,
        &[sample.to_str().unwrap(), "-o", out.to_str().unwrap()],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    assert!(stdout.contains("[CapturedPaid]: ok"), "{stdout}");
    assert!(
        stdout.contains("[CapturedPaidAnyOrder]: counterexample"),
        "{stdout}"
    );
    assert!(!stderr.contains("error[CapturedPaid]:"), "{stderr}");
    assert!(
        stderr.contains("error[CapturedPaidAnyOrder]: property CapturedPaidAnyOrder is violated"),
        "{stderr}"
    );
    let tla = std::fs::read_to_string(tmp.join("Cone_CapturedPaid.tla")).unwrap();
    assert!(
        tla.contains("Payment_exists[b0] = \"present\" /\\ Order_exists[b1] = \"present\" /\\ Payment_owner_Order[b0] = b1) =>"),
        "{tla}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

/// A property without `.fair` rests on the whole model's `WF_vars(Next)`, which the cone
/// does not keep: the cone's two runs leave it undecided, and the whole-model run decides it
/// under the budget. With a budget of 0 s the whole-model run is stopped, the property is
/// listed as not decided, and the run still ends in `OK:` as a BFS cone at its cap does.
#[test]
fn a_property_the_cone_cannot_decide_runs_on_the_whole_model_under_the_budget() {
    let tmp = scratch("verify_cli_whole_model");
    let sample = repo().join("samples/formal-verification/order.rdra");
    let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}{stderr}");
    assert!(
        stdout.contains("\ntemporal properties: 2, each checked on its cone, and on the whole model only when the cone does not decide it (budget 300 s)\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("\nCone_PaidLeadsToShipped [PaidLeadsToShipped]: not decided (31 states, Cone_PaidLeadsToShipped_WF: ok, needs the whole model's WF_vars(Next))\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("\nRdraSpec_PaidLeadsToShipped [PaidLeadsToShipped]: ok (31 states)\n"),
        "{stdout}"
    );
    assert!(!stdout.contains("not checked:"), "{stdout}");

    let output = verify(
        &repo(),
        &tmp,
        &[sample.to_str().unwrap(), "--temporal-budget", "0"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}{stderr}");
    assert!(
        stdout.contains("\nRdraSpec_PaidLeadsToShipped [PaidLeadsToShipped]: not decided (stopped at the time budget of 0 s)\n")
            || stdout.contains("\nRdraSpec_PaidLeadsToShipped [PaidLeadsToShipped]: not decided (31 states, stopped at the time budget of 0 s)\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("\nnot checked: 2 temporal properties not decided\n  PaidLeadsToShipped: the whole-model run stopped at the time budget of 0 s"),
        "{stdout}"
    );
    assert!(
        stdout.contains("; raise --temporal-budget, or name the use cases the property relies on with .fair(UseCase, ...)"),
        "{stdout}"
    );
    // Exit 0, but something is undecided: `result:` is not `ok`.
    assert!(!stdout.contains("result: ok"), "{stdout}");
    assert!(
        stderr.lines().any(|l| l == "result: not decided"),
        "{stderr}"
    );
    assert!(!stdout.contains("tlc_result"), "{stdout}");
    assert!(
        stdout.ends_with("OK: no errors within 2 instances per entity, Int 0..5\n"),
        "{stdout}"
    );
    assert!(
        stderr.contains("warning: 2 temporal properties the budget left undecided; the OK line holds only for what was decided (--deny-undecided fails the run)\n"),
        "{stderr}"
    );

    let output = verify(
        &repo(),
        &tmp,
        &[
            sample.to_str().unwrap(),
            "--temporal-budget",
            "0",
            "--deny-undecided",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    assert!(
        stderr.contains("verification incomplete: 2 temporal properties the budget left undecided (--deny-undecided)"),
        "{stderr}"
    );
    assert!(!stdout.contains("OK:"), "{stdout}");
    assert!(
        stderr.lines().any(|l| l == "result: not decided"),
        "{stderr}"
    );
    assert!(!stdout.contains("result: ok"), "{stdout}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// `verify` states the bounds first, and a passing run ends with them in its `OK:` line.
#[test]
fn verify_prints_the_bounds_line_and_an_ok_line_that_repeats_them() {
    let tmp = scratch("verify_cli_bounds");
    let sample = repo().join("samples/formal-verification/check_met.rdra");
    let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.lines().any(|l| l
            == "bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); no state cap per BFS cone, memory 16GiB; entities a check does not read are unbounded"),
        "{stdout}"
    );
    assert_eq!(
        stdout.lines().last(),
        Some("OK: no errors within 2 instances per entity, Int 0..5"),
        "{stdout}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

/// `--max-states` is the state cap of every BFS run: a cone `order.rdra` decides in 31 states
/// is not decided at 10, and decided at 31; the header names the cap in force.
#[test]
fn max_states_raises_the_cap_of_each_cone() {
    let tmp = scratch("verify_cli_max_states_cone");
    let sample = repo().join("samples/formal-verification/order.rdra");
    let sample = sample.to_str().unwrap();
    let small = verify(&repo(), &tmp, &[sample, "--max-states", "10"]);
    let stdout = String::from_utf8_lossy(&small.stdout);
    assert!(
        stdout.contains("; 10 states per BFS cone;"),
        "the header names the cap in force: {stdout}"
    );
    assert!(
        stdout.contains("Cone_Forbidden_Order_0 [Forbidden_Order_0, Inv_Order_0, AfterAssert_DeliverOrder_0]: not decided (10 states, stopped at the state or pattern cap (--max-states, --max-patterns) or the memory limit)"),
        "{stdout}"
    );
    let stderr = String::from_utf8_lossy(&small.stderr);
    assert!(
        stderr.lines().any(|l| l == "result: not decided"),
        "{stderr}"
    );

    let large = verify(&repo(), &tmp, &[sample, "--max-states", "31"]);
    let stdout = String::from_utf8_lossy(&large.stdout);
    assert!(stdout.contains("; 31 states per BFS cone;"), "{stdout}");
    assert!(
        stdout.contains("\nCone_Forbidden_Order_0 [Forbidden_Order_0, Inv_Order_0, AfterAssert_DeliverOrder_0]: ok (31 states)\n"),
        "{stdout}"
    );
    assert!(stdout.contains("\nresult: ok\n"), "{stdout}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// The whole-model run takes the same `--max-states`: with a second entity the whole model
/// has 403 states, though each cone has 31. At 100 the run is stopped and the hint names the
/// flag; at 403 it decides. Without the flag it stays under 2^21 and decides.
#[test]
fn max_states_raises_the_cap_of_the_whole_model_run() {
    let tmp = scratch("verify_cli_max_states_whole");
    let mut src =
        std::fs::read_to_string(repo().join("samples/formal-verification/order.rdra")).unwrap();
    src.push_str(
        r#"
entity Parcel "Parcel" {
  id: Int @pk
  status: Enum(packed, sent, received) @default(packed)
}
usecase PackParcel "Pack"
usecase SendParcel "Send"
usecase ReceiveParcel "Receive"
buc BucParcel "Parcel"
contains(BucParcel, PackParcel)
contains(BucParcel, SendParcel)
contains(BucParcel, ReceiveParcel)
creates(PackParcel, Parcel)
updates(SendParcel, Parcel)
updates(ReceiveParcel, Parcel)
transitions(SendParcel, Parcel.status, packed -> sent)
transitions(ReceiveParcel, Parcel.status, sent -> received)
"#,
    );
    let model = tmp.join("m.rdra");
    std::fs::write(&model, src).unwrap();
    let model = model.to_str().unwrap();

    let small = verify(&repo(), &tmp, &[model, "--max-states", "100"]);
    let stdout = String::from_utf8_lossy(&small.stdout);
    assert!(
        stdout.contains("\nRdraSpec_PaidLeadsToShipped [PaidLeadsToShipped]: not decided (100 states, stopped at the state cap of 100 states)\n"),
        "{stdout}"
    );
    assert!(stdout.contains("so raise --max-states ("), "{stdout}");

    for args in [&["--max-states", "403"][..], &[]] {
        let large = verify(&repo(), &tmp, &[&[model][..], args].concat());
        let stdout = String::from_utf8_lossy(&large.stdout);
        assert!(
            stdout
                .contains("\nRdraSpec_PaidLeadsToShipped [PaidLeadsToShipped]: ok (403 states)\n"),
            "{args:?}: {stdout}"
        );
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

/// `--max-patterns` caps the distinct patterns per entity: `order.rdra` has five, so a cap of
/// 3 leaves its findings undecided, and the default 256 decides them.
#[test]
fn max_patterns_caps_the_patterns_of_each_entity() {
    let tmp = scratch("verify_cli_max_patterns");
    let sample = repo().join("samples/formal-verification/order.rdra");
    let sample = sample.to_str().unwrap();
    let capped = verify(&repo(), &tmp, &[sample, "--max-patterns", "3"]);
    let stdout = String::from_utf8_lossy(&capped.stdout);
    assert!(
        stdout.contains("state Order.status = shipped is not decided: the Order cone stopped at the pattern cap (3 patterns)"),
        "{stdout}"
    );
    let stderr = String::from_utf8_lossy(&capped.stderr);
    assert!(
        stderr.lines().any(|l| l == "result: not decided"),
        "{stderr}"
    );

    let default = verify(&repo(), &tmp, &[sample]);
    let stdout = String::from_utf8_lossy(&default.stdout);
    assert!(!stdout.contains("pattern cap ("), "{stdout}");
    assert!(stdout.contains("\nresult: ok\n"), "{stdout}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// A state no use case reaches fails `verify` from BFS on the entity cones, though no
/// invariant or property is violated: `result: counterexample` and exit 1.
#[test]
fn an_unwitnessed_state_fails_verify_while_the_checks_pass() {
    let tmp = scratch("verify_cli_unwitnessed");
    let sample = repo().join("samples/formal-verification/unwitnessed_state.rdra");
    let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    assert!(!stdout.contains("result: ok"), "{stdout}");
    assert!(
        stderr.lines().any(|l| l == "result: counterexample"),
        "{stderr}"
    );
    assert!(!stdout.contains("OK: no errors"), "{stdout}");
    assert!(
        stderr.contains(
            "error[Witness_Order_status_refunded]: state Order.status = refunded has no reachability witness: no sequence of use cases from the start reaches it\n  state model: Order.status, entered by Refund (shipped -> refunded)"
        ),
        "{stderr}"
    );
    assert!(stderr.contains("unwitnessed_state.rdra:31:1"), "{stderr}");
    assert!(
        stderr.contains("\nwarning[Runs_Refund]: use case Refund can never run: each of its guards holds in some reachable state, but never all of them in one\n"),
        "{stderr}"
    );
    assert!(
        stdout.lines().any(|l| l
            == "state models (BFS on 1 entity cone): 1 state model, 3 states witnessed, 1 deadlock, 1 use case that never runs"),
        "{stdout}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

/// An instance that can never leave a state fails `verify` from BFS on the entity cones, though
/// no invariant or property is violated; the summary line counts it.
#[test]
fn a_deadlock_fails_verify_while_the_checks_pass() {
    let tmp = scratch("verify_cli_deadlock");
    let sample = repo().join("samples/formal-verification/deadlock.rdra");
    let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    assert!(!stdout.contains("result: ok"), "{stdout}");
    assert!(
        stderr.lines().any(|l| l == "result: counterexample"),
        "{stderr}"
    );
    assert!(!stdout.contains("OK: no errors"), "{stdout}");
    assert!(
        stderr.contains(
            "error[Deadlock_Order_status_paid]: deadlock: Order#1 stays at status = paid forever, and paid is not an end state\n  leaves paid: Ship (paid -> shipped)"
        ),
        "{stderr}"
    );
    assert!(stderr.contains("deadlock.rdra:29:1"), "{stderr}");
    assert!(
        stdout.lines().any(|l| l
            == "state models (BFS on 1 entity cone): 1 state model, 3 states witnessed, 1 deadlock, 1 use case that never runs"),
        "{stdout}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

/// `verify` decides a `can_reach` by BFS at its bounds and says so; TLC could not check it.
/// At two instances the trap is the same, and the passing model passes.
#[test]
fn verify_decides_a_can_reach_natively() {
    let tmp = scratch("verify_cli_can_reach");
    let sample = repo().join("samples/formal-verification/can_reach_unmet.rdra");
    let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    assert!(
        stdout.lines().any(|l| l
            == "can_reach properties: 1, decided natively by BFS on each one's cone (TLC cannot check AG EF)"),
        "{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|l| l == "can_reach (BFS on 1 cone): 1 checked, 1 violated"),
        "{stdout}"
    );
    assert!(!stdout.contains("not checked"), "{stdout}");
    assert!(
        stderr.contains(
            "error[OrderCanFinish]: property OrderCanFinish is violated: from the counterexample's last state no path reaches its goal for Order#1"
        ),
        "{stderr}"
    );
    assert!(
        stderr.contains("  confined: from there Order#1 stays within 34 reachable states, where its goal never holds:\n    status = pending\n    status = on_hold\n"),
        "{stderr}"
    );
    assert!(
        stderr.contains("Error: verification failed: 1 can_reach error(s) from BFS on their cones"),
        "{stderr}"
    );
    let met = repo().join("samples/formal-verification/can_reach_met.rdra");
    let output = verify(&repo(), &tmp, &[met.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}");
    assert!(
        stdout
            .lines()
            .any(|l| l == "can_reach (BFS on 1 cone): 1 checked, 0 violated"),
        "{stdout}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

/// `two_queues.rdra`: each entity cone finishes under `--max-states 40`, while the cone of the
/// `can_reach`, the product of both, stops at it. That `can_reach` is not decided: it is listed
/// under `not checked:` with the cap, the `result:` line says `not decided`, and
/// `--deny-undecided` fails the run. Run to the end, its cone breaks it (#246).
#[test]
fn a_can_reach_whose_cone_stops_at_the_state_cap_is_not_decided() {
    let tmp = scratch("verify_cli_can_reach_capped");
    let fixture = repo().join("crates/rdra-ish-cli/tests/fixtures/reach/two_queues.rdra");
    let fixture = fixture.to_str().unwrap();

    let whole = verify(&repo(), &tmp, &[fixture]);
    let stdout = String::from_utf8_lossy(&whole.stdout);
    assert_eq!(whole.status.code(), Some(1), "{stdout}");
    assert!(
        stdout
            .lines()
            .any(|l| l == "can_reach (BFS on 1 cone): 1 checked, 1 violated"),
        "{stdout}"
    );

    let capped = verify(&repo(), &tmp, &[fixture, "--max-states", "40"]);
    let stdout = String::from_utf8_lossy(&capped.stdout);
    let stderr = String::from_utf8_lossy(&capped.stderr);
    assert_eq!(capped.status.code(), Some(0), "{stdout}{stderr}");
    assert!(
        stdout.contains(
            "\nnot checked: 1 finding not decided\n  can_reach BothCanFinish is not decided: the \
             BothCanFinish cone stopped at the state cap (40 states)\n"
        ),
        "{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|l| l == "can_reach (BFS on 1 cone): 1 checked, 0 violated, 1 not decided"),
        "{stdout}"
    );
    assert!(
        stderr.lines().any(|l| l == "result: not decided"),
        "{stderr}"
    );
    assert!(
        stderr.contains(
            "warning: 1 finding a cone stopped at the state cap or the memory limit left undecided"
        ),
        "{stderr}"
    );

    let denied = verify(
        &repo(),
        &tmp,
        &[fixture, "--max-states", "40", "--deny-undecided"],
    );
    let stdout = String::from_utf8_lossy(&denied.stdout);
    let stderr = String::from_utf8_lossy(&denied.stderr);
    assert_eq!(denied.status.code(), Some(1), "{stdout}{stderr}");
    assert!(
        stderr.lines().any(|l| l == "result: not decided"),
        "{stderr}"
    );
    assert!(!stdout.contains("OK:"), "{stdout}");
    assert!(
        stderr.contains(
            "verification incomplete: 1 finding a cone stopped at the state cap or the memory \
             limit left undecided (--deny-undecided)"
        ),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

/// `verify --entity Order` defers Audit's build error, decides Order's invariant cone, and
/// passes in the selected scope; the whole run stops at the build gate.
#[test]
fn an_entity_scope_verifies_part_of_a_model() {
    let tmp = scratch("verify_cli_partial_scope");
    let sample = repo().join("samples/formal-verification/partial_scope.rdra");
    let sample = sample.to_str().unwrap();
    let whole = verify(&repo(), &tmp, &[sample]);
    let stderr = String::from_utf8_lossy(&whole.stderr);
    assert_eq!(whole.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("no TLA+ written"), "{stderr}");

    let scoped = verify(&repo(), &tmp, &[sample, "--entity", "Order"]);
    let stdout = String::from_utf8_lossy(&scoped.stdout);
    let stderr = String::from_utf8_lossy(&scoped.stderr);
    assert_eq!(scoped.status.code(), Some(0), "{stdout}{stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[1], "scope: --entity Order", "{stdout}");
    assert!(
        stdout.contains(
            "\nnot checked: 1 entity view (Audit), 1 invariant (Forbidden_Audit_1), 0 after-asserts; 1 build error deferred; 1 finding not decided\n  deferred error[DerivedColumnRead_Audit_score]: Audit: Audit.score is derived by arithmetic"
        ),
        "{stdout}"
    );
    assert!(
        lines.contains(&"Cone_Forbidden_Order_0 [Forbidden_Order_0]: ok (7 states)"),
        "{stdout}"
    );
    assert!(lines.contains(&"result: ok"), "{stdout}");
    assert_eq!(
        lines.last(),
        Some(&"OK: no errors in the selected scope, within 2 instances per entity, Int 0..5"),
        "{stdout}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

/// `verify --buc` runs the BFS pass on the scoped system, lists what it cannot decide and the
/// temporal properties it skips under `not checked:`, and passes in the selected scope.
#[test]
fn a_buc_scope_skips_temporal_properties_and_leaves_progress_undecided() {
    let tmp = scratch("verify_cli_buc_scope");
    let sample = repo().join("samples/formal-verification/order.rdra");
    let output = verify(
        &repo(),
        &tmp,
        &[sample.to_str().unwrap(), "--buc", "BucOrderLifecycle"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}{stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines[1].starts_with("scope: --buc BucOrderLifecycle ("),
        "{stdout}"
    );
    assert!(
        lines.contains(&"temporal properties: 2, not checked: a scoped run checks none"),
        "{stdout}"
    );
    assert!(
        lines.contains(&"not checked: 2 temporal properties"),
        "{stdout}"
    );
    assert_eq!(
        lines.last(),
        Some(&"OK: no errors in the selected scope, within 2 instances per entity, Int 0..5"),
        "{stdout}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

/// A condition on the use case that creates a row reads a column the creation leaves at its
/// default, so the guard reads a successor only `UNCHANGED` binds: the invariant's cone, the
/// property's cone, and the whole model all decide.
#[test]
fn a_creation_guard_on_an_unassigned_column_is_decided() {
    let tmp = scratch("verify_cli_guard_on_unassigned");
    let model = tmp.join("m.rdra");
    std::fs::write(
        &model,
        r#"entity E "E" { id: Int @pk  status: Enum(a, b) @default(a)  seen: Bool @default(false) }
usecase Make "作成"
usecase Flip "切替"
creates(Make, E)
transitions(Flip, E.status, a -> b)
sets(Flip, E.seen == true)
condition Fresh "新規"
holds(Fresh, E.status == a)
applies_to(Fresh, Make)
forbidden(E, status == b, seen == false)
property P eventually(E.status == b)
"#,
    )
    .unwrap();
    let output = verify(&repo(), &tmp, &[model.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}{stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines.contains(&"Cone_Forbidden_E_0 [Forbidden_E_0]: ok (7 states)"),
        "{stdout}"
    );
    assert!(lines.contains(&"RdraSpec_P [P]: ok (7 states)"), "{stdout}");
    assert!(lines.contains(&"result: ok"), "{stdout}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Runs `verify` on `src` with `-o <tmp>/<out>`; returns stdout, stderr, the exit code, and
/// the out dir.
fn verify_model(test: &str, src: &str, out: &str) -> (String, String, Option<i32>, PathBuf) {
    verify_model_with(test, src, out, false)
}

/// `verify_model`, under `--with-tlc` when `tlc`.
fn verify_model_with(
    test: &str,
    src: &str,
    out: &str,
    tlc: bool,
) -> (String, String, Option<i32>, PathBuf) {
    let tmp = scratch(test);
    let model = tmp.join("m.rdra");
    std::fs::write(&model, src).unwrap();
    let dir = tmp.join("out");
    let target = dir.join(out);
    let args = [model.to_str().unwrap(), "-o", target.to_str().unwrap()];
    let output = match tlc {
        true => verify_with_tlc(&repo(), &tmp, &args),
        false => verify(&repo(), &tmp, &args),
    };
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code(),
        dir,
    )
}

/// A leads_to `P` fails only on the cone with `WF_vars(Next)`, module `Cone_P_WF`.
const WF_NAMED: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, delivered, cancelled) @default(pending)  delivered_at: DateTime @null }
usecase Place "注文"
usecase Deliver "配達"
usecase Cancel "取消"
usecase Reopen "再開"
creates(Place, Order)
transitions(Deliver, Order.status, pending -> delivered)
sets(Deliver, Order.delivered_at == present)
transitions(Cancel, Order.status, pending -> cancelled)
transitions(Reopen, Order.status, cancelled -> pending)
property P "注文はいずれ届く"
  leads_to(Order.status == pending, Order.status == delivered)
property P_WF "届いた注文には日時がある"
  always(Order.status != delivered or Order.delivered_at == present)
"#;

/// `Q`'s cone reduces the gate and the key behind it, so its counterexample is read on the
/// unreduced cone, module `Cone_Q_Full`. The key is what the reduction folds: the gate's states
/// alone are each a class (#101).
const FULL_NAMED: &str = r#"
entity Key "鍵" { id: Int @pk  status: Enum(locked, unlocked) @default(locked) }
usecase Unlock "解錠"
transitions(Unlock, Key.status, locked -> unlocked)
condition KeyOpen "鍵が開いている"
holds(KeyOpen, Key.status == unlocked)
applies_to(KeyOpen, Swing)
entity Gate "門" { id: Int @pk  status: Enum(shut, open) @default(shut)  latch: Bool @default(false) }
entity Job "作業" { id: Int @pk  status: Enum(waiting, running) @default(waiting) }
relate(Job, Gate, N:1)
usecase BuildGate "設置"
usecase Swing "開く"
usecase Unlatch "掛け金を外す"
usecase Submit "投入"
usecase Start "開始"
creates(BuildGate, Gate)
transitions(Swing, Gate.status, shut -> open)
sets(Unlatch, Gate.latch == true)
creates(Submit, Job)
transitions(Start, Job.status, waiting -> running)
condition GateOpen "門が開いている"
holds(GateOpen, Gate.status == open)
applies_to(GateOpen, Start)
property Q "作業は待つ"
  always(Job.status == waiting)
property Q_Full "作業はまだ待つ"
  always(Job.status == waiting)
"#;

/// A property named like another property's `_WF` or `_Full` module keeps `Cone_<name>`, and
/// the other module takes `_2`; the two shared one file, so the first property's second run
/// read the wrong spec and ended in `tool_error`.
#[test]
fn a_property_named_like_another_property_module_keeps_its_own_spec() {
    let (stdout, stderr, code, dir) = verify_model("verify_cli_wf_named", WF_NAMED, "M.tla");
    assert_eq!(code, Some(1), "{stdout}{stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("Cone_P [P]: counterexample (")
                && l.ends_with(", Cone_P_WF_2: counterexample)")),
        "{stdout}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("Cone_P_WF [P_WF]: ok (")),
        "{stdout}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("cones.txt")).unwrap(),
        "Cone_P.tla: P\nCone_P_WF_2.tla: P\nCone_P_WF.tla: P_WF\n"
    );
    assert!(
        stderr.contains("error[P]: property P is violated: "),
        "{stderr}"
    );
    assert!(!stderr.contains("error[P_WF]"), "{stderr}");

    let (stdout, stderr, code, dir) = verify_model("verify_cli_full_named", FULL_NAMED, "M.tla");
    assert_eq!(code, Some(1), "{stdout}{stderr}");
    for (module, property, full) in [
        ("Cone_Q", "Q", "Cone_Q_Full_2"),
        ("Cone_Q_Full", "Q_Full", "Cone_Q_Full_Full"),
    ] {
        assert!(
            stdout.lines().any(|l| l
                .starts_with(&format!("{module} [{property}]: counterexample ("))
                && l.ends_with(&format!(", {full}: counterexample)"))),
            "{stdout}"
        );
        assert!(
            stderr.contains(&format!(
                "error[{property}]: property {property} is violated: "
            )),
            "{stderr}"
        );
    }
    assert!(dir.join("Cone_Q_Full_2.tla").exists());
    assert!(!stdout.contains("tool_error"), "{stdout}");
}

/// A cone reruns without the check it has reported, from `Cone_MultiInv_Order_0_rest1.cfg`,
/// the cfg of the property `MultiInv_Order_0_rest1`; the rerun takes `_rest1_2.cfg`, and the
/// property's cfg still names the property.
#[test]
fn a_rerun_cfg_named_like_a_property_module_leaves_that_cfg() {
    if !have_tlc() {
        return;
    }
    let src = format!(
        "{SAME_STATE_RULES}property MultiInv_Order_0_rest1 \"注文は払われない\"\n  always(Order.status == pending)\n"
    );
    let (stdout, stderr, code, dir) =
        verify_model_with("verify_cli_rest_named", &src, "M.tla", true);
    assert_eq!(code, Some(1), "{stdout}{stderr}");
    assert!(
        stderr.contains("error[MultiInv_Order_0]: invariant MultiInv_Order_0 is violated: "),
        "{stderr}"
    );
    let cfg = |name: &str| std::fs::read_to_string(dir.join(format!("{name}.cfg"))).unwrap();
    assert!(
        cfg("Cone_MultiInv_Order_0_rest1").contains("PROPERTY MultiInv_Order_0_rest1\n"),
        "{}",
        cfg("Cone_MultiInv_Order_0_rest1")
    );
    let rest = cfg("Cone_MultiInv_Order_0_rest1_2");
    assert!(rest.contains("INVARIANT MultiInv_Order_0\n"), "{rest}");
    assert!(!rest.contains("Forbidden_Parcel_0"), "{rest}");
}

/// `-o Cone.tla` names the whole-model module `Cone`, so the whole-model run of `P` is
/// `Cone_P`, the name of `P`'s cone: the whole-model spec takes `Cone_P_2`, and the cone's
/// spec is left as it ran.
#[test]
fn a_whole_model_run_named_like_a_cone_leaves_the_cone_spec() {
    let src = r#"entity E "E" { id: Int @pk  status: Enum(a, b) @default(a)  seen: Bool @default(false) }
usecase Make "作成"
usecase Flip "切替"
creates(Make, E)
transitions(Flip, E.status, a -> b)
sets(Flip, E.seen == true)
condition Fresh "新規"
holds(Fresh, E.status == a)
applies_to(Fresh, Make)
property P eventually(E.status == b)
"#;
    let (stdout, stderr, code, dir) = verify_model("verify_cli_whole_named", src, "Cone.tla");
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("Cone_P [P]: not decided (")),
        "{stdout}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("Cone_P_2 [P]: ok (")),
        "{stdout}"
    );
    let cone = std::fs::read_to_string(dir.join("Cone_P.tla")).unwrap();
    assert!(cone.starts_with("---- MODULE Cone_P ----\n"), "{cone}");
    assert!(!cone.contains("WF_vars(Next)"), "the cone's spec: {cone}");
    let whole = std::fs::read_to_string(dir.join("Cone_P_2.tla")).unwrap();
    assert!(
        whole.contains("WF_vars(Next)"),
        "the whole-model spec: {whole}"
    );
    assert!(dir.join("Cone.tla").exists());
}

/// `property Ready` and `property ready` wrote `Cone_Ready.tla` and `Cone_ready.tla`, one file
/// where the file system folds case (macOS, Windows), so a run read the other's spec and
/// ended in `tool_error`. The later module takes `_2`; each property keeps its name.
#[test]
fn properties_whose_names_differ_in_case_write_two_files() {
    let src = r#"
entity Job "作業" { id: Int @pk  status: Enum(waiting, running, done) @default(waiting) }
usecase Submit "投入"
usecase Start "開始"
usecase Finish "完了"
creates(Submit, Job)
transitions(Start, Job.status, waiting -> running)
transitions(Finish, Job.status, running -> done)
property Ready "x"
  always(Job.status != running)
property ready "y"
  always(Job.status != done)
"#;
    let (stdout, stderr, code, dir) = verify_model("verify_cli_case_named", src, "M.tla");
    assert_eq!(code, Some(1), "{stdout}{stderr}");
    assert_eq!(
        std::fs::read_to_string(dir.join("cones.txt")).unwrap(),
        "Cone_Ready.tla: Ready\nCone_ready_2.tla: ready\n"
    );
    for (module, property) in [("Cone_Ready", "Ready"), ("Cone_ready_2", "ready")] {
        assert!(
            stdout
                .lines()
                .any(|l| l.starts_with(&format!("{module} [{property}]: counterexample ("))),
            "{stdout}"
        );
        assert!(
            stderr.contains(&format!(
                "error[{property}]: property {property} is violated: "
            )),
            "{stderr}"
        );
    }
    assert!(!stdout.contains("tool_error"), "{stdout}");
}

/// A property named like a var (`Job_status`, the column `Job.status`) keeps its name in
/// `verify`'s labels and findings; the var takes `_2`.
#[test]
fn a_property_named_like_a_var_keeps_its_name() {
    let src = r#"
entity Job "作業" { id: Int @pk  status: Enum(waiting, running, done) @default(waiting) }
usecase Submit "投入"
usecase Start "開始"
creates(Submit, Job)
transitions(Start, Job.status, waiting -> running)
property Job_status "never running"
  always(Job.status != running)
"#;
    let (stdout, stderr, code, _dir) = verify_model("verify_cli_property_var", src, "M.tla");
    assert_eq!(code, Some(1), "{stdout}{stderr}");
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with("Cone_Job_status [Job_status]: counterexample (")),
        "{stdout}"
    );
    assert!(
        stderr.contains("error[Job_status]: property Job_status is violated: "),
        "{stderr}"
    );
    if have_tlc() {
        let (_, stderr, code, _) =
            verify_model_with("verify_cli_property_var_tlc", src, "M.tla", true);
        assert_eq!(code, Some(1), "{stderr}");
        assert!(stderr.contains("Job_status_2[1] = "), "{stderr}");
    }
}

/// `--tlc-jar` names the jar TLC runs from under `--with-tlc`, so a missing jar is an error
/// that names it, with or without `tlc` on PATH.
#[test]
fn a_missing_tlc_jar_is_named_in_the_error() {
    let tmp = scratch("missing_tlc_jar");
    let jar = tmp.join("absent.jar");
    let output = verify(
        &repo(),
        &tmp,
        &[
            "samples/formal-verification/check_met.rdra",
            "--with-tlc",
            "--tlc-jar",
            jar.to_str().unwrap(),
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("tla2tools.jar not found at {}", jar.display())),
        "{stderr}"
    );
}

/// The check on `qty` reads one column; the forbidden pair reads `qty` and `flagged`, so its
/// cone holds the check's cone and its module the check too.
const NESTED_CONES: &str = r#"
entity Order "注文" { id: Int @pk  qty: Int @default("1") @check(qty >= 1)  flagged: Bool @default(false) }
usecase Place "注文"
usecase Bump "増量"
usecase Flag "印"
creates(Place, Order)
sets(Bump, Order.qty == 2)
sets(Flag, Order.flagged == true)
"#;

/// A cone whose checks another cone's module holds is not run when that cone passes: its line
/// names the cone that checked it. When that cone fails, the smaller one runs as before, so
/// its line carries its own state count and the counterexample is the larger cone's alone.
#[test]
fn a_cone_another_cone_covers_runs_only_when_that_cone_fails() {
    let line = |stdout: &str| {
        stdout
            .lines()
            .find(|l| l.starts_with("Cone_Check_Order_qty_0 "))
            .unwrap_or_else(|| panic!("no line for the check's cone\n{stdout}"))
            .to_string()
    };
    // No order has qty 0, so the pair is never reached and the larger cone passes.
    let passing = format!("{NESTED_CONES}forbidden(Order, qty == 0, flagged == true)\n");
    let (stdout, stderr, code, dir) = verify_model("verify_cli_covered_cone", &passing, "specs");
    assert_eq!(code, Some(0), "{stdout}\n{stderr}");
    let covered = line(&stdout);
    assert!(
        covered.ends_with(": ok (checked in Cone_Forbidden_Order_0)"),
        "{covered}"
    );
    // The covered cone's spec is still written, for a run by hand.
    assert!(dir.join("specs/Cone_Check_Order_qty_0.tla").is_file());

    // Bump then Flag reaches the pair: the larger cone fails, so the check's cone runs.
    let failing = format!("{NESTED_CONES}forbidden(Order, qty == 2, flagged == true)\n");
    let (stdout, stderr, code, _) = verify_model("verify_cli_uncovered_cone", &failing, "specs");
    assert_eq!(code, Some(1), "{stdout}\n{stderr}");
    let ran = line(&stdout);
    assert!(ran.contains(": ok (") && ran.contains(" states)"), "{ran}");
    assert!(!stdout.contains("checked in"), "{stdout}");
    assert!(stderr.contains("Forbidden_Order_0"), "{stderr}");
    assert!(
        !stderr.contains("Check_Order_qty_0 is violated"),
        "{stderr}"
    );
}

/// Mark writes `a` and `b` and must leave an open row. The forbidden pair reads `status` and
/// `a`, so its cone holds the assert's reads and its module the assert, but not `b`: there a
/// Mark that only sets `b` is a stutter, on which the assert passes. Make, Close, Mark is a
/// real run that breaks the assert, and only the assert's own cone, which holds everything
/// Mark writes, shows it.
const ASSERT_BEYOND_THE_LARGER_MODULE: &str = r#"
entity E "e" { id: Int @pk  status: Enum(open, closed) @default(open)  a: Bool @default(false)  b: Bool @default(false) }
usecase Make "make"
usecase Mark "mark"
usecase Close "close"
creates(Make, E)
updates(Mark, E)
updates(Close, E)
sets(Make, E.a == true)
sets(Mark, E.a == true)
sets(Mark, E.b == true)
transitions(Close, E.status, open -> closed)
after(Mark).assert(E.status == open)
forbidden(E, status == closed, a == false)
"#;

/// Handle runs only on an event that Raise raises, and Raise never runs. The cone of
/// `v == true` alone leaves the event's mark out, so Handle runs freely there and the rule
/// fails; the cone of the pair holds `status`, and with it the mark, so Handle never runs and
/// both rules pass there. The larger cone holds Handle back where the smaller lets it run.
const GUARD_ONLY_THE_LARGER_CONE_KEEPS: &str = r#"
entity E "e" { id: Int @pk  status: Enum(open, ready, raised) @default(open)  v: Bool @default(false) }
usecase Make "make"
usecase Raise "raise"
usecase Handle "handle"
event Ev "ev"
creates(Make, E)
updates(Raise, E)
updates(Handle, E)
transitions(Raise, E.status, ready -> raised)
sets(Handle, E.v == true)
raises(Raise, Ev)
triggers(Ev, Handle)
forbidden(E, v == true)
forbidden(E, v == true, status == raised)
"#;

/// A module that holds a cone's checks does not cover the cone when it takes fewer steps
/// than the cone does: it lacks a var the cone's use case writes, or it keeps a guard the
/// cone drops. Each cone runs and reports its counterexample.
#[test]
fn a_cone_that_takes_steps_the_larger_module_does_not_still_runs() {
    for (test, src, failing) in [
        (
            "verify_cli_assert_cone",
            ASSERT_BEYOND_THE_LARGER_MODULE,
            "Cone_AfterAssert_Mark_0 ",
        ),
        (
            "verify_cli_dropped_guard_cone",
            GUARD_ONLY_THE_LARGER_CONE_KEEPS,
            "Cone_Forbidden_E_0 ",
        ),
    ] {
        let (stdout, stderr, code, _) = verify_model(test, src, "specs");
        assert_eq!(code, Some(1), "{test}\n{stdout}\n{stderr}");
        assert!(!stdout.contains("checked in"), "{test}\n{stdout}");
        let line = stdout
            .lines()
            .find(|l| l.starts_with(failing))
            .unwrap_or_else(|| panic!("{test}: no line for {failing}\n{stdout}"));
        assert!(line.contains(": counterexample ("), "{test}: {line}");
    }
}

/// No order is ever held, so the rule about held orders says nothing; Ship needs a held
/// order, so it never runs and its assertion says nothing; and the trigger of the leads_to
/// never holds. TLC passes all three.
const VACUOUS_CHECKS: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(open, paid) @default(open)  held: Bool @default(false)  flagged: Bool @default(false) }
usecase Place "注文"
usecase Pay "支払"
usecase Ship "出荷"
creates(Place, Order)
transitions(Pay, Order.status, open -> paid)
updates(Ship, Order)
sets(Ship, Order.flagged == true)
condition Held "保留"
holds(Held, Order.held == true)
applies_to(Held, Ship)
invariant(Order).when(held == true).then(flagged == true)
after(Ship).assert(Order.flagged == true)
property HeldFlags "保留は印"
  leads_to(Order.held == true, Order.flagged == true)
"#;

/// `verify` explores each cone it decides and warns of a check that holds vacuously: a rule,
/// an `after` assertion, and a `leads_to`. The run still passes, and at two instances
/// the warning carries no note about one instance.
#[test]
fn verify_warns_of_checks_that_hold_vacuously() {
    let (stdout, stderr, code, _) = verify_model("verify_cli_vacuous", VACUOUS_CHECKS, "specs");
    assert_eq!(code, Some(0), "{stdout}\n{stderr}");
    assert!(stdout.contains("\nresult: ok\n"), "{stdout}");
    for warning in [
        "warning[Vacuous_Inv_Order_0]: Inv_Order_0 holds vacuously: no reachable state has a row of Order with held=true\n",
        "warning[Vacuous_AfterAssert_Ship_0]: AfterAssert_Ship_0 holds vacuously: Ship never runs to its last step\n",
        "warning[Vacuous_HeldFlags]: HeldFlags holds vacuously: its leads_to trigger holds in no reachable state\n",
    ] {
        assert!(stderr.contains(warning), "missing {warning}\n{stderr}");
    }
}

/// The names of the `error[<name>]:` findings, sorted.
fn error_names(stderr: &str) -> Vec<String> {
    let mut names: Vec<String> = stderr
        .lines()
        .filter_map(|l| l.strip_prefix("error[")?.split_once("]:"))
        .map(|(name, _)| name.to_string())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Capture moves a payment without paying its order, and nothing sets `paid_at`: the rule on
/// shipped orders, the assertion after Pay, and both `always` properties fail, the one with
/// `.along` on a payment beside its own order.
const SAFETY_VIOLATED: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(draft, paid, shipped) @default(draft)  paid_at: DateTime @null }
entity Payment "支払" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
relate(Payment, Order, N:1)
usecase Pay "支払"
usecase Ship "出荷"
usecase Capture "支払確定"
transitions(Pay, Order.status, draft -> paid)
transitions(Ship, Order.status, paid -> shipped)
transitions(Capture, Payment.status, pending -> captured)
invariant(Order).when(status == shipped).then(paid_at == present)
after(Pay).assert(Order.paid_at == present)
property CapturedPaid always((not (Payment.status == captured)) or (Order.status != draft)).along(Payment, Order)
property CapturedPaidAnyOrder always((not (Payment.status == captured)) or (Order.status != draft))
"#;

/// `SAFETY_VIOLATED` with Pay capturing the payment of the order it pays and setting
/// `paid_at`, and a way out of draft and pending for the rows Pay leaves: every check holds.
const SAFETY_HELD: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(draft, paid, shipped, cancelled) @default(draft)  paid_at: DateTime @null }
entity Payment "支払" { id: Int @pk  status: Enum(pending, captured, voided) @default(pending) }
relate(Payment, Order, N:1)
usecase Pay "支払"
usecase Ship "出荷"
usecase Cancel "取消"
usecase Void "無効"
transitions(Cancel, Order.status, draft -> cancelled)
transitions(Void, Payment.status, pending -> voided)
transitions(Pay, Order.status, draft -> paid)
transitions(Pay, Payment.status, pending -> captured)
sets(Pay, Order.paid_at == present)
transitions(Ship, Order.status, paid -> shipped)
invariant(Order).when(status == shipped).then(paid_at == present)
after(Pay).assert(Order.paid_at == present)
property CapturedPaid always((not (Payment.status == captured)) or (Order.status != draft)).along(Payment, Order)
"#;

/// The native checker decides a rule, an `after` assertion, and `always` with and without
/// `.along`, with no TLC on PATH: the run fails with the error names TLC reports beside it,
/// each with BFS's trace, and passes where they hold.
#[test]
fn a_native_verify_decides_the_safety_checks_without_tlc() {
    let tmp = scratch("verify_cli_native_safety");
    let model = tmp.join("violated.rdra");
    std::fs::write(&model, SAFETY_VIOLATED).unwrap();
    let output = verify(&repo(), &tmp, &[model.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    let names = error_names(&stderr);
    for name in [
        "Inv_Order_0",
        "AfterAssert_Pay_0",
        "CapturedPaid",
        "CapturedPaidAnyOrder",
    ] {
        assert!(
            names.iter().any(|n| n == name),
            "{name}: {names:?}\n{stderr}"
        );
    }
    assert!(!stdout.contains("tlc_result:"), "{stdout}");
    assert!(!stderr.contains("from TLC"), "{stderr}");
    assert!(
        stdout.lines().any(|l| l.starts_with("Cone_Inv_Order_0 [")
            && l.contains("]: counterexample (")
            && l.ends_with(" states)")),
        "{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with("Cone_CapturedPaid [CapturedPaid]: counterexample (")),
        "{stdout}"
    );
    // The shortest trace: Capture moves the payment of a draft order in one step.
    assert!(
        stderr.contains("error[CapturedPaid]: property CapturedPaid is violated: its always condition is false for Payment#1, Order#1 after step 1\n"),
        "{stderr}"
    );
    assert!(
        stderr.contains("verification failed: 2 invariant or after-assert violation(s) and 2 property violation(s)"),
        "{stderr}"
    );
    if have_tlc() {
        let tlc = verify_with_tlc(&repo(), &tmp, &[model.to_str().unwrap()]);
        let tlc_stderr = String::from_utf8_lossy(&tlc.stderr);
        assert_eq!(tlc.status.code(), Some(1), "{tlc_stderr}");
        assert_eq!(error_names(&tlc_stderr), names, "{tlc_stderr}");
        assert!(
            tlc_stderr.contains("\ntlc_result: counterexample\n"),
            "{tlc_stderr}"
        );
    }

    let model = tmp.join("held.rdra");
    std::fs::write(&model, SAFETY_HELD).unwrap();
    let output = verify(&repo(), &tmp, &[model.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}{stderr}");
    assert_eq!(error_names(&stderr), Vec::<String>::new(), "{stderr}");
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with("Cone_Inv_Order_0 [") && l.contains("]: ok (")),
        "{stdout}"
    );
    assert!(
        stdout.contains("\nCone_CapturedPaid [CapturedPaid]: ok ("),
        "{stdout}"
    );
    if have_tlc() {
        let tlc = verify_with_tlc(&repo(), &tmp, &[model.to_str().unwrap()]);
        let tlc_stdout = String::from_utf8_lossy(&tlc.stdout);
        assert_eq!(
            tlc.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&tlc.stderr)
        );
        assert!(
            tlc_stdout.contains("\nresult: ok\ntlc_result: ok\n"),
            "{tlc_stdout}"
        );
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

/// A `.lemma()` rule only helps `--prove`, so the run explores no cone for it,
/// and still decides the other rule's cone. That cone's system holds the lemma, so it is
/// checked there, as TLC checks every rule of the module it runs.
#[test]
fn a_native_verify_runs_no_cone_for_a_lemma() {
    let tmp = scratch("verify_cli_native_lemma");
    let model = tmp.join("m.rdra");
    std::fs::write(
        &model,
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
    let specs = tmp.join("specs");
    let output = verify(
        &repo(),
        &tmp,
        &[model.to_str().unwrap(), "-o", specs.to_str().unwrap()],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}{stderr}");
    assert!(stdout.contains("\ncones: 1 for 2 invariant(s)"), "{stdout}");
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with("Cone_Unique_Shipment_Order [") && l.contains("]: ok (")),
        "{stdout}"
    );
    assert!(!stdout.contains("Cone_Quantifier_Order_0"), "{stdout}");
    let cones = std::fs::read_to_string(specs.join("cones.txt")).unwrap();
    assert!(!cones.contains("Cone_Quantifier_Order_0"), "{cones}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// `eventually` and `leads_to` are decided natively, with no TLC on PATH: on the cone with
/// `Fair_P` alone, again with `WF_vars(Next)`, and on the whole model when the cone leaves
/// them open. `--with-tlc` runs TLC's ladder beside it, and the two agree run by run.
#[test]
fn liveness_is_decided_without_tlc_and_tlc_agrees() {
    let tmp = scratch("verify_cli_native_liveness");
    for (sample, code, lines) in [
        (
            "temporal_unmet.rdra",
            1,
            &["Cone_PendingLeadsToDelivered [PendingLeadsToDelivered]: counterexample (31 states, Cone_PendingLeadsToDelivered_WF: counterexample)"][..],
        ),
        (
            "temporal_fair.rdra",
            1,
            &["Cone_GrantEnds [GrantEnds]: ok (43 states)"][..],
        ),
        (
            "order.rdra",
            0,
            &[
                "Cone_EventuallyTerminal [EventuallyTerminal]: not decided (31 states, Cone_EventuallyTerminal_WF: ok, needs the whole model's WF_vars(Next))",
                "RdraSpec_EventuallyTerminal [EventuallyTerminal]: ok (31 states)",
            ][..],
        ),
    ] {
        let sample = repo().join("samples/formal-verification").join(sample);
        let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(code), "{stdout}{stderr}");
        for line in lines {
            assert!(stdout.lines().any(|l| l == *line), "{line}\n{stdout}");
        }
        let result = match code {
            0 => stdout.lines().any(|l| l == "result: ok"),
            _ => stderr.lines().any(|l| l == "result: counterexample"),
        };
        assert!(result, "{stdout}{stderr}");
        if !have_tlc() {
            continue;
        }
        let tlc = verify_with_tlc(&repo(), &tmp, &[sample.to_str().unwrap()]);
        let tlc_stdout = String::from_utf8_lossy(&tlc.stdout);
        let tlc_stderr = String::from_utf8_lossy(&tlc.stderr);
        assert_eq!(tlc.status.code(), Some(code), "{tlc_stdout}{tlc_stderr}");
        assert!(!tlc_stderr.contains("EngineDisagreement"), "{tlc_stderr}");
        for line in lines {
            assert!(
                tlc_stdout.lines().any(|l| l == format!("tlc: {line}")),
                "tlc: {line}\n{tlc_stdout}"
            );
        }
        assert_eq!(error_names(&tlc_stderr), error_names(&stderr), "{tlc_stderr}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

/// A `tlc` on PATH that passes every module whatever it holds: a stand-in for a TLC that
/// disagrees with the native checker.
#[cfg(unix)]
fn fake_tlc(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("fake-tlc");
    std::fs::create_dir_all(&bin).unwrap();
    let tlc = bin.join("tlc");
    std::fs::write(
        &tlc,
        "#!/bin/sh\necho '7 states generated, 7 distinct states found, 0 states left on queue.'\necho 'Model checking completed. No error has been found.'\n",
    )
    .unwrap();
    std::fs::set_permissions(&tlc, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// When the engines decide a check differently, neither verdict is the result: the run prints
/// the engine-disagreement line with both, reports `tool_error`, keeps its specs, and exits 1.
#[cfg(unix)]
#[test]
fn an_engine_disagreement_fails_the_run_and_keeps_the_specs() {
    let tmp = scratch("verify_cli_disagreement");
    let path = fake_tlc(&tmp);
    let sample = repo().join("samples/formal-verification/check_unmet.rdra");
    let output = verify_on_path(
        &repo(),
        &tmp,
        &path,
        &[sample.to_str().unwrap(), "--with-tlc"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    assert!(
        stderr.contains("\nerror[EngineDisagreement]: Cone_Check_CartLine_qty_0 [Check_CartLine_qty_0]: native counterexample, TLC ok; one engine is wrong (a tool bug, not a finding about the model)\n"),
        "{stderr}"
    );
    assert!(
        stderr.contains("\nresult: tool_error\ntlc_result: tool_error\n"),
        "{stderr}"
    );
    let dir = stderr
        .lines()
        .find_map(|l| l.strip_prefix("specs: "))
        .and_then(|l| l.strip_suffix(" (kept because the engines disagree)"))
        .unwrap_or_else(|| panic!("no kept specs: line\n{stderr}"));
    assert!(Path::new(dir)
        .join("Cone_Check_CartLine_qty_0.tla")
        .is_file());
    assert!(
        stderr.contains("1 check(s) the native checker and TLC decide differently (a tool bug)"),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(&tmp);
}

/// `--backend tlc` still runs TLC, as `--with-tlc` does, and warns that it is deprecated.
#[cfg(unix)]
#[test]
fn backend_tlc_warns_and_runs_tlc_beside_the_native_checker() {
    let tmp = scratch("verify_cli_backend_tlc");
    let path = fake_tlc(&tmp);
    let sample = repo().join("samples/formal-verification/check_met.rdra");
    let output = verify_on_path(
        &repo(),
        &tmp,
        &path,
        &[sample.to_str().unwrap(), "--backend", "tlc"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}{stderr}");
    assert!(
        stderr.starts_with("warning: --backend tlc is deprecated: it now means --with-tlc"),
        "{stderr}"
    );
    assert!(
        stdout.contains("\ntlc: Cone_Forbidden_CartLine_0 [Forbidden_CartLine_0, Check_CartLine_qty_0]: ok (7 states)\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("\nresult: ok\ntlc_result: ok\n"),
        "{stdout}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

/// `--with-tlc` without `tlc` on PATH is an error; without the flag a `--tlc-jar` that names
/// nothing starts no TLC and only warns.
#[test]
fn with_tlc_needs_tlc_and_tlc_jar_needs_with_tlc() {
    let tmp = scratch("verify_cli_needs_tlc");
    let sample = repo().join("samples/formal-verification/check_met.rdra");
    let output = verify(&repo(), &tmp, &[sample.to_str().unwrap(), "--with-tlc"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("TLC not found on PATH"), "{stderr}");

    let jar = tmp.join("absent.jar");
    let output = verify(
        &repo(),
        &tmp,
        &[sample.to_str().unwrap(), "--tlc-jar", jar.to_str().unwrap()],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}{stderr}");
    assert!(
        stderr.contains("warning: --tlc-jar applies only with --with-tlc; this run starts no TLC"),
        "{stderr}"
    );
    assert!(!stdout.contains("tlc_result"), "{stdout}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// A safety cone whose system is an entity cone's is explored once, by the entity pass, with
/// every check (#209): the run goes to the end for the entity's findings, so the failing
/// cone's line counts the whole cone, 65,793 states, where a run of its own stops after 3
/// (`a_safety_cone_stops_once_every_check_on_it_has_failed`). The counterexample is the one
/// BFS finds first either way.
#[test]
fn a_safety_cone_shared_with_an_entity_cone_counts_the_whole_cone() {
    let tmp = scratch("verify_cli_shared_cone");
    let model = repo().join("crates/rdra-ish-cli/tests/fixtures/early_stop/flags_fail.rdra");
    let output = verify(&repo(), &tmp, &[model.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    assert!(
        stdout.contains(
            "\nCone_Forbidden_Task_0 [Forbidden_Task_0]: counterexample (65793 states)\n"
        ),
        "{stdout}"
    );
    assert!(
        stderr.contains("error[Forbidden_Task_0]: invariant Forbidden_Task_0 is violated"),
        "{stderr}"
    );
    assert!(stderr.contains("  counterexample (2 steps):"), "{stderr}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// A safety cone stops once every check on it has failed (#213): the flags rule's cone has
/// 65,793 states at two instances, and BFS stops after 3 with the 2-step counterexample and
/// no cap note. TLC, which explores the whole module, gives the same verdict. Task's entity
/// cone also holds `note`, which the rule does not read, so the two cones are apart.
#[test]
fn a_safety_cone_stops_once_every_check_on_it_has_failed() {
    let tmp = scratch("verify_cli_early_stop");
    let model = repo().join("crates/rdra-ish-cli/tests/fixtures/early_stop/flags_fail_apart.rdra");
    let output = verify(&repo(), &tmp, &[model.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    assert!(
        stdout.contains("\nCone_Forbidden_Task_0 [Forbidden_Task_0]: counterexample (3 states)\n"),
        "{stdout}"
    );
    assert!(
        stderr.contains("error[Forbidden_Task_0]: invariant Forbidden_Task_0 is violated"),
        "{stderr}"
    );
    assert!(stderr.contains("  counterexample (2 steps):"), "{stderr}");
    if have_tlc() {
        let output = verify_with_tlc(&repo(), &tmp, &[model.to_str().unwrap()]);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stdout.contains(
                "\ntlc: Cone_Forbidden_Task_0 [Forbidden_Task_0]: counterexample (65793 states)\n"
            ),
            "{stdout}"
        );
        assert!(!stderr.contains("EngineDisagreement"), "{stderr}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
