//! Document outline and workspace symbols.

use std::path::Path;

use rdra_ish_core::WorkspaceAnalysis;
use rdra_ish_syntax::ast::{Ast, Item};
use tower_lsp::lsp_types::{DocumentSymbol, Location, SymbolInformation, SymbolKind};
use url::Url;

use crate::convert::span_to_range;

pub fn document_symbols(ast: &Ast) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();
    for item in &ast.items {
        if let Item::Instance(inst) = item {
            symbols.push(DocumentSymbol {
                name: inst.id.clone(),
                detail: Some(inst.label.clone()),
                kind: SymbolKind::CLASS,
                tags: None,
                range: span_to_range(&ast.source, inst.span.clone()),
                selection_range: span_to_range(&ast.source, inst.span.clone()),
                children: None,
                #[allow(deprecated)]
                deprecated: None,
            });
        }
    }
    symbols
}

pub fn workspace_symbols(analysis: &WorkspaceAnalysis, query: &str) -> Vec<SymbolInformation> {
    let query = query.to_lowercase();
    let mut symbols = Vec::new();

    for (kind, id, site) in analysis.model.decl_sites.iter() {
        if !query.is_empty()
            && !id.to_lowercase().contains(&query)
            && !kind.to_lowercase().contains(&query)
        {
            continue;
        }
        let Some((path, text, _)) = analysis.program.sources.get(site.source_id) else {
            continue;
        };
        let Ok(uri) = path_to_uri(path) else {
            continue;
        };
        symbols.push(SymbolInformation {
            name: id.to_string(),
            kind: SymbolKind::CLASS,
            tags: None,
            location: Location {
                uri,
                range: span_to_range(text, site.span.clone()),
            },
            container_name: Some(kind.to_string()),
            #[allow(deprecated)]
            deprecated: None,
        });
    }

    symbols.sort_by(|a, b| a.name.cmp(&b.name));
    symbols
}

fn path_to_uri(path: &Path) -> std::io::Result<Url> {
    Url::from_file_path(path)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid path for uri"))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rdra_ish_core::analyze_workspace;

    use super::*;

    #[test]
    fn workspace_symbol_search_matches_id_and_kind() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/purchase");
        let entry = root.join("buc/buc_purchase.rdra");
        let analysis = analyze_workspace(&[entry], &[root], &Default::default());
        let all = workspace_symbols(&analysis, "");
        assert!(!all.is_empty());

        let actors = workspace_symbols(&analysis, "actor");
        assert!(actors
            .iter()
            .any(|symbol| symbol.container_name.as_deref() == Some("actor")));

        let misses = workspace_symbols(&analysis, "zzzznotfound");
        assert!(misses.is_empty());
    }
}
