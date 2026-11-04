//! Condition, guard and integer-comparison lint rules.

use crate::location::LocatedSpan;
use crate::model::{
    CmpOpModel, CmpRhs, ColumnType, ComparisonProp, CrossCmpRhs, CrossEntityCondition, EntityKey,
    NodeRef, RelKind, SemanticModel,
};

use super::elements::push_element_info;
use super::{push_element_lint, LintIssue, LintSeverity};

/// A `holds` body that names neither a variation column nor a lifecycle column is not
/// RDRA 条件: it combines no バリエーション and no 状態モデル. A body that guards a use case
/// is exempt: it is the use case's rule, and a guard has no other way to be written. It
/// guards one through `applies_to` a use case or an API, or `.when` on a `transitions` arm, a
/// `chooses` value, or a `sets` value (each a `Guards` relation, see `check_write_guards`). A
/// body a `forbidden` or `invariant` reads through `.when(Cond)` is that rule's condition
/// and is exempt the same way.
pub(super) fn lint_condition_anchors(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
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
pub(super) fn lint_bodiless_guards(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
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
pub(super) fn lint_int_comparisons(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
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
pub(super) fn lint_unguarded_triggered(model: &SemanticModel, issues: &mut Vec<LintIssue>) {
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
