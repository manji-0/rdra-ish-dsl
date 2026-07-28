//! `states` CLI command.

use std::path::PathBuf;

use anyhow::Result;
use rdra_ish_core::{
    derive_state_patterns, state_diag_severity_with_policy, state_results_have_errors_with_policy,
    StateDiagPolicy,
};
use rdra_ish_emit::{
    state_pattern::{StatePatternCsvEmitter, StatePatternJsonEmitter, StatePatternTableEmitter},
    typescript::TypeScriptStateUnionEmitter,
    Emitter, View,
};

use crate::cli::StatesFormat;
use crate::list_output::filter_entity_output;
use crate::load::{load_model, reject_model_errors};

pub fn run_states(
    inputs: &[PathBuf],
    format: StatesFormat,
    buc: Vec<String>,
    max_patterns: usize,
    entity: Option<String>,
    allow_tla_only: bool,
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

    let policy = if allow_tla_only {
        StateDiagPolicy::allow_tla_only()
    } else {
        StateDiagPolicy::default()
    };

    let view = View::bucs(buc.clone());
    // Exit on structured severity (not string scan) so csv / typescript also fail-closed.
    let results = derive_state_patterns(&model, &buc, max_patterns);
    let has_error = state_results_have_errors_with_policy(&results, policy);

    if let Some(ref entity_id) = entity {
        let other_errors: usize = results
            .iter()
            .filter(|r| r.entity_id != *entity_id)
            .flat_map(|r| r.diagnostics.iter())
            .filter(|d| state_diag_severity_with_policy(d, policy).is_error())
            .count();
        if other_errors > 0 {
            eprintln!(
                "note: --entity `{entity_id}` filters stdout, but {other_errors} Error-severity state diagnostic(s) remain on other entities (exit still fail-closed)"
            );
        }
    }

    let output = match format {
        StatesFormat::Table => {
            let emitter = StatePatternTableEmitter { cap: max_patterns };
            emitter.emit(&model, &view)?
        }
        StatesFormat::Csv => {
            let emitter = StatePatternCsvEmitter { cap: max_patterns };
            emitter.emit(&model, &view)?
        }
        StatesFormat::Json => {
            let emitter = StatePatternJsonEmitter { cap: max_patterns };
            emitter.emit(&model, &view)?
        }
        StatesFormat::TypeScript => {
            let emitter = TypeScriptStateUnionEmitter { cap: max_patterns };
            emitter.emit(&model, &view)?
        }
    };

    let rendered = if let Some(ref entity_id) = entity {
        filter_entity_output(&output, entity_id, &format)
    } else {
        output
    };

    print!("{}", rendered);

    if has_error {
        std::process::exit(1);
    }

    Ok(())
}
