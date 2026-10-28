use crate::{EmitError, Emitter, View};
use rdra_ish_core::model::{ColumnType, EntityKey, ForeignKey, Min, ModelColumn, SemanticModel};
use serde_json::{json, Map, Number, Value};

pub struct JsonSchemaEmitter;

impl Emitter for JsonSchemaEmitter {
    fn emit(&self, model: &SemanticModel, _view: &View) -> Result<String, EmitError> {
        let doc = json_schema_document(model);
        Ok(serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".to_string()) + "\n")
    }
}

fn json_schema_document(model: &SemanticModel) -> Value {
    let mut defs = Map::new();

    let mut dtos: Vec<_> = model.dtos.iter().collect();
    dtos.sort_by_key(|(_, dto)| dto.id.as_str());
    for (_, dto) in dtos {
        defs.insert(
            format!("Dto.{}", dto.id),
            object_schema(
                model,
                None,
                "dto",
                &dto.id,
                &dto.label,
                dto.description.as_deref(),
                &dto.fields,
                &[],
                &[],
            ),
        );
    }

    let mut entities: Vec<_> = model.entities.iter().collect();
    entities.sort_by_key(|(_, entity)| entity.id.as_str());
    for (key, entity) in entities {
        let unique_keys: Vec<Vec<String>> = model
            .unique_keys(key)
            .into_iter()
            .map(|unique| unique.columns)
            .collect();
        defs.insert(
            format!("Entity.{}", entity.id),
            object_schema(
                model,
                Some(key),
                "entity",
                &entity.id,
                &entity.label,
                entity.description.as_deref(),
                &entity.columns,
                &entity.indexes,
                &unique_keys,
            ),
        );
    }

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "RDRA-ish model schemas",
        "type": "object",
        "$defs": defs
    })
}

#[allow(clippy::too_many_arguments)]
fn object_schema(
    model: &SemanticModel,
    entity: Option<EntityKey>,
    kind: &str,
    id: &str,
    label: &str,
    description: Option<&str>,
    columns: &[ModelColumn],
    indexes: &[Vec<String>],
    unique_constraints: &[Vec<String>],
) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for column in columns {
        let fk = entity.and_then(|key| model.column_fk(key, &column.name));
        properties.insert(
            column.name.clone(),
            column_schema(model, fk.map(|(_, fk)| fk), column),
        );
        if !column.is_nullable {
            required.push(Value::String(column.name.clone()));
        }
    }

    let mut schema = Map::new();
    schema.insert("type".to_string(), json!("object"));
    schema.insert("title".to_string(), json!(label));
    if let Some(description) = description {
        schema.insert("description".to_string(), json!(description));
    }
    schema.insert("additionalProperties".to_string(), json!(false));
    schema.insert("properties".to_string(), Value::Object(properties));
    if !required.is_empty() {
        schema.insert("required".to_string(), Value::Array(required));
    }
    schema.insert("x-rdra-ish-kind".to_string(), json!(kind));
    schema.insert("x-rdra-ish-id".to_string(), json!(id));
    if !indexes.is_empty() {
        schema.insert("x-rdra-ish-indexes".to_string(), json!(indexes));
    }
    if !unique_constraints.is_empty() {
        schema.insert(
            "x-rdra-ish-unique-constraints".to_string(),
            json!(unique_constraints),
        );
    }
    Value::Object(schema)
}

fn column_schema(model: &SemanticModel, fk: Option<&ForeignKey>, column: &ModelColumn) -> Value {
    let mut schema = match &column.col_type {
        ColumnType::Int => json!({ "type": json_type("integer", column.is_nullable) }),
        ColumnType::String => json!({ "type": json_type("string", column.is_nullable) }),
        ColumnType::Money | ColumnType::Decimal => {
            json!({ "type": json_type("number", column.is_nullable) })
        }
        ColumnType::DateTime => json!({
            "type": json_type("string", column.is_nullable),
            "format": "date-time"
        }),
        ColumnType::Date => json!({
            "type": json_type("string", column.is_nullable),
            "format": "date"
        }),
        ColumnType::Bool => json!({ "type": json_type("boolean", column.is_nullable) }),
        ColumnType::Enum(_) | ColumnType::Variation(_) => {
            let mut enum_values: Vec<Value> = model
                .column_values(column)
                .unwrap_or(&[])
                .iter()
                .map(|value| Value::String(value.clone()))
                .collect();
            if column.is_nullable {
                enum_values.push(Value::Null);
            }
            json!({
                "type": json_type("string", column.is_nullable),
                "enum": enum_values
            })
        }
    };

    if let Some(obj) = schema.as_object_mut() {
        if let Some(label) = &column.label {
            obj.insert("title".to_string(), json!(label));
        }
        if let Some(default) = &column.default_val {
            obj.insert(
                "default".to_string(),
                default_value(default, &column.col_type),
            );
        }
        insert_column_extensions(model, obj, fk, column);
    }

    schema
}

fn insert_column_extensions(
    model: &SemanticModel,
    obj: &mut Map<String, Value>,
    fk: Option<&ForeignKey>,
    column: &ModelColumn,
) {
    if column.is_pk {
        obj.insert("x-rdra-ish-primary-key".to_string(), json!(true));
    }
    if column.is_unique {
        obj.insert("x-rdra-ish-unique".to_string(), json!(true));
    }
    if column.is_indexed {
        obj.insert("x-rdra-ish-indexed".to_string(), json!(true));
    }
    if let Some(fk) = fk {
        obj.insert("x-rdra-ish-foreign-key".to_string(), json!(true));
        obj.insert(
            "x-rdra-ish-fk-target".to_string(),
            json!(model.entities[fk.parent].id),
        );
        // Only a composite key needs the pairing, and only a named key the role, so a
        // single-column key under the default role prints as it always has.
        if fk.columns.len() > 1 {
            if let Some((_, parent_column)) = fk.columns.iter().find(|(c, _)| *c == column.name) {
                obj.insert(
                    "x-rdra-ish-fk-target-column".to_string(),
                    json!(parent_column),
                );
            }
        }
        if let Some(role) = &fk.role {
            obj.insert("x-rdra-ish-fk-role".to_string(), json!(role));
        }
        if fk.optional() {
            obj.insert("x-rdra-ish-fk-optional".to_string(), json!(true));
        }
        // A child-side `1..*` / `1..1`: each parent has at least one child row.
        if fk.children.min == Min::One {
            obj.insert("x-rdra-ish-fk-min-children".to_string(), json!(1));
        }
        if let Some(action) = fk.on_delete {
            obj.insert(
                "x-rdra-ish-fk-on-delete".to_string(),
                json!(action.as_str()),
            );
        }
        if let Some(action) = fk.on_update {
            obj.insert(
                "x-rdra-ish-fk-on-update".to_string(),
                json!(action.as_str()),
            );
        }
    }
    if !column.check_constraints.is_empty() {
        obj.insert(
            "x-rdra-ish-check-constraints".to_string(),
            json!(column.check_constraints),
        );
    }
    if column.is_soft_delete {
        obj.insert("x-rdra-ish-soft-delete".to_string(), json!(true));
    }
    if column.is_history {
        obj.insert("x-rdra-ish-history".to_string(), json!(true));
    }
    if column.is_tenant_scope {
        obj.insert("x-rdra-ish-tenant-scope".to_string(), json!(true));
    }
    if let Some(expr) = &column.derived_expr {
        obj.insert("x-rdra-ish-derived".to_string(), json!(expr));
    }
}

fn json_type(base: &str, nullable: bool) -> Value {
    if nullable {
        Value::Array(vec![
            Value::String(base.to_string()),
            Value::String("null".to_string()),
        ])
    } else {
        Value::String(base.to_string())
    }
}

fn default_value(value: &str, col_type: &ColumnType) -> Value {
    match col_type {
        ColumnType::Int => value
            .parse::<i64>()
            .map(|number| Value::Number(number.into()))
            .unwrap_or_else(|_| Value::String(value.to_string())),
        ColumnType::Money | ColumnType::Decimal => value
            .parse::<f64>()
            .ok()
            .and_then(Number::from_f64)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(value.to_string())),
        ColumnType::Bool => value
            .parse::<bool>()
            .map(Value::Bool)
            .unwrap_or_else(|_| Value::String(value.to_string())),
        _ => Value::String(value.to_string()),
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
    fn emits_json_schema_defs_for_dtos_and_entities() {
        let model = model_from(
            r#"
dto CreateOrderRequest "Create order request" {
  customer_id: Int
  note: String @null
}
entity Order "Order" description "Logical order" {
  id: Int @pk
  tenant_id: Int @tenant
  status: Enum(pending, paid) @default(pending)
  total: Money @check("total >= 0")
  deleted_at: DateTime @null @soft_delete
  net_total: Money @derived("total - discount")
  store_id: Int @index(status, store_id) @unique(status, store_id)
}
"#,
        );

        let json = JsonSchemaEmitter.emit(&model, &View::whole()).unwrap();
        let doc: Value = serde_json::from_str(&json).unwrap();

        assert_eq!(
            doc["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert_eq!(
            doc["$defs"]["Dto.CreateOrderRequest"]["required"],
            json!(["customer_id"])
        );
        assert_eq!(
            doc["$defs"]["Dto.CreateOrderRequest"]["properties"]["note"]["type"],
            json!(["string", "null"])
        );
        assert_eq!(doc["$defs"]["Entity.Order"]["description"], "Logical order");
        assert_eq!(
            doc["$defs"]["Entity.Order"]["properties"]["status"]["enum"],
            json!(["pending", "paid"])
        );
        assert_eq!(
            doc["$defs"]["Entity.Order"]["properties"]["tenant_id"]["x-rdra-ish-tenant-scope"],
            true
        );
        assert_eq!(
            doc["$defs"]["Entity.Order"]["properties"]["total"]["x-rdra-ish-check-constraints"],
            json!(["total >= 0"])
        );
        assert_eq!(
            doc["$defs"]["Entity.Order"]["x-rdra-ish-unique-constraints"],
            json!([["status", "store_id"]])
        );
    }

    #[test]
    fn variation_column_lists_the_variation_values_as_enum() {
        let model = model_from(
            r#"
variation Channel "Channel" {
  web
  phone
}
entity Order "Order" {
  id: Int @pk
  channel: Channel
  backup_channel: Channel @null
}
"#,
        );
        let json = JsonSchemaEmitter.emit(&model, &View::whole()).unwrap();
        let doc: Value = serde_json::from_str(&json).unwrap();
        let props = &doc["$defs"]["Entity.Order"]["properties"];
        assert_eq!(props["channel"]["enum"], json!(["web", "phone"]));
        assert_eq!(
            props["backup_channel"]["enum"],
            json!(["web", "phone", null])
        );
    }

    #[test]
    fn a_child_side_minimum_marks_the_key_column_and_only_then() {
        let model = model_from(
            "entity Order \"O\" { id: Int @pk }\nentity Line \"L\" { id: Int @pk }\nentity Note \"N\" { id: Int @pk }\nrelate(Line, Order, 1..*:1)\nrelate(Note, Order, N:1)\n",
        );
        let json = JsonSchemaEmitter.emit(&model, &View::whole()).unwrap();
        let doc: Value = serde_json::from_str(&json).unwrap();
        let line = &doc["$defs"]["Entity.Line"]["properties"]["order_id"];
        assert_eq!(line["x-rdra-ish-fk-min-children"], 1);
        let note = &doc["$defs"]["Entity.Note"]["properties"]["order_id"];
        assert!(note.get("x-rdra-ish-fk-min-children").is_none(), "{note}");
    }

    #[test]
    fn composite_key_columns_name_their_parent_column_and_a_named_key_its_role() {
        let model = model_from(
            r#"
entity Bin "Bin" {
  site: String @pk(site, shelf)
  shelf: Int
}
entity Address "Address" { id: Int @pk }
entity Stock "Stock" { id: Int @pk }
relate(Stock, Bin, N:1)
relate(Stock, Address, N:1)
relate(Stock, Address, N:1).role(bill_to)
"#,
        );
        let json = JsonSchemaEmitter.emit(&model, &View::whole()).unwrap();
        let doc: Value = serde_json::from_str(&json).unwrap();
        let props = &doc["$defs"]["Entity.Stock"]["properties"];
        for (column, parent) in [("bin_site", "site"), ("bin_shelf", "shelf")] {
            assert_eq!(props[column]["x-rdra-ish-foreign-key"], true);
            assert_eq!(props[column]["x-rdra-ish-fk-target"], "Bin");
            assert_eq!(props[column]["x-rdra-ish-fk-target-column"], parent);
        }
        assert_eq!(
            props["address_id"],
            json!({
                "type": "integer",
                "x-rdra-ish-foreign-key": true,
                "x-rdra-ish-fk-target": "Address"
            })
        );
        assert_eq!(props["bill_to_id"]["x-rdra-ish-fk-role"], "bill_to");
        assert!(props["bill_to_id"]
            .get("x-rdra-ish-fk-target-column")
            .is_none());
    }
}
