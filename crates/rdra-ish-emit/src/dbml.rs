use crate::{EmitError, Emitter, View};
use rdra_ish_core::model::{ColumnType, Entity, ModelColumn, SemanticModel};

pub struct DbmlEmitter;

impl Emitter for DbmlEmitter {
    fn emit(&self, model: &SemanticModel, _view: &View) -> Result<String, EmitError> {
        Ok(dbml_document(model))
    }
}

fn dbml_document(model: &SemanticModel) -> String {
    let mut out = String::new();

    let enums = dbml_enums(model);
    if !enums.is_empty() {
        out.push_str(&enums.join("\n\n"));
        out.push_str("\n\n");
    }

    let mut entities: Vec<_> = model.entities.iter().collect();
    entities.sort_by_key(|(_, entity)| entity.id.as_str());
    for (index, (_, entity)) in entities.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&dbml_table(entity));
        out.push('\n');
    }

    let refs = dbml_refs(model);
    if !refs.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&refs.join("\n"));
        out.push('\n');
    }

    out
}

fn dbml_enums(model: &SemanticModel) -> Vec<String> {
    let mut defs = Vec::new();
    let mut entities: Vec<_> = model.entities.iter().collect();
    entities.sort_by_key(|(_, entity)| entity.id.as_str());
    for (_, entity) in entities {
        for column in &entity.columns {
            let ColumnType::Enum(values) = &column.col_type else {
                continue;
            };
            let mut lines = vec![format!(
                "Enum {} {{",
                dbml_ident(&enum_type_name(entity, column))
            )];
            for value in values {
                lines.push(format!("  {}", dbml_ident(value)));
            }
            lines.push("}".to_string());
            defs.push(lines.join("\n"));
        }
    }
    defs
}

fn dbml_table(entity: &Entity) -> String {
    let mut lines = vec![format!(
        "Table {} [{}] {{",
        dbml_ident(&entity.id),
        dbml_settings(&table_settings(entity))
    )];
    for column in &entity.columns {
        lines.push(format!("  {}", dbml_column(entity, column)));
    }
    let indexes = dbml_indexes(entity);
    if !indexes.is_empty() {
        lines.push(String::new());
        lines.push("  indexes {".to_string());
        for index in indexes {
            lines.push(format!("    {}", index));
        }
        lines.push("  }".to_string());
    }
    lines.push("}".to_string());
    lines.join("\n")
}

fn table_settings(entity: &Entity) -> Vec<String> {
    let mut settings = Vec::new();
    let mut note = entity.label.clone();
    if let Some(description) = &entity.description {
        if !description.trim().is_empty() {
            note.push_str(": ");
            note.push_str(description.trim());
        }
    }
    if !note.trim().is_empty() {
        settings.push(format!("note: {}", dbml_string(&note)));
    }
    settings
}

fn dbml_column(entity: &Entity, column: &ModelColumn) -> String {
    let settings = column_settings(column);
    let settings = if settings.is_empty() {
        String::new()
    } else {
        format!(" [{}]", dbml_settings(&settings))
    };
    format!(
        "{} {}{}",
        dbml_ident(&column.name),
        dbml_type(entity, column),
        settings
    )
}

fn column_settings(column: &ModelColumn) -> Vec<String> {
    let mut settings = Vec::new();
    if column.is_pk {
        settings.push("pk".to_string());
    }
    if column.is_unique {
        settings.push("unique".to_string());
    }
    if !column.is_nullable {
        settings.push("not null".to_string());
    }
    if let Some(default) = &column.default_val {
        settings.push(format!("default: `{}`", default.replace('`', "'")));
    }
    let note = column_note(column);
    if !note.is_empty() {
        settings.push(format!("note: {}", dbml_string(&note)));
    } else if let Some(label) = &column.label {
        settings.push(format!("note: {}", dbml_string(label)));
    }
    settings
}

fn column_note(column: &ModelColumn) -> String {
    let mut notes = Vec::new();
    if let Some(label) = &column.label {
        notes.push(label.clone());
    }
    for check in &column.check_constraints {
        notes.push(format!("check: {check}"));
    }
    if column.is_soft_delete {
        notes.push("soft delete marker".to_string());
    }
    if column.is_history {
        notes.push("history/versioning marker".to_string());
    }
    if column.is_tenant_scope {
        notes.push("tenant scope discriminator".to_string());
    }
    if let Some(expr) = &column.derived_expr {
        notes.push(format!("derived: {expr}"));
    }
    notes.join("; ")
}

fn dbml_type(entity: &Entity, column: &ModelColumn) -> String {
    match &column.col_type {
        ColumnType::Int => "int".to_string(),
        ColumnType::String => "varchar".to_string(),
        ColumnType::Money | ColumnType::Decimal => "decimal".to_string(),
        ColumnType::DateTime => "datetime".to_string(),
        ColumnType::Date => "date".to_string(),
        ColumnType::Bool => "boolean".to_string(),
        ColumnType::Enum(_) => dbml_ident(&enum_type_name(entity, column)),
    }
}

fn dbml_indexes(entity: &Entity) -> Vec<String> {
    let mut indexes = Vec::new();
    for columns in &entity.indexes {
        indexes.push(format_index(columns, &[]));
    }
    for columns in &entity.unique_constraints {
        if columns.len() == 1
            && entity
                .columns
                .iter()
                .any(|column| column.name == columns[0] && column.is_unique)
        {
            continue;
        }
        indexes.push(format_index(columns, &["unique"]));
    }
    indexes.sort();
    indexes.dedup();
    indexes
}

fn format_index(columns: &[String], settings: &[&str]) -> String {
    let cols = columns
        .iter()
        .map(|column| dbml_ident(column))
        .collect::<Vec<_>>()
        .join(", ");
    if settings.is_empty() {
        format!("({cols})")
    } else {
        format!("({cols}) [{}]", settings.join(", "))
    }
}

fn dbml_refs(model: &SemanticModel) -> Vec<String> {
    let mut refs = Vec::new();
    let mut entities: Vec<_> = model.entities.iter().collect();
    entities.sort_by_key(|(_, entity)| entity.id.as_str());
    for (_, entity) in entities {
        for column in &entity.columns {
            let Some(target_id) = &column.fk_target else {
                continue;
            };
            let Some(target) = model
                .entities
                .iter()
                .map(|(_, entity)| entity)
                .find(|candidate| candidate.id == *target_id)
            else {
                continue;
            };
            let Some(target_pk) = target.columns.iter().find(|column| column.is_pk) else {
                continue;
            };
            let mut settings = Vec::new();
            if let Some(action) = &column.fk_on_delete {
                settings.push(format!("delete: {}", dbml_ref_action(action)));
            }
            if let Some(action) = &column.fk_on_update {
                settings.push(format!("update: {}", dbml_ref_action(action)));
            }
            let settings = if settings.is_empty() {
                String::new()
            } else {
                format!(" [{}]", settings.join(", "))
            };
            refs.push(format!(
                "Ref: {}.{} > {}.{}{}",
                dbml_ident(&entity.id),
                dbml_ident(&column.name),
                dbml_ident(&target.id),
                dbml_ident(&target_pk.name),
                settings
            ));
        }
    }
    refs
}

fn enum_type_name(entity: &Entity, column: &ModelColumn) -> String {
    format!("{}_{}", entity.id, column.name)
}

fn dbml_settings(settings: &[String]) -> String {
    settings.join(", ")
}

fn dbml_ref_action(action: &str) -> String {
    action.replace('_', " ")
}

fn dbml_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn dbml_ident(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return "\"\"".to_string();
    };
    if (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, errors) = parse(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "diagnostics: {:?}", diags);
        model
    }

    #[test]
    fn emits_dbml_tables_enums_indexes_and_refs() {
        let model = model_from(
            r#"
entity Customer "Customer" {
  id: Int @pk
  email: String @unique @index
}
entity Order "Order" description "Logical order table" {
  id: Int @pk
  tenant_id: Int @tenant
  status: Enum(pending, paid) @default(pending)
  total: Money @check("total >= 0")
  deleted_at: DateTime @null @soft_delete
  net_total: Money @derived("total - discount")
  store_id: Int @index(status, store_id) @unique(status, store_id)
}
relate(Order, Customer, "N:1").optional().on_delete(set_null).on_update(cascade)
"#,
        );

        let dbml = DbmlEmitter.emit(&model, &View::whole()).unwrap();

        assert!(dbml.contains("Enum Order_status"));
        assert!(dbml.contains("Table Customer [note: 'Customer']"));
        assert!(dbml.contains("email varchar [unique, not null]"));
        assert!(dbml.contains("(email)"));
        assert!(dbml.contains("status Order_status [not null, default: `pending`]"));
        assert!(dbml.contains("total decimal [not null, note: 'check: total >= 0']"));
        assert!(dbml.contains("tenant scope discriminator"));
        assert!(dbml.contains("derived: total - discount"));
        assert!(dbml.contains("(status, store_id) [unique]"));
        assert!(dbml
            .contains("Ref: Order.customer_id > Customer.id [delete: set null, update: cascade]"));
    }
}
