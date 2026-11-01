//! Model loading for the CLI.

use anyhow::{bail, Result};
use rdra_ish_core::{format_diagnostic_message, Diagnostic, RdraError, ResolvedProgram};

pub(crate) fn eprint_diagnostic(program: &ResolvedProgram, diag: &Diagnostic) {
    let message = format_diagnostic_message(
        Some(program),
        diag.is_warning,
        diag.location.as_ref(),
        &diag.error.to_string(),
    );
    eprintln!("{message}");
}

/// Make every `DeprecatedSyntax` warning an error (`check --deny-deprecated`).
pub(crate) fn deny_deprecated(diags: &mut [Diagnostic]) {
    for diag in diags {
        if matches!(diag.error, RdraError::DeprecatedSyntax { .. }) {
            diag.is_warning = false;
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_deprecated_makes_only_deprecations_errors() {
        let mut diags = vec![
            Diagnostic::warning(RdraError::DeprecatedSyntax {
                path: "m.rdra".into(),
                msg: "write `new(...)`".into(),
            }),
            Diagnostic::warning(RdraError::UndefinedSymbol { id: "X".into() }),
        ];
        deny_deprecated(&mut diags);
        let warnings: Vec<bool> = diags.iter().map(|d| d.is_warning).collect();
        assert_eq!(warnings, vec![false, true]);
    }
}
