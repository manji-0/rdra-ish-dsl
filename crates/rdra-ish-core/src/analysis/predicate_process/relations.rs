//! The relation predicates: relate kinds and cards, `navigates`, and `belongs` context.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use crate::analysis::arg_resolve::resolve_arg;
use crate::analysis::constraint::context_value_from_arg;

fn process_belongs_context(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    from: &NodeRef,
    to: &NodeRef,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let (NodeRef::Buc(buc), NodeRef::Business(business)) = (from, to) else {
        return;
    };
    let mut bys = Vec::new();

    for cc in pred.chain.iter().filter(|cc| cc.name == "by") {
        for arg in &cc.args {
            if let Some(value) = context_value_from_arg(model, arg, "medium", ctx, diags) {
                bys.push(value);
            }
        }
    }

    if !bys.is_empty() {
        model
            .business_mapping_contexts
            .push(BusinessMappingContext {
                buc: *buc,
                business: *business,
                bys,
            });
    }
}

pub(super) fn process_relation_predicate(
    model: &mut SemanticModel,
    kind: RelKind,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    if let (Some(Some(from)), Some(Some(to))) = (resolved.first(), resolved.get(1)) {
        let belongs = kind == RelKind::Belongs;
        model.relations.push(Relation {
            site: Some(ctx.locate(pred.span.clone())),
            from: from.clone(),
            to: to.clone(),
            kind,
            options: RelationOptions::default(),
            via: None,
        });
        if belongs {
            process_belongs_context(model, pred, from, to, ctx, diags);
        }
    }
}

/// `navigates(Screen, Screen)` with at most one `.via(UseCase)`: the use case whose operation
/// moves the user from the first screen to the second.
pub(super) fn process_navigates_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let mut via = None;
    for (index, cc) in pred.chain.iter().filter(|cc| cc.name == "via").enumerate() {
        let span = cc.span.clone();
        if index > 0 || cc.args.len() != 1 {
            push_error(ctx, diags, span, RdraError::NavigatesViaRequiresUseCase);
            continue;
        }
        match resolve_arg(model, &cc.args[0], ctx, diags) {
            Some(NodeRef::UseCase(usecase)) => via = Some(usecase),
            Some(_) => push_error(ctx, diags, span, RdraError::NavigatesViaRequiresUseCase),
            // resolve_arg already reported an undefined or ambiguous reference.
            None if matches!(cc.args.first(), Some(PredicateArg::Ref(_))) => {}
            None => push_error(ctx, diags, span, RdraError::NavigatesViaRequiresUseCase),
        }
    }
    let (Some(Some(NodeRef::Screen(from))), Some(Some(NodeRef::Screen(to)))) =
        (resolved.first(), resolved.get(1))
    else {
        return;
    };
    model.relations.push(Relation {
        site: Some(ctx.locate(pred.span.clone())),
        from: NodeRef::Screen(*from),
        to: NodeRef::Screen(*to),
        kind: RelKind::Navigates,
        options: RelationOptions::default(),
        via: via.map(NodeRef::UseCase),
    });
}

/// Resolve `.via(Entity)` from the relate chain. Returns `(via, had_via_call)`.
fn resolve_relate_via(
    model: &SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> (Option<NodeRef>, bool) {
    let mut via = None;
    let mut had_via = false;
    for cc in &pred.chain {
        if cc.name != "via" {
            continue;
        }
        had_via = true;
        let span = cc.span.clone();
        if cc.args.len() != 1 {
            push_error(ctx, diags, span, RdraError::ViaRequiresEntity);
            continue;
        }
        match resolve_arg(model, &cc.args[0], ctx, diags) {
            Some(node @ NodeRef::Entity(_)) => {
                via = Some(node);
            }
            Some(_) => {
                push_error(ctx, diags, span, RdraError::ViaRequiresEntity);
            }
            None => {
                // resolve_arg already reported undefined / ambiguity when Ref.
                if !matches!(cc.args.first(), Some(PredicateArg::Ref(_))) {
                    push_error(ctx, diags, span, RdraError::ViaRequiresEntity);
                }
            }
        }
    }
    (via, had_via)
}

/// A card's kind and its child-side minimum and maximum. A side is `1`, `N`/`M`, or `lo..hi`
/// with `lo` in {0, 1} and `hi` in {1, 2..=255, *}; a range stands for the side `1` (hi 1)
/// or `N`/`M` (hi above 1 or *), and only the child side, the many side of N:1 / 1:N and the
/// left of 1:1, may carry one. The maximum is `hi` from 2 to 255, `None` for 1 and `*`.
/// `Ok(None)` for a card no relate kind names, which today's forms drop silently.
fn parse_card(card: &str) -> Result<Option<(RelKind, Min, Option<u8>)>, RdraError> {
    let Some((left, right)) = card.split_once(':') else {
        return Ok(None);
    };
    let range = |side: &str| -> Result<Option<(Min, bool, Option<u8>)>, RdraError> {
        let Some((lo, hi)) = side.split_once("..") else {
            return Ok(None);
        };
        let out_of_range = || RdraError::MinCardOutOfRange {
            range: side.to_string(),
        };
        let min = match lo {
            "0" => Min::Zero,
            "1" => Min::One,
            _ => return Err(out_of_range()),
        };
        let (many, most) = match hi {
            "1" => (false, None),
            "*" => (true, None),
            n => match n.parse::<u8>() {
                Ok(n) if n >= 2 && n.to_string() == hi => (true, Some(n)),
                _ => return Err(out_of_range()),
            },
        };
        Ok(Some((min, many, most)))
    };
    let (l, r) = (range(left)?, range(right)?);
    let base = |side: &'static str, written: &str, r: Option<(Min, bool, Option<u8>)>| match r {
        Some((_, true, _)) => side,
        Some((_, false, _)) => "1",
        None => match written {
            "1" => "1",
            "N" => "N",
            "M" => "M",
            _ => "?",
        },
    };
    let left_base = base("N", left, l);
    let right_base = base(if left_base == "1" { "N" } else { "M" }, right, r);
    let kind = match (left_base, right_base) {
        ("1", "1") => RelKind::RelateOneToOne,
        ("1", "N") => RelKind::RelateOneToMany,
        ("N", "1") => RelKind::RelateManyToOne,
        ("N", "M") => RelKind::RelateManyToMany,
        _ => return Ok(None),
    };
    let (child, parent) = match kind {
        RelKind::RelateOneToMany => (r, l),
        RelKind::RelateManyToMany if l.is_some() || r.is_some() => {
            return Err(RdraError::MinCardOnManyToMany {
                card: card.to_string(),
            })
        }
        _ => (l, r),
    };
    if parent.is_some() {
        return Err(RdraError::MinCardOnParentSide {
            card: card.to_string(),
        });
    }
    let (min, most) = child.map_or((Min::Zero, None), |(min, _, most)| (min, most));
    Ok(Some((kind, min, most)))
}

/// The relate card argument's span, or the whole call's when the card has none.
fn card_span(pred: &PredicateCall) -> Span {
    pred.args
        .get(2)
        .map(arg_span)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| pred.span.clone())
}

/// The `.via(..)` chain call's span, or the whole call's when absent.
fn via_span(pred: &PredicateCall) -> Span {
    pred.chain
        .iter()
        .find(|cc| cc.name == "via")
        .map(|cc| cc.span.clone())
        .unwrap_or_else(|| pred.span.clone())
}

pub(super) fn process_relate_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let card_str = match pred.args.get(2) {
        Some(PredicateArg::Card(c)) => c.as_str(),
        Some(PredicateArg::Lit(c)) => c.as_str(),
        _ => return,
    };
    let (Some(Some(from)), Some(Some(to))) = (resolved.first(), resolved.get(1)) else {
        return;
    };

    let (via, had_via) = resolve_relate_via(model, pred, ctx, diags);
    let mut options = crate::analysis::fk::relation_options(pred, ctx, diags);

    let (kind, children_min, children_most) = match parse_card(card_str) {
        Ok(Some(card)) => card,
        Ok(None) => return,
        Err(err) => {
            push_error(ctx, diags, card_span(pred), err);
            return;
        }
    };
    options.children_min = children_min;
    options.children_most = children_most;

    let many_to_many = kind == RelKind::RelateManyToMany;
    if had_via && !many_to_many {
        push_error(ctx, diags, via_span(pred), RdraError::ViaOnlyForNM);
        return;
    }

    if many_to_many {
        if !many_to_many_is_valid(model, pred, (from, to), via.as_ref(), had_via, ctx, diags) {
            return;
        }
        // The junction relates synthesized from this one must not inherit them.
        options.role = None;
        options.key.clear();
    }

    model.relations.push(Relation {
        site: Some(ctx.locate(pred.span.clone())),
        from: from.clone(),
        to: to.clone(),
        kind,
        options: options.clone(),
        via: via.clone(),
    });

    // N:M.via(J) ≡ two N:1 edges J→A, J→B so TLA/ER/.along/has_relate_edge see the junction path.
    if let (true, NodeRef::Entity(_), NodeRef::Entity(_), Some(NodeRef::Entity(junction))) =
        (many_to_many, from, to, via.as_ref())
    {
        let junction_ref = NodeRef::Entity(*junction);
        for end in [from.clone(), to.clone()] {
            if !relation_exists(model, &junction_ref, &end, RelKind::RelateManyToOne)
                && !relation_exists(model, &end, &junction_ref, RelKind::RelateOneToMany)
            {
                model.relations.push(Relation {
                    site: Some(ctx.locate(pred.span.clone())),
                    from: junction_ref.clone(),
                    to: end,
                    kind: RelKind::RelateManyToOne,
                    options: options.clone(),
                    via: None,
                });
            }
        }
    }
}

/// Reports what is wrong with an N:M relate and returns whether it is sound.
/// Errors come in a fixed order: `.role`/`.key` first, then a missing `.via`,
/// then a self relation, then a junction that is one of the two ends.
fn many_to_many_is_valid(
    model: &SemanticModel,
    pred: &PredicateCall,
    (from, to): (&NodeRef, &NodeRef),
    via: Option<&NodeRef>,
    had_via: bool,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    for cc in pred
        .chain
        .iter()
        .filter(|cc| matches!(cc.name.as_str(), "role" | "key"))
    {
        let err = RdraError::KeyOptionOnManyToMany {
            method: cc.name.clone(),
        };
        push_error(ctx, diags, cc.span.clone(), err);
    }

    if via.is_none() {
        // If `.via` was present but failed to resolve, keep that diagnostic only.
        if !had_via {
            let entity_id = |node: &NodeRef| match node {
                NodeRef::Entity(k) => model.entities[*k].id.clone(),
                _ => "?".into(),
            };
            let err = RdraError::NMRelation {
                from: entity_id(from),
                to: entity_id(to),
            };
            push_error(ctx, diags, card_span(pred), err);
        }
        return false;
    }

    let (NodeRef::Entity(fk), NodeRef::Entity(tk)) = (from, to) else {
        return true;
    };
    if fk == tk {
        let err = RdraError::SelfNMRelation {
            entity: model.entities[*fk].id.clone(),
        };
        push_error(ctx, diags, pred.span.clone(), err);
        return false;
    }
    if let Some(NodeRef::Entity(jk)) = via {
        if jk == fk || jk == tk {
            let err = RdraError::ViaMustBeDistinct {
                from: model.entities[*fk].id.clone(),
                to: model.entities[*tk].id.clone(),
                via: model.entities[*jk].id.clone(),
            };
            push_error(ctx, diags, via_span(pred), err);
            return false;
        }
    }
    true
}

fn relation_exists(model: &SemanticModel, from: &NodeRef, to: &NodeRef, kind: RelKind) -> bool {
    model
        .relations
        .iter()
        .any(|r| r.from == *from && r.to == *to && r.kind == kind)
}
