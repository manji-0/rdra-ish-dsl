//! A product that binds other models' published surfaces: every reference is checked, every
//! mistake has a named diagnostic, and no output names an owner's internals.

mod common;

use clap::ValueEnum;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[allow(dead_code)]
#[path = "../src/cli.rs"]
mod cli;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    dunce::canonicalize(repo().join("tests/fixtures/xsys").join(name)).unwrap()
}

fn rdra_ish(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(cwd)
        .env("RUST_BACKTRACE", "0")
        .args(args)
        .output()
        .expect("rdra-ish runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// The xsys fixtures copied under `dir`, so a test can edit a model without touching the repo.
fn xsys_copy(test: &str) -> PathBuf {
    let dir = scratch(test);
    for model in [
        "auth-model",
        "payments-model",
        "shop-model",
        "product-mistakes",
    ] {
        copy_dir(&fixture(model), &dir.join(model));
    }
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    dir
}

/// Ids of the owners' internals that no output of the shop may name.
const SENTINELS: &[&str] = &[
    "Session\n",
    "Session,",
    "Session\"",
    "Session ",
    "/sessions",
    "Revoke",
    "BucSignIn",
    "Charge",
    "/captures",
    "RefundPayment",
    "BucSettle",
    "Merchant",
];

fn leaks(text: &str) -> Vec<&'static str> {
    SENTINELS
        .iter()
        .copied()
        .filter(|s| text.contains(s))
        .collect()
}

fn kind_names<E: ValueEnum>() -> Vec<String> {
    E::value_variants()
        .iter()
        .filter_map(|v| v.to_possible_value())
        .map(|v| v.get_name().to_string())
        .collect()
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if dir.is_file() {
        return vec![dir.to_path_buf()];
    }
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(files_under(&path));
        } else {
            files.push(path);
        }
    }
    files
}

#[test]
fn every_output_kind_of_the_shop_names_no_internal_of_a_dependency() {
    let out = scratch("cross_model_leak");
    let shop = fixture("shop-model");
    let shop = shop.to_str().unwrap();
    let mut runs: Vec<(String, Vec<String>)> = Vec::new();
    for kind in kind_names::<cli::ListKind>() {
        let mut args = vec!["list", shop, "--kind", &kind];
        if kind != "surface" {
            args.extend(["--format", "csv"]);
        }
        runs.push((
            format!("list {kind}"),
            args.iter().map(|s| s.to_string()).collect(),
        ));
    }
    for kind in kind_names::<cli::CsvKind>() {
        let o = out.join(format!("csv-{kind}.csv"));
        runs.push((
            format!("csv {kind}"),
            vec![
                "csv".into(),
                shop.into(),
                "--kind".into(),
                kind,
                "--out".into(),
                o.to_str().unwrap().into(),
            ],
        ));
    }
    for kind in kind_names::<cli::ExportKind>() {
        let o = out.join(format!("export-{kind}"));
        runs.push((
            format!("export {kind}"),
            vec![
                "export".into(),
                shop.into(),
                "--kind".into(),
                kind,
                "--out".into(),
                o.to_str().unwrap().into(),
            ],
        ));
    }
    for kind in kind_names::<cli::DiagramKind>() {
        let o = out.join(format!("diagram-{kind}"));
        runs.push((
            format!("diagram {kind}"),
            vec![
                "diagram".into(),
                shop.into(),
                "--kind".into(),
                kind,
                "--format".into(),
                "mermaid".into(),
                "--out".into(),
                o.to_str().unwrap().into(),
            ],
        ));
    }
    assert!(runs.len() > 40, "{} kinds", runs.len());

    let mut failures = Vec::new();
    for (name, args) in &runs {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = rdra_ish(&args, &out);
        if !output.status.success() {
            failures.push(format!(
                "{name}: exit {:?}\n{}",
                output.status.code(),
                text(&output.stderr)
            ));
            continue;
        }
        let mut produced = text(&output.stdout);
        produced.push_str(&text(&output.stderr));
        for file in files_under(&out) {
            produced.push_str(&std::fs::read_to_string(&file).unwrap_or_default());
            std::fs::remove_file(file).ok();
        }
        for hit in leaks(&produced) {
            failures.push(format!("{name}: names {hit:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn check_on_the_shop_prints_the_environment_once_before_the_bounds() {
    let output = rdra_ish(&["check", fixture("shop-model").to_str().unwrap()], &repo());
    let stderr = text(&output.stderr);
    let stdout = text(&output.stdout);

    assert!(output.status.success(), "{stderr}");
    let env = "environment: Auth (auth, path ../auth-model), Pay (payments, path ../payments-model/model)\n";
    assert_eq!(stderr.matches("environment:").count(), 1, "{stderr}");
    assert!(stderr.starts_with(env), "{stderr}");
    assert!(
        stderr.find("environment:") < stderr.find("bounds:"),
        "{stderr}"
    );
    assert!(
        leaks(&format!("{stderr}{stdout}")).is_empty(),
        "{stderr}{stdout}"
    );
}

#[test]
fn check_on_the_product_mistakes_names_each_mistake_at_its_line() {
    let dir = fixture("product-mistakes");
    let output = rdra_ish(&["check", dir.to_str().unwrap()], &repo());
    let stderr = common::relative_to(&text(&output.stderr), &dir);

    assert_eq!(output.status.code(), Some(1));
    let expected = [
        ("error", "mistakes.rdra:4:1: ForeignImport: module 'auth.session' belongs to dependency 'auth'", "extsystem X \"...\" from auth.<System>"),
        ("error", "mistakes.rdra:26:27: UnknownForeignName: 'Auth' publishes no 'Logon'", "closest published names: `Login`"),
        ("error", "mistakes.rdra:28:19: NotExposed: 'Auth.Session' is auth's internal entity 'Session'", "read `Auth.SessionView`"),
        ("error", "mistakes.rdra:29:21: ForeignWrite: `updates` on 'Auth.SessionView' writes another model's data", "`invokes(<UseCase>, Auth.Login)`"),
        ("error", "mistakes.rdra:30:23: ForeignContained: 'Auth.Login' is a use case of external system 'Auth'", "invokes(<UseCase>, Auth.Login)"),
        ("warning", "mistakes.rdra:31:19: DirectEntityReference: `reads` names 'Pay.Charge', an internal entity payments shares", "prefer a published projection"),
        ("error", "mistakes.rdra:33:17: ForeignStateNeedsEnvironment: a guard on 'Auth.SessionView' reads another model's state", "react to an event the owner exposes"),
    ];
    for (level, message, hint) in expected {
        assert!(
            stderr.contains(&format!("{level}: {message}")),
            "missing {message}\n{stderr}"
        );
        assert!(stderr.contains(hint), "missing hint {hint}\n{stderr}");
    }
    assert_eq!(stderr.matches("error: ").count(), 6, "{stderr}");
    assert_eq!(stderr.matches("warning: ").count(), 1, "{stderr}");
}

#[test]
fn list_dependency_prints_one_row_per_foreign_reference_with_its_site() {
    let dir = fixture("shop-model");
    let output = rdra_ish(
        &[
            "list",
            dir.to_str().unwrap(),
            "--kind",
            "dependency",
            "--format",
            "csv",
        ],
        &repo(),
    );
    let stdout = common::relative_to(&text(&output.stdout), &dir);

    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(
        stdout,
        "use,foreign,local,file,line\n\
         has_permission,Auth.CanLogin,Shopper,shop/checkout.rdra,12\n\
         invokes,Auth.Login,SignInAtCheckout,shop/checkout.rdra,34\n\
         invokes,Pay.CapturePayment,CapturePaymentForOrder,shop/checkout.rdra,51\n\
         reads,Auth.SessionView,PlaceOrder,shop/checkout.rdra,45\n\
         triggers,Pay.EvPaymentCaptured,MarkPaid,shop/checkout.rdra,54\n"
    );
    for row in stdout.lines().skip(1) {
        let cells: Vec<&str> = row.split(',').collect();
        let source = std::fs::read_to_string(dir.join(cells[3])).unwrap();
        let line = source
            .lines()
            .nth(cells[4].parse::<usize>().unwrap() - 1)
            .unwrap();
        assert!(line.contains(cells[1]), "{row}: {line}");
    }
}

#[test]
fn a_transition_on_a_foreign_column_is_a_foreign_write() {
    let dir = xsys_copy("cross_model_foreign_transition");
    let file = dir.join("shop-model/shop/checkout.rdra");
    let src = std::fs::read_to_string(&file).unwrap().replace(
        "reads(PlaceOrder, Auth.SessionView)",
        "transitions(Auth.SessionView.status, PlaceOrder, active -> ended)",
    );
    std::fs::write(&file, src).unwrap();

    let output = rdra_ish(&["check", "shop-model"], &dir);
    let stderr = text(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr.contains("checkout.rdra:45:13: ForeignWrite: `transitions` on 'Auth.SessionView'"),
        "{stderr}"
    );
}

#[test]
fn a_local_id_equal_to_a_stubs_generated_name_is_rejected() {
    let dir = xsys_copy("cross_model_shadow");
    let file = dir.join("shop-model/shop/checkout.rdra");
    let mut src = std::fs::read_to_string(&file).unwrap();
    src.push_str("entity Auth__SessionView \"Shadow\" { id: Int @pk }\n");
    std::fs::write(&file, src).unwrap();

    let output = rdra_ish(&["check", "shop-model"], &dir);
    let stderr = text(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr.contains(
            "IdShadowsForeignSlug: 'Auth__SessionView' is the generated name of 'Auth.SessionView'"
        ),
        "{stderr}"
    );
}

#[test]
fn a_second_binding_of_one_system_and_a_binding_of_a_transitive_dependency_are_rejected() {
    let dir = xsys_copy("cross_model_bindings");
    std::fs::write(
        dir.join("auth-model/rdra.toml"),
        "[model]\nname = \"auth\"\n[dependencies]\npayments = { path = \"../payments-model/model\" }\n",
    )
    .unwrap();
    let file = dir.join("shop-model/shop/checkout.rdra");
    let mut src = std::fs::read_to_string(&file).unwrap();
    src.push_str("extsystem Auth2 \"Again\" from auth.AuthSystem\nextsystem Ledger \"Ledger\" from payments.Nope\nextsystem Idp \"Idp\" from idp.IdpSystem\n");
    std::fs::write(&file, src).unwrap();

    let output = rdra_ish(&["check", "shop-model"], &dir);
    let stderr = text(&output.stderr);

    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("DuplicateBinding: auth.AuthSystem is already bound as extsystem 'Auth'"),
        "{stderr}"
    );
    assert!(
        stderr.contains("UnknownForeignName: 'payments' publishes no 'Nope'"),
        "{stderr}"
    );
    assert!(
        stderr.contains("closest published names: `PaymentSystem`"),
        "{stderr}"
    );
    assert!(stderr.contains("UnknownDependency: 'idp' is not a [dependencies] entry of the manifest (auth, payments)"), "{stderr}");
}

/// One broken dependency per case: a missing path, a cycle, and a dependency with an error.
fn break_dependency(dir: &Path, case: &str) {
    match case {
        "missing path" => std::fs::write(
            dir.join("shop-model/rdra.toml"),
            "[model]\nname = \"shop\"\n[dependencies]\nauth = { path = \"../nowhere\" }\npayments = { path = \"../payments-model/model\" }\n",
        )
        .unwrap(),
        "cycle" => std::fs::write(
            dir.join("auth-model/rdra.toml"),
            "[model]\nname = \"auth\"\n[dependencies]\nshop = { path = \"../shop-model\" }\n",
        )
        .unwrap(),
        "dependency with errors" => {
            let file = dir.join("auth-model/auth/session.rdra");
            let mut src = std::fs::read_to_string(&file).unwrap();
            src.push_str("performs(Nobody, Login)\n");
            std::fs::write(&file, src).unwrap();
        }
        other => panic!("unknown case {other}"),
    }
}

#[test]
fn a_broken_dependency_fails_check_with_one_diagnostic_that_names_it() {
    let cases = [
        (
            "missing path",
            "DependencyNotFound: dependency 'auth' (path ../nowhere) is not a directory",
        ),
        (
            "cycle",
            "DependencyCycle: dependency 'shop' (path ../shop-model) depends back on a model being loaded",
        ),
        (
            "dependency with errors",
            "DependencyHasErrors: dependency 'auth' (path ../auth-model) has 1 error(s); the first is error: ",
        ),
    ];
    for (name, expected) in cases {
        let dir = xsys_copy("cross_model_dep_failures");
        break_dependency(&dir, name);
        let output = rdra_ish(&["check", "shop-model"], &dir);
        let stderr = text(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{name}: {stderr}");
        assert!(stderr.contains(expected), "{name}: {stderr}");
        assert!(!stderr.contains("panicked"), "{name}: {stderr}");
        assert_eq!(
            stderr.lines().filter(|l| l.contains("rdra.toml")).count(),
            1,
            "{name}: {stderr}"
        );
    }
}

#[test]
fn an_import_of_a_dependency_becomes_a_binding() {
    let dir = xsys_copy("cross_model_import_to_binding");
    let file = dir.join("product-mistakes/mistakes.rdra");
    let src = "module product\nimport auth.session\nusecase Checkout \"Checkout\"\nactor Shopper \"Shopper\"\nperforms(Shopper, Checkout)\nentity Order \"Order\" { id: Int @pk }\ncreates(Checkout, Order)\ntransitions(Session.status, Checkout, active -> revoked)\n";
    std::fs::write(&file, src).unwrap();
    let output = rdra_ish(&["check", "product-mistakes"], &dir);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr.contains(
            "mistakes.rdra:2:1: ForeignImport: module 'auth.session' belongs to dependency 'auth'"
        ),
        "{stderr}"
    );
    assert!(
        stderr.contains(
            "hint: bind the system it publishes with `extsystem X \"...\" from auth.<System>`"
        ),
        "{stderr}"
    );

    let bound = src
        .replace(
            "import auth.session\n",
            "extsystem Auth \"Auth\" from auth.AuthSystem\n",
        )
        .replace(
            "transitions(Session.status",
            "transitions(Auth.SessionView.status",
        );
    std::fs::write(&file, bound).unwrap();
    let output = rdra_ish(&["check", "product-mistakes"], &dir);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr.contains("mistakes.rdra:8:13: ForeignWrite: `transitions` on 'Auth.SessionView'"),
        "{stderr}"
    );
}
