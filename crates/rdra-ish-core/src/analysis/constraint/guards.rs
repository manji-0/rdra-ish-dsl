//! Rule labels, `when` guards and their resolution onto rules.

use crate::analysis::arg_resolve::resolve_arg;
use crate::analysis::qref_util::qref_display;
use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use super::cross::add_condition_entities_to_scope;

/// The text of the rule's one `.label("...")` chain call. A second call, or an argument that is
/// not a single string literal, is reported and the label is left unset.
pub(crate) fn rule_label(
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<String> {
    let mut calls = pred.chain.iter().filter(|cc| cc.name == "label");
    let first = calls.next()?;
    if let Some(second) = calls.next() {
        let err = RdraError::DuplicateRuleLabel {
            pred: pred.name.clone(),
        };
        push_error(ctx, diags, second.span.clone(), err);
    }
    match first.args.as_slice() {
        [PredicateArg::Lit(text)] => Some(text.clone()),
        _ => {
            let err = RdraError::RuleLabelArg {
                pred: pred.name.clone(),
            };
            push_error(ctx, diags, first.span.clone(), err);
            None
        }
    }
}

/// The condition a rule's `.when(Cond)` names: `Some(key)` when the call's one argument is a
/// reference to a condition, `Ok(None)` when it is anything else (a comparison, or the two-word
/// pair), `Err` after reporting a reference to something that is not a condition.
pub(crate) fn when_condition_ref(
    model: &SemanticModel,
    pred: &str,
    cc: &ChainCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Result<Option<ConditionKey>, ()> {
    let [PredicateArg::Ref(qref)] = cc.args.as_slice() else {
        return Ok(None);
    };
    match resolve_arg(model, &cc.args[0], ctx, diags) {
        Some(NodeRef::Condition(ck)) => Ok(Some(ck)),
        Some(other) => {
            let got = qref_display(qref);
            let err = RdraError::RuleWhenArg {
                pred: pred.to_string(),
                hint: format!(
                    "`{got}` is a {}; name a condition declared with `holds(Name, Entity.column == value)`, or write the comparison itself",
                    other.kind_tag()
                ),
                got,
            };
            push_error(ctx, diags, cc.span.clone(), err);
            Err(())
        }
        // resolve_arg already reported an undefined or ambiguous reference.
        None => Err(()),
    }
}

/// `.when(...)` on a `forbidden`, which takes a condition only: its comparisons are its
/// arguments. One condition key per call, or `Err` after reporting the first other call.
pub(crate) fn forbidden_when_conditions(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Result<Vec<(ConditionKey, Span)>, ()> {
    let mut conditions = Vec::new();
    for cc in pred.chain.iter().filter(|cc| cc.name == "when") {
        match when_condition_ref(model, &pred.name, cc, ctx, diags)? {
            Some(ck) => conditions.push((ck, cc.span.clone())),
            None => {
                let got = cc
                    .args
                    .iter()
                    .map(crate::analysis::predicate_process::predicate_arg_display)
                    .collect::<Vec<_>>()
                    .join(", ");
                let err = RdraError::RuleWhenArg {
                    pred: pred.name.clone(),
                    got,
                    hint: "write a comparison as another argument, `forbidden(Entity, ..., status == value)`, and name a condition declared with `holds(Name, Entity.column == value)` in `.when(Name)`".to_string(),
                };
                push_error(ctx, diags, cc.span.clone(), err);
                return Err(());
            }
        }
    }
    Ok(conditions)
}

/// Records the `.when(Cond)` calls of a rule against the rule just pushed.
pub(crate) fn push_rule_guards(
    model: &mut SemanticModel,
    rule: RuleRef,
    conditions: &[(ConditionKey, Span)],
    ctx: DiagCtxt,
) {
    for (condition, span) in conditions {
        model.rule_guards.push(RuleGuard {
            rule,
            condition: *condition,
            site: Some(ctx.locate(span.clone())),
        });
    }
}

/// A body comparison as an entity-local rule reads it, or `None` when it names another
/// entity.
fn local_condition(cond: &CrossEntityCondition, entity: EntityKey) -> Option<LocalCondition> {
    match cond {
        CrossEntityCondition::Equals { column, value } if column.entity == entity => {
            Some(LocalCondition::Equals(column.column.clone(), value.clone()))
        }
        CrossEntityCondition::Comparison(prop) if prop.lhs.entity == entity => {
            let rhs = match &prop.rhs {
                CrossCmpRhs::Column(c) if c.entity == entity => CmpRhs::Column(c.column.clone()),
                CrossCmpRhs::Column(_) => return None,
                CrossCmpRhs::IntLit(n) => CmpRhs::IntLit(*n),
                CrossCmpRhs::Now => CmpRhs::Now,
                CrossCmpRhs::Value(v) => CmpRhs::Value(v.clone()),
                CrossCmpRhs::Values(vs) => CmpRhs::Values(vs.clone()),
            };
            Some(LocalCondition::Comparison(ComparisonProp {
                lhs_column: prop.lhs.column.clone(),
                op: prop.op,
                rhs,
            }))
        }
        _ => None,
    }
}

enum LocalCondition {
    Equals(String, EffectValue),
    Comparison(ComparisonProp),
}

/// The entity a body comparison names first.
fn first_entity(cond: &CrossEntityCondition) -> EntityKey {
    match cond {
        CrossEntityCondition::Equals { column, .. } => column.entity,
        CrossEntityCondition::Comparison(prop) => prop.lhs.entity,
    }
}

/// After every predicate is processed: each `.when(Cond)` on a rule reads the condition's
/// `holds` body as if its comparisons were written in that `.when(...)` (an invariant) or
/// among the rule's arguments (a `forbidden`). A condition with no body, one whose body has a
/// `.has/.none`, `.along` or `.role`, or one that compares an entity an entity-local rule does
/// not read, is an error, and the rule is left as written.
pub(crate) fn resolve_rule_guards(model: &mut SemanticModel, diags: &mut Vec<Diagnostic>) {
    let guards = model.rule_guards.clone();
    for g in &guards {
        let (pred, subject) = match g.rule {
            RuleRef::Forbidden(i) => ("forbidden", vec![model.forbidden_constraints[i].entity]),
            RuleRef::Invariant(i) => ("invariant", vec![model.entity_invariants[i].entity]),
            RuleRef::CrossForbidden(i) => (
                "forbidden",
                model.cross_forbidden_constraints[i].scope.clone(),
            ),
            RuleRef::CrossInvariant(i) => {
                ("invariant", model.cross_entity_invariants[i].scope.clone())
            }
            RuleRef::Quantifier(i) => ("invariant", vec![model.quantifier_constraints[i].anchor]),
        };
        let condition = &model.conditions[g.condition];
        let id = condition.id.clone();
        let report = |diags: &mut Vec<Diagnostic>, err: RdraError| {
            diags.push(match &g.site {
                Some(site) => Diagnostic::error_at(err, site.clone()),
                None => Diagnostic::error(err),
            });
        };
        let Some(body) = &condition.body else {
            let subject: Vec<&str> = subject
                .iter()
                .map(|e| model.entities[*e].id.as_str())
                .collect();
            report(
                diags,
                RdraError::WhenConditionWithoutBody {
                    pred: pred.to_string(),
                    subject: subject.join(", "),
                    condition: id,
                },
            );
            continue;
        };
        let chained = !body.quantifiers.is_empty()
            || !body.by.is_empty()
            || !matches!(body.along, CrossConstraintScope::GlobalProduct);
        if chained {
            report(
                diags,
                RdraError::RuleWhenConditionScope {
                    pred: pred.to_string(),
                    condition: id,
                    reason: "a rule's `.when` reads the comparisons of a body, and this body also has `.has(...)`, `.none(...)`, `.along(...)` or `.role(...)`".to_string(),
                    fix: "write the quantifier, the path or the key on the rule itself, after its `.when(...)`".to_string(),
                },
            );
            continue;
        }
        let conds = body.conds.clone();
        match g.rule {
            RuleRef::Forbidden(_) | RuleRef::Invariant(_) => {
                let entity = subject[0];
                let mut equals = Vec::new();
                let mut comparisons = Vec::new();
                let mut outside = None;
                for cond in &conds {
                    match local_condition(cond, entity) {
                        Some(LocalCondition::Equals(column, value)) => equals.push((column, value)),
                        Some(LocalCondition::Comparison(prop)) => comparisons.push(prop),
                        None => {
                            outside = Some(first_entity(cond));
                            break;
                        }
                    }
                }
                if let Some(other) = outside {
                    let (this, other) = (
                        model.entities[entity].id.clone(),
                        model.entities[other].id.clone(),
                    );
                    report(
                        diags,
                        RdraError::RuleWhenConditionScope {
                            pred: pred.to_string(),
                            condition: id,
                            reason: format!(
                                "its body compares {other}, and `{pred}({this}, ...)` reads {this} only"
                            ),
                            fix: format!(
                                "list the entities first, `{pred}({this}, {other}, ...)`, with `.along({this}, {other})` to read the {other} row the {this} belongs to"
                            ),
                        },
                    );
                    continue;
                }
                match g.rule {
                    RuleRef::Forbidden(i) => {
                        let rule = &mut model.forbidden_constraints[i];
                        rule.conditions.extend(equals);
                        rule.comparisons.extend(comparisons);
                    }
                    RuleRef::Invariant(i) => {
                        let rule = &mut model.entity_invariants[i];
                        rule.guards.extend(equals);
                        rule.guard_comparisons.extend(comparisons);
                    }
                    _ => unreachable!("matched above"),
                }
            }
            RuleRef::CrossForbidden(i) => {
                let rule = &mut model.cross_forbidden_constraints[i];
                add_condition_entities_to_scope(&mut rule.scope, &conds);
                rule.conditions.extend(conds);
            }
            RuleRef::CrossInvariant(i) => {
                let rule = &mut model.cross_entity_invariants[i];
                add_condition_entities_to_scope(&mut rule.scope, &conds);
                rule.guards.extend(conds);
            }
            RuleRef::Quantifier(i) => model.quantifier_constraints[i].guards.extend(conds),
        }
    }
}
