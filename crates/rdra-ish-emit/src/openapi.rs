use crate::{EmitError, Emitter, View};
use rdra_ish_core::model::{ColumnType, DtoKey, NodeRef, RelKind, SemanticModel};
use serde_json::{json, Map, Value};

pub struct OpenApiJsonEmitter;

impl Emitter for OpenApiJsonEmitter {
    fn emit(&self, model: &SemanticModel, _view: &View) -> Result<String, EmitError> {
        let doc = openapi_document(model);
        Ok(serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".to_string()) + "\n")
    }
}

fn openapi_document(model: &SemanticModel) -> Value {
    let mut paths = Map::new();
    let mut apis: Vec<_> = model.apis.iter().collect();
    apis.sort_by_key(|(_, api)| api.id.as_str());

    for (api_key, api) in apis {
        let (Some(method), Some(path)) = (&api.method, &api.path) else {
            continue;
        };
        let method = method.to_ascii_lowercase();
        let operation = openapi_operation(model, api_key);
        let path_entry = paths.entry(path.clone()).or_insert_with(|| json!({}));
        if let Some(obj) = path_entry.as_object_mut() {
            obj.insert(method, operation);
        }
    }

    let mut schemas = Map::new();
    let mut dtos: Vec<_> = model.dtos.iter().collect();
    dtos.sort_by_key(|(_, dto)| dto.id.as_str());
    for (_, dto) in dtos {
        schemas.insert(dto.id.clone(), dto_schema(dto));
    }

    let mut components = Map::new();
    components.insert("schemas".to_string(), Value::Object(schemas));
    let security_schemes = security_schemes(model);
    if !security_schemes.is_empty() {
        components.insert(
            "securitySchemes".to_string(),
            Value::Object(security_schemes),
        );
    }

    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "RDRA-ish API",
            "version": "0.1.0"
        },
        "paths": paths,
        "components": components
    })
}

fn openapi_operation(model: &SemanticModel, api: rdra_ish_core::model::ApiKey) -> Value {
    let api_model = &model.apis[api];
    let request = related_dto(model, NodeRef::Api(api), RelKind::Request);
    let response = related_dto(model, NodeRef::Api(api), RelKind::Response);
    let error_response = related_dto(model, NodeRef::Api(api), RelKind::ErrorResponse);

    let mut operation = Map::new();
    operation.insert("operationId".to_string(), json!(api_model.id));
    operation.insert("summary".to_string(), json!(api_model.label));
    if let Some(description) = &api_model.description {
        operation.insert("description".to_string(), json!(description));
    }
    if let Some(idempotency) = &api_model.idempotency {
        operation.insert("x-rdra-ish-idempotency".to_string(), json!(idempotency));
    }
    if let Some(mode) = &api_model.mode {
        operation.insert("x-rdra-ish-mode".to_string(), json!(mode));
    }
    if let Some(auth) = &api_model.auth_scheme {
        let mut requirement = Map::new();
        requirement.insert(auth.clone(), json!([]));
        operation.insert(
            "security".to_string(),
            Value::Array(vec![Value::Object(requirement)]),
        );
    }
    if let Some(dto) = request {
        operation.insert(
            "requestBody".to_string(),
            json!({
                "required": true,
                "content": {
                    "application/json": {
                        "schema": schema_ref(model.dtos[dto].id.as_str())
                    }
                }
            }),
        );
    }

    let mut responses = Map::new();
    match response {
        Some(dto) => {
            responses.insert(
                "200".to_string(),
                json!({
                    "description": "OK",
                    "content": {
                        "application/json": {
                            "schema": schema_ref(model.dtos[dto].id.as_str())
                        }
                    }
                }),
            );
        }
        None => {
            responses.insert("204".to_string(), json!({ "description": "No Content" }));
        }
    }
    if let Some(dto) = error_response {
        responses.insert(
            "default".to_string(),
            json!({
                "description": "Error",
                "content": {
                    "application/json": {
                        "schema": schema_ref(model.dtos[dto].id.as_str())
                    }
                }
            }),
        );
    }
    operation.insert("responses".to_string(), Value::Object(responses));

    Value::Object(operation)
}

fn related_dto(model: &SemanticModel, api: NodeRef, kind: RelKind) -> Option<DtoKey> {
    model
        .relations
        .iter()
        .filter(|rel| rel.kind == kind && rel.from == api)
        .filter_map(|rel| match rel.to {
            NodeRef::Dto(dto) => Some(dto),
            _ => None,
        })
        .min_by_key(|dto| model.dtos[*dto].id.as_str())
}

fn dto_schema(dto: &rdra_ish_core::model::Dto) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for field in &dto.fields {
        properties.insert(field.name.clone(), column_schema(field));
        if !field.is_nullable {
            required.push(Value::String(field.name.clone()));
        }
    }

    let mut schema = Map::new();
    schema.insert("type".to_string(), json!("object"));
    schema.insert("properties".to_string(), Value::Object(properties));
    if !required.is_empty() {
        schema.insert("required".to_string(), Value::Array(required));
    }
    Value::Object(schema)
}

fn column_schema(col: &rdra_ish_core::model::ModelColumn) -> Value {
    let mut schema = match &col.col_type {
        ColumnType::Int => json!({ "type": "integer", "format": "int64" }),
        ColumnType::String => json!({ "type": "string" }),
        ColumnType::Money | ColumnType::Decimal => json!({ "type": "number", "format": "double" }),
        ColumnType::DateTime => json!({ "type": "string", "format": "date-time" }),
        ColumnType::Date => json!({ "type": "string", "format": "date" }),
        ColumnType::Bool => json!({ "type": "boolean" }),
        ColumnType::Enum(values) => json!({ "type": "string", "enum": values }),
    };
    if col.is_nullable {
        schema["nullable"] = json!(true);
    }
    if let Some(label) = &col.label {
        schema["title"] = json!(label);
    }
    schema
}

fn schema_ref(name: &str) -> Value {
    json!({ "$ref": format!("#/components/schemas/{name}") })
}

fn security_schemes(model: &SemanticModel) -> Map<String, Value> {
    let mut schemes = Map::new();
    let mut apis: Vec<_> = model.apis.iter().collect();
    apis.sort_by_key(|(_, api)| api.id.as_str());
    for (_, api) in apis {
        let Some(auth) = &api.auth_scheme else {
            continue;
        };
        schemes
            .entry(auth.clone())
            .or_insert_with(|| match auth.as_str() {
                "bearer" => json!({ "type": "http", "scheme": "bearer" }),
                "api_key" => json!({ "type": "apiKey", "in": "header", "name": "X-API-Key" }),
                other => json!({ "type": "http", "scheme": other }),
            });
    }
    schemes
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
    fn emits_openapi_paths_components_and_auth_from_api_contracts() {
        let model = model_from(
            r#"
api CreateOrder "Create order" method POST path "/orders" idempotency idempotent mode sync auth bearer
dto CreateOrderRequest "Create order request" {
  customer_id: Int
  note: String @null
}
dto OrderResponse "Order response" {
  order_id: Int
  status: Enum(accepted, rejected)
}
dto ErrorResponse "Error response" {
  code: String
}
request(CreateOrder, CreateOrderRequest)
response(CreateOrder, OrderResponse)
error_response(CreateOrder, ErrorResponse)
"#,
        );

        let json = OpenApiJsonEmitter.emit(&model, &View::whole()).unwrap();
        let doc: Value = serde_json::from_str(&json).unwrap();

        assert_eq!(doc["openapi"], "3.0.3");
        assert_eq!(
            doc["paths"]["/orders"]["post"]["operationId"],
            "CreateOrder"
        );
        assert_eq!(
            doc["paths"]["/orders"]["post"]["requestBody"]["content"]["application/json"]["schema"]
                ["$ref"],
            "#/components/schemas/CreateOrderRequest"
        );
        assert_eq!(
            doc["paths"]["/orders"]["post"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/OrderResponse"
        );
        assert_eq!(
            doc["components"]["schemas"]["CreateOrderRequest"]["required"],
            json!(["customer_id"])
        );
        assert_eq!(
            doc["components"]["schemas"]["OrderResponse"]["properties"]["status"]["enum"],
            json!(["accepted", "rejected"])
        );
        assert_eq!(
            doc["paths"]["/orders"]["post"]["security"],
            json!([{ "bearer": [] }])
        );
        assert_eq!(
            doc["components"]["securitySchemes"]["bearer"],
            json!({ "type": "http", "scheme": "bearer" })
        );
    }
}
