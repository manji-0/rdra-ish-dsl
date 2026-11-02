//! `diagram` without `--out` prints the Mermaid text to stdout and leaves no file behind;
//! with `--out` it writes `<out>.mmd` as before.
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).expect("repo root")
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn diagram(cwd: &Path, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(cwd)
        .arg("diagram")
        .arg(repo().join("tests/fixtures/xsys/auth-model"))
        .args(["--kind", "er"])
        .args(extra)
        .output()
        .expect("rdra-ish runs")
}

#[test]
fn without_out_the_text_goes_to_stdout_and_no_file_is_written() {
    let cwd = scratch("diagram_stdout_none");
    let output = diagram(&cwd, &[]);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("%% rdra-ish diagram=er"), "{stdout}");
    assert_eq!(std::fs::read_dir(&cwd).unwrap().count(), 0);
}

#[test]
fn with_out_the_text_is_written_to_the_file() {
    let cwd = scratch("diagram_stdout_out");
    let output = diagram(&cwd, &["--out", "d"]);
    assert!(output.status.success(), "{output:?}");
    let written = std::fs::read_to_string(cwd.join("d.mmd")).unwrap();
    assert!(written.starts_with("%% rdra-ish diagram=er"), "{written}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "wrote d.mmd\n");
}
