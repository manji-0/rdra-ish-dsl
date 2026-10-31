//! Workspace rename for declared symbols.

use std::collections::HashMap;
use std::path::Path;

use crate::convert::span_to_range;
use crate::refs::{
    id_span_in_qref, instance_id_span, reference_at_offset, symbol_target_at, SymbolTarget,
};
use crate::uri::{path_to_uri, paths_equal};
use rdra_ish_core::WorkspaceAnalysis;
use rdra_ish_syntax::ast::{Ast, ChainCall, Expr, Item, Operand, PredicateArg, PredicateCall};
use tower_lsp::lsp_types::{Range, TextEdit, Url, WorkspaceEdit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameError {
    InvalidIdentifier,
    NoSymbol,
    Conflict,
}

pub fn is_valid_rename_ident(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub fn prepare_rename_range(
    analysis: &WorkspaceAnalysis,
    path: &Path,
    offset: usize,
) -> Option<Range> {
    let (_, text, ast) = analysis
        .program
        .sources
        .iter()
        .find(|(source_path, _, _)| paths_equal(source_path, path))?;
    let reference = reference_at_offset(ast, offset)?;
    let target = symbol_target_at(&analysis.model, reference, text, offset)?;
    let span = rename_span_at_offset(ast, text, &target, offset)?;
    Some(span_to_range(text, span))
}

pub fn workspace_rename(
    analysis: &WorkspaceAnalysis,
    path: &Path,
    offset: usize,
    new_name: &str,
) -> Result<WorkspaceEdit, RenameError> {
    if !is_valid_rename_ident(new_name) {
        return Err(RenameError::InvalidIdentifier);
    }

    let (_, text, ast) = analysis
        .program
        .sources
        .iter()
        .find(|(source_path, _, _)| paths_equal(source_path, path))
        .ok_or(RenameError::NoSymbol)?;
    let reference = reference_at_offset(ast, offset).ok_or(RenameError::NoSymbol)?;
    let target =
        symbol_target_at(&analysis.model, reference, text, offset).ok_or(RenameError::NoSymbol)?;

    if analysis
        .model
        .decl_sites
        .get(&target.kind, new_name)
        .is_some()
        || is_foreign_slug(&analysis.model, new_name)
    {
        return Err(RenameError::Conflict);
    }

    let _ = rename_span_at_offset(ast, text, &target, offset).ok_or(RenameError::NoSymbol)?;

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for (source_path, source_text, source_ast) in &analysis.program.sources {
        let edits = rename_edits_in_ast(source_ast, source_text, &target, new_name);
        if edits.is_empty() {
            continue;
        }
        let uri = path_to_uri(source_path).map_err(|_| RenameError::NoSymbol)?;
        changes.insert(uri, edits);
    }

    Ok(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

/// `Auth__Login`: the generated name of a stub, which no local id may take.
fn is_foreign_slug(model: &rdra_ish_core::SemanticModel, name: &str) -> bool {
    use rdra_ish_core::model::slug;
    model
        .use_cases
        .foreign()
        .map(|(_, u, _)| slug(&u.id))
        .chain(model.events.foreign().map(|(_, e, _)| slug(&e.id)))
        .chain(model.entities.foreign().map(|(_, e, _)| slug(&e.id)))
        .chain(model.permissions.foreign().map(|(_, p, _)| slug(&p.id)))
        .any(|s| s == name)
}

fn rename_edits_in_ast(
    ast: &Ast,
    text: &str,
    target: &SymbolTarget,
    new_name: &str,
) -> Vec<TextEdit> {
    let mut spans = Vec::new();
    for item in &ast.items {
        match item {
            Item::Instance(inst) => {
                if inst.kind.name() == target.kind && inst.id == target.id {
                    spans.push(instance_id_span(inst, text));
                }
            }
            Item::Predicate(pred) => collect_rename_spans(pred, text, target, &mut spans),
            Item::Import(_) | Item::Module(_, _) | Item::Property(_) => {}
        }
    }

    spans.sort_by_key(|span| span.start);
    spans.dedup_by(|a, b| a.start == b.start && a.end == b.end);

    spans
        .into_iter()
        .map(|span| TextEdit {
            range: span_to_range(text, span),
            new_text: new_name.to_string(),
        })
        .collect()
}

fn collect_rename_spans(
    pred: &PredicateCall,
    text: &str,
    target: &SymbolTarget,
    spans: &mut Vec<std::ops::Range<usize>>,
) {
    for arg in &pred.args {
        collect_rename_spans_in_arg(arg, text, target, spans);
    }
    for chain in &pred.chain {
        collect_rename_spans_in_chain(chain, text, target, spans);
    }
}

fn collect_rename_spans_in_chain(
    chain: &ChainCall,
    text: &str,
    target: &SymbolTarget,
    spans: &mut Vec<std::ops::Range<usize>>,
) {
    for arg in &chain.args {
        collect_rename_spans_in_arg(arg, text, target, spans);
    }
}

fn collect_rename_spans_in_arg(
    arg: &PredicateArg,
    text: &str,
    target: &SymbolTarget,
    spans: &mut Vec<std::ops::Range<usize>>,
) {
    match arg {
        PredicateArg::Ref(qref) => {
            if let Some(span) = id_span_in_qref(qref, text, target) {
                spans.push(span);
            }
        }
        PredicateArg::Expr(expr) => collect_expr_rename_spans(expr, text, target, spans),
        PredicateArg::Transition { .. } | PredicateArg::Card(_) | PredicateArg::Lit(_) => {}
    }
}

fn collect_expr_rename_spans(
    expr: &Expr,
    text: &str,
    target: &SymbolTarget,
    spans: &mut Vec<std::ops::Range<usize>>,
) {
    match expr {
        Expr::Cmp(cmp) => {
            if let Operand::QualifiedColumn(col) = &cmp.lhs {
                if let Some(span) = id_span_in_qref(&col.entity, text, target) {
                    spans.push(span);
                }
            }
            if let Operand::QualifiedColumn(col) = &cmp.rhs {
                if let Some(span) = id_span_in_qref(&col.entity, text, target) {
                    spans.push(span);
                }
            }
        }
        Expr::Not(inner) => collect_expr_rename_spans(inner, text, target, spans),
        Expr::And(a, b) | Expr::Or(a, b) => {
            collect_expr_rename_spans(a, text, target, spans);
            collect_expr_rename_spans(b, text, target, spans);
        }
    }
}

fn rename_span_at_offset(
    ast: &Ast,
    text: &str,
    target: &SymbolTarget,
    offset: usize,
) -> Option<std::ops::Range<usize>> {
    let reference = reference_at_offset(ast, offset)?;
    match reference {
        crate::refs::ReferenceAt::Declaration { kind, id } => {
            for item in &ast.items {
                if let Item::Instance(inst) = item {
                    if inst.kind.name() == kind && inst.id == id {
                        return Some(instance_id_span(inst, text));
                    }
                }
            }
            None
        }
        crate::refs::ReferenceAt::Symbol(qref) => id_span_in_qref(qref, text, target),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rdra_ish_core::analyze_workspace;

    use super::*;

    #[test]
    fn renames_declaration_and_references() {
        let src = r#"usecase Book "Book"
actor Staff "Staff"
performs(Staff, Book)
"#;
        let root = std::env::temp_dir().join("rdra-rename-test");
        let _ = std::fs::create_dir_all(&root);
        let file = root.join("sample.rdra");
        std::fs::write(&file, src).unwrap();

        let analysis = analyze_workspace(
            std::slice::from_ref(&file),
            std::slice::from_ref(&root),
            &Default::default(),
        );
        let offset = src.find("Book").unwrap();
        let edit = workspace_rename(&analysis, &file, offset, "Reserve").unwrap();
        let changes = edit.changes.expect("changes");
        assert_eq!(changes.len(), 1);

        let uri = path_to_uri(&file).unwrap();
        let edits = changes.get(&uri).expect("file edits");
        assert_eq!(edits.len(), 2);

        std::fs::remove_file(&file).ok();
        std::fs::remove_dir(root).ok();
    }

    #[test]
    fn a_reference_to_a_bound_external_system_is_neither_renamed_nor_a_rename_target() {
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/xsys/shop-model");
        let file = root.join("shop/checkout.rdra");
        let src = std::fs::read_to_string(&file).unwrap();
        let project = rdra_ish_core::load_project(std::slice::from_ref(&root), None).unwrap();
        let analysis = rdra_ish_core::analyze_project(project);

        let offset = src.find("usecase::Auth.Login").unwrap() + "usecase::Auth.".len();
        assert!(matches!(
            workspace_rename(&analysis, &file, offset, "SignIn"),
            Err(RenameError::NoSymbol)
        ));

        let offset = src.find("usecase SignInAtCheckout").unwrap() + "usecase ".len();
        let edit = workspace_rename(&analysis, &file, offset, "SignIn").unwrap();
        let changes = edit.changes.expect("changes");
        let edits = changes
            .get(&path_to_uri(&file).unwrap())
            .expect("file edits");
        assert_eq!(edits.len(), 4, "{edits:?}");
    }

    #[test]
    fn renaming_a_bound_external_system_rewrites_every_qualifier() {
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/xsys/shop-model");
        let file = root.join("shop/checkout.rdra");
        let src = std::fs::read_to_string(&file).unwrap();
        let project = rdra_ish_core::load_project(std::slice::from_ref(&root), None).unwrap();
        let analysis = rdra_ish_core::analyze_project(project);

        let offset = src.find("extsystem Auth").unwrap() + "extsystem ".len();
        let edit = workspace_rename(&analysis, &file, offset, "Authn").unwrap();
        let changes = edit.changes.expect("changes");
        let edits = changes
            .get(&path_to_uri(&file).unwrap())
            .expect("file edits");
        assert_eq!(edits.len(), 4, "{edits:?}");

        let mut renamed = src.clone();
        let mut sorted: Vec<&TextEdit> = edits.iter().collect();
        sorted.sort_by_key(|e| std::cmp::Reverse((e.range.start.line, e.range.start.character)));
        for e in sorted {
            let lines: Vec<&str> = renamed.lines().collect();
            let offset_of = |line: usize, ch: usize| {
                lines[..line].iter().map(|l| l.len() + 1).sum::<usize>() + ch
            };
            let (a, b) = (
                offset_of(
                    e.range.start.line as usize,
                    e.range.start.character as usize,
                ),
                offset_of(e.range.end.line as usize, e.range.end.character as usize),
            );
            renamed.replace_range(a..b, &e.new_text);
        }
        assert!(!renamed.contains("Auth."), "{renamed}");
        assert!(renamed.contains("usecase::Authn.Login") && renamed.contains("Authn.SessionView"));
        let scratch = std::env::temp_dir().join("rdra-rename-authn");
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("shop-model/shop")).unwrap();
        std::fs::create_dir_all(scratch.join(".git")).unwrap();
        for dep in ["auth-model", "payments-model"] {
            copy_dir(&root.join("..").join(dep), &scratch.join(dep));
        }
        std::fs::copy(root.join("rdra.toml"), scratch.join("shop-model/rdra.toml")).unwrap();
        std::fs::write(scratch.join("shop-model/shop/checkout.rdra"), renamed).unwrap();
        let project = rdra_ish_core::load_project(&[scratch.join("shop-model")], None).unwrap();
        assert!(project.diags.is_empty(), "{:?}", project.diags);
        let _ = std::fs::remove_dir_all(scratch);
    }

    fn copy_dir(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    #[test]
    fn the_qualifier_of_a_reference_renames_the_bound_external_system() {
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/xsys/shop-model");
        let file = root.join("shop/checkout.rdra");
        let src = std::fs::read_to_string(&file).unwrap();
        let project = rdra_ish_core::load_project(std::slice::from_ref(&root), None).unwrap();
        let analysis = rdra_ish_core::analyze_project(project);

        let offset = src.find("Auth.CanLogin").unwrap() + 1;
        let range = prepare_rename_range(&analysis, &file, offset).expect("renamable");
        assert_eq!(
            range.end.character - range.start.character,
            "Auth".len() as u32
        );
        let edit = workspace_rename(&analysis, &file, offset, "Authn").unwrap();
        let edits = edit
            .changes
            .expect("changes")
            .remove(&path_to_uri(&file).unwrap())
            .expect("file edits");
        assert_eq!(edits.len(), 4, "{edits:?}");

        let offset = src.find("Auth.CanLogin").unwrap() + "Auth.".len() + 1;
        assert!(prepare_rename_range(&analysis, &file, offset).is_none());
    }

    #[test]
    fn a_generated_foreign_name_is_refused_as_a_new_name() {
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/xsys/shop-model");
        let file = root.join("shop/checkout.rdra");
        let src = std::fs::read_to_string(&file).unwrap();
        let project = rdra_ish_core::load_project(std::slice::from_ref(&root), None).unwrap();
        let analysis = rdra_ish_core::analyze_project(project);

        let offset = src.find("actor Shopper").unwrap() + "actor ".len();
        assert!(matches!(
            workspace_rename(&analysis, &file, offset, "Auth__Login"),
            Err(RenameError::Conflict)
        ));
    }

    #[test]
    fn rejects_invalid_identifiers() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/purchase");
        let entry = root.join("buc/buc_purchase.rdra");
        let analysis = analyze_workspace(
            std::slice::from_ref(&entry),
            std::slice::from_ref(&root),
            &Default::default(),
        );
        let src = std::fs::read_to_string(&entry).unwrap();
        let offset = src.find("performs").unwrap();
        let result = workspace_rename(&analysis, &entry, offset, "not valid");
        assert_eq!(result, Err(RenameError::InvalidIdentifier));
    }
}
