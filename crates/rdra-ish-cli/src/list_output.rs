//! List/table/CSV/JSON formatters for the `list` CLI command.

use anyhow::Result;
use rdra_ish_core::model::{NodeRef, RelKind};
use rdra_ish_core::{
    consistency_diagnostics, derive_actor_input_inferences, derive_actor_permission_audit,
    derive_permission_callables, ActorInputSource, LintIssue,
};

use crate::cli::{ListFormat, ListKind, StatesFormat};

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

fn node_kind_name(node: &NodeRef) -> &'static str {
    match node {
        NodeRef::Actor(_) => "actor",
        NodeRef::ExtSystem(_) => "extsystem",
        NodeRef::System(_) => "system",
        NodeRef::Requirement(_) => "requirement",
        NodeRef::Adr(_) => "adr",
        NodeRef::Nfr(_) => "nfr",
        NodeRef::Quality(_) => "quality",
        NodeRef::Constraint(_) => "constraint",
        NodeRef::Concept(_) => "concept",
        NodeRef::DomainObject(_) => "domain-object",
        NodeRef::Aggregate(_) => "aggregate",
        NodeRef::ValueObject(_) => "value-object",
        NodeRef::Business(_) => "business",
        NodeRef::Buc(_) => "buc",
        NodeRef::Flow(_) => "flow",
        NodeRef::Step(_) => "step",
        NodeRef::UsageScene(_) => "usagescene",
        NodeRef::UseCase(_) => "usecase",
        NodeRef::Screen(_) => "screen",
        NodeRef::Field(_) => "field",
        NodeRef::Event(_) => "event",
        NodeRef::Entity(_) => "entity",
        NodeRef::State(_) => "state",
        NodeRef::Condition(_) => "condition",
        NodeRef::Variation(_) => "variation",
        NodeRef::Api(_) => "api",
        NodeRef::Dto(_) => "dto",
        NodeRef::Location(_) => "location",
        NodeRef::Timing(_) => "timing",
        NodeRef::Medium(_) => "medium",
        NodeRef::Permission(_) => "permission",
    }
}

fn node_id(model: &rdra_ish_core::SemanticModel, node: &NodeRef) -> Option<String> {
    Some(match node {
        NodeRef::Actor(key) => model.actors.get(*key)?.id.clone(),
        NodeRef::ExtSystem(key) => model.ext_systems.get(*key)?.id.clone(),
        NodeRef::System(key) => model.systems.get(*key)?.id.clone(),
        NodeRef::Requirement(key) => model.requirements.get(*key)?.id.clone(),
        NodeRef::Adr(key) => model.adrs.get(*key)?.id.clone(),
        NodeRef::Nfr(key) => model.nfrs.get(*key)?.id.clone(),
        NodeRef::Quality(key) => model.qualities.get(*key)?.id.clone(),
        NodeRef::Constraint(key) => model.constraints.get(*key)?.id.clone(),
        NodeRef::Concept(key) => model.concepts.get(*key)?.id.clone(),
        NodeRef::DomainObject(key) => model.domain_objects.get(*key)?.id.clone(),
        NodeRef::Aggregate(key) => model.aggregates.get(*key)?.id.clone(),
        NodeRef::ValueObject(key) => model.value_objects.get(*key)?.id.clone(),
        NodeRef::Business(key) => model.businesses.get(*key)?.id.clone(),
        NodeRef::Buc(key) => model.bucs.get(*key)?.id.clone(),
        NodeRef::Flow(key) => model.flows.get(*key)?.id.clone(),
        NodeRef::Step(key) => model.steps.get(*key)?.id.clone(),
        NodeRef::UsageScene(key) => model.usage_scenes.get(*key)?.id.clone(),
        NodeRef::UseCase(key) => model.use_cases.get(*key)?.id.clone(),
        NodeRef::Screen(key) => model.screens.get(*key)?.id.clone(),
        NodeRef::Field(key) => model.fields.get(*key)?.id.clone(),
        NodeRef::Event(key) => model.events.get(*key)?.id.clone(),
        NodeRef::Entity(key) => model.entities.get(*key)?.id.clone(),
        NodeRef::State(key) => model.states.get(*key)?.id.clone(),
        NodeRef::Condition(key) => model.conditions.get(*key)?.id.clone(),
        NodeRef::Variation(key) => model.variations.get(*key)?.id.clone(),
        NodeRef::Api(key) => model.apis.get(*key)?.id.clone(),
        NodeRef::Dto(key) => model.dtos.get(*key)?.id.clone(),
        NodeRef::Location(key) => model.locations.get(*key)?.id.clone(),
        NodeRef::Timing(key) => model.timings.get(*key)?.id.clone(),
        NodeRef::Medium(key) => model.media.get(*key)?.id.clone(),
        NodeRef::Permission(key) => model.permissions.get(*key)?.id.clone(),
    })
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
                "cross-entity forbidden state is reachable across [{}]: {} witnessed by {}",
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
                "cross-entity invariant violated across [{}]: when {} then {} is broken by {}",
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
            "cross-entity constraint was not fully evaluated across [{}]: {} ({})",
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

/// Build a table row separator line.
fn table_separator(col_widths: &[usize]) -> String {
    col_widths
        .iter()
        .map(|&w| "\u{2500}".repeat(w))
        .collect::<Vec<_>>()
        .join("  ")
}

pub(crate) fn list_elements(
    model: &rdra_ish_core::SemanticModel,
    kind: &ListKind,
    format: &ListFormat,
) -> Result<String> {
    match kind {
        ListKind::Actor => {
            let mut items: Vec<(&str, &str)> = model
                .actors
                .iter()
                .map(|(_, a)| (a.id.as_str(), a.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "actors")
        }
        ListKind::Buc => {
            let mut items: Vec<(&str, &str)> = model
                .bucs
                .iter()
                .map(|(_, b)| (b.id.as_str(), b.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "BUCs")
        }
        ListKind::Flow => {
            let mut items: Vec<(&str, &str)> = model
                .flows
                .iter()
                .map(|(_, f)| (f.id.as_str(), f.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "flows")
        }
        ListKind::Step => {
            let mut items: Vec<(&str, &str)> = model
                .steps
                .iter()
                .map(|(_, s)| (s.id.as_str(), s.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "steps")
        }
        ListKind::Usecase => format_usecases(model, format),
        ListKind::Field => format_fields(model, format),
        ListKind::Entity => format_entities(model, format),
        ListKind::Requirement => format_requirements(model, format),
        ListKind::Adr => format_adrs(model, format),
        ListKind::AdrImpact => format_adr_impacts(model, format),
        ListKind::Nfr => format_nfrs(model, format),
        ListKind::Quality => {
            let mut items: Vec<(&str, &str)> = model
                .qualities
                .iter()
                .map(|(_, q)| (q.id.as_str(), q.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "qualities")
        }
        ListKind::Constraint => format_constraints(model, format),
        ListKind::Concept => {
            let mut items: Vec<(&str, &str)> = model
                .concepts
                .iter()
                .map(|(_, c)| (c.id.as_str(), c.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "concepts")
        }
        ListKind::DomainObject => {
            let mut items: Vec<(&str, &str)> = model
                .domain_objects
                .iter()
                .map(|(_, d)| (d.id.as_str(), d.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "domain objects")
        }
        ListKind::Aggregate => {
            let mut items: Vec<(&str, &str)> = model
                .aggregates
                .iter()
                .map(|(_, a)| (a.id.as_str(), a.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "aggregates")
        }
        ListKind::ValueObject => {
            let mut items: Vec<(&str, &str)> = model
                .value_objects
                .iter()
                .map(|(_, v)| (v.id.as_str(), v.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "value objects")
        }
        ListKind::System => {
            let mut items: Vec<(&str, &str)> = model
                .systems
                .iter()
                .map(|(_, s)| (s.id.as_str(), s.label.as_str()))
                .collect();
            items.sort_by_key(|(id, _)| *id);
            format_id_label(&items, format, "systems")
        }
        ListKind::Api => format_apis(model, format),
        ListKind::Dto => format_dtos(model, format),
        ListKind::PermissionCallables => format_permission_callables(model, format),
        ListKind::ActorPermissionAudit => format_actor_permission_audit(model, format),
        ListKind::BusinessInputs => format_business_inputs(model, format),
    }
}

fn format_business_inputs(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let headers = [
        "actor_id",
        "buc_id",
        "usecase_id",
        "source",
        "entity_id",
        "column_name",
        "operation",
    ];
    let rows: Vec<[String; 7]> = derive_actor_input_inferences(model)
        .into_iter()
        .map(|entry| {
            let source = match entry.source {
                ActorInputSource::UseCase => model.use_cases[entry.usecase].id.clone(),
                ActorInputSource::Api(api) => model.apis[api].id.clone(),
            };
            [
                model.actors[entry.actor].id.clone(),
                entry
                    .buc
                    .map(|buc| model.bucs[buc].id.clone())
                    .unwrap_or_default(),
                model.use_cases[entry.usecase].id.clone(),
                source,
                model.entities[entry.entity].id.clone(),
                entry.column,
                entry.operation.as_str().to_string(),
            ]
        })
        .collect();
    format_rows(&headers, &rows, format, "actor input candidates")
}

fn format_permission_callables(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let headers = [
        "permission_id",
        "permission_label",
        "usecase_ids",
        "api_ids",
        "usecase_api_paths",
    ];

    let rows: Vec<[String; 5]> = derive_permission_callables(model)
        .into_iter()
        .map(|entry| {
            let permission = &model.permissions[entry.permission];
            let usecase_ids = entry
                .usecases
                .iter()
                .map(|key| model.use_cases[*key].id.as_str())
                .collect::<Vec<_>>()
                .join("|");
            let api_ids = entry
                .apis
                .iter()
                .map(|key| model.apis[*key].id.as_str())
                .collect::<Vec<_>>()
                .join("|");
            let usecase_api_paths = permission_api_paths(model, &entry.api_paths);
            [
                permission.id.clone(),
                permission.label.clone(),
                usecase_ids,
                api_ids,
                usecase_api_paths,
            ]
        })
        .collect();

    match format {
        ListFormat::Table => {
            if rows.is_empty() {
                return Ok(String::from("No permissions found.\n"));
            }
            let mut col_widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
            for row in &rows {
                for (i, cell) in row.iter().enumerate() {
                    col_widths[i] = col_widths[i].max(cell.chars().count());
                }
            }
            let mut out = String::new();
            let header_line: Vec<String> = headers
                .iter()
                .enumerate()
                .map(|(i, h)| format!("{:<width$}", h.to_uppercase(), width = col_widths[i]))
                .collect();
            out.push_str(&header_line.join("  "));
            out.push('\n');
            let sep_line: Vec<String> = col_widths.iter().map(|&w| "\u{2500}".repeat(w)).collect();
            out.push_str(&sep_line.join("  "));
            out.push('\n');
            for row in &rows {
                let row_line: Vec<String> = row
                    .iter()
                    .enumerate()
                    .map(|(i, cell)| format!("{:<width$}", cell, width = col_widths[i]))
                    .collect();
                out.push_str(&row_line.join("  "));
                out.push('\n');
            }
            Ok(out)
        }
        ListFormat::Csv => {
            let mut out = format!("{}\n", headers.join(","));
            for row in &rows {
                let cells: Vec<String> = row.iter().map(|c| csv_field(c)).collect();
                out.push_str(&format!("{}\n", cells.join(",")));
            }
            Ok(out)
        }
        ListFormat::Json => {
            let entries: Vec<String> = rows
                .iter()
                .map(|row| {
                    format!(
                        "{{\"permission_id\":{},\"permission_label\":{},\"usecase_ids\":{},\"api_ids\":{},\"usecase_api_paths\":{}}}",
                        serde_json::to_string(&row[0]).unwrap(),
                        serde_json::to_string(&row[1]).unwrap(),
                        serde_json::to_string(&row[2]).unwrap(),
                        serde_json::to_string(&row[3]).unwrap(),
                        serde_json::to_string(&row[4]).unwrap(),
                    )
                })
                .collect();
            Ok(format!("[{}]\n", entries.join(",")))
        }
    }
}

fn format_actor_permission_audit(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let headers = [
        "actor_id",
        "actor_label",
        "permission_id",
        "permission_label",
        "assigned",
        "required",
        "status",
        "required_usecase_ids",
        "required_api_paths",
    ];

    let rows: Vec<[String; 9]> = derive_actor_permission_audit(model)
        .into_iter()
        .map(|entry| {
            let actor = &model.actors[entry.actor];
            let permission = &model.permissions[entry.permission];
            [
                actor.id.clone(),
                actor.label.clone(),
                permission.id.clone(),
                permission.label.clone(),
                bool_cell(entry.assigned),
                bool_cell(entry.required),
                entry.status.as_str().to_string(),
                required_usecase_ids(model, &entry.sources),
                required_api_paths(model, &entry.sources),
            ]
        })
        .collect();

    match format {
        ListFormat::Table => {
            if rows.is_empty() {
                return Ok(String::from("No actor permission audit rows found.\n"));
            }
            let mut col_widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
            for row in &rows {
                for (i, cell) in row.iter().enumerate() {
                    col_widths[i] = col_widths[i].max(cell.chars().count());
                }
            }
            let mut out = String::new();
            let header_line: Vec<String> = headers
                .iter()
                .enumerate()
                .map(|(i, h)| format!("{:<width$}", h.to_uppercase(), width = col_widths[i]))
                .collect();
            out.push_str(&header_line.join("  "));
            out.push('\n');
            let sep_line: Vec<String> = col_widths.iter().map(|&w| "\u{2500}".repeat(w)).collect();
            out.push_str(&sep_line.join("  "));
            out.push('\n');
            for row in &rows {
                let row_line: Vec<String> = row
                    .iter()
                    .enumerate()
                    .map(|(i, cell)| format!("{:<width$}", cell, width = col_widths[i]))
                    .collect();
                out.push_str(&row_line.join("  "));
                out.push('\n');
            }
            Ok(out)
        }
        ListFormat::Csv => {
            let mut out = format!("{}\n", headers.join(","));
            for row in &rows {
                let cells: Vec<String> = row.iter().map(|c| csv_field(c)).collect();
                out.push_str(&format!("{}\n", cells.join(",")));
            }
            Ok(out)
        }
        ListFormat::Json => {
            let entries: Vec<String> = rows
                .iter()
                .map(|row| {
                    format!(
                        "{{\"actor_id\":{},\"actor_label\":{},\"permission_id\":{},\"permission_label\":{},\"assigned\":{},\"required\":{},\"status\":{},\"required_usecase_ids\":{},\"required_api_paths\":{}}}",
                        serde_json::to_string(&row[0]).unwrap(),
                        serde_json::to_string(&row[1]).unwrap(),
                        serde_json::to_string(&row[2]).unwrap(),
                        serde_json::to_string(&row[3]).unwrap(),
                        row[4],
                        row[5],
                        serde_json::to_string(&row[6]).unwrap(),
                        serde_json::to_string(&row[7]).unwrap(),
                        serde_json::to_string(&row[8]).unwrap(),
                    )
                })
                .collect();
            Ok(format!("[{}]\n", entries.join(",")))
        }
    }
}

fn required_usecase_ids(
    model: &rdra_ish_core::SemanticModel,
    sources: &[rdra_ish_core::ActorPermissionRequirementSource],
) -> String {
    let mut ids: Vec<&str> = sources
        .iter()
        .filter(|source| source.api.is_none())
        .map(|source| model.use_cases[source.usecase].id.as_str())
        .collect();
    ids.sort();
    ids.dedup();
    ids.join("|")
}

fn permission_api_paths(
    model: &rdra_ish_core::SemanticModel,
    paths: &[rdra_ish_core::PermissionApiPath],
) -> String {
    let mut paths: Vec<String> = paths
        .iter()
        .map(|path| {
            format!(
                "{}->{}",
                model.use_cases[path.usecase].id, model.apis[path.api].id
            )
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths.join("|")
}

fn required_api_paths(
    model: &rdra_ish_core::SemanticModel,
    sources: &[rdra_ish_core::ActorPermissionRequirementSource],
) -> String {
    let mut paths: Vec<String> = sources
        .iter()
        .filter_map(|source| {
            source.api.map(|api| {
                format!(
                    "{}->{}",
                    model.use_cases[source.usecase].id, model.apis[api].id
                )
            })
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths.join("|")
}

fn bool_cell(value: bool) -> String {
    (if value { "true" } else { "false" }).to_string()
}

fn format_rows<const N: usize>(
    headers: &[&str; N],
    rows: &[[String; N]],
    format: &ListFormat,
    empty_label: &str,
) -> Result<String> {
    match format {
        ListFormat::Table => {
            if rows.is_empty() {
                return Ok(format!("No {} found.\n", empty_label));
            }
            let mut col_widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
            for row in rows {
                for (i, cell) in row.iter().enumerate() {
                    col_widths[i] = col_widths[i].max(cell.chars().count());
                }
            }
            let mut out = String::new();
            let header_line: Vec<String> = headers
                .iter()
                .enumerate()
                .map(|(i, h)| format!("{:<width$}", h.to_uppercase(), width = col_widths[i]))
                .collect();
            out.push_str(&header_line.join("  "));
            out.push('\n');
            let sep_line: Vec<String> = col_widths.iter().map(|&w| "\u{2500}".repeat(w)).collect();
            out.push_str(&sep_line.join("  "));
            out.push('\n');
            for row in rows {
                let row_line: Vec<String> = row
                    .iter()
                    .enumerate()
                    .map(|(i, cell)| format!("{:<width$}", cell, width = col_widths[i]))
                    .collect();
                out.push_str(&row_line.join("  "));
                out.push('\n');
            }
            Ok(out)
        }
        ListFormat::Csv => {
            let mut out = format!("{}\n", headers.join(","));
            for row in rows {
                let cells: Vec<String> = row.iter().map(|c| csv_field(c)).collect();
                out.push_str(&format!("{}\n", cells.join(",")));
            }
            Ok(out)
        }
        ListFormat::Json => {
            let entries: Vec<String> = rows
                .iter()
                .map(|row| {
                    let fields: Vec<String> = headers
                        .iter()
                        .enumerate()
                        .map(|(i, header)| {
                            format!(
                                "{}:{}",
                                serde_json::to_string(header).unwrap(),
                                serde_json::to_string(&row[i]).unwrap()
                            )
                        })
                        .collect();
                    format!("{{{}}}", fields.join(","))
                })
                .collect();
            Ok(format!("[{}]\n", entries.join(",")))
        }
    }
}

fn format_id_label(
    items: &[(&str, &str)],
    format: &ListFormat,
    empty_label: &str,
) -> Result<String> {
    match format {
        ListFormat::Table => {
            if items.is_empty() {
                return Ok(format!("No {} found.\n", empty_label));
            }
            let id_w = items
                .iter()
                .map(|(id, _)| id.len())
                .max()
                .unwrap_or(2)
                .max(2);
            let label_w = items
                .iter()
                .map(|(_, l)| l.chars().count())
                .max()
                .unwrap_or(5)
                .max(5);
            let header_id = format!("{:<width$}", "ID", width = id_w);
            let header_label = format!("{:<width$}", "LABEL", width = label_w);
            let sep_id = table_separator(&[id_w]);
            let sep_label = table_separator(&[label_w]);
            let mut out = format!(
                "{}  {}\n{}  {}\n",
                header_id, header_label, sep_id, sep_label
            );
            for (id, label) in items {
                out.push_str(&format!("{:<width$}  {}\n", id, label, width = id_w));
            }
            Ok(out)
        }
        ListFormat::Csv => {
            let mut out = String::from("id,label\n");
            for (id, label) in items {
                // Simple CSV: quote if contains comma or quote
                let escaped_id = csv_field(id);
                let escaped_label = csv_field(label);
                out.push_str(&format!("{},{}\n", escaped_id, escaped_label));
            }
            Ok(out)
        }
        ListFormat::Json => {
            let entries: Vec<String> = items
                .iter()
                .map(|(id, label)| {
                    format!(
                        "{{\"id\":{},\"label\":{}}}",
                        serde_json::to_string(id).unwrap(),
                        serde_json::to_string(label).unwrap()
                    )
                })
                .collect();
            Ok(format!("[{}]\n", entries.join(",")))
        }
    }
}

fn optional_cell(value: &Option<String>) -> String {
    value.clone().unwrap_or_default()
}

fn repeated_cell(values: &[String]) -> String {
    values.join("|")
}

fn format_requirements(
    model: &rdra_ish_core::SemanticModel,
    format: &ListFormat,
) -> Result<String> {
    let mut requirements: Vec<_> = model.requirements.iter().collect();
    requirements.sort_by_key(|(_, requirement)| requirement.id.as_str());

    let headers = [
        "id",
        "label",
        "priority",
        "sources",
        "stakeholders",
        "owner",
        "acceptance_criteria",
        "status",
        "risk",
        "rationale",
        "description",
    ];
    let rows: Vec<[String; 11]> = requirements
        .into_iter()
        .map(|(_, requirement)| {
            [
                requirement.id.clone(),
                requirement.label.clone(),
                optional_cell(&requirement.priority),
                repeated_cell(&requirement.sources),
                repeated_cell(&requirement.stakeholders),
                optional_cell(&requirement.owner),
                repeated_cell(&requirement.acceptance_criteria),
                optional_cell(&requirement.status),
                optional_cell(&requirement.risk),
                optional_cell(&requirement.rationale),
                optional_cell(&requirement.description),
            ]
        })
        .collect();

    format_rows(&headers, &rows, format, "requirements")
}

fn format_adrs(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut adrs: Vec<_> = model.adrs.iter().collect();
    adrs.sort_by_key(|(_, adr)| adr.id.as_str());

    let headers = [
        "id",
        "label",
        "status",
        "context",
        "decision",
        "consequences",
        "accepted_options",
        "rejected_options",
        "reasons",
        "target_kinds",
        "target_ids",
        "description",
    ];
    let rows: Vec<[String; 12]> = adrs
        .into_iter()
        .map(|(key, adr)| {
            let targets = adr_targets(model, key);
            let target_kinds = targets
                .iter()
                .map(|target| node_kind_name(target).to_string())
                .collect::<Vec<_>>()
                .join("|");
            let target_ids = targets
                .iter()
                .filter_map(|target| node_id(model, target))
                .collect::<Vec<_>>()
                .join("|");
            [
                adr.id.clone(),
                adr.label.clone(),
                optional_cell(&adr.status),
                repeated_cell(&adr.context),
                optional_cell(&adr.decision),
                repeated_cell(&adr.consequences),
                repeated_cell(&adr.accepted_options),
                repeated_cell(&adr.rejected_options),
                repeated_cell(&adr.reasons),
                target_kinds,
                target_ids,
                optional_cell(&adr.description),
            ]
        })
        .collect();

    format_rows(&headers, &rows, format, "ADRs")
}

fn format_adr_impacts(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let headers = [
        "adr_id",
        "adr_label",
        "adr_status",
        "target_kind",
        "target_id",
        "target_label",
    ];
    let mut rows = Vec::new();
    let mut adrs: Vec<_> = model.adrs.iter().collect();
    adrs.sort_by_key(|(_, adr)| adr.id.as_str());
    for (key, adr) in adrs {
        for target in adr_targets(model, key) {
            rows.push([
                adr.id.clone(),
                adr.label.clone(),
                optional_cell(&adr.status),
                node_kind_name(&target).to_string(),
                node_id(model, &target).unwrap_or_default(),
                node_label(model, &target).unwrap_or_default(),
            ]);
        }
    }
    rows.sort_by(|left, right| {
        left[0]
            .cmp(&right[0])
            .then_with(|| left[3].cmp(&right[3]))
            .then_with(|| left[4].cmp(&right[4]))
    });

    format_rows(&headers, &rows, format, "ADR impacts")
}

fn adr_targets(
    model: &rdra_ish_core::SemanticModel,
    adr: rdra_ish_core::model::AdrKey,
) -> Vec<NodeRef> {
    let mut targets: Vec<_> = model
        .relations
        .iter()
        .filter(|relation| relation.kind == RelKind::Decides && relation.from == NodeRef::Adr(adr))
        .map(|relation| relation.to.clone())
        .collect();
    targets.sort_by_key(|target| {
        (
            node_kind_name(target).to_string(),
            node_id(model, target).unwrap_or_default(),
        )
    });
    targets
}

fn node_label(model: &rdra_ish_core::SemanticModel, node: &NodeRef) -> Option<String> {
    Some(match node {
        NodeRef::Actor(key) => model.actors.get(*key)?.label.clone(),
        NodeRef::ExtSystem(key) => model.ext_systems.get(*key)?.label.clone(),
        NodeRef::System(key) => model.systems.get(*key)?.label.clone(),
        NodeRef::Requirement(key) => model.requirements.get(*key)?.label.clone(),
        NodeRef::Adr(key) => model.adrs.get(*key)?.label.clone(),
        NodeRef::Nfr(key) => model.nfrs.get(*key)?.label.clone(),
        NodeRef::Quality(key) => model.qualities.get(*key)?.label.clone(),
        NodeRef::Constraint(key) => model.constraints.get(*key)?.label.clone(),
        NodeRef::Concept(key) => model.concepts.get(*key)?.label.clone(),
        NodeRef::DomainObject(key) => model.domain_objects.get(*key)?.label.clone(),
        NodeRef::Aggregate(key) => model.aggregates.get(*key)?.label.clone(),
        NodeRef::ValueObject(key) => model.value_objects.get(*key)?.label.clone(),
        NodeRef::Business(key) => model.businesses.get(*key)?.label.clone(),
        NodeRef::Buc(key) => model.bucs.get(*key)?.label.clone(),
        NodeRef::Flow(key) => model.flows.get(*key)?.label.clone(),
        NodeRef::Step(key) => model.steps.get(*key)?.label.clone(),
        NodeRef::UsageScene(key) => model.usage_scenes.get(*key)?.label.clone(),
        NodeRef::UseCase(key) => model.use_cases.get(*key)?.label.clone(),
        NodeRef::Screen(key) => model.screens.get(*key)?.label.clone(),
        NodeRef::Field(key) => model.fields.get(*key)?.label.clone(),
        NodeRef::Event(key) => model.events.get(*key)?.label.clone(),
        NodeRef::Entity(key) => model.entities.get(*key)?.label.clone(),
        NodeRef::State(key) => model.states.get(*key)?.label.clone(),
        NodeRef::Condition(key) => model.conditions.get(*key)?.label.clone(),
        NodeRef::Variation(key) => model.variations.get(*key)?.label.clone(),
        NodeRef::Api(key) => model.apis.get(*key)?.label.clone(),
        NodeRef::Dto(key) => model.dtos.get(*key)?.label.clone(),
        NodeRef::Location(key) => model.locations.get(*key)?.label.clone(),
        NodeRef::Timing(key) => model.timings.get(*key)?.label.clone(),
        NodeRef::Medium(key) => model.media.get(*key)?.label.clone(),
        NodeRef::Permission(key) => model.permissions.get(*key)?.label.clone(),
    })
}

fn format_usecases(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut usecases: Vec<_> = model.use_cases.iter().collect();
    usecases.sort_by_key(|(_, usecase)| usecase.id.as_str());

    let headers = [
        "id",
        "label",
        "preconditions",
        "guards",
        "postconditions",
        "alternatives",
        "errors",
        "description",
    ];
    let rows: Vec<[String; 8]> = usecases
        .into_iter()
        .map(|(_, usecase)| {
            [
                usecase.id.clone(),
                usecase.label.clone(),
                repeated_cell(&usecase.preconditions),
                repeated_cell(&usecase.guards),
                repeated_cell(&usecase.postconditions),
                repeated_cell(&usecase.alternatives),
                repeated_cell(&usecase.errors),
                optional_cell(&usecase.description),
            ]
        })
        .collect();

    format_rows(&headers, &rows, format, "use cases")
}

const NFR_HEADERS: [&str; 13] = [
    "id",
    "label",
    "metric",
    "target",
    "window",
    "slo",
    "availability",
    "resilience",
    "audit",
    "logging",
    "retention",
    "privacy",
    "description",
];

fn nfr_row(
    id: &str,
    label: &str,
    description: &Option<String>,
    metadata: [&Option<String>; 10],
) -> [String; 13] {
    [
        id.to_string(),
        label.to_string(),
        optional_cell(metadata[0]),
        optional_cell(metadata[1]),
        optional_cell(metadata[2]),
        optional_cell(metadata[3]),
        optional_cell(metadata[4]),
        optional_cell(metadata[5]),
        optional_cell(metadata[6]),
        optional_cell(metadata[7]),
        optional_cell(metadata[8]),
        optional_cell(metadata[9]),
        optional_cell(description),
    ]
}

fn format_nfrs(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut nfrs: Vec<_> = model.nfrs.iter().collect();
    nfrs.sort_by_key(|(_, nfr)| nfr.id.as_str());

    let rows: Vec<[String; 13]> = nfrs
        .into_iter()
        .map(|(_, nfr)| {
            nfr_row(
                &nfr.id,
                &nfr.label,
                &nfr.description,
                [
                    &nfr.metric,
                    &nfr.target,
                    &nfr.window,
                    &nfr.slo,
                    &nfr.availability,
                    &nfr.resilience,
                    &nfr.audit,
                    &nfr.logging,
                    &nfr.retention,
                    &nfr.privacy,
                ],
            )
        })
        .collect();

    format_rows(&NFR_HEADERS, &rows, format, "NFRs")
}

fn format_constraints(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut constraints: Vec<_> = model.constraints.iter().collect();
    constraints.sort_by_key(|(_, constraint)| constraint.id.as_str());

    let rows: Vec<[String; 13]> = constraints
        .into_iter()
        .map(|(_, constraint)| {
            nfr_row(
                &constraint.id,
                &constraint.label,
                &constraint.description,
                [
                    &constraint.metric,
                    &constraint.target,
                    &constraint.window,
                    &constraint.slo,
                    &constraint.availability,
                    &constraint.resilience,
                    &constraint.audit,
                    &constraint.logging,
                    &constraint.retention,
                    &constraint.privacy,
                ],
            )
        })
        .collect();

    format_rows(&NFR_HEADERS, &rows, format, "constraints")
}

fn col_type_s(ct: &rdra_ish_core::model::ColumnType) -> &'static str {
    use rdra_ish_core::model::ColumnType;
    match ct {
        ColumnType::Int => "Int",
        ColumnType::String => "String",
        ColumnType::Money => "Money",
        ColumnType::DateTime => "DateTime",
        ColumnType::Date => "Date",
        ColumnType::Bool => "Bool",
        ColumnType::Decimal => "Decimal",
        ColumnType::Enum(_) => "Enum",
    }
}

fn format_apis(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut apis: Vec<_> = model.apis.iter().collect();
    apis.sort_by_key(|(_, api)| api.id.as_str());

    let headers = [
        "api_id",
        "api_label",
        "method",
        "path",
        "idempotency",
        "mode",
        "auth_scheme",
    ];
    let rows: Vec<[String; 7]> = apis
        .into_iter()
        .map(|(_, api)| {
            [
                api.id.clone(),
                api.label.clone(),
                optional_cell(&api.method),
                optional_cell(&api.path),
                optional_cell(&api.idempotency),
                optional_cell(&api.mode),
                optional_cell(&api.auth_scheme),
            ]
        })
        .collect();

    format_rows(&headers, &rows, format, "APIs")
}

fn format_dtos(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut dtos: Vec<_> = model.dtos.iter().collect();
    dtos.sort_by_key(|(_, dto)| dto.id.as_str());

    let headers = [
        "dto_id",
        "dto_label",
        "field_name",
        "field_type",
        "required",
    ];
    let mut rows: Vec<[String; 5]> = Vec::new();
    for (_, dto) in dtos {
        if dto.fields.is_empty() {
            rows.push([
                dto.id.clone(),
                dto.label.clone(),
                String::new(),
                String::new(),
                String::new(),
            ]);
            continue;
        }
        for field in &dto.fields {
            rows.push([
                dto.id.clone(),
                dto.label.clone(),
                field.name.clone(),
                col_type_s(&field.col_type).to_string(),
                bool_cell(!field.is_nullable),
            ]);
        }
    }

    format_rows(&headers, &rows, format, "DTOs")
}

fn format_fields(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut fields: Vec<_> = model.fields.iter().collect();
    fields.sort_by_key(|(_, field)| field.id.as_str());

    let headers = [
        "field_id",
        "field_label",
        "access",
        "required",
        "source",
        "entity_id",
        "column_name",
    ];
    let mut rows: Vec<[String; 7]> = Vec::new();
    for (field_key, field) in fields {
        let mappings: Vec<_> = model
            .field_mappings
            .iter()
            .filter(|mapping| mapping.field == field_key)
            .collect();
        if mappings.is_empty() {
            rows.push([
                field.id.clone(),
                field.label.clone(),
                optional_cell(&field.access),
                field.required.map(bool_cell).unwrap_or_default(),
                optional_cell(&field.source),
                String::new(),
                String::new(),
            ]);
            continue;
        }
        for mapping in mappings {
            rows.push([
                field.id.clone(),
                field.label.clone(),
                optional_cell(&field.access),
                field.required.map(bool_cell).unwrap_or_default(),
                optional_cell(&field.source),
                model.entities[mapping.entity].id.clone(),
                mapping.column.clone(),
            ]);
        }
    }

    format_rows(&headers, &rows, format, "fields")
}

fn format_entities(model: &rdra_ish_core::SemanticModel, format: &ListFormat) -> Result<String> {
    let mut ents: Vec<_> = model.entities.iter().collect();
    ents.sort_by_key(|(_, e)| e.id.as_str());

    let headers = [
        "entity_id",
        "entity_label",
        "column_name",
        "column_type",
        "is_pk",
        "is_unique",
        "is_indexed",
        "is_fk",
        "fk_target",
        "fk_optional",
        "fk_on_delete",
        "fk_on_update",
        "is_nullable",
        "default_val",
        "check_constraints",
        "is_soft_delete",
        "is_history",
        "is_tenant_scope",
        "derived_expr",
    ];

    let mut rows: Vec<[String; 19]> = Vec::new();
    for (_, ent) in &ents {
        for col in &ent.columns {
            rows.push([
                ent.id.clone(),
                ent.label.clone(),
                col.name.clone(),
                col_type_s(&col.col_type).to_string(),
                if col.is_pk { "true" } else { "false" }.to_string(),
                if col.is_unique { "true" } else { "false" }.to_string(),
                if col.is_indexed { "true" } else { "false" }.to_string(),
                if col.is_fk { "true" } else { "false" }.to_string(),
                col.fk_target.clone().unwrap_or_default(),
                bool_cell(col.fk_optional),
                col.fk_on_delete.clone().unwrap_or_default(),
                col.fk_on_update.clone().unwrap_or_default(),
                bool_cell(col.is_nullable),
                col.default_val.clone().unwrap_or_default(),
                col.check_constraints.join("|"),
                bool_cell(col.is_soft_delete),
                bool_cell(col.is_history),
                bool_cell(col.is_tenant_scope),
                col.derived_expr.clone().unwrap_or_default(),
            ]);
        }
    }

    format_rows(&headers, &rows, format, "entities")
}

/// `--entity` フィルタ: 指定 entity_id の出力行のみを残す。
/// table 形式はブロック単位で、csv/json はフィールドでフィルタする。
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

/// Minimal CSV field escaping.
fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
