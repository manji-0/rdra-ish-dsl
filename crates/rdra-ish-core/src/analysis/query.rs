//! `reads(Source, Entity).by(...).order(...).page(...)`: query shapes, read after the
//! foreign keys exist so a key column (`customer_id`) can be filtered on.

use crate::analysis::arg_resolve::resolve_arg;
use crate::analysis_diag::push_error;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use crate::SourceId;
use rdra_ish_syntax::ast::*;

pub(super) fn resolve_queries(
    model: &mut SemanticModel,
    items: &[(SourceId, Item)],
    diags: &mut Vec<Diagnostic>,
) {
    for (source_id, item) in items {
        let Item::Predicate(pred) = item else {
            continue;
        };
        if pred.name != "reads" || pred.chain.is_empty() {
            continue;
        }
        let ctx = DiagCtxt::new(*source_id);
        // The predicate pass already reported what does not resolve.
        let mut quiet = Vec::new();
        let resolved: Vec<Option<NodeRef>> = pred
            .args
            .iter()
            .map(|a| resolve_arg(model, a, ctx, &mut quiet))
            .collect();
        let (Some(Some(source)), Some(Some(NodeRef::Entity(entity)))) =
            (resolved.first(), resolved.get(1))
        else {
            continue;
        };
        if !matches!(source, NodeRef::UseCase(_) | NodeRef::Api(_)) {
            continue;
        }
        if let Some(query) = build_query(model, pred, source.clone(), *entity, ctx, diags) {
            model.queries.push(query);
        }
    }
}

fn build_query(
    model: &SemanticModel,
    pred: &PredicateCall,
    source: NodeRef,
    entity: EntityKey,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<Query> {
    let e = &model.entities[entity];
    let reported = diags.len();
    let fail = |span: &Span, detail: String, diags: &mut Vec<Diagnostic>| {
        push_error(
            ctx,
            diags,
            span.clone(),
            RdraError::InvalidQueryShape { detail },
        );
    };
    let mut query = Query {
        source,
        entity,
        filter: Vec::new(),
        order: Vec::new(),
        page: None,
        aggregate: None,
        site: Some(ctx.locate(pred.span.clone())),
    };
    let mut seen: Vec<&str> = Vec::new();
    for call in &pred.chain {
        let name = call.name.as_str();
        let slot = if name == "sum" { "count" } else { name };
        if seen.contains(&slot) {
            let what = if slot == "count" {
                "one `.count()` or `.sum(...)`".to_string()
            } else {
                format!("one `.{name}(...)`")
            };
            fail(&call.span, format!("a read takes {what}"), diags);
            continue;
        }
        seen.push(slot);
        let words: Vec<Option<Vec<String>>> = call.args.iter().map(arg_words).collect();
        match name {
            "by" | "sum" | "order" if call.args.is_empty() => {
                fail(&call.span, format!("`.{name}(...)` names a column"), diags);
            }
            "by" => {
                for (arg, w) in call.args.iter().zip(&words) {
                    match w.as_deref() {
                        Some([column]) if has_column(e, column) => {
                            query.filter.push(column.clone());
                        }
                        Some([column]) => {
                            unknown_column(ctx, diags, e, column, &call.span);
                        }
                        _ => fail(
                            &arg_span(arg, call),
                            "`.by(...)` takes columns of the entity it reads".to_string(),
                            diags,
                        ),
                    }
                }
            }
            "order" => {
                for (arg, w) in call.args.iter().zip(&words) {
                    let key = match w.as_deref() {
                        Some([column]) => Some((column.clone(), false)),
                        Some([column, dir]) if dir == "desc" || dir == "asc" => {
                            Some((column.clone(), dir == "desc"))
                        }
                        _ => None,
                    };
                    match key {
                        Some((column, desc)) if has_column(e, &column) => {
                            query.order.push(OrderKey { column, desc });
                        }
                        Some((column, _)) => {
                            unknown_column(ctx, diags, e, &column, &call.span);
                        }
                        None => fail(
                            &arg_span(arg, call),
                            "`.order(...)` takes a column, or `column.desc` / `column.asc`"
                                .to_string(),
                            diags,
                        ),
                    }
                }
            }
            "page" => match words.as_slice() {
                [Some(w)] if w.as_slice() == ["cursor"] => query.page = Some(Page::Cursor),
                [Some(w)] if w.as_slice() == ["offset"] => query.page = Some(Page::Offset),
                _ => fail(
                    &call.span,
                    "`.page(...)` takes `cursor` or `offset`".to_string(),
                    diags,
                ),
            },
            "count" if !call.args.is_empty() => {
                fail(
                    &call.span,
                    "`.count()` takes no arguments".to_string(),
                    diags,
                );
            }
            "count" => query.aggregate = Some(Aggregate::Count),
            "sum" => match words.as_slice() {
                [Some(w)] if w.len() == 1 => {
                    let column = &w[0];
                    match e.columns.iter().find(|c| &c.name == column) {
                        Some(c)
                            if matches!(
                                c.col_type,
                                ColumnType::Int | ColumnType::Money | ColumnType::Decimal
                            ) =>
                        {
                            query.aggregate = Some(Aggregate::Sum(column.clone()));
                        }
                        Some(_) => fail(
                            &call.span,
                            format!("`.sum({column})` needs an Int, Money, or Decimal column"),
                            diags,
                        ),
                        None => {
                            unknown_column(ctx, diags, e, column, &call.span);
                        }
                    }
                }
                _ => fail(
                    &call.span,
                    "`.sum(...)` takes one column".to_string(),
                    diags,
                ),
            },
            _ => {}
        }
    }
    if query.page == Some(Page::Cursor) && query.order.is_empty() {
        fail(
            &pred.span,
            "`.page(cursor)` continues after the sort key of the last row, so it needs `.order(...)`"
                .to_string(),
            diags,
        );
    }
    if query.aggregate.is_some() && (!query.order.is_empty() || query.page.is_some()) {
        fail(
            &pred.span,
            "`.count()` and `.sum(...)` return one row, so they take no `.order(...)` or `.page(...)`"
                .to_string(),
            diags,
        );
    }
    (diags.len() == reported).then_some(query)
}

/// The dotted words of a bare argument: `status` → `["status"]`, `created_at.desc` →
/// `["created_at", "desc"]`.
fn arg_words(arg: &PredicateArg) -> Option<Vec<String>> {
    match arg {
        PredicateArg::Ref(q) if q.kind_qualifier.is_none() => Some(q.parts.clone()),
        PredicateArg::Lit(s) => Some(vec![s.clone()]),
        _ => None,
    }
}

fn arg_span(arg: &PredicateArg, call: &ChainCall) -> Span {
    match arg {
        PredicateArg::Ref(q) => q.span.clone(),
        _ => call.span.clone(),
    }
}

fn has_column(entity: &Entity, column: &str) -> bool {
    entity.columns.iter().any(|c| c.name == column)
}

fn unknown_column(
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
    entity: &Entity,
    column: &str,
    span: &Span,
) {
    push_error(
        ctx,
        diags,
        span.clone(),
        RdraError::UnknownColumn {
            entity: entity.id.clone(),
            col: column.to_string(),
        },
    );
}
