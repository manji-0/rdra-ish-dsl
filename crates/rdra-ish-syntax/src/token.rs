use logos::Logos;

/// All tokens for the RDRA DSL.
#[derive(Logos, Debug, Clone, PartialEq, Eq, Hash)]
#[logos(skip r"[ \t\r\n]+")] // whitespace
pub enum Token {
    /// A `//` or `/* */` comment. `parser::lex` moves it out of the token stream into
    /// `Ast::comments`, so the grammar never sees it and the formatter can re-emit it.
    #[regex(r"//[^\n]*")]
    #[regex(r"/\*([^*]|\*[^/])*\*/")]
    Comment,

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
    #[token("system")]
    System,
    #[token("requirement")]
    Requirement,
    #[token("adr")]
    Adr,
    #[token("nfr")]
    Nfr,
    #[token("quality")]
    Quality,
    #[token("constraint")]
    Constraint,
    #[token("business")]
    Business,
    #[token("buc")]
    Buc,
    #[token("flow")]
    Flow,
    #[token("step")]
    Step,
    #[token("usecase")]
    UseCase,
    #[token("screen")]
    Screen,
    #[token("field")]
    Field,
    #[token("event")]
    Event,
    #[token("entity")]
    Entity,
    #[token("api")]
    Api,
    #[token("dto")]
    Dto,
    #[token("timer")]
    Timer,
    #[token("medium")]
    Medium,
    #[token("permission")]
    Permission,
    #[token("variation")]
    Variation,
    #[token("condition")]
    Condition,
    #[token("property")]
    Property,

    // English logical connectives (temporal properties); also accepted as TLA glyphs below.
    #[token("and")]
    And,
    #[token("or")]
    Or,
    #[token("not")]
    Not,

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
    #[token("@index")]
    AtIndex,
    #[token("@check")]
    AtCheck,
    #[token("@null")]
    AtNull,
    #[token("@default")]
    AtDefault,
    #[token("@label")]
    AtLabel,
    #[token("@soft_delete")]
    AtSoftDelete,
    #[token("@history")]
    AtHistory,
    #[token("@tenant")]
    AtTenant,
    #[token("@derived")]
    AtDerived,

    // ── Comparison / temporal ────────────────────────────────────────
    /// Built-in temporal reference for comparison expressions (e.g. `expired_at < now`)
    #[token("now")]
    Now,

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
    // ".." must come before "." so the longer token wins
    #[token("..")]
    DotDot,
    #[token(".")]
    Dot,
    /// The open upper bound of a cardinality range (`1..*`), and multiplication.
    #[token("*")]
    Star,
    #[token("+")]
    Plus,
    // logos takes the longest match, so "->" never lexes as "-" then ">".
    #[token("-")]
    Minus,
    #[token("->")]
    Arrow,
    // Comparison operators: longer tokens must come before shorter ones
    // so that "<=" is lexed before "<", etc.
    #[token("<=")]
    Le,
    #[token(">=")]
    Ge,
    #[token("==")]
    EqEq,
    #[token("!=")]
    Ne,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    // TLA-style logical connectives (aliases for and/or/not).
    #[token("\\/")]
    TlaOr,
    #[token("/\\")]
    TlaAnd,
    #[token("~")]
    TlaNot,

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
