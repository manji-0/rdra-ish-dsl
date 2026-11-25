//! Reference-count code lenses on declarations.

use std::collections::HashMap;
use std::path::Path;

use rdra_ish_core::WorkspaceAnalysis;
use rdra_ish_syntax::ast::{Ast, Item, Span};
use tower_lsp::lsp_types::{CodeLens, Command, Location, Url};

use crate::convert::{byte_offset_to_position, span_to_range};
use crate::refs::{ast_references, instance_id_span};
use crate::uri::ClientUris;

pub fn code_lenses(
    analysis: &WorkspaceAnalysis,
    uris: &ClientUris,
    ast: &Ast,
    text: &str,
    file_path: &Path,
) -> Vec<CodeLens> {
    let mut lenses = Vec::new();
    let file_uri = match uris.uri(file_path) {
        Ok(uri) => uri,
        Err(_) => return lenses,
    };

    let index = reference_index(analysis, uris);
    for item in &ast.items {
        let Item::Instance(inst) = item else {
            continue;
        };
        let locations = index
            .get(&(inst.kind.name(), inst.id.as_str()))
            .map(Vec::as_slice)
            .unwrap_or_default();
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
                    serde_json::to_value(locations).expect("locations"),
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

type ReferenceIndex<'a> = HashMap<(&'a str, &'a str), Vec<Location>>;

fn add_location<'a>(
    index: &mut ReferenceIndex<'a>,
    kind: &'a str,
    id: &'a str,
    (uri, text): (&Url, &str),
    span: Span,
) {
    index.entry((kind, id)).or_default().push(Location {
        uri: uri.clone(),
        range: span_to_range(text, span),
    });
}

/// The declaration and reference locations of every `(kind, id)` declared in the workspace,
/// in source order with a source's declarations before its references. One walk of the
/// workspace answers every lens, instead of one walk per declaration.
fn reference_index<'a>(analysis: &'a WorkspaceAnalysis, uris: &ClientUris) -> ReferenceIndex<'a> {
    let mut kinds_by_id: HashMap<&str, Vec<&str>> = HashMap::new();
    for (_, _, ast) in &analysis.program.sources {
        for item in &ast.items {
            if let Item::Instance(inst) = item {
                let kinds = kinds_by_id.entry(&inst.id).or_default();
                if !kinds.contains(&inst.kind.name()) {
                    kinds.push(inst.kind.name());
                }
            }
        }
    }

    let mut index = ReferenceIndex::new();
    for (path, text, ast) in &analysis.program.sources {
        let Ok(uri) = uris.uri(path) else {
            continue;
        };
        let mut push = |kind, id, span| add_location(&mut index, kind, id, (&uri, text), span);
        for item in &ast.items {
            if let Item::Instance(inst) = item {
                push(inst.kind.name(), &inst.id, instance_id_span(inst, text));
            }
        }
        for qref in ast_references(ast) {
            let Some(id) = qref.parts.last() else {
                continue;
            };
            match &qref.kind_qualifier {
                Some(kind) => push(kind.name(), id, qref.span.clone()),
                None => {
                    for kind in kinds_by_id.get(id.as_str()).into_iter().flatten() {
                        push(kind, id, qref.span.clone());
                    }
                }
            }
        }
    }
    index
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
        let lenses = code_lenses(&analysis, &ClientUris::default(), ast, src, &file);
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
