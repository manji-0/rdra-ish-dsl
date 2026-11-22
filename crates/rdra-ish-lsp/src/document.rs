//! Workspace document lookup helpers shared by LSP request handlers.

use std::path::Path;

use rdra_ish_core::WorkspaceAnalysis;
use rdra_ish_syntax::ast::Ast;

/// A resolved open document in the workspace analysis.
pub struct OpenDocument<'a> {
    pub source_id: usize,
    pub text: &'a str,
    pub ast: &'a Ast,
}

/// Resolve a filesystem path to its workspace source entry.
pub fn open_document<'a>(analysis: &'a WorkspaceAnalysis, path: &Path) -> Option<OpenDocument<'a>> {
    // The resolver stores every source under its canonical path, so the request path is the
    // only one that needs canonicalizing.
    let path = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    analysis
        .program
        .sources
        .iter()
        .enumerate()
        .find_map(|(source_id, (source_path, text, ast))| {
            (*source_path == path).then_some(OpenDocument {
                source_id,
                text,
                ast,
            })
        })
}
