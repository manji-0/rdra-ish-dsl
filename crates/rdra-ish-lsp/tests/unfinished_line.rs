//! A declaration being typed at the end of a file keeps the rest of the file in the analysis:
//! its outline, and the symbols other files import from it.

mod common;

use common::{uri, Client};
use serde_json::json;

#[test]
fn an_unfinished_last_line_keeps_the_outline_and_the_importers() {
    let root = common::workspace("rdra_lsp_unfinished");
    let lib = root.join("lib.rdra");
    let main = root.join("main.rdra");
    let lib_text = "module lib\nactor Alice \"Alice\"\n";
    std::fs::write(&lib, lib_text).unwrap();
    std::fs::write(
        &main,
        "import lib\nusecase Buy \"Buy\"\nperforms(Alice, Buy)\n",
    )
    .unwrap();
    let mut client = Client::initialized(&root);
    client.open(&lib, lib_text);

    // The user starts a declaration on the last line of lib.rdra: its syntax error is
    // published, then the diagnostics of main.rdra from the same analysis.
    client.notify(
        "textDocument/didChange",
        json!({"textDocument": {"uri": uri(&lib), "version": 2},
            "contentChanges": [{"text": format!("{lib_text}actor ")}]}),
    );
    client.wait_for_diagnostics(&uri(&lib), "lib.rdra's syntax error", |d| !d.is_empty());

    // main.rdra still resolves `import lib` and Alice: no unknown module, no undefined symbol.
    let diagnostics = client.wait_for("main.rdra's diagnostics after the edit", |m| {
        m["method"] == "textDocument/publishDiagnostics"
            && m["params"]["uri"] == uri(&main).as_str()
    })["params"]["diagnostics"]
        .clone();
    let messages: Vec<&str> = diagnostics
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["message"].as_str())
        .collect();
    assert!(
        messages
            .iter()
            .all(|m| !m.contains("unknown module") && !m.contains("undefined symbol")),
        "{messages:?}"
    );

    let outline = client.request(
        "textDocument/documentSymbol",
        json!({"textDocument": {"uri": uri(&lib)}}),
    );
    let names: Vec<&str> = outline["result"]
        .as_array()
        .unwrap_or_else(|| panic!("{outline}"))
        .iter()
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert!(names.iter().any(|n| n.contains("Alice")), "{outline}");

    let _ = std::fs::remove_dir_all(root);
}
