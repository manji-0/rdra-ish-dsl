//! The variation and condition diagram (`--kind condition`): each condition with the
//! variations and state models it combines and what it applies to, and each variation with
//! its values.

use super::{buc_members, declare, omitted_linked_kinds, render_edges, GraphEdge};
use crate::diagram_common::{class_defs, contract_id, desc_line, quote, Header};
use crate::{collect_object_graph_nodes, node_kind_filter_name, EmitError, Emitter, Scope, View};
use rdra_ish_core::analysis::{condition_body_display, condition_edges, ConditionEdge};
use rdra_ish_core::model::{NodeRef, RelKind, SemanticModel, VariationKey};
use std::collections::{BTreeMap, HashSet};

pub struct ConditionMermaidEmitter;

/// The node id and label of a state model, as `--kind state` and `--kind information` give them.
fn state_model(model: &SemanticModel, edge: &ConditionEdge) -> Option<(String, String)> {
    let ConditionEdge::StateModel { entity, column } = edge else {
        return None;
    };
    let entity = &model.entities.get(*entity)?.id;
    Some((
        format!("statemodel__{entity}_{column}"),
        format!("{entity}.{column}"),
    ))
}

/// What a condition of the BUCs `ids` applies to: the use cases the BUCs contain, directly or
/// through a step of one of their flows, and the APIs those use cases invoke.
fn buc_targets(model: &SemanticModel, ids: &[String]) -> HashSet<NodeRef> {
    let bucs: Vec<NodeRef> = model
        .bucs
        .iter()
        .filter(|(_, buc)| ids.contains(&buc.id))
        .map(|(key, _)| NodeRef::Buc(key))
        .collect();
    let mut targets: HashSet<NodeRef> = buc_members(model, &bucs).0.into_iter().collect();
    let apis: Vec<NodeRef> = model
        .relations
        .iter()
        .filter(|rel| rel.kind == RelKind::Invokes && targets.contains(&rel.from))
        .map(|rel| rel.to.clone())
        .collect();
    targets.extend(apis);
    targets
}

/// The variation `id` as a subgraph holding one `:::value` node per value.
fn render_variation(out: &mut String, model: &SemanticModel, key: VariationKey, id: &str) {
    let Some(variation) = model.variations.get(key) else {
        return;
    };
    out.push_str(&format!(
        "  subgraph {id}[\"{}\"]\n",
        quote(&variation.label)
    ));
    if let Some(desc) = desc_line(model, &NodeRef::Variation(key), id) {
        out.push_str(&format!("    {desc}\n"));
    }
    for (value, label) in variation.values.iter().zip(&variation.labels) {
        let label = label.as_deref().unwrap_or(value);
        out.push_str(&format!(
            "    {id}__{value}[\"{}\"]:::value\n",
            quote(label)
        ));
    }
    out.push_str("  end\n");
}

impl Emitter for ConditionMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let targets_in_scope = match &view.scope {
            Scope::Bucs(ids) => Some(buc_targets(model, ids)),
            Scope::Whole | Scope::UseCases(_) => None,
        };
        let visible = |nr: &NodeRef| targets_in_scope.as_ref().is_none_or(|set| set.contains(nr));
        let conditions = collect_object_graph_nodes(model, &|nr| {
            matches!(nr, NodeRef::Condition(_))
                && (targets_in_scope.is_none()
                    || model.relations.iter().any(|rel| {
                        rel.kind == RelKind::AppliesTo && rel.from == *nr && visible(&rel.to)
                    }))
        });

        let mut edges = Vec::new();
        let mut state_models = BTreeMap::new();
        let mut variations: HashSet<VariationKey> = HashSet::new();
        let mut targets: HashSet<NodeRef> = HashSet::new();
        for condition in &conditions {
            let (NodeRef::Condition(key), Some(id)) = (condition, contract_id(model, condition))
            else {
                continue;
            };
            let derived = model.conditions[*key]
                .body
                .is_some()
                .then(|| "from holds".to_string());
            for edge in condition_edges(model, *key) {
                let to = match &edge {
                    ConditionEdge::Variation(variation) => {
                        variations.insert(*variation);
                        contract_id(model, &NodeRef::Variation(*variation))
                    }
                    ConditionEdge::StateModel { .. } => {
                        state_model(model, &edge).map(|(to, label)| {
                            state_models.insert(to.clone(), label);
                            to
                        })
                    }
                };
                if let Some(to) = to {
                    edges.push(GraphEdge {
                        from: id.clone(),
                        label: "combines".to_string(),
                        to,
                        arrow: "-.->",
                        derived: derived.clone(),
                    });
                }
            }
            for rel in &model.relations {
                if rel.kind != RelKind::AppliesTo || rel.from != *condition || !visible(&rel.to) {
                    continue;
                }
                if let Some(to) = contract_id(model, &rel.to) {
                    targets.insert(rel.to.clone());
                    edges.push(GraphEdge {
                        from: id.clone(),
                        label: "applies_to".to_string(),
                        to,
                        arrow: "-.->",
                        derived: None,
                    });
                }
            }
        }
        // The whole model is the variation sheet: every variation, a condition's or not.
        if targets_in_scope.is_none() {
            variations.extend(model.variations.keys());
        }

        let mut body = String::new();
        let mut kinds: Vec<&'static str> = Vec::new();
        for condition in &conditions {
            declare(&mut body, model, condition, "  ");
            if let (NodeRef::Condition(key), Some(id)) = (condition, contract_id(model, condition))
            {
                if let Some(holds) = condition_body_display(model, *key) {
                    body.push_str(&format!("  %% holds {id}: {holds}\n"));
                }
            }
        }
        if !conditions.is_empty() {
            kinds.push("condition");
        }
        for (id, label) in &state_models {
            body.push_str(&format!("  {id}[\"{}\"]:::statemodel\n", quote(label)));
        }
        if !state_models.is_empty() {
            kinds.push("statemodel");
        }
        let variation_nodes = collect_object_graph_nodes(
            model,
            &|nr| matches!(nr, NodeRef::Variation(key) if variations.contains(key)),
        );
        for variation in &variation_nodes {
            if let (NodeRef::Variation(key), Some(id)) = (variation, contract_id(model, variation))
            {
                render_variation(&mut body, model, *key, &id);
            }
        }
        if !variation_nodes.is_empty() {
            kinds.push("variation");
        }
        if variation_nodes.iter().any(
            |nr| matches!(nr, NodeRef::Variation(key) if !model.variations[*key].values.is_empty()),
        ) {
            kinds.push("value");
        }
        let targets = collect_object_graph_nodes(model, &|nr| targets.contains(nr));
        for target in &targets {
            declare(&mut body, model, target, "  ");
            let kind = node_kind_filter_name(target);
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
        render_edges(&mut body, edges);
        body.push_str(&class_defs(&kinds));

        // A state model stands for its entity, which the diagram does not draw itself.
        let mut drawn: HashSet<NodeRef> = conditions.iter().cloned().collect();
        drawn.extend(variation_nodes.iter().cloned());
        drawn.extend(targets.iter().cloned());
        drawn.extend(
            model
                .relations
                .iter()
                .filter(|rel| rel.kind == RelKind::Combines && matches!(rel.to, NodeRef::Entity(_)))
                .map(|rel| rel.to.clone()),
        );
        let header = Header {
            diagram: "condition",
            view,
            layers: None,
            omitted: omitted_linked_kinds(
                model,
                |nr| matches!(nr, NodeRef::Condition(_)) && drawn.contains(nr),
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

    const CLINIC: &str = r#"
variation Ratio "Burden ratio" { r1 "10%"  r3 "30%" }
variation Channel "Channel" { web  store }
variation Unused "Unused" { a "A" }
buc BucPay "Pay"
buc BucOther "Other"
usecase Bill "Bill"
usecase CheckIn "Check in"
usecase Elsewhere "Elsewhere"
api BillApi "Bill API"
entity Appointment "Appointment" {
  id: Int @pk
  status: Enum(booked, arrived) @default(booked)
  ratio: Ratio
}
entity Order "Order" {
  id: Int @pk
  channel: Channel
}
transitions(Appointment.status, CheckIn, booked -> arrived)
contains(BucPay, Bill)
contains(BucPay, CheckIn)
contains(BucOther, Elsewhere)
updates(CheckIn, Appointment)
reads(Elsewhere, Order)
condition Capped "Capped at the limit"
combines(Capped, Ratio)
combines(Capped, Appointment.status)
applies_to(Capped, Bill)
applies_to(Capped, BillApi)
condition Booked "Booked patient"
holds(Booked, Appointment.status == booked)
applies_to(Booked, CheckIn)
"#;

    fn emit(view: View) -> String {
        ConditionMermaidEmitter
            .emit(&model_from(CLINIC), &view)
            .unwrap()
    }

    #[test]
    fn a_link_read_from_holds_is_derived_and_a_declared_one_is_not() {
        let result = emit(View::whole());
        assert!(
            result.contains(
                "  condition__Booked -.->|combines| statemodel__Appointment_status\n  %% derived condition__Booked -> statemodel__Appointment_status from holds\n"
            ),
            "{result}"
        );
        assert!(
            result.contains(
                "  condition__Capped -.->|combines| statemodel__Appointment_status\n  condition__Capped -.->|combines| variation__Ratio\n  classDef"
            ),
            "{result}"
        );
        assert!(result.contains("  %% holds condition__Booked: Appointment.status == booked\n"));
        assert!(result
            .contains("  statemodel__Appointment_status[\"Appointment.status\"]:::statemodel\n"));
    }

    #[test]
    fn a_variation_holds_its_values_and_an_unused_one_is_drawn_too() {
        let result = emit(View::whole());
        assert!(
            result.contains(
                "  subgraph variation__Ratio[\"Burden ratio\"]\n    variation__Ratio__r1[\"10%\"]:::value\n    variation__Ratio__r3[\"30%\"]:::value\n  end\n"
            ),
            "{result}"
        );
        assert!(result.contains("    variation__Channel__web[\"web\"]:::value\n"));
        assert!(result.contains("  subgraph variation__Unused[\"Unused\"]\n"));
        assert!(
            result.starts_with(
                "%% rdra-ish diagram=condition scope=whole\n%% kinds: condition statemodel variation value usecase api\n%% omitted: (none)\nflowchart LR\n"
            ),
            "{result}"
        );
    }

    #[test]
    fn a_condition_applies_to_use_cases_and_apis() {
        let result = emit(View::whole());
        for edge in [
            "  condition__Booked -.->|applies_to| usecase__CheckIn\n",
            "  condition__Capped -.->|applies_to| usecase__Bill\n",
            "  condition__Capped -.->|applies_to| api__BillApi\n",
        ] {
            assert!(result.contains(edge), "{edge}: {result}");
        }
        assert!(result.contains("  api__BillApi[\"Bill API\"]:::api\n"));
    }

    #[test]
    fn a_buc_scope_draws_the_conditions_of_its_use_cases_and_what_they_combine() {
        let pay = emit(View::bucs(vec!["BucPay".to_string()]));
        assert!(pay.starts_with("%% rdra-ish diagram=condition scope=buc:BucPay\n"));
        assert!(pay.contains("subgraph variation__Ratio["), "{pay}");
        assert!(pay.contains("condition__Booked[") && pay.contains("condition__Capped["));
        assert!(!pay.contains("api__BillApi"), "{pay}");
        for absent in ["variation__Channel", "variation__Unused"] {
            assert!(!pay.contains(absent), "{absent}: {pay}");
        }

        let other = emit(View::bucs(vec!["BucOther".to_string()]));
        assert!(
            other.contains("%% kinds: (none)\n%% omitted: (none)\nflowchart LR\n"),
            "{other}"
        );
    }
}
