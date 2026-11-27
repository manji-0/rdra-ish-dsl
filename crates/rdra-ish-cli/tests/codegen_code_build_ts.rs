//! `check --code=build` and `--code=run` on TypeScript (docs/design/code-check.md, P3 and P4):
//! the reference service `fixtures/codegen/ts_service/order_service.ts` around the code
//! `codegen --code` writes for conformance.rdra, with one omission planted at a time. Each gives
//! its named diagnostic, mapped from tsc's error back to the use case and its model location;
//! each of the four faults of codegen_conformance.rs fails `--code=run` through the probe,
//! which Node runs as it is. Needs tsc and node; skipped without them unless
//! `RDRA_REQUIRE_CODEGEN_TOOLS=1`.

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
use tools::{tools, CODEGEN};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codegen")
}

const MANIFEST: &str = r#"[model]
name = "shop"

[code.ts]
domain = "../svc/src/domain.ts"
traces = "../svc/src/traces.txt"
conformance_states = 4096
probe = "../svc/src/rdra_contract.ts"
service = "../svc/src/order_service.ts#OrderService"
"#;

/// The project's own tsconfig, which `tsconfig.rdra.json` extends.
const TSCONFIG: &str = r#"{
  "compilerOptions": {
    "strict": true,
    "target": "es2022",
    "module": "nodenext",
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "exactOptionalPropertyTypes": true,
    "noUncheckedIndexedAccess": true
  }
}
"#;

/// A project: `model/` with the fixture and rdra.toml, `svc/` a TypeScript project, its
/// generated files written by `codegen --code`.
fn project(name: &str, manifest: &str) -> PathBuf {
    project_of(
        name,
        manifest,
        "conformance.rdra",
        "ts_service/order_service.ts",
    )
}

/// [`project`] of the model `model` and the service `service`, both under fixtures/codegen.
fn project_of(name: &str, manifest: &str, model: &str, service: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("rdra-ish-code-build-ts-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    for d in ["model", "svc/src", ".git"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    std::fs::copy(fixtures().join(model), dir.join("model/order.rdra")).unwrap();
    std::fs::write(dir.join("model/rdra.toml"), manifest).unwrap();
    std::fs::write(dir.join("svc/tsconfig.json"), TSCONFIG).unwrap();
    std::fs::copy(
        fixtures().join(service),
        dir.join("svc/src/order_service.ts"),
    )
    .unwrap();
    let out = rdra(&dir, &["codegen", "model", "--code"], "");
    assert!(out.status.success(), "{}", text_slashed(&out));
    for file in ["svc/src/rdra_contract.ts", "svc/tsconfig.rdra.json"] {
        assert!(
            text_slashed(&out).contains(&format!("wrote {file}")),
            "{}",
            text_slashed(&out)
        );
    }
    dir
}

fn rdra(dir: &Path, args: &[&str], fault: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(dir)
        .args(args)
        .env("FAULT", fault)
        .output()
        .expect("runs rdra-ish")
}

/// The probe and tsconfig.rdra.json `codegen --code` writes.
#[test]
fn the_probe_names_each_use_case_and_tsconfig_holds_only_it() {
    if !tools(&["tsc", "node"], CODEGEN) {
        return;
    }
    let dir = project("files", MANIFEST);
    let probe = std::fs::read_to_string(dir.join("svc/src/rdra_contract.ts")).unwrap();
    for line in [
        "import * as domain from \"./domain.ts\";",
        "import * as service from \"./order_service.ts\";",
        "type Service = service.OrderService;",
        "export const _Ship = (it: Service): domain.ShipUseCase => it; // usecase Ship",
        "domain.conform((): Service => new service.OrderService(), traces).then(",
        "readFileSync(process.env[\"RDRA_ISH_TRACES\"] || \"src/traces.txt\", \"utf8\")",
    ] {
        assert!(probe.contains(line), "no {line:?} in\n{probe}");
    }
    let config = std::fs::read_to_string(dir.join("svc/tsconfig.rdra.json")).unwrap();
    for line in [
        "\"extends\": \"./tsconfig.json\",",
        "\"files\": [\"src/rdra_contract.ts\"],",
        "\"include\": []",
    ] {
        assert!(config.contains(line), "no {line:?} in\n{config}");
    }
    let fresh = rdra(&dir, &["check", "model", "--code"], "");
    assert!(fresh.status.success(), "{}", text_slashed(&fresh));
    // A probe without the harness: type-only imports, no test.
    let bare = MANIFEST.replace(
        "traces = \"../svc/src/traces.txt\"\nconformance_states = 4096\n",
        "",
    );
    let dir = project("bare", &bare);
    let probe = std::fs::read_to_string(dir.join("svc/src/rdra_contract.ts")).unwrap();
    assert!(
        probe.contains("import type * as domain from \"./domain.ts\";"),
        "{probe}"
    );
    assert!(!probe.contains("conform"), "{probe}");
}

/// What `check --code=build` says of each planted omission: the diagnostic, and a line of
/// its `model:` or `code:`.
#[test]
fn each_omission_is_a_named_diagnostic() {
    if !tools(&["tsc", "node"], CODEGEN) {
        return;
    }
    let dir = project("omissions", MANIFEST);
    let out = rdra(&dir, &["check", "model", "--code=build"], "");
    assert!(
        out.status.success(),
        "the reference service: {}",
        text_slashed(&out)
    );
    assert!(text_slashed(&out).contains("OK: code: what the model requires, at --code=build"));

    let service = dir.join("svc/src/order_service.ts");
    let original = std::fs::read_to_string(&service).unwrap();
    type Plant = fn(&Path);
    let plants: [(&str, Plant, &[&str]); 5] = [
        (
            "Ship not implemented",
            |p| cut(p, "  prepareShip = async (", "    return done;\n  };\n"),
            &[
                "error[CodeMissingUseCase_Ship]: use case Ship has no implementation in",
                "model: model/order.rdra:22:1  usecase Ship \"Ship\"",
                "TS2739: Type 'OrderService' is missing the following properties from type 'ShipUseCase': prepareShip, commitShip",
                "fix:   give OrderService the members of ShipUseCase (prepareShip, commitShip)",
            ],
        ),
        (
            "commitCancel missing",
            |p| cut(p, "  commitCancel = async (", "    return done;\n  };\n"),
            &[
                "error[CodeIncomplete_Cancel]: use case Cancel is implemented in part in",
                "TS2741: Property 'commitCancel' is missing in type 'OrderService' but required in type 'CancelUseCase'.",
            ],
        ),
        (
            "Cancel takes a narrower argument than the model's",
            |p| {
                edit(
                    p,
                    "prepareCancel = async (id: d.OrderId)",
                    "prepareCancel = async (id: d.OrderId & { readonly checked: true })",
                )
            },
            &[
                "error[CodeSignature_Cancel]: use case Cancel takes other inputs than the model's",
                "model: model/order.rdra:24:1  usecase Cancel \"C\"",
                "TS2322: Type 'OrderService' is not assignable to type 'CancelUseCase'.",
                "fix:   take and return the types CancelUseCase declares in the generated code",
            ],
        ),
        (
            "Place returns another state than the model's",
            |p| edit(p, "  preparePlace = async (", "  preparePlace = async (\n    ...rest: [d.CustomerId, d.OrderId, d.Method, string]\n  ): Promise<d.Result<d.PayOutcome, d.PlaceError>> => ({ ok: false, error: { kind: \"NotEnabled\", usecase: String(rest.length) } });\n  preparePlace_ = async ("),
            &["error[CodeSignature_Place]", "TS2322"],
        ),
        (
            "a typo in a member the contract names",
            |p| edit(p, "  commitSend = async (", "  commitSendd = async ("),
            &["error[CodeIncomplete_Send]", "TS2741", "'commitSend'"],
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

/// A service with no member of any contract: each use case is not implemented, whether tsc
/// lists every missing member (TS2739, two members) or four and "2 more" (TS2740, Notify's six).
#[test]
fn a_contract_missing_whole_is_not_implemented_at_any_size() {
    if !tools(&["tsc", "node"], CODEGEN) {
        return;
    }
    let manifest = MANIFEST
        .replace(
            "traces = \"../svc/src/traces.txt\"\nconformance_states = 4096\n",
            "",
        )
        .replace("#OrderService", "#Nothing");
    let dir = project_of(
        "nothing",
        &manifest,
        "ts_service/notify.rdra",
        "ts_service/nothing.ts",
    );
    let out = rdra(&dir, &["check", "model", "--code=build"], "");
    let all = text_slashed(&out);
    assert_eq!(out.status.code(), Some(1), "{all}");
    for uc in ["Place", "Pay", "Ship", "Finish", "Notify"] {
        assert!(
            all.contains(&format!(
                "error[CodeMissingUseCase_{uc}]: use case {uc} has no implementation in"
            )),
            "{uc}: {all}"
        );
    }
    assert!(all.contains("TS2740"), "{all}");
    assert!(all.contains("and 2 more"), "{all}");
    assert!(!all.contains("CodeIncomplete"), "{all}");
    assert!(!all.contains("CodeUnmapped"), "{all}");
}

/// The four faults of codegen_conformance.rs, each through `--code=run`.
#[test]
fn each_fault_fails_the_run() {
    if !tools(&["tsc", "node"], CODEGEN) {
        return;
    }
    let dir = project("faults", MANIFEST);
    let out = rdra(&dir, &["check", "model", "--code=run"], "");
    assert!(
        out.status.success(),
        "the reference service: {}",
        text_slashed(&out)
    );
    assert!(
        text_slashed(&out)
            .contains("rdra-ish conformance: ok 9 traces, 19 calls, 9 refusals, 7 interleavings")
            || text_slashed(&out).contains("OK: code: what the model requires, at --code=run"),
        "{}",
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

/// The self-check of `--code=run` (#439): the reference service fails every mutant trace at
/// its changed row; a snapshot that is what was seeded fails the real traces; a service that
/// runs a call on whatever row it finds passes them and is caught by the absent-row mutant.
#[test]
fn the_self_check_catches_a_service_that_ignores_the_id() {
    if !tools(&["tsc", "node"], CODEGEN) {
        return;
    }
    let dir = project("self-check", MANIFEST);
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
    assert!(!all.contains("self-check:"), "{all}");

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
        all.contains("  self-check: 3 of 4 mutant traces failed at their changed row (seed 0)"),
        "{all}"
    );
}

const ADAPTER_MANIFEST: &str = r#"[model]
name = "shop"

[code.ts]
adapter = "../svc/src/model.ts"
traces = "../svc/src/traces.txt"
conformance_states = 4096
probe = "../svc/src/rdra_adapter.ts"
service = "../svc/src/adapter.ts#Adapter"
new = "service.adapter()"
"#;

/// The black-box model adapter (#439) through `check --code`: the domain code is the
/// project's own (generated once, then edited), and the reference adapter binds the service to
/// the model. `fresh` reads no domain file, `build` type-checks the probe, the run states what
/// it ran, and the self-check works through the adapter.
#[test]
fn a_model_adapter_conforms_whatever_the_domain_code_became() {
    if !tools(&["tsc", "node"], CODEGEN) {
        return;
    }
    let dir = std::env::temp_dir()
        .join(format!("rdra-ish-code-build-ts-{}", std::process::id()))
        .join("adapter");
    let _ = std::fs::remove_dir_all(&dir);
    for d in ["model", "svc/src", ".git"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    std::fs::copy(
        fixtures().join("conformance.rdra"),
        dir.join("model/order.rdra"),
    )
    .unwrap();
    std::fs::write(dir.join("model/rdra.toml"), ADAPTER_MANIFEST).unwrap();
    std::fs::write(dir.join("svc/tsconfig.json"), TSCONFIG).unwrap();
    for file in ["order_service.ts", "adapter.ts"] {
        std::fs::copy(
            fixtures().join("ts_service").join(file),
            dir.join("svc/src").join(file),
        )
        .unwrap();
    }
    let out = rdra(
        &dir,
        &[
            "codegen",
            "--lang",
            "ts",
            "--conformance",
            "svc/our-traces.txt",
            "-o",
            "svc/src/domain.ts",
            "model",
        ],
        "",
    );
    assert!(out.status.success(), "{}", text_slashed(&out));
    let domain = std::fs::read_to_string(dir.join("svc/src/domain.ts")).unwrap();
    std::fs::write(
        dir.join("svc/src/domain.ts"),
        format!(
            "{domain}\n\n/** The project's own addition. */\nexport const projectOwned = true;\n"
        ),
    )
    .unwrap();
    let out = rdra(&dir, &["codegen", "model", "--code"], "");
    let all = text_slashed(&out);
    assert!(out.status.success(), "{all}");
    assert!(!all.contains("domain.ts"), "{all}");

    let out = rdra(&dir, &["check", "model", "--code=build"], "");
    assert!(out.status.success(), "{}", text_slashed(&out));

    let out = rdra(&dir, &["check", "model", "--code=run"], "");
    let all = text_slashed(&out);
    assert!(out.status.success(), "{all}");
    assert!(
        all.contains("  ran: 9 traces, 19 calls, 9 refused calls, 7 interleaved commits\n"),
        "{all}"
    );
    assert!(
        all.contains("  self-check: 4 of 4 mutant traces failed at their changed row (seed 0)"),
        "{all}"
    );

    let out = rdra(&dir, &["check", "model", "--code=run"], "skip-core");
    let all = text_slashed(&out);
    assert_eq!(out.status.code(), Some(1), "{all}");
    assert!(all.contains("error[CodeConformance]"), "{all}");
    assert!(
        all.contains("after `Pay 1 2026-01-01T00:00:00Z`, the store differs from the model:"),
        "{all}"
    );

    let out = rdra(&dir, &["check", "model", "--code=run"], "ignore-id");
    let all = text_slashed(&out);
    assert_eq!(out.status.code(), Some(1), "{all}");
    let survived: Vec<&str> = all
        .lines()
        .filter(|l| l.starts_with("error[CodeMutantSurvived]"))
        .collect();
    assert_eq!(survived.len(), 1, "{all}");
    assert!(survived[0].contains("the absent-row mutant"), "{all}");
}
