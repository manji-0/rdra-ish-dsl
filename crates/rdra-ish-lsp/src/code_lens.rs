//! Reference-count code lenses on declarations.

use std::path::Path;

use rdra_ish_core::WorkspaceAnalysis;
use rdra_ish_syntax::ast::{Ast, Item};
use tower_lsp::lsp_types::{CodeLens, Command, Location};

use crate::convert::{byte_offset_to_position, span_to_range};
use crate::refs::{find_symbol_references, instance_id_span, SymbolTarget};
use crate::uri::path_to_uri;

pub fn code_lenses(
    analysis: &WorkspaceAnalysis,
    ast: &Ast,
    text: &str,
    file_path: &Path,
) -> Vec<CodeLens> {
    let mut lenses = Vec::new();
    let file_uri = match path_to_uri(file_path) {
        Ok(uri) => uri,
        Err(_) => return lenses,
    };

    for item in &ast.items {
        let Item::Instance(inst) = item else {
            continue;
        };
        let target = SymbolTarget {
            kind: inst.kind.name().to_string(),
            id: inst.id.clone(),
        };
        let locations = reference_locations(analysis, &target);
        let count = locations.len();

        let id_span = instance_id_span(inst, text);
        let range = declaration_lens_range(text, &id_span);
        let position = byte_offset_to_position(text, id_span.start);

        let command = if count == 0 {
            Command {
                title: "0 references".to_string(),
                command: String::new(),
                arguments: None,
            }
        } else {
            Command {
                title: format!("{count} references"),
                command: "editor.action.showReferences".to_string(),
                arguments: Some(vec![
                    serde_json::to_value(&file_uri).expect("uri"),
                    serde_json::to_value(position).expect("position"),
                    serde_json::to_value(&locations).expect("locations"),
                ]),
            }
        };

        lenses.push(CodeLens {
            range,
            command: Some(command),
            data: None,
        });
    }

    lenses
}

pub fn reference_locations(analysis: &WorkspaceAnalysis, target: &SymbolTarget) -> Vec<Location> {
    let mut locations = Vec::new();

    for (path, source_text, source_ast) in &analysis.program.sources {
        let Ok(uri) = path_to_uri(path) else {
            continue;
        };

        for item in &source_ast.items {
            if let Item::Instance(inst) = item {
                if inst.kind.name() == target.kind && inst.id == target.id {
                    locations.push(Location {
                        uri: uri.clone(),
                        range: span_to_range(source_text, instance_id_span(inst, source_text)),
                    });
                }
            }
        }

        for span in find_symbol_references(source_ast, target) {
            locations.push(Location {
                uri: uri.clone(),
                range: span_to_range(source_text, span),
            });
        }
    }

    locations
}

fn declaration_lens_range(
    text: &str,
    id_span: &std::ops::Range<usize>,
) -> tower_lsp::lsp_types::Range {
    let line_start = text[..id_span.start]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let line_end = text[id_span.start..]
        .find('\n')
        .map(|index| id_span.start + index)
        .unwrap_or(text.len());
    span_to_range(text, line_start..line_end)
}

#[cfg(test)]
mod tests {
    use rdra_ish_core::analyze_workspace;

    use super::*;

    #[test]
    fn counts_predicate_references() {
        let src = r#"usecase Book "Book"
actor Staff "Staff"
performs(Staff, Book)
invokes(Book, BookingApi)
api BookingApi "API"
"#;
        let root = std::env::temp_dir().join("rdra-codelens-test");
        let _ = std::fs::create_dir_all(&root);
        let file = root.join("sample.rdra");
        std::fs::write(&file, src).unwrap();

        let analysis = analyze_workspace(
            std::slice::from_ref(&file),
            std::slice::from_ref(&root),
            &Default::default(),
        );
        let (_, _, ast) = &analysis.program.sources[0];
        let lenses = code_lenses(&analysis, ast, src, &file);
        let book = lenses
            .iter()
            .find(|lens| {
                lens.command
                    .as_ref()
                    .is_some_and(|cmd| cmd.title.contains("references"))
            })
            .and_then(|lens| lens.command.as_ref())
            .expect("book lens");
        assert!(book.title.starts_with('3'));
        assert_eq!(book.command, "editor.action.showReferences".to_string());
        assert!(book.arguments.as_ref().is_some_and(|args| args.len() == 3));

        std::fs::remove_file(&file).ok();
        std::fs::remove_dir(root).ok();
    }
}
