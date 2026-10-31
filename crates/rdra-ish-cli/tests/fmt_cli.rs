//! `fmt --write` rewrites the file in place, so it must keep every comment.

use std::path::Path;
use std::process::Command;

#[test]
fn fmt_write_keeps_comments() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fmt_cli_comments");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a.rdra");
    std::fs::write(
        &path,
        "module a\n\n// sources/a.txt:12\nactor A \"a\" // trailing\n/* footer */\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .arg("fmt")
        .arg("--write")
        .arg(&path)
        .output()
        .expect("rdra-ish runs");

    assert!(output.status.success());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "module a\n\n// sources/a.txt:12\nactor A \"a\" // trailing\n\n/* footer */\n"
    );
}

#[test]
fn fmt_round_trips_the_cross_model_forms() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fmt_cli_cross_model");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("checkout.rdra");
    let original = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/xsys/shop-model/shop/checkout.rdra"),
    )
    .unwrap();
    std::fs::write(&path, &original).unwrap();

    let fmt_write = || {
        let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
            .args(["fmt", "--write"])
            .arg(&path)
            .output()
            .expect("rdra-ish runs");
        assert!(output.status.success());
        std::fs::read_to_string(&path).unwrap()
    };
    let once = fmt_write();
    let twice = fmt_write();
    assert_eq!(twice, once, "the second fmt changed the file");
    let formatted = twice;
    for form in [
        "from auth.AuthSystem",
        "usecase::Auth.Login",
        "Auth.SessionView",
        "Pay.EvPaymentCaptured",
    ] {
        assert!(formatted.contains(form), "{form} missing");
    }
    let check = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .args(["fmt", "--check"])
        .arg(&path)
        .output()
        .expect("rdra-ish runs");
    assert!(check.status.success());
}
