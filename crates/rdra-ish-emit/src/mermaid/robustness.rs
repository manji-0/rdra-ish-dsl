//! The ロバストネス図 (`--kind robustness`): an ICONIX robustness diagram of the use cases in
//! scope. The edge grammar is actor -> boundary -> control -> entity, so a screen, an API, and
//! an entity never share an edge with anything else.
//!
//! - boundary: a `screen` a use case displays, an `extsystem` that raises an event starting one
//!   or receives an event one raises, and a `timer` that starts one.
//! - control: each `api` a use case invokes, and the use case itself when it reads or writes
//!   entities directly or has no API.
//! - entity: each entity a control reads or writes.

use super::usecase::{is_crud, scoped_use_cases};
use super::{kinds_by_layer, omitted_linked_kinds, render_edges, GraphEdge};
use crate::diagram_common::{class_defs, contract_id, node_label, quote, Header};
use crate::{node_kind_filter_name, EmitError, Emitter, View};
use rdra_ish_core::model::{NodeRef, RelKind, SemanticModel};
use std::collections::{BTreeMap, HashSet};

pub struct RobustnessMermaidEmitter;

/// `C`, `R`, `U`, `D` of the operations in `kinds`; `writes` counts as `U`.
fn crud_letters(kinds: &[RelKind]) -> String {
    let has = |kind: RelKind| kinds.contains(&kind);
    [
        (has(RelKind::Creates), 'C'),
        (has(RelKind::Reads), 'R'),
        (has(RelKind::Updates) || has(RelKind::Writes), 'U'),
        (has(RelKind::Deletes), 'D'),
    ]
    .into_iter()
    .filter_map(|(present, letter)| present.then_some(letter))
    .collect()
}

/// A label safe on an edge: `|` would end it and `"` would read as a node.
fn edge_label(model: &SemanticModel, node: &NodeRef) -> String {
    quote(node_label(model, node).unwrap_or_default()).replace('|', "/")
}

/// The controls of each use case in scope: its APIs, and itself when it reads or writes
/// entities directly or has no API.
pub(super) fn controls_of(
    model: &SemanticModel,
    usecases: &[NodeRef],
) -> Vec<(NodeRef, Vec<NodeRef>)> {
    usecases
        .iter()
        .map(|usecase| {
            let mut own: Vec<NodeRef> = model
                .relations
                .iter()
                .filter(|rel| rel.kind == RelKind::Invokes && rel.from == *usecase)
                .map(|rel| rel.to.clone())
                .collect();
            let direct = model.relations.iter().any(|rel| {
                is_crud(&rel.kind) && rel.from == *usecase && matches!(rel.to, NodeRef::Entity(_))
            });
            if direct || own.is_empty() {
                own.push(usecase.clone());
            }
            (usecase.clone(), own)
        })
        .collect()
}

impl Emitter for RobustnessMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let usecases: Vec<NodeRef> = scoped_use_cases(model, &view.scope)
            .into_iter()
            .map(NodeRef::UseCase)
            .collect();
        let in_scope: HashSet<NodeRef> = usecases.iter().cloned().collect();
        let controls = controls_of(model, &usecases);
        let controls_for = |usecase: &NodeRef| -> Vec<NodeRef> {
            controls
                .iter()
                .find(|(owner, _)| owner == usecase)
                .map(|(_, own)| own.clone())
                .unwrap_or_default()
        };

        let id = |node: &NodeRef| contract_id(model, node);
        let mut edges = Vec::new();
        let mut nodes: HashSet<NodeRef> = HashSet::new();
        let mut connect = |from: &NodeRef, label: String, to: &NodeRef, arrow: &'static str| {
            if let (Some(from_id), Some(to_id)) = (id(from), id(to)) {
                nodes.insert(from.clone());
                nodes.insert(to.clone());
                edges.push(GraphEdge {
                    from: from_id,
                    label,
                    to: to_id,
                    arrow,
                    derived: None,
                });
            }
        };

        // control -> entity: the union of the control's operations on the entity.
        for (_, own) in &controls {
            for control in own {
                let mut per_entity: BTreeMap<String, (NodeRef, Vec<RelKind>)> = BTreeMap::new();
                for rel in &model.relations {
                    if is_crud(&rel.kind)
                        && rel.from == *control
                        && matches!(rel.to, NodeRef::Entity(_))
                    {
                        if let Some(entity_id) = id(&rel.to) {
                            per_entity
                                .entry(entity_id)
                                .or_insert_with(|| (rel.to.clone(), Vec::new()))
                                .1
                                .push(rel.kind.clone());
                        }
                    }
                }
                for (entity, kinds) in per_entity.values() {
                    connect(control, crud_letters(kinds), entity, "-->");
                }
            }
        }

        // actor -> screen, and screen -> control labelled by the use case that joins them.
        for (usecase, own) in &controls {
            let label = edge_label(model, usecase);
            let performers: Vec<NodeRef> = match usecase {
                NodeRef::UseCase(key) => rdra_ish_core::actors_for_usecase(model, *key)
                    .into_iter()
                    .map(NodeRef::Actor)
                    .collect(),
                _ => Vec::new(),
            };
            for rel in &model.relations {
                if rel.kind != RelKind::Displays || rel.from != *usecase {
                    continue;
                }
                for control in own {
                    connect(&rel.to, label.clone(), control, "-->");
                }
                for actor in &performers {
                    connect(actor, "uses".to_string(), &rel.to, "-->");
                }
            }
        }

        // An event reaches a control from the control of the use case that raises it or from
        // the external system that does; a timer reaches it as the event does.
        for rel in &model.relations {
            if rel.kind != RelKind::Triggers {
                continue;
            }
            let label = edge_label(model, &rel.from);
            let receivers = match &rel.to {
                target @ NodeRef::UseCase(_) if in_scope.contains(target) => controls_for(target),
                target @ NodeRef::ExtSystem(_) => vec![target.clone()],
                _ => continue,
            };
            let senders: Vec<NodeRef> = match &rel.from {
                timer @ NodeRef::Timer(_) => vec![timer.clone()],
                event @ NodeRef::Event(_) => model
                    .relations
                    .iter()
                    .filter(|raise| raise.kind == RelKind::Raises && raise.to == *event)
                    .flat_map(|raise| match &raise.from {
                        usecase @ NodeRef::UseCase(_) if in_scope.contains(usecase) => {
                            controls_for(usecase)
                        }
                        ext @ NodeRef::ExtSystem(_) => vec![ext.clone()],
                        _ => Vec::new(),
                    })
                    .collect(),
                _ => continue,
            };
            for sender in &senders {
                for receiver in &receivers {
                    let both_external = matches!(sender, NodeRef::ExtSystem(_))
                        && matches!(receiver, NodeRef::ExtSystem(_));
                    if !both_external {
                        connect(sender, label.clone(), receiver, "-.->");
                    }
                }
            }
        }

        // Nodes in the order actor, boundary, control, entity; each group sorted by id.
        let group = |node: &NodeRef| match node {
            NodeRef::Actor(_) => 0,
            NodeRef::Screen(_) | NodeRef::ExtSystem(_) | NodeRef::Timer(_) => 1,
            NodeRef::Api(_) | NodeRef::UseCase(_) => 2,
            _ => 3,
        };
        let mut drawn: Vec<(u8, String, NodeRef)> = nodes
            .iter()
            .filter_map(|node| Some((group(node), id(node)?, node.clone())))
            .collect();
        drawn.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));

        let mut body = String::new();
        for (_, node_id, node) in &drawn {
            body.push_str(&format!(
                "  {node_id}[\"{}\"]:::{}\n",
                quote(node_label(model, node).unwrap_or_default()),
                node_kind_filter_name(node)
            ));
        }
        render_edges(&mut body, edges);

        let nodes: Vec<NodeRef> = drawn.into_iter().map(|(_, _, node)| node).collect();
        let mut kinds = kinds_by_layer(&nodes);
        kinds.sort_by_key(|kind| {
            nodes
                .iter()
                .position(|node| node_kind_filter_name(node) == *kind)
        });
        body.push_str(&class_defs(&kinds));
        let drawn: HashSet<NodeRef> = nodes.into_iter().collect();
        let header = Header {
            diagram: "robustness",
            view,
            layers: None,
            omitted: omitted_linked_kinds(model, |node| in_scope.contains(node), &drawn, &kinds),
            kinds,
        };
        Ok(format!("{}flowchart LR\n{body}", header.render()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram_common::model_from;

    const CLINIC: &str = r#"
actor Patient "Patient"
actor Clerk "Clerk"
extsystem Bank "Bank"
buc BucPay "Pay"
usecase Pay "Pay bill"
usecase Archive "Archive bill"
usecase Reconcile "Reconcile"
usecase Remind "Remind"
screen PayScreen "Pay screen"
api PayApi "Pay API"
entity Bill "Bill" { id: Int @pk }
entity Receipt "Receipt" { id: Int @pk }
event Paid "Paid"
event Posted "Posted"
event Wire "Wire"
timer Nightly "Nightly"
performs(Patient, BucPay)
performs(Clerk, Remind)
contains(BucPay, Pay)
contains(BucPay, Archive)
contains(BucPay, Reconcile)
contains(BucPay, Remind)
displays(Pay, PayScreen)
invokes(Pay, PayApi)
creates(PayApi, Receipt)
writes(PayApi, Bill)
reads(PayApi, Bill)
updates(Archive, Bill)
raises(Pay, Paid)
triggers(Paid, Reconcile)
raises(Bank, Wire)
triggers(Wire, Archive)
raises(Archive, Posted)
triggers(Posted, Bank)
triggers(Nightly, Remind)
"#;

    fn emit(view: View) -> String {
        RobustnessMermaidEmitter
            .emit(&model_from(CLINIC), &view)
            .unwrap()
    }

    #[test]
    fn a_screen_reaches_the_api_its_use_case_invokes_and_the_api_its_entities() {
        let result = emit(View::whole());
        assert!(
            result.starts_with("%% rdra-ish diagram=robustness scope=whole\n"),
            "{result}"
        );
        for line in [
            "  actor__Patient -->|uses| screen__PayScreen\n",
            "  screen__PayScreen -->|Pay bill| api__PayApi\n",
            "  api__PayApi -->|RU| entity__Bill\n",
            "  api__PayApi -->|C| entity__Receipt\n",
        ] {
            assert!(result.contains(line), "{line}{result}");
        }
        assert!(!result.contains("usecase__Pay["), "{result}");
    }

    #[test]
    fn a_use_case_without_an_api_is_its_own_control() {
        let result = emit(View::whole());
        assert!(
            result.contains("  usecase__Archive[\"Archive bill\"]:::usecase\n"),
            "{result}"
        );
        assert!(
            result.contains("  usecase__Archive -->|U| entity__Bill\n"),
            "{result}"
        );
        // Reconcile has neither an API nor CRUD: a control of its own, with no entity edge.
        assert!(result.contains("  usecase__Reconcile["), "{result}");
        assert!(!result.contains("usecase__Reconcile -->"), "{result}");
    }

    #[test]
    fn events_and_timers_join_controls_and_boundaries() {
        let result = emit(View::whole());
        for line in [
            "  api__PayApi -.->|Paid| usecase__Reconcile\n",
            "  extsystem__Bank -.->|Wire| usecase__Archive\n",
            "  usecase__Archive -.->|Posted| extsystem__Bank\n",
            "  timer__Nightly -.->|Nightly| usecase__Remind\n",
        ] {
            assert!(result.contains(line), "{line}{result}");
        }
    }

    #[test]
    fn an_actor_reaches_only_a_screen_and_skips_a_use_case_an_event_starts() {
        let result = emit(View::whole());
        // Clerk performs Remind, which has no screen: no actor edge to draw.
        assert!(!result.contains("actor__Clerk"), "{result}");
        for line in result.lines().filter(|l| l.trim().starts_with("actor__")) {
            if line.contains("-->") {
                assert!(line.contains("| screen__"), "{line}");
            }
        }
    }

    #[test]
    fn a_scope_draws_the_use_cases_it_names() {
        let one = emit(View::usecases(vec!["Archive".to_string()]));
        assert!(one.starts_with("%% rdra-ish diagram=robustness scope=usecase:Archive\n"));
        assert!(
            one.contains("usecase__Archive") && !one.contains("api__PayApi"),
            "{one}"
        );
        assert!(!one.contains("usecase__Reconcile"), "{one}");
        assert!(emit(View::bucs(vec!["BucPay".to_string()])).contains("api__PayApi"));
    }
}
