//! `check --code=build` and `--code=run` on Go (docs/design/code-check.md, P2 and P4): a
//! reference service (`tests/fixtures/codegen/go_service`) implements every use case of
//! `conformance.rdra`. Each test plants one omission in it and checks the named diagnostic, or
//! plants one bug (`FAULT`) the conformance test in the probe must catch; and the probe of
//! every sample compiles, the service lacking only the use cases it does not implement.
//! Without `go` on PATH the tests are skipped, except under `RDRA_REQUIRE_CODEGEN_TOOLS=1`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codegen")
}

fn go_available() -> bool {
    let found = Command::new("go").arg("version").output().is_ok();
    if !found {
        assert!(
            std::env::var("RDRA_REQUIRE_CODEGEN_TOOLS").as_deref() != Ok("1"),
            "go is not on PATH, and RDRA_REQUIRE_CODEGEN_TOOLS=1 requires it"
        );
        eprintln!("skipping: go is not on PATH");
    }
    found
}

const MANIFEST: &str = r#"[model]
name = "shop"

[code.go]
domain = "../domain/domain.go"
traces = "../svc/traces.txt"
conformance_states = 4096
probe = "../svc/rdra_contract_test.go"
service = "example.com/shop/svc.Service"
new = "svc.New()"
"#;

/// A Go module of its own: `model/` with `model` and `manifest`, `svc/` with `service`, and
/// the files `codegen --code` writes.
fn project(name: &str, model: &Path, manifest: &str, service: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("rdra-ish-code-check-go-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("model")).unwrap();
    std::fs::create_dir_all(dir.join("svc")).unwrap();
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::copy(model, dir.join("model/model.rdra")).unwrap();
    std::fs::write(dir.join("model/rdra.toml"), manifest).unwrap();
    std::fs::write(dir.join("go.mod"), "module example.com/shop\n\ngo 1.21\n").unwrap();
    std::fs::write(dir.join("svc/service.go"), service).unwrap();
    let (code, _, stderr) = text(&rdra(&dir, &["codegen", "model", "--code"], None));
    assert_eq!(code, Some(0), "{name}: {stderr}");
    dir
}

/// The reference service with `edits` made to it.
fn shop(name: &str, edits: &[(&str, &str)]) -> PathBuf {
    let mut service = std::fs::read_to_string(fixtures().join("go_service/service.go")).unwrap();
    for (from, to) in edits {
        assert!(service.contains(from), "the service holds {from:?}");
        service = service.replacen(from, to, 1);
    }
    project(
        name,
        &fixtures().join("conformance.rdra"),
        MANIFEST,
        &service,
    )
}

fn rdra(dir: &Path, args: &[&str], fault: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rdra-ish"));
    cmd.current_dir(dir).args(args).env("GOFLAGS", "-mod=mod");
    if let Some(fault) = fault {
        cmd.env("FAULT", fault);
    }
    cmd.output().expect("runs rdra-ish")
}

fn text(out: &Output) -> (Option<i32>, String, String) {
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The names of the `error[Name]` lines of `stderr`.
fn errors(stderr: &str) -> Vec<&str> {
    stderr
        .lines()
        .filter_map(|l| l.strip_prefix("error["))
        .filter_map(|l| l.split(']').next())
        .collect()
}

#[test]
fn the_reference_service_builds_and_conforms() {
    if !go_available() {
        return;
    }
    let dir = shop("complete", &[]);
    let (code, stdout, stderr) = text(&rdra(&dir, &["check", "model", "--code=run"], None));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stdout.ends_with("OK: code: what the model requires, at --code=run\n"),
        "{stdout}"
    );
    let probe = std::fs::read_to_string(dir.join("svc/rdra_contract_test.go")).unwrap();
    let gofmt = Command::new("gofmt")
        .arg("-l")
        .arg(dir.join("svc/rdra_contract_test.go"))
        .output()
        .expect("runs gofmt");
    assert!(gofmt.stdout.is_empty(), "gofmt names the probe:\n{probe}");
}

/// One omission at a time, each with the diagnostic that names it.
#[test]
fn each_planted_omission_is_named() {
    if !go_available() {
        return;
    }
    // (name, edits to the service, the diagnostic, a detail of it)
    type Omission<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a str, &'a str);
    let cases: &[Omission] = &[
        (
            "missing",
            &[
                ("func (s *Service) PrepareShip(", "func (s *Service) prepareShip("),
                ("func (s *Service) CommitShip(", "func (s *Service) commitShip("),
            ],
            "CodeMissingUseCase_Ship",
            "model: model.rdra:22:1  usecase Ship",
        ),
        (
            "incomplete",
            &[("func (s *Service) CommitShip(", "func (s *Service) commitShip(")],
            "CodeIncomplete_Ship",
            "give example.com/shop/svc.Service the methods of ShipUseCase (PrepareShip, CommitShip)",
        ),
        (
            "signature",
            &[(
                "PreparePay(id domain.OrderId, paidAt domain.DateTime)",
                "PreparePay(id domain.OrderId, paidAt domain.DateTime, extra int)",
            )],
            "CodeSignature_Pay",
            "wrong type for method PreparePay",
        ),
        (
            "unmapped",
            &[("return domain.RunPay(state, paidAt)", "return domain.RunPay(state, 1)")],
            "CodeUnmapped",
            "svc/service.go:",
        ),
    ];
    for (name, edits, diagnostic, detail) in cases {
        let dir = shop(name, edits);
        let (code, stdout, stderr) = text(&rdra(&dir, &["check", "model", "--code=build"], None));
        assert_eq!(code, Some(1), "{name}: {stderr}");
        assert_eq!(errors(&stderr), [*diagnostic], "{name}: {stderr}");
        assert!(stderr.contains(detail), "{name}: {detail:?} in\n{stderr}");
        assert!(
            stdout.contains("FAIL: code: 1 error(s) at --code=build"),
            "{name}: {stdout}"
        );
    }
}

/// `build` in rdra.toml replaces the default command.
#[test]
fn a_build_command_in_rdra_toml_replaces_the_default() {
    if !go_available() {
        return;
    }
    let service = std::fs::read_to_string(fixtures().join("go_service/service.go")).unwrap();
    let manifest = format!("{MANIFEST}build = [\"go\", \"vet\", \"./svc\"]\n");
    let dir = project(
        "override",
        &fixtures().join("conformance.rdra"),
        &manifest,
        &service.replacen(
            "func (s *Service) CommitShip(",
            "func (s *Service) commitShip(",
            1,
        ),
    );
    let (code, _, stderr) = text(&rdra(&dir, &["check", "model", "--code=build"], None));
    assert_eq!(code, Some(1), "{stderr}");
    assert_eq!(errors(&stderr), ["CodeIncomplete_Ship"], "{stderr}");
}

/// Each fault of the reference service fails the conformance test the probe holds.
#[test]
fn each_fault_fails_the_conformance_run() {
    if !go_available() {
        return;
    }
    let dir = shop("faults", &[]);
    for fault in ["skip-core", "drop-row", "accept-disabled", "unconditional"] {
        let (code, stdout, stderr) =
            text(&rdra(&dir, &["check", "model", "--code=run"], Some(fault)));
        assert_eq!(code, Some(1), "{fault}: {stderr}");
        assert_eq!(errors(&stderr), ["CodeConformance"], "{fault}: {stderr}");
        assert!(
            stdout.contains("FAIL: code: 1 error(s) at --code=run"),
            "{fault}: {stdout}"
        );
    }
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

/// The `.rdra` files of `from` (a file, or a directory searched in full) under `to`.
fn copy_model(from: &Path, to: &Path) {
    if from.is_file() {
        std::fs::copy(from, to.join(from.file_name().unwrap())).unwrap();
        return;
    }
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            let sub = to.join(path.file_name().unwrap());
            std::fs::create_dir_all(&sub).unwrap();
            copy_model(&path, &sub);
        } else if path.extension().is_some_and(|e| e == "rdra") {
            std::fs::copy(&path, to.join(path.file_name().unwrap())).unwrap();
        }
    }
}

/// The probe of every sample compiles against a service with no method: the compiler's only
/// complaints are the use cases the service lacks, each one `CodeMissingUseCase`.
#[test]
fn the_probe_of_every_sample_compiles() {
    if !go_available() {
        return;
    }
    let manifest = "[model]\nname = \"sample\"\n\n[code.go]\ndomain = \"../domain/domain.go\"\nprobe = \"../svc/rdra_contract_test.go\"\nservice = \"example.com/shop/svc.Service\"\n";
    let empty = "// Package svc implements no use case.\npackage svc\n\n// Service has no method.\ntype Service struct{}\n";
    let mut checked = Vec::new();
    for sample in samples() {
        let path = repo().join(&sample);
        let name: String = sample
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let dir = std::env::temp_dir()
            .join(format!("rdra-ish-code-check-go-{}", std::process::id()))
            .join(&name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("model")).unwrap();
        std::fs::create_dir_all(dir.join("svc")).unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        copy_model(&path, &dir.join("model"));
        std::fs::write(dir.join("model/rdra.toml"), manifest).unwrap();
        std::fs::write(dir.join("go.mod"), "module example.com/shop\n\ngo 1.21\n").unwrap();
        std::fs::write(dir.join("svc/service.go"), empty).unwrap();
        let (code, _, stderr) = text(&rdra(&dir, &["codegen", "model", "--code"], None));
        if code != Some(0) && stderr.contains("refusing to generate code") {
            continue;
        }
        assert_eq!(code, Some(0), "{sample}: {stderr}");
        let contracts = std::fs::read_to_string(dir.join("svc/rdra_contract_test.go"))
            .unwrap()
            .lines()
            .filter(|l| l.starts_with("var _ domain."))
            .count();
        let (_, _, stderr) = text(&rdra(&dir, &["check", "model", "--code=build"], None));
        let found = errors(&stderr);
        assert!(
            found.iter().all(|e| e.starts_with("CodeMissingUseCase_")),
            "{sample}: {stderr}"
        );
        assert_eq!(found.len(), contracts, "{sample}: {stderr}");
        checked.push((sample, contracts));
    }
    eprintln!("probes compiled: {checked:?}");
    // Most samples pass `check`; a refused one has no Go to probe.
    assert!(checked.len() * 2 > samples().len(), "{checked:?}");
}
