//! The variation and condition diagram (`--kind condition`): each condition with the
//! variations and state models it combines and what it applies to, and each variation with
//! its values.

use super::{buc_scope, declare, guard_edge, omitted_linked_kinds, render_edges, GraphEdge};
use crate::diagram_common::{class_defs, contract_id, desc_line, node_line, quote, Header};
use crate::{collect_object_graph_nodes, node_kind_filter_name, EmitError, Emitter, Scope, View};
use rdra_ish_core::analysis::{
    chosen_variation, condition_body_display, condition_edges, ConditionEdge,
};
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
        out.push_str(&node_line(
            "    ",
            &format!("{id}__{value}"),
            &quote(label),
            "value",
        ));
    }
    out.push_str("  end\n");
}

impl Emitter for ConditionMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let targets_in_scope = match &view.scope {
            Scope::Bucs(ids) => Some(buc_scope(model, ids)),
            Scope::Whole | Scope::UseCases(_) => None,
        };
        let visible = |nr: &NodeRef| targets_in_scope.as_ref().is_none_or(|set| set.contains(nr));
        let conditions = collect_object_graph_nodes(model, &|nr| {
            matches!(nr, NodeRef::Condition(_))
                && (targets_in_scope.is_none()
                    || model.relations.iter().any(|rel| {
                        matches!(rel.kind, RelKind::AppliesTo | RelKind::Guards)
                            && rel.from == *nr
                            && visible(&rel.to)
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
                // The state model of a `.when` arm is derived from the arm, the variation of a
                // `.when` value from its `chooses`.
                let from_when = match &edge {
                    ConditionEdge::StateModel { entity, column } => {
                        model.state_transitions.iter().any(|t| {
                            t.guard == Some(*key) && t.entity == *entity && t.column == *column
                        })
                    }
                    ConditionEdge::Variation(vk) => model.column_choices.iter().any(|c| {
                        c.values.iter().any(|v| v.guard == Some(*key))
                            && chosen_variation(model, c) == Some(*vk)
                    }),
                };
                if let Some(to) = to {
                    edges.push(GraphEdge {
                        from: id.clone(),
                        label: "combines".to_string(),
                        to,
                        arrow: "-.->",
                        derived: match from_when {
                            true => Some("from when".to_string()),
                            false => derived.clone(),
                        },
                    });
                }
            }
            // What it applies to, and the use cases whose arm or value its `.when` guards.
            for rel in &model.relations {
                if rel.from != *condition || !visible(&rel.to) {
                    continue;
                }
                let edge = match rel.kind {
                    RelKind::AppliesTo => contract_id(model, &rel.to).map(|to| GraphEdge {
                        from: id.clone(),
                        label: "applies_to".to_string(),
                        to,
                        arrow: "-.->",
                        derived: None,
                    }),
                    RelKind::Guards => guard_edge(model, rel),
                    _ => None,
                };
                if let Some(edge) = edge {
                    targets.insert(rel.to.clone());
                    edges.push(edge);
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
            body.push_str(&node_line("  ", id, &quote(label), "statemodel"));
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
        render_edges(&mut body, "condition", edges);
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
transitions(CheckIn, Appointment.status, booked -> arrived)
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
                "  condition__Booked -.-> statemodel__Appointment_status\n  %% derived condition__Booked -> statemodel__Appointment_status from holds\n"
            ),
            "{result}"
        );
        assert!(
            result.contains(
                "  condition__Capped -.-> statemodel__Appointment_status\n  condition__Capped -.-> variation__Ratio\n  classDef"
            ),
            "{result}"
        );
        assert!(result.contains("  %% holds condition__Booked: Appointment.status == booked\n"));
        assert!(result
            .contains("  statemodel__Appointment_status[[\"Appointment.status\"]]:::statemodel\n"));
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
            "  condition__Capped -.-> api__BillApi\n",
        ] {
            assert!(result.contains(edge), "{edge}: {result}");
        }
        assert!(result.contains("  api__BillApi{{\"Bill API\"}}:::api\n"));
    }

    #[test]
    fn a_when_guard_is_a_derived_edge_to_the_use_case_naming_what_it_guards() {
        let src = r#"
variation Scope "Scope" {
  free
  limited
}
entity Notice "Notice" {
  id: Int @pk
  kind: Enum(first, last) @default(first)
  amendment: Scope
  status: Enum(open, drafted, closed) @default(open)
}
usecase Draft "Draft"
condition FirstOnly "First notice"
holds(FirstOnly, Notice.kind == first)
transitions(Draft, Notice.status, open -> drafted).when(FirstOnly)
transitions(Draft, Notice.status, open -> closed)
chooses(Draft, Notice.amendment, free).when(FirstOnly)
chooses(Draft, Notice.amendment, limited)
"#;
        let result = ConditionMermaidEmitter
            .emit(&model_from(src), &View::whole())
            .unwrap();
        assert!(
            result.contains(
                "  condition__FirstOnly -.->|when| usecase__Draft\n  %% derived condition__FirstOnly -> usecase__Draft via transitions Notice.status open->drafted, chooses Notice.amendment free\n"
            ),
            "{result}"
        );
        assert!(
            result.contains("  usecase__Draft(\"Draft\"):::usecase\n"),
            "{result}"
        );
    }

    #[test]
    fn a_buc_scope_draws_the_conditions_of_its_use_cases_and_what_they_combine() {
        let pay = emit(View::bucs(vec!["BucPay".to_string()]));
        assert!(pay.starts_with("%% rdra-ish diagram=condition scope=buc:BucPay\n"));
        assert!(pay.contains("subgraph variation__Ratio["), "{pay}");
        assert!(pay.contains("condition__Booked{") && pay.contains("condition__Capped{"));
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
