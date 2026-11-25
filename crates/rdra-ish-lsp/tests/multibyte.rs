//! Requests at a cursor right after a multi-byte character must answer, not crash the
//! server (#356).

mod common;

use std::path::{Path, PathBuf};

use common::{position, uri, Client};
use serde_json::{json, Value};

const LABELS: &str = "actor A \"受付(窓口)\"\nusecase U \"顧客(法人)\"\nperforms(A, U) // 受付(\n";

/// The open file. Dropping it removes its workspace.
struct OpenFile(PathBuf);

impl std::ops::Deref for OpenFile {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for OpenFile {
    fn drop(&mut self) {
        if let Some(root) = self.0.parent() {
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

/// A server with `text` open as `labels.rdra`, analyzed.
fn session(text: &str) -> (Client, OpenFile) {
    let root = common::workspace("rdra_lsp_multibyte");
    let path = root.join("labels.rdra");
    std::fs::write(&path, text).unwrap();
    let mut client = Client::initialized(&root);
    client.open(&path, text);
    (client, OpenFile(path))
}

/// Sends `method` at the byte offset just after the last `needle` in `text`, and returns the
/// response, which must not be an error.
fn request_after(
    client: &mut Client,
    path: &Path,
    text: &str,
    method: &str,
    needle: &str,
) -> Value {
    let at = text
        .rfind(needle)
        .unwrap_or_else(|| panic!("{needle:?} in the text"))
        + needle.len();
    let response = client.request(
        method,
        json!({"textDocument": {"uri": uri(path)}, "position": position(text, at)}),
    );
    assert!(
        response.get("error").is_none(),
        "{method} failed: {response}"
    );
    response
}

#[test]
fn hover_after_a_paren_inside_a_japanese_label_answers() {
    let (mut client, path) = session(LABELS);
    for needle in ["\"受付(", "\"顧客(法人"] {
        let response = request_after(&mut client, &path, LABELS, "textDocument/hover", needle);
        assert!(response["result"].is_null(), "{response}");
    }
}

#[test]
fn hover_after_a_paren_in_a_japanese_comment_answers() {
    let (mut client, path) = session(LABELS);
    let response = request_after(&mut client, &path, LABELS, "textDocument/hover", "// 受付(");
    assert!(response["result"].is_null(), "{response}");
}

#[test]
fn signature_help_after_a_paren_in_a_japanese_comment_or_label_answers() {
    let (mut client, path) = session(LABELS);
    for needle in ["// 受付(", "\"顧客("] {
        let response = request_after(
            &mut client,
            &path,
            LABELS,
            "textDocument/signatureHelp",
            needle,
        );
        assert!(response["result"].is_null(), "{needle}: {response}");
    }
}

#[test]
fn completion_after_a_paren_in_a_japanese_comment_answers() {
    let (mut client, path) = session(LABELS);
    let response = request_after(
        &mut client,
        &path,
        LABELS,
        "textDocument/completion",
        "// 受付(",
    );
    assert!(response["result"].is_array(), "{response}");
}

#[test]
fn completion_after_an_unclosed_japanese_label_answers() {
    let text = "actor A \"受付\"\nactor B \"受付";
    let (mut client, path) = session(text);
    let response = request_after(
        &mut client,
        &path,
        text,
        "textDocument/completion",
        "\"受付",
    );
    let result = &response["result"];
    assert!(result.is_array(), "{response}");
}

#[test]
fn requests_on_a_line_with_japanese_text_still_find_the_call() {
    let (mut client, path) = session(LABELS);
    let response = request_after(
        &mut client,
        &path,
        LABELS,
        "textDocument/signatureHelp",
        "performs(A, ",
    );
    let help = &response["result"];
    assert_eq!(
        help["signatures"][0]["label"], "performs(actor, usecase|buc|step)",
        "{response}"
    );
    assert_eq!(help["activeParameter"], 1, "{response}");

    let response = request_after(
        &mut client,
        &path,
        LABELS,
        "textDocument/hover",
        "performs(A, ",
    );
    let hover = response["result"]["contents"]["value"]
        .as_str()
        .unwrap_or_default();
    assert!(hover.contains("顧客(法人)"), "{response}");
}
