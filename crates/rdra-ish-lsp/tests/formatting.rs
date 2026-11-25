//! Formatting through the server gives what `rdra-ish fmt` gives, and formatting that
//! result again changes nothing (#357).

mod common;

use common::{uri, Client};
use serde_json::{json, Value};

fn format(client: &mut Client, path: &std::path::Path) -> Value {
    let response = client.request(
        "textDocument/formatting",
        json!({"textDocument": {"uri": uri(path)},
            "options": {"tabSize": 2, "insertSpaces": true}}),
    );
    assert!(response.get("error").is_none(), "{response}");
    response["result"].clone()
}

/// Applies LSP text edits to `text`, last edit first.
fn apply(text: &str, edits: &[Value]) -> String {
    let mut spans: Vec<_> = edits
        .iter()
        .map(|edit| {
            let start = common::offset(text, &edit["range"]["start"]);
            let end = common::offset(text, &edit["range"]["end"]);
            (start, end, edit["newText"].as_str().unwrap())
        })
        .collect();
    spans.sort_by_key(|&(start, _, _)| std::cmp::Reverse(start));
    let mut text = text.to_string();
    for (start, end, new_text) in spans {
        text.replace_range(start..end, new_text);
    }
    text
}

fn formats_to_what_fmt_prints_and_then_stays(input: &str) {
    let expected = rdra_ish_syntax::format_source(input).expect("the input parses");
    assert_ne!(expected, input, "the input must need formatting");

    let root = common::workspace("rdra_lsp_formatting");
    let path = root.join("model.rdra");
    std::fs::write(&path, input).unwrap();
    let mut client = Client::initialized(&root);
    client.open(&path, input);

    let edits = format(&mut client, &path);
    let edits = edits.as_array().unwrap_or_else(|| panic!("edits: {edits}"));
    assert!(!edits.is_empty(), "the first format must change the text");
    let formatted = apply(input, edits);
    assert_eq!(formatted, expected);

    client.change(&path, 2, &formatted);
    let again = format(&mut client, &path);
    assert_eq!(
        again,
        json!([]),
        "formatting the result again must change nothing"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn formats_lf_text() {
    formats_to_what_fmt_prints_and_then_stays(
        "actor   A   \"顧客\"\n\n\n\nusecase U \"注文\"\nperforms(A,U)\n",
    );
}

#[test]
fn formats_crlf_text() {
    formats_to_what_fmt_prints_and_then_stays(
        "actor   A   \"顧客\"\r\n\r\n\r\n\r\nusecase U \"注文\"\r\nperforms(A,U)\r\n",
    );
}

#[test]
fn formats_text_without_a_final_newline() {
    formats_to_what_fmt_prints_and_then_stays(
        "actor   A   \"顧客\"\nusecase U \"注文\"\nperforms(A,U) // 受付",
    );
}

#[test]
fn formats_text_that_ends_in_japanese_and_blank_lines() {
    formats_to_what_fmt_prints_and_then_stays(
        "actor   A   \"顧客(法人)\"\nusecase U \"注文\"\nperforms(A,U) // 受付\n\n\n",
    );
}
