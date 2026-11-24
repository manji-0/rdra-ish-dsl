//! The `perf` feature's counters and lines (`crate::perf`).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Allocations and reallocations since the start.
static ALLOCS: AtomicU64 = AtomicU64::new(0);
/// Bytes they asked for.
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
/// Bytes allocated and not yet freed.
static LIVE: AtomicU64 = AtomicU64::new(0);
/// The most `LIVE` has been. A `realloc` counts the old and the new block, which are both
/// alive while it copies, so this bounds the live heap from above.
static PEAK: AtomicU64 = AtomicU64::new(0);
/// The most the command's memory pool has held (`MemoryPool`).
static POOL_PEAK: AtomicU64 = AtomicU64::new(0);

/// `RDRA_PERF`: 0 prints nothing, 1 the pass and run lines, 2 the costly columns too.
pub fn level() -> u8 {
    static LEVEL: OnceLock<u8> = OnceLock::new();
    *LEVEL.get_or_init(|| {
        std::env::var("RDRA_PERF")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    })
}

/// The global allocator of a `perf` build: `System`, counting what it hands out.
pub struct Counting;

// SAFETY: every call goes to `System` with the arguments it was given; the counters are
// atomics and allocate nothing.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        grew(layout.size() as u64);
        // SAFETY: the caller's contract for `alloc`, passed on.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        grew(layout.size() as u64);
        // SAFETY: the caller's contract for `alloc_zeroed`, passed on.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as u64, Relaxed);
        // SAFETY: the caller's contract for `dealloc`, passed on.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        grew(new_size as u64);
        LIVE.fetch_sub(layout.size() as u64, Relaxed);
        // SAFETY: the caller's contract for `realloc`, passed on.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

fn grew(bytes: u64) {
    ALLOCS.fetch_add(1, Relaxed);
    ALLOC_BYTES.fetch_add(bytes, Relaxed);
    let live = LIVE.fetch_add(bytes, Relaxed) + bytes;
    PEAK.fetch_max(live, Relaxed);
}

/// Notes what the command's memory pool holds after a charge.
pub(crate) fn pool_held(bytes: u64) {
    POOL_PEAK.fetch_max(bytes, Relaxed);
}

/// CPU time in ns: the process's, or with `thread` the calling thread's. 0 where the platform
/// has no such clock here.
pub fn cpu_ns(thread: bool) -> u64 {
    #[cfg(all(
        target_pointer_width = "64",
        any(target_os = "macos", target_os = "linux")
    ))]
    {
        #[repr(C)]
        struct Timespec {
            sec: i64,
            nsec: i64,
        }
        extern "C" {
            fn clock_gettime(id: i32, ts: *mut Timespec) -> i32;
        }
        // CLOCK_PROCESS_CPUTIME_ID and CLOCK_THREAD_CPUTIME_ID.
        let id = match (cfg!(target_os = "macos"), thread) {
            (true, false) => 12,
            (true, true) => 16,
            (false, false) => 2,
            (false, true) => 3,
        };
        let mut ts = Timespec { sec: 0, nsec: 0 };
        // SAFETY: `ts` is a valid timespec of the C library's layout on a 64-bit target, and
        // `id` one of the platform's clock ids.
        let ok = unsafe { clock_gettime(id, &mut ts) } == 0;
        match ok {
            true => ts.sec as u64 * 1_000_000_000 + ts.nsec as u64,
            false => 0,
        }
    }
    #[cfg(not(all(
        target_pointer_width = "64",
        any(target_os = "macos", target_os = "linux")
    )))]
    {
        let _ = thread;
        0
    }
}

/// Where the current pass started.
struct Mark {
    name: &'static str,
    wall: Instant,
    cpu: u64,
    allocs: u64,
    bytes: u64,
}

static MARK: Mutex<Option<Mark>> = Mutex::new(None);

const MIB: f64 = 1024.0 * 1024.0;

/// Ends the pass before, printing its line, and starts `name`. Runs started from here on
/// belong to `name` (`pass_name`).
pub fn pass(name: &'static str) {
    if level() == 0 {
        return;
    }
    let now = Mark {
        name,
        wall: Instant::now(),
        cpu: cpu_ns(false),
        allocs: ALLOCS.load(Relaxed),
        bytes: ALLOC_BYTES.load(Relaxed),
    };
    let mut mark = MARK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(m) = mark.as_ref() {
        eprintln!(
            "perf-pass name={} wall_ms={:.1} cpu_ms={:.1} allocs={} alloc_mb={:.1} live_mb={:.1} peak_live_mb={:.1} pool_peak_mb={:.1}",
            m.name,
            now.wall.duration_since(m.wall).as_secs_f64() * 1e3,
            now.cpu.saturating_sub(m.cpu) as f64 / 1e6,
            now.allocs - m.allocs,
            (now.bytes - m.bytes) as f64 / MIB,
            LIVE.load(Relaxed) as f64 / MIB,
            PEAK.load(Relaxed) as f64 / MIB,
            POOL_PEAK.load(Relaxed) as f64 / MIB,
        );
    }
    *mark = Some(now);
}

/// The pass the runs belong to.
pub fn pass_name() -> &'static str {
    MARK.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map_or("", |m| m.name)
}

thread_local! {
    static LABEL: Cell<&'static str> = const { Cell::new("") };
}

/// Labels the runs this thread starts until the guard drops (`S` for a quotient's run).
pub fn label(name: &'static str) -> Label {
    Label(LABEL.with(|l| l.replace(name)))
}

/// The label of the runs this thread starts.
pub fn label_name() -> &'static str {
    LABEL.with(Cell::get)
}

/// Restores the label before it when dropped.
pub struct Label(&'static str);

impl Drop for Label {
    fn drop(&mut self) {
        LABEL.with(|l| l.set(self.0));
    }
}
