//! Whether a tool a test runs is here. A test without its tool is skipped with a printed
//! reason, except when the test's `RDRA_REQUIRE_*` variable is 1: CI sets it in the dev shell
//! that provides the tool, so a missing tool fails there instead of passing unnoticed.

// A test binary that includes this file may use only `tool`.
#![allow(dead_code)]

use std::path::PathBuf;

/// The file `name` names on PATH, as a command would find it.
pub fn on_path(name: &str) -> Option<PathBuf> {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(&file))
        .find(|path| path.is_file())
}

/// `found`; when it is false, a panic under `require=1`, else a printed `skipped:` line.
/// `missing` says what is missing, as "go is not on PATH".
pub fn found_or_skip(found: bool, missing: &str, require: &str) -> bool {
    if !found {
        assert!(
            std::env::var(require).as_deref() != Ok("1"),
            "{require}=1 but {missing} (run inside `nix develop`)"
        );
        println!("skipped: {missing}");
    }
    found
}

/// Whether every one of `names` is on PATH; see [`found_or_skip`].
pub fn tools(names: &[&str], require: &str) -> bool {
    match names.iter().find(|name| on_path(name).is_none()) {
        None => true,
        Some(name) => found_or_skip(false, &format!("{name} is not on PATH"), require),
    }
}

/// Whether `name` is on PATH; see [`found_or_skip`].
pub fn tool(name: &str, require: &str) -> bool {
    tools(&[name], require)
}

/// The variable that requires the code generators' compilers (cargo, go, tsc, node).
pub const CODEGEN: &str = "RDRA_REQUIRE_CODEGEN_TOOLS";
