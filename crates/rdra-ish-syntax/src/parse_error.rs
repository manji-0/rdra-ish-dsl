use std::fmt;

use chumsky::error::{Simple, SimpleReason};

use crate::token::Token;

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Token::Module => "module",
            Token::Import => "import",
            Token::As => "as",
            Token::Actor => "actor",
            Token::ExtSystem => "extsystem",
            Token::System => "system",
            Token::Requirement => "requirement",
            Token::Adr => "adr",
            Token::Nfr => "nfr",
            Token::Quality => "quality",
            Token::Constraint => "constraint",
            Token::Concept => "concept",
            Token::DomainObject => "domain_object",
            Token::Aggregate => "aggregate",
            Token::ValueObject => "valueobject",
            Token::Business => "business",
            Token::Buc => "buc",
            Token::Flow => "flow",
            Token::Step => "step",
            Token::UsageScene => "usagescene",
            Token::UseCase => "usecase",
            Token::Screen => "screen",
            Token::Field => "field",
            Token::Event => "event",
            Token::Entity => "entity",
            Token::State => "state",
            Token::Condition => "condition",
            Token::Variation => "variation",
            Token::Api => "api",
            Token::Dto => "dto",
            Token::Location => "location",
            Token::Timing => "timing",
            Token::Medium => "medium",
            Token::Permission => "permission",
            Token::Property => "property",
            Token::And => "and",
            Token::Or => "or",
            Token::Not => "not",
            Token::TInt => "Int",
            Token::TString => "String",
            Token::TMoney => "Money",
            Token::TDateTime => "DateTime",
            Token::TDate => "Date",
            Token::TBool => "Bool",
            Token::TDecimal => "Decimal",
            Token::TEnum => "Enum",
            Token::AtPk => "@pk",
            Token::AtUnique => "@unique",
            Token::AtIndex => "@index",
            Token::AtCheck => "@check",
            Token::AtNull => "@null",
            Token::AtDefault => "@default",
            Token::AtLabel => "@label",
            Token::AtSoftDelete => "@soft_delete",
            Token::AtHistory => "@history",
            Token::AtTenant => "@tenant",
            Token::AtDerived => "@derived",
            Token::Now => "now",
            Token::LBrace => "{",
            Token::RBrace => "}",
            Token::LParen => "(",
            Token::RParen => ")",
            Token::Comma => ",",
            Token::ColonColon => "::",
            Token::Colon => ":",
            Token::Dot => ".",
            Token::Arrow => "->",
            Token::Le => "<=",
            Token::Ge => ">=",
            Token::EqEq => "==",
            Token::Ne => "!=",
            Token::Lt => "<",
            Token::Gt => ">",
            Token::TlaOr => "\\/",
            Token::TlaAnd => "/\\",
            Token::TlaNot => "~",
            Token::Ident(s) => return write!(f, "{s}"),
            Token::StringLit(s) => return write!(f, "\"{s}\""),
            Token::IntLit(s) => return write!(f, "{s}"),
        };
        f.write_str(s)
    }
}

fn describe_expected(tok: &Option<Token>) -> String {
    match tok {
        None => "end of input".into(),
        Some(t) => format!("`{t}`"),
    }
}

fn describe_found(tok: Option<&Token>) -> String {
    match tok {
        None => "end of input".into(),
        Some(t) => format!("`{t}`"),
    }
}

/// Cap expected-set size so messages stay readable when many alternatives apply.
fn format_expected_set<'a>(expected: impl Iterator<Item = &'a Option<Token>>) -> String {
    let mut items: Vec<String> = expected.map(describe_expected).collect();
    items.sort();
    items.dedup();
    const MAX: usize = 8;
    if items.is_empty() {
        "something else".into()
    } else if items.len() <= MAX {
        items.join(", ")
    } else {
        let shown = items[..MAX].join(", ");
        format!("{shown}, … ({} more)", items.len() - MAX)
    }
}

/// Human-readable message for a chumsky parse error.
pub fn format_parse_error(err: &Simple<Token>) -> String {
    match err.reason() {
        SimpleReason::Custom(msg) => msg.clone(),
        SimpleReason::Unclosed { delimiter, .. } => {
            format!("unclosed `{delimiter}`; expected a matching closer")
        }
        SimpleReason::Unexpected => {
            let found = describe_found(err.found());
            let expected = format_expected_set(err.expected());
            format!("unexpected {found}; expected {expected}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn truncated_entity_body_is_human_readable() {
        let src = r#"module t
entity X "X" { id: Int @pk
"#;
        let (_ast, errs) = parse(src);
        assert!(!errs.is_empty());
        let msg = format_parse_error(&errs[0]);
        assert!(!msg.contains("Simple {"), "must not dump Debug: {msg}");
        assert!(
            msg.contains("unexpected") || msg.contains("expected"),
            "msg={msg}"
        );
        assert!(
            msg.contains('}') || msg.contains("`@") || msg.contains("end of input"),
            "msg={msg}"
        );
    }
}
