//! Where `verify` leaves its specs. Without `-o` a failed run keeps its temp dir and names
//! it; a relative `-o` resolves against the working directory, not TLC's.
//!
//! Without `tlc` on PATH each test is skipped with a printed reason; `RDRA_REQUIRE_TLC=1`
//! turns the skip into a failure.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
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
        "error: invariant CrossForbidden_Order_0 is violated: ",
        "  rule: ",
        "cross_order_payment_split.rdra:41:1",
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
            == "bounds: 2 instances per entity, Int 0..5 (TLA+ InstanceCount=2, IntRange=0..5); 2097152 states per BFS cone; entities a check does not read are unbounded"),
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
            "error: state Order.status = refunded has no reachability witness: no sequence of use cases from the start reaches it\n  state model: Order.status, entered by Refund (shipped -> refunded)"
        ),
        "{stderr}"
    );
    assert!(stderr.contains("unwitnessed_state.rdra:31:1"), "{stderr}");
    assert!(
        stderr.contains("\nwarning: use case Refund can never run: each of its guards holds in some reachable state, but never all of them in one\n"),
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
            "error: deadlock: Order#1 stays at status = paid forever, and paid is not an end state\n  leaves paid: Ship (paid -> shipped)"
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
