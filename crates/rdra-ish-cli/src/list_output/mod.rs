//! List/table/CSV/JSON formatters for the `list` CLI command.

use anyhow::Result;
use rdra_ish_core::LintIssue;

use crate::cli::{ListFormat, ListKind};
mod access;
mod buc_roles;
mod elements;
mod node_ref;
mod render;
mod state_findings;
mod transitions;

use access::{format_actor_permission_audit, format_business_inputs, format_permission_callables};
use buc_roles::format_buc_roles;
pub(crate) use elements::{
    format_actors, format_adr_impacts, format_adrs, format_apis, format_bucs, format_conditions,
    format_constraints, format_dtos, format_entities, format_fields, format_goals, format_nfrs,
    format_requirements, format_rules, format_usecases,
};
use render::{format_rows, format_sorted_id_labels};
#[cfg(test)]
pub(crate) use state_findings::state_consistency_findings;
pub(crate) use state_findings::{
    note_state_errors, reject_state_errors, reject_state_errors_except_deadlock, render_violation,
    state_diag_message, state_findings, Finding, TraceSource,
};
pub(crate) use transitions::format_transitions;

macro_rules! list_id_label {
    ($model:expr, $field:ident, $format:expr, $label:expr) => {
        format_sorted_id_labels($model.$field.values(), $format, $label, |item| {
            (item.id.as_str(), item.label.as_str())
        })
    };
}

pub(crate) fn format_lint_issues(issues: &[LintIssue], format: &ListFormat) -> Result<String> {
    let headers = [
        "severity",
        "rule",
        "subject_kind",
        "subject_id",
        "message",
        "hint",
    ];
    let rows: Vec<[String; 6]> = issues
        .iter()
        .map(|issue| {
            [
                issue.severity.as_str().to_string(),
                issue.rule.to_string(),
                issue.subject_kind.clone(),
                issue.subject_id.clone(),
                issue.message.clone(),
                issue.hint.clone(),
            ]
        })
        .collect();

    format_rows(&headers, &rows, format, "lint issues")
}
pub(crate) fn list_elements(
    model: &rdra_ish_core::SemanticModel,
    kind: &ListKind,
    format: &ListFormat,
) -> Result<String> {
    match kind {
        ListKind::Actor => format_actors(model, format),
        ListKind::Buc => format_bucs(model, format),
        ListKind::Flow => list_id_label!(model, flows, format, "flows"),
        ListKind::Step => list_id_label!(model, steps, format, "steps"),
        ListKind::Usecase => format_usecases(model, format),
        ListKind::Field => format_fields(model, format),
        ListKind::Entity => format_entities(model, format),
        ListKind::Requirement => format_requirements(model, format),
        ListKind::Adr => format_adrs(model, format),
        ListKind::AdrImpact => format_adr_impacts(model, format),
        ListKind::Goal => format_goals(model, format),
        ListKind::Nfr => format_nfrs(model, format),
        ListKind::Quality => list_id_label!(model, qualities, format, "qualities"),
        ListKind::Constraint => format_constraints(model, format),
        ListKind::System => list_id_label!(model, systems, format, "systems"),
        ListKind::Api => format_apis(model, format),
        ListKind::Dto => format_dtos(model, format),
        ListKind::Variation => list_id_label!(model, variations, format, "variations"),
        ListKind::Screen => list_id_label!(model, screens, format, "screens"),
        ListKind::Event => list_id_label!(model, events, format, "events"),
        ListKind::Timer => list_id_label!(model, timers, format, "timers"),
        ListKind::Condition => format_conditions(model, format),
        ListKind::Rule => format_rules(model, None, format),
        ListKind::PermissionCallables => format_permission_callables(model, format),
        ListKind::ActorPermissionAudit => format_actor_permission_audit(model, format),
        ListKind::BusinessInputs => format_business_inputs(model, format),
        ListKind::BucRoles => format_buc_roles(model, format),
        ListKind::Transition => format_transitions(model, None, format),
        ListKind::Surface => Ok(rdra_ish_core::Surface::of(model).text()),
    }
}
