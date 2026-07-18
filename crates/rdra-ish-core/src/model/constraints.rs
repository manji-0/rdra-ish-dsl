use super::comparison::{CmpOpModel, ComparisonProp};
use super::effects::EffectValue;
use super::keys::{EntityKey, UseCaseKey};

/// `forbidden(Entity, col == val, ...)` で宣言された禁止状態制約。
/// `conditions` に列挙した全ての col == val が同時に成立する状態は禁止（AND）。
#[derive(Debug, Clone)]
pub struct ForbiddenConstraint {
    pub entity: EntityKey,
    /// 禁止する等値 (カラム名, 値) の組合せ（全件 AND）
    pub conditions: Vec<(std::string::String, EffectValue)>,
    /// 禁止条件に含まれる比較命題（全件 AND、等値条件と合わせて評価）
    pub comparisons: Vec<ComparisonProp>,
}

/// `invariant(Entity).when(col == val).then(col == val)` で宣言された不変条件。
/// `guards` が全て成立するとき、`requireds` も全て成立しなければならない。
#[derive(Debug, Clone)]
pub struct EntityInvariant {
    pub entity: EntityKey,
    /// 等値ガード条件 (カラム名, 値)（全件 AND）
    pub guards: Vec<(std::string::String, EffectValue)>,
    /// 比較命題ガード条件（全件 AND、等値 guards と合わせて評価）
    pub guard_comparisons: Vec<ComparisonProp>,
    /// 等値必要条件 (カラム名, 値)（全件 AND）
    pub requireds: Vec<(std::string::String, EffectValue)>,
    /// 比較命題必要条件（全件 AND、等値 requireds と合わせて評価）
    pub required_comparisons: Vec<ComparisonProp>,
}

/// `required(Entity, col == val, ...)` で宣言された常時成立制約。
/// `conditions` と `comparisons` が全て成立しない到達状態は違反。
#[derive(Debug, Clone)]
pub struct RequiredConstraint {
    pub entity: EntityKey,
    /// 常に成立すべき等値条件（全件 AND）
    pub conditions: Vec<(std::string::String, EffectValue)>,
    /// 常に true であるべき比較命題（全件 AND）
    pub comparisons: Vec<ComparisonProp>,
}

/// `exclusive(Entity, col == val, ...)` で宣言された相互排他制約。
/// 列挙した条件のうち 2 件以上が同時に成立する到達状態は違反。
#[derive(Debug, Clone)]
pub struct ExclusiveConstraint {
    pub entity: EntityKey,
    /// 相互排他にしたい等値条件
    pub conditions: Vec<(std::string::String, EffectValue)>,
    /// 相互排他にしたい比較命題
    pub comparisons: Vec<ComparisonProp>,
}

// ── クロスエンティティ制約 ───────────────────────────────────────────────────

/// A column reference resolved to a concrete entity and one of its columns.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QualifiedModelColumnRef {
    pub entity: EntityKey,
    pub column: std::string::String,
}

/// Right-hand side of a cross-entity comparison.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CrossCmpRhs {
    /// Column reference on the same or another entity.
    Column(QualifiedModelColumnRef),
    /// Integer literal.
    IntLit(i64),
    /// Built-in temporal reference `now`.
    Now,
}

/// A comparison proposition that may reference columns on multiple entities.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrossComparisonProp {
    pub lhs: QualifiedModelColumnRef,
    pub op: CmpOpModel,
    pub rhs: CrossCmpRhs,
}

/// One condition inside a cross-entity constraint.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CrossEntityCondition {
    /// `Entity.column == value`, written as `Entity.column == value`.
    Equals {
        column: QualifiedModelColumnRef,
        value: EffectValue,
    },
    /// A typed comparison expression such as `Order.total > Payment.amount`.
    Comparison(CrossComparisonProp),
}

/// How a cross-entity constraint should choose entity combinations to evaluate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CrossConstraintScope {
    /// Evaluate the cross-product of each participating entity's reached patterns.
    GlobalProduct,
    /// Intended to evaluate only instances connected by the declared relation path.
    RelationPath(Vec<EntityKey>),
}

/// `forbidden(EntityA, EntityB, ...)`.
#[derive(Debug, Clone)]
pub struct CrossForbiddenConstraint {
    pub scope: Vec<EntityKey>,
    pub scope_semantics: CrossConstraintScope,
    pub conditions: Vec<CrossEntityCondition>,
}

/// `invariant(EntityA, EntityB).when(...).then(...)`.
#[derive(Debug, Clone)]
pub struct CrossEntityInvariant {
    pub scope: Vec<EntityKey>,
    pub scope_semantics: CrossConstraintScope,
    pub guards: Vec<CrossEntityCondition>,
    pub requireds: Vec<CrossEntityCondition>,
}

/// `after(UseCase).assert(...)` で宣言される時相アンカー制約。
#[derive(Debug, Clone)]
pub struct TemporalAssertion {
    pub anchor: UseCaseKey,
    pub scope: Vec<EntityKey>,
    pub requireds: Vec<CrossEntityCondition>,
}

/// to-many 関連先に対する量化制約。
#[derive(Debug, Clone)]
pub enum QuantifierKind {
    Has,
    None,
}

/// `when(...).has/none(...)` または
/// `invariant(...).when(...).has/none(...)` で宣言される集計制約。
#[derive(Debug, Clone)]
pub struct QuantifierConstraint {
    pub anchor: EntityKey,
    pub guards: Vec<CrossEntityCondition>,
    pub kind: QuantifierKind,
    pub related: EntityKey,
    pub related_conditions: Vec<CrossEntityCondition>,
}

// ── Temporal properties (TLA+ / TLC) ──────────────────────────────────────────

/// Atomic comparison over an entity column (equality or arithmetic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalAtom {
    /// Entity id when written as `Order.status`; `None` for bare columns.
    pub entity: Option<std::string::String>,
    pub column: std::string::String,
    pub op: super::comparison::CmpOpModel,
    pub rhs: TemporalRhs,
}

/// Right-hand side of a temporal atom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemporalRhs {
    /// Enum / Bool / null / present (and Int via EffectValue::Int).
    Value(EffectValue),
    /// Integer literal (`stock < 5`).
    IntLit(i64),
    /// Column reference (`stock < selling` or `Item.stock < Item.selling`).
    Column {
        entity: Option<std::string::String>,
        column: std::string::String,
    },
}

/// Boolean expression inside a temporal property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemporalExpr {
    Atom(TemporalAtom),
    Not(Box<TemporalExpr>),
    And(Box<TemporalExpr>, Box<TemporalExpr>),
    Or(Box<TemporalExpr>, Box<TemporalExpr>),
}

/// Path property mapped to TLA+ temporal operators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemporalFormula {
    /// `always(expr)` → `[]expr`
    Always(TemporalExpr),
    /// `eventually(expr)` → `<>expr`
    Eventually(TemporalExpr),
    /// `leads_top == q` → `p ~> q`
    LeadsTo {
        antecedent: TemporalExpr,
        consequent: TemporalExpr,
    },
}

/// `property Id "label"` with a temporal formula body.
#[derive(Debug, Clone)]
pub struct TemporalProperty {
    pub id: std::string::String,
    pub label: std::string::String,
    pub formula: TemporalFormula,
}
