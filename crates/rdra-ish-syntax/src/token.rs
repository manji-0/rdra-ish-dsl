use logos::Logos;

/// All tokens for the RDRA DSL.
#[derive(Logos, Debug, Clone, PartialEq, Eq, Hash)]
#[logos(skip r"[ \t\r\n]+")] // whitespace
#[logos(skip r"//[^\n]*")] // line comments
pub enum Token {
    // ── Keywords ────────────────────────────────────────────────────
    #[token("module")]
    Module,
    #[token("import")]
    Import,
    #[token("as")]
    As,

    #[token("actor")]
    Actor,
    #[token("extsystem")]
    ExtSystem,
    #[token("requirement")]
    Requirement,
    #[token("business")]
    Business,
    #[token("buc")]
    Buc,
    #[token("usagescene")]
    UsageScene,
    #[token("usecase")]
    UseCase,
    #[token("screen")]
    Screen,
    #[token("event")]
    Event,
    #[token("entity")]
    Entity,
    #[token("state")]
    State,
    #[token("condition")]
    Condition,
    #[token("variation")]
    Variation,

    // ── Type keywords ────────────────────────────────────────────────
    #[token("Int")]
    TInt,
    #[token("String")]
    TString,
    #[token("Money")]
    TMoney,
    #[token("DateTime")]
    TDateTime,
    #[token("Date")]
    TDate,
    #[token("Bool")]
    TBool,
    #[token("Decimal")]
    TDecimal,
    #[token("Enum")]
    TEnum,

    // ── Annotations ─────────────────────────────────────────────────
    #[token("@pk")]
    AtPk,
    #[token("@unique")]
    AtUnique,
    #[token("@null")]
    AtNull,
    #[token("@default")]
    AtDefault,
    #[token("@label")]
    AtLabel,

    // ── Punctuation ──────────────────────────────────────────────────
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token(",")]
    Comma,
    // "::" must come before ":" so the longer token wins
    #[token("::")]
    ColonColon,
    #[token(":")]
    Colon,
    #[token(".")]
    Dot,

    // ── Literals ─────────────────────────────────────────────────────
    /// Identifiers: `[A-Za-z_][A-Za-z0-9_]*`
    #[regex(r"[A-Za-z_][A-Za-z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    /// String literals (may contain UTF-8 / Japanese). Quotes stripped.
    #[regex(r#""[^"]*""#, |lex| {
        let s = lex.slice();
        s[1..s.len()-1].to_string()
    })]
    StringLit(String),

    /// Integer literals: `[0-9]+`
    #[regex(r"[0-9]+", |lex| lex.slice().to_string())]
    IntLit(String),
}
