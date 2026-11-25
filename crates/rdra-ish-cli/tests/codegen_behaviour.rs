//! The code `codegen` prints behaves as the model says, run in each language: on
//! `tests/fixtures/codegen/order.rdra`, a driver per language converts rows to states and
//! back, refuses a row that breaks its state's refinement, dispatches the use cases by state
//! (`NotEnabled`, `Rejected`, the writes, the raised event), and runs the rule checks. Code
//! outside the generated module or package cannot build a state or change one of its fields
//! (#331): the compiler refuses it. In Go, a state the model gains breaks every state
//! visitor that lacks its method (#363, P7). A compiler or runtime missing from PATH skips its language, except under
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

/// The same in Go, outside the generated package, and what the compiler says. A state type is
/// an interface whose one implementation is unexported, so not even its zero value is a state
/// (#363, P7): the zero value of the interface is nil, which every function refuses.
const GO_FORGERIES: [(&str, &str); 4] = [
    (
        "o := domain.OrderPaid{}; _ = o",
        "invalid composite literal type domain.OrderPaid",
    ),
    (
        "var o domain.OrderPaid; o.qty = 7; _ = o",
        "o.qty undefined",
    ),
    // `go vet` says `undefined: domain.orderPaidState` or `name orderPaidState not exported`.
    ("o := domain.orderPaidState{}; _ = o", "orderPaidState"),
    (
        "var o domain.Order = struct{ domain.OrderKind }{}; _ = o",
        "does not implement domain.Order",
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

/// A visitor outside the package, over the three states of `order.rdra`'s `Order`.
const GO_VISITOR: &str = r#"package domain_test

import (
	"errors"
	"testing"

	"example.com/domain"
)

type label struct{}

func (label) Pending(domain.OrderPending) string     { return "pending" }
func (label) Paid(domain.OrderPaid) string           { return "paid" }
func (label) Cancelled(domain.OrderCancelled) string { return "cancelled" }

func TestVisitor(t *testing.T) {
	row := domain.OrderRow{Id: 1, Status: domain.OrderStatusPending, Method: domain.MethodCard, Qty: 1, Note: "n", CustomerId: 7}
	order, err := domain.OrderFromRow(row)
	if err != nil {
		t.Fatal(err)
	}
	if got, err := domain.MatchOrder[string](order, label{}); err != nil || got != "pending" {
		t.Fatalf("MatchOrder: %q %v", got, err)
	}
	if _, err := domain.MatchOrder[string](nil, label{}); !errors.Is(err, domain.ErrInvalidArgument) {
		t.Fatalf("MatchOrder refuses a nil state: %v", err)
	}
}
"#;

/// P7 of docs/design/code-check.md: Go's type switch says nothing of a state it misses, so the
/// Go output has a visitor per state type. A visitor over the states of `order.rdra` runs; once
/// the model gains a state, the same visitor no longer compiles, and the compiler names the
/// method it lacks.
#[test]
fn go_visitor_fails_to_compile_on_a_new_state() {
    if !available("go", "version") {
        return;
    }
    let dir = scratch("go-visitor");
    generate("go", &dir.join("domain.go"));
    std::fs::write(dir.join("go.mod"), "module example.com/domain\n\ngo 1.21\n").unwrap();
    std::fs::write(dir.join("visitor_test.go"), GO_VISITOR).unwrap();
    run(
        Command::new("go").args(["test", "./..."]).current_dir(&dir),
        "go test on the visitor",
    );

    let model = std::fs::read_to_string(fixture().join("order.rdra")).unwrap();
    let grown = model.replace(
        "Enum(pending, paid, cancelled)",
        "Enum(pending, paid, cancelled, refunded)",
    );
    assert_ne!(grown, model, "the fixture's status column moved");
    std::fs::write(dir.join("order.rdra"), grown).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .args(["codegen", "--lang", "go", "-o"])
        .arg(dir.join("domain.go"))
        .arg(dir.join("order.rdra"))
        .output()
        .expect("runs rdra-ish");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new("go")
        .args(["vet", "./..."])
        .current_dir(&dir)
        .output()
        .expect("runs go");
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "the visitor compiles with a state missing"
    );
    assert!(
        err.contains("does not implement domain.OrderVisitor[string] (missing method Refunded)"),
        "{err}"
    );
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
