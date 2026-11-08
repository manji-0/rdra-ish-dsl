//! `rdra-ish codegen` output builds as printed: on every `samples.txt` root, the TypeScript
//! output passes `tsc --strict`, the Rust output compiles as a library, and the Go output
//! passes `go vet`. A compiler missing from PATH skips its language, except under
//! `RDRA_REQUIRE_CODEGEN_TOOLS=1`, which CI sets in the dev shell that provides all three.

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
            std::env::var("RDRA_REQUIRE_CODEGEN_TOOLS").as_deref() != Ok("1"),
            "{tool} is not on PATH, and RDRA_REQUIRE_CODEGEN_TOOLS=1 requires it; the dev shell provides it"
        );
        eprintln!("skipping: {tool} is not on PATH");
    }
    found
}

fn codegen(sample: &str, lang: &str, path: &Path, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(["codegen", sample, "--lang", lang])
        .args(extra)
        .arg("-o")
        .arg(path)
        .output()
        .expect("runs rdra-ish")
}

/// The codegen output of `sample` in `lang`, written to `dir/file`. A sample `check`
/// rejects is refused, and generated again with `--allow-failing`.
fn generate(sample: &str, lang: &str, dir: &Path, file: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(file);
    let mut output = codegen(sample, lang, &path, &[]);
    if !output.status.success()
        && String::from_utf8_lossy(&output.stderr).contains("refusing to generate code")
    {
        output = codegen(sample, lang, &path, &["--allow-failing"]);
    }
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

/// A model `check` rejects is refused unless `--allow-failing`, which lists its errors as
/// unmet obligations and narrows no state type.
#[test]
fn a_failing_model_is_refused_unless_allowed() {
    let sample = "samples/formal-verification/cross_order_payment_split.rdra";
    let dir = scratch("refuse", sample);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("domain.ts");
    let refused = codegen(sample, "ts", &path, &[]);
    assert_eq!(refused.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("MultiForbidden_Order_0"), "{stderr}");
    assert!(!path.exists(), "nothing is written when refused");

    let allowed = codegen(sample, "ts", &path, &["--allow-failing"]);
    assert!(
        allowed.status.success(),
        "{}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    let code = std::fs::read_to_string(&path).unwrap();
    assert!(code.contains("NOT MET by the model"), "{code}");
    assert!(code.contains("MultiForbidden_Order_0"), "{code}");
}
