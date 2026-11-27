//! The black-box model adapter `codegen --adapter` prints (#439): plain data of the model's
//! state and calls, the `ModelAdapter` interface, and a harness over it, with no type of the
//! generated domain code. On `tests/fixtures/codegen/conformance.rdra`, a reference adapter per
//! language over the reference service of codegen_conformance.rs passes the harness, and each
//! planted fault fails it with the words the port's harness uses. An adapter without
//! `prepare` and `commit` passes with the interleaved rows counted as skipped, and so its
//! unconditional commits go unseen.
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

#[path = "common/samples.rs"]
mod samples;
use samples::{repo, samples};

#[path = "common/reference.rs"]
mod reference;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codegen")
}

fn scratch(lang: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("rdra-ish-codegen-adapter-{}", std::process::id()))
        .join(lang);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `codegen` of the fixture in `lang` into `dir/file`, with its traces in `dir/<traces>`.
fn generate(lang: &str, dir: &Path, file: &str, traces: &str, extra: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .args(["codegen", "--lang", lang, "-o"])
        .arg(dir.join(file))
        .arg("--conformance")
        .arg(dir.join(traces))
        .args(extra)
        .arg(fixture().join("conformance.rdra"))
        .output()
        .expect("runs rdra-ish");
    assert!(output.status.success(), "{}", text(&output));
}

/// The domain code the reference service is built on, and the adapter file with its traces.
fn generate_both(lang: &str, dir: &Path, domain: &str, adapter: &str, extra: &[&str]) {
    generate(lang, dir, domain, "domain-traces.txt", extra);
    let mut args = vec!["--adapter"];
    args.extend(extra);
    generate(lang, dir, adapter, "traces.txt", &args);
    // The adapter drives the same traces as the port.
    assert_eq!(
        std::fs::read_to_string(dir.join("traces.txt")).unwrap(),
        std::fs::read_to_string(dir.join("domain-traces.txt")).unwrap()
    );
}

/// The faults of the reference service, and what the harness reports for each.
const FAULTS: [(&str, &str); 4] = [
    (
        "skip-core",
        "after `Pay 101 2026-01-01T00:00:00Z`, the store differs from the model:\n  model only: Order 101 status=paid paid_at=+ qty=2 customer_id=1",
    ),
    ("drop-row", "model only: Shipment 201 status=packed order_id=101"),
    (
        "accept-disabled",
        "the model refuses `Cancel 101` here, and the service ran it",
    ),
    (
        "unconditional",
        "the service committed `Place 1 101 card s`, prepared before `Place 1 101 card s` changed a row it writes",
    ),
];

/// The reference adapter passes with every row run, fails each fault where the port's harness
/// does, and, with no prepare and commit, passes with the interleaved rows skipped: an
/// unconditional commit then goes unseen.
fn check(lang: &str, run: impl Fn(&str, bool) -> Output) {
    let passed = run("", true);
    let out = text(&passed);
    assert!(
        passed.status.success(),
        "{lang}: the reference adapter fails:\n{out}"
    );
    assert!(
        out.contains("rdra-ish conformance: ok 11 traces, 21 calls, 11 refusals, 7 interleavings, 0 interleavings skipped, 32 split calls"),
        "{lang}: the reference adapter checked less than expected:\n{out}"
    );
    // A commit that never stores refuses every interleaved commit as it should; the calls
    // made again by prepare and commit catch it.
    let stub = run("stub-commit", true);
    let out = text(&stub);
    assert!(
        !stub.status.success(),
        "{lang}: a stub commit passes:\n{out}"
    );
    assert!(
        out.contains("and the service refused it through prepare and commit: "),
        "{lang}: a stub commit fails for another reason:\n{out}"
    );
    for (fault, report) in FAULTS {
        let failed = run(fault, true);
        let out = text(&failed);
        assert!(!failed.status.success(), "{lang}: {fault} passes:\n{out}");
        assert!(
            report.lines().all(|line| out.contains(line.trim())),
            "{lang}: {fault} fails for another reason:\n{out}"
        );
        assert!(out.contains("traces line "), "{lang}: {fault}:\n{out}");
    }
    for fault in ["", "unconditional"] {
        let unsplit = run(fault, false);
        let out = text(&unsplit);
        assert!(unsplit.status.success(), "{lang} unsplit {fault}:\n{out}");
        assert!(
            out.contains("rdra-ish conformance: ok 11 traces, 21 calls, 11 refusals, 0 interleavings, 7 interleavings skipped, 0 split calls"),
            "{lang} unsplit {fault}:\n{out}"
        );
    }
}

#[test]
fn rust_adapter_tells_faults_apart() {
    let dir = scratch("rust");
    generate_both("rust", &dir, "domain.rs", "model.rs", &[]);
    std::fs::write(dir.join("service.rs"), reference::rust_service()).unwrap();
    std::fs::copy(
        fixture().join("conformance_adapter.rs"),
        dir.join("adapter.rs"),
    )
    .unwrap();
    std::fs::write(
        dir.join("main.rs"),
        "#[allow(dead_code)]\nmod domain;\nmod model;\nmod service;\nmod adapter;\n\nfn main() {\n    let path = std::env::args().nth(1).expect(\"the traces file\");\n    let traces = std::fs::read_to_string(path).expect(\"reads the traces\");\n    let mut a = adapter::Adapter::new();\n    match model::conform(&mut a, &traces) {\n        Ok(r) => println!(\"{}\", r.line()),\n        Err(why) => {\n            println!(\"FAIL {why}\");\n            std::process::exit(1);\n        }\n    }\n}\n",
    )
    .unwrap();
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let exe = dir.join(format!("adapter{}", std::env::consts::EXE_SUFFIX));
    let built = Command::new(rustc)
        .args(["--edition", "2021", "-D", "warnings", "-o"])
        .arg(&exe)
        .arg(dir.join("main.rs"))
        .output()
        .expect("runs rustc");
    assert!(built.status.success(), "rustc:\n{}", text(&built));
    check("rust", |fault, split| {
        Command::new(&exe)
            .arg(dir.join("traces.txt"))
            .env("FAULT", fault)
            .env("SPLIT", if split { "1" } else { "0" })
            .output()
            .expect("runs the adapter")
    });
}

/// `--adapter` without `--conformance` is a usage error: the adapter drives the traces.
#[test]
fn the_adapter_needs_the_traces() {
    let out = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .args(["codegen", "--lang", "rust", "--adapter"])
        .arg(fixture().join("conformance.rdra"))
        .output()
        .expect("runs rdra-ish");
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains("--conformance"), "{}", text(&out));
}

#[test]
fn go_adapter_tells_faults_apart() {
    if !tool("go", CODEGEN) {
        return;
    }
    let dir = scratch("go");
    for d in ["domain", "modeladapter", "svc"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    generate("go", &dir, "domain/domain.go", "domain-traces.txt", &[]);
    generate(
        "go",
        &dir,
        "modeladapter/modeladapter.go",
        "traces.txt",
        &["--adapter", "--package", "modeladapter"],
    );
    std::fs::write(dir.join("go.mod"), "module example.com/shop\n\ngo 1.21\n").unwrap();
    for file in ["service.go", "adapter.go"] {
        std::fs::copy(
            fixture().join("go_service").join(file),
            dir.join("svc").join(file),
        )
        .unwrap();
    }
    std::fs::write(
        dir.join("svc/main_test.go"),
        "package svc_test\n\nimport (\n\t\"os\"\n\t\"testing\"\n\n\t\"example.com/shop/modeladapter\"\n\t\"example.com/shop/svc\"\n)\n\nfunc TestAdapter(t *testing.T) {\n\ttraces, err := os.ReadFile(os.Getenv(\"TRACES\"))\n\tif err != nil {\n\t\tt.Fatal(err)\n\t}\n\treport, err := modeladapter.Conform(svc.NewAdapter(), string(traces))\n\tif err != nil {\n\t\tt.Fatal(\"FAIL \" + err.Error())\n\t}\n\tt.Log(report.Line())\n}\n",
    )
    .unwrap();
    check("go", |fault, split| {
        Command::new("go")
            .args(["test", "-count=1", "-v", "./svc"])
            .current_dir(&dir)
            .env("FAULT", fault)
            .env("SPLIT", if split { "1" } else { "0" })
            .env("TRACES", dir.join("traces.txt"))
            .env("GOFLAGS", "-mod=mod")
            .output()
            .expect("runs go test")
    });
}

#[test]
fn typescript_adapter_tells_faults_apart() {
    if !tools(&["tsc", "node"], CODEGEN) {
        return;
    }
    let dir = scratch("ts");
    generate_both("ts", &dir, "domain.ts", "model.ts", &[]);
    for file in ["order_service.ts", "adapter.ts"] {
        std::fs::copy(fixture().join("ts_service").join(file), dir.join(file)).unwrap();
    }
    std::fs::write(
        dir.join("main.ts"),
        "import * as m from \"./model.ts\";\nimport { adapter } from \"./adapter.ts\";\n\ndeclare const process: {\n  argv: string[];\n  exitCode: number | undefined;\n  getBuiltinModule: (id: \"node:fs\") => { readFileSync: (path: string, encoding: \"utf8\") => string };\n};\ndeclare const console: { log: (line: string) => void };\n\nconst traces = process.getBuiltinModule(\"node:fs\").readFileSync(process.argv[2] ?? \"\", \"utf8\");\nvoid m.conform(adapter(), traces).then((r) => {\n  if (r.ok) {\n    console.log(m.reportLine(r.value));\n  } else {\n    console.log(`FAIL ${r.error}`);\n    process.exitCode = 1;\n  }\n});\n",
    )
    .unwrap();
    let built = Command::new("tsc")
        .args([
            "--strict",
            "--noEmit",
            "--target",
            "es2022",
            "--module",
            "nodenext",
            "--allowImportingTsExtensions",
            "--noUnusedLocals",
            "--noUnusedParameters",
            "--exactOptionalPropertyTypes",
            "--noUncheckedIndexedAccess",
        ])
        .arg(dir.join("main.ts"))
        .output()
        .expect("runs tsc");
    assert!(built.status.success(), "tsc:\n{}", text(&built));
    check("ts", |fault, split| {
        Command::new("node")
            .arg(dir.join("main.ts"))
            .arg(dir.join("traces.txt"))
            .env("FAULT", fault)
            .env("SPLIT", if split { "1" } else { "0" })
            .output()
            .expect("runs node")
    });
}

/// The models whose adapter every language builds: the samples `check` passes and the
/// fixtures of codegen_builds.rs that have a harness.
fn models() -> Vec<String> {
    let mut models = samples();
    for fixture in [
        "keywords.rdra",
        "derived.rdra",
        "nullable_enum_bool.rdra",
        "derived_null_in.rdra",
        "use_case_vocabulary.rdra",
        "entry_only.rdra",
        "go_names.rdra",
        "conformance.rdra",
        "adapter_names.rdra",
    ] {
        models.push(format!(
            "crates/rdra-ish-cli/tests/fixtures/codegen/{fixture}"
        ));
    }
    models
}

/// The adapter of `sample` in `lang`, in `dir/file`; `None` for a model `check` rejects, which
/// has no transition functions and so no traces.
fn adapter_of(sample: &str, lang: &str, dir: &Path, file: &str, extra: &[&str]) -> Option<PathBuf> {
    std::fs::create_dir_all(dir).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args([
            "codegen",
            sample,
            "--lang",
            lang,
            "--adapter",
            "--conformance",
        ])
        .arg(dir.join("traces.txt"))
        .args(["--conformance-states", "4096", "-o"])
        .arg(dir.join(file))
        .args(extra)
        .output()
        .expect("runs rdra-ish");
    if !out.status.success() && text(&out).contains("refusing to generate code") {
        return None;
    }
    assert!(
        out.status.success(),
        "{sample} --lang {lang}: {}",
        text(&out)
    );
    Some(dir.join(file))
}

fn sample_dir(lang: &str, sample: &str) -> PathBuf {
    let name: String = sample
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    scratch("samples").join(lang).join(name)
}

/// The Rust adapter of every model passes clippy with every public item documented.
#[test]
fn rust_adapters_build() {
    if !tool("clippy-driver", CODEGEN) {
        return;
    }
    for sample in models() {
        let dir = sample_dir("rust", &sample);
        let Some(path) = adapter_of(&sample, "rust", &dir, "model.rs", &[]) else {
            continue;
        };
        let out = Command::new("clippy-driver")
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
            .arg(dir.join("libmodel.rlib"))
            .arg(&path)
            .output()
            .expect("runs clippy-driver");
        assert!(
            out.status.success(),
            "clippy on the adapter of {sample}:\n{}",
            text(&out)
        );
    }
}

/// The Go adapter of every model passes `go vet` and is what `gofmt` prints.
#[test]
fn go_adapters_build() {
    if !tool("go", CODEGEN) {
        return;
    }
    for sample in models() {
        let dir = sample_dir("go", &sample);
        let Some(path) = adapter_of(&sample, "go", &dir, "model.go", &["--package", "model"])
        else {
            continue;
        };
        std::fs::write(dir.join("go.mod"), "module example.com/model\n\ngo 1.21\n").unwrap();
        let out = Command::new("go")
            .args(["vet", "./..."])
            .current_dir(&dir)
            .env("GOFLAGS", "-mod=mod")
            .output()
            .expect("runs go vet");
        assert!(
            out.status.success(),
            "go vet on the adapter of {sample}:\n{}",
            text(&out)
        );
        let gofmt = Command::new("gofmt")
            .arg("-l")
            .arg(&path)
            .output()
            .expect("runs gofmt");
        assert!(
            gofmt.status.success() && gofmt.stdout.is_empty(),
            "gofmt -l names the adapter of {sample}: run `gofmt -d` on it"
        );
        let code = std::fs::read_to_string(&path).unwrap();
        assert!(
            !code.contains("panic("),
            "the adapter of {sample} calls panic"
        );
    }
}

/// The TypeScript adapter of every model passes tsc at the strictest flags a project may set,
/// and keeps the Kamae shapes of the domain code: no interface, no type assertion, no throw.
#[test]
fn typescript_adapters_build() {
    if !tool("tsc", CODEGEN) {
        return;
    }
    for sample in models() {
        let dir = sample_dir("ts", &sample);
        let Some(path) = adapter_of(&sample, "ts", &dir, "model.ts", &[]) else {
            continue;
        };
        let out = Command::new("tsc")
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
            .arg(&path)
            .output()
            .expect("runs tsc");
        assert!(
            out.status.success(),
            "tsc on the adapter of {sample}:\n{}",
            text(&out)
        );
        let code = std::fs::read_to_string(&path).unwrap();
        for line in code.lines().filter(|l| {
            let t = l.trim_start();
            !(t.starts_with("//") || t.starts_with("/*") || t.starts_with('*'))
        }) {
            assert!(!line.contains("interface "), "{sample}: {line}");
            assert!(!line.contains("throw "), "{sample}: {line}");
            assert_eq!(
                line.matches(" as ").count(),
                line.matches(" as const").count(),
                "{sample}: {line}"
            );
        }
    }
}

/// A `[code.<lang>]` table names its code file once: `domain` or `adapter`, and an adapter
/// with its traces. Another table fails to load, as a table missing a required key does, so
/// every command that reads rdra.toml fails on it.
#[test]
fn a_code_table_names_the_domain_or_the_adapter() {
    let dir = scratch("tables");
    std::fs::copy(fixture().join("conformance.rdra"), dir.join("order.rdra")).unwrap();
    for (table, says) in [
        ("probe = \"p.rs\"", "names neither `domain` nor `adapter`"),
        (
            "domain = \"d.rs\"\nadapter = \"a.rs\"\ntraces = \"t.txt\"",
            "names both `domain` and `adapter`",
        ),
        ("adapter = \"a.rs\"", "names `adapter` without `traces`"),
    ] {
        std::fs::write(
            dir.join("rdra.toml"),
            format!("[model]\nname = \"shop\"\n\n[code.rust]\n{table}\n"),
        )
        .unwrap();
        for args in [
            &["codegen", ".", "--code"][..],
            &["check", ".", "--code=fresh"][..],
            &["check", "."][..],
        ] {
            let out = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
                .current_dir(&dir)
                .args(args)
                .output()
                .expect("runs rdra-ish");
            assert!(!out.status.success(), "{table}: {}", text(&out));
            assert!(
                text(&out).contains(&format!("rdra.toml:4: [code.rust] {says}")),
                "{table}: {}",
                text(&out)
            );
        }
    }
}
