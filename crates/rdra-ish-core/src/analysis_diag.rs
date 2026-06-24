//! Diagnostic push helpers shared by semantic analysis.

use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::DiagCtxt;
use crate::model::SemanticModel;
use rdra_ish_syntax::ast::{Comparison, Expr, PredicateArg, QRef, Span};

pub(crate) fn push_error(ctx: DiagCtxt, diags: &mut Vec<Diagnostic>, span: Span, err: RdraError) {
    diags.push(Diagnostic::error_at(err, ctx.locate(span)));
}

pub(crate) fn push_warning(ctx: DiagCtxt, diags: &mut Vec<Diagnostic>, span: Span, err: RdraError) {
    diags.push(Diagnostic::warning_at(err, ctx.locate(span)));
}

pub(crate) fn push_error_cmp(
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
    cmp: &Comparison,
    err: RdraError,
) {
    push_error(ctx, diags, cmp.span.clone(), err);
}

pub(crate) fn push_error_qref(
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
    qref: &QRef,
    err: RdraError,
) {
    push_error(ctx, diags, qref.span.clone(), err);
}

pub(crate) fn push_error_parse_effect(
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
    value_arg: &PredicateArg,
    err: RdraError,
) {
    push_error(ctx, diags, arg_span(value_arg), err);
}

pub(crate) fn arg_span(arg: &PredicateArg) -> Span {
    match arg {
        PredicateArg::Ref(qref) => qref.span.clone(),
        PredicateArg::Expr(Expr::Cmp(cmp)) => cmp.span.clone(),
        PredicateArg::Lit(_) | PredicateArg::Tuple(_) => 0..0,
    }
}

pub(crate) fn push_error_arg(
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
    args: &[PredicateArg],
    index: usize,
    err: RdraError,
) {
    let span = args.get(index).map(arg_span).unwrap_or(0..0);
    push_error(ctx, diags, span, err);
}

pub(crate) fn push_entity_error(
    model: &SemanticModel,
    diags: &mut Vec<Diagnostic>,
    entity_id: &str,
    err: RdraError,
) {
    if let Some(loc) = model.decl_sites.located("entity", entity_id) {
        diags.push(Diagnostic::error_at(err, loc));
    } else {
        diags.push(Diagnostic::error(err));
    }
}
