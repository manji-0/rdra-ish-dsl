//! The system context (`--kind context`): the system at the centre, the actors who use it, and
//! the external systems it exchanges events with. Every edge is derived.

use super::{
    buc_members, buc_scope, declare, kinds_by_layer, omitted_linked_kinds, render_edges, GraphEdge,
};
use crate::diagram_common::{class_defs, contract_id, edge_label, node_label, Header};
use crate::{
    collect_object_graph_nodes, object_graph_layer, EmitError, Emitter, ObjectGraphLayer, Scope,
    View,
};
use rdra_ish_core::model::{NodeRef, RelKind, SemanticModel};
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub struct ContextMermaidEmitter;

/// The node that stands for the system when no declared system can: none is declared, or
/// several are and the use cases behind an edge invoke no API one of them contains.
const SYNTHESIZED: &str = "system__Model";

/// `to` of every `kind` relation from `from`.
fn targets(model: &SemanticModel, from: &NodeRef, kind: RelKind) -> Vec<NodeRef> {
    model
        .relations
        .iter()
        .filter(|rel| rel.kind == kind && rel.from == *from)
        .map(|rel| rel.to.clone())
        .collect()
}

/// The use cases behind a node an actor performs or an event triggers: the use case itself,
/// the use cases of a BUC, or the use cases a step covers.
fn usecases_of(model: &SemanticModel, node: &NodeRef) -> Vec<NodeRef> {
    match node {
        NodeRef::UseCase(_) => vec![node.clone()],
        NodeRef::Buc(_) => buc_members(model, std::slice::from_ref(node)).0,
        NodeRef::Step(_) => targets(model, node, RelKind::Covers)
            .into_iter()
            .filter(|to| matches!(to, NodeRef::UseCase(_)))
            .collect(),
        _ => Vec::new(),
    }
}

/// The system-side node ids an edge through `usecases` ends at. With one declared system it
/// is that system; with several, each that contains an API one of the use cases invokes.
/// Otherwise the synthesized node.
fn system_ends(
    model: &SemanticModel,
    systems: &[NodeRef],
    usecases: &[NodeRef],
) -> BTreeSet<String> {
    let mut ends: BTreeSet<String> = match systems {
        [system] => contract_id(model, system).into_iter().collect(),
        [] => BTreeSet::new(),
        _ => {
            let apis: Vec<NodeRef> = usecases
                .iter()
                .flat_map(|usecase| targets(model, usecase, RelKind::Invokes))
                .collect();
            systems
                .iter()
                .filter(|system| {
                    targets(model, system, RelKind::Contains)
                        .iter()
                        .any(|api| apis.contains(api))
                })
                .filter_map(|system| contract_id(model, system))
                .collect()
        }
    };
    if ends.is_empty() {
        ends.insert(SYNTHESIZED.to_string());
    }
    ends
}

/// The edges of the diagram, one per ordered pair of ends, each with the elements it was
/// derived through (id and label, in model order).
#[derive(Default)]
struct Exchanges(BTreeMap<(String, String), Vec<(String, String)>>);

impl Exchanges {
    fn add(&mut self, from: String, to: String, via: (String, String)) {
        let entry = self.0.entry((from, to)).or_default();
        if !entry.iter().any(|(id, _)| *id == via.0) {
            entry.push(via);
        }
    }

    fn ends(&self) -> HashSet<&str> {
        self.0
            .keys()
            .flat_map(|(from, to)| [from.as_str(), to.as_str()])
            .collect()
    }
}

/// The BUCs a node an actor performs belongs to: the BUC itself, or each BUC containing the
/// use case or step. Empty for a node outside every BUC.
fn bucs_of(model: &SemanticModel, node: &NodeRef) -> Vec<NodeRef> {
    if matches!(node, NodeRef::Buc(_)) {
        return vec![node.clone()];
    }
    model
        .bucs
        .keys()
        .map(NodeRef::Buc)
        .filter(|buc| {
            let (usecases, steps) = buc_members(model, std::slice::from_ref(buc));
            usecases.contains(node) || steps.contains(node)
        })
        .collect()
}

impl Emitter for ContextMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let scope = match &view.scope {
            Scope::Bucs(ids) => Some(buc_scope(model, ids)),
            Scope::Whole | Scope::UseCases(_) => None,
        };
        let in_scope = |nr: &NodeRef| scope.as_ref().is_none_or(|set| set.contains(nr));
        let systems = collect_object_graph_nodes(model, &|nr| matches!(nr, NodeRef::System(_)));
        let via_of = |node: &NodeRef| -> Option<(String, String)> {
            Some((
                contract_id(model, node)?,
                node_label(model, node)?.to_string(),
            ))
        };

        // Actors to the system with the BUCs they perform, external systems to it with the
        // events they raise, and from it with the events it raises to them. The system end is
        // a (to, from) pair of ids per edge, found through the use cases behind the edge.
        let mut exchanges = Exchanges::default();
        let mut linked: HashSet<NodeRef> = HashSet::new();
        for rel in &model.relations {
            // The outside node, what labels the edge, the use cases behind it, and whether the
            // edge runs from the system.
            let (outside, vias, usecases, outbound) = match (&rel.kind, &rel.from, &rel.to) {
                (RelKind::Performs, actor @ NodeRef::Actor(_), target) if in_scope(target) => {
                    let bucs: Vec<NodeRef> = bucs_of(model, target)
                        .into_iter()
                        .filter(|buc| in_scope(buc))
                        .collect();
                    let vias: Vec<(String, String)> = if bucs.is_empty() {
                        via_of(target).into_iter().collect()
                    } else {
                        bucs.iter().filter_map(&via_of).collect()
                    };
                    (actor, vias, usecases_of(model, target), false)
                }
                (RelKind::Raises, extsystem @ NodeRef::ExtSystem(_), event) => {
                    let usecases: Vec<NodeRef> = targets(model, event, RelKind::Triggers)
                        .iter()
                        .flat_map(|target| usecases_of(model, target))
                        .filter(|usecase| in_scope(usecase))
                        .collect();
                    if usecases.is_empty() {
                        continue;
                    }
                    (
                        extsystem,
                        via_of(event).into_iter().collect(),
                        usecases,
                        false,
                    )
                }
                (RelKind::Triggers, event, extsystem @ NodeRef::ExtSystem(_)) => {
                    let usecases: Vec<NodeRef> = model
                        .relations
                        .iter()
                        .filter(|r| {
                            r.kind == RelKind::Raises
                                && r.to == *event
                                && matches!(r.from, NodeRef::UseCase(_))
                                && in_scope(&r.from)
                        })
                        .map(|r| r.from.clone())
                        .collect();
                    if usecases.is_empty() {
                        continue;
                    }
                    (
                        extsystem,
                        via_of(event).into_iter().collect(),
                        usecases,
                        true,
                    )
                }
                _ => continue,
            };
            let Some(outside_id) = contract_id(model, outside) else {
                continue;
            };
            if vias.is_empty() {
                continue;
            }
            linked.insert(outside.clone());
            for end in system_ends(model, &systems, &usecases) {
                for via in &vias {
                    let (from, to) = if outbound {
                        (end.clone(), outside_id.clone())
                    } else {
                        (outside_id.clone(), end.clone())
                    };
                    exchanges.add(from, to, via.clone());
                }
            }
        }

        let synthesized = systems.is_empty()
            || exchanges
                .0
                .keys()
                .any(|(from, to)| from == SYNTHESIZED || to == SYNTHESIZED);
        let ends = exchanges.ends();
        let nodes = collect_object_graph_nodes(model, &|nr| match nr {
            NodeRef::Actor(_) | NodeRef::ExtSystem(_) => scope.is_none() || linked.contains(nr),
            NodeRef::System(_) => {
                scope.is_none()
                    || contract_id(model, nr).is_some_and(|id| ends.contains(id.as_str()))
            }
            _ => false,
        });

        let mut body = String::new();
        for layer in [ObjectGraphLayer::SystemValue, ObjectGraphLayer::System] {
            for node in nodes.iter().filter(|nr| object_graph_layer(nr) == layer) {
                declare(&mut body, model, node, "  ");
            }
        }
        if synthesized {
            let reason = if systems.is_empty() {
                "no system is declared"
            } else {
                "no declared system contains an API the use cases behind its edges invoke"
            };
            body.push_str(&format!(
                "  {SYNTHESIZED}[\"システム\"]:::system\n  %% synthesized {SYNTHESIZED}: {reason}\n"
            ));
        }
        render_edges(
            &mut body,
            exchanges
                .0
                .into_iter()
                .map(|((from, to), vias)| GraphEdge {
                    from,
                    label: vias
                        .iter()
                        .map(|(_, label)| edge_label(label))
                        .collect::<Vec<_>>()
                        .join("、"),
                    to,
                    arrow: "-.->",
                    derived: Some(format!(
                        "via {}",
                        vias.into_iter()
                            .map(|(id, _)| id)
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect::<Vec<_>>()
                            .join(",")
                    )),
                })
                .collect(),
        );

        let mut kinds = kinds_by_layer(&nodes);
        if synthesized && !kinds.contains(&"system") {
            kinds.push("system");
        }
        body.push_str(&class_defs(&kinds));
        let drawn: HashSet<NodeRef> = nodes.iter().cloned().collect();
        let header = Header {
            diagram: "context",
            view,
            layers: None,
            omitted: omitted_linked_kinds(
                model,
                |nr| matches!(nr, NodeRef::Actor(_) | NodeRef::ExtSystem(_)) && drawn.contains(nr),
                &drawn,
                &kinds,
            ),
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
actor Idle "Idle"
extsystem Gateway "Gateway"
extsystem Carrier "Carrier"
buc BucPay "Pay"
buc BucShip "Ship"
flow ShipFlow "Ship flow"
step Handover "Handover"
usecase Capture "Capture"
usecase Refund "Refund"
usecase Dispatch "Dispatch"
event Authorized "Authorized"
event Shipped "Shipped"
performs(Customer, BucPay)
performs(Clerk, Refund)
performs(Courier, Handover)
contains(BucPay, Capture)
contains(BucPay, Refund)
contains(BucShip, Dispatch)
contains(BucShip, ShipFlow)
contains(ShipFlow, Handover)
covers(Handover, Dispatch)
raises(Gateway, Authorized)
triggers(Authorized, Capture)
raises(Dispatch, Shipped)
triggers(Shipped, Carrier)
"#;

    const SYSTEMS: &str = r#"
system Payments "Payments"
system Logistics "Logistics"
api CaptureApi "Capture API"
api DispatchApi "Dispatch API"
contains(Payments, CaptureApi)
contains(Logistics, DispatchApi)
invokes(Capture, CaptureApi)
invokes(Dispatch, DispatchApi)
"#;

    fn emit(src: &str, view: View) -> String {
        ContextMermaidEmitter.emit(&model_from(src), &view).unwrap()
    }

    #[test]
    fn without_a_system_the_edges_meet_a_synthesized_one() {
        let result = emit(SHOP, View::whole());
        assert!(
            result.contains(
                "  system__Model[\"システム\"]:::system\n  %% synthesized system__Model: no system is declared\n"
            ),
            "{result}"
        );
        for edge in [
            "  actor__Customer -.->|Pay| system__Model\n  %% derived actor__Customer -> system__Model via buc__BucPay\n",
            "  actor__Courier -.->|Ship| system__Model\n  %% derived actor__Courier -> system__Model via buc__BucShip\n",
            "  extsystem__Gateway -.->|Authorized| system__Model\n  %% derived extsystem__Gateway -> system__Model via event__Authorized\n",
            "  system__Model -.->|Shipped| extsystem__Carrier\n  %% derived system__Model -> extsystem__Carrier via event__Shipped\n",
        ] {
            assert!(result.contains(edge), "{edge}: {result}");
        }
        assert!(
            result.contains("  actor__Idle[\"Idle\"]:::actor\n"),
            "{result}"
        );
        assert!(
            result.starts_with(
                "%% rdra-ish diagram=context scope=whole\n%% kinds: actor extsystem system\n%% omitted: buc step usecase event\n"
            ),
            "{result}"
        );
    }

    #[test]
    fn with_several_systems_an_edge_meets_the_system_of_the_apis_its_use_cases_invoke() {
        let result = emit(&format!("{SHOP}{SYSTEMS}"), View::whole());
        for edge in [
            "  actor__Customer -.->|Pay| system__Payments\n",
            "  actor__Courier -.->|Ship| system__Logistics\n",
            "  extsystem__Gateway -.->|Authorized| system__Payments\n",
            "  system__Logistics -.->|Shipped| extsystem__Carrier\n",
        ] {
            assert!(result.contains(edge), "{edge}: {result}");
        }
        // Refund invokes no API, so no declared system can be named.
        assert!(
            result.contains(
                "  actor__Clerk -.->|Pay| system__Model\n  %% derived actor__Clerk -> system__Model via buc__BucPay\n"
            ),
            "{result}"
        );
        assert!(result.contains(
            "  %% synthesized system__Model: no declared system contains an API the use cases behind its edges invoke\n"
        ));
        assert!(!result.contains("actor__Customer -.->|Pay| system__Logistics"));
    }

    #[test]
    fn one_declared_system_takes_every_edge() {
        let src = format!("{SHOP}system Shop \"Shop\"\n");
        let result = emit(&src, View::whole());
        assert!(!result.contains("system__Model"), "{result}");
        assert!(result.contains("  actor__Clerk -.->|Pay| system__Shop\n"));
        assert!(result.contains("  system__Shop -.->|Shipped| extsystem__Carrier\n"));
    }

    #[test]
    fn a_buc_scope_draws_who_takes_part_in_it() {
        let result = emit(
            &format!("{SHOP}{SYSTEMS}"),
            View::bucs(vec!["BucPay".to_string()]),
        );
        assert!(result.starts_with("%% rdra-ish diagram=context scope=buc:BucPay\n"));
        for present in [
            "actor__Customer[",
            "actor__Clerk[",
            "extsystem__Gateway[",
            "system__Payments[",
        ] {
            assert!(result.contains(present), "{present}: {result}");
        }
        for absent in [
            "actor__Courier",
            "extsystem__Carrier",
            "system__Logistics[",
            "actor__Idle",
        ] {
            assert!(!result.contains(absent), "{absent}: {result}");
        }
    }

    #[test]
    fn an_actor_has_one_edge_labelled_with_every_buc_it_performs() {
        let src = format!(
            "{SHOP}performs(Customer, BucShip)\nraises(Gateway, Authorized)\nevent Refunded \"Refunded\"\nraises(Gateway, Refunded)\ntriggers(Refunded, Dispatch)\n"
        );
        let result = emit(&src, View::whole());
        assert!(
            result.contains(
                "  actor__Customer -.->|Pay、Ship| system__Model\n  %% derived actor__Customer -> system__Model via buc__BucPay,buc__BucShip\n"
            ),
            "{result}"
        );
        assert!(
            result.contains(
                "  extsystem__Gateway -.->|Authorized、Refunded| system__Model\n  %% derived extsystem__Gateway -> system__Model via event__Authorized,event__Refunded\n"
            ),
            "{result}"
        );
        assert_eq!(
            result.matches("actor__Customer -.->").count(),
            1,
            "{result}"
        );
    }

    #[test]
    fn the_diagram_draws_no_layer_subgraphs() {
        let result = emit(SHOP, View::whole());
        assert!(!result.contains("subgraph"), "{result}");
        assert!(!result.contains("%% layers:"), "{result}");
    }
}
