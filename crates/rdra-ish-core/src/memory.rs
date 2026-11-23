//! The memory a command may use (`--memory`, `memory` in `rdra.toml`), and the pool its BFS
//! runs charge their state stores to.

use std::cell::Cell;
use std::fmt;
use std::num::NonZeroU64;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

const KIB: u64 = 1 << 10;
const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;
const TIB: u64 = 1 << 40;

/// A number of bytes, written with a binary unit: `4GiB`, `512MiB`. `G`, `GB`, and `GiB`
/// all mean 2^30, and likewise for `K`, `M`, and `T`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemorySize(NonZeroU64);

impl MemorySize {
    /// What `check` and `states` may use unless told otherwise.
    pub const CHECK: MemorySize = MemorySize::gib(4);
    /// What `verify` may use unless told otherwise.
    pub const VERIFY: MemorySize = MemorySize::gib(16);

    const fn gib(n: u64) -> MemorySize {
        match NonZeroU64::new(n * GIB) {
            Some(bytes) => MemorySize(bytes),
            None => panic!("a size is not zero"),
        }
    }

    pub fn from_bytes(bytes: u64) -> Option<MemorySize> {
        NonZeroU64::new(bytes).map(MemorySize)
    }

    pub fn bytes(self) -> u64 {
        self.0.get()
    }

    /// The least size of whole GiB, or of whole MiB under 1GiB, that holds `bytes`: what to
    /// give `--memory` for a run that needs that much.
    pub fn covering(bytes: u64) -> MemorySize {
        let unit = if bytes > GIB { GIB } else { MIB };
        let rounded = bytes.div_ceil(unit).max(1).saturating_mul(unit);
        MemorySize(NonZeroU64::new(rounded).expect("a size is not zero"))
    }
}

/// Why a size did not parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BadSize(String);

impl fmt::Display for BadSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid memory size {:?}: write a whole number and a unit, such as 4GiB or \
             512MiB (G and GB also mean GiB)",
            self.0
        )
    }
}

impl std::error::Error for BadSize {}

impl FromStr for MemorySize {
    type Err = BadSize;

    fn from_str(text: &str) -> Result<Self, BadSize> {
        let bad = || BadSize(text.to_string());
        let trimmed = text.trim();
        let split = trimmed
            .find(|c: char| !c.is_ascii_digit())
            .ok_or_else(bad)?;
        let (digits, unit) = trimmed.split_at(split);
        let unit = match unit.trim().to_ascii_uppercase().as_str() {
            "K" | "KB" | "KIB" => KIB,
            "M" | "MB" | "MIB" => MIB,
            "G" | "GB" | "GIB" => GIB,
            "T" | "TB" | "TIB" => TIB,
            _ => return Err(bad()),
        };
        digits
            .parse::<u64>()
            .ok()
            .and_then(|n| n.checked_mul(unit))
            .and_then(MemorySize::from_bytes)
            .ok_or_else(bad)
    }
}

/// The largest unit that divides the size: `4GiB`, `1536MiB`, `100KiB`.
impl fmt::Display for MemorySize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = self.bytes();
        let (n, unit) = [(TIB, "TiB"), (GIB, "GiB"), (MIB, "MiB"), (KIB, "KiB")]
            .into_iter()
            .find(|(unit, _)| b.is_multiple_of(*unit))
            .map_or((b, "B"), |(unit, name)| (b / unit, name));
        write!(f, "{n}{unit}")
    }
}

impl<'de> serde::Deserialize<'de> for MemorySize {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// The bytes the BFS runs of one command hold, against the share of its memory they may
/// hold. A run charges its state store here as the store grows and stops at the memory limit
/// when the pool refuses. A finished run keeps charged what its result holds (`kept`) until
/// the result is dropped.
#[derive(Debug)]
pub struct MemoryPool {
    /// The limit a stopped run names.
    limit: MemorySize,
    /// What the pool lets its runs hold: `limit`, or less for a run that goes after others
    /// whose results stay (`with_room`).
    room: u64,
    /// How to raise `limit`, named in the message of a run that stops.
    raise: &'static str,
    held: AtomicU64,
    /// The pool this one is carved from, which counts what this one holds as well.
    parent: Option<&'static MemoryPool>,
}

impl MemoryPool {
    /// A pool for one command, alive until the process ends, so that the `Copy` caps of every
    /// run can name it. `raise` says how to raise `limit`, such as `--memory`.
    pub fn for_command(limit: MemorySize, raise: &'static str) -> &'static MemoryPool {
        MemoryPool::share_of(limit, limit, raise)
    }

    /// `for_command` for the runs that may hold only `share` of the command's `limit`. A run
    /// that stops names the limit.
    pub fn share_of(
        limit: MemorySize,
        share: MemorySize,
        raise: &'static str,
    ) -> &'static MemoryPool {
        Box::leak(Box::new(MemoryPool {
            limit,
            room: share.bytes(),
            raise,
            held: AtomicU64::new(0),
            parent: None,
        }))
    }

    /// A pool that lets its runs hold `room` bytes and names this pool's limit. What its runs
    /// hold counts in this pool too, past its limit if need be.
    pub(crate) fn with_room(&'static self, room: u64) -> &'static MemoryPool {
        Box::leak(Box::new(MemoryPool {
            room,
            held: AtomicU64::new(0),
            parent: Some(self),
            ..*self
        }))
    }

    pub fn limit(&self) -> MemorySize {
        self.limit
    }

    pub fn raise(&self) -> &'static str {
        self.raise
    }

    pub(crate) fn room(&self) -> u64 {
        self.room
    }

    /// Takes `bytes` from the pool; false, taking nothing, when the pool has less left.
    fn charge(&self, bytes: u64) -> bool {
        let taken = self
            .held
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |held| {
                held.checked_add(bytes).filter(|after| *after <= self.room)
            })
            .is_ok();
        if let (true, Some(parent)) = (taken, self.parent) {
            parent.held.fetch_add(bytes, Ordering::Relaxed);
        }
        taken
    }

    fn release(&self, bytes: u64) {
        self.held.fetch_sub(bytes, Ordering::Relaxed);
        if let Some(parent) = self.parent {
            parent.held.fetch_sub(bytes, Ordering::Relaxed);
        }
    }
}

/// Pools compare by identity: two runs share a budget only when they charge one pool.
impl PartialEq for MemoryPool {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for MemoryPool {}

/// Bytes a run holds charged to a pool; dropping it gives them back.
#[derive(Debug)]
pub(crate) struct Charge {
    pool: &'static MemoryPool,
    bytes: u64,
}

impl Charge {
    pub(crate) fn new(pool: &'static MemoryPool) -> Charge {
        Charge { pool, bytes: 0 }
    }

    /// Raises the charge to cover `bytes`, a mebibyte ahead when the pool has it so that a
    /// growing run seldom touches the pool; false, keeping the charge, when the pool has less
    /// than `bytes` left. A refusal of `bytes` within the pool's room is a crowded one: other
    /// runs hold the rest (`watch`).
    pub(crate) fn cover(&mut self, bytes: u64) -> bool {
        let covered = bytes <= self.bytes || self.grow_to(bytes + MIB) || self.grow_to(bytes);
        WATCH.with(|w| {
            let mut seen = w.get();
            if covered {
                seen.peak = seen.peak.max(bytes);
            } else if bytes <= self.pool.room {
                seen.crowded = true;
            }
            w.set(seen);
        });
        covered
    }

    /// Lowers the charge to `bytes`, giving the rest back.
    pub(crate) fn shrink_to(&mut self, bytes: u64) {
        if bytes < self.bytes {
            self.pool.release(self.bytes - bytes);
            self.bytes = bytes;
        }
    }

    /// Raises the charge to `bytes`; false, keeping the charge, when the pool refuses.
    fn grow_to(&mut self, bytes: u64) -> bool {
        if bytes <= self.bytes {
            return true;
        }
        if !self.pool.charge(bytes - self.bytes) {
            return false;
        }
        self.bytes = bytes;
        true
    }

    pub(crate) fn pool(&self) -> &'static MemoryPool {
        self.pool
    }
}

impl Drop for Charge {
    fn drop(&mut self) {
        self.pool.release(self.bytes);
    }
}

/// What the runs on one thread met while `watch` looked on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Watched {
    /// A run was refused memory it would have had alone: other runs held the rest.
    pub(crate) crowded: bool,
    /// The most bytes a run covered.
    pub(crate) peak: u64,
}

thread_local! {
    static WATCH: Cell<Watched> = const {
        Cell::new(Watched {
            crowded: false,
            peak: 0,
        })
    };
}

/// `f()`, and what its runs on this thread met. Each run's bytes are the same on every
/// machine; which run went first, and so what the others held, is not.
pub(crate) fn watch<R>(f: impl FnOnce() -> R) -> (R, Watched) {
    WATCH.set(Watched::default());
    let r = f();
    (r, WATCH.replace(Watched::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_take_binary_units_with_or_without_ib() {
        for (text, bytes) in [
            ("4GiB", 4 * GIB),
            ("4G", 4 * GIB),
            ("4GB", 4 * GIB),
            ("4gb", 4 * GIB),
            ("4096MiB", 4 * GIB),
            (" 512 MiB ", 512 * MIB),
            ("64K", 64 * KIB),
            ("1TiB", TIB),
        ] {
            assert_eq!(
                text.parse::<MemorySize>().map(MemorySize::bytes),
                Ok(bytes),
                "{text}"
            );
        }
    }

    #[test]
    fn a_size_needs_a_unit_and_is_not_zero() {
        for text in [
            "4096",
            "",
            "GiB",
            "0GiB",
            "4XB",
            "-1GiB",
            "1.5GiB",
            "99999999999TiB",
        ] {
            let err = text.parse::<MemorySize>().unwrap_err().to_string();
            assert!(err.contains("such as 4GiB"), "{text}: {err}");
        }
    }

    #[test]
    fn a_size_prints_in_its_largest_whole_unit() {
        for (text, shown) in [("4096MiB", "4GiB"), ("1536M", "1536MiB"), ("16G", "16GiB")] {
            assert_eq!(text.parse::<MemorySize>().unwrap().to_string(), shown);
        }
    }

    #[test]
    fn a_pool_refuses_past_its_limit_and_takes_back_what_is_dropped() {
        let pool = MemoryPool::for_command(MemorySize::from_bytes(100).unwrap(), "--memory");
        let mut a = Charge::new(pool);
        assert!(a.cover(60));
        let mut b = Charge::new(pool);
        let (refused, seen) = watch(|| b.cover(50));
        assert!(
            !refused && seen.crowded,
            "60 + 50 is past 100, and 50 alone is not"
        );
        let (covered, seen) = watch(|| b.cover(40));
        assert!(covered && !seen.crowded && seen.peak == 40);
        drop(a);
        assert!(b.cover(100));
        let (refused, seen) = watch(|| b.cover(101));
        assert!(!refused && !seen.crowded, "101 is past the limit alone");
        b.shrink_to(30);
        let mut c = Charge::new(pool.with_room(50));
        assert!(!c.cover(51), "past the room");
        assert!(c.cover(50));
        assert!(!Charge::new(pool).cover(21), "30 + 50 + 21 is past 100");
    }
}
