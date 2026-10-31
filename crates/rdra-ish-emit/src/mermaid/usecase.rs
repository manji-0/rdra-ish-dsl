//! The UC複合図 (`--kind usecase`): each use case with the actors who perform it, the screens
//! it displays and what they show, the information it reads and writes, the events, external
//! systems, and timers around it, the conditions that apply to it, and the states it moves.

use super::{
    buc_members, kinds_by_layer, omitted_linked_kinds, render_edges, render_layered_nodes,
    state_key, EdgeSet,
};
use crate::diagram_common::{class_defs, node_id, Header};
use crate::{collect_object_graph_nodes, object_graph_rel_label, EmitError, Emitter, Scope, View};
use rdra_ish_core::model::{NodeRef, RelKind, Relation, SemanticModel, UseCaseKey};
use std::collections::HashSet;

pub struct UsecaseMermaidEmitter;

/// One edge the diagram draws: declared when `via` is `None`, else derived through `via`.
struct Fact {
    from: NodeRef,
    kind: RelKind,
    label: String,
    to: NodeRef,
    via: Option<NodeRef>,
}

impl Fact {
    fn declared(rel: &Relation) -> Self {
        Fact {
            from: rel.from.clone(),
            kind: rel.kind.clone(),
            label: object_graph_rel_label(&rel.kind).to_string(),
            to: rel.to.clone(),
            via: None,
        }
    }
}

fn is_crud(kind: &RelKind) -> bool {
    matches!(
        kind,
        RelKind::Reads | RelKind::Writes | RelKind::Creates | RelKind::Updates | RelKind::Deletes
    )
}

/// The use cases of the scope, sorted by id: every one, the named ones, or those a BUC
/// contains, directly or through a step of one of its flows.
fn scoped_use_cases(model: &SemanticModel, scope: &Scope) -> Vec<UseCaseKey> {
    let named = |ids: &[String], node: &NodeRef| {
        node_id(model, node).is_some_and(|id| ids.iter().any(|wanted| wanted == id))
    };
    let selected: Vec<NodeRef> = match scope {
        Scope::Whole => model.use_cases.keys().map(NodeRef::UseCase).collect(),
        Scope::UseCases(ids) => model
            .use_cases
            .keys()
            .map(NodeRef::UseCase)
            .filter(|node| named(ids, node))
            .collect(),
        Scope::Bucs(ids) => {
            let bucs: Vec<NodeRef> = model
                .bucs
                .keys()
                .map(NodeRef::Buc)
                .filter(|node| named(ids, node))
                .collect();
            buc_members(model, &bucs).0
        }
    };
    let mut keys: Vec<UseCaseKey> = selected
        .into_iter()
        .filter_map(|node| match node {
            NodeRef::UseCase(key) => Some(key),
            _ => None,
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    keys.sort_by_key(|key| &model.use_cases[*key].id);
    keys
}

/// The edges drawn for `usecases`, and the nodes drawn without an edge of their own (the
/// state a transition leaves).
fn facts(model: &SemanticModel, usecases: &HashSet<NodeRef>) -> (Vec<Fact>, Vec<NodeRef>) {
    let triggered: HashSet<NodeRef> = rdra_ish_core::triggered_use_cases(model)
        .into_iter()
        .map(NodeRef::UseCase)
        .collect();
    let mut facts = Vec::new();
    let mut screens = HashSet::new();
    let mut events = HashSet::new();
    for rel in &model.relations {
        let kept = match (&rel.kind, &rel.from, &rel.to) {
            (RelKind::Performs, NodeRef::Actor(_), usecase)
            | (RelKind::Triggers, NodeRef::Event(_) | NodeRef::Timer(_), usecase)
            | (RelKind::AppliesTo, NodeRef::Condition(_), usecase) => usecases.contains(usecase),
            (RelKind::Displays, usecase, NodeRef::Screen(_))
            | (RelKind::Raises, usecase, NodeRef::Event(_)) => usecases.contains(usecase),
            (kind, usecase, NodeRef::Entity(_)) if is_crud(kind) => usecases.contains(usecase),
            _ => false,
        };
        if !kept {
            continue;
        }
        match (&rel.kind, &rel.from, &rel.to) {
            (RelKind::Displays, _, screen) => {
                screens.insert(screen.clone());
            }
            (RelKind::Raises, _, event) | (RelKind::Triggers, event @ NodeRef::Event(_), _) => {
                events.insert(event.clone());
            }
            _ => {}
        }
        facts.push(Fact::declared(rel));
    }

    for rel in &model.relations {
        match (&rel.kind, &rel.from, &rel.to) {
            (RelKind::Shows, screen, NodeRef::Entity(_)) if screens.contains(screen) => {
                facts.push(Fact::declared(rel));
            }
            (RelKind::Raises, NodeRef::ExtSystem(_), event)
            | (RelKind::Triggers, event, NodeRef::ExtSystem(_))
                if events.contains(event) =>
            {
                facts.push(Fact::declared(rel));
            }
            // An actor performs each use case of a BUC or a step it performs, except one an
            // event or a timer starts.
            (RelKind::Performs, NodeRef::Actor(_), via @ (NodeRef::Buc(_) | NodeRef::Step(_))) => {
                let link = if matches!(via, NodeRef::Buc(_)) {
                    RelKind::Contains
                } else {
                    RelKind::Covers
                };
                for inner in &model.relations {
                    if inner.kind == link
                        && inner.from == *via
                        && usecases.contains(&inner.to)
                        && !triggered.contains(&inner.to)
                    {
                        facts.push(Fact {
                            to: inner.to.clone(),
                            via: Some(via.clone()),
                            ..Fact::declared(rel)
                        });
                    }
                }
            }
            // A use case reads and writes what each API it invokes reads and writes.
            (RelKind::Invokes, usecase, api @ NodeRef::Api(_)) if usecases.contains(usecase) => {
                for inner in &model.relations {
                    if is_crud(&inner.kind)
                        && inner.from == *api
                        && matches!(inner.to, NodeRef::Entity(_))
                    {
                        facts.push(Fact {
                            from: usecase.clone(),
                            via: Some(api.clone()),
                            ..Fact::declared(inner)
                        });
                    }
                }
            }
            _ => {}
        }
    }

    let mut left = Vec::new();
    for transition in &model.state_transitions {
        let usecase = NodeRef::UseCase(transition.usecase);
        let (Some(entity), true) = (
            model.entities.get(transition.entity),
            usecases.contains(&usecase),
        ) else {
            continue;
        };
        let Some(to) = state_key(model, transition.entity, &transition.to) else {
            continue;
        };
        facts.push(Fact {
            from: usecase,
            kind: RelKind::Transitions,
            label: format!(
                "transitions {}.{} {}->{}",
                entity.id, transition.column, transition.from, transition.to
            ),
            to: NodeRef::State(to),
            via: None,
        });
        left.extend(state_key(model, transition.entity, &transition.from).map(NodeRef::State));
    }
    (facts, left)
}

impl Emitter for UsecaseMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let usecases: HashSet<NodeRef> = scoped_use_cases(model, &view.scope)
            .into_iter()
            .map(NodeRef::UseCase)
            .collect();
        let (facts, left) = facts(model, &usecases);

        let mut edges = EdgeSet::default();
        for fact in facts {
            edges.add(
                model,
                (&fact.from, &fact.kind, fact.label, &fact.to),
                fact.via.as_ref(),
            );
        }
        let mut drawn: HashSet<NodeRef> = usecases.clone();
        drawn.extend(left);
        drawn.extend(edges.nodes.iter().cloned());
        let nodes = collect_object_graph_nodes(model, &|nr| drawn.contains(nr));

        let mut body = String::new();
        let layers = render_layered_nodes(&mut body, model, &nodes);
        render_edges(&mut body, edges.into_graph_edges());

        let kinds = kinds_by_layer(&nodes);
        body.push_str(&class_defs(&kinds));
        let header = Header {
            diagram: "usecase",
            view,
            layers: Some(layers),
            omitted: omitted_linked_kinds(
                model,
                |nr| matches!(nr, NodeRef::UseCase(_) | NodeRef::Screen(_)) && drawn.contains(nr),
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
buc BucPay "Pay"
flow PayFlow "Pay flow"
step Settle "Settle"
usecase Capture "Capture"
usecase Refund "Refund"
usecase Browse "Browse"
usecase Reconcile "Reconcile"
usecase Remind "Remind"
event Settled "Settled"
timer Nightly "Nightly"
api CaptureApi "Capture API"
screen ResultScreen "Result"
entity Order "Order" {
  id: Int @pk
  status: Enum(awaiting, paid) @default(awaiting)
}
entity Payment "Payment" { id: Int @pk }
performs(Customer, BucPay)
performs(Clerk, Settle)
performs(Clerk, Refund)
contains(BucPay, Capture)
contains(BucPay, Reconcile)
contains(BucPay, Remind)
triggers(Settled, Reconcile)
triggers(Nightly, Remind)
performs(Clerk, Remind)
contains(BucPay, PayFlow)
contains(PayFlow, Settle)
covers(Settle, Refund)
invokes(Capture, CaptureApi)
updates(CaptureApi, Payment)
updates(CaptureApi, Order)
updates(Capture, Order)
displays(Capture, ResultScreen)
shows(ResultScreen, Payment)
transitions(Order.status, Capture, awaiting -> paid)
"#;

    fn emit(view: View) -> String {
        UsecaseMermaidEmitter
            .emit(&model_from(SHOP), &view)
            .unwrap()
    }

    #[test]
    fn an_actor_of_the_buc_performs_its_use_cases_by_derivation() {
        let result = emit(View::whole());
        assert!(
            result.contains(
                "  actor__Customer -.->|performs| usecase__Capture\n  %% derived actor__Customer -> usecase__Capture via buc__BucPay\n"
            ),
            "{result}"
        );
        // Declared and derived through the step: the declaration wins.
        assert!(
            result.contains("  actor__Clerk -.->|performs| usecase__Refund\n  actor__Clerk"),
            "{result}"
        );
        assert!(!result.contains("via step__Settle"), "{result}");
    }

    #[test]
    fn a_use_case_an_event_or_timer_starts_is_performed_only_as_declared() {
        let result = emit(View::whole());
        // Nobody performs Reconcile: the event starts it, and the BUC's actor does not.
        assert!(
            !result.contains("|performs| usecase__Reconcile"),
            "{result}"
        );
        // A declared performer of a timer-started use case stays, and the BUC adds no other.
        assert!(
            result.contains("  actor__Clerk -.->|performs| usecase__Remind\n"),
            "{result}"
        );
        assert!(
            !result.contains("actor__Customer -.->|performs| usecase__Remind"),
            "{result}"
        );
    }

    #[test]
    fn the_crud_of_an_invoked_api_is_drawn_from_the_use_case() {
        let result = emit(View::whole());
        assert!(
            result.contains(
                "  usecase__Capture -.->|updates| entity__Payment\n  %% derived usecase__Capture -> entity__Payment via api__CaptureApi\n"
            ),
            "{result}"
        );
        // Declared on the use case as well: one edge, not derived.
        assert!(
            result.contains("  usecase__Capture -.->|updates| entity__Order\n  usecase__Capture"),
            "{result}"
        );
        assert!(!result.contains("api__CaptureApi["), "{result}");
        assert!(result.contains("%% omitted: buc step api\n"), "{result}");
    }

    #[test]
    fn a_transition_names_its_state_model_and_draws_both_states() {
        let result = emit(View::whole());
        assert!(
            result.contains(
                "  usecase__Capture -.->|transitions Order.status awaiting->paid| state__Order_paid\n"
            ),
            "{result}"
        );
        assert!(result.contains("    state__Order_awaiting[\"awaiting\"]:::state\n"));
        assert!(result.contains("  usecase__Capture -.->|displays| screen__ResultScreen\n"));
        assert!(result.contains("  screen__ResultScreen -.->|shows| entity__Payment\n"));
    }

    #[test]
    fn a_scope_draws_the_use_cases_it_names() {
        let buc = emit(View::bucs(vec!["BucPay".to_string()]));
        assert!(buc.starts_with("%% rdra-ish diagram=usecase scope=buc:BucPay\n"));
        assert!(buc.contains("usecase__Capture[") && buc.contains("usecase__Refund["));
        assert!(!buc.contains("usecase__Browse"), "{buc}");

        let one = emit(View::usecases(vec!["Refund".to_string()]));
        assert!(one.starts_with("%% rdra-ish diagram=usecase scope=usecase:Refund\n"));
        assert!(one.contains("%% kinds: actor usecase\n"), "{one}");
        assert!(!one.contains("usecase__Capture"), "{one}");
    }
}
