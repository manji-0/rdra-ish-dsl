//! `list --kind buc-roles`: who takes part in which BUC.

use anyhow::Result;
use rdra_ish_core::model::{BucKey, NodeRef, RelKind};
use rdra_ish_core::{buc_participants, Involvement, SemanticModel};

use crate::cli::ListFormat;

use super::render::format_table;

/// The BUCs in the order the table has them as columns: by business (sorted by id), each
/// BUC under the first business it belongs to, by id; BUCs of no business last.
fn bucs_by_business(model: &SemanticModel) -> Vec<BucKey> {
    let mut bucs: Vec<BucKey> = model.bucs.keys().collect();
    bucs.sort_by_key(|buc| model.bucs[*buc].id.clone());
    let first_business = |buc: BucKey| -> Option<String> {
        model
            .relations
            .iter()
            .filter(|rel| rel.kind == RelKind::Belongs && rel.from == NodeRef::Buc(buc))
            .filter_map(|rel| match rel.to {
                NodeRef::Business(business) => Some(model.businesses[business].id.clone()),
                _ => None,
            })
            .min()
    };
    // `sort_by_key` is stable, so the BUCs of one business stay sorted by id.
    bucs.sort_by_key(|buc| match first_business(*buc) {
        Some(id) => (0, id),
        None => (1, String::new()),
    });
    bucs
}

fn involvement_cell(involvements: &std::collections::BTreeSet<Involvement>) -> String {
    involvements
        .iter()
        .map(|involvement| match involvement {
            Involvement::Performs => "担当",
            Involvement::SendsEvent => "イベント送信",
            Involvement::ReceivesEvent => "イベント受信",
        })
        .collect::<Vec<_>>()
        .join("、")
}

/// One row per actor, then per external system (each sorted by id), and one column per BUC.
/// A cell says how the row takes part in the BUC: `担当` for an actor that performs the BUC,
/// a use case in it, or a step in it; `イベント送信` for an external system whose event starts
/// the BUC or one of its use cases; `イベント受信` for one that receives an event a use case in
/// the BUC raises. An empty cell is no part.
pub(crate) fn format_buc_roles(model: &SemanticModel, format: &ListFormat) -> Result<String> {
    let bucs = bucs_by_business(model);
    let participants: Vec<_> = bucs
        .iter()
        .map(|buc| buc_participants(model, *buc))
        .collect();

    let mut roles: Vec<(NodeRef, &str, &str, &str)> = Vec::new();
    for (key, actor) in &model.actors {
        roles.push((NodeRef::Actor(key), &actor.id, &actor.label, "actor"));
    }
    roles.sort_by_key(|(_, id, _, _)| *id);
    let mut external: Vec<(NodeRef, &str, &str, &str)> = model
        .ext_systems
        .iter()
        .map(|(key, ext)| {
            (
                NodeRef::ExtSystem(key),
                ext.id.as_str(),
                ext.label.as_str(),
                "extsystem",
            )
        })
        .collect();
    external.sort_by_key(|(_, id, _, _)| *id);
    roles.extend(external);

    let headers: Vec<String> = ["role", "kind"]
        .into_iter()
        .map(str::to_string)
        .chain(bucs.iter().map(|buc| model.bucs[*buc].label.clone()))
        .collect();
    let rows: Vec<Vec<String>> = roles
        .iter()
        .map(|(node, _, label, kind)| {
            let cells = participants.iter().map(|in_buc| {
                in_buc
                    .iter()
                    .find(|(seen, _)| seen == node)
                    .map(|(_, set)| involvement_cell(set))
                    .unwrap_or_default()
            });
            [label.to_string(), kind.to_string()]
                .into_iter()
                .chain(cells)
                .collect()
        })
        .collect();
    format_table(&headers, &rows, format, "actors or external systems")
}
