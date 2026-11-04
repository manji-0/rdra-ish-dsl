//! Entity-local conditions and context values collected from predicate arguments.

use crate::analysis::arg_resolve::resolve_arg;
use crate::analysis::comparison::{resolve_bare_rhs, resolve_comparison, BareRhs};
use crate::analysis::effect::parse_effect_value;
use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use slotmap::SlotMap;

#[derive(Default)]
pub(crate) struct EntityConditions {
    pub(crate) equals: Vec<(String, EffectValue)>,
    pub(crate) comparisons: Vec<ComparisonProp>,
}

pub(crate) fn resolve_entity_equals_from_comparison(
    variations: &SlotMap<VariationKey, Variation>,
    entity_cols: &[ModelColumn],
    entity_id: &str,
    cmp: &Comparison,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Result<Option<(String, EffectValue)>, ()> {
    if cmp.op != CmpOp::Eq {
        return Ok(None);
    }
    let col_str = match &cmp.lhs {
        Operand::Column(name) => name.clone(),
        Operand::QualifiedColumn(qcol) => {
            let Some(q_entity) = crate::analysis::qref_util::qref_id(&qcol.entity) else {
                return Ok(None);
            };
            if q_entity != entity_id {
                return Ok(None);
            }
            qcol.column.clone()
        }
        _ => return Ok(None),
    };
    let Some(col) = entity_cols.iter().find(|c| c.name == col_str).cloned() else {
        push_error(
            ctx,
            diags,
            cmp.span.clone(),
            RdraError::UnknownColumn {
                entity: entity_id.to_string(),
                col: col_str,
            },
        );
        return Err(());
    };
    let value_lit = match &cmp.rhs {
        Operand::Column(name) => {
            let qualified = |name: &str| {
                entity_cols
                    .iter()
                    .any(|c| c.name == name)
                    .then(|| format!("{entity_id}.{name}"))
            };
            return match resolve_bare_rhs(variations, &col, &cmp.op, name, qualified) {
                Ok(BareRhs::Value(value)) => Ok(Some((col_str, value))),
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
    match parse_effect_value(variations, &col, &value_lit) {
        Ok(value) => Ok(Some((col_str, value))),
        Err(e) => {
            push_error_cmp(ctx, diags, cmp, e);
            Err(())
        }
    }
}

pub(crate) fn collect_entity_conditions(
    variations: &SlotMap<VariationKey, Variation>,
    entity_cols: &[ModelColumn],
    entity_id: &str,
    args: &[PredicateArg],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<EntityConditions> {
    let mut conditions = EntityConditions::default();
    for arg in args {
        if let PredicateArg::Expr(Expr::Cmp(cmp)) = arg {
            match resolve_entity_equals_from_comparison(
                variations,
                entity_cols,
                entity_id,
                cmp,
                ctx,
                diags,
            ) {
                Ok(Some(condition)) => conditions.equals.push(condition),
                Ok(None) => {
                    if let Some(prop) =
                        resolve_comparison(variations, entity_cols, entity_id, cmp, ctx, diags)
                    {
                        conditions.comparisons.push(prop);
                    }
                }
                Err(()) => return None,
            }
        }
    }
    Some(conditions)
}

pub(crate) fn context_value_from_arg(
    model: &SemanticModel,
    arg: &PredicateArg,
    expected_kind: &str,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<BusinessMappingContextValue> {
    match arg {
        PredicateArg::Lit(s) => Some(BusinessMappingContextValue::Text(s.clone())),
        PredicateArg::Ref(_) => {
            let node = resolve_arg(model, arg, ctx, diags)?;
            let actual = node.kind_tag();
            if actual != expected_kind {
                push_error(
                    ctx,
                    diags,
                    arg_span(arg),
                    RdraError::TypeMismatch {
                        pred: "belongs context".to_string(),
                        id: context_arg_id(arg),
                        actual: actual.to_string(),
                        expected: expected_kind.to_string(),
                    },
                );
                return None;
            }
            Some(BusinessMappingContextValue::Ref(node))
        }
        PredicateArg::Transition { .. } | PredicateArg::Card(_) | PredicateArg::Expr(_) => None,
    }
}

fn context_arg_id(arg: &PredicateArg) -> String {
    match arg {
        PredicateArg::Ref(q) => {
            let id = q.parts.last().cloned().unwrap_or_default();
            match &q.kind_qualifier {
                Some(k) => format!("{}::{}", k.name(), id),
                None => id,
            }
        }
        PredicateArg::Lit(s) => s.clone(),
        PredicateArg::Transition { .. } => "<transition>".to_string(),
        PredicateArg::Card(c) => c.clone(),
        PredicateArg::Expr(_) => "<expr>".to_string(),
    }
}

// ── クロスエンティティ制約ヘルパー ───────────────────────────────────────────

pub(crate) fn push_unique_entity(scope: &mut Vec<EntityKey>, entity: EntityKey) {
    if !scope.contains(&entity) {
        scope.push(entity);
    }
}

pub(super) fn condition_entities(cond: &CrossEntityCondition, out: &mut Vec<EntityKey>) {
    match cond {
        CrossEntityCondition::Equals { column, .. } => push_unique_entity(out, column.entity),
        CrossEntityCondition::Comparison(prop) => {
            push_unique_entity(out, prop.lhs.entity);
            if let CrossCmpRhs::Column(col) = &prop.rhs {
                push_unique_entity(out, col.entity);
            }
        }
    }
}
