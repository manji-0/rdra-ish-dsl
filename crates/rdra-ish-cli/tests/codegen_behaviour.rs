//! The code `codegen` prints behaves as the model says, run in each language: on
//! `tests/fixtures/codegen/order.rdra`, a driver per language converts rows to states and
//! back, refuses a row that breaks its state's refinement, dispatches the use cases by state
//! (`NotEnabled`, `Rejected`, the writes, the raised event), and runs the rule checks. A
//! compiler or runtime missing from PATH skips its language, except under CI.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codegen")
}

fn available(tool: &str, arg: &str) -> bool {
    let found = Command::new(tool).arg(arg).output().is_ok();
    if !found {
        assert!(
            std::env::var_os("CI").is_none(),
            "{tool} is not on PATH; the dev shell provides it"
        );
        eprintln!("skipping: {tool} is not on PATH");
    }
    found
}

fn scratch(lang: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("rdra-ish-codegen-behaviour-{}", std::process::id()))
        .join(lang);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn generate(lang: &str, out: &Path) {
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .args(["codegen", "--lang", lang, "-o"])
        .arg(out)
        .arg(fixture().join("order.rdra"))
        .output()
        .expect("runs rdra-ish");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run(cmd: &mut Command, what: &str) -> String {
    let output = cmd.output().unwrap_or_else(|e| panic!("{what}: {e}"));
    assert!(
        output.status.success(),
        "{what}:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn rust_behaves() {
    let dir = scratch("rust");
    generate("rust", &dir.join("domain.rs"));
    std::fs::copy(fixture().join("driver.rs"), dir.join("driver.rs")).unwrap();
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    run(
        Command::new(rustc)
            .args(["--edition", "2021", "-D", "warnings", "-o"])
            .arg(dir.join("driver"))
            .arg(dir.join("driver.rs")),
        "rustc on the driver",
    );
    let out = run(&mut Command::new(dir.join("driver")), "the Rust driver");
    assert_eq!(out.trim(), "ok");
}

#[test]
fn typescript_behaves() {
    if !available("tsc", "--version") || !available("node", "--version") {
        return;
    }
    let dir = scratch("ts");
    generate("ts", &dir.join("domain.ts"));
    std::fs::copy(fixture().join("driver.ts"), dir.join("driver.ts")).unwrap();
    run(
        Command::new("tsc")
            .args([
                "--strict", "--target", "es2020", "--module", "commonjs", "--outDir",
            ])
            .arg(dir.join("out"))
            .arg(dir.join("driver.ts")),
        "tsc on the driver",
    );
    let out = run(
        Command::new("node").arg(dir.join("out/driver.js")),
        "the TypeScript driver",
    );
    assert_eq!(out.trim(), "ok");
}

#[test]
fn go_behaves() {
    if !available("go", "version") {
        return;
    }
    let dir = scratch("go");
    generate("go", &dir.join("domain.go"));
    std::fs::copy(fixture().join("driver_test.go"), dir.join("driver_test.go")).unwrap();
    std::fs::write(dir.join("go.mod"), "module example.com/domain\n\ngo 1.21\n").unwrap();
    run(
        Command::new("go").args(["test", "./..."]).current_dir(&dir),
        "go test on the driver",
    );
}
