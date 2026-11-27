//! The Claude Code plugin: the marketplace at `.claude-plugin/marketplace.json` lists the
//! plugin in `plugins/rdra-ish/`, whose `.claude-plugin/plugin.json` and `.lsp.json` parse,
//! keep the workspace version, and use only the keys Claude Code documents (an unknown key
//! in `.lsp.json` makes Claude Code skip the whole file). Its `skills` is a link to the
//! repository's `skills/`, which an install copies into the plugin's cache.

use std::path::{Path, PathBuf};

use serde_json::Value;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_json(relative: &str) -> Value {
    let path = repo().join(relative);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {relative}: {e}"))
}

#[test]
fn the_marketplace_lists_the_plugin_in_plugins_rdra_ish() {
    let marketplace = read_json(".claude-plugin/marketplace.json");
    assert_eq!(marketplace["name"], "rdra-ish");
    assert!(marketplace["owner"]["name"].is_string());
    assert!(marketplace["description"].is_string());
    let plugins = marketplace["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0]["name"], "rdra-ish");
    assert_eq!(plugins[0]["source"], "./plugins/rdra-ish");
}

#[test]
fn the_plugin_version_is_the_workspace_version() {
    let plugin = read_json("plugins/rdra-ish/.claude-plugin/plugin.json");
    assert_eq!(plugin["name"], "rdra-ish");
    assert_eq!(
        plugin["version"],
        env!("CARGO_PKG_VERSION"),
        "bump plugins/rdra-ish/.claude-plugin/plugin.json with Cargo.toml and pyproject.toml"
    );
}

#[test]
fn the_language_server_runs_rdra_ish_lsp_for_rdra_files() {
    const DOCUMENTED: &[&str] = &[
        "command",
        "extensionToLanguage",
        "args",
        "transport",
        "env",
        "initializationOptions",
        "settings",
        "workspaceFolder",
        "startupTimeout",
        "shutdownTimeout",
        "requestTimeout",
        "restartOnCrash",
        "maxRestarts",
        "diagnostics",
    ];
    let servers = read_json("plugins/rdra-ish/.lsp.json");
    let servers = servers.as_object().unwrap();
    assert_eq!(
        servers.keys().collect::<Vec<_>>(),
        ["rdra-ish"],
        "one server"
    );
    let server = servers["rdra-ish"].as_object().unwrap();
    for key in server.keys() {
        assert!(
            DOCUMENTED.contains(&key.as_str()),
            "{key} is not a documented LSP server key"
        );
    }
    assert_eq!(server["command"], "rdra-ish");
    assert_eq!(server["args"], serde_json::json!(["lsp"]));

    // The language id is the one the VS Code extension declares for .rdra.
    let package = read_json("editors/vscode/package.json");
    let language = package["contributes"]["languages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| {
            l["extensions"]
                .as_array()
                .unwrap()
                .contains(&".rdra".into())
        })
        .unwrap();
    assert_eq!(server["extensionToLanguage"][".rdra"], language["id"]);
}

#[test]
fn the_plugin_skills_are_a_link_to_the_repository_skills() {
    let link = repo().join("plugins/rdra-ish/skills");
    let meta = std::fs::symlink_metadata(&link).unwrap();
    // A checkout without symlinks (Windows, core.symlinks=false) writes the target as text.
    let target = if meta.file_type().is_symlink() {
        std::fs::read_link(&link).unwrap()
    } else {
        PathBuf::from(std::fs::read_to_string(&link).unwrap().trim())
    };
    assert_eq!(target, Path::new("../../skills"));
}
