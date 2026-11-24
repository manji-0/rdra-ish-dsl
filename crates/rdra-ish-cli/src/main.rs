fn main() -> anyhow::Result<()> {
    #[cfg(feature = "shadow")]
    shadow_report_at_exit();
    rdra_ish_cli::run()
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
