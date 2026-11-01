//! Comparison expression typing and model conversion.

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use crate::model::*;
use rdra_ish_syntax::ast::*;

use slotmap::SlotMap;

use super::effect::parse_effect_value;
use super::qref_util::{qref_id, qualified_column_display};

// ── The bare right-hand side ─────────────────────────────────────────────────

/// How a bare identifier on the right of a comparison reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BareRhs {
    /// A value of the left-hand column's type.
    Value(EffectValue),
    /// A column; the caller resolves it.
    Column,
}

/// The one reading of `x` in `col == x` / `col != x`, which every construct that takes a
/// comparison shares (rules, `holds`, `after`, `sets`, `@check`, `@derived`, `property`).
///
/// The type of `col` decides. An Enum, variation, or Bool column has a closed set of values,
/// and a bare `x` is one of them: `status == paid` is the value even when the entity has a
/// column `paid`. A bare `x` that is no such value is an error, and when it names a column the
/// error says to write `Entity.x`, the only spelling of a comparison between two such
/// columns. On any other column, `null` and `present` are values of a nullable column and any
/// other name is a column, as `qty == limit` is. An order comparison takes no value.
///
/// `qualified(x)` is `Entity.x` when `x` is a column the comparison could mean.
pub(crate) fn resolve_bare_rhs(
    variations: &SlotMap<VariationKey, Variation>,
    lhs: &ModelColumn,
    op: &CmpOp,
    name: &str,
    qualified: impl Fn(&str) -> Option<std::string::String>,
) -> Result<BareRhs, RdraError> {
    if is_order_op(op) {
        return Ok(BareRhs::Column);
    }
    let closed = matches!(
        lhs.col_type,
        ColumnType::Enum(_) | ColumnType::Variation(_) | ColumnType::Bool
    );
    if !closed && !matches!(name, "null" | "present") {
        return Ok(BareRhs::Column);
    }
    match parse_effect_value(variations, lhs, name) {
        Ok(value) => Ok(BareRhs::Value(value)),
        Err(err) => match qualified(name) {
            Some(qualified) if closed => Err(RdraError::ComparisonRhsNotAValue {
                col: lhs.name.clone(),
                rhs: name.to_string(),
                allowed: column_values(variations, lhs).join(", "),
                qualified,
            }),
            _ => Err(err),
        },
    }
}

/// The values of `col in (..)` / `col not in (..)`, each a value of `col`'s type and listed
/// once. The column has a fixed set of values (Enum, variation, Bool, or `@null`), or is
/// Int-typed and takes integer literals.
pub(crate) fn resolve_set_values(
    variations: &SlotMap<VariationKey, Variation>,
    col: &ModelColumn,
    names: &[std::string::String],
) -> Result<Vec<EffectValue>, RdraError> {
    let numeric = type_category(&col.col_type) == "numeric";
    let closed = matches!(
        col.col_type,
        ColumnType::Enum(_) | ColumnType::Variation(_) | ColumnType::Bool
    );
    if !(closed || numeric || col.is_nullable) {
        return Err(RdraError::SetColumnNotEnumerable {
            col: col.name.clone(),
            col_type: format!("{:?}", col.col_type),
        });
    }
    let mut values: Vec<EffectValue> = Vec::new();
    for name in names {
        let value = parse_effect_value(variations, col, name).map_err(|_| {
            let mut allowed = column_values(variations, col);
            if numeric {
                allowed.insert(0, "integer literals".to_string());
            }
            RdraError::SetValueNotAValue {
                col: col.name.clone(),
                value: name.clone(),
                allowed: allowed.join(", "),
            }
        })?;
        if values.iter().any(|v| v.normalize() == value.normalize()) {
            return Err(RdraError::DuplicateSetValue {
                col: col.name.clone(),
                value: name.clone(),
            });
        }
        values.push(value);
    }
    Ok(values)
}

/// The values a bare right-hand side of `col` may name.
fn column_values(
    variations: &SlotMap<VariationKey, Variation>,
    col: &ModelColumn,
) -> Vec<std::string::String> {
    let mut values: Vec<std::string::String> = match &col.col_type {
        ColumnType::Enum(variants) => variants.clone(),
        ColumnType::Variation(key) => variations
            .get(*key)
            .map(|v| v.values.clone())
            .unwrap_or_default(),
        ColumnType::Bool => vec!["true".to_string(), "false".to_string()],
        _ => Vec::new(),
    };
    if col.is_nullable {
        values.extend(["null".to_string(), "present".to_string()]);
    }
    values
}

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
        ColumnType::String | ColumnType::Bool | ColumnType::Enum(_) | ColumnType::Variation(_) => {
            "equality"
        }
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
        CmpOp::In => CmpOpModel::In,
        CmpOp::NotIn => CmpOpModel::NotIn,
    }
}

/// 比較式 `Comparison` を解析して `ComparisonProp` に変換する。
///
/// - 左辺はカラム参照必須。
/// - 演算子と右辺の型整合を検査する。
/// - 型不整合があれば `diags` にエラーを push し `None` を返す。
pub(crate) fn resolve_comparison(
    variations: &SlotMap<VariationKey, Variation>,
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

    if let Operand::Values(names) = &cmp.rhs {
        return match resolve_set_values(variations, lhs_col, names) {
            Ok(values) => Some(ComparisonProp {
                lhs_column: lhs_col_name,
                op: to_model_op(&cmp.op),
                rhs: CmpRhs::Values(values),
            }),
            Err(err) => {
                push_error_cmp(ctx, diags, cmp, err);
                None
            }
        };
    }

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
            let qualified = |name: &str| {
                entity_cols
                    .iter()
                    .any(|c| c.name == name)
                    .then(|| format!("{entity_id}.{name}"))
            };
            match resolve_bare_rhs(variations, lhs_col, &cmp.op, rhs_name, qualified) {
                Ok(BareRhs::Value(value)) => {
                    return Some(ComparisonProp {
                        lhs_column: lhs_col_name,
                        op: to_model_op(&cmp.op),
                        rhs: CmpRhs::Value(value),
                    });
                }
                Ok(BareRhs::Column) => {}
                Err(err) => {
                    push_error_cmp(ctx, diags, cmp, err);
                    return None;
                }
            }
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
        Operand::Values(_) => unreachable!("a set is resolved above"),
    };

    Some(ComparisonProp {
        lhs_column: lhs_col_name,
        op: to_model_op(&cmp.op),
        rhs,
    })
}
