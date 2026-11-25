//! `codegen --check <FILE>` generates again with the same options and compares with FILE:
//! exit 0 when FILE is what the model generates, the fingerprint line aside, and exit 1 with
//! a summary that names the cause otherwise (file edited, model changed, generator changed,
//! no fingerprint).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codegen/order.rdra")
}

/// A directory of its own holding a copy of the fixture model.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("rdra-ish-codegen-check-{}", std::process::id()))
        .join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(fixture(), dir.join("order.rdra")).unwrap();
    dir
}

fn codegen(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(dir)
        .args(["codegen", "order.rdra"])
        .args(args)
        .output()
        .expect("runs rdra-ish")
}

fn generate(dir: &Path, lang: &str, file: &str) {
    let out = codegen(dir, &["--lang", lang, "-o", file]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The exit code and stderr of `--check`; stdout stays empty.
fn check(dir: &Path, args: &[&str]) -> (Option<i32>, String) {
    let out = codegen(dir, args);
    assert!(out.stdout.is_empty(), "--check prints nothing on stdout");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn edit(path: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains(from), "{} holds {from:?}", path.display());
    std::fs::write(path, text.replacen(from, to, 1)).unwrap();
}

#[test]
fn the_file_the_model_generates_passes() {
    let dir = scratch("same");
    generate(&dir, "rust", "domain.rs");
    let before = std::fs::read_to_string(dir.join("domain.rs")).unwrap();
    assert!(
        before
            .lines()
            .nth(1)
            .is_some_and(|l| l.starts_with("// rdra-ish codegen fingerprint: model=")),
        "the fingerprint is the second line:\n{before}"
    );
    let (code, stderr) = check(&dir, &["--lang", "rust", "--check", "domain.rs"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stderr.contains("ok: domain.rs"), "{stderr}");
    assert_eq!(
        std::fs::read_to_string(dir.join("domain.rs")).unwrap(),
        before,
        "--check writes nothing"
    );
}

#[test]
fn an_edited_file_fails_as_edited() {
    let dir = scratch("edited");
    generate(&dir, "go", "domain.go");
    edit(
        &dir.join("domain.go"),
        "type Rejected struct",
        "type Refusal struct",
    );
    let (code, stderr) = check(&dir, &["--lang", "go", "--check", "domain.go"]);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("file edited"), "{stderr}");
    assert!(!stderr.contains("model changed"), "{stderr}");
    assert!(
        stderr.contains("1 line(s) only in domain.go, 1 only in the generated code"),
        "{stderr}"
    );
    assert!(stderr.contains("type Refusal struct"), "{stderr}");
}

#[test]
fn a_changed_model_fails_as_changed() {
    let dir = scratch("model");
    generate(&dir, "rust", "domain.rs");
    edit(&dir.join("order.rdra"), "qty > 4", "qty > 3");
    let (code, stderr) = check(&dir, &["--lang", "rust", "--check", "domain.rs"]);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("model changed"), "{stderr}");
    assert!(!stderr.contains("file edited"), "{stderr}");
}

#[test]
fn a_model_edit_that_changes_no_code_passes() {
    let dir = scratch("comment");
    generate(&dir, "ts", "domain.ts");
    let model = dir.join("order.rdra");
    let text = std::fs::read_to_string(&model).unwrap();
    std::fs::write(&model, format!("{text}\n// a comment changes no code\n")).unwrap();
    let (code, stderr) = check(&dir, &["--lang", "ts", "--check", "domain.ts"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stderr.contains("the code it generates did not"), "{stderr}");
}

#[test]
fn other_options_fail_as_a_generator_change() {
    let dir = scratch("options");
    generate(&dir, "go", "domain.go");
    let (code, stderr) = check(
        &dir,
        &["--lang", "go", "--package", "store", "--check", "domain.go"],
    );
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("generator changed"), "{stderr}");
}

#[test]
fn a_file_without_a_fingerprint_fails() {
    let dir = scratch("unstamped");
    generate(&dir, "rust", "domain.rs");
    let path = dir.join("domain.rs");
    let text = std::fs::read_to_string(&path).unwrap();
    let unstamped: String = text
        .lines()
        .filter(|l| !l.starts_with("// rdra-ish codegen fingerprint:"))
        .map(|l| format!("{l}\n"))
        .collect();
    std::fs::write(&path, format!("{unstamped}// added\n")).unwrap();
    let (code, stderr) = check(&dir, &["--lang", "rust", "--check", "domain.rs"]);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("no fingerprint line"), "{stderr}");
}

#[test]
fn check_and_out_are_exclusive() {
    let dir = scratch("exclusive");
    let out = codegen(
        &dir,
        &["--lang", "rust", "-o", "a.rs", "--check", "domain.rs"],
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(!dir.join("a.rs").exists());
}
