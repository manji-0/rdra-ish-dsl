/// A `perf` build counts every allocation (`rdra_ish_core::perf`).
#[cfg(feature = "perf")]
#[global_allocator]
static COUNTING: rdra_ish_core::perf::Counting = rdra_ish_core::perf::Counting;

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "shadow")]
    shadow_report_at_exit();
    #[cfg(feature = "perf")]
    perf_pass_at_exit();
    rdra_ish_cli::run()
}

/// Starts the `load` pass, and ends the last pass of a `perf` build however the process exits,
/// as `shadow_report_at_exit` prints the twins' counts.
#[cfg(feature = "perf")]
fn perf_pass_at_exit() {
    rdra_ish_core::perf::pass("load");
    extern "C" fn last() {
        rdra_ish_core::perf::pass("exit");
    }
    extern "C" {
        fn atexit(f: extern "C" fn()) -> std::os::raw::c_int;
    }
    // SAFETY: `atexit` from the C library the standard library links; `last` takes no
    // argument and does not unwind past it, since it only prints.
    unsafe {
        atexit(last);
    }
}

/// Prints the twins' counts (`rdra_ish_core::shadow::report`) however the process exits:
/// `check` and `verify` end a failing run with `std::process::exit`, which runs `atexit`
/// handlers and no destructor.
#[cfg(feature = "shadow")]
fn shadow_report_at_exit() {
    extern "C" fn report() {
        rdra_ish_core::shadow::report();
    }
    extern "C" {
        fn atexit(f: extern "C" fn()) -> std::os::raw::c_int;
    }
    // SAFETY: `atexit` from the C library the standard library links; `report` takes no
    // argument and does not unwind past it, since it only prints.
    unsafe {
        atexit(report);
    }
}
