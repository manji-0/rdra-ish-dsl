//! Symbol resolution for predicate reference arguments.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::import_scope::{FlatBinding, NamespacedResolveError};
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
        PredicateArg::Ref(qref) => resolve_qref(model, qref, ctx, diags),
    }
}

fn resolve_binding(
    model: &SemanticModel,
    binding: &FlatBinding,
    kind: Option<&Kind>,
) -> Option<NodeRef> {
    // When a module is bound (namespaced / selective / All import), do not fall
    // back to a different module's declaration.
    if let Some(kind) = kind {
        if let Some(module) = &binding.module {
            return model
                .symbols
                .lookup_qualified_in_module(kind, &binding.canonical_id, module)
                .cloned();
        }
        return model
            .symbols
            .lookup_qualified(kind, &binding.canonical_id)
            .cloned();
    }

    if let Some(module) = &binding.module {
        return model
            .symbols
            .lookup_in_module(&binding.canonical_id, module)
            .cloned();
    }

    match model.symbols.lookup(&binding.canonical_id) {
        LookupResult::Found(n) => Some(n.clone()),
        _ => None,
    }
}

fn resolve_qref(
    model: &SemanticModel,
    qref: &QRef,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<NodeRef> {
    let binding = match qref.parts.as_slice() {
        [] => {
            push_error(
                ctx,
                diags,
                qref.span.clone(),
                RdraError::UndefinedSymbol {
                    id: "<empty>".into(),
                },
            );
            return None;
        }
        [name] => match model.import_scopes.resolve_flat(ctx.source_id, name) {
            Some(b) => b,
            None => {
                push_error(
                    ctx,
                    diags,
                    qref.span.clone(),
                    RdraError::UndefinedSymbol { id: name.clone() },
                );
                return None;
            }
        },
        [alias, name] => match model
            .import_scopes
            .resolve_namespaced(ctx.source_id, alias, name)
        {
            Ok(b) => b,
            Err(NamespacedResolveError::UnknownAlias { alias }) => {
                push_error(
                    ctx,
                    diags,
                    qref.span.clone(),
                    RdraError::UndefinedSymbol {
                        id: format!("{alias}.{name}"),
                    },
                );
                return None;
            }
            Err(NamespacedResolveError::UnknownModule { module })
            | Err(NamespacedResolveError::NotExported { module, .. }) => {
                push_error(
                    ctx,
                    diags,
                    qref.span.clone(),
                    RdraError::UndefinedSymbol {
                        id: format!("{module}.{name}"),
                    },
                );
                return None;
            }
        },
        parts => {
            let name = parts.last().unwrap();
            let scope = model.import_scopes.scope_for(ctx.source_id);
            if scope.unrestricted {
                FlatBinding {
                    canonical_id: name.clone(),
                    module: None,
                }
            } else {
                push_error(
                    ctx,
                    diags,
                    qref.span.clone(),
                    RdraError::UndefinedSymbol {
                        id: parts.join("."),
                    },
                );
                return None;
            }
        }
    };

    let display = if qref.parts.len() > 1 {
        qref.parts.join(".")
    } else {
        binding.canonical_id.clone()
    };

    match resolve_binding(model, &binding, qref.kind_qualifier.as_ref()) {
        Some(n) => Some(n),
        None => {
            let id = if let Some(kind) = &qref.kind_qualifier {
                format!("{}::{}", kind.name(), display)
            } else {
                display
            };
            // Distinguish ambiguous vs missing when possible.
            match model.symbols.lookup(&binding.canonical_id) {
                LookupResult::Ambiguous(kinds) => {
                    push_error(
                        ctx,
                        diags,
                        qref.span.clone(),
                        RdraError::AmbiguousReference {
                            id: binding.canonical_id,
                            kinds: kinds.join(", "),
                        },
                    );
                }
                _ => {
                    push_error(
                        ctx,
                        diags,
                        qref.span.clone(),
                        RdraError::UndefinedSymbol { id },
                    );
                }
            }
            None
        }
    }
}
