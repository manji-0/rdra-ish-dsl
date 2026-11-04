//! The `sets`, `links`, and `drives` predicates: what a use case writes to a column or a key.

use super::transitions::when_condition;
use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use crate::analysis::comparison::resolve_comparison;

pub(super) fn process_sets_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let before = diags.len();
    let Ok(guard) = when_condition(model, pred, ctx, diags) else {
        return;
    };
    let Some(target) = effect_target(model, pred, resolved, ctx, diags) else {
        return;
    };
    let recorded = target.entity.is_some_and(|entity| {
        record_sets_effect(
            model,
            pred,
            target.origin,
            entity,
            target.cmp,
            guard,
            ctx,
            diags,
        )
    });
    if !recorded {
        let err = RdraError::SetsNoEffect {
            usecase: model.use_cases[target.origin].id.clone(),
            entity: target.entity_id(model),
        };
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
    }
}

/// `links(UseCase, Child, Parent)`, `relinks(..)` or `unlinks(..)` (`kind`), with
/// `.role(role)` where several keys join the two. The key is picked once the keys exist
/// (`fk::resolve_links`).
pub(super) fn process_links_predicate(
    model: &mut SemanticModel,
    kind: LinkKind,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
) {
    let (
        Some(Some(NodeRef::UseCase(origin))),
        Some(Some(NodeRef::Entity(child))),
        Some(Some(NodeRef::Entity(parent))),
    ) = (resolved.first(), resolved.get(1), resolved.get(2))
    else {
        return;
    };
    model.links.push(Link {
        kind,
        origin: *origin,
        child: *child,
        parent: *parent,
        by: crate::analysis::constraint::selectors(&pred.chain, None),
        key: None,
        site: Some(ctx.locate(pred.span.clone())),
    });
    model.typed_predicates.push(TypedPredicate::Links {
        kind,
        origin: *origin,
        child: *child,
        parent: *parent,
    });
}

pub(super) fn process_drives_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let before = diags.len();
    let Some(target) = effect_target(model, pred, resolved, ctx, diags) else {
        return;
    };
    let recorded = target.entity.is_some_and(|entity| {
        record_drives_effect(model, pred, target.origin, entity, target.cmp, ctx, diags)
    });
    if !recorded {
        let err = RdraError::DrivesNoEffect {
            usecase: model.use_cases[target.origin].id.clone(),
            entity: target.entity_id(model),
        };
        push_error_unless_reported(ctx, diags, before, pred.span.clone(), err);
    }
}

/// What a `sets` or `drives` writes: the use case it names first, and the entity and
/// comparison of its second argument, `Entity.column op rhs`.
struct EffectTarget<'a> {
    origin: UseCaseKey,
    /// `None` when the second argument names no entity column; the caller reports it.
    entity: Option<EntityKey>,
    cmp: Option<&'a Comparison>,
}

impl EffectTarget<'_> {
    fn entity_id(&self, model: &SemanticModel) -> String {
        self.entity
            .map_or_else(|| "Entity".to_string(), |k| model.entities[k].id.clone())
    }
}

/// The target of a `sets` or `drives`, or `None` when its use case or its entity does not
/// resolve, either of which is already reported.
fn effect_target<'a>(
    model: &SemanticModel,
    pred: &'a PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<EffectTarget<'a>> {
    let Some(Some(NodeRef::UseCase(origin))) = resolved.first() else {
        return None;
    };
    let cmp = match pred.args.get(1) {
        Some(PredicateArg::Expr(Expr::Cmp(cmp))) => Some(cmp),
        _ => None,
    };
    let entity = match cmp.map(|cmp| &cmp.lhs) {
        Some(Operand::QualifiedColumn(qcol)) => {
            Some(crate::analysis::constraint::resolve_entity_qref(
                model,
                &pred.name,
                &qcol.entity,
                ctx,
                diags,
            )?)
        }
        _ => None,
    };
    Some(EffectTarget {
        origin: *origin,
        entity,
        cmp,
    })
}

/// `cmp`, or `None` after reporting a set membership: a use case leaves one value.
fn effect_comparison<'a>(
    pred: &PredicateCall,
    cmp: Option<&'a Comparison>,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<&'a Comparison> {
    let cmp = cmp?;
    if cmp.op.is_membership() {
        let col = match &cmp.lhs {
            Operand::Column(name) => name.clone(),
            Operand::QualifiedColumn(q) => q.column.clone(),
            _ => String::new(),
        };
        push_error(
            ctx,
            diags,
            cmp.span.clone(),
            RdraError::EffectMembership {
                pred: pred.name.clone(),
                col,
            },
        );
        return None;
    }
    Some(cmp)
}

#[allow(clippy::too_many_arguments)]
fn push_column_effect(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    origin: UseCaseKey,
    entity: EntityKey,
    column: String,
    value: SetValue,
    guard: Option<ConditionKey>,
    ctx: DiagCtxt,
) {
    let by = crate::analysis::constraint::selectors(&pred.chain, None);
    model.column_effects.push(ColumnEffect {
        site: Some(ctx.locate(pred.span.clone())),
        origin,
        entity,
        column: column.clone(),
        value,
        by,
        guard,
    });
    model.typed_predicates.push(TypedPredicate::SetsColumn {
        origin,
        entity,
        column,
    });
}

/// Records the column effect `sets(UC, E.column == value)` states, with its `.when(Cond)`
/// guard, and says whether it did.
#[allow(clippy::too_many_arguments)]
fn record_sets_effect(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    origin: UseCaseKey,
    entity_key: EntityKey,
    cmp: Option<&Comparison>,
    guard: Option<ConditionKey>,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let Some(cmp) = effect_comparison(pred, cmp, ctx, diags) else {
        return false;
    };
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    let value = match crate::analysis::constraint::resolve_entity_equals_from_comparison(
        &model.variations,
        &entity_cols,
        &entity_id,
        cmp,
        ctx,
        diags,
    ) {
        Ok(Some((column, value))) => Some((column, SetValue::Value(value))),
        Ok(None) => match crate::analysis::effect::resolve_copy(model, entity_key, cmp, ctx, diags)
        {
            Ok(copy) => copy.map(|(column, source)| (column, SetValue::Copy(source))),
            Err(()) => None,
        },
        Err(()) => None,
    };
    let Some((column, value)) = value else {
        return false;
    };
    match &value {
        SetValue::Value(_) => {
            crate::analysis::constraint::report_leading_by(&pred.chain, ctx, diags)
        }
        SetValue::Copy(source) if guard.is_some() => {
            push_error(
                ctx,
                diags,
                pred.span.clone(),
                RdraError::SetsWhenOnCopy {
                    usecase: model.use_cases[origin].id.clone(),
                    column: format!("{entity_id}.{column}"),
                    copied: format!("{}.{}", model.entities[source.entity].id, source.column),
                },
            );
            return true;
        }
        SetValue::Copy(_) => {}
    }
    push_column_effect(model, pred, origin, entity_key, column, value, guard, ctx);
    true
}

/// Records the comparison `drives(UC, E.column op rhs, true|false)` drives, and says whether
/// it did. A value equality (`E.status == paid, true`) sets the value, as `sets` does; driving
/// one `false` is an error, since it names no value to leave.
fn record_drives_effect(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    origin: UseCaseKey,
    entity_key: EntityKey,
    cmp: Option<&Comparison>,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let truth = match pred.args.get(2) {
        Some(PredicateArg::Ref(q)) if q.kind_qualifier.is_none() && q.parts.len() == 1 => {
            q.parts[0].as_str()
        }
        Some(PredicateArg::Lit(s)) => s.as_str(),
        _ => return false,
    };
    let truth = match truth {
        "true" => true,
        "false" => false,
        _ => return false,
    };
    let Some(cmp) = effect_comparison(pred, cmp, ctx, diags) else {
        return false;
    };
    let entity_id = model.entities[entity_key].id.clone();
    let entity_cols = model.entities[entity_key].columns.clone();
    match crate::analysis::constraint::resolve_entity_equals_from_comparison(
        &model.variations,
        &entity_cols,
        &entity_id,
        cmp,
        ctx,
        diags,
    ) {
        Ok(Some((column, value))) if truth => {
            let value = SetValue::Value(value);
            push_column_effect(model, pred, origin, entity_key, column, value, None, ctx);
            true
        }
        Ok(Some((col, _))) => {
            let value = match &cmp.rhs {
                Operand::Column(v) | Operand::IntLit(v) => v.clone(),
                Operand::Now => "now".into(),
                Operand::QualifiedColumn(q) => q.column.clone(),
                Operand::Values(vs) => format!("({})", vs.join(", ")),
            };
            push_error(
                ctx,
                diags,
                cmp.span.clone(),
                RdraError::DrivesFalseOnEquals { col, value },
            );
            false
        }
        Ok(None) => {
            let Some(prop) =
                resolve_comparison(&model.variations, &entity_cols, &entity_id, cmp, ctx, diags)
            else {
                return false;
            };
            model.proposition_effects.push(PropositionEffect {
                site: Some(ctx.locate(pred.span.clone())),
                origin,
                entity: entity_key,
                prop: prop.clone(),
                truth,
            });
            model.typed_predicates.push(TypedPredicate::Drives {
                origin,
                entity: entity_key,
                prop,
                truth,
            });
            true
        }
        Err(()) => false,
    }
}
