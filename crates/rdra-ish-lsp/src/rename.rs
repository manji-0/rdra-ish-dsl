//! Workspace rename for declared symbols.

use std::collections::HashMap;
use std::path::Path;

use crate::convert::span_to_range;
use crate::refs::{
    id_span_in_qref, instance_id_span, reference_at_offset, references_to, symbol_target,
    SymbolTarget, ROLE_KIND,
};
use crate::uri::{path_to_uri, paths_equal};
use rdra_ish_core::WorkspaceAnalysis;
use rdra_ish_syntax::ast::{Ast, Item};
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

    let taken = if target.kind == ROLE_KIND {
        analysis
            .model
            .foreign_keys()
            .any(|(_, fk)| fk.role.as_deref() == Some(new_name))
    } else {
        analysis
            .model
            .decl_sites
            .get(&target.kind, new_name)
            .is_some()
    };
    if taken {
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
        if let Item::Instance(inst) = item {
            if inst.kind.name() == target.kind && inst.id == target.id {
                spans.push(instance_id_span(inst, text));
            }
        }
    }
    spans.extend(
        references_to(ast, target)
            .into_iter()
            .filter_map(|qref| id_span_in_qref(qref, text, target)),
    );

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

    /// The source of a copy (`sets(UC, E.col == Src.col)`, #69) is a reference to `Src`.
    #[test]
    fn renames_the_source_entity_of_a_copy() {
        let src = "variation K \"k\" {\n  a \"a\"\n  b \"b\"\n}\nentity Src \"s\" {\n  id: Int @pk\n  kind: K\n}\nentity Dst \"d\" {\n  id: Int @pk\n  kind: K\n}\nrelate(Dst, Src, N:1)\nusecase Make \"m\"\ncreates(Make, Dst)\nsets(Make, Dst.kind == Src.kind)\n";
        let root = std::env::temp_dir().join(format!("rdra-rename-copy-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        let file = root.join("sample.rdra");
        std::fs::write(&file, src).unwrap();
        let analysis = analyze_workspace(
            std::slice::from_ref(&file),
            std::slice::from_ref(&root),
            &Default::default(),
        );
        let offset = src.rfind("Src.kind").unwrap();
        let edit = workspace_rename(&analysis, &file, offset, "Origin").unwrap();
        let uri = path_to_uri(&file).unwrap();
        let edits = edit
            .changes
            .expect("changes")
            .remove(&uri)
            .expect("file edits");
        std::fs::remove_dir_all(&root).ok();
        // The declaration, the relate, and the copy.
        assert_eq!(edits.len(), 3, "{edits:?}");
    }

    /// A key's role is a symbol: `relate(..).role(renews)` declares it, and the `.role(renews)` of a
    /// `holds`, a `.none(..)` and a copy refer to it (#73). A value spelled the same is not a
    /// reference, and renaming onto another role of the model is a conflict.
    #[test]
    fn renames_a_key_role_and_each_role_that_picks_it() {
        let src = "entity Permit \"p\" {\n  id: Int @pk\n  status: Enum(valid, renews)\n}\nentity App \"a\" {\n  id: Int @pk\n  status: Enum(open, renews)\n}\nrelate(Permit, App, 1:1)\nrelate(App, Permit, N:1).optional().role(renews)\ncondition C \"c\"\nholds(C, Permit.status == valid).role(renews)\ncondition D \"d\"\nholds(D, App.status == open).none(Permit).role(renews)\n";
        let root = std::env::temp_dir().join(format!("rdra-rename-role-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        let file = root.join("sample.rdra");
        std::fs::write(&file, src).unwrap();
        let analysis = analyze_workspace(
            std::slice::from_ref(&file),
            std::slice::from_ref(&root),
            &Default::default(),
        );
        let uri = path_to_uri(&file).unwrap();
        let pick_at = src.find("valid).role(renews)").unwrap() + "valid).role(".len();
        let role_at = src.find(".optional().role(renews)").unwrap() + ".optional().role(".len();
        for offset in [pick_at, role_at] {
            let edit = workspace_rename(&analysis, &file, offset, "continues").unwrap();
            let edits = edit
                .changes
                .expect("changes")
                .remove(&uri)
                .expect("file edits");
            // The relate's `.role(renews)` and the two that pick it; not the two enum values.
            assert_eq!(edits.len(), 3, "{edits:?}");
        }
        assert_eq!(
            workspace_rename(&analysis, &file, pick_at, "renews"),
            Err(RenameError::Conflict)
        );
        let (_, _, ast) = &analysis.program.sources[0];
        let reference = reference_at_offset(ast, pick_at).expect("reference");
        let decl = crate::refs::resolve_decl_site(&analysis.model, reference).expect("decl");
        assert!(src[decl.span.clone()].starts_with("relate(App, Permit, N:1)"));
        std::fs::remove_dir_all(&root).ok();
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

    /// Declarations the chain-argument samples rename: each declares `Tgt` as one kind.
    const ENTITY_TGT: &str = "entity A \"a\" {\n  id: Int @pk\n  status: String\n}\nentity B \"b\" {\n  id: Int @pk\n}\nentity Tgt \"t\" {\n  id: Int @pk\n  status: String\n}\n";
    const USECASE_TGT: &str =
        "usecase Tgt \"t\"\nusecase U \"u\"\nscreen S1 \"s1\"\nscreen S2 \"s2\"\n";
    const CONDITION_TGT: &str = "condition Tgt \"t\"\nusecase U \"u\"\nentity A \"a\" {\n  id: Int @pk\n  status: String\n}\n";
    const MEDIUM_TGT: &str = "medium Tgt \"m\"\nbuc B \"b\"\nbusiness Z \"z\"\n";

    /// A sample that writes `Tgt` in the argument of one chain method, or `None` when the
    /// method takes no symbol reference (a column name, a keyword, a label string).
    fn chain_reference_sample(pred: &str, method: &str) -> Option<(&'static str, &'static str)> {
        let sample = match (pred, method) {
            ("belongs", "by") => (MEDIUM_TGT, "belongs(B, Z).by(Tgt)"),
            ("relate", "via") => (ENTITY_TGT, "relate(A, B, N:M).via(Tgt)"),
            ("relate", "role" | "key" | "optional" | "on_delete" | "on_update") => return None,
            ("forbidden", "along") => (
                ENTITY_TGT,
                "forbidden(A).along(A, Tgt).when(Tgt.status == x)",
            ),
            ("forbidden", "when") => (
                CONDITION_TGT,
                "forbidden(A, A.status == x).when(Tgt)",
            ),
            (_, "label") => return None,
            ("forbidden" | "invariant", "role") => return None,
            // `.role(Tgt)` names the key by its parent id when the key has no role.
            ("holds", "role") => (
                ENTITY_TGT,
                "relate(A, Tgt, N:1)\ncondition C \"c\"\nholds(C, Tgt.status == x).role(Tgt)",
            ),
            ("sets", "role") => (
                ENTITY_TGT,
                "relate(A, Tgt, N:1)\nusecase U \"u\"\nupdates(U, A)\nsets(U, A.status == Tgt.status).role(Tgt)",
            ),
            ("links", "role") => (
                ENTITY_TGT,
                "relate(A, Tgt, N:1).optional()\nusecase U \"u\"\nlinks(U, A, Tgt).role(Tgt)",
            ),
            ("invariant", "when") => (
                ENTITY_TGT,
                "invariant(A).when(Tgt.status == x).then(A.status == y)",
            ),
            ("invariant", "then") => (
                ENTITY_TGT,
                "invariant(A).when(A.status == x).then(Tgt.status == y)",
            ),
            ("invariant", "has") => (
                ENTITY_TGT,
                "invariant(A).when(A.status == x).has(Tgt.status == y)",
            ),
            ("invariant", "none") => (
                ENTITY_TGT,
                "invariant(A).when(A.status == x).none(Tgt.status == y)",
            ),
            ("invariant", "along") => (
                ENTITY_TGT,
                "invariant(A).along(A, Tgt).when(A.status == x).then(A.status == y)",
            ),
            ("holds", "along") => (
                ENTITY_TGT,
                "relate(A, Tgt, N:1)\ncondition C \"c\"\nholds(C, Tgt.status == x).along(A, Tgt)",
            ),
            ("holds", "has") => (
                ENTITY_TGT,
                "condition C \"c\"\nholds(C, A.status == x).has(Tgt.status == y)",
            ),
            ("holds", "none") => (
                ENTITY_TGT,
                "condition C \"c\"\nholds(C, A.status == x).none(Tgt.status == y)",
            ),
            ("after", "assert") => (
                ENTITY_TGT,
                "usecase U \"u\"\nafter(U).assert(Tgt.status == x)",
            ),
            ("exposes", "append" | "upsert" | "caller") => return None,
            ("navigates", "via") => (USECASE_TGT, "navigates(S1, S2).via(Tgt)"),
            ("transitions", "when") => (
                CONDITION_TGT,
                "transitions(U, A.status, x -> y).when(Tgt)",
            ),
            ("chooses", "when") => (CONDITION_TGT, "chooses(U, A.status, x).when(Tgt)"),
            _ => panic!(
                "`{pred}(...).{method}(...)` has no rename sample: add one, or `None` if it takes no symbol"
            ),
        };
        Some(sample)
    }

    /// Renames `Tgt` once from its declaration and once from the reference inside the
    /// `.{method}(...)` argument, and asserts both edit that reference and the declaration.
    fn assert_chain_reference_renamed(label: &str, method: &str, decls: &str, usage: &str) {
        let src = format!("{decls}{usage}\n");
        let chain_at = src.find(&format!(".{method}(")).expect("chain call");
        let ref_at = chain_at + src[chain_at..].find("Tgt").expect("Tgt in chain argument");
        let decl_at = src.find("Tgt").unwrap();

        let root = std::env::temp_dir().join(format!(
            "rdra-rename-chain-{}-{label}-{method}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("sample.rdra");
        std::fs::write(&file, &src).unwrap();
        let analysis = analyze_workspace(
            std::slice::from_ref(&file),
            std::slice::from_ref(&root),
            &Default::default(),
        );
        let uri = path_to_uri(&file).unwrap();
        let ref_range = span_to_range(&src, ref_at..ref_at + 3);
        let decl_range = span_to_range(&src, decl_at..decl_at + 3);

        for (from, offset) in [("declaration", decl_at), ("chain argument", ref_at)] {
            let edit = workspace_rename(&analysis, &file, offset, "Renamed")
                .unwrap_or_else(|e| panic!("{label}.{method} from {from}: {e:?}\n{src}"));
            let edits = edit
                .changes
                .expect("changes")
                .remove(&uri)
                .expect("file edits");
            let ranges: Vec<Range> = edits.iter().map(|e| e.range).collect();
            assert!(
                ranges.contains(&ref_range) && ranges.contains(&decl_range),
                "{label}.{method} from {from}: rename missed a `Tgt`, edits {ranges:?}\n{src}"
            );
        }

        std::fs::remove_file(&file).ok();
        std::fs::remove_dir(root).ok();
    }

    #[test]
    fn renames_references_in_every_chain_method_argument() {
        let mut covered = 0;
        for pred in rdra_ish_core::KNOWN_PREDICATES {
            for method in rdra_ish_core::predicate::predicate_chain_methods(pred) {
                if let Some((decls, usage)) = chain_reference_sample(pred, method) {
                    assert_chain_reference_renamed(pred, method, decls, usage);
                    covered += 1;
                }
            }
        }
        assert!(covered >= 13, "only {covered} chain samples ran");

        // A property's `.fair(UseCase, ...)` is a chain outside the predicate catalog.
        assert_chain_reference_renamed(
            "property",
            "fair",
            "usecase Tgt \"t\"\nentity Order \"o\" {\n  id: Int @pk\n  status: String\n}\n",
            "property P leads_to(Order.status == open, Order.status == paid).fair(Tgt)",
        );
    }
}
