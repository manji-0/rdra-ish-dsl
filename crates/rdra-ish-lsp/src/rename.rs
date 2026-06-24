//! Workspace rename for declared symbols.

use std::collections::HashMap;
use std::path::Path;

use rdra_ish_core::WorkspaceAnalysis;
use rdra_ish_syntax::ast::{Ast, ChainCall, Expr, Item, Operand, PredicateArg, PredicateCall};
use tower_lsp::lsp_types::{Range, TextEdit, Url, WorkspaceEdit};
use url::Url as UrlParser;

use crate::convert::span_to_range;
use crate::refs::{
    id_span_in_qref, instance_id_span, reference_at_offset, symbol_target, SymbolTarget,
};

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
    let target = symbol_target(&analysis.model, reference)?;
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
    let target = symbol_target(&analysis.model, reference).ok_or(RenameError::NoSymbol)?;

    if analysis
        .model
        .decl_sites
        .get(&target.kind, new_name)
        .is_some()
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
            Item::Import(_) | Item::Module(_, _) => {}
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
        PredicateArg::Expr(expr) => {
            let Expr::Cmp(cmp) = expr;
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
        PredicateArg::Tuple(args) => {
            for inner in args {
                collect_rename_spans_in_arg(inner, text, target, spans);
            }
        }
        PredicateArg::Lit(_) => {}
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

fn path_to_uri(path: &Path) -> std::io::Result<Url> {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let url = UrlParser::from_file_path(&path).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid path for uri")
    })?;
    Ok(url)
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    let ca = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let cb = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    ca == cb
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
