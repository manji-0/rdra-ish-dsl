//! `states` CLI command.

use std::path::PathBuf;

use anyhow::Result;
use rdra_ish_core::format_location;
use rdra_ish_core::lifecycle::{derive_states_built, Site};
use rdra_ish_emit::state_pattern::{render_csv, render_json, render_table};

use crate::cli::{BoundsArgs, ScopeArgs, StatesFormat};
use crate::load::load_clean;
use crate::memory::{charged, Budgeted};
use crate::state_findings::{state_diag_message, Finding};
use rdra_ish_core::memory::MemorySize;

pub fn run_states(
    inputs: &[PathBuf],
    format: StatesFormat,
    bounds: &BoundsArgs,
    (max_patterns, max_states, memory): (Option<usize>, Option<usize>, Option<MemorySize>),
    scope: &ScopeArgs,
) -> Result<()> {
    let (program, model, manifest) = load_clean(inputs)?;
    let bounds = bounds.bounds(&model, manifest.as_ref())?;
    let caps = BoundsArgs::caps(&bounds, max_patterns, max_states);
    let caps = charged(caps, memory, manifest.as_ref(), Budgeted::Check);
    let scope = scope.resolve(&model)?;

    // Exit on structured severity (not string scan) so csv and json also fail-closed.
    let (report, ts) = derive_states_built(&model, &scope, bounds, caps);
    eprintln!("{}", report.bounds_line(&model));
    for d in report.unattributed() {
        eprintln!("{}", Finding::new(d, &[], state_diag_message(d)));
    }
    for (ids, d) in report.outside_views(&model) {
        let message = format!("{}: {}", ids.join(", "), state_diag_message(d));
        eprintln!("{}", Finding::new(d, &ids, message));
    }
    let not_checked = report.not_checked(&ts, &model);
    if !not_checked.is_empty() {
        eprintln!("{not_checked}");
    }

    let output = match format {
        StatesFormat::Table => render_table(&report.views, &model),
        StatesFormat::Csv => render_csv(&report.views)?,
        StatesFormat::Json => render_json(&report.views, &model, &|site: &Site| {
            format_location(&program, site.span.as_ref()?)
        })?,
    };
    print!("{}", output);

    if report.has_errors() {
        std::process::exit(1);
    }

    Ok(())
}
