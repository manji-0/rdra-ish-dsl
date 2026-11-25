//! File path ↔ LSP URI helpers shared across language-server modules.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use url::Url;

/// The URI of `path` after resolving symlinks: the path the analysis stores a source under.
/// A result sent to the client goes through [`ClientUris::uri`] instead.
pub fn path_to_uri(path: &Path) -> std::io::Result<Url> {
    let path = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Url::from_file_path(&path)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid path for uri"))
}

/// The URIs the client knows the workspace's files by. The analysis stores each source under
/// its canonical path; a client that opened the workspace through a symlink names the same
/// file by another path, and a diagnostic or a location under the canonical one lands on a
/// document it does not have open.
#[derive(Debug, Default, Clone)]
pub struct ClientUris {
    /// The URI each opened document was opened by, by its canonical path.
    opened: HashMap<PathBuf, Url>,
    /// Each workspace root: `(canonical path, the path the client named)`.
    roots: Vec<(PathBuf, PathBuf)>,
}

impl ClientUris {
    /// A workspace root, as the client named it.
    pub fn add_root(&mut self, root: &Path) {
        let canonical = dunce::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        if !self.roots.iter().any(|(c, _)| *c == canonical) {
            self.roots.push((canonical, root.to_path_buf()));
            // The deepest root first, so a nested root wins over the one holding it.
            self.roots
                .sort_by_key(|(c, _)| std::cmp::Reverse(c.components().count()));
        }
    }

    /// A document the client opened by `uri`.
    pub fn open(&mut self, uri: &Url) {
        if let Ok(path) = uri.to_file_path() {
            let canonical = dunce::canonicalize(&path).unwrap_or(path);
            self.opened.insert(canonical, uri.clone());
        }
    }

    /// The URI the client knows `path` by: the one it opened the document by, else the path
    /// under the workspace root as the client named the root, else the canonical path.
    pub fn uri(&self, path: &Path) -> std::io::Result<Url> {
        let canonical = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if let Some(uri) = self.opened.get(&canonical) {
            return Ok(uri.clone());
        }
        for (root, named) in &self.roots {
            if let Ok(rest) = canonical.strip_prefix(root) {
                if let Ok(uri) = Url::from_file_path(named.join(rest)) {
                    return Ok(uri);
                }
            }
        }
        path_to_uri(&canonical)
    }
}

pub fn paths_equal(a: &Path, b: &Path) -> bool {
    let ca = dunce::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let cb = dunce::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    ca == cb
}

pub fn watched_path_is_rdra(uri: &Url) -> bool {
    uri.to_file_path()
        .ok()
        .and_then(|path| path.extension().map(|ext| ext == "rdra"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_under_a_symlinked_root_keeps_the_root_the_client_named() {
        let dir = std::env::temp_dir().join(format!(
            "rdra_lsp_client_uris_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let real = dunce::canonicalize(&dir).unwrap().join("real");
        std::fs::create_dir_all(real.join("sub")).unwrap();
        std::fs::write(real.join("sub/a.rdra"), "").unwrap();
        std::fs::write(real.join("b.rdra"), "").unwrap();
        let link = real.with_file_name("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).unwrap();
        #[cfg(not(unix))]
        return;

        let mut uris = ClientUris::default();
        uris.add_root(&link);
        let opened = Url::from_file_path(link.join("b.rdra")).unwrap();
        uris.open(&opened);

        assert_eq!(uris.uri(&real.join("b.rdra")).unwrap(), opened);
        assert_eq!(
            uris.uri(&real.join("sub/a.rdra")).unwrap(),
            Url::from_file_path(link.join("sub/a.rdra")).unwrap()
        );
        let outside = dir.join("elsewhere.rdra");
        assert_eq!(uris.uri(&outside).unwrap(), path_to_uri(&outside).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
