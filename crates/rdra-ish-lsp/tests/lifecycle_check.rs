//! The background lifecycle check (#214): after an analysis publishes, the server runs
//! `check`'s exploration at one instance per entity and publishes its findings at the rules
//! they are about, with a code lens on each rule.

mod common;

use std::path::Path;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use common::{uri, Client, Workspace};
use serde_json::{json, Value};

const SOURCE: &str = "rdra-ish check (1 instance)";
const ORDERS: &str = "buc/buc_orders_results.rdra";
const ENTITIES: &str = "shared/entities.rdra";
/// The guard that keeps a specimen from being collected before the physician confirms the
/// order; with `false` for `true`, `Forbidden_ClinicalOrder_1` fails.
const GUARD: &str = "holds(OrderConfirmedByPhysician, ClinicalOrder.physician_confirmed == true,";
const BROKEN_GUARD: &str =
    "holds(OrderConfirmedByPhysician, ClinicalOrder.physician_confirmed == false,";
/// The effect `Inv_ClinicalOrder_3` needs: without it a resulted order has no result time.
const RESULT_TIME: &str = "sets(ReceiveLabResult, ClinicalOrder.resulted_at == present)\n";
const A: &str = "Forbidden_ClinicalOrder_1";
const B: &str = "Inv_ClinicalOrder_3";

/// A workspace holding the clinic-ops sample's model files.
fn clinic_ops() -> Workspace {
    let root = common::workspace("rdra_lsp_lifecycle");
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/clinic-ops");
    for dir in ["buc", "shared"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
        for entry in std::fs::read_dir(sample.join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rdra") {
                std::fs::copy(&path, root.join(dir).join(path.file_name().unwrap())).unwrap();
            }
        }
    }
    root
}

/// A server with the check on. A debug build explores far slower than a release one, so the
/// budget leaves room for it.
fn client(root: &Path) -> Client {
    Client::initialized_with(
        root,
        json!({"lifecycleCheck": {"enabled": true, "timeBudgetMs": 60000, "quietMs": 50}}),
    )
}

/// A `z3` that records each call in the file the first path names, put first on `PATH` and
/// named by `RDRA_Z3` in the environment the second holds: `verify` runs the proof by
/// default, and the editor's check runs none, so nothing calls it.
#[cfg(unix)]
fn z3_spy(root: &Path) -> (std::path::PathBuf, Vec<(&'static str, std::ffi::OsString)>) {
    use std::os::unix::fs::PermissionsExt;
    let dir = root.join(".z3-spy");
    std::fs::create_dir_all(&dir).unwrap();
    let called = dir.join("called");
    let z3 = dir.join("z3");
    std::fs::write(
        &z3,
        format!("#!/bin/sh\necho \"$@\" >> '{}'\nexit 3\n", called.display()),
    )
    .unwrap();
    std::fs::set_permissions(&z3, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(std::iter::once(dir).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    (
        called,
        vec![("PATH", path), ("RDRA_Z3", z3.into_os_string())],
    )
}

/// The line index of the first line of `text` that contains `needle`.
fn line_of(text: &str, needle: &str) -> u64 {
    text.lines()
        .position(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("no line holds {needle}")) as u64
}

/// The check's diagnostic named `code`, if `diagnostics` hold one.
fn finding<'a>(diagnostics: &'a [Value], code: &str) -> Option<&'a Value> {
    diagnostics
        .iter()
        .find(|d| d["source"] == SOURCE && d["code"] == code)
}

/// Waits for diagnostics of `path` that hold a finding of the check and pass `ok`.
fn wait_for_findings(
    client: &Client,
    path: &Path,
    what: &str,
    mut ok: impl FnMut(&[Value]) -> bool,
) -> Vec<Value> {
    client.wait_for_diagnostics(&uri(path), what, |d| {
        d.iter().any(|d| d["source"] == SOURCE) && ok(d)
    })
}

/// The title of the check's code lens on line `line` of `path`.
fn lens_at(client: &mut Client, path: &Path, line: u64) -> String {
    let lenses = client.request(
        "textDocument/codeLens",
        json!({"textDocument": {"uri": uri(path)}}),
    );
    let lenses = lenses["result"].as_array().unwrap();
    let titles: Vec<&str> = lenses
        .iter()
        .filter(|l| l["range"]["start"]["line"] == line)
        .filter_map(|l| l["command"]["title"].as_str())
        .filter(|t| t.starts_with("check:"))
        .collect();
    assert_eq!(titles.len(), 1, "lenses on line {line}: {lenses:?}");
    titles[0].to_string()
}

/// `text` with `from` replaced by `to`, which it must hold.
fn edit(text: &str, from: &str, to: &str) -> String {
    assert!(text.contains(from), "the text holds no {from:?}");
    text.replace(from, to)
}

#[test]
fn a_broken_guard_shows_the_rule_it_breaks_with_its_lens() {
    let root = clinic_ops();
    let (orders, entities) = (root.join(ORDERS), root.join(ENTITIES));
    let rules = std::fs::read_to_string(&entities).unwrap();
    let rule_a = line_of(&rules, "医師事務作業補助者が入力したオーダは");
    let text = std::fs::read_to_string(&orders).unwrap();
    // The check runs no proof: a z3 on PATH is never called (unix only).
    #[cfg(unix)]
    let (called, env) = z3_spy(&root);
    #[cfg(not(unix))]
    let env: Vec<(&str, std::ffi::OsString)> = Vec::new();
    let env: Vec<(&str, &std::ffi::OsStr)> = env.iter().map(|(k, v)| (*k, v.as_os_str())).collect();
    let mut client = Client::initialized_with_env(
        &root,
        json!({"lifecycleCheck": {"enabled": true, "timeBudgetMs": 60000, "quietMs": 50}}),
        &env,
    );
    client.open(&orders, &text);
    let first = wait_for_findings(&client, &entities, "the first check", |d| {
        finding(d, A).is_none()
    });
    // A finding that names no entity of its own is shown once per entity, under the name
    // `check` prints.
    for entity in [
        "AccountBalance",
        "AuditEvent",
        "InsurancePolicy",
        "Provider",
    ] {
        let code = format!("NoCreationPath_{entity}");
        let found = finding(&first, &code).unwrap_or_else(|| panic!("no {code}: {first:?}"));
        assert_eq!(found["severity"], 3, "{found}");
    }
    assert_eq!(
        lens_at(&mut client, &entities, rule_a),
        "check: ok (1 instance)"
    );

    let started = Instant::now();
    client.change_now(&orders, 2, &edit(&text, GUARD, BROKEN_GUARD));
    let found = wait_for_findings(&client, &entities, "the broken rule", |d| {
        finding(d, A).is_some()
    });
    eprintln!(
        "clinic-ops: the broken rule was published {:?} after the edit",
        started.elapsed()
    );
    let a = finding(&found, A).unwrap();
    assert_eq!(a["severity"], 1, "{a}");
    assert_eq!(a["range"]["start"]["line"], rule_a, "{a}");
    assert_eq!(
        lens_at(&mut client, &entities, rule_a),
        "check: violated (1 instance)"
    );
    #[cfg(unix)]
    assert!(
        !called.exists(),
        "the lifecycle check ran z3: {:?}",
        std::fs::read_to_string(&called)
    );
}

#[test]
fn a_burst_of_edits_ends_on_the_findings_of_the_last_text() {
    let root = clinic_ops();
    let (orders, entities) = (root.join(ORDERS), root.join(ENTITIES));
    let rules = std::fs::read_to_string(&entities).unwrap();
    let (rule_a, rule_b) = (
        line_of(&rules, "医師事務作業補助者が入力したオーダは"),
        line_of(&rules, "結果が出たオーダには結果日時"),
    );
    let text = std::fs::read_to_string(&orders).unwrap();
    let mut client = client(&root);
    client.open(&orders, &text);
    wait_for_findings(&client, &entities, "the first check", |_| true);

    // Twenty edits with the guard broken, each with a comment of multi-byte text appended, and
    // a last one that mends the guard and drops the result time instead.
    let broken = edit(&text, GUARD, BROKEN_GUARD);
    for version in 2..22 {
        client.change_now(&orders, version, &format!("{broken}// 編集 {version} 𠮷\n"));
    }
    client.change_now(
        &orders,
        22,
        &format!("{}// 最後の編集 𠮷\n", edit(&text, RESULT_TIME, "")),
    );

    let mut last = wait_for_findings(&client, &entities, "the findings of the last text", |d| {
        finding(d, B).is_some()
    });
    // Nothing older may be published after them.
    loop {
        match client.messages.recv_timeout(Duration::from_secs(3)) {
            Ok(m) => {
                if m["method"] == "textDocument/publishDiagnostics"
                    && m["params"]["uri"] == uri(&entities).as_str()
                {
                    last = m["params"]["diagnostics"].as_array().unwrap().clone();
                }
            }
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => panic!("server exited"),
        }
    }
    assert!(finding(&last, A).is_none(), "a stale finding: {last:?}");
    let b = finding(&last, B).unwrap_or_else(|| panic!("no {B} in {last:?}"));
    assert_eq!(b["range"]["start"]["line"], rule_b, "{b}");
    assert_eq!(
        lens_at(&mut client, &entities, rule_a),
        "check: ok (1 instance)"
    );
    assert_eq!(
        lens_at(&mut client, &entities, rule_b),
        "check: violated (1 instance)"
    );
}

/// A check whose budget is spent decides nothing: every cone stops at once, which is
/// information, and each rule's lens says it is not decided.
#[test]
fn a_spent_budget_leaves_the_rules_not_decided_and_reports_no_error() {
    let root = clinic_ops();
    let (orders, entities) = (root.join(ORDERS), root.join(ENTITIES));
    let rules = std::fs::read_to_string(&entities).unwrap();
    let rule_a = line_of(&rules, "医師事務作業補助者が入力したオーダは");
    let text = std::fs::read_to_string(&orders).unwrap();
    let mut client = Client::initialized_with(
        &root,
        json!({"lifecycleCheck": {"enabled": true, "timeBudgetMs": 0, "quietMs": 50}}),
    );
    client.open(&orders, &edit(&text, GUARD, BROKEN_GUARD));
    let found = wait_for_findings(&client, &entities, "the stopped check", |d| {
        d.iter().any(|d| d["code"] == "TimeBudgetReached_Provider")
    });
    let checked: Vec<&Value> = found.iter().filter(|d| d["source"] == SOURCE).collect();
    assert!(finding(&found, A).is_none(), "{checked:?}");
    assert!(
        checked.iter().all(|d| d["severity"] != 1),
        "an error from a stopped check: {checked:?}"
    );
    assert_eq!(
        lens_at(&mut client, &entities, rule_a),
        "check: not decided (1 instance)"
    );
}
