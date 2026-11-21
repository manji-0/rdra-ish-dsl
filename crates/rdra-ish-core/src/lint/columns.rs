//! Column and enum attribute lint rules.

use crate::model::{ColumnType, Reader, SemanticModel};

use super::naming::is_snake_caseish;
use super::{push_element_lint, LintIssue, LintSeverity};

pub(super) fn lint_column_names(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    for (_, entity) in &model.entities {
        for column in &entity.columns {
            if !is_snake_caseish(&column.name) {
                push_element_lint(
                    model,
                    issues,
                    "entity",
                    "entity-column",
                    &entity.id,
                    "naming-column",
                    format!(
                        "entity column '{}' does not follow snake_case project convention",
                        column.name
                    ),
                    "rename the column to snake_case or document the external naming constraint",
                );
            }
        }
    }

    for (_, dto) in &model.dtos {
        for field in &dto.fields {
            if !is_snake_caseish(&field.name) {
                push_element_lint(
                    model,
                    issues,
                    "dto",
                    "dto-field",
                    &dto.id,
                    "naming-dto-field",
                    format!(
                        "DTO field '{}' does not follow snake_case project convention",
                        field.name
                    ),
                    "rename the DTO field to snake_case or document the external contract constraint",
                );
            }
        }
    }
}

/// An inline `Enum` that no `transitions` and no `sets` moves is an attribute, not a lifecycle.
pub(super) fn lint_enum_attributes(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    for (ek, entity) in &model.entities {
        for column in &entity.columns {
            if !matches!(column.col_type, ColumnType::Enum(_)) {
                continue;
            }
            let moved = model.is_lifecycle_column(ek, &column.name)
                || model
                    .column_effects
                    .iter()
                    .any(|e| e.entity == ek && e.column == column.name);
            if moved {
                continue;
            }
            push_element_lint(
                model,
                issues,
                "entity",
                "entity-column",
                &entity.id,
                "enum-attribute",
                format!(
                    "{}.{} is an inline Enum that no transitions or sets moves",
                    entity.id, column.name
                ),
                "type it by a variation if an actor chooses the value",
            );
        }
    }
}

/// A nullable column a screen field shows starts `null`, and `states` keeps it there unless a
/// `sets` or a `chooses` gives it a value, so the field shows nothing the model can produce.
pub(super) fn lint_unwritten_field_columns(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    for mapping in &model.field_mappings {
        let Some(entity) = model.entities.get(mapping.entity) else {
            continue;
        };
        let Some(field) = model.fields.get(mapping.field) else {
            continue;
        };
        let nullable = entity
            .columns
            .iter()
            .any(|c| c.name == mapping.column && c.is_nullable);
        let written = model
            .column_effects
            .iter()
            .any(|e| e.entity == mapping.entity && e.column == mapping.column)
            || model
                .column_choices
                .iter()
                .any(|c| c.entity == mapping.entity && c.column == mapping.column);
        if !nullable || written {
            continue;
        }
        push_element_lint(
            model,
            issues,
            "field",
            "field",
            &field.id,
            "field-column-unwritten",
            format!(
                "{} shows {}.{}, a nullable column that no use case sets, so it stays null",
                field.id, entity.id, mapping.column
            ),
            "add sets(UseCase, Entity.column == present) where the value is given, or drop the column's @null",
        );
    }
}

/// An `@end` value no `transitions` arm leaves and no `chooses` can move off is an end state
/// already, so the declaration changes nothing.
pub(super) fn lint_redundant_end_values(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    for (ek, entity) in &model.entities {
        for column in &entity.columns {
            let chosen = model
                .column_choices
                .iter()
                .any(|c| c.entity == ek && c.column == column.name);
            if chosen {
                continue;
            }
            for value in &column.end_values {
                let leaves = model.state_transitions.iter().any(|t| {
                    t.entity == ek && t.column == column.name && t.from == *value && t.to != *value
                });
                if leaves {
                    continue;
                }
                push_element_lint(
                    model,
                    issues,
                    "entity",
                    "entity-column",
                    &entity.id,
                    "end-redundant",
                    format!(
                        "@end({value}) on {}.{}: no transition leaves {value}, so it is an end state without @end",
                        entity.id, column.name
                    ),
                    "delete it from @end: @end is for a state a transition can leave but a row may also stay at for good",
                );
            }
        }
    }
}

/// A lookup that no key or `@index` serves: a declared read, a timer's use case that finds
/// rows by their state, or a `.has` / `.none` over a large entity. Names the index it wants,
/// as the `@index(...)` that declares it; one finding per index, at the first reader. An
/// entity whose `rows` is below `SMALL_TABLE_ROWS` draws none: a scan is cheap.
pub(super) fn lint_query_indexes(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    for suggestion in model.index_suggestions() {
        if suggestion.is_small() {
            continue;
        }
        let entity = &model.entities[suggestion.entity];
        let columns: Vec<String> = suggestion
            .columns
            .iter()
            .map(|k| k.column.clone())
            .collect();
        let readers: Vec<String> = suggestion
            .readers
            .iter()
            .map(|r| match r {
                Reader::Query(_) => format!("the read by {}", model.reader_name(r)),
                Reader::Batch { column, from, .. } => format!(
                    "{} (a timer starts it) finding the rows at {column} {}",
                    model.reader_name(r),
                    match from.as_slice() {
                        [one] => format!("== {one}"),
                        many => format!("in ({})", many.join(", ")),
                    }
                ),
                Reader::Quantifier { on } => format!("the .has/.none of {on}"),
                Reader::Guard { usecases, .. } => format!(
                    "the guard {} on {}, which no row they write joins",
                    model.reader_name(r),
                    usecases
                        .iter()
                        .map(|uc| model.use_cases[*uc].id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            })
            .collect();
        let shown: Vec<String> = suggestion
            .columns
            .iter()
            .map(|k| {
                if k.desc {
                    format!("{} DESC", k.column)
                } else {
                    k.column.clone()
                }
            })
            .collect();
        let mixed = suggestion.mixes_directions();
        let hint = if mixed {
            "take the suggested index from `export --kind sql`: `@index` has no sort direction, and this sort mixes them".to_string()
        } else {
            format!("declare `@index({})` on {}", columns.join(", "), entity.id)
        };
        let hint = if suggestion
            .readers
            .iter()
            .any(|r| matches!(r, Reader::Guard { .. }))
        {
            format!("{hint}; or, when the guard means a row related to the ones the use case writes, join it with `.along(...)`")
        } else {
            hint
        };
        let mut issue = LintIssue::new(
            LintSeverity::Warning,
            "query-index-missing",
            "entity",
            &entity.id,
            format!(
                "no index of {} serves {}: it wants ({}), so its rows are found by a full scan",
                entity.id,
                readers.join(", "),
                shown.join(", ")
            ),
            hint,
        );
        if let Some(site) = model.reader_site(&suggestion.readers[0]) {
            issue = issue.with_location(site);
        }
        issues.push(issue);
    }
}
