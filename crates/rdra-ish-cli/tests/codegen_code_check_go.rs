//! `check --code=build` and `--code=run` on Go (docs/design/code-check.md, P2 and P4): a
//! reference service (`tests/fixtures/codegen/go_service`) implements every use case of
//! `conformance.rdra`. Each test plants one omission in it and checks the named diagnostic, or
//! plants one bug (`FAULT`) the conformance test in the probe must catch; and the probe of
//! every sample compiles, the service lacking only the use cases it does not implement.
//! Without `go` on PATH the tests are skipped, except under `RDRA_REQUIRE_CODEGEN_TOOLS=1`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "common/output.rs"]
mod output;
#[path = "common/samples.rs"]
mod samples;
use output::outcome;
use samples::{repo, samples};

#[path = "common/tools.rs"]
mod tools;
use tools::{tool, CODEGEN};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codegen")
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

/// The model the reference service implements.
fn conformance() -> String {
    std::fs::read_to_string(fixtures().join("conformance.rdra")).unwrap()
}

/// A Go module of its own: `model/` with `model` and `manifest`, `svc/` with `service`, and
/// the files `codegen --code` writes.
fn project(name: &str, model: &str, manifest: &str, service: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("rdra-ish-code-check-go-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("model")).unwrap();
    std::fs::create_dir_all(dir.join("svc")).unwrap();
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::write(dir.join("model/model.rdra"), model).unwrap();
    std::fs::write(dir.join("model/rdra.toml"), manifest).unwrap();
    std::fs::write(dir.join("go.mod"), "module example.com/shop\n\ngo 1.21\n").unwrap();
    std::fs::write(dir.join("svc/service.go"), service).unwrap();
    let (code, _, stderr) = outcome(&rdra(&dir, &["codegen", "model", "--code"], None));
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
    project(name, &conformance(), MANIFEST, &service)
}

fn rdra(dir: &Path, args: &[&str], fault: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rdra-ish"));
    cmd.current_dir(dir).args(args).env("GOFLAGS", "-mod=mod");
    if let Some(fault) = fault {
        cmd.env("FAULT", fault);
    }
    cmd.output().expect("runs rdra-ish")
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
    if !tool("go", CODEGEN) {
        return;
    }
    let dir = shop("complete", &[]);
    let (code, stdout, stderr) = outcome(&rdra(&dir, &["check", "model", "--code=run"], None));
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
    if !tool("go", CODEGEN) {
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
            "model: model/model.rdra:22:1  usecase Ship",
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
        let (code, stdout, stderr) =
            outcome(&rdra(&dir, &["check", "model", "--code=build"], None));
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
    if !tool("go", CODEGEN) {
        return;
    }
    let service = std::fs::read_to_string(fixtures().join("go_service/service.go")).unwrap();
    let manifest = format!("{MANIFEST}build = [\"go\", \"vet\", \"./svc\"]\n");
    let dir = project(
        "override",
        &conformance(),
        &manifest,
        &service.replacen(
            "func (s *Service) CommitShip(",
            "func (s *Service) commitShip(",
            1,
        ),
    );
    let (code, _, stderr) = outcome(&rdra(&dir, &["check", "model", "--code=build"], None));
    assert_eq!(code, Some(1), "{stderr}");
    assert_eq!(errors(&stderr), ["CodeIncomplete_Ship"], "{stderr}");
}

/// With an actor that performs Pay (#337), `PreparePay` takes a `PayCaller` first: the service
/// that takes it builds, and one that takes no caller, or another actor's capability, is a
/// `CodeSignature_Pay`.
#[test]
fn a_prepare_takes_the_caller_of_its_use_case() {
    if !tool("go", CODEGEN) {
        return;
    }
    let model = format!(
        "{}actor Clerk \"Clerk\"\nactor Courier \"Courier\"\nperforms(Clerk, Pay)\nperforms(Courier, Ship)\n",
        conformance()
    );
    let service = std::fs::read_to_string(fixtures().join("go_service/service.go")).unwrap();
    let pay = "PreparePay(id domain.OrderId, paidAt domain.DateTime)";
    let ship = "PrepareShip(id domain.OrderId, shipmentID domain.ShipmentId)";
    let ship_by =
        "PrepareShip(_ domain.ShipCaller, id domain.OrderId, shipmentID domain.ShipmentId)";
    for (name, by, says) in [
        ("caller", "_ domain.PayCaller, ", None),
        ("no-caller", "", Some("wrong type for method PreparePay")),
        (
            "other-actor",
            "_ domain.CourierCapability, ",
            Some("wrong type for method PreparePay"),
        ),
    ] {
        let service = service
            .replacen(pay, &pay.replacen('(', &format!("({by}"), 1), 1)
            .replacen(ship, ship_by, 1);
        let dir = project(name, &model, MANIFEST, &service);
        let (code, _, stderr) = outcome(&rdra(&dir, &["check", "model", "--code=build"], None));
        match says {
            None => assert_eq!(code, Some(0), "{name}: {stderr}"),
            Some(says) => {
                assert_eq!(code, Some(1), "{name}: {stderr}");
                assert_eq!(errors(&stderr), ["CodeSignature_Pay"], "{name}: {stderr}");
                assert!(stderr.contains(says), "{name}: {stderr}");
            }
        }
    }
}

/// Each fault of the reference service fails the conformance test the probe holds.
#[test]
fn each_fault_fails_the_conformance_run() {
    if !tool("go", CODEGEN) {
        return;
    }
    let dir = shop("faults", &[]);
    for (fault, reason) in [
        (
            "skip-core",
            "after `Pay 101 2026-01-01T00:00:00Z`, the store differs from the model",
        ),
        (
            "drop-row",
            "after `Ship 101 201`, the store differs from the model",
        ),
        (
            "accept-disabled",
            "the model refuses `Cancel 101` here, and the service ran it",
        ),
        (
            "unconditional",
            "the write must be conditional on what the row was",
        ),
    ] {
        let (code, stdout, stderr) =
            outcome(&rdra(&dir, &["check", "model", "--code=run"], Some(fault)));
        assert_eq!(code, Some(1), "{fault}: {stderr}");
        assert_eq!(errors(&stderr), ["CodeConformance"], "{fault}: {stderr}");
        // The diagnostic quotes the harness's report of the first difference.
        let quoted = stderr
            .lines()
            .find(|l| l.starts_with("  code:  traces line "))
            .unwrap_or_default();
        assert!(quoted.contains(reason), "{fault}: {reason:?} in\n{stderr}");
        assert!(
            stdout.contains("FAIL: code: 1 error(s) at --code=run"),
            "{fault}: {stdout}"
        );
    }
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
/// complaints are the use cases the service lacks, each one `CodeMissingUseCase`. The service's
/// package `app` sorts before `domain`, and the probe is what `gofmt` prints all the same.
#[test]
fn the_probe_of_every_sample_compiles() {
    if !tool("go", CODEGEN) {
        return;
    }
    let manifest = "[model]\nname = \"sample\"\n\n[code.go]\ndomain = \"../domain/domain.go\"\nprobe = \"../app/rdra_contract_test.go\"\nservice = \"example.com/shop/app.Service\"\n";
    let empty = "// Package app implements no use case.\npackage app\n\n// Service has no method.\ntype Service struct{}\n";
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
        std::fs::create_dir_all(dir.join("app")).unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        copy_model(&path, &dir.join("model"));
        std::fs::write(dir.join("model/rdra.toml"), manifest).unwrap();
        std::fs::write(dir.join("go.mod"), "module example.com/shop\n\ngo 1.21\n").unwrap();
        std::fs::write(dir.join("app/service.go"), empty).unwrap();
        let (code, _, stderr) = outcome(&rdra(&dir, &["codegen", "model", "--code"], None));
        if code != Some(0) && stderr.contains("refusing to generate code") {
            continue;
        }
        assert_eq!(code, Some(0), "{sample}: {stderr}");
        let gofmt = Command::new("gofmt")
            .arg("-l")
            .arg(dir.join("app/rdra_contract_test.go"))
            .output()
            .expect("runs gofmt");
        assert!(gofmt.stdout.is_empty(), "{sample}: gofmt names the probe");
        let contracts = std::fs::read_to_string(dir.join("app/rdra_contract_test.go"))
            .unwrap()
            .lines()
            .filter(|l| l.starts_with("var _ domain."))
            .count();
        let (_, _, stderr) = outcome(&rdra(&dir, &["check", "model", "--code=build"], None));
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

/// A domain package whose `package` clause is not the last element of its import path:
/// `domain.go` in `store/` with `package = "domain"`. The probe names it by `[code.go]
/// package`, and the reference service, which imports it as `domain`, builds and conforms.
#[test]
fn the_probe_names_the_domain_package_by_its_package_clause() {
    if !tool("go", CODEGEN) {
        return;
    }
    let service = std::fs::read_to_string(fixtures().join("go_service/service.go")).unwrap();
    let manifest = MANIFEST
        .replace("../domain/domain.go", "../store/domain.go")
        .replace("[code.go]\n", "[code.go]\npackage = \"domain\"\n");
    let dir = project(
        "package-clause",
        &conformance(),
        &manifest,
        &service.replacen(
            "\t\"example.com/shop/domain\"",
            "\t\"example.com/shop/store\"",
            1,
        ),
    );
    let (code, stdout, stderr) = outcome(&rdra(&dir, &["check", "model", "--code=run"], None));
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let probe = std::fs::read_to_string(dir.join("svc/rdra_contract_test.go")).unwrap();
    assert!(
        probe.contains("\tdomain \"example.com/shop/store\"\n"),
        "{probe}"
    );
}

/// The self-check of `--code=run` (#439): the reference service fails every mutant trace at
/// its changed row; a snapshot that is what was seeded fails the real traces; a service that
/// runs a call on whatever row it finds passes them and is caught by the absent-row mutant.
#[test]
fn the_self_check_catches_a_service_that_ignores_the_id() {
    if !tool("go", CODEGEN) {
        return;
    }
    let dir = shop("self-check", &[]);
    let (code, stdout, stderr) = outcome(&rdra(&dir, &["check", "model", "--code=run"], None));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stdout
            .contains("  self-check: 5 of 5 mutant traces failed at their changed row (seed 0); guard-removed mutants: Big\n"),
        "{stdout}"
    );

    let (code, stdout, stderr) = outcome(&rdra(
        &dir,
        &["check", "model", "--code=run"],
        Some("constant-observe"),
    ));
    assert_eq!(code, Some(1), "{stderr}");
    assert_eq!(errors(&stderr), ["CodeConformance"], "{stderr}");
    assert!(!stdout.contains("self-check:"), "{stdout}");

    let (code, stdout, stderr) = outcome(&rdra(
        &dir,
        &["check", "model", "--code=run"],
        Some("ignore-id"),
    ));
    assert_eq!(code, Some(1), "{stderr}");
    assert_eq!(errors(&stderr), ["CodeMutantSurvived"], "{stderr}");
    assert!(stderr.contains("the absent-row mutant"), "{stderr}");
    assert!(
        stdout
            .contains("  self-check: 4 of 5 mutant traces failed at their changed row (seed 0); guard-removed mutants: Big\n"),
        "{stdout}"
    );
}

const ADAPTER_MANIFEST: &str = r#"[model]
name = "shop"

[code.go]
adapter = "../modeladapter/modeladapter.go"
package = "modeladapter"
traces = "../svc/traces.txt"
conformance_states = 4096
probe = "../svc/rdra_adapter_test.go"
service = "example.com/shop/svc.Adapter"
module = "example.com/shop/modeladapter"
new = "svc.NewAdapter()"
"#;

/// The black-box model adapter (#439) through `check --code`: the domain code is the
/// project's own (generated once, then edited), and the reference adapter binds the service to
/// the model in a package of its own. `fresh` reads no domain file, the run states what it
/// ran, and the self-check works through the adapter.
#[test]
fn a_model_adapter_conforms_whatever_the_domain_code_became() {
    if !tool("go", CODEGEN) {
        return;
    }
    let dir = std::env::temp_dir()
        .join(format!("rdra-ish-code-check-go-{}", std::process::id()))
        .join("adapter");
    let _ = std::fs::remove_dir_all(&dir);
    for d in ["model", "svc", "domain", ".git"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    std::fs::write(dir.join("model/model.rdra"), conformance()).unwrap();
    std::fs::write(dir.join("model/rdra.toml"), ADAPTER_MANIFEST).unwrap();
    std::fs::write(dir.join("go.mod"), "module example.com/shop\n\ngo 1.21\n").unwrap();
    for file in ["service.go", "adapter.go"] {
        std::fs::copy(
            fixtures().join("go_service").join(file),
            dir.join("svc").join(file),
        )
        .unwrap();
    }
    let (code, _, stderr) = outcome(&rdra(
        &dir,
        &[
            "codegen",
            "--lang",
            "go",
            "--conformance",
            "domain/traces.txt",
            "-o",
            "domain/domain.go",
            "model",
        ],
        None,
    ));
    assert_eq!(code, Some(0), "{stderr}");
    let domain = std::fs::read_to_string(dir.join("domain/domain.go")).unwrap();
    std::fs::write(
        dir.join("domain/domain.go"),
        format!("{domain}\n\n// ProjectOwned is the project's own addition.\nfunc ProjectOwned() {{}}\n"),
    )
    .unwrap();
    let (code, _, stderr) = outcome(&rdra(&dir, &["codegen", "model", "--code"], None));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(!stderr.contains("domain.go"), "{stderr}");
    let (code, _, stderr) = outcome(&rdra(&dir, &["check", "model", "--code=fresh"], None));
    assert_eq!(code, Some(0), "{stderr}");

    let (code, stdout, stderr) = outcome(&rdra(&dir, &["check", "model", "--code=run"], None));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stdout.contains("  ran: 11 traces, 21 calls, 11 refused calls, 7 interleaved commits; 32 calls made again by prepare and commit\n"),
        "{stdout}"
    );
    assert!(
        stdout
            .contains("  self-check: 5 of 5 mutant traces failed at their changed row (seed 0); guard-removed mutants: Big\n"),
        "{stdout}"
    );
    for file in ["svc/rdra_adapter_test.go", "modeladapter/modeladapter.go"] {
        let gofmt = Command::new("gofmt")
            .arg("-l")
            .arg(dir.join(file))
            .output()
            .expect("runs gofmt");
        assert!(gofmt.stdout.is_empty(), "gofmt names {file}");
    }

    let (code, _, stderr) = outcome(&rdra(
        &dir,
        &["check", "model", "--code=run"],
        Some("accept-disabled"),
    ));
    assert_eq!(code, Some(1), "{stderr}");
    assert_eq!(errors(&stderr), ["CodeConformance"], "{stderr}");
    assert!(
        stderr.contains("the model refuses `Cancel 101` here, and the service ran it"),
        "{stderr}"
    );

    let (code, _, stderr) = outcome(&rdra(
        &dir,
        &["check", "model", "--code=run"],
        Some("ignore-id"),
    ));
    assert_eq!(code, Some(1), "{stderr}");
    assert_eq!(errors(&stderr), ["CodeMutantSurvived"], "{stderr}");
    assert!(stderr.contains("the absent-row mutant"), "{stderr}");
}
