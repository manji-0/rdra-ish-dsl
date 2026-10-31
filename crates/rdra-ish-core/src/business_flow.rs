//! How BUCs connect and who takes part in them: the hand-offs between BUCs through events,
//! and each actor's and external system's part in a BUC. Both read declared relations only.

use crate::model::{BucKey, EventKey, NodeRef, RelKind, SemanticModel};
use std::collections::BTreeSet;

/// The use cases and steps that `bucs` contain: a use case directly or through a step of one
/// of their flows, and each step of their flows.
pub fn buc_members(model: &SemanticModel, bucs: &[NodeRef]) -> (Vec<NodeRef>, Vec<NodeRef>) {
    let contained = |parents: &[NodeRef], link: RelKind| -> Vec<NodeRef> {
        model
            .relations
            .iter()
            .filter(|rel| rel.kind == link && parents.contains(&rel.from))
            .map(|rel| rel.to.clone())
            .collect()
    };
    let direct = contained(bucs, RelKind::Contains);
    let steps: Vec<NodeRef> = contained(&direct, RelKind::Contains)
        .into_iter()
        .filter(|node| matches!(node, NodeRef::Step(_)))
        .collect();
    let usecases = direct
        .into_iter()
        .chain(contained(&steps, RelKind::Covers))
        .filter(|node| matches!(node, NodeRef::UseCase(_)))
        .collect();
    (usecases, steps)
}

/// A BUC that starts another: a use case contained in `from` raises `event`, and `event`
/// triggers a use case contained in `to` or `to` itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Handoff {
    pub from: BucKey,
    pub to: BucKey,
    pub event: EventKey,
}

/// Every hand-off between two different BUCs, once per `(from, to, event)`, sorted by the ids
/// of the three. A hand-off that no event carries (one BUC's output feeding another through
/// data only) is not here.
pub fn buc_handoffs(model: &SemanticModel) -> Vec<Handoff> {
    let members: Vec<(BucKey, Vec<NodeRef>)> = model
        .bucs
        .keys()
        .map(|buc| {
            let (usecases, _) = buc_members(model, &[NodeRef::Buc(buc)]);
            (buc, usecases)
        })
        .collect();
    let raised_by = |buc: &[NodeRef], event: &NodeRef| {
        model
            .relations
            .iter()
            .any(|rel| rel.kind == RelKind::Raises && rel.to == *event && buc.contains(&rel.from))
    };
    let mut handoffs = BTreeSet::new();
    for trigger in model
        .relations
        .iter()
        .filter(|rel| rel.kind == RelKind::Triggers && matches!(rel.from, NodeRef::Event(_)))
    {
        let NodeRef::Event(event) = trigger.from else {
            continue;
        };
        for (from, from_usecases) in &members {
            if !raised_by(from_usecases, &trigger.from) {
                continue;
            }
            for (to, to_usecases) in &members {
                let starts = match &trigger.to {
                    NodeRef::Buc(target) => target == to,
                    usecase @ NodeRef::UseCase(_) => to_usecases.contains(usecase),
                    _ => false,
                };
                if starts && from != to {
                    handoffs.insert(Handoff {
                        from: *from,
                        to: *to,
                        event,
                    });
                }
            }
        }
    }
    let mut handoffs: Vec<Handoff> = handoffs.into_iter().collect();
    handoffs.sort_by_key(|h| {
        (
            model.bucs[h.from].id.clone(),
            model.bucs[h.to].id.clone(),
            model.events[h.event].id.clone(),
        )
    });
    handoffs
}

/// How an actor or an external system takes part in a BUC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Involvement {
    /// An actor performs the BUC, a use case, or a step in it.
    Performs,
    /// An external system raises an event that starts a use case in the BUC, or the BUC.
    SendsEvent,
    /// An event a use case in the BUC raises is delivered to the external system.
    ReceivesEvent,
}

/// The actors and external systems that take part in `buc`, each with the ways it does.
pub fn buc_participants(
    model: &SemanticModel,
    buc: BucKey,
) -> Vec<(NodeRef, BTreeSet<Involvement>)> {
    let buc = NodeRef::Buc(buc);
    let (usecases, steps) = buc_members(model, std::slice::from_ref(&buc));
    let triggers = |event: &NodeRef, target: &NodeRef| {
        model
            .relations
            .iter()
            .any(|rel| rel.kind == RelKind::Triggers && rel.from == *event && rel.to == *target)
    };
    let mut found: Vec<(NodeRef, Involvement)> = Vec::new();
    for rel in &model.relations {
        match (&rel.kind, &rel.from, &rel.to) {
            (RelKind::Performs, actor @ NodeRef::Actor(_), target)
                if *target == buc || usecases.contains(target) || steps.contains(target) =>
            {
                found.push((actor.clone(), Involvement::Performs));
            }
            (RelKind::Raises, extsystem @ NodeRef::ExtSystem(_), event)
                if triggers(event, &buc) || usecases.iter().any(|uc| triggers(event, uc)) =>
            {
                found.push((extsystem.clone(), Involvement::SendsEvent));
            }
            (RelKind::Raises, usecase, event) if usecases.contains(usecase) => {
                for delivery in model.relations.iter().filter(|delivery| {
                    delivery.kind == RelKind::Triggers && delivery.from == *event
                }) {
                    if matches!(delivery.to, NodeRef::ExtSystem(_)) {
                        found.push((delivery.to.clone(), Involvement::ReceivesEvent));
                    }
                }
            }
            _ => {}
        }
    }
    let mut participants: Vec<(NodeRef, BTreeSet<Involvement>)> = Vec::new();
    for (node, involvement) in found {
        match participants.iter_mut().find(|(seen, _)| *seen == node) {
            Some((_, set)) => {
                set.insert(involvement);
            }
            None => participants.push((node, BTreeSet::from([involvement]))),
        }
    }
    participants
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOP: &str = r#"
actor Clerk "Clerk"
actor Idle "Idle"
extsystem Gateway "Gateway"
extsystem Carrier "Carrier"
buc BucPay "Pay"
buc BucShip "Ship"
buc BucBook "Book"
flow ShipFlow "Ship flow"
step Handover "Handover"
usecase Capture "Capture"
usecase Dispatch "Dispatch"
event Authorized "Authorized"
event Paid "Paid"
event Shipped "Shipped"
contains(BucPay, Capture)
contains(BucShip, ShipFlow)
contains(ShipFlow, Handover)
covers(Handover, Dispatch)
performs(Clerk, Handover)
raises(Gateway, Authorized)
triggers(Authorized, Capture)
raises(Capture, Paid)
triggers(Paid, BucShip)
raises(Dispatch, Shipped)
triggers(Shipped, Carrier)
"#;

    fn model() -> SemanticModel {
        let (ast, _) = rdra_ish_syntax::parse(SHOP);
        crate::build_model(&ast).0
    }

    fn id(model: &SemanticModel, node: &NodeRef) -> String {
        match node {
            NodeRef::Actor(k) => model.actors[*k].id.clone(),
            NodeRef::ExtSystem(k) => model.ext_systems[*k].id.clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_hand_off_needs_an_event_between_two_bucs() {
        let model = model();
        let found: Vec<(&str, &str, &str)> = buc_handoffs(&model)
            .iter()
            .map(|h| {
                (
                    model.bucs[h.from].id.as_str(),
                    model.bucs[h.to].id.as_str(),
                    model.events[h.event].id.as_str(),
                )
            })
            .collect();
        assert_eq!(found, [("BucPay", "BucShip", "Paid")]);
    }

    #[test]
    fn participants_are_the_actors_and_external_systems_in_a_buc() {
        let model = model();
        let buc = |name: &str| model.bucs.iter().find(|(_, b)| b.id == name).unwrap().0;
        let of = |name: &str| -> Vec<(String, Vec<Involvement>)> {
            buc_participants(&model, buc(name))
                .into_iter()
                .map(|(node, set)| (id(&model, &node), set.into_iter().collect()))
                .collect()
        };
        assert_eq!(
            of("BucPay"),
            [("Gateway".to_string(), vec![Involvement::SendsEvent])]
        );
        let mut ship = of("BucShip");
        ship.sort();
        assert_eq!(
            ship,
            [
                ("Carrier".to_string(), vec![Involvement::ReceivesEvent]),
                ("Clerk".to_string(), vec![Involvement::Performs]),
            ]
        );
        assert!(of("BucBook").is_empty());
    }
}
