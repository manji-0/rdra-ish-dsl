//! Per-element lint rules: goals, screen transitions and rule labels.

use crate::model::{NodeRef, RelKind, SemanticModel};
use crate::rules::business_rules;

use super::{LintIssue, LintSeverity};

pub(super) fn push_element_info(
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
pub(super) fn lint_goals(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
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
pub(super) fn lint_screen_transitions(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
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
pub(super) fn lint_rule_labels(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
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
