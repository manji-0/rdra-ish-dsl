//! The code `codegen` prints behaves as the model says, run in each language: on
//! `tests/fixtures/codegen/order.rdra`, a driver per language converts rows to states and
//! back, refuses a row that breaks its state's refinement, dispatches the use cases by state
//! (`NotEnabled`, `Rejected`, the writes, the raised event), and runs the rule checks. Code
//! outside the generated module or package cannot build a state or change one of its fields
//! (#331): the compiler refuses it. A
//! compiler or runtime missing from PATH skips its language, except under
//! `RDRA_REQUIRE_CODEGEN_TOOLS=1`, which CI sets in the dev shell.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codegen")
}

fn available(tool: &str, arg: &str) -> bool {
    let found = Command::new(tool).arg(arg).output().is_ok();
    if !found {
        assert!(
            std::env::var("RDRA_REQUIRE_CODEGEN_TOOLS").as_deref() != Ok("1"),
            "{tool} is not on PATH, and RDRA_REQUIRE_CODEGEN_TOOLS=1 requires it; the dev shell provides it"
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
            .arg(dir.join(format!("driver{}", std::env::consts::EXE_SUFFIX)))
            .arg(dir.join("driver.rs")),
        "rustc on the driver",
    );
    let driver = dir.join(format!("driver{}", std::env::consts::EXE_SUFFIX));
    let out = run(&mut Command::new(driver), "the Rust driver");
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

/// Code outside the generated module that builds a state, writes a field of one, or makes a
/// scalar or an id without its parser, and what rustc says.
const RUST_FORGERIES: [(&str, &str); 4] = [
    (
        "let o = OrderPaid { id: OrderId::from(1), paid_at: \"2026-01-01T00:00:00Z\".parse().unwrap(), method: Method::Card, qty: 9, note: String::new(), customer_id: CustomerId::from(1) }; let _ = o;",
        "E0451",
    ),
    (
        "let mut o = Order::from_row(OrderRow { id: OrderId::from(1), status: OrderStatus::Pending, paid_at: None, method: Method::Card, qty: 1, note: String::new(), customer_id: CustomerId::from(1) }).unwrap(); if let Order::Pending(p) = &mut o { p.qty = 7; }",
        "E0616",
    ),
    ("let t = DateTime(String::from(\"t\")); let _ = t;", "E0423"),
    ("let id = OrderId(1); let _ = id;", "E0423"),
];

/// The same in Go, outside the generated package, and what the compiler says.
const GO_FORGERIES: [(&str, &str); 2] = [
    ("o := domain.OrderPaid{Qty: 9}; _ = o", "unknown field Qty"),
    (
        "var o domain.OrderPaid; o.qty = 7; _ = o",
        "o.qty undefined",
    ),
];

#[test]
fn rust_states_are_not_built_outside() {
    let dir = scratch("rust-forge");
    generate("rust", &dir.join("domain.rs"));
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    for (body, code) in RUST_FORGERIES {
        std::fs::write(
            dir.join("forge.rs"),
            format!("#[path = \"domain.rs\"]\nmod domain;\nuse domain::*;\nfn main() {{\n    {body}\n}}\n"),
        )
        .unwrap();
        let output = Command::new(&rustc)
            .args(["--edition", "2021", "-o"])
            .arg(dir.join("forge"))
            .arg(dir.join("forge.rs"))
            .output()
            .expect("runs rustc");
        let err = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "rustc builds {body}");
        assert!(err.contains(code), "{body}: {err}");
    }
}

#[test]
fn go_states_are_not_built_outside() {
    if !available("go", "version") {
        return;
    }
    let dir = scratch("go-forge");
    generate("go", &dir.join("domain.go"));
    std::fs::write(dir.join("go.mod"), "module example.com/domain\n\ngo 1.21\n").unwrap();
    for (body, says) in GO_FORGERIES {
        std::fs::write(
            dir.join("forge_test.go"),
            format!("package domain_test\n\nimport (\n\t\"testing\"\n\n\t\"example.com/domain\"\n)\n\nfunc TestForge(t *testing.T) {{\n\t{body}\n}}\n"),
        )
        .unwrap();
        let output = Command::new("go")
            .args(["vet", "./..."])
            .current_dir(&dir)
            .output()
            .expect("runs go");
        let err = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "go builds {body}");
        assert!(err.contains(says), "{body}: {err}");
    }
}

/// Code that calls a use case of oidc-auth, and what rustc says (#337): `DenyDevice` is
/// performed by EndUser only, so its prepare takes a `DenyDeviceCaller`, which only an
/// `EndUserCapability` makes.
const RUST_CAPABILITY_CALLS: [(&str, Option<&str>); 3] = [
    (
        "fn f<S: DenyDeviceUseCase>(s: &mut S, d: DeviceAuthorizationId) { let _ = s.prepare_deny_device(DenyDeviceCaller::from(EndUserCapability::issue()), d); }",
        None,
    ),
    (
        "fn f<S: DenyDeviceUseCase>(s: &mut S, d: DeviceAuthorizationId) { let _ = s.prepare_deny_device(d); }",
        Some("E0061"),
    ),
    (
        "fn f<S: DenyDeviceUseCase>(s: &mut S, d: DeviceAuthorizationId) { let _ = s.prepare_deny_device(DenyDeviceCaller::RelyingParty(RelyingPartyCapability::issue()), d); }",
        Some("E0599"),
    ),
];

#[test]
fn rust_a_use_case_takes_the_capability_of_an_actor_that_performs_it() {
    let dir = scratch("rust-capability");
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(fixture().join("../../../../.."))
        .args(["codegen", "samples/oidc-auth", "--lang", "rust", "-o"])
        .arg(dir.join("domain.rs"))
        .output()
        .expect("runs rdra-ish");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    for (body, code) in RUST_CAPABILITY_CALLS {
        std::fs::write(
            dir.join("call.rs"),
            format!("#[path = \"domain.rs\"]\n#[allow(dead_code)]\nmod domain;\nuse domain::*;\n#[allow(dead_code)]\n{body}\nfn main() {{}}\n"),
        )
        .unwrap();
        let output = Command::new(&rustc)
            .args(["--edition", "2021", "-o"])
            .arg(dir.join("call"))
            .arg(dir.join("call.rs"))
            .output()
            .expect("runs rustc");
        let err = String::from_utf8_lossy(&output.stderr);
        match code {
            None => assert!(output.status.success(), "{body}: {err}"),
            Some(code) => {
                assert!(!output.status.success(), "rustc builds {body}");
                assert!(err.contains(code), "{body}: {err}");
            }
        }
    }
}
