use crate::diagnostics::*;
use crate::location::{DiagCtxt, SourceId};
use crate::model::*;
use rdra_ish_syntax::ast::*;

mod arg_resolve;
mod comparison;
mod constraint;
mod effect;
mod fk;
mod instance;
mod metadata;
mod nodes;
mod predicate_process;
mod property;
mod qref_util;

use fk::generate_fks;
use instance::register_instance;
use predicate_process::process_predicate;
use property::register_property;

pub fn build_model(ast: &Ast) -> (SemanticModel, Vec<Diagnostic>) {
    let items: Vec<(SourceId, Item)> = ast.items.iter().cloned().map(|item| (0, item)).collect();
    build_model_items_with_scopes(&items, crate::import_scope::ImportScopes::unrestricted(0))
}

pub fn build_model_items(items: &[(SourceId, Item)]) -> (SemanticModel, Vec<Diagnostic>) {
    build_model_items_with_scopes(items, crate::import_scope::ImportScopes::unrestricted(0))
}

pub fn build_model_items_with_scopes(
    items: &[(SourceId, Item)],
    import_scopes: crate::import_scope::ImportScopes,
) -> (SemanticModel, Vec<Diagnostic>) {
    let mut model = SemanticModel {
        import_scopes,
        ..SemanticModel::default()
    };
    let mut diags: Vec<Diagnostic> = vec![];

    for (source_id, item) in items {
        if let Item::Instance(inst) = item {
            register_instance(&mut model, inst, DiagCtxt::new(*source_id), &mut diags);
        }
    }

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

    generate_fks(&mut model, &mut diags);
    diags.extend(crate::event_flow::api_route_diagnostics(&model));

    (model, diags)
}

#[cfg(test)]
mod integration_tests;
