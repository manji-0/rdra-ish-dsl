//! Symbol resolution for predicate reference arguments.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

pub(crate) fn resolve_arg(
    model: &SemanticModel,
    arg: &PredicateArg,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<NodeRef> {
    match arg {
        PredicateArg::Lit(_) => None,
        PredicateArg::Transition { .. } | PredicateArg::Card(_) => None,
        PredicateArg::Expr(_) => None,
        PredicateArg::Ref(qref) => {
            let id = qref.parts.last().unwrap();

            if let Some(kind) = &qref.kind_qualifier {
                model
                    .symbols
                    .lookup_qualified(kind, id)
                    .cloned()
                    .or_else(|| {
                        push_error(
                            ctx,
                            diags,
                            qref.span.clone(),
                            RdraError::UndefinedSymbol {
                                id: format!("{}::{}", kind.name(), id),
                            },
                        );
                        None
                    })
            } else {
                match model.symbols.lookup(id) {
                    LookupResult::Found(n) => Some(n.clone()),
                    LookupResult::NotFound => {
                        push_error(
                            ctx,
                            diags,
                            qref.span.clone(),
                            RdraError::UndefinedSymbol { id: id.clone() },
                        );
                        None
                    }
                    LookupResult::Ambiguous(kinds) => {
                        push_error(
                            ctx,
                            diags,
                            qref.span.clone(),
                            RdraError::AmbiguousReference {
                                id: id.clone(),
                                kinds: kinds.join(", "),
                            },
                        );
                        None
                    }
                }
            }
        }
    }
}
