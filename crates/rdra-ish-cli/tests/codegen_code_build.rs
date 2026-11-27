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
            "after `Pay 101 2026-01-01T00:00:00Z`, the store differs from the model:",
        ),
        (
            "drop-row",
            "model only: Shipment 201 status=packed order_id=101",
        ),
        (
            "accept-disabled",
            "the model refuses `Cancel 101` here, and the service ran it",
        ),
        (
            "unconditional",
            "prepared before `Place 1 101 card s` changed a row it writes",
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
        all.contains("  self-check: 5 of 5 mutant traces failed at their changed row (seed 0); guard-removed mutants: Big"),
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
        all.contains("  self-check: 4 of 5 mutant traces failed at their changed row (seed 0); guard-removed mutants: Big"),
        "{all}"
    );
}

const ADAPTER_MANIFEST: &str = r#"[model]
name = "shop"

[code.rust]
adapter = "../svc/src/model.rs"
traces = "../svc/tests/traces.txt"
conformance_states = 4096
probe = "../svc/tests/rdra_adapter.rs"
service = "svc::adapter::Adapter"
module = "svc::model"
new = "svc::adapter::Adapter::new()"
"#;

/// The black-box model adapter (#439) through `check --code`: the project took the domain code
/// once, renamed its file and edited it, and binds the reference adapter over it. `fresh`
/// compares only the adapter file, the traces and the probe, so the edit is no difference; the
/// run passes and states what it ran; the self-check works through the adapter; and an adapter
/// without prepare and commit passes with its interleavings stated as plain calls.
#[test]
fn a_model_adapter_conforms_whatever_the_domain_code_became() {
    if !tool("cargo", CODEGEN) {
        return;
    }
    let dir = std::env::temp_dir()
        .join(format!("rdra-ish-code-build-{}", std::process::id()))
        .join("adapter");
    let _ = std::fs::remove_dir_all(&dir);
    for d in ["model", "svc/src", "svc/tests", ".git"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    std::fs::copy(
        fixtures().join("conformance.rdra"),
        dir.join("model/order.rdra"),
    )
    .unwrap();
    std::fs::write(dir.join("model/rdra.toml"), ADAPTER_MANIFEST).unwrap();
    std::fs::write(
        dir.join("svc/Cargo.toml"),
        "[package]\nname = \"svc\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    // The domain code, generated once and then the project's own: renamed and edited.
    let out = rdra(
        &dir,
        &[
            "codegen",
            "--lang",
            "rust",
            "--conformance",
            "svc/our-traces.txt",
            "-o",
            "svc/src/our_core.rs",
            "model",
        ],
        "",
    );
    assert!(out.status.success(), "{}", text_slashed(&out));
    let core = std::fs::read_to_string(dir.join("svc/src/our_core.rs")).unwrap();
    std::fs::write(
        dir.join("svc/src/our_core.rs"),
        format!("{core}\n/// The project's own addition.\npub fn project_owned() {{}}\n"),
    )
    .unwrap();
    std::fs::write(
        dir.join("svc/src/lib.rs"),
        "#[allow(dead_code)]\n#[path = \"our_core.rs\"]\npub mod domain;\npub mod service;\npub mod model;\npub mod adapter;\n",
    )
    .unwrap();
    std::fs::write(dir.join("svc/src/service.rs"), service_module()).unwrap();
    std::fs::copy(
        fixtures().join("conformance_adapter.rs"),
        dir.join("svc/src/adapter.rs"),
    )
    .unwrap();
    let out = rdra(&dir, &["codegen", "model", "--code"], "");
    let all = text_slashed(&out);
    assert!(out.status.success(), "{all}");
    for file in [
        "svc/src/model.rs",
        "svc/tests/traces.txt",
        "svc/tests/rdra_adapter.rs",
    ] {
        assert!(all.contains(&format!("wrote {file}")), "{all}");
    }
    assert!(!all.contains("our_core"), "{all}");

    let out = rdra(&dir, &["check", "model", "--code=fresh"], "");
    assert!(out.status.success(), "{}", text_slashed(&out));

    let out = rdra(&dir, &["check", "model", "--code=run"], "");
    let all = text_slashed(&out);
    assert!(out.status.success(), "{all}");
    assert!(
        all.contains("  ran: 11 traces, 21 calls, 11 refused calls, 7 interleaved commits; 32 calls made again by prepare and commit\n"),
        "{all}"
    );
    assert!(
        all.contains("  self-check: 5 of 5 mutant traces failed at their changed row (seed 0); guard-removed mutants: Big"),
        "{all}"
    );

    for (fault, says) in [
        (
            "drop-row",
            "model only: Shipment 201 status=packed order_id=101",
        ),
        (
            "unconditional",
            "prepared before `Place 1 101 card s` changed a row it writes",
        ),
    ] {
        let out = rdra(&dir, &["check", "model", "--code=run"], fault);
        let all = text_slashed(&out);
        assert_eq!(out.status.code(), Some(1), "{fault}: {all}");
        assert!(all.contains("error[CodeConformance]"), "{fault}: {all}");
        assert!(all.contains(says), "{fault}: {all}");
    }

    let out = rdra(&dir, &["check", "model", "--code=run"], "ignore-id");
    let all = text_slashed(&out);
    assert_eq!(out.status.code(), Some(1), "{all}");
    let survived: Vec<&str> = all
        .lines()
        .filter(|l| l.starts_with("error[CodeMutantSurvived]"))
        .collect();
    assert_eq!(survived.len(), 1, "{all}");
    assert!(survived[0].contains("the absent-row mutant"), "{all}");

    let out = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(&dir)
        .args(["check", "model", "--code=run"])
        .env("FAULT", "unconditional")
        .env("SPLIT", "0")
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .output()
        .expect("runs rdra-ish");
    let all = text_slashed(&out);
    assert!(out.status.success(), "{all}");
    assert!(
        all.contains("  ran: 11 traces, 21 calls, 11 refused calls, 0 interleaved commits; 7 interleavings run as plain calls: the adapter has no prepare and commit, so conditional writes are not checked"),
        "{all}"
    );
}

/// The unimplemented port (docs/design/algebraic-codegen.md): a service that marks Cancel
/// `UnimplementedCancelUseCase` in place of implementing it builds, and its run passes with
/// the calls of Cancel listed by use case and a warning, which `--deny-incomplete` makes an
/// error.
#[test]
fn a_contract_marked_unimplemented_builds_and_its_calls_are_reported() {
    if !tool("cargo", CODEGEN) {
        return;
    }
    let dir = project("unimplemented");
    let service = dir.join("svc/src/service.rs");
    cut(&service, "impl CancelUseCase for Service {", "\n}\n");
    let text = std::fs::read_to_string(&service).unwrap();
    std::fs::write(
        &service,
        format!("{text}\nimpl UnimplementedCancelUseCase for Service {{}}\n"),
    )
    .unwrap();
    let out = rdra(&dir, &["check", "model", "--code=run"], "");
    let all = text_slashed(&out);
    assert!(out.status.success(), "{all}");
    assert!(
        all.contains("  unimplemented: 1 use case (Cancel): "),
        "{all}"
    );
    assert!(all.contains("warning[CodeUnimplemented]"), "{all}");
    // The panic of the unimplemented method is the harness's, not printed as the test's.
    assert!(!all.contains("panicked"), "{all}");
    let out = rdra(
        &dir,
        &["check", "model", "--code=run", "--deny-incomplete"],
        "",
    );
    let all = text_slashed(&out);
    assert_eq!(out.status.code(), Some(1), "{all}");
    assert!(all.contains("error[CodeUnimplemented]"), "{all}");
}

/// `[code.rust] serde = true`: the domain code builds against serde with no clippy warning,
/// and fixtures/codegen/serde_roundtrip.rs takes a row, its state, and an event through
/// serde_json and back in the model's wire spelling. The crate resolves its dependencies
/// offline, at the workspace's lock file, from the registry the workspace's own build filled.
/// Without the key no line of the output names serde (the snapshots hold the rest).
#[test]
fn serde_derives_round_trip_a_row() {
    if !tool("cargo", CODEGEN) {
        return;
    }
    let dir = std::env::temp_dir()
        .join(format!("rdra-ish-code-build-{}", std::process::id()))
        .join("serde");
    let _ = std::fs::remove_dir_all(&dir);
    for d in ["model", "svc/src", "svc/tests", ".git"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    std::fs::copy(
        fixtures().join("conformance.rdra"),
        dir.join("model/order.rdra"),
    )
    .unwrap();
    let manifest = "[model]\nname = \"shop\"\n\n[code.rust]\ndomain = \"../svc/src/domain.rs\"\n";
    std::fs::write(dir.join("model/rdra.toml"), manifest).unwrap();
    let out = rdra(&dir, &["codegen", "model", "--code"], "");
    assert!(out.status.success(), "{}", text_slashed(&out));
    let plain = std::fs::read_to_string(dir.join("svc/src/domain.rs")).unwrap();
    assert!(!plain.contains("serde"), "serde without the key");

    std::fs::write(
        dir.join("model/rdra.toml"),
        format!("{manifest}serde = true\n"),
    )
    .unwrap();
    std::fs::write(
        dir.join("svc/Cargo.toml"),
        "[package]\nname = \"svc\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\nserde_json = \"1\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"),
        dir.join("svc/Cargo.lock"),
    )
    .unwrap();
    std::fs::write(
        dir.join("svc/src/lib.rs"),
        "//! The generated domain code with serde.\n#[allow(dead_code)]\npub mod domain;\n",
    )
    .unwrap();
    std::fs::copy(
        fixtures().join("serde_roundtrip.rs"),
        dir.join("svc/tests/serde_roundtrip.rs"),
    )
    .unwrap();
    let out = rdra(&dir, &["codegen", "model", "--code"], "");
    assert!(out.status.success(), "{}", text_slashed(&out));
    let with = std::fs::read_to_string(dir.join("svc/src/domain.rs")).unwrap();
    assert!(
        with.contains("::serde::Deserialize"),
        "the key derives serde"
    );

    let cargo = |args: &[&str]| {
        Command::new("cargo")
            .current_dir(dir.join("svc"))
            .args(args)
            .env("CARGO_TARGET_DIR", dir.join("target"))
            .output()
            .expect("runs cargo")
    };
    let out = cargo(&[
        "clippy",
        "--offline",
        "--",
        "-D",
        "warnings",
        "-D",
        "missing_docs",
    ]);
    assert!(out.status.success(), "clippy: {}", text_slashed(&out));
    let out = cargo(&["test", "--offline"]);
    let all = text_slashed(&out);
    assert!(out.status.success(), "{all}");
    assert!(all.contains("test result: ok. 4 passed"), "{all}");
}
