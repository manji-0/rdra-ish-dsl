use crate::{EmitError, Emitter, View};
use rdra_ish_core::{collect_event_flows, model::SemanticModel};
use serde_json::{json, Map, Value};

pub struct AsyncApiJsonEmitter;

impl Emitter for AsyncApiJsonEmitter {
    fn emit(&self, model: &SemanticModel, _view: &View) -> Result<String, EmitError> {
        let doc = asyncapi_document(model);
        Ok(serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".to_string()) + "\n")
    }
}

fn asyncapi_document(model: &SemanticModel) -> Value {
    let flows = collect_event_flows(model);
    let mut channels = Map::new();
    let mut operations = Map::new();
    let mut messages = Map::new();

    for flow in &flows {
        let Some(event) = model.events.get(flow.event) else {
            continue;
        };
        let message_key = asyncapi_key(&event.id);

        channels.insert(
            message_key.clone(),
            json!({
                "address": event.id,
                "messages": {
                    message_key.clone(): {
                        "$ref": format!("#/components/messages/{message_key}")
                    }
                }
            }),
        );

        messages.insert(message_key.clone(), event_message(model, flow));

        if !flow.raised_by.is_empty() || model.outbox_events.contains(&flow.event) {
            let mut operation = operation_base("send", &message_key);
            if let Some(obj) = operation.as_object_mut() {
                obj.insert(
                    "x-rdra-ish-raised-by-usecases".to_string(),
                    json!(usecase_ids(model, &flow.raised_by)),
                );
                if model.outbox_events.contains(&flow.event) {
                    obj.insert("x-rdra-ish-outbox".to_string(), json!(true));
                }
            }
            operations.insert(format!("publish{message_key}"), operation);
        }

        if !flow.triggers_ucs.is_empty()
            || !flow.triggers_bucs.is_empty()
            || !flow.transitions.is_empty()
        {
            let mut operation = operation_base("receive", &message_key);
            if let Some(obj) = operation.as_object_mut() {
                obj.insert(
                    "x-rdra-ish-triggers-usecases".to_string(),
                    json!(usecase_ids(model, &flow.triggers_ucs)),
                );
                obj.insert(
                    "x-rdra-ish-triggers-bucs".to_string(),
                    json!(buc_ids(model, &flow.triggers_bucs)),
                );
                obj.insert(
                    "x-rdra-ish-transitions".to_string(),
                    json!(transition_ids(model, &flow.transitions)),
                );
            }
            operations.insert(format!("consume{message_key}"), operation);
        }
    }

    json!({
        "asyncapi": "3.1.0",
        "info": {
            "title": "RDRA-ish event catalog",
            "version": "0.1.0"
        },
        "channels": channels,
        "operations": operations,
        "components": {
            "messages": messages
        }
    })
}

fn event_message(model: &SemanticModel, flow: &rdra_ish_core::EventFlow) -> Value {
    let event = &model.events[flow.event];
    let mut message = Map::new();
    message.insert("name".to_string(), json!(event.id));
    message.insert("title".to_string(), json!(event.label));
    if let Some(description) = &event.description {
        message.insert("summary".to_string(), json!(description));
    }
    message.insert(
        "payload".to_string(),
        json!({
            "type": "object",
            "additionalProperties": true,
            "x-rdra-ish-payload": "unspecified",
            "x-rdra-ish-event-id": event.id
        }),
    );
    message.insert(
        "x-rdra-ish-raised-by-usecases".to_string(),
        json!(usecase_ids(model, &flow.raised_by)),
    );
    message.insert(
        "x-rdra-ish-triggers-usecases".to_string(),
        json!(usecase_ids(model, &flow.triggers_ucs)),
    );
    message.insert(
        "x-rdra-ish-triggers-bucs".to_string(),
        json!(buc_ids(model, &flow.triggers_bucs)),
    );
    message.insert(
        "x-rdra-ish-transitions".to_string(),
        json!(transition_ids(model, &flow.transitions)),
    );
    if model.outbox_events.contains(&flow.event) {
        message.insert("x-rdra-ish-outbox".to_string(), json!(true));
    }
    Value::Object(message)
}

fn operation_base(action: &str, message_key: &str) -> Value {
    json!({
        "action": action,
        "channel": {
            "$ref": format!("#/channels/{message_key}")
        },
        "messages": [
            {
                "$ref": format!("#/channels/{message_key}/messages/{message_key}")
            }
        ]
    })
}

fn usecase_ids(model: &SemanticModel, keys: &[rdra_ish_core::model::UseCaseKey]) -> Vec<String> {
    let mut ids: Vec<_> = keys
        .iter()
        .filter_map(|key| model.use_cases.get(*key).map(|uc| uc.id.clone()))
        .collect();
    ids.sort();
    ids
}

fn buc_ids(model: &SemanticModel, keys: &[rdra_ish_core::model::BucKey]) -> Vec<String> {
    let mut ids: Vec<_> = keys
        .iter()
        .filter_map(|key| model.bucs.get(*key).map(|buc| buc.id.clone()))
        .collect();
    ids.sort();
    ids
}

fn transition_ids(_model: &SemanticModel, transitions: &[(String, String)]) -> Vec<Value> {
    let mut ids: Vec<_> = transitions
        .iter()
        .map(|(from, to)| {
            json!({
                "from": from,
                "to": to
            })
        })
        .collect();
    ids.sort_by_key(|value| value.to_string());
    ids
}

fn asyncapi_key(id: &str) -> String {
    let mut key = String::new();
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
            key.push(ch);
        } else {
            key.push('_');
        }
    }
    if key.is_empty() {
        "Event".to_string()
    } else {
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|diag| !diag.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        model
    }

    #[test]
    fn emits_asyncapi_event_catalog_from_event_flows() {
        let model = model_from(
            r#"
usecase SignEncounter "Sign encounter"
buc ClaimSubmission "Claim submission"
entity Encounter "Encounter" {
  id: Int @pk
  status: Enum(draft, signed) @default(draft)
}
event EncounterSigned "Encounter signed"
  description "Published when an encounter is signed."
raises(SignEncounter, EncounterSigned)
triggers(EncounterSigned, ClaimSubmission)
transitions(Encounter.status, EncounterSigned, draft -> signed)
outbox(EncounterSigned)
"#,
        );

        let json = AsyncApiJsonEmitter.emit(&model, &View::whole()).unwrap();
        let doc: Value = serde_json::from_str(&json).unwrap();

        assert_eq!(doc["asyncapi"], "3.1.0");
        assert_eq!(
            doc["channels"]["EncounterSigned"]["messages"]["EncounterSigned"]["$ref"],
            "#/components/messages/EncounterSigned"
        );
        assert_eq!(
            doc["components"]["messages"]["EncounterSigned"]["x-rdra-ish-outbox"],
            true
        );
        assert_eq!(
            doc["operations"]["publishEncounterSigned"]["action"],
            "send"
        );
        assert_eq!(
            doc["operations"]["consumeEncounterSigned"]["x-rdra-ish-transitions"][0]["to"],
            "signed"
        );
    }
}
