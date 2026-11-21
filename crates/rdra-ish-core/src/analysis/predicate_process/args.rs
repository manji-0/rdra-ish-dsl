//! Predicate argument resolution, argument-type and arity checks.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use crate::predicate::{ArgKind, Predicate, TRIGGER_PAIRS};
use rdra_ish_syntax::ast::*;

use super::predicate_arg_display;
use crate::analysis::arg_resolve::resolve_arg;

pub(super) fn resolve_predicate_args(
    model: &SemanticModel,
    pred: &PredicateCall,
    sig: &[Vec<ArgKind>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Option<NodeRef>> {
    pred.args
        .iter()
        .enumerate()
        .map(|(i, arg)| match node_kinds(sig.get(i)?) {
            Some(_) => resolve_arg(model, arg, ctx, diags),
            None => None,
        })
        .collect()
}

/// The declaration kinds an argument takes, when it takes nothing but declarations; its
/// processor reads any other argument itself.
fn node_kinds(kinds: &[ArgKind]) -> Option<Vec<&'static str>> {
    kinds.iter().map(|k| k.node()).collect()
}

pub(super) fn validate_predicate_arg_types(
    pred: &PredicateCall,
    sig: &[Vec<ArgKind>],
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    for (i, kinds) in sig.iter().enumerate() {
        let Some(expected_kinds) = node_kinds(kinds) else {
            continue;
        };
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
    sig: &[Vec<ArgKind>],
    resolved: &[Option<NodeRef>],
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let (Some(Some(from)), Some(Some(to))) = (resolved.first(), resolved.get(1)) else {
        return true;
    };
    let pair = (from.kind_tag(), to.kind_tag());
    // A kind outside the signature was already reported by `validate_predicate_arg_types`.
    let in_signature =
        sig[0].contains(&ArgKind::Node(pair.0)) && sig[1].contains(&ArgKind::Node(pair.1));
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

pub(super) fn arity_error(p: Predicate, pred: &PredicateCall) -> Option<RdraError> {
    let got = pred.args.len();
    let arity = p.arity();
    (!arity.accepts(got)).then(|| RdraError::WrongArity {
        name: pred.name.clone(),
        expected: arity.expected(),
        got,
    })
}
