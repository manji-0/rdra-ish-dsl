//! Flow-cycle and structural-coverage lint rules.

use crate::model::{NodeRef, RelKind, SemanticModel, StepKey};

use super::{push_element_lint, LintIssue};

pub(super) fn lint_flow_cycles(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    use std::collections::{HashMap, HashSet};

    let mut adj: HashMap<StepKey, Vec<StepKey>> = HashMap::new();
    for rel in &model.relations {
        if !matches!(rel.kind, RelKind::Precedes | RelKind::Branches) {
            continue;
        }
        if let (NodeRef::Step(from), NodeRef::Step(to)) = (&rel.from, &rel.to) {
            adj.entry(*from).or_default().push(*to);
            adj.entry(*to).or_default();
        }
    }
    if adj.is_empty() {
        return;
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Color {
        White,
        Gray,
        Black,
    }
    let mut color: HashMap<StepKey, Color> = adj.keys().map(|k| (*k, Color::White)).collect();
    let mut reported = HashSet::new();

    fn dfs(
        model: &SemanticModel,
        node: StepKey,
        adj: &HashMap<StepKey, Vec<StepKey>>,
        color: &mut HashMap<StepKey, Color>,
        stack: &mut Vec<StepKey>,
        reported: &mut HashSet<StepKey>,
        issues: &mut Vec<LintIssue>,
    ) {
        color.insert(node, Color::Gray);
        stack.push(node);
        for &next in adj.get(&node).into_iter().flatten() {
            match color.get(&next).copied().unwrap_or(Color::White) {
                Color::Gray => {
                    // Cycle: report each step once.
                    let cycle_start = stack.iter().position(|s| *s == next).unwrap_or(0);
                    for &step in &stack[cycle_start..] {
                        if reported.insert(step) {
                            push_element_lint(
                                model,
                                issues,
                                "step",
                                "step",
                                &model.steps[step].id,
                                "flow-cycle",
                                "business step participates in a precedes/branches cycle",
                                "remove or redirect precedes/branches edges so the flow graph is acyclic",
                            );
                        }
                    }
                }
                Color::White => dfs(model, next, adj, color, stack, reported, issues),
                Color::Black => {}
            }
        }
        stack.pop();
        color.insert(node, Color::Black);
    }

    let mut keys: Vec<_> = adj.keys().copied().collect();
    keys.sort_by_key(|k| model.steps[*k].id.as_str());
    let mut stack = Vec::new();
    for key in keys {
        if color.get(&key).copied() == Some(Color::White) {
            dfs(
                model,
                key,
                &adj,
                &mut color,
                &mut stack,
                &mut reported,
                issues,
            );
        }
    }
}

pub(super) fn lint_structural_coverage(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    for (buc_key, buc) in &model.bucs {
        let has_child = model.relations.iter().any(|rel| {
            rel.kind == RelKind::Contains
                && rel.from == NodeRef::Buc(buc_key)
                && matches!(rel.to, NodeRef::UseCase(_) | NodeRef::Flow(_))
        });
        if !has_child {
            push_element_lint(
                model,
                issues,
                "buc",
                "buc",
                &buc.id,
                "buc-empty",
                "BUC contains no use case or business flow",
                "add contains(Buc, UseCase) or contains(Buc, Flow)",
            );
        }
    }

    for (flow_key, flow) in &model.flows {
        let has_step = model.relations.iter().any(|rel| {
            rel.kind == RelKind::Contains
                && rel.from == NodeRef::Flow(flow_key)
                && matches!(rel.to, NodeRef::Step(_))
        });
        if !has_step {
            push_element_lint(
                model,
                issues,
                "flow",
                "flow",
                &flow.id,
                "flow-empty",
                "business flow has no steps",
                "add contains(Flow, Step) for each business step",
            );
        }
    }

    for (step_key, step) in &model.steps {
        let covers_model_element = model.relations.iter().any(|rel| {
            rel.kind == RelKind::Covers
                && rel.from == NodeRef::Step(step_key)
                && matches!(
                    rel.to,
                    NodeRef::UseCase(_) | NodeRef::Api(_) | NodeRef::Event(_)
                )
        });
        if !covers_model_element {
            push_element_lint(
                model,
                issues,
                "step",
                "step",
                &step.id,
                "step-no-cover",
                "business step is not anchored to a use case, API, or event",
                "add covers(Step, UseCase), covers(Step, Api), or covers(Step, Event)",
            );
        }
    }

    for (api_key, api) in &model.apis {
        let invoked = model
            .relations
            .iter()
            .any(|rel| rel.kind == RelKind::Invokes && rel.to == NodeRef::Api(api_key));
        if !invoked {
            push_element_lint(
                model,
                issues,
                "api",
                "api",
                &api.id,
                "api-unused",
                "API is not invoked by any use case",
                "add invokes(UseCase, Api) or remove the API from this model slice",
            );
        }

        if api.method.is_some() ^ api.path.is_some() {
            push_element_lint(
                model,
                issues,
                "api",
                "api",
                &api.id,
                "api-contract-incomplete",
                "API contract declares only one of method/path",
                "declare both method and path before exporting OpenAPI",
            );
        } else if api.method.is_none() && api.path.is_none() {
            push_element_lint(
                model,
                issues,
                "api",
                "api",
                &api.id,
                "api-contract-missing",
                "API has no method/path; OpenAPI export will omit it",
                "declare method and path when OpenAPI projection is in scope",
            );
        }
    }

    for (dto_key, dto) in &model.dtos {
        let used = model.relations.iter().any(|rel| {
            matches!(
                rel.kind,
                RelKind::Request | RelKind::Response | RelKind::ErrorResponse
            ) && rel.to == NodeRef::Dto(dto_key)
        });
        if !used {
            push_element_lint(
                model,
                issues,
                "dto",
                "dto",
                &dto.id,
                "dto-unused",
                "DTO is not referenced by any API request or response",
                "add request(Api, Dto), response(Api, Dto), or error_response(Api, Dto)",
            );
        }
    }

    for (field_key, field) in &model.fields {
        let contained = model.relations.iter().any(|rel| {
            rel.kind == RelKind::Contains
                && rel.to == NodeRef::Field(field_key)
                && matches!(rel.from, NodeRef::Screen(_))
        });
        if !contained {
            push_element_lint(
                model,
                issues,
                "field",
                "field",
                &field.id,
                "field-unplaced",
                "screen field is not contained by any screen",
                "add contains(Screen, Field)",
            );
        }

        let mapped = model
            .field_mappings
            .iter()
            .any(|mapping| mapping.field == field_key);
        if !mapped {
            push_element_lint(
                model,
                issues,
                "field",
                "field",
                &field.id,
                "field-unmapped",
                "screen field is not mapped to an Entity column",
                "add maps_field(Field, Entity.column) when the field has data lineage",
            );
        }
    }

    for (screen_key, screen) in &model.screens {
        let has_field = model.relations.iter().any(|rel| {
            rel.kind == RelKind::Contains
                && rel.from == NodeRef::Screen(screen_key)
                && matches!(rel.to, NodeRef::Field(_))
        });
        if !has_field {
            push_element_lint(
                model,
                issues,
                "screen",
                "screen",
                &screen.id,
                "screen-no-fields",
                "screen has no first-class fields",
                "add field declarations and contains(Screen, Field) for input/output review",
            );
        }
    }

    for (requirement_key, requirement) in &model.requirements {
        let motivates = model.relations.iter().any(|rel| {
            rel.kind == RelKind::Motivates && rel.from == NodeRef::Requirement(requirement_key)
        });
        if !motivates {
            push_element_lint(
                model,
                issues,
                "requirement",
                "requirement",
                &requirement.id,
                "requirement-untraced",
                "requirement does not motivate a BUC",
                "add motivates(Requirement, Buc) to preserve requirement traceability",
            );
        }
    }

    for (nfr_key, nfr) in &model.nfrs {
        let applies = model
            .relations
            .iter()
            .any(|rel| rel.kind == RelKind::AppliesTo && rel.from == NodeRef::Nfr(nfr_key));
        if !applies {
            push_element_lint(
                model,
                issues,
                "nfr",
                "nfr",
                &nfr.id,
                "nfr-unscoped",
                "NFR is not applied to a use case, API, or system",
                "add applies_to(Nfr, UseCase|Api|System)",
            );
        }
    }

    for (constraint_key, constraint) in &model.constraints {
        let constrains = model.relations.iter().any(|rel| {
            rel.kind == RelKind::Constrains && rel.from == NodeRef::Constraint(constraint_key)
        });
        if !constrains {
            push_element_lint(
                model,
                issues,
                "constraint",
                "constraint",
                &constraint.id,
                "constraint-unscoped",
                "constraint is not attached to a target model element",
                "add constrains(Constraint, UseCase|Api|System|Entity|Dto)",
            );
        }
    }
}
