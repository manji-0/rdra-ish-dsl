use crate::{EmitError, Emitter, View};
use rdra_ish_core::model::{ColumnType, Entity, EntityKey, ModelColumn, RefAction, SemanticModel};

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
    for (index, (key, entity)) in entities.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&dbml_table(model, *key, entity));
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
    let mut variations: Vec<_> = model.variations.values().collect();
    variations.sort_by_key(|variation| variation.id.as_str());
    for variation in variations {
        let mut lines = vec![format!("Enum {} {{", dbml_ident(&variation.id))];
        for (value, label) in variation.values.iter().zip(&variation.labels) {
            lines.push(match label {
                Some(label) => format!(
                    "  {} [note: '{}']",
                    dbml_ident(value),
                    label.replace('\'', "\\'")
                ),
                None => format!("  {}", dbml_ident(value)),
            });
        }
        lines.push("}".to_string());
        defs.push(lines.join("\n"));
    }
    defs
}

fn dbml_table(model: &SemanticModel, key: EntityKey, entity: &Entity) -> String {
    let mut lines = vec![format!(
        "Table {} [{}] {{",
        dbml_ident(&entity.id),
        dbml_settings(&table_settings(entity))
    )];
    for column in &entity.columns {
        lines.push(format!("  {}", dbml_column(model, entity, column)));
    }
    let indexes = dbml_indexes(model, key, entity);
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

fn dbml_column(model: &SemanticModel, entity: &Entity, column: &ModelColumn) -> String {
    let settings = column_settings(entity, column);
    let settings = if settings.is_empty() {
        String::new()
    } else {
        format!(" [{}]", dbml_settings(&settings))
    };
    format!(
        "{} {}{}",
        dbml_ident(&column.name),
        dbml_type(model, entity, column),
        settings
    )
}

fn column_settings(entity: &Entity, column: &ModelColumn) -> Vec<String> {
    let mut settings = Vec::new();
    // Single-column PK stays on the column; composite PKs use Indexes { (a, b) [pk] }.
    if column.is_pk && entity.primary_key.len() <= 1 {
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

fn dbml_type(model: &SemanticModel, entity: &Entity, column: &ModelColumn) -> String {
    match &column.col_type {
        ColumnType::Int => "int".to_string(),
        ColumnType::String => "varchar".to_string(),
        ColumnType::Money | ColumnType::Decimal => "decimal".to_string(),
        ColumnType::DateTime => "datetime".to_string(),
        ColumnType::Date => "date".to_string(),
        ColumnType::Bool => "boolean".to_string(),
        ColumnType::Enum(_) => dbml_ident(&enum_type_name(entity, column)),
        ColumnType::Variation(key) => model
            .variations
            .get(*key)
            .map(|variation| dbml_ident(&variation.id))
            .unwrap_or_else(|| "varchar".to_string()),
    }
}

fn dbml_indexes(model: &SemanticModel, key: EntityKey, entity: &Entity) -> Vec<String> {
    let mut indexes = Vec::new();
    if entity.primary_key.len() > 1 {
        indexes.push(format_index(&entity.primary_key, &["pk"]));
    }
    for columns in &entity.indexes {
        indexes.push(format_index(columns, &[]));
    }
    for unique in model.unique_keys(key) {
        let columns = &unique.columns;
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
    for (key, entity) in entities {
        for column in &entity.columns {
            let Some((_, fk)) = model.column_fk(key, &column.name) else {
                continue;
            };
            // One ref per key, printed at its first column.
            if fk
                .columns
                .first()
                .is_none_or(|(first, _)| *first != column.name)
            {
                continue;
            }
            let target = &model.entities[fk.parent];
            let mut settings = Vec::new();
            if let Some(action) = fk.on_delete {
                settings.push(format!("delete: {}", dbml_ref_action(action)));
            }
            if let Some(action) = fk.on_update {
                settings.push(format!("update: {}", dbml_ref_action(action)));
            }
            let settings = if settings.is_empty() {
                String::new()
            } else {
                format!(" [{}]", settings.join(", "))
            };
            let relation = if fk.unique() { "-" } else { ">" };
            refs.push(format!(
                "Ref: {}.{} {relation} {}.{}{}",
                dbml_ident(&entity.id),
                ref_columns(fk.columns.iter().map(|(child, _)| child)),
                dbml_ident(&target.id),
                ref_columns(fk.columns.iter().map(|(_, parent)| parent)),
                settings
            ));
        }
    }
    refs
}

/// `a` for one key column, `(a, b)` for a composite key.
fn ref_columns<'a>(columns: impl Iterator<Item = &'a String>) -> String {
    let columns: Vec<String> = columns.map(|c| dbml_ident(c)).collect();
    match columns.as_slice() {
        [one] => one.clone(),
        _ => format!("({})", columns.join(", ")),
    }
}

fn enum_type_name(entity: &Entity, column: &ModelColumn) -> String {
    format!("{}_{}", entity.id, column.name)
}

fn dbml_settings(settings: &[String]) -> String {
    settings.join(", ")
}

fn dbml_ref_action(action: RefAction) -> String {
    action.as_str().replace('_', " ")
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
relate(Order, Customer, N:1).optional().on_delete(set_null).on_update(cascade)
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

    #[test]
    fn a_composite_key_is_one_ref_over_its_columns() {
        let model = model_from(
            r#"
entity Bin "Bin" {
  site: String @pk(site, shelf)
  shelf: Int
}
entity Stock "Stock" {
  id: Int @pk
  alt_site: String
  alt_shelf: Int
}
entity Label "Label" { id: Int @pk }
relate(Stock, Bin, N:1)
relate(Stock, Bin, N:1).role(alt).key(alt_site, alt_shelf).optional()
relate(Label, Bin, 1:1)
"#,
        );
        let dbml = DbmlEmitter.emit(&model, &View::whole()).unwrap();
        let refs: Vec<&str> = dbml.lines().filter(|l| l.starts_with("Ref:")).collect();
        assert_eq!(
            refs,
            [
                "Ref: Label.(bin_site, bin_shelf) - Bin.(site, shelf)",
                "Ref: Stock.(alt_site, alt_shelf) > Bin.(site, shelf)",
                "Ref: Stock.(bin_site, bin_shelf) > Bin.(site, shelf)",
            ]
        );
        assert!(
            dbml.contains("    (bin_shelf, bin_site) [unique]\n"),
            "{dbml}"
        );
        assert!(dbml.contains("  alt_site varchar\n"), "{dbml}");
    }

    #[test]
    fn nm_via_emits_junction_composite_unique() {
        let model = model_from(
            r#"
entity Order "Order" { id: Int @pk }
entity Product "Product" { id: Int @pk }
entity OrderProduct "Order product" { id: Int @pk }
relate(Order, Product, N:M).via(OrderProduct)
"#,
        );
        let dbml = DbmlEmitter.emit(&model, &View::whole()).unwrap();
        assert!(
            dbml.contains("(order_id, product_id) [unique]")
                || dbml.contains("(product_id, order_id) [unique]"),
            "expected composite unique on junction FKs in DBML:\n{dbml}"
        );
    }

    #[test]
    fn variation_is_one_enum_shared_by_every_column_typed_by_it() {
        let model = model_from(
            r#"
variation PaymentMethod "Payment Method" {
  cash "Cash"
  card
}
entity Payment "Payment" {
  id: Int @pk
  method: PaymentMethod @default(card)
}
entity Refund "Refund" {
  id: Int @pk
  method: PaymentMethod
}
"#,
        );
        let dbml = DbmlEmitter.emit(&model, &View::whole()).unwrap();
        assert_eq!(dbml.matches("Enum PaymentMethod {").count(), 1, "{dbml}");
        assert!(dbml.contains("cash [note: 'Cash']"), "{dbml}");
        assert!(
            dbml.contains("method PaymentMethod [not null, default: `card`]"),
            "{dbml}"
        );
        assert!(dbml.contains("method PaymentMethod [not null]"), "{dbml}");
        assert!(!dbml.contains("Payment_method"), "{dbml}");
    }
}
