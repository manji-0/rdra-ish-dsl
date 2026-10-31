//! A malformed `rdra.toml` must not leave the editor with stale or no feedback.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

struct Client {
    child: Child,
    stdin: ChildStdin,
    messages: Receiver<Value>,
}

impl Client {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rdra-ish-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let (tx, messages) = channel();
        std::thread::spawn(move || loop {
            let mut length = 0;
            loop {
                let mut line = String::new();
                if stdout.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.strip_prefix("Content-Length:") {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            stdout.read_exact(&mut body).unwrap();
            if tx.send(serde_json::from_slice(&body).unwrap()).is_err() {
                return;
            }
        });
        Self {
            child,
            stdin,
            messages,
        }
    }

    fn send(&mut self, message: Value) {
        let body = message.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    fn wait_for(&self, what: &str, mut matches: impl FnMut(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.messages.recv_timeout(left) {
                Ok(message) if matches(&message) => return message,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => panic!("timed out waiting for {what}"),
                Err(RecvTimeoutError::Disconnected) => panic!("server exited waiting for {what}"),
            }
        }
    }

    fn wait_for_diagnostics(&self, uri: &str, what: &str, mut ok: impl FnMut(&[Value]) -> bool) {
        self.wait_for(what, |m| {
            m["method"] == "textDocument/publishDiagnostics"
                && m["params"]["uri"] == uri
                && ok(m["params"]["diagnostics"].as_array().unwrap())
        });
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn uri(path: &Path) -> String {
    url::Url::from_file_path(path).unwrap().to_string()
}

fn workspace() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "rdra_lsp_manifest_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(root.join("rdra.toml"), "[model]\nname = \"shop\"\n").unwrap();
    std::fs::write(root.join("open.rdra"), "actor Customer \"Customer\"\n").unwrap();
    std::fs::write(
        root.join("other.rdra"),
        "entity Order \"Order\" { id: Int @pk }\n",
    )
    .unwrap();
    dunce::canonicalize(root).unwrap()
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
