//! Inlay hints for predicate parameters and symbol kinds.

use rdra_ish_core::{node_ref_kind, LookupResult, SemanticModel};
use rdra_ish_syntax::ast::{
    Ast, ChainCall, Expr, Item, Operand, PredicateArg, PredicateCall, QRef,
};
use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Range};

use crate::convert::byte_offset_to_position;
use crate::predicates::predicate_signature_parameters;

pub fn inlay_hints(
    model: &SemanticModel,
    ast: &Ast,
    text: &str,
    range: Option<Range>,
) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    for item in &ast.items {
        match item {
            Item::Predicate(pred) => {
                hints.extend(predicate_parameter_hints(text, pred));
                collect_reference_hints(model, text, pred, &mut hints);
            }
            Item::Instance(_) | Item::Import(_) | Item::Module(_, _) => {}
        }
    }

    if let Some(range) = range {
        hints.retain(|hint| hint_in_range(hint, &range));
    }

    hints
}

fn predicate_parameter_hints(source: &str, pred: &PredicateCall) -> Vec<InlayHint> {
    let Some(params) = predicate_signature_parameters(&pred.name) else {
        return Vec::new();
    };
    let positions = arg_delimiter_positions(source, pred);
    let mut hints = Vec::new();

    for (index, offset) in positions.into_iter().enumerate() {
        let Some(label) = params.get(index) else {
            break;
        };
        if label.is_empty() {
            continue;
        }
        hints.push(InlayHint {
            position: byte_offset_to_position(source, offset),
            label: InlayHintLabel::String(format!("{label}: ")),
            kind: Some(InlayHintKind::PARAMETER),
            text_edits: None,
            tooltip: None,
            padding_left: Some(false),
            padding_right: Some(false),
            data: None,
        });
    }

    hints
}

fn arg_delimiter_positions(source: &str, pred: &PredicateCall) -> Vec<usize> {
    let Some(slice) = source.get(pred.span.clone()) else {
        return Vec::new();
    };
    let Some(paren_rel) = slice.find('(') else {
        return Vec::new();
    };
    let mut positions = vec![pred.span.start + paren_rel + 1];
    for (index, ch) in slice.char_indices() {
        if ch == ',' {
            positions.push(pred.span.start + index + 1);
        }
    }
    positions
}

fn collect_reference_hints(
    model: &SemanticModel,
    text: &str,
    pred: &PredicateCall,
    hints: &mut Vec<InlayHint>,
) {
    for arg in &pred.args {
        collect_reference_hints_in_arg(model, text, arg, hints);
    }
    for chain in &pred.chain {
        collect_reference_hints_in_chain(model, text, chain, hints);
    }
}

fn collect_reference_hints_in_chain(
    model: &SemanticModel,
    text: &str,
    chain: &ChainCall,
    hints: &mut Vec<InlayHint>,
) {
    for arg in &chain.args {
        collect_reference_hints_in_arg(model, text, arg, hints);
    }
}

fn collect_reference_hints_in_arg(
    model: &SemanticModel,
    text: &str,
    arg: &PredicateArg,
    hints: &mut Vec<InlayHint>,
) {
    match arg {
        PredicateArg::Ref(qref) => {
            if let Some(hint) = reference_kind_hint(model, text, qref) {
                hints.push(hint);
            }
        }
        PredicateArg::Expr(expr) => {
            let Expr::Cmp(cmp) = expr;
            if let Operand::QualifiedColumn(col) = &cmp.lhs {
                if let Some(hint) = reference_kind_hint(model, text, &col.entity) {
                    hints.push(hint);
                }
            }
            if let Operand::QualifiedColumn(col) = &cmp.rhs {
                if let Some(hint) = reference_kind_hint(model, text, &col.entity) {
                    hints.push(hint);
                }
            }
        }
        PredicateArg::Tuple(args) => {
            for inner in args {
                collect_reference_hints_in_arg(model, text, inner, hints);
            }
        }
        PredicateArg::Lit(_) => {}
    }
}

fn reference_kind_hint(model: &SemanticModel, text: &str, qref: &QRef) -> Option<InlayHint> {
    if qref.kind_qualifier.is_some() {
        return None;
    }
    let id = qref.parts.last()?;
    let node = match model.symbols.lookup(id) {
        LookupResult::Found(node) => node,
        _ => return None,
    };
    let kind = node_ref_kind(node);
    Some(InlayHint {
        position: byte_offset_to_position(text, qref.span.end),
        label: InlayHintLabel::String(kind.to_string()),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: None,
        padding_left: Some(true),
        padding_right: Some(false),
        data: None,
    })
}

fn hint_in_range(hint: &InlayHint, range: &Range) -> bool {
    let hint_range = Range {
        start: hint.position,
        end: hint.position,
    };
    ranges_overlap(&hint_range, range)
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
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    use super::*;

    #[test]
    fn shows_predicate_parameter_and_reference_hints() {
        let src = r#"usecase Book "Book"
actor Staff "Staff"
performs(Staff, Book)
"#;
        let (ast, errs) = parse(src);
        assert!(errs.is_empty());
        let (model, _) = build_model(&ast);
        let hints = inlay_hints(&model, &ast, src, None);
        assert!(hints.iter().any(
            |hint| matches!(hint.label, InlayHintLabel::String(ref s) if s.contains("actor"))
        ));
        assert!(hints
            .iter()
            .any(|hint| matches!(hint.label, InlayHintLabel::String(ref s) if s == "usecase")));
    }
}
