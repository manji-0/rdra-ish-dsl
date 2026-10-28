//! `states` CLI command.

use std::path::PathBuf;

use anyhow::Result;
use rdra_ish_core::entity_state_variants;
use rdra_ish_core::lifecycle::{derive_states, Scope, Severity};
use rdra_ish_emit::{
    state_pattern::{render_csv, render_json, render_table},
    typescript::emit_typescript_state_unions,
};

use crate::cli::StatesFormat;
use crate::list_output::{filter_entity_output, severity_word, state_diag_message};
use crate::load::{load_model, reject_model_errors};

pub fn run_states(
    inputs: &[PathBuf],
    format: StatesFormat,
    buc: Vec<String>,
    max_patterns: usize,
    entity: Option<String>,
) -> Result<()> {
    let (program, model, diags) = load_model(inputs)?;
    reject_model_errors(&program, &diags)?;

    if let Some(ref entity_id) = entity {
        if !model.entities.values().any(|e| e.id == *entity_id) {
            anyhow::bail!("unknown entity `{entity_id}`");
        }
    }
    for buc_id in &buc {
        if !model.bucs.values().any(|b| b.id == *buc_id) {
            anyhow::bail!("unknown buc `{buc_id}`");
        }
    }

    // Exit on structured severity (not string scan) so csv / typescript also fail-closed.
    let report = derive_states(&model, &Scope::from_bucs(&buc), max_patterns);
    eprintln!("{}", report.bounds_line());
    for d in report.unattributed() {
        eprintln!("{}: {}", severity_word(d.severity()), state_diag_message(d));
    }

    if let Some(ref entity_id) = entity {
        let other_errors: usize = report
            .views
            .iter()
            .filter(|r| r.entity_id != *entity_id)
            .flat_map(|r| r.diagnostics.iter())
            .filter(|d| d.severity() == Severity::Error)
            .count();
        if other_errors > 0 {
            eprintln!(
                "note: --entity `{entity_id}` filters stdout, but {other_errors} Error-severity state diagnostic(s) remain on other entities (exit still fail-closed)"
            );
        }
    }

    let output = match format {
        StatesFormat::Table => render_table(&report.views),
        StatesFormat::Csv => render_csv(&report.views)?,
        StatesFormat::Json => render_json(&report.views)?,
        StatesFormat::TypeScript => {
            emit_typescript_state_unions(&entity_state_variants(&model, &report.views))
        }
    };

    let rendered = if let Some(ref entity_id) = entity {
        filter_entity_output(&output, entity_id, &format)
    } else {
        output
    };

    print!("{}", rendered);

    if report.has_errors() {
        std::process::exit(1);
    }

    Ok(())
}
