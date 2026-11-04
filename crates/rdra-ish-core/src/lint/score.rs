//! Coverage score and stage readiness findings.

use std::collections::HashMap;

use crate::model::{ColumnType, EntityKey, NodeRef, SemanticModel, StateKey};
use crate::refinement::{assess, Depth, Next};

use super::{LintIssue, LintSeverity};

/// Whether any element of each model layer exists. The coverage score subtracts the missing
/// ones; `refinement` decides the stage of each BUC.
fn model_layers(model: &SemanticModel) -> [(&'static str, bool); 6] {
    [
        (
            "scope",
            !model.actors.is_empty() && !model.bucs.is_empty() && !model.use_cases.is_empty(),
        ),
        (
            "business-flow",
            !model.bucs.is_empty() && (!model.flows.is_empty() || !model.use_cases.is_empty()),
        ),
        ("data", !model.entities.is_empty()),
        (
            "interaction",
            !model.screens.is_empty() || !model.apis.is_empty(),
        ),
        (
            "system-boundary",
            !model.systems.is_empty() && !model.apis.is_empty(),
        ),
        (
            "rules",
            !model.requirements.is_empty()
                || !model.nfrs.is_empty()
                || !model.constraints.is_empty()
                || !model.forbidden_constraints.is_empty()
                || !model.entity_invariants.is_empty()
                || !model.required_constraints.is_empty()
                || !model.exclusive_constraints.is_empty()
                || !model.cross_forbidden_constraints.is_empty()
                || !model.cross_entity_invariants.is_empty(),
        ),
    ]
}

pub(super) fn coverage_score_issue(model: &SemanticModel, findings: &[LintIssue]) -> LintIssue {
    let elements = model_element_count(model);
    if elements == 0 {
        return LintIssue::new(
            LintSeverity::Info,
            "coverage-score",
            "model",
            "",
            "coverage score: 0/100 across 0 model elements and 0 findings (empty model)",
            "add actors/BUCs/entities before treating the score as a readiness signal; run `rdra-ish next` for the first stage",
        );
    }

    let finding_penalty: i32 = findings
        .iter()
        .map(|issue| match issue.severity {
            LintSeverity::Error => 15,
            LintSeverity::Warning => {
                if issue.rule == "semantic-warning" || issue.rule == "consistency-warning" {
                    4
                } else {
                    2
                }
            }
            LintSeverity::Info => 0,
        })
        .sum();
    let missing_layers = model_layers(model).iter().filter(|(_, ok)| !*ok).count() as i32;
    let layer_penalty = missing_layers.saturating_mul(8);
    // Scale absolute finding penalties by model size so large models are not
    // pinned at 0 while tiny stubs score near 100 for the same density.
    let scaled_finding = (finding_penalty * 25) / elements.max(1) as i32;
    let score = (100 - scaled_finding - layer_penalty).max(0);
    LintIssue::new(
        LintSeverity::Info,
        "coverage-score",
        "model",
        "",
        format!(
            "coverage score: {}/100 across {} model elements and {} findings (finding penalty scaled ×25/elements; {} missing layers × -8)",
            score,
            elements,
            findings.len(),
            missing_layers
        ),
        "use warning rows as the review backlog and `rdra-ish next` for each BUC's next stage; the score is a size-normalized readiness signal",
    )
}

pub(super) fn stage_readiness_issue(model: &SemanticModel) -> LintIssue {
    let standings = assess(model, Depth::Structure).bucs;
    let message = if standings.is_empty() {
        "no BUC yet; next stage: 0 Scope sketch".to_string()
    } else {
        standings
            .iter()
            .map(|p| {
                let stage = p.reached().map_or("-".to_string(), |stage| stage.number());
                match p.next() {
                    Next::Required(view) => {
                        format!("{} stage {stage} (next {})", p.id, view.id.number())
                    }
                    Next::Optional(_) => {
                        format!("{} stage {stage} (required stages complete)", p.id)
                    }
                }
            })
            .collect::<Vec<_>>()
            .join("; ")
    };
    LintIssue::new(
        LintSeverity::Info,
        "stage-readiness",
        "model",
        "",
        message,
        "run `rdra-ish next` for each BUC's gaps, the questions to ask, and the commands that review the next stage",
    )
}

fn model_element_count(model: &SemanticModel) -> usize {
    model.actors.len()
        + model.ext_systems.len()
        + model.systems.len()
        + model.requirements.len()
        + model.goals.len()
        + model.nfrs.len()
        + model.qualities.len()
        + model.constraints.len()
        + model.businesses.len()
        + model.bucs.len()
        + model.flows.len()
        + model.steps.len()
        + model.use_cases.len()
        + model.screens.len()
        + model.fields.len()
        + model.events.len()
        + model.entities.len()
        + model.states.len()
        + model.apis.len()
        + model.dtos.len()
        + model.timers.len()
        + model.media.len()
        + model.permissions.len()
        + model.variations.len()
        + model.conditions.len()
}

pub(super) fn relation_degree(model: &SemanticModel) -> HashMap<NodeRef, usize> {
    let mut degree = HashMap::new();
    let mut touch = |node: NodeRef| {
        *degree.entry(node).or_insert(0) += 1;
    };

    for rel in &model.relations {
        touch(rel.from.clone());
        touch(rel.to.clone());
    }
    for mapping in &model.field_mappings {
        touch(NodeRef::Field(mapping.field));
        touch(NodeRef::Entity(mapping.entity));
    }
    let state_by_value: HashMap<(EntityKey, &str, &str), StateKey> = model
        .states
        .iter()
        .map(|(key, s)| ((s.entity, s.column.as_str(), s.value.as_str()), key))
        .collect();
    for transition in &model.state_transitions {
        touch(NodeRef::UseCase(transition.usecase));
        touch(NodeRef::Entity(transition.entity));
        for value in [&transition.from, &transition.to] {
            let at = (
                transition.entity,
                transition.column.as_str(),
                value.as_str(),
            );
            if let Some(&state) = state_by_value.get(&at) {
                touch(NodeRef::State(state));
            }
        }
    }
    for entity in model.entities.values() {
        for column in &entity.columns {
            if let ColumnType::Variation(key) = column.col_type {
                touch(NodeRef::Variation(key));
            }
        }
    }
    for coordination in &model.boundary_coordinations {
        touch(NodeRef::UseCase(coordination.usecase));
        touch(NodeRef::Entity(coordination.left));
        touch(NodeRef::Entity(coordination.right));
    }
    for effect in &model.column_effects {
        touch(NodeRef::UseCase(effect.origin));
        touch(NodeRef::Entity(effect.entity));
    }
    for choice in &model.column_choices {
        touch(NodeRef::UseCase(choice.origin));
        touch(NodeRef::Entity(choice.entity));
    }
    for effect in &model.proposition_effects {
        touch(NodeRef::UseCase(effect.origin));
        touch(NodeRef::Entity(effect.entity));
    }
    for constraint in &model.forbidden_constraints {
        touch(NodeRef::Entity(constraint.entity));
    }
    for invariant in &model.entity_invariants {
        touch(NodeRef::Entity(invariant.entity));
    }
    for constraint in &model.required_constraints {
        touch(NodeRef::Entity(constraint.entity));
    }
    for constraint in &model.exclusive_constraints {
        touch(NodeRef::Entity(constraint.entity));
    }
    for constraint in &model.cross_forbidden_constraints {
        for entity in &constraint.scope {
            touch(NodeRef::Entity(*entity));
        }
    }
    for invariant in &model.cross_entity_invariants {
        for entity in &invariant.scope {
            touch(NodeRef::Entity(*entity));
        }
    }
    for assertion in &model.temporal_assertions {
        touch(NodeRef::UseCase(assertion.anchor));
        for entity in &assertion.scope {
            touch(NodeRef::Entity(*entity));
        }
    }
    for constraint in &model.quantifier_constraints {
        touch(NodeRef::Entity(constraint.anchor));
        touch(NodeRef::Entity(constraint.quantifier.related));
    }
    for event in &model.outbox_events {
        touch(NodeRef::Event(*event));
    }
    for exposure in &model.publication.exposures {
        touch(NodeRef::System(exposure.system));
        touch(exposure.item.target());
    }
    for share in &model.publication.shares {
        touch(NodeRef::System(share.system));
        touch(NodeRef::Entity(share.entity));
    }

    degree
}
