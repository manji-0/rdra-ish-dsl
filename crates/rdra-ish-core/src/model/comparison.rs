use super::keys::EntityKey;
use super::refs::NodeRef;

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
