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

fn tla_emitter(instance_count: u32, int_range_max: u32) -> Result<TlaPlusEmitter> {
    if instance_count == 0 {
        anyhow::bail!("--instance-count must be >= 1");
    }
    if int_range_max == 0 {
        anyhow::bail!(
            "--int-range-max must be >= 1 (IntRange 0..0 is not a useful verification domain)"
        );
    }
    if instance_count == 1 {
        eprintln!(
            "warning: --instance-count 1 yields singleton Entity_Ids; multi-entity .along / quantifiers cannot distinguish instances"
        );
    }
    Ok(TlaPlusEmitter {
        multi_axis: true,
        instance_count: i64::from(instance_count),
        int_range_max: i64::from(int_range_max),
    })
}

pub fn export_tla_bundle(
    model: &SemanticModel,
    view: &View,
    instance_count: u32,
    int_range_max: u32,
) -> Result<TlaBundle> {
    Ok(tla_emitter(instance_count, int_range_max)?.emit_bundle(model, view)?)
}

pub fn export_tla_bundle_named(
    model: &SemanticModel,
    view: &View,
    module_name: &str,
    instance_count: u32,
    int_range_max: u32,
) -> Result<TlaBundle> {
    Ok(tla_emitter(instance_count, int_range_max)?.emit_bundle_named(model, view, module_name)?)
}

/// Warnings that mean the TLA+ artifact does not preserve source obligations.
/// Only stable `[TLA_FATAL:code]` tags are treated as fatal.
pub fn tla_obligation_errors(warnings: &[String]) -> Option<String> {
    let fatal: Vec<&str> = warnings
        .iter()
        .filter(|w| w.contains("[TLA_FATAL:"))
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
