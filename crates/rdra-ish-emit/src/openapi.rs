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
    let request = related_dtos(model, NodeRef::Api(api), RelKind::Request);
    let response = related_dtos(model, NodeRef::Api(api), RelKind::Response);
    let error_response = related_dtos(model, NodeRef::Api(api), RelKind::ErrorResponse);

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

    let permissions = required_permissions(model, api);
    if !permissions.is_empty() {
        operation.insert(
            "x-rdra-ish-requires-permission".to_string(),
            json!(permissions),
        );
    }

    if let Some(path) = &api_model.path {
        let params = path_parameters(path);
        if !params.is_empty() {
            operation.insert("parameters".to_string(), Value::Array(params));
        }
    }

    if let Some(schema) = dto_content_schema(model, &request) {
        operation.insert(
            "requestBody".to_string(),
            json!({
                "required": true,
                "content": {
                    "application/json": {
                        "schema": schema
                    }
                }
            }),
        );
    }

    let mut responses = Map::new();
    if response.is_empty() {
        responses.insert("204".to_string(), json!({ "description": "No Content" }));
    } else if let Some(schema) = dto_content_schema(model, &response) {
        let mut body = json!({
            "description": "OK",
            "content": {
                "application/json": {
                    "schema": schema
                }
            }
        });
        if response.len() > 1 {
            body.as_object_mut().unwrap().insert(
                "x-rdra-ish-ambiguous-status".to_string(),
                json!("multiple response DTOs; status codes are not modeled — emitted as oneOf under 200"),
            );
        }
        responses.insert("200".to_string(), body);
    }
    if !error_response.is_empty() {
        if let Some(schema) = dto_content_schema(model, &error_response) {
            let mut body = json!({
                "description": "Error",
                "content": {
                    "application/json": {
                        "schema": schema
                    }
                }
            });
            if error_response.len() > 1 {
                body.as_object_mut().unwrap().insert(
                    "x-rdra-ish-ambiguous-status".to_string(),
                    json!("multiple error_response DTOs; status codes are not modeled — emitted as oneOf under default"),
                );
            }
            responses.insert("default".to_string(), body);
        }
    }
    operation.insert("responses".to_string(), Value::Object(responses));

    Value::Object(operation)
}

fn dto_content_schema(model: &SemanticModel, dtos: &[DtoKey]) -> Option<Value> {
    match dtos {
        [] => None,
        [dto] => Some(schema_ref(model.dtos[*dto].id.as_str())),
        many => {
            let one_of: Vec<Value> = many
                .iter()
                .map(|dto| schema_ref(model.dtos[*dto].id.as_str()))
                .collect();
            Some(json!({ "oneOf": one_of }))
        }
    }
}

/// Extract `{param}` segments from an OpenAPI-style path template.
fn path_parameters(path: &str) -> Vec<Value> {
    let mut params: Vec<Value> = Vec::new();
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            break;
        };
        let name = &after[..end];
        if !name.is_empty() && !params.iter().any(|p: &Value| p["name"] == name) {
            params.push(json!({
                "name": name,
                "in": "path",
                "required": true,
                "schema": { "type": "string" }
            }));
        }
        rest = &after[end + 1..];
    }
    params
}

fn required_permissions(model: &SemanticModel, api: rdra_ish_core::model::ApiKey) -> Vec<String> {
    let mut ids: Vec<String> = model
        .relations
        .iter()
        .filter(|rel| {
            rel.kind == RelKind::RequiresPermission
                && matches!(
                    (&rel.from, &rel.to),
                    (NodeRef::Api(a), NodeRef::Permission(_)) if *a == api
                )
        })
        .filter_map(|rel| match rel.to {
            NodeRef::Permission(p) => Some(model.permissions[p].id.clone()),
            _ => None,
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

fn related_dtos(model: &SemanticModel, api: NodeRef, kind: RelKind) -> Vec<DtoKey> {
    let mut dtos: Vec<DtoKey> = model
        .relations
        .iter()
        .filter(|rel| rel.kind == kind && rel.from == api)
        .filter_map(|rel| match rel.to {
            NodeRef::Dto(dto) => Some(dto),
            _ => None,
        })
        .collect();
    dtos.sort_by_key(|dto| model.dtos[*dto].id.as_str());
    dtos.dedup();
    dtos
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
                other => json!({
                    "type": "apiKey",
                    "in": "header",
                    "name": "X-Auth",
                    "x-rdra-ish-auth-scheme": other
                }),
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

    #[test]
    fn emits_path_parameters_permissions_and_unknown_auth_extension() {
        let model = model_from(
            r#"
permission OrderRead "Order read"
api GetOrder "Get order" method GET path "/orders/{id}" auth oauth2
requires_permission(GetOrder, OrderRead)
"#,
        );

        let json = OpenApiJsonEmitter.emit(&model, &View::whole()).unwrap();
        let doc: Value = serde_json::from_str(&json).unwrap();

        assert_eq!(
            doc["paths"]["/orders/{id}"]["get"]["parameters"],
            json!([{
                "name": "id",
                "in": "path",
                "required": true,
                "schema": { "type": "string" }
            }])
        );
        assert_eq!(
            doc["paths"]["/orders/{id}"]["get"]["x-rdra-ish-requires-permission"],
            json!(["OrderRead"])
        );
        assert_eq!(
            doc["components"]["securitySchemes"]["oauth2"]["type"],
            "apiKey"
        );
        assert_eq!(
            doc["components"]["securitySchemes"]["oauth2"]["x-rdra-ish-auth-scheme"],
            "oauth2"
        );
    }

    #[test]
    fn path_parameters_extracts_multiple_segments() {
        let params = path_parameters("/tenants/{tenant_id}/orders/{id}");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0]["name"], "tenant_id");
        assert_eq!(params[1]["name"], "id");
    }

    #[test]
    fn emits_oneof_for_multiple_response_dtos() {
        let model = model_from(
            r#"
api CreateOrder "Create order" method POST path "/orders"
dto AcceptedResponse "Accepted"
dto OrderResponse "Order"
response(CreateOrder, OrderResponse)
response(CreateOrder, AcceptedResponse)
"#,
        );
        let json = OpenApiJsonEmitter.emit(&model, &View::whole()).unwrap();
        let doc: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            doc["paths"]["/orders"]["post"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["oneOf"],
            json!([
                { "$ref": "#/components/schemas/AcceptedResponse" },
                { "$ref": "#/components/schemas/OrderResponse" }
            ])
        );
        assert!(
            doc["paths"]["/orders"]["post"]["responses"]["200"]["x-rdra-ish-ambiguous-status"]
                .as_str()
                .unwrap_or("")
                .contains("oneOf")
        );
    }
}
