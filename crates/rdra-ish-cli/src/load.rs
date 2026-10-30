//! Model loading for the CLI.

use anyhow::{bail, Result};
use rdra_ish_core::{format_diagnostic_message, Diagnostic, ResolvedProgram};

pub(crate) fn eprint_diagnostic(program: &ResolvedProgram, diag: &Diagnostic) {
    let message = format_diagnostic_message(
        Some(program),
        diag.is_warning,
        diag.location.as_ref(),
        &diag.error.to_string(),
    );
    eprintln!("{message}");
}

/// Print diagnostics and fail if any are errors (fail-closed for generators).
pub(crate) fn reject_model_errors(program: &ResolvedProgram, diags: &[Diagnostic]) -> Result<()> {
    let mut has_error = false;
    for diag in diags {
        eprint_diagnostic(program, diag);
        if !diag.is_warning {
            has_error = true;
        }
    }
    if has_error {
        bail!("model has errors; refusing to continue");
    }
    Ok(())
}
