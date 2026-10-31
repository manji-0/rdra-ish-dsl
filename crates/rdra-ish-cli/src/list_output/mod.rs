//! List/table/CSV/JSON formatters for the `list` CLI command.

use anyhow::Result;
use rdra_ish_core::LintIssue;

use crate::cli::{ListFormat, ListKind};
mod access;
mod elements;
mod node_ref;
mod render;
mod state_findings;

use access::{format_actor_permission_audit, format_business_inputs, format_permission_callables};
use elements::{
    format_adr_impacts, format_adrs, format_apis, format_conditions, format_constraints,
    format_dtos, format_entities, format_fields, format_nfrs, format_requirements, format_usecases,
};
use render::{format_rows, format_sorted_id_labels};
#[cfg(test)]
pub(crate) use state_findings::state_consistency_findings;
pub(crate) use state_findings::{
    note_state_errors, reject_state_errors, render_violation, state_diag_message, state_findings,
    Finding, TraceSource,
};

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
/// Render one `list --kind`. `program` locates each row of `--kind dependency`; without it
/// the file and line columns stay empty.
pub(crate) fn list_elements(
    program: Option<&rdra_ish_core::ResolvedProgram>,
    model: &rdra_ish_core::SemanticModel,
    kind: &ListKind,
    format: &ListFormat,
) -> Result<String> {
    match kind {
        ListKind::Actor => list_id_label!(model, actors, format, "actors"),
        ListKind::Buc => list_id_label!(model, bucs, format, "BUCs"),
        ListKind::Flow => list_id_label!(model, flows, format, "flows"),
        ListKind::Step => list_id_label!(model, steps, format, "steps"),
        ListKind::Usecase => format_usecases(model, format),
        ListKind::Field => format_fields(model, format),
        ListKind::Entity => format_entities(model, format),
        ListKind::Requirement => format_requirements(model, format),
        ListKind::Adr => format_adrs(model, format),
        ListKind::AdrImpact => format_adr_impacts(model, format),
        ListKind::Nfr => format_nfrs(model, format),
        ListKind::Quality => list_id_label!(model, qualities, format, "qualities"),
        ListKind::Constraint => format_constraints(model, format),
        ListKind::System => list_id_label!(model, systems, format, "systems"),
        ListKind::Api => format_apis(model, format),
        ListKind::Dto => format_dtos(model, format),
        ListKind::Variation => list_id_label!(model, variations, format, "variations"),
        ListKind::Condition => format_conditions(model, format),
        ListKind::PermissionCallables => format_permission_callables(model, format),
        ListKind::ActorPermissionAudit => format_actor_permission_audit(model, format),
        ListKind::BusinessInputs => format_business_inputs(model, format),
        ListKind::Surface => Ok(rdra_ish_core::Surface::of(model).text()),
        ListKind::Dependency => format_dependencies(program, model, format),
    }
}

/// `list --kind dependency`: every relation with a foreign end, as `use` (the predicate), the
/// foreign element, the local element, and the file and line of the predicate.
fn format_dependencies(
    program: Option<&rdra_ish_core::ResolvedProgram>,
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    use rdra_ish_core::model::RelKind;
    let headers = ["use", "foreign", "local", "file", "line"];
    let mut rows: Vec<[String; 5]> = Vec::new();
    for rel in &model.relations {
        let (foreign, local) = match (model.is_foreign(&rel.from), model.is_foreign(&rel.to)) {
            (true, _) => (&rel.from, &rel.to),
            (_, true) => (&rel.to, &rel.from),
            _ => continue,
        };
        let use_ = match rel.kind {
            RelKind::Invokes => "invokes",
            RelKind::Triggers => "triggers",
            RelKind::HasPermission => "has_permission",
            RelKind::Reads => "reads",
            RelKind::Shows => "shows",
            _ => continue,
        };
        let (file, line) = match (program, &rel.site) {
            (Some(program), Some(site)) => (
                site.path(program)
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                site.start_position(program)
                    .map(|p| p.line.to_string())
                    .unwrap_or_default(),
            ),
            _ => (String::new(), String::new()),
        };
        rows.push([
            use_.to_string(),
            node_ref::node_id(model, foreign).unwrap_or_default(),
            node_ref::node_id(model, local).unwrap_or_default(),
            file,
            line,
        ]);
    }
    rows.sort();
    format_rows(&headers, &rows, format, "dependency references")
}
