//! The Claude Code plugin: the marketplace at `.claude-plugin/marketplace.json` lists the
//! plugin in `plugins/rdra-ish/`, whose `.claude-plugin/plugin.json` and `.lsp.json` parse,
//! keep the workspace version, and use only the keys Claude Code documents (an unknown key
//! in `.lsp.json` makes Claude Code skip the whole file). Its `skills/` is a copy of the
//! repository's `skills/` that `scripts/sync-plugin-skills.sh` writes.

use std::collections::{BTreeMap, BTreeSet};
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

/// Every file under `dir`, by its path relative to `dir`, with its bytes.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, files);
            } else {
                let relative = path.strip_prefix(root).unwrap().to_path_buf();
                files.insert(relative, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(dir, dir, &mut files);
    files
}

#[test]
fn the_plugin_skills_are_a_copy_of_the_repository_skills() {
    let skills = repo().join("skills");
    let mut want = BTreeMap::new();
    for entry in std::fs::read_dir(&skills).unwrap() {
        let dir = entry.unwrap().path();
        if !dir.join("SKILL.md").is_file() {
            continue;
        }
        let name = PathBuf::from(dir.file_name().unwrap());
        for (path, bytes) in tree(&dir) {
            want.insert(name.join(path), bytes);
        }
    }
    let have = tree(&repo().join("plugins/rdra-ish/skills"));
    let differ: BTreeSet<_> = (want.keys().chain(have.keys()))
        .filter(|path| want.get(*path) != have.get(*path))
        .collect();
    assert!(
        differ.is_empty(),
        "plugins/rdra-ish/skills differs from skills/ at {differ:?}: run scripts/sync-plugin-skills.sh"
    );
}
