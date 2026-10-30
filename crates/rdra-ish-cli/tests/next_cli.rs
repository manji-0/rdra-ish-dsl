//! `next` places each step of the incremental-order sample on the stage it is named after.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo() -> PathBuf {
    dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).expect("repo root")
}

fn next(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .arg("next")
        .args(args)
        .output()
        .expect("rdra-ish runs")
}

fn json(args: &[&str]) -> serde_json::Value {
    let output = next(&[args, &["--format", "json"]].concat());
    assert!(output.status.success(), "{output:?}");
    serde_json::from_slice(&output.stdout).expect("json output")
}

#[test]
fn each_incremental_order_step_is_at_its_own_stage() {
    let steps = [
        ("step-0-scope", "0", "1"),
        ("step-1-buc-skeleton", "1", "2"),
        ("step-2-data-touchpoints", "2", "3"),
        ("step-3-interaction-boundary", "3", "4"),
        ("step-4-entity-structure", "4", "5"),
        ("step-5-lifecycle", "5", "6A"),
        // Step 6 adds a `forbidden` and an `invariant`: guardrails and obligations.
        ("step-6-business-rules", "6B", "6C"),
    ];
    for (step, stage, next_stage) in steps {
        let src = format!("samples/incremental-order/{step}/src");
        let out = json(&[&src]);
        let buc = &out["bucs"][0];
        assert_eq!(buc["id"], "BucStoreRestock", "{step}");
        assert_eq!(buc["stage"], stage, "{step}");
        assert_eq!(buc["next"]["stage"], next_stage, "{step}");
    }
}

#[test]
fn text_output_names_the_next_stage_its_gaps_and_review_commands() {
    let output = next(&["samples/incremental-order/step-2-data-touchpoints/src"]);
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.starts_with("BucStoreRestock \"Maintain Store Restock\": stage 2 Data touchpoints\n"),
        "{text}"
    );
    assert!(
        text.contains("Next: stage 3 Interaction boundary\n"),
        "{text}"
    );
    assert!(
        text.contains(
            "    - usecase ChangeNextRestockDate: add `displays(ChangeNextRestockDate, Screen)` or `invokes(ChangeNextRestockDate, Api)`"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "    rdra-ish csv samples/incremental-order/step-2-data-touchpoints/src --kind screen-constraints\n"
        ),
        "{text}"
    );
    assert!(
        text.contains("  See docs/incremental-modeling.md#stage-3-interaction-boundary\n"),
        "{text}"
    );
}

#[test]
fn buc_filter_keeps_the_named_bucs_and_rejects_unknown_ids() {
    let out = json(&["samples/ec-site", "--buc", "BucOrder"]);
    let bucs = out["bucs"].as_array().unwrap();
    assert_eq!(bucs.len(), 1);
    assert_eq!(bucs[0]["id"], "BucOrder");

    let output = next(&["samples/ec-site", "--buc", "BucNope"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown BUC `BucNope`"));
}
