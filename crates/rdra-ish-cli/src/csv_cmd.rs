//! `csv` CLI command.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rdra_ish_emit::{
    csv::{
        ActorListCsvEmitter, ActorPermissionAuditCsvEmitter, ApiEntityMatrixCsvEmitter,
        ApiListCsvEmitter, BusinessInputCsvEmitter, EntityListCsvEmitter,
        PermissionCallableCsvEmitter, RelationMatrixCsvEmitter, ScreenConstraintCsvEmitter,
    },
    Emitter, View,
};
use std::fs;

use crate::cli::{CsvFormat, CsvKind};
use crate::list_output::{note_state_errors, reject_state_errors};
use crate::load::reject_model_errors;
use rdra_ish_core::{load_project, Project};

pub fn run_csv(inputs: &[PathBuf], kind: CsvKind, format: CsvFormat, out: PathBuf) -> Result<()> {
    let Project {
        program,
        model,
        diags,
        ..
    } = load_project(inputs, None)?;
    reject_model_errors(&program, &diags)?;
    // The entity list is the data layer; the other CSVs keep the lifecycle gate.
    if matches!(kind, CsvKind::Entity) {
        note_state_errors(&program, &model);
    } else {
        reject_state_errors(&program, &model)?;
    }

    let view = View::whole();

    let (csv_content, ext) = match kind {
        CsvKind::Actor => (ActorListCsvEmitter.emit(&model, &view)?, "actor.csv"),
        CsvKind::Entity => (EntityListCsvEmitter.emit(&model, &view)?, "entity.csv"),
        CsvKind::Matrix => (RelationMatrixCsvEmitter.emit(&model, &view)?, "matrix.csv"),
        CsvKind::Api => (ApiListCsvEmitter.emit(&model, &view)?, "api.csv"),
        CsvKind::ApiMatrix => (
            ApiEntityMatrixCsvEmitter.emit(&model, &view)?,
            "api-matrix.csv",
        ),
        CsvKind::ScreenConstraints => (
            ScreenConstraintCsvEmitter.emit(&model, &view)?,
            "screen-constraints.csv",
        ),
        CsvKind::PermissionCallables => (
            PermissionCallableCsvEmitter.emit(&model, &view)?,
            "permission-callables.csv",
        ),
        CsvKind::ActorPermissionAudit => (
            ActorPermissionAuditCsvEmitter.emit(&model, &view)?,
            "actor-permission-audit.csv",
        ),
        CsvKind::BusinessInputs => (
            BusinessInputCsvEmitter.emit(&model, &view)?,
            "business-inputs.csv",
        ),
    };

    let (csv_content, ext) = match format {
        CsvFormat::Csv => (csv_content, ext.to_string()),
        CsvFormat::Markdown => (
            rdra_ish_emit::markdown::from_csv(&csv_content)?,
            ext.replace(".csv", ".md"),
        ),
    };

    let out_path = if out.extension().is_some() {
        out.clone()
    } else {
        out.with_extension(ext.trim_start_matches("*."))
    };

    fs::write(&out_path, &csv_content)
        .with_context(|| format!("failed to write {}", out_path.display()))?;
    println!("wrote {}", out_path.display());

    Ok(())
}
