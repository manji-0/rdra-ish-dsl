//! Folding ranges for instance metadata and column blocks.

use rdra_ish_syntax::ast::{Ast, Item};
use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};

use crate::convert::byte_offset_to_position;

pub fn folding_ranges(ast: &Ast, text: &str) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();

    for item in &ast.items {
        let Item::Instance(inst) = item else {
            continue;
        };

        if !inst.columns.is_empty() {
            if let Some(range) = brace_block_range(text, &inst.span) {
                ranges.push(range);
            }
        } else if let Some(range) = metadata_block_range(text, &inst.span) {
            ranges.push(range);
        }
    }

    ranges
}

fn brace_block_range(text: &str, span: &std::ops::Range<usize>) -> Option<FoldingRange> {
    let slice = text.get(span.clone())?;
    let open = slice.find('{')?;
    let close = slice.rfind('}')?;
    let start = byte_offset_to_position(text, span.start + open).line;
    let end = byte_offset_to_position(text, span.start + close).line;
    Some(FoldingRange {
        start_line: start,
        end_line: end,
        start_character: None,
        end_character: None,
        kind: Some(FoldingRangeKind::Region),
        collapsed_text: None,
    })
}

fn metadata_block_range(text: &str, span: &std::ops::Range<usize>) -> Option<FoldingRange> {
    let slice = text.get(span.clone())?;
    if slice.contains('{') {
        return None;
    }
    let first_line_end = slice.find('\n')?;
    let mut line_start = span.start + first_line_end + 1;
    if line_start >= span.end {
        return None;
    }

    let mut first_meta_line = None;
    let mut last_meta_line_end = line_start;

    while line_start < span.end {
        let line_end = text[line_start..span.end]
            .find('\n')
            .map(|index| line_start + index)
            .unwrap_or(span.end);
        let line = &text[line_start..line_end];
        if line.starts_with("  ") {
            if first_meta_line.is_none() {
                first_meta_line = Some(byte_offset_to_position(text, line_start).line);
            }
            last_meta_line_end = line_end;
        } else if !line.trim().is_empty() {
            break;
        }
        line_start = if line_end < span.end {
            line_end + 1
        } else {
            span.end
        };
    }

    let start_line = first_meta_line?;
    let end_line = byte_offset_to_position(text, last_meta_line_end.saturating_sub(1)).line;
    if end_line <= start_line {
        return None;
    }

    Some(FoldingRange {
        start_line,
        end_line,
        start_character: None,
        end_character: None,
        kind: Some(FoldingRangeKind::Region),
        collapsed_text: None,
    })
}

#[cfg(test)]
mod tests {
    use rdra_ish_syntax::parse;

    use super::*;

    #[test]
    fn folds_column_block() {
        let src = r#"dto OrderRequest "Order request" {
  customer_id: Int
  note: String
}
"#;
        let (ast, errs) = parse(src);
        assert!(errs.is_empty());
        let ranges = folding_ranges(&ast, src);
        assert!(ranges
            .iter()
            .any(|range| range.start_line == 0 && range.end_line == 3));
    }

    #[test]
    fn folds_metadata_block() {
        let src = r#"api CreateOrder "Create order"
  method POST
  path "/orders"
"#;
        let (ast, errs) = parse(src);
        assert!(errs.is_empty());
        let ranges = folding_ranges(&ast, src);
        assert!(ranges.iter().any(|range| range.start_line == 1));
    }
}
