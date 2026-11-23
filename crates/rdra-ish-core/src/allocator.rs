//! The allocator the `rdra-ish` binaries run on, so that the memory a run frees leaves the
//! process (#242).
//!
//! On macOS the system allocator keeps a large block that is freed in the process's footprint
//! (its large-block cache; `MallocLargeCache=0` turns it off) and hands it out again only for
//! a request of about its size. A BFS store grows by doubling and is freed whole when its
//! findings are read, so the next store, growing through other sizes, took fresh memory
//! beside the blocks kept: `verify --instance-count 2 --memory 2GiB` on `oidc-auth` peaked at
//! 3795MiB with at most 2047MiB counted. `Allocator` maps each block of `LARGE` bytes and
//! more from the kernel and unmaps it when it is freed, so the footprint follows what is
//! live. A block grows by moving into a new mapping, held twice while it is copied; a BFS
//! store counts each buffer at its next size before it grows (`explore/store.rs`), which
//! covers both. Elsewhere it is the system allocator.

use std::alloc::{GlobalAlloc, Layout, System};

/// The system allocator, with blocks of `LARGE` bytes and more mapped one by one on macOS.
pub struct Allocator;

/// The least block mapped on its own: smaller ones are packed by the system allocator.
pub const LARGE: usize = 1 << 20;

/// The least page size of any Mac: a mapping is aligned to it, so a block that needs no more
/// alignment can be one.
const PAGE: usize = 4096;

#[cfg(target_os = "macos")]
mod mapped {
    use std::ffi::c_void;

    const PROT_READ: i32 = 0x1;
    const PROT_WRITE: i32 = 0x2;
    const MAP_PRIVATE: i32 = 0x2;
    const MAP_ANON: i32 = 0x1000;

    extern "C" {
        fn mmap(
            addr: *mut c_void,
            len: usize,
            prot: i32,
            flags: i32,
            fd: i32,
            off: i64,
        ) -> *mut c_void;
        fn munmap(addr: *mut c_void, len: usize) -> i32;
    }

    /// `size` fresh zeroed bytes; null when the kernel refuses.
    pub fn map(size: usize) -> *mut u8 {
        // SAFETY: an anonymous private mapping at an address the kernel picks touches no
        // memory the program holds.
        let p = unsafe {
            mmap(
                std::ptr::null_mut(),
                size,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANON,
                -1,
                0,
            )
        };
        if p as isize == -1 {
            std::ptr::null_mut()
        } else {
            p.cast()
        }
    }

    /// Gives back a block `map(size)` returned.
    ///
    /// # Safety
    /// `p` came from `map(size)` and is not used again.
    pub unsafe fn unmap(p: *mut u8, size: usize) {
        // SAFETY: the caller hands over the whole mapping.
        unsafe { munmap(p.cast(), size) };
    }
}

impl Allocator {
    /// Whether a block of this layout is mapped on its own.
    fn mapped(layout: Layout) -> bool {
        cfg!(target_os = "macos") && layout.size() >= LARGE && layout.align() <= PAGE
    }
}

// SAFETY: every block is either the system allocator's, passed through with its layout, or a
// mapping of exactly `layout.size()` bytes made and unmade here; `mapped` decides which from
// the layout alone, so `dealloc` and `realloc` see the same answer `alloc` did.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        #[cfg(target_os = "macos")]
        if Self::mapped(layout) {
            return mapped::map(layout.size());
        }
        // SAFETY: the caller's layout, passed on.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        #[cfg(target_os = "macos")]
        if Self::mapped(layout) {
            // A fresh mapping reads as zeros.
            return mapped::map(layout.size());
        }
        // SAFETY: the caller's layout, passed on.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        #[cfg(target_os = "macos")]
        if Self::mapped(layout) {
            // SAFETY: `alloc` mapped this block at this size.
            return unsafe { mapped::unmap(ptr, layout.size()) };
        }
        // SAFETY: the system allocator's block, with its layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: `GlobalAlloc::realloc` promises a valid layout of `new_size` at this
        // alignment.
        let new = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        if !Self::mapped(layout) && !Self::mapped(new) {
            // SAFETY: the system allocator's block, passed on.
            return unsafe { System.realloc(ptr, layout, new_size) };
        }
        // A mapped block moves: into a new block, copied, then the old one freed.
        // SAFETY: `new` is a valid layout of nonzero size.
        let moved = unsafe { self.alloc(new) };
        if !moved.is_null() {
            // SAFETY: both blocks hold at least the bytes copied and do not overlap.
            unsafe {
                std::ptr::copy_nonoverlapping(ptr, moved, layout.size().min(new_size));
                self.dealloc(ptr, layout);
            }
        }
        moved
    }
}
