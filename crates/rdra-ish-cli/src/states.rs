//! `states` CLI command.

use std::path::PathBuf;

use anyhow::Result;
use rdra_ish_emit::{
    state_pattern::{StatePatternCsvEmitter, StatePatternJsonEmitter, StatePatternTableEmitter},
    typescript::TypeScriptStateUnionEmitter,
    Emitter, View,
};

use crate::cli::StatesFormat;
use crate::list_output::filter_entity_output;
use crate::load::{eprint_diagnostic, load_model};

pub fn run_states(
    inputs: &[PathBuf],
    format: StatesFormat,
    buc: Vec<String>,
    max_patterns: usize,
    entity: Option<String>,
) -> Result<()> {
    let (program, model, diags) = load_model(inputs)?;

    for diag in &diags {
        eprint_diagnostic(&program, diag);
    }

    let view = View::bucs(buc);

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

    if let Some(ref entity_id) = entity {
        let filtered = filter_entity_output(&output, entity_id, &format);
        print!("{}", filtered);
    } else {
        print!("{}", output);
    }

    Ok(())
}
