//! Review lint findings for model coverage, naming, and structural gaps.

use std::collections::HashMap;

use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::LocatedSpan;
use crate::model::{node_ref_kind, ColumnType, NodeRef, RelKind, SemanticModel, StateKey, StepKey};
use crate::rdra_rules::{owned_by_registry, rdra_findings};
use crate::refinement::{assess, Assessment, Depth, Next, StageId};

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
/// `Info` findings are left out.
pub fn lint_review_diagnostics(model: &SemanticModel) -> Vec<Diagnostic> {
    collect_findings(model)
        .into_iter()
        .filter(|issue| issue.severity != LintSeverity::Info)
        .map(|issue| issue.to_diagnostic())
        .collect()
}

const STAGED_RULES: &[(&str, StageId)] = &[("buc-empty", StageId::Skeleton)];

fn demote_premature(issues: &mut [LintIssue], assessment: &Assessment) {
    for issue in issues {
        let Some((_, stage)) = STAGED_RULES.iter().find(|(rule, _)| *rule == issue.rule) else {
            continue;
        };
        let premature = issue.subject_kind == "buc"
            && assessment
                .bucs
                .iter()
                .find(|buc| buc.id == issue.subject_id)
                .is_some_and(|buc| !buc.has_passed(*stage));
        if premature {
            issue.severity = LintSeverity::Info;
            issue.hint = format!(
                "expected before stage {} {}; {}",
                stage.number(),
                stage.spec().name,
                issue.hint
            );
        }
    }
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
            | RdraError::TimerTriggeredUseCaseUnreachable { .. }
            | RdraError::SeparateTxInferred { .. }
            | RdraError::ApiOperatesEntityOutsideOwner { .. }
            | RdraError::CrossSystemEntityRelation { .. }
            | RdraError::CoordinationNotCrossSystem { .. }
            | RdraError::CoordinationMissingApi { .. }
            | RdraError::PublishedEntityMutated { .. }
            | RdraError::PublishedEntityReadByOwner { .. }
            | RdraError::PublishedEntityKeysInternal { .. }
            | RdraError::PublishedNotInSourceTx { .. }
            | RdraError::InternalEntityExposed { .. }
            | RdraError::PublishedEntityNeverWritten { .. }
            | RdraError::SharedInternalEntity { .. }
            | RdraError::SharedEntityAlsoExposed { .. }
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

        if degree.get(&node).copied().unwrap_or(0) == 0 && !owned_by_registry(&node) {
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

        // State ids are derived as `{Entity}_{variant}` from Enum variants, not authored.
        if !matches!(node, NodeRef::State(_)) && !is_upper_camelish_id(&id) {
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
    lint_flow_cycles(model, &mut findings);
    lint_column_names(model, &mut findings);
    lint_enum_attributes(model, &mut findings);
    lint_condition_anchors(model, &mut findings);
    findings.extend(rdra_findings(model));
    demote_premature(&mut findings, &assess(model, Depth::Structure));
    findings
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn push_element_lint(
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

fn lint_flow_cycles(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
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

/// An inline `Enum` that no `transitions` and no `sets` moves is an attribute, not a lifecycle.
fn lint_enum_attributes(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    for (ek, entity) in &model.entities {
        for column in &entity.columns {
            if !matches!(column.col_type, ColumnType::Enum(_)) {
                continue;
            }
            let moved = model.is_lifecycle_column(ek, &column.name)
                || model
                    .column_effects
                    .iter()
                    .any(|e| e.entity == ek && e.column == column.name);
            if moved {
                continue;
            }
            push_element_lint(
                model,
                issues,
                "entity",
                "entity-column",
                &entity.id,
                "enum-attribute",
                format!(
                    "{}.{} is an inline Enum that no transitions or sets moves",
                    entity.id, column.name
                ),
                "type it by a variation if an actor chooses the value",
            );
        }
    }
}

/// A `holds` body that names neither a variation column nor a lifecycle column is not
/// RDRA 条件: it combines no バリエーション and no 状態モデル.
fn lint_condition_anchors(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    for (key, condition) in &model.conditions {
        if condition.body.is_none() || !crate::analysis::condition_edges(model, key).is_empty() {
            continue;
        }
        push_element_lint(
            model,
            issues,
            "condition",
            "condition",
            &condition.id,
            "condition-no-rdra-anchor",
            format!(
                "condition {} has a body that names no variation column and no lifecycle column",
                condition.id
            ),
            "compare a column typed by a variation or a column some `transitions` moves, or use `combines` to link the condition for traceability",
        );
    }
}

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

fn coverage_score_issue(model: &SemanticModel, findings: &[LintIssue]) -> LintIssue {
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

fn stage_readiness_issue(model: &SemanticModel) -> LintIssue {
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
    let state_by_id: HashMap<&str, StateKey> = model
        .states
        .iter()
        .map(|(key, state)| (state.id.as_str(), key))
        .collect();
    for transition in &model.state_transitions {
        touch(NodeRef::UseCase(transition.usecase));
        touch(NodeRef::Entity(transition.entity));
        let entity_id = &model.entities[transition.entity].id;
        for variant in [&transition.from, &transition.to] {
            if let Some(&state) = state_by_id.get(format!("{entity_id}_{variant}").as_str()) {
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
        touch(NodeRef::Entity(constraint.related));
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
    nodes.extend(
        model
            .businesses
            .iter()
            .map(|(key, _)| NodeRef::Business(key)),
    );
    nodes.extend(model.bucs.iter().map(|(key, _)| NodeRef::Buc(key)));
    nodes.extend(model.flows.iter().map(|(key, _)| NodeRef::Flow(key)));
    nodes.extend(model.steps.iter().map(|(key, _)| NodeRef::Step(key)));
    nodes.extend(model.use_cases.iter().map(|(key, _)| NodeRef::UseCase(key)));
    nodes.extend(model.screens.iter().map(|(key, _)| NodeRef::Screen(key)));
    nodes.extend(model.fields.iter().map(|(key, _)| NodeRef::Field(key)));
    nodes.extend(model.events.iter().map(|(key, _)| NodeRef::Event(key)));
    nodes.extend(model.entities.iter().map(|(key, _)| NodeRef::Entity(key)));
    nodes.extend(model.states.iter().map(|(key, _)| NodeRef::State(key)));
    nodes.extend(model.apis.iter().map(|(key, _)| NodeRef::Api(key)));
    nodes.extend(model.dtos.iter().map(|(key, _)| NodeRef::Dto(key)));
    nodes.extend(model.timers.iter().map(|(key, _)| NodeRef::Timer(key)));
    nodes.extend(model.media.iter().map(|(key, _)| NodeRef::Medium(key)));
    nodes.extend(
        model
            .permissions
            .iter()
            .map(|(key, _)| NodeRef::Permission(key)),
    );
    nodes.extend(
        model
            .variations
            .iter()
            .map(|(key, _)| NodeRef::Variation(key)),
    );
    nodes.extend(
        model
            .conditions
            .iter()
            .map(|(key, _)| NodeRef::Condition(key)),
    );
    nodes
}

pub(crate) fn node_kind_display(node: &NodeRef) -> String {
    match node {
        NodeRef::Actor(_) => "actor".to_string(),
        NodeRef::ExtSystem(_) => "extsystem".to_string(),
        NodeRef::System(_) => "system".to_string(),
        NodeRef::Requirement(_) => "requirement".to_string(),
        NodeRef::Adr(_) => "adr".to_string(),
        NodeRef::Nfr(_) => "nfr".to_string(),
        NodeRef::Quality(_) => "quality".to_string(),
        NodeRef::Constraint(_) => "constraint".to_string(),
        NodeRef::Business(_) => "business".to_string(),
        NodeRef::Buc(_) => "buc".to_string(),
        NodeRef::Flow(_) => "flow".to_string(),
        NodeRef::Step(_) => "step".to_string(),
        NodeRef::UseCase(_) => "usecase".to_string(),
        NodeRef::Screen(_) => "screen".to_string(),
        NodeRef::Field(_) => "field".to_string(),
        NodeRef::Event(_) => "event".to_string(),
        NodeRef::Entity(_) => "entity".to_string(),
        NodeRef::State(_) => "state".to_string(),
        NodeRef::Api(_) => "api".to_string(),
        NodeRef::Dto(_) => "dto".to_string(),
        NodeRef::Timer(_) => "timer".to_string(),
        NodeRef::Medium(_) => "medium".to_string(),
        NodeRef::Permission(_) => "permission".to_string(),
        NodeRef::Variation(_) => "variation".to_string(),
        NodeRef::Condition(_) => "condition".to_string(),
    }
}

pub(crate) fn node_id(model: &SemanticModel, node: &NodeRef) -> Option<String> {
    Some(match node {
        NodeRef::Actor(key) => model.actors.get(*key)?.id.clone(),
        NodeRef::ExtSystem(key) => model.ext_systems.get(*key)?.id.clone(),
        NodeRef::System(key) => model.systems.get(*key)?.id.clone(),
        NodeRef::Requirement(key) => model.requirements.get(*key)?.id.clone(),
        NodeRef::Adr(key) => model.adrs.get(*key)?.id.clone(),
        NodeRef::Nfr(key) => model.nfrs.get(*key)?.id.clone(),
        NodeRef::Quality(key) => model.qualities.get(*key)?.id.clone(),
        NodeRef::Constraint(key) => model.constraints.get(*key)?.id.clone(),
        NodeRef::Business(key) => model.businesses.get(*key)?.id.clone(),
        NodeRef::Buc(key) => model.bucs.get(*key)?.id.clone(),
        NodeRef::Flow(key) => model.flows.get(*key)?.id.clone(),
        NodeRef::Step(key) => model.steps.get(*key)?.id.clone(),
        NodeRef::UseCase(key) => model.use_cases.get(*key)?.id.clone(),
        NodeRef::Screen(key) => model.screens.get(*key)?.id.clone(),
        NodeRef::Field(key) => model.fields.get(*key)?.id.clone(),
        NodeRef::Event(key) => model.events.get(*key)?.id.clone(),
        NodeRef::Entity(key) => model.entities.get(*key)?.id.clone(),
        NodeRef::State(key) => model.states.get(*key)?.id.clone(),
        NodeRef::Api(key) => model.apis.get(*key)?.id.clone(),
        NodeRef::Dto(key) => model.dtos.get(*key)?.id.clone(),
        NodeRef::Timer(key) => model.timers.get(*key)?.id.clone(),
        NodeRef::Medium(key) => model.media.get(*key)?.id.clone(),
        NodeRef::Permission(key) => model.permissions.get(*key)?.id.clone(),
        NodeRef::Variation(key) => model.variations.get(*key)?.id.clone(),
        NodeRef::Condition(key) => model.conditions.get(*key)?.id.clone(),
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
    fn a_premature_buc_empty_is_info_and_not_published_to_the_editor() {
        let model = model_from(
            r#"
business Commerce "Commerce"
buc BucOrder "Process Order"
belongs(BucOrder, Commerce)
"#,
        );
        let issue = collect_findings(&model)
            .into_iter()
            .find(|issue| issue.rule == "buc-empty")
            .unwrap();
        assert_eq!(issue.severity, LintSeverity::Info);
        assert!(
            issue
                .hint
                .starts_with("expected before stage 1 BUC skeleton; "),
            "{}",
            issue.hint
        );
        let published: Vec<String> = lint_review_diagnostics(&model)
            .iter()
            .map(|d| d.error.to_string())
            .collect();
        assert!(
            published.iter().all(|m| !m.contains("buc-empty")),
            "{published:?}"
        );
    }

    #[test]
    fn buc_empty_on_an_event_started_buc_stays_a_warning() {
        let model = model_from(
            r#"
business Commerce "Commerce"
buc BucShip "Ship"
event OrderPaid "Order paid"
belongs(BucShip, Commerce)
triggers(OrderPaid, BucShip)
"#,
        );
        let issue = collect_findings(&model)
            .into_iter()
            .find(|issue| issue.rule == "buc-empty")
            .unwrap();
        assert_eq!(issue.severity, LintSeverity::Warning);
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
    fn lint_leaves_states_derived_from_transitions_alone() {
        let model = model_from(
            r#"
usecase Place "Place"
entity Order "Order" {
  Id: Int @pk
  status: Enum(pending, paid)
}
transitions(Order.status, Place, pending -> paid)
"#,
        );

        assert_eq!(model.states.len(), 2);
        let on_states: Vec<_> = collect_findings(&model)
            .into_iter()
            .filter(|issue| issue.subject_kind == "state")
            .collect();
        assert!(on_states.is_empty(), "{on_states:?}");
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

    #[test]
    fn coverage_score_is_zero_for_empty_model() {
        let model = model_from("");
        let issues = lint_issues(&model, &[]);
        let score = issues
            .iter()
            .find(|i| i.rule == "coverage-score")
            .expect("coverage-score");
        assert!(
            score.message.starts_with("coverage score: 0/100"),
            "empty model must not score high: {}",
            score.message
        );
        assert!(score.message.contains("empty model"));
    }

    #[test]
    fn coverage_score_penalizes_missing_layers_for_tiny_models() {
        let model = model_from(r#"actor Solo "Solo""#);
        let issues = lint_issues(&model, &[]);
        let score = issues
            .iter()
            .find(|i| i.rule == "coverage-score")
            .expect("coverage-score");
        // orphan warning scaled by elements + 6 missing layers × -8 = large penalty
        let numeric: i32 = score
            .message
            .strip_prefix("coverage score: ")
            .and_then(|s| s.split('/').next())
            .and_then(|n| n.parse().ok())
            .expect("parse score");
        assert!(
            numeric <= 50,
            "tiny model score should stay low, got {} from {}",
            numeric,
            score.message
        );
        assert!(score.message.contains("missing layers"));
        assert!(score.message.contains("scaled"));
    }

    fn enum_attribute_columns(model: &SemanticModel) -> Vec<String> {
        collect_findings(model)
            .into_iter()
            .filter(|issue| issue.rule == "enum-attribute")
            .map(|issue| issue.message)
            .collect()
    }

    #[test]
    fn enum_attribute_flags_inline_enum_nothing_moves() {
        let model = model_from(
            r#"
usecase Place "Place"
usecase Pay "Pay"
entity Order "Order" {
  Id: Int @pk
  status: Enum(pending, paid) @default(pending)
  channel: Enum(web, phone)
  priority: Enum(low, high)
  method: PaymentMethod
}
variation PaymentMethod "Payment Method" {
  cash
  card
}
transitions(Order.status, Pay, pending -> paid)
sets(Place, Order, priority == high)
"#,
        );
        let flagged = enum_attribute_columns(&model);
        assert_eq!(flagged.len(), 1, "{flagged:?}");
        assert!(flagged[0].contains("Order.channel"), "{flagged:?}");
        let issue = collect_findings(&model)
            .into_iter()
            .find(|issue| issue.rule == "enum-attribute")
            .unwrap();
        assert_eq!(issue.severity, LintSeverity::Warning);
        assert!(issue.hint.contains("variation"), "{}", issue.hint);
    }

    #[test]
    fn exposes_and_shares_connect_the_system_and_the_element() {
        let model = model_from(
            r#"
system S "S"
usecase U "U"
entity Beat "Beat" { id: Int @pk }
entity Log "Log" { id: Int @pk }
entity Note "Note" { id: Int @pk }
creates(U, Beat)
creates(U, Log)
exposes(S, Log).append()
shares(S, Note)
"#,
        );
        let orphans: Vec<String> = collect_findings(&model)
            .into_iter()
            .filter(|i| i.rule == "orphan-node")
            .map(|i| i.subject_id)
            .collect();
        assert!(orphans.is_empty(), "{orphans:?}");
    }

    #[test]
    fn variation_typing_a_column_of_a_reached_entity_is_anchored() {
        let model = model_from(
            r#"
buc B "B"
usecase U "U"
contains(B, U)
api Ap "Ap"
invokes(U, Ap)
variation Used "Used" {
  a
}
variation Unused "Unused" {
  a
}
entity E "E" {
  Id: Int @pk
  kind: Used
}
reads(Ap, E)
"#,
        );
        let findings = collect_findings(&model);
        assert!(!findings
            .iter()
            .any(|i| i.rule == "orphan-node" && i.subject_kind == "variation"));
        let unconnected: Vec<_> = findings
            .into_iter()
            .filter(|i| i.rule == "rdra-variation-no-buc")
            .map(|i| i.subject_id)
            .collect();
        assert_eq!(unconnected, vec!["Unused".to_string()]);
    }

    #[test]
    fn condition_body_without_a_variation_or_lifecycle_operand_has_no_rdra_anchor() {
        let model = model_from(
            r#"
variation Method "Method" {
  cash
  card
}
entity Pay "Pay" {
  Id: Int @pk
  method: Method
  amount: Int
  status: Enum(open, paid) @default(open)
}
usecase Settle "Settle"
transitions(Pay.status, Settle, open -> paid)
condition Big "Big"
holds(Big, Pay.amount > 100)
condition ByCard "By card"
holds(ByCard, Pay.method == card)
condition Paid "Paid"
holds(Paid, Pay.status == paid)
condition Traceable "Traceable"
combines(Traceable, Method)
"#,
        );
        let flagged: Vec<_> = collect_findings(&model)
            .into_iter()
            .filter(|i| i.rule == "condition-no-rdra-anchor")
            .map(|i| i.subject_id)
            .collect();
        assert_eq!(flagged, vec!["Big".to_string()]);
    }

    #[test]
    fn flow_cycle_detects_precedes_loop() {
        let model = model_from(
            r#"
step A "A"
step B "B"
precedes(A, B)
precedes(B, A)
"#,
        );
        let issues = lint_issues(&model, &[]);
        let cycles: Vec<_> = issues.iter().filter(|i| i.rule == "flow-cycle").collect();
        assert!(
            cycles.len() >= 2,
            "expected flow-cycle on both steps, got: {:?}",
            issues
                .iter()
                .map(|i| (i.rule, i.subject_id.as_str()))
                .collect::<Vec<_>>()
        );
        assert!(cycles.iter().any(|i| i.subject_id == "A"));
        assert!(cycles.iter().any(|i| i.subject_id == "B"));
    }
}
