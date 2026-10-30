//! The business flow (`--kind flow`): each flow with its steps in order, the branches,
//! exceptions, and repeats between them, and the use cases, APIs, events, and timers each
//! step covers. A step one actor performs sits in that actor's swimlane.

use super::{declare, omitted_linked_kinds, rdra_arrow, render_edges, GraphEdge};
use crate::diagram_common::{
    class_defs, contract_id, desc_line, node_id, node_label, quote, Header,
};
use crate::{collect_object_graph_nodes, node_kind_filter_name, EmitError, Emitter, Scope, View};
use rdra_ish_core::model::{NodeRef, RelKind, SemanticModel};
use std::collections::{HashMap, HashSet};

pub struct FlowMermaidEmitter;

/// The flows of the scope, sorted by id: every one, or those the named BUCs contain.
fn scoped_flows(model: &SemanticModel, scope: &Scope) -> Vec<NodeRef> {
    let contained: Option<HashSet<NodeRef>> = match scope {
        Scope::Bucs(ids) => Some(
            model
                .relations
                .iter()
                .filter(|rel| {
                    rel.kind == RelKind::Contains
                        && matches!(rel.from, NodeRef::Buc(_))
                        && node_id(model, &rel.from).is_some_and(|id| ids.iter().any(|w| w == id))
                })
                .map(|rel| rel.to.clone())
                .collect(),
        ),
        Scope::Whole | Scope::UseCases(_) => None,
    };
    collect_object_graph_nodes(model, &|nr| {
        matches!(nr, NodeRef::Flow(_)) && contained.as_ref().is_none_or(|set| set.contains(nr))
    })
}

/// `to` of every `kind` relation from `from`.
fn targets<'a>(
    model: &'a SemanticModel,
    from: &'a NodeRef,
    kind: RelKind,
) -> impl Iterator<Item = &'a NodeRef> + 'a {
    model
        .relations
        .iter()
        .filter(move |rel| rel.kind == kind && rel.from == *from)
        .map(|rel| &rel.to)
}

impl Emitter for FlowMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let flows = scoped_flows(model, &view.scope);
        // Each step of a drawn flow, and in the whole model each step of no flow.
        let mut steps_of: Vec<(Option<&NodeRef>, Vec<NodeRef>)> = flows
            .iter()
            .map(|flow| {
                let steps: HashSet<&NodeRef> = targets(model, flow, RelKind::Contains)
                    .filter(|nr| matches!(nr, NodeRef::Step(_)))
                    .collect();
                (
                    Some(flow),
                    collect_object_graph_nodes(model, &|nr| steps.contains(nr)),
                )
            })
            .collect();
        if matches!(view.scope, Scope::Whole | Scope::UseCases(_)) {
            let in_flow: HashSet<&NodeRef> = model
                .relations
                .iter()
                .filter(|rel| rel.kind == RelKind::Contains && matches!(rel.from, NodeRef::Flow(_)))
                .map(|rel| &rel.to)
                .collect();
            let loose = collect_object_graph_nodes(model, &|nr| {
                matches!(nr, NodeRef::Step(_)) && !in_flow.contains(nr)
            });
            if !loose.is_empty() {
                steps_of.push((None, loose));
            }
        }
        let steps: HashSet<NodeRef> = steps_of
            .iter()
            .flat_map(|(_, steps)| steps.iter().cloned())
            .collect();

        // A step of a flow that one actor performs sits in that actor's lane; any other
        // performed step has a `performs` edge from each of its actors.
        let mut performers: HashMap<&NodeRef, Vec<&NodeRef>> = HashMap::new();
        for rel in &model.relations {
            if rel.kind == RelKind::Performs
                && matches!(rel.from, NodeRef::Actor(_))
                && steps.contains(&rel.to)
            {
                let actors = performers.entry(&rel.to).or_default();
                if !actors.contains(&&rel.from) {
                    actors.push(&rel.from);
                }
            }
        }
        let in_flow: HashSet<&NodeRef> = steps_of
            .iter()
            .filter(|(flow, _)| flow.is_some())
            .flat_map(|(_, steps)| steps)
            .collect();
        let lane_of = |step: &NodeRef| match performers.get(step).map(Vec::as_slice) {
            Some([actor]) if in_flow.contains(step) => Some(*actor),
            _ => None,
        };

        let mut body = String::new();
        let mut kinds: Vec<&'static str> = Vec::new();
        let mut add_kind = |kind: &'static str| {
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        };
        for (flow, steps) in &steps_of {
            let indent = match flow {
                Some(flow) => {
                    let (Some(id), Some(label)) =
                        (contract_id(model, flow), node_label(model, flow))
                    else {
                        continue;
                    };
                    add_kind("flow");
                    body.push_str(&format!("  subgraph {id}[\"{}\"]\n", quote(label)));
                    if let Some(desc) = desc_line(model, flow, &id) {
                        body.push_str(&format!("    {desc}\n"));
                    }
                    "    "
                }
                None => "  ",
            };
            if !steps.is_empty() {
                add_kind("step");
            }
            for step in steps.iter().filter(|step| lane_of(step).is_none()) {
                declare(&mut body, model, step, indent);
            }
            let lanes = collect_object_graph_nodes(model, &|nr| {
                steps.iter().any(|step| lane_of(step) == Some(nr))
            });
            for actor in &lanes {
                let (Some(actor_id), Some(label)) =
                    (node_id(model, actor), node_label(model, actor))
                else {
                    continue;
                };
                let flow_id = flow.and_then(|flow| node_id(model, flow)).unwrap_or("");
                add_kind("lane");
                body.push_str(&format!(
                    "{indent}subgraph lane__{flow_id}__{actor_id}[\"{}\"]\n\
                     {indent}  %% lane lane__{flow_id}__{actor_id}: actor__{actor_id}\n",
                    quote(label)
                ));
                for step in steps.iter().filter(|step| lane_of(step) == Some(actor)) {
                    declare(&mut body, model, step, &format!("{indent}  "));
                }
                body.push_str(&format!("{indent}end\n"));
            }
            if flow.is_some() {
                body.push_str("  end\n");
            }
        }

        let mut edges = Vec::new();
        let mut outside: HashSet<NodeRef> = HashSet::new();
        for rel in &model.relations {
            let drawn = match (&rel.kind, &rel.from, &rel.to) {
                (
                    RelKind::Precedes | RelKind::Branches | RelKind::Excepts | RelKind::Repeats,
                    from,
                    to,
                ) => steps.contains(from) && steps.contains(to),
                (RelKind::Covers, step, target) if steps.contains(step) => {
                    outside.insert(target.clone());
                    true
                }
                (RelKind::Performs, actor @ NodeRef::Actor(_), step)
                    if steps.contains(step) && lane_of(step).is_none() =>
                {
                    outside.insert(actor.clone());
                    true
                }
                _ => false,
            };
            if let (true, Some(from), Some(to)) = (
                drawn,
                contract_id(model, &rel.from),
                contract_id(model, &rel.to),
            ) {
                edges.push(GraphEdge {
                    from,
                    label: crate::object_graph_rel_label(&rel.kind).to_string(),
                    to,
                    arrow: rdra_arrow(&rel.kind),
                    derived: None,
                });
            }
        }
        let outside = collect_object_graph_nodes(model, &|nr| outside.contains(nr));
        for node in &outside {
            declare(&mut body, model, node, "  ");
            add_kind(node_kind_filter_name(node));
        }
        render_edges(&mut body, edges);
        body.push_str(&class_defs(&kinds));

        let mut drawn: HashSet<NodeRef> = flows.iter().cloned().collect();
        drawn.extend(steps.iter().cloned());
        drawn.extend(outside.iter().cloned());
        drawn.extend(performers.values().flatten().map(|actor| (*actor).clone()));
        let header = Header {
            diagram: "flow",
            view,
            layers: None,
            omitted: omitted_linked_kinds(
                model,
                |nr| matches!(nr, NodeRef::Flow(_) | NodeRef::Step(_)) && drawn.contains(nr),
                &drawn,
                &kinds,
            ),
            kinds,
        };
        Ok(format!("{}flowchart TD\n{body}", header.render()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram_common::model_from;

    const SHOP: &str = r#"
actor Clerk "Clerk"
actor Courier "Courier"
actor Manager "Manager"
buc BucShip "Ship"
buc BucOther "Other"
flow ShipFlow "Ship flow"
flow OtherFlow "Other flow"
step Pick "Pick"
step Pack "Pack"
step Recheck "Recheck"
step Handover "Handover"
step Lost "Lost"
step Elsewhere "Elsewhere"
usecase PickItems "Pick items"
api LabelApi "Label API"
event Shipped "Shipped"
timer Cutoff "Cutoff"
contains(BucShip, ShipFlow)
contains(BucOther, OtherFlow)
contains(ShipFlow, Pick)
contains(ShipFlow, Pack)
contains(ShipFlow, Recheck)
contains(ShipFlow, Handover)
contains(ShipFlow, Lost)
contains(OtherFlow, Elsewhere)
precedes(Pick, Pack)
branches(Pack, Recheck)
repeats(Recheck, Pack)
precedes(Pack, Handover)
excepts(Handover, Lost)
covers(Pick, PickItems)
covers(Pack, LabelApi)
covers(Handover, Shipped)
covers(Pick, Cutoff)
performs(Clerk, Pick)
performs(Clerk, Pack)
performs(Courier, Handover)
performs(Manager, Handover)
"#;

    fn emit(view: View) -> String {
        FlowMermaidEmitter.emit(&model_from(SHOP), &view).unwrap()
    }

    #[test]
    fn the_four_ordering_edges_keep_their_predicate_and_line() {
        let result = emit(View::whole());
        for edge in [
            "  step__Pick -->|precedes| step__Pack\n",
            "  step__Pack -->|branches| step__Recheck\n",
            "  step__Recheck -->|repeats| step__Pack\n",
            "  step__Handover -.->|excepts| step__Lost\n",
        ] {
            assert!(result.contains(edge), "{edge}: {result}");
        }
    }

    #[test]
    fn a_step_covers_use_cases_apis_events_and_timers() {
        let result = emit(View::whole());
        for edge in [
            "  step__Pick -.->|covers| usecase__PickItems\n",
            "  step__Pack -.->|covers| api__LabelApi\n",
            "  step__Handover -.->|covers| event__Shipped\n",
            "  step__Pick -.->|covers| timer__Cutoff\n",
        ] {
            assert!(result.contains(edge), "{edge}: {result}");
        }
        assert!(
            result.contains("  timer__Cutoff[\"Cutoff\"]:::timer\n"),
            "{result}"
        );
        assert!(
            result.contains("%% kinds: flow step lane actor timer usecase event api\n"),
            "{result}"
        );
    }

    #[test]
    fn a_step_one_actor_performs_sits_in_that_actors_lane() {
        let result = emit(View::whole());
        assert!(
            result.contains(
                "  subgraph flow__ShipFlow[\"Ship flow\"]\n    step__Handover[\"Handover\"]:::step\n    step__Lost[\"Lost\"]:::step\n    step__Recheck[\"Recheck\"]:::step\n    subgraph lane__ShipFlow__Clerk[\"Clerk\"]\n      %% lane lane__ShipFlow__Clerk: actor__Clerk\n      step__Pack[\"Pack\"]:::step\n      step__Pick[\"Pick\"]:::step\n    end\n  end\n"
            ),
            "{result}"
        );
        // Performed by two actors: in the flow, with an edge from each.
        assert!(result.contains("  actor__Courier -.->|performs| step__Handover\n"));
        assert!(result.contains("  actor__Manager -.->|performs| step__Handover\n"));
        assert!(!result.contains("actor__Clerk -.->"), "{result}");
    }

    #[test]
    fn without_performs_there_are_no_lanes_and_a_buc_draws_its_flows() {
        let result = emit(View::bucs(vec!["BucOther".to_string()]));
        assert_eq!(
            result,
            "%% rdra-ish diagram=flow scope=buc:BucOther\n%% kinds: flow step\n%% omitted: buc\nflowchart TD\n  subgraph flow__OtherFlow[\"Other flow\"]\n    step__Elsewhere[\"Elsewhere\"]:::step\n  end\n  classDef flow stroke:#333;\n  classDef step stroke:#333;\n"
        );
    }
}
