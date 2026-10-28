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

/// A never-run action names its transaction group and lists only the guards that hold in no
/// reachable state, each at its site: Pay_2's settlement condition reads a state only Refund
/// enters, and Refund leaves `paid`, which only Pay_2 enters.
#[test]
fn check_lists_only_the_guards_that_never_hold() {
    let dir = std::env::temp_dir().join(format!("rdra-never-runs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();
    let file = dir.join("never_runs.rdra");
    std::fs::write(
        &file,
        r#"entity Order "注文" { id: Int @pk  status: Enum(pending, paid, refunded) @default(pending) }
entity Payment "決済" { id: Int @pk  status: Enum(pending, captured) @default(pending) }
relate(Payment, Order, N:1)
usecase Place "注文確定"
usecase Pay "決済確定"
usecase Refund "返金"
api CapturePaymentApi "Capture"
api PayOrderApi "Pay order"
creates(Place, Order)
creates(Place, Payment)
invokes(Pay, CapturePaymentApi)
updates(CapturePaymentApi, Payment)
invokes(Pay, PayOrderApi)
updates(PayOrderApi, Order)
updates(Refund, Order)
transitions(Order.status, Pay, pending -> paid)
transitions(Payment.status, Pay, pending -> captured)
transitions(Order.status, Refund, paid -> refunded)
condition Settled "精算済み"
holds(Settled, Order.status == refunded)
applies_to(Settled, Pay)
"#,
    )
    .unwrap();
    let output = run(&["check", file.to_str().unwrap()]);
    let _ = std::fs::remove_dir_all(&dir);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let blocks: Vec<&str> = stderr
        .split("\nwarning: ")
        .skip(1)
        .map(|b| b.split("\nerror: ").next().unwrap())
        .collect();
    assert_eq!(blocks.len(), 2, "{stderr}");
    let squash = |b: &str| {
        b.lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n")
            .replace(&format!("{}/", dir.display()), "")
    };
    assert_eq!(
        squash(blocks[0]),
        "\
transaction 2 of Pay (Pay_2) can never run: in no reachable state do its guards hold
declared: transaction 2 of Pay (Pay_2) never_runs.rdra:5:1
guard: condition Settled never_runs.rdra:20:1"
    );
    assert_eq!(
        squash(blocks[1]),
        "\
use case Refund can never run: in no reachable state do its guards hold
declared: use case Refund never_runs.rdra:6:1
guard: Order.status is paid (transitions of Refund) never_runs.rdra:18:1"
    );
    let warnings = stderr
        .lines()
        .filter(|l| l.starts_with("warning: "))
        .count();
    assert_eq!(warnings, 2, "each action warns once: {stderr}");
}

/// A state of a state model that no use case reaches is one error, with the arm that would
/// enter it; the Order that would leave shipped through it never does, a deadlock; the use
/// case that would enter it never runs, a warning with its declaration; `states` reports all
/// three on the entity's view, and each reachable pattern carries the path that reaches it.
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
error: deadlock: Order#1 stays at status = shipped forever, and shipped is not an end state
  leaves shipped: Refund (shipped -> refunded)   {file}:31:1
    guard: condition RefundWindow                {file}:36:1
  reached by (3 steps):
    1. Place [Order#1]                                 {file}:13:1
         + Order#1 (status=pending, shipped_at=null)
    2. Pay [Order#1]                                   {file}:14:1
         Order#1.status: pending -> paid               {file}:29:1
    3. Ship [Order#1]                                  {file}:15:1
         Order#1.status: paid -> shipped               {file}:30:1
         Order#1.shipped_at: null -> present           {file}:33:1
warning: use case Refund can never run: each of its guards holds in some reachable state, but never all of them in one
  declared: use case Refund   {file}:16:1
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
    assert_eq!(
        order["diagnostics"][1],
        "[error] deadlock: Order#1 stays at status = shipped forever, and shipped is not an end state"
    );
    assert_eq!(
        order["diagnostics"][2],
        "[warn] Refund can never run: each of its guards holds in some reachable state, but never all of them in one"
    );
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

/// An Order whose only way out of paid never opens is one deadlock error: the way out with
/// the guard that never holds while it is stuck, and a shortest path into it.
#[test]
fn check_reports_an_instance_that_never_leaves_its_state() {
    let sample = "samples/formal-verification/deadlock.rdra";
    let output = run(&["check", sample]);
    assert_eq!(output.status.code(), Some(1));
    let root = format!("{}/", repo().display());
    let stderr = String::from_utf8_lossy(&output.stderr).replace(&root, "");
    let file = "skills/rdra-ish-verify/samples/deadlock.rdra";
    assert_eq!(
        stderr,
        format!(
            "\
{BOUNDS}
error: deadlock: Order#1 stays at status = paid forever, and paid is not an end state
  leaves paid: Ship (paid -> shipped)   {file}:29:1
    guard: condition Packed             {file}:33:1
  reached by (2 steps):
    1. Place [Order#1]                     {file}:12:1
         + Order#1 (status=pending)
    2. Pay [Order#1]                       {file}:13:1
         Order#1.status: pending -> paid   {file}:28:1
warning: use case Ship can never run: each of its guards holds in some reachable state, but never all of them in one
  declared: use case Ship   {file}:14:1
"
        )
    );
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
