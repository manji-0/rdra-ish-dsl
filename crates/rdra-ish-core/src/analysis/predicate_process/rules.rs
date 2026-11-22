//! The `forbidden`, `required`, `exclusive`, and `invariant` predicates.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use crate::analysis::comparison::resolve_comparison;
use crate::analysis::constraint::{
    arg_as_str, collect_entity_conditions, forbidden_when_conditions, mark_lemma, push_rule_guards,
    resolve_entity_equals_from_comparison, rule_label, when_condition_ref,
};
use crate::analysis::effect::parse_effect_value;

pub(super) fn process_forbidden_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let entity_key = match resolved.first() {
        Some(Some(NodeRef::Entity(k))) => *k,
        _ => return,
    };
    let before = diags.len();
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    let Some(conditions) = collect_entity_conditions(
        &model.variations,
        &entity_cols,
        &entity_id,
        &pred.args[1..],
        ctx,
        diags,
    ) else {
        return;
    };
    let Ok(when) = forbidden_when_conditions(model, pred, ctx, diags) else {
        return;
    };

    if conditions.is_empty() && when.is_empty() {
        let err = RdraError::RuleWithoutCondition {
            pred: pred.name.clone(),
        };
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
    } else {
        let label = rule_label(pred, ctx, diags);
        model.forbidden_constraints.push(EntityRule {
            label,
            site: Some(ctx.locate(pred.span.clone())),
            entity: entity_key,
            conditions,
        });
        let rule = RuleRef::Forbidden(model.forbidden_constraints.len() - 1);
        push_rule_guards(model, rule, &when, ctx);
        mark_lemma(model, pred, rule);
    }
}

pub(super) fn process_required_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let entity_key = match resolved.first() {
        Some(Some(NodeRef::Entity(k))) => *k,
        _ => return,
    };
    let before = diags.len();
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    let Some(conditions) = collect_entity_conditions(
        &model.variations,
        &entity_cols,
        &entity_id,
        &pred.args[1..],
        ctx,
        diags,
    ) else {
        return;
    };

    if conditions.is_empty() {
        let err = RdraError::RuleWithoutCondition {
            pred: pred.name.clone(),
        };
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
    } else {
        let label = rule_label(pred, ctx, diags);
        model.required_constraints.push(EntityRule {
            label,
            site: Some(ctx.locate(pred.span.clone())),
            entity: entity_key,
            conditions,
        });
    }
}

pub(super) fn process_exclusive_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let entity_key = match resolved.first() {
        Some(Some(NodeRef::Entity(k))) => *k,
        _ => return,
    };
    let before = diags.len();
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    let Some(conditions) = collect_entity_conditions(
        &model.variations,
        &entity_cols,
        &entity_id,
        &pred.args[1..],
        ctx,
        diags,
    ) else {
        return;
    };

    let got = conditions.len();
    if got < 2 {
        let err = RdraError::ExclusiveNeedsTwo { got };
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
    } else {
        let label = rule_label(pred, ctx, diags);
        model.exclusive_constraints.push(EntityRule {
            label,
            site: Some(ctx.locate(pred.span.clone())),
            entity: entity_key,
            conditions,
        });
    }
}

pub(super) fn process_invariant_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let entity_key = match resolved.first() {
        Some(Some(NodeRef::Entity(k))) => *k,
        _ => return,
    };
    let before = diags.len();
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    let mut guards: Vec<EntityCondition> = Vec::new();
    // `.when(Cond)` calls: their bodies join the guards after every predicate is read.
    let mut when: Vec<(ConditionKey, Span)> = Vec::new();
    let mut requireds: Vec<EntityCondition> = Vec::new();

    for cc in &pred.chain {
        let is_guard = cc.name == "when";
        let is_required = cc.name == "then";
        if !is_guard && !is_required {
            continue;
        }
        if is_guard {
            match when_condition_ref(model, &pred.name, cc, ctx, diags) {
                Ok(Some(ck)) => {
                    when.push((ck, cc.span.clone()));
                    continue;
                }
                Ok(None) => {}
                Err(()) => return,
            }
        }

        let mut processed_eq = false;
        for arg in &cc.args {
            if processed_eq {
                break;
            }
            match arg {
                PredicateArg::Expr(Expr::Cmp(cmp)) => {
                    match resolve_entity_equals_from_comparison(
                        &model.variations,
                        &entity_cols,
                        &entity_id,
                        cmp,
                        ctx,
                        diags,
                    ) {
                        Ok(Some((column, value))) => {
                            let cond = EntityCondition::Equals { column, value };
                            if is_guard {
                                guards.push(cond);
                            } else {
                                requireds.push(cond);
                            }
                        }
                        Ok(None) => {
                            if let Some(prop) = resolve_comparison(
                                &model.variations,
                                &entity_cols,
                                &entity_id,
                                cmp,
                                ctx,
                                diags,
                            ) {
                                let cond = EntityCondition::Comparison(prop);
                                if is_guard {
                                    guards.push(cond);
                                } else {
                                    requireds.push(cond);
                                }
                            }
                        }
                        Err(()) => return,
                    }
                }
                _ => {
                    if cc.args.len() < 2 {
                        break;
                    }
                    let Some(col_str) = arg_as_str(&cc.args[0]) else {
                        break;
                    };
                    let Some(val_str) = arg_as_str(&cc.args[1]) else {
                        break;
                    };
                    let col = entity_cols.iter().find(|c| c.name == col_str).cloned();
                    let Some(col) = col else {
                        push_error(
                            ctx,
                            diags,
                            arg_span(&cc.args[0]),
                            RdraError::UnknownColumn {
                                entity: entity_id.clone(),
                                col: col_str,
                            },
                        );
                        return;
                    };
                    match parse_effect_value(&model.variations, &col, &val_str) {
                        Ok(value) => {
                            let cond = EntityCondition::Equals {
                                column: col_str,
                                value,
                            };
                            if is_guard {
                                guards.push(cond);
                            } else {
                                requireds.push(cond);
                            }
                        }
                        Err(e) => {
                            push_error_parse_effect(ctx, diags, &cc.args[1], e);
                            return;
                        }
                    }
                    processed_eq = true;
                }
            }
        }
    }

    let has_guards = !guards.is_empty() || !when.is_empty();
    let has_requireds = !requireds.is_empty();
    if !has_requireds {
        let err = RdraError::InvariantWithoutConsequence;
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
    } else if !has_guards {
        let err = RdraError::InvariantWithoutWhen;
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
    } else {
        let label = rule_label(pred, ctx, diags);
        model.entity_invariants.push(EntityInvariant {
            label,
            site: Some(ctx.locate(pred.span.clone())),
            entity: entity_key,
            guards,
            requireds,
        });
        let rule = RuleRef::Invariant(model.entity_invariants.len() - 1);
        push_rule_guards(model, rule, &when, ctx);
        mark_lemma(model, pred, rule);
    }
}
