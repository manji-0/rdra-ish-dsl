//! Comparison expression typing and model conversion.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use super::qref_util::{qref_id, qualified_column_display};

// ── 比較式の型整合チェック・モデル変換 ────────────────────────────────────────

/// `ColumnType` が「比較に使える型カテゴリ」を返す。
/// - `"numeric"`: Int/Money/Decimal
/// - `"temporal"`: Date/DateTime
/// - `"equality"`: それ以外（等値比較 == / != のみ許容）
/// - `"none"`: 比較不可（比較を拒否）
pub(crate) fn type_category(col_type: &ColumnType) -> &'static str {
    match col_type {
        ColumnType::Int | ColumnType::Money | ColumnType::Decimal => "numeric",
        ColumnType::Date | ColumnType::DateTime => "temporal",
        ColumnType::String | ColumnType::Bool | ColumnType::Enum(_) => "equality",
    }
}

/// `CmpOp` が順序比較か（`<`, `>`, `<=`, `>=`）。
pub(crate) fn is_order_op(op: &CmpOp) -> bool {
    matches!(op, CmpOp::Lt | CmpOp::Gt | CmpOp::Le | CmpOp::Ge)
}

/// `ast::CmpOp` → `model::CmpOpModel` への変換。
pub(crate) fn to_model_op(op: &CmpOp) -> CmpOpModel {
    match op {
        CmpOp::Lt => CmpOpModel::Lt,
        CmpOp::Gt => CmpOpModel::Gt,
        CmpOp::Le => CmpOpModel::Le,
        CmpOp::Ge => CmpOpModel::Ge,
        CmpOp::Eq => CmpOpModel::Eq,
        CmpOp::Ne => CmpOpModel::Ne,
    }
}

/// 比較式 `Comparison` を解析して `ComparisonProp` に変換する。
///
/// - 左辺はカラム参照必須。
/// - 演算子と右辺の型整合を検査する。
/// - 型不整合があれば `diags` にエラーを push し `None` を返す。
pub(crate) fn resolve_comparison(
    entity_cols: &[ModelColumn],
    entity_id: &str,
    cmp: &Comparison,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<ComparisonProp> {
    // ── 左辺はカラム参照のみ ──────────────────────────────────────────────────
    let lhs_col_name = match &cmp.lhs {
        Operand::Column(name) => name.clone(),
        Operand::QualifiedColumn(qcol) => {
            let Some(q_entity) = qref_id(&qcol.entity) else {
                push_error_cmp(ctx, diags, cmp, RdraError::ComparisonLhsMustBeColumn);
                return None;
            };
            if q_entity != entity_id {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::TypeMismatch {
                        pred: "comparison".to_string(),
                        id: qualified_column_display(qcol),
                        actual: format!("column of entity {}", q_entity),
                        expected: format!("column of entity {}", entity_id),
                    },
                );
                return None;
            }
            qcol.column.clone()
        }
        _ => {
            push_error_cmp(ctx, diags, cmp, RdraError::ComparisonLhsMustBeColumn);
            return None;
        }
    };

    // 左辺カラムを解決
    let lhs_col = match entity_cols.iter().find(|c| c.name == lhs_col_name) {
        Some(c) => c,
        None => {
            push_error_cmp(
                ctx,
                diags,
                cmp,
                RdraError::UnknownColumn {
                    entity: entity_id.to_string(),
                    col: lhs_col_name.clone(),
                },
            );
            return None;
        }
    };

    let lhs_cat = type_category(&lhs_col.col_type);

    // 順序比較演算子が使えない型か確認
    if is_order_op(&cmp.op) && lhs_cat == "equality" {
        push_error_cmp(
            ctx,
            diags,
            cmp,
            RdraError::ComparisonOpNotOrdered {
                col: lhs_col_name.clone(),
                col_type: format!("{:?}", lhs_col.col_type),
                op: cmp.op.as_str().to_string(),
            },
        );
        return None;
    }

    // ── 右辺の解決と型整合チェック ────────────────────────────────────────────
    let rhs = match &cmp.rhs {
        Operand::Column(rhs_name) => {
            let rhs_col = match entity_cols.iter().find(|c| &c.name == rhs_name) {
                Some(c) => c,
                None => {
                    push_error_cmp(
                        ctx,
                        diags,
                        cmp,
                        RdraError::ComparisonRhsColumnUnknown {
                            entity: entity_id.to_string(),
                            col: rhs_name.clone(),
                        },
                    );
                    return None;
                }
            };
            let rhs_cat = type_category(&rhs_col.col_type);
            if lhs_cat != rhs_cat {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonTypeMismatch {
                        lhs: lhs_col_name.clone(),
                        lhs_type: format!("{:?}", lhs_col.col_type),
                        rhs: rhs_name.clone(),
                        rhs_type: format!("{:?}", rhs_col.col_type),
                    },
                );
                return None;
            }
            CmpRhs::Column(rhs_name.clone())
        }
        Operand::QualifiedColumn(qcol) => {
            let Some(q_entity) = qref_id(&qcol.entity) else {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonRhsColumnUnknown {
                        entity: entity_id.to_string(),
                        col: qualified_column_display(qcol),
                    },
                );
                return None;
            };
            if q_entity != entity_id {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::TypeMismatch {
                        pred: "comparison".to_string(),
                        id: qualified_column_display(qcol),
                        actual: format!("column of entity {}", q_entity),
                        expected: format!("column of entity {}", entity_id),
                    },
                );
                return None;
            }
            let rhs_name = qcol.column.clone();
            let rhs_col = match entity_cols.iter().find(|c| c.name == rhs_name) {
                Some(c) => c,
                None => {
                    push_error_cmp(
                        ctx,
                        diags,
                        cmp,
                        RdraError::ComparisonRhsColumnUnknown {
                            entity: entity_id.to_string(),
                            col: rhs_name.clone(),
                        },
                    );
                    return None;
                }
            };
            let rhs_cat = type_category(&rhs_col.col_type);
            if lhs_cat != rhs_cat {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonTypeMismatch {
                        lhs: lhs_col_name.clone(),
                        lhs_type: format!("{:?}", lhs_col.col_type),
                        rhs: rhs_name.clone(),
                        rhs_type: format!("{:?}", rhs_col.col_type),
                    },
                );
                return None;
            }
            CmpRhs::Column(rhs_name)
        }
        Operand::IntLit(s) => {
            if lhs_cat != "numeric" {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonTypeMismatch {
                        lhs: lhs_col_name.clone(),
                        lhs_type: format!("{:?}", lhs_col.col_type),
                        rhs: s.clone(),
                        rhs_type: "integer_literal".to_string(),
                    },
                );
                return None;
            }
            match s.parse::<i64>() {
                Ok(n) => CmpRhs::IntLit(n),
                Err(_) => {
                    push_error_cmp(
                        ctx,
                        diags,
                        cmp,
                        RdraError::ComparisonInvalidIntLit { lit: s.clone() },
                    );
                    return None;
                }
            }
        }
        Operand::Now => {
            if lhs_cat != "temporal" {
                push_error_cmp(
                    ctx,
                    diags,
                    cmp,
                    RdraError::ComparisonNowRequiresTemporal {
                        col: lhs_col_name.clone(),
                        col_type: format!("{:?}", lhs_col.col_type),
                    },
                );
                return None;
            }
            CmpRhs::Now
        }
    };

    Some(ComparisonProp {
        lhs_column: lhs_col_name,
        op: to_model_op(&cmp.op),
        rhs,
    })
}
