//! Semantic token classification for syntax highlighting.

use rdra_ish_syntax::ast::{
    Ast, ChainCall, ColType, Expr, Item, Operand, PredicateArg, PredicateCall, QRef,
};

use crate::convert::byte_offset_to_position;
use crate::refs::instance_id_span;

const TOKEN_KEYWORD: u32 = 0;
const TOKEN_TYPE: u32 = 1;
const TOKEN_FUNCTION: u32 = 2;
const TOKEN_STRING: u32 = 3;
const TOKEN_VARIABLE: u32 = 4;
const TOKEN_PROPERTY: u32 = 5;

const MOD_DEFINITION: u32 = 1 << 1;

pub const TOKEN_TYPES: &[&str] = &[
    "keyword", "type", "function", "string", "variable", "property",
];

pub const TOKEN_MODIFIERS: &[&str] = &["declaration", "definition"];

struct RawToken {
    start: usize,
    length: u32,
    token_type: u32,
    modifiers: u32,
}

pub fn semantic_tokens(ast: &Ast) -> tower_lsp::lsp_types::SemanticTokens {
    let source = ast.source.as_str();
    let mut raw = Vec::new();

    for item in &ast.items {
        match item {
            Item::Module(_, span) => {
                push_keyword(source, &mut raw, keyword_span(source, span, "module"));
            }
            Item::Import(import) => {
                push_keyword(
                    source,
                    &mut raw,
                    keyword_span(source, &import.span, "import"),
                );
            }
            Item::Instance(inst) => {
                push_token(
                    source,
                    &mut raw,
                    keyword_span(source, &inst.span, inst.kind.name()),
                    TOKEN_TYPE,
                    0,
                );
                push_token(
                    source,
                    &mut raw,
                    instance_id_span(inst, source),
                    TOKEN_VARIABLE,
                    MOD_DEFINITION,
                );
                tokenize_quoted_strings(source, &mut raw, inst.span.clone());
                for column in &inst.columns {
                    push_column_name(source, &mut raw, column);
                    push_col_type(source, &mut raw, &column.col_type, column.span.clone());
                }
            }
            Item::Predicate(pred) => {
                tokenize_predicate(source, &mut raw, pred);
            }
            Item::Property(_) => {}
        }
    }

    raw.sort_by_key(|token| token.start);
    raw.dedup_by(|a, b| a.start == b.start && a.length == b.length);

    encode_tokens(source, raw)
}

fn tokenize_predicate(source: &str, raw: &mut Vec<RawToken>, pred: &PredicateCall) {
    push_token(
        source,
        raw,
        predicate_name_span(pred, source),
        TOKEN_FUNCTION,
        0,
    );
    for arg in &pred.args {
        tokenize_predicate_arg(source, raw, arg);
    }
    for chain in &pred.chain {
        push_token(
            source,
            raw,
            chain_name_span(chain, pred, source),
            TOKEN_FUNCTION,
            0,
        );
        for arg in &chain.args {
            tokenize_predicate_arg(source, raw, arg);
        }
    }
}

fn tokenize_predicate_arg(source: &str, raw: &mut Vec<RawToken>, arg: &PredicateArg) {
    match arg {
        PredicateArg::Ref(qref) => tokenize_qref(source, raw, qref),
        PredicateArg::Lit(_) => {}
        PredicateArg::Transition { .. } | PredicateArg::Card(_) => {}
        PredicateArg::Expr(expr) => tokenize_expr(source, raw, expr),
    }
}

fn tokenize_expr(source: &str, raw: &mut Vec<RawToken>, expr: &Expr) {
    match expr {
        Expr::Cmp(cmp) => {
            visit_operand(source, raw, &cmp.lhs);
            visit_operand(source, raw, &cmp.rhs);
        }
        Expr::Not(inner) => tokenize_expr(source, raw, inner),
        Expr::And(a, b) | Expr::Or(a, b) => {
            tokenize_expr(source, raw, a);
            tokenize_expr(source, raw, b);
        }
    }
}

fn visit_operand(source: &str, raw: &mut Vec<RawToken>, operand: &Operand) {
    if let Operand::QualifiedColumn(col) = operand {
        tokenize_qref(source, raw, &col.entity);
        push_token(
            source,
            raw,
            column_name_span(&col.column, col.span.clone(), source),
            TOKEN_PROPERTY,
            0,
        );
    }
}

fn tokenize_qref(source: &str, raw: &mut Vec<RawToken>, qref: &QRef) {
    if let Some(kind) = &qref.kind_qualifier {
        let slice = source.get(qref.span.clone()).unwrap_or_default();
        if let Some(pos) = slice.find("::") {
            let kind_end = qref.span.start + pos;
            push_token(source, raw, qref.span.start..kind_end, TOKEN_TYPE, 0);
            push_token(source, raw, kind_end + 2..qref.span.end, TOKEN_VARIABLE, 0);
            let _ = kind;
            return;
        }
    }
    push_token(source, raw, qref.span.clone(), TOKEN_VARIABLE, 0);
}

fn push_column_name(source: &str, raw: &mut Vec<RawToken>, column: &rdra_ish_syntax::ast::Column) {
    let slice = source.get(column.span.clone()).unwrap_or_default();
    if let Some(end) = slice.find(':') {
        push_token(
            source,
            raw,
            column.span.start..column.span.start + end,
            TOKEN_PROPERTY,
            0,
        );
    }
}

fn push_col_type(
    source: &str,
    raw: &mut Vec<RawToken>,
    col_type: &ColType,
    span: rdra_ish_syntax::ast::Span,
) {
    let name = match col_type {
        ColType::Int => "Int",
        ColType::String => "String",
        ColType::Money => "Money",
        ColType::DateTime => "DateTime",
        ColType::Date => "Date",
        ColType::Bool => "Bool",
        ColType::Decimal => "Decimal",
        ColType::Enum(_) => return,
    };
    if let Some(rel) = source.get(span.clone()).and_then(|slice| slice.find(name)) {
        let start = span.start + rel;
        push_token(source, raw, start..start + name.len(), TOKEN_TYPE, 0);
    }
}

fn tokenize_quoted_strings(source: &str, raw: &mut Vec<RawToken>, span: std::ops::Range<usize>) {
    let slice = source.get(span.clone()).unwrap_or_default();
    let mut in_string = false;
    let mut start = 0usize;
    for (index, ch) in slice.char_indices() {
        if ch == '"' {
            if in_string {
                let abs_start = span.start + start;
                let abs_end = span.start + index + 1;
                push_token(source, raw, abs_start..abs_end, TOKEN_STRING, 0);
                in_string = false;
            } else {
                start = index;
                in_string = true;
            }
        }
    }
}

fn predicate_name_span(pred: &PredicateCall, source: &str) -> std::ops::Range<usize> {
    let slice = source.get(pred.span.clone()).unwrap_or_default();
    if slice.starts_with(&pred.name) {
        pred.span.start..pred.span.start + pred.name.len()
    } else {
        pred.span.start..pred.span.end
    }
}

fn chain_name_span(
    chain: &ChainCall,
    pred: &PredicateCall,
    source: &str,
) -> std::ops::Range<usize> {
    let slice = source.get(pred.span.clone()).unwrap_or_default();
    let needle = format!(".{}(", chain.name);
    if let Some(rel) = slice.find(&needle) {
        let start = pred.span.start + rel + 1;
        start..start + chain.name.len()
    } else {
        pred.span.clone()
    }
}

fn column_name_span(
    column: &str,
    span: std::ops::Range<usize>,
    source: &str,
) -> std::ops::Range<usize> {
    let slice = source.get(span.clone()).unwrap_or_default();
    if let Some(rel) = slice.rfind(column) {
        let start = span.start + rel;
        start..start + column.len()
    } else {
        span
    }
}

fn keyword_span(
    source: &str,
    span: &std::ops::Range<usize>,
    keyword: &str,
) -> std::ops::Range<usize> {
    let slice = source.get(span.clone()).unwrap_or_default();
    if let Some(rel) = slice.find(keyword) {
        let start = span.start + rel;
        start..start + keyword.len()
    } else {
        span.clone()
    }
}

fn push_keyword(source: &str, raw: &mut Vec<RawToken>, span: std::ops::Range<usize>) {
    push_token(source, raw, span, TOKEN_KEYWORD, 0);
}

fn push_token(
    source: &str,
    raw: &mut Vec<RawToken>,
    span: std::ops::Range<usize>,
    token_type: u32,
    modifiers: u32,
) {
    if span.start >= span.end {
        return;
    }
    let Some(slice) = source.get(span.clone()) else {
        return;
    };
    let length = slice.encode_utf16().count() as u32;
    if length == 0 {
        return;
    }
    raw.push(RawToken {
        start: span.start,
        length,
        token_type,
        modifiers,
    });
}

fn encode_tokens(source: &str, raw: Vec<RawToken>) -> tower_lsp::lsp_types::SemanticTokens {
    use tower_lsp::lsp_types::SemanticToken;

    let mut data = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_char = 0u32;

    for token in raw {
        let pos = byte_offset_to_position(source, token.start);
        let delta_line = pos.line.saturating_sub(prev_line);
        let delta_start = if delta_line == 0 {
            pos.character.saturating_sub(prev_char)
        } else {
            pos.character
        };
        data.push(SemanticToken {
            delta_line,
            delta_start,
            length: token.length,
            token_type: token.token_type,
            token_modifiers_bitset: token.modifiers,
        });
        prev_line = pos.line;
        prev_char = pos.character;
    }

    tower_lsp::lsp_types::SemanticTokens {
        result_id: None,
        data,
    }
}

#[cfg(test)]
mod tests {
    use rdra_ish_syntax::parse;

    use super::*;

    #[test]
    fn emits_tokens_for_declarations_and_predicates() {
        let src = r#"usecase Book "Book"
actor Staff "Staff"
performs(Staff, Book)
"#;
        let (ast, errs) = parse(src);
        assert!(errs.is_empty());
        let tokens = semantic_tokens(&ast);
        assert!(tokens.data.len() >= 4);
    }
}
