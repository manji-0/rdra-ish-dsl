//! The requirement model (`--kind requirement`): a value tree, left to right from the goals,
//! through the requirements that achieve them, to the BUCs those requirements motivate. A
//! requirement marked `status "out-of-scope"` is drawn apart, with dashed edges. NFRs,
//! constraints, qualities, entities, and systems are another question; the header's
//! `omitted:` line names them.

use super::{buc_scope, kinds_by_layer, omitted_linked_kinds, render_edges, GraphEdge};
use crate::diagram_common::{
    class_defs, contract_id, desc_line, node_label, node_line, quote, Header,
};
use crate::{collect_object_graph_nodes, node_kind_filter_name, EmitError, Emitter, Scope, View};
use rdra_ish_core::model::{Goal, NodeRef, RelKind, SemanticModel};
use std::collections::HashSet;

pub struct RequirementMermaidEmitter;

/// The subgraph that holds the requirements marked `status "out-of-scope"`.
const OUT_OF_SCOPE_GROUP: &str = "group_out_of_scope";

fn is_out_of_scope(model: &SemanticModel, node: &NodeRef) -> bool {
    matches!(node, NodeRef::Requirement(key)
        if model.requirements.get(*key).is_some_and(|r| r.status.as_deref() == Some("out-of-scope")))
}

/// `text` safe inside a node label that Mermaid renders as HTML.
fn label_text(text: &str) -> String {
    quote(text).replace('<', "#lt;").replace('>', "#gt;")
}

/// The second line of a goal's label: its metric with `baseline → target`, when it has them.
fn measure(goal: &Goal) -> Option<String> {
    let range = match (&goal.baseline, &goal.target) {
        (Some(baseline), Some(target)) => Some(format!("{baseline} → {target}")),
        (None, Some(target)) => Some(format!("→ {target}")),
        (Some(baseline), None) => Some(format!("{baseline} →")),
        (None, None) => None,
    };
    match (&goal.metric, range) {
        (Some(metric), Some(range)) => Some(format!("{metric}: {range}")),
        (Some(metric), None) => Some(metric.clone()),
        (None, range) => range,
    }
}

/// `node` with its `:::<kind>` and its `%% desc` line, at `indent`; a goal adds its measure
/// under the label.
fn declare_node(out: &mut String, model: &SemanticModel, node: &NodeRef, indent: &str) {
    let (Some(id), Some(label)) = (contract_id(model, node), node_label(model, node)) else {
        return;
    };
    let mut text = label_text(label);
    if let NodeRef::Goal(key) = node {
        if let Some(measure) = model.goals.get(*key).and_then(measure) {
            text.push_str(&format!("<br/>{}", label_text(&measure)));
        }
    }
    let kind = node_kind_filter_name(node);
    out.push_str(&node_line(indent, &id, &text, kind));
    if let Some(desc) = desc_line(model, node, &id) {
        out.push_str(&format!("{indent}{desc}\n"));
    }
}

impl Emitter for RequirementMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let scope = match &view.scope {
            Scope::Bucs(ids) => Some(buc_scope(model, ids)),
            Scope::Whole | Scope::UseCases(_) => None,
        };
        let in_scope = |nr: &NodeRef| scope.as_ref().is_none_or(|set| set.contains(nr));

        // The BUCs a requirement motivates, in the scope.
        let motivated: Vec<(&NodeRef, &NodeRef)> = model
            .relations
            .iter()
            .filter(|rel| {
                rel.kind == RelKind::Motivates
                    && matches!(rel.from, NodeRef::Requirement(_))
                    && matches!(rel.to, NodeRef::Buc(_))
                    && in_scope(&rel.to)
            })
            .map(|rel| (&rel.from, &rel.to))
            .collect();
        let mut drawn: HashSet<NodeRef> = motivated
            .iter()
            .flat_map(|(requirement, buc)| [(*requirement).clone(), (*buc).clone()])
            .collect();
        if scope.is_none() {
            drawn.extend(collect_object_graph_nodes(model, &|nr| {
                matches!(nr, NodeRef::Requirement(_) | NodeRef::Goal(_))
            }));
        }
        let achieved: Vec<(&NodeRef, &NodeRef)> = model
            .relations
            .iter()
            .filter(|rel| rel.kind == RelKind::Achieves && drawn.contains(&rel.from))
            .map(|rel| (&rel.from, &rel.to))
            .collect();
        drawn.extend(achieved.iter().map(|(_, goal)| (*goal).clone()));
        let nodes = collect_object_graph_nodes(model, &|nr| drawn.contains(nr));

        let mut body = String::new();
        let (apart, tree): (Vec<&NodeRef>, Vec<&NodeRef>) =
            nodes.iter().partition(|nr| is_out_of_scope(model, nr));
        for node in tree {
            declare_node(&mut body, model, node, "  ");
        }
        if !apart.is_empty() {
            body.push_str(&format!(
                "  subgraph {OUT_OF_SCOPE_GROUP}[\"やらないこと\"]\n"
            ));
            for node in apart {
                declare_node(&mut body, model, node, "    ");
            }
            body.push_str("  end\n");
        }

        let arrow = |requirement: &NodeRef| {
            if is_out_of_scope(model, requirement) {
                "-.->"
            } else {
                "-->"
            }
        };
        let mut edges: Vec<GraphEdge> = Vec::new();
        for (requirement, buc) in motivated {
            if let (Some(from), Some(to)) =
                (contract_id(model, requirement), contract_id(model, buc))
            {
                edges.push(GraphEdge {
                    from,
                    label: "motivates".to_string(),
                    to,
                    arrow: arrow(requirement),
                    derived: None,
                });
            }
        }
        // The model says `achieves(Requirement, Goal)`; the tree reads from the goal, so the
        // edge is drawn the other way and says so.
        for (requirement, goal) in achieved {
            if let (Some(req_id), Some(goal_id)) =
                (contract_id(model, requirement), contract_id(model, goal))
            {
                edges.push(GraphEdge {
                    from: goal_id,
                    label: "achieved_by".to_string(),
                    to: req_id.clone(),
                    arrow: arrow(requirement),
                    derived: Some(format!("via {req_id}")),
                });
            }
        }
        render_edges(&mut body, "requirement", edges);

        let kinds = kinds_by_layer(&nodes);
        body.push_str(&class_defs(&kinds));
        let mut omitted = omitted_linked_kinds(
            model,
            |nr| {
                drawn.contains(nr)
                    || matches!(
                        nr,
                        NodeRef::Nfr(_) | NodeRef::Quality(_) | NodeRef::Constraint(_)
                    )
            },
            &drawn,
            &kinds,
        );
        for (kind, present) in [
            ("nfr", !model.nfrs.is_empty()),
            ("quality", !model.qualities.is_empty()),
            ("constraint", !model.constraints.is_empty()),
            ("entity", !model.entities.is_empty()),
            ("system", !model.systems.is_empty()),
        ] {
            if present && !omitted.iter().any(|k| k == kind) {
                omitted.push(kind.to_string());
            }
        }
        let header = Header {
            diagram: "requirement",
            view,
            layers: None,
            omitted,
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
requirement ReqFast "Pay fast"
requirement ReqLoose "Nothing yet"
requirement ReqVideo "No video payment"
  status "out-of-scope"
goal ShortWait "Short wait"
  metric "Median wait"
  baseline "45 min"
  target "30 min"
goal Unserved "Unserved"
goal Bare "Bare"
  metric "Complaints"
goal Below "Below"
  metric "Rate"
  target "<5%"
nfr Latency "Latency"
quality Performance "Performance"
quality Privacy "Privacy"
constraint Pii "PII"
adr AdrOne "One system"
system Shop "Shop"
buc BucPay "Pay"
buc BucShip "Ship"
buc BucIdle "Idle"
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
motivates(ReqFast, BucShip)
motivates(ReqVideo, BucPay)
achieves(ReqFast, ShortWait)
achieves(ReqFast, Below)
applies_to(Latency, CaptureApi)
qualifies(Latency, Performance)
qualifies(Pii, Privacy)
constrains(Pii, Payment)
decides(AdrOne, ReqFast)
"#;

    fn emit(view: View) -> String {
        RequirementMermaidEmitter
            .emit(&model_from(SHOP), &view)
            .unwrap()
    }

    #[test]
    fn the_tree_runs_from_a_goal_through_a_requirement_to_a_buc() {
        let result = emit(View::whole());
        for edge in [
            "  goal__ShortWait --> requirement__ReqFast\n  %% derived goal__ShortWait -> requirement__ReqFast via requirement__ReqFast\n",
            "  requirement__ReqFast --> buc__BucPay\n",
            "  requirement__ReqFast --> buc__BucShip\n",
        ] {
            assert!(result.contains(edge), "{edge}: {result}");
        }
        for present in [
            "  requirement__ReqFast[/\"Pay fast\"\\]:::requirement\n",
            "  requirement__ReqLoose[/\"Nothing yet\"\\]:::requirement\n",
            "  goal__Unserved[/\"Unserved\"/]:::goal\n",
        ] {
            assert!(result.contains(present), "{present}: {result}");
        }
        assert!(!result.contains("buc__BucIdle"), "{result}");
        assert!(!result.contains("subgraph layer_"), "{result}");
    }

    #[test]
    fn a_goal_adds_its_metric_and_baseline_to_target_under_its_label() {
        let result = emit(View::whole());
        for present in [
            "  goal__ShortWait[/\"Short wait<br/>Median wait: 45 min → 30 min\"/]:::goal\n",
            "  goal__Bare[/\"Bare<br/>Complaints\"/]:::goal\n",
            "  goal__Below[/\"Below<br/>Rate: → #lt;5%\"/]:::goal\n",
        ] {
            assert!(result.contains(present), "{present}: {result}");
        }
    }

    #[test]
    fn an_out_of_scope_requirement_is_drawn_apart_with_dashed_edges() {
        let result = emit(View::whole());
        assert!(
            result.contains(
                "  subgraph group_out_of_scope[\"やらないこと\"]\n    requirement__ReqVideo[/\"No video payment\"\\]:::requirement\n  end\n"
            ),
            "{result}"
        );
        assert!(
            result.contains("  requirement__ReqVideo -.-> buc__BucPay\n"),
            "{result}"
        );
        assert!(!result.contains("やらないこと\"]\n    requirement__ReqFast"));
    }

    #[test]
    fn nfrs_constraints_qualities_entities_and_systems_leave_the_diagram() {
        let result = emit(View::whole());
        for absent in [
            "nfr__",
            "quality__",
            "constraint__",
            "entity__",
            "system__",
            "api__",
        ] {
            assert!(!result.contains(absent), "{absent}: {result}");
        }
        assert!(
            result.starts_with(
                "%% rdra-ish diagram=requirement scope=whole\n%% kinds: requirement goal buc\n"
            ),
            "{result}"
        );
        let omitted = result
            .lines()
            .find_map(|line| line.strip_prefix("%% omitted: "))
            .unwrap();
        for kind in ["adr", "nfr", "quality", "constraint", "entity", "system"] {
            assert!(omitted.split(' ').any(|k| k == kind), "{kind}: {omitted}");
        }
    }

    #[test]
    fn a_buc_scope_keeps_the_requirements_that_motivate_it_and_their_goals() {
        let ship = emit(View::bucs(vec!["BucShip".to_string()]));
        assert!(
            ship.starts_with("%% rdra-ish diagram=requirement scope=buc:BucShip\n"),
            "{ship}"
        );
        for present in [
            "  requirement__ReqFast --> buc__BucShip\n",
            "  goal__ShortWait --> requirement__ReqFast\n",
        ] {
            assert!(ship.contains(present), "{present}: {ship}");
        }
        for absent in [
            "buc__BucPay",
            "requirement__ReqVideo",
            "requirement__ReqLoose",
            "goal__Unserved",
        ] {
            assert!(!ship.contains(absent), "{absent}: {ship}");
        }

        let pay = emit(View::bucs(vec!["BucPay".to_string()]));
        assert!(pay.contains("group_out_of_scope"), "{pay}");
        assert!(!pay.contains("buc__BucShip"), "{pay}");
    }
}
