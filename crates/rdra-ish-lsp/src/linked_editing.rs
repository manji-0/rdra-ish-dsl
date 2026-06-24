//! Linked editing ranges for synchronized symbol renames while typing.

use rdra_ish_core::SemanticModel;
use rdra_ish_syntax::ast::{Ast, Expr, Item, Operand, PredicateArg, PredicateCall};
use tower_lsp::lsp_types::{LinkedEditingRanges, Range};

use crate::convert::span_to_range;
use crate::refs::{
    id_span_in_qref, instance_id_span, reference_at_offset, symbol_target, SymbolTarget,
};

pub fn linked_editing_ranges(
    model: &SemanticModel,
    ast: &Ast,
    text: &str,
    offset: usize,
) -> Option<LinkedEditingRanges> {
    let reference = reference_at_offset(ast, offset)?;
    let target = symbol_target(model, reference)?;

    let active = active_id_range(ast, text, &target, offset)?;
    let mut ranges = vec![active];

    for item in &ast.items {
        match item {
            Item::Instance(inst) => {
                if inst.kind.name() == target.kind && inst.id == target.id {
                    let range = span_to_range(text, instance_id_span(inst, text));
                    push_unique_range(&mut ranges, range);
                }
            }
            Item::Predicate(pred) => {
                collect_linked_ranges(pred, text, &target, &mut ranges);
            }
            Item::Import(_) | Item::Module(_, _) => {}
        }
    }

    if ranges.len() < 2 {
        return None;
    }

    Some(LinkedEditingRanges {
        ranges,
        word_pattern: None,
    })
}

fn active_id_range(ast: &Ast, text: &str, target: &SymbolTarget, offset: usize) -> Option<Range> {
    let reference = reference_at_offset(ast, offset)?;
    match reference {
        crate::refs::ReferenceAt::Declaration { kind, id } => {
            for item in &ast.items {
                if let Item::Instance(inst) = item {
                    if inst.kind.name() == kind && inst.id == id {
                        return Some(span_to_range(text, instance_id_span(inst, text)));
                    }
                }
            }
            None
        }
        crate::refs::ReferenceAt::Symbol(qref) => {
            id_span_in_qref(qref, text, target).map(|span| span_to_range(text, span))
        }
    }
}

fn collect_linked_ranges(
    pred: &PredicateCall,
    text: &str,
    target: &SymbolTarget,
    ranges: &mut Vec<Range>,
) {
    for arg in &pred.args {
        collect_linked_ranges_in_arg(arg, text, target, ranges);
    }
    for chain in &pred.chain {
        for arg in &chain.args {
            collect_linked_ranges_in_arg(arg, text, target, ranges);
        }
    }
}

fn collect_linked_ranges_in_arg(
    arg: &PredicateArg,
    text: &str,
    target: &SymbolTarget,
    ranges: &mut Vec<Range>,
) {
    match arg {
        PredicateArg::Ref(qref) => {
            if let Some(span) = id_span_in_qref(qref, text, target) {
                push_unique_range(ranges, span_to_range(text, span));
            }
        }
        PredicateArg::Expr(expr) => {
            let Expr::Cmp(cmp) = expr;
            if let Operand::QualifiedColumn(col) = &cmp.lhs {
                if let Some(span) = id_span_in_qref(&col.entity, text, target) {
                    push_unique_range(ranges, span_to_range(text, span));
                }
            }
            if let Operand::QualifiedColumn(col) = &cmp.rhs {
                if let Some(span) = id_span_in_qref(&col.entity, text, target) {
                    push_unique_range(ranges, span_to_range(text, span));
                }
            }
        }
        PredicateArg::Tuple(args) => {
            for inner in args {
                collect_linked_ranges_in_arg(inner, text, target, ranges);
            }
        }
        PredicateArg::Lit(_) => {}
    }
}

fn push_unique_range(ranges: &mut Vec<Range>, range: Range) {
    if !ranges.iter().any(|existing| existing == &range) {
        ranges.push(range);
    }
}

#[cfg(test)]
mod tests {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    use super::*;

    #[test]
    fn links_declaration_and_predicate_references() {
        let src = r#"usecase Book "Book"
actor Staff "Staff"
performs(Staff, Book)
"#;
        let (ast, errs) = parse(src);
        assert!(errs.is_empty());
        let (model, _) = build_model(&ast);
        let offset = src.find("Book").unwrap();
        let linked = linked_editing_ranges(&model, &ast, src, offset).expect("linked ranges");
        assert!(linked.ranges.len() >= 2);
    }
}
