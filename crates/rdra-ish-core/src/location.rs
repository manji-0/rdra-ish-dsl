//! Source locations for diagnostics and future LSP support.

use rdra_ish_syntax::ast::Span;
use std::path::Path;

use crate::resolver::ResolvedProgram;

/// Identifies a single source file by its index in [`ResolvedProgram::sources`].
pub type SourceId = usize;

/// Byte offset range in a source file (`start..end`).
pub type ByteSpan = Span;

/// A span tied to a file in a [`ResolvedProgram`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedSpan {
    pub source_id: SourceId,
    pub span: ByteSpan,
}

impl LocatedSpan {
    pub fn new(source_id: SourceId, span: ByteSpan) -> Self {
        Self { source_id, span }
    }

    pub fn path<'a>(&self, program: &'a ResolvedProgram) -> Option<&'a Path> {
        program
            .sources
            .get(self.source_id)
            .map(|(path, _, _)| path.as_path())
    }

    pub fn source_text<'a>(&self, program: &'a ResolvedProgram) -> Option<&'a str> {
        program
            .sources
            .get(self.source_id)
            .map(|(_, text, _)| text.as_str())
    }

    /// 1-based line and column of the span start.
    pub fn start_position(&self, program: &ResolvedProgram) -> Option<Position> {
        let text = self.source_text(program)?;
        Some(byte_offset_to_position(text, self.span.start))
    }

    /// 1-based line and column of the span end.
    pub fn end_position(&self, program: &ResolvedProgram) -> Option<Position> {
        let text = self.source_text(program)?;
        Some(byte_offset_to_position(text, self.span.end))
    }
}

/// 1-based line/column position in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

/// Declaration site for a model element (`kind:id` key).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclSite {
    pub source_id: SourceId,
    pub span: ByteSpan,
}

impl DeclSite {
    pub fn located(&self) -> LocatedSpan {
        LocatedSpan::new(self.source_id, self.span.clone())
    }
}

/// Per-file diagnostic context passed while building the semantic model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagCtxt {
    pub source_id: SourceId,
}

impl DiagCtxt {
    pub fn new(source_id: SourceId) -> Self {
        Self { source_id }
    }

    pub fn locate(&self, span: ByteSpan) -> LocatedSpan {
        LocatedSpan::new(self.source_id, span)
    }
}

/// Convert a byte offset to a 1-based line/column position.
pub fn byte_offset_to_position(source: &str, offset: usize) -> Position {
    let mut line = 1u32;
    let mut column = 1u32;
    for (idx, ch) in source.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    Position { line, column }
}

/// Format `path:line:column` for CLI and editor integration.
pub fn format_location(program: &ResolvedProgram, loc: &LocatedSpan) -> Option<String> {
    let path = loc.path(program)?;
    let pos = loc.start_position(program)?;
    Some(format!("{}:{}:{}", path.display(), pos.line, pos.column))
}

/// Format a diagnostic prefix (`error` / `warning`) with an optional location.
pub fn format_diagnostic_message(
    program: Option<&ResolvedProgram>,
    is_warning: bool,
    location: Option<&LocatedSpan>,
    message: &str,
) -> String {
    let level = if is_warning { "warning" } else { "error" };
    if let (Some(program), Some(loc)) = (program, location) {
        if let Some(prefix) = format_location(program, loc) {
            return format!("{level}: {prefix}: {message}");
        }
    }
    format!("{level}: {message}")
}

/// Lookup table: `kind:id` → declaration site.
#[derive(Debug, Default, Clone)]
pub struct DeclIndex {
    sites: std::collections::HashMap<String, DeclSite>,
}

impl DeclIndex {
    pub fn insert(&mut self, kind: &str, id: &str, site: DeclSite) {
        self.sites.insert(decl_key(kind, id), site);
    }

    pub fn get(&self, kind: &str, id: &str) -> Option<&DeclSite> {
        self.sites.get(&decl_key(kind, id))
    }

    pub fn located(&self, kind: &str, id: &str) -> Option<LocatedSpan> {
        self.get(kind, id).map(DeclSite::located)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str, &DeclSite)> + '_ {
        self.sites.iter().filter_map(|(key, site)| {
            let (kind, id) = key.split_once(':')?;
            Some((kind, id, site))
        })
    }
}

pub fn decl_key(kind: &str, id: &str) -> String {
    format!("{kind}:{id}")
}

/// Push a diagnostic at a declaration site when known, otherwise without location.
pub fn push_decl_diagnostic(
    decl_sites: &DeclIndex,
    diags: &mut Vec<crate::diagnostics::Diagnostic>,
    kind: &str,
    id: &str,
    err: crate::diagnostics::RdraError,
    is_warning: bool,
) {
    let diag = match decl_sites.located(kind, id) {
        Some(loc) if is_warning => crate::diagnostics::Diagnostic::warning_at(err, loc),
        Some(loc) => crate::diagnostics::Diagnostic::error_at(err, loc),
        None if is_warning => crate::diagnostics::Diagnostic::warning(err),
        None => crate::diagnostics::Diagnostic::error(err),
    };
    diags.push(diag);
}

/// Convenience wrapper using [`SemanticModel::decl_sites`].
pub fn push_model_decl_diagnostic(
    model: &crate::model::SemanticModel,
    diags: &mut Vec<crate::diagnostics::Diagnostic>,
    kind: &str,
    id: &str,
    err: crate::diagnostics::RdraError,
    is_warning: bool,
) {
    push_decl_diagnostic(&model.decl_sites, diags, kind, id, err, is_warning);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_offset_to_position_counts_lines() {
        let src = "actor A \"a\"\nusecase B \"b\"";
        let second_line = src.find("usecase").unwrap();
        let pos = byte_offset_to_position(src, second_line);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn decl_index_stores_and_retrieves_sites() {
        let mut index = DeclIndex::default();
        index.insert(
            "actor",
            "Customer",
            DeclSite {
                source_id: 0,
                span: 0..7,
            },
        );
        assert!(index.get("actor", "Customer").is_some());
        assert!(index.get("actor", "Staff").is_none());
    }
}
