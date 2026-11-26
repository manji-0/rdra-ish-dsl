//! A finished command's output as text.

// A test binary that includes this file uses one of these shapes.
#![allow(dead_code)]

use std::process::Output;

/// stdout then stderr.
pub fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// stdout then stderr, each `\` in stderr written as `/` so that a site reads the same on
/// every OS.
pub fn text_slashed(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr).replace('\\', "/")
    )
}

/// The exit code, stdout, and stderr with each `\` written as `/`.
pub fn outcome(out: &Output) -> (Option<i32>, String, String) {
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).replace('\\', "/"),
    )
}
