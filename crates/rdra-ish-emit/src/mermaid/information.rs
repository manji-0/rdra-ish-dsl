//! The information model (`--kind information`): the entities with the `relate` links of the ER
//! diagram, each entity's state models, and the variations its columns take, without columns.

use super::{omitted_linked_kinds, render_edges, GraphEdge};
use crate::diagram_common::{class_defs, contract_id, desc_line, node_label, quote, Header};
use crate::{object_graph_rel_label, EmitError, Emitter, Scope, View};
use rdra_ish_core::model::{ColumnType, NodeRef, RelKind, SemanticModel};
use std::collections::{BTreeSet, HashSet};

pub struct InformationMermaidEmitter;

fn is_relate(kind: &RelKind) -> bool {
    matches!(
        kind,
        RelKind::RelateOneToOne
            | RelKind::RelateOneToMany
            | RelKind::RelateManyToOne
            | RelKind::RelateManyToMany
    )
}

impl Emitter for InformationMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        // The entities `diagram --kind er` draws for the same scope.
        let reachable = match &view.scope {
            Scope::Bucs(buc_ids) => Some(rdra_ish_core::reachable_from_bucs(model, buc_ids)),
            Scope::Whole | Scope::UseCases(_) => None,
        };
        let visible = |nr: &NodeRef| reachable.as_ref().is_none_or(|set| set.contains(nr));
        let mut entities: Vec<_> = model
            .entities
            .iter()
            .filter(|(key, _)| visible(&NodeRef::Entity(*key)))
            .collect();
        entities.sort_by_key(|(_, entity)| &entity.id);

        let mut body = String::new();
        let mut kinds: Vec<&'static str> = Vec::new();
        let mut declare = |body: &mut String, id: &str, label: &str, kind: &'static str| {
            body.push_str(&format!("  {id}[\"{}\"]:::{kind}\n", quote(label)));
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        };
        let mut edges = Vec::new();
        let mut state_models = Vec::new();
        let mut variations = BTreeSet::new();
        for (key, entity) in &entities {
            let node = NodeRef::Entity(*key);
            let Some(id) = contract_id(model, &node) else {
                continue;
            };
            declare(&mut body, &id, &entity.label, "entity");
            if let Some(desc) = desc_line(model, &node, &id) {
                body.push_str(&format!("  {desc}\n"));
            }
            for column in &entity.columns {
                if model.is_lifecycle_column(*key, &column.name) {
                    let state_model = format!("statemodel__{}_{}", entity.id, column.name);
                    edges.push(GraphEdge {
                        from: id.clone(),
                        label: "statemodel".to_string(),
                        to: state_model.clone(),
                        arrow: "-.->",
                        derived: None,
                    });
                    state_models.push((state_model, format!("{}.{}", entity.id, column.name)));
                }
                if let ColumnType::Variation(key) = column.col_type {
                    let Some(to) = contract_id(model, &NodeRef::Variation(key)) else {
                        continue;
                    };
                    edges.push(GraphEdge {
                        from: id.clone(),
                        label: format!("variation {}", column.name),
                        to: to.clone(),
                        arrow: "-.->",
                        derived: None,
                    });
                    variations.insert((to, key));
                }
            }
        }
        state_models.sort();
        for (id, label) in &state_models {
            declare(&mut body, id, label, "statemodel");
        }
        for (id, key) in &variations {
            let variation = &NodeRef::Variation(*key);
            if let Some(label) = node_label(model, variation) {
                declare(&mut body, id, label, "variation");
                if let Some(desc) = desc_line(model, variation, id) {
                    body.push_str(&format!("  {desc}\n"));
                }
            }
        }

        for rel in model.relations.iter().filter(|rel| is_relate(&rel.kind)) {
            if !visible(&rel.from) || !visible(&rel.to) {
                continue;
            }
            if let (Some(from), Some(to)) =
                (contract_id(model, &rel.from), contract_id(model, &rel.to))
            {
                edges.push(GraphEdge {
                    from,
                    label: object_graph_rel_label(&rel.kind).to_string(),
                    to,
                    arrow: "---",
                    derived: None,
                });
            }
        }
        render_edges(&mut body, edges);
        body.push_str(&class_defs(&kinds));

        let drawn: HashSet<NodeRef> = entities
            .iter()
            .map(|(key, _)| NodeRef::Entity(*key))
            .chain(
                variations
                    .into_iter()
                    .map(|(_, key)| NodeRef::Variation(key)),
            )
            .collect();
        let omitted = std::iter::once("columns".to_string())
            .chain(omitted_linked_kinds(
                model,
                |nr| matches!(nr, NodeRef::Entity(_)) && drawn.contains(nr),
                &drawn,
                &kinds,
            ))
            .collect();
        let header = Header {
            diagram: "information",
            view,
            layers: None,
            kinds,
            omitted,
        };
        Ok(format!("{}flowchart LR\n{body}", header.render()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram_common::model_from;

    const SHOP: &str = r#"
variation Method "Method" { card "Card"  cash "Cash" }
buc BucPay "Pay"
usecase Capture "Capture"
entity Customer "Customer" { id: Int @pk }
entity Order "Order" {
  id: Int @pk
  status: Enum(pending, paid) @default(pending)
  channel: Enum(web, store)
}
entity Payment "Payment" {
  id: Int @pk
  method: Method
}
entity Audit "Audit" { id: Int @pk }
relate(Order, Customer, N:1)
relate(Payment, Order, 1:1)
contains(BucPay, Capture)
updates(Capture, Payment)
transitions(Order.status, Capture, pending -> paid)
"#;

    fn emit(view: View) -> String {
        InformationMermaidEmitter
            .emit(&model_from(SHOP), &view)
            .unwrap()
    }

    #[test]
    fn only_a_column_with_transitions_is_a_state_model() {
        let result = emit(View::whole());
        assert!(
            result.contains("  entity__Order -.->|statemodel| statemodel__Order_status\n"),
            "{result}"
        );
        assert!(result.contains("  statemodel__Order_status[\"Order.status\"]:::statemodel\n"));
        assert!(!result.contains("Order_channel"), "{result}");
    }

    #[test]
    fn a_variation_edge_names_its_column() {
        let result = emit(View::whole());
        assert!(
            result.contains("  entity__Payment -.->|variation method| variation__Method\n"),
            "{result}"
        );
        assert!(result.contains("  variation__Method[\"Method\"]:::variation\n"));
    }

    #[test]
    fn relates_keep_their_left_end_and_cardinality_and_columns_are_omitted() {
        let result = emit(View::whole());
        assert!(result.contains("  entity__Order ---|N:1| entity__Customer\n"));
        assert!(result.contains("  entity__Payment ---|1:1| entity__Order\n"));
        assert!(
            !result.contains("method:") && !result.contains("Int "),
            "{result}"
        );
        assert!(
            result.starts_with(
                "%% rdra-ish diagram=information scope=whole\n%% kinds: entity statemodel variation\n%% omitted: columns usecase\nflowchart LR\n"
            ),
            "{result}"
        );
    }

    #[test]
    fn a_buc_scope_draws_the_entities_its_use_cases_reach() {
        let result = emit(View::bucs(vec!["BucPay".to_string()]));
        assert!(result.contains("entity__Payment["), "{result}");
        assert!(!result.contains("entity__Audit"), "{result}");
    }
}
