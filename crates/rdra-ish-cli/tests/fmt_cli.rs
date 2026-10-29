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
