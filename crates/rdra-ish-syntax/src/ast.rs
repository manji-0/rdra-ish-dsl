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
    Adr,
    Nfr,
    Quality,
    Constraint,
    Concept,
    DomainObject,
    Aggregate,
    ValueObject,
    Business,
    Buc,
    Flow,
    Step,
    UsageScene,
    UseCase,
    Screen,
    Field,
    Event,
    Entity,
    State,
    Condition,
    Variation,
    Api,
    Dto,
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
            Kind::Adr => "adr",
            Kind::Nfr => "nfr",
            Kind::Quality => "quality",
            Kind::Constraint => "constraint",
            Kind::Concept => "concept",
            Kind::DomainObject => "domain_object",
            Kind::Aggregate => "aggregate",
            Kind::ValueObject => "valueobject",
            Kind::Business => "business",
            Kind::Buc => "buc",
            Kind::Flow => "flow",
            Kind::Step => "step",
            Kind::UsageScene => "usagescene",
            Kind::UseCase => "usecase",
            Kind::Screen => "screen",
            Kind::Field => "field",
            Kind::Event => "event",
            Kind::Entity => "entity",
            Kind::State => "state",
            Kind::Condition => "condition",
            Kind::Variation => "variation",
            Kind::Api => "api",
            Kind::Dto => "dto",
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
    /// `@unique(a, b)`
    UniqueComposite(Vec<std::string::String>),
    /// `@index`
    Index,
    /// `@index(a, b)`
    IndexComposite(Vec<std::string::String>),
    /// `@check("...")`
    Check(std::string::String),
    /// `@null`
    Null,
    /// `@default(value)`
    Default(std::string::String),
    /// `@label("...")`
    Label(std::string::String),
    /// `@soft_delete`
    SoftDelete,
    /// `@history`
    History,
    /// `@tenant`
    Tenant,
    /// `@derived("...")`
    Derived(std::string::String),
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RequirementMetadata {
    pub priority: Option<std::string::String>,
    pub sources: Vec<std::string::String>,
    pub stakeholders: Vec<std::string::String>,
    pub owner: Option<std::string::String>,
    pub acceptance_criteria: Vec<std::string::String>,
    pub status: Option<std::string::String>,
    pub risk: Option<std::string::String>,
    pub rationale: Option<std::string::String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AdrMetadata {
    pub status: Option<std::string::String>,
    pub context: Vec<std::string::String>,
    pub decision: Option<std::string::String>,
    pub consequences: Vec<std::string::String>,
    pub accepted_options: Vec<std::string::String>,
    pub rejected_options: Vec<std::string::String>,
    pub reasons: Vec<std::string::String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApiMetadata {
    pub method: Option<std::string::String>,
    pub path: Option<std::string::String>,
    pub idempotency: Option<std::string::String>,
    pub mode: Option<std::string::String>,
    pub auth_scheme: Option<std::string::String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NfrMetadata {
    pub metric: Option<std::string::String>,
    pub target: Option<std::string::String>,
    pub window: Option<std::string::String>,
    pub slo: Option<std::string::String>,
    pub availability: Option<std::string::String>,
    pub resilience: Option<std::string::String>,
    pub audit: Option<std::string::String>,
    pub logging: Option<std::string::String>,
    pub retention: Option<std::string::String>,
    pub privacy: Option<std::string::String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldMetadata {
    pub access: Option<std::string::String>,
    pub required: Option<bool>,
    pub source: Option<std::string::String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UseCaseMetadata {
    pub preconditions: Vec<std::string::String>,
    pub postconditions: Vec<std::string::String>,
    pub guards: Vec<std::string::String>,
    pub alternatives: Vec<std::string::String>,
    pub errors: Vec<std::string::String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstanceDecl {
    pub kind: Kind,
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
    /// Non-empty only for `requirement` declarations.
    pub requirement: RequirementMetadata,
    /// Non-empty only for `adr` declarations.
    pub adr: AdrMetadata,
    /// Non-empty only for `api` declarations.
    pub api: ApiMetadata,
    /// Non-empty only for `nfr` and `constraint` declarations.
    pub nfr: NfrMetadata,
    /// Non-empty only for `field` declarations.
    pub field: FieldMetadata,
    /// Non-empty only for `usecase` declarations.
    pub usecase: UseCaseMetadata,
    /// Non-empty only for `entity` declarations.
    /// DTO declarations also reuse this column shape for contract fields.
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

/// Expression node for comparisons and logical combinations.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A comparison predicate, e.g. `stock < selling`.
    Cmp(Comparison),
    /// Logical AND: `a and b` or `/\ a b` (TLA alias).
    And(Box<Expr>, Box<Expr>),
    /// Logical OR: `a or b` or `\/ a b` (TLA alias).
    Or(Box<Expr>, Box<Expr>),
    /// Logical NOT: `not a` or `~ a` (TLA alias).
    Not(Box<Expr>),
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
    /// Comparison or logical expression (e.g. `stock < selling`, `status == cancelled`).
    Expr(Expr),
    /// State transition endpoint: `pending -> paid`.
    Transition {
        from: std::string::String,
        to: std::string::String,
    },
    /// Cardinality literal: `N:1`, `1:N`, `1:1`, `N:M`.
    Card(std::string::String),
}

// ── Chain call ────────────────────────────────────────────────────────────────

/// チェーン呼び出し: `predicate(E).method(args...)` の `.method(args...)` 部分。
/// `invariant(E).when(status == delivered).then(delivered_at == present)` のような
/// メソッドチェーン表現で使用する。
#[derive(Debug, Clone, PartialEq)]
pub struct ChainCall {
    pub name: std::string::String,
    pub args: Vec<PredicateArg>,
    pub span: Span,
}

// ── Temporal property declarations ───────────────────────────────────────────

/// A temporal formula used in `property` declarations.
#[derive(Debug, Clone, PartialEq)]
pub enum AstTemporalFormula {
    Always(Expr),
    Eventually(Expr),
    LeadsTo { antecedent: Expr, consequent: Expr },
}

/// A `property` declaration binding a temporal formula to an id/label.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyDecl {
    pub id: std::string::String,
    /// Optional display label; omitted in `property Id <formula>`.
    pub label: Option<std::string::String>,
    pub formula: AstTemporalFormula,
    pub span: Span,
}

// ── Top-level item ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Item {
    Module(DottedName, Span),
    Import(ImportDecl),
    Instance(InstanceDecl),
    Predicate(PredicateCall),
    Property(PropertyDecl),
}

// ── Full AST ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Ast {
    pub items: Vec<Item>,
    pub source: std::string::String,
}
