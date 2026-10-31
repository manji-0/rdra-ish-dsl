use crate::diagnostics::*;
use crate::location::{DiagCtxt, SourceId};
use crate::model::*;
use rdra_ish_syntax::ast::*;

mod arg_resolve;
mod column_expr;
mod comparison;
mod condition;
mod constraint;
mod effect;
mod fk;
mod instance;
mod metadata;
mod nodes;
mod predicate_process;
mod property;
mod qref_util;

pub(crate) use arg_resolve::resolve_arg;
pub use condition::{applied_bodies, condition_body_display, condition_edges, ConditionEdge};
use fk::{resolve_along_links, resolve_foreign_keys};
use instance::register_instance;
use predicate_process::process_predicate;
use property::register_property;

pub fn build_model(ast: &Ast) -> (SemanticModel, Vec<Diagnostic>) {
    let items: Vec<(SourceId, Item)> = ast.items.iter().cloned().map(|item| (0, item)).collect();
    build_model_items_with_scopes(
        &items,
        crate::import_scope::ImportScopes::unrestricted(0),
        Vec::new(),
        false,
    )
}

pub fn build_model_items(items: &[(SourceId, Item)]) -> (SemanticModel, Vec<Diagnostic>) {
    build_model_items_with_scopes(
        items,
        crate::import_scope::ImportScopes::unrestricted(0),
        Vec::new(),
        false,
    )
}

pub fn build_model_items_with_scopes(
    items: &[(SourceId, Item)],
    import_scopes: crate::import_scope::ImportScopes,
    dependencies: Vec<std::sync::Arc<crate::binding::DependencySurface>>,
    has_manifest: bool,
) -> (SemanticModel, Vec<Diagnostic>) {
    let mut model = SemanticModel {
        import_scopes,
        dependencies,
        has_manifest,
        ..SemanticModel::default()
    };
    let mut diags: Vec<Diagnostic> = vec![];

    // Variations first: entity columns resolve their type name against them.
    for variations_pass in [true, false] {
        for (source_id, item) in items {
            if let Item::Instance(inst) = item {
                if (inst.kind == Kind::Variation) == variations_pass {
                    register_instance(&mut model, inst, DiagCtxt::new(*source_id), &mut diags);
                }
            }
        }
    }

    crate::binding::shadow_diagnostics(&model, &mut diags);

    for (source_id, item) in items {
        match item {
            Item::Predicate(pred) => {
                process_predicate(&mut model, pred, DiagCtxt::new(*source_id), &mut diags);
            }
            Item::Property(prop) => {
                register_property(&mut model, prop, DiagCtxt::new(*source_id), &mut diags);
            }
            _ => {}
        }
    }

    column_expr::reject_derived_writes(&model, &mut diags);
    condition::derive_condition_links(&mut model, &mut diags);
    resolve_foreign_keys(&mut model, &mut diags);
    resolve_along_links(&mut model, &mut diags);
    diags.extend(crate::event_flow::api_route_diagnostics(&model));
    diags.extend(crate::publication::publication_diagnostics(&model));

    (model, diags)
}

#[cfg(test)]
mod integration_tests;
