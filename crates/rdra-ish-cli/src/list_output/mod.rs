//! List/table/CSV/JSON formatters for the `list` CLI command.

use anyhow::Result;
use rdra_ish_core::{consistency_diagnostics, LintIssue};

use crate::cli::{ListFormat, ListKind, StatesFormat};
mod access;
mod elements;
mod node_ref;
mod render;

use access::{format_actor_permission_audit, format_business_inputs, format_permission_callables};
use elements::{
    format_adr_impacts, format_adrs, format_apis, format_constraints, format_dtos, format_entities,
    format_fields, format_nfrs, format_requirements, format_usecases,
};
use render::{format_rows, format_sorted_id_labels};

macro_rules! list_id_label {
    ($model:expr, $field:ident, $format:expr, $label:expr) => {
        format_sorted_id_labels($model.$field.values(), $format, $label, |item| {
            (item.id.as_str(), item.label.as_str())
        })
    };
}

pub(crate) fn consistency_warnings(model: &rdra_ish_core::SemanticModel) -> Vec<String> {
    let mut warnings: Vec<String> = consistency_diagnostics(model)
        .into_iter()
        .map(|diag| diag.error.to_string())
        .collect();

    for result in
        rdra_ish_core::derive_state_patterns(model, &[], rdra_ish_core::DEFAULT_PATTERN_CAP)
    {
        for diag in result.diagnostics {
            warnings.push(format!(
                "state derivation for entity '{}': {}",
                result.entity_id,
                state_diag_message(&diag)
            ));
        }
    }

    warnings
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
pub(crate) fn state_diag_message(diag: &rdra_ish_core::StateDiag) -> String {
    match diag {
        rdra_ish_core::StateDiag::UnreachableEnumVariant { column, variant } => format!(
            "enum variant '{}.{}' is unreachable; add a create/default/transition/sets path or remove the variant",
            column, variant
        ),
        rdra_ish_core::StateDiag::ConflictingEffects { usecase, column } => format!(
            "usecase '{}' assigns conflicting effects to column '{}'; the last effect wins",
            usecase, column
        ),
        rdra_ish_core::StateDiag::DoubleModeledEnum { column } => format!(
            "enum column '{}' is driven by both transitions and sets; transitions are treated as the source of truth",
            column
        ),
        rdra_ish_core::StateDiag::NoCreationPath => {
            "no creates path; state derivation is seeded from defaults only".to_string()
        }
        rdra_ish_core::StateDiag::PatternCapReached { cap, bound } => format!(
            "pattern cap reached at {} while the theoretical state-space bound is {}",
            cap, bound
        ),
        rdra_ish_core::StateDiag::ForbiddenStateViolated {
            conditions,
            pattern_desc,
            correlation_hint,
        } => {
            let mut message = format!(
                "forbidden state is reachable: {} witnessed by {}",
                conditions, pattern_desc
            );
            if let Some(hint) = correlation_hint {
                message.push_str(&format!("; hint: {}", hint));
            }
            message
        }
        rdra_ish_core::StateDiag::InvariantViolated {
            guards,
            requireds,
            pattern_desc,
            flow_order_hint,
        } => {
            let mut message = format!(
                "invariant violated: when {} then {} is broken by {}",
                guards, requireds, pattern_desc
            );
            if let Some(hint) = flow_order_hint {
                message.push_str(&format!("; hint: {}", hint));
            }
            message
        }
        rdra_ish_core::StateDiag::RequiredStateViolated {
            conditions,
            pattern_desc,
        } => format!(
            "required state is missing: {} is not satisfied by {}",
            conditions, pattern_desc
        ),
        rdra_ish_core::StateDiag::ExclusiveStateViolated {
            conditions,
            pattern_desc,
        } => format!(
            "exclusive state conditions co-occur: {} witnessed by {}",
            conditions, pattern_desc
        ),
        rdra_ish_core::StateDiag::CrossForbiddenViolated {
            entities,
            conditions,
            pattern_desc,
            scope_hint,
        } => {
            let mut message = format!(
                "multi-entity forbidden state is reachable across [{}]: {} witnessed by {}",
                entities, conditions, pattern_desc
            );
            if let Some(hint) = scope_hint {
                message.push_str(&format!("; hint: {}", hint));
            }
            message
        }
        rdra_ish_core::StateDiag::CrossInvariantViolated {
            entities,
            guards,
            requireds,
            pattern_desc,
            scope_hint,
        } => {
            let mut message = format!(
                "multi-entity invariant violated across [{}]: when {} then {} is broken by {}",
                entities, guards, requireds, pattern_desc
            );
            if let Some(hint) = scope_hint {
                message.push_str(&format!("; hint: {}", hint));
            }
            message
        }
        rdra_ish_core::StateDiag::CrossConstraintNotEvaluated {
            entities,
            constraint,
            reason,
        } => format!(
            "multi-entity constraint was not fully evaluated across [{}]: {} ({})",
            entities, constraint, reason
        ),
        rdra_ish_core::StateDiag::TemporalAssertionViolated {
            anchor,
            requireds,
            actual,
        } => format!(
            "temporal assertion violated after '{}': expected {}, but {}",
            anchor, requireds, actual
        ),
        rdra_ish_core::StateDiag::TemporalAssertionNotEvaluated {
            anchor,
            requireds,
            reason,
        } => format!(
            "temporal assertion after '{}' was not evaluated: {} ({})",
            anchor, requireds, reason
        ),
        rdra_ish_core::StateDiag::QuantifierConstraintNotEvaluated {
            anchor,
            related,
            constraint,
            reason,
        } => format!(
            "to-many quantifier constraint was not evaluated from '{}' to '{}': {} ({})",
            anchor, related, constraint, reason
        ),
        rdra_ish_core::StateDiag::UndrivenComparisonProp {
            proposition,
            usage,
            effect,
        } => format!(
            "comparison proposition '{}' used in {} is not driven by sets(..., <comparison>, true/false): {}",
            proposition, usage, effect
        ),
    }
}

pub(crate) fn list_elements(
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
        ListKind::Concept => list_id_label!(model, concepts, format, "concepts"),
        ListKind::DomainObject => list_id_label!(model, domain_objects, format, "domain objects"),
        ListKind::Aggregate => list_id_label!(model, aggregates, format, "aggregates"),
        ListKind::ValueObject => list_id_label!(model, value_objects, format, "value objects"),
        ListKind::System => list_id_label!(model, systems, format, "systems"),
        ListKind::Api => format_apis(model, format),
        ListKind::Dto => format_dtos(model, format),
        ListKind::PermissionCallables => format_permission_callables(model, format),
        ListKind::ActorPermissionAudit => format_actor_permission_audit(model, format),
        ListKind::BusinessInputs => format_business_inputs(model, format),
    }
}
pub(crate) fn filter_entity_output(output: &str, entity_id: &str, format: &StatesFormat) -> String {
    match format {
        StatesFormat::Table => {
            // "Entity: <id>" で始まるブロックを切り出す
            let prefix = format!("Entity: {} ", entity_id);
            let mut in_block = false;
            let mut block = String::new();
            for line in output.lines() {
                if line.starts_with("Entity: ") {
                    if in_block {
                        break; // 次のエンティティが来たら終了
                    }
                    if line.starts_with(&prefix) {
                        in_block = true;
                    }
                }
                if in_block {
                    block.push_str(line);
                    block.push('\n');
                }
            }
            block
        }
        StatesFormat::Csv => {
            // entity_id カラム（第1列）でフィルタ
            let mut filtered = String::new();
            for (i, line) in output.lines().enumerate() {
                if i == 0 {
                    filtered.push_str(line);
                    filtered.push('\n');
                    continue;
                }
                if line
                    .split(',')
                    .next()
                    .is_some_and(|id| id.trim_matches('"') == entity_id)
                {
                    filtered.push_str(line);
                    filtered.push('\n');
                }
            }
            filtered
        }
        StatesFormat::Json => {
            // JSON 配列から entity_id が一致するオブジェクトのみ残す
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(output) {
                let filtered: Vec<_> = arr
                    .into_iter()
                    .filter(|v| {
                        v.get("entity_id")
                            .and_then(|id| id.as_str())
                            .is_some_and(|id| id == entity_id)
                    })
                    .collect();
                serde_json::to_string_pretty(&filtered).unwrap_or_default() + "\n"
            } else {
                output.to_string()
            }
        }
        StatesFormat::TypeScript => {
            let marker = format!("/** Reachable state variants for {entity_id}. */");
            let mut blocks: Vec<&str> = output
                .split("\n\n")
                .filter(|block| block.contains(&marker))
                .collect();
            if blocks.is_empty() {
                return "// Generated by rdra-ish. Do not edit manually.\n\n".to_string();
            }
            if !blocks[0].starts_with("// Generated by rdra-ish") {
                blocks.insert(0, "// Generated by rdra-ish. Do not edit manually.");
            }
            blocks.join("\n\n") + "\n"
        }
    }
}
