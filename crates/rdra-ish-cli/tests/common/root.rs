//! The repo root, canonical, for the CLI tests that compare the paths it prints.

use std::path::{Path, PathBuf};

pub fn repo() -> PathBuf {
    dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).expect("repo root")
}
