//! The screen transitions (`--kind screen`): each screen, a solid edge for each `navigates`
//! labelled with the use case of its `.via(...)`, and where none is declared a dashed
//! 業務上の順番 edge for the order a business flow uses the screens in, one actor's lane at a
//! time. A dashed edge is derived, so a `%% derived` line names the flow and steps behind it.

use super::{omitted_linked_kinds, render_edges, GraphEdge};
use crate::diagram_common::{
    class_defs, contract_id, desc_line, edge_label, node_line, quote, Header,
};
use crate::{EmitError, Emitter, Scope, View};
use rdra_ish_core::model::{NodeRef, RelKind, SemanticModel};
use rdra_ish_core::{buc_members, business_order, screen_navigations};
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub struct ScreenMermaidEmitter;

/// The label of a dashed edge: the order of the work, not a move the UI offers.
const BUSINESS_ORDER: &str = "業務上の順番";

/// The screens and steps `--buc` keeps: the screens the BUCs' use cases display and the steps
/// of their flows. `None` keeps everything.
fn scoped(model: &SemanticModel, scope: &Scope) -> Option<(HashSet<NodeRef>, HashSet<NodeRef>)> {
    let Scope::Bucs(ids) = scope else {
        return None;
    };
    let bucs: Vec<NodeRef> = model
        .bucs
        .iter()
        .filter(|(_, buc)| ids.contains(&buc.id))
        .map(|(key, _)| NodeRef::Buc(key))
        .collect();
    let (usecases, steps) = buc_members(model, &bucs);
    let screens = model
        .relations
        .iter()
        .filter(|rel| rel.kind == RelKind::Displays && usecases.contains(&rel.from))
        .map(|rel| rel.to.clone())
        .collect();
    Some((screens, steps.into_iter().collect()))
}

impl Emitter for ScreenMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let scope = scoped(model, &view.scope);
        let visible = |node: &NodeRef| scope.as_ref().is_none_or(|(s, _)| s.contains(node));
        let step_kept = |node: &NodeRef| scope.as_ref().is_none_or(|(_, s)| s.contains(node));

        let mut screens: Vec<(String, NodeRef)> = model
            .screens
            .keys()
            .map(NodeRef::Screen)
            .filter(|node| visible(node))
            .filter_map(|node| Some((contract_id(model, &node)?, node)))
            .collect();
        screens.sort_by(|a, b| a.0.cmp(&b.0));

        let mut body = String::new();
        for (id, node) in &screens {
            if let Some(label) = model.node_label(node) {
                body.push_str(&node_line("  ", id, &quote(label), "screen"));
                if let Some(desc) = desc_line(model, node, id) {
                    body.push_str(&format!("  {desc}\n"));
                }
            }
        }

        let mut edges = Vec::new();
        let mut vias = BTreeSet::new();
        for navigation in screen_navigations(model) {
            let (from, to) = (
                NodeRef::Screen(navigation.from),
                NodeRef::Screen(navigation.to),
            );
            if !visible(&from) || !visible(&to) {
                continue;
            }
            let (Some(from), Some(to)) = (contract_id(model, &from), contract_id(model, &to))
            else {
                continue;
            };
            let via = navigation.via.map(NodeRef::UseCase);
            let label = match via.as_ref().and_then(|uc| model.node_label(uc)) {
                Some(label) => edge_label(label),
                None => "navigates".to_string(),
            };
            if let Some(via) = via.as_ref().and_then(|uc| contract_id(model, uc)) {
                vias.insert(format!("%% via {from} -> {to} {via}"));
            }
            edges.push(GraphEdge {
                from,
                label,
                to,
                arrow: "-->",
                derived: None,
            });
        }

        let mut ordered: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        for link in business_order(model) {
            let (from, to) = (NodeRef::Screen(link.from), NodeRef::Screen(link.to));
            let (from_step, to_step) = (NodeRef::Step(link.from_step), NodeRef::Step(link.to_step));
            if !visible(&from) || !visible(&to) || !step_kept(&from_step) || !step_kept(&to_step) {
                continue;
            }
            let ids = [
                contract_id(model, &from),
                contract_id(model, &to),
                contract_id(model, &NodeRef::Flow(link.flow)),
                contract_id(model, &from_step),
                contract_id(model, &to_step),
            ];
            let [Some(from), Some(to), Some(flow), Some(from_step), Some(to_step)] = ids else {
                continue;
            };
            ordered
                .entry((from, to))
                .or_default()
                .extend([flow, from_step, to_step]);
        }
        edges.extend(ordered.into_iter().map(|((from, to), through)| GraphEdge {
            from,
            label: BUSINESS_ORDER.to_string(),
            to,
            arrow: "-.->",
            derived: Some(format!(
                "via {}",
                through.into_iter().collect::<Vec<_>>().join(",")
            )),
        }));
        render_edges(&mut body, "screen", edges);
        for via in vias {
            body.push_str(&format!("  {via}\n"));
        }

        let kinds = if screens.is_empty() {
            Vec::new()
        } else {
            vec!["screen"]
        };
        body.push_str(&class_defs(&kinds));
        let drawn: HashSet<NodeRef> = screens.into_iter().map(|(_, node)| node).collect();
        let omitted = omitted_linked_kinds(
            model,
            |node| matches!(node, NodeRef::Screen(_)) && drawn.contains(node),
            &drawn,
            &kinds,
        );
        let header = Header {
            diagram: "screen",
            view,
            layers: None,
            kinds,
            omitted,
        };
        Ok(format!("{}flowchart LR\n{body}", header.render()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram_common::model_from;

    const SHOP: &str = r#"
actor Clerk "Clerk"
actor Courier "Courier"
buc BucPick "Pick"
buc BucShip "Ship"
usecase Find "Find"
usecase Pick "Pick"
usecase Pack "Pack"
usecase Ship "Ship"
screen SearchScreen "Search" description "Finds the order"
screen PickScreen "Pick"
screen PackScreen "Pack"
screen ShipScreen "Ship"
screen KioskScreen "Kiosk"
contains(BucPick, Find)
contains(BucPick, Pick)
contains(BucPick, Pack)
contains(BucShip, Ship)
displays(Find, SearchScreen)
displays(Pick, PickScreen)
displays(Pack, PackScreen)
displays(Ship, ShipScreen)
navigates(PickScreen, PackScreen).via(Pack)
navigates(PackScreen, ShipScreen)
flow PickFlow "Pick"
step FindStep "Find"
step PickStep "Pick"
step PackStep "Pack"
contains(BucPick, PickFlow)
contains(PickFlow, FindStep)
contains(PickFlow, PickStep)
contains(PickFlow, PackStep)
covers(FindStep, Find)
covers(PickStep, Pick)
covers(PackStep, Pack)
precedes(FindStep, PickStep)
precedes(PickStep, PackStep)
performs(Clerk, FindStep)
performs(Clerk, PickStep)
performs(Clerk, PackStep)
"#;

    #[test]
    fn navigates_is_solid_and_the_business_order_is_dashed_and_derived() {
        let model = model_from(SHOP);
        let out = ScreenMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(
            out.starts_with("%% rdra-ish diagram=screen scope=whole\n%% kinds: screen\n"),
            "{out}"
        );
        assert!(out.contains("flowchart LR\n"), "{out}");
        assert!(
            out.contains("  screen__KioskScreen[\"Kiosk\"]:::screen\n"),
            "{out}"
        );
        assert!(
            out.contains("  %% desc screen__SearchScreen: Finds the order\n"),
            "{out}"
        );
        // Declared: labelled with the via use case, or `navigates` without one.
        assert!(
            out.contains("  screen__PickScreen -->|Pack| screen__PackScreen\n"),
            "{out}"
        );
        assert!(
            out.contains("  %% via screen__PickScreen -> screen__PackScreen usecase__Pack\n"),
            "{out}"
        );
        assert!(
            out.contains("  screen__PackScreen -->|navigates| screen__ShipScreen\n"),
            "{out}"
        );
        // Derived: Search -> Pick from Find -> Pick; Pick -> Pack is declared already.
        assert!(
            out.contains(
                "  screen__SearchScreen -.->|業務上の順番| screen__PickScreen\n  %% derived screen__SearchScreen -> screen__PickScreen via flow__PickFlow,step__FindStep,step__PickStep\n"
            ),
            "{out}"
        );
        assert_eq!(out.matches("-.->").count(), 1, "{out}");
        assert!(out.contains("  classDef screen "), "{out}");
        assert!(out.contains("%% omitted: usecase\n"), "{out}");
    }

    #[test]
    fn a_buc_scope_keeps_the_screens_its_use_cases_display() {
        let model = model_from(SHOP);
        let out = ScreenMermaidEmitter
            .emit(&model, &View::bucs(vec!["BucPick".to_string()]))
            .unwrap();
        assert!(out.contains("scope=buc:BucPick"), "{out}");
        for kept in ["SearchScreen", "PickScreen", "PackScreen"] {
            assert!(out.contains(&format!("  screen__{kept}[")), "{kept}\n{out}");
        }
        for dropped in ["ShipScreen", "KioskScreen"] {
            assert!(
                !out.contains(&format!("screen__{dropped}")),
                "{dropped}\n{out}"
            );
        }
        assert!(out.contains("-.->|業務上の順番|"), "{out}");
    }

    #[test]
    fn a_model_without_screens_draws_an_empty_chart() {
        let model = model_from("usecase Find \"Find\"\n");
        let out = ScreenMermaidEmitter.emit(&model, &View::whole()).unwrap();
        assert!(out.contains("%% kinds: (none)\n"), "{out}");
        assert!(!out.contains("classDef"), "{out}");
    }
}
