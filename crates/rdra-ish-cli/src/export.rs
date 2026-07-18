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
