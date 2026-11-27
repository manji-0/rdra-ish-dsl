//! `rdra-ish lsp` serves the language server over stdio: a client that opens a file with an
//! error gets its diagnostics, and stdout carries nothing but protocol messages.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

fn send(stdin: &mut impl Write, message: &Value) {
    let body = message.to_string();
    write!(stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    stdin.flush().unwrap();
}

/// Reads one framed message; panics on anything else, so a stray line on stdout fails the test.
fn read_message(reader: &mut impl BufRead) -> Option<Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap() == 0 {
            return None;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        let (name, value) = line
            .split_once(": ")
            .unwrap_or_else(|| panic!("not an LSP header on stdout: {line:?}"));
        if name.eq_ignore_ascii_case("Content-Length") {
            length = Some(value.parse::<usize>().unwrap());
        }
    }
    let mut body = vec![0; length.expect("a Content-Length header")];
    reader.read_exact(&mut body).unwrap();
    Some(serde_json::from_slice(&body).unwrap())
}

fn file_uri(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

#[test]
fn lsp_publishes_diagnostics_for_an_opened_file() {
    let root = std::env::temp_dir().join(format!(
        "rdra_ish_lsp_cli_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let root = dunce::canonicalize(&root).unwrap();
    let file = root.join("bad.rdra");
    // `reads` takes a use case, not an actor: a type mismatch.
    let text = "actor Customer \"Customer\"\nentity Order \"Order\" { id: Int @pk }\nreads(Customer, Order)\n";
    std::fs::write(&file, text).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut stdout = BufReader::new(stdout);
        while let Some(message) = read_message(&mut stdout) {
            if tx.send(message).is_err() {
                break;
            }
        }
    });

    send(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "processId": null,
                "rootUri": file_uri(&root),
                "capabilities": {},
                "initializationOptions": { "lifecycleCheck": { "enabled": false } },
            },
        }),
    );
    // A client waits for the initialize response before it sends anything else.
    let deadline = Instant::now() + Duration::from_secs(10);
    let response = rx
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .expect("the initialize response within 10 s");
    assert_eq!(response["id"], 1, "{response}");
    assert_eq!(
        response["result"]["serverInfo"]["name"], "rdra-ish-lsp",
        "{response}"
    );
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
    );
    send(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0", "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": file_uri(&file), "languageId": "rdra", "version": 1, "text": text,
            } },
        }),
    );

    let mut diagnostics = None;
    while diagnostics.is_none() {
        let left = deadline.saturating_duration_since(Instant::now());
        let message = rx
            .recv_timeout(left)
            .expect("publishDiagnostics for bad.rdra within 10 s");
        if message["method"] == "textDocument/publishDiagnostics"
            && message["params"]["uri"]
                .as_str()
                .is_some_and(|uri| uri.ends_with("/bad.rdra"))
            && message["params"]["diagnostics"]
                .as_array()
                .is_some_and(|d| !d.is_empty())
        {
            diagnostics = Some(message["params"]["diagnostics"].clone());
        }
    }
    let diagnostics = diagnostics.unwrap();
    assert!(
        diagnostics
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["severity"] == 1),
        "an error among {diagnostics}"
    );

    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": null }),
    );
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "method": "exit", "params": null }),
    );
    drop(stdin);
    let status = child.wait().unwrap();
    // Every byte left on stdout was a framed message too, or the reader panicked.
    reader.join().expect("stdout held only LSP messages");
    assert!(status.success(), "rdra-ish lsp exited with {status}");
    let _ = std::fs::remove_dir_all(root);
}
