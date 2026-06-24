use super::keys::{EntityKey, EventKey, StateKey};
use super::refs::NodeRef;

/// 状態遷移の三つ組（状態遷移図用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateTransition {
    pub event: EventKey,
    pub from: StateKey,
    pub to: StateKey,
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
