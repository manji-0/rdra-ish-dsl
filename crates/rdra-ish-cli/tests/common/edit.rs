//! In-place text edits of a file a test copied.

// A test binary that includes this file may use only `edit`.
#![allow(dead_code)]

use std::path::Path;

pub fn edit(path: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains(from), "{} holds {from:?}", path.display());
    std::fs::write(path, text.replacen(from, to, 1)).unwrap();
}

/// Cuts the text from `from` to the first `to` after it, `to` included.
pub fn cut(path: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(path).unwrap();
    let start = text.find(from).unwrap_or_else(|| panic!("{from:?}"));
    let end = start + text[start..].find(to).unwrap_or_else(|| panic!("{to:?}")) + to.len();
    std::fs::write(path, format!("{}{}", &text[..start], &text[end..])).unwrap();
}
