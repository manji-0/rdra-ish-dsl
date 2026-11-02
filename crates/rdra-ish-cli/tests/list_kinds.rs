//! `list --kind screen|event|timer`: one id and label per node, sorted by id, in every format.
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).expect("repo root")
}

fn model_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("list_kinds");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("m.rdra"),
        r#"
screen OrderScreen "Order"
screen CartScreen "Cart"
event OrderPlaced "Order placed"
timer NightlyClose "Nightly close"
"#,
    )
    .unwrap();
    dir
}

fn list(kind: &str, format: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .arg("list")
        .arg(model_dir())
        .args(["--kind", kind, "--format", format])
        .output()
        .expect("rdra-ish runs")
}

#[test]
fn screen_event_and_timer_list_as_id_and_label() {
    for (kind, rows) in [
        ("screen", "CartScreen,Cart\nOrderScreen,Order\n"),
        ("event", "OrderPlaced,Order placed\n"),
        ("timer", "NightlyClose,Nightly close\n"),
    ] {
        let output = list(kind, "csv");
        assert!(output.status.success(), "{kind}: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("id,label\n{rows}"),
            "{kind}"
        );
    }
}

#[test]
fn the_other_formats_carry_the_same_rows() {
    let json = list("timer", "json");
    assert!(json.status.success(), "{json:?}");
    assert!(String::from_utf8_lossy(&json.stdout).contains("NightlyClose"));
    let table = list("event", "table");
    assert!(table.status.success(), "{table:?}");
    assert!(String::from_utf8_lossy(&table.stdout).contains("OrderPlaced"));
}
