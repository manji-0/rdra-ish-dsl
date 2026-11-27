//! The commit part of the codegen output (docs/design/algebraic-codegen.md, "Committing a
//! result") lets a service commit every result with one generic call. On
//! `tests/fixtures/codegen/commit.rdra` (a use case split in two steps linked by a run mark, an
//! event a triggered use case runs on, a use case with two cases, a delete), a service per
//! language implements the generated `CommitStore` once, commits every result through
//! `commit_writes` / `CommitWrites` / `commitWrites`, and passes the conformance harness. With
//! `FAULT=keep-mark` its store keeps the run mark the last step ends, and the harness fails at
//! that step: the result carries the mark to delete.
//!
//! A compiler missing from PATH skips its language, except under
//! `RDRA_REQUIRE_CODEGEN_TOOLS=1`, which CI sets in the dev shell.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "common/output.rs"]
mod output;
use output::text;

#[path = "common/tools.rs"]
mod tools;
use tools::{tool, tools, CODEGEN};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codegen")
}

fn scratch(lang: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("rdra-ish-codegen-commit-{}", std::process::id()))
        .join(lang);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The code of `model` in `lang`, written to `dir/file`, with its traces when `traces`.
fn generate(model: &str, lang: &str, dir: &Path, file: &str, traces: bool) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rdra-ish"));
    cmd.args(["codegen", "--lang", lang, "-o"])
        .arg(dir.join(file));
    if traces {
        cmd.arg("--conformance").arg(dir.join("traces.txt"));
    }
    let output = cmd
        .arg(fixture().join(model))
        .output()
        .expect("runs rdra-ish");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn check(lang: &str, run: impl Fn(&str) -> Output) {
    let passed = run("");
    let out = text(&passed);
    assert!(
        passed.status.success() && out.contains("ok "),
        "{lang}: the service that commits through the helper fails:\n{out}"
    );
    let failed = run("keep-mark");
    let out = text(&failed);
    assert!(!failed.status.success(), "{lang}: keep-mark passes:\n{out}");
    assert!(
        out.contains("after `Ship_2 1 1`") && out.contains("store only: mark Ship Order 1 1"),
        "{lang}: keep-mark fails for another reason:\n{out}"
    );
}

fn run_ok(cmd: &mut Command, what: &str) {
    let out = cmd.output().expect(what);
    assert!(out.status.success(), "{what}:\n{}", text(&out));
}

#[test]
fn rust_service_commits_every_result_through_the_helper() {
    let dir = scratch("rust");
    generate("commit.rdra", "rust", &dir, "domain.rs", true);
    std::fs::copy(fixture().join("commit_service.rs"), dir.join("service.rs")).unwrap();
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let exe = dir.join(format!("service{}", std::env::consts::EXE_SUFFIX));
    run_ok(
        Command::new(rustc)
            .args(["--edition", "2021", "-D", "warnings", "-o"])
            .arg(&exe)
            .arg(dir.join("service.rs")),
        "rustc",
    );
    check("rust", |fault| {
        Command::new(&exe)
            .arg(dir.join("traces.txt"))
            .env("FAULT", fault)
            .output()
            .expect("runs the service")
    });
}

#[test]
fn go_service_commits_every_result_through_the_helper() {
    if !tool("go", CODEGEN) {
        return;
    }
    let dir = scratch("go");
    generate("commit.rdra", "go", &dir, "domain.go", true);
    std::fs::copy(
        fixture().join("commit_service_test.go"),
        dir.join("service_test.go"),
    )
    .unwrap();
    std::fs::write(dir.join("go.mod"), "module example.com/domain\n\ngo 1.21\n").unwrap();
    check("go", |fault| {
        Command::new("go")
            .args(["test", "-count=1", "-v", "./..."])
            .current_dir(&dir)
            .env("FAULT", fault)
            .env("TRACES", dir.join("traces.txt"))
            .output()
            .expect("runs go test")
    });
}

#[test]
fn typescript_service_commits_every_result_through_the_helper() {
    if !tools(&["tsc", "node"], CODEGEN) {
        return;
    }
    let dir = scratch("ts");
    generate("commit.rdra", "ts", &dir, "domain.ts", true);
    std::fs::copy(fixture().join("commit_service.ts"), dir.join("service.ts")).unwrap();
    run_ok(
        Command::new("tsc")
            .args([
                "--strict", "--target", "es2020", "--module", "commonjs", "--outDir",
            ])
            .arg(dir.join("out"))
            .arg(dir.join("service.ts")),
        "tsc",
    );
    check("ts", |fault| {
        Command::new("node")
            .arg(dir.join("out/service.js"))
            .arg(dir.join("traces.txt"))
            .env("FAULT", fault)
            .output()
            .expect("runs the service")
    });
}

/// A model whose types are named like the commit part's (`Writes`, `RowWrite`) keeps them:
/// the commit part takes a `_`, and in Go, where a result's field `Writes` and a method of that
/// name cannot both be, the method does too. The output still builds.
#[test]
fn commit_names_give_way_to_the_model() {
    let dir = scratch("names");
    generate("commit_names.rdra", "rust", &dir, "domain.rs", false);
    let rust = std::fs::read_to_string(dir.join("domain.rs")).unwrap();
    assert!(rust.contains("pub enum RowWrite_ {"), "{rust}");
    assert!(rust.contains("pub struct Writes_ {"), "{rust}");
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    run_ok(
        Command::new(rustc)
            .args([
                "--edition",
                "2021",
                "--crate-type",
                "lib",
                "-D",
                "warnings",
                "-o",
            ])
            .arg(dir.join("libdomain.rlib"))
            .arg(dir.join("domain.rs")),
        "rustc",
    );
    if tool("go", CODEGEN) {
        let go = dir.join("go");
        std::fs::create_dir_all(&go).unwrap();
        generate("commit_names.rdra", "go", &go, "domain.go", false);
        let code = std::fs::read_to_string(go.join("domain.go")).unwrap();
        assert!(code.contains(") Writes_() Writes_ {"), "{code}");
        std::fs::write(go.join("go.mod"), "module example.com/domain\n\ngo 1.21\n").unwrap();
        run_ok(
            Command::new("go")
                .args(["vet", "./..."])
                .current_dir(&go)
                .env("GOFLAGS", "-mod=mod"),
            "go vet",
        );
    }
    if tool("tsc", CODEGEN) {
        generate("commit_names.rdra", "ts", &dir, "domain.ts", false);
        run_ok(
            Command::new("tsc")
                .args([
                    "--strict",
                    "--noEmit",
                    "--noUnusedLocals",
                    "--noUnusedParameters",
                ])
                .arg(dir.join("domain.ts")),
            "tsc",
        );
    }
}
