use rdra_ish_syntax::ast::Kind;
use slotmap::{new_key_type, SlotMap};
use std::collections::HashMap;

// --- Key types ---
new_key_type! {
    pub struct ActorKey;
    pub struct ExtSystemKey;
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
}

/// NodeRef: 異種ノード間関連を一様表現
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeRef {
    Actor(ActorKey),
    ExtSystem(ExtSystemKey),
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
    Motivates,
    Transitions,
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
}

#[derive(Debug, Clone)]
pub struct ExtSystem {
    pub id: std::string::String,
    pub label: std::string::String,
}

#[derive(Debug, Clone)]
pub struct Requirement {
    pub id: std::string::String,
    pub label: std::string::String,
}

#[derive(Debug, Clone)]
pub struct Business {
    pub id: std::string::String,
    pub label: std::string::String,
}

#[derive(Debug, Clone)]
pub struct Buc {
    pub id: std::string::String,
    pub label: std::string::String,
}

#[derive(Debug, Clone)]
pub struct UsageScene {
    pub id: std::string::String,
    pub label: std::string::String,
}

#[derive(Debug, Clone)]
pub struct UseCase {
    pub id: std::string::String,
    pub label: std::string::String,
}

#[derive(Debug, Clone)]
pub struct Screen {
    pub id: std::string::String,
    pub label: std::string::String,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub id: std::string::String,
    pub label: std::string::String,
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub id: std::string::String,
    pub label: std::string::String,
    pub columns: Vec<ModelColumn>,
}

#[derive(Debug, Clone)]
pub struct State {
    pub id: std::string::String,
    pub label: std::string::String,
}

#[derive(Debug, Clone)]
pub struct Condition {
    pub id: std::string::String,
    pub label: std::string::String,
}

#[derive(Debug, Clone)]
pub struct Variation {
    pub id: std::string::String,
    pub label: std::string::String,
}

// ── Symbol table ─────────────────────────────────────────────────────────────

fn node_kind_tag(node: &NodeRef) -> &'static str {
    match node {
        NodeRef::Actor(_) => "actor",
        NodeRef::ExtSystem(_) => "extsystem",
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
    }
}

fn node_ref_matches_kind(node: &NodeRef, kind: &Kind) -> bool {
    matches!(
        (node, kind),
        (NodeRef::Actor(_), Kind::Actor)
            | (NodeRef::ExtSystem(_), Kind::ExtSystem)
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
        matches!(self, EffectValue::Present | EffectValue::Null | EffectValue::TypedPresent(_))
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

/// セマンティックモデル
#[derive(Debug, Default)]
pub struct SemanticModel {
    pub actors: SlotMap<ActorKey, Actor>,
    pub ext_systems: SlotMap<ExtSystemKey, ExtSystem>,
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
    pub relations: Vec<Relation>,
    pub state_transitions: Vec<StateTransition>,
    pub column_effects: Vec<ColumnEffect>,
    pub symbols: SymbolTable,
}
