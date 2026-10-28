//! `states` CLI command.

use std::path::PathBuf;

use anyhow::Result;
use rdra_ish_core::lifecycle::{derive_states_built, Scope, Site};
use rdra_ish_core::{entity_state_variants, format_location};
use rdra_ish_emit::{
    state_pattern::{render_csv, render_json, render_table},
    typescript::emit_typescript_state_unions,
};

use crate::cli::{ScopeArgs, StatesFormat};
use crate::list_output::{severity_word, state_diag_message};
use crate::load::{load_model, reject_model_errors};

pub fn run_states(
    inputs: &[PathBuf],
    format: StatesFormat,
    max_patterns: usize,
    scope: &ScopeArgs,
) -> Result<()> {
    let (program, model, diags) = load_model(inputs)?;
    reject_model_errors(&program, &diags)?;
    let scope = Scope::parse(&model, &scope.buc, &scope.entity)?;

    // Exit on structured severity (not string scan) so csv / typescript also fail-closed.
    let (report, ts) = derive_states_built(&model, &scope, max_patterns);
    eprintln!("{}", report.bounds_line(&model));
    for d in report.unattributed() {
        eprintln!("{}: {}", severity_word(d.severity()), state_diag_message(d));
    }
    for (ids, d) in report.outside_views(&model) {
        eprintln!(
            "{}: {}: {}",
            severity_word(d.severity()),
            ids.join(", "),
            state_diag_message(d)
        );
    }
    let not_checked = report.not_checked(&ts, &model);
    if !not_checked.is_empty() {
        eprintln!("{not_checked}");
    }

    let output = match format {
        StatesFormat::Table => render_table(&report.views),
        StatesFormat::Csv => render_csv(&report.views)?,
        StatesFormat::Json => render_json(&report.views, &model, &|site: &Site| {
            format_location(&program, site.span.as_ref()?)
        })?,
        StatesFormat::TypeScript => {
            emit_typescript_state_unions(&entity_state_variants(&model, &report.views))
        }
    };
    print!("{}", output);

    if report.has_errors() {
        std::process::exit(1);
    }

    Ok(())
}
