//! Review lint findings for model coverage, naming, and structural gaps.

use std::collections::HashMap;

use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::LocatedSpan;
use crate::model::{
    CmpOpModel, CmpRhs, ColumnType, ComparisonProp, CrossCmpRhs, CrossEntityCondition, EntityKey,
    NodeRef, RelKind, SemanticModel, StateKey, StepKey,
};
use crate::rdra_rules::{owned_by_registry, rdra_findings};
use crate::refinement::{assess, Assessment, Depth, Next, StageId};
use crate::rules::business_rules;

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
    } else if matches!(diag.error, RdraError::DeprecatedSyntax { .. }) {
        (
            "deprecated-syntax",
            "run `rdra-ish fmt --write` to rewrite it to the current form before the version that removes it",
        )
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
        let Some(id) = model.node_id(&node) else {
            continue;
        };
        let kind = node.kind_tag();

        if degree.get(&node).copied().unwrap_or(0) == 0 && !owned_by_registry(&node) {
            push_element_lint(
                model,
                &mut findings,
                kind,
                kind,
                id,
                "orphan-node",
                "model element is declared but not connected to another element",
                "connect it with a predicate, map it, or remove it if it is not part of the model",
            );
        }

        // State ids are derived as `{Entity}_{variant}` from Enum variants, not authored.
        if !matches!(node, NodeRef::State(_)) && !is_upper_camelish_id(id) {
            push_element_lint(
                model,
                &mut findings,
                kind,
                kind,
                id,
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
    lint_unwritten_field_columns(model, &mut findings);
    lint_condition_anchors(model, &mut findings);
    lint_bodiless_guards(model, &mut findings);
    lint_int_comparisons(model, &mut findings);
    lint_unguarded_triggered(model, &mut findings);
    lint_rule_labels(model, &mut findings);
    lint_goals(model, &mut findings);
    lint_screen_transitions(model, &mut findings);
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

/// A nullable column a screen field shows starts `null`, and `states` keeps it there unless a
/// `sets` or a `chooses` gives it a value, so the field shows nothing the model can produce.
fn lint_unwritten_field_columns(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    for mapping in &model.field_mappings {
        let Some(entity) = model.entities.get(mapping.entity) else {
            continue;
        };
        let Some(field) = model.fields.get(mapping.field) else {
            continue;
        };
        let nullable = entity
            .columns
            .iter()
            .any(|c| c.name == mapping.column && c.is_nullable);
        let written = model
            .column_effects
            .iter()
            .any(|e| e.entity == mapping.entity && e.column == mapping.column)
            || model
                .column_choices
                .iter()
                .any(|c| c.entity == mapping.entity && c.column == mapping.column);
        if !nullable || written {
            continue;
        }
        push_element_lint(
            model,
            issues,
            "field",
            "field",
            &field.id,
            "field-column-unwritten",
            format!(
                "{} shows {}.{}, a nullable column that no use case sets, so it stays null",
                field.id, entity.id, mapping.column
            ),
            "add sets(UseCase, Entity.column == present) where the value is given, or drop the column's @null",
        );
    }
}

/// A `holds` body that names neither a variation column nor a lifecycle column is not
/// RDRA 条件: it combines no バリエーション and no 状態モデル. A body that guards a use case
/// is exempt: it is the use case's rule, and a guard has no other way to be written. It
/// guards one through `applies_to` a use case or an API, or `.when` on a `transitions` arm, a
/// `chooses` value, or a `sets` value (each a `Guards` relation, see `check_write_guards`). A
/// body a `forbidden` or `invariant` reads through `.when(Cond)` is that rule's condition
/// and is exempt the same way.
fn lint_condition_anchors(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    for (key, condition) in &model.conditions {
        if condition.body.is_none() || !crate::analysis::condition_edges(model, key).is_empty() {
            continue;
        }
        if model.rule_guards.iter().any(|g| g.condition == key) {
            continue;
        }
        let guards = model.relations.iter().any(|rel| {
            rel.from == NodeRef::Condition(key)
                && (rel.kind == RelKind::Guards
                    || (rel.kind == RelKind::AppliesTo
                        && matches!(rel.to, NodeRef::UseCase(_) | NodeRef::Api(_))))
        });
        if guards {
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
                "condition {} has a body that guards no use case and names no variation column and no lifecycle column",
                condition.id
            ),
            "apply it to the use case it guards with `applies_to`, compare a column typed by a variation or a column some `transitions` moves, or use `combines` to link the condition for traceability",
        );
    }
}

/// A condition with no `holds` body and no `combines` that `applies_to` a use case or an API
/// guards nothing in `check` and `verify` and links no variation or state model, so it is
/// only a name: the use case runs whether or not it holds. One that `combines` is an RDRA
/// trace link and is left alone.
fn lint_bodiless_guards(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    for (key, condition) in &model.conditions {
        if condition.body.is_some() || !crate::analysis::condition_edges(model, key).is_empty() {
            continue;
        }
        let guarded: Vec<String> = model
            .relations
            .iter()
            .filter(|rel| rel.kind == RelKind::AppliesTo && rel.from == NodeRef::Condition(key))
            .filter_map(|rel| match &rel.to {
                NodeRef::UseCase(uc) => Some(model.use_cases[*uc].id.clone()),
                NodeRef::Api(api) => Some(model.apis[*api].id.clone()),
                _ => None,
            })
            .collect();
        if guarded.is_empty() {
            continue;
        }
        push_element_info(
            model,
            issues,
            "condition",
            &condition.id,
            "condition-applied-without-body",
            &format!(
                "condition {} applies to {} but has no holds body, so check and verify run {} whether or not it holds",
                condition.id,
                guarded.join(", "),
                if guarded.len() == 1 { "it" } else { "them" }
            ),
            &format!(
                "write the rule as `holds({}, Entity.column == value)` to guard the use case, or keep it as a trace link",
                condition.id
            ),
        );
    }
}

/// An order comparison of a numeric column with an Int literal, where a rule or a `holds`
/// body writes it.
struct IntComparison {
    entity: EntityKey,
    column: String,
    op: CmpOpModel,
    value: i64,
    site: Option<LocatedSpan>,
}

fn int_comparisons(model: &SemanticModel) -> Vec<IntComparison> {
    let mut out = Vec::new();
    let mut single = |entity: EntityKey, props: &[ComparisonProp], site: &Option<LocatedSpan>| {
        for prop in props {
            if let CmpRhs::IntLit(value) = prop.rhs {
                out.push(IntComparison {
                    entity,
                    column: prop.lhs_column.clone(),
                    op: prop.op,
                    value,
                    site: site.clone(),
                });
            }
        }
    };
    for c in &model.forbidden_constraints {
        single(c.entity, &c.comparisons, &c.site);
    }
    for c in &model.required_constraints {
        single(c.entity, &c.comparisons, &c.site);
    }
    for c in &model.exclusive_constraints {
        single(c.entity, &c.comparisons, &c.site);
    }
    for c in &model.entity_invariants {
        single(c.entity, &c.guard_comparisons, &c.site);
        single(c.entity, &c.required_comparisons, &c.site);
    }
    let mut cross = |conds: &mut dyn Iterator<Item = &CrossEntityCondition>,
                     site: &Option<LocatedSpan>| {
        for cond in conds {
            if let CrossEntityCondition::Comparison(prop) = cond {
                if let CrossCmpRhs::IntLit(value) = prop.rhs {
                    out.push(IntComparison {
                        entity: prop.lhs.entity,
                        column: prop.lhs.column.clone(),
                        op: prop.op,
                        value,
                        site: site.clone(),
                    });
                }
            }
        }
    };
    for c in &model.cross_forbidden_constraints {
        cross(&mut c.conditions.iter(), &c.site);
    }
    for c in &model.cross_entity_invariants {
        cross(&mut c.guards.iter().chain(&c.requireds), &c.site);
    }
    for c in &model.quantifier_constraints {
        cross(
            &mut c.guards.iter().chain(&c.quantifier.conditions),
            &c.quantifier.site,
        );
    }
    for (_, condition) in &model.conditions {
        if let Some(body) = &condition.body {
            cross(&mut body.read_conditions(), &body.site);
        }
    }
    out
}

/// A comparison of an Int, Money or Decimal column with a literal that no value of the bounded
/// range `0..int_max` satisfies never holds, and one that every value satisfies, on a column
/// that is not `@null`, always does: `check` and `verify` explore only that range, so the rule
/// or the guard that writes it means something other than it says there.
fn lint_int_comparisons(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    let max = i64::from(crate::lifecycle::Bounds::STATES.int_max);
    let mut seen: Vec<(EntityKey, String, &'static str, i64, Option<LocatedSpan>)> = Vec::new();
    for c in int_comparisons(model) {
        let entity = &model.entities[c.entity];
        let Some(column) = entity.columns.iter().find(|col| col.name == c.column) else {
            continue;
        };
        if !matches!(
            column.col_type,
            ColumnType::Int | ColumnType::Money | ColumnType::Decimal
        ) {
            continue;
        }
        let holds = |v: i64| match c.op {
            CmpOpModel::Lt => Some(v < c.value),
            CmpOpModel::Le => Some(v <= c.value),
            CmpOpModel::Gt => Some(v > c.value),
            CmpOpModel::Ge => Some(v >= c.value),
            _ => None,
        };
        let Some(count) = (0..=max)
            .map(holds)
            .try_fold(0, |n, h| h.map(|h| n + usize::from(h)))
        else {
            continue;
        };
        let key = (
            c.entity,
            c.column.clone(),
            c.op.as_str(),
            c.value,
            c.site.clone(),
        );
        if seen.contains(&key) {
            continue;
        }
        let text = format!("{}.{} {} {}", entity.id, c.column, c.op.as_str(), c.value);
        let (rule, message, hint) = if count == 0 {
            (
                "comparison-unsatisfiable-in-range",
                format!(
                    "{text} holds for no value in 0..{max}, the Int range check and verify explore, so what it guards or forbids never applies"
                ),
                format!("compare with a value inside 0..{max}, or drop the comparison"),
            )
        } else if count == (max + 1) as usize && !column.is_nullable {
            (
                "comparison-trivial-in-range",
                format!(
                    "{text} holds for every value in 0..{max}, the Int range check and verify explore, so it constrains nothing"
                ),
                format!("drop the comparison, or compare with a value inside 0..{max} that some value fails"),
            )
        } else {
            continue;
        };
        seen.push(key);
        let mut issue = LintIssue::new(
            LintSeverity::Warning,
            rule,
            "entity",
            entity.id.clone(),
            message,
            hint,
        );
        if let Some(site) = c.site {
            issue = issue.with_location(site);
        }
        issues.push(issue);
    }
}

/// A use case only an event or a timer starts runs, in `check` and `verify`, in every state
/// where it can: the order of `raises` and `triggers` adds no guard. With no condition guard
/// and no `transitions` from-state, nothing keeps it from running before the event.
fn lint_unguarded_triggered(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    let writing: std::collections::HashSet<_> = crate::tx::infer_usecase_transactions(model)
        .iter()
        .filter(|utx| utx.writes().next().is_some())
        .map(|utx| utx.usecase)
        .collect();
    let guarded: std::collections::HashSet<_> = crate::analysis::applied_bodies(model)
        .iter()
        .map(|b| b.usecase)
        .collect();
    let mut triggered: Vec<_> = crate::access::triggered_use_cases(model)
        .into_iter()
        .collect();
    triggered.sort_by(|a, b| model.use_cases[*a].id.cmp(&model.use_cases[*b].id));
    for uc in triggered {
        let performed = model
            .relations
            .iter()
            .any(|rel| rel.kind == RelKind::Performs && rel.to == NodeRef::UseCase(uc));
        let from_state = model.state_transitions.iter().any(|t| t.usecase == uc);
        if performed || from_state || guarded.contains(&uc) || !writing.contains(&uc) {
            continue;
        }
        let starters: Vec<String> = model
            .relations
            .iter()
            .filter(|rel| rel.kind == RelKind::Triggers && rel.to == NodeRef::UseCase(uc))
            .filter_map(|rel| match &rel.from {
                NodeRef::Event(e) => Some(format!("event {}", model.events[*e].id)),
                NodeRef::Timer(t) => Some(format!("timer {}", model.timers[*t].id)),
                _ => None,
            })
            .collect();
        let id = &model.use_cases[uc].id;
        push_element_info(
            model,
            issues,
            "usecase",
            id,
            "triggered-usecase-unguarded",
            &format!(
                "use case {id} is started only by {}, and no condition guards it and no transitions gives it a from-state, so check and verify run it in every reachable state, before the event too",
                starters.join(" and ")
            ),
            &format!(
                "add the state the event follows as a guard: `holds(C, Entity.column == value)` with `applies_to(C, {id})`, or a `transitions({id}, Entity.column, from -> to)` arm"
            ),
        );
    }
}

fn push_element_info(
    model: &SemanticModel,
    issues: &mut Vec<LintIssue>,
    kind: &str,
    id: &str,
    rule: &'static str,
    message: &str,
    hint: &str,
) {
    let mut issue = LintIssue::new(LintSeverity::Info, rule, kind, id, message, hint);
    if let Some(loc) = model.decl_sites.located(kind, id) {
        issue = issue.with_location(loc);
    }
    issues.push(issue);
}

/// A requirement states what to build; a goal states why. A requirement that serves no goal,
/// and a goal with no `metric` or `target`, leave the outcome unmeasurable. A requirement
/// marked `status "out-of-scope"` is a non-goal and needs no goal.
fn lint_goals(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    for (key, requirement) in &model.requirements {
        let achieves = model
            .relations
            .iter()
            .any(|rel| rel.kind == RelKind::Achieves && rel.from == NodeRef::Requirement(key));
        if achieves || requirement.status.as_deref() == Some("out-of-scope") {
            continue;
        }
        push_element_info(
            model,
            issues,
            "requirement",
            &requirement.id,
            "requirement-no-goal",
            "requirement achieves no goal",
            "add achieves(Requirement, Goal), or mark it `status \"out-of-scope\"` with a `rationale` if it is a non-goal",
        );
    }
    for goal in model.goals.values() {
        if goal.metric.is_some() && goal.target.is_some() {
            continue;
        }
        push_element_info(
            model,
            issues,
            "goal",
            &goal.id,
            "goal-unmeasured",
            "goal has no metric or no target, so its success cannot be measured",
            "add `metric` and `target` (and a `baseline` and `window` when known)",
        );
    }
}

/// A screen no `navigates` enters or leaves, and no business flow passes through in order,
/// stands alone in `diagram --kind screen`: nothing says how the user gets there.
fn lint_screen_transitions(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    let mut linked = std::collections::HashSet::new();
    for navigation in crate::screen_flow::screen_navigations(model) {
        linked.extend([navigation.from, navigation.to]);
    }
    for order in crate::screen_flow::business_order(model) {
        linked.extend([order.from, order.to]);
    }
    for (key, screen) in &model.screens {
        if linked.contains(&key) {
            continue;
        }
        push_element_info(
            model,
            issues,
            "screen",
            &screen.id,
            "screen-no-transition",
            "no navigates enters or leaves the screen, and no business flow reaches it in order",
            "add navigates(FromScreen, ToScreen).via(UseCase) for the move into or out of it",
        );
    }
}

/// A constraint predicate without `.label("...")` has its meaning only in a comment, so the
/// business-rule table lists it with an empty label.
fn lint_rule_labels(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
    for rule in business_rules(model) {
        if !rule.is_constraint() || rule.label.is_some() {
            continue;
        }
        let mut issue = LintIssue::new(
            LintSeverity::Info,
            "rule-unlabeled",
            rule.kind,
            rule.entities.join("|"),
            format!(
                "{} on {} has no label: {}",
                rule.kind,
                rule.entities.join(", "),
                rule.text
            ),
            "add `.label(\"the rule in plain words\")` so `list --kind rule` and a violation name it",
        );
        if let Some(site) = rule.site {
            issue = issue.with_location(site);
        }
        issues.push(issue);
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
    nodes.extend(model.goals.iter().map(|(key, _)| NodeRef::Goal(key)));
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
mod tests;

#[cfg(test)]
mod rule_label_tests;

#[cfg(test)]
mod range_and_guard_tests;
