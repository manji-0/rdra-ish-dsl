//! A workspace opened through a symlink gets its results under the paths the client named:
//! the diagnostics of an open and of an unopened file, and a definition in another file.

#![cfg(unix)]

mod common;

use common::{position, uri, Client};
use serde_json::json;

#[test]
fn results_go_to_the_uris_the_client_opened() {
    let real = common::workspace("rdra_lsp_symlinked");
    let link = real.with_file_name(format!(
        "{}_link",
        real.file_name().unwrap().to_str().unwrap()
    ));
    std::os::unix::fs::symlink(&real, &link).unwrap();
    std::fs::write(real.join("actors.rdra"), "actor Customer \"Customer\"\n").unwrap();
    let main_text = "usecase Buy \"Buy\"\nperforms(Customer, Buy)\nperforms(Customer, Nope)\n";
    std::fs::write(real.join("main.rdra"), main_text).unwrap();

    let mut client = Client::initialized(&link);
    let main = link.join("main.rdra");
    client.notify(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": uri(&main), "languageId": "rdra", "version": 1,
            "text": main_text}}),
    );
    // Both files' diagnostics, under the link: the open one with its error, and the other.
    let (main_uri, actors_uri) = (uri(&main), uri(&link.join("actors.rdra")));
    let (mut main_seen, mut actors_seen) = (false, false);
    client.wait_for("both files' diagnostics under the link", |m| {
        if m["method"] == "textDocument/publishDiagnostics" {
            let d = m["params"]["diagnostics"].as_array().unwrap();
            if m["params"]["uri"] == main_uri.as_str() {
                main_seen |= d
                    .iter()
                    .any(|d| d["message"].as_str().unwrap().contains("Nope"));
            }
            actors_seen |= m["params"]["uri"] == actors_uri.as_str();
        }
        main_seen && actors_seen
    });

    let at = main_text.find("Customer").unwrap() + 1;
    let definition = client.request(
        "textDocument/definition",
        json!({"textDocument": {"uri": uri(&main)}, "position": position(main_text, at)}),
    );
    assert_eq!(
        definition["result"]["uri"],
        uri(&link.join("actors.rdra")),
        "{definition}"
    );

    let _ = std::fs::remove_file(&link);
}
