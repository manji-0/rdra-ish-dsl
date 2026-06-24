//! Quick fixes for common semantic diagnostics.

use std::collections::HashMap;

use rdra_ish_core::{Diagnostic, RdraError, WorkspaceAnalysis};
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Range, TextEdit, Url, WorkspaceEdit,
};

use crate::convert::span_to_range;

pub fn code_actions(
    analysis: &WorkspaceAnalysis,
    source_id: usize,
    text: &str,
    uri: Url,
    range: Range,
) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    for diag in &analysis.diagnostics {
        let Some(loc) = &diag.location else {
            continue;
        };
        if loc.source_id != source_id {
            continue;
        }
        let diag_range = span_to_range(text, loc.span.clone());
        if !ranges_overlap(&diag_range, &range) {
            continue;
        }
        actions.extend(quick_fixes_for_diagnostic(diag, text, uri.clone()));
    }

    actions
}

fn quick_fixes_for_diagnostic(diag: &Diagnostic, text: &str, uri: Url) -> Vec<CodeActionOrCommand> {
    match &diag.error {
        RdraError::AmbiguousReference { id, kinds } => kinds
            .split(',')
            .map(str::trim)
            .filter(|kind| !kind.is_empty())
            .filter_map(|kind| qualify_reference_action(diag, text, uri.clone(), kind, id))
            .collect(),
        RdraError::TypeMismatch { id, expected, .. } => expected
            .split('|')
            .map(str::trim)
            .filter(|kind| !kind.is_empty())
            .filter_map(|kind| qualify_reference_action(diag, text, uri.clone(), kind, bare_id(id)))
            .collect(),
        RdraError::RequirementMetadataOnNonRequirement { .. } => {
            change_declaration_kind_action(diag, text, uri, "requirement")
                .into_iter()
                .collect()
        }
        RdraError::AdrMetadataOnNonAdr { .. } => {
            change_declaration_kind_action(diag, text, uri, "adr")
                .into_iter()
                .collect()
        }
        RdraError::ApiMetadataOnNonApi { .. } => {
            change_declaration_kind_action(diag, text, uri, "api")
                .into_iter()
                .collect()
        }
        RdraError::NfrMetadataOnInvalidKind { .. } => {
            change_declaration_kind_action(diag, text, uri, "nfr")
                .into_iter()
                .collect()
        }
        RdraError::FieldMetadataOnNonField { .. } => {
            change_declaration_kind_action(diag, text, uri, "field")
                .into_iter()
                .collect()
        }
        RdraError::UseCaseMetadataOnNonUseCase { .. } => {
            change_declaration_kind_action(diag, text, uri, "usecase")
                .into_iter()
                .collect()
        }
        _ => Vec::new(),
    }
}

fn change_declaration_kind_action(
    diag: &Diagnostic,
    text: &str,
    uri: Url,
    new_kind: &str,
) -> Option<CodeActionOrCommand> {
    let loc = diag.location.as_ref()?;
    let kind_span = declaration_kind_span(text, loc.span.clone())?;
    let current = text.get(kind_span.clone())?;
    if current == new_kind {
        return None;
    }

    let mut changes = HashMap::new();
    changes.insert(
        uri,
        vec![TextEdit {
            range: span_to_range(text, kind_span),
            new_text: new_kind.to_string(),
        }],
    );

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: format!("Change declaration kind to `{new_kind}`"),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    }))
}

fn declaration_kind_span(
    text: &str,
    decl_span: std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    let slice = text.get(decl_span.clone())?;
    let line_end = slice.find('\n').unwrap_or(slice.len());
    let line = &slice[..line_end];
    let trim_start = line.len() - line.trim_start().len();
    let trimmed = line.trim_start();
    let kind_len = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    if kind_len == 0 {
        return None;
    }
    let start = decl_span.start + trim_start;
    Some(start..start + kind_len)
}

fn qualify_reference_action(
    diag: &Diagnostic,
    text: &str,
    uri: Url,
    kind: &str,
    id: &str,
) -> Option<CodeActionOrCommand> {
    let loc = diag.location.as_ref()?;
    let id_span = id_span_in_diagnostic_span(text, loc.span.clone(), id);
    let slice = text.get(id_span.clone())?;
    if slice.contains("::") {
        return None;
    }

    let mut changes = HashMap::new();
    changes.insert(
        uri,
        vec![TextEdit {
            range: span_to_range(text, id_span),
            new_text: format!("{kind}::{id}"),
        }],
    );

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: format!("Qualify as {kind}::{id}"),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    }))
}

fn bare_id(id: &str) -> &str {
    id.rsplit("::").next().unwrap_or(id)
}

fn id_span_in_diagnostic_span(
    text: &str,
    diag_span: std::ops::Range<usize>,
    id: &str,
) -> std::ops::Range<usize> {
    let slice = text.get(diag_span.clone()).unwrap_or_default();
    if let Some(rel) = slice.rfind(id) {
        let start = diag_span.start + rel;
        start..start + id.len()
    } else {
        diag_span
    }
}

fn ranges_overlap(a: &Range, b: &Range) -> bool {
    if a.start.line > b.end.line || b.start.line > a.end.line {
        return false;
    }
    if a.start.line == b.end.line && a.start.character > b.end.character {
        return false;
    }
    if b.start.line == a.end.line && b.start.character > a.end.character {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use rdra_ish_core::analyze_workspace;
    use rdra_ish_syntax::parse;
    use tower_lsp::lsp_types::{Position, Url};

    use super::*;

    #[test]
    fn offers_kind_qualifier_for_ambiguous_reference() {
        let src = r#"usecase Book "Book"
actor Book "Actor Book"
performs(Book, Book)
"#;
        let root = std::env::temp_dir().join("rdra-code-action-test");
        let _ = std::fs::create_dir_all(&root);
        let file = root.join("sample.rdra");
        std::fs::write(&file, src).unwrap();

        let analysis = analyze_workspace(
            std::slice::from_ref(&file),
            std::slice::from_ref(&root),
            &Default::default(),
        );
        let source_id = 0;
        let uri = Url::from_file_path(&file).unwrap();
        let range = Range {
            start: Position {
                line: 2,
                character: 10,
            },
            end: Position {
                line: 2,
                character: 14,
            },
        };
        let actions = code_actions(&analysis, source_id, src, uri, range);
        assert!(actions.iter().any(|action| {
            let CodeActionOrCommand::CodeAction(action) = action else {
                return false;
            };
            action.title.contains("usecase::Book") || action.title.contains("actor::Book")
        }));

        std::fs::remove_file(&file).ok();
        std::fs::remove_dir(root).ok();
        let (_ast, _) = parse(src);
    }

    #[test]
    fn offers_kind_change_for_requirement_metadata() {
        let src = "usecase Req1 \"Req\"\n  priority \"must\"\n";
        let root = std::env::temp_dir().join("rdra-metadata-action-test");
        let _ = std::fs::create_dir_all(&root);
        let file = root.join("sample.rdra");
        std::fs::write(&file, src).unwrap();

        let analysis = analyze_workspace(
            std::slice::from_ref(&file),
            std::slice::from_ref(&root),
            &Default::default(),
        );
        let uri = Url::from_file_path(&file).unwrap();
        let range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 1,
                character: 20,
            },
        };
        let actions = code_actions(&analysis, 0, src, uri, range);
        assert!(actions.iter().any(|action| {
            let CodeActionOrCommand::CodeAction(action) = action else {
                return false;
            };
            action.title.contains("requirement")
        }));

        std::fs::remove_file(&file).ok();
        std::fs::remove_dir(root).ok();
    }
}
