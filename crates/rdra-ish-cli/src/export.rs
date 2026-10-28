//! CLI export artifact dispatch.

use std::num::NonZeroU8;

use anyhow::{bail, Result};
use rdra_ish_core::lifecycle::{has_errors, Bounds, Scope, Severity, TransitionSystem};
use rdra_ish_core::SemanticModel;
use rdra_ish_emit::{
    asyncapi::AsyncApiJsonEmitter,
    dbml::DbmlEmitter,
    json_schema::JsonSchemaEmitter,
    mermaid::ErMermaidEmitter,
    openapi::OpenApiJsonEmitter,
    plantuml::ErPlantUmlEmitter,
    tla::{self, PrintMode, TlaBundle},
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
        ExportKind::Tla => Ok((
            tla_bundle(model, Bounds::VERIFY, "RdraSpec")?.tla,
            "RdraSpec.tla",
        )),
    }
}

/// `--instance-count` and `--int-range-max` as the system's bounds.
pub fn tla_bounds(instance_count: u32, int_range_max: u32) -> Result<Bounds> {
    let Some(instances) = u8::try_from(instance_count).ok().and_then(NonZeroU8::new) else {
        bail!("--instance-count must be between 1 and 255");
    };
    let int_max = match u8::try_from(int_range_max) {
        Ok(0) => bail!(
            "--int-range-max must be >= 1 (IntRange 0..0 is not a useful verification domain)"
        ),
        Ok(n) => n,
        Err(_) => bail!("--int-range-max must be at most 255"),
    };
    if instances.get() == 1 {
        eprintln!(
            "warning: --instance-count 1 yields singleton Entity_Ids; multi-entity .along / quantifiers cannot distinguish instances"
        );
    }
    Ok(Bounds { instances, int_max })
}

/// The whole system printed in Check mode, after the `checked_system` gate.
pub fn tla_bundle(model: &SemanticModel, bounds: Bounds, module_name: &str) -> Result<TlaBundle> {
    let ts = checked_system(model, bounds)?;
    Ok(tla::print(&ts, PrintMode::Check, module_name))
}

/// Builds the transition system at `bounds` and reports its diagnostics. Any Error diagnostic
/// stops the command, the gate `states` and `check` apply too.
pub fn checked_system(model: &SemanticModel, bounds: Bounds) -> Result<TransitionSystem> {
    let built = TransitionSystem::build(model, bounds, &Scope::All);
    for d in &built.diags {
        let level = match d.diag.severity() {
            Severity::Info => continue,
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        let ids: Vec<&str> = d
            .entities
            .iter()
            .map(|ek| model.entities[*ek].id.as_str())
            .collect();
        if ids.is_empty() {
            eprintln!("{level}: {}", d.diag);
        } else {
            eprintln!("{level}: {}: {}", ids.join(", "), d.diag);
        }
    }
    if has_errors(&built.diags) {
        bail!("the lifecycle model has errors; no TLA+ written");
    }
    Ok(built.ts)
}
