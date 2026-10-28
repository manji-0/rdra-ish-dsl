//! State diagnostics as `check` and the generator gates print them.

use rdra_ish_core::lifecycle::{derive_states, Caps, InvKind, InvText, Scope, Severity, StateDiag};
use rdra_ish_core::SemanticModel;

/// The word `check` and `states` print before a diagnostic.
pub(crate) fn severity_word(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
    }
}

/// Every diagnostic of the lifecycle views at the `states` defaults, with its severity: those
/// on an entity view first, then those that name no entity.
pub(crate) fn state_consistency_findings(model: &SemanticModel) -> Vec<(Severity, String)> {
    let report = derive_states(model, &Scope::All, Caps::DEFAULT_PATTERNS);
    let on_views = report.views.iter().flat_map(|result| {
        result.diagnostics.iter().map(move |diag| {
            (
                diag.severity(),
                format!(
                    "state derivation for entity '{}': {}",
                    result.entity_id,
                    state_diag_message(diag)
                ),
            )
        })
    });
    let unattributed = report
        .unattributed()
        .map(|diag| (diag.severity(), state_diag_message(diag)));
    on_views.chain(unattributed).collect()
}

/// Fail closed when state derivation has Error-severity diagnostics (generators).
pub(crate) fn reject_state_errors(model: &SemanticModel) -> anyhow::Result<()> {
    let mut has_error = false;
    for (severity, message) in state_consistency_findings(model) {
        if severity == Severity::Error {
            eprintln!("error: {message}");
            has_error = true;
        }
    }
    if has_error {
        anyhow::bail!("model has state errors; refusing to emit artifacts");
    }
    Ok(())
}

/// A constraint violation in the wording `check` has always used for its kind.
fn violation_message(kind: &InvKind, text: &InvText, p: &str) -> Option<String> {
    Some(match (kind, text) {
        (InvKind::Forbidden, InvText::Conditions(c)) => {
            format!("forbidden state is reachable: {c} witnessed by {p}")
        }
        (InvKind::Required, InvText::Conditions(c)) => {
            format!("required state is missing: {c} is not satisfied by {p}")
        }
        (InvKind::Exclusive, InvText::Conditions(c)) => {
            format!("exclusive state conditions co-occur: {c} witnessed by {p}")
        }
        (InvKind::Invariant, InvText::WhenThen { guards, requireds }) => {
            format!("invariant violated: when {guards} then {requireds} is broken by {p}")
        }
        (
            InvKind::CrossForbidden,
            InvText::Cross {
                entities,
                conditions,
            },
        ) => format!(
            "multi-entity forbidden state is reachable across [{entities}]: {conditions} witnessed by {p}"
        ),
        (
            InvKind::CrossInvariant,
            InvText::CrossWhenThen {
                entities,
                guards,
                requireds,
            },
        ) => format!(
            "multi-entity invariant violated across [{entities}]: when {guards} then {requireds} is broken by {p}"
        ),
        (
            InvKind::Quantifier,
            InvText::Quantifier {
                anchor,
                related,
                constraint,
            },
        ) => format!(
            "to-many quantifier constraint is violated from '{anchor}' to '{related}': {constraint} witnessed by {p}"
        ),
        _ => return None,
    })
}

pub(crate) fn state_diag_message(diag: &StateDiag) -> String {
    match diag {
        StateDiag::UnreachableEnumVariant { column, variant } => format!(
            "enum variant '{}.{}' is unreachable; add a create/default/transition/sets path or remove the variant",
            column, variant
        ),
        StateDiag::ConflictingEffects { usecase, column, .. } => format!(
            "usecase '{}' assigns conflicting effects to column '{}'; the last effect wins",
            usecase, column
        ),
        StateDiag::DoubleModeledEnum { column, .. } => format!(
            "enum column '{}' is driven by both transitions and sets; transitions are treated as the source of truth",
            column
        ),
        StateDiag::NoCreationPath => {
            "no creates path; state derivation is seeded from defaults only".to_string()
        }
        StateDiag::PatternCapReached { cap, bound } => format!(
            "pattern cap reached at {} while the theoretical state-space bound is {} (constraints on truncated set are not sound; raise --max-patterns or narrow --buc)",
            cap, bound
        ),
        StateDiag::ConstraintViolated {
            kind,
            text,
            pattern_desc,
            ..
        } => violation_message(kind, text, pattern_desc).unwrap_or_else(|| diag.to_string()),
        StateDiag::AfterAssertViolated {
            anchor,
            requireds,
            actual,
            ..
        } => format!(
            "temporal assertion violated after '{anchor}': expected {requireds}, but found {actual}"
        ),
        StateDiag::StateCapReached { .. }
        | StateDiag::AssertOnInertUseCase { .. }
        | StateDiag::IntLiteralOutOfBounds { .. }
        | StateDiag::UnbindableScope { .. }
        | StateDiag::UnwrittenConstrainedVar { .. }
        | StateDiag::UnsupportedComparison { .. }
        | StateDiag::UnresolvedPropertyAtom { .. } => diag.to_string(),
    }
}
