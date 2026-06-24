//! File path ↔ LSP URI helpers shared across language-server modules.

use std::path::Path;

use url::Url;

pub fn path_to_uri(path: &Path) -> std::io::Result<Url> {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Url::from_file_path(&path)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid path for uri"))
}

pub fn paths_equal(a: &Path, b: &Path) -> bool {
    let ca = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let cb = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    ca == cb
}

pub fn watched_path_is_rdra(uri: &Url) -> bool {
    uri.to_file_path()
        .ok()
        .and_then(|path| path.extension().map(|ext| ext == "rdra"))
        .unwrap_or(false)
}
