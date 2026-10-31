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
        if let Some(bound_kind) = &binding.kind {
            if bound_kind != kind.name() {
                return None;
            }
        }
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

/// Resolve a bare name to a `variation` visible from `ctx`, without reporting.
pub(crate) fn resolve_variation(
    model: &SemanticModel,
    name: &str,
    ctx: DiagCtxt,
) -> Option<VariationKey> {
    let mut found = None;
    for binding in model
        .import_scopes
        .resolve_flat_bindings(ctx.source_id, name)
    {
        if let Some(NodeRef::Variation(key)) =
            resolve_binding(model, &binding, Some(&Kind::Variation))
        {
            if found.replace(key).is_some() {
                return None;
            }
        }
    }
    found
}

/// Resolve `alias.Name` to a `variation` through the import scope, without reporting.
pub(crate) fn resolve_namespaced_variation(
    model: &SemanticModel,
    alias: &str,
    name: &str,
    ctx: DiagCtxt,
) -> Option<VariationKey> {
    let bindings = model
        .import_scopes
        .resolve_namespaced(ctx.source_id, alias, name)
        .ok()?;
    let mut found = None;
    for binding in bindings {
        if let Some(NodeRef::Variation(key)) =
            resolve_binding(model, &binding, Some(&Kind::Variation))
        {
            if found.replace(key).is_some() {
                return None;
            }
        }
    }
    found
}

/// Resolve a bare name to an `entity` visible from `ctx`, without reporting.
pub(crate) fn resolve_entity_name(
    model: &SemanticModel,
    name: &str,
    ctx: DiagCtxt,
) -> Option<EntityKey> {
    let mut found = None;
    for binding in model
        .import_scopes
        .resolve_flat_bindings(ctx.source_id, name)
    {
        if let Some(NodeRef::Entity(key)) = resolve_binding(model, &binding, Some(&Kind::Entity)) {
            if found.replace(key).is_some() {
                return None;
            }
        }
    }
    found
}

fn push_undefined(ctx: DiagCtxt, diags: &mut Vec<Diagnostic>, span: Span, id: String) {
    push_error(ctx, diags, span, RdraError::UndefinedSymbol { id });
}

fn resolve_from_bindings(
    model: &SemanticModel,
    bindings: &[FlatBinding],
    qref: &QRef,
    display: &str,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<NodeRef> {
    let kind = qref.kind_qualifier.as_ref();

    if let Some(kind) = kind {
        let mut found = None;
        for binding in bindings {
            if let Some(n) = resolve_binding(model, binding, Some(kind)) {
                if found.is_some() {
                    // Same kind+id in multiple modules visible under this name.
                    push_error(
                        ctx,
                        diags,
                        qref.span.clone(),
                        RdraError::AmbiguousReference {
                            id: binding.canonical_id.clone(),
                            kinds: kind.name().to_string(),
                        },
                    );
                    return None;
                }
                found = Some(n);
            }
        }
        if found.is_some() {
            return found;
        }

        let id = format!("{}::{}", kind.name(), display);
        // Distinguish ambiguous vs missing when possible.
        match model.symbols.lookup(
            bindings
                .first()
                .map(|b| b.canonical_id.as_str())
                .unwrap_or(display),
        ) {
            LookupResult::Ambiguous(kinds) => {
                push_error(
                    ctx,
                    diags,
                    qref.span.clone(),
                    RdraError::AmbiguousReference {
                        id: bindings
                            .first()
                            .map(|b| b.canonical_id.clone())
                            .unwrap_or_else(|| display.to_string()),
                        kinds: kinds.join(", "),
                    },
                );
            }
            _ => {
                push_undefined(ctx, diags, qref.span.clone(), id);
            }
        }
        return None;
    }

    match bindings {
        [] => {
            push_undefined(ctx, diags, qref.span.clone(), display.to_string());
            None
        }
        [only] => match resolve_binding(model, only, None) {
            Some(n) => Some(n),
            None => {
                match model.symbols.lookup(&only.canonical_id) {
                    LookupResult::Ambiguous(kinds) => {
                        push_error(
                            ctx,
                            diags,
                            qref.span.clone(),
                            RdraError::AmbiguousReference {
                                id: only.canonical_id.clone(),
                                kinds: kinds.join(", "),
                            },
                        );
                    }
                    _ => {
                        push_undefined(ctx, diags, qref.span.clone(), display.to_string());
                    }
                }
                None
            }
        },
        many => {
            // Multiple visible kinds/modules: require kind:: or alias disambiguation.
            let mut kinds: Vec<String> = many
                .iter()
                .filter_map(|b| b.kind.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            if kinds.is_empty() {
                match model.symbols.lookup(&many[0].canonical_id) {
                    LookupResult::Ambiguous(k) => {
                        kinds = k.into_iter().map(str::to_string).collect();
                    }
                    LookupResult::Found(_) => {
                        // Unique symbol despite multiple bindings — try first that resolves.
                        for binding in many {
                            if let Some(n) = resolve_binding(model, binding, None) {
                                return Some(n);
                            }
                        }
                    }
                    LookupResult::NotFound => {}
                }
            }
            push_error(
                ctx,
                diags,
                qref.span.clone(),
                RdraError::AmbiguousReference {
                    id: many[0].canonical_id.clone(),
                    kinds: kinds.join(", "),
                },
            );
            None
        }
    }
}

fn resolve_qref(
    model: &SemanticModel,
    qref: &QRef,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<NodeRef> {
    let bindings = match qref.parts.as_slice() {
        [] => {
            push_undefined(ctx, diags, qref.span.clone(), "<empty>".into());
            return None;
        }
        [name] => {
            let bindings = model
                .import_scopes
                .resolve_flat_bindings(ctx.source_id, name);
            if bindings.is_empty() {
                push_undefined(ctx, diags, qref.span.clone(), name.clone());
                return None;
            }
            bindings
        }
        [alias, name] => match model
            .import_scopes
            .resolve_namespaced(ctx.source_id, alias, name)
        {
            _ if crate::binding::bound_ext(model, alias).is_some() => {
                return crate::binding::resolve_foreign(model, qref, ctx, diags).flatten();
            }
            Ok(b) => {
                if b.is_empty() {
                    push_undefined(ctx, diags, qref.span.clone(), format!("{alias}.{name}"));
                    return None;
                }
                b
            }
            Err(NamespacedResolveError::UnknownAlias { alias }) => {
                push_undefined(ctx, diags, qref.span.clone(), format!("{alias}.{name}"));
                return None;
            }
            Err(NamespacedResolveError::UnknownModule { module })
            | Err(NamespacedResolveError::NotExported { module, .. }) => {
                push_undefined(ctx, diags, qref.span.clone(), format!("{module}.{name}"));
                return None;
            }
        },
        parts => {
            let name = parts.last().unwrap();
            let scope = model.import_scopes.scope_for(ctx.source_id);
            if scope.unrestricted {
                vec![FlatBinding {
                    canonical_id: name.clone(),
                    module: None,
                    kind: None,
                }]
            } else {
                push_undefined(ctx, diags, qref.span.clone(), parts.join("."));
                return None;
            }
        }
    };

    let display = if qref.parts.len() > 1 {
        qref.parts.join(".")
    } else {
        bindings
            .first()
            .map(|b| b.canonical_id.clone())
            .unwrap_or_else(|| qref.parts.join("."))
    };

    resolve_from_bindings(model, &bindings, qref, &display, ctx, diags)
}
