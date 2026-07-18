use logos::Logos;

/// All tokens for the RDRA DSL.
#[derive(Logos, Debug, Clone, PartialEq, Eq, Hash)]
#[logos(skip r"[ \t\r\n]+")] // whitespace
#[logos(skip r"//[^\n]*")] // line comments
#[logos(skip r"/\*([^*]|\*[^/])*\*/")] // block comments
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
    #[token("concept")]
    Concept,
    #[token("domain_object")]
    DomainObject,
    #[token("aggregate")]
    Aggregate,
    #[token("valueobject")]
    ValueObject,
    #[token("business")]
    Business,
    #[token("buc")]
    Buc,
    #[token("flow")]
    Flow,
    #[token("step")]
    Step,
    #[token("usagescene")]
    UsageScene,
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
    #[token("state")]
    State,
    #[token("condition")]
    Condition,
    #[token("variation")]
    Variation,
    #[token("api")]
    Api,
    #[token("dto")]
    Dto,
    #[token("location")]
    Location,
    #[token("timing")]
    Timing,
    #[token("medium")]
    Medium,
    #[token("permission")]
    Permission,
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
    #[token(".")]
    Dot,
    // "->" must come before bare "-" if it is ever added
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
