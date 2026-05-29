use chumsky::prelude::*;
use chumsky::Stream;
use logos::Logos;

use crate::ast::*;
use crate::token::Token;

// ── Lexer bridge ─────────────────────────────────────────────────────────────

/// Run the logos lexer and return a `Vec<(Token, Span)>`.
/// Tokens that fail to lex are silently dropped.
pub fn lex(src: &str) -> Vec<Spanned<Token>> {
    Token::lexer(src)
        .spanned()
        .filter_map(|(tok, span)| tok.ok().map(|t| (t, span)))
        .collect()
}

// ── Parser helpers ────────────────────────────────────────────────────────────

/// Match a single `Token::Ident` and return the inner string.
fn ident() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    select! { Token::Ident(s) => s }
}

/// Match a single `Token::StringLit` and return the inner string (quotes stripped).
fn string_lit() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    select! { Token::StringLit(s) => s }
}

// ── Dotted name ───────────────────────────────────────────────────────────────

/// `foo.bar.baz`
fn dotted_name() -> impl Parser<Token, DottedName, Error = Simple<Token>> + Clone {
    ident()
        .then(just(Token::Dot).ignore_then(ident()).repeated())
        .map(|(head, tail)| {
            let mut parts = vec![head];
            parts.extend(tail);
            DottedName(parts)
        })
}

// ── Import ────────────────────────────────────────────────────────────────────

/// `Customer` or `Customer as C`
fn select_item() -> impl Parser<Token, SelectItem, Error = Simple<Token>> + Clone {
    ident()
        .map_with_span(|name, span| (name, span))
        .then(just(Token::As).ignore_then(ident()).or_not())
        .map_with_span(|((name, _name_span), alias), span| SelectItem { name, alias, span })
}

/// Parse one `import` declaration.
/// Grammar:
///   import <dotted_name>
///   import <dotted_name> as <ident>
///   import <dotted_name>.{<select_item> (, <select_item>)*}
///
/// The path is always the ident-dot sequence *before* any `as` or `.{`.
/// Because dotted_name greedily consumes all ident.ident segments, a selective
/// import like `shared.actors.{Customer}` needs the path built without the final
/// dot, and the brace suffix is matched after. We handle this by matching the
/// path directly as ident segments and then dispatching on the suffix.
fn import_decl() -> impl Parser<Token, ImportDecl, Error = Simple<Token>> + Clone {
    // One ident segment (not inside braces).
    let path_segment = ident();

    // dotted sequence: head (. segment)*
    let path = path_segment
        .clone()
        .then(
            just(Token::Dot)
                .ignore_then(path_segment.clone())
                .repeated(),
        )
        .map(|(head, tail): (String, Vec<String>)| {
            let mut parts = vec![head];
            parts.extend(tail);
            DottedName(parts)
        });

    // `.{item, ...}` suffix
    let select_suffix = just(Token::Dot)
        .ignore_then(just(Token::LBrace))
        .ignore_then(
            select_item()
                .separated_by(just(Token::Comma))
                .allow_trailing(),
        )
        .then_ignore(just(Token::RBrace));

    // `as <ident>` suffix
    let alias_suffix = just(Token::As).ignore_then(ident());

    // suffix: SelectItems | Alias | nothing
    let suffix = select_suffix
        .map(|items| ImportKind::Select(items))
        .or(alias_suffix.map(ImportKind::Alias))
        .or_not()
        .map(|opt| opt.unwrap_or(ImportKind::All));

    just(Token::Import)
        .ignore_then(path.then(suffix))
        .map_with_span(|(path, kind), span| ImportDecl { path, kind, span })
}

// ── Column annotations ────────────────────────────────────────────────────────

/// An ident or string-lit used as an annotation argument value.
fn ann_value() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    ident().or(string_lit())
}

fn annotation() -> impl Parser<Token, Annotation, Error = Simple<Token>> + Clone {
    // @pk  or  @pk(a, b)
    let at_pk = just(Token::AtPk)
        .ignore_then(
            just(Token::LParen)
                .ignore_then(ident().separated_by(just(Token::Comma)).allow_trailing())
                .then_ignore(just(Token::RParen))
                .or_not(),
        )
        .map(|args| match args {
            None => Annotation::Pk,
            Some(v) if v.is_empty() => Annotation::Pk,
            Some(v) => Annotation::PkComposite(v),
        });

    // @unique
    let at_unique = just(Token::AtUnique).to(Annotation::Unique);

    // @null
    let at_null = just(Token::AtNull).to(Annotation::Null);

    // @default(value)
    let at_default = just(Token::AtDefault)
        .ignore_then(just(Token::LParen))
        .ignore_then(ann_value())
        .then_ignore(just(Token::RParen))
        .map(Annotation::Default);

    // @label("...")
    let at_label = just(Token::AtLabel)
        .ignore_then(just(Token::LParen))
        .ignore_then(string_lit())
        .then_ignore(just(Token::RParen))
        .map(Annotation::Label);

    choice((at_pk, at_unique, at_null, at_default, at_label))
}

// ── Column type ───────────────────────────────────────────────────────────────

fn col_type() -> impl Parser<Token, ColType, Error = Simple<Token>> + Clone {
    let simple = select! {
        Token::TInt      => ColType::Int,
        Token::TString   => ColType::String,
        Token::TMoney    => ColType::Money,
        Token::TDateTime => ColType::DateTime,
        Token::TDate     => ColType::Date,
        Token::TBool     => ColType::Bool,
        Token::TDecimal  => ColType::Decimal,
    };

    // Enum(active, discontinued)
    let enum_ty = just(Token::TEnum)
        .ignore_then(just(Token::LParen))
        .ignore_then(ident().separated_by(just(Token::Comma)).allow_trailing())
        .then_ignore(just(Token::RParen))
        .map(ColType::Enum);

    enum_ty.or(simple)
}

// ── Column definition ─────────────────────────────────────────────────────────

/// `name: Type @ann1 @ann2 ...`
fn column() -> impl Parser<Token, Column, Error = Simple<Token>> + Clone {
    ident()
        .then_ignore(just(Token::Colon))
        .then(col_type())
        .then(annotation().repeated())
        .map_with_span(|((name, col_type), annotations), span| Column {
            name,
            col_type,
            annotations,
            span,
        })
}

// ── Instance declaration ──────────────────────────────────────────────────────

fn kind_token() -> impl Parser<Token, Kind, Error = Simple<Token>> + Clone {
    select! {
        Token::Actor       => Kind::Actor,
        Token::ExtSystem   => Kind::ExtSystem,
        Token::Requirement => Kind::Requirement,
        Token::Business    => Kind::Business,
        Token::Buc         => Kind::Buc,
        Token::UsageScene  => Kind::UsageScene,
        Token::UseCase     => Kind::UseCase,
        Token::Screen      => Kind::Screen,
        Token::Event       => Kind::Event,
        Token::Entity      => Kind::Entity,
        Token::State       => Kind::State,
        Token::Condition   => Kind::Condition,
        Token::Variation   => Kind::Variation,
    }
}

fn instance_decl() -> impl Parser<Token, InstanceDecl, Error = Simple<Token>> + Clone {
    let body = just(Token::LBrace)
        .ignore_then(column().repeated())
        .then_ignore(just(Token::RBrace));

    kind_token()
        .then(ident())
        .then(string_lit())
        .then(body.or_not())
        .map_with_span(|(((kind, id), label), columns), span| InstanceDecl {
            kind,
            id,
            label,
            columns: columns.unwrap_or_default(),
            span,
        })
}

// ── Qualified reference ───────────────────────────────────────────────────────

/// Parse a reference to a declared element.
///
/// Two forms are accepted:
///   `usecase::Browse`       — kind-qualified (resolves unambiguously when
///                             the same identifier is used for multiple kinds)
///   `Foo` or `a.Foo`        — plain or namespace-qualified (existing syntax)
fn qref() -> impl Parser<Token, QRef, Error = Simple<Token>> + Clone {
    // Typed form: `<kind_keyword> :: <ident>`
    let typed = kind_token()
        .then_ignore(just(Token::ColonColon))
        .then(ident())
        .map_with_span(|(kind, name), span| QRef {
            kind_qualifier: Some(kind),
            parts: vec![name],
            span,
        });

    // Plain form: `ident ("." ident)*`
    let plain = ident()
        .then(just(Token::Dot).ignore_then(ident()).repeated())
        .map_with_span(|(head, tail), span| {
            let mut parts = vec![head];
            parts.extend(tail);
            QRef {
                kind_qualifier: None,
                parts,
                span,
            }
        });

    typed.or(plain)
}

// ── Predicate call ────────────────────────────────────────────────────────────

/// Argument: either a string literal or a qualified ref.
fn predicate_arg() -> impl Parser<Token, PredicateArg, Error = Simple<Token>> + Clone {
    let lit = string_lit().map(PredicateArg::Lit);
    let r = qref().map(PredicateArg::Ref);
    lit.or(r)
}

fn predicate_call() -> impl Parser<Token, PredicateCall, Error = Simple<Token>> + Clone {
    ident()
        .then_ignore(just(Token::LParen))
        .then(
            predicate_arg()
                .separated_by(just(Token::Comma))
                .allow_trailing(),
        )
        .then_ignore(just(Token::RParen))
        .map_with_span(|(name, args), span| PredicateCall { name, args, span })
}

// ── Module declaration ────────────────────────────────────────────────────────

fn module_decl() -> impl Parser<Token, Item, Error = Simple<Token>> + Clone {
    just(Token::Module)
        .ignore_then(dotted_name())
        .map_with_span(|name, span| Item::Module(name, span))
}

// ── Top-level item ────────────────────────────────────────────────────────────

fn item() -> impl Parser<Token, Item, Error = Simple<Token>> + Clone {
    let import = import_decl().map(Item::Import);
    let instance = instance_decl().map(Item::Instance);
    let predicate = predicate_call().map(Item::Predicate);

    choice((module_decl(), import, instance, predicate))
}

// ── Root parser ───────────────────────────────────────────────────────────────

fn root_parser() -> impl Parser<Token, Vec<Item>, Error = Simple<Token>> {
    item()
        .recover_with(skip_then_retry_until([]))
        .repeated()
        .then_ignore(end())
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Lex and parse `src`. Returns the best-effort AST and any parse errors.
pub fn parse(src: &str) -> (Ast, Vec<Simple<Token>>) {
    let tokens = lex(src);
    let len = src.len();

    let stream = Stream::from_iter(len..len + 1, tokens.into_iter());
    let (items, errors) = root_parser().parse_recovery(stream);

    let ast = Ast {
        items: items.unwrap_or_default(),
        source: src.to_string(),
    };
    (ast, errors)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(src: &str) -> Ast {
        let (ast, errors) = parse(src);
        if !errors.is_empty() {
            panic!("parse errors: {:?}", errors);
        }
        ast
    }

    #[test]
    fn test_parse_instance_decl() {
        let ast = parse_ok(r#"actor Customer "顧客""#);
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn test_parse_entity() {
        let src = r#"
entity Product "商品" {
  id:  Int    @pk
  sku: String @unique
  name: String @label("商品名")
  price: Decimal
  status: Enum(active, discontinued) @default(active)
  note: String @null
}
"#;
        let ast = parse_ok(src);
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn test_parse_predicate() {
        let ast = parse_ok("performs(Customer, Browse)");
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn test_parse_relate() {
        let ast = parse_ok(r#"relate(Order, Customer, "N:1")"#);
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn test_parse_import() {
        let src = r#"
import shared.actors
import shared.entities as e
import shared.actors.{Customer, Staff}
import shared.actors.{Customer as C}
"#;
        let ast = parse_ok(src);
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn test_parse_module() {
        let ast = parse_ok("module shared.actors");
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn test_parse_full_snippet() {
        let src = r#"
// コメント
module shared.actors

import shared.entities

actor   Customer "顧客"
usecase Browse   "商品を探す"

entity  Order "注文" {
  id: Int @pk
  total: Money
  ordered_at: DateTime
}

performs(Customer, Browse)
relate(Order, Customer, "N:1")
"#;
        let ast = parse_ok(src);
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn test_parse_typed_qref() {
        let src = r#"
actor    Add "追加"
usecase  Add "追加する"
performs(actor::Add, usecase::Add)
"#;
        let ast = parse_ok(src);
        // Verify the predicate args carry kind qualifiers.
        let pred = ast
            .items
            .iter()
            .find_map(|i| {
                if let Item::Predicate(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("predicate not found");
        assert_eq!(pred.name, "performs");
        if let PredicateArg::Ref(qref) = &pred.args[0] {
            assert_eq!(qref.kind_qualifier, Some(Kind::Actor));
            assert_eq!(qref.parts, vec!["Add"]);
        } else {
            panic!("expected Ref arg");
        }
        if let PredicateArg::Ref(qref) = &pred.args[1] {
            assert_eq!(qref.kind_qualifier, Some(Kind::UseCase));
            assert_eq!(qref.parts, vec!["Add"]);
        } else {
            panic!("expected Ref arg");
        }
    }
}
