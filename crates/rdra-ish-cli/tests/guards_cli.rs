//! `check --guards`: each guard dropped in turn, and the rules that then fail.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

#[path = "common/root.rs"]
mod root;
use root::repo;

/// The exit code and stdout of `rdra-ish <args>` run at the repository root, with sites
/// relative to it.
fn run(args: &[&str]) -> (Option<i32>, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(args)
        .output()
        .expect("rdra-ish runs");
    let text = |b: &[u8]| common::relative_to(&String::from_utf8_lossy(b), &repo());
    (
        output.status.code(),
        text(&output.stdout),
        text(&output.stderr),
    )
}

const FIXTURES: &str = "crates/rdra-ish-cli/tests/fixtures/guards";

/// One guard keeps the rule; the other keeps none, and the report says what that means
/// without calling it wrong. The report is on stdout and leaves the exit code to `check`.
#[test]
fn a_guard_no_rule_depends_on_is_named_with_its_use_case() {
    let file = format!("{FIXTURES}/unprotected.rdra");
    let (code, stdout, stderr) = run(&["check", &file, "--guards"]);
    assert_eq!(code, Some(0), "{stderr}");
    let expected = format!(
        "\
guards: 2 guards: 1 protect a rule, 1 no rule depends on; 2 cones explored again
guard: condition PaidOnly on Ship   {file}:28:1
  protects: 「A shipped order was paid」
guard: condition StockChecked on Ship   {file}:32:1
  no rule depends on this guard; it is either a business rule nobody wrote as a rule, or not needed
rules:
  「A shipped order was paid」 depends on condition PaidOnly on Ship
OK: no errors within 1 instance per entity, Int 0..5
"
    );
    assert_eq!(stdout, expected);
    assert!(
        !stderr.contains("warning") && !stderr.contains("error"),
        "the report adds no finding: {stderr}"
    );
}

/// Without `--guards`, `check` prints no report.
#[test]
fn check_without_the_flag_prints_no_report() {
    let file = format!("{FIXTURES}/unprotected.rdra");
    let (code, stdout, _) = run(&["check", &file]);
    assert_eq!(code, Some(0));
    assert_eq!(
        stdout,
        "OK: no errors within 1 instance per entity, Int 0..5\n"
    );
}

/// A cone that stops at the cap once a guard is gone decides nothing: the rule is not
/// decided without the guard, and the guard is neither protecting nor unprotected.
#[test]
fn a_cone_past_the_cap_without_the_guard_is_not_decided() {
    let file = format!("{FIXTURES}/not_decided.rdra");
    let (code, stdout, stderr) = run(&["check", &file, "--guards", "--max-states", "6"]);
    assert_eq!(code, Some(0), "{stderr}");
    let expected = format!(
        "\
guards: 2 guards: 0 protect a rule, 0 no rule depends on, 2 not decided; 2 cones explored again
guard: condition BFirst on FlipA   {file}:31:1
  not decided: 「c moves only after b」 (without the guard its cone stopped at the state cap (6 states))
guard: condition ReadyForC on FlipC   {file}:35:1
  not decided: 「c moves only after b」 (without the guard its cone stopped at the state cap (6 states))
rules:
  「c moves only after b」 depends on no guard, not decided without condition BFirst on FlipA; condition ReadyForC on FlipC
OK: no errors within 1 instance per entity, Int 0..5
"
    );
    assert_eq!(stdout, expected);
    // With room for the 7 states, the rule holds without either guard alone.
    let (_, stdout, _) = run(&["check", &file, "--guards", "--max-states", "7"]);
    assert!(
        stdout.starts_with("guards: 2 guards: 0 protect a rule, 2 no rule depends on;"),
        "{stdout}"
    );
}

/// A model that fails `check` gets no report: a rule that already fails depends on nothing.
#[test]
fn a_failing_model_gets_no_report() {
    let (code, stdout, stderr) = run(&[
        "check",
        "samples/formal-verification/deadlock.rdra",
        "--guards",
    ]);
    assert_eq!(code, Some(1));
    assert!(!stdout.contains("guards:"), "{stdout}");
    assert!(
        stderr.contains("note: --guards reports nothing on a model that fails check"),
        "{stderr}"
    );
}

/// The ids a `.rdra` declaration line starting with `keyword` names, in every file of `dir`.
fn declared(dir: &Path, keyword: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("sample dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rdra") {
                let text = std::fs::read_to_string(&path).expect("rdra file");
                for line in text.lines() {
                    if let Some(rest) = line.strip_prefix(&format!("{keyword} ")) {
                        let id: String = rest
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect();
                        out.insert(id);
                    }
                }
            }
        }
    }
    out
}

/// The identifiers inside the backtick spans of a table cell.
fn quoted_ids(cell: &str) -> BTreeSet<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .flat_map(|span| {
            span.split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .filter(|w| !w.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The "Rules" table of samples/oidc-auth/README.md against the report: one row per
/// labelled rule, and in each row the use cases (and conditions) of exactly the guards the
/// rule depends on. A rule the README protects by something other than a guard (the token
/// sets of the implicit responses, transitions of `RejectCodeReplay`, the `links` of
/// `ApproveDevice`) depends on no guard; a row that pairs a guard with a trigger (the
/// Logout Token rule) depends on neither alone.
#[test]
fn oidc_auth_readme_rules_table_is_what_the_report_finds() {
    let (code, stdout, stderr) = run(&["check", "samples/oidc-auth", "--guards"]);
    assert_eq!(code, Some(0), "{stderr}");
    let sample = repo().join("samples/oidc-auth");
    let usecases = declared(&sample, "usecase");
    let conditions = declared(&sample, "condition");
    let readme = std::fs::read_to_string(sample.join("README.md")).expect("README");
    let (_, table) = readme.split_once("\n## Rules\n").expect("a Rules section");
    let rows: BTreeMap<&str, &str> = table
        .lines()
        .skip_while(|l| !l.starts_with("| Rule |"))
        .skip(2)
        .take_while(|l| l.starts_with('|'))
        .map(|l| {
            let cells: Vec<&str> = l.trim_matches('|').split(" | ").map(str::trim).collect();
            (cells[0], cells[1])
        })
        .collect();
    assert_eq!(rows.len(), 13, "{rows:?}");
    // `「label」 depends on <guard>; <guard>` or `depends on no guard`.
    let mut seen = 0;
    for line in stdout.lines().skip_while(|l| *l != "rules:").skip(1) {
        let Some(rest) = line.trim().strip_prefix('「') else {
            continue;
        };
        let (label, guards) = rest.split_once("」 depends on ").expect("a rules line");
        let cell = rows
            .get(label)
            .unwrap_or_else(|| panic!("no README row for 「{label}」"));
        seen += 1;
        let named = quoted_ids(cell);
        let named_ucs: BTreeSet<&String> = named.iter().filter(|w| usecases.contains(*w)).collect();
        let named_conds: BTreeSet<&String> =
            named.iter().filter(|w| conditions.contains(*w)).collect();
        if guards == "no guard" {
            assert!(
                named_conds.is_empty() || cell.contains("triggers("),
                "「{label}」 depends on no single guard, but its row names one alone: {cell}"
            );
            continue;
        }
        let (mut ucs, mut conds) = (BTreeSet::new(), BTreeSet::new());
        for guard in guards.split("; ") {
            let (what, uc) = guard.rsplit_once(" on ").expect("a guard on a use case");
            ucs.insert(uc.to_string());
            if let Some(cond) = what.strip_prefix("condition ") {
                conds.insert(cond.to_string());
            }
        }
        assert_eq!(
            named_ucs,
            ucs.iter().collect(),
            "「{label}」: the row's use cases are not those of the guards it depends on: {cell}"
        );
        assert_eq!(
            named_conds,
            conds.iter().collect(),
            "「{label}」: the row's conditions are not those it depends on: {cell}"
        );
    }
    assert_eq!(
        seen,
        rows.len(),
        "every labelled rule has a rules: line: {stdout}"
    );
}
