//! The `after` temporal predicate and its assert conditions.

use crate::analysis::comparison::{resolve_bare_rhs, BareRhs};
use crate::analysis::effect::parse_effect_value;
use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use super::cross::{add_condition_entities_to_scope, resolve_cross_condition};
use super::entity_scope::{find_entity_column, resolve_entity_qref};
use super::guards::rule_label;

pub(super) fn temporal_equals_from_comparison(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    cmp: &Comparison,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Result<Option<CrossEntityCondition>, ()> {
    if cmp.op != CmpOp::Eq {
        return Ok(None);
    }

    let (column, model_col) = match &cmp.lhs {
        Operand::Column(column) if scope.len() == 1 => {
            let entity = scope[0];
            let model_col =
                find_entity_column(model, entity, column, cmp.span.clone(), ctx, diags).ok_or(())?;
            (
                QualifiedModelColumnRef {
                    entity,
                    column: column.clone(),
                },
                model_col,
            )
        }
        Operand::QualifiedColumn(qcol) => {
            let entity = resolve_entity_qref(model, pred, &qcol.entity, ctx, diags).ok_or(())?;
            let model_col =
                find_entity_column(model, entity, &qcol.column, qcol.span.clone(), ctx, diags)
                    .ok_or(())?;
            (
                QualifiedModelColumnRef {
                    entity,
                    column: qcol.column.clone(),
                },
                model_col,
            )
        }
        _ => return Ok(None),
    };

    let value_lit = match &cmp.rhs {
        Operand::Column(name) => {
            let qualified = |name: &str| cross_column_named(model, column.entity, scope, name);
            return match resolve_bare_rhs(&model.variations, &model_col, &cmp.op, name, qualified) {
                Ok(BareRhs::Value(value)) => {
                    Ok(Some(CrossEntityCondition::Equals { column, value }))
                }
                Ok(BareRhs::Column) => Ok(None),
                Err(e) => {
                    push_error_cmp(ctx, diags, cmp, e);
                    Err(())
                }
            };
        }
        Operand::IntLit(value) => value.clone(),
        Operand::Now => "now".to_string(),
        Operand::QualifiedColumn(_) | Operand::Values(_) => return Ok(None),
    };
    match parse_effect_value(&model.variations, &model_col, &value_lit) {
        Ok(value) => Ok(Some(CrossEntityCondition::Equals { column, value })),
        Err(e) => {
            push_error_cmp(ctx, diags, cmp, e);
            Err(())
        }
    }
}

/// `Entity.name` for the column `name` a bare right-hand side could mean: one of the
/// left-hand entity's, else one of an entity in scope.
pub(super) fn cross_column_named(
    model: &SemanticModel,
    lhs_entity: EntityKey,
    scope: &[EntityKey],
    name: &str,
) -> Option<String> {
    std::iter::once(lhs_entity)
        .chain(scope.iter().copied())
        .find(|ek| model.entities[*ek].columns.iter().any(|c| c.name == name))
        .map(|ek| format!("{}.{name}", model.entities[ek].id))
}

fn collect_temporal_assert_conditions(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    args: &[PredicateArg],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Vec<CrossEntityCondition> {
    let mut conditions = Vec::new();
    for arg in args {
        if let PredicateArg::Expr(Expr::Cmp(cmp)) = arg {
            match temporal_equals_from_comparison(model, scope, pred, cmp, ctx, diags) {
                Ok(Some(cond)) => conditions.push(cond),
                Ok(None) => {
                    if let Some(cond) = resolve_cross_condition(model, scope, pred, arg, ctx, diags)
                    {
                        conditions.push(cond);
                    }
                }
                Err(()) => {}
            }
        }
    }
    conditions
}

pub(crate) fn process_after_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(Some(NodeRef::UseCase(anchor))) = resolved.first() else {
        return;
    };

    let before = diags.len();
    let mut scope = Vec::new();
    let mut requireds = Vec::new();
    for cc in &pred.chain {
        if cc.name == "assert" {
            requireds.extend(collect_temporal_assert_conditions(
                model, &scope, &pred.name, &cc.args, ctx, diags,
            ));
        }
    }
    if requireds.is_empty() {
        let err = RdraError::AfterWithoutAssert {
            usecase: model.use_cases[*anchor].id.clone(),
        };
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
        return;
    }

    add_condition_entities_to_scope(&mut scope, &requireds);
    let label = rule_label(pred, ctx, diags);
    model
        .temporal_assertions
        .push(crate::model::TemporalAssertion {
            label,
            site: Some(ctx.locate(pred.span.clone())),
            anchor: *anchor,
            scope,
            requireds,
        });
    model
        .typed_predicates
        .push(TypedPredicate::After { anchor: *anchor });
}
