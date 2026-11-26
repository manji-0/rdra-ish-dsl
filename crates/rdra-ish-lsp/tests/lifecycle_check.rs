//! The background lifecycle check (#214): after an analysis publishes, the server runs
//! `check`'s exploration at one instance per entity and publishes its findings at the rules
//! they are about, with a code lens on each rule.

mod common;

use std::path::{Path, PathBuf};
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use common::{uri, Client};
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
fn clinic_ops() -> PathBuf {
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
    client.wait_for(what, |m| {
        m["method"] == "textDocument/publishDiagnostics"
            && m["params"]["uri"] == uri(path).as_str()
            && {
                let d = m["params"]["diagnostics"].as_array().unwrap();
                d.iter().any(|d| d["source"] == SOURCE) && ok(d)
            }
    })["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .clone()
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

fn change(client: &mut Client, path: &Path, version: i64, text: &str) {
    client.notify(
        "textDocument/didChange",
        json!({"textDocument": {"uri": uri(path), "version": version},
            "contentChanges": [{"text": text}]}),
    );
}

#[test]
fn a_broken_guard_shows_the_rule_it_breaks_with_its_lens() {
    let root = clinic_ops();
    let (orders, entities) = (root.join(ORDERS), root.join(ENTITIES));
    let rules = std::fs::read_to_string(&entities).unwrap();
    let rule_a = line_of(&rules, "医師事務作業補助者が入力したオーダは");
    let text = std::fs::read_to_string(&orders).unwrap();
    let mut client = client(&root);
    client.open(&orders, &text);
    wait_for_findings(&client, &entities, "the first check", |d| {
        finding(d, A).is_none()
    });
    assert_eq!(
        lens_at(&mut client, &entities, rule_a),
        "check: ok (1 instance)"
    );

    let started = Instant::now();
    change(&mut client, &orders, 2, &text.replace(GUARD, BROKEN_GUARD));
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
    let _ = std::fs::remove_dir_all(root);
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
    let broken = text.replace(GUARD, BROKEN_GUARD);
    for version in 2..22 {
        change(
            &mut client,
            &orders,
            version,
            &format!("{broken}// 編集 {version} 𠮷\n"),
        );
    }
    change(
        &mut client,
        &orders,
        22,
        &format!("{}// 最後の編集 𠮷\n", text.replace(RESULT_TIME, "")),
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
    let _ = std::fs::remove_dir_all(root);
}
