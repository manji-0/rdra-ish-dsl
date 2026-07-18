//! Entity and cross-entity constraint predicate helpers.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use super::arg_resolve::resolve_arg;
use super::comparison::{is_order_op, resolve_comparison, to_model_op, type_category};
use super::effect::parse_effect_value;
use super::nodes::node_kind_tag_str;
use super::qref_util::qref_display;

// ── 制約述語用ヘルパー ────────────────────────────────────────────────────────

/// `Lit(s)` または kind修飾なし1セグメントの `Ref` から文字列を取り出す。
/// `whenstatus == delivered` の裸ident引数と `sets(...)` の引用符付きリテラル
/// 引数の両方を許容するための統一抽出。
pub(crate) fn arg_as_str(arg: &PredicateArg) -> Option<String> {
    match arg {
        PredicateArg::Lit(s) => Some(s.clone()),
        PredicateArg::Ref(qref) if qref.kind_qualifier.is_none() && qref.parts.len() == 1 => {
            Some(qref.parts[0].clone())
        }
        _ => None,
    }
}

#[derive(Default)]
pub(crate) struct EntityConditions {
    pub(crate) equals: Vec<(String, EffectValue)>,
    pub(crate) comparisons: Vec<ComparisonProp>,
}

pub(crate) fn resolve_entity_equals_from_comparison(
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
            let Some(q_entity) = super::qref_util::qref_id(&qcol.entity) else {
                return Ok(None);
            };
            if q_entity != entity_id {
                return Ok(None);
            }
            qcol.column.clone()
        }
        _ => return Ok(None),
    };
    let value_lit = match &cmp.rhs {
        Operand::Column(value) => {
            // Prefer column-column comparison when the RHS names an existing column.
            if entity_cols.iter().any(|c| c.name == *value) {
                return Ok(None);
            }
            value.clone()
        }
        Operand::IntLit(value) => value.clone(),
        Operand::Now => "now".to_string(),
        Operand::QualifiedColumn(_) => return Ok(None),
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
    match parse_effect_value(&col, &value_lit) {
        Ok(value) => Ok(Some((col_str, value))),
        Err(e) => {
            push_error_cmp(ctx, diags, cmp, e);
            Err(())
        }
    }
}

pub(crate) fn collect_entity_conditions(
    entity_cols: &[ModelColumn],
    entity_id: &str,
    args: &[PredicateArg],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<EntityConditions> {
    let mut conditions = EntityConditions::default();
    for arg in args {
        if let PredicateArg::Expr(Expr::Cmp(cmp)) = arg {
            match resolve_entity_equals_from_comparison(entity_cols, entity_id, cmp, ctx, diags) {
                Ok(Some(condition)) => conditions.equals.push(condition),
                Ok(None) => {
                    if let Some(prop) = resolve_comparison(entity_cols, entity_id, cmp, ctx, diags)
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
            let actual = node_kind_tag_str(&node);
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

fn condition_entities(cond: &CrossEntityCondition, out: &mut Vec<EntityKey>) {
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

fn resolve_entity_qref(
    model: &SemanticModel,
    pred: &str,
    qref: &QRef,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<EntityKey> {
    let id = qref.parts.last()?.clone();
    if let Some(kind) = &qref.kind_qualifier {
        if kind != &Kind::Entity {
            push_error_qref(
                ctx,
                diags,
                qref,
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id: qref_display(qref),
                    actual: kind.name().to_string(),
                    expected: "entity".to_string(),
                },
            );
            return None;
        }
        return match model.symbols.lookup_qualified(kind, &id).cloned() {
            Some(NodeRef::Entity(k)) => Some(k),
            _ => {
                push_error_qref(
                    ctx,
                    diags,
                    qref,
                    RdraError::UndefinedSymbol {
                        id: format!("entity::{}", id),
                    },
                );
                None
            }
        };
    }

    if let Some(NodeRef::Entity(k)) = model.symbols.lookup_qualified(&Kind::Entity, &id).cloned() {
        return Some(k);
    }

    match model.symbols.lookup(&id) {
        LookupResult::Found(node) => {
            push_error_qref(
                ctx,
                diags,
                qref,
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id,
                    actual: node_kind_tag_str(node).to_string(),
                    expected: "entity".to_string(),
                },
            );
            None
        }
        LookupResult::NotFound => {
            push_error_qref(ctx, diags, qref, RdraError::UndefinedSymbol { id });
            None
        }
        LookupResult::Ambiguous(kinds) => {
            push_error_qref(
                ctx,
                diags,
                qref,
                RdraError::AmbiguousReference {
                    id,
                    kinds: kinds.join(", "),
                },
            );
            None
        }
    }
}

fn resolve_entity_scope_arg(
    model: &SemanticModel,
    pred: &str,
    arg: &PredicateArg,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<EntityKey> {
    match arg {
        PredicateArg::Ref(qref) => resolve_entity_qref(model, pred, qref, ctx, diags),
        PredicateArg::Lit(s) => {
            push_error(
                ctx,
                diags,
                arg_span(arg),
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id: s.clone(),
                    actual: "literal".to_string(),
                    expected: "entity".to_string(),
                },
            );
            None
        }
        PredicateArg::Transition { .. } | PredicateArg::Card(_) => {
            push_error(
                ctx,
                diags,
                arg_span(arg),
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id: "<arg>".to_string(),
                    actual: "argument".to_string(),
                    expected: "entity".to_string(),
                },
            );
            None
        }
        PredicateArg::Expr(Expr::Cmp(cmp)) => {
            push_error_cmp(
                ctx,
                diags,
                cmp,
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id: "<expr>".to_string(),
                    actual: "expression".to_string(),
                    expected: "entity".to_string(),
                },
            );
            None
        }
        PredicateArg::Expr(_) => {
            push_error(
                ctx,
                diags,
                arg_span(arg),
                RdraError::TypeMismatch {
                    pred: pred.to_string(),
                    id: "<expr>".to_string(),
                    actual: "expression".to_string(),
                    expected: "entity".to_string(),
                },
            );
            None
        }
    }
}

fn find_entity_column(
    model: &SemanticModel,
    entity: EntityKey,
    column: &str,
    span: Span,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<ModelColumn> {
    let entity_id = model.entities[entity].id.clone();
    match model.entities[entity]
        .columns
        .iter()
        .find(|c| c.name == column)
        .cloned()
    {
        Some(col) => Some(col),
        None => {
            push_error(
                ctx,
                diags,
                span,
                RdraError::UnknownColumn {
                    entity: entity_id,
                    col: column.to_string(),
                },
            );
            None
        }
    }
}

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
        Operand::IntLit(_) | Operand::Now => None,
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
    };

    Some(CrossComparisonProp {
        lhs,
        op: to_model_op(&cmp.op),
        rhs,
    })
}

fn resolve_cross_condition(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    arg: &PredicateArg,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<CrossEntityCondition> {
    match arg {
        PredicateArg::Expr(Expr::Cmp(cmp)) => {
            if let Some(cond) = temporal_equals_from_comparison(model, scope, pred, cmp, ctx, diags)
            {
                Some(cond)
            } else {
                resolve_cross_comparison(model, scope, pred, cmp, ctx, diags)
                    .map(CrossEntityCondition::Comparison)
            }
        }
        _ => None,
    }
}

fn collect_cross_scope_prefix(
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

fn collect_cross_chain_conditions(
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
) -> Option<Vec<EntityKey>> {
    let along = pred.chain.iter().find(|cc| cc.name == "along")?;
    let mut path = Vec::new();
    for arg in &along.args {
        if let Some(entity) = resolve_entity_scope_arg(model, &pred.name, arg, ctx, diags) {
            path.push(entity);
        }
    }
    Some(path)
}

pub(crate) fn cross_scope_semantics_from_chain(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> CrossConstraintScope {
    match collect_cross_along_path(model, pred, ctx, diags) {
        Some(path) => CrossConstraintScope::RelationPath(path),
        None => CrossConstraintScope::GlobalProduct,
    }
}

pub(crate) fn process_forbidden(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let (mut scope, first_condition) = collect_cross_scope_prefix(model, pred, ctx, diags);
    let mut conditions = Vec::new();
    for arg in pred.args.iter().skip(first_condition) {
        if let Some(cond) = resolve_cross_condition(model, &scope, &pred.name, arg, ctx, diags) {
            conditions.push(cond);
        }
    }

    if conditions.is_empty() {
        return;
    }

    add_condition_entities_to_scope(&mut scope, &conditions);
    let scope_semantics = cross_scope_semantics_from_chain(model, pred, ctx, diags);
    model
        .cross_forbidden_constraints
        .push(CrossForbiddenConstraint {
            scope: scope.clone(),
            scope_semantics,
            conditions,
        });
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
    let mut scope = Vec::new();
    for arg in &pred.args {
        if let Some(entity) = resolve_entity_scope_arg(model, &pred.name, arg, ctx, diags) {
            push_unique_entity(&mut scope, entity);
        }
    }

    let mut guards = Vec::new();
    let mut requireds = Vec::new();
    for cc in &pred.chain {
        match cc.name.as_str() {
            "when" => guards.extend(collect_cross_chain_conditions(
                model, &scope, &pred.name, &cc.args, ctx, diags,
            )),
            "then" => requireds.extend(collect_cross_chain_conditions(
                model, &scope, &pred.name, &cc.args, ctx, diags,
            )),
            "has" | "none" => process_quantifier_chain(model, &scope, &guards, cc, ctx, diags),
            _ => {}
        }
    }

    if guards.is_empty() || requireds.is_empty() {
        return;
    }

    add_condition_entities_to_scope(&mut scope, &guards);
    add_condition_entities_to_scope(&mut scope, &requireds);
    let scope_semantics = cross_scope_semantics_from_chain(model, pred, ctx, diags);
    model.cross_entity_invariants.push(CrossEntityInvariant {
        scope: scope.clone(),
        scope_semantics,
        guards,
        requireds,
    });
    model
        .typed_predicates
        .push(TypedPredicate::CrossInvariant { scope });
}

/// Top-level `when(...).none/has(...)` quantifier (replaces `forbidden_when`).
pub(crate) fn process_when_quantifier_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let mut scope = Vec::new();
    let guards = collect_cross_chain_conditions(model, &scope, &pred.name, &pred.args, ctx, diags);
    if guards.is_empty() {
        return;
    }
    add_condition_entities_to_scope(&mut scope, &guards);
    let Some(anchor) = scope.first().copied() else {
        return;
    };

    for cc in &pred.chain {
        if matches!(cc.name.as_str(), "has" | "none") {
            process_quantifier_chain(model, &[anchor], &guards, cc, ctx, diags);
        }
    }
    model
        .typed_predicates
        .push(TypedPredicate::ForbiddenWhen { entity: anchor });
}

fn process_quantifier_chain(
    model: &mut SemanticModel,
    anchor_scope: &[EntityKey],
    guards: &[CrossEntityCondition],
    cc: &ChainCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(anchor) = anchor_scope.first().copied() else {
        return;
    };

    // New form: `.none(Assign.status == active)` — infer related entity from conditions.
    // Legacy-compatible: `.none(Assign.status == active)` if first arg is an entity Ref.
    let (related, related_args) = match cc.args.first() {
        Some(PredicateArg::Ref(_)) => {
            let Some(related) = resolve_entity_scope_arg(model, &cc.name, &cc.args[0], ctx, diags)
            else {
                return;
            };
            (related, &cc.args[1..])
        }
        _ => {
            let mut related_scope = Vec::new();
            let preview = collect_cross_chain_conditions(
                model,
                &related_scope,
                &cc.name,
                &cc.args,
                ctx,
                diags,
            );
            add_condition_entities_to_scope(&mut related_scope, &preview);
            let Some(related) = related_scope
                .iter()
                .copied()
                .find(|e| *e != anchor)
                .or_else(|| related_scope.first().copied())
            else {
                return;
            };
            (related, cc.args.as_slice())
        }
    };

    let related_scope = vec![related];
    let related_conditions =
        collect_cross_chain_conditions(model, &related_scope, &cc.name, related_args, ctx, diags);
    if related_conditions.is_empty() {
        return;
    }

    let kind = match cc.name.as_str() {
        "has" => crate::model::QuantifierKind::Has,
        "none" => crate::model::QuantifierKind::None,
        _ => return,
    };
    model
        .quantifier_constraints
        .push(crate::model::QuantifierConstraint {
            anchor,
            guards: guards.to_vec(),
            kind,
            related,
            related_conditions,
        });
}

fn temporal_equals_from_comparison(
    model: &SemanticModel,
    scope: &[EntityKey],
    pred: &str,
    cmp: &Comparison,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<CrossEntityCondition> {
    if cmp.op != CmpOp::Eq {
        return None;
    }

    let (column, model_col) = match &cmp.lhs {
        Operand::Column(column) if scope.len() == 1 => {
            let entity = scope[0];
            let model_col =
                find_entity_column(model, entity, column, cmp.span.clone(), ctx, diags)?;
            (
                QualifiedModelColumnRef {
                    entity,
                    column: column.clone(),
                },
                model_col,
            )
        }
        Operand::QualifiedColumn(qcol) => {
            let entity = resolve_entity_qref(model, pred, &qcol.entity, ctx, diags)?;
            let model_col =
                find_entity_column(model, entity, &qcol.column, qcol.span.clone(), ctx, diags)?;
            (
                QualifiedModelColumnRef {
                    entity,
                    column: qcol.column.clone(),
                },
                model_col,
            )
        }
        _ => return None,
    };

    let value_lit = match &cmp.rhs {
        Operand::Column(value) => {
            // Prefer column-column comparison when RHS names a column in scope.
            if scope.len() == 1 {
                let entity = scope[0];
                if model.entities[entity]
                    .columns
                    .iter()
                    .any(|c| c.name == *value)
                {
                    return None;
                }
            } else if scope
                .iter()
                .any(|ek| model.entities[*ek].columns.iter().any(|c| c.name == *value))
            {
                // Multi-entity bare column RHS is ambiguous for equals; leave to comparison.
                return None;
            }
            value.clone()
        }
        Operand::IntLit(value) => value.clone(),
        Operand::Now => "now".to_string(),
        Operand::QualifiedColumn(_) => return None,
    };
    match parse_effect_value(&model_col, &value_lit) {
        Ok(value) => Some(CrossEntityCondition::Equals { column, value }),
        Err(e) => {
            push_error_cmp(ctx, diags, cmp, e);
            None
        }
    }
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
            if let Some(cond) = temporal_equals_from_comparison(model, scope, pred, cmp, ctx, diags)
            {
                conditions.push(cond);
            } else if let Some(cond) = resolve_cross_condition(model, scope, pred, arg, ctx, diags)
            {
                conditions.push(cond);
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
        return;
    }

    add_condition_entities_to_scope(&mut scope, &requireds);
    model
        .temporal_assertions
        .push(crate::model::TemporalAssertion {
            anchor: *anchor,
            scope,
            requireds,
        });
    model
        .typed_predicates
        .push(TypedPredicate::After { anchor: *anchor });
}
