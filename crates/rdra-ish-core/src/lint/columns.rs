//! Column and enum attribute lint rules.

use crate::model::{ColumnType, SemanticModel};

use super::naming::is_snake_caseish;
use super::{push_element_lint, LintIssue};

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
