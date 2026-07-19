//! Review lint findings for model coverage, naming, and structural gaps.

use std::collections::HashMap;

use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::LocatedSpan;
use crate::model::{node_ref_kind, NodeRef, RelKind, SemanticModel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Info,
    Warning,
    Error,
}

/// One lint finding for CLI export or editor diagnostics.
#[derive(Debug, Clone)]
pub struct LintIssue {
    pub severity: LintSeverity,
    pub rule: &'static str,
    pub subject_kind: String,
    pub subject_id: String,
    pub message: String,
    pub hint: String,
    pub location: Option<LocatedSpan>,
}

impl LintIssue {
    pub fn new(
        severity: LintSeverity,
        rule: &'static str,
        subject_kind: impl Into<String>,
        subject_id: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            rule,
            subject_kind: subject_kind.into(),
            subject_id: subject_id.into(),
            message: message.into(),
            hint: hint.into(),
            location: None,
        }
    }

    pub fn with_location(mut self, location: LocatedSpan) -> Self {
        self.location = Some(location);
        self
    }

    pub fn to_diagnostic(&self) -> Diagnostic {
        let err = RdraError::LintFinding {
            rule: self.rule,
            message: self.message.clone(),
            hint: self.hint.clone(),
        };
        match (self.severity, &self.location) {
            (LintSeverity::Error, Some(loc)) => Diagnostic::error_at(err, loc.clone()),
            (LintSeverity::Error, None) => Diagnostic::error(err),
            (_, Some(loc)) => Diagnostic::warning_at(err, loc.clone()),
            (_, None) => Diagnostic::warning(err),
        }
    }
}

impl LintSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// Collect review findings that are not already emitted as semantic/consistency diagnostics.
pub fn lint_review_diagnostics(model: &SemanticModel) -> Vec<Diagnostic> {
    collect_findings(model)
        .into_iter()
        .map(|issue| issue.to_diagnostic())
        .collect()
}

/// Full lint report for `rdra-ish lint`, including semantic and consistency diagnostics.
pub fn lint_issues(model: &SemanticModel, semantic_diags: &[Diagnostic]) -> Vec<LintIssue> {
    let mut findings = Vec::new();

    for diag in semantic_diags {
        findings.push(diagnostic_to_lint_issue(diag));
    }

    for diag in super::workspace::consistency_diagnostics(model) {
        findings.push(diagnostic_to_lint_issue(&diag));
    }

    findings.extend(collect_findings(model));

    let mut issues = vec![
        coverage_score_issue(model, &findings),
        stage_readiness_issue(model),
    ];
    issues.extend(findings);
    issues
}

fn diagnostic_to_lint_issue(diag: &Diagnostic) -> LintIssue {
    let (rule, hint) = if let RdraError::LintFinding { rule, hint, .. } = &diag.error {
        (*rule, hint.as_str())
    } else if !diag.is_warning {
        (
            "semantic-error",
            "fix the DSL diagnostic before relying on derived review output",
        )
    } else if is_consistency_diag(&diag.error) {
        (
            "consistency-warning",
            "review the consistency diagnostic and add the missing relation or metadata",
        )
    } else {
        (
            "semantic-warning",
            "fix the DSL diagnostic before relying on derived review output",
        )
    };

    LintIssue {
        severity: if diag.is_warning {
            LintSeverity::Warning
        } else {
            LintSeverity::Error
        },
        rule,
        subject_kind: "model".to_string(),
        subject_id: String::new(),
        message: diag.error.to_string(),
        hint: hint.to_string(),
        location: diag.location.clone(),
    }
}

fn is_consistency_diag(err: &RdraError) -> bool {
    matches!(
        err,
        RdraError::ActorPermissionMissing { .. }
            | RdraError::ActorPermissionExcess { .. }
            | RdraError::UseCasePermissionNoActor { .. }
            | RdraError::ApiPermissionNoActor { .. }
            | RdraError::ApiNeverInvoked { .. }
            | RdraError::ApiInvokedButNoEntity { .. }
            | RdraError::EventNeverRaised { .. }
            | RdraError::EventNeverConsumed { .. }
            | RdraError::TriggeredUseCaseUnreachable { .. }
            | RdraError::SeparateTxInferred { .. }
            | RdraError::ApiOperatesEntityOutsideOwner { .. }
            | RdraError::CrossSystemEntityRelation { .. }
            | RdraError::CoordinationNotCrossSystem { .. }
            | RdraError::CoordinationMissingApi { .. }
    )
}

fn collect_findings(model: &SemanticModel) -> Vec<LintIssue> {
    let mut findings = Vec::new();
    let degree = relation_degree(model);

    for node in lint_node_refs(model) {
        let Some(id) = node_id(model, &node) else {
            continue;
        };
        let decl_kind = node_ref_kind(&node);
        let subject_kind = node_kind_display(&node);

        if degree.get(&node).copied().unwrap_or(0) == 0 {
            push_element_lint(
                model,
                &mut findings,
                decl_kind,
                &subject_kind,
                &id,
                "orphan-node",
                "model element is declared but not connected to another element",
                "connect it with a predicate, map it, or remove it if it is not part of the model",
            );
        }

        if !is_upper_camelish_id(&id) {
            push_element_lint(
                model,
                &mut findings,
                decl_kind,
                &subject_kind,
                &id,
                "naming-id",
                "element id does not follow UpperCamelCase project convention",
                "rename the id to UpperCamelCase; keep labels for human-readable text",
            );
        }
    }

    lint_structural_coverage(model, &mut findings);
    lint_column_names(model, &mut findings);
    findings
}

#[allow(clippy::too_many_arguments)]
fn push_element_lint(
    model: &SemanticModel,
    issues: &mut Vec<LintIssue>,
    decl_kind: &str,
    subject_kind: &str,
    id: &str,
    rule: &'static str,
    message: impl Into<String>,
    hint: impl Into<String>,
) {
    let mut issue = LintIssue::new(LintSeverity::Warning, rule, subject_kind, id, message, hint);
    if let Some(loc) = model.decl_sites.located(decl_kind, id) {
        issue = issue.with_location(loc);
    }
    issues.push(issue);
}

fn lint_structural_coverage(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
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
                "add maps_field(Field, Entity, \"column\") when the field has data lineage",
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

fn lint_column_names(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    for (_, entity) in &model.entities {
        for column in &entity.columns {
            if !is_snake_caseish(&column.name) {
                push_element_lint(
                    model,
                    issues,
                    "entity",
                    "entity-column",
                    &entity.id,
                    "naming-column",
                    format!(
                        "entity column '{}' does not follow snake_case project convention",
                        column.name
                    ),
                    "rename the column to snake_case or document the external naming constraint",
                );
            }
        }
    }

    for (_, dto) in &model.dtos {
        for field in &dto.fields {
            if !is_snake_caseish(&field.name) {
                push_element_lint(
                    model,
                    issues,
                    "dto",
                    "dto-field",
                    &dto.id,
                    "naming-dto-field",
                    format!(
                        "DTO field '{}' does not follow snake_case project convention",
                        field.name
                    ),
                    "rename the DTO field to snake_case or document the external contract constraint",
                );
            }
        }
    }
}

fn coverage_score_issue(model: &SemanticModel, findings: &[LintIssue]) -> LintIssue {
    let penalty: i32 = findings
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
    let score = (100 - penalty).max(0);
    LintIssue::new(
        LintSeverity::Info,
        "coverage-score",
        "model",
        "",
        format!(
            "coverage score: {}/100 across {} model elements and {} findings",
            score,
            model_element_count(model),
            findings.len()
        ),
        "use warning rows as the review backlog; the score is a lightweight readiness signal",
    )
}

fn stage_readiness_issue(model: &SemanticModel) -> LintIssue {
    let stages = [
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
    ];
    let ready: Vec<_> = stages
        .iter()
        .filter_map(|(stage, ok)| ok.then_some(*stage))
        .collect();
    let missing: Vec<_> = stages
        .iter()
        .filter_map(|(stage, ok)| (!ok).then_some(*stage))
        .collect();
    LintIssue::new(
        LintSeverity::Info,
        "stage-readiness",
        "model",
        "",
        format!(
            "ready stages: {}; missing stages: {}",
            if ready.is_empty() {
                "none".to_string()
            } else {
                ready.join("|")
            },
            if missing.is_empty() {
                "none".to_string()
            } else {
                missing.join("|")
            }
        ),
        "use missing stages to decide the next modeling refinement pass",
    )
}

fn model_element_count(model: &SemanticModel) -> usize {
    model.actors.len()
        + model.ext_systems.len()
        + model.systems.len()
        + model.requirements.len()
        + model.nfrs.len()
        + model.qualities.len()
        + model.constraints.len()
        + model.concepts.len()
        + model.domain_objects.len()
        + model.aggregates.len()
        + model.value_objects.len()
        + model.businesses.len()
        + model.bucs.len()
        + model.flows.len()
        + model.steps.len()
        + model.usage_scenes.len()
        + model.use_cases.len()
        + model.screens.len()
        + model.fields.len()
        + model.events.len()
        + model.entities.len()
        + model.states.len()
        + model.conditions.len()
        + model.variations.len()
        + model.apis.len()
        + model.dtos.len()
        + model.locations.len()
        + model.timings.len()
        + model.media.len()
        + model.permissions.len()
}

fn relation_degree(model: &SemanticModel) -> HashMap<NodeRef, usize> {
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
    for transition in &model.state_transitions {
        touch(NodeRef::Event(transition.event));
        touch(NodeRef::Entity(transition.entity));
    }
    for coordination in &model.boundary_coordinations {
        touch(NodeRef::UseCase(coordination.usecase));
        touch(NodeRef::Entity(coordination.left));
        touch(NodeRef::Entity(coordination.right));
    }
    for effect in &model.column_effects {
        touch(effect.origin.clone());
        touch(NodeRef::Entity(effect.entity));
    }
    for effect in &model.proposition_effects {
        touch(effect.origin.clone());
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
    for mapping in &model.concept_mappings {
        touch(NodeRef::Entity(mapping.entity));
        touch(mapping.source.as_node_ref());
    }
    for assertion in &model.temporal_assertions {
        touch(NodeRef::UseCase(assertion.anchor));
        for entity in &assertion.scope {
            touch(NodeRef::Entity(*entity));
        }
    }
    for constraint in &model.quantifier_constraints {
        touch(NodeRef::Entity(constraint.anchor));
        touch(NodeRef::Entity(constraint.related));
    }
    for event in &model.outbox_events {
        touch(NodeRef::Event(*event));
    }

    degree
}

fn lint_node_refs(model: &SemanticModel) -> Vec<NodeRef> {
    let mut nodes = Vec::new();
    nodes.extend(model.actors.iter().map(|(key, _)| NodeRef::Actor(key)));
    nodes.extend(
        model
            .ext_systems
            .iter()
            .map(|(key, _)| NodeRef::ExtSystem(key)),
    );
    nodes.extend(model.systems.iter().map(|(key, _)| NodeRef::System(key)));
    nodes.extend(
        model
            .requirements
            .iter()
            .map(|(key, _)| NodeRef::Requirement(key)),
    );
    nodes.extend(model.adrs.iter().map(|(key, _)| NodeRef::Adr(key)));
    nodes.extend(model.nfrs.iter().map(|(key, _)| NodeRef::Nfr(key)));
    nodes.extend(model.qualities.iter().map(|(key, _)| NodeRef::Quality(key)));
    nodes.extend(
        model
            .constraints
            .iter()
            .map(|(key, _)| NodeRef::Constraint(key)),
    );
    nodes.extend(model.concepts.iter().map(|(key, _)| NodeRef::Concept(key)));
    nodes.extend(
        model
            .domain_objects
            .iter()
            .map(|(key, _)| NodeRef::DomainObject(key)),
    );
    nodes.extend(
        model
            .aggregates
            .iter()
            .map(|(key, _)| NodeRef::Aggregate(key)),
    );
    nodes.extend(
        model
            .value_objects
            .iter()
            .map(|(key, _)| NodeRef::ValueObject(key)),
    );
    nodes.extend(
        model
            .businesses
            .iter()
            .map(|(key, _)| NodeRef::Business(key)),
    );
    nodes.extend(model.bucs.iter().map(|(key, _)| NodeRef::Buc(key)));
    nodes.extend(model.flows.iter().map(|(key, _)| NodeRef::Flow(key)));
    nodes.extend(model.steps.iter().map(|(key, _)| NodeRef::Step(key)));
    nodes.extend(
        model
            .usage_scenes
            .iter()
            .map(|(key, _)| NodeRef::UsageScene(key)),
    );
    nodes.extend(model.use_cases.iter().map(|(key, _)| NodeRef::UseCase(key)));
    nodes.extend(model.screens.iter().map(|(key, _)| NodeRef::Screen(key)));
    nodes.extend(model.fields.iter().map(|(key, _)| NodeRef::Field(key)));
    nodes.extend(model.events.iter().map(|(key, _)| NodeRef::Event(key)));
    nodes.extend(model.entities.iter().map(|(key, _)| NodeRef::Entity(key)));
    nodes.extend(model.states.iter().map(|(key, _)| NodeRef::State(key)));
    nodes.extend(
        model
            .conditions
            .iter()
            .map(|(key, _)| NodeRef::Condition(key)),
    );
    nodes.extend(
        model
            .variations
            .iter()
            .map(|(key, _)| NodeRef::Variation(key)),
    );
    nodes.extend(model.apis.iter().map(|(key, _)| NodeRef::Api(key)));
    nodes.extend(model.dtos.iter().map(|(key, _)| NodeRef::Dto(key)));
    nodes.extend(
        model
            .locations
            .iter()
            .map(|(key, _)| NodeRef::Location(key)),
    );
    nodes.extend(model.timings.iter().map(|(key, _)| NodeRef::Timing(key)));
    nodes.extend(model.media.iter().map(|(key, _)| NodeRef::Medium(key)));
    nodes.extend(
        model
            .permissions
            .iter()
            .map(|(key, _)| NodeRef::Permission(key)),
    );
    nodes
}

fn node_kind_display(node: &NodeRef) -> String {
    match node {
        NodeRef::Actor(_) => "actor".to_string(),
        NodeRef::ExtSystem(_) => "extsystem".to_string(),
        NodeRef::System(_) => "system".to_string(),
        NodeRef::Requirement(_) => "requirement".to_string(),
        NodeRef::Adr(_) => "adr".to_string(),
        NodeRef::Nfr(_) => "nfr".to_string(),
        NodeRef::Quality(_) => "quality".to_string(),
        NodeRef::Constraint(_) => "constraint".to_string(),
        NodeRef::Concept(_) => "concept".to_string(),
        NodeRef::DomainObject(_) => "domain-object".to_string(),
        NodeRef::Aggregate(_) => "aggregate".to_string(),
        NodeRef::ValueObject(_) => "value-object".to_string(),
        NodeRef::Business(_) => "business".to_string(),
        NodeRef::Buc(_) => "buc".to_string(),
        NodeRef::Flow(_) => "flow".to_string(),
        NodeRef::Step(_) => "step".to_string(),
        NodeRef::UsageScene(_) => "usagescene".to_string(),
        NodeRef::UseCase(_) => "usecase".to_string(),
        NodeRef::Screen(_) => "screen".to_string(),
        NodeRef::Field(_) => "field".to_string(),
        NodeRef::Event(_) => "event".to_string(),
        NodeRef::Entity(_) => "entity".to_string(),
        NodeRef::State(_) => "state".to_string(),
        NodeRef::Condition(_) => "condition".to_string(),
        NodeRef::Variation(_) => "variation".to_string(),
        NodeRef::Api(_) => "api".to_string(),
        NodeRef::Dto(_) => "dto".to_string(),
        NodeRef::Location(_) => "location".to_string(),
        NodeRef::Timing(_) => "timing".to_string(),
        NodeRef::Medium(_) => "medium".to_string(),
        NodeRef::Permission(_) => "permission".to_string(),
    }
}

fn node_id(model: &SemanticModel, node: &NodeRef) -> Option<String> {
    Some(match node {
        NodeRef::Actor(key) => model.actors.get(*key)?.id.clone(),
        NodeRef::ExtSystem(key) => model.ext_systems.get(*key)?.id.clone(),
        NodeRef::System(key) => model.systems.get(*key)?.id.clone(),
        NodeRef::Requirement(key) => model.requirements.get(*key)?.id.clone(),
        NodeRef::Adr(key) => model.adrs.get(*key)?.id.clone(),
        NodeRef::Nfr(key) => model.nfrs.get(*key)?.id.clone(),
        NodeRef::Quality(key) => model.qualities.get(*key)?.id.clone(),
        NodeRef::Constraint(key) => model.constraints.get(*key)?.id.clone(),
        NodeRef::Concept(key) => model.concepts.get(*key)?.id.clone(),
        NodeRef::DomainObject(key) => model.domain_objects.get(*key)?.id.clone(),
        NodeRef::Aggregate(key) => model.aggregates.get(*key)?.id.clone(),
        NodeRef::ValueObject(key) => model.value_objects.get(*key)?.id.clone(),
        NodeRef::Business(key) => model.businesses.get(*key)?.id.clone(),
        NodeRef::Buc(key) => model.bucs.get(*key)?.id.clone(),
        NodeRef::Flow(key) => model.flows.get(*key)?.id.clone(),
        NodeRef::Step(key) => model.steps.get(*key)?.id.clone(),
        NodeRef::UsageScene(key) => model.usage_scenes.get(*key)?.id.clone(),
        NodeRef::UseCase(key) => model.use_cases.get(*key)?.id.clone(),
        NodeRef::Screen(key) => model.screens.get(*key)?.id.clone(),
        NodeRef::Field(key) => model.fields.get(*key)?.id.clone(),
        NodeRef::Event(key) => model.events.get(*key)?.id.clone(),
        NodeRef::Entity(key) => model.entities.get(*key)?.id.clone(),
        NodeRef::State(key) => model.states.get(*key)?.id.clone(),
        NodeRef::Condition(key) => model.conditions.get(*key)?.id.clone(),
        NodeRef::Variation(key) => model.variations.get(*key)?.id.clone(),
        NodeRef::Api(key) => model.apis.get(*key)?.id.clone(),
        NodeRef::Dto(key) => model.dtos.get(*key)?.id.clone(),
        NodeRef::Location(key) => model.locations.get(*key)?.id.clone(),
        NodeRef::Timing(key) => model.timings.get(*key)?.id.clone(),
        NodeRef::Medium(key) => model.media.get(*key)?.id.clone(),
        NodeRef::Permission(key) => model.permissions.get(*key)?.id.clone(),
    })
}

fn is_upper_camelish_id(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase()
        && id.chars().all(|c| c.is_ascii_alphanumeric())
        && id.chars().any(|c| c.is_ascii_lowercase())
}

fn is_snake_caseish(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first == '_')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
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

    #[test]
    fn lint_findings_attach_declaration_locations() {
        let model = model_from(
            r#"
actor customer "Customer"
buc Checkout "Checkout"
api CreateOrder "Create order"
"#,
        );

        let findings: Vec<_> = collect_findings(&model)
            .into_iter()
            .filter(|issue| issue.rule == "orphan-node" || issue.rule == "naming-id")
            .collect();
        assert!(!findings.is_empty());
        for issue in &findings {
            assert!(
                issue.location.is_some(),
                "expected location for {:?}",
                issue.rule
            );
        }
    }

    #[test]
    fn lint_reports_coverage_readiness_and_review_findings() {
        let model = model_from(
            r#"
actor customer "Customer"
buc Checkout "Checkout"
flow CheckoutFlow "Checkout flow"
step ReviewCart "Review cart"
api CreateOrder "Create order" method POST
dto CreateOrderRequest "Create order request"
field ShippingAddress "Shipping address" access editable source actor
entity Order "Order" {
  Id: Int @pk
  total: Money
}
"#,
        );

        let issues = lint_issues(&model, &[]);
        let rules: Vec<_> = issues.iter().map(|issue| issue.rule).collect();
        assert!(rules.contains(&"coverage-score"));
        assert!(rules.contains(&"stage-readiness"));
        assert!(rules.contains(&"naming-id"));
        assert!(rules.contains(&"api-contract-incomplete"));
        assert!(rules.contains(&"field-unmapped"));
        assert!(rules.contains(&"naming-column"));
    }
}
