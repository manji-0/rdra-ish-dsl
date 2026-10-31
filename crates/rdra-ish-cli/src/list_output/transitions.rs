//! `list --kind transition`: the state-transition table behind the state diagram.

use anyhow::Result;
use rdra_ish_core::analysis::guard_labels;
use rdra_ish_core::{
    actors_for_usecase, state_label, state_models, ResolvedProgram, SemanticModel,
};

use crate::cli::ListFormat;

use super::render::{format_rows, site_cell};

/// One row per transition the state diagram draws, by state model (entity id, then column)
/// and, within one, by the `Enum` order of `from` and `to`, then the use case id. `from` and
/// `to` are `label (value)`; `guards` are the conditions whose `holds` body guards the use
/// case; `actors` are those `actors_for_usecase` gives. `program` turns each transition's
/// site into `file:line`; without it the site column is empty.
pub(crate) fn format_transitions(
    model: &SemanticModel,
    program: Option<&ResolvedProgram>,
    format: &ListFormat,
) -> Result<String> {
    let headers = [
        "entity", "column", "from", "to", "usecase", "guards", "actors", "site",
    ];
    let mut rows: Vec<[String; 8]> = Vec::new();
    for sm in state_models(model) {
        let mut transitions = sm.transitions.clone();
        transitions.sort_by(|a, b| {
            let key = |t: &rdra_ish_core::StateTransition| {
                (
                    sm.position(&t.from),
                    sm.position(&t.to),
                    model.use_cases[t.usecase].id.clone(),
                )
            };
            key(a).cmp(&key(b))
        });
        // One row per (from, to, use case), as the diagram draws one arrow for it.
        transitions.dedup_by(|a, b| (&a.from, &a.to, a.usecase) == (&b.from, &b.to, b.usecase));
        for t in transitions {
            let value =
                |variant: &str| format!("{} ({variant})", state_label(model, sm.entity, variant));
            let usecase = &model.use_cases[t.usecase];
            let actors: Vec<String> = actors_for_usecase(model, t.usecase)
                .into_iter()
                .map(|actor| model.actors[actor].label.clone())
                .collect();
            rows.push([
                sm.entity_id.to_string(),
                sm.column.to_string(),
                value(&t.from),
                value(&t.to),
                format!("{} ({})", usecase.label, usecase.id),
                guard_labels(model, t.usecase).join("|"),
                actors.join("|"),
                site_cell(program, t.site.as_ref()),
            ]);
        }
    }
    format_rows(&headers, &rows, format, "transitions")
}
