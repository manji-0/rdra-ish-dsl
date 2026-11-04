//! The `forbidden` and `invariant` rule predicates.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use super::cross::{
    add_condition_entities_to_scope, along_scope, collect_cross_chain_conditions,
    collect_cross_scope_prefix, cross_scope_semantics_from_chain, resolve_cross_condition,
};
use super::entity_conditions::push_unique_entity;
use super::entity_scope::resolve_entity_scope_arg;
use super::guards::{forbidden_when_conditions, push_rule_guards, rule_label, when_condition_ref};
use super::quantifier::resolve_related_quantifier;
use super::{report_leading_by, selectors};

pub(crate) fn process_forbidden(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let before = diags.len();
    let (mut scope, first_condition) = collect_cross_scope_prefix(model, pred, ctx, diags);
    let mut conditions = Vec::new();
    for arg in pred.args.iter().skip(first_condition) {
        if let Some(cond) = resolve_cross_condition(model, &scope, &pred.name, arg, ctx, diags) {
            conditions.push(cond);
        }
    }
    let Ok(when) = forbidden_when_conditions(model, pred, ctx, diags) else {
        return;
    };

    if conditions.is_empty() && when.is_empty() {
        let err = RdraError::RuleWithoutCondition {
            pred: pred.name.clone(),
        };
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
        return;
    }

    add_condition_entities_to_scope(&mut scope, &conditions);
    let scope_semantics = cross_scope_semantics_from_chain(model, pred, ctx, diags);
    let label = rule_label(pred, ctx, diags);
    model
        .cross_forbidden_constraints
        .push(CrossForbiddenConstraint {
            label,
            site: Some(ctx.locate(pred.span.clone())),
            scope: scope.clone(),
            scope_semantics,
            conditions,
        });
    let rule = RuleRef::CrossForbidden(model.cross_forbidden_constraints.len() - 1);
    push_rule_guards(model, rule, &when, ctx);
    model
        .typed_predicates
        .push(TypedPredicate::CrossForbidden { scope });
}

pub(crate) fn process_invariant(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let before = diags.len();
    let mut scope = Vec::new();
    for arg in &pred.args {
        if let Some(entity) = resolve_entity_scope_arg(model, &pred.name, arg, ctx, diags) {
            push_unique_entity(&mut scope, entity);
        }
    }

    let mut guards = Vec::new();
    // `.when(Cond)` calls so far: their bodies join the guards after every predicate is read.
    let mut when: Vec<(ConditionKey, Span)> = Vec::new();
    let mut requireds = Vec::new();
    let mut quantified = false;
    let label = rule_label(pred, ctx, diags);
    let along = along_scope(model, pred, ctx, diags);
    for (at, cc) in pred.chain.iter().enumerate() {
        match cc.name.as_str() {
            "when" => match when_condition_ref(model, &pred.name, cc, ctx, diags) {
                Ok(Some(ck)) => when.push((ck, cc.span.clone())),
                Ok(None) => guards.extend(collect_cross_chain_conditions(
                    model, &scope, &pred.name, &cc.args, ctx, diags,
                )),
                Err(()) => return,
            },
            "then" => requireds.extend(collect_cross_chain_conditions(
                model, &scope, &pred.name, &cc.args, ctx, diags,
            )),
            "has" | "none" => {
                quantified = true;
                // The anchor is the first entity the scope names. An empty scope is a
                // leading entity that did not resolve, which is reported.
                let Some(&anchor) = scope.first() else {
                    continue;
                };
                let by = selectors(&pred.chain, Some(at));
                let Some(quantifier) =
                    resolve_related_quantifier(model, anchor, cc, by, ctx, diags)
                else {
                    continue;
                };
                model
                    .quantifier_constraints
                    .push(crate::model::QuantifierConstraint {
                        label: label.clone(),
                        anchor,
                        guards: guards.clone(),
                        quantifier,
                        along: along.clone(),
                    });
                let rule = RuleRef::Quantifier(model.quantifier_constraints.len() - 1);
                push_rule_guards(model, rule, &when, ctx);
            }
            _ => {}
        }
    }

    report_leading_by(&pred.chain, ctx, diags);
    if requireds.is_empty() {
        if !quantified {
            let err = RdraError::InvariantWithoutConsequence;
            push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
        }
        return;
    }
    if guards.is_empty() && when.is_empty() {
        let err = RdraError::InvariantWithoutWhen;
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
        return;
    }

    add_condition_entities_to_scope(&mut scope, &guards);
    add_condition_entities_to_scope(&mut scope, &requireds);
    let scope_semantics = along;
    model.cross_entity_invariants.push(CrossEntityInvariant {
        label,
        site: Some(ctx.locate(pred.span.clone())),
        scope: scope.clone(),
        scope_semantics,
        guards,
        requireds,
    });
    let rule = RuleRef::CrossInvariant(model.cross_entity_invariants.len() - 1);
    push_rule_guards(model, rule, &when, ctx);
    model
        .typed_predicates
        .push(TypedPredicate::CrossInvariant { scope });
}
