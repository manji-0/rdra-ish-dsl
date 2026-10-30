//! `list --kind surface`: the canonical text, and the rejection of any `--format`.
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).expect("repo root")
}

fn list_surface(extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args([
            "list",
            "tests/fixtures/xsys/auth-model",
            "--kind",
            "surface",
        ])
        .args(extra)
        .output()
        .expect("rdra-ish runs")
}

#[test]
fn the_default_format_prints_the_canonical_text() {
    let output = list_surface(&[]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "\
usecase Login \"Log in\" caller(actor) requires(CanLogin) raises(EvLoggedIn)
entity SessionView \"Session (published)\" upsert { session_id: Int @pk  account_id: Int  status: Enum(active, ended) }
writer SessionView Login: status == active
writer SessionView (unexposed): status == ended
"
    );
}

#[test]
fn any_other_format_is_a_usage_error() {
    for format in ["json", "csv"] {
        let output = list_surface(&["--format", format]);
        assert_eq!(output.status.code(), Some(2), "--format {format}");
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("`list --kind surface` prints canonical text only; drop `--format`"),
            "{stderr}"
        );
    }
}
