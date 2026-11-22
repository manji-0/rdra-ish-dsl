//! The JVM environment of one TLC run, shared by `verify` and `tests/engine_equivalence.rs`.

use std::path::Path;

const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;

/// The heap a run gets: a cone holds one restricted system, a temporal property run the whole model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heap {
    Cone,
    Temporal,
}

impl Heap {
    /// The heap a run of this kind gets when neither the user nor `verify`'s memory names
    /// one.
    pub fn default_bytes(self) -> u64 {
        match self {
            Heap::Cone => GIB,
            Heap::Temporal => 2 * GIB,
        }
    }
}

/// `JAVA_TOOL_OPTIONS` for a TLC run of `heap`'s kind: the heap the user names
/// (`user_heap`), else the kind's default.
pub fn tlc_env(heap: Heap, tmpdir: &Path) -> (&'static str, String) {
    let user = current_user_heap().unwrap_or(heap.default_bytes());
    tlc_env_sized(user, tmpdir)
}

/// `JAVA_TOOL_OPTIONS` for a TLC run with a heap of `bytes`: the heap, a `java.io.tmpdir` of
/// its own, then the user's `JAVA_TOOL_OPTIONS`. The JVM takes the last `-Xmx`, so the user's
/// value wins.
pub fn tlc_env_sized(bytes: u64, tmpdir: &Path) -> (&'static str, String) {
    let user = std::env::var("JAVA_TOOL_OPTIONS").ok();
    (
        "JAVA_TOOL_OPTIONS",
        java_tool_options(bytes, tmpdir, user.as_deref()),
    )
}

/// `user_heap` of this process's environment.
pub fn current_user_heap() -> Option<u64> {
    user_heap(
        std::env::var("RDRA_TLC_HEAP").ok().as_deref(),
        std::env::var("JAVA_TOOL_OPTIONS").ok().as_deref(),
    )
}

/// The heap the user names for TLC: the last `-Xmx` of `JAVA_TOOL_OPTIONS`, which the JVM
/// takes over the one `verify` passes, else `RDRA_TLC_HEAP` (such as `4g`).
pub fn user_heap(tlc_heap: Option<&str>, java_tool_options: Option<&str>) -> Option<u64> {
    let from_options = java_tool_options.and_then(|opts| {
        opts.split_whitespace()
            .filter_map(|o| o.strip_prefix("-Xmx"))
            .next_back()
            .and_then(jvm_size)
    });
    from_options.or_else(|| tlc_heap.and_then(jvm_size))
}

/// A JVM size, as `-Xmx` takes it: `4g`, `512m`, `65536k`, or bytes.
fn jvm_size(text: &str) -> Option<u64> {
    let text = text.trim();
    let (digits, unit) = match text.char_indices().last()? {
        (at, c) if c.is_ascii_alphabetic() => (&text[..at], c.to_ascii_lowercase()),
        _ => (text, 'b'),
    };
    let unit: u64 = match unit {
        'b' => 1,
        'k' => 1 << 10,
        'm' => MIB,
        'g' => GIB,
        't' => 1 << 40,
        _ => return None,
    };
    digits
        .parse::<u64>()
        .ok()?
        .checked_mul(unit)
        .filter(|b| *b > 0)
}

/// `-Xmx` in the largest unit that divides `bytes`.
fn xmx(bytes: u64) -> String {
    match bytes {
        b if b.is_multiple_of(GIB) => format!("{}g", b / GIB),
        b if b.is_multiple_of(MIB) => format!("{}m", b / MIB),
        b if b.is_multiple_of(1024) => format!("{}k", b / 1024),
        b => b.to_string(),
    }
}

fn java_tool_options(bytes: u64, tmpdir: &Path, user: Option<&str>) -> String {
    let mut opts = format!("-Xmx{} -Djava.io.tmpdir={}", xmx(bytes), tmpdir.display());
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
            java_tool_options(Heap::Cone.default_bytes(), tmp, None),
            "-Xmx1g -Djava.io.tmpdir=/w/Cone_A.tmp"
        );
        assert_eq!(
            java_tool_options(Heap::Temporal.default_bytes(), tmp, Some(" ")),
            "-Xmx2g -Djava.io.tmpdir=/w/Cone_A.tmp"
        );
        assert_eq!(
            java_tool_options(768 * MIB, tmp, None),
            "-Xmx768m -Djava.io.tmpdir=/w/Cone_A.tmp"
        );
    }

    #[test]
    fn the_user_value_comes_last() {
        let tmp = Path::new("/w/t");
        assert_eq!(
            java_tool_options(4 * GIB, tmp, Some("-Xmx8g -Dx=1")),
            "-Xmx4g -Djava.io.tmpdir=/w/t -Xmx8g -Dx=1"
        );
    }

    #[test]
    fn the_user_heap_is_the_last_xmx_else_rdra_tlc_heap() {
        assert_eq!(user_heap(None, None), None);
        assert_eq!(user_heap(Some(" "), None), None);
        assert_eq!(user_heap(Some("4g"), None), Some(4 * GIB));
        assert_eq!(
            user_heap(Some("4g"), Some("-Dx=1 -Xmx512m -Xmx3G")),
            Some(3 * GIB)
        );
        assert_eq!(user_heap(Some("4g"), Some("-Dfoo=1")), Some(4 * GIB));
        assert_eq!(user_heap(Some("lots"), None), None);
        assert_eq!(jvm_size("65536k"), Some(64 * MIB));
        assert_eq!(jvm_size("1073741824"), Some(GIB));
    }
}
