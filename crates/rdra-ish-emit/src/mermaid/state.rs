//! The state transition diagram (`--kind state`).

use crate::diagram_common::{contract_id, lifecycle_start_states, quote, Diagram, Header};
use crate::{EmitError, Explored, OnDeadlock, Scope, StatesEmitter, View};
use rdra_ish_core::analysis::transition_guard_labels;
use rdra_ish_core::model::{ColumnType, NodeRef, SemanticModel};
use rdra_ish_core::{state_id, state_key, state_models, StateModel};
use std::collections::{BTreeSet, HashSet};

pub struct StateMermaidEmitter;

/// Every state model with a transition in scope, sorted by entity and column.
fn state_models_in_scope<'a>(
    model: &'a SemanticModel,
    in_scope: &HashSet<NodeRef>,
) -> Vec<StateModel<'a>> {
    let mut models = state_models(model);
    models.retain(|sm| {
        sm.transitions
            .iter()
            .any(|t| in_scope.contains(&NodeRef::UseCase(t.usecase)))
    });
    models
}

impl StatesEmitter for StateMermaidEmitter {
    fn on_deadlock(&self) -> OnDeadlock {
        OnDeadlock::Draw
    }

    fn emit(
        &self,
        model: &SemanticModel,
        view: &View,
        explored: &Explored,
    ) -> Result<String, EmitError> {
        // A BUC scope draws each state model one of its use cases moves, whole.
        let in_scope: HashSet<NodeRef> = match &view.scope {
            Scope::Bucs(buc_ids) => rdra_ish_core::reachable_from_bucs(model, buc_ids),
            Scope::Whole | Scope::UseCases(_) => model
                .use_cases
                .keys()
                .map(NodeRef::UseCase)
                .chain(model.entities.keys().map(NodeRef::Entity))
                .collect(),
        };
        let mut models = state_models_in_scope(model, &in_scope);
        let entities = &view.state.entities;
        if !entities.is_empty() {
            models.retain(|sm| entities.iter().any(|id| id == sm.entity_id));
        }
        // The whole model is an index; a BUC or an entity scope already narrows it to full models.
        let index = matches!(view.scope, Scope::Whole | Scope::UseCases(_)) && entities.is_empty();
        let marks = explored.marks();
        let starts = lifecycle_start_states(
            model,
            models.iter().map(|sm| (sm.entity, sm.column.to_string())),
            |entity, column, variant| state_key(model, entity, column, variant),
        );

        let mut body = String::new();
        let mut classes = String::new();
        let mut used_classes: Vec<&'static str> = Vec::new();
        let mut kinds: Vec<&'static str> = Vec::new();
        let mut add_kind = |kind: &'static str| {
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        };
        // Some drawn `@end` state has a transition out, so the legend names `@end`.
        let mut declared_end_leaves = false;
        for sm in &models {
            let mark = |set: &BTreeSet<(String, String, String)>, v: &str| {
                set.contains(&(
                    sm.entity_id.to_string(),
                    sm.column.to_string(),
                    v.to_string(),
                ))
            };
            // A value no transition names has no State node; an unreachable one is still drawn.
            let state_id = |variant: &str| {
                state_key(model, sm.entity, sm.column, variant)
                    .and_then(|sk| contract_id(model, &NodeRef::State(sk)))
                    .or_else(|| {
                        mark(&marks.unreachable, variant).then(|| {
                            format!("state__{}", state_id(model, sm.entity, sm.column, variant))
                        })
                    })
            };
            let mut arms: Vec<(&str, &str, String, String)> = sm
                .transitions
                .iter()
                .filter_map(|t| {
                    let usecase = contract_id(model, &NodeRef::UseCase(t.usecase))?;
                    Some((
                        t.from.as_str(),
                        t.to.as_str(),
                        usecase,
                        transition_label(model, t),
                    ))
                })
                .collect();
            arms.sort_by(|a, b| {
                (sm.position(a.0), sm.position(a.1), &a.2).cmp(&(
                    sm.position(b.0),
                    sm.position(b.1),
                    &b.2,
                ))
            });
            arms.dedup();
            let start_variants: Vec<&str> = sm
                .variants
                .iter()
                .copied()
                .filter(|v| {
                    state_key(model, sm.entity, sm.column, v).is_some_and(|sk| starts.contains(&sk))
                })
                .collect();
            let drawn: Vec<&str> = sm
                .variants
                .iter()
                .copied()
                .filter(|v| {
                    start_variants.contains(v)
                        || arms.iter().any(|(from, to, ..)| from == v || to == v)
                        || mark(&marks.unreachable, v)
                })
                .collect();

            add_kind("statemodel");
            body.push_str(&format!(
                "  state \"{}.{}\" as statemodel__{}_{} {{\n",
                sm.entity_id, sm.column, sm.entity_id, sm.column
            ));
            if index {
                add_kind("state");
                body.push_str(&format!(
                    "    state \"{} 状態 / {} 遷移 (--entity {})\" as index__{}_{}\n  }}\n",
                    sm.variants.len(),
                    arms.len(),
                    sm.entity_id,
                    sm.entity_id,
                    sm.column
                ));
                continue;
            }
            for variant in &start_variants {
                if let Some(id) = state_id(variant) {
                    body.push_str(&format!("    [*] --> {id}\n"));
                }
            }
            for variant in &drawn {
                if let Some(id) = state_id(variant) {
                    add_kind("state");
                    let label = state_key(model, sm.entity, sm.column, variant)
                        .map_or(*variant, |key| model.states[key].label.as_str());
                    body.push_str(&format!("    state \"{}\" as {id}\n", quote(label)));
                }
            }
            for (from, to, usecase, label) in &arms {
                if let (Some(from), Some(to)) = (state_id(from), state_id(to)) {
                    add_kind("usecase");
                    body.push_str(&format!("    %% transition {usecase}\n"));
                    body.push_str(&format!("    {from} --> {to} : {label}\n"));
                }
            }
            // An end state has no transition to another state, or `@end` declares it one.
            let declared_ends = model.entities[sm.entity]
                .columns
                .iter()
                .find(|c| c.name == sm.column)
                .map_or(&[][..], |c| c.end_values.as_slice());
            for variant in &drawn {
                let declared = declared_ends.iter().any(|v| v == variant);
                let moves_on = arms
                    .iter()
                    .any(|(from, to, ..)| from == variant && to != variant);
                declared_end_leaves |= declared && moves_on;
                let leaves = moves_on && !declared;
                let Some(id) = state_id(variant) else {
                    continue;
                };
                let unreachable = mark(&marks.unreachable, variant);
                if !leaves && !unreachable {
                    body.push_str(&format!("    {id} --> [*]\n"));
                }
                let class = if mark(&marks.deadlock, variant) {
                    Some("statedeadlock")
                } else if unreachable {
                    Some("stateunreachable")
                } else if !leaves {
                    Some("stateend")
                } else {
                    None
                };
                if let Some(class) = class {
                    classes.push_str(&format!("  class {id} {class}\n"));
                    if !used_classes.contains(&class) {
                        used_classes.push(class);
                    }
                }
            }
            body.push_str("  }\n");
        }

        // An Enum column no `transitions` drive is no state model. One a `sets` writes is still
        // a state axis of `states` and `check`, so it is named apart from one that never moves.
        let (mut unmodeled, mut set_only): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
        let mut all_entities: Vec<_> = model.entities.iter().collect();
        all_entities.sort_by_key(|(_, e)| &e.id);
        for (key, entity) in all_entities {
            if !in_scope.contains(&NodeRef::Entity(key))
                || (!entities.is_empty() && !entities.contains(&entity.id))
            {
                continue;
            }
            for column in &entity.columns {
                if !matches!(column.col_type, ColumnType::Enum(_))
                    || model.is_lifecycle_column(key, &column.name)
                {
                    continue;
                }
                let name = format!("{}.{}", entity.id, column.name);
                let set = model
                    .column_effects
                    .iter()
                    .any(|e| e.entity == key && e.column == column.name);
                match set {
                    true => set_only.push(name),
                    false => unmodeled.push(name),
                }
            }
        }
        let omitted = if unmodeled.is_empty() {
            Vec::new()
        } else {
            vec![format!(
                "{} (Enum without transitions)",
                unmodeled.join(" ")
            )]
        };
        let header = Header {
            diagram: Diagram::State,
            view,
            layers: None,
            kinds,
            omitted,
        };
        let set_only = if set_only.is_empty() {
            String::new()
        } else {
            format!(
                "%% set-only: {} (Enum a `sets` writes and no `transitions` drive: a state axis of check, not a state model)\n",
                set_only.join(" ")
            )
        };
        let legend = match (index, declared_end_leaves) {
            (true, _) => "",
            (false, false) => "%% legend: stateunreachable = no reachable state holds it, statedeadlock = an instance cannot leave it, stateend = no transition out\n",
            (false, true) => "%% legend: stateunreachable = no reachable state holds it, statedeadlock = an instance cannot leave it, stateend = no transition out, or declared by @end\n",
        };
        let defs: String = [
            (
                "stateunreachable",
                "fill:#fdd,stroke:#c00,stroke-dasharray:4 2,color:#600",
            ),
            ("statedeadlock", "fill:#fd9,stroke:#d60,color:#630"),
            ("stateend", "fill:#dfd,stroke:#383,color:#141"),
        ]
        .iter()
        .filter(|(name, _)| used_classes.contains(name))
        .map(|(name, style)| format!("  classDef {name} {style}\n"))
        .collect();
        Ok(format!(
            "{}{set_only}{legend}stateDiagram-v2\n{body}{defs}{classes}",
            header.render()
        ))
    }
}

/// A transition's label: the use case's label, then `[...]` with the label of each condition
/// that guards it, the use case's and the arm's own `.when` (UML guard notation).
fn transition_label(model: &SemanticModel, t: &rdra_ish_core::StateTransition) -> String {
    let guards = transition_guard_labels(model, t);
    let mut label = quote(&model.use_cases[t.usecase].label);
    let mut together: Vec<String> = model
        .state_transitions
        .iter()
        .filter(|o| o.usecase == t.usecase && o.entity == t.entity && o.column != t.column)
        .map(|o| format!("{}.{}", model.entities[o.entity].id, o.column))
        .collect();
    together.sort();
    together.dedup();
    if !together.is_empty() {
        // One step of the use case moves every column it names, and runs only where each has an arm.
        label.push_str(&format!(" ({} と同時)", together.join(", ")));
    }
    if guards.is_empty() {
        label
    } else {
        format!("{label} [{}]", quote(&guards.join(", ")))
    }
}
