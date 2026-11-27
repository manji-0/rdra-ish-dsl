//! `check --code=build` and `--code=run` on Rust (docs/design/code-check.md, P2 and P4): a
//! reference service crate around the code `codegen --code` writes for conformance.rdra, with
//! one omission planted at a time. Each gives its named diagnostic, mapped from rustc's error
//! back to the use case and its model location; each of the four faults of
//! codegen_conformance.rs fails `--code=run` through the probe's conformance test. Needs
//! cargo; skipped without it unless `RDRA_REQUIRE_CODEGEN_TOOLS=1`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "common/edit.rs"]
mod edit;
#[path = "common/output.rs"]
mod output;
use edit::{cut, edit};
use output::text_slashed;

#[path = "common/tools.rs"]
mod tools;
use tools::{tool, CODEGEN};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codegen")
}

const MANIFEST: &str = r#"[model]
name = "shop"

[code.rust]
domain = "../svc/src/domain.rs"
traces = "../svc/tests/traces.txt"
conformance_states = 4096
probe = "../svc/tests/rdra_contract.rs"
service = "svc::service::Service"
module = "svc::domain"
new = "svc::service::Service::new()"
"#;

/// The reference service of codegen_conformance.rs as a module of a library crate: its
/// `main` dropped, and `Service::new()` reading the fault from `FAULT`.
fn service_module() -> String {
    let text = std::fs::read_to_string(fixtures().join("conformance_service.rs")).unwrap();
    let text = text
        .replace(
            "#[path = \"domain.rs\"]\n#[allow(dead_code)] // the service uses part of the module\nmod domain;\nuse domain::*;",
            "use crate::domain::*;",
        )
        .replace("struct Service {", "pub struct Service {");
    let main = text.find("fn main()").expect("the service has a main");
    format!(
        "{}\nimpl Service {{\n    /// A service with the fault `FAULT` names planted.\n    pub fn new() -> Self {{\n        Service {{ fault: std::env::var(\"FAULT\").unwrap_or_default(), ..Service::default() }}\n    }}\n}}\n",
        &text[..main]
    )
}

/// A project: `model/` with the fixture and rdra.toml, `svc/` a crate, its generated files
/// written by `codegen --code`.
fn project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("rdra-ish-code-build-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    for d in ["model", "svc/src", "svc/tests", ".git"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    std::fs::copy(
        fixtures().join("conformance.rdra"),
        dir.join("model/order.rdra"),
    )
    .unwrap();
    std::fs::write(dir.join("model/rdra.toml"), MANIFEST).unwrap();
    std::fs::write(
        dir.join("svc/Cargo.toml"),
        "[package]\nname = \"svc\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("svc/src/lib.rs"),
        "#[allow(dead_code)]\npub mod domain;\npub mod service;\n",
    )
    .unwrap();
    std::fs::write(dir.join("svc/src/service.rs"), service_module()).unwrap();
    let out = rdra(&dir, &["codegen", "model", "--code"], "");
    assert!(out.status.success(), "{}", text_slashed(&out));
    assert!(
        text_slashed(&out).contains("wrote svc/tests/rdra_contract.rs"),
        "{}",
        text_slashed(&out)
    );
    dir
}

fn rdra(dir: &Path, args: &[&str], fault: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(dir)
        .args(args)
        .env("FAULT", fault)
        // A target directory per project: cargo names a path package's outputs without its
        // path, so two projects of one package name would overwrite each other's.
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .output()
        .expect("runs rdra-ish")
}

/// What `check --code=build` says of each planted omission: the diagnostic, and a line of
/// its `model:` or `code:`.
#[test]
fn each_omission_is_a_named_diagnostic() {
    if !tool("cargo", CODEGEN) {
        return;
    }
    let dir = project("omissions");
    let out = rdra(&dir, &["check", "model", "--code=build"], "");
    assert!(
        out.status.success(),
        "the reference service: {}",
        text_slashed(&out)
    );
    assert!(text_slashed(&out).contains("OK: code: what the model requires, at --code=build"));

    let service = dir.join("svc/src/service.rs");
    let original = std::fs::read_to_string(&service).unwrap();
    type Plant = fn(&Path);
    let plants: [(&str, Plant, &[&str]); 4] = [
        (
            "Ship not implemented",
            |p| cut(p, "impl ShipUseCase for Service {", "\n}\n"),
            &[
                "error[CodeMissingUseCase_Ship]: use case Ship has no implementation in svc::service::Service",
                "model: model/order.rdra:22:1  usecase Ship \"Ship\"",
                // The library's `impl ConformancePort` needs the contract, so rustc stops
                // there before the probe; a service with no harness impl fails on the probe.
                "E0277: the trait bound `Service: ShipUseCase` is not satisfied",
                "fix:   impl ShipUseCase for svc::service::Service (prepare_ship, commit_ship)",
            ],
        ),
        (
            "Ship's impl empty",
            |p| {
                cut(p, "impl ShipUseCase for Service {", "\n}\n");
                let text = std::fs::read_to_string(p).unwrap();
                std::fs::write(p, format!("{text}\nimpl ShipUseCase for Service {{}}\n")).unwrap();
            },
            &["error[CodeMissingUseCase_Ship]", "E0046"],
        ),
        (
            "commit_cancel missing",
            |p| {
                cut(
                    p,
                    "    fn commit_cancel(",
                    "        Ok(())\n    }\n",
                )
            },
            &["error[CodeIncomplete_Cancel]", "E0046"],
        ),
        (
            "Pay takes a bare integer for OrderId",
            |p| {
                edit(p, "fn prepare_pay(&mut self, order: OrderId,", "fn prepare_pay(&mut self, order: i64,");
                edit(p, "self.order(order, \"Pay\")", "self.order(OrderId::from(order), \"Pay\")");
            },
            &[
                "error[CodeSignature_Pay]: use case Pay takes other inputs than the model's",
                "E0053",
                "prepare_pay",
            ],
        ),
    ];
    for (what, plant, says) in plants {
        std::fs::write(&service, &original).unwrap();
        plant(&service);
        let out = rdra(&dir, &["check", "model", "--code=build"], "");
        let all = text_slashed(&out);
        assert_eq!(out.status.code(), Some(1), "{what}: {all}");
        for line in says {
            assert!(all.contains(line), "{what}: no {line:?} in\n{all}");
        }
        assert!(!all.contains("CodeUnmapped"), "{what}: {all}");
    }
}

/// The four faults of codegen_conformance.rs, each through `--code=run`.
#[test]
fn each_fault_fails_the_run() {
    if !tool("cargo", CODEGEN) {
        return;
    }
    let dir = project("faults");
    let out = rdra(&dir, &["check", "model", "--code=run"], "");
    assert!(
        out.status.success(),
        "the reference service: {}",
        text_slashed(&out)
    );
    for (fault, says) in [
        (
            "skip-core",
            "after `Pay 1 2026-01-01T00:00:00Z`, the store differs from the model:",
        ),
        (
            "drop-row",
            "model only: Shipment 1 status=packed order_id=1",
        ),
        (
            "accept-disabled",
            "the model refuses `Cancel 1` here, and the service ran it",
        ),
        (
            "unconditional",
            "prepared before `Place 1 1 card s` changed a row it writes",
        ),
    ] {
        let out = rdra(&dir, &["check", "model", "--code=run"], fault);
        let all = text_slashed(&out);
        assert_eq!(out.status.code(), Some(1), "{fault}: {all}");
        assert!(all.contains("error[CodeConformance]"), "{fault}: {all}");
        assert!(all.contains(says), "{fault}: {all}");
    }
}

/// The self-check of `--code=run` (#439) on a real harness: the reference service fails every
/// mutant trace at its changed row. Of two lying services, the one whose snapshot is what was
/// seeded fails the real traces already, and the one that runs a call on whatever row it
/// finds passes them (every id is 1 at one row per entity) and is caught by the absent-row
/// mutant alone.
#[test]
fn the_self_check_catches_a_service_that_ignores_the_id() {
    if !tool("cargo", CODEGEN) {
        return;
    }
    let dir = project("self-check");
    let out = rdra(&dir, &["check", "model", "--code=run"], "");
    let all = text_slashed(&out);
    assert!(out.status.success(), "{all}");
    assert!(
        all.contains("  self-check: 4 of 4 mutant traces failed at their changed row (seed 0)"),
        "{all}"
    );

    let out = rdra(&dir, &["check", "model", "--code=run"], "constant-observe");
    let all = text_slashed(&out);
    assert_eq!(out.status.code(), Some(1), "{all}");
    assert!(all.contains("error[CodeConformance]"), "{all}");
    assert!(
        !all.contains("self-check:"),
        "the real traces failed first: {all}"
    );

    // The variable that points the probe at substitute traces is cleared for the real run:
    // one left in the environment never stands in for the real traces.
    let out = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(&dir)
        .args(["check", "model", "--code=run", "--mutants", "0"])
        .env("RDRA_ISH_TRACES", dir.join("no-such-traces.txt"))
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .output()
        .expect("runs rdra-ish");
    assert!(out.status.success(), "{}", text_slashed(&out));

    let out = rdra(&dir, &["check", "model", "--code=run"], "ignore-id");
    let all = text_slashed(&out);
    assert_eq!(out.status.code(), Some(1), "{all}");
    assert!(!all.contains("error[CodeConformance]"), "{all}");
    let survived: Vec<&str> = all
        .lines()
        .filter(|l| l.starts_with("error[CodeMutantSurvived]"))
        .collect();
    assert_eq!(survived.len(), 1, "{all}");
    assert!(survived[0].contains("the absent-row mutant"), "{all}");
    assert!(
        all.contains("fix:   the service must look the row up by the id it is given"),
        "{all}"
    );
    assert!(
        all.contains("  self-check: 3 of 4 mutant traces failed at their changed row (seed 0)"),
        "{all}"
    );
}
