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
