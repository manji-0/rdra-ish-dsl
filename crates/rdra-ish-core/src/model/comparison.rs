use super::keys::{EntityKey, UseCaseKey};

// ── 比較命題 ─────────────────────────────────────────────────────────────────

/// 比較演算子（モデル層）。`ast::CmpOp` の写し。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// 効果を起こす usecase。
    pub origin: UseCaseKey,
    /// 対象 entity のキー。
    pub entity: EntityKey,
    /// 真偽を変化させる比較命題。
    pub prop: ComparisonProp,
    /// 設定する真偽値。
    pub truth: bool,

    /// Source site of the predicate/declaration that produced this fact.
    pub site: Option<crate::location::LocatedSpan>,
}

// ── Column expressions (`@check`, `@derived`) ───────────────────────────────

/// A condition over one entity's columns: a `@check`, or a logical `@derived`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CheckExpr {
    Cmp(ComparisonProp),
    Not(Box<CheckExpr>),
    And(Box<CheckExpr>, Box<CheckExpr>),
    Or(Box<CheckExpr>, Box<CheckExpr>),
}

impl CheckExpr {
    /// The canonical text, as `rdra-ish fmt` prints it.
    pub fn display(&self) -> std::string::String {
        match self {
            CheckExpr::Cmp(prop) => prop.display(),
            CheckExpr::Not(inner) => format!("not ({})", inner.display()),
            CheckExpr::And(a, b) => format!("({}) and ({})", a.display(), b.display()),
            CheckExpr::Or(a, b) => format!("({}) or ({})", a.display(), b.display()),
        }
    }

    /// Every comparison leaf, left to right.
    pub fn comparisons(&self) -> Vec<&ComparisonProp> {
        match self {
            CheckExpr::Cmp(prop) => vec![prop],
            CheckExpr::Not(inner) => inner.comparisons(),
            CheckExpr::And(a, b) | CheckExpr::Or(a, b) => {
                let mut out = a.comparisons();
                out.extend(b.comparisons());
                out
            }
        }
    }
}

/// One `@check(expr)` of a column.
#[derive(Debug, Clone)]
pub struct ColumnCheck {
    pub expr: CheckExpr,
    pub site: Option<crate::location::LocatedSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArithOperand {
    Column(std::string::String),
    Int(i64),
}

impl ArithOperand {
    fn display(&self) -> std::string::String {
        match self {
            ArithOperand::Column(c) => c.clone(),
            ArithOperand::Int(n) => n.to_string(),
        }
    }
}

/// `lhs op rhs` with `op` one of `+ - *`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArithExpr {
    pub lhs: ArithOperand,
    pub op: rdra_ish_syntax::ast::ArithOp,
    pub rhs: ArithOperand,
}

/// The expression of `@derived(expr)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DerivedExpr {
    /// Derives a Bool column; the lifecycle substitutes it where a construct reads it.
    Logical(CheckExpr),
    /// Derives a numeric column; the lifecycle cannot read it.
    Arith(ArithExpr),
}

impl DerivedExpr {
    /// The canonical text, as `rdra-ish fmt` prints it.
    pub fn display(&self) -> std::string::String {
        match self {
            DerivedExpr::Logical(expr) => expr.display(),
            DerivedExpr::Arith(a) => {
                format!("{} {} {}", a.lhs.display(), a.op.as_str(), a.rhs.display())
            }
        }
    }
}
