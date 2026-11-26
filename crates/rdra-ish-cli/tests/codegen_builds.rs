//! `rdra-ish codegen` output builds as printed: on every `samples.txt` root, on
//! `tests/fixtures/codegen/keywords.rdra`, whose names are keywords of the targets, on
//! `tests/fixtures/codegen/derived.rdra`, whose functions compute `@derived` columns, on
//! `tests/fixtures/codegen/nullable_enum_bool.rdra`, whose Enum and Bool columns hold null,
//! on `tests/fixtures/codegen/derived_null_in.rdra`, whose `@derived` `in` reads a null Int, and
//! on `tests/fixtures/codegen/use_case_vocabulary.rdra`, whose use cases link, copy and choose,
//! and on `tests/fixtures/codegen/entry_only.rdra`, whose every action is an entry point, the
//! TypeScript output passes `tsc --strict`, the Rust output compiles as a library with no
//! clippy warning and no undocumented public item (`clippy-driver -D warnings -D missing_docs`),
//! and the Go output passes `go vet` and `gofmt -l` and calls no `panic`, each with the
//! conformance harness of `--conformance`. A compiler missing from PATH skips its language,
//! except under `RDRA_REQUIRE_CODEGEN_TOOLS=1`, which CI sets in the dev shell that provides
//! them all.

use std::path::{Path, PathBuf};
use std::process::Command;

mod codegen_cases;

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

/// The models every language builds: the samples, a model whose names are keywords, one
/// whose functions compute derived columns, one whose Enum and Bool columns hold null, and one
/// whose derived `in` reads a null Int, one whose use cases link, copy and choose (#335), and
/// one whose every action is an entry point: it has no function, only use-case errors.
fn models() -> Vec<String> {
    let mut models = samples();
    models.push("crates/rdra-ish-cli/tests/fixtures/codegen/keywords.rdra".to_string());
    models.push("crates/rdra-ish-cli/tests/fixtures/codegen/derived.rdra".to_string());
    models.push("crates/rdra-ish-cli/tests/fixtures/codegen/nullable_enum_bool.rdra".to_string());
    models.push("crates/rdra-ish-cli/tests/fixtures/codegen/derived_null_in.rdra".to_string());
    models.push("crates/rdra-ish-cli/tests/fixtures/codegen/use_case_vocabulary.rdra".to_string());
    models.push("crates/rdra-ish-cli/tests/fixtures/codegen/entry_only.rdra".to_string());
    models
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

/// The codegen output of `sample` in `lang`, written to `dir/file`; Rust and Go with the
/// conformance harness, its traces from at most 4096 states. A sample `check` rejects is
/// refused, and generated again with `--allow-failing` and no harness.
fn generate(sample: &str, lang: &str, dir: &Path, file: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(file);
    let traces = dir.join("traces.txt");
    let harness = [
        "--conformance",
        traces.to_str().expect("a UTF-8 path"),
        "--conformance-states",
        "4096",
    ];
    let mut output = codegen(sample, lang, &path, &harness);
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
    for sample in models() {
        let path = generate(&sample, "ts", &scratch("ts", &sample), "domain.ts");
        // The strictest flags a project may set: the probe's tsconfig.rdra.json extends the
        // project's own, so the domain module is checked under them.
        run(
            Command::new("tsc")
                .args([
                    "--strict",
                    "--noEmit",
                    "--target",
                    "es2020",
                    "--module",
                    "esnext",
                    "--noUnusedLocals",
                    "--noUnusedParameters",
                    "--exactOptionalPropertyTypes",
                    "--noUncheckedIndexedAccess",
                    "--noImplicitReturns",
                    "--noFallthroughCasesInSwitch",
                    "--noPropertyAccessFromIndexSignature",
                    "--isolatedModules",
                ])
                .arg(&path),
            &format!("tsc on the codegen of {sample}"),
        );
        kamae_shapes(&sample, &std::fs::read_to_string(&path).unwrap());
    }
}

/// The TypeScript output keeps the Kamae rules a compiler does not check: no interface, no
/// type assertion but `as const`, and no `throw` but the one in `assertNever`.
fn kamae_shapes(sample: &str, code: &str) {
    let lines: Vec<&str> = code
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !(t.starts_with("//") || t.starts_with("/*") || t.starts_with('*'))
        })
        .collect();
    for line in &lines {
        let line = &without_strings(line);
        assert!(!line.contains("interface "), "{sample}: {line}");
        let casts = line.matches(" as ").count();
        let allowed = line.matches(" as const").count();
        assert_eq!(casts, allowed, "{sample}: a type assertion: {line}");
    }
    let throws = lines
        .iter()
        .filter(|l| without_strings(l).contains("throw "))
        .count();
    assert_eq!(throws, 1, "{sample}: only assertNever throws");
}

/// `line` with the text of its `"..."` literals and trailing `//` comment left out, so a
/// model label cannot look like code.
fn without_strings(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push('"');
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            chars.next();
                        }
                        '"' => break,
                        _ => {}
                    }
                }
                out.push('"');
            }
            '/' if chars.clone().next() == Some('/') => break,
            c => out.push(c),
        }
    }
    out
}

/// The Rust output passes clippy as it is printed: no crate-level allow hides a lint, and every
/// public item has rustdoc (docs/design/algebraic-codegen.md, "Generated code style").
#[test]
fn rust_builds() {
    if !available("clippy-driver") {
        return;
    }
    for sample in models() {
        let dir = scratch("rust", &sample);
        let path = generate(&sample, "rust", &dir, "domain.rs");
        run(
            Command::new("clippy-driver")
                .args([
                    "--edition",
                    "2021",
                    "--crate-type",
                    "lib",
                    "-D",
                    "warnings",
                    "-D",
                    "missing_docs",
                    "-o",
                ])
                .arg(dir.join("libdomain.rlib"))
                .arg(&path),
            &format!("clippy on the codegen of {sample}"),
        );
    }
}

/// The probe of every sample `check` passes names each contract on its own line: compiled
/// against the generated module with a service that implements none, rustc reports E0277 on
/// every use case line of the line map, and nothing else.
#[test]
fn rust_probes_name_every_contract() {
    if !available("rustc") {
        return;
    }
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let mut checked = 0;
    for sample in samples() {
        if !codegen_cases::check_passes(&sample) {
            continue;
        }
        let project = rdra_ish_core::load_project(&[repo().join(&sample)], None).expect("loads");
        let domain = rdra_ish_emit::codegen::build_with(
            &project.model,
            Some(codegen_cases::caps()),
            false,
            &|_| None,
        )
        .expect("builds");
        let binding = rdra_ish_emit::codegen::ProbeBinding {
            service: "svc::Nothing".to_string(),
            module: "svc::domain".to_string(),
            conformance: None,
            package: None,
        };
        let Some(probe) = rdra_ish_emit::codegen::probe_for(
            rdra_ish_emit::codegen::Lang::Rust,
            &domain,
            &binding,
        ) else {
            continue;
        };
        let dir = scratch("rust-probe", &sample);
        generate(&sample, "rust", &dir, "domain.rs");
        std::fs::write(
            dir.join("lib.rs"),
            "#[path = \"domain.rs\"]\npub mod domain;\n/// A service that implements no contract.\npub struct Nothing;\n",
        )
        .unwrap();
        run(
            Command::new(&rustc)
                .args([
                    "--edition",
                    "2021",
                    "--crate-type",
                    "lib",
                    "--crate-name",
                    "svc",
                    "-A",
                    "warnings",
                    "-o",
                ])
                .arg(dir.join("libsvc.rlib"))
                .arg(dir.join("lib.rs")),
            &format!("rustc on the module of {sample}"),
        );
        // The file as written: the fingerprint is its second line.
        let (first, rest) = probe.text.split_once('\n').expect("a first line");
        std::fs::write(
            dir.join("probe.rs"),
            format!("{first}\n// fingerprint\n{rest}"),
        )
        .unwrap();
        let out = Command::new(&rustc)
            .args([
                "--edition",
                "2021",
                "--test",
                "--error-format=json",
                "--extern",
            ])
            .arg(format!("svc={}", dir.join("libsvc.rlib").display()))
            .arg("-o")
            .arg(dir.join("probe"))
            .arg(dir.join("probe.rs"))
            .output()
            .expect("runs rustc");
        let mut lines = std::collections::BTreeSet::new();
        for line in String::from_utf8_lossy(&out.stderr).lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if v["level"] != "error" || v["code"].is_null() {
                continue;
            }
            assert_eq!(v["code"]["code"], "E0277", "{sample}: {}", v["rendered"]);
            let span = v["spans"]
                .as_array()
                .and_then(|s| s.iter().find(|s| s["is_primary"] == true))
                .expect("a primary span");
            lines.insert(span["line_start"].as_u64().unwrap() as usize);
        }
        let want: std::collections::BTreeSet<usize> = probe.lines.iter().map(|l| l.line).collect();
        assert_eq!(lines, want, "{sample}: E0277 lines against the line map");
        checked += want.len();
    }
    assert!(checked > 100, "only {checked} probe lines checked");
}

/// The TypeScript probe of every sample `check` passes names each contract on its own line:
/// checked against the generated module with a service type that has no member of any
/// contract, tsc reports a missing member (TS2739, TS2740 or TS2741) on every use case line of
/// the line map, and nothing else.
#[test]
fn typescript_probes_name_every_contract() {
    if !available("tsc") {
        return;
    }
    let mut checked = 0;
    for sample in samples() {
        if !codegen_cases::check_passes(&sample) {
            continue;
        }
        let project = rdra_ish_core::load_project(&[repo().join(&sample)], None).expect("loads");
        let domain = rdra_ish_emit::codegen::build_with(
            &project.model,
            Some(codegen_cases::caps()),
            false,
            &|_| None,
        )
        .expect("builds");
        let binding = rdra_ish_emit::codegen::ProbeBinding {
            service: "./nothing.ts#Nothing".to_string(),
            module: "./domain.ts".to_string(),
            conformance: None,
            package: None,
        };
        let Some(probe) = rdra_ish_emit::codegen::probe_for(
            rdra_ish_emit::codegen::Lang::TypeScript,
            &domain,
            &binding,
        ) else {
            continue;
        };
        let dir = scratch("ts-probe", &sample);
        generate(&sample, "ts", &dir, "domain.ts");
        std::fs::write(
            dir.join("nothing.ts"),
            "/** A service that implements no contract. */\nexport type Nothing = Readonly<{ nothing: true }>;\n",
        )
        .unwrap();
        // The file as written: the fingerprint is its second line.
        let (first, rest) = probe.text.split_once('\n').expect("a first line");
        std::fs::write(
            dir.join("probe.ts"),
            format!("{first}\n// fingerprint\n{rest}"),
        )
        .unwrap();
        let out = Command::new("tsc")
            .args([
                "--strict",
                "--noEmit",
                "--pretty",
                "false",
                "--target",
                "es2022",
                "--module",
                "nodenext",
                "--allowImportingTsExtensions",
                "--noUnusedLocals",
            ])
            .arg(dir.join("probe.ts"))
            .output()
            .expect("runs tsc");
        let mut lines = std::collections::BTreeSet::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let Some((head, text)) = line.split_once("): error ") else {
                continue;
            };
            assert!(head.contains("probe.ts("), "{sample}: {line}");
            let code = text.split(':').next().unwrap_or_default();
            assert!(
                ["TS2739", "TS2740", "TS2741"].contains(&code),
                "{sample}: {line}"
            );
            let at = head.rsplit_once('(').unwrap().1;
            lines.insert(at.split(',').next().unwrap().parse::<usize>().unwrap());
        }
        let want: std::collections::BTreeSet<usize> = probe.lines.iter().map(|l| l.line).collect();
        assert_eq!(
            lines, want,
            "{sample}: missing-member lines against the line map"
        );
        checked += want.len();
    }
    assert!(checked > 100, "only {checked} probe lines checked");
}

/// The Go output passes `go vet`, is what `gofmt` prints (`gofmt -l` names no file), and
/// calls no `panic`: a failure a caller can cause is an error value
/// (docs/design/algebraic-codegen.md, "Generated code style").
#[test]
fn go_builds() {
    if !available("go") {
        return;
    }
    let mut models = models();
    // Model types named like the names the Go printer derives: each of those takes a `_`.
    models.push("crates/rdra-ish-cli/tests/fixtures/codegen/go_names.rdra".to_string());
    for sample in models {
        let dir = scratch("go", &sample);
        let path = generate(&sample, "go", &dir, "domain.go");
        std::fs::write(dir.join("go.mod"), "module example.com/domain\n\ngo 1.21\n").unwrap();
        run(
            Command::new("go")
                .args(["vet", "./..."])
                .current_dir(&dir)
                .env("GOFLAGS", "-mod=mod"),
            &format!("go vet on the codegen of {sample}"),
        );
        let gofmt = Command::new("gofmt")
            .arg("-l")
            .arg(&path)
            .output()
            .expect("runs gofmt");
        assert!(
            gofmt.status.success() && gofmt.stdout.is_empty(),
            "gofmt -l names the codegen of {sample}: run `gofmt -d` on it\n{}",
            String::from_utf8_lossy(&gofmt.stderr)
        );
        let code = std::fs::read_to_string(&path).unwrap();
        assert!(
            !code.contains("panic("),
            "the codegen of {sample} calls panic"
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
