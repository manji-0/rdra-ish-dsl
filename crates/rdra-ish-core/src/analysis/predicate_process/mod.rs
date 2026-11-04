//! Predicate argument resolution and semantic processing.

mod args;
mod effects;
mod relations;
mod rules;
mod structure;
mod transitions;

use self::args::arity_error;
use self::args::resolve_predicate_args;
use self::args::validate_contains_pair;
use self::args::validate_predicate_arg_types;
use self::args::validate_trigger_pair;
use self::effects::process_drives_predicate;
use self::effects::process_links_predicate;
use self::effects::process_sets_predicate;
use self::relations::process_navigates_predicate;
use self::relations::process_relate_predicate;
use self::relations::process_relation_predicate;
use self::rules::process_exclusive_predicate;
use self::rules::process_forbidden_predicate;
use self::rules::process_invariant_predicate;
use self::rules::process_required_predicate;
use self::structure::process_coordinates_predicate;
use self::structure::process_exposes_predicate;
use self::structure::process_maps_field_predicate;
use self::structure::process_outbox_predicate;
use self::structure::process_shares_predicate;
use self::transitions::process_chooses_predicate;
use self::transitions::process_transitions_predicate;
use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use crate::predicate::{predicate_chain_methods, predicate_signature, unknown_chain_method_hint};
use rdra_ish_syntax::ast::*;

use super::constraint::{
    process_after_predicate, process_forbidden as process_cross_forbidden,
    process_invariant as process_cross_invariant,
};

pub(super) fn predicate_arg_display(arg: &PredicateArg) -> String {
    match arg {
        PredicateArg::Ref(q) => {
            let id = q.parts.last().cloned().unwrap_or_default();
            match &q.kind_qualifier {
                Some(k) => format!("{}::{}", k.name(), id),
                None => id,
            }
        }
        PredicateArg::Lit(s) => s.clone(),
        PredicateArg::Expr(_) => "<expr>".to_string(),
        PredicateArg::Transition { from, to } => {
            format!("{} -> {to}", rdra_ish_syntax::format::format_sources(from))
        }
        PredicateArg::Card(c) => c.clone(),
    }
}

pub(crate) fn process_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(sig) = predicate_signature(&pred.name) else {
        push_error(
            ctx,
            diags,
            pred.span.clone(),
            RdraError::UnknownPredicate {
                name: pred.name.clone(),
            },
        );
        return;
    };

    let allowed = predicate_chain_methods(&pred.name);
    for cc in &pred.chain {
        if !allowed.contains(&cc.name.as_str()) {
            push_error(
                ctx,
                diags,
                pred.span.clone(),
                RdraError::UnknownChainMethod {
                    pred: pred.name.clone(),
                    name: cc.name.clone(),
                    hint: unknown_chain_method_hint(&pred.name, &cc.name),
                },
            );
        }
    }

    if let Some(err) = arity_error(pred, &sig) {
        push_error(ctx, diags, pred.span.clone(), err);
        return;
    }

    // Unified forbidden/invariant: detect cross-entity vs single-entity form.
    // Cross-entity: `.along(...)`, two or more leading entity refs
    // (e.g. `forbidden(Order, Payment, Order.status == x, ...)`), or an invariant whose
    // `.has/.none(...)` ranges over related rows. Single-entity: `forbidden(Order, status == x)`.
    if pred.name == "forbidden" || pred.name == "invariant" {
        let leading_entities = pred
            .args
            .iter()
            .take_while(|a| matches!(a, PredicateArg::Ref(_)))
            .count();
        let is_cross = pred.chain.iter().any(|cc| cc.name == "along")
            || leading_entities >= 2
            || (pred.name == "invariant"
                && pred
                    .chain
                    .iter()
                    .any(|cc| matches!(cc.name.as_str(), "has" | "none")));
        if is_cross {
            match pred.name.as_str() {
                "forbidden" => process_cross_forbidden(model, pred, ctx, diags),
                "invariant" => process_cross_invariant(model, pred, ctx, diags),
                _ => {}
            }
            return;
        }
        if let Some(role) = pred.chain.iter().find(|cc| cc.name == "role") {
            push_error(ctx, diags, role.span.clone(), RdraError::RoleWithoutJoin);
        }
    }

    let resolved = resolve_predicate_args(model, pred, &sig, ctx, diags);
    validate_predicate_arg_types(pred, &sig, &resolved, ctx, diags);
    if !validate_contains_pair(pred, &resolved, ctx, diags)
        || !validate_trigger_pair(pred, &sig, &resolved, ctx, diags)
    {
        return;
    }

    match pred.name.as_str() {
        "coordinates" => process_coordinates_predicate(model, &resolved),
        "transitions" => process_transitions_predicate(model, pred, &resolved, ctx, diags),
        "outbox" => process_outbox_predicate(model, &resolved),
        "exposes" => process_exposes_predicate(model, pred, &resolved, ctx, diags),
        "shares" => process_shares_predicate(model, pred, &resolved, ctx),
        "after" => process_after_predicate(model, pred, &resolved, ctx, diags),
        "sets" => process_sets_predicate(model, pred, &resolved, ctx, diags),
        "drives" => process_drives_predicate(model, pred, &resolved, ctx, diags),
        "links" => process_links_predicate(model, LinkKind::Links, pred, &resolved, ctx),
        "relinks" => process_links_predicate(model, LinkKind::Relinks, pred, &resolved, ctx),
        "unlinks" => process_links_predicate(model, LinkKind::Unlinks, pred, &resolved, ctx),
        "chooses" => process_chooses_predicate(model, pred, &resolved, ctx, diags),
        "combines" => {
            super::condition::process_combines_predicate(model, pred, &resolved, ctx, diags)
        }
        "holds" => super::condition::process_holds_predicate(model, pred, &resolved, ctx, diags),
        "forbidden" => process_forbidden_predicate(model, pred, &resolved, ctx, diags),
        "invariant" => process_invariant_predicate(model, pred, &resolved, ctx, diags),
        "required" => process_required_predicate(model, pred, &resolved, ctx, diags),
        "exclusive" => process_exclusive_predicate(model, pred, &resolved, ctx, diags),
        "maps_field" => process_maps_field_predicate(model, pred, &resolved, ctx, diags),
        "relate" => process_relate_predicate(model, pred, &resolved, ctx, diags),
        "navigates" => process_navigates_predicate(model, pred, &resolved, ctx, diags),
        _ => process_relation_predicate(model, pred, &resolved, ctx, diags),
    }

    if let Some(typed) = crate::typed_predicate::build_typed_predicate(&pred.name, &resolved, pred)
    {
        model.typed_predicates.push(typed);
    }
}
