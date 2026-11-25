//! Predicate call context and signature formatting.

use rdra_ish_core::predicate::Predicate;

pub fn format_predicate_signature(pred: &str) -> Option<String> {
    let args = signature_arguments(pred, "|")?;
    Some(format!("{pred}({})", args.join(", ")))
}

pub fn predicate_signature_parameters(pred: &str) -> Option<Vec<String>> {
    signature_arguments(pred, " | ")
}

/// The declaration kinds each argument of `pred` takes, joined by `separator`.
fn signature_arguments(pred: &str, separator: &str) -> Option<Vec<String>> {
    let sig = Predicate::parse(pred)?.signature();
    Some(
        sig.iter()
            .map(|kinds| {
                kinds
                    .iter()
                    .filter_map(|kind| kind.node())
                    .collect::<Vec<_>>()
                    .join(separator)
            })
            .collect(),
    )
}

/// Returns `(predicate_name, active_argument_index)` when the cursor is inside a call.
pub fn predicate_arg_context(source: &str, offset: usize) -> Option<(String, usize)> {
    let offset = offset.min(source.len());
    let before = &source[..offset];
    let mut depth = 0usize;
    let mut paren_start = None;
    for (index, ch) in before.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    paren_start = Some(index);
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    let paren_start = paren_start?;
    let prefix = before[..paren_start].trim_end();
    let pred_name = trailing_word(prefix, |ch| {
        ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'
    })
    .trim_start_matches('.')
    .to_string();
    if pred_name.is_empty() || !is_ident_segment(&pred_name) {
        return None;
    }
    let arg_index = before[paren_start + 1..]
        .chars()
        .filter(|ch| *ch == ',')
        .count();
    Some((pred_name, arg_index))
}

/// Returns predicate name and optional active argument index for signature help.
pub fn predicate_call_context(source: &str, offset: usize) -> Option<(String, Option<usize>)> {
    if let Some((name, arg_index)) = predicate_arg_context(source, offset) {
        return Some((name, Some(arg_index)));
    }

    let offset = offset.min(source.len());
    let line_start = source[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let line = &source[line_start..];
    let col = offset - line_start;
    let line_end = line.find('\n').unwrap_or(line.len());
    let line = &line[..line_end];

    for (pred, _) in rdra_ish_core::KNOWN_PREDICATES
        .iter()
        .map(|name| (*name, name.len()))
    {
        if let Some(rel) = line.find(pred) {
            let start = rel;
            let end = rel + pred.len();
            if col >= start && col <= end {
                let after = line.get(end..).unwrap_or_default();
                if after.starts_with('(')
                    || after.is_empty()
                    || after.starts_with(char::is_whitespace)
                {
                    return Some((pred.to_string(), None));
                }
            }
        }
    }
    None
}

pub fn predicate_arg_kinds(pred_name: &str, arg_index: usize) -> Option<Vec<&'static str>> {
    let sig = Predicate::parse(pred_name)?.signature();
    let kinds = sig.get(arg_index)?;
    let symbol_kinds: Vec<&'static str> = kinds.iter().filter_map(|kind| kind.node()).collect();
    if symbol_kinds.is_empty() {
        None
    } else {
        Some(symbol_kinds)
    }
}

/// The longest suffix of `text` whose characters all satisfy `in_word`. It starts after the
/// last character that does not, at that character's end, so it never splits a multi-byte
/// character.
pub fn trailing_word(text: &str, in_word: impl Fn(char) -> bool) -> &str {
    let start = text
        .char_indices()
        .rev()
        .find(|&(_, ch)| !in_word(ch))
        .map_or(0, |(index, ch)| index + ch.len_utf8());
    &text[start..]
}

fn is_ident_segment(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_predicate_signature() {
        assert_eq!(
            format_predicate_signature("performs"),
            Some("performs(actor, usecase|buc|step)".to_string())
        );
    }

    #[test]
    fn detects_argument_index() {
        let src = "performs(Staff, Book)";
        let offset = src.find("Book").unwrap();
        let (name, index) = predicate_arg_context(src, offset).unwrap();
        assert_eq!(name, "performs");
        assert_eq!(index, 1);
    }

    #[test]
    fn a_paren_after_a_multibyte_character_is_not_a_call() {
        for src in [
            "actor A \"受付(窓口)\"",
            "usecase U \"顧客(法人)\"",
            "performs(A, U) // 受付(",
        ] {
            for (offset, _) in src.char_indices().chain([(src.len(), ' ')]) {
                let _ = predicate_call_context(src, offset);
            }
            let after_paren = src.rfind('(').unwrap() + 1;
            assert_eq!(predicate_arg_context(src, after_paren), None, "{src}");
        }
    }

    #[test]
    fn finds_a_call_after_multibyte_text() {
        let src = "// 受付\nperforms(受付, ";
        assert_eq!(
            predicate_arg_context(src, src.len()),
            Some(("performs".to_string(), 1))
        );
    }

    #[test]
    fn trailing_word_starts_at_a_character_boundary() {
        let ident = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
        assert_eq!(trailing_word("\"受付", ident), "");
        assert_eq!(trailing_word("受付abc", ident), "abc");
        assert_eq!(trailing_word("𠮷x_1", ident), "x_1");
        assert_eq!(trailing_word("abc", ident), "abc");
        assert_eq!(trailing_word("", ident), "");
    }
}
