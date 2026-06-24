//! Access-control and business-input list formatters.

use anyhow::Result;
use rdra_ish_core::{
    derive_actor_input_inferences, derive_actor_permission_audit, derive_permission_callables,
    ActorInputSource,
};

use crate::cli::ListFormat;

use super::render::{bool_cell, csv_field, format_rows};

pub(crate) fn format_business_inputs(
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

pub(crate) fn format_permission_callables(
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

pub(crate) fn format_actor_permission_audit(
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
