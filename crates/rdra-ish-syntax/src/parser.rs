// chumsky の select! マクロが生成する Simple<Token> クロージャは型サイズが大きく
// result_large_err が出るが、サードパーティ型のため制御不能。
#![allow(clippy::result_large_err)]

use chumsky::prelude::*;
use chumsky::Stream;
use logos::Logos;

use crate::ast::*;
use crate::metadata::{metadata_clause, metadata_value, Metadata};
use crate::removed::{legacy_predicate, migration_hint, removed_when};
use crate::token::Token;

// ── Lexer bridge ─────────────────────────────────────────────────────────────

/// The output of [`lex`]: the tokens the grammar reads, the comments it skips, and the
/// source spans of any characters that did not match a token (fail-closed: callers must
/// treat these as errors).
pub struct Lexed {
    pub tokens: Vec<Spanned<Token>>,
    pub comments: Vec<Comment>,
    pub errors: Vec<Span>,
}

/// Run the logos lexer.
pub fn lex(src: &str) -> Lexed {
    let mut lexed = Lexed {
        tokens: Vec::new(),
        comments: Vec::new(),
        errors: Vec::new(),
    };
    for (tok, span) in Token::lexer(src).spanned() {
        match tok {
            Ok(Token::Comment) => {
                let trailing = lexed
                    .tokens
                    .last()
                    .is_some_and(|(_, code)| !src[code.end..span.start].contains('\n'));
                lexed.comments.push(Comment { span, trailing });
            }
            Ok(t) => lexed.tokens.push((t, span)),
            Err(()) => lexed.errors.push(span),
        }
    }
    lexed
}

// ── Parser helpers ────────────────────────────────────────────────────────────

/// Match a single `Token::Ident` and return the inner string.
fn ident() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    select! { Token::Ident(s) => s }
}

/// Match a single `Token::StringLit` and return the inner string (quotes stripped).
pub(crate) fn string_lit() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    select! { Token::StringLit(s) => s }
}

/// Match a token that can be used as a module/import path segment.
///
/// Path segments are namespace labels, so keyword-like names such as
/// `buc.purchase` must remain valid even though `buc` is a declaration keyword
/// elsewhere in the grammar.
pub(crate) fn path_ident() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    select! {
        Token::Ident(s) => s,
        Token::Actor => "actor".to_string(),
        Token::ExtSystem => "extsystem".to_string(),
        Token::System => "system".to_string(),
        Token::Requirement => "requirement".to_string(),
        Token::Adr => "adr".to_string(),
        Token::Goal => "goal".to_string(),
        Token::Nfr => "nfr".to_string(),
        Token::Quality => "quality".to_string(),
        Token::Constraint => "constraint".to_string(),
        Token::Business => "business".to_string(),
        Token::Buc => "buc".to_string(),
        Token::Flow => "flow".to_string(),
        Token::Step => "step".to_string(),
        Token::UseCase => "usecase".to_string(),
        Token::Screen => "screen".to_string(),
        Token::Field => "field".to_string(),
        Token::Event => "event".to_string(),
        Token::Entity => "entity".to_string(),
        Token::Api => "api".to_string(),
        Token::Dto => "dto".to_string(),
        Token::Timer => "timer".to_string(),
        Token::Medium => "medium".to_string(),
        Token::Permission => "permission".to_string(),
        Token::Variation => "variation".to_string(),
        Token::Condition => "condition".to_string(),
        Token::Property => "property".to_string(),
    }
}

// ── Dotted name ───────────────────────────────────────────────────────────────

/// `foo.bar.baz`
fn dotted_name() -> impl Parser<Token, DottedName, Error = Simple<Token>> + Clone {
    path_ident()
        .then(just(Token::Dot).ignore_then(path_ident()).repeated())
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
    let path_segment = path_ident();

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
        .map(ImportKind::Select)
        .or(alias_suffix.map(ImportKind::Alias))
        .or_not()
        .map(|opt| opt.unwrap_or(ImportKind::All));

    just(Token::Import)
        .ignore_then(path.then(suffix))
        .map_with_span(|(path, kind), span| ImportDecl { path, kind, span })
}

// ── Column annotations ────────────────────────────────────────────────────────

/// An ident, string-lit, or int-lit used as an annotation argument value.
fn ann_value() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    let int_lit = select! { Token::IntLit(s) => s };
    ident().or(string_lit()).or(int_lit)
}

fn annotation() -> impl Parser<Token, Annotation, Error = Simple<Token>> + Clone {
    let annotation_args = || {
        just(Token::LParen)
            .ignore_then(ident().separated_by(just(Token::Comma)).allow_trailing())
            .then_ignore(just(Token::RParen))
            .or_not()
    };

    // @pk  or  @pk(a, b)
    let at_pk = just(Token::AtPk)
        .ignore_then(annotation_args())
        .map(|args| match args {
            None => Annotation::Pk,
            Some(v) if v.is_empty() => Annotation::Pk,
            Some(v) => Annotation::PkComposite(v),
        });

    // @unique  or  @unique(a, b)
    let at_unique = just(Token::AtUnique)
        .ignore_then(annotation_args())
        .map(|args| match args {
            None => Annotation::Unique,
            Some(v) if v.is_empty() => Annotation::Unique,
            Some(v) => Annotation::UniqueComposite(v),
        });

    // @index  or  @index(a, b)
    let at_index = just(Token::AtIndex)
        .ignore_then(annotation_args())
        .map(|args| match args {
            None => Annotation::Index,
            Some(v) if v.is_empty() => Annotation::Index,
            Some(v) => Annotation::IndexComposite(v),
        });

    let at_check = expr_annotation(Token::AtCheck, "@check", Annotation::Check);

    // @null
    let at_null = just(Token::AtNull).to(Annotation::Null);

    // @default(value)
    let at_default = just(Token::AtDefault)
        .ignore_then(just(Token::LParen))
        .ignore_then(
            string_lit()
                .map(|value| (value, true))
                .or(ann_value().map(|value| (value, false))),
        )
        .then_ignore(just(Token::RParen))
        .map(|(value, quoted)| Annotation::Default { value, quoted });

    // @label("...")
    let at_label = just(Token::AtLabel)
        .ignore_then(just(Token::LParen))
        .ignore_then(string_lit())
        .then_ignore(just(Token::RParen))
        .map(Annotation::Label);

    let at_soft_delete = just(Token::AtSoftDelete).to(Annotation::SoftDelete);
    let at_history = just(Token::AtHistory).to(Annotation::History);
    let at_tenant = just(Token::AtTenant).to(Annotation::Tenant);
    let at_derived = expr_annotation(Token::AtDerived, "@derived", Annotation::Derived);

    choice((
        at_pk,
        at_unique,
        at_index,
        at_check,
        at_null,
        at_default,
        at_label,
        at_soft_delete,
        at_history,
        at_tenant,
        at_derived,
    ))
}

/// `@check(expr)` / `@derived(expr)`. The quoted form of earlier versions is an error whose
/// message is the unquoted rewrite.
fn expr_annotation(
    token: Token,
    name: &'static str,
    wrap: fn(ColumnExpr) -> Annotation,
) -> impl Parser<Token, Annotation, Error = Simple<Token>> + Clone {
    just(token)
        .ignore_then(just(Token::LParen))
        .ignore_then(string_lit().map(Err).or(column_expr().map(Ok)))
        .then_ignore(just(Token::RParen))
        .try_map(move |expr, span| match expr {
            Ok(expr) => Ok(wrap(expr)),
            Err(quoted) => Err(Simple::custom(
                span,
                format!("`{name}` takes an unquoted expression; write `{name}({quoted})`"),
            )),
        })
}

/// `operand (+|-|*) operand` over a column or an integer, or a logical expression.
fn column_expr() -> impl Parser<Token, ColumnExpr, Error = Simple<Token>> + Clone {
    let arith_operand =
        select! { Token::IntLit(s) => Operand::IntLit(s) }.or(column_name().map(Operand::Column));
    let arith_op = select! {
        Token::Plus => ArithOp::Add,
        Token::Minus => ArithOp::Sub,
        Token::Star => ArithOp::Mul,
    };
    let arith = arith_operand
        .clone()
        .then(arith_op)
        .then(arith_operand)
        .map_with_span(|((lhs, op), rhs), span| ColumnExpr::Arith(Arith { lhs, op, rhs, span }));
    arith.or(logical_expr().map(ColumnExpr::Logical))
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

    // Enum(active "販売中", discontinued) — at least one variant required, labels optional
    let enum_value = path_ident()
        .then(string_lit().or_not())
        .map(|(value, label)| EnumValue { value, label });
    let enum_ty = just(Token::TEnum)
        .ignore_then(just(Token::LParen))
        .ignore_then(enum_value.separated_by(just(Token::Comma)).allow_trailing())
        .then_ignore(just(Token::RParen))
        .try_map(|variants: Vec<EnumValue>, span| {
            if variants.is_empty() {
                Err(Simple::custom(span, "Enum() requires at least one variant"))
            } else {
                Ok(ColType::Enum(variants))
            }
        });

    let named = ident().map(ColType::Named);

    enum_ty.or(simple).or(named)
}

// ── Column definition ─────────────────────────────────────────────────────────

/// A column name or value: an identifier, or any kind keyword.
///
/// Columns and values never share a position with a declaration, so a word such as
/// `goal` or `field` is unambiguous where a column or a value is expected.
fn column_name() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    path_ident()
}

/// `cash "Cash"` or `adjustment` (label optional)
fn variation_value() -> impl Parser<Token, VariationValue, Error = Simple<Token>> + Clone {
    path_ident()
        .then(string_lit().or_not())
        .map_with_span(|(value, label), span| VariationValue { value, label, span })
}

/// `name: Type @ann1 @ann2 ...`
fn column() -> impl Parser<Token, Column, Error = Simple<Token>> + Clone {
    column_name()
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
        Token::System      => Kind::System,
        Token::Requirement => Kind::Requirement,
        Token::Adr         => Kind::Adr,
        Token::Goal        => Kind::Goal,
        Token::Nfr         => Kind::Nfr,
        Token::Quality     => Kind::Quality,
        Token::Constraint  => Kind::Constraint,
        Token::Business    => Kind::Business,
        Token::Buc         => Kind::Buc,
        Token::Flow        => Kind::Flow,
        Token::Step        => Kind::Step,
        Token::UseCase     => Kind::UseCase,
        Token::Screen      => Kind::Screen,
        Token::Field       => Kind::Field,
        Token::Event       => Kind::Event,
        Token::Entity      => Kind::Entity,
        Token::Api         => Kind::Api,
        Token::Dto         => Kind::Dto,
        Token::Timer      => Kind::Timer,
        Token::Medium      => Kind::Medium,
        Token::Permission  => Kind::Permission,
        Token::Variation   => Kind::Variation,
        Token::Condition   => Kind::Condition,
    }
}

fn instance_decl() -> impl Parser<Token, InstanceDecl, Error = Simple<Token>> + Clone {
    let body = just(Token::LBrace).ignore_then(
        column()
            .repeated()
            .map(DeclBody::Columns)
            .then_ignore(just(Token::RBrace))
            .or(variation_value()
                .repeated()
                .at_least(1)
                .map(DeclBody::Values)
                .then_ignore(just(Token::RBrace))),
    );

    let description = just(Token::Ident("description".to_string()))
        .ignore_then(metadata_value())
        .or_not();

    kind_token()
        .then(ident())
        .then(string_lit())
        .then(description)
        .then(metadata_clause().repeated())
        .then(body.or_not())
        .try_map(
            |(((((kind, id), label), description), clauses), body), span| {
                let (columns, variation_values) = match body {
                    None => (Vec::new(), Vec::new()),
                    Some(DeclBody::Columns(columns)) => (columns, Vec::new()),
                    Some(DeclBody::Values(values)) => (Vec::new(), values),
                };
                if kind == Kind::Variation {
                    check_variation_values(&variation_values, &span)?;
                } else if !variation_values.is_empty() {
                    return Err(Simple::custom(
                        span,
                        format!(
                            "only `variation` declarations list values; `{}` takes columns",
                            kind.name()
                        ),
                    ));
                }
                if kind == Kind::Variation && !columns.is_empty() {
                    return Err(Simple::custom(
                        span,
                        "a `variation` lists values (`cash \"Cash\"`), not columns",
                    ));
                }
                let mut metadata = Metadata::collect(&kind, clauses);
                check_goal_metadata(&kind, &metadata.nfr, &span)?;
                if let Some(description) = &description {
                    if description.bare {
                        metadata
                            .bare_values
                            .push(("description".to_string(), description.text.clone()));
                    }
                }
                Ok(InstanceDecl {
                    kind,
                    id,
                    label,
                    description: description.map(|d| d.text),
                    requirement: metadata.requirement,
                    adr: metadata.adr,
                    api: metadata.api,
                    nfr: metadata.nfr,
                    field: metadata.field,
                    usecase: metadata.usecase,
                    bare_values: metadata.bare_values,
                    columns,
                    variation_values,
                    span,
                })
            },
        )
}

fn check_goal_metadata(kind: &Kind, meta: &NfrMetadata, span: &Span) -> Result<(), Simple<Token>> {
    if *kind == Kind::Goal {
        let extra = meta.slo.is_some()
            || meta.availability.is_some()
            || meta.resilience.is_some()
            || meta.audit.is_some()
            || meta.logging.is_some()
            || meta.retention.is_some()
            || meta.privacy.is_some();
        if extra {
            return Err(Simple::custom(
                span.clone(),
                "a `goal` takes only `metric`, `baseline`, `target` and `window`",
            ));
        }
    } else if meta.baseline.is_some() {
        return Err(Simple::custom(
            span.clone(),
            format!("`baseline` is only for `goal`, not `{}`", kind.name()),
        ));
    }
    Ok(())
}

fn check_variation_values(
    values: &[VariationValue],
    decl_span: &Span,
) -> Result<(), Simple<Token>> {
    if values.is_empty() {
        return Err(Simple::custom(
            decl_span.clone(),
            "a `variation` needs a body listing at least one value",
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for value in values {
        if !seen.insert(value.value.as_str()) {
            return Err(Simple::custom(
                value.span.clone(),
                format!("duplicate variation value `{}`", value.value),
            ));
        }
    }
    Ok(())
}

enum DeclBody {
    Columns(Vec<Column>),
    Values(Vec<VariationValue>),
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

/// リテラルまたは修飾参照。
fn predicate_atom() -> impl Parser<Token, PredicateArg, Error = Simple<Token>> + Clone {
    let lit = string_lit().map(PredicateArg::Lit);
    let r = qref().map(PredicateArg::Ref);
    lit.or(r)
}

/// `N:1` / `1:N` / `1:1` / `N:M` カーディナリティ（非クォート）。A side may be a range
/// `lo..hi` (`1..*:1`); the analysis checks its bounds and which side carries it.
fn card_arg() -> impl Parser<Token, PredicateArg, Error = Simple<Token>> + Clone {
    let int = select! { Token::IntLit(s) => s };
    let range = int
        .then_ignore(just(Token::DotDot))
        .then(int.or(just(Token::Star).to("*".to_string())))
        .map(|(lo, hi)| format!("{lo}..{hi}"));
    let simple = select! {
        Token::Ident(s) if s == "N" || s == "M" => s,
        Token::IntLit(s) if s == "1" => s,
    };
    let side = range.or(simple);
    side.clone()
        .then_ignore(just(Token::Colon))
        .then(side)
        .map(|(left, right)| PredicateArg::Card(format!("{left}:{right}")))
}

/// `pending -> paid` 遷移端点, or `(pending, held) -> paid`: one arm per listed source.
fn transition_arg() -> impl Parser<Token, PredicateArg, Error = Simple<Token>> + Clone {
    let one = ident().map(|from| vec![from]);
    let many = ident()
        .separated_by(just(Token::Comma))
        .at_least(1)
        .delimited_by(just(Token::LParen), just(Token::RParen));
    one.or(many)
        .then_ignore(just(Token::Arrow))
        .then(ident())
        .map(|(from, to)| PredicateArg::Transition { from, to })
}

/// 比較式の被演算子（Operand）:
/// 裸ident（カラム参照）、`Entity.column`、整数リテラル、または `now`。
fn operand() -> impl Parser<Token, Operand, Error = Simple<Token>> + Clone {
    let now = just(Token::Now).map(|_| Operand::Now);
    let int_lit = select! { Token::IntLit(s) => Operand::IntLit(s) };
    let qualified_col = ident()
        .map_with_span(|entity, span| {
            (
                entity.clone(),
                QRef {
                    kind_qualifier: None,
                    parts: vec![entity],
                    span,
                },
            )
        })
        .then_ignore(just(Token::Dot))
        .then(column_name())
        .map_with_span(|((_entity_name, entity), column), span| {
            Operand::QualifiedColumn(QualifiedColumnRef {
                entity,
                column,
                span,
            })
        });
    let col = column_name().map(Operand::Column);
    // `now` must come before generic ident because logos lexes it as Token::Now
    now.or(int_lit).or(qualified_col).or(col)
}

/// 比較演算子トークン → `CmpOp`
fn cmp_op() -> impl Parser<Token, CmpOp, Error = Simple<Token>> + Clone {
    select! {
        Token::Le  => CmpOp::Le,
        Token::Ge  => CmpOp::Ge,
        Token::EqEq => CmpOp::Eq,
        Token::Ne  => CmpOp::Ne,
        Token::Lt  => CmpOp::Lt,
        Token::Gt  => CmpOp::Gt,
    }
}

/// The span of each `in ()` / `not in ()`, which names no value.
fn empty_sets(items: &[Item]) -> Vec<Span> {
    fn expr(e: &Expr, out: &mut Vec<Span>) {
        match e {
            Expr::Cmp(cmp) => {
                if matches!(&cmp.rhs, Operand::Values(values) if values.is_empty()) {
                    out.push(cmp.span.clone());
                }
            }
            Expr::Not(inner) => expr(inner, out),
            Expr::And(a, b) | Expr::Or(a, b) => {
                expr(a, out);
                expr(b, out);
            }
        }
    }
    let mut out = Vec::new();
    for item in items {
        match item {
            Item::Predicate(call) => {
                let chained = call.chain.iter().flat_map(|c| c.args.iter());
                for arg in call.args.iter().chain(chained) {
                    if let PredicateArg::Expr(e) = arg {
                        expr(e, &mut out);
                    }
                }
            }
            Item::Property(prop) => match &prop.formula {
                AstTemporalFormula::Always(e) | AstTemporalFormula::Eventually(e) => {
                    expr(e, &mut out)
                }
                AstTemporalFormula::LeadsTo {
                    antecedent,
                    consequent,
                } => {
                    expr(antecedent, &mut out);
                    expr(consequent, &mut out);
                }
            },
            Item::Instance(inst) => {
                for ann in inst.columns.iter().flat_map(|c| c.annotations.iter()) {
                    if let Annotation::Check(ColumnExpr::Logical(e))
                    | Annotation::Derived(ColumnExpr::Logical(e)) = ann
                    {
                        expr(e, &mut out);
                    }
                }
            }
            Item::Module(..) | Item::Import(_) => {}
        }
    }
    out
}

/// `in (a, b)` / `not in (a, b)` after a comparison's left operand. `in` is no keyword: it is
/// read as one only here, so a column may still be named `in`.
fn membership() -> impl Parser<Token, (CmpOp, Operand), Error = Simple<Token>> + Clone {
    let in_word = select! { Token::Ident(s) if s == "in" => () };
    let value = select! {
        Token::Ident(s) => s,
        Token::IntLit(s) => s,
    };
    just(Token::Not)
        .or_not()
        .then_ignore(in_word)
        .then(
            // `in ()` parses, so that `parse` reports it by name (`empty_sets`).
            value
                .separated_by(just(Token::Comma))
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .map(|(not, values)| {
            let op = if not.is_some() {
                CmpOp::NotIn
            } else {
                CmpOp::In
            };
            (op, Operand::Values(values))
        })
}

/// 比較式: `operand cmp_op operand`（例: `stock < selling`, `expired_at < now`）, or
/// `operand in (v, ...)` / `operand not in (v, ...)`.
fn comparison() -> impl Parser<Token, Expr, Error = Simple<Token>> + Clone {
    operand()
        .then(cmp_op().then(operand()).or(membership()))
        .map_with_span(|(lhs, (op, rhs)), span| Expr::Cmp(Comparison { lhs, op, rhs, span }))
}

/// Logical expression for temporal properties: `and`/`or`/`not` (TLA `/\` `\/` `~` are aliases).
fn logical_expr() -> impl Parser<Token, Expr, Error = Simple<Token>> + Clone {
    recursive(|expr| {
        let atom = comparison().or(just(Token::LParen)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::RParen)));

        let not_op = just(Token::Not).or(just(Token::TlaNot));
        let not_expr = not_op
            .ignore_then(atom.clone())
            .map(|inner| Expr::Not(Box::new(inner)))
            .or(atom);

        let and_op = just(Token::And).or(just(Token::TlaAnd));
        let and_expr = not_expr
            .clone()
            .then(and_op.ignore_then(not_expr.clone()).repeated())
            .map(|(first, rest)| {
                rest.into_iter()
                    .fold(first, |acc, next| Expr::And(Box::new(acc), Box::new(next)))
            });

        let or_op = just(Token::Or).or(just(Token::TlaOr));
        and_expr
            .clone()
            .then(or_op.ignore_then(and_expr.clone()).repeated())
            .map(|(first, rest)| {
                rest.into_iter()
                    .fold(first, |acc, next| Expr::Or(Box::new(acc), Box::new(next)))
            })
    })
}

fn property_decl() -> impl Parser<Token, PropertyDecl, Error = Simple<Token>> + Clone {
    let always = just(Token::Ident("always".to_string()))
        .ignore_then(just(Token::LParen))
        .ignore_then(logical_expr())
        .then_ignore(just(Token::RParen))
        .map(AstTemporalFormula::Always);

    let eventually = just(Token::Ident("eventually".to_string()))
        .ignore_then(just(Token::LParen))
        .ignore_then(logical_expr())
        .then_ignore(just(Token::RParen))
        .map(AstTemporalFormula::Eventually);

    let leads_to = just(Token::Ident("leads_to".to_string()))
        .ignore_then(just(Token::LParen))
        .ignore_then(logical_expr())
        .then_ignore(just(Token::Comma))
        .then(logical_expr())
        .then_ignore(just(Token::RParen))
        .map(|(antecedent, consequent)| AstTemporalFormula::LeadsTo {
            antecedent,
            consequent,
        });

    let formula = choice((leads_to, always, eventually));

    just(Token::Property)
        .ignore_then(ident())
        .then(string_lit().or_not())
        .then(formula)
        .then(chain_call().repeated())
        .map_with_span(|(((id, label), formula), chain), span| PropertyDecl {
            id,
            label,
            formula,
            chain,
            span,
        })
}

/// 引数: 比較式、遷移端点、カーディナリティ、文字列リテラル、または修飾参照。
///
/// 比較式を **最優先** でパースし、続けて遷移 / カード / atom にフォールバックする。
/// - `forbidden(E, status == cancelled)` → `PredicateArg::Expr`
/// - `transitions(UseCase, E.col, a -> b)` の第3引数 → `PredicateArg::Transition`
/// - `relate(A, B, N:1)` の第3引数 → `PredicateArg::Card`
/// - `performs(A, B)` → `PredicateArg::Ref`
fn predicate_arg() -> impl Parser<Token, PredicateArg, Error = Simple<Token>> + Clone {
    let expr = comparison().map(PredicateArg::Expr);
    expr.or(transition_arg())
        .or(card_arg())
        .or(predicate_atom())
}

fn kind_word_arg() -> impl Parser<Token, PredicateArg, Error = Simple<Token>> + Clone {
    select! {
        Token::Actor => "actor".to_string(),
        Token::System => "system".to_string(),
    }
    .map_with_span(|word, span| {
        PredicateArg::Ref(QRef {
            kind_qualifier: None,
            parts: vec![word],
            span,
        })
    })
}

/// `.method(args...)` のチェーン呼び出し1件。`.caller(...)` alone also takes the kind keywords
/// `actor` and `system` as bare words.
fn chain_call() -> impl Parser<Token, ChainCall, Error = Simple<Token>> + Clone {
    just(Token::Dot)
        .ignore_then(ident())
        .then_ignore(just(Token::LParen))
        .then(
            predicate_arg()
                .map(|arg| (arg, false))
                .or(kind_word_arg().map(|arg| (arg, true)))
                .separated_by(just(Token::Comma))
                .allow_trailing(),
        )
        .then_ignore(just(Token::RParen))
        .try_map(|(name, args), span| {
            if name != "caller" && args.iter().any(|(_, bare)| *bare) {
                return Err(Simple::custom(
                    span,
                    "`actor` and `system` are bare words only in `.caller(...)`",
                ));
            }
            let args = args.into_iter().map(|(arg, _)| arg).collect();
            Ok(ChainCall { name, args, span })
        })
}

fn predicate_call() -> impl Parser<Token, PredicateCall, Error = Simple<Token>> + Clone {
    select! { Token::Ident(s) if migration_hint(&s).is_none() => s }
        .then_ignore(just(Token::LParen))
        .then(
            predicate_arg()
                .separated_by(just(Token::Comma))
                .allow_trailing(),
        )
        .then_ignore(just(Token::RParen))
        .then(chain_call().repeated())
        .map_with_span(|((name, args), chain), span| PredicateCall {
            name,
            args,
            chain,
            span,
        })
}

// ── Module declaration ────────────────────────────────────────────────────────

fn module_decl() -> impl Parser<Token, Item, Error = Simple<Token>> + Clone {
    just(Token::Module)
        .ignore_then(dotted_name())
        .map_with_span(Item::Module)
}

// ── Top-level item ────────────────────────────────────────────────────────────

fn item() -> impl Parser<Token, Item, Error = Simple<Token>> + Clone {
    let import = import_decl().map(Item::Import);
    let instance = instance_decl().map(Item::Instance);
    let property = property_decl().map(Item::Property);
    let predicate = predicate_call().map(Item::Predicate);
    let removed =
        select! { Token::Ident(s) if migration_hint(&s).is_some() => s }.try_map(|word, span| {
            Err::<Item, _>(Simple::custom(
                span,
                migration_hint(&word).unwrap_or_default(),
            ))
        });

    // property before instance so `property` keyword is not consumed as Ident.
    choice((
        module_decl(),
        import,
        property,
        instance,
        removed,
        predicate,
    ))
}

// ── Root parser ───────────────────────────────────────────────────────────────

fn root_parser() -> impl Parser<Token, Vec<Item>, Error = Simple<Token>> {
    item()
        .recover_with(skip_then_retry_until([]))
        .repeated()
        .then_ignore(end())
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Lex and parse `src`. Returns the best-effort AST and any lex/parse errors. A deprecated
/// form is rewritten in the AST without an error; [`parse_with_deprecations`] also returns
/// its warning.
pub fn parse(src: &str) -> (Ast, Vec<Simple<Token>>) {
    let parsed = parse_with_deprecations(src);
    (parsed.ast, parsed.errors)
}

/// The result of [`parse_with_deprecations`].
#[derive(Debug)]
pub struct Parsed {
    pub ast: Ast,
    pub errors: Vec<Simple<Token>>,
    /// One warning per deprecated form, already rewritten to its current form in `ast`.
    pub deprecations: Vec<Deprecation>,
}

/// A deprecated form the parser rewrote: where it was written and the warning naming the
/// rewrite and the version it is removed in.
#[derive(Debug, Clone, PartialEq)]
pub struct Deprecation {
    pub span: Span,
    pub message: String,
}

/// Lex and parse `src`, returning the warnings for deprecated forms beside the errors.
pub fn parse_with_deprecations(src: &str) -> Parsed {
    let Lexed {
        tokens,
        comments,
        errors: lex_errors,
    } = lex(src);
    let len = src.len();

    let stream = Stream::from_iter(len..len + 1, tokens.into_iter());
    let (items, mut errors) = root_parser().parse_recovery(stream);
    let mut items = items.unwrap_or_default();
    items.retain(|item| match item {
        Item::Predicate(call) => match removed_when(call, src) {
            Some(msg) => {
                errors.push(Simple::custom(call.span.clone(), msg));
                false
            }
            None => true,
        },
        _ => true,
    });

    let mut deprecations = Vec::new();
    for item in &mut items {
        let Item::Predicate(call) = item else {
            continue;
        };
        let Some(legacy) = legacy_predicate(call) else {
            continue;
        };
        let span = call.span.clone();
        *call = legacy.rewritten;
        if legacy.deprecated {
            deprecations.push(Deprecation {
                span,
                message: legacy.message,
            });
        } else {
            errors.push(Simple::custom(span, legacy.message));
        }
    }

    for span in empty_sets(&items) {
        errors.push(Simple::custom(
            span,
            "`in` / `not in` takes at least one value: write `col in (a, b)`",
        ));
    }

    for span in lex_errors {
        let snippet = src.get(span.clone()).unwrap_or("");
        errors.push(Simple::custom(
            span,
            format!("unexpected character(s) `{snippet}`"),
        ));
    }

    let ast = Ast {
        items,
        comments,
        source: src.to_string(),
    };
    Parsed {
        ast,
        errors,
        deprecations,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
