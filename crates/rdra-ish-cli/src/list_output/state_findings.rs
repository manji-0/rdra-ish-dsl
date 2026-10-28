//! State diagnostics as `check` and the generator gates print them, and the one renderer of
//! counterexample traces.

use rdra_ish_core::lifecycle::{
    derive_states, ActionOrigin, Caps, Change, InvKind, InvText, Scope, Severity, Site, StateDiag,
    TraceStep,
};
use rdra_ish_core::{format_location, ResolvedProgram, SemanticModel};

/// The word `check` and `states` print before a diagnostic.
pub(crate) fn severity_word(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
    }
}

/// Every diagnostic of the lifecycle views at the `states` defaults, with its severity: those
/// on an entity view first, then each violated constraint or assert once with its
/// counterexample, then those that name no entity.
pub(crate) fn state_consistency_findings(
    program: &ResolvedProgram,
    model: &SemanticModel,
) -> Vec<(Severity, String)> {
    let report = derive_states(model, &Scope::All, Caps::DEFAULT_PATTERNS);
    let on_views = report.views.iter().flat_map(|result| {
        let diags = result.diagnostics.iter().filter(|d| d.trace().is_none());
        diags.map(move |diag| {
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
    let violations = report.diags.iter().filter_map(|d| {
        let text = render_violation(program, model, &d.diag)?;
        Some((d.diag.severity(), text))
    });
    let unattributed = report
        .unattributed()
        .filter(|d| d.trace().is_none())
        .map(|diag| (diag.severity(), state_diag_message(diag)));
    on_views.chain(violations).chain(unattributed).collect()
}

fn location(program: &ResolvedProgram, site: &Site) -> Option<String> {
    format_location(program, site.span.as_ref()?)
}

/// A violation, the rule's site, and its counterexample: one step per line with its use case
/// site, and under it each changed value with the site of the effect that wrote it. `None`
/// for a diagnostic that is not a violation.
fn render_violation(
    program: &ResolvedProgram,
    model: &SemanticModel,
    diag: &StateDiag,
) -> Option<String> {
    let (head, site, trace) = match diag {
        StateDiag::ConstraintViolated {
            name, site, trace, ..
        } => (
            format!("invariant {name} is violated: {}", state_diag_message(diag)),
            site,
            trace,
        ),
        StateDiag::AfterAssertViolated { site, trace, .. } => {
            (state_diag_message(diag), site, trace)
        }
        _ => return None,
    };
    let mut out = head;
    if let Some(at) = location(program, site) {
        out.push_str(&format!("\n  rule: {at}"));
    }
    let steps = trace.steps.len();
    let plural = if steps == 1 { "" } else { "s" };
    out.push_str(&format!("\n  counterexample ({steps} step{plural}):"));
    let mut rows: Vec<(String, Option<String>)> = Vec::new();
    if !trace.start.is_empty() {
        rows.push(("    start".to_string(), None));
        rows.extend(trace.start.iter().map(|c| change_row(program, c)));
    }
    for (i, step) in trace.steps.iter().enumerate() {
        rows.push((
            format!("    {}. {}", i + 1, step_text(model, step)),
            location(program, &step.site),
        ));
        rows.extend(step.changes.iter().map(|c| change_row(program, c)));
    }
    let width = rows
        .iter()
        .map(|(text, _)| text.chars().count())
        .max()
        .unwrap_or(0);
    for (text, at) in rows {
        match at {
            Some(at) => {
                let pad = width - text.chars().count();
                out.push_str(&format!("\n{text}{}   {at}", " ".repeat(pad)));
            }
            None => out.push_str(&format!("\n{text}")),
        }
    }
    for entity in &trace.over_bound {
        out.push_str(&format!(
            "\n  note: the counterexample creates more {entity} instances than the instance bound allows; this check's cone does not count {entity} slots, so the whole model may not reach it at this bound"
        ));
    }
    Some(out)
}

/// `Pay_1, transaction 1 of Pay [Payment#1]`.
fn step_text(model: &SemanticModel, step: &TraceStep) -> String {
    let mut text = step.action.clone();
    if let ActionOrigin::UseCase { uc, .. } = &step.origin {
        let id = &model.use_cases[*uc].id;
        if let Some(n) = step.action.strip_prefix(&format!("{id}_")) {
            text.push_str(&format!(", transaction {n} of {id}"));
        }
    }
    if !step.bound.is_empty() {
        let bound: Vec<String> = step.bound.iter().map(ToString::to_string).collect();
        text.push_str(&format!(" [{}]", bound.join(", ")));
    }
    text
}

fn change_row(program: &ResolvedProgram, change: &Change) -> (String, Option<String>) {
    let indent = "         ";
    match change {
        Change::Create { at, values } if values.is_empty() => (format!("{indent}+ {at}"), None),
        Change::Create { at, values } => {
            let values: Vec<String> = values.iter().map(|(f, v)| format!("{f}={v}")).collect();
            (format!("{indent}+ {at} ({})", values.join(", ")), None)
        }
        Change::Delete { at } => (format!("{indent}- {at}"), None),
        Change::Set {
            at,
            field,
            from,
            to,
            cause,
        } => (
            format!("{indent}{at}.{field}: {from} -> {to}"),
            cause.as_ref().and_then(|site| location(program, site)),
        ),
        Change::Tick { from, to } => (format!("{indent}now: {from} -> {to}"), None),
    }
}

/// Fail closed when state derivation has Error-severity diagnostics (generators).
pub(crate) fn reject_state_errors(
    program: &ResolvedProgram,
    model: &SemanticModel,
) -> anyhow::Result<()> {
    let mut has_error = false;
    for (severity, message) in state_consistency_findings(program, model) {
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

/// Data-layer outputs read no reachability result, so lifecycle errors only earn a note.
pub(crate) fn note_state_errors(program: &ResolvedProgram, model: &SemanticModel) {
    let errors = state_consistency_findings(program, model)
        .into_iter()
        .filter(|(severity, _)| *severity == Severity::Error)
        .count();
    if errors > 0 {
        eprintln!(
            "note: the lifecycle has {errors} error(s) (see `rdra-ish check`); the data layer does not depend on them"
        );
    }
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
        (InvKind::Check, InvText::Conditions(c)) => {
            format!("check constraint is false: {c} is broken by {p}")
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
        (
            InvKind::Unique,
            InvText::Unique {
                child,
                parent,
                role,
            },
        ) => format!(
            "1:1 key is shared: two '{child}' rows belong to one '{parent}' through '{role}', witnessed by {p}"
        ),
        (
            InvKind::MinCard,
            InvText::MinCard {
                child,
                parent,
                role,
            },
        ) => format!(
            "minimum children unmet: a '{parent}' has no '{child}' through '{role}', witnessed by {p}"
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
        | StateDiag::ContradictoryDrivenComparison { .. }
        | StateDiag::AssertOnInertUseCase { .. }
        | StateDiag::IntLiteralOutOfBounds { .. }
        | StateDiag::UnbindableScope { .. }
        | StateDiag::UnbindableCondition { .. }
        | StateDiag::UnwrittenConstrainedVar { .. }
        | StateDiag::PreExistingChildOfCreatedParent { .. }
        | StateDiag::MinCardOfPreExistingParent { .. }
        | StateDiag::MinCardNeedsCreate { .. }
        | StateDiag::UnsupportedComparison { .. }
        | StateDiag::DerivedColumnRead { .. }
        | StateDiag::UnresolvedPropertyAtom { .. } => diag.to_string(),
    }
}
