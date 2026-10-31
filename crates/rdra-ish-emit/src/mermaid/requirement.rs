//! The requirement model (`--kind requirement`): the requirements with the BUCs they motivate
//! and the goals they achieve,
//! and the non-functional requirements and constraints with what they apply to or constrain
//! and the quality each belongs to.

use super::{
    buc_scope, kinds_by_layer, omitted_linked_kinds, render_edges, render_layered_nodes, EdgeSet,
};
use crate::diagram_common::Header;
use crate::{collect_object_graph_nodes, object_graph_rel_label, EmitError, Emitter, Scope, View};
use rdra_ish_core::model::{NodeRef, RelKind, SemanticModel};
use std::collections::HashSet;

pub struct RequirementMermaidEmitter;

/// Whether `rel` is an edge of the requirement model: a requirement's `motivates` or
/// `achieves`, an NFR's `applies_to`, a constraint's `constrains`, or a `qualifies`.
fn is_requirement_edge(rel: &rdra_ish_core::model::Relation) -> bool {
    matches!(
        (&rel.kind, &rel.from),
        (
            RelKind::Motivates | RelKind::Achieves,
            NodeRef::Requirement(_)
        ) | (RelKind::AppliesTo, NodeRef::Nfr(_))
            | (RelKind::Constrains, NodeRef::Constraint(_))
            | (RelKind::Qualifies, NodeRef::Nfr(_) | NodeRef::Constraint(_))
    )
}

/// What a BUC scope reaches: its BUCs, use cases, steps, and APIs, the entities those use
/// cases and APIs read and write, and the systems that contain those APIs.
fn scope_of(model: &SemanticModel, ids: &[String]) -> HashSet<NodeRef> {
    let mut scope = buc_scope(model, ids);
    let reached: Vec<NodeRef> = model
        .relations
        .iter()
        .filter_map(|rel| match (&rel.kind, &rel.from, &rel.to) {
            (
                RelKind::Reads
                | RelKind::Writes
                | RelKind::Creates
                | RelKind::Updates
                | RelKind::Deletes,
                from,
                entity @ NodeRef::Entity(_),
            ) if scope.contains(from) => Some(entity.clone()),
            (RelKind::Contains, system @ NodeRef::System(_), api) if scope.contains(api) => {
                Some(system.clone())
            }
            _ => None,
        })
        .collect();
    scope.extend(reached);
    scope
}

impl Emitter for RequirementMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let scope = match &view.scope {
            Scope::Bucs(ids) => Some(scope_of(model, ids)),
            Scope::Whole | Scope::UseCases(_) => None,
        };
        let in_scope = |nr: &NodeRef| scope.as_ref().is_none_or(|set| set.contains(nr));

        let mut edges = EdgeSet::default();
        for rel in model
            .relations
            .iter()
            .filter(|rel| is_requirement_edge(rel))
        {
            if !matches!(rel.kind, RelKind::Qualifies | RelKind::Achieves) && in_scope(&rel.to) {
                let label = object_graph_rel_label(&rel.kind).to_string();
                edges.add(model, (&rel.from, &rel.kind, label, &rel.to), None);
            }
        }
        // Each quality of a drawn NFR or constraint and each goal of a drawn requirement; in the
        // whole model every requirement element, linked or not.
        let sources: HashSet<NodeRef> = edges.nodes.iter().cloned().collect();
        for rel in model
            .relations
            .iter()
            .filter(|rel| matches!(rel.kind, RelKind::Qualifies | RelKind::Achieves))
        {
            if scope.is_none() || sources.contains(&rel.from) {
                let label = object_graph_rel_label(&rel.kind).to_string();
                edges.add(model, (&rel.from, &rel.kind, label, &rel.to), None);
            }
        }
        let mut drawn: HashSet<NodeRef> = edges.nodes.iter().cloned().collect();
        if scope.is_none() {
            drawn.extend(collect_object_graph_nodes(model, &|nr| {
                matches!(
                    nr,
                    NodeRef::Requirement(_)
                        | NodeRef::Goal(_)
                        | NodeRef::Nfr(_)
                        | NodeRef::Quality(_)
                        | NodeRef::Constraint(_)
                )
            }));
        }
        let nodes = collect_object_graph_nodes(model, &|nr| drawn.contains(nr));

        let mut body = String::new();
        let layers = render_layered_nodes(&mut body, model, &nodes);
        render_edges(&mut body, edges.into_graph_edges());
        let kinds = kinds_by_layer(&nodes);
        body.push_str(&crate::diagram_common::class_defs(&kinds));
        let header = Header {
            diagram: "requirement",
            view,
            layers: Some(layers),
            omitted: omitted_linked_kinds(
                model,
                |nr| {
                    matches!(
                        nr,
                        NodeRef::Requirement(_)
                            | NodeRef::Goal(_)
                            | NodeRef::Nfr(_)
                            | NodeRef::Quality(_)
                            | NodeRef::Constraint(_)
                    ) && drawn.contains(nr)
                },
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
requirement ReqFast "Check out fast"
requirement ReqLoose "Nothing yet"
nfr Latency "Latency"
quality Performance "Performance"
quality Privacy "Privacy"
constraint Pii "PII"
adr AdrOne "One system"
system Shop "Shop"
buc BucPay "Pay"
buc BucShip "Ship"
usecase Capture "Capture"
usecase Dispatch "Dispatch"
api CaptureApi "Capture API"
entity Payment "Payment" { id: Int @pk }
entity Parcel "Parcel" { id: Int @pk }
contains(BucPay, Capture)
contains(BucShip, Dispatch)
contains(Shop, CaptureApi)
invokes(Capture, CaptureApi)
updates(CaptureApi, Payment)
updates(Dispatch, Parcel)
motivates(ReqFast, BucPay)
applies_to(Latency, CaptureApi)
applies_to(Latency, Shop)
qualifies(Latency, Performance)
qualifies(Pii, Privacy)
constrains(Pii, Payment)
constrains(Pii, Parcel)
decides(AdrOne, ReqFast)
"#;

    fn emit(view: View) -> String {
        RequirementMermaidEmitter
            .emit(&model_from(SHOP), &view)
            .unwrap()
    }

    #[test]
    fn each_requirement_element_links_what_it_motivates_applies_to_or_constrains() {
        let result = emit(View::whole());
        for edge in [
            "  requirement__ReqFast -.->|motivates| buc__BucPay\n",
            "  nfr__Latency -.->|applies_to| api__CaptureApi\n",
            "  nfr__Latency -.->|applies_to| system__Shop\n",
            "  nfr__Latency -.->|qualifies| quality__Performance\n",
            "  constraint__Pii -.->|constrains| entity__Payment\n",
            "  constraint__Pii -.->|qualifies| quality__Privacy\n",
        ] {
            assert!(result.contains(edge), "{edge}: {result}");
        }
        assert!(
            result.contains("    requirement__ReqLoose[\"Nothing yet\"]:::requirement\n"),
            "{result}"
        );
        assert!(
            result.starts_with(
                "%% rdra-ish diagram=requirement scope=whole\n%% layers: value=システム価値 environment=システム外部環境 system=システム\n%% kinds: requirement nfr quality constraint buc api system entity\n%% omitted: adr\n"
            ),
            "{result}"
        );
    }

    #[test]
    fn a_goal_is_drawn_with_the_requirements_that_achieve_it() {
        let src = format!(
            "{SHOP}goal ShortWait \"Short wait\"\ngoal Unserved \"Unserved\"\nachieves(ReqFast, ShortWait)\n"
        );
        let whole = RequirementMermaidEmitter
            .emit(&model_from(&src), &View::whole())
            .unwrap();
        for present in [
            "  requirement__ReqFast -.->|achieves| goal__ShortWait\n",
            "    goal__ShortWait[\"Short wait\"]:::goal\n",
            "    goal__Unserved[\"Unserved\"]:::goal\n",
        ] {
            assert!(whole.contains(present), "{present}: {whole}");
        }

        let pay = RequirementMermaidEmitter
            .emit(&model_from(&src), &View::bucs(vec!["BucPay".to_string()]))
            .unwrap();
        assert!(
            pay.contains("  requirement__ReqFast -.->|achieves| goal__ShortWait\n"),
            "{pay}"
        );
        assert!(!pay.contains("goal__Unserved"), "{pay}");
    }

    #[test]
    fn a_buc_scope_draws_what_reaches_its_use_cases_apis_entities_and_systems() {
        let pay = emit(View::bucs(vec!["BucPay".to_string()]));
        for present in [
            "  requirement__ReqFast -.->|motivates| buc__BucPay\n",
            "  nfr__Latency -.->|applies_to| system__Shop\n",
            "  constraint__Pii -.->|constrains| entity__Payment\n",
            "  constraint__Pii -.->|qualifies| quality__Privacy\n",
        ] {
            assert!(pay.contains(present), "{present}: {pay}");
        }
        for absent in ["entity__Parcel", "requirement__ReqLoose"] {
            assert!(!pay.contains(absent), "{absent}: {pay}");
        }

        let ship = emit(View::bucs(vec!["BucShip".to_string()]));
        assert!(ship.contains("  constraint__Pii -.->|constrains| entity__Parcel\n"));
        assert!(
            !ship.contains("requirement__") && !ship.contains("nfr__"),
            "{ship}"
        );
    }
}
