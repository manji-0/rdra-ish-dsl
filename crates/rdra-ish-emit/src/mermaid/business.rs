//! The business context and business use case diagram (`--kind business`): each business with
//! its BUCs, and the actors and external systems each BUC involves.

use super::{buc_members, declare, kinds_by_layer, omitted_linked_kinds, render_edges, EdgeSet};
use crate::diagram_common::{
    class_defs, contract_id, desc_line, node_id, node_label, quote, Header,
};
use crate::{collect_object_graph_nodes, EmitError, Emitter, ObjectGraphLayer, Scope, View};
use rdra_ish_core::model::{NodeRef, RelKind, SemanticModel};
use std::collections::HashSet;

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

/// The edges between `buc` and its actors and external systems: `performs(Actor, Buc)` as
/// declared, and the rest derived through the BUC's use cases, steps, and events.
fn buc_edges(model: &SemanticModel, buc: &NodeRef, edges: &mut EdgeSet) {
    let (usecases, steps) = buc_members(model, std::slice::from_ref(buc));
    let triggers = |event: &NodeRef, target: &NodeRef| {
        model
            .relations
            .iter()
            .any(|rel| rel.kind == RelKind::Triggers && rel.from == *event && rel.to == *target)
    };
    let event_id = |event: &NodeRef| node_id(model, event).unwrap_or_default().to_string();
    for rel in &model.relations {
        match (&rel.kind, &rel.from, &rel.to) {
            (RelKind::Performs, actor @ NodeRef::Actor(_), target) => {
                let via = (target != buc).then_some(target);
                if target == buc || usecases.contains(target) || steps.contains(target) {
                    edges.add(model, (actor, &rel.kind, "performs".into(), buc), via);
                }
            }
            // An external system raises an event that starts the BUC or one of its use cases.
            (RelKind::Raises, extsystem @ NodeRef::ExtSystem(_), event) => {
                let label = format!("raises {}", event_id(event));
                if triggers(event, buc) {
                    edges.add(
                        model,
                        (extsystem, &rel.kind, label.clone(), buc),
                        Some(event),
                    );
                }
                for usecase in usecases.iter().filter(|uc| triggers(event, uc)) {
                    edges.add(
                        model,
                        (extsystem, &rel.kind, label.clone(), buc),
                        Some(usecase),
                    );
                }
            }
            // A use case of the BUC raises an event delivered to an external system.
            (RelKind::Raises, usecase, event) if usecases.contains(usecase) => {
                for target in &model.relations {
                    if target.kind == RelKind::Triggers
                        && target.from == *event
                        && matches!(target.to, NodeRef::ExtSystem(_))
                    {
                        let label = format!("triggers {}", event_id(event));
                        edges.add(model, (buc, &target.kind, label, &target.to), Some(usecase));
                    }
                }
            }
            _ => {}
        }
    }
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
        let bucs = scoped_bucs(model, &view.scope);
        let mut edges = EdgeSet::default();
        // A BUC that belongs to one business is drawn inside it; one that belongs to several
        // is drawn beside them with its `belongs` edges.
        let mut nested: Vec<(NodeRef, NodeRef)> = Vec::new();
        for buc in &bucs {
            buc_edges(model, buc, &mut edges);
            match businesses_of(model, buc).as_slice() {
                [business] => nested.push((business.clone(), buc.clone())),
                businesses => {
                    for business in businesses {
                        edges.add(
                            model,
                            (buc, &RelKind::Belongs, "belongs".into(), business),
                            None,
                        );
                    }
                }
            }
        }
        let mut drawn: HashSet<NodeRef> = bucs.iter().cloned().collect();
        drawn.extend(nested.iter().map(|(business, _)| business.clone()));
        drawn.extend(edges.nodes.iter().cloned());
        let nodes = collect_object_graph_nodes(model, &|nr| drawn.contains(nr));

        let mut body = String::new();
        let mut layers = Vec::new();
        let value: Vec<&NodeRef> = nodes
            .iter()
            .filter(|nr| matches!(nr, NodeRef::Actor(_) | NodeRef::ExtSystem(_)))
            .collect();
        if !value.is_empty() {
            layers.push(ObjectGraphLayer::SystemValue);
            body.push_str(&format!(
                "  subgraph layer_{}[\"{}\"]\n",
                ObjectGraphLayer::SystemValue.token(),
                ObjectGraphLayer::SystemValue.label()
            ));
            for node in value {
                declare(&mut body, model, node, "    ");
            }
            body.push_str("  end\n");
        }
        if !bucs.is_empty() {
            let environment = ObjectGraphLayer::ExternalEnvironment;
            layers.push(environment);
            body.push_str(&format!(
                "  subgraph layer_{}[\"{}\"]\n",
                environment.token(),
                environment.label()
            ));
            for business in nodes.iter().filter(|nr| matches!(nr, NodeRef::Business(_))) {
                let (Some(id), Some(label)) =
                    (contract_id(model, business), node_label(model, business))
                else {
                    continue;
                };
                body.push_str(&format!("    subgraph {id}[\"{}\"]\n", quote(label)));
                if let Some(desc) = desc_line(model, business, &id) {
                    body.push_str(&format!("      {desc}\n"));
                }
                for (_, buc) in nested.iter().filter(|(b, _)| b == business) {
                    declare(&mut body, model, buc, "      ");
                }
                body.push_str("    end\n");
            }
            for buc in bucs
                .iter()
                .filter(|buc| !nested.iter().any(|(_, b)| b == *buc))
            {
                declare(&mut body, model, buc, "    ");
            }
            body.push_str("  end\n");
        }
        render_edges(&mut body, edges.into_graph_edges());

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
            layers: Some(layers),
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
actor Courier "Courier"
extsystem Gateway "Gateway"
extsystem Carrier "Carrier"
business Shop "Shop"
business Logistics "Logistics"
buc BucPay "Pay"
buc BucShip "Ship"
buc BucLoose "Loose"
flow ShipFlow "Ship flow"
step Handover "Handover"
usecase Capture "Capture"
usecase Dispatch "Dispatch"
event Authorized "Authorized"
event Shipped "Shipped"
belongs(BucPay, Shop)
belongs(BucShip, Shop)
belongs(BucShip, Logistics)
performs(Customer, BucPay)
performs(Clerk, Capture)
performs(Courier, Handover)
contains(BucPay, Capture)
contains(BucShip, Dispatch)
contains(BucShip, ShipFlow)
contains(ShipFlow, Handover)
covers(Handover, Dispatch)
raises(Gateway, Authorized)
triggers(Authorized, Capture)
raises(Dispatch, Shipped)
triggers(Shipped, Carrier)
"#;

    fn emit(view: View) -> String {
        BusinessMermaidEmitter
            .emit(&model_from(SHOP), &view)
            .unwrap()
    }

    #[test]
    fn an_actor_reaches_a_buc_directly_through_a_use_case_or_through_a_step() {
        let result = emit(View::whole());
        assert!(
            result.contains("  actor__Customer -.->|performs| buc__BucPay\n  buc__BucShip"),
            "{result}"
        );
        assert!(
            result.contains(
                "  actor__Clerk -.->|performs| buc__BucPay\n  %% derived actor__Clerk -> buc__BucPay via usecase__Capture\n"
            ),
            "{result}"
        );
        assert!(
            result.contains(
                "  actor__Courier -.->|performs| buc__BucShip\n  %% derived actor__Courier -> buc__BucShip via step__Handover\n"
            ),
            "{result}"
        );
    }

    #[test]
    fn external_systems_meet_a_buc_through_its_use_cases_events() {
        let result = emit(View::whole());
        assert!(
            result.contains(
                "  extsystem__Gateway -.->|raises Authorized| buc__BucPay\n  %% derived extsystem__Gateway -> buc__BucPay via usecase__Capture\n"
            ),
            "{result}"
        );
        assert!(
            result.contains(
                "  buc__BucShip -.->|triggers Shipped| extsystem__Carrier\n  %% derived buc__BucShip -> extsystem__Carrier via usecase__Dispatch\n"
            ),
            "{result}"
        );
    }

    #[test]
    fn a_buc_sits_in_its_one_business_and_beside_several() {
        let result = emit(View::whole());
        assert!(
            result.contains(
                "    subgraph business__Shop[\"Shop\"]\n      buc__BucPay[\"Pay\"]:::buc\n    end\n"
            ),
            "{result}"
        );
        assert!(result.contains("    subgraph business__Logistics[\"Logistics\"]\n    end\n"));
        assert!(result.contains("    buc__BucLoose[\"Loose\"]:::buc\n"));
        assert!(result.contains("  buc__BucShip -->|belongs| business__Logistics\n"));
        assert!(result.contains("  buc__BucShip -->|belongs| business__Shop\n"));
        assert!(
            result.contains("%% kinds: actor extsystem business buc\n"),
            "{result}"
        );
    }

    #[test]
    fn a_buc_scope_draws_the_named_bucs_and_their_businesses() {
        let result = emit(View::bucs(vec!["BucPay".to_string()]));
        assert!(result.starts_with("%% rdra-ish diagram=business scope=buc:BucPay\n"));
        assert!(result.contains("subgraph business__Shop["), "{result}");
        for absent in ["buc__BucShip", "business__Logistics", "actor__Courier"] {
            assert!(!result.contains(absent), "{absent}: {result}");
        }
        assert!(result.contains("%% omitted: usecase event\n"), "{result}");
    }
}
