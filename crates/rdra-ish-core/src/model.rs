use rdra_ish_syntax::ast::Kind;
use slotmap::{new_key_type, SlotMap};
use std::collections::HashMap;

// --- Key types ---
new_key_type! {
    pub struct ActorKey;
    pub struct ExtSystemKey;
    pub struct SystemKey;
    pub struct RequirementKey;
    pub struct BusinessKey;
    pub struct BucKey;
    pub struct UsageSceneKey;
    pub struct UseCaseKey;
    pub struct ScreenKey;
    pub struct EventKey;
    pub struct EntityKey;
    pub struct StateKey;
    pub struct ConditionKey;
    pub struct VariationKey;
    pub struct ApiKey;
    pub struct LocationKey;
    pub struct TimingKey;
    pub struct MediumKey;
    pub struct PermissionKey;
}

/// NodeRef: 異種ノード間関連を一様表現
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeRef {
    Actor(ActorKey),
    ExtSystem(ExtSystemKey),
    System(SystemKey),
    Requirement(RequirementKey),
    Business(BusinessKey),
    Buc(BucKey),
    UsageScene(UsageSceneKey),
    UseCase(UseCaseKey),
    Screen(ScreenKey),
    Event(EventKey),
    Entity(EntityKey),
    State(StateKey),
    Condition(ConditionKey),
    Variation(VariationKey),
    Api(ApiKey),
    Location(LocationKey),
    Timing(TimingKey),
    Medium(MediumKey),
    Permission(PermissionKey),
}

/// 述語の種類
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RelKind {
    Performs,
    Uses,
    Reads,
    Writes,
    Creates,
    Updates,
    Deletes,
    Displays,
    Shows,
    Raises,
    Triggers,
    Contains,
    Belongs,
    HasPermission,
    RequiresPermission,
    RequiresMedium,
    Motivates,
    Transitions,
    Invokes, // usecase → api
    // Entity ER
    RelateOneToOne,   // 1:1
    RelateOneToMany,  // 1:N (A側が1, B側がMany)
    RelateManyToOne,  // N:1 (A側がMany, B側が1) → A に FK
    RelateManyToMany, // N:M (警告のみ)
}

/// リレーション（from, to, kind）
#[derive(Debug, Clone)]
pub struct Relation {
    pub from: NodeRef,
    pub to: NodeRef,
    pub kind: RelKind,
}

/// `coordinates(usecase, entity, entity)` で宣言される、system境界越えrelationの調停責務。
#[derive(Debug, Clone)]
pub struct BoundaryCoordination {
    pub usecase: UseCaseKey,
    pub left: EntityKey,
    pub right: EntityKey,
}

/// `belongs(Buc, Business).when(...).where(...).by(...)` で宣言される、
/// Business と BUC の対応関係に付く文脈値。
#[derive(Debug, Clone)]
pub enum BusinessMappingContextValue {
    Text(std::string::String),
    Ref(NodeRef),
}

/// Business と BUC の対応関係に付く When / Where / By 文脈。
#[derive(Debug, Clone)]
pub struct BusinessMappingContext {
    pub buc: BucKey,
    pub business: BusinessKey,
    pub whens: Vec<BusinessMappingContextValue>,
    pub wheres: Vec<BusinessMappingContextValue>,
    pub bys: Vec<BusinessMappingContextValue>,
}

/// カラム型
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnType {
    Int,
    String,
    Money,
    DateTime,
    Date,
    Bool,
    Decimal,
    Enum(Vec<std::string::String>),
}

/// カラム
#[derive(Debug, Clone)]
pub struct ModelColumn {
    pub name: std::string::String,
    pub col_type: ColumnType,
    pub is_pk: bool,
    pub is_unique: bool,
    pub is_nullable: bool,
    pub default_val: Option<std::string::String>,
    pub label: Option<std::string::String>,
    pub is_fk: bool,
    pub fk_target: Option<std::string::String>,
}

/// 各要素（全て id: String, label: String を持つ最小構造）
#[derive(Debug, Clone)]
pub struct Actor {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct ExtSystem {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct System {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Requirement {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Business {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Buc {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct UsageScene {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct UseCase {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Screen {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
    pub columns: Vec<ModelColumn>,
}

#[derive(Debug, Clone)]
pub struct State {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Condition {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Variation {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Api {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Location {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Timing {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Medium {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

#[derive(Debug, Clone)]
pub struct Permission {
    pub id: std::string::String,
    pub label: std::string::String,
    pub description: Option<std::string::String>,
}

// ── Symbol table ─────────────────────────────────────────────────────────────

fn node_kind_tag(node: &NodeRef) -> &'static str {
    match node {
        NodeRef::Actor(_) => "actor",
        NodeRef::ExtSystem(_) => "extsystem",
        NodeRef::System(_) => "system",
        NodeRef::Requirement(_) => "requirement",
        NodeRef::Business(_) => "business",
        NodeRef::Buc(_) => "buc",
        NodeRef::UsageScene(_) => "usagescene",
        NodeRef::UseCase(_) => "usecase",
        NodeRef::Screen(_) => "screen",
        NodeRef::Event(_) => "event",
        NodeRef::Entity(_) => "entity",
        NodeRef::State(_) => "state",
        NodeRef::Condition(_) => "condition",
        NodeRef::Variation(_) => "variation",
        NodeRef::Api(_) => "api",
        NodeRef::Location(_) => "location",
        NodeRef::Timing(_) => "timing",
        NodeRef::Medium(_) => "medium",
        NodeRef::Permission(_) => "permission",
    }
}

fn node_ref_matches_kind(node: &NodeRef, kind: &Kind) -> bool {
    matches!(
        (node, kind),
        (NodeRef::Actor(_), Kind::Actor)
            | (NodeRef::ExtSystem(_), Kind::ExtSystem)
            | (NodeRef::System(_), Kind::System)
            | (NodeRef::Requirement(_), Kind::Requirement)
            | (NodeRef::Business(_), Kind::Business)
            | (NodeRef::Buc(_), Kind::Buc)
            | (NodeRef::UsageScene(_), Kind::UsageScene)
            | (NodeRef::UseCase(_), Kind::UseCase)
            | (NodeRef::Screen(_), Kind::Screen)
            | (NodeRef::Event(_), Kind::Event)
            | (NodeRef::Entity(_), Kind::Entity)
            | (NodeRef::State(_), Kind::State)
            | (NodeRef::Condition(_), Kind::Condition)
            | (NodeRef::Variation(_), Kind::Variation)
            | (NodeRef::Api(_), Kind::Api)
            | (NodeRef::Location(_), Kind::Location)
            | (NodeRef::Timing(_), Kind::Timing)
            | (NodeRef::Medium(_), Kind::Medium)
            | (NodeRef::Permission(_), Kind::Permission)
    )
}

/// The result of an unqualified symbol lookup.
pub enum LookupResult<'a> {
    /// Exactly one node with this id.
    Found(&'a NodeRef),
    /// No node with this id.
    NotFound,
    /// Multiple nodes with this id (different kinds). Carries the kind names.
    Ambiguous(Vec<&'static str>),
}

/// Symbol table that allows the same identifier to be reused across different
/// kinds (e.g. `actor Add` and `usecase Add` can coexist).
///
/// Qualified references (`usecase::Add`) resolve unambiguously.
/// Unqualified references (`Add`) succeed only when the identifier is unique
/// across all kinds; otherwise `LookupResult::Ambiguous` is returned.
#[derive(Debug, Default)]
pub struct SymbolTable {
    by_id: HashMap<std::string::String, Vec<NodeRef>>,
}

impl SymbolTable {
    /// Insert a node.
    ///
    /// Returns `true` when a node of the **same kind** with the same id already
    /// exists (duplicate definition error). Cross-kind duplicates are allowed.
    pub fn insert(&mut self, id: std::string::String, node: NodeRef) -> bool {
        let entries = self.by_id.entry(id).or_default();
        if entries
            .iter()
            .any(|n| node_kind_tag(n) == node_kind_tag(&node))
        {
            return true;
        }
        entries.push(node);
        false
    }

    /// Unqualified lookup.
    pub fn lookup(&self, id: &str) -> LookupResult<'_> {
        match self.by_id.get(id) {
            None => LookupResult::NotFound,
            Some(v) if v.len() == 1 => LookupResult::Found(&v[0]),
            Some(v) => LookupResult::Ambiguous(v.iter().map(|n| node_kind_tag(n)).collect()),
        }
    }

    /// Kind-qualified lookup: `kind::id`.
    pub fn lookup_qualified(&self, kind: &Kind, id: &str) -> Option<&NodeRef> {
        self.by_id
            .get(id)?
            .iter()
            .find(|n| node_ref_matches_kind(n, kind))
    }

    /// Namespace-qualified lookup: `a.Foo` → look up the last segment.
    /// Returns `None` when the id is missing or ambiguous.
    pub fn resolve_qref(&self, parts: &[std::string::String]) -> Option<&NodeRef> {
        let id = parts.last()?;
        match self.lookup(id) {
            LookupResult::Found(n) => Some(n),
            _ => None,
        }
    }

    /// Simple presence check by bare id (any kind).
    pub fn get(&self, id: &str) -> Option<&NodeRef> {
        match self.lookup(id) {
            LookupResult::Found(n) => Some(n),
            _ => None,
        }
    }
}

/// 状態遷移の三つ組（状態遷移図用）
#[derive(Debug, Clone)]
pub struct StateTransition {
    pub event: NodeRef, // Event
    pub from: NodeRef,  // State before
    pub to: NodeRef,    // State after
}

/// `sets(...)` 述語で宣言されるカラム効果の抽象値
///
/// 到達判定において `TypedPresent(_)` は `Present` と同値であり、
/// 型名はメタデータ（出力・provenance 用）として記録される。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EffectValue {
    /// Enum カラムの特定バリアント
    EnumVariant(std::string::String),
    /// Bool カラムの true/false
    Bool(bool),
    /// Nullable カラムが非null（値あり）
    Present,
    /// Nullable カラムが null
    Null,
    /// Nullable カラムが非null で、PostgreSQL 特殊型名を記録（例: "timestamptz", "jsonb"）。
    /// 到達判定は `Present` と同一。
    TypedPresent(std::string::String),
}

impl EffectValue {
    /// 到達判定用の正規化: `TypedPresent` を `Present` に畳む
    pub fn normalize(&self) -> &EffectValue {
        match self {
            EffectValue::TypedPresent(_) => &EffectValue::Present,
            other => other,
        }
    }

    /// null/非null の軸か（EffectValue が Present/Null/TypedPresent であれば true）
    pub fn is_nullable_axis(&self) -> bool {
        matches!(
            self,
            EffectValue::Present | EffectValue::Null | EffectValue::TypedPresent(_)
        )
    }
}

/// `sets(...)` 述語由来のカラム効果（解析後）
#[derive(Debug, Clone)]
pub struct ColumnEffect {
    /// 効果を起こす usecase または event の NodeRef
    pub origin: NodeRef,
    /// 対象 entity のキー
    pub entity: EntityKey,
    /// 対象カラム名
    pub column: std::string::String,
    /// 設定する抽象値
    pub value: EffectValue,
}

// ── 比較命題 ─────────────────────────────────────────────────────────────────

/// 比較演算子（モデル層）。`ast::CmpOp` の写し。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CmpOpModel {
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
}

impl CmpOpModel {
    pub fn as_str(&self) -> &'static str {
        match self {
            CmpOpModel::Lt => "<",
            CmpOpModel::Gt => ">",
            CmpOpModel::Le => "<=",
            CmpOpModel::Ge => ">=",
            CmpOpModel::Eq => "==",
            CmpOpModel::Ne => "!=",
        }
    }
}

/// 比較式の右辺。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CmpRhs {
    /// 同エンティティの別カラム参照（例: `selling`）。
    Column(std::string::String),
    /// 整数リテラル。
    IntLit(i64),
    /// 組み込み時間参照 `now`。
    Now,
}

impl CmpRhs {
    /// 軸キー・診断メッセージ用の表示文字列。
    pub fn display(&self) -> std::string::String {
        match self {
            CmpRhs::Column(c) => c.clone(),
            CmpRhs::IntLit(n) => n.to_string(),
            CmpRhs::Now => "now".to_string(),
        }
    }
}

/// `stock < selling` のような比較命題。
/// BFS 状態空間では「デフォルト false の派生 Bool 軸」として扱われる。
/// 真偽は `sets(origin, entity, <expr>, true/false)` によって駆動される。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComparisonProp {
    /// 比較左辺のカラム名（必ずカラム参照）。
    pub lhs_column: std::string::String,
    pub op: CmpOpModel,
    pub rhs: CmpRhs,
}

impl ComparisonProp {
    /// 軸キー文字列を返す（例: `"stock<selling"`, `"expired_at<now"`）。
    /// 同一比較式を一意な軸に対応付けるためのキーとして使用。
    pub fn axis_key(&self) -> std::string::String {
        format!(
            "{}{}{}",
            self.lhs_column,
            self.op.as_str(),
            self.rhs.display()
        )
    }

    /// 人が読める表示文字列（例: `"stock < selling"`）。
    pub fn display(&self) -> std::string::String {
        format!(
            "{} {} {}",
            self.lhs_column,
            self.op.as_str(),
            self.rhs.display()
        )
    }
}

/// `sets(origin, entity, <comparison_expr>, true/false)` で宣言された
/// 比較命題の真偽効果（解析後）。
#[derive(Debug, Clone)]
pub struct PropositionEffect {
    /// 効果を起こす usecase または event の NodeRef。
    pub origin: NodeRef,
    /// 対象 entity のキー。
    pub entity: EntityKey,
    /// 真偽を変化させる比較命題。
    pub prop: ComparisonProp,
    /// 設定する真偽値。
    pub truth: bool,
}

/// `forbidden(Entity, (col, val), ...)` で宣言された禁止状態制約。
/// `conditions` に列挙した全ての (col, val) が同時に成立する状態は禁止（AND）。
#[derive(Debug, Clone)]
pub struct ForbiddenConstraint {
    pub entity: EntityKey,
    /// 禁止する等値 (カラム名, 値) の組合せ（全件 AND）
    pub conditions: Vec<(std::string::String, EffectValue)>,
    /// 禁止条件に含まれる比較命題（全件 AND、等値条件と合わせて評価）
    pub comparisons: Vec<ComparisonProp>,
}

/// `invariant(Entity).when(col, val).then(col, val)` で宣言された不変条件。
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
    /// `Entity.column == value`, written as `(Entity.column, value)`.
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

/// `cross_forbidden(EntityA, EntityB, ...)`.
#[derive(Debug, Clone)]
pub struct CrossForbiddenConstraint {
    pub scope: Vec<EntityKey>,
    pub scope_semantics: CrossConstraintScope,
    pub conditions: Vec<CrossEntityCondition>,
}

/// `cross_invariant(EntityA, EntityB).when(...).then(...)`.
#[derive(Debug, Clone)]
pub struct CrossEntityInvariant {
    pub scope: Vec<EntityKey>,
    pub scope_semantics: CrossConstraintScope,
    pub guards: Vec<CrossEntityCondition>,
    pub requireds: Vec<CrossEntityCondition>,
}

/// セマンティックモデル
#[derive(Debug, Default)]
pub struct SemanticModel {
    pub actors: SlotMap<ActorKey, Actor>,
    pub ext_systems: SlotMap<ExtSystemKey, ExtSystem>,
    pub systems: SlotMap<SystemKey, System>,
    pub requirements: SlotMap<RequirementKey, Requirement>,
    pub businesses: SlotMap<BusinessKey, Business>,
    pub bucs: SlotMap<BucKey, Buc>,
    pub usage_scenes: SlotMap<UsageSceneKey, UsageScene>,
    pub use_cases: SlotMap<UseCaseKey, UseCase>,
    pub screens: SlotMap<ScreenKey, Screen>,
    pub events: SlotMap<EventKey, Event>,
    pub entities: SlotMap<EntityKey, Entity>,
    pub states: SlotMap<StateKey, State>,
    pub conditions: SlotMap<ConditionKey, Condition>,
    pub variations: SlotMap<VariationKey, Variation>,
    pub apis: SlotMap<ApiKey, Api>,
    pub locations: SlotMap<LocationKey, Location>,
    pub timings: SlotMap<TimingKey, Timing>,
    pub media: SlotMap<MediumKey, Medium>,
    pub permissions: SlotMap<PermissionKey, Permission>,
    pub relations: Vec<Relation>,
    pub boundary_coordinations: Vec<BoundaryCoordination>,
    pub business_mapping_contexts: Vec<BusinessMappingContext>,
    pub state_transitions: Vec<StateTransition>,
    pub column_effects: Vec<ColumnEffect>,
    /// `sets(origin, entity, <comparison_expr>, bool)` で宣言された比較命題の真偽効果
    pub proposition_effects: Vec<PropositionEffect>,
    /// `forbidden(...)` 述語で宣言された禁止状態制約
    pub forbidden_constraints: Vec<ForbiddenConstraint>,
    /// `invariant(...)` 述語で宣言された不変条件制約
    pub entity_invariants: Vec<EntityInvariant>,
    /// `cross_forbidden(...)` 述語で宣言されたクロスエンティティ禁止制約
    pub cross_forbidden_constraints: Vec<CrossForbiddenConstraint>,
    /// `cross_invariant(...)` 述語で宣言されたクロスエンティティ不変条件
    pub cross_entity_invariants: Vec<CrossEntityInvariant>,
    pub symbols: SymbolTable,
}
