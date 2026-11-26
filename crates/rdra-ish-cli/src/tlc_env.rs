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

/// How the JVM compiles TLC. C1 alone starts a run sooner and spends less CPU on one that
/// ends within a second (on an 87-state cone on JDK 17, 0.61 s wall and 0.26 user CPU-s against
/// 0.63 s and 0.38), and slows a long one by up to 67% (#309), so it is for runs known to be small.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Jit {
    /// The JVM's default: C1, then C2 for the hot code.
    Tiered,
    /// C1 only.
    C1,
}

impl Jit {
    /// The JVM flags of this setting. Every HotSpot JVM since JDK 8 takes them, but an unknown
    /// `-XX` flag stops a JVM, so a run gets them only once its TLC has been seen to start
    /// with them (`Tlc::jit`).
    pub fn flags(self) -> &'static str {
        match self {
            Jit::Tiered => "",
            Jit::C1 => "-XX:TieredStopAtLevel=1",
        }
    }
}

/// `JAVA_TOOL_OPTIONS` for a TLC run of `heap`'s kind: the heap the user names
/// (`user_heap`), else the kind's default.
pub fn tlc_env(heap: Heap, tmpdir: &Path) -> (&'static str, String) {
    let user = current_user_heap().unwrap_or(heap.default_bytes());
    tlc_env_sized(user, tmpdir, Jit::Tiered)
}

/// `JAVA_TOOL_OPTIONS` for a TLC run with a heap of `bytes`: the heap, a `java.io.tmpdir` of
/// its own, the flags of `jit`, then the user's `JAVA_TOOL_OPTIONS`. The JVM takes the last
/// value of a flag, so the user's wins.
pub fn tlc_env_sized(bytes: u64, tmpdir: &Path, jit: Jit) -> (&'static str, String) {
    let user = std::env::var("JAVA_TOOL_OPTIONS").ok();
    (
        "JAVA_TOOL_OPTIONS",
        java_tool_options(bytes, tmpdir, jit, user.as_deref()),
    )
}

/// `user_heap` of this process's environment.
pub fn current_user_heap() -> Option<u64> {
    user_heap(
        std::env::var("RDRA_TLC_HEAP").ok().as_deref(),
        std::env::var("JAVA_TOOL_OPTIONS").ok().as_deref(),
    )
}

/// Whether `RDRA_TLC_HEAP=states` asks `verify` to raise a cone run's heap by the states BFS
/// counted in it (`TlcHeap::with_states`, #384). An `-Xmx` in `JAVA_TOOL_OPTIONS` wins.
pub fn heap_from_states() -> bool {
    states_heap(
        std::env::var("RDRA_TLC_HEAP").ok().as_deref(),
        std::env::var("JAVA_TOOL_OPTIONS").ok().as_deref(),
    )
}

fn states_heap(tlc_heap: Option<&str>, java_tool_options: Option<&str>) -> bool {
    tlc_heap.is_some_and(|h| h.trim().eq_ignore_ascii_case("states"))
        && user_heap(None, java_tool_options).is_none()
}

/// The heap the user names for TLC: the last `-Xmx` of `JAVA_TOOL_OPTIONS`, which the JVM
/// takes over the one `verify` passes, else `RDRA_TLC_HEAP` (such as `4g`; `states` names
/// none, see `heap_from_states`).
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
pub(crate) fn xmx(bytes: u64) -> String {
    match bytes {
        b if b.is_multiple_of(GIB) => format!("{}g", b / GIB),
        b if b.is_multiple_of(MIB) => format!("{}m", b / MIB),
        b if b.is_multiple_of(1024) => format!("{}k", b / 1024),
        b => b.to_string(),
    }
}

fn java_tool_options(bytes: u64, tmpdir: &Path, jit: Jit, user: Option<&str>) -> String {
    let mut opts = format!("-Xmx{} -Djava.io.tmpdir={}", xmx(bytes), tmpdir.display());
    if jit != Jit::Tiered {
        opts.push(' ');
        opts.push_str(jit.flags());
    }
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
            java_tool_options(Heap::Cone.default_bytes(), tmp, Jit::Tiered, None),
            "-Xmx1g -Djava.io.tmpdir=/w/Cone_A.tmp"
        );
        assert_eq!(
            java_tool_options(Heap::Temporal.default_bytes(), tmp, Jit::Tiered, Some(" ")),
            "-Xmx2g -Djava.io.tmpdir=/w/Cone_A.tmp"
        );
        assert_eq!(
            java_tool_options(768 * MIB, tmp, Jit::Tiered, None),
            "-Xmx768m -Djava.io.tmpdir=/w/Cone_A.tmp"
        );
    }

    #[test]
    fn the_user_value_comes_last() {
        let tmp = Path::new("/w/t");
        assert_eq!(
            java_tool_options(4 * GIB, tmp, Jit::Tiered, Some("-Xmx8g -Dx=1")),
            "-Xmx4g -Djava.io.tmpdir=/w/t -Xmx8g -Dx=1"
        );
        assert_eq!(
            java_tool_options(GIB, tmp, Jit::C1, Some("-XX:TieredStopAtLevel=4")),
            "-Xmx1g -Djava.io.tmpdir=/w/t -XX:TieredStopAtLevel=1 -XX:TieredStopAtLevel=4"
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
        assert_eq!(user_heap(Some("states"), None), None);
        assert!(states_heap(Some(" States "), Some("-Dx=1")));
        assert!(!states_heap(Some("states"), Some("-Xmx2g")));
        assert!(!states_heap(Some("4g"), None));
        assert!(!states_heap(None, None));
        assert_eq!(jvm_size("1073741824"), Some(GIB));
    }
}
