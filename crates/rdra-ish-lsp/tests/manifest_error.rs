//! A malformed `rdra.toml` must not leave the editor with stale or no feedback.

mod common;

use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use common::{uri, Client};
use serde_json::{json, Value};

fn workspace() -> PathBuf {
    let root = common::workspace("rdra_lsp_manifest");
    std::fs::write(root.join("open.rdra"), "actor Customer \"Customer\"\n").unwrap();
    std::fs::write(
        root.join("other.rdra"),
        "entity Order \"Order\" { id: Int @pk }\n",
    )
    .unwrap();
    root
}

#[test]
fn a_malformed_manifest_clears_diagnostics_and_reports_the_error() {
    let root = workspace();
    let open = root.join("open.rdra");
    let other = root.join("other.rdra");
    let mut client = Client::start();
    client.send(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"processId": null, "rootUri": uri(&root), "capabilities": {}}}));
    client.wait_for("initialize", |m| m["id"] == 1);
    client.notify("initialized", json!({}));
    client.notify(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": uri(&open), "languageId": "rdra", "version": 1,
            "text": std::fs::read_to_string(&open).unwrap()}}),
    );
    client.wait_for_diagnostics(&uri(&other), "diagnostics on the valid workspace", |d| {
        !d.is_empty()
    });

    std::fs::write(
        root.join("rdra.toml"),
        "[model]\nname = \"shop\"\nbroken = = 1\n",
    )
    .unwrap();
    client.notify(
        "textDocument/didChange",
        json!({"textDocument": {"uri": uri(&open), "version": 2},
            "contentChanges": [{"text": "actor Customer \"Customer\"\n"}]}),
    );

    let (mut shown, mut cleared, mut flagged) = (None, false, false);
    client.wait_for("the error shown and the stale diagnostics cleared", |m| {
        if m["method"] == "window/showMessage" {
            shown = Some(m["params"].clone());
        } else if m["method"] == "textDocument/publishDiagnostics" {
            let diagnostics = m["params"]["diagnostics"].as_array().unwrap();
            if m["params"]["uri"] == uri(&other).as_str() {
                cleared |= diagnostics.is_empty();
            } else if m["params"]["uri"] == uri(&open).as_str() {
                flagged |= diagnostics.len() == 1
                    && diagnostics[0]["severity"] == 1
                    && diagnostics[0]["message"]
                        .as_str()
                        .unwrap()
                        .contains("rdra.toml:3");
            }
        }
        shown.is_some() && cleared && flagged
    });
    let shown = shown.unwrap();
    assert_eq!(shown["type"], 1);
    let text = shown["message"].as_str().unwrap();
    assert!(text.contains("rdra.toml:3"), "{text}");

    std::fs::write(root.join("rdra.toml"), "[model]\nname = \"shop\"\n").unwrap();
    client.notify(
        "textDocument/didSave",
        json!({"textDocument": {"uri": uri(&open)}, "text": "actor Customer \"Customer\"\n"}),
    );
    let (mut back, mut withdrawn) = (false, false);
    client.wait_for("the workspace back and the manifest error withdrawn", |m| {
        if m["method"] == "textDocument/publishDiagnostics" {
            let diagnostics = m["params"]["diagnostics"].as_array().unwrap();
            if m["params"]["uri"] == uri(&other).as_str() {
                back |= !diagnostics.is_empty();
            } else if m["params"]["uri"] == uri(&open).as_str() {
                withdrawn |= diagnostics
                    .iter()
                    .all(|d| !d["message"].as_str().unwrap().contains("rdra.toml"));
            }
        }
        back && withdrawn
    });
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_burst_of_edits_ends_on_the_diagnostics_of_the_last_text() {
    let root = workspace();
    let open = root.join("other.rdra");
    let mut client = Client::start();
    client.send(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"processId": null, "rootUri": uri(&root), "capabilities": {}}}));
    client.wait_for("initialize", |m| m["id"] == 1);
    client.notify("initialized", json!({}));
    client.notify(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": uri(&open), "languageId": "rdra", "version": 1,
            "text": std::fs::read_to_string(&open).unwrap()}}),
    );
    client.wait_for_diagnostics(&uri(&open), "diagnostics of the opened text", |_| true);
    for version in 2..40 {
        let text = if version == 39 {
            "entity Order \"Order\" { id: Int @pk }\n".to_string()
        } else {
            // Older texts are larger, so their analysis is slower and would finish last.
            let mut text = format!(
                "entity Order \"Order\" {{ id: Int @pk }}\nreads(Missing{version}, Order)\n"
            );
            for n in 0..(40 - version) * 40 {
                text.push_str(&format!(
                    "entity E{n} \"E\" {{ id: Int @pk  o: Int }}\nrelate(E{n}, Order, N:1)\n"
                ));
            }
            text
        };
        client.notify(
            "textDocument/didChange",
            json!({"textDocument": {"uri": uri(&open), "version": version},
                "contentChanges": [{"text": text}]}),
        );
    }

    // Analyses of intermediate texts can take seconds on a slow runner, so a quiet window
    // alone does not mean the server has settled. Wait for the last text's diagnostics first,
    // then make sure nothing older is published after them.
    let is_last_text = |diagnostics: &Value| {
        diagnostics
            .as_array()
            .unwrap()
            .iter()
            .all(|d| !d["message"].as_str().unwrap().contains("Missing"))
    };
    let mut last = client.wait_for("diagnostics of the last text", |m| {
        m["method"] == "textDocument/publishDiagnostics"
            && m["params"]["uri"] == uri(&open).as_str()
            && is_last_text(&m["params"]["diagnostics"])
    })["params"]["diagnostics"]
        .clone();
    loop {
        match client.messages.recv_timeout(Duration::from_secs(3)) {
            Ok(m) => {
                if m["method"] == "textDocument/publishDiagnostics"
                    && m["params"]["uri"] == uri(&open).as_str()
                {
                    last = m["params"]["diagnostics"].clone();
                }
            }
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => panic!("server exited"),
        }
    }
    assert!(
        is_last_text(&last),
        "the last diagnostics are of an older text: {last}"
    );
    let _ = std::fs::remove_dir_all(root);
}
