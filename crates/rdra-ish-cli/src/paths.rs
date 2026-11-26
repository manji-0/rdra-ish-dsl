//! Paths as the CLI prints and writes them.

use std::path::{Component, Path, PathBuf};

/// `path` relative to `base` when it is under it, for a header or a message.
pub(crate) fn label(path: &Path, base: &Path) -> String {
    let absolute = |p: &Path| dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (p, b) = (absolute(path), absolute(base));
    match p.strip_prefix(&b) {
        Ok(rest) if rest.as_os_str().is_empty() => ".".to_string(),
        Ok(rest) => rest.display().to_string(),
        Err(_) => path.display().to_string(),
    }
}

/// `path` as the user can find it from the working directory: relative, with `..` where it
/// is not under it.
pub(crate) fn shown(path: &Path) -> String {
    let Ok(cwd) = std::env::current_dir() else {
        return path.display().to_string();
    };
    let resolve = |p: &Path| -> PathBuf {
        if let Ok(c) = dunce::canonicalize(p) {
            return c;
        }
        match (p.parent(), p.file_name()) {
            (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
                dunce::canonicalize(parent).map_or(p.to_path_buf(), |c| c.join(name))
            }
            _ => p.to_path_buf(),
        }
    };
    let (to, from) = (resolve(&cwd.join(path)), resolve(&cwd));
    let (a, b): (Vec<_>, Vec<_>) = (to.components().collect(), from.components().collect());
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    if common == 0 {
        return to.display().to_string();
    }
    let mut out = PathBuf::new();
    for _ in common..b.len() {
        out.push("..");
    }
    for c in &a[common..] {
        out.push(c.as_os_str());
    }
    if out.as_os_str().is_empty() {
        ".".to_string()
    } else {
        out.display().to_string()
    }
}

/// `to` as a path relative to the directory `from`, by their components, with `/` separators;
/// empty when they are the same.
pub(crate) fn relative(from: &Path, to: &Path) -> String {
    let from: Vec<Component> = from.components().collect();
    let to: Vec<Component> = to.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".to_string(); from.len() - common];
    parts.extend(
        to[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    parts.join("/")
}
