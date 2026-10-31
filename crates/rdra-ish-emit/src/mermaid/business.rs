//! The business flow (`--kind business`): each business with its BUCs, and an edge for each
//! hand-off, a BUC whose use case raises an event that starts another BUC. Who takes part in
//! each BUC is a table (`list --kind buc-roles`), not part of the diagram.

use super::{declare, kinds_by_layer, omitted_linked_kinds, render_edges, GraphEdge};
use crate::diagram_common::{
    class_defs, contract_id, desc_line, edge_label, node_id, node_label, quote, Header,
};
use crate::{collect_object_graph_nodes, EmitError, Emitter, Scope, View};
use rdra_ish_core::model::{NodeRef, RelKind, SemanticModel};
use rdra_ish_core::{buc_handoffs, buc_members};
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub struct BusinessMermaidEmitter;

/// The BUCs of the scope, sorted by id: every one, or the named ones.
fn scoped_bucs(model: &SemanticModel, scope: &Scope) -> Vec<NodeRef> {
    collect_object_graph_nodes(model, &|node| {
        matches!(node, NodeRef::Buc(_))
            && match scope {
                Scope::Bucs(ids) => {
                    node_id(model, node).is_some_and(|id| ids.iter().any(|w| w == id))
                }
                Scope::Whole | Scope::UseCases(_) => true,
            }
    })
}

/// The businesses a BUC belongs to.
fn businesses_of(model: &SemanticModel, buc: &NodeRef) -> Vec<NodeRef> {
    model
        .relations
        .iter()
        .filter(|rel| rel.kind == RelKind::Belongs && rel.from == *buc)
        .map(|rel| rel.to.clone())
        .collect()
}

impl Emitter for BusinessMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let named = scoped_bucs(model, &view.scope);
        let handoffs: Vec<_> = buc_handoffs(model)
            .into_iter()
            .filter(|h| {
                matches!(view.scope, Scope::Whole | Scope::UseCases(_))
                    || named.contains(&NodeRef::Buc(h.from))
                    || named.contains(&NodeRef::Buc(h.to))
            })
            .collect();
        // A scope draws the named BUCs and the BUCs they hand off to or from.
        let neighbours: Vec<NodeRef> = handoffs
            .iter()
            .flat_map(|h| [NodeRef::Buc(h.from), NodeRef::Buc(h.to)])
            .collect();
        let bucs: Vec<NodeRef> = collect_object_graph_nodes(model, &|nr| {
            named.contains(nr) || (matches!(view.scope, Scope::Bucs(_)) && neighbours.contains(nr))
        });

        // A BUC sits in the first business (by id) it belongs to; each other one is a
        // `belongs` edge to that business.
        let mut nested: Vec<(NodeRef, NodeRef)> = Vec::new();
        let mut belongs: Vec<(NodeRef, NodeRef)> = Vec::new();
        for buc in &bucs {
            let mut businesses = businesses_of(model, buc);
            businesses.sort_by_key(|b| node_id(model, b).unwrap_or_default().to_string());
            let mut businesses = businesses.into_iter();
            if let Some(first) = businesses.next() {
                nested.push((first, buc.clone()));
            }
            belongs.extend(businesses.map(|business| (buc.clone(), business)));
        }
        let mut drawn: HashSet<NodeRef> = bucs.iter().cloned().collect();
        drawn.extend(nested.iter().map(|(business, _)| business.clone()));
        drawn.extend(belongs.iter().map(|(_, business)| business.clone()));
        let nodes = collect_object_graph_nodes(model, &|nr| drawn.contains(nr));

        let mut body = String::new();
        for business in nodes.iter().filter(|nr| matches!(nr, NodeRef::Business(_))) {
            let (Some(id), Some(label)) =
                (contract_id(model, business), node_label(model, business))
            else {
                continue;
            };
            body.push_str(&format!("  subgraph {id}[\"{}\"]\n", quote(label)));
            if let Some(desc) = desc_line(model, business, &id) {
                body.push_str(&format!("    {desc}\n"));
            }
            for (_, buc) in nested.iter().filter(|(b, _)| b == business) {
                declare(&mut body, model, buc, "    ");
            }
            body.push_str("  end\n");
        }
        for buc in bucs
            .iter()
            .filter(|buc| !nested.iter().any(|(_, b)| b == *buc))
        {
            declare(&mut body, model, buc, "  ");
        }

        // `(from, label, to)` to the events the hand-off passes.
        let mut flow: BTreeMap<(String, String, String), BTreeSet<String>> = BTreeMap::new();
        for h in &handoffs {
            let (from, to, event) = (
                NodeRef::Buc(h.from),
                NodeRef::Buc(h.to),
                NodeRef::Event(h.event),
            );
            if let (Some(from), Some(to), Some(via), Some(label)) = (
                contract_id(model, &from),
                contract_id(model, &to),
                contract_id(model, &event),
                node_label(model, &event),
            ) {
                flow.entry((from, edge_label(label), to))
                    .or_default()
                    .insert(via);
            }
        }
        let mut edges: Vec<GraphEdge> = flow
            .into_iter()
            .map(|((from, label, to), vias)| GraphEdge {
                from,
                label,
                to,
                arrow: "-->",
                derived: Some(format!(
                    "via {}",
                    vias.into_iter().collect::<Vec<_>>().join(",")
                )),
            })
            .collect();
        for (buc, business) in &belongs {
            if let (Some(from), Some(to)) = (contract_id(model, buc), contract_id(model, business))
            {
                edges.push(GraphEdge {
                    from,
                    label: "belongs".to_string(),
                    to,
                    arrow: "-->",
                    derived: None,
                });
            }
        }
        render_edges(&mut body, edges);

        let kinds = kinds_by_layer(&nodes);
        body.push_str(&class_defs(&kinds));
        let anchors: HashSet<NodeRef> = bucs
            .iter()
            .flat_map(|buc| {
                let (usecases, _) = buc_members(model, std::slice::from_ref(buc));
                usecases.into_iter().chain([buc.clone()])
            })
            .collect();
        let header = Header {
            diagram: "business",
            view,
            layers: None,
            omitted: omitted_linked_kinds(model, |nr| anchors.contains(nr), &drawn, &kinds),
            kinds,
        };
        Ok(format!("{}flowchart LR\n{body}", header.render()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram_common::model_from;

    const SHOP: &str = r#"
actor Customer "Customer"
actor Clerk "Clerk"
extsystem Gateway "Gateway"
extsystem Carrier "Carrier"
business Shop "Shop"
business Logistics "Logistics"
buc BucPay "Pay"
buc BucShip "Ship"
buc BucWrap "Wrap"
buc BucLoose "Loose"
buc BucBook "Book"
usecase Capture "Capture"
usecase Refund "Refund"
usecase Dispatch "Dispatch"
usecase Reserve "Reserve"
event Authorized "Authorized"
event Paid "Paid | settled"
event Shipped "Shipped"
event Wrapped "Wrapped"
belongs(BucPay, Shop)
belongs(BucShip, Shop)
belongs(BucShip, Logistics)
belongs(BucWrap, Logistics)
performs(Customer, BucPay)
contains(BucPay, Capture)
contains(BucPay, Refund)
contains(BucShip, Dispatch)
contains(BucBook, Reserve)
raises(Gateway, Authorized)
triggers(Authorized, Capture)
raises(Capture, Paid)
raises(Refund, Paid)
triggers(Paid, Dispatch)
raises(Dispatch, Shipped)
triggers(Shipped, Carrier)
raises(Dispatch, Wrapped)
triggers(Wrapped, BucWrap)
"#;

    fn emit(view: View) -> String {
        BusinessMermaidEmitter
            .emit(&model_from(SHOP), &view)
            .unwrap()
    }

    #[test]
    fn a_hand_off_is_an_edge_labelled_with_the_event_and_drawn_once() {
        let result = emit(View::whole());
        assert!(
            result.contains(
                "  buc__BucPay -->|Paid #124; settled| buc__BucShip\n  %% derived buc__BucPay -> buc__BucShip via event__Paid\n"
            ),
            "{result}"
        );
        // Raised by two use cases of Pay, still one edge.
        assert_eq!(result.matches("-->|Paid").count(), 1, "{result}");
        // An event that triggers the BUC itself is a hand-off too.
        assert!(
            result.contains("  buc__BucShip -->|Wrapped| buc__BucWrap\n"),
            "{result}"
        );
    }

    #[test]
    fn an_event_to_an_external_system_and_a_handoff_without_an_event_draw_no_edge() {
        let result = emit(View::whole());
        for absent in ["extsystem__", "actor__", "Shipped", "Authorized"] {
            assert!(!result.contains(absent), "{absent}: {result}");
        }
        assert!(!result.contains("buc__BucBook -->"), "{result}");
        assert!(!result.contains("--> buc__BucBook"), "{result}");
    }

    #[test]
    fn a_buc_sits_in_its_first_business_by_id_and_has_a_belongs_edge_to_the_others() {
        let result = emit(View::whole());
        assert!(
            result.contains(
                "  subgraph business__Logistics[\"Logistics\"]\n    buc__BucShip[\"Ship\"]:::buc\n    buc__BucWrap[\"Wrap\"]:::buc\n  end\n"
            ),
            "{result}"
        );
        assert!(
            result.contains(
                "  subgraph business__Shop[\"Shop\"]\n    buc__BucPay[\"Pay\"]:::buc\n  end\n"
            ),
            "{result}"
        );
        assert!(result.contains("  buc__BucShip -->|belongs| business__Shop\n"));
        assert!(result.contains("  buc__BucLoose[\"Loose\"]:::buc\n"));
        assert!(!result.contains("subgraph layer_"), "{result}");
        assert!(
            result.starts_with(
                "%% rdra-ish diagram=business scope=whole\n%% kinds: business buc\n%% omitted: actor usecase event\n"
            ),
            "{result}"
        );
    }

    #[test]
    fn a_buc_scope_draws_the_named_bucs_and_the_bucs_they_hand_off_with() {
        let result = emit(View::bucs(vec!["BucPay".to_string()]));
        assert!(result.starts_with("%% rdra-ish diagram=business scope=buc:BucPay\n"));
        assert!(
            result.contains("  buc__BucPay -->|Paid #124; settled| buc__BucShip\n"),
            "{result}"
        );
        assert!(result.contains("subgraph business__Shop["), "{result}");
        for absent in ["buc__BucWrap", "buc__BucLoose", "-->|Wrapped|"] {
            assert!(!result.contains(absent), "{absent}: {result}");
        }
    }
}
