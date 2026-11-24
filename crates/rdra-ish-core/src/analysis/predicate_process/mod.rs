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
pub(crate) use self::structure::process_dto_maps_predicate;
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
use crate::predicate::{unknown_chain_method_hint, Predicate};
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
        PredicateArg::Card(c) | PredicateArg::Int(c) => c.clone(),
    }
}

pub(crate) fn process_predicate(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(p) = Predicate::parse(&pred.name) else {
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

    let allowed = p.chain_methods();
    for cc in &pred.chain {
        if !allowed.contains(&cc.name.as_str()) {
            push_error(
                ctx,
                diags,
                pred.span.clone(),
                RdraError::UnknownChainMethod {
                    pred: pred.name.clone(),
                    name: cc.name.clone(),
                    hint: unknown_chain_method_hint(p, &cc.name),
                },
            );
        }
    }

    if let Some(err) = arity_error(p, pred) {
        push_error(ctx, diags, pred.span.clone(), err);
        return;
    }

    // `maps_field(Dto.field, Entity.column)` waits for the generated key columns:
    // `analysis::query` reads it after the foreign keys exist.
    if is_dto_mapping(pred) {
        return;
    }

    // Unified forbidden/invariant: detect cross-entity vs single-entity form.
    // Cross-entity: `.along(...)`, two or more leading entity refs
    // (e.g. `forbidden(Order, Payment, Order.status == x, ...)`), or an invariant whose
    // `.has/.none(...)` ranges over related rows. Single-entity: `forbidden(Order, status == x)`.
    if matches!(p, Predicate::Forbidden | Predicate::Invariant) {
        let leading_entities = pred
            .args
            .iter()
            .take_while(|a| matches!(a, PredicateArg::Ref(_)))
            .count();
        let is_cross = pred.chain.iter().any(|cc| cc.name == "along")
            || leading_entities >= 2
            || (p == Predicate::Invariant
                && pred
                    .chain
                    .iter()
                    .any(|cc| matches!(cc.name.as_str(), "has" | "none")));
        if is_cross {
            if p == Predicate::Forbidden {
                process_cross_forbidden(model, pred, ctx, diags);
            } else {
                process_cross_invariant(model, pred, ctx, diags);
            }
            return;
        }
        if let Some(role) = pred.chain.iter().find(|cc| cc.name == "role") {
            push_error(ctx, diags, role.span.clone(), RdraError::RoleWithoutJoin);
        }
    }

    let sig = p.signature();
    let resolved = resolve_predicate_args(model, pred, &sig, ctx, diags);
    if !validate_predicate_arg_types(pred, &sig, &resolved, ctx, diags) {
        return;
    }
    let paired = match p {
        Predicate::Contains => validate_contains_pair(pred, &resolved, ctx, diags),
        Predicate::Triggers => validate_trigger_pair(pred, &sig, &resolved, ctx, diags),
        _ => true,
    };
    if !paired {
        return;
    }

    use Predicate::*;
    match p {
        Coordinates => process_coordinates_predicate(model, &resolved),
        Transitions => process_transitions_predicate(model, pred, &resolved, ctx, diags),
        Outbox => process_outbox_predicate(model, &resolved),
        Exposes => process_exposes_predicate(model, pred, &resolved, ctx, diags),
        Shares => process_shares_predicate(model, pred, &resolved, ctx),
        After => process_after_predicate(model, pred, &resolved, ctx, diags),
        Sets => process_sets_predicate(model, pred, &resolved, ctx, diags),
        Drives => process_drives_predicate(model, pred, &resolved, ctx, diags),
        Links => process_links_predicate(model, LinkKind::Links, pred, &resolved, ctx),
        Relinks => process_links_predicate(model, LinkKind::Relinks, pred, &resolved, ctx),
        Unlinks => process_links_predicate(model, LinkKind::Unlinks, pred, &resolved, ctx),
        Chooses => process_chooses_predicate(model, pred, &resolved, ctx, diags),
        Combines => {
            super::condition::process_combines_predicate(model, pred, &resolved, ctx, diags)
        }
        Holds => super::condition::process_holds_predicate(model, pred, &resolved, ctx, diags),
        Forbidden => process_forbidden_predicate(model, pred, &resolved, ctx, diags),
        Invariant => process_invariant_predicate(model, pred, &resolved, ctx, diags),
        Required => process_required_predicate(model, pred, &resolved, ctx, diags),
        Exclusive => process_exclusive_predicate(model, pred, &resolved, ctx, diags),
        MapsField => process_maps_field_predicate(model, pred, &resolved, ctx, diags),
        Relate => process_relate_predicate(model, pred, &resolved, ctx, diags),
        Navigates => process_navigates_predicate(model, pred, &resolved, ctx, diags),
        _ => {
            if let Some(kind) = p.rel_kind() {
                process_relation_predicate(model, kind, pred, &resolved, ctx, diags);
            }
        }
    }

    // `.apart()` on a use case's write: its row need not be joined to the other rows it writes.
    if matches!(p, Creates | Updates | Deletes) && pred.chain.iter().any(|cc| cc.name == "apart") {
        if let [Some(NodeRef::UseCase(uc)), Some(NodeRef::Entity(ek))] = resolved.as_slice() {
            if !model.apart_writes.contains(&(*uc, *ek)) {
                model.apart_writes.push((*uc, *ek));
            }
        }
    }
    if p == Triggers {
        process_counted(model, pred, &resolved, ctx, diags);
    }
}

/// The largest `k` of `.counted(k)`: each row holds one of `k + 2` values per counted edge.
pub(crate) const MAX_COUNTED: u8 = 16;

/// `triggers(Event, UseCase).counted(k)`: one integer from 1 to `MAX_COUNTED`, once, on an
/// edge from an event to a use case.
fn process_counted(
    model: &mut SemanticModel,
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let calls: Vec<_> = pred.chain.iter().filter(|c| c.name == "counted").collect();
    let Some(first) = calls.first() else {
        return;
    };
    let fail = |span: &Span, detail: String, diags: &mut Vec<Diagnostic>| {
        push_error(
            ctx,
            diags,
            span.clone(),
            RdraError::CountedTriggerInvalid {
                detail,
                max: MAX_COUNTED,
            },
        );
    };
    if let Some(second) = calls.get(1) {
        fail(
            &second.span,
            "a `triggers` line takes one `.counted(k)`".to_string(),
            diags,
        );
        return;
    }
    let (event, uc) = match resolved {
        [Some(NodeRef::Event(ev)), Some(NodeRef::UseCase(uc))] => (*ev, *uc),
        [Some(NodeRef::Timer(_)), _] => {
            return fail(
                &first.span,
                "`.counted` counts the raises of an event; a timer is never raised".to_string(),
                diags,
            )
        }
        [_, Some(_)] => {
            return fail(
                &first.span,
                "`.counted` needs a use case to run once per raise; a BUC or an external system runs nothing"
                    .to_string(),
                diags,
            )
        }
        _ => return,
    };
    let k = match first.args.as_slice() {
        [PredicateArg::Int(n)] => n
            .parse::<u8>()
            .ok()
            .filter(|k| (1..=MAX_COUNTED).contains(k)),
        _ => None,
    };
    let Some(k) = k else {
        let written = first
            .args
            .iter()
            .map(predicate_arg_display)
            .collect::<Vec<_>>()
            .join(", ");
        return fail(
            &first.span,
            format!("`.counted({written})` is not a count of pending raises"),
            diags,
        );
    };
    if !model
        .counted_triggers
        .iter()
        .any(|(e, u, _)| (*e, *u) == (event, uc))
    {
        model.counted_triggers.push((event, uc, k));
    }
}

/// `maps_field(Dto.field, Entity.column)`: a screen field is one name, a DTO field two.
pub(crate) fn is_dto_mapping(pred: &PredicateCall) -> bool {
    pred.name == "maps_field"
        && matches!(pred.args.first(), Some(PredicateArg::Ref(q))
            if q.kind_qualifier.is_none() && q.parts.len() == 2)
}
