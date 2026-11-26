//! `codegen --format json`: the contract the printers print, as data (P0 of
//! docs/design/code-check.md). Types, states, use cases, functions, and obligations, each
//! with its model location and the names every language derives its own from. It is a view
//! of the [`DomainModel`], not the IR itself, so the IR can change without moving it.

use rdra_ish_core::location::LocatedSpan;
use rdra_ish_core::SemanticModel;
use serde_json::{json, Value};

use super::ir::{
    DomainModel, EnumDecl, Field, FieldType, FnDecl, InputType, RecordKind, UseCaseKind,
    VariantDecl,
};
use super::{CheckStatus, Options};

/// The value of the `format` key; a consumer refuses another.
pub const JSON_FORMAT: &str = "rdra-ish codegen json 1";

/// The contract of `domain` as pretty-printed JSON.
pub fn print(
    model: &SemanticModel,
    domain: &DomainModel,
    options: &Options,
    site: &dyn Fn(&LocatedSpan) -> Option<String>,
) -> String {
    let at = |kind: &str, id: &str| model.decl_sites.located(kind, id).as_ref().and_then(site);
    let value = json!({
        "format": JSON_FORMAT,
        "generator": format!("rdra-ish {}", options.version),
        "model": options.model_path,
        "check": match &options.check {
            CheckStatus::Passed { bounds, .. } => json!({ "passed": true, "bounds": bounds }),
            CheckStatus::Failed { errors } => json!({ "passed": false, "errors": errors }),
        },
        "types": types(model, domain, &at),
        "states": domain.sums.iter().map(|s| json!({
            "name": s.name,
            "entity": s.entity,
            "site": at("entity", &s.entity),
            "row": s.row,
            "kind": s.kind,
            "tag": field(&s.tag),
            "variants": s.variants.iter().map(variant).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "usecases": domain.usecases.iter().map(|uc| json!({
            "id": uc.id,
            "label": uc.label,
            "site": uc.site,
            "error": uc.error,
            "kind": match domain.usecase_kind(uc) {
                UseCaseKind::Functions => "functions",
                UseCaseKind::Unprinted => "unprinted",
                UseCaseKind::ReadOnly => "read-only",
            },
            "actions": uc.actions.iter().map(|a| {
                let skipped = domain.skipped.iter().find(|s| s.action == *a);
                json!({
                    "name": a,
                    "functions": domain.fns.iter().filter(|f| f.action == *a).map(|f| f.name.clone()).collect::<Vec<_>>(),
                    "dispatcher": domain.dispatchers.iter().find(|d| domain.fns.iter().any(|f| f.action == *a && d.cases.iter().any(|c| c.function == f.name))).map(|d| d.name.clone()),
                    "unprinted": skipped.map(|s| s.reason.clone()),
                })
            }).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "functions": domain.fns.iter().map(function).collect::<Vec<_>>(),
        "obligations": domain.obligations.iter().map(|o| json!({
            "name": o.name,
            "checked": o.checked,
            "text": o.text,
            "site": o.site,
        })).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&value).expect("a JSON value prints")
}

fn types(
    model: &SemanticModel,
    domain: &DomainModel,
    at: &dyn Fn(&str, &str) -> Option<String>,
) -> Value {
    let mut out: Vec<Value> = Vec::new();
    for e in &domain.enums {
        let variation = model
            .variations
            .values()
            .find(|v| super::type_ident(&v.id) == e.name);
        let mut decl = enum_decl(e);
        decl["variation"] = json!(variation.map(|v| v.id.clone()));
        decl["site"] = json!(variation.and_then(|v| at("variation", &v.id)));
        out.push(decl);
    }
    for id in &domain.ids {
        out.push(json!({
            "kind": "id",
            "name": id.name,
            "entity": id.entity,
            "repr": id.repr.name(),
        }));
    }
    for r in &domain.records {
        out.push(json!({
            "kind": match r.kind { RecordKind::Entity => "entity", RecordKind::Dto => "dto" },
            "name": r.name,
            "id": r.id,
            "site": at(match r.kind { RecordKind::Entity => "entity", RecordKind::Dto => "dto" }, &r.id),
            "fields": r.fields.iter().map(field).collect::<Vec<_>>(),
        }));
    }
    Value::Array(out)
}

fn enum_decl(e: &EnumDecl) -> Value {
    json!({
        "kind": "enum",
        "name": e.name,
        "values": e.values.iter().map(|v| json!({
            "wire": v.wire,
            "ident": v.ident,
            "label": v.label,
        })).collect::<Vec<_>>(),
    })
}

fn variant(v: &VariantDecl) -> Value {
    json!({
        "name": v.name,
        "value": v.value.wire,
        "fields": v.fields.iter().map(field).collect::<Vec<_>>(),
        "always_null": v.always_null.iter().map(|f| f.wire.clone()).collect::<Vec<_>>(),
    })
}

fn field(f: &Field) -> Value {
    json!({
        "name": f.wire,
        "domain_name": f.name,
        "ident": f.pascal,
        "type": ty(&f.ty),
        "nullable": f.nullable,
    })
}

fn ty(t: &FieldType) -> Value {
    match t {
        FieldType::Scalar(s) => json!({ "kind": "scalar", "name": s.name() }),
        FieldType::Enum(n) => json!({ "kind": "enum", "name": n }),
        FieldType::Id(n) => json!({ "kind": "id", "name": n }),
    }
}

fn input_ty(t: &InputType) -> Value {
    match t {
        InputType::Value(t) => ty(t),
        InputType::Rows(n) => json!({ "kind": "rows", "name": n }),
    }
}

fn function(f: &FnDecl) -> Value {
    json!({
        "name": f.name,
        "usecase": f.usecase,
        "action": f.action,
        "step": f.step.map(|(i, n)| json!({ "step": i, "of": n })),
        "rows": f.rows.iter().map(|r| json!({
            "name": r.name,
            "entity": r.entity,
            "type": r.ty,
            "state": r.state.as_ref().map(|(column, value)| json!({ "column": column, "value": value })),
        })).collect::<Vec<_>>(),
        "marks": f.marks_in.iter().map(|m| json!({ "name": m.name, "type": m.ty })).collect::<Vec<_>>(),
        "event": f.event_in.as_ref().map(|e| json!({ "name": e.name, "type": e.ty, "event": e.event })),
        "inputs": f.inputs.iter().map(|i| json!({
            "name": i.name,
            "type": input_ty(&i.ty),
            "nullable": i.nullable,
        })).collect::<Vec<_>>(),
        "conditions": f.conditions.iter().map(|c| json!({ "id": c.id, "site": c.site })).collect::<Vec<_>>(),
        "result": f.result,
        "writes": f.outputs.iter().map(|o| o.name.clone()).collect::<Vec<_>>(),
        "raises": f.raises.iter().map(|r| r.ty.clone()).collect::<Vec<_>>(),
    })
}
