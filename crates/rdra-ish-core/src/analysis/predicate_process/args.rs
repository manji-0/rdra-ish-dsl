//! Predicate argument resolution, argument-type and arity checks.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use crate::predicate::TRIGGER_PAIRS;
use rdra_ish_syntax::ast::*;

use super::predicate_arg_display;
use crate::analysis::arg_resolve::resolve_arg;

pub(super) fn resolve_predicate_args(
    model: &SemanticModel,
    pred: &PredicateCall,
    sig: &[Vec<&'static str>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Option<NodeRef>> {
    pred.args
        .iter()
        .enumerate()
        .map(|(i, arg)| {
            let Some(kinds) = sig.get(i) else {
                return if matches!(
                    pred.name.as_str(),
                    "forbidden"
                        | "required"
                        | "exclusive"
                        | "sets"
                        | "drives"
                        | "invariant"
                        | "after"
                        | "holds"
                ) {
                    None
                } else {
                    resolve_arg(model, arg, ctx, diags)
                };
            };
            if matches!(
                kinds.as_slice(),
                ["_card"] | ["_col"] | ["_val"] | ["variation", "_col"]
            ) {
                return None;
            }
            resolve_arg(model, arg, ctx, diags)
        })
        .collect()
}

pub(super) fn validate_predicate_arg_types(
    pred: &PredicateCall,
    sig: &[Vec<&'static str>],
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    for (i, expected_kinds) in sig.iter().enumerate() {
        if matches!(
            expected_kinds.as_slice(),
            ["_card"] | ["_col"] | ["_val"] | ["variation", "_col"]
        ) {
            continue;
        }
        if let Some(Some(node)) = resolved.get(i) {
            let actual = node.kind_tag();
            if !expected_kinds.contains(&actual) {
                push_error_arg(
                    ctx,
                    diags,
                    &pred.args,
                    i,
                    RdraError::TypeMismatch {
                        pred: pred.name.clone(),
                        id: predicate_arg_display(&pred.args[i]),
                        actual: actual.to_string(),
                        expected: expected_kinds.join("|"),
                    },
                );
            }
        }
    }
}

pub(super) fn validate_contains_pair(
    pred: &PredicateCall,
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    if pred.name != "contains" {
        return true;
    }
    if let (Some(Some(from)), Some(Some(to))) = (resolved.first(), resolved.get(1)) {
        let valid = matches!(
            (from, to),
            (NodeRef::Buc(_), NodeRef::UseCase(_))
                | (NodeRef::Buc(_), NodeRef::Flow(_))
                | (NodeRef::Flow(_), NodeRef::Step(_))
                | (NodeRef::Screen(_), NodeRef::Field(_))
                | (NodeRef::System(_), NodeRef::Api(_))
        );
        if !valid {
            push_error(
                ctx,
                diags,
                pred.span.clone(),
                RdraError::TypeMismatch {
                    pred: pred.name.clone(),
                    id: "contains pair".to_string(),
                    actual: format!("{} -> {}", from.kind_tag(), to.kind_tag()),
                    expected: "buc->usecase|buc->flow|flow->step|screen->field|system->api"
                        .to_string(),
                },
            );
            return false;
        }
    }
    true
}

/// `triggers` accepts only the kind pairs in `TRIGGER_PAIRS`; the per-argument signature
/// cannot express that a timer has no external-system target.
pub(super) fn validate_trigger_pair(
    pred: &PredicateCall,
    sig: &[Vec<&'static str>],
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    if pred.name != "triggers" {
        return true;
    }
    let (Some(Some(from)), Some(Some(to))) = (resolved.first(), resolved.get(1)) else {
        return true;
    };
    let pair = (from.kind_tag(), to.kind_tag());
    // A kind outside the signature was already reported by `validate_predicate_arg_types`.
    let in_signature = sig[0].contains(&pair.0) && sig[1].contains(&pair.1);
    if TRIGGER_PAIRS.contains(&pair) || !in_signature {
        return true;
    }
    push_error(
        ctx,
        diags,
        pred.span.clone(),
        RdraError::TypeMismatch {
            pred: pred.name.clone(),
            id: "triggers pair".to_string(),
            actual: format!("{} -> {}", pair.0, pair.1),
            expected: TRIGGER_PAIRS
                .iter()
                .map(|(source, target)| format!("{source}->{target}"))
                .collect::<Vec<_>>()
                .join("|"),
        },
    );
    false
}

/// Fixed-arity predicates must match the signature length exactly.
/// Variadic / trailing-condition predicates only enforce a minimum.
pub(super) fn arity_error(pred: &PredicateCall, sig: &[Vec<&'static str>]) -> Option<RdraError> {
    let got = pred.args.len();
    let (expected, ok) = match pred.name.as_str() {
        "sets" => ("2".into(), got == 2),
        "drives" => ("3".into(), got == 3),
        "after" => ("at least 1".into(), got >= 1),
        "forbidden" | "invariant" | "required" | "exclusive" => ("at least 1".into(), got >= 1),
        "holds" => ("at least 2".into(), got >= 2),
        "transitions" => ("3".into(), got == 3),
        "chooses" => ("2 or 3".into(), got == 2 || got == 3),
        "maps_field" => ("2".into(), got == 2),
        "relate" => ("3".into(), got == 3),
        _ => {
            let n = sig.len();
            (n.to_string(), got == n)
        }
    };
    if ok {
        None
    } else {
        Some(RdraError::WrongArity {
            name: pred.name.clone(),
            expected,
            got,
        })
    }
}
