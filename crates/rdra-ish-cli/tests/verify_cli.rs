//! Where `verify` leaves its specs. Without `-o` a failed run keeps its temp dir and names
//! it; a relative `-o` resolves against the working directory, not TLC's.
//!
//! Without `tlc` on PATH each test is skipped with a printed reason; `RDRA_REQUIRE_TLC=1`
//! turns the skip into a failure.

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

fn verify(cwd: &Path, tmpdir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(cwd)
        .env("TMPDIR", tmpdir)
        .arg("verify")
        .args(args)
        .output()
        .expect("rdra-ish runs")
}

#[test]
fn failed_verify_without_out_keeps_and_names_its_specs() {
    if !have_tlc() {
        return;
    }
    let tmp = scratch("verify_cli_kept_specs");
    let sample = repo().join("samples/formal-verification-fail/order.rdra");
    let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
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

#[test]
fn passing_verify_without_out_removes_its_specs() {
    if !have_tlc() {
        return;
    }
    let tmp = scratch("verify_cli_removed_specs");
    let sample = repo().join("samples/formal-verification/check_met.rdra");
    let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let left: Vec<_> = std::fs::read_dir(&tmp).unwrap().flatten().collect();
    assert!(
        left.iter()
            .all(|e| !e.file_name().to_string_lossy().starts_with("rdra-ish-tlc-")),
        "a passing run left {left:?}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn relative_out_is_relative_to_the_working_directory() {
    if !have_tlc() {
        return;
    }
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

/// TLC's counterexample prints in model terms, through the renderer `check` uses, before
/// TLC's own error line.
#[test]
fn a_tlc_counterexample_prints_in_model_terms() {
    if !have_tlc() {
        return;
    }
    let tmp = scratch("verify_cli_model_terms");
    let sample = repo().join("samples/formal-verification/cross_order_payment_split.rdra");
    let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    let mut rest: &str = &stderr;
    for expected in [
        "error[CrossForbidden_Order_0]: invariant CrossForbidden_Order_0 is violated: ",
        "  rule: ",
        "cross_order_payment_split.rdra:44:1",
        "  counterexample (3 steps, from TLC):",
        "    1. Place [Order#1, Payment#1]",
        "    2. Pay_1, transaction 1 of Pay [Payment#1]",
        "         Payment#1.status: pending -> captured",
        "    3. Cancel [Order#1]",
        "violated: Error: Invariant CrossForbidden_Order_0 is violated.",
    ] {
        let at = rest
            .find(expected)
            .unwrap_or_else(|| panic!("{expected:?} missing or out of order\n{stderr}"));
        rest = &rest[at + expected.len()..];
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
    if !have_tlc() {
        return;
    }
    let tmp = scratch("verify_cli_same_state");
    let model = tmp.join("m.rdra");
    std::fs::write(&model, SAME_STATE_RULES).unwrap();
    let output = verify(&repo(), &tmp, &[model.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    let cone = stderr
        .split("\nCone_")
        .find(|block| block.starts_with("CrossInv_Order_0 ("))
        .unwrap_or_else(|| panic!("no block for the cross-entity cone\n{stderr}"));
    for name in ["Forbidden_Parcel_0", "CrossInv_Order_0"] {
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
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Each temporal property runs in its own module and its lasso prints in model terms, named
/// after the property. TLC picks the lasso, so only its shape is fixed: a leads_to that an
/// endless cancel and reopen breaks loops back; an always may stutter or loop.
#[test]
fn a_temporal_counterexample_prints_its_lasso_in_model_terms() {
    if !have_tlc() {
        return;
    }
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
        &rest[..rest.find("\nviolated: ").unwrap_or(rest.len())]
    };
    let always = block("NeverShipped");
    assert!(
        always.contains("its always condition is false for Order#"),
        "{always}"
    );
    assert!(always.contains(", from TLC):"), "{always}");
    assert!(
        always.ends_with("\n    then nothing changes forever")
            || always.contains("\n    then loops back to step "),
        "{always}"
    );
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
    if !have_tlc() {
        return;
    }
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

/// A property without `.fair` rests on the whole model's `WF_vars(Next)`, which the cone
/// does not keep: the cone's two runs leave it undecided, and the whole-model run decides it
/// under the budget. With a budget of 0 s the whole-model run is stopped, the property is
/// listed as not decided, and the run still ends in `OK:` as a BFS cone at its cap does.
#[test]
fn a_property_the_cone_cannot_decide_runs_on_the_whole_model_under_the_budget() {
    if !have_tlc() {
        return;
    }
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
    assert!(stdout.contains("\ntlc_result: ok\n"), "{stdout}");
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
    let _ = std::fs::remove_dir_all(&tmp);
}

/// `verify` states the bounds first, and a passing run ends with them in its `OK:` line.
#[test]
fn verify_prints_the_bounds_line_and_an_ok_line_that_repeats_them() {
    if !have_tlc() {
        return;
    }
    let tmp = scratch("verify_cli_bounds");
    let sample = repo().join("samples/formal-verification/check_met.rdra");
    let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.lines().any(|l| l
            == "bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); 8388608 states per BFS cone; entities a check does not read are unbounded"),
        "{stdout}"
    );
    assert_eq!(
        stdout.lines().last(),
        Some("OK: no errors within 2 instances per entity, Int 0..5"),
        "{stdout}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

/// A state no use case reaches fails `verify` from BFS on the entity cones, though TLC finds
/// no violation.
#[test]
fn an_unwitnessed_state_fails_verify_while_tlc_passes() {
    if !have_tlc() {
        return;
    }
    let tmp = scratch("verify_cli_unwitnessed");
    let sample = repo().join("samples/formal-verification/unwitnessed_state.rdra");
    let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    assert!(stdout.lines().any(|l| l == "tlc_result: ok"), "{stdout}");
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

/// An instance that can never leave a state fails `verify` from BFS on the entity cones,
/// though TLC finds no violation; the summary line counts it.
#[test]
fn a_deadlock_fails_verify_while_tlc_passes() {
    if !have_tlc() {
        return;
    }
    let tmp = scratch("verify_cli_deadlock");
    let sample = repo().join("samples/formal-verification/deadlock.rdra");
    let output = verify(&repo(), &tmp, &[sample.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    assert!(stdout.lines().any(|l| l == "tlc_result: ok"), "{stdout}");
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

/// `verify --entity Order` defers Audit's build error, runs TLC on Order's invariant cone,
/// and passes in the selected scope; the whole run stops at the build gate.
#[test]
fn an_entity_scope_verifies_part_of_a_model() {
    if !have_tlc() {
        return;
    }
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
    assert!(lines.contains(&"tlc_result: ok"), "{stdout}");
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
    if !have_tlc() {
        return;
    }
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
    if !have_tlc() {
        return;
    }
    let tmp = scratch("verify_cli_guard_on_unassigned");
    let model = tmp.join("m.rdra");
    std::fs::write(
        &model,
        r#"entity E "E" { id: Int @pk  status: Enum(a, b) @default(a)  seen: Bool @default(false) }
usecase Make "作成"
usecase Flip "切替"
creates(Make, E)
transitions(Flip, E.status, a -> b)
sets(Flip, E, seen == true)
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
    assert!(lines.contains(&"tlc_result: ok"), "{stdout}");
    let _ = std::fs::remove_dir_all(&tmp);
}
