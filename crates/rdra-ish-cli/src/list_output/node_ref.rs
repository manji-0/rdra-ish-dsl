//! NodeRef display helpers for list output.

use rdra_ish_core::model::{NodeRef, RelKind};

pub(crate) fn adr_targets(
    model: &rdra_ish_core::SemanticModel,
    adr: rdra_ish_core::model::AdrKey,
) -> Vec<NodeRef> {
    let mut targets: Vec<_> = model
        .relations
        .iter()
        .filter(|relation| relation.kind == RelKind::Decides && relation.from == NodeRef::Adr(adr))
        .map(|relation| relation.to.clone())
        .collect();
    targets.sort_by_key(|target| {
        (
            target.kind_tag().to_string(),
            model.node_id(target).unwrap_or_default().to_string(),
        )
    });
    targets
}
