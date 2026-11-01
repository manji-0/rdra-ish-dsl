//! A deprecated form end to end: the four-argument `sets` loads as `drives` with a warning,
//! `--deny-deprecated` fails on it, `lint` lists it, and `fmt --write` migrates it to a file
//! that loads without a warning and derives the same states.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MODEL: &str = "\
entity Stock \"Stock\" {
  id: Int @pk
  status: Enum(listed, sold) @default(listed)
  stock: Int
  selling: Int
}
usecase List \"List\"
usecase Sell \"Sell\"
usecase Refund \"Refund\"
creates(List, Stock)
transitions(Sell, Stock.status, listed -> sold)
transitions(Refund, Stock.status, sold -> listed)
// sold out
sets(Sell, Stock, stock < selling, true)
sets(Refund, Stock, stock < selling, false)
invariant(Stock).when(status == sold).then(stock < selling)
";

/// A fresh copy of [`MODEL`] in its own directory.
fn model(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("deprecated_cli_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("model.rdra");
    std::fs::write(&path, MODEL).unwrap();
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

fn warnings(text: &str) -> Vec<&str> {
    text.lines().filter(|l| l.starts_with("warning:")).collect()
}

#[test]
fn check_warns_with_the_rewrite_and_passes() {
    let path = model("check");
    let check = run(&["check"], &path);
    let dir = dunce::canonicalize(path.parent().unwrap()).unwrap();
    let err = stderr(&check).replace(
        &format!("{}{}", dir.display(), std::path::MAIN_SEPARATOR),
        "",
    );
    assert!(check.status.success(), "{err}");
    assert_eq!(
        warnings(&err),
        vec![
            "warning: model.rdra:14:1: deprecated syntax in 'model.rdra': `sets` with a true/false \
             argument drives a comparison, which is now `drives`; write \
             `drives(Sell, Stock, stock < selling, true)` (deprecated since 0.5.0, removed in \
             2.0.0; `rdra-ish fmt --write` rewrites it)",
            "warning: model.rdra:15:1: deprecated syntax in 'model.rdra': `sets` with a true/false \
             argument drives a comparison, which is now `drives`; write \
             `drives(Refund, Stock, stock < selling, false)` (deprecated since 0.5.0, removed in \
             2.0.0; `rdra-ish fmt --write` rewrites it)",
        ],
        "{err}"
    );
}

#[test]
fn deny_deprecated_fails_on_four_argument_sets() {
    let path = model("deny");
    let check = run(&["check", "--deny-deprecated"], &path);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
}

#[test]
fn lint_lists_four_argument_sets_as_deprecated_syntax() {
    let path = model("lint");
    let lint = run(&["lint"], &path);
    let out = format!("{}{}", String::from_utf8_lossy(&lint.stdout), stderr(&lint));
    let lines: Vec<_> = out
        .lines()
        .filter(|l| l.contains("deprecated-syntax"))
        .collect();
    assert_eq!(lines.len(), 2, "{out}");
}

#[test]
fn fmt_write_migrates_to_drives_with_the_same_states() {
    let path = model("fmt");
    let before = run(&["states"], &path);
    assert!(before.status.success(), "{}", stderr(&before));

    let fmt = run(&["fmt", "--write"], &path);
    assert!(fmt.status.success(), "{}", stderr(&fmt));
    let migrated = std::fs::read_to_string(&path).unwrap();
    assert!(
        migrated.contains(
            "// sold out\ndrives(Sell, Stock, stock < selling, true)\n\
             drives(Refund, Stock, stock < selling, false)\n"
        ),
        "{migrated}"
    );
    assert!(!migrated.contains("sets("), "{migrated}");

    let check = run(&["check", "--deny-deprecated"], &path);
    let err = stderr(&check);
    assert!(check.status.success(), "{err}");
    assert!(warnings(&err).is_empty(), "{err}");

    let after = run(&["states"], &path);
    assert!(after.status.success(), "{}", stderr(&after));
    assert_eq!(
        String::from_utf8_lossy(&after.stdout),
        String::from_utf8_lossy(&before.stdout)
    );
    assert!(!String::from_utf8_lossy(&after.stdout).is_empty());
}
