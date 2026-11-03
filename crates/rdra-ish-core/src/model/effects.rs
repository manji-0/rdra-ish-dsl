use super::keys::{ConditionKey, EntityKey, UseCaseKey};

/// `transitions(UseCase, Entity.column, from -> to)`: the use case moves the column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateTransition {
    pub entity: EntityKey,
    pub column: std::string::String,
    pub usecase: UseCaseKey,
    pub from: std::string::String,
    pub to: std::string::String,
    /// `.when(Cond)`: the arm is taken only while the condition's `holds` body holds.
    pub guard: Option<ConditionKey>,

    /// Source site of the predicate/declaration that produced this fact.
    pub site: Option<crate::location::LocatedSpan>,
}

/// `sets(...)` 述語で宣言されるカラム効果の抽象値
///
/// 到達判定において `TypedPresent(_)` は `Present` と同値であり、
/// 型名はメタデータ（出力・provenance 用）として記録される。
/// `Int` は TLA+/TLC 算術軸向け。BFS `states` では状態軸にならない。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EffectValue {
    /// Enum カラムの特定バリアント
    EnumVariant(std::string::String),
    /// Bool カラムの true/false
    Bool(bool),
    /// Int / Money / Decimal への整数代入（TLA IntRange 向け）
    Int(i64),
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

    /// The value as the DSL writes it: `paid`, `true`, `3`, `present`, `null`.
    pub fn literal(&self) -> std::string::String {
        match self.normalize() {
            EffectValue::EnumVariant(v) => v.clone(),
            EffectValue::Bool(b) => b.to_string(),
            EffectValue::Int(n) => n.to_string(),
            EffectValue::Null => "null".to_string(),
            EffectValue::Present | EffectValue::TypedPresent(_) => "present".to_string(),
        }
    }
}

/// `chooses(UseCase, Entity.column)`: the actor picks the variation column's value when the
/// use case runs, as on a creation; any value of the variation may follow. With
/// `chooses(UseCase, Entity.column, value)` lines, only the values they list may follow.
#[derive(Debug, Clone)]
pub struct ColumnChoice {
    pub origin: UseCaseKey,
    pub entity: EntityKey,
    pub column: std::string::String,
    /// One per `chooses(.., value)` line, in declaration order; empty for the value-less form
    /// (any value). A value listed twice keeps both lines.
    pub values: Vec<ChosenValue>,

    /// Source site of the predicate/declaration that produced this fact.
    pub site: Option<crate::location::LocatedSpan>,
}

/// One `chooses(UseCase, Entity.column, value)` line.
#[derive(Debug, Clone)]
pub struct ChosenValue {
    pub value: std::string::String,
    /// `.when(Cond)`: the value is a choice only while the condition's `holds` body holds.
    pub guard: Option<ConditionKey>,
    pub site: Option<crate::location::LocatedSpan>,
}

/// What `sets(UC, E.column == rhs)` leaves in the column.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SetValue {
    /// `column == value`: a literal of the column's type.
    Value(EffectValue),
    /// `column == F.source`: the value `F.source` holds in the row the step binds for `F`.
    Copy(super::QualifiedModelColumnRef),
}

impl SetValue {
    /// The literal, or `None` for a copy.
    pub fn literal(&self) -> Option<&EffectValue> {
        match self {
            SetValue::Value(v) => Some(v),
            SetValue::Copy(_) => None,
        }
    }
}

/// `links(UseCase, Child, Parent)`: the use case links an existing child row to an existing
/// parent row through the `.optional()` key `key`, which `analysis::fk::resolve_links` picks
/// once the keys exist (`None` after it reported why there is none).
#[derive(Debug, Clone)]
pub struct Link {
    pub origin: UseCaseKey,
    pub child: EntityKey,
    pub parent: EntityKey,
    /// `.role(role)`: the key to follow where several join the child to the parent.
    pub by: Vec<String>,
    pub key: Option<super::FkId>,
    pub site: Option<crate::location::LocatedSpan>,
}

/// `sets(...)` 述語由来のカラム効果（解析後）
#[derive(Debug, Clone)]
pub struct ColumnEffect {
    /// 効果を起こす usecase
    pub origin: UseCaseKey,
    /// 対象 entity のキー
    pub entity: EntityKey,
    /// 対象カラム名
    pub column: std::string::String,
    /// The value the use case leaves: a literal, or a copy of another row's column.
    pub value: SetValue,
    /// `.role(role)` on a copy: the key that leads to the source row, where several could.
    pub by: Vec<String>,

    /// Source site of the predicate/declaration that produced this fact.
    pub site: Option<crate::location::LocatedSpan>,
}
