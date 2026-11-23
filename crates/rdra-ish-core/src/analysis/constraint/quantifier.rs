//! Related-entity quantifiers (`.has` / `.none`) and their path checks.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use super::cross::{add_condition_entities_to_scope, collect_cross_chain_conditions};
use super::entity_conditions::push_unique_entity;
use super::entity_scope::resolve_entity_scope_arg;

/// The related entity, conditions and kind of one `.has(...)` / `.none(...)` chain call. The
/// related entity is the one a bare first argument names (`.none(Permit)`, any row at all), or
/// else the first one the conditions name other than `anchor`. `by` holds the `.role(role)`
/// selectors that follow the call. `None` only with an error reported, so a quantifier is
/// never dropped silently.
pub(crate) fn resolve_related_quantifier(
    model: &SemanticModel,
    anchor: EntityKey,
    cc: &ChainCall,
    by: Vec<String>,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<crate::model::RelatedQuantifier> {
    let before = diags.len();
    let quantifier = related_quantifier(model, anchor, cc, by, ctx, diags);
    if quantifier.is_none() {
        let err = RdraError::QuantifierWithoutCondition {
            method: cc.name.clone(),
        };
        push_error_unless_reported(ctx, diags, before, cc.span.clone(), err);
    }
    quantifier
}

/// The `.along` path of a `.has`/`.none` invariant chains the counted rows to the anchor's
/// when it runs from the anchor to the counted entity, each entity once. Any other shape is
/// `QuantifierAlongShape`, with the path rewritten as the shortest key path from the anchor to
/// the counted entity, or, with no key path, from the anchor to the counted entity through the
/// other path entities in order. Runs once the keys exist; a misshapen rule is dropped.
pub(crate) fn check_quantifier_paths(model: &mut SemanticModel, diags: &mut Vec<Diagnostic>) {
    let constraints = std::mem::take(&mut model.quantifier_constraints);
    for c in constraints {
        match quantifier_path_misfit(model, &c) {
            None => model.quantifier_constraints.push(c),
            Some(err) => diags.push(match &c.quantifier.site {
                Some(site) => Diagnostic::error_at(err, site.clone()),
                None => Diagnostic::error(err),
            }),
        }
    }
}

fn quantifier_path_misfit(
    model: &SemanticModel,
    c: &crate::model::QuantifierConstraint,
) -> Option<RdraError> {
    let CrossConstraintScope::RelationPath { path, .. } = &c.along else {
        return None;
    };
    let (anchor, q) = (c.anchor, &c.quantifier);
    let mut seen: Vec<EntityKey> = Vec::new();
    for ek in path {
        push_unique_entity(&mut seen, *ek);
    }
    let each_once = seen.len() == path.len();
    if path.len() >= 2 && path[0] == anchor && path[path.len() - 1] == q.related && each_once {
        return None;
    }
    let id = |ek: &EntityKey| model.entities[*ek].id.clone();
    let fix = model.key_path(&[anchor], q.related).unwrap_or_else(|| {
        let mut fix = vec![anchor];
        for ek in path {
            if *ek != anchor && *ek != q.related {
                push_unique_entity(&mut fix, *ek);
            }
        }
        fix.push(q.related);
        fix
    });
    Some(RdraError::QuantifierAlongShape {
        method: q.kind.method().to_string(),
        ends: format!("{} to {}", id(&anchor), id(&q.related)),
        path: path.iter().map(id).collect::<Vec<_>>().join(", "),
        fix: fix.iter().map(id).collect::<Vec<_>>().join(", "),
    })
}

fn related_quantifier(
    model: &SemanticModel,
    anchor: EntityKey,
    cc: &ChainCall,
    by: Vec<String>,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<crate::model::RelatedQuantifier> {
    let (kind, args) = match cc.name.as_str() {
        "has" => (crate::model::QuantifierKind::Has, cc.args.as_slice()),
        "none" => (crate::model::QuantifierKind::None, cc.args.as_slice()),
        method => {
            let bound = crate::model::CountBound::of_method(method)?;
            let n = count_arg(cc, bound, ctx, diags)?;
            (
                crate::model::QuantifierKind::Count { bound, n },
                &cc.args[1..],
            )
        }
    };
    // `.none(Permit)` and `.none(Permit, Permit.status == x)` name the related entity first;
    // `.none(Permit.status == x)` names it in the conditions.
    let (related, related_args) = match args.first() {
        Some(PredicateArg::Ref(_)) => {
            let related = resolve_entity_scope_arg(model, &cc.name, &args[0], ctx, diags)?;
            (related, &args[1..])
        }
        _ => {
            let mut related_scope = Vec::new();
            let preview =
                collect_cross_chain_conditions(model, &related_scope, &cc.name, args, ctx, diags);
            add_condition_entities_to_scope(&mut related_scope, &preview);
            let related = related_scope
                .iter()
                .copied()
                .find(|e| *e != anchor)
                .or_else(|| related_scope.first().copied())?;
            (related, args)
        }
    };

    let related_scope = vec![related];
    let related_conditions =
        collect_cross_chain_conditions(model, &related_scope, &cc.name, related_args, ctx, diags);
    if related_conditions.is_empty() && !related_args.is_empty() {
        return None;
    }
    Some(crate::model::RelatedQuantifier {
        kind,
        related,
        conditions: related_conditions,
        by,
        site: Some(ctx.locate(cc.span.clone())),
    })
}

/// The count of `.at_most(n, ..)`, `.at_least(n, ..)` or `.exactly(n, ..)`: its first argument,
/// a whole number from 1 to 255. A count another form says (`at_least(1)` is `.has`,
/// `at_most(0)` and `exactly(0)` are `.none`, `at_least(0)` always holds) is
/// `QuantifierCountSpelling`; anything else is `QuantifierCountInvalid`. Each reports its
/// error and gives `None`.
fn count_arg(
    cc: &ChainCall,
    bound: crate::model::CountBound,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<u8> {
    use crate::model::CountBound::*;
    let invalid = |found: Option<String>| RdraError::QuantifierCountInvalid {
        method: cc.name.clone(),
        found,
    };
    let n = match cc.args.first() {
        Some(PredicateArg::Int(text)) => match text.parse::<u8>() {
            Ok(n) => n,
            Err(_) => {
                push_error(ctx, diags, cc.span.clone(), invalid(Some(text.clone())));
                return None;
            }
        },
        Some(PredicateArg::Ref(q)) => {
            let found = q.parts.join(".");
            push_error(ctx, diags, cc.span.clone(), invalid(Some(found)));
            return None;
        }
        Some(PredicateArg::Lit(text)) => {
            push_error(
                ctx,
                diags,
                cc.span.clone(),
                invalid(Some(format!("\"{text}\""))),
            );
            return None;
        }
        _ => {
            push_error(ctx, diags, cc.span.clone(), invalid(None));
            return None;
        }
    };
    let written = format!("{}({n}", cc.name);
    let respell = |meaning: &str, fix: &str| RdraError::QuantifierCountSpelling {
        written: written.clone(),
        meaning: meaning.to_string(),
        fix: fix.to_string(),
    };
    let err = match (bound, n) {
        (AtLeast, 0) => respell(
            "true of every row: no count is below 0",
            "remove it, or write the least count the rule asks for",
        ),
        (AtLeast, 1) => respell(
            "the rule `.has(...)` states",
            "write `.has(...)` with the same arguments after the count",
        ),
        (AtMost | Exactly, 0) => respell(
            "the rule `.none(...)` states",
            "write `.none(...)` with the same arguments after the count",
        ),
        _ => return Some(n),
    };
    push_error(ctx, diags, cc.span.clone(), err);
    None
}
