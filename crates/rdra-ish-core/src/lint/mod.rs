//! Review lint findings for model coverage, naming, and structural gaps.

mod columns;
mod conditions;
mod elements;
mod naming;
mod score;
mod structure;

use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::LocatedSpan;
use crate::model::{NodeRef, SemanticModel};
use crate::rdra_rules::{owned_by_registry, rdra_findings};
use crate::refinement::{assess, Assessment, Depth, StageId};

use self::columns::{lint_column_names, lint_enum_attributes, lint_unwritten_field_columns};
use self::conditions::{
    lint_bodiless_guards, lint_condition_anchors, lint_int_comparisons, lint_unguarded_triggered,
};
use self::elements::{lint_goals, lint_rule_labels, lint_screen_transitions};
use self::naming::{is_upper_camelish_id, lint_node_refs};
use self::score::{coverage_score_issue, relation_degree, stage_readiness_issue};
use self::structure::{lint_flow_cycles, lint_structural_coverage};

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

#[cfg(test)]
mod tests;

#[cfg(test)]
mod rule_label_tests;

#[cfg(test)]
mod range_and_guard_tests;
