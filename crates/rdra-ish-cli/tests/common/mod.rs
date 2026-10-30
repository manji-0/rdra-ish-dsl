//! Helpers shared by the CLI integration tests.

use std::path::{Path, MAIN_SEPARATOR};

/// `text` with every path under `root` made relative to it and, on Windows, each `\` written
/// as `/`, so an expected site reads `samples/x.rdra:3:1` on every OS. The CLI prints sites
/// with the OS separator.
pub fn relative_to(text: &str, root: &Path) -> String {
    let relative = text.replace(&format!("{}{MAIN_SEPARATOR}", root.display()), "");
    if cfg!(windows) {
        relative.replace('\\', "/")
    } else {
        relative
    }
}
