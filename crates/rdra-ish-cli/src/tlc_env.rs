//! The JVM environment of one TLC run, shared by `verify` and `tests/engine_equivalence.rs`
//! (which includes this file by path, so it stays std only).

use std::path::Path;

/// The heap a run gets: a cone holds one restricted system, the temporal run the whole model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heap {
    Cone,
    Temporal,
}

impl Heap {
    fn default_size(self) -> &'static str {
        match self {
            Heap::Cone => "1g",
            Heap::Temporal => "2g",
        }
    }
}

/// `JAVA_TOOL_OPTIONS` for a TLC run: the heap (`RDRA_TLC_HEAP`, such as `4g`, replaces the
/// default), a `java.io.tmpdir` of its own, then the user's `JAVA_TOOL_OPTIONS`. The JVM
/// takes the last `-Xmx`, so the user's value wins.
pub fn tlc_env(heap: Heap, tmpdir: &Path) -> (&'static str, String) {
    let size = std::env::var("RDRA_TLC_HEAP").ok();
    let user = std::env::var("JAVA_TOOL_OPTIONS").ok();
    (
        "JAVA_TOOL_OPTIONS",
        java_tool_options(heap, size.as_deref(), tmpdir, user.as_deref()),
    )
}

fn java_tool_options(heap: Heap, size: Option<&str>, tmpdir: &Path, user: Option<&str>) -> String {
    let size = size
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(heap.default_size());
    let mut opts = format!("-Xmx{size} -Djava.io.tmpdir={}", tmpdir.display());
    if let Some(user) = user.map(str::trim).filter(|u| !u.is_empty()) {
        opts.push(' ');
        opts.push_str(user);
    }
    opts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_per_run_kind() {
        let tmp = Path::new("/w/Cone_A.tmp");
        assert_eq!(
            java_tool_options(Heap::Cone, None, tmp, None),
            "-Xmx1g -Djava.io.tmpdir=/w/Cone_A.tmp"
        );
        assert_eq!(
            java_tool_options(Heap::Temporal, Some(""), tmp, Some(" ")),
            "-Xmx2g -Djava.io.tmpdir=/w/Cone_A.tmp"
        );
    }

    #[test]
    fn override_replaces_the_heap_and_the_user_value_comes_last() {
        let tmp = Path::new("/w/t");
        assert_eq!(
            java_tool_options(Heap::Temporal, Some("4g"), tmp, Some("-Xmx8g -Dx=1")),
            "-Xmx4g -Djava.io.tmpdir=/w/t -Xmx8g -Dx=1"
        );
    }
}
