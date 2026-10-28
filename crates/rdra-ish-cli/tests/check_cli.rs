//! What `check` prints for a violated rule: one verdict, its rule site, and a shortest
//! counterexample in model terms.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

#[test]
fn check_prints_one_verdict_with_its_counterexample() {
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args([
            "check",
            "samples/formal-verification/cross_order_payment_split.rdra",
        ])
        .output()
        .expect("rdra-ish runs");
    assert_eq!(output.status.code(), Some(1));
    let root = format!("{}/", repo().display());
    let stderr = String::from_utf8_lossy(&output.stderr).replace(&root, "");
    let file = "skills/rdra-ish-verify/samples/cross_order_payment_split.rdra";
    let expected = format!(
        "\
bounds: 1 instance per entity, Int 0..5; 4096 states per cone; entities a check does not read are unbounded
error: invariant CrossForbidden_Order_0 is violated: multi-entity forbidden state is reachable across [Order, Payment]: Order.status=cancelled AND Payment.status=captured witnessed by Order(status=cancelled); Payment(status=captured)
  rule: {file}:41:1
  counterexample (3 steps):
    1. Place [Order#1, Payment#1]                            {file}:14:1
         + Order#1 (status=pending)
         + Payment#1 (status=pending, owner Order=Order#1)
    2. Pay_1, transaction 1 of Pay [Payment#1]               {file}:15:1
         Payment#1.status: pending -> captured               {file}:36:1
    3. Cancel [Order#1]                                      {file}:16:1
         Order#1.status: pending -> cancelled                {file}:35:1
"
    );
    assert_eq!(stderr, expected);
    let verdicts = stderr.lines().filter(|l| l.starts_with("error:")).count();
    assert_eq!(verdicts, 1, "a multi-entity rule prints once: {stderr}");
}

const BOUNDS: &str = "bounds: 1 instance per entity, Int 0..5; 4096 states per cone; entities a check does not read are unbounded";

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(args)
        .output()
        .expect("rdra-ish runs")
}

/// `check` and `states` state the bounds on stderr, and a clean `check` repeats them in its
/// `OK:` line.
#[test]
fn check_and_states_print_the_bounds_line() {
    let sample = "samples/formal-verification/check_met.rdra";
    let check = run(&["check", sample]);
    assert!(check.status.success());
    let stderr = String::from_utf8_lossy(&check.stderr);
    assert_eq!(
        stderr.lines().find(|l| l.starts_with("bounds:")),
        Some(BOUNDS)
    );
    assert_eq!(
        String::from_utf8_lossy(&check.stdout),
        "OK: no errors within 1 instance per entity, Int 0..5\n"
    );

    let states = run(&["states", sample]);
    let stderr = String::from_utf8_lossy(&states.stderr);
    assert_eq!(
        stderr.lines().find(|l| l.starts_with("bounds:")),
        Some(BOUNDS)
    );
}

/// A state of a state model that no use case reaches is one error, with the arm that would
/// enter it; `states` reports it on the entity's view, and each reachable pattern carries the
/// path that reaches it.
#[test]
fn check_reports_a_state_no_use_case_reaches() {
    let sample = "samples/formal-verification/unwitnessed_state.rdra";
    let output = run(&["check", sample]);
    assert_eq!(output.status.code(), Some(1));
    let root = format!("{}/", repo().display());
    let stderr = String::from_utf8_lossy(&output.stderr).replace(&root, "");
    let file = "skills/rdra-ish-verify/samples/unwitnessed_state.rdra";
    assert_eq!(
        stderr,
        format!(
            "\
{BOUNDS}
error: state Order.status = refunded has no reachability witness: no sequence of use cases from the start reaches it
  state model: Order.status, entered by Refund (shipped -> refunded)   {file}:31:1
"
        )
    );

    let states = run(&["states", sample, "--format", "json"]);
    assert_eq!(states.status.code(), Some(1));
    let json: serde_json::Value =
        serde_json::from_slice(&states.stdout).expect("states prints JSON");
    let order = &json[0];
    assert!(order["diagnostics"][0]
        .as_str()
        .unwrap()
        .contains("Order.status = refunded has no reachability witness"));
    let shipped = order["patterns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["values"]["status"] == "shipped")
        .expect("shipped is reachable");
    let actions: Vec<&str> = shipped["witness"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, ["Place", "Pay", "Ship"]);
}

/// From a relative input, `states --format json` names witness sites in the path form
/// `check`'s text renderer prints for the same file.
#[test]
fn witness_sites_use_the_path_form_of_check() {
    let sample = "samples/formal-verification/unwitnessed_state.rdra";
    let check = run(&["check", sample]);
    let stderr = String::from_utf8_lossy(&check.stderr);
    let text_site = stderr
        .lines()
        .find(|l| l.contains("entered by Refund"))
        .and_then(|l| l.split_whitespace().last())
        .expect("check names the arm's site");
    let text_path = text_site.rsplitn(3, ':').nth(2).unwrap();

    let states = run(&["states", sample, "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&states.stdout).unwrap();
    let sites: Vec<String> = json[0]["patterns"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["witness"]["steps"].as_array().unwrap().clone())
        .map(|s| {
            s["site"]
                .as_str()
                .expect("a use case step has a site")
                .to_string()
        })
        .collect();
    assert!(!sites.is_empty());
    for site in &sites {
        assert_eq!(
            site.rsplitn(3, ':').nth(2),
            Some(text_path),
            "{site} vs {text_site}"
        );
    }
}
