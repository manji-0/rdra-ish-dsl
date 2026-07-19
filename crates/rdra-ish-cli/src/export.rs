//! CLI export artifact dispatch.

use anyhow::Result;
use rdra_ish_core::SemanticModel;
use rdra_ish_emit::{
    asyncapi::AsyncApiJsonEmitter,
    dbml::DbmlEmitter,
    json_schema::JsonSchemaEmitter,
    mermaid::ErMermaidEmitter,
    openapi::OpenApiJsonEmitter,
    plantuml::ErPlantUmlEmitter,
    tla::{TlaBundle, TlaPlusEmitter},
    typescript::TypeScriptStateUnionEmitter,
    Emitter, View,
};

use crate::cli::ExportKind;

pub fn export_artifact(
    model: &SemanticModel,
    kind: &ExportKind,
    view: &View,
) -> Result<(String, &'static str)> {
    match kind {
        ExportKind::Openapi => Ok((OpenApiJsonEmitter.emit(model, view)?, "openapi.json")),
        ExportKind::Asyncapi => Ok((AsyncApiJsonEmitter.emit(model, view)?, "asyncapi.json")),
        ExportKind::Dbml => Ok((DbmlEmitter.emit(model, view)?, "schema.dbml")),
        ExportKind::JsonSchema => Ok((JsonSchemaEmitter.emit(model, view)?, "json-schema.json")),
        ExportKind::TypeScriptStates => Ok((
            TypeScriptStateUnionEmitter::default().emit(model, view)?,
            "entity-states.ts",
        )),
        ExportKind::MermaidEr => Ok((ErMermaidEmitter.emit(model, view)?, "er.mmd")),
        ExportKind::PlantumlEr => Ok((ErPlantUmlEmitter.emit(model, view)?, "er.puml")),
        ExportKind::Tla => Ok((TlaPlusEmitter::default().emit(model, view)?, "RdraSpec.tla")),
    }
}

pub fn export_tla_bundle(model: &SemanticModel, view: &View) -> Result<TlaBundle> {
    Ok(TlaPlusEmitter::default().emit_bundle(model, view)?)
}

pub fn export_tla_bundle_named(
    model: &SemanticModel,
    view: &View,
    module_name: &str,
) -> Result<TlaBundle> {
    Ok(TlaPlusEmitter::default().emit_bundle_named(model, view, module_name)?)
}

/// Warnings that mean the TLA+ artifact does not preserve source obligations.
pub fn tla_obligation_errors(warnings: &[String]) -> Option<String> {
    let fatal: Vec<&str> = warnings
        .iter()
        .filter(|w| {
            w.contains("not exported")
                || w.contains("not yet mapped")
                || w.contains("contradictory after.assert")
                || w.contains("duplicate property")
                || w.contains("duplicate action")
        })
        .map(String::as_str)
        .collect();
    if fatal.is_empty() {
        None
    } else {
        Some(format!(
            "TLA+ export dropped or corrupted proof obligations:\n  - {}",
            fatal.join("\n  - ")
        ))
    }
}
