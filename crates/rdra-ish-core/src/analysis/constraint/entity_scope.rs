//! Entity and column reference resolution for constraint arguments.

use crate::analysis::qref_util::qref_display;
use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

pub(crate) fn resolve_entity_qref(
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
                        import: None,
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
                    actual: node.kind_tag().to_string(),
                    expected: "entity".to_string(),
                },
            );
            None
        }
        LookupResult::NotFound => {
            push_error_qref(
                ctx,
                diags,
                qref,
                RdraError::UndefinedSymbol { id, import: None },
            );
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

pub(super) fn resolve_entity_scope_arg(
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

pub(crate) fn find_entity_column(
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
