//! The allocator of this crate's tests: the binaries' `Allocator`, counting the bytes each
//! thread holds, so that a test can see what the code it runs on its own thread allocates.

use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;

use crate::allocator::Allocator;

struct Counting;

#[global_allocator]
static GLOBAL: Counting = Counting;

thread_local! {
    /// The bytes this thread has allocated less those it freed, and the most that has been.
    static HELD: Cell<(i64, i64)> = const { Cell::new((0, 0)) };
}

fn count(bytes: i64) {
    // A thread that is exiting has no counter left; what it frees then is not counted.
    let _ = HELD.try_with(|held| {
        let (now, peak) = held.get();
        let now = now + bytes;
        held.set((now, peak.max(now)));
    });
}

/// `f()`, and the most bytes this thread held beyond what it held before, while it ran. A
/// block that grows counts at its new size from the moment it grows: the copy it moves into
/// is not counted beside it.
pub(crate) fn most_held<R>(f: impl FnOnce() -> R) -> (R, u64) {
    let (start, _) = HELD.with(Cell::get);
    HELD.with(|held| held.set((start, start)));
    let r = f();
    let (_, peak) = HELD.with(Cell::get);
    (r, (peak - start) as u64)
}

// SAFETY: every call goes to `Allocator` as it came; the counter only reads sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size() as i64);
        // SAFETY: the caller's layout, passed on.
        unsafe { Allocator.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size() as i64);
        // SAFETY: the caller's layout, passed on.
        unsafe { Allocator.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count(-(layout.size() as i64));
        // SAFETY: the caller's block, passed on.
        unsafe { Allocator.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size as i64 - layout.size() as i64);
        // SAFETY: the caller's block, passed on.
        unsafe { Allocator.realloc(ptr, layout, new_size) }
    }
}
