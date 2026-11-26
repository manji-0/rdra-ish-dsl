//! A language-server client over stdio, shared by the tests that drive the server binary.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

pub struct Client {
    child: Child,
    stdin: ChildStdin,
    pub messages: Receiver<Value>,
    next_id: u64,
}

impl Client {
    pub fn start() -> Self {
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
            next_id: 100,
        }
    }

    /// Starts a server on `root` and waits until it has answered `initialize`. The background
    /// lifecycle check is off, so that every diagnostics message is of an analysis.
    pub fn initialized(root: &Path) -> Self {
        Self::initialized_with(root, json!({"lifecycleCheck": {"enabled": false}}))
    }

    /// `initialized` with these initialization options.
    pub fn initialized_with(root: &Path, options: Value) -> Self {
        let mut client = Self::start();
        client.send(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"processId": null, "rootUri": uri(root), "capabilities": {},
                "initializationOptions": options}}));
        client.wait_for("initialize", |m| m["id"] == 1);
        client.notify("initialized", json!({}));
        client
    }

    pub fn send(&mut self, message: Value) {
        let body = message.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.stdin.flush().unwrap();
    }

    pub fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    /// Sends a request and returns the whole response message, `result` or `error`.
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        self.wait_for(method, |m| m["id"] == id && m.get("method").is_none())
    }

    /// Opens `path` with `text` and waits for its diagnostics, so that the server has
    /// analyzed the text before the next request.
    pub fn open(&mut self, path: &Path, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": uri(path), "languageId": "rdra", "version": 1,
                "text": text}}),
        );
        self.wait_for_diagnostics(&uri(path), "diagnostics of the opened text", |_| true);
    }

    /// Replaces the text of `path` and waits for its diagnostics.
    pub fn change(&mut self, path: &Path, version: i64, text: &str) {
        self.notify(
            "textDocument/didChange",
            json!({"textDocument": {"uri": uri(path), "version": version},
                "contentChanges": [{"text": text}]}),
        );
        self.wait_for_diagnostics(&uri(path), "diagnostics of the changed text", |_| true);
    }

    pub fn wait_for(&self, what: &str, mut matches: impl FnMut(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(60);
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

    pub fn wait_for_diagnostics(
        &self,
        uri: &str,
        what: &str,
        mut ok: impl FnMut(&[Value]) -> bool,
    ) {
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

pub fn uri(path: &Path) -> String {
    url::Url::from_file_path(path).unwrap().to_string()
}

/// A fresh workspace root under the temp directory, with an `rdra.toml` and a `.git`.
pub fn workspace(prefix: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "{prefix}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(root.join("rdra.toml"), "[model]\nname = \"shop\"\n").unwrap();
    dunce::canonicalize(root).unwrap()
}

/// The LSP position of byte `offset` in `text`: a line index and a UTF-16 column.
pub fn position(text: &str, offset: usize) -> Value {
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let character: usize = before[line_start..].chars().map(char::len_utf16).sum();
    json!({"line": line, "character": character})
}

/// The byte offset of an LSP position in `text`, counting columns in UTF-16 units.
pub fn offset(text: &str, position: &Value) -> usize {
    let line = position["line"].as_u64().unwrap() as usize;
    let character = position["character"].as_u64().unwrap() as usize;
    let line_start = if line == 0 {
        0
    } else {
        text.match_indices('\n')
            .nth(line - 1)
            .map(|(index, _)| index + 1)
            .unwrap_or_else(|| panic!("no line {line} in {text:?}"))
    };
    let mut units = 0;
    for (index, ch) in text[line_start..].char_indices() {
        if units == character {
            return line_start + index;
        }
        assert!(
            ch != '\n',
            "column {character} is past the end of line {line}"
        );
        units += ch.len_utf16();
    }
    assert_eq!(
        units, character,
        "column {character} is past the end of the text"
    );
    text.len()
}
