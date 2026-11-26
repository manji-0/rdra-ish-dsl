//! Diagnostics follow the files of the workspace: a file that leaves the analysis, deleted
//! or renamed, has its diagnostics withdrawn.

mod common;

use common::{uri, Client};
use serde_json::json;

const BAD: &str = "actor Customer \"Customer\"\nusecase Buy \"Buy\"\nperforms(Customer, Nope)\n";

fn deleted(client: &mut Client, path: &std::path::Path) {
    std::fs::remove_file(path).unwrap();
    client.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [{"uri": uri(path), "type": 3}]}),
    );
}

#[test]
fn a_deleted_file_has_its_diagnostics_withdrawn() {
    let root = common::workspace("rdra_lsp_deleted");
    let good = root.join("good.rdra");
    let bad = root.join("bad.rdra");
    std::fs::write(&good, "entity Order \"Order\" { id: Int @pk }\n").unwrap();
    std::fs::write(&bad, BAD).unwrap();
    let mut client = Client::initialized(&root);
    client.wait_for_diagnostics(&uri(&bad), "the error in bad.rdra", |d| !d.is_empty());

    deleted(&mut client, &bad);

    client.wait_for_diagnostics(&uri(&bad), "bad.rdra's diagnostics withdrawn", |d| {
        d.is_empty()
    });
}

#[test]
fn deleting_the_last_file_withdraws_its_diagnostics() {
    let root = common::workspace("rdra_lsp_deleted_last");
    let bad = root.join("bad.rdra");
    std::fs::write(&bad, BAD).unwrap();
    let mut client = Client::initialized(&root);
    client.wait_for_diagnostics(&uri(&bad), "the error in bad.rdra", |d| !d.is_empty());

    deleted(&mut client, &bad);

    client.wait_for_diagnostics(&uri(&bad), "bad.rdra's diagnostics withdrawn", |d| {
        d.is_empty()
    });
}
