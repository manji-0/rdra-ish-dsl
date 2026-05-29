use std::ops::Range;

pub type Span = Range<usize>;
pub type Spanned<T> = (T, Span);

// ── Dotted name (e.g. "shared.actors") ──────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct DottedName(pub Vec<String>);

// ── Import ───────────────────────────────────────────────────────────────────

/// One item in a selective import: `Customer` or `Customer as C`
#[derive(Debug, Clone, PartialEq)]
pub struct SelectItem {
    pub name: String,
    pub alias: Option<String>,
    pub span: Span,
}

/// How an import is scoped.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportKind {
    /// `import shared.actors`
    All,
    /// `import shared.actors as a`
    Alias(String),
    /// `import shared.actors.{Customer, Staff as S}`
    Select(Vec<SelectItem>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub path: DottedName,
    pub kind: ImportKind,
    pub span: Span,
}

// ── RDRA element kinds ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Actor,
    ExtSystem,
    Requirement,
    Business,
    Buc,
    UsageScene,
    UseCase,
    Screen,
    Event,
    Entity,
    State,
    Condition,
    Variation,
}

impl Kind {
    pub fn name(&self) -> &'static str {
        match self {
            Kind::Actor => "actor",
            Kind::ExtSystem => "extsystem",
            Kind::Requirement => "requirement",
            Kind::Business => "business",
            Kind::Buc => "buc",
            Kind::UsageScene => "usagescene",
            Kind::UseCase => "usecase",
            Kind::Screen => "screen",
            Kind::Event => "event",
            Kind::Entity => "entity",
            Kind::State => "state",
            Kind::Condition => "condition",
            Kind::Variation => "variation",
        }
    }
}

// ── Column type ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ColType {
    Int,
    String,
    Money,
    DateTime,
    Date,
    Bool,
    Decimal,
    /// `Enum(active, discontinued)`
    Enum(Vec<std::string::String>),
}

// ── Column annotation ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Annotation {
    /// `@pk`
    Pk,
    /// `@pk(a, b)`
    PkComposite(Vec<std::string::String>),
    /// `@unique`
    Unique,
    /// `@null`
    Null,
    /// `@default(value)`
    Default(std::string::String),
    /// `@label("...")`
    Label(std::string::String),
}

// ── Column definition ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub name: std::string::String,
    pub col_type: ColType,
    pub annotations: Vec<Annotation>,
    pub span: Span,
}

// ── Instance declaration ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct InstanceDecl {
    pub kind: Kind,
    pub id: std::string::String,
    pub label: std::string::String,
    /// Non-empty only for `entity` declarations.
    pub columns: Vec<Column>,
    pub span: Span,
}

// ── Qualified reference ──────────────────────────────────────────────────────

/// A reference to a declared element, optionally qualified by kind.
///
/// Plain:   `Customer` or `a.Customer`  → `kind_qualifier = None`
/// Typed:   `usecase::Browse`            → `kind_qualifier = Some(Kind::UseCase)`
#[derive(Debug, Clone, PartialEq)]
pub struct QRef {
    /// Present when the reference uses the `kind::Id` syntax.
    pub kind_qualifier: Option<Kind>,
    /// Namespace segments plus the final identifier name.
    /// For typed references this is always a single element.
    pub parts: Vec<std::string::String>,
    pub span: Span,
}

// ── Predicate call ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct PredicateCall {
    pub name: std::string::String,
    pub args: Vec<PredicateArg>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PredicateArg {
    Ref(QRef),
    Lit(std::string::String),
}

// ── Top-level item ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Module(DottedName, Span),
    Import(ImportDecl),
    Instance(InstanceDecl),
    Predicate(PredicateCall),
}

// ── Full AST ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Ast {
    pub items: Vec<Item>,
    pub source: std::string::String,
}
