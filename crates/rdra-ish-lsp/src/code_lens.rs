//! Reference-count code lenses on declarations.

use rdra_ish_core::WorkspaceAnalysis;
use rdra_ish_syntax::ast::{Ast, Item};
use tower_lsp::lsp_types::{CodeLens, Command, Range};

use crate::convert::span_to_range;
use crate::refs::{find_symbol_references, instance_id_span, SymbolTarget};

pub fn code_lenses(analysis: &WorkspaceAnalysis, ast: &Ast, text: &str) -> Vec<CodeLens> {
    let mut lenses = Vec::new();

    for item in &ast.items {
        let Item::Instance(inst) = item else {
            continue;
        };
        let target = SymbolTarget {
            kind: inst.kind.name().to_string(),
            id: inst.id.clone(),
        };
        let mut count = 0usize;
        for (_, _, file_ast) in &analysis.program.sources {
            count += find_symbol_references(file_ast, &target).len();
        }

        let id_span = instance_id_span(inst, text);
        let range = declaration_lens_range(text, &id_span);
        lenses.push(CodeLens {
            range,
            command: Some(Command {
                title: format!("{count} references"),
                command: String::new(),
                arguments: None,
            }),
            data: None,
        });
    }

    lenses
}

fn declaration_lens_range(text: &str, id_span: &std::ops::Range<usize>) -> Range {
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
        let lenses = code_lenses(&analysis, ast, src);
        let book = lenses
            .iter()
            .find(|lens| {
                lens.command
                    .as_ref()
                    .is_some_and(|cmd| cmd.title.contains("references"))
            })
            .and_then(|lens| lens.command.as_ref())
            .expect("book lens");
        assert!(book.title.starts_with('2'));

        std::fs::remove_file(&file).ok();
        std::fs::remove_dir(root).ok();
    }
}
