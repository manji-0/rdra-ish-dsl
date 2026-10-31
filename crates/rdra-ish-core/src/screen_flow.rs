//! How a user moves between screens: the transitions `navigates` declares, and where none is
//! declared, the order a business flow uses the screens in. The second is not a transition the
//! UI offers, only the order the work visits the screens, so callers keep the two apart.

use crate::model::{FlowKey, NodeRef, RelKind, ScreenKey, SemanticModel, StepKey, UseCaseKey};

/// `navigates(from, to)`, with the use case of its `.via(...)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Navigation {
    pub from: ScreenKey,
    pub to: ScreenKey,
    pub via: Option<UseCaseKey>,
}

/// Two consecutive steps of `flow` in one actor's lane: a use case `from_step` covers displays
/// `from`, and one `to_step` covers displays `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusinessOrder {
    pub from: ScreenKey,
    pub to: ScreenKey,
    pub flow: FlowKey,
    pub from_step: StepKey,
    pub to_step: StepKey,
}

/// Every `navigates`, in declaration order.
pub fn screen_navigations(model: &SemanticModel) -> Vec<Navigation> {
    model
        .relations
        .iter()
        .filter(|rel| rel.kind == RelKind::Navigates)
        .filter_map(|rel| match (&rel.from, &rel.to) {
            (NodeRef::Screen(from), NodeRef::Screen(to)) => Some(Navigation {
                from: *from,
                to: *to,
                via: match rel.via {
                    Some(NodeRef::UseCase(usecase)) => Some(usecase),
                    _ => None,
                },
            }),
            _ => None,
        })
        .collect()
}

/// The screen order the business flows imply where `navigates` says nothing. For each
/// `precedes`, `branches`, or `excepts` between two steps of one flow that the same single
/// actor performs (the steps `diagram --kind flow` puts in one lane), each screen the first
/// step's use cases display is followed by each screen the second step's display. A screen
/// followed by itself, and a pair some `navigates` already declares in that direction, are
/// left out. `repeats` goes back, not on, so it adds nothing.
pub fn business_order(model: &SemanticModel) -> Vec<BusinessOrder> {
    let related = |kind: RelKind, from: &NodeRef| -> Vec<NodeRef> {
        model
            .relations
            .iter()
            .filter(|rel| rel.kind == kind && rel.from == *from)
            .map(|rel| rel.to.clone())
            .collect()
    };
    let lane = |step: &NodeRef| -> Option<NodeRef> {
        let mut actors: Vec<&NodeRef> = Vec::new();
        for rel in &model.relations {
            if rel.kind == RelKind::Performs
                && rel.to == *step
                && matches!(rel.from, NodeRef::Actor(_))
                && !actors.contains(&&rel.from)
            {
                actors.push(&rel.from);
            }
        }
        match actors.as_slice() {
            [actor] => Some((*actor).clone()),
            _ => None,
        }
    };
    let screens = |step: &NodeRef| -> Vec<ScreenKey> {
        let mut screens = Vec::new();
        for usecase in related(RelKind::Covers, step) {
            for screen in related(RelKind::Displays, &usecase) {
                if let NodeRef::Screen(key) = screen {
                    if !screens.contains(&key) {
                        screens.push(key);
                    }
                }
            }
        }
        screens
    };
    let declared = screen_navigations(model);
    let flows_of = |step: &NodeRef| -> Vec<FlowKey> {
        model
            .relations
            .iter()
            .filter(|rel| rel.kind == RelKind::Contains && rel.to == *step)
            .filter_map(|rel| match rel.from {
                NodeRef::Flow(flow) => Some(flow),
                _ => None,
            })
            .collect()
    };

    let mut order = Vec::new();
    for rel in model.relations.iter().filter(|rel| {
        matches!(
            rel.kind,
            RelKind::Precedes | RelKind::Branches | RelKind::Excepts
        )
    }) {
        let (NodeRef::Step(from_step), NodeRef::Step(to_step)) = (&rel.from, &rel.to) else {
            continue;
        };
        let (Some(from_lane), Some(to_lane)) = (lane(&rel.from), lane(&rel.to)) else {
            continue;
        };
        if from_lane != to_lane {
            continue;
        }
        let to_flows = flows_of(&rel.to);
        let Some(flow) = flows_of(&rel.from)
            .into_iter()
            .find(|flow| to_flows.contains(flow))
        else {
            continue;
        };
        for from in screens(&rel.from) {
            for to in screens(&rel.to) {
                let navigated = declared.iter().any(|n| n.from == from && n.to == to);
                let link = BusinessOrder {
                    from,
                    to,
                    flow,
                    from_step: *from_step,
                    to_step: *to_step,
                };
                if from != to && !navigated && !order.contains(&link) {
                    order.push(link);
                }
            }
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        model
    }

    const SHOP: &str = r#"
actor Clerk "Clerk"
actor Courier "Courier"
usecase Find "Find"
usecase Pick "Pick"
usecase Pack "Pack"
usecase Ship "Ship"
screen SearchScreen "Search"
screen PickScreen "Pick"
screen PackScreen "Pack"
screen ShipScreen "Ship"
displays(Find, SearchScreen)
displays(Pick, PickScreen)
displays(Pick, SearchScreen)
displays(Pack, PackScreen)
displays(Ship, ShipScreen)
flow ShipFlow "Ship"
step FindStep "Find"
step PickStep "Pick"
step PackStep "Pack"
step ShipStep "Ship"
contains(ShipFlow, FindStep)
contains(ShipFlow, PickStep)
contains(ShipFlow, PackStep)
contains(ShipFlow, ShipStep)
covers(FindStep, Find)
covers(PickStep, Pick)
covers(PackStep, Pack)
covers(ShipStep, Ship)
precedes(FindStep, PickStep)
precedes(PickStep, PackStep)
precedes(PackStep, ShipStep)
repeats(PackStep, PickStep)
performs(Clerk, FindStep)
performs(Clerk, PickStep)
performs(Clerk, PackStep)
performs(Courier, ShipStep)
"#;

    fn screen_pairs(model: &SemanticModel, order: &[BusinessOrder]) -> Vec<(String, String)> {
        order
            .iter()
            .map(|link| {
                (
                    model.screens[link.from].id.clone(),
                    model.screens[link.to].id.clone(),
                )
            })
            .collect()
    }

    #[test]
    fn consecutive_steps_in_one_lane_order_their_screens() {
        let model = model_from(SHOP);
        let order = business_order(&model);
        // Find -> Pick gives Search -> Pick (Search -> Search is left out); Pick -> Pack gives
        // Pick -> Pack and Search -> Pack; Pack -> Ship crosses lanes; repeats goes back.
        assert_eq!(
            screen_pairs(&model, &order),
            [
                ("SearchScreen".to_string(), "PickScreen".to_string()),
                ("PickScreen".to_string(), "PackScreen".to_string()),
                ("SearchScreen".to_string(), "PackScreen".to_string()),
            ]
        );
        let first = order[0];
        assert_eq!(model.flows[first.flow].id, "ShipFlow");
        assert_eq!(model.steps[first.from_step].id, "FindStep");
        assert_eq!(model.steps[first.to_step].id, "PickStep");
    }

    #[test]
    fn a_declared_navigation_replaces_the_business_order_in_its_direction() {
        let model = model_from(&format!(
            "{SHOP}navigates(PickScreen, PackScreen).via(Pack)\nnavigates(PackScreen, SearchScreen)\n"
        ));
        assert_eq!(
            screen_pairs(&model, &business_order(&model)),
            [
                ("SearchScreen".to_string(), "PickScreen".to_string()),
                ("SearchScreen".to_string(), "PackScreen".to_string()),
            ]
        );
        let navigations = screen_navigations(&model);
        assert_eq!(navigations.len(), 2);
        assert_eq!(
            navigations[0].via.map(|uc| model.use_cases[uc].id.as_str()),
            Some("Pack")
        );
        assert_eq!(navigations[1].via, None);
    }

    #[test]
    fn a_step_two_actors_perform_sits_in_no_lane() {
        let model = model_from(&format!("{SHOP}performs(Courier, PickStep)\n"));
        assert_eq!(
            screen_pairs(&model, &business_order(&model)),
            Vec::<(String, String)>::new()
        );
    }
}
