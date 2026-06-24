//! Workspace document lookup helpers shared by LSP request handlers.

use std::path::Path;

use rdra_ish_core::WorkspaceAnalysis;
use rdra_ish_syntax::ast::Ast;
use tower_lsp::lsp_types::Position;

use crate::convert::position_to_byte_offset;
use crate::uri::paths_equal;

/// A resolved open document in the workspace analysis.
pub struct OpenDocument<'a> {
    pub source_id: usize,
    pub text: &'a str,
    pub ast: &'a Ast,
}

/// Resolve a filesystem path to its workspace source entry.
pub fn open_document<'a>(analysis: &'a WorkspaceAnalysis, path: &Path) -> Option<OpenDocument<'a>> {
    analysis
        .program
        .sources
        .iter()
        .enumerate()
        .find_map(|(source_id, (source_path, text, ast))| {
            paths_equal(source_path, path).then_some(OpenDocument {
                source_id,
                text,
                ast,
            })
        })
}

/// Resolve a document and byte offset from an LSP cursor position.
pub fn byte_offset_at<'a>(
    analysis: &'a WorkspaceAnalysis,
    path: &Path,
    position: Position,
) -> Option<(OpenDocument<'a>, usize)> {
    let doc = open_document(analysis, path)?;
    let offset = position_to_byte_offset(doc.text, position)?;
    Some((doc, offset))
}
