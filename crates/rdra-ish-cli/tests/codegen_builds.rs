//! `rdra-ish codegen` output builds as printed: on every `samples.txt` root, the TypeScript
//! output passes `tsc --strict`, the Rust output compiles as a library, and the Go output
//! passes `go vet`. A compiler missing from PATH skips its language, except under CI, where
//! the dev shell provides all three and a missing one fails.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn samples() -> Vec<String> {
    std::fs::read_to_string(repo().join("samples.txt"))
        .expect("samples.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn available(tool: &str) -> bool {
    let found = Command::new(tool).arg("--version").output().is_ok()
        || Command::new(tool).arg("version").output().is_ok();
    if !found {
        assert!(
            std::env::var_os("CI").is_none(),
            "{tool} is not on PATH; the dev shell provides it"
        );
        eprintln!("skipping: {tool} is not on PATH");
    }
    found
}

/// The codegen output of `sample` in `lang`, written to `dir/file`.
fn generate(sample: &str, lang: &str, dir: &Path, file: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(file);
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(["codegen", sample, "--lang", lang, "-o"])
        .arg(&path)
        .output()
        .expect("runs rdra-ish");
    assert!(
        output.status.success(),
        "{sample} --lang {lang}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    path
}

fn scratch(lang: &str, sample: &str) -> PathBuf {
    let name: String = sample
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    std::env::temp_dir()
        .join(format!("rdra-ish-codegen-{}", std::process::id()))
        .join(lang)
        .join(name)
}

fn run(cmd: &mut Command, what: &str) {
    let output = cmd.output().unwrap_or_else(|e| panic!("{what}: {e}"));
    assert!(
        output.status.success(),
        "{what}:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn typescript_builds() {
    if !available("tsc") {
        return;
    }
    for sample in samples() {
        let path = generate(&sample, "ts", &scratch("ts", &sample), "domain.ts");
        run(
            Command::new("tsc")
                .args(["--strict", "--noEmit", "--target", "es2020"])
                .arg(&path),
            &format!("tsc on the codegen of {sample}"),
        );
    }
}

#[test]
fn rust_builds() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    for sample in samples() {
        let dir = scratch("rust", &sample);
        let path = generate(&sample, "rust", &dir, "domain.rs");
        run(
            Command::new(&rustc)
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
                .arg(&path),
            &format!("rustc on the codegen of {sample}"),
        );
    }
}

#[test]
fn go_builds() {
    if !available("go") {
        return;
    }
    for sample in samples() {
        let dir = scratch("go", &sample);
        generate(&sample, "go", &dir, "domain.go");
        std::fs::write(dir.join("go.mod"), "module example.com/domain\n\ngo 1.21\n").unwrap();
        run(
            Command::new("go")
                .args(["vet", "./..."])
                .current_dir(&dir)
                .env("GOFLAGS", "-mod=mod"),
            &format!("go vet on the codegen of {sample}"),
        );
    }
}
