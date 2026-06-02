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
    System,
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
    Api,
    Location,
    Timing,
    Medium,
    Permission,
}

impl Kind {
    pub fn name(&self) -> &'static str {
        match self {
            Kind::Actor => "actor",
            Kind::ExtSystem => "extsystem",
            Kind::System => "system",
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
            Kind::Api => "api",
            Kind::Location => "location",
            Kind::Timing => "timing",
            Kind::Medium => "medium",
            Kind::Permission => "permission",
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
    pub description: Option<std::string::String>,
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

// ── Comparison expressions ───────────────────────────────────────────────────

/// Comparison operator used in comparison expressions (e.g. `stock < selling`).
#[derive(Debug, Clone, PartialEq)]
pub enum CmpOp {
    Lt, // <
    Gt, // >
    Le, // <=
    Ge, // >=
    Eq, // ==
    Ne, // !=
}

impl CmpOp {
    /// Returns the canonical operator symbol string (used for axis key generation).
    pub fn as_str(&self) -> &'static str {
        match self {
            CmpOp::Lt => "<",
            CmpOp::Gt => ">",
            CmpOp::Le => "<=",
            CmpOp::Ge => ">=",
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
        }
    }
}

/// One operand in a comparison expression.
/// Designed to be extensible toward arithmetic expressions in the future.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    /// A bare identifier referencing a column (e.g. `stock`, `expired_at`).
    Column(std::string::String),
    /// An entity-qualified column reference (e.g. `Order.status`).
    QualifiedColumn(QualifiedColumnRef),
    /// An integer literal (stored as string to avoid lossy conversion).
    IntLit(std::string::String),
    /// The built-in temporal reference `now`.
    Now,
    // Future: Arith(Box<Operand>, ArithOp, Box<Operand>)
}

/// A column reference qualified by an entity id.
///
/// This is intentionally separate from `QRef`: the entity portion resolves to a
/// declared entity, while `column` resolves inside that entity's column list.
#[derive(Debug, Clone, PartialEq)]
pub struct QualifiedColumnRef {
    pub entity: QRef,
    pub column: std::string::String,
    pub span: Span,
}

/// A single comparison expression, e.g. `stock < selling` or `expired_at < now`.
/// Represents one boolean proposition whose truth value drives the comparison axis.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub lhs: Operand,
    pub op: CmpOp,
    pub rhs: Operand,
    pub span: Span,
}

/// Expression node. Currently only comparison; extensible to logical combinations.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A comparison predicate, e.g. `stock < selling`.
    Cmp(Comparison),
    // Future: And(Box<Expr>, Box<Expr>), Or(...), Not(...)
}

// ── Predicate call ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct PredicateCall {
    pub name: std::string::String,
    pub args: Vec<PredicateArg>,
    /// `.when(...).then(...)` のようなチェーン呼び出しリスト。
    pub chain: Vec<ChainCall>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PredicateArg {
    Ref(QRef),
    Lit(std::string::String),
    /// インライン `(col, val)` タプル。entity 制約述語で使用。
    Tuple(Vec<PredicateArg>),
    /// 比較式（例: `stock < selling`, `expired_at < now`）。
    /// entity 制約述語の条件、および sets の比較命題引数として使用。
    Expr(Expr),
}

// ── Chain call ────────────────────────────────────────────────────────────────

/// チェーン呼び出し: `predicate(E).method(args...)` の `.method(args...)` 部分。
/// `invariant(E).when(status, delivered).then(delivered_at, present)` のような
/// メソッドチェーン表現で使用する。
#[derive(Debug, Clone, PartialEq)]
pub struct ChainCall {
    pub name: std::string::String,
    pub args: Vec<PredicateArg>,
    pub span: Span,
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
