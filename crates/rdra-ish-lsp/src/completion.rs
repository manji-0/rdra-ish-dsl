//! Completion candidates for the RDRA-ish DSL.

use rdra_ish_core::{SemanticModel, KNOWN_PREDICATES};
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

use crate::predicates::{predicate_arg_context, predicate_arg_kinds};

const KIND_KEYWORDS: &[&str] = &[
    "actor",
    "extsystem",
    "system",
    "requirement",
    "adr",
    "nfr",
    "quality",
    "constraint",
    "concept",
    "domain_object",
    "aggregate",
    "valueobject",
    "business",
    "buc",
    "flow",
    "step",
    "usagescene",
    "usecase",
    "screen",
    "field",
    "event",
    "entity",
    "state",
    "condition",
    "variation",
    "api",
    "dto",
    "location",
    "timing",
    "medium",
    "permission",
];

pub fn completion_items(model: &SemanticModel, source: &str, offset: usize) -> Vec<CompletionItem> {
    let prefix = identifier_prefix(source, offset);
    let mut items = Vec::new();

    if completing_kind_keyword(source, offset) {
        extend_filtered(
            &mut items,
            KIND_KEYWORDS,
            prefix,
            CompletionItemKind::KEYWORD,
            None,
        );
    }

    if completing_predicate_name(source, offset) {
        extend_filtered(
            &mut items,
            KNOWN_PREDICATES,
            prefix,
            CompletionItemKind::FUNCTION,
            Some("predicate"),
        );
    }

    if completing_symbol(source, offset) {
        let allowed_kinds = predicate_arg_context(source, offset)
            .and_then(|(pred, arg_index)| predicate_arg_kinds(&pred, arg_index));

        for (kind, id, _) in model.decl_sites.iter() {
            if let Some(kinds) = &allowed_kinds {
                if !kinds.contains(&kind) {
                    continue;
                }
            }
            if prefix.is_empty() || id.starts_with(prefix) {
                items.push(CompletionItem {
                    label: id.to_string(),
                    kind: Some(CompletionItemKind::REFERENCE),
                    detail: Some(kind.to_string()),
                    ..Default::default()
                });
            }
        }
    }

    if completing_kind_qualifier(source, offset) {
        for name in KIND_KEYWORDS {
            if prefix.is_empty() || name.starts_with(prefix) {
                items.push(CompletionItem {
                    label: format!("{name}::"),
                    kind: Some(CompletionItemKind::ENUM),
                    detail: Some("kind-qualified reference".to_string()),
                    ..Default::default()
                });
            }
        }
    }

    items.sort_by(|a, b| a.label.cmp(&b.label));
    items.dedup_by(|a, b| a.label == b.label);
    items
}

fn extend_filtered(
    items: &mut Vec<CompletionItem>,
    labels: &[&str],
    prefix: &str,
    kind: CompletionItemKind,
    detail: Option<&str>,
) {
    for label in labels {
        if prefix.is_empty() || label.starts_with(prefix) {
            items.push(CompletionItem {
                label: (*label).to_string(),
                kind: Some(kind),
                detail: detail.map(str::to_string),
                ..Default::default()
            });
        }
    }
}

fn identifier_prefix(source: &str, offset: usize) -> &str {
    let offset = offset.min(source.len());
    let line_start = source[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let line = &source[line_start..offset];
    line.rfind(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .map(|index| &line[index + 1..])
        .unwrap_or(line)
}

fn completing_predicate_name(source: &str, offset: usize) -> bool {
    let offset = offset.min(source.len());
    let line_start = source[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let before = source[line_start..offset].trim();
    if before.is_empty() {
        return true;
    }
    if before.contains('(') {
        return false;
    }
    before
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn completing_kind_keyword(source: &str, offset: usize) -> bool {
    let offset = offset.min(source.len());
    let line_start = source[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let line = source[line_start..offset].trim();
    line.is_empty()
        || (line
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            && !line.contains('(')
            && !line.contains(':'))
}

fn completing_symbol(source: &str, offset: usize) -> bool {
    let offset = offset.min(source.len());
    let before = &source[..offset];
    before.contains('(') || before.contains(',')
}

fn completing_kind_qualifier(source: &str, offset: usize) -> bool {
    let offset = offset.min(source.len());
    let line_start = source[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let line = &source[line_start..offset];
    !line.contains("::") && line.contains(':') && line.trim_end().ends_with(':')
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    #[test]
    fn filters_predicate_args_by_kind() {
        let src = r#"usecase Book "Book"
actor Staff "Staff"
actor Guest "Guest"
performs(Staff, )
"#;
        let (ast, errs) = parse(src);
        assert!(errs.is_empty());
        let (model, _) = build_model(&ast);
        let offset = src.find(", )").unwrap() + 2;
        let items = completion_items(&model, src, offset);
        let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();
        assert!(labels.contains(&"Book"));
        assert!(!labels.contains(&"Staff"));
        assert!(!labels.contains(&"Guest"));
    }

    #[test]
    fn offers_predicate_name_at_line_start() {
        let src = "inv";
        let (ast, _) = parse("usecase X \"x\"");
        let (model, _) = build_model(&ast);
        let items = completion_items(&model, src, src.len());
        assert!(items.iter().any(|item| item.label == "invokes"));
    }
}
