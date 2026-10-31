//! `next` places each step of the incremental-order sample on its stage, and reads small models as the modeling guide says.

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
    ];
    for (step, stage, next_stage) in steps {
        let src = format!("samples/incremental-order/{step}/src");
        let out = json(&[&src]);
        let buc = &out["bucs"][0];
        assert_eq!(buc["id"], "BucStoreRestock", "{step}");
        assert_eq!(buc["stage"], stage, "{step}");
        assert_eq!(buc["required_complete"], false, "{step}");
        assert_eq!(buc["next"]["kind"], "required", "{step}");
        assert_eq!(buc["next"]["stage"], next_stage, "{step}");
    }
}

#[test]
fn a_buc_with_a_lifecycle_is_complete_and_the_rule_stages_are_optional() {
    let out = json(&["samples/incremental-order/step-5-lifecycle/src"]);
    let buc = &out["bucs"][0];
    assert_eq!(buc["stage"], "5");
    assert_eq!(buc["slug"], "lifecycle");
    assert_eq!(buc["required_complete"], true);
    assert_eq!(buc["next"]["kind"], "optional");
    let offered: Vec<&str> = buc["next"]["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|stage| stage["slug"].as_str().unwrap())
        .collect();
    assert_eq!(
        offered,
        ["local-guardrails", "local-obligations", "advanced-rules"]
    );

    let out = json(&["samples/incremental-order/step-6-business-rules/src"]);
    let buc = &out["bucs"][0];
    assert_eq!(buc["stage"], "5");
    let verdicts: Vec<(&str, &str)> = buc["gates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|gate| gate["stage"].as_str().unwrap().starts_with('6'))
        .map(|gate| {
            (
                gate["stage"].as_str().unwrap(),
                gate["verdict"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(verdicts, [("6A", "met"), ("6B", "met"), ("6C", "open")]);

    let output = next(&["samples/incremental-order/step-6-business-rules/src"]);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.starts_with(
            "BucStoreRestock \"Maintain Store Restock\": stage 5 Lifecycle, required stages complete\n"
        ),
        "{text}"
    );
    assert!(
        text.contains("\nOptional: stage 6C Advanced rules\n"),
        "{text}"
    );
    assert!(!text.contains("\nNext: stage"), "{text}");
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

#[test]
fn json_output_matches_the_snapshots() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/next_snapshots");
    let mut stale = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("snapshot dir") {
        let path = entry.unwrap().path();
        let step = path.file_stem().unwrap().to_str().unwrap().to_string();
        let src = format!("samples/incremental-order/{step}/src");
        let output = next(&[&src, "--format", "json"]);
        assert!(output.status.success(), "{output:?}");
        let actual = String::from_utf8(output.stdout).unwrap();
        let expected = std::fs::read_to_string(&path).unwrap();
        if actual != expected {
            if std::env::var_os("RDRA_ISH_BLESS").is_some() {
                std::fs::write(&path, actual).unwrap();
            } else {
                let first = actual
                    .lines()
                    .zip(expected.lines())
                    .find(|(a, e)| a != e)
                    .map(|(a, e)| format!("got {a:?}, want {e:?}"))
                    .unwrap_or_else(|| format!("bytes {} vs {}", actual.len(), expected.len()));
                stale.push(format!("{step}: {first}"));
            }
        }
    }
    assert!(
        stale.is_empty(),
        "stale snapshots {stale:?}; run with RDRA_ISH_BLESS=1 to rewrite"
    );
}

fn model(name: &str, src: &str) -> String {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("next_cli");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.rdra"));
    std::fs::write(&path, src).unwrap();
    path.to_str().unwrap().to_string()
}

const ORDER_SKELETON: &str = r#"
business Commerce "Commerce"
buc BucOrder "Process Order"
actor Customer "Customer"
usecase PlaceOrder "Place Order"
screen Checkout "Checkout"
belongs(BucOrder, Commerce)
performs(Customer, BucOrder)
contains(BucOrder, PlaceOrder)
displays(PlaceOrder, Checkout)
"#;

const PAID_ORDER: &str = r#"
business Commerce "Commerce"
buc BucOrder "Process Order"
actor Customer "Customer"
usecase PlaceOrder "Place Order"
usecase PayOrder "Pay Order"
screen Checkout "Checkout"
belongs(BucOrder, Commerce)
performs(Customer, BucOrder)
contains(BucOrder, PlaceOrder)
contains(BucOrder, PayOrder)
displays(PlaceOrder, Checkout)
displays(PayOrder, Checkout)
creates(PlaceOrder, Order)
updates(PayOrder, Order)
transitions(Order.status, PayOrder, pending -> paid)
"#;

fn gate<'a>(buc: &'a serde_json::Value, stage: &str) -> &'a serde_json::Value {
    buc["gates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|gate| gate["stage"] == stage)
        .unwrap()
}

#[test]
fn an_event_started_buc_passes_the_skeleton_and_is_asked_for_its_entry_use_case() {
    let src = model(
        "event_started",
        r#"
business Commerce "Commerce"
buc BucShip "Ship"
event OrderPaid "Order paid"
belongs(BucShip, Commerce)
triggers(OrderPaid, BucShip)
"#,
    );
    let buc = &json(&[&src])["bucs"][0];
    assert_eq!(buc["stage"], "1");
    assert_eq!(buc["next"]["stage"], "2");
    assert_eq!(
        buc["next"]["gaps"][0]["message"],
        "name the use case that OrderPaid starts: add a `usecase` and `contains(BucShip, UseCase)`"
    );
}

#[test]
fn a_use_case_without_crud_is_a_note_once_another_touches_data() {
    let src = model(
        "data_free",
        &format!(
            "{ORDER_SKELETON}
usecase BrowseCatalog \"Browse Catalog\"
contains(BucOrder, BrowseCatalog)
displays(BrowseCatalog, Checkout)
entity Order \"Order\" {{
  id: Int @pk
  total: Money
}}
creates(PlaceOrder, Order)
"
        ),
    );
    let buc = &json(&[&src])["bucs"][0];
    let data = gate(buc, "2");
    assert_eq!(data["verdict"], "met");
    assert_eq!(data["notes"], 1);
    assert_eq!(buc["stage"], "4");

    let output = next(&[&src]);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("  [x] 2  Data touchpoints      1/2 use cases touch an entity (1 note)\n"),
        "{text}"
    );
}

#[test]
fn a_buc_whose_use_cases_touch_no_data_is_open_at_stage_two() {
    let src = model(
        "no_data",
        &format!(
            "{ORDER_SKELETON}
entity Order \"Order\" {{
  id: Int @pk
}}
"
        ),
    );
    let buc = &json(&[&src])["bucs"][0];
    assert_eq!(buc["stage"], "1");
    assert_eq!(buc["next"]["stage"], "2");
    assert_eq!(buc["next"]["gaps"][0]["kind"], "buc");
}

#[test]
fn permission_and_medium_are_notes_only_once_the_model_declares_them() {
    let base = format!(
        "{ORDER_SKELETON}
entity Order \"Order\" {{
  id: Int @pk
  total: Money
}}
creates(PlaceOrder, Order)
"
    );
    let plain = model("access_plain", &base);
    let buc = &json(&[&plain])["bucs"][0];
    assert_eq!(gate(buc, "3")["notes"], 0);

    let declared = model(
        "access_declared",
        &format!("{base}permission OrderWrite \"Order Write\"\n"),
    );
    let buc = &json(&[&declared])["bucs"][0];
    assert_eq!(gate(buc, "3")["verdict"], "met");
    assert_eq!(gate(buc, "3")["notes"], 1);
    let next = &buc["next"];
    assert_eq!(next["stage"], "5");
    let output = next_text(&declared);
    assert!(
        output.contains("  [x] 3  Interaction boundary  1/1 use cases display a screen, invoke an API, or run on a trigger (1 note)\n"),
        "{output}"
    );
}

fn next_text(src: &str) -> String {
    let output = next(&[src]);
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn entity_structure_asks_for_columns_and_notes_a_missing_relate() {
    let src = model(
        "structure",
        &format!(
            "{ORDER_SKELETON}
entity Order \"Order\" {{
  id: Int @pk
}}
entity OrderLine \"Order Line\" {{
  id: Int @pk
  qty: Int
}}
creates(PlaceOrder, Order)
creates(PlaceOrder, OrderLine)
"
        ),
    );
    let buc = &json(&[&src])["bucs"][0];
    let next = &buc["next"];
    assert_eq!(next["stage"], "4");
    assert_eq!(
        next["gaps"][0]["message"],
        "add the columns that identify and describe Order"
    );
    let notes: Vec<&str> = next["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|note| note["id"].as_str().unwrap())
        .collect();
    assert_eq!(notes, ["Order", "OrderLine"]);

    let related = model(
        "structure_related",
        &format!(
            "{ORDER_SKELETON}
entity Order \"Order\" {{
  id: Int @pk
  total: Money
}}
entity OrderLine \"Order Line\" {{
  id: Int @pk
  qty: Int
}}
creates(PlaceOrder, Order)
creates(PlaceOrder, OrderLine)
relate(OrderLine, Order, N:1)
"
        ),
    );
    let buc = &json(&[&related])["bucs"][0];
    assert_eq!(gate(buc, "4")["verdict"], "met");
    assert_eq!(gate(buc, "4")["notes"], 0);
}

#[test]
fn a_sets_on_a_nullable_column_alone_does_not_pass_the_lifecycle() {
    let src = model(
        "nullable_only",
        &format!(
            "{ORDER_SKELETON}
entity Order \"Order\" {{
  id: Int @pk
  note: String @null
}}
creates(PlaceOrder, Order)
sets(PlaceOrder, Order, note == present)
"
        ),
    );
    let buc = &json(&[&src])["bucs"][0];
    assert_eq!(buc["stage"], "4");
    let next = &buc["next"];
    assert_eq!(next["stage"], "5");
    assert_eq!(
        next["notes"][0]["message"],
        "`sets` moves Order.note, which is not an Enum or Bool: which lifecycle state does it follow?"
    );
    let text = next_text(&src);
    assert!(
        text.contains("  [ ] 5  Lifecycle             no use case moves an Enum or Bool column; `sets` moves Order.note (1 note)\n"),
        "{text}"
    );
}

#[test]
fn a_rule_counts_for_a_buc_only_through_a_column_it_moves() {
    let src = model(
        "rule_scope",
        &format!(
            "{PAID_ORDER}
entity Order \"Order\" {{
  id: Int @pk
  status: Enum(pending, paid) @default(pending)
  flagged: Bool @default(false)
}}
forbidden(Order, flagged == true)
"
        ),
    );
    let buc = &json(&[&src])["bucs"][0];
    assert_eq!(buc["required_complete"], true);
    assert_eq!(gate(buc, "6A")["verdict"], "open");
    assert_eq!(
        gate(buc, "6A")["evidence"],
        "no `forbidden` / `exclusive` reads a column this BUC moves"
    );
}

#[test]
fn a_comparison_forbidden_is_an_advanced_rule_and_not_a_guardrail() {
    let src = model(
        "comparison",
        &format!(
            "{PAID_ORDER}
entity Order \"Order\" {{
  id: Int @pk
  status: Enum(pending, paid) @default(pending)
  qty: Int
  stock: Int
}}
sets(PlaceOrder, Order, qty > stock, false)
forbidden(Order, qty > stock)
"
        ),
    );
    let buc = &json(&[&src])["bucs"][0];
    assert_eq!(buc["required_complete"], true);
    assert_eq!(gate(buc, "6A")["verdict"], "open");
    assert_eq!(gate(buc, "6B")["verdict"], "open");
    assert_eq!(gate(buc, "6C")["verdict"], "met");
    assert_eq!(
        gate(buc, "6C")["evidence"],
        "comparison or cross-entity rule reads Order.qty, Order.stock"
    );
}

#[test]
fn a_lifecycle_check_rejects_blocks_the_stage() {
    let src = model(
        "witness",
        r#"
business Commerce "Commerce"
buc BucOrder "Order"
actor Customer "Customer"
usecase PlaceOrder "Place"
screen S "S"
entity Order "Order" {
  id: Int @pk
  status: Enum(pending, paid, refunded) @default(pending)
}
belongs(BucOrder, Commerce)
performs(Customer, BucOrder)
contains(BucOrder, PlaceOrder)
displays(PlaceOrder, S)
creates(PlaceOrder, Order)
transitions(Order.status, PlaceOrder, pending -> paid)
"#,
    );
    let buc = &json(&[&src])["bucs"][0];
    assert_eq!(buc["stage"], "4");
    assert_eq!(buc["required_complete"], false);
    assert_eq!(gate(buc, "5")["verdict"], "blocked");
    let next = &buc["next"];
    assert_eq!(next["stage"], "5");
    assert_eq!(next["verdict"], "blocked");
    assert_eq!(next["gaps"].as_array().unwrap().len(), 0);
    let findings: Vec<(&str, &str)> = next["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["severity"].as_str().unwrap(), f["name"].as_str().unwrap()))
        .collect();
    assert!(
        findings.contains(&("error", "Witness_Order_status_pending")),
        "{findings:?}"
    );

    let text = next_text(&src);
    assert!(
        text.contains("  [!] 5  Lifecycle             use cases move Order.status (1 error from check, 1 warning from check)\n"),
        "{text}"
    );
    assert!(
        text.contains(
            "\nNext: stage 5 Lifecycle is blocked: the elements exist, but check rejects them\n"
        ),
        "{text}"
    );
    assert!(
        text.contains("    - error[Witness_Order_status_pending]\n"),
        "{text}"
    );
}

#[test]
fn lint_calls_buc_empty_info_before_the_skeleton_stage() {
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args([
            "lint",
            "samples/incremental-order/step-0-scope/src",
            "--format",
            "json",
        ])
        .output()
        .expect("rdra-ish runs");
    assert!(output.status.success(), "{output:?}");
    let issues: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let buc_empty = issues
        .as_array()
        .unwrap()
        .iter()
        .find(|issue| issue["rule"] == "buc-empty")
        .unwrap();
    assert_eq!(buc_empty["severity"], "info");
    assert!(
        buc_empty["hint"]
            .as_str()
            .unwrap()
            .starts_with("expected before stage 1 BUC skeleton; "),
        "{buc_empty}"
    );
}
