//! Linked editing ranges for synchronized symbol renames while typing.

use rdra_ish_core::SemanticModel;
use rdra_ish_syntax::ast::{Ast, Item};
use tower_lsp::lsp_types::{LinkedEditingRanges, Range};

use crate::convert::span_to_range;
use crate::refs::{
    id_span_in_qref, instance_id_span, reference_at_offset, references_to, symbol_target,
    SymbolTarget,
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
        if let Item::Instance(inst) = item {
            if inst.kind.name() == target.kind && inst.id == target.id {
                let range = span_to_range(text, instance_id_span(inst, text));
                push_unique_range(&mut ranges, range);
            }
        }
    }
    for qref in references_to(ast, &target) {
        if let Some(span) = id_span_in_qref(qref, text, &target) {
            push_unique_range(&mut ranges, span_to_range(text, span));
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
