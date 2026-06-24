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
mod qref_util;

use fk::generate_fks;
use instance::register_instance;
use predicate_process::process_predicate;

pub fn build_model(ast: &Ast) -> (SemanticModel, Vec<Diagnostic>) {
    let items: Vec<(SourceId, Item)> = ast.items.iter().cloned().map(|item| (0, item)).collect();
    build_model_items(&items)
}

pub fn build_model_items(items: &[(SourceId, Item)]) -> (SemanticModel, Vec<Diagnostic>) {
    let mut model = SemanticModel::default();
    let mut diags: Vec<Diagnostic> = vec![];

    for (source_id, item) in items {
        if let Item::Instance(inst) = item {
            register_instance(&mut model, inst, DiagCtxt::new(*source_id), &mut diags);
        }
    }

    for (source_id, item) in items {
        if let Item::Predicate(pred) = item {
            process_predicate(&mut model, pred, DiagCtxt::new(*source_id), &mut diags);
        }
    }

    generate_fks(&mut model, &mut diags);

    (model, diags)
}

#[cfg(test)]
mod integration_tests;
