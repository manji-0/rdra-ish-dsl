//! `files` prints what a root loads in load order, which `scripts/verify-samples.sh` reads
//! the `// expect` header from.

use std::path::Path;
use std::process::Command;

#[test]
fn files_lists_a_root_in_load_order_whatever_the_creation_order() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("files_cli_order");
    let _ = std::fs::remove_dir_all(&dir);
    for rel in ["z.rdra", "b/y.rdra", "a.rdra", "b/notes.txt", "b/c.rdra"] {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "").unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .arg("files")
        .arg(&dir)
        .output()
        .expect("rdra-ish runs");
    assert!(output.status.success());
    let root = format!("{}/", dir.display());
    let stdout = String::from_utf8_lossy(&output.stdout).replace(&root, "");
    assert_eq!(stdout, "a.rdra\nb/c.rdra\nb/y.rdra\nz.rdra\n");
}

#[test]
fn files_fails_on_a_root_without_rdra_files() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("files_cli_empty");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .arg("files")
        .arg(&dir)
        .output()
        .expect("rdra-ish runs");
    assert_eq!(output.status.code(), Some(1));
}
