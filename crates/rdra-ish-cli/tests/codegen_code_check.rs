//! `check --code[=fresh]` and `codegen --code` (docs/design/code-check.md, P1): `[code.<lang>]`
//! in rdra.toml names the generated files, `codegen --code` writes them, and `check --code`
//! generates them again and compares: the code by its text, the traces by hash. Each cause of a
//! difference (file edited, model changed, generator changed) is named, as `codegen --check`
//! names it. `check` without `--code` behaves as before, with one note on stderr.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codegen/conformance.rdra")
}

const MANIFEST: &str = r#"[model]
name = "shop"

[code.rust]
domain = "../svc/src/domain.rs"
traces = "../svc/tests/traces.txt"
conformance_states = 4096
"#;

/// A project of its own: `model/` with the fixture and `manifest`, `svc/` for the code.
fn project(name: &str, manifest: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("rdra-ish-code-check-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("model")).unwrap();
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::copy(fixture(), dir.join("model/order.rdra")).unwrap();
    std::fs::write(dir.join("model/rdra.toml"), manifest).unwrap();
    dir
}

fn rdra(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("runs rdra-ish")
}

fn text(out: &Output) -> (Option<i32>, String, String) {
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn generated(name: &str) -> PathBuf {
    let dir = project(name, MANIFEST);
    let (code, _, stderr) = text(&rdra(&dir, &["codegen", "model", "--code"]));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stderr.contains("wrote svc/src/domain.rs"), "{stderr}");
    assert!(stderr.contains("wrote svc/tests/traces.txt"), "{stderr}");
    dir
}

fn edit(path: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains(from), "{} holds {from:?}", path.display());
    std::fs::write(path, text.replacen(from, to, 1)).unwrap();
}

#[test]
fn the_files_codegen_writes_are_fresh_wherever_check_runs() {
    let dir = generated("same");
    let traces = std::fs::read_to_string(dir.join("svc/tests/traces.txt")).unwrap();
    let mut lines = traces.lines();
    assert_eq!(lines.next(), Some("# rdra-ish conformance traces 3"));
    assert!(
        lines
            .next()
            .is_some_and(|l| l.starts_with("# rdra-ish codegen fingerprint: model=")),
        "{traces}"
    );
    for (cwd, input) in [(dir.clone(), "model"), (dir.join("model"), ".")] {
        let (code, stdout, stderr) = text(&rdra(&cwd, &["check", input, "--code"]));
        assert_eq!(code, Some(0), "{stderr}");
        assert!(
            stdout.ends_with("OK: code: what the model requires, at --code=fresh\n"),
            "{stdout}"
        );
        assert!(
            stderr.contains("domain.rs is what the model generates"),
            "{stderr}"
        );
        assert!(
            stderr.contains("traces.txt holds the traces the model generates"),
            "{stderr}"
        );
    }
}

#[test]
fn an_edited_domain_file_fails_as_edited() {
    let dir = generated("edited");
    edit(
        &dir.join("svc/src/domain.rs"),
        "pub struct Rejected",
        "pub struct Refusal",
    );
    let (code, stdout, stderr) = text(&rdra(&dir, &["check", "model", "--code=fresh"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("file edited"), "{stderr}");
    assert!(!stderr.contains("model changed"), "{stderr}");
    assert!(stderr.contains("rdra-ish codegen model --code"), "{stderr}");
    assert!(
        stdout.contains("FAIL: code: 1 error(s) at --code=fresh"),
        "{stdout}"
    );
}

#[test]
fn edited_traces_fail_as_edited_by_their_hash() {
    let dir = generated("traces");
    let path = dir.join("svc/tests/traces.txt");
    let text_before = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, format!("{text_before}T 0\n")).unwrap();
    let (code, _, stderr) = text(&rdra(&dir, &["check", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("traces.txt is not the traces `codegen` generates"),
        "{stderr}"
    );
    assert!(stderr.contains("file edited"), "{stderr}");
    assert!(
        stderr.contains("domain.rs is what the model generates"),
        "{stderr}"
    );
}

#[test]
fn a_changed_model_fails_as_changed() {
    let dir = generated("model");
    edit(&dir.join("model/order.rdra"), "qty > 4", "qty > 3");
    let (code, _, stderr) = text(&rdra(&dir, &["check", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("model changed"), "{stderr}");
    assert!(!stderr.contains("file edited"), "{stderr}");
}

#[test]
fn other_options_in_rdra_toml_fail_as_a_generator_change() {
    let dir = generated("options");
    std::fs::write(
        dir.join("model/rdra.toml"),
        MANIFEST.replace("conformance_states = 4096", "conformance_states = 3"),
    )
    .unwrap();
    let (code, _, stderr) = text(&rdra(&dir, &["check", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("generator changed"), "{stderr}");
}

#[test]
fn a_missing_file_fails_and_says_what_writes_it() {
    let dir = project("missing", MANIFEST);
    let (code, _, stderr) = text(&rdra(&dir, &["check", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("svc/src/domain.rs does not exist: `codegen --code` writes it"),
        "{stderr}"
    );
}

#[test]
fn code_without_a_code_table_is_an_error() {
    let dir = project("no-table", "[model]\nname = \"shop\"\n");
    let (code, stdout, stderr) = text(&rdra(&dir, &["check", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("--code needs a [code.rust], [code.go] or [code.ts] table"),
        "{stderr}"
    );
    assert!(!stdout.contains("OK"), "{stdout}");
    let (code, _, stderr) = text(&rdra(&dir, &["codegen", "model", "--code"]));
    assert_eq!(code, Some(1), "{stderr}");
}

#[test]
fn check_without_code_prints_a_note_and_nothing_else_changes() {
    let with = project("note", MANIFEST);
    let without = project("plain", "[model]\nname = \"shop\"\n");
    let (code_with, stdout_with, stderr_with) = text(&rdra(&with, &["check", "model"]));
    let (code_without, stdout_without, stderr_without) = text(&rdra(&without, &["check", "model"]));
    assert_eq!(code_with, code_without);
    assert_eq!(stdout_with, stdout_without);
    let note: Vec<&str> = stderr_with
        .lines()
        .filter(|l| !stderr_without.lines().any(|w| w == *l))
        .collect();
    assert_eq!(
        note,
        ["note: model/rdra.toml has [code]; `check --code` also checks the service code against the model"],
        "{stderr_with}"
    );
}

#[test]
fn an_unknown_key_under_code_is_an_error() {
    let dir = project(
        "unknown",
        &format!("{MANIFEST}servise = \"shop::Service\"\n"),
    );
    let (code, _, stderr) = text(&rdra(&dir, &["check", "model"]));
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("rdra.toml"), "{stderr}");
    assert!(stderr.contains("servise"), "{stderr}");
}

/// The level is one token with `=`: `--code model` reads `model` as an input, not a level.
#[test]
fn a_bare_code_flag_takes_no_level_from_the_next_argument() {
    let dir = generated("bare");
    let (code, stdout, stderr) = text(&rdra(&dir, &["check", "--code", "model"]));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("at --code=fresh"), "{stdout}");
}

/// `codegen --format json` prints the contract as data, without `--lang`.
#[test]
fn the_json_contract_names_obligations_and_sites() {
    let dir = project("json", MANIFEST);
    let (code, stdout, stderr) = text(&rdra(&dir, &["codegen", "model", "--format", "json"]));
    assert_eq!(code, Some(0), "{stderr}");
    for want in [
        "\"format\": \"rdra-ish codegen json 1\"",
        "\"name\": \"Unique_Shipment_Order\"",
        "\"name\": \"Ref_Shipment_order_id\"",
        "\"site\": \"model/order.rdra:",
        "\"kind\": \"functions\"",
    ] {
        assert!(stdout.contains(want), "{want}:\n{stdout}");
    }
    let (code, _, stderr) = text(&rdra(
        &dir,
        &["codegen", "model", "--format", "json", "--check", "x.json"],
    ));
    assert_eq!(code, Some(1), "{stderr}");
}
