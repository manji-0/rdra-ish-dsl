//! A removed form end to end: the model fails to load with the rewrite of each line, and
//! `scripts/migrate-syntax-v3.py` turns it into the model that derives the same states as the
//! one written in the current forms.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const DECLS: &str = "\
entity Stock \"Stock\" {
  id: Int @pk
  status: Enum(listed, sold) @default(listed)
  stock: Int
  selling: Int
  sold_at: DateTime @null
}
usecase List \"List\"
usecase Sell \"Sell\"
usecase Refund \"Refund\"
creates(List, Stock)
transitions(Sell, Stock.status, listed -> sold)
transitions(Refund, Stock.status, sold -> listed)
invariant(Stock).when(status == sold).then(stock < selling)
";

const OLD: &str = "\
sets(Sell, Stock, sold_at == present)
drives(Sell, Stock, stock < selling, true)
sets(Refund, Stock, stock < selling, false)
";

const NEW: &str = "\
sets(Sell, Stock.sold_at == present)
drives(Sell, Stock.stock < selling, true)
drives(Refund, Stock.stock < selling, false)
";

/// `body` after [`DECLS`] in a directory of its own.
fn model(name: &str, body: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("removed_cli_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("model.rdra");
    std::fs::write(&path, format!("{DECLS}{body}")).unwrap();
    path
}

fn run(args: &[&str], path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(path.parent().unwrap())
        .args(args)
        .arg("model.rdra")
        .output()
        .expect("rdra-ish runs")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn check_fails_with_the_rewrite_of_each_removed_line() {
    let path = model("check", OLD);
    let check = run(&["check"], &path);
    let err = stderr(&check);
    assert_eq!(check.status.code(), Some(1), "{err}");
    for rewrite in NEW.lines() {
        assert!(
            err.contains(&format!("write `{rewrite}`")),
            "{rewrite}\n{err}"
        );
    }
}

#[test]
fn the_migration_script_writes_the_current_forms() {
    let path = model("migrate", OLD);
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/migrate-syntax-v3.py");
    let migrate = Command::new("python3")
        .arg("-I")
        .arg(&script)
        .arg(&path)
        .output()
        .expect("python3 runs");
    assert!(migrate.status.success(), "{}", stderr(&migrate));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!("{DECLS}{NEW}")
    );

    let check = run(&["check"], &path);
    assert!(check.status.success(), "{}", stderr(&check));
    let migrated = run(&["states"], &path);
    let written = run(&["states"], &model("written", NEW));
    assert!(written.status.success(), "{}", stderr(&written));
    assert_eq!(migrated.stdout, written.stdout);
    assert!(!migrated.stdout.is_empty());
}
