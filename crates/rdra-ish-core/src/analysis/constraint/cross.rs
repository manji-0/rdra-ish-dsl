//! Cross-entity condition and scope resolution.

use crate::analysis::comparison::{
    is_order_op, resolve_bare_rhs, resolve_set_values, to_model_op, type_category, BareRhs,
};
use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use super::entity_conditions::{condition_entities, push_unique_entity};
use super::entity_scope::{find_entity_column, resolve_entity_qref, resolve_entity_scope_arg};
use super::temporal::{cross_column_named, temporal_equals_from_comparison};
use super::{report_leading_by, selectors};

fn resolve_cross_operand_column(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    operand: &Operand,
    span: Span,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<(QualifiedModelColumnRef, ModelColumn)> {
    match operand {
        Operand::Column(column) if scope.len() == 1 => {
            let entity = scope[0];
            let model_col = find_entity_column(model, entity, column, span.clone(), ctx, diags)?;
            Some((
                QualifiedModelColumnRef {
                    entity,
                    column: column.clone(),
                },
                model_col,
            ))
        }
        Operand::Column(column) => {
            push_error(
                ctx,
                diags,
                span,
                RdraError::CrossConstraintColumnNeedsEntity {
                    column: column.clone(),
                    example: format!("Entity.{}", column),
                },
            );
            None
        }
        Operand::QualifiedColumn(qcol) => {
            let entity = resolve_entity_qref(model, pred, &qcol.entity, ctx, diags)?;
            let model_col =
                find_entity_column(model, entity, &qcol.column, qcol.span.clone(), ctx, diags)?;
            Some((
                QualifiedModelColumnRef {
                    entity,
                    column: qcol.column.clone(),
                },
                model_col,
            ))
        }
        Operand::IntLit(_) | Operand::Now | Operand::Values(_) => None,
    }
}

fn resolve_cross_comparison(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    cmp: &Comparison,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<CrossComparisonProp> {
    let (lhs, lhs_col) = match resolve_cross_operand_column(
        model,
        scope,
        pred,
        &cmp.lhs,
        cmp.span.clone(),
        ctx,
        diags,
    ) {
        Some(v) => v,
        None => {
            push_error_cmp(ctx, diags, cmp, RdraError::ComparisonLhsMustBeColumn);
            return None;
        }
    };
    if let Operand::Values(names) = &cmp.rhs {
        return match resolve_set_values(&model.variations, &lhs_col, names) {
            Ok(values) => Some(CrossComparisonProp {
                lhs,
                op: to_model_op(&cmp.op),
                rhs: CrossCmpRhs::Values(values),
            }),
            Err(err) => {
                push_error_cmp(ctx, diags, cmp, err);
                None
            }
        };
    }

    let lhs_cat = type_category(&lhs_col.col_type);

    if is_order_op(&cmp.op) && lhs_cat == "equality" {
        push_error_cmp(
            ctx,
            diags,
            cmp,
            RdraError::ComparisonOpNotOrdered {
                col: lhs.column.clone(),
                col_type: format!("{:?}", lhs_col.col_type),
                op: cmp.op.as_str().to_string(),
            },
        );
        return None;
    }

    if let Operand::Column(name) = &cmp.rhs {
        let qualified = |name: &str| cross_column_named(model, lhs.entity, scope, name);
        match resolve_bare_rhs(&model.variations, &lhs_col, &cmp.op, name, qualified) {
            Ok(BareRhs::Value(value)) => {
                return Some(CrossComparisonProp {
                    lhs,
                    op: to_model_op(&cmp.op),
                    rhs: CrossCmpRhs::Value(value),
                });
            }
            Ok(BareRhs::Column) => {}
            Err(e) => {
                push_error_cmp(ctx, diags, cmp, e);
                return None;
            }
        }
    }

    let rhs = match &cmp.rhs {
        Operand::Column(_) | Operand::QualifiedColumn(_) => {
            let (rhs_ref, rhs_col) = resolve_cross_operand_column(
                model,
                scope,
                pred,
                &cmp.rhs,
                cmp.span.clone(),
                ctx,
                diags,
            )?;
            let rhs_cat = type_category(&rhs_col.col_type);
            if lhs_cat != rhs_cat {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonTypeMismatch {
                        lhs: lhs.column.clone(),
                        lhs_type: format!("{:?}", lhs_col.col_type),
                        rhs: rhs_ref.column.clone(),
                        rhs_type: format!("{:?}", rhs_col.col_type),
                    },
                );
                return None;
            }
            CrossCmpRhs::Column(rhs_ref)
        }
        Operand::IntLit(s) => {
            if lhs_cat != "numeric" {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonTypeMismatch {
                        lhs: lhs.column.clone(),
                        lhs_type: format!("{:?}", lhs_col.col_type),
                        rhs: s.clone(),
                        rhs_type: "integer_literal".to_string(),
                    },
                );
                return None;
            }
            match s.parse::<i64>() {
                Ok(n) => CrossCmpRhs::IntLit(n),
                Err(_) => {
                    push_error_cmp(
                        ctx,
                        diags,
                        cmp,
                        RdraError::ComparisonInvalidIntLit { lit: s.clone() },
                    );
                    return None;
                }
            }
        }
        Operand::Now => {
            if lhs_cat != "temporal" {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonNowRequiresTemporal {
                        col: lhs.column.clone(),
                        col_type: format!("{:?}", lhs_col.col_type),
                    },
                );
                return None;
            }
            CrossCmpRhs::Now
        }
        Operand::Values(_) => unreachable!("a set is resolved above"),
    };

    Some(CrossComparisonProp {
        lhs,
        op: to_model_op(&cmp.op),
        rhs,
    })
}

pub(crate) fn resolve_cross_condition(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    arg: &PredicateArg,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<CrossEntityCondition> {
    match arg {
        PredicateArg::Expr(Expr::Cmp(cmp)) => {
            match temporal_equals_from_comparison(model, scope, pred, cmp, ctx, diags) {
                Ok(Some(cond)) => Some(cond),
                Ok(None) => resolve_cross_comparison(model, scope, pred, cmp, ctx, diags)
                    .map(CrossEntityCondition::Comparison),
                Err(()) => None,
            }
        }
        _ => None,
    }
}

pub(super) fn collect_cross_scope_prefix(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> (Vec<EntityKey>, usize) {
    let mut scope = Vec::new();
    let mut first_condition = pred.args.len();
    for (idx, arg) in pred.args.iter().enumerate() {
        if matches!(arg, PredicateArg::Expr(_)) {
            first_condition = idx;
            break;
        }
        match arg {
            PredicateArg::Ref(_) => {
                if let Some(entity) = resolve_entity_scope_arg(model, &pred.name, arg, ctx, diags) {
                    push_unique_entity(&mut scope, entity);
                }
            }
            _ => {
                first_condition = idx;
                break;
            }
        }
    }
    (scope, first_condition)
}

pub(super) fn collect_cross_chain_conditions(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    args: &[PredicateArg],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Vec<CrossEntityCondition> {
    let mut conditions = Vec::new();
    for arg in args {
        if let Some(cond) = resolve_cross_condition(model, scope, pred, arg, ctx, diags) {
            conditions.push(cond);
        }
    }
    conditions
}

pub(crate) fn add_condition_entities_to_scope(
    scope: &mut Vec<EntityKey>,
    conditions: &[CrossEntityCondition],
) {
    for cond in conditions {
        condition_entities(cond, scope);
    }
}

fn collect_cross_along_path(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<(usize, Vec<EntityKey>)> {
    let at = pred.chain.iter().position(|cc| cc.name == "along")?;
    let mut path = Vec::new();
    for arg in &pred.chain[at].args {
        if let Some(entity) = resolve_entity_scope_arg(model, &pred.name, arg, ctx, diags) {
            path.push(entity);
        }
    }
    Some((at, path))
}

/// The scope of a cross-entity `forbidden` or `invariant`. A `.role(..)` that follows no
/// `.along`, `.has` or `.none` is reported here.
pub(crate) fn cross_scope_semantics_from_chain(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> CrossConstraintScope {
    report_leading_by(&pred.chain, ctx, diags);
    along_scope(model, pred, ctx, diags)
}

/// The `.along(..)` path of a statement and the selectors of its hops, or `GlobalProduct`
/// without one.
pub(crate) fn along_scope(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> CrossConstraintScope {
    match collect_cross_along_path(model, pred, ctx, diags) {
        Some((at, path)) => CrossConstraintScope::RelationPath {
            path,
            by: selectors(&pred.chain, Some(at)),
            links: Vec::new(),
        },
        None => CrossConstraintScope::GlobalProduct,
    }
}
