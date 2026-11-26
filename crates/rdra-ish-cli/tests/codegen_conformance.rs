//! The conformance harness `codegen --conformance` prints tells a service that keeps to the
//! model from one that does not. On `tests/fixtures/codegen/conformance.rdra`, an in-memory
//! service per language (Rust, Go, TypeScript) built on the generated core passes `conform` /
//! `Conform` along the traces, and each planted fault fails it at the call that shows it:
//!
//! - `skip-core`: Pay writes the tag itself, so `paid_at` and `qty` are not what the model
//!   holds after Pay.
//! - `drop-row`: Ship stores the order but not the shipment the core returns.
//! - `accept-disabled`: Cancel cancels a shipped order, a call the model refuses.
//! - `unconditional`: every commit writes without checking what its rows still are, so a
//!   second Place prepared before the first commits overwrites the order (#332).
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
        .join(format!(
            "rdra-ish-codegen-conformance-{}",
            std::process::id()
        ))
        .join(lang);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The code in `lang` and its traces, written into `dir`.
fn generate(lang: &str, dir: &Path, file: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .args(["codegen", "--lang", lang, "-o"])
        .arg(dir.join(file))
        .arg("--conformance")
        .arg(dir.join("traces.txt"))
        .arg(fixture().join("conformance.rdra"))
        .output()
        .expect("runs rdra-ish");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Each fault, and what the harness reports for it.
const FAULTS: [(&str, &str); 4] = [
    (
        "skip-core",
        "after `Pay 1 2026-01-01T00:00:00Z`, the store differs from the model:\n  model only: Order 1 status=paid paid_at=+ qty=2 customer_id=1",
    ),
    ("drop-row", "model only: Shipment 1 status=packed order_id=1"),
    (
        "accept-disabled",
        "the model refuses `Cancel 1` here, and the service ran it",
    ),
    (
        "unconditional",
        "the service committed `Place 1 1 card s`, prepared before `Place 1 1 card s` changed a row it writes",
    ),
];

fn check(lang: &str, run: impl Fn(&str) -> Output) {
    let passed = run("");
    let out = text(&passed);
    assert!(
        passed.status.success(),
        "{lang}: the reference service fails:\n{out}"
    );
    assert!(
        out.contains("ok 9 traces, 19 calls, 9 refusals, 7 interleavings"),
        "{lang}: the reference service checked less than expected:\n{out}"
    );
    for (fault, report) in FAULTS {
        let failed = run(fault);
        let out = text(&failed);
        assert!(!failed.status.success(), "{lang}: {fault} passes:\n{out}");
        // `go test` indents the lines of a message, so each line is looked for on its own.
        assert!(
            report.lines().all(|line| out.contains(line.trim())),
            "{lang}: {fault} fails for another reason:\n{out}"
        );
    }
}

#[test]
fn rust_harness_tells_faults_apart() {
    let dir = scratch("rust");
    generate("rust", &dir, "domain.rs");
    std::fs::copy(
        fixture().join("conformance_service.rs"),
        dir.join("service.rs"),
    )
    .unwrap();
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let exe = dir.join(format!("service{}", std::env::consts::EXE_SUFFIX));
    let built = Command::new(rustc)
        .args(["--edition", "2021", "-D", "warnings", "-o"])
        .arg(&exe)
        .arg(dir.join("service.rs"))
        .output()
        .expect("runs rustc");
    assert!(built.status.success(), "rustc:\n{}", text(&built));
    check("rust", |fault| {
        Command::new(&exe)
            .arg(dir.join("traces.txt"))
            .env("FAULT", fault)
            .output()
            .expect("runs the service")
    });
}

#[test]
fn go_harness_tells_faults_apart() {
    if !tool("go", CODEGEN) {
        return;
    }
    let dir = scratch("go");
    generate("go", &dir, "domain.go");
    std::fs::copy(
        fixture().join("conformance_service_test.go"),
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
fn typescript_harness_tells_faults_apart() {
    if !tools(&["tsc", "node"], CODEGEN) {
        return;
    }
    let dir = scratch("ts");
    generate("ts", &dir, "domain.ts");
    std::fs::copy(
        fixture().join("conformance_service.ts"),
        dir.join("service.ts"),
    )
    .unwrap();
    let built = Command::new("tsc")
        .args([
            "--strict", "--target", "es2020", "--module", "commonjs", "--outDir",
        ])
        .arg(dir.join("out"))
        .arg(dir.join("service.ts"))
        .output()
        .expect("runs tsc");
    assert!(built.status.success(), "tsc:\n{}", text(&built));
    check("ts", |fault| {
        Command::new("node")
            .arg(dir.join("out/service.js"))
            .arg(dir.join("traces.txt"))
            .env("FAULT", fault)
            .output()
            .expect("runs the service")
    });
}
