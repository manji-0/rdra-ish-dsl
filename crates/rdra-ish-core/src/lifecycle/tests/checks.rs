//! Which invariants and after-asserts each cone's run evaluates (`Checks`): the checks among
//! its targets, every check for a cone of checks alone, and none for an entity cone.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::model_from;
use crate::lifecycle::{
    Bounds, Caps, Checks, Diag, EntId, Scope, StateDiag, Target, TransitionSystem,
};

/// A closed customer breaks the rule within two steps, and the Customer entity's cone holds
/// the rule.
const CLOSING: &str = r#"
entity Customer "Customer" {
  id: Int @pk
  status: Enum(active, closed) @default(active)
}
entity Order "Order" {
  id: Int @pk
  status: Enum(open, paid) @default(open)
}
relate(Order, Customer, N:1)
usecase Join "Join"
usecase Place "Place"
usecase Pay "Pay"
usecase Close "Close"
creates(Join, Customer)
creates(Place, Order)
transitions(Pay, Order.status, open -> paid)
transitions(Close, Customer.status, active -> closed)
forbidden(Customer, status == closed)
"#;

fn violations(diags: &[Diag]) -> Vec<Diag> {
    diags
        .iter()
        .filter(|d| matches!(d.diag, StateDiag::ConstraintViolated { .. }))
        .cloned()
        .collect()
}

/// An entity cone evaluates no check, so it reports nothing of a rule it holds and breaks;
/// its graph is the one a run of every check explores. The rule's own cone reports the
/// violation with the trace a run of every check finds, alone or beside the entity target.
#[test]
fn an_entity_cone_reports_no_check_and_the_rule_cone_reports_its_own() {
    let ts = TransitionSystem::build(&model_from(CLOSING), Bounds::STATES, &Scope::ALL).ts;
    let caps = Caps::for_instances(1);
    let customer = (0..ts.entities().len())
        .map(EntId::at)
        .find(|e| ts.entity(*e).id == "Customer")
        .expect("Customer");
    let (entity, rule) = (Target::Entity(customer), Target::Invariant(0));

    // The entity cone, run as it ran before: it holds the rule and breaks it.
    let cone = ts.target_cone(entity);
    let all = ts.restrict_to(&cone.vars, &cone.keep).run(caps);
    assert_eq!(violations(&all.diags).len(), 1, "{:?}", all.diags);

    let alone = ts.explore_targets(&[entity], caps);
    let run = &alone.cone_of(entity).run;
    assert_eq!(run.checks(), &Checks::none());
    assert!(violations(&run.diags).is_empty(), "{:?}", run.diags);
    assert!(violations(alone.verdicts()).is_empty());
    assert_eq!(run.states, all.states);
    assert_eq!(run.edges(), all.edges());
    assert_eq!(run.complete, all.complete);

    // The rule's own cone, alone: it stops once the rule fails, as before.
    let checked = ts.explore_targets(&[rule], caps);
    let run = &checked.cone_of(rule).run;
    assert_eq!(run.checks(), &Checks::All);
    let cone = ts.target_cone(rule);
    let before = ts
        .restrict_to(&cone.vars, &cone.keep)
        .run(caps.until_all_violated());
    let found = violations(&run.diags);
    assert_eq!(found.len(), 1, "{:?}", run.diags);
    assert_eq!(found, violations(&before.diags));
    let verdict = violations(checked.verdicts());
    assert_eq!(verdict.len(), 1);

    // Beside the entity target: on one cone the run evaluates the rule, on two the rule's
    // cone does. Either way the verdict is the one above.
    let both = ts.explore_targets(&[entity, rule], caps);
    assert_eq!(violations(both.verdicts()), verdict);
    let run = &both.cone_of(rule).run;
    assert!(run.checks().reads(&ts.invariants()[0].name));
    assert_eq!(violations(&run.diags), found);
    assert!(violations(&both.cone_of(entity).run.diags).len() <= 1);
}

/// The places that read a run's invariant and after-assert results. A run evaluates only the
/// checks its caller asks for (`Checks`), so each of these must read a check its run
/// evaluated: a check target on its own cone (`explore_targets_with` evaluates the checks
/// among a cone's targets), or a run of `Checks::All` (`TransitionSystem::run`, verify's
/// safety cones). A new reader belongs here once it does; it fails this test until then.
const CHECK_READERS: &[(&str, &str)] = &[
    ("rdra-ish-cli/src/verify/bfs.rs", "of_parts"),
    (
        "rdra-ish-core/src/lifecycle/explore/bfs.rs",
        "stop_if_all_failed",
    ),
    (
        "rdra-ish-core/src/lifecycle/guards.rs",
        "guard_dependencies",
    ),
    // Reads the verdicts of each check target, which its own cone evaluated.
    (
        "rdra-ish-core/src/lifecycle/states.rs",
        "derive_states_checked",
    ),
    // Reads the violation of the check it decides again, on a run of the check's exact cone
    // that evaluates every check (`run`).
    ("rdra-ish-core/src/lifecycle/targets.rs", "recheck"),
    ("rdra-ish-core/src/lifecycle/targets.rs", "unmet_of_assert"),
    ("rdra-ish-core/src/lifecycle/targets.rs", "unmet_of_rule"),
    ("rdra-ish-core/src/lifecycle/targets.rs", "verdict"),
];

/// Every function outside tests that reads a run's check results: one that reads `.diags`
/// (or `.diagnostics()`, or takes `diags: &[Diag]`) and names a violation (`ConstraintViolated`, `AfterAssertViolated`, or `reports(`), or
/// asks whether every check has failed (`violated.all(`).
#[test]
fn the_readers_of_check_results_are_pinned() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .to_path_buf();
    let mut files = Vec::new();
    collect_sources(&crates, &mut files);
    let mut found = BTreeSet::new();
    for file in files {
        let rel = file
            .strip_prefix(&crates)
            .expect("under crates/")
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(&file).expect("a source file");
        for (name, body) in functions(&text) {
            // `.diags` on a run, `.diagnostics()` on a `StoredRun`, or the diags a caller
            // hands in.
            let diags = [".diags", ".diagnostics()", "diags: &[Diag]"]
                .iter()
                .any(|k| body.contains(k));
            let names = ["ConstraintViolated", "AfterAssertViolated", "reports("]
                .iter()
                .any(|k| body.contains(k));
            let violated = body.contains("violated.all(");
            if (diags && names) || violated {
                found.insert((rel.clone(), name));
            }
        }
    }
    let pinned: BTreeSet<(String, String)> = CHECK_READERS
        .iter()
        .map(|(f, n)| (f.to_string(), n.to_string()))
        .collect();
    assert_eq!(
        found, pinned,
        "the readers of a run's check results changed"
    );
}

/// The `.rs` files under each crate's `src/`, test modules left out.
fn collect_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("a directory")
        .map(|e| e.expect("an entry").path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if !matches!(
                name,
                "tests" | "target" | "examples" | "benches" | "fixtures"
            ) {
                collect_sources(&path, out);
            }
        } else if name.ends_with(".rs")
            && name != "tests.rs"
            && !name.ends_with("_tests.rs")
            && path.components().any(|c| c.as_os_str() == "src")
        {
            out.push(path);
        }
    }
}

/// Each function of a file, by name, with its body: the lines from its `fn` to the next
/// `fn`, comments left out. A `#[cfg(test)]` module ends the file's code.
fn functions(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut test_cfg = false;
    for line in text.lines() {
        let code = line.trim_start();
        if code.starts_with("//") || code.is_empty() {
            continue;
        }
        if test_cfg && code.starts_with("mod ") {
            break;
        }
        test_cfg = code.starts_with("#[cfg(test)]");
        if let Some(name) = fn_name(code) {
            out.push((name, String::new()));
        }
        if let Some((_, body)) = out.last_mut() {
            body.push_str(code);
            body.push('\n');
        }
    }
    out
}

/// The name a line declares with `fn`, if it does.
fn fn_name(code: &str) -> Option<String> {
    let at = code.find("fn ")?;
    if at > 0 && !code[..at].ends_with(' ') {
        return None;
    }
    let head = &code[..at];
    let modifiers = head
        .split_whitespace()
        .all(|w| w == "pub" || w.starts_with("pub(") || ["const", "async", "unsafe"].contains(&w));
    if !modifiers {
        return None;
    }
    let name: String = code[at + 3..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}
