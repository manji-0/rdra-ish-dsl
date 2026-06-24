//! Byte span ↔ LSP position/range conversion.

use rdra_ish_syntax::ast::Span;
use tower_lsp::lsp_types::{Position, Range};

pub fn span_to_range(source: &str, span: Span) -> Range {
    Range {
        start: byte_offset_to_position(source, span.start),
        end: byte_offset_to_position(source, span.end),
    }
}

pub fn byte_offset_to_position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() as u32;
    let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let character = source[line_start..offset].encode_utf16().count() as u32;
    Position { line, character }
}

pub fn position_to_byte_offset(source: &str, position: Position) -> Option<usize> {
    let mut line = 0u32;
    let mut line_start = 0usize;
    for (idx, ch) in source.char_indices() {
        if line == position.line {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = idx + ch.len_utf8();
        }
    }
    if line != position.line {
        return None;
    }

    let mut utf16_col = 0u32;
    let mut byte_offset = line_start;
    for ch in source[line_start..].chars() {
        if ch == '\n' {
            break;
        }
        if utf16_col == position.character {
            return Some(byte_offset);
        }
        utf16_col += ch.encode_utf16(&mut [0u16; 2]).len() as u32;
        byte_offset += ch.len_utf8();
    }
    if utf16_col == position.character {
        return Some(byte_offset);
    }
    None
}

pub fn full_document_range(text: &str) -> Range {
    let line_count = text.lines().count().max(1);
    let last_line = text.lines().last().unwrap_or("");
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: (line_count - 1) as u32,
            character: last_line.encode_utf16().count() as u32,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ascii_position() {
        let src = "actor A \"a\"\nusecase B \"b\"";
        let offset = src.find("usecase").unwrap();
        let pos = byte_offset_to_position(src, offset);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
        assert_eq!(position_to_byte_offset(src, pos), Some(offset));
    }
}
