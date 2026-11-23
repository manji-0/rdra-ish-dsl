//! The binaries' allocator gives freed memory back (#242). On macOS the system allocator
//! keeps a large block that was freed in the process's footprint, and reuses it only for a
//! request of about its size; a store that grows by doubling asks for every other size.

#[global_allocator]
static ALLOCATOR: rdra_ish_core::allocator::Allocator = rdra_ish_core::allocator::Allocator;

#[cfg(target_os = "macos")]
fn footprint() -> u64 {
    extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut u64) -> i32;
    }
    // `rusage_info_v4`: a 16-byte UUID, then `ri_phys_footprint` as its eighth `u64`.
    const RUSAGE_INFO_V4: i32 = 4;
    let mut info = [0u64; 64];
    // SAFETY: the buffer is larger than `rusage_info_v4`.
    let ok =
        unsafe { proc_pid_rusage(std::process::id() as i32, RUSAGE_INFO_V4, info.as_mut_ptr()) };
    assert_eq!(ok, 0, "proc_pid_rusage");
    info[2 + 7]
}

/// Buffers grown by doubling and dropped, one after another, leave nothing in the footprint:
/// what the process holds is what it has not freed.
#[cfg(target_os = "macos")]
#[test]
fn freed_buffers_leave_the_footprint() {
    const MIB: usize = 1 << 20;
    let before = footprint();
    for round in 0..4 {
        // Each round stops at another size, as stores of other state widths do.
        let target = (48 + 8 * round) * MIB;
        let mut store: Vec<u8> = Vec::new();
        while store.len() < target {
            let more = store.len().max(4096);
            store.reserve_exact(more);
            store.resize(store.len() + more, round as u8 + 1);
        }
        std::hint::black_box(&mut store);
        drop(store);
    }
    let kept = footprint().saturating_sub(before);
    assert!(
        kept < 16 * MIB as u64,
        "{} MiB of freed buffers still in the footprint",
        kept / MIB as u64
    );
}
